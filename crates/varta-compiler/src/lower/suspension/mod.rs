//! K4.4 suspension foundation on typed IR after lowering: evaluation-order
//! normalization, then a resumable state machine whose saved values all live in
//! an explicit continuation frame. No source syntax produces suspension points
//! yet; tests reach them through the test-only lowering hook (compiler unit
//! tests, or other crates' tests through the `suspension-test-hook` feature).
use super::*;
use swc_ecma_ast::BinaryOp;
use varta_common::function::frame;
mod construct;
#[cfg(any(test, feature = "suspension-test-hook"))]
pub(crate) mod hook;
mod liveness;
mod normalize;
mod states;
#[cfg(test)]
mod tests;
mod walk;

const DETAIL: &str = "the suspension foundation does not admit it yet";

fn rejected(construct: &str) -> CompilerError {
    unsupported_suspension(format!("point inside {construct}"))
}
/// The same rejection for a point the source spells differently (`await`).
fn unsupported_suspension_as(point: &str, construct: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: format!("{point} inside {construct}"),
        detail: DETAIL.into(),
    }
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
        .filter(|(_, function)| function.generator.is_some() || walk::body_suspends(&function.body))
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
        let generator = function.generator;
        if let Some(id) = generator {
            // Calls created the generator object; the frame returns `TReturn`.
            function.result = types.generators[id as usize].returned;
        }
        let namer = construct::Namer::new(graph, &function.source, types.profile);
        let body = std::mem::take(&mut function.body);
        let mut normalizer =
            normalize::Normalizer::new(&mut function.locals, &namer, &function.source);
        normalizer.awaits = types.asynchrony.scheduled.contains(&index);
        normalizer.publish = types.asynchrony.published.get(&index).copied();
        let body = normalizer.statements(body)?;
        let liveness = types.profile.frame_liveness();
        let (body, machine) = states::Machine {
            locals: &mut function.locals,
            types: &*types,
            source: &function.source,
            exception,
            result: function.result,
            generator,
            scheduled: types.asynchrony.scheduled.contains(&index),
            liveness: liveness.then_some((&function.cells, function.params.len())),
            resumed: Vec::new(),
        }
        .build(function.span, body)?;
        let saved = match &machine.live {
            // A liveness frame holds the largest saved set of any state,
            // the creator's parameters included.
            Some(live) => {
                let entries = |slots: &mut dyn Iterator<Item = u32>| -> usize {
                    slots
                        .map(|slot| {
                            if function.cells.contains(&slot) {
                                1
                            } else {
                                function.locals[slot as usize].width() as usize
                            }
                        })
                        .sum()
                };
                live.iter()
                    .map(|saved| entries(&mut saved.iter().copied()))
                    .chain([entries(&mut (0..function.params.len() as u32))])
                    .max()
                    .unwrap_or(0)
            }
            // State, completion and input locals are re-initialized, never saved.
            None => {
                let entry = 2 + machine.inputs.len();
                function.locals.iter().filter(|ty| ty.width() > 0).count() - entry
            }
        };
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
    if function.generator.is_some() {
        let reason = if function.environment || function.self_slot.is_some() {
            Some("generator closure")
        } else if function.receiver.is_some() {
            Some("generator method")
        } else if function.shape.extended() {
            Some("generator with optional or rest parameters")
        } else {
            None
        };
        return match reason {
            Some(reason) => Err(function.source.locate(
                CompilerError::Unsupported {
                    kind: reason.into(),
                    detail: "outside the synchronous generator subset of checked-f64-utf16-v33"
                        .into(),
                },
                function.span,
            )),
            None => Ok(()),
        };
    }
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
    } else if function.self_slot.is_some()
        || function.environment && !async_closure_body(program, index)
    {
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

/// The scheduled body of an async closure (v35 `async_methods`): an
/// environment function whose captured cells its frame keeps as shared slots.
fn async_closure_body(program: &ir::Program, index: usize) -> bool {
    program.types.profile.async_methods() && program.types.asynchrony.scheduled.contains(&index)
}

/// Suspendable functions are not ordinary callables until generator/async
/// objects exist: direct calls and function values are rejected.
fn references(program: &ir::Program, suspendable: &[usize]) -> Result<()> {
    for function in &program.functions {
        let mut found = None;
        walk::statements(&function.body, &mut |e| {
            // Calling a `function*` creates its generator object.
            let target = match &e.kind {
                E::Call(index, _) if program.functions[*index].generator.is_none() => Some(*index),
                E::FunctionRef { index, .. } => Some(*index),
                _ => None,
            };
            if found.is_none()
                && let Some(target) = target.filter(|t| suspendable.contains(t))
            {
                found = Some((e.span, target));
            }
        });
        if let Some((span, target)) = found {
            let generator = program.functions[target].generator.is_some();
            return Err(function.source.locate(
                CompilerError::Unsupported {
                    kind: if generator {
                        "generator function as a value"
                    } else {
                        "reference to a function containing a suspension point"
                    }
                    .into(),
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
