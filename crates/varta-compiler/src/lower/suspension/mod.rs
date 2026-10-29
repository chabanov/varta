//! K4.4 suspension foundation on typed IR after lowering: evaluation-order
//! normalization, then a resumable state machine whose saved values all live in
//! an explicit continuation frame. No source syntax produces suspension points
//! yet; compiler tests reach them through the `#[cfg(test)]` lowering hook.
use super::*;
use swc_ecma_ast::BinaryOp;
use varta_common::function::frame;
mod construct;
#[cfg(test)]
pub(crate) mod hook;
mod normalize;
mod states;
#[cfg(test)]
mod tests;
mod walk;

const DETAIL: &str = "the suspension foundation does not admit it yet";

fn rejected(construct: &str) -> CompilerError {
    unsupported_suspension(format!("point inside {construct}"))
}
fn unsupported_suspension(what: String) -> CompilerError {
    CompilerError::Unsupported {
        kind: format!("suspension {what}"),
        detail: DETAIL.into(),
    }
}

/// Normalize and transform every function containing a suspension point.
pub(crate) fn prepare(program: &mut ir::Program, graph: &crate::modules::Graph) -> Result<()> {
    let suspendable: Vec<usize> = program
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| walk::body_suspends(&function.body))
        .map(|(index, _)| index)
        .collect();
    if suspendable.is_empty() {
        return Ok(());
    }
    for &index in &suspendable {
        admissible(program, index)?;
    }
    references(program, &suspendable)?;
    let exception = exceptions::exception_union(&mut program.types)?;
    let ir::Program {
        functions,
        types,
        suspendable: machines,
        ..
    } = program;
    for index in suspendable {
        let function = &mut functions[index];
        let namer = construct::Namer::new(graph, &function.source);
        let body = std::mem::take(&mut function.body);
        let body = normalize::Normalizer::new(&mut function.locals, &namer, &function.source)
            .statements(body)?;
        let (body, machine) = states::Machine {
            locals: &mut function.locals,
            types: &*types,
            source: &function.source,
            exception,
            result: function.result,
        }
        .build(function.span, body)?;
        // State, completion and input locals are re-initialized, never saved.
        let entry = 2 + machine.inputs.len();
        let saved = function.locals.iter().filter(|ty| ty.width() > 0).count() - entry;
        if saved > frame::MAX_FRAME_BINDINGS {
            return Err(function.source.locate(
                invalid("suspendable function exceeds the continuation frame budget"),
                function.span,
            ));
        }
        function.body = body;
        machines.insert(index, machine);
    }
    Ok(())
}

/// The foundation admits plain module functions with direct parameters only.
fn admissible(program: &ir::Program, index: usize) -> Result<()> {
    let function = &program.functions[index];
    let profile = program.types.profile;
    let reason = if !(profile.exceptions() && profile.closures() && profile.property_deletion()) {
        Some("a profile without frame metadata (requires checked-f64-utf16-v30)")
    } else if index == program.main
        || program.initializers.contains(&index)
        || program.tools.iter().any(|(tool, _)| *tool == index)
        || program
            .http
            .as_ref()
            .is_some_and(|(http, _)| *http == index)
    {
        Some("an entry function")
    } else if function.environment || function.self_slot.is_some() {
        Some("a closure")
    } else if function.receiver.is_some() {
        Some("a method")
    } else if function.shape.extended() {
        Some("a function with optional or rest parameters")
    } else if function.result.width() > 1 {
        Some("a function returning VartaResult")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(function
            .source
            .locate(rejected(reason), suspension_span(&function.body))),
        None => Ok(()),
    }
}

/// Suspendable functions are not ordinary callables until generator/async
/// objects exist: direct calls and function values are rejected.
fn references(program: &ir::Program, suspendable: &[usize]) -> Result<()> {
    for function in &program.functions {
        let mut found = None;
        walk::statements(&function.body, &mut |e| {
            let target = match &e.kind {
                E::Call(index, _) | E::FunctionRef { index, .. } => Some(*index),
                _ => None,
            };
            if found.is_none() && target.is_some_and(|t| suspendable.contains(&t)) {
                found = Some(e.span);
            }
        });
        if let Some(span) = found {
            return Err(function.source.locate(
                CompilerError::Unsupported {
                    kind: "reference to a function containing a suspension point".into(),
                    detail: DETAIL.into(),
                },
                span,
            ));
        }
    }
    Ok(())
}

fn suspension_span(body: &[S]) -> swc_common::Span {
    normalize::body_span(body).unwrap_or_default()
}
