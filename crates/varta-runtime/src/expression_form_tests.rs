//! V38 K9 C expressions and functions (`expression_forms`): `await` in
//! every operand position, the comma operator, `void`, `delete`, `in`,
//! `instanceof`, optional chains, logical and exponent assignments,
//! labelled blocks, `switch` fallthrough, `new.target` comparisons, tagged
//! templates and `String.raw`, spreads into fixed parameters and `new`,
//! explicit `this` parameters, inferred class-field arrows, object-literal
//! methods that use `this`, constant computed keys, destructuring
//! everywhere (iterables included), hand-written iterators, `using` and
//! `await using`, structural thenables and generators driven by `next`,
//! `return` and `throw` (`expression_form_cases/*.ts`). Every `*.expected`
//! file is the output of Node 24.20.0 running the same program with
//! `await main();` appended, recorded once and checked in. Every program
//! runs under the production budgets and again in collector stress builds;
//! refused forms fail with their named diagnostics, and v37 keeps
//! rejecting every new form with its old diagnostic.
use super::*;
use crate::codegen_v2_tests::run_test_budgets;
use crate::collector_stress_tests::stressed;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
/// Test-only fuel budgets of one stress run, in production fuel budgets:
/// stress builds collect continuously (as in `collector_stress_tests`).
const STRESS_BUDGETS: u64 = 400;
/// Work units per allocation of the stress builds.
const STRESS_UNITS: [u32; 2] = [2, 64];

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("expression_form_cases/", $name, ".ts")),
            include_str!(concat!("expression_form_cases/", $name, ".expected")),
        )
    };
}

