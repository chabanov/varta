//! V38 symbol values (`symbol_values`, K9 D2): `Symbol()` values with the
//! value18 symbol tag, `Symbol.for`/`Symbol.keyFor`, the well-known
//! symbols, `typeof` and its narrowing, symbols in records, arrays, unions,
//! `unknown`, `Map`/`Set` keys and JSON, and symbol-keyed members
//! (`symbol_cases/*.ts`). Every `*.expected` file is the output of Node
//! 24.20.0 running the same program with `await main();` appended,
//! recorded once and checked in. Every program runs under the production
//! budgets and again in collector stress builds; v37 keeps rejecting every
//! new form.
use super::*;
use crate::codegen_v2_tests::run_test_budgets;
use crate::collector_stress_tests::stressed;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
/// Test-only fuel budgets of one stress run, in production fuel budgets
/// (as in `standard_library_tests`).
const STRESS_BUDGETS: u64 = 400;
/// Work units per allocation of the stress builds.
const STRESS_UNITS: [u32; 2] = [2, 64];

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("symbol_cases/", $name, ".ts")),
            include_str!(concat!("symbol_cases/", $name, ".expected")),
        )
    };
}

const CASES: &[(&str, &str, &str)] = &[
    case!("basics"),
    case!("values"),
    case!("declared"),
    case!("guide"),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

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

fn logs(source: &str) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_in(V38, source), &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    printed(&output.lines)
}

fn matches_node(name: &str) {
    let (_, source, expected) = CASES
        .iter()
        .find(|(case, ..)| *case == name)
        .unwrap_or_else(|| panic!("missing case {name}"));
    let expected: Vec<&str> = expected.lines().collect();
    assert_eq!(logs(source), expected, "{name}");
}

fn refused(profile: LanguageProfile, source: &str, diagnostic: &str) {
    let error = varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} compiled\n{source}"))
        .to_string();
    assert!(error.contains(diagnostic), "{profile:?}: {error}\n{source}");
}

#[test]
fn symbol_basics_match_node() {
    matches_node("basics");
}

#[test]
fn symbol_values_in_aggregates_and_json_match_node() {
    matches_node("values");
}

#[test]
fn declared_symbols_match_node() {
    matches_node("declared");
}

/// The `docs/language-guide.md` example, verbatim.
#[test]
fn symbol_guide_example_matches_node() {
    matches_node("guide");
}

/// Symbols keep their records rooted across every allocation: the stress
/// builds print what the normal build prints.
#[test]
fn symbols_survive_collector_stress() {
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

/// V37 keeps value layout 17 and rejects every symbol form.
#[test]
fn symbol_forms_stay_refused_by_v37() {
    for source in [
        "export function main(): void { const s = Symbol('a'); console.log(String(s)); }",
        "export function main(): void { const s: symbol = Symbol.iterator; }",
        "export function main(): void { console.log(String(Symbol.for('k'))); }",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V37).is_err(),
            "{source}"
        );
    }
    let _ = V38;
    refused(
        V38,
        "export function main(): void { const s = Symbol(true); }",
        "Symbol description must be a string, a number or undefined",
    );
}
