//! V38 `collection_hashing`: hashed `Map` and `Set` tables. Behaviour is
//! pinned against Node 24 (`collection_hashing_cases`, plus the v37
//! collection cases under v38), operations cost amortized constant fuel at
//! every key class (strings, numbers, booleans, BigInts and references), a
//! 300,000-entry map or set fits one application turn under the default
//! budgets, and `collection_fuel_measurements` prints the table recorded in
//! docs/development-plan.md (`--ignored --nocapture`).
use super::*;
use varta_common::budget::Budgets;
use varta_common::language::LanguageProfile as P;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}

/// Run `main` once under the v38 turn budgets with test-only `fuel`
/// (CLAUDE.md §11) and the production v38 memory. Returns the logs or the
/// failure, and the fuel consumed.
fn run_budgeted(bytes: &[u8], fuel: u64) -> (Result<Vec<String>, RuntimeError>, u64) {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(bytes).unwrap();
    let budgets = Budgets {
        fuel_per_turn: fuel,
        host_calls_per_turn: u32::MAX,
        ..Budgets::TURN
    };
    let mut execution =
        crate::execution::Execution::with_test_budgets(&runtime.engine, budgets).unwrap();
    execution.allow_test_wall_clock(std::time::Duration::from_secs(600));
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let result = (|| {
        let start = instance.get_func(&mut *store, "_start").unwrap();
        start.call(&mut *store, &[], &mut [])?;
        exceptions::finish(&instance, store)?;
        scheduler::checkpoint(&instance, store)?;
        Ok::<(), RuntimeError>(())
    })()
    .map_err(|error| resources::classify(&instance, store, scheduler::typed(error)));
    let used = fuel - store.get_fuel().unwrap();
    (
        result.map(|()| std::mem::take(&mut store.data_mut().output.lines)),
        used,
    )
}
/// Test-only fuel of the semantic cases: ten v38 turns.
const CASE_FUEL: u64 = 10 * varta_common::budget::TURN_FUEL;
fn run(source: &str, profile: P) -> Vec<String> {
    let (result, _) = run_budgeted(&compile(source, profile), CASE_FUEL);
    result.unwrap_or_else(|e| panic!("{profile:?}: {e:?}\n{source}"))
}

const SEMANTICS: &str = include_str!("collection_hashing_cases/semantics.ts");

/// Node 24 (`--experimental-transform-types`, `main()` appended).
#[test]
fn hashed_collections_match_node() {
    assert_eq!(
        run(SEMANTICS, V38),
        lines(&[
            "7 nan2 nan2 zero2 zero2",
            "NaN,0,1.5,Infinity,-Infinity,9007199254740992,-1e-300 true false big tiny",
            "true false true false 5",
            "false true 1",
            "2 true true,false",
            "4 2 3 5 6 false",
            "4 true true false true",
            "2 p2 q false",
            "2 true 1 2 2 true",
            "b5 c3 a4",
            "45 0,36,37,38,39,100 138,139,200 45",
            "xw 1",
            "3000 4351000 50 r2950,r2951,r2952",
            "1572 355360 key0,key5,key10,key15 -35 -7 false",
            "1334 true false 0,1.5,3,4.5",
            "3 4 4 3 true true",
            "4 z 5,3,4,0",
        ])
    );
}

/// The docs/language-guide.md example (Node 24: `50000 99999 row1 1000
/// false`), within one default v38 turn.
#[test]
fn hashed_collection_guide_example_matches_node() {
    let source = r#"export function main(): void {
  const index = new Map<string, number>();
  const seen = new Set<bigint>();
  for (let i = 0; i < 100000; i++) {
    index.set(`row${i}`, i);
    seen.add(BigInt(i % 1000));
  }
  for (let i = 0; i < 100000; i += 2) {
    index.delete(`row${i}`);
  }
  let first = "";
  for (const key of index.keys()) {
    first = key;
    break;
  }
  console.log(`${index.size} ${index.get("row99999")} ${first} ${seen.size} ${index.has("row4")}`);
}"#;
    let (result, used) = run_budgeted(&compile(source, V38), varta_common::budget::TURN_FUEL);
    assert_eq!(
        result.unwrap_or_else(|e| panic!("{e:?} at {used} fuel")),
        lines(&["50000 99999 row1 1000 false"])
    );
}

