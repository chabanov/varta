//! V38 weak references (`weak_references`, K9 D5): `WeakMap`, `WeakSet`,
//! `WeakRef` and `FinalizationRegistry` over ephemeron tables that the
//! collector processes after marking (`weak_reference_cases/*.ts`). Every
//! `*.expected` file is the output of Node 24.20.0 running the same program
//! with `node --expose-gc` and `await main();` appended, recorded once and
//! checked in; the Varta builds expose the same `gc()` (a complete
//! collection) through `varta_compiler::with_exposed_gc`. Every program
//! also runs in collector stress builds; v37 keeps rejecting every new name.
use super::*;
use varta_common::language::LanguageProfile;
use varta_common::scheduler::TIMER_CAPABILITY;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
/// Work units per allocation of the stress builds.
const STRESS_UNITS: [u32; 2] = [2, 64];

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("weak_reference_cases/", $name, ".ts")),
            include_str!(concat!("weak_reference_cases/", $name, ".expected")),
        )
    };
}

const CASES: &[(&str, &str, &str)] = &[
    case!("collections"),
    case!("lifecycle"),
    case!("registry"),
    case!("guide"),
];

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::with_exposed_gc(|| {
        varta_compiler::compile_typescript_with_profile(source, V38)
    })
    .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn timer_grant() -> Vec<CapabilityGrant> {
    vec![CapabilityGrant {
        id: TIMER_CAPABILITY.into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }]
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

fn run(bytes: &[u8]) -> Result<RunOutput, RuntimeError> {
    scheduler::with_virtual_clock(|| VartaRuntime::new().run_wasm_bytes(bytes, &timer_grant()))
}

fn logs(source: &str) -> Vec<String> {
    let output = run(&compile(source)).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    printed(&output.lines)
}

fn expected(name: &str) -> (&'static str, Vec<String>) {
    let (_, source, expected) = CASES
        .iter()
        .find(|(case, ..)| *case == name)
        .unwrap_or_else(|| panic!("missing case {name}"));
    (source, expected.lines().map(str::to_owned).collect())
}

fn matches_node(name: &str) {
    let (source, expected) = expected(name);
    assert_eq!(logs(source), expected, "{name}");
}

#[test]
fn weak_collections_match_node() {
    matches_node("collections");
}

#[test]
fn weak_reference_lifecycle_matches_node() {
    matches_node("lifecycle");
}

#[test]
fn finalization_registry_edges_match_node() {
    matches_node("registry");
}

/// The `docs/language-guide.md` example, verbatim.
#[test]
fn weak_reference_guide_example_matches_node() {
    matches_node("guide");
}

/// Cases whose output does not depend on when collections run between the
/// explicit `gc()` calls.
const TIMING_FREE: [&str; 3] = ["collections", "lifecycle", "guide"];

/// Every case runs in stress builds, whose cycles run back to back and are
/// each verified by a complete collection that traps on any reachable freed
/// block (a table never names one). Where the output does not depend on the
/// extra collections, it is still Node's; the registry case observes
/// cleanups that a stress build's earlier collections legitimately advance.
#[test]
fn weak_references_survive_collector_stress() {
    for (name, source, expected) in CASES {
        let expected: Vec<String> = expected.lines().map(str::to_owned).collect();
        for units in STRESS_UNITS {
            let bytes = varta_compiler::with_collector_stress(units, || compile(source));
            let output = run(&bytes).unwrap_or_else(|e| panic!("{name} ({units}): {e:?}"));
            if TIMING_FREE.contains(name) {
                assert_eq!(
                    printed(&output.lines),
                    expected,
                    "{name}: stress build ({units} units per allocation)"
                );
            } else {
                assert_eq!(printed(&output.lines).len(), expected.len(), "{name}");
            }
        }
    }
}

fn report(bytes: &[u8]) -> RunReport {
    scheduler::with_virtual_clock(|| {
        VartaRuntime::new().run_with(
            bytes,
            RunOptions {
                requested: timer_grant(),
                ..RunOptions::default()
            },
        )
    })
}

/// Keys per turn and numbers in each value's payload of the churn program.
const CHURN_KEYS: usize = 100;
const CHURN_NUMBERS: usize = 512;
/// Turns of the churn runs.
const CHURN_TURNS: usize = 320;

/// The churn program: every turn makes `CHURN_KEYS` fresh keys whose
/// values hold their key and a payload, in one long-lived WeakMap; one key
/// in a thousand stays held (every key with `strong`). Prints the keys made
/// and the held keys whose value is still found.
fn churn_source(turns: usize, strong: bool) -> String {
    format!(
        r#"
class Key {{
  id: number;
  constructor(id: number) {{
    this.id = id;
  }}
}}
class Value {{
  key: Key;
  payload: number[];
  constructor(key: Key) {{
    this.key = key;
    this.payload = [];
    for (let i = 0; i < {CHURN_NUMBERS}; i++) {{
      this.payload.push(i);
    }}
  }}
}}
const map = new WeakMap<Key, Value>();
const survivors: Key[] = [];
let created = 0;
function churn(): void {{
  for (let i = 0; i < {CHURN_KEYS}; i++) {{
    const key = new Key(created);
    created++;
    __STORE__
    if ({strong} || created % 1000 === 0) {{
      survivors.push(key);
    }}
  }}
}}
function turn(): Promise<void> {{
  return new Promise<void>((resolve) => {{
    setTimeout(resolve, 0);
  }});
}}
export async function main(): Promise<void> {{
  for (let t = 0; t < {turns}; t++) {{
    churn();
    await turn();
  }}
  let kept = 0;
  for (const key of survivors) {{
    if (map.get(key)?.key === key) {{
      kept++;
    }}
  }}
  console.log(`${{created}} ${{kept}}`);
}}
"#
    )
    .replace("__STORE__", "map.set(key, new Value(key));")
}

/// The memory budget of the churn runs: a small fraction of what the long
/// run allocates.
const CHURN_MEMORY_BYTES: u64 = 16 << 20;

/// A churn run under [`CHURN_MEMORY_BYTES`].
fn churn(turns: usize, strong: bool) -> RunReport {
    let bytes = compile(&churn_source(turns, strong));
    scheduler::with_virtual_clock(|| {
        VartaRuntime::new().run_with(
            &bytes,
            RunOptions {
                requested: timer_grant(),
                budgets: varta_common::budget::BudgetRequest {
                    memory_bytes: Some(CHURN_MEMORY_BYTES),
                    ..Default::default()
                },
                ..RunOptions::default()
            },
        )
    })
}

/// A WeakMap value that refers to its own key keeps neither alive: many
/// turns of fresh keys whose values hold the key and a payload run within a
/// memory budget far below what they allocate, by the collections inside
/// the turns alone (no `gc()`), while every key held (what a strong
/// stand-in would do to the pairs) exhausts that budget.
#[test]
fn weak_map_values_do_not_keep_their_keys_alive() {
    let turns = CHURN_TURNS;
    let created = turns * CHURN_KEYS;
    let allocated = created * CHURN_NUMBERS * size_of::<f64>();
    assert!(allocated as u64 > 4 * CHURN_MEMORY_BYTES);
    let weak = churn(turns, false);
    assert!(weak.failure.is_none(), "{:?}", weak.failure);
    assert_eq!(
        printed(&weak.output.lines),
        [format!("{created} {}", created / 1000)]
    );
    assert!(weak.budget.unwrap().peak_memory_bytes as u64 <= CHURN_MEMORY_BYTES);
    let strong = churn(turns, true);
    let failure = strong
        .failure
        .expect("the strong churn exhausts its memory");
    assert!(
        matches!(&failure, RuntimeError::Budget(exhausted) if exhausted.budget == varta_common::budget::BudgetKind::Memory)
            || failure.to_string().contains("memory"),
        "{failure:?}"
    );
}

/// A chain of `__N__` ephemerons, each in its own WeakMap: only the first
/// key is held, and the weak list visits the maps newest first, so every
/// pass of the fixed point marks one more link and the collection makes
/// `__N__` passes.
const CHAIN: &str = r#"
class Link {
  id: number;
  constructor(id: number) {
    this.id = id;
  }
}
const maps: WeakMap<Link, Link>[] = [];
const head: Link[] = [];
function build(n: number): void {
  const links: Link[] = [];
  for (let i = 0; i <= n; i++) {
    links.push(new Link(i));
  }
  for (let i = 0; i < n; i++) {
    const map = new WeakMap<Link, Link>();
    map.set(links[i], links[i + 1]);
    maps.push(map);
  }
  head.push(links[0]);
}
function last(): number {
  let link = head[0];
  for (const map of maps) {
    link = map.get(link) ?? link;
  }
  return link.id;
}
export function main(): void {
  build(__N__);
  __GC__
  console.log(String(last()));
}
"#;

fn chain(links: usize, collect: bool) -> (Vec<String>, u64) {
    let source = CHAIN
        .replace("__N__", &links.to_string())
        .replace("__GC__", if collect { "gc();" } else { "" });
    let report = report(&compile(&source));
    assert!(report.failure.is_none(), "{:?}", report.failure);
    (
        printed(&report.output.lines),
        report.budget.unwrap().total_fuel,
    )
}

/// The ephemeron fixed point is guest work charged to the turn that
/// collects: it keeps every link of a chain reachable only through the
/// previous link's pair, it costs fuel growing with the passes a chain
/// needs (quadratically here, beyond the linear marking), the same fuel on
/// every run, and a turn whose fuel cannot pay for it fails with the fuel
/// budget instead of finishing the collection.
#[test]
fn the_ephemeron_fixed_point_is_charged_to_the_turn() {
    const SHORT: usize = 32;
    const LONG: usize = 128;
    let (short, short_fuel) = chain(SHORT, true);
    let (long, long_fuel) = chain(LONG, true);
    assert_eq!(
        (short, long),
        (vec![SHORT.to_string()], vec![LONG.to_string()])
    );
    let short_gc = short_fuel - chain(SHORT, false).1;
    let long_gc = long_fuel - chain(LONG, false).1;
    let ratio = (LONG / SHORT) as u64;
    assert!(
        long_gc > ratio * ratio / 2 * short_gc,
        "gc fuel {short_gc} for {SHORT} links, {long_gc} for {LONG}"
    );
    assert_eq!(chain(LONG, true).1, long_fuel);
    // Under a turn budget below the collection's cost, the turn fails on
    // fuel in the middle of the fixed point.
    let source = CHAIN
        .replace("__N__", &LONG.to_string())
        .replace("__GC__", "gc();");
    let report = scheduler::with_virtual_clock(|| {
        VartaRuntime::new().run_with(
            &compile(&source),
            RunOptions {
                budgets: varta_common::budget::BudgetRequest {
                    fuel_per_turn: Some(long_fuel - long_gc / 2),
                    ..Default::default()
                },
                ..RunOptions::default()
            },
        )
    });
    let failure = report
        .failure
        .expect("the collection exceeds the turn's fuel");
    assert!(
        matches!(&failure, RuntimeError::Budget(exhausted) if exhausted.budget == varta_common::budget::BudgetKind::Fuel),
        "{failure:?}"
    );
    assert!(report.output.lines.is_empty());
}

/// A cleanup callback that throws ends the instance with that exception
/// uncaught, as a timer callback does (Node reports an uncaught exception),
/// after what the cleanup job printed before it.
#[test]
fn a_throwing_cleanup_callback_is_an_uncaught_exception() {
    let source = r#"
class Item {
  id: number;
  constructor(id: number) {
    this.id = id;
  }
}
function register(registry: FinalizationRegistry<number>): void {
  registry.register(new Item(1), 1);
}
export async function main(): Promise<void> {
  const registry = new FinalizationRegistry<number>((held) => {
    console.log(`cleanup ${held}`);
    throw new RangeError(`held ${held}`);
  });
  register(registry);
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
  gc();
  console.log(String(registry.unregister(new Item(2))));
}
"#;
    let output = scheduler::with_virtual_clock(|| {
        let bytes = compile(source);
        let runtime = VartaRuntime::new();
        runtime.run_with(
            &bytes,
            RunOptions {
                requested: timer_grant(),
                ..RunOptions::default()
            },
        )
    });
    assert_eq!(printed(&output.output.lines), ["false", "cleanup 1"]);
    let failure = output
        .failure
        .expect("the callback's exception is uncaught");
    assert!(matches!(failure, RuntimeError::Uncaught(_)), "{failure:?}");
    assert!(failure.to_string().contains("held 1"), "{failure}");
}

/// The keys follow TypeScript's `WeakKey` (`lib.es2023`): objects and
/// symbols, also through the exported alias; a primitive key type is a
/// compile-time error of the type argument's constraint.
#[test]
fn weak_keys_are_objects_and_symbols() {
    let source = r#"
function holds(set: WeakSet<WeakKey>, key: WeakKey): boolean {
  return set.has(key);
}
export function main(): void {
  const key: WeakKey = Symbol('k');
  const object: WeakKey = { a: 1 };
  const set = new WeakSet<WeakKey>([key]);
  console.log(`${holds(set, key)} ${holds(set, object)} ${typeof key} ${typeof object}`);
}
"#;
    assert_eq!(logs(source), ["true false symbol object"]);
    for source in [
        "export function main(): void { const m = new WeakMap<number, string>(); }",
        "export function main(): void { const s = new WeakSet<string>(); }",
        "export function main(): void { const r = new WeakRef<bigint>(1n); }",
        "export function main(): void { const r = new WeakRef(1); }",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(source, V38)
            .err()
            .unwrap_or_else(|| panic!("compiled: {source}"))
            .to_string();
        assert!(
            error.contains("constraint") || error.contains("mismatch"),
            "{source}: {error}"
        );
    }
}

/// The checkpoint export exists exactly in programs that use weak
/// references, with the signature the hosts call.
#[test]
fn the_checkpoint_export_is_validated() {
    let weak = compile("export function main(): void { const s = new WeakSet<object>(); }");
    let plain = compile("export function main(): void { console.log('x'); }");
    let exports = |bytes: &[u8]| -> Vec<String> {
        wasmtime::Module::new(&wasmtime::Engine::default(), bytes)
            .unwrap()
            .exports()
            .map(|export| export.name().to_owned())
            .collect()
    };
    let name = varta_common::weak::CHECKPOINT_EXPORT;
    assert!(exports(&weak).iter().any(|export| export == name));
    assert!(!exports(&plain).iter().any(|export| export == name));
    VartaRuntime::new().prepare_module(&weak).unwrap();
}

#[test]
fn v37_rejects_weak_references() {
    for source in [
        "export function main(): void { const m = new WeakMap<object, number>(); }",
        "export function main(): void { const s = new WeakSet<object>(); }",
        "export function main(): void { const r = new WeakRef({}); }",
        "export function main(): void { const f = new FinalizationRegistry<string>((h) => {}); }",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V37).is_err(),
            "{source}"
        );
    }
}

/// Object integrity bits must never make ordinary objects look like weak tables.
#[test]
fn sealed_and_frozen_keys_survive_weak_collection() {
    let source = r#"
export function main(): void {
  const sealed = { value: 1 };
  const frozen = { value: 2 };
  const map = new WeakMap<object, { label: string }>();
  const set = new WeakSet<object>();
  map.set(sealed, { label: 'sealed' });
  map.set(frozen, { label: 'frozen' });
  set.add(sealed).add(frozen);
  Object.seal(sealed);
  Object.freeze(frozen);
  gc();
  sealed.value = 3;
  try { frozen.value = 4; } catch (error) {
    console.log(error instanceof TypeError);
  }
  gc();
  console.log(sealed.value, frozen.value, Object.isSealed(sealed), Object.isFrozen(frozen));
  console.log(map.get(sealed)?.label, map.get(frozen)?.label, set.has(sealed), set.has(frozen));
}
"#;
    let expected = ["true", "3 2 true true", "sealed frozen true true"];
    assert_eq!(logs(source), expected);
    for units in STRESS_UNITS {
        let bytes = varta_compiler::with_collector_stress(units, || compile(source));
        assert_eq!(printed(&run(&bytes).unwrap().lines), expected);
    }
}
