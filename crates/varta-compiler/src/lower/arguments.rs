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
        match *params.last()? {
            Type::Array(id) => Some(types.arrays[id as usize]),
            Type::Tuple(id) => tuple_position(id, index - shape.fixed_count(), None, types),
            _ => None,
        }
    } else {
        None
    }
}
/// The source argument type of a tuple rest parameter. Trailing elements
/// of a variadic tuple are selected using the total argument count.
fn tuple_position(
    id: u32,
    index: usize,
    count: Option<usize>,
    types: &ir::ValueTypes,
) -> Option<Type> {
    let t = &types.tuples[id as usize];
    if index < t.elements.len() {
        return t.elements.get(index).copied();
    }
    if let Some(count) = count {
        let trailing = count.saturating_sub(t.trailing.len());
        if index >= trailing {
            return t.trailing.get(index - trailing).copied();
        }
    }
    t.rest
}
pub(super) fn call_context(
    index: usize,
    count: usize,
    params: &[Type],
    shape: &ir::ParameterShape,
    types: &ir::ValueTypes,
) -> Option<Type> {
    if shape.rest
        && index >= shape.fixed_count()
        && let Some(Type::Tuple(id)) = params.last()
    {
        tuple_position(
            *id,
            index - shape.fixed_count(),
            Some(count - shape.fixed_count()),
            types,
        )
    } else {
        context(index, params, shape, types)
    }
}
pub(super) fn typed_count(
    count: usize,
    params: &[Type],
    shape: &ir::ParameterShape,
    types: &ir::ValueTypes,
) -> Result<()> {
    self::count(count, shape)?;
    if shape.rest
        && let Some(Type::Tuple(id)) = params.last()
    {
        let tuple = &types.tuples[*id as usize];
        let tail = count.saturating_sub(shape.fixed_count());
        if tail < tuple.required + tuple.trailing.len()
            || tuple.rest.is_none() && tail > tuple.elements.len() + tuple.trailing.len()
        {
            return Err(invalid("tuple rest argument count mismatch"));
        }
    }
    Ok(())
}
/// A required finite tuple rest has the same source arity as fixed
/// parameters; adapters use this view without changing its physical tuple slot.
pub(super) fn expanded(
    params: &[Type],
    shape: &ir::ParameterShape,
    types: &ir::ValueTypes,
) -> Option<(Vec<Type>, ir::ParameterShape)> {
    if !shape.rest {
        return None;
    }
    let Type::Tuple(id) = *params.last()? else {
        return None;
    };
    let tuple = &types.tuples[id as usize];
    if tuple.rest.is_some() || !tuple.trailing.is_empty() || tuple.required != tuple.elements.len()
    {
        return None;
    }
    let mut values = params[..shape.fixed_count()].to_vec();
    values.extend(&tuple.elements);
    let mut optional = shape.optional[..shape.fixed_count()].to_vec();
    optional.extend(std::iter::repeat_n(false, tuple.elements.len()));
    Some((
        values,
        ir::ParameterShape {
            optional,
            rest: false,
        },
    ))
}
pub(super) fn accepts(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    nominal_accepts(expected, actual, types) || subtyping::assignable(expected, actual, types)
}
/// `accepts` without structural views (v38 `structural_subtyping`).
pub(super) fn nominal_accepts(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    expected == actual
        || tagged_templates::widens(expected, actual, types)
        || literals::assignable(expected, actual, types)
        || classes::upcast(types, actual, expected)
        || dictionaries::converts(expected, actual, types)
        || expected.is_union() && {
            let target = unions::members(expected, types);
            unions::members(actual, types).iter().all(|t| {
                literals::member_of(*t, &target, types)
                    || target.iter().any(|e| {
                        classes::upcast(types, *t, *e)
                            || dictionaries::converts(*e, *t, types)
                            || tagged_templates::widens(*e, *t, types)
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
        typed_count(args.len(), params, shape, self.types)?;
        args.iter()
            .enumerate()
            .map(|(index, arg)| {
                let expected = call_context(index, args.len(), params, shape, self.types);
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
        typed_count(values.len(), params, shape, self.types)?;
        let total = values.len();
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
            let array = match ty {
                Type::Array(_) => ty,
                Type::Tuple(id) => self.types.tuples[id as usize].array,
                _ => return Err(invalid("invalid rest parameter type")),
            };
            let Type::Array(id) = array else {
                return Err(invalid("invalid rest array"));
            };
            let element = self.types.arrays[id as usize];
            let mut tail = Vec::new();
            for (index, value) in values.enumerate() {
                let expected = call_context(
                    shape.fixed_count() + index,
                    total,
                    params,
                    shape,
                    self.types,
                )
                .unwrap_or(element);
                let value = unions::widen(value, Some(expected), self.types)?;
                if value.ty != expected {
                    return Err(self.source.locate(
                        type_error(format!(
                            "rest argument mismatch: expected {expected}, found {}",
                            value.ty
                        )),
                        value.span,
                    ));
                }
                let value = self.physical_array(value)?;
                tail.push(Some(unions::widen(value, Some(element), self.types)?));
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