const CASES: &[(&str, &str, &str)] = &[
    case!("await_positions"),
    case!("await_arguments"),
    case!("await_operands"),
    case!("operators"),
    case!("tagged_templates"),
    case!("spreads"),
    case!("receivers"),
    case!("object_methods"),
    case!("computed_keys"),
    case!("destructuring"),
    case!("callback_patterns"),
    case!("iterators"),
    case!("using"),
    case!("thenables"),
    case!("generators"),
    // The `docs/language-guide.md` example, verbatim.
    case!("guide"),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn logs(source: &str) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_in(V38, source), &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    output
        .lines
        .iter()
        .map(|line| {
            line.strip_prefix("varta log: ")
                .unwrap_or_else(|| panic!("not a log line: {line}"))
                .to_owned()
        })
        .collect()
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
fn await_in_every_operand_position_matches_node() {
    matches_node("await_positions");
    matches_node("await_arguments");
    matches_node("await_operands");
}

#[test]
fn operators_labels_and_switch_fallthrough_match_node() {
    matches_node("operators");
}

#[test]
fn tagged_templates_and_string_raw_match_node() {
    matches_node("tagged_templates");
}

#[test]
fn spreads_into_fixed_parameters_match_node() {
    matches_node("spreads");
}

#[test]
fn this_parameters_and_field_arrows_match_node() {
    matches_node("receivers");
}

#[test]
fn object_literal_methods_with_this_match_node() {
    matches_node("object_methods");
}

#[test]
fn constant_computed_keys_match_node() {
    matches_node("computed_keys");
}

#[test]
fn destructuring_everywhere_matches_node() {
    matches_node("destructuring");
}

/// Array, tuple and object patterns with defaults and rest in the
/// parameters of arrows, functions, methods and the callbacks of array
/// methods and iterator helpers, and the rest of a tuple pattern.
#[test]
fn callback_parameter_patterns_match_node() {
    matches_node("callback_patterns");
}

#[test]
fn hand_written_iterators_and_iterable_destructuring_match_node() {
    matches_node("iterators");
}

#[test]
fn using_declarations_dispose_like_node() {
    matches_node("using");
}

#[test]
fn structural_thenables_settle_in_node_job_order() {
    matches_node("thenables");
}

#[test]
fn generators_driven_by_next_return_and_throw_match_node() {
    matches_node("generators");
}

#[test]
fn expression_language_guide_example_matches_node() {
    matches_node("guide");
    let guide = include_str!("../../../docs/language-guide.md");
    assert!(
        guide.contains(include_str!("expression_form_cases/guide.ts")),
        "docs/language-guide.md no longer shows expression_form_cases/guide.ts verbatim"
    );
}

/// Split value blocks around `await`, bound methods, thenable jobs, taken
/// iterator elements and disposal keep every value rooted: the stress
/// builds, which collect while nearly every block is marked and trap on a
/// reachable freed block, print what the normal build prints.
#[test]
fn expression_forms_survive_collector_stress() {
    for (name, source, expected) in CASES {
        let expected: Vec<String> = expected
            .lines()
            .map(|line| format!("varta log: {line}"))
            .collect();
        for units in STRESS_UNITS {
            let run = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
            assert_eq!(
                (&run.lines, &run.error),
                (&expected, &None),
                "{name}: stress build ({units} units per allocation)"
            );
        }
    }
}

#[test]
fn unsupported_expression_forms_are_refused_by_name() {
    for (source, diagnostic) in [
        (
            "export function main(): void { const o = { n: 1, get two(): number { return this.n * 2; } }; console.log(String(o.two)); }",
            "getter or setter in an object literal",
        ),
        (
            "function tag(s: TemplateStringsArray): string { return s.raw[0]; } export function main(): void { console.log(tag`a`); }",
            "TemplateStringsArray.raw outside String.raw",
        ),
        (
            "async function v(): Promise<number> { return 1; } const x: number = await v(); export function main(): void { console.log(String(x)); }",
            "top-level await is not supported",
        ),
        (
            "async function v(): Promise<number> { return 1; } export async function main(): Promise<void> { switch (1) { case await v(): console.log('one'); } }",
            "await inside switch case test",
        ),
        (
            "class P { constructor(...xs: number[]) {} } export function main(): void { const xs = [1, 2]; new P(...xs); }",
            "spread to a class constructor",
        ),
        (
            "export function main(): void { const o = { n: 1, get(): number { return this.n; } }; const c = { ...o }; console.log(String(c.get())); }",
            "built by an object literal whose methods use this",
        ),
        (
            "class C { v = 1; f = () => this.v; } export function main(): void { console.log(String(new C().f())); }",
            "class field function without a return type annotation",
        ),
        (
            "export function main(): void { const k: string = 'a' + 'b'; const o = { [k]: 1 }; console.log(JSON.stringify(o)); }",
            "computed object key that is not a string or number constant",
        ),
        (
            "interface B { v: number; get: (this: B) => number } function make(): B { const b: B = { v: 1, get(this: B): number { return this.v; } }; return b; } export function main(): void { console.log(String(make().get())); }",
            "on a base other than a binding or this",
        ),
        (
            "export function main(): void { const o = { n: 1, *g(): Generator<number> { yield this.n; } }; console.log(String([...o.g()].length)); }",
            "generator method that uses this in an object literal",
        ),
        (
            "class A { constructor() { const f = (): boolean => new.target === A; console.log(String(f())); } } export function main(): void { new A(); }",
            "constructor closures cannot capture new.target",
        ),
        (
            "class B {} class A { constructor() { console.log(String(new.target === 1)); } } export function main(): void { new A(); }",
            "new.target compared with a value that is not a class",
        ),
    ] {
        refused(V38, source, diagnostic);
    }
}

/// V37 keeps every diagnostic the new forms had before v38.
#[test]
fn expression_forms_stay_refused_by_v37() {
    for (source, diagnostic) in [
        (
            "async function v(n: number): Promise<number> { return n; } export async function main(): Promise<void> { console.log(String(Math.abs(await v(1)))); }",
            "await inside call to String",
        ),
        (
            "export function main(): void { let i = 0; const x = (i++, i); console.log(String(x)); }",
            "unsupported expression",
        ),
        (
            "export function main(): void { const o = { ['a']: 1 }; console.log(String(o.a)); }",
            "computed object initializer",
        ),
        (
            "function* g(): Generator<number> { yield 1; } export function main(): void { const [a] = g(); console.log(String(a)); }",
            "array binding pattern requires a checked array",
        ),
        (
            "class C { v = 1; f = (): number => this.v; } export function main(): void { console.log(String(new C().f())); }",
            "class field f requires a type annotation or a literal initializer",
        ),
        (
            "class R { [Symbol.iterator](): Iterator<number> { return { next(): IteratorResult<number> { return { value: undefined, done: true }; } }; } } export function main(): void { }",
            "computed or numeric class member name",
        ),
        (
            "class A { constructor() { console.log(String(new.target === A)); } } export function main(): void { new A(); }",
            "new.target",
        ),
        (
            "export function main(): void { const o = { n: 2, d(): number { return this.n; } }; console.log(String(o.d())); }",
            "this or super in an object literal method",
        ),
        (
            "function f(a: number, b: number): number { return a + b; } export function main(): void { const t: [number, number] = [1, 2]; console.log(String(f(...t))); }",
            "spread operand requires a narrowed checked array",
        ),
        (
            "export function main(): void { console.log(String.raw`a${1}`); }",
            "tagged template literal (including String.raw)",
        ),
        (
            "interface T { then(ok: (v: number) => void): void } export async function main(): Promise<void> { const t: T = { then(ok: (v: number) => void): void { ok(1); } }; console.log(String(await t)); }",
            "structural thenable resolution",
        ),
        (
            "interface B { v: number } function get(this: B): number { return this.v; } export function main(): void { const o = { v: 1, get }; console.log(String(o.get())); }",
            "method-style call of a function field whose type declares a this parameter",
        ),
        (
            "const f = ([a, b]: [number, string]): string => b + a; export function main(): void { console.log(f([1, 'x'])); }",
            "parameter pattern does not match its checked aggregate type",
        ),
        (
            "class R { [Symbol.dispose](): void {} } export function main(): void { using r = new R(); }",
            "computed or numeric class member name",
        ),
    ] {
        refused(V37, source, diagnostic);
    }
}
