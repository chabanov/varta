//! Primitive unary conversion preserves numeric kinds and cached source effects.
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
impl Lower<'_> {
    fn primitive_numeric_unary(
        &mut self,
        op: ast::UnaryOp,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if value.ty == Type::BigInt {
            if op != ast::UnaryOp::Plus {
                return self.bigint_unary(op, value, span);
            }
            // Unary plus is ToNumber, which rejects BigInt. Its successful
            // result domain remains Number even when this path always throws.
            let checked = self.bigint_unary(op, value, span)?;
            let output = self.hidden(Type::Float64);
            let marker = self.hidden(Type::Number);
            return Ok(Expr {
                span,
                ty: Type::Float64,
                kind: E::ValueBlock {
                    output,
                    marker,
                    roots: vec![output],
                    // Keep native validation/work accounting ahead of the
                    // recoverable TypeError; malformed leaves remain fatal.
                    body: vec![
                        S::Expr(checked),
                        S::Init(
                            output,
                            Expr {
                                span,
                                ty: Type::Float64,
                                kind: E::Float64(0.0),
                            },
                        ),
                    ],
                },
            });
        }
        match op {
            ast::UnaryOp::Plus => Ok(value),
            ast::UnaryOp::Minus => Ok(Expr {
                span,
                ty: Type::Float64,
                kind: E::Negative(Box::new(value)),
            }),
            ast::UnaryOp::Tilde => self.number_bitwise_not(value, span),
            _ => Err(invalid("non-numeric primitive unary operator")),
        }
    }
    pub(super) fn primitive_unary(
        &mut self,
        op: ast::UnaryOp,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if unions::members(value.ty, self.types)
            .iter()
            .any(|ty| !unions::primitive(*ty) && *ty != Type::BigInt)
        {
            return Err(unsupported("object ToPrimitive in unary arithmetic"));
        }
        let value = self.checked_update_numeric(value)?;
        if !value.ty.is_union() {
            return self.primitive_numeric_unary(op, value, span);
        }
        let input_ty = value.ty;
        let input = self.hidden(input_ty);
        let bigint = self.primitive_numeric_unary(
            op,
            Expr {
                span,
                ty: Type::BigInt,
                kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
            },
            span,
        )?;
        let number = self.primitive_numeric_unary(
            op,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
            },
            span,
        )?;
        let (ty, bigint, number) = self.join(bigint, number)?;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let result = Expr {
            span,
            ty,
            kind: E::Conditional(
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUnionMember(Box::new(local(span, input, input_ty)), Type::BigInt),
                }),
                Box::new(bigint),
                Box::new(number),
            ),
        };
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body: vec![S::Init(input, value), S::Init(output, result)],
            },
        })
    }
}