/// The v37 collection cases keep their Node 24 results under the v38
/// prelude (`collection_tests` pins them under v37).
#[test]
fn v37_collection_cases_keep_their_results_under_v38() {
    for (case, expected) in [
        (
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
            ][..],
        ),
        (
            include_str!("collection_cases/live_iteration.ts"),
            &[
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
            ][..],
        ),
    ] {
        assert_eq!(run(case, V38), lines(expected), "{case}");
    }
}

/// The v37 ceiling of 1024 entries stays: v37 compiles the frozen prelude.
#[test]
fn v37_keeps_its_collection_ceiling() {
    let source = "export function main(): void { const s = new Set<number>(); \
                  for (let i = 0; i < 1025; i++) { s.add(i); } console.log(`${s.size}`); }";
    let (result, _) = run_budgeted(&compile(source, V37), 100 * abi::FUEL);
    let error = result.unwrap_err();
    assert!(
        matches!(&error, RuntimeError::Uncaught(exception) if exception.value.to_string()
            .contains("Set maximum size exceeded")),
        "{error:?}"
    );
    assert_eq!(run(source, V38), lines(&["1025"]));
}

/// The table intrinsics belong to the engine prelude: a program module
/// cannot name them.
#[test]
fn table_intrinsics_stay_private_to_the_prelude() {
    for call in [
        "varta.collections.table(8)",
        "varta.collections.lookup(keys, t, 'a')",
        "varta.collections.insert(keys, t, 'a', 0)",
        "varta.collections.unlink(keys, t, 'a')",
        "varta.collections.indexKeys(keys, t)",
    ] {
        let source = format!(
            "export function main(): void {{ const keys: string[] = []; const t: number[] = []; \
             const n = {call}; console.log(`${{n}}`); }}"
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(&source, V38).is_err(),
            "{call}"
        );
    }
}

/// One program filling a collection with `n` entries by `operation` (`i` is
/// the loop index) after `setup`, then logging `report`.
fn program(n: usize, setup: &str, operation: &str, report: &str) -> String {
    format!(
        "export function main(): void {{ {setup} for (let i = 0; i < {n}; i++) {{ {operation} }} \
         console.log(`${{{report}}}`); }}"
    )
}

/// Workloads of every key class: `(name, setup, operation, report)`.
const WORKLOADS: [(&str, &str, &str, &str); 8] = [
    (
        "Map<string, string> set",
        "const m = new Map<string, string>();",
        "m.set(`k${i}`, `v${i}`);",
        "m.size",
    ),
    (
        "Set<string> add",
        "const s = new Set<string>();",
        "s.add(`k${i}`);",
        "s.size",
    ),
    (
        "Map<number, number> set, get, delete",
        "const m = new Map<number, number>(); let t = 0;",
        "m.set(i * 0.5, i); t += m.get(i * 0.25) ?? 0; if (i % 2 === 0) { m.delete(i * 0.5 - 1); }",
        "m.size + t",
    ),
    (
        "Set<bigint> add",
        "const s = new Set<bigint>(); let b = 0n;",
        "s.add(b); b += 1n;",
        "s.size",
    ),
    (
        "Map<object, number> set",
        "const keys: { i: number }[] = []; const m = new Map<{ i: number }, number>();",
        "const k = { i }; keys.push(k); m.set(k, i);",
        "m.size + m.get(keys[7])!",
    ),
    (
        "Set<number> queue",
        "const s = new Set<number>([0]); let t = 0;",
        "for (const first of s) { t += first; s.delete(first); break; } s.add(i + 1);",
        "s.size + t",
    ),
    (
        "two string arrays (comparison)",
        "const a: string[] = []; const b: string[] = [];",
        "a.push(`k${i}`); b.push(`v${i}`);",
        "a.length + b.length",
    ),
    (
        "dictionary set (comparison)",
        "const d: { [key: string]: string } = {};",
        "d[`k${i}`] = `v${i}`;",
        "Object.keys(d).length",
    ),
];

/// Test-only fuel of one measured workload: twenty v38 turns.
const MEASURED_FUEL: u64 = 20 * varta_common::budget::TURN_FUEL;

