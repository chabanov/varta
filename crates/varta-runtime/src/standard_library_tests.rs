//! V38 standard library (`standard_library`, K9 D): the ECMAScript 2025
//! Array copying/reordering methods with V8's sort order, iterator helpers,
//! Set methods, `Object.groupBy`/`Map.groupBy`/`Object.fromEntries`,
//! `JSON.stringify` property lists and indentation, `AggregateError`, mixed
//! `Promise.reject` combinator arrays, `Math.max`/`min` spreads and the
//! remaining String methods (`standard_library_cases/*.ts`). Every
//! `*.expected` file is the output of Node 24.20.0 running the same program
//! with `await main();` appended (with `TZ=UTC`), recorded once and checked
//! in. Every program runs under the production budgets and again in
//! collector stress builds; the refused forms fail with their named
//! diagnostics, and v37 keeps rejecting every new form.
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
            include_str!(concat!("standard_library_cases/", $name, ".ts")),
            include_str!(concat!("standard_library_cases/", $name, ".expected")),
        )
    };
}

const CASES: &[(&str, &str, &str)] = &[
    case!("sorting"),
    case!("arrays"),
    case!("collections"),
    case!("strings"),
    case!("iterators"),
    case!("json"),
    case!("errors"),
    case!("promises"),
    case!("math"),
    case!("dates"),
    // The `docs/language-guide.md` example, verbatim.
    case!("guide"),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

/// The logged text as the lines Node prints: a logged string with line
/// breaks (indented JSON) is several lines.
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

/// Comparator call sequences, comparator counts of every size class,
/// stability, an inconsistent comparator, holes and `undefined`: V8's
/// TimSort order exactly.
#[test]
fn sorting_matches_node() {
    matches_node("sorting");
}

#[test]
fn array_methods_match_node() {
    matches_node("arrays");
}

#[test]
fn set_methods_and_grouping_match_node() {
    matches_node("collections");
}

#[test]
fn string_methods_match_node() {
    matches_node("strings");
}

#[test]
fn iterator_helpers_match_node() {
    matches_node("iterators");
}

#[test]
fn json_property_lists_and_indentation_match_node() {
    matches_node("json");
}

#[test]
fn aggregate_errors_match_node() {
    matches_node("errors");
}

#[test]
fn mixed_rejection_combinators_match_node() {
    matches_node("promises");
}

#[test]
fn math_spreads_match_node() {
    matches_node("math");
}

/// The complete `Date` in the UTC zone (no host zone is granted): Node with
/// `TZ=UTC` prints the same lines.
#[test]
fn dates_match_node_in_utc() {
    matches_node("dates");
}

#[test]
fn standard_library_guide_example_matches_node() {
    matches_node("guide");
}

/// Sorts, splices, iterator helpers and groupings keep every work array,
/// source and result rooted: the stress builds, which collect while nearly
/// every block is marked and trap on a reachable freed block, print what the
/// normal build prints.
#[test]
fn standard_library_survives_collector_stress() {
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

/// A copying method that reads a hole of an element type without
/// `undefined` throws `TypeError`, as `for...of` does (JavaScript reads
/// `undefined`, which such an array cannot hold); with `undefined` in the
/// element type the hole reads as `undefined` (see `sorting.ts`).
#[test]
fn copied_holes_throw_like_for_of() {
    let source = "export function main(): void {
           const xs: number[] = [3, 1];
           xs[3] = 2;
           try { console.log(xs.toSorted((a, b) => a - b).join(',')); } catch (e) { console.log(`toSorted ${(e as TypeError).name}`); }
           try { console.log(xs.toReversed().join(',')); } catch (e) { console.log(`toReversed ${(e as TypeError).name}`); }
           xs.sort((a, b) => a - b);
           console.log(`${xs.length} ${String(Object.hasOwn(xs, 3))} ${xs[0]},${xs[1]},${xs[2]}`);
         }";
    assert_eq!(
        logs(source),
        [
            "toSorted TypeError",
            "toReversed TypeError",
            "4 false 1,2,3"
        ]
    );
}

#[test]
fn unsupported_standard_forms_are_refused_by_name() {
    for (source, diagnostic) in [
        (
            "interface P { x: number } export function main(): void { const ps: P[] = [{ x: 1 }]; ps.sort(); }",
            "Array.prototype.sort without a comparator of non-primitive elements",
        ),
        (
            "export function main(): void { const d = 1; const xs: number[][] = [[1]]; console.log(xs.flat(d).length); }",
            "Array.prototype.flat with a depth other than a number literal or Infinity",
        ),
        (
            "export function main(): void { const xs = [1, 2]; console.log(xs.flatMap((x) => (x > 1 ? [x] : x)).length); }",
            "Array.prototype.flatMap with a callback returning both arrays and other values",
        ),
        (
            "export function main(): void { const xs: number[][] = [[1]]; const ys = [2]; console.log(xs.concat(ys).length); }",
            "Array.prototype.concat of an array whose elements are arrays",
        ),
        (
            "export function main(): void { const a = new Set([1]); const b = new Set(['x']); console.log(a.union(b).size); }",
            "Set method argument",
        ),
        (
            "function* g(): Generator<number, string> { yield 1; return 'done'; } export function main(): void { console.log(g().toArray().length); }",
            "Iterator.prototype.toArray of a generator that returns a value",
        ),
        (
            "export function main(): void { console.log(JSON.stringify({ a: 1 }, (k: string, v: unknown) => v)); }",
            "JSON.stringify replacer of type",
        ),
        (
            "function gap(): number { return 2; } export function main(): void { console.log(JSON.stringify({ a: 1 }, null, gap())); }",
            "JSON.stringify replacer or space argument other than a literal, binding or field read",
        ),
        (
            "export function main(): void { console.log(Object.getOwnPropertyNames([1]).length); }",
            "Object.getOwnPropertyNames of a value other than a record or dictionary",
        ),
        (
            "export function main(): void { const xs = new Set([1]); console.log(Math.max(...xs)); }",
            "to a Math function (a number[] is required)",
        ),
        (
            "export function main(): void { const o = { a: 1 }; const p = new Proxy(o, {}); console.log(p.a); }",
            "Proxy (traps would intercept property access",
        ),
        (
            "export function main(): void { console.log(Reflect.ownKeys({ a: 1 }).length); }",
            "Reflect (it operates on property descriptors",
        ),
        (
            "export function main(): void { const o = structuredClone({ a: [1] }); console.log(o.a.length); }",
            "structuredClone (a type-directed deep copy",
        ),
        (
            "export function main(): void { console.log(new Intl.NumberFormat('en').format(1)); }",
            "Intl (ECMA-402 needs locale data",
        ),
    ] {
        refused(V38, source, diagnostic);
    }
}

/// Weak collections are now part of v38, including empty class keys.
#[test]
fn weak_maps_with_empty_class_keys_run_in_v38() {
    let source = "class K {} export function main(): void { \
        const key = new K(); const m = new WeakMap<K, number>(); \
        m.set(key, 1); console.log(m.get(key)); }";
    assert_eq!(logs(source), ["1"]);
    assert!(varta_compiler::compile_typescript_with_profile(source, V37).is_err());
}

/// V37 keeps rejecting every form the stream admits.
#[test]
fn standard_forms_stay_refused_by_v37() {
    for source in [
        "export function main(): void { const xs = [2, 1]; xs.sort((a, b) => a - b); }",
        "export function main(): void { console.log([[1]].flat().length); }",
        "export function main(): void { console.log([1].toReversed().length); }",
        "export function main(): void { console.log(new Set([1]).union(new Set([2])).size); }",
        "export function main(): void { console.log(JSON.stringify({ a: 1 }, null, 2)); }",
        "export function main(): void { console.log(Array.of(1).length); }",
        "export function main(): void { console.log(Math.max(...[1, 2])); }",
        "export function main(): void { console.log(new AggregateError([]).message); }",
        "export function main(): void { console.log(Date.UTC(2020, 1)); }",
        "export function main(): void { console.log(new Date(2020, 1).getTime()); }",
        "export function main(): void { console.log(new Date(0).toString()); }",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V37).is_err(),
            "v37 compiled\n{source}"
        );
    }
}
