//! Infer one admitted element union without re-evaluating source expressions.
use super::*;
fn unparen(mut value: &ast::Expr) -> &ast::Expr {
    while let ast::Expr::Paren(paren) = value {
        value = &paren.expr;
    }
    value
}
fn empty_array(value: &ast::Expr) -> bool {
    matches!(unparen(value),ast::Expr::Array(array) if array.elems.iter().all(Option::is_none))
}
impl Lower<'_> {
    pub(super) fn inferred_array(&mut self, array: &ast::ArrayLit) -> Result<Expr> {
        let mut values = Vec::new();
        let mut evidence = Vec::new();
        let mut empty = Vec::new();
        for entry in &array.elems {
            let Some(entry) = entry.as_ref() else {
                values.push(None);
                continue;
            };
            if entry.spread.is_some() {
                return Err(self
                    .source
                    .locate(unsupported("array spread"), entry.expr.span()));
            }
            // Empty/all-hole peers have no expressions, binding reads or flow
            // effects. Their type can be resolved later without visiting any
            // effectful initializer twice or changing its lowering order.
            if empty_array(&entry.expr) {
                empty.push((values.len(), &*entry.expr));
                values.push(None);
                continue;
            }
            let hints = references::AggregateHints::from_members(&evidence);
            let value = self.expr_contextual(&entry.expr, None, Some(hints))?;
            if value.ty.value_kind().is_none()
                || value.ty == Type::BigInt && !self.types.profile.bigint_values()
            {
                return Err(self
                    .source
                    .locate(unsupported("array element type"), value.span));
            }
            let members = unions::members(value.ty, self.types);
            for member in members {
                if evidence.iter().any(|old| match (*old, member) {
                    (Type::Array(a), Type::Array(b)) | (Type::Record(a), Type::Record(b)) => a != b,
                    _ => false,
                }) {
                    return Err(self.source.locate(
                        unsupported("multiple record shapes or array element types in one union"),
                        value.span,
                    ));
                }
                if !evidence.contains(&member) {
                    evidence.push(member);
                }
            }
            values.push(Some(value));
        }
        if let Some((_, expression)) = empty.first() {
            let Some(context) = evidence
                .iter()
                .find(|ty| matches!(ty, Type::Array(_)))
                .copied()
            else {
                return Err(self.source.locate(
                    invalid(
                        "empty array requires an element type annotation or a typed array peer",
                    ),
                    expression.span(),
                ));
            };
            for (slot, expression) in empty {
                values[slot] = Some(self.expr_expected(expression, Some(context))?);
            }
        }
        let element = match evidence.as_slice() {
            [] => return Err(invalid("empty array requires an element type annotation")),
            [element] => *element,
            _ => unions::intern(evidence, self.types)?,
        };
        // Intern the final union once. Intermediate prefixes must not consume
        // the program's bounded union-type inventory.
        let values = values
            .into_iter()
            .map(|value| {
                value
                    .map(|value| {
                        let span = value.span;
                        unions::widen(value, Some(element), self.types)
                            .map_err(|error| self.source.locate(error, span))
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Expr {
            span: array.span,
            ty: arrays::intern(element, self.types)?,
            kind: E::Array(values),
        })
    }
}
