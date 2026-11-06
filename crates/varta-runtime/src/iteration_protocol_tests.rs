//! V38 iteration protocol (`iteration_protocol`): iterable classes through
//! `[Symbol.iterator]()` generator methods, array literal spreads and
//! `Array.from` of arrays, strings, generators, collections and iterable
//! instances, synchronous `yield*` and the `Iterator`/`IterableIterator`/
//! `Iterable` annotations (`iteration_protocol_cases/*.ts`). Every `*.expected` file is
//! the output of Node 24.20.0 running the same program with `await main();`
//! appended, recorded once and checked in. Every program runs under the
//! production budgets and again in collector stress builds; the refused
//! forms fail with their named diagnostics, and v37 keeps rejecting every
//! new form with its old diagnostic.
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

const CASES: &[(&str, &str, &str)] = &[
    (
        "classes",
        include_str!("iteration_protocol_cases/classes.ts"),
        include_str!("iteration_protocol_cases/classes.expected"),
    ),
    (
        "spreads",
        include_str!("iteration_protocol_cases/spreads.ts"),
        include_str!("iteration_protocol_cases/spreads.expected"),
    ),
    (
        "delegation",
        include_str!("iteration_protocol_cases/delegation.ts"),
        include_str!("iteration_protocol_cases/delegation.expected"),
    ),
    (
        "closures",
        include_str!("iteration_protocol_cases/closures.ts"),
        include_str!("iteration_protocol_cases/closures.expected"),
    ),
    (
        "strings",
        include_str!("iteration_protocol_cases/strings.ts"),
        include_str!("iteration_protocol_cases/strings.expected"),
    ),
    // The `docs/language-guide.md` example, verbatim.
    (
        "guide",
        include_str!("iteration_protocol_cases/guide.ts"),
        include_str!("iteration_protocol_cases/guide.expected"),
    ),
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

fn case(name: &str) -> (&'static str, &'static str) {
    CASES
        .iter()
        .find(|(case, ..)| *case == name)
        .map(|(_, source, expected)| (*source, *expected))
        .unwrap_or_else(|| panic!("missing case {name}"))
}

/// The program's log equals the Node output recorded for `name`.
fn matches_node(name: &str) {
    let (source, expected) = case(name);
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
fn iterable_classes_match_node() {
    matches_node("classes");
}

#[test]
fn array_spreads_and_array_from_match_node() {
    matches_node("spreads");
}

#[test]
fn yield_star_delegation_matches_node() {
    matches_node("delegation");
}

#[test]
fn generator_method_closures_match_node() {
    matches_node("closures");
}

#[test]
fn string_code_points_match_node() {
    matches_node("strings");
}

#[test]
fn iteration_language_guide_example_matches_node() {
    matches_node("guide");
}

/// `x[Symbol.asyncIterator]()` calls the well-known async method like
/// `for await` does. Node 24 prints the same lines.
#[test]
fn explicit_async_iterator_calls_match_node() {
    let source = "class Feed { async *[Symbol.asyncIterator](): AsyncGenerator<string> { yield 'a'; yield 'b'; } }
         export async function main(): Promise<void> {
           const it = new Feed()[Symbol.asyncIterator]();
           const first = await it.next();
           const second = await it.next();
           const third = await it.next();
           console.log(`${first.value} ${second.value} ${third.done}`);
           for await (const x of new Feed()) console.log(x);
         }";
    assert_eq!(logs(source), ["a b true", "a", "b"]);
    refused(V37, source, "computed class member access");
}

/// Spreads, delegations and generator method bodies keep every source,
/// result array and observed input rooted: the stress builds, which collect
/// while nearly every block is marked and trap on a reachable freed block,
/// print what the normal build prints.
#[test]
fn iteration_survives_collector_stress() {
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

/// A hole of an element type without `undefined` is a `TypeError` in a
/// spread, `Array.from` and `yield*`, as in `for...of`; with `undefined` in
/// the element type it reads as `undefined` (see `spreads.ts`).
#[test]
fn spread_holes_throw_like_for_of() {
    let source = "function* all(xs: number[]): Generator<number> { yield* xs; }
         export function main(): void {
           const xs: number[] = [1, , 3];
           try { console.log(`${[...xs].length}`); } catch (e) { console.log(`spread ${(e as TypeError).message}`); }
           try { console.log(`${Array.from(xs).length}`); } catch (e) { console.log(`from ${(e as TypeError).message}`); }
           try { console.log(`${[...all(xs)].length}`); } catch (e) { console.log(`yield* ${(e as TypeError).message}`); }
         }";
    assert_eq!(
        logs(source),
        [
            "spread array element is a hole",
            "from array element is a hole",
            "yield* array element is a hole",
        ]
    );
}

#[test]
fn unsupported_iteration_forms_are_refused_by_name() {
    for (source, diagnostic) in [
        (
            "class C { n = 1; } export function main(): void { for (const x of new C()) console.log(`${x}`); }",
            "for...of over a class instance: the class declares no [Symbol.iterator]() method",
        ),
        (
            "class C { get [Symbol.iterator](): number { return 1; } } export function main(): void { console.log(`${[...new C()].length}`); }",
            "[Symbol.iterator]() must be a method without parameters returning a synchronous generator",
        ),
        (
            "class C { *[Symbol.iterator](n: number): Generator<number> { yield n; } } export function main(): void { console.log(`${[...new C()].length}`); }",
            "[Symbol.iterator]() must be a method without parameters returning a synchronous generator",
        ),
        (
            "async function* a(): AsyncGenerator<number> { yield 1; } export function main(): void { console.log(`${[...a()].length}`); }",
            "array spread: an async generator is not iterable synchronously",
        ),
        (
            "export function main(): void { const xs: number[] = [1]; console.log(`${[...xs, , 2].length}`); }",
            "array literal hole beside a spread element",
        ),
        (
            "export function main(): void { const xs: number[] = [1]; const ys: string[] = [...xs]; console.log(ys[0]); }",
            "array element type mismatch: expected string, found number",
        ),
        (
            "function f(xs: Iterable<number>): number { return 0; } export function main(): void { console.log(`${f([1])}`); }",
            "Iterable<T> annotation",
        ),
        (
            "class C implements Iterable<string> { *[Symbol.iterator](): Generator<number> { yield 1; } } export function main(): void { }",
            "class C incorrectly implements Iterable",
        ),
        (
            "class C { *each(...xs: number[]): Generator<number> { yield 1; } } export function main(): void { }",
            "generator method with optional or rest parameters",
        ),
        (
            "async function* a(): AsyncGenerator<number> { yield 1; } function* g(): Generator<number> { yield* a(); } export function main(): void { }",
            "yield*: an async generator is not iterable synchronously",
        ),
        (
            "function* inner(): Generator<number, void, number> { yield 1; } function* g(): Generator<number> { yield* inner(); } export function main(): void { }",
            "yield* over a generator requires TNext to accept undefined",
        ),
        (
            "function* inner(): Generator<number, number> { yield 1; return 2; } function* g(): Generator<number, string> { yield* inner(); return 'g'; } export function main(): void { }",
            "yield* forwards what this generator receives from return() to its source",
        ),
        (
            "function* inner(): Generator<string> { yield 'a'; } function* g(): Generator<number> { yield* inner(); } export function main(): void { }",
            "yield* source yields string",
        ),
    ] {
        refused(V38, source, diagnostic);
    }
}

/// V37 keeps every diagnostic the new forms had before v38.
#[test]
fn iteration_forms_stay_refused_by_v37() {
    for (source, diagnostic) in [
        (
            "class C { *[Symbol.iterator](): Generator<number> { yield 1; } } export function main(): void { }",
            "computed or numeric class member name",
        ),
        (
            "class C { *each(): Generator<number> { yield 1; } } export function main(): void { for (const x of new C().each()) console.log(`${x}`); }",
            "generator method",
        ),
        (
            "export function main(): void { const xs: number[] = [1]; console.log(`${[...xs].length}`); }",
            "array spread",
        ),
        (
            "export function main(): void { const xs: number[] = [1]; console.log(`${Array.from(xs).length}`); }",
            "Array.from of a value other than a Map, Set, Uint8Array or their iterators",
        ),
        (
            "function* g(): Generator<number> { yield* [1]; } export function main(): void { }",
            "yield* delegation",
        ),
        (
            "function* g(): Iterator<number> { yield 1; } export function main(): void { }",
            "Iterator",
        ),
        (
            "class C implements Iterable<number> { n = 1; } export function main(): void { }",
            "Iterable",
        ),
    ] {
        refused(V37, source, diagnostic);
    }
}
