//! Unreachable function removal (v37 `language_core`).
//!
//! Engine preludes are ordinary modules: every function and class member of
//! a loaded prelude takes a function slot when it is linked, whether or not
//! the program calls it. Under v37 lowering links against the wider
//! compile-time cap [`LINKED_FUNCTIONS`]; afterwards this pass keeps only the
//! functions reachable from the module's entry points (`main`, tools, the
//! HTTP handler and module-constant initializers) through direct calls,
//! function values, scheduled frames and async-generator relays, renumbers
//! them densely in their original order and enforces
//! `abi::MAX_USER_FUNCTIONS` on what remains. Function indices exist only in
//! the IR nodes and program fields rewritten here; the emitter derives
//! everything else (dispatch targets, frames, catalogs) from them. Earlier
//! profiles keep every linked function and their bytes.
use super::*;
use std::collections::BTreeSet;

/// Function slots v37 may allocate while linking, before unreachable
/// functions are removed: four times the emitted limit.
pub(crate) const LINKED_FUNCTIONS: usize = 4 * varta_common::abi::MAX_USER_FUNCTIONS;

/// The function indices `expr` refers to directly.
fn references(expr: &Expr, found: &mut Vec<usize>) {
    match &expr.kind {
        E::Call(index, _)
        | E::FunctionRef { index, .. }
        | E::Schedule {
            function: index, ..
        } => found.push(*index),
        E::Await {
            relay: Some((index, _)),
            ..
        } => found.push(*index),
        _ => {}
    }
}

/// Remove unreachable functions and renumber the rest (v37 only).
pub(super) fn prune(program: &mut ir::Program) -> Result<()> {
    if !program.types.profile.language_core() {
        return Ok(());
    }
    if program.types.profile.program_limits() {
        unread_constants(program)?;
    }
    let count = program.functions.len();
    let mut stack = vec![program.main];
    stack.extend(program.tools.iter().map(|(index, _)| *index));
    stack.extend(program.http.iter().map(|(index, _)| *index));
    stack.extend(program.initializers.iter().copied());
    // V37 keeps every suspendable function; v38 (`program_limits`) keeps only
    // generator bodies unconditionally (their step functions dispatch over
    // the generator type's frames) and removes unreachable async functions
    // and methods with their frame layouts.
    // The test hook exports every frame creator, so it keeps every frame.
    #[cfg(any(test, feature = "suspension-test-hook"))]
    let hooked = super::suspension::hook::enabled();
    #[cfg(not(any(test, feature = "suspension-test-hook")))]
    let hooked = false;
    let every_frame = !program.types.profile.program_limits() || hooked;
    stack.extend(
        program
            .suspendable
            .iter()
            .filter(|(_, machine)| every_frame || machine.generator.is_some())
            .map(|(index, _)| *index),
    );
    let mut live = vec![false; count];
    while let Some(index) = stack.pop() {
        let Some(seen) = live.get_mut(index) else {
            return Err(CompilerError::Internal(format!(
                "reference to missing function {index}"
            )));
        };
        if std::mem::replace(seen, true) {
            continue;
        }
        let mut found = Vec::new();
        statements(&mut program.functions[index].body, &mut |expr| {
            references(expr, &mut found)
        });
        stack.extend(found);
    }
    let mut map = vec![None; count];
    let mut kept = 0usize;
    for (index, live) in live.iter().enumerate() {
        if *live {
            map[index] = Some(kept);
            kept += 1;
        }
    }
    let limit = program.types.profile.limits().user_functions;
    if kept > limit {
        return Err(invalid(format!(
            "too many functions across modules: {kept} reachable functions exceed the limit of {limit}"
        )));
    }
    let remap = |index: usize| -> Result<usize> {
        map.get(index).copied().flatten().ok_or_else(|| {
            CompilerError::Internal(format!("reference to removed function {index}"))
        })
    };
    let functions = std::mem::take(&mut program.functions);
    let mut missing = None;
    for (index, mut function) in functions.into_iter().enumerate() {
        if !live[index] {
            continue;
        }
        statements(&mut function.body, &mut |expr| {
            let slot = match &mut expr.kind {
                E::Call(index, _)
                | E::FunctionRef { index, .. }
                | E::Schedule {
                    function: index, ..
                } => index,
                E::Await {
                    relay: Some((index, _)),
                    ..
                } => index,
                _ => return,
            };
            match map.get(*slot).copied().flatten() {
                Some(new) => *slot = new,
                None => missing = Some(*slot),
            }
        });
        program.functions.push(function);
    }
    if let Some(index) = missing {
        return Err(CompilerError::Internal(format!(
            "reference to removed function {index}"
        )));
    }
    program.main = remap(program.main)?;
    for (index, _) in &mut program.tools {
        *index = remap(*index)?;
    }
    if let Some((index, _)) = &mut program.http {
        *index = remap(*index)?;
    }
    for index in &mut program.initializers {
        *index = remap(*index)?;
    }
    program.suspendable = std::mem::take(&mut program.suspendable)
        .into_iter()
        .filter(|(index, _)| live[*index])
        .map(|(index, machine)| Ok((remap(index)?, machine)))
        .collect::<Result<_>>()?;
    Ok(())
}

