//! Preserve original arguments through ignored effects before primitive conversion.
use super::*;
impl Lower<'_> {
    pub(super) fn primitive_builtin(&mut self, call: &ast::CallExpr, name: &str) -> Result<Expr> {
        if !self.types.profile.primitive_builtins() {
            return Err(unsupported(
                "Boolean/global numeric predicates require checked-f64-utf16-v23",
            ));
        }
        let span = call.span;
        let boolean = name == "Boolean";
        let mut roots = Vec::new();
        let mut body = Vec::new();
        let input = if let Some((first, rest)) = call.args.split_first() {
            let value = self.expr(&first.expr)?;
            let admitted = if boolean {
                value.ty.result_value().is_none() && value.ty != Type::Void
            } else {
                unions::members(value.ty, self.types)
                    .iter()
                    .all(|ty| unions::primitive(*ty))
            };
            if !admitted {
                return Err(self.source.locate(type_error(if boolean {"Boolean requires an ordinary value; opaque result pairs need explicit fields"} else {"global numeric predicates require primitives; object conversion is unsupported"}),value.span));
            }
            let ty = value.ty;
            let slot = self.hidden(ty);
            roots.push(slot);
            body.push(S::Init(slot, value));
            for arg in rest {
                body.push(S::Expr(self.expr(&arg.expr)?));
            }
            Expr {
                span,
                ty,
                kind: E::Local(slot),
            }
        } else if boolean {
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            }
        } else {
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Float64(f64::NAN),
            }
        };
        let kind = if boolean {
            E::ToBoolean(Box::new(input))
        } else {
            let value = if input.ty == Type::Float64 {
                input
            } else {
                let target = unions::intern(
                    vec![
                        Type::Float64,
                        Type::Boolean,
                        Type::String,
                        Type::Null,
                        Type::Undefined,
                    ],
                    self.types,
                )?;
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Host(
                        Host::PrimitiveNumber,
                        vec![unions::widen(input, Some(target), self.types)?],
                    ),
                }
            };
            E::NumberPredicate(
                if name == "isFinite" {
                    ir::NumberPredicate::Finite
                } else {
                    ir::NumberPredicate::Nan
                },
                vec![value],
            )
        };
        let output = self.hidden(Type::Boolean);
        roots.push(output);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: Type::Boolean,
                kind,
            },
        ));
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
