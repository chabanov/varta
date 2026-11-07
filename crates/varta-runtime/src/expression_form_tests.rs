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
    case!("template_objects"),
    case!("spreads"),
    case!("receivers"),
    case!("object_methods"),
    case!("method_receivers"),
    case!("accessors"),
    case!("accessor_order"),
    case!("accessor_json"),
    case!("spread_order"),
    case!("accessor_cycles"),
    case!("accessor_holders"),
    case!("accessor_unknown"),
    case!("accessor_spreads"),
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
    // A logged message may span several lines.
    let actual = logs(source).join("\n");
    let actual: Vec<&str> = actual.lines().collect();
    assert_eq!(actual, expected, "{name}");
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

/// GetTemplateObject: one frozen strings array per site with its `raw`
/// array, the same object at every evaluation of the site.
#[test]
fn template_objects_are_cached_per_site_like_node() {
    matches_node("template_objects");
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

/// A method sees the receiver it is called on: spread copies, methods
/// moved between objects, extracted methods (`undefined`) and enumeration.
/// Getters and setters of object literals are accessor properties: reads,
/// assignments, compound assignments, views, spreads, own keys, entries and
/// `JSON.stringify` go through them.
#[test]
fn object_literal_accessors_match_node() {
    matches_node("accessors");
}

/// Own keys of an object with accessors follow OrdinaryOwnPropertyKeys
/// (integer keys ascending, then creation order), and `Object.values` and
/// `Object.entries` call each getter when its key is reached.
#[test]
fn accessor_keys_and_getter_calls_follow_node_order() {
    matches_node("accessor_order");
}

/// `JSON.stringify` calls a getter once per occurrence of its object, in
/// SerializeJSONProperty order, and writes keys in the objects' own order.
/// Class instances whose fields hold objects with accessors serialize their
/// own fields (a subclass instance its own class's), and such objects held
/// by `unknown` (directly, in arrays, records and class instances) are read
/// through their getters by `JSON.stringify` and JSON-object reads.
#[test]
fn accessor_objects_in_instances_and_unknown_match_node() {
    matches_node("accessor_holders");
    matches_node("accessor_unknown");
}

/// Spreads in a literal with getters and setters copy data properties as
/// CopyDataProperties does; a later accessor or spread replaces a property
/// in its first position.
#[test]
fn spreads_in_accessor_literals_match_node() {
    matches_node("accessor_spreads");
}

/// SerializeJSONObject/SerializeJSONArray step 1: an ancestor reached
/// again (here through getters) throws V8's circular-structure `TypeError`
/// with its path; a repeated object that is no ancestor is serialized again.
#[test]
fn json_cycles_through_getters_throw_like_node() {
    matches_node("accessor_cycles");
}

/// CopyDataProperties: spreads create their keys in the source's own-key
/// order (integer keys first), literal members in place, and rest patterns
/// and `Object.assign` keep it.
#[test]
fn spreads_copy_keys_in_source_order_like_node() {
    matches_node("spread_order");
}

#[test]
fn json_of_accessor_objects_matches_node() {
    matches_node("accessor_json");
}

#[test]
fn object_literal_methods_see_their_receiver_like_node() {
    matches_node("method_receivers");
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
        let expected: Vec<&str> = expected.lines().collect();
        for units in STRESS_UNITS {
            let run = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
            // A logged message may span several lines.
            let logged = run
                .lines
                .iter()
                .map(|line| {
                    line.strip_prefix("varta log: ")
                        .unwrap_or_else(|| panic!("not a log line: {line}"))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let actual: Vec<&str> = logged.lines().collect();
            assert_eq!(
                (&actual, &run.error),
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
            "export function main(): void { const o = { n: 1, set two(v: number) { this.n = v; } }; o.two = 2; console.log(String(o.n)); }",
            "setter two without a getter",
        ),
        (
            "export function main(): void { const o = { get x(): number { return 1; } }; const u: unknown = o; console.log(u); }",
            "of a unknown that may hold an object with getters",
        ),
        (
            "interface O { n?: number; readonly two: number } export function main(): void { const base: { n?: number } = {}; const o: O = { ...base, get two(): number { return 2; } }; console.log(String(o.two)); }",
            "with optional members or of a class instance into an object literal with getters or setters",
        ),
        (
            "function tag(s: TemplateStringsArray): string { s.raw = ['x']; return s[0]; } export function main(): void { console.log(tag`a`); }",
            "cannot assign to raw because it is a read-only property",
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
            "export function main(): void { const base = { n: 1 }; const o = { ...base, get(): number { return this.n; } }; console.log(String(o.get())); }",
            "spread in an object literal whose methods use this",
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
            "export function main(): void { const o = { n: 2, get d(): number { return 2; } }; console.log(String(o.d)); }",
            "object method, getter or setter",
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
