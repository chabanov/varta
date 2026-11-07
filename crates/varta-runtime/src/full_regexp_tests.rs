//! V38 full RegExp (`full_regexp`, K9 D3): the ECMAScript 2025 pattern
//! language compiled ahead of time to backtracking programs and run by the
//! linked matcher (`varta_intrinsics::regexp`) as guest code metered as fuel.
//! Every `full_regexp_cases/*.expected` file is the output of Node 24.20.0
//! running the same program with `await main();` appended, recorded once
//! and checked in. Every program runs under the production budgets;
//! adversarial patterns end with the turn's fuel exhaustion; v37 keeps
//! rejecting every new form.
use super::*;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("full_regexp_cases/", $name, ".ts")),
            include_str!(concat!("full_regexp_cases/", $name, ".expected")),
        )
    };
}

const CASES: &[(&str, &str, &str)] = &[
    case!("backtracking"),
    case!("flags_groups"),
    case!("unicode_sets"),
    case!("string_methods"),
    case!("differential"),
    case!("runtime_patterns"),
    case!("match_arrays"),
    case!("replacers"),
    // The `docs/language-guide.md` examples, verbatim.
    case!("guide"),
    case!("guide_runtime"),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

/// The logged lines as Node prints them.
fn logs(source: &str) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_in(V38, source), &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    output
        .lines
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

#[test]
fn full_regexp_cases_match_node() {
    for (name, source, expected) in CASES {
        let expected: Vec<&str> = expected.lines().collect();
        let actual = logs(source);
        for (index, (left, right)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(left, right, "{name}: line {}", index + 1);
        }
        assert_eq!(actual.len(), expected.len(), "{name}");
    }
}

#[test]
fn v37_keeps_rejecting_the_full_forms() {
    for (_, source, _) in CASES {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V37).is_err(),
            "{source}"
        );
    }
}

/// The execution error of `source` under the production budgets.
fn failure(source: &str) -> RuntimeError {
    VartaRuntime::new()
        .run_wasm_bytes(&compile_in(V38, source), &[])
        .expect_err(source)
}

/// Catastrophic backtracking (ReDoS) is bounded by the turn's fuel: an
/// exponential pattern on a long adversarial input ends with the typed fuel
/// exhaustion of its turn, the same error on every run, instead of hanging.
#[test]
fn catastrophic_patterns_end_with_fuel_exhaustion() {
    for pattern in [r"/^(a+)+$/", r"/(a|a)*b/", r"/^(\w+\s?)*$/", r"/(a|aa)+c/"] {
        let source = format!(
            "export function main():void{{console.log(String({pattern}.test('a'.repeat(64) + '!')));}}"
        );
        let error = failure(&source);
        assert!(
            matches!(&error, RuntimeError::Budget(exhausted)
                if exhausted.budget == varta_common::budget::BudgetKind::Fuel),
            "{pattern}: {error:?}"
        );
        let first = format!("{error:?}");
        assert_eq!(format!("{:?}", failure(&source)), first, "{pattern}");
    }
}

/// Guest fuel of a v38 program's `_start` with one turn's fuel.
fn fuel(source: &str) -> u64 {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile_in(V38, source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    store.set_fuel(varta_common::budget::TURN_FUEL).unwrap();
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    varta_common::budget::TURN_FUEL - store.get_fuel().unwrap()
}
/// Fuel of `pattern.test(input)` beyond building the input.
fn test_fuel(pattern: &str, input: &str) -> u64 {
    let base = format!("export function main():void{{const s={input};}}");
    let program = format!("export function main():void{{const s={input};{pattern}.test(s);}}");
    fuel(&program) - fuel(&base)
}

/// Measured fuel of the linked matcher under the production budgets (the
/// contract's figures): a run's fixed cost, a literal-led and a class-led
/// search per input unit, and a nested quantifier on a matching input.
#[test]
fn matcher_fuel_stays_within_the_contract() {
    let per_unit = |pattern: &str, unit: &str| {
        let input = |n: usize| format!("'{unit}'.repeat({n})");
        (test_fuel(pattern, &input(500)) - test_fuel(pattern, &input(250))) / 250
    };
    let fixed = test_fuel("/b/", "'a'");
    let scan = per_unit(r"/\r\n|\r|\n/", "a");
    let class = per_unit(r"/[0-9]/", "a");
    let nested = per_unit(r"/^(a+)+$/", "a");
    // Measured 2026-11-06: about 3 700, 33, 33 and 162.
    assert!(fixed <= 4_500, "{fixed}");
    assert!(scan <= 40, "{scan}");
    assert!(class <= 40, "{class}");
    assert!(nested <= 200, "{nested}");
}

/// Fuel of constructing `new RegExp(pattern, flags)` from values the
/// compiler cannot see, beyond building the arguments.
fn construction_fuel(pattern: &str, flags: &str) -> u64 {
    let arguments = format!("const p:string[]=[{pattern:?},{flags:?}];console.log(p[1]??'');");
    let base = format!("export function main():void{{{arguments}}}");
    let program = format!("export function main():void{{{arguments}new RegExp(p[0]??'',p[1]);}}");
    fuel(&program) - fuel(&base)
}

/// A pattern only known at run time is compiled by the linked pattern
/// compiler as guest code: metered as fuel (the contract's figures) and
/// absent from modules that construct no such pattern.
#[test]
fn runtime_pattern_compilation_is_metered_and_linked_on_demand() {
    let small = construction_fuel("a(b)+c", "g");
    let case = construction_fuel(r"\w+@\w+\.com", "iu");
    let letters = construction_fuel(r"\p{L}+", "u");
    let emoji = construction_fuel(r"\p{RGI_Emoji}", "v");
    // Measured 2026-11-06: about 37 000, 228 000, 3.3 million and 48 million.
    assert!(small <= 60_000, "{small}");
    assert!(case <= 400_000, "{case}");
    assert!(letters <= 5_000_000, "{letters}");
    assert!(emoji <= 70_000_000, "{emoji}");
    let constant = compile_in(
        V38,
        "export function main():void{console.log(String(/a/.test('a')));}",
    );
    let runtime = compile_in(
        V38,
        "export function main():void{const p:string[]=['a'];console.log(String(new RegExp(p[0]??'').test('a')));}",
    );
    // The pattern compiler and its Unicode data join only the second.
    assert!(
        runtime.len() > constant.len() + 100_000,
        "{} {}",
        runtime.len(),
        constant.len()
    );
}
