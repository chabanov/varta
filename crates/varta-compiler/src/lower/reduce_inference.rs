//! Seed context from an explicit reducer signature, then aggregate result context.
//! Each operand is lowered once and remains in runtime source order.
use super::*;

fn unparen(mut expr: &ast::Expr) -> &ast::Expr {
    while let ast::Expr::Paren(paren) = expr {
        expr = &paren.expr;
    }
    expr
}
fn parameter_annotation(pattern: Option<&ast::Pat>) -> Option<&ast::TsTypeAnn> {
    match pattern {
        Some(ast::Pat::Ident(id)) => id.type_ann.as_deref(),
        Some(ast::Pat::Assign(p)) => parameter_annotation(Some(&p.left)),
        Some(ast::Pat::Object(p)) => p.type_ann.as_deref(),
        Some(ast::Pat::Array(p)) => p.type_ann.as_deref(),
        _ => None,
    }
}
impl Lower<'_> {
    pub(super) fn reduce_seed(
        &mut self,
        callback: &ast::Expr,
        seed: &ast::Expr,
        expected: Option<Type>,
    ) -> Result<(Option<Expr>, Option<Expr>)> {
        let (context, resolved) = if let ast::Expr::Arrow(arrow) = unparen(callback) {
            let parameter = annotation(parameter_annotation(arrow.params.first()), self.types)
                .map_err(|e| self.source.locate(e, arrow.span))?;
            let context = if parameter.is_some() {
                parameter
            } else {
                annotation(arrow.return_type.as_deref(), self.types)
                    .map_err(|error| self.source.locate(error, arrow.span))?
            };
            (context, None)
        } else if let ast::Expr::Fn(expression) = unparen(callback) {
            let function = &expression.function;
            let parameter = annotation(
                parameter_annotation(function.params.first().map(|p| &p.pat)),
                self.types,
            )
            .map_err(|e| self.source.locate(e, function.span))?;
            let context = if parameter.is_some() {
                parameter
            } else {
                annotation(function.return_type.as_deref(), self.types)
                    .map_err(|e| self.source.locate(e, function.span))?
            };
            (context, None)
        } else {
            let value = self.expr(callback)?;
            let Type::Function(id) = value.ty else {
                return Err(self.source.locate(
                    type_error("array callback must be a typed function"),
                    value.span,
                ));
            };
            let signature = &self.types.functions[id as usize];
            (
                Some(
                    if signature.shape.rest || signature.shape.optional.first() == Some(&true) {
                        signature.result
                    } else {
                        signature
                            .params
                            .first()
                            .copied()
                            .unwrap_or(signature.result)
                    },
                ),
                Some(value),
            )
        };
        // A surrounding union must not widen an independently inferred scalar
        // seed (e.g. numeric addition). Only aggregate literals use this fallback;
        // select their matching aggregate member rather than the enclosing union.
        let context = context.or_else(|| match unparen(seed) {
            literal @ (ast::Expr::Array(_) | ast::Expr::Object(_)) => {
                self.context(literal, expected)
            }
            _ => None,
        });
        let initial = self.expr_expected(seed, context)?;
        if let Some(expected) = context
            && initial.ty != expected
        {
            return Err(self.source.locate(
                type_error(format!(
                    "array.reduce initial value mismatch: expected {expected}, found {}",
                    initial.ty
                )),
                initial.span,
            ));
        }
        Ok((Some(initial), resolved))
    }
}
