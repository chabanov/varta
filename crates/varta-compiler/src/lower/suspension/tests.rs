//! Normalization shape and construct-specific diagnostics. Execution order and
//! resumption behavior are covered by the Wasmtime tests in `suspension_tests`.
use super::*;
use crate::suspension_tests::compile;
use varta_common::language::LanguageProfile;

fn program(body: &str) -> String {
    format!("{body}\nexport function main(): void {{}}")
}
fn message(body: &str) -> String {
    match compile(&program(body)) {
        Ok(_) => format!("unexpectedly compiled: {body}"),
        Err(error) => error.to_string(),
    }
}
fn lowered(body: &str) -> ir::Program {
    hook::with(|| {
        let graph = crate::modules::Graph::load_with_profile(
            crate::ModuleSource {
                path: "input.ts".into(),
                text: program(body),
            },
            &crate::modules::NoImports,
            LanguageProfile::CheckedF64Utf16V31,
        )
        .unwrap();
        lower(&graph, LanguageProfile::CheckedF64Utf16V31).unwrap()
    })
}

/// Every suspension point is a void statement in a running guard with a
/// suspension-free operand, numbered 1..=n in statement order.
fn canonical(body: &[S], states: &mut Vec<u32>) {
    for statement in body {
        match statement {
            S::Expr(Expr {
                ty: Type::Void,
                kind: E::Suspend { value, state },
                ..
            }) => {
                assert!(!walk::suspends(value));
                states.push(*state);
            }
            S::If(test, yes, no) => {
                assert!(!walk::suspends(test));
                canonical(yes, states);
                canonical(no, states);
            }
            S::While(_, body)
            | S::DoWhile(_, body)
            | S::Block(body)
            | S::Scope(_, body)
            | S::Label { body, .. }
            | S::TryCatch { body, .. }
            | S::TryFinally { body, .. } => canonical(body, states),
            S::For(init, _, update, body) => {
                canonical(init, states);
                canonical(body, states);
                canonical(update, states);
            }
            other => {
                assert!(!walk::body_suspends(std::slice::from_ref(other)));
            }
        }
    }
}

#[test]
fn normalized_state_machines_hold_only_canonical_suspension_points() {
    let program = lowered(
        "function pick(a: number, b: number): number { return a - b; }
         function task(flag: boolean): number {
           let total = pick(1, __varta_suspend(2)) + (flag ? __varta_suspend(3) : 4);
           for (let i = 0; i < 2; i = i + 1) { total += __varta_suspend(i); }
           try { total = total + __varta_suspend(total); } catch (e) { total = 0; }
           return total;
         }",
    );
    let (&index, machine) = program.suspendable.iter().next().unwrap();
    let mut states = Vec::new();
    canonical(&program.functions[index].body, &mut states);
    assert_eq!(states, [1, 2, 3, 4]);
    assert_eq!(machine.points.len(), 4);
    assert!(
        machine
            .points
            .iter()
            .all(|point| point.input == Type::Float64)
    );
    assert_eq!(machine.return_input, Some(Type::Float64));
    assert!(machine.inputs.contains_key(&Type::Float64));
}

#[test]
fn unsupported_positions_fail_with_construct_specific_diagnostics() {
    for (body, construct) in [
        (
            "function t(): string { return String(__varta_suspend(1)); }",
            "call to String",
        ),
        (
            "function t(): number { return Math.hypot(1, __varta_suspend(2)); }",
            "call to Math.hypot",
        ),
        (
            "function t(): number { try { return 1; } catch (e) { return __varta_suspend(1); } }",
            "catch clause",
        ),
        (
            "function t(): number { try { return 1; } finally { __varta_suspend(1); } }",
            "finally clause",
        ),
        (
            "function t(x: number): number { switch (x) { case 1: return __varta_suspend(1); } return 0; }",
            "switch case",
        ),
        (
            "function t(): number { let i = 0; do { i = i + 1; } while (__varta_suspend(i) > 0); return i; }",
            "do-while condition",
        ),
        (
            "function t(): string { return \"a\".repeat(__varta_suspend(2)); }",
            "`repeat` method call",
        ),
        (
            "function t(): number { return [1, 2].indexOf(__varta_suspend(2)); }",
            "`indexOf` method call",
        ),
        (
            "export function main(): void { __varta_suspend(1); }\nfunction t(): void {}",
            "an entry function",
        ),
        (
            "function t(): () => number { return () => __varta_suspend(1); }",
            "a closure",
        ),
    ] {
        let message = if construct == "an entry function" {
            compile(body).map_or_else(|e| e.to_string(), |_| body.to_owned())
        } else {
            message(body)
        };
        assert!(
            message.contains(&format!("unsupported suspension point inside {construct}")),
            "{construct}: {message}"
        );
    }
}

#[test]
fn branch_operators_over_intrinsic_blocks_are_split() {
    for body in [
        "function t(): number { const o: { n?: number } = {}; return o.n ?? __varta_suspend(2); }",
        "function t(flag: boolean): boolean { return flag && __varta_suspend(true, true); }",
        "function t(flag: boolean): number { return flag ? __varta_suspend(1) : 2; }",
    ] {
        compile(&program(body)).unwrap();
    }
}

#[test]
fn suspension_inputs_and_values_are_checked() {
    let error = message("function t(): string { return __varta_suspend(1, \"text\"); }");
    assert!(
        error.contains("unsupported suspension input type string"),
        "{error}"
    );
    let error = message(
        "function t(): void { __varta_suspend(varta.result.capture(varta.env.get(\"X\"))); }",
    );
    assert!(
        error.contains("unsupported suspension of a VartaResult<string> value"),
        "{error}"
    );
}

#[test]
fn suspendable_functions_are_not_callable_or_function_values() {
    for body in [
        "function t(): number { return __varta_suspend(1); }
         function caller(): number { return t(); }",
        "function t(): number { return __varta_suspend(1); }
         function caller(): () => number { return t; }",
    ] {
        let error = message(body);
        assert!(
            error.contains("reference to a function containing a suspension point"),
            "{error}"
        );
    }
}

#[test]
fn suspension_requires_frame_metadata_and_the_test_hook() {
    let source = program("function t(): number { return __varta_suspend(1); }");
    let error = hook::with(|| {
        crate::compile_typescript_with_profile(&source, LanguageProfile::CheckedF64Utf16V29)
    })
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("inside a profile without frame metadata"),
        "{error}"
    );
    // No profile admits suspension syntax: without the hook it is an
    // ordinary unresolved call.
    let error =
        crate::compile_typescript_with_profile(&source, LanguageProfile::CheckedF64Utf16V31)
            .unwrap_err()
            .to_string();
    assert!(!error.contains("suspension"), "{error}");
}
