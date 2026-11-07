//! Overloaded function types (TypeScript Handbook, Function Overloads):
//! an object type with several call signatures (`{ (x: string): string;
//! (x: number): number }`) and an intersection of function types. One value
//! has one physical signature, so an overloaded type is the function type
//! whose parameters and result are the unions of the overloads' (marked
//! `FunctionType::overloads`). A call selects the first overload whose
//! parameters accept the arguments' static types, checks the arguments
//! against it, and reads the result as that overload's result type through
//! a run-time member test. A value converts to the overloaded type when its
//! own signature is the merged one (`(x: string | number) => string |
//! number`, typically through `as`). When an overload has a rest parameter
//! the merged signature is one rest parameter of every parameter type
//! (`(...args: (string | number)[]) => string`).
use super::*;

/// The overloads of a function type: its set, or itself.
pub(super) fn signatures(ty: Type, types: &ir::ValueTypes) -> Vec<Type> {
    match ty {
        Type::Function(id) => match types.functions[id as usize].overloads {
            Some(set) => types.type_level.overloads[set as usize].clone(),
            None => vec![ty],
        },
        other => vec![other],
    }
}

/// The overloaded function type of `signatures`, in order.
pub(super) fn overloaded(signatures: Vec<Type>, types: &mut ir::ValueTypes) -> Result<Type> {
    let mut functions = Vec::new();
    for signature in &signatures {
        let Type::Function(id) = *signature else {
            return Err(invalid("missing overload signature"));
        };
        let function = types.functions[id as usize].clone();
        if function.receiver.is_some() || function.generic.is_some() {
            return Err(unsupported(
                "overload with a this parameter or type parameters (its merged signature has no single layout)",
            ));
        }
        functions.push(function);
    }
    let construct = functions[0].construct;
    if functions.iter().any(|f| f.construct != construct) {
        return Err(unsupported("call and construct signatures in one type"));
    }
    let set = match types
        .type_level
        .overloads
        .iter()
        .position(|s| *s == signatures)
    {
        Some(set) => set,
        None => {
            types.type_level.overloads.push(signatures.clone());
            types.type_level.overloads.len() - 1
        }
    } as u32;
    let result = union_of(functions.iter().map(|f| f.result).collect(), types)?;
    if functions.iter().any(|f| f.shape.rest) {
        let mut members = Vec::new();
        for function in &functions {
            members.extend(function.params.iter().take(function.shape.fixed_count()));
            if function.shape.rest
                && let Some(Type::Array(id)) = function.params.last()
            {
                members.push(types.arrays[*id as usize]);
            }
        }
        let element = union_of(members, types)?;
        let rest = arrays::intern(element, types)?;
        return intern_merged(
            ir::FunctionType {
                shape: ir::ParameterShape {
                    optional: vec![false],
                    rest: true,
                },
                receiver: None,
                params: vec![rest],
                result,
                generic: None,
                construct,
                overloads: Some(set),
            },
            types,
        );
    }
    let count = functions.iter().map(|f| f.params.len()).max().unwrap_or(0);
    let mut params = Vec::new();
    let mut optional = Vec::new();
    for position in 0..count {
        let mut members = Vec::new();
        let mut absent = false;
        for function in &functions {
            match function.params.get(position) {
                Some(ty) => {
                    members.push(*ty);
                    absent |= function.shape.optional[position];
                }
                None => absent = true,
            }
        }
        let mut ty = union_of(members, types)?;
        if absent {
            ty = tuple_types::union_with_undefined(ty, types)?;
        }
        params.push(ty);
        optional.push(absent);
    }
    let merged = ir::FunctionType {
        shape: ir::ParameterShape {
            optional,
            rest: false,
        },
        receiver: None,
        params,
        result,
        generic: None,
        construct,
        overloads: Some(set),
    };
    intern_merged(merged, types)
}

/// The function type `merged`, interned.
fn intern_merged(merged: ir::FunctionType, types: &mut ir::ValueTypes) -> Result<Type> {
    if let Some(index) = types.functions.iter().position(|f| *f == merged) {
        return Ok(Type::Function(index as u32));
    }
    if types.functions.len() >= types.profile.limits().function_types {
        return Err(invalid("function type count exceeds limit"));
    }
    types.functions.push(merged);
    Ok(Type::Function(types.functions.len() as u32 - 1))
}

/// A function value whose own signature is the merged one of the
/// overloaded type `expected`, retyped to it.
pub(in crate::lower) fn overloaded_value(
    value: Expr,
    expected: Option<Type>,
    types: &ir::ValueTypes,
) -> Expr {
    let (Some(Type::Function(wanted)), Type::Function(given)) = (expected, value.ty) else {
        return value;
    };
    let (target, source) = (
        &types.functions[wanted as usize],
        &types.functions[given as usize],
    );
    if target.overloads.is_none()
        || source.overloads.is_some()
        || target.params != source.params
        || target.shape != source.shape
        || target.result != source.result
        || target.receiver != source.receiver
    {
        return value;
    }
    Expr {
        span: value.span,
        ty: Type::Function(wanted),
        kind: E::Reinterpret(Box::new(value)),
    }
}

impl Lower<'_> {
    /// `f(args)` of an overloaded function value.
    pub(in crate::lower) fn overloaded_call(
        &mut self,
        call: &ast::CallExpr,
        callee: Expr,
        set: u32,
    ) -> Result<Expr> {
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread argument to an overloaded function"));
        }
        let Type::Function(merged_id) = callee.ty else {
            return Err(invalid("missing overloaded function"));
        };
        let merged = self.types.functions[merged_id as usize].clone();
        let candidates = self.types.type_level.overloads[set as usize].clone();
        let actual: Vec<Option<Type>> = call
            .args
            .iter()
            .map(|arg| {
                self.peek_type(&arg.expr, 0)
                    .or_else(|| parameters::literal(&arg.expr, self.types))
            })
            .collect();
        let chosen = candidates.iter().find_map(|candidate| {
            let Type::Function(id) = *candidate else {
                return None;
            };
            let function = self.types.functions[id as usize].clone();
            let fits = arguments::count(call.args.len(), &function.shape).is_ok()
                && actual.iter().enumerate().all(|(index, given)| {
                    let wanted =
                        arguments::context(index, &function.params, &function.shape, self.types);
                    match (given, wanted) {
                        (Some(given), Some(wanted)) => extends(*given, wanted, self.types),
                        (None, _) => true,
                        (Some(_), None) => false,
                    }
                });
            fits.then_some(function)
        });
        let Some(chosen) = chosen else {
            return Err(self.source.locate(
                type_error(format!(
                    "no overload of {} matches this call",
                    void_functions::describe(callee.ty, self.types)
                )),
                span,
            ));
        };
        let values = self.call_arguments(&call.args, &chosen.params, &chosen.shape)?;
        let mut widened = Vec::new();
        for (index, value) in values.into_iter().enumerate() {
            let wanted = arguments::context(index, &merged.params, &merged.shape, self.types);
            widened.push(unions::widen(value, wanted, self.types)?);
        }
        let args = self.bind_arguments(widened, &merged.params, &merged.shape, span)?;
        let result = self.invoke_bound_receiver(callee, None, args, span)?;
        if result.ty == chosen.result {
            return Ok(result);
        }
        if result.ty.is_cell() {
            return self.checked_narrow(
                result,
                chosen.result,
                "overloaded function returned a value of another overload's result type",
            );
        }
        Ok(result)
    }
}
