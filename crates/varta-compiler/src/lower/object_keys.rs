//! Own-key enumeration uses closed shapes without reading or serializing payloads.
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
impl Lower<'_> {
    pub(super) fn object_keys(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported("Object.keys requires checked-f64-utf16-v12"));
        }
        if self.binding("Object").is_ok() || self.functions.contains_key("Object") {
            return Err(unsupported("shadowed Object.keys intrinsic"));
        }
        if call.args.len() != 1 || call.type_args.is_some() {
            return Err(invalid(
                "Object.keys expects one value and no type arguments",
            ));
        }
        let value = self.expr(&call.args[0].expr)?;
        let present: Vec<_> = unions::members(value.ty, self.types)
            .into_iter()
            .filter(|ty| !matches!(ty, Type::Null | Type::Undefined))
            .collect();
        let shape = match present.as_slice() {
            [Type::Record(id)] => *id,
            _ => {
                return Err(self.source.locate(
                    type_error("Object.keys requires a closed record"),
                    value.span,
                ));
            }
        };
        let span = call.span;
        let input_ty = value.ty;
        let input = self.hidden(input_ty);
        let record = Type::Record(shape);
        let source = if input_ty == record {
            input
        } else {
            self.hidden(record)
        };
        let result = arrays::intern(Type::String, self.types)?;
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        let mut roots = vec![input, output];
        let mut body = vec![S::Init(input, value)];
        if source != input {
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                "Cannot convert undefined or null to object",
                span,
            )?;
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsNullish(Box::new(local(span, input, input_ty))),
                },
                vec![S::Throw(error)],
                vec![],
            ));
            roots.push(source);
            body.push(S::Init(
                source,
                Expr {
                    span,
                    ty: record,
                    kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
                },
            ));
        }
        let elements = self.types.records[shape as usize]
            .fields
            .iter()
            .map(|field| {
                Some(Expr {
                    span,
                    ty: Type::String,
                    kind: E::String(self.types.profile.string_bytes(&field.name)),
                })
            })
            .collect();
        // Allocate final capacity once, then fill the own-key prefix and clear
        // every unused payload/bit before publishing the shorter logical length.
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: result,
                kind: E::Array(elements),
            },
        ));
        body.push(S::RecordKeys {
            source,
            target: output,
            shape,
            slots: [
                self.hidden(Type::Number),
                self.hidden(Type::Number),
                self.hidden(Type::Number),
                self.hidden(Type::Number),
            ],
        });
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
