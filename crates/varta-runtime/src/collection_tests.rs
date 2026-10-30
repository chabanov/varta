//! V37 `collections`: `Map`, `Set`, tuples, `Uint8Array`, the UTF-8 text
//! codecs, base64, the `crypto` random functions, the `Date` subset and the
//! string/array built-ins of the engine preludes. Expected logs of the
//! `collection_cases` programs were produced by Node 24 (type stripping,
//! `main()` appended, `TZ=UTC`) and are pinned here, so the tests need no Node
//! installation (differential oracle).
use super::*;
use varta_common::{CapabilityGrant, CapabilityValue, language::LanguageProfile as P};

pub(crate) const V37: P = P::CheckedF64Utf16V37;
const V36: P = P::CheckedF64Utf16V36;

pub(crate) fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
pub(crate) fn module(source: &str) -> Vec<u8> {
    let bytes = compile(source, V37).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&module(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        lines(expected),
        "{source}"
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
/// Run with an explicit test-only fuel budget of `fuel` production budgets
/// (the churn and compaction cases exceed one on purpose).
fn run_with_fuel(source: &str, fuel: u64) -> Vec<String> {
    let bytes = module(source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(fuel * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    execution.store.data().output.lines.clone()
}
/// Fuel and host calls a program consumes under the production budgets.
fn cost(source: &str) -> (u64, u32) {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&module(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start
        .call(&mut *store, &[], &mut [])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    crate::scheduler::checkpoint(&instance, store).unwrap();
    (
        abi::FUEL - store.get_fuel().unwrap(),
        store.data().host_calls,
    )
}
fn grant(id: &str) -> CapabilityGrant {
    CapabilityGrant {
        id: id.into(),
        value: CapabilityValue::Boolean(true),
    }
}

#[test]
fn string_and_array_builtins_match_node() {
    run(
        include_str!("collection_cases/strings.ts"),
        &[
            "005",
            "abc12121",
            "   x",
            "abc",
            "ab",
            "HELLO, WORLD",
            "àéî straße",
            "STRASSE",
            "4",
            "a|b||c",
            "a-b-c",
            "a+b",
            "1",
            "0",
            "1",
            "abc",
            "1,2.5,0",
            "x and y",
            "p//q",
            "HiA",
            "😀A",
            "128512",
            "56832",
            "true",
            "00FF",
            "Invalid code point 1.5",
        ],
    );
}

#[test]
fn maps_and_sets_match_node() {
    run(
        include_str!("collection_cases/collections.ts"),
        &[
            "2 true false",
            "a",
            "b",
            "true false 3",
            "2 true true",
            "k 7 2",
            "k=7",
            "2 3 undefined",
            "x:3",
            "y:2",
            "y",
            "true",
            "a+b",
            "false 2",
            "2",
            "0",
        ],
    );
}

/// Iteration is live (additions visited, deletions skipped, `clear` keeps
/// later additions), keys compare by SameValueZero and identity, and the
/// entry vectors compact under churn without losing an iteration cursor.
#[test]
fn live_iteration_identity_keys_and_compaction_match_node() {
    assert_eq!(
        run_with_fuel(include_str!("collection_cases/live_iteration.ts"), 40),
        lines(&[
            "1,3,4,5",
            "bca",
            "1,9",
            "2 3 2 false",
            "3 1097,1098,1099 v",
            "110 2099 110",
            "no yes",
            "2 true false",
            "x=1",
            "y=2",
            "1",
            "2",
            "pp",
            "p/p",
            "x->1",
            "y->2",
        ])
    );
}

#[test]
fn bytes_text_codecs_base64_and_random_values_match_node() {
    run(
        include_str!("collection_cases/binary_text.ts"),
        &[
            "4 44 255 3 undefined",
            "44-255-3-0",
            "010200",
            "a%20b%26%C3%A9%E2%82%AC%F0%9F%98%80",
            "héllo €",
            "[][€A]",
            "�A��",
            "The encoded data was not valid for encoding utf-8",
            "aGVsbG8=",
            "hello",
            "InvalidCharacterError: The string to be decoded is not correctly encoded.",
            "16",
            "36 4 5",
            "2 2",
            "0,0,7,8,0",
            "3 false",
        ],
    );
}

#[test]
fn date_parsing_and_utc_fields_match_node() {
    run(
        include_str!("collection_cases/date.ts"),
        &[
            "2021-02-31 1614729600000",
            "2020-02-29T24:00:00Z 1583020800000",
            "2020-01-01T24:00:01Z NaN",
            "2020-01-01T10:00 1577872800000",
            "2020-01-01T10:00:00.123456Z 1577872800123",
            "+020000-01-01T00:00:00Z 568971820800000",
            "-000000-01-01T00:00:00Z NaN",
            "2020 1577836800000",
            "2020-13 NaN",
            "Sun, 06 Nov 1994 08:49:37 GMT 784111777000",
            "2020-01-01T10:00:00+05:30 1577853000000",
            "2020-01-01t10:00:00z 1577872800000",
            "2020-01-01 10:00:00Z 1577872800000",
            "junk NaN",
            "+275760-09-13T00:00:00.001Z NaN",
            "1970-01-01T00:00:00.000Z",
            "1969-12-31T23:59:59.999Z",
            "+275760-09-13T00:00:00.000Z",
            "-000001-01-01T00:00:00.000Z",
            "Invalid time value",
            "2 2024 2 5 6 7 8 9 1709618828009",
        ],
    );
}

#[test]
fn tuples_match_node() {
    run(
        include_str!("collection_cases/tuples.ts"),
        &[
            "3 2",
            "apples:4",
            "APPLES 6 2",
            "1..9",
            "edge true 1",
            "ann.3",
            "bob.5",
            "8",
            "ann=3",
            "x1 y2",
            "2",
            "apples",
            "3",
            "[\"apples\",3]",
            "{\"bounds\":[1,9]}",
            "3apples",
        ],
    );
}

/// Collection storage, keys and values are exact heap edges: churn that
/// forces many collections leaves every entry (string keys, record values,
/// nested sets and byte arrays) intact.
#[test]
fn collection_storage_survives_forced_collections() {
    let source = r#"
interface Item { name: string; tags: Set<string> }
function churn(): number {
  let text = "x";
  for (let i = 0; i < 12; i++) text = text + text;
  let total = 0;
  for (let i = 0; i < 60; i++) total += (text + "y").length;
  return total;
}
export function main(): void {
  const items = new Map<string, Item>();
  const bytes = new Uint8Array([7, 8, 9]);
  for (let i = 0; i < 12; i++) {
    const key = "key-" + String(i);
    const tags = new Set<string>(["tag-" + String(i), "shared"]);
    items.set(key, { name: "item-" + String(i), tags });
    churn();
  }
  items.delete("key-3");
  churn();
  let names = "";
  for (const [key, item] of items) {
    names += `${key}=${item.name}/${item.tags.size}/${item.tags.has("shared")} `;
  }
  console.log(names.trim());
  console.log(`${bytes.join("")} ${items.get("key-11")?.tags.has("tag-11")}`);
}
"#;
    let expected = (0..12)
        .filter(|i| *i != 3)
        .map(|i| format!("key-{i}=item-{i}/2/true"))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        run_with_fuel(source, 400),
        lines(&[expected.as_str(), "789 true"])
    );
}

/// A pilot-sized workload (status sets, a small cache map, hex encoding of
/// random bytes, a UUID, query encoding and SSE line joining) stays inside the
/// production fuel and host-call budgets.
#[test]
fn typical_collection_workload_stays_within_production_budgets() {
    let source = r#"
const RETRYABLE = new Set([408, 409, 429, 500, 502, 503, 504]);
function encode(value: string): string {
  let out = "";
  for (const byte of new TextEncoder().encode(value)) {
    out += "%" + byte.toString(16).toUpperCase().padStart(2, "0");
  }
  return out;
}
export function main(): void {
  let retryable = 0;
  const statuses = [200, 429, 503, 404];
  for (let i = 0; i < statuses.length; i++) if (RETRYABLE.has(statuses[i]!)) retryable++;
  const cache = new Map<string, number>();
  const keys = ["a", "b", "c", "a"];
  for (let i = 0; i < keys.length; i++) cache.set(keys[i]!, (cache.get(keys[i]!) ?? 0) + 1);
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  const hex = [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
  const id = crypto.randomUUID();
  const lines = "data: one\ndata: two".split("\n").map((line) => line.slice(6));
  console.log(`${retryable} ${cache.get("a")} ${hex.length} ${id.length} ${encode("a b").length} ${lines.join("|")}`);
}
"#;
    let (fuel, calls) = cost(source);
    assert!(fuel < abi::FUEL, "fuel {fuel}");
    assert!(calls < abi::MAX_HOST_CALLS / 4, "host calls {calls}");
    run(source, &["2 2 32 36 9 one|two"]);
}

/// ASCII case mapping is a guest scan; only text above U+007F takes one pure
/// host call, and per-unit host calls are never made.
#[test]
fn case_conversion_calls_the_host_only_for_non_ascii_text() {
    let ascii = r#"export function main(): void { let s = ""; for (let i = 0; i < 20; i++) s = ("Ab" + s).toUpperCase().toLowerCase(); console.log(s.slice(0, 4)); }"#;
    let (_, calls) = cost(ascii);
    // One host call: the final log.
    assert_eq!(calls, 1);
    let unicode = r#"export function main(): void { console.log("Ärger".toUpperCase() + "ÉCOLE".toLowerCase()); }"#;
    let (_, calls) = cost(unicode);
    assert_eq!(calls, 3);
    run(unicode, &["ÄRGERécole"]);
}

/// `Date.now()` and `new Date()` read the clock only under the exact
/// `time.now` grant; a denial ends the execution at the call, before any
/// later guest effect. Parsing and formatting need no grant.
#[test]
fn the_clock_requires_the_time_now_grant_before_any_effect() {
    let source = r#"
export function main(): void {
  console.log(new Date(0).toISOString());
  console.log("before");
  const now = Date.now();
  console.log(`${now > 1.7e12} ${new Date().getTime() >= now}`);
}
"#;
    let bytes = module(source);
    let runtime = VartaRuntime::new();
    let output = runtime
        .run_wasm_bytes(&bytes, &[grant("time.now")])
        .unwrap();
    assert_eq!(
        output.lines,
        lines(&["1970-01-01T00:00:00.000Z", "before", "true true"])
    );
    for grants in [vec![], vec![grant("time.timer")]] {
        let error = runtime.run_wasm_bytes(&bytes, &grants).unwrap_err();
        assert!(
            matches!(
                &error,
                RuntimeError::Policy(varta_common::VartaError::CapabilityDenied { capability, .. })
                    if capability == "time.now"
            ),
            "{error:?}"
        );
    }
    // A host ceiling without the grant denies what the package requested.
    let error = runtime
        .run_wasm_bytes_with_policy(&bytes, &[grant("time.now")], &[])
        .unwrap_err();
    assert!(matches!(error, RuntimeError::Policy(_)), "{error:?}");
}

/// Collections, `Uint8Array` and the v37 builtin classes have private state:
/// value layout 17 gives them no tag, so logging, JSON and `unknown` reject
/// every value that may hold one, at compile time.
#[test]
fn collection_values_stay_private_to_host_value_readers() {
    for (source, needle) in [
        (
            "export function main(): void { const s = new Set<number>(); console.log(s); }",
            "logging of a value holding a Map",
        ),
        (
            "export function main(): void { const s: Set<number> | undefined = new Set<number>(); console.log(s); }",
            "logging of a value holding a Map",
        ),
        (
            "export function main(): void { const m = new Map<string, number>(); console.log(JSON.stringify({ m })); }",
            "JSON.stringify of a value holding a Map",
        ),
        (
            "export function main(): void { const b = new Uint8Array(2); const u: unknown = b; console.log(typeof u); }",
            "conversion to unknown of a value holding a Map",
        ),
        (
            "export function main(): void { const u: unknown = new TextEncoder(); console.log(typeof u); }",
            "conversion to unknown",
        ),
        (
            "export function main(): void { console.log(JSON.stringify(new Date(0))); }",
            "JSON.stringify of a value holding",
        ),
    ] {
        let message = rejected(source, V37);
        assert!(message.contains(needle), "{message}\n{source}");
    }
}

#[test]
fn unsupported_collection_forms_are_rejected_with_their_reason() {
    for (source, needle) in [
        (
            "export function main(): void { const s = new Set<string | number>(); }",
            "Set key type",
        ),
        (
            "export function main(): void { const s = new Set(); }",
            "cannot infer the type arguments of new Set()",
        ),
        (
            "export function main(): void { const m = new Map([]); }",
            "cannot infer the type arguments of new Map([])",
        ),
        (
            "export function main(): void { const s = new Set<number>(); s.forEach((v, k, set) => {}); }",
            "forEach callback's third (collection) parameter",
        ),
        (
            "export function main(): void { const s = new Set<number>(); s.union(s); }",
            "Set.prototype.union",
        ),
        (
            "export function main(): void { const b = new Uint8Array(4); b.subarray(1); }",
            "subarray",
        ),
        (
            "export function main(): void { const b = new Uint8Array(new Set<number>()); }",
            "Uint8Array constructor argument",
        ),
        (
            "export function main(): void { const t: [string, number] = ['a', 1]; console.log(t[2]); }",
            "tuple index 2 is out of range",
        ),
        (
            "export function main(): void { const t: [string, number] = ['a', 1]; t[0] = 'b'; }",
            "assignment to a tuple element",
        ),
        (
            "export function main(): void { const t: [string, number] = ['a']; }",
            "tuple literal has 1 elements",
        ),
        (
            "export function main(): void { const t: [string, ...number[]] = ['a']; }",
            "tuple rest element",
        ),
        (
            "interface R { a: number } export function main(): void { const r: R[] = [{ a: 1 }]; console.log(r.join()); }",
            "Array.prototype.join of non-primitive elements",
        ),
        (
            "export function main(): void { const n = 5; console.log(n.padStart(2)); }",
            "requires a string receiver",
        ),
        (
            "export function main(): void { console.log(Array.from([1])); }",
            "Array.from of a value other than",
        ),
        (
            "export function main(): void { console.log([1].concat([2]).length); }",
            "unsupported Array.prototype.concat",
        ),
    ] {
        let message = rejected(source, V37);
        assert!(message.contains(needle), "{message}\n{source}");
    }
}

/// V36 keeps rejecting every v37 collection form: no v37 prelude loads and
/// no tuple or collection type exists there.
#[test]
fn v36_keeps_rejecting_collection_forms() {
    for source in [
        "export function main(): void { const s = new Set<number>(); }",
        "export function main(): void { const t: [string, number] = ['a', 1]; }",
        "export function main(): void { console.log('a'.padStart(3)); }",
        "export function main(): void { console.log([1].join()); }",
        "export function main(): void { const d = new TextEncoder(); }",
        "export function main(): void { console.log(String(Date.now())); }",
        "export function main(): void { console.log(btoa('a')); }",
    ] {
        rejected(source, V36);
    }
}

/// Digests of these programs compiled under v36, recorded from the compiler
/// before the v37 collections stream (commit d2942ba): v36 loads no v37
/// prelude and keeps every byte.
const FROZEN_V36: [u64; 8] = [
    1361167432367252120,
    10132191227865720272,
    7430011093785533120,
    6200128730592170896,
    102813085301331532,
    2493880408488466042,
    12291709275887132004,
    6017947384852421744,
];
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn v36_modules_keep_their_bytes() {
    let digests: Vec<u64> = [
        include_str!("collection_cases/frozen_v36.ts"),
        include_str!("web_cases/headers_values.ts"),
        include_str!("web_cases/abort_flow.ts"),
        include_str!("function_field_cases/fields.ts"),
        include_str!("timer_cases/ordering.ts"),
        include_str!("async_generator_cases/features.ts"),
        include_str!("v35_cases/spread.ts"),
        include_str!("v35_cases/unknown_json.ts"),
    ]
    .iter()
    .map(|source| digest(&compile(source, V36).unwrap_or_else(|e| panic!("{e}\n{source}"))))
    .collect();
    assert_eq!(digests, FROZEN_V36);
}