/// V38 (`program_limits`): drop the initialization of every module constant
/// that no reachable code reads. Initializers are a checked pure subset, so
/// skipping an unread one is unobservable; this removes the startup cost of
/// unused constants (the pilot's 356 enum objects) and of the descriptors and
/// functions only they reference. Reads are found to a fixpoint: reachable
/// functions (initializer bodies excluded), then the kept initializer
/// statements, whose values may read earlier constants, through locals of the
/// same initializer, and function values. An initializer with any statement
/// other than `Init`/`InitModuleConstant` is kept whole. Constant slots keep
/// their numbers; a dropped constant's root slot stays empty.
fn unread_constants(program: &mut ir::Program) -> Result<()> {
    let count = program.functions.len();
    let initializer = |function: &ir::Function| {
        function
            .body
            .iter()
            .all(|statement| matches!(statement, S::Init(..) | S::InitModuleConstant(..)))
    };
    let prunable: BTreeSet<usize> = program
        .initializers
        .iter()
        .copied()
        .filter(|index| program.functions.get(*index).is_some_and(initializer))
        .collect();
    let mut live = vec![false; count];
    let mut read = BTreeSet::new();
    let mut stack = vec![program.main];
    stack.extend(program.tools.iter().map(|(index, _)| *index));
    stack.extend(program.http.iter().map(|(index, _)| *index));
    stack.extend(program.suspendable.keys().copied());
    let mut kept: BTreeMap<usize, Vec<bool>> = BTreeMap::new();
    loop {
        while let Some(index) = stack.pop() {
            let Some(seen) = live.get_mut(index) else {
                return Err(CompilerError::Internal(format!(
                    "reference to missing function {index}"
                )));
            };
            if std::mem::replace(seen, true) || prunable.contains(&index) {
                continue;
            }
            // The visitor only reads the nodes.
            statements(&mut program.functions[index].body, &mut |expr| {
                reads(expr, &mut stack, &mut read)
            });
        }
        // Keep the initializer statements the read constants need, backwards
        // through each straight-line body.
        let before = read.len();
        for &index in &prunable {
            let body = &mut program.functions[index].body;
            let mut needed = BTreeSet::new();
            let mut keep = vec![false; body.len()];
            for (at, statement) in body.iter_mut().enumerate().rev() {
                let value = match statement {
                    S::InitModuleConstant(constant, value) if read.contains(constant) => value,
                    S::Init(slot, value) if needed.contains(slot) => value,
                    _ => continue,
                };
                keep[at] = true;
                // The visitor only reads the nodes.
                expression(value, &mut |expr| {
                    if let E::Local(slot) = &expr.kind {
                        needed.insert(*slot);
                    }
                    reads(expr, &mut stack, &mut read);
                });
            }
            kept.insert(index, keep);
        }
        // Only functions not visited yet are new work.
        stack.retain(|index| !live.get(*index).copied().unwrap_or(false));
        if stack.is_empty() && read.len() == before {
            break;
        }
    }
    for (index, keep) in kept {
        let body = std::mem::take(&mut program.functions[index].body);
        program.functions[index].body = body
            .into_iter()
            .zip(keep)
            .filter_map(|(statement, keep)| keep.then_some(statement))
            .collect();
    }
    // An initializer left empty is no longer called (and is then removed as
    // unreachable).
    let empty: BTreeSet<usize> = prunable
        .into_iter()
        .filter(|index| program.functions[*index].body.is_empty())
        .collect();
    program.initializers.retain(|index| !empty.contains(index));
    program.functions[program.main].body.retain(|statement| {
        !matches!(statement, S::Expr(Expr { kind: E::Call(index, args), .. })
            if args.is_empty() && empty.contains(index))
    });
    renumber_constants(program);
    Ok(())
}

