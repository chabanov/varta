//! Build dense entry pairs from rooted snapshots of the same own-key order.
use super::object_enumeration::Kind;
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, n: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(n),
    }
}
impl Lower<'_> {
    pub(super) fn object_entries(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if self.binding("Object").is_ok() || self.functions.contains_key("Object") {
            return Err(unsupported("shadowed Object.entries intrinsic"));
        }
        if call.type_args.is_some() {
            return Err(invalid("Object.entries has no type arguments"));
        }
        let span = call.span;
        let value = if let Some(first) = call.args.first() {
            self.object_value(&first.expr)?
        } else {
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            }
        };
        let ty = value.ty;
        let source = self.hidden(ty);
        let mut roots = vec![source];
        let mut body = vec![S::Init(source, value)];
        for (index, arg) in call.args.iter().enumerate() {
            if arg.spread.is_some() {
                return Err(unsupported("Object.entries argument spread"));
            }
            if index > 0 {
                body.push(S::Expr(self.expr(&arg.expr)?));
            }
        }
        let keys = self.enumerate_value(local(span, source, ty), Kind::Keys, span)?;
        let values = self.enumerate_value(local(span, source, ty), Kind::Values, span)?;
        let Type::Array(values_id) = values.ty else {
            return Err(invalid("missing enumeration array type"));
        };
        let value_ty = self.types.arrays[values_id as usize];
        let element = unions::intern(vec![Type::String, value_ty], self.types)?;
        let physical = arrays::intern(element, self.types)?;
        // V37 (`collections`) types each entry as the tuple `[string, T]`,
        // physically the same pair array as before.
        let pair = if self.types.profile.collections() {
            tuples::intern(
                vec![Type::String, value_ty],
                tuples::PAIR,
                false,
                self.types,
            )?
        } else {
            physical
        };
        let result = arrays::intern(pair, self.types)?;
        let keys_ty = keys.ty;
        let values_ty = values.ty;
        let key_slot = self.hidden(keys_ty);
        let value_slot = self.hidden(values_ty);
        let output = self.hidden(result);
        let index = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        roots.extend([key_slot, value_slot, output, index]);
        body.extend([
            S::Init(key_slot, keys),
            S::Init(value_slot, values),
            S::Init(
                output,
                Expr {
                    span,
                    ty: result,
                    kind: E::Array(vec![]),
                },
            ),
            S::Init(index, number(span, 0.0)),
        ]);
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(local(span, key_slot, keys_ty)),
                Box::new(local(span, index, Type::Float64)),
            ),
        };
        let value = Expr {
            span,
            ty: value_ty,
            kind: E::Index(
                Box::new(local(span, value_slot, values_ty)),
                Box::new(local(span, index, Type::Float64)),
            ),
        };
        let entry = Expr {
            span,
            ty: pair,
            kind: E::Array(vec![
                Some(unions::widen(key, Some(element), self.types)?),
                Some(unions::widen(value, Some(element), self.types)?),
            ]),
        };
        body.push(S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Lt,
                    Box::new(local(span, index, Type::Float64)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Length(Box::new(local(span, key_slot, keys_ty))),
                    }),
                ),
            },
            vec![
                S::Expr(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::ArrayPush(
                        Box::new(local(span, output, result)),
                        Box::new(Expr {
                            span,
                            ty: result,
                            kind: E::Array(vec![Some(entry)]),
                        }),
                    ),
                }),
                S::Set(
                    index,
                    Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Binary(
                            ast::BinaryOp::Add,
                            Box::new(local(span, index, Type::Float64)),
                            Box::new(number(span, 1.0)),
                        ),
                    },
                ),
            ],
        ));
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
