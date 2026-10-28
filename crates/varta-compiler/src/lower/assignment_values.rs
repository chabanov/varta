//! Simple assignment expressions share rooted prepared references with patterns.
use super::*;

impl Lower<'_> {
    pub(super) fn simple_assignment_value(&mut self, assign: &ast::AssignExpr) -> Result<Expr> {
        if assign.op != ast::AssignOp::Assign {
            return self.compound_assignment_value(assign);
        }
        if !self.types.profile.closures() {
            return Err(unsupported(
                "assignment expressions require checked-f64-utf16-v12",
            ));
        }
        let target = match &assign.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(name)) => self
                .prepare_pattern_target(&ast::Pat::Ident(name.clone()))?
                .ok_or_else(|| invalid("missing assignment binding"))?,
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                let base = self.expr(&member.obj)?;
                if let Type::Record(shape) = base.ty
                    && let ast::MemberProp::Computed(key) = &member.prop
                    && !matches!(
                        &*key.expr,
                        ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                    )
                {
                    return self.dynamic_record_write_value(assign, base, &key.expr, shape);
                }
                let field = self.place_value(member, base)?;
                self.prepare_member_reference(field, member.span)?
            }
            _ => return Err(unsupported("assignment expression target")),
        };
        self.prepared_assignment_value(target, &assign.right, assign.span)
    }
}