/// Number the constants that remaining code reads or writes densely, so a
/// dropped constant takes no permanent root slot the collector scans.
fn renumber_constants(program: &mut ir::Program) {
    let mut used = BTreeSet::new();
    for function in &mut program.functions {
        each_constant(&mut function.body, &mut |index| {
            used.insert(*index);
        });
    }
    let map: BTreeMap<u32, u32> = used
        .iter()
        .enumerate()
        .map(|(new, old)| (*old, new as u32))
        .collect();
    for function in &mut program.functions {
        // Every index was collected above, so the map holds it.
        each_constant(&mut function.body, &mut |index| *index = map[index]);
    }
    program.constants = map.len() as u32;
}

/// Visit every module-constant index in `body`: reads (`ModuleConstant`) in
/// any expression and writes (`InitModuleConstant`) in any statement,
/// including statement bodies nested in expressions.
fn each_constant(body: &mut [S], visit: &mut dyn FnMut(&mut u32)) {
    constant_writes(body, visit);
    statements(body, &mut |expr| match &mut expr.kind {
        E::ModuleConstant(index) => visit(index),
        E::Assignment { body, .. } | E::ValueBlock { body, .. } => constant_writes(body, visit),
        _ => {}
    });
}

/// `InitModuleConstant` indices of `body` and its nested statement bodies;
/// bodies inside expressions are reached by the expression walk.
fn constant_writes(body: &mut [S], visit: &mut dyn FnMut(&mut u32)) {
    for statement in body {
        match statement {
            S::InitModuleConstant(index, _) => visit(index),
            S::TryCatch { body, handler, .. } => {
                constant_writes(body, visit);
                constant_writes(handler, visit);
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                constant_writes(body, visit);
                constant_writes(finalizer, visit);
            }
            S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => {
                constant_writes(body, visit)
            }
            S::If(_, yes, no) => {
                constant_writes(yes, visit);
                constant_writes(no, visit);
            }
            S::While(_, body) | S::DoWhile(_, body) => constant_writes(body, visit),
            S::For(init, _, update, body) => {
                constant_writes(init, visit);
                constant_writes(update, visit);
                constant_writes(body, visit);
            }
            S::Switch { cases, .. } => {
                for (_, body) in cases {
                    constant_writes(body, visit);
                }
            }
            _ => {}
        }
    }
}

/// Function and module-constant references of one expression node.
fn reads(expr: &Expr, functions: &mut Vec<usize>, constants: &mut BTreeSet<u32>) {
    if let E::ModuleConstant(index) = &expr.kind {
        constants.insert(*index);
    }
    references(expr, functions);
}

