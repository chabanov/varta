//! Direct, unshadowed Math intrinsics accept exactly one narrowed Number.
use super::*;
impl Lower<'_> {
    pub(super) fn math_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Expr> {
        if !self.types.profile.math_intrinsics() {
            return Err(unsupported("Math intrinsics require checked-f64-utf16-v15"));
        }
        if self.binding("Math").is_ok() || self.functions.contains_key("Math") {
            return Err(unsupported("shadowed Math intrinsic"));
        }
        let ast::MemberProp::Ident(method) = &member.prop else {
            return Err(unsupported("computed Math intrinsic"));
        };
        let mode = match method.sym.as_ref() {
            "abs" => ir::MathUnary::Abs,
            "floor" => ir::MathUnary::Floor,
            "ceil" => ir::MathUnary::Ceil,
            "trunc" => ir::MathUnary::Trunc,
            "round" => ir::MathUnary::Round,
            "sqrt" => ir::MathUnary::Sqrt,
            _ => return Err(unsupported(&format!("Math.{} intrinsic", method.sym))),
        };
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            return Err(unsupported(
                "Math intrinsic requires exactly one non-spread argument",
            ));
        }
        let value = self.expr(&call.args[0].expr)?;
        if value.ty != Type::Float64 {
            return Err(self.source.locate(
                type_error(
                    "Math intrinsic argument must be a narrowed Number; coercion is unsupported",
                ),
                value.span,
            ));
        }
        Ok(Expr {
            span: call.span,
            ty: Type::Float64,
            kind: E::MathUnary(mode, Box::new(value)),
        })
    }
}