/// The fuel of `workload` over `n` entries, or its failure.
fn workload_fuel(n: usize, workload: (&str, &str, &str, &str)) -> Result<u64, String> {
    let (name, setup, operation, report) = workload;
    let source = program(n, setup, operation, report);
    let (result, used) = run_budgeted(&compile(&source, V38), MEASURED_FUEL);
    result
        .map(|_| used)
        .map_err(|e| format!("{name} ({n}): {e} at {used} fuel"))
}

#[test]
#[ignore = "measurement: prints the collection fuel table (--ignored --nocapture)"]
fn collection_fuel_measurements() {
    println!("| workload | 1,000 | 10,000 | 100,000 | 300,000 | per entry at 300,000 |");
    for workload in WORKLOADS {
        let fuel: Vec<Result<u64, String>> = [1_000, 10_000, 100_000, 300_000]
            .into_iter()
            .map(|n| workload_fuel(n, workload))
            .collect();
        let cells: Vec<String> = fuel
            .iter()
            .map(|fuel| match fuel {
                Ok(fuel) => format!("{:.1}M", *fuel as f64 / 1e6),
                Err(error) => error.clone(),
            })
            .collect();
        let per = fuel[3]
            .as_ref()
            .map(|fuel| format!("{:.0}", *fuel as f64 / 300_000.0))
            .unwrap_or_default();
        println!("| {} | {} | {per} |", workload.0, cells.join(" | "));
    }
}

/// Ten times the entries cost under twenty times the fuel for every key
/// class (the v37-layout prelude chained every BigInt key in one bucket; the
/// remaining growth is the collector's work over a larger live heap).
#[test]
fn collection_operations_cost_amortized_constant_fuel() {
    for workload in WORKLOADS {
        let small = workload_fuel(2_000, workload).unwrap();
        let large = workload_fuel(20_000, workload).unwrap();
        let ratio = large as f64 / small as f64;
        assert!(
            ratio < 20.0,
            "{}: 2k entries {small} fuel, 20k entries {large} fuel ({ratio:.1}x)",
            workload.0
        );
    }
}

fn event(record: &str) -> HostEvent {
    HostEvent {
        source: "ui".into(),
        record: record.into(),
        gesture: false,
        coalesce: false,
    }
}

/// The reported pattern: an application handler fills a `Map` and a `Set`
/// with 300,000 entries, each in one turn under the default v38 budgets
/// (the v37-layout prelude exhausted the turn fuel on either).
#[test]
fn three_hundred_thousand_entries_fit_one_turn_under_default_budgets() {
    let source = r#"
export function main(): void {
  varta.events.listen('ui', (record: string) => {
    if (record === 'map') {
      const m = new Map<string, string>();
      for (let i = 0; i < 300000; i++) { m.set(`k${i}`, `v${i}`); }
      console.log(`map ${m.size} ${m.get('k299999')}`);
    } else {
      const s = new Set<string>();
      for (let i = 0; i < 300000; i++) { s.add(`k${i}`); }
      console.log(`set ${s.size} ${s.has('k123456')}`);
    }
  });
}
"#;
    let bytes = compile(source, V38);
    let mut app = VartaRuntime::new()
        .start_application(&bytes, ApplicationOptions::default())
        .unwrap();
    for record in ["map", "set"] {
        assert_eq!(app.state(), InstanceState::Running, "{:?}", app.failure());
        let _ = app.deliver(event(record));
        while let Step::Turn(_) = app.step(None) {}
    }
    assert_eq!(app.failure().map(ToString::to_string), None);
    assert_eq!(
        app.take_output(),
        lines(&["map 300000 v299999", "set 300000 true"])
    );
    let report = app.budget_report();
    assert_eq!(
        report.budgets.fuel_per_turn,
        varta_common::budget::TURN_FUEL
    );
    let turns: Vec<u64> = report.turns.iter().map(|turn| turn.fuel).collect();
    println!("turn fuel: {turns:?}");
    let largest = turns.iter().copied().max().unwrap();
    assert!(
        largest < varta_common::budget::TURN_FUEL,
        "turn fuel {turns:?}"
    );
}