/// Visit `expr` mutably and every nested expression, including statement
/// bodies (the mutable mirror of `suspension::walk`).
fn expression(expr: &mut Expr, visit: &mut dyn FnMut(&mut Expr)) {
    visit(expr);
    match &mut expr.kind {
        E::Integer(_)
        | E::Float64(_)
        | E::String(_)
        | E::Local(_)
        | E::Status
        | E::ModuleConstant(_)
        | E::Zero
        | E::FunctionRef { .. } => {}
        E::Await {
            promise, result, ..
        } => [promise, result]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        E::Schedule { args, .. } => args.iter_mut().for_each(|value| expression(value, visit)),
        E::Assignment { value, body, .. } => {
            expression(value, visit);
            statements(body, visit);
        }
        E::ValueBlock { body, .. } => statements(body, visit),
        E::GeneratorStep {
            generator, value, ..
        } => [generator, value]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        E::FunctionCheck { value, .. }
        | E::GeneratorValue(value)
        | E::Reinterpret(value)
        | E::Suspend { value, .. }
        | E::StringTrim { value, .. }
        | E::TextUnits { value, .. }
        | E::ArrayEnumerate { value, .. }
        | E::ArrayReserve { length: value, .. }
        | E::NewError(_, value)
        | E::ErrorName(value)
        | E::ErrorMessage(value)
        | E::ErrorText(value)
        | E::ErrorOwn(value)
        | E::BoxUnion(value)
        | E::UnboxUnion(value)
        | E::IsUndefined(value)
        | E::IsError(value)
        | E::IsClass { value, .. }
        | E::ArrayKind(value, _)
        | E::IsNullish(value)
        | E::IsUnionMember(value, _)
        | E::Undefined(value)
        | E::Negative(value)
        | E::ToBoolean(value)
        | E::Not(value)
        | E::Capture(value)
        | E::ResultField(_, value)
        | E::HasOwn(value, _)
        | E::Field(value, _)
        | E::Json(value, _)
        | E::ParseJson(value)
        | E::ArrayPop(value)
        | E::ArrayReverse(value)
        | E::IndexKey(value)
        | E::Length(value) => expression(value, visit),
        E::FunctionCall {
            callee,
            ignored_receiver,
            args,
            ..
        } => {
            expression(callee, visit);
            if let Some(receiver) = ignored_receiver {
                expression(receiver, visit);
            }
            args.iter_mut().for_each(|value| expression(value, visit));
        }
        E::StringSearch(_, a, b, c)
        | E::Conditional(a, b, c)
        | E::ArraySearch(_, a, b, c)
        | E::ArraySlice(a, b, c)
        | E::StringRange {
            value: a,
            start: b,
            end: c,
            ..
        }
        | E::StringRepeat {
            value: a,
            count: b,
            error: c,
            ..
        } => [a, b, c].into_iter().for_each(|v| expression(v, visit)),
        E::UnionLogical(_, a, b)
        | E::Sequence(a, b)
        | E::Binary(_, a, b)
        | E::NumberSameValue(a, b)
        | E::MaybeIndex(a, b)
        | E::ArrayHasOwn(a, b)
        | E::ArrayStringHasOwn(a, b)
        | E::ArrayPush(a, b)
        | E::ArrayAt(a, b)
        | E::Index(a, b)
        | E::RecordDelete {
            object: a,
            absent: b,
            ..
        } => [a, b].into_iter().for_each(|v| expression(v, visit)),
        E::ArrayFill(a, b, c, d) => [a, b, c, d].into_iter().for_each(|v| expression(v, visit)),
        E::Math(_, args)
        | E::NumberPredicate(_, args)
        | E::Intrinsic(_, args)
        | E::RegExp(_, args)
        | E::Call(_, args)
        | E::Host(_, args)
        | E::CheckedBigIntHost { args, .. } => {
            args.iter_mut().for_each(|value| expression(value, visit))
        }
        E::Record(fields) | E::OptionalRecord(fields, _) => fields
            .iter_mut()
            .for_each(|(_, value)| expression(value, visit)),
        E::Array(elements) => elements
            .iter_mut()
            .flatten()
            .for_each(|value| expression(value, visit)),
        E::ArrayDelete {
            array,
            index,
            error,
        } => {
            expression(array, visit);
            index
                .iter_mut()
                .chain(error.iter_mut())
                .for_each(|v| expression(v, visit));
        }
    }
}

fn statements(body: &mut [S], visit: &mut dyn FnMut(&mut Expr)) {
    for statement in body {
        self::statement(statement, visit);
    }
}

fn statement(statement: &mut S, visit: &mut dyn FnMut(&mut Expr)) {
    match statement {
        S::Trap
        | S::Rebind(_)
        | S::RecordRest { .. }
        | S::BreakLabel(_)
        | S::ContinueLabel(_)
        | S::Break
        | S::Continue
        | S::Return(None) => {}
        S::Throw(value)
        | S::Init(_, value)
        | S::Set(_, value)
        | S::InitModuleConstant(_, value)
        | S::Expr(value)
        | S::Return(Some(value)) => expression(value, visit),
        S::TryCatch { body, handler, .. } => {
            statements(body, visit);
            statements(handler, visit);
        }
        S::TryFinally {
            body, finalizer, ..
        } => {
            statements(body, visit);
            statements(finalizer, visit);
        }
        S::RecordEnumerate { values, .. } => values.iter_mut().for_each(|v| expression(v, visit)),
        S::SetField(object, _, value) => {
            expression(object, visit);
            expression(value, visit);
        }
        S::ArrayLength {
            array,
            length,
            error,
        } => [array, length, error]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        S::SetIndex {
            array,
            index,
            value,
            ..
        } => [array, index, value]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => statements(body, visit),
        S::If(test, yes, no) => {
            expression(test, visit);
            statements(yes, visit);
            statements(no, visit);
        }
        S::While(test, body) | S::DoWhile(test, body) => {
            expression(test, visit);
            statements(body, visit);
        }
        S::For(init, test, update, body) => {
            statements(init, visit);
            if let Some(test) = test {
                expression(test, visit);
            }
            statements(update, visit);
            statements(body, visit);
        }
        S::Switch { value, cases, .. } => {
            expression(value, visit);
            for (test, body) in cases {
                if let Some(test) = test {
                    expression(test, visit);
                }
                statements(body, visit);
            }
        }
    }
}
