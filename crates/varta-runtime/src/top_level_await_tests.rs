//! V38 module evaluation order and top-level `await` (`expression_forms`
//! over `module_evaluation`). Each case under `top_level_await_cases/` is a
//! module graph whose `expected` file is the output of Node 24.20.0 running
//! its `main.ts` as an ES module with `await main();` appended, recorded
//! once and checked in.
use super::*;
use crate::codegen_v2_tests::run_test_budgets;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use varta_common::language::LanguageProfile;
use varta_compiler::{CompilerError, ModuleLoader, ModuleSource};

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;

struct Modules(BTreeMap<PathBuf, &'static str>);
impl ModuleLoader for Modules {
    fn resolve(&self, _: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
        Ok(specifier.trim_start_matches("./").into())
    }
    fn load(&self, path: &Path) -> Result<String, CompilerError> {
        self.0
            .get(path)
            .map(|text| (*text).to_owned())
            .ok_or_else(|| CompilerError::Module("missing virtual module".into()))
    }
}

/// The modules of the `ordering` case: `main.ts` requests `c.ts` (which
/// awaits through `a.ts`), `b.ts` and `d.ts` in that source order.
const ORDERING: &[(&str, &str)] = &[
    (
        "log.ts",
        include_str!("top_level_await_cases/ordering/log.ts"),
    ),
    ("a.ts", include_str!("top_level_await_cases/ordering/a.ts")),
    ("b.ts", include_str!("top_level_await_cases/ordering/b.ts")),
    ("c.ts", include_str!("top_level_await_cases/ordering/c.ts")),
    ("d.ts", include_str!("top_level_await_cases/ordering/d.ts")),
];
const ORDERING_MAIN: &str = include_str!("top_level_await_cases/ordering/main.ts");
const ORDERING_EXPECTED: &str = include_str!("top_level_await_cases/ordering/expected");

fn compile(
    entry: &str,
    modules: &[(&str, &'static str)],
    profile: LanguageProfile,
) -> Result<Vec<u8>, CompilerError> {
    let loader = Modules(
        modules
            .iter()
            .map(|(path, text)| (PathBuf::from(path), *text))
            .collect(),
    );
    varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "main.ts".into(),
            text: entry.into(),
        },
        &loader,
        profile,
    )
    .map(|compilation| compilation.wasm)
}

fn lines(entry: &str, modules: &[(&str, &'static str)]) -> Vec<String> {
    let bytes = compile(entry, modules, V38).unwrap_or_else(|e| panic!("{e}\n{entry}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    let run = run_test_budgets(&bytes, 1);
    assert_eq!(run.error, None, "{entry}");
    run.lines
}

fn expected(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| format!("varta log: {line}"))
        .collect()
}

#[test]
fn modules_evaluate_in_source_order_and_await_like_node() {
    assert_eq!(lines(ORDERING_MAIN, ORDERING), expected(ORDERING_EXPECTED));
}

/// The entry module itself awaits: `main` runs at the end of its
/// evaluation, after the awaited value is available.
#[test]
fn an_awaiting_entry_module_calls_main_after_its_evaluation() {
    let entry = "import { note, tick } from './log.ts';\
         const first = await tick('entry tick');\
         const second = note('entry after ' + String(first));\
         export function main(): void { note('main ' + String(second)); console.log(events()); }\
         function events(): string { return 'done'; }";
    let modules: &[(&str, &'static str)] = &[(
        "log.ts",
        include_str!("top_level_await_cases/ordering/log.ts"),
    )];
    assert_eq!(lines(entry, modules), expected("done"));
}

/// A rejected top-level `await` fails the evaluation: dependents and
/// `main` never run and the failure is an unhandled rejection.
#[test]
fn a_failed_module_evaluation_never_runs_main() {
    let entry = "import { value } from './bad.ts';\
         export function main(): void { console.log('main ' + value); }";
    let modules: &[(&str, &'static str)] = &[(
        "bad.ts",
        "async function load(): Promise<string> { throw new Error('load failed'); }\
         export const value: string = await load();",
    )];
    let bytes = compile(entry, modules, V38).unwrap_or_else(|e| panic!("{e}"));
    let run = run_test_budgets(&bytes, 1);
    assert!(run.lines.is_empty(), "{:?}", run.lines);
    let error = run.error.expect("an unhandled rejection");
    assert!(error.contains("load failed"), "{error}");
}

#[test]
fn top_level_await_stays_refused_by_v37() {
    let error = compile(
        "async function v(): Promise<number> { return 1; } const x: number = await v(); export function main(): void { console.log(String(x)); }",
        &[],
        V37,
    )
    .expect_err("v37 compiled top-level await")
    .to_string();
    assert!(error.contains("top-level await"), "{error}");
}

/// An HTTP request runs module evaluation in its own Store before its
/// adapter; an evaluation that awaits has no such synchronous start.
#[test]
fn top_level_await_with_an_http_service_is_refused() {
    let source = r#"
        async function config(): Promise<number> { return 200; }
        const status: number = await config();
        function handle(r: VartaHttpRequest): VartaHttpResponse { return { status, body: r.body, headers: [] }; }
        export function main(): void {}
        varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
    "#;
    let error = compile(source, &[], V38)
        .expect_err("top-level await with HTTP compiled")
        .to_string();
    assert!(
        error.contains("top-level await with tool or HTTP entry points"),
        "{error}"
    );
}
