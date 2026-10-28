//! Number-only bitwise operations retain ordinary union narrowing requirements.
use super::*;

impl Lower<'_> {
    pub(super) fn number_bitwise(
        &mut self,
        op: ast::BinaryOp,
        left: Expr,
        right: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported(
                "Number bitwise operators require checked-f64-utf16-v12",
            ));
        }
        for value in [&left, &right] {
            if value.ty != Type::Float64 {
                return Err(self.source.locate(
                    type_error(
                        "bitwise operand must be a narrowed Number; coercion is unsupported",
                    ),
                    value.span,
                ));
            }
        }
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::Binary(op, Box::new(left), Box::new(right)),
        })
    }
    pub(super) fn number_bitwise_not(
        &mut self,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let all_bits = Expr {
            span,
            ty: Type::Float64,
            kind: E::Float64(-1.0),
        };
        self.number_bitwise(ast::BinaryOp::BitXor, value, all_bits, span)
    }
}
