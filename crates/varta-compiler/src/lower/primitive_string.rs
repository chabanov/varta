//! Cache a primitive before ignored arguments, then convert to String or Number.
use super::*;
impl Lower<'_> {
    pub(super) fn primitive_conversion(
        &mut self,
        call: &ast::CallExpr,
        number: bool,
    ) -> Result<Expr> {
        if number && !self.types.profile.primitive_number() {
            return Err(unsupported(
                "Number conversion requires checked-f64-utf16-v20",
            ));
        }
        let result_ty = if number { Type::Float64 } else { Type::String };
        if !self.types.profile.primitive_string() {
            return Err(unsupported(
                "String conversion requires checked-f64-utf16-v19",
            ));
        }
        let span = call.span;
        let Some((first, rest)) = call.args.split_first() else {
            return Ok(Expr {
                span,
                ty: result_ty,
                kind: if number {
                    E::Float64(0.0)
                } else {
                    E::String(Vec::new())
                },
            });
        };
        let value = self.expr(&first.expr)?;
        self.primitive_convert(value, rest, number, span)
    }
    /// Cache an already lowered primitive, evaluate and discard `rest`, then
    /// convert it with the profile's String or Number conversion.
    pub(super) fn primitive_convert(
        &mut self,
        value: Expr,
        rest: &[ast::ExprOrSpread],
        number: bool,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let name = if number { "Number" } else { "String" };
        let result_ty = if number { Type::Float64 } else { Type::String };
        if value.ty == Type::BigInt {
            let input = self.hidden(Type::BigInt);
            let mut body = vec![S::Init(input, value)];
            for arg in rest {
                body.push(S::Expr(self.expr(&arg.expr)?));
            }
            let input_value = Expr {
                span,
                ty: Type::BigInt,
                kind: E::Local(input),
            };
            let converted = if number {
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Host(Host::BigIntToNumber, vec![input_value]),
                }
            } else {
                self.bigint_text(input_value)?
            };
            let output = self.hidden(result_ty);
            let marker = self.hidden(Type::Number);
            body.push(S::Init(output, converted));
            return Ok(Expr {
                span,
                ty: result_ty,
                kind: E::ValueBlock {
                    output,
                    marker,
                    roots: vec![input, output],
                    body,
                },
            });
        }
        if unions::members(value.ty, self.types).iter().any(|ty| {
            !unions::primitive(*ty) && !(self.types.profile.bigint_values() && *ty == Type::BigInt)
        }) {
            return Err(self.source.locate(type_error(format!("{name} conversion requires primitives; object-to-primitive conversion is not yet supported")),value.span));
        }
        let input = self.hidden(value.ty);
        let input_ty = value.ty;
        let mut body = vec![S::Init(input, value)];
        for arg in rest {
            body.push(S::Expr(self.expr(&arg.expr)?));
        }
        let mut members = vec![
            Type::Float64,
            Type::Boolean,
            Type::String,
            Type::Null,
            Type::Undefined,
        ];
        if self.types.profile.bigint_values() {
            members.push(Type::BigInt);
        }
        let union = unions::intern(members, self.types)?;
        let value = unions::widen(
            Expr {
                span,
                ty: input_ty,
                kind: E::Local(input),
            },
            Some(union),
            self.types,
        )?;
        let output = self.hidden(result_ty);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: result_ty,
                kind: E::Host(
                    if self.types.profile.bigint_values() {
                        if number {
                            Host::PrimitiveNumberV2
                        } else {
                            Host::PrimitiveStringV2
                        }
                    } else if number {
                        Host::PrimitiveNumber
                    } else {
                        Host::PrimitiveString
                    },
                    vec![value],
                ),
            },
        ));
        Ok(Expr {
            span,
            ty: result_ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body,
            },
        })
    }
}
