//! Plain exponentiation requires narrowed Number operands in its new profile.
use super::*;
impl Lower<'_> {
    pub(super) fn number_power(
        &mut self,
        left: Expr,
        right: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if !self.types.profile.number_power() {
            return Err(unsupported(
                "Number exponentiation requires checked-f64-utf16-v15",
            ));
        }
        for value in [&left, &right] {
            if value.ty != Type::Float64 {
                return Err(self.source.locate(
                    type_error(
                        "exponentiation operand must be a narrowed Number; coercion is unsupported",
                    ),
                    value.span,
                ));
            }
        }
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::Binary(ast::BinaryOp::Exp, Box::new(left), Box::new(right)),
        })
    }
}
