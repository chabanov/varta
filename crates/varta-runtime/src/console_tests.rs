//! V38 console notation (`node_console`, K9 D1): `console.log`,
//! `console.info` and `console.debug` with any number of arguments print
//! Node 24's `util.formatWithOptions` text (`console_cases/*.ts`). Every
//! `*.expected` file is the output of Node 24.20.0 running the same program
//! with `main();` appended, under `--stack-trace-limit=0` (Varta records no
//! stack frames) and `TZ=UTC`, recorded once and checked in. Every program
//! runs under the production budgets and again in collector stress builds;
//! the refused forms fail with their named diagnostics, and v37 keeps its
//! one-argument rule and notation.
use super::*;
use crate::codegen_v2_tests::run_test_budgets;
use crate::collector_stress_tests::stressed;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
/// Test-only fuel budgets of one stress run, in production fuel budgets.
const STRESS_BUDGETS: u64 = 400;
/// Work units per allocation of the stress builds.
const STRESS_UNITS: [u32; 2] = [2, 64];

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("console_cases/", $name, ".ts")),
            include_str!(concat!("console_cases/", $name, ".expected")),
        )
    };
}

const CASES: &[(&str, &str, &str)] = &[
    case!("values"),
    case!("structures"),
    case!("named_errors"),
    // The `docs/language-guide.md` example, verbatim.
    case!("guide"),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

/// The logged text as the lines Node prints.
fn printed(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .flat_map(|line| {
            line.strip_prefix("varta log: ")
                .unwrap_or_else(|| panic!("not a log line: {line}"))
                .split('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn logs_in(profile: LanguageProfile, source: &str) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_in(profile, source), &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    printed(&output.lines)
}

fn matches_node(name: &str) {
    let (_, source, expected) = CASES
        .iter()
        .find(|(case, ..)| *case == name)
        .unwrap_or_else(|| panic!("missing case {name}"));
    let expected: Vec<&str> = expected.lines().collect();
    assert_eq!(logs_in(V38, source), expected, "{name}");
}

fn refused(profile: LanguageProfile, source: &str, diagnostic: &str) {
    let error = varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} compiled\n{source}"))
        .to_string();
    assert!(error.contains(diagnostic), "{profile:?}: {error}\n{source}");
}

/// Primitives, nesting, depth elision, line breaking and column grouping,
/// class instances, Error objects, Map/Set/Date/`Uint8Array`/Promise,
/// circular references and the `%` directives.
#[test]
fn values_match_node() {
    matches_node("values");
}

/// `console.info`/`console.debug`, Error subclasses, causes and
/// `AggregateError`, sparse arrays, `%o`/`%O`/`%j`, quoting and escapes.
#[test]
fn structures_match_node() {
    matches_node("structures");
}

/// An assigned `name` is the Error's own property.
#[test]
fn named_errors_match_node() {
    matches_node("named_errors");
}

#[test]
fn console_guide_example_matches_node() {
    matches_node("guide");
}

/// The argument list, its cells and every value they reach stay rooted
/// while the linked formatter reads them.
#[test]
fn console_notation_survives_collector_stress() {
    for (name, source, expected) in CASES {
        let expected: Vec<String> = expected.lines().map(str::to_owned).collect();
        for units in STRESS_UNITS {
            let run = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
            assert_eq!(
                (&printed(&run.lines), &run.error),
                (&expected, &None),
                "{name}: stress build ({units} units per allocation)"
            );
        }
    }
}

/// Forms without a Node notation stay refused by name.
#[test]
fn unsupported_logging_is_refused_by_name() {
    refused(
        V38,
        "export function main(): void { const xs = [1]; console.log(...xs); }",
        "unsupported spread",
    );
    refused(
        V38,
        "export function main(): void { console.log(new URL('https://a.example/')); }",
        "unsupported logging of a value holding a URL",
    );
    refused(
        V38,
        "export function main(): void { console.error('x'); }",
        "unsupported",
    );
}

/// V37 keeps one argument and its notation (booleans as integers).
#[test]
fn v37_keeps_its_console_notation() {
    refused(
        V37,
        "export function main(): void { console.log('a', 1); }",
        "log expects one argument",
    );
    let source = "export function main(): void { console.log(true); const u: unknown = { a: [1] }; console.log(u); }";
    assert_eq!(logs_in(V37, source), ["1", "{\"a\":[1]}"]);
    assert_eq!(logs_in(V38, source), ["true", "{ a: [ 1 ] }"]);
}
