//! Number predicates test types without coercion, including union values.
use super::*;
impl Lower<'_> {
    pub(super) fn number_predicate_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Expr> {
        if !self.types.profile.number_predicates() {
            return Err(unsupported(
                "Number predicates require checked-f64-utf16-v15",
            ));
        }
        if self.binding("Number").is_ok() || self.functions.contains_key("Number") {
            return Err(unsupported("shadowed Number intrinsic"));
        }
        let ast::MemberProp::Ident(method) = &member.prop else {
            return Err(unsupported("computed Number intrinsic"));
        };
        if matches!(method.sym.as_ref(), "parseInt" | "parseFloat") {
            return self.number_parse(call, method.sym == "parseInt");
        }
        let mode = match method.sym.as_ref() {
            "isFinite" => ir::NumberPredicate::Finite,
            "isNaN" => ir::NumberPredicate::Nan,
            "isInteger" => ir::NumberPredicate::Integer,
            "isSafeInteger" => ir::NumberPredicate::SafeInteger,
            _ => return Err(unsupported(&format!("Number.{} intrinsic", method.sym))),
        };
        let mut args = Vec::new();
        for arg in &call.args {
            if arg.spread.is_some() {
                return Err(unsupported("spread to a Number predicate"));
            }
            args.push(self.expr(&arg.expr)?);
        }
        Ok(Expr {
            span: call.span,
            ty: Type::Boolean,
            kind: E::NumberPredicate(mode, args),
        })
    }
}
