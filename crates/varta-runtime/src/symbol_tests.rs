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
    case!("keyed"),
    case!("protocols"),
    case!("guide_keys"),
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

#[test]
fn symbol_keyed_members_match_node() {
    matches_node("keyed");
}

#[test]
fn symbol_protocols_match_node() {
    matches_node("protocols");
}

/// The `docs/language-guide.md` examples, verbatim.
#[test]
fn symbol_guide_example_matches_node() {
    matches_node("guide");
    matches_node("guide_keys");
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

struct Modules(std::collections::BTreeMap<std::path::PathBuf, String>);
impl varta_compiler::ModuleLoader for Modules {
    fn resolve(
        &self,
        _: &std::path::Path,
        specifier: &str,
    ) -> Result<std::path::PathBuf, varta_compiler::CompilerError> {
        Ok(specifier.trim_start_matches("./").into())
    }
    fn load(&self, path: &std::path::Path) -> Result<String, varta_compiler::CompilerError> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| varta_compiler::CompilerError::Module("missing virtual module".into()))
    }
}

/// A `unique symbol` keeps one member key across modules: imported,
/// re-exported and renamed bindings name the member its declaring module
/// declared.
#[test]
fn symbol_keys_are_stable_across_modules() {
    let modules = Modules(
        [
            (
                "keys",
                "export const TAG: unique symbol = Symbol('tag');\n\
                 export interface Tagged { [TAG]: string; size: number }\n\
                 export function make(size: number): Tagged { return { [TAG]: 'made', size }; }",
            ),
            ("again", "export { TAG as LABEL } from './keys';"),
        ]
        .into_iter()
        .map(|(path, text)| (path.into(), text.to_owned()))
        .collect(),
    );
    let entry = "import { make, TAG, type Tagged } from './keys';\n\
                 import { LABEL } from './again';\n\
                 export function main(): void {\n\
                   const t: Tagged = make(2);\n\
                   console.log(t[TAG] + ' ' + t[LABEL] + ' ' + String(TAG === LABEL));\n\
                   t[LABEL] = 'changed';\n\
                   console.log(t[TAG] + ' ' + JSON.stringify(t));\n\
                 }";
    let bytes = varta_compiler::compile_modules_with_profile(
        varta_compiler::ModuleSource {
            path: "main.ts".into(),
            text: entry.into(),
        },
        &modules,
        V38,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .wasm;
    let output = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    // Node 24.20.0 prints the same two lines.
    assert_eq!(
        printed(&output.lines),
        ["made made true", "changed {\"size\":2}"]
    );
}

/// Symbol-keyed members are refused where a string-key view would list
/// them, and quoted keys cannot spell a symbol key.
#[test]
fn symbol_keys_stay_out_of_string_key_forms() {
    refused(
        V38,
        "const K = Symbol('k');\nexport function main(): void { const o = { [K]: 1, a: 2 }; console.log(Object.keys(o).join()); }",
        "Object.keys of an object with symbol-keyed members",
    );
    refused(
        V38,
        "export function main(): void { const o = { '#[3:K]': 1 }; console.log(String(o)); }",
        "reserved for symbol keys",
    );
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
    refused(
        V38,
        "export function main(): void { const s = Symbol(true); }",
        "Symbol description must be a string, a number or undefined",
    );
}
