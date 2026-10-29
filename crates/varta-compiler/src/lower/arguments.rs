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
    expected == actual
        || classes::upcast(types, actual, expected)
        || expected.is_union()
            && unions::members(actual, types).iter().all(|t| {
                unions::members(expected, types)
                    .iter()
                    .any(|e| e == t || classes::upcast(types, *t, *e))
            })
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
                self.expr_expected(&arg.expr, context(index, params, shape, self.types))
            })
            .collect()
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
                return Err(self.source.locate(
                    type_error(format!(
                        "argument type mismatch: expected {ty}, found {}",
                        value.ty
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
            if tail.len() > varta_common::value::MAX_ARRAY_ELEMENTS {
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
