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
//!
//! Step 4 (2026-11-06): an overload may be generic (`<T>(x: T): T`) or
//! declare a `this` parameter. A generic overload takes part through its
//! erased signature (`lower::erased_methods`: `unknown` where a type
//! parameter stands); the call tries the overloads in order, as tsc's
//! overload resolution does, instantiating a generic overload by inference
//! from the arguments (`erased_instance`) before testing it, and checks the
//! merged result back to the selected instance's result. The merged
//! signature's receiver is the union of the overloads' `this` types; the
//! receiver of a method-style call must suit the selected overload.
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
        functions.push(types.functions[id as usize].clone());
    }
    if functions.iter().any(|f| f.constructors.is_some()) {
        return Err(unsupported(
            "overloaded generic construct signatures (each would need its own instantiation table)",
        ));
    }
    // The merged receiver: the union of the declared `this` types. An
    // overload without one would be called without a receiver, which the
    // merged physical signature cannot express.
    let receivers: Vec<Type> = functions.iter().filter_map(|f| f.receiver).collect();
    let receiver = match receivers.len() {
        0 => None,
        count if count == functions.len() => Some(merge(receivers, types)?),
        _ => {
            return Err(unsupported(
                "overload set in which only some overloads declare a this parameter (the merged \
                 signature has no single receiver)",
            ));
        }
    };
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
    let result = merge(functions.iter().map(|f| f.result).collect(), types)?;
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
        let element = merge(members, types)?;
        let rest = arrays::intern(element, types)?;
        return intern_merged(
            ir::FunctionType {
                shape: ir::ParameterShape {
                    optional: vec![false],
                    rest: true,
                },
                receiver,
                params: vec![rest],
                result,
                generic: None,
                construct,
                overloads: Some(set),
                constructors: None,
                properties: None,
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
        let mut ty = merge(members, types)?;
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
        receiver,
        params,
        result,
        generic: None,
        construct,
        overloads: Some(set),
        constructors: None,
        properties: None,
    };
    intern_merged(merged, types)
}

/// The merged type of overload positions: their union, or `unknown` when
/// one is `unknown` (the erased type of a generic overload's parameter).
fn merge(members: Vec<Type>, types: &mut ir::ValueTypes) -> Result<Type> {
    if members.contains(&Type::Unknown) {
        return Ok(Type::Unknown);
    }
    union_of(members, types)
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

/// One overload selected for a call: its (instantiated) signature and the
/// erased declaration of a generic overload.
struct Selected {
    signature: ir::FunctionType,
    generic: Option<u32>,
}

impl Lower<'_> {
    /// Whether `call` calls a function value of a generic or overloaded
    /// function type, whose call takes explicit type arguments (v38).
    pub(in crate::lower) fn generic_value_callee(&self, call: &ast::CallExpr) -> bool {
        let ast::Callee::Expr(callee) = &call.callee else {
            return false;
        };
        self.types.profile.type_level()
            && matches!(self.peek_type(callee, 0), Some(Type::Function(id))
                if self.types.functions[id as usize].generic.is_some()
                    || self.types.functions[id as usize].overloads.is_some())
    }
    /// `f(args)` or (with `receiver`) `object.f(args)` of an overloaded
    /// function value: the first overload, in declaration order, whose
    /// receiver and parameters accept the call (a generic one instantiated
    /// from the arguments first).
    pub(in crate::lower) fn overloaded_call(
        &mut self,
        call: &ast::CallExpr,
        callee: Expr,
        set: u32,
        receiver: Option<Expr>,
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
        let this = receiver.as_ref().map(|value| value.ty);
        let mut chosen = None;
        for candidate in &candidates {
            let Type::Function(id) = *candidate else {
                continue;
            };
            let function = self.types.functions[id as usize].clone();
            let selected = match function.generic {
                // A generic overload whose instantiation fails (a violated
                // constraint) does not apply to this call.
                Some(generic) => match self.erased_instance(call, generic) {
                    Ok(instance) => Selected {
                        signature: ir::FunctionType {
                            receiver: function.receiver,
                            ..instance
                        },
                        generic: Some(generic),
                    },
                    Err(_) => continue,
                },
                None => Selected {
                    signature: function,
                    generic: None,
                },
            };
            if self.overload_fits(&selected.signature, &actual, this, call.args.len()) {
                chosen = Some(selected);
                break;
            }
        }
        let Some(chosen) = chosen else {
            return Err(self.source.locate(
                type_error(format!(
                    "no overload of {} matches this call",
                    void_functions::describe(callee.ty, self.types)
                )),
                span,
            ));
        };
        let instance = &chosen.signature;
        let values = self.call_arguments(&call.args, &instance.params, &instance.shape)?;
        let mut widened = Vec::new();
        for (index, value) in values.into_iter().enumerate() {
            let wanted = arguments::call_context(
                index,
                call.args.len(),
                &merged.params,
                &merged.shape,
                self.types,
            );
            widened.push(
                if wanted == Some(Type::Unknown) && value.ty != Type::Unknown {
                    unknown::widen(value, self.types)?
                } else {
                    unions::widen(value, wanted, self.types)?
                },
            );
        }
        let args = self.bind_arguments(widened, &merged.params, &merged.shape, span)?;
        let result = self.invoke_bound_receiver(callee, receiver, args, span)?;
        if result.ty == instance.result {
            return Ok(result);
        }
        if let Some(generic) = chosen.generic
            && result.ty == Type::Unknown
        {
            return self.erased_result(result, instance, generic, span);
        }
        if result.ty == Type::Unknown {
            let name = subtyping::describe(instance.result, self.types);
            let message = format!("overloaded function returned a value that is not a {name}");
            return self.assert_value(result, instance.result, &name, &message, span);
        }
        if result.ty.is_cell() {
            return self.checked_narrow(
                result,
                instance.result,
                "overloaded function returned a value of another overload's result type",
            );
        }
        Ok(result)
    }

    /// Whether an overload accepts a call's receiver type `this` and the
    /// static types `actual` of its `count` arguments.
    fn overload_fits(
        &self,
        function: &ir::FunctionType,
        actual: &[Option<Type>],
        this: Option<Type>,
        count: usize,
    ) -> bool {
        let receiver = match (function.receiver, this) {
            (Some(wanted), Some(given)) => extends(given, wanted, self.types),
            // tsc: a `this` parameter requires a receiver of its type.
            (Some(wanted), None) => extends(Type::Undefined, wanted, self.types),
            (None, _) => true,
        };
        receiver
            && arguments::typed_count(count, &function.params, &function.shape, self.types).is_ok()
            && actual.iter().enumerate().all(|(index, given)| {
                let wanted = arguments::call_context(
                    index,
                    count,
                    &function.params,
                    &function.shape,
                    self.types,
                );
                match (given, wanted) {
                    (Some(given), Some(wanted)) => extends(*given, wanted, self.types),
                    (None, _) => true,
                    (Some(_), None) => false,
                }
            })
    }
}
