//! `expression satisfies T` (TypeScript 4.9): the expression keeps its own
//! type and must be assignable to `T`. When its own type does not already
//! satisfy `T` (a literal property that `T` keeps literal, a function
//! literal whose parameters `T` types), `T` is its contextual type and the
//! expression is lowered against `T`, taking `T`'s type.
use super::*;

/// Whether `expression` contains a function literal whose parameters need a
/// contextual type.
fn contextual(expression: &ast::Expr) -> bool {
    use swc_ecma_visit::{Visit, VisitWith};
    struct Untyped(bool);
    impl Visit for Untyped {
        fn visit_arrow_expr(&mut self, arrow: &ast::ArrowExpr) {
            self.0 |= arrow
                .params
                .iter()
                .any(|param| !matches!(param, ast::Pat::Ident(id) if id.type_ann.is_some()));
        }
        fn visit_function(&mut self, function: &ast::Function) {
            self.0 |= function
                .params
                .iter()
                .any(|param| !matches!(&param.pat, ast::Pat::Ident(id) if id.type_ann.is_some()));
        }
    }
    let mut visitor = Untyped(false);
    expression.visit_with(&mut visitor);
    visitor.0
}

impl Lower<'_> {
    pub(in crate::lower) fn satisfies(
        &mut self,
        satisfies: &ast::TsSatisfiesExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let target = keyword_type(&satisfies.type_ann, self.types)
            .map_err(|e| self.source.locate(e, satisfies.type_ann.span()))?;
        // The expression keeps its own type when that type already
        // satisfies `T`; otherwise `T` is its contextual type (literal
        // properties, function literal parameters).
        let own = self.peek_type(&satisfies.expr, 0);
        let value = match own {
            Some(own) if !contextual(&satisfies.expr) && extends(own, target, self.types) => {
                self.expr_expected(&satisfies.expr, expected)?
            }
            _ => self.expr_expected(&satisfies.expr, Some(target))?,
        };
        if !extends(value.ty, target, self.types) {
            return Err(self.source.locate(
                type_error(format!(
                    "type {} does not satisfy the expected type {}",
                    subtyping::describe(value.ty, self.types),
                    subtyping::describe(target, self.types)
                )),
                satisfies.span,
            ));
        }
        Ok(value)
    }
}
