//! Normalize checked calls before emission, retaining operand order and roots.
use super::*;

pub(super) fn context(
    index: usize,
    params: &[Type],
    shape: &ir::ParameterShape,
    types: &ir::ValueTypes,
) -> Option<Type> {
    if index < shape.fixed_count() {
        params.get(index).copied()
    } else if shape.rest {
        let Type::Array(id) = *params.last()? else {
            return None;
        };
        Some(types.arrays[id as usize])
    } else {
        None
    }
}
pub(super) fn accepts(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    nominal_accepts(expected, actual, types) || subtyping::assignable(expected, actual, types)
}
/// `accepts` without structural views (v38 `structural_subtyping`).
pub(super) fn nominal_accepts(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    expected == actual
        || literals::assignable(expected, actual, types)
        || classes::upcast(types, actual, expected)
        || dictionaries::converts(expected, actual, types)
        || expected.is_union() && {
            let target = unions::members(expected, types);
            unions::members(actual, types).iter().all(|t| {
                literals::member_of(*t, &target, types)
                    || target.iter().any(|e| {
                        classes::upcast(types, *t, *e) || dictionaries::converts(*e, *t, types)
                    })
            })
        }
}
pub(super) fn count(count: usize, shape: &ir::ParameterShape) -> Result<()> {
    if count < shape.minimum() || !shape.rest && count > shape.optional.len() {
        Err(invalid("function argument count mismatch"))
    } else {
        Ok(())
    }
}
impl Lower<'_> {
    pub(super) fn call_arguments(
        &mut self,
        args: &[ast::ExprOrSpread],
        params: &[Type],
        shape: &ir::ParameterShape,
    ) -> Result<Vec<Expr>> {
        count(args.len(), shape)?;
        args.iter()
            .enumerate()
            .map(|(index, arg)| {
                let expected = context(index, params, shape, self.types);
                if self.unknown_reason(&arg.expr, expected)? {
                    // `reject(reason)` with an `unknown` reason (v36): checked
                    // like a thrown `unknown` value.
                    let value = self.expr(&arg.expr)?;
                    return self.thrown_unknown(value);
                }
                self.expr_expected(&arg.expr, expected)
            })
            .collect()
    }
    /// Whether `argument` is an `unknown` path passed where the exception
    /// union (a promise rejection reason) is expected (v36 `function_fields`).
    fn unknown_reason(&mut self, argument: &ast::Expr, expected: Option<Type>) -> Result<bool> {
        Ok(self.types.profile.function_fields()
            && expected.is_some_and(Type::is_union)
            && self.peek_type(argument, 0) == Some(Type::Unknown)
            && expected == Some(self.exception_type()?))
    }
    pub(super) fn bind_arguments(
        &mut self,
        values: Vec<Expr>,
        params: &[Type],
        shape: &ir::ParameterShape,
        span: swc_common::Span,
    ) -> Result<Vec<Expr>> {
        count(values.len(), shape)?;
        let mut values = values.into_iter();
        let mut result = Vec::new();
        for ty in params.iter().take(shape.fixed_count()) {
            let value = values.next().unwrap_or(Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            });
            let value = unions::widen(value, Some(*ty), self.types)?;
            if value.ty != *ty {
                let (expected, found) = self.mismatch(*ty, value.ty);
                return Err(self.source.locate(
                    type_error(format!(
                        "argument type mismatch: expected {expected}, found {found}"
                    )),
                    value.span,
                ));
            }
            result.push(value);
        }
        if shape.rest {
            let ty = *params
                .last()
                .ok_or_else(|| invalid("missing rest parameter"))?;
            let Type::Array(id) = ty else {
                return Err(invalid("invalid rest parameter type"));
            };
            let element = self.types.arrays[id as usize];
            let mut tail = Vec::new();
            for value in values {
                let value = unions::widen(value, Some(element), self.types)?;
                if value.ty != element {
                    return Err(self.source.locate(
                        type_error(format!(
                            "rest argument mismatch: expected {element}, found {}",
                            value.ty
                        )),
                        value.span,
                    ));
                }
                tail.push(Some(value));
            }
            if tail.len() > self.types.profile.data().array_elements {
                return Err(invalid("rest array exceeds element bound"));
            }
            result.push(Expr {
                span,
                ty,
                kind: E::Array(tail),
            });
        }
        Ok(result)
    }
}
