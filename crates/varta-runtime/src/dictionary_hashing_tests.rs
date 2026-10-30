//! V38 `dictionary_hashing` (value layout 18): hashed dictionaries and mixed
//! records. Behaviour is pinned against Node 24 (key order with array-index
//! keys, deletions, re-insertions, tombstones, resizes, hash collisions and
//! forced collections at large sizes), every table is validated against its
//! entries at the external boundaries (a corrupt table is rejected, never
//! trusted), and `dictionary_fuel_measurements` prints the fuel table
//! recorded in docs/development-plan.md (`--ignored --nocapture`).
use super::*;
use varta_common::{language::LanguageProfile as P, value};

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
/// Run under explicit test-only budgets (CLAUDE.md §11): `fuel` production
/// fuel budgets and `memory` bytes. Returns the logs or the failure, and the
/// fuel consumed.
fn run_budgeted(
    bytes: &[u8],
    fuel: u64,
    memory: usize,
) -> (Result<Vec<String>, RuntimeError>, u64) {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(bytes).unwrap();
    let budgets = varta_common::budget::Budgets {
        fuel_per_turn: fuel * abi::FUEL,
        memory_bytes: memory,
        host_calls_per_turn: u32::MAX,
        ..varta_common::budget::Budgets::PRODUCTION
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
    let used = fuel * abi::FUEL - store.get_fuel().unwrap();
    (
        result.map(|()| std::mem::take(&mut store.data_mut().output.lines)),
        used,
    )
}
fn run(source: &str, profile: P) -> Vec<String> {
    let (result, _) = run_budgeted(&compile(source, profile), 100, abi::MAX_MEMORY_BYTES);
    result.unwrap_or_else(|e| panic!("{profile:?}: {e:?}\n{source}"))
}

const ORDER: &str = include_str!("dictionary_hashing_cases/order.ts");
const COLLISIONS: &str = include_str!("dictionary_hashing_cases/collisions.ts");
const MIXED: &str = include_str!("dictionary_hashing_cases/mixed.ts");
const LARGE: &str = include_str!("dictionary_hashing_cases/large.ts");
const FRAMES: &str = include_str!("dictionary_hashing_cases/frames.ts");

#[test]
fn hashed_dictionaries_keep_javascript_key_order() {
    let expected = lines(&[
        r#"["k0","k1","k2","k3","k4","k5","k6","k7","k8","k9","k10","k11","k12","k13","k14","k15","k16","k17","k18","k19"] [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19]"#,
        r#"["3","7","10","4294967294","k0","k1","k2","k3","k4","k5","k6","k7","k8","k9","k10","k11","k12","k13","k14","k15","k16","k17","k18","k19","01","4294967295"] [103,107,110,4294967294,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,1001,4294967295]"#,
        r#"["3","7","10","4294967294","k1","k2","k4","k5","k6","k7","k8","k9","k10","k11","k12","k13","k14","k15","k16","k17","k18","k19","01","4294967295","k3"] [103,77,110,4294967294,1,2,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,1001,4294967295,33]"#,
        "true false true true 19",
        "56 8589941210 3,7,10,4294967294,k1,k5,k7,k9",
        "604",
        "3,7,10,4294967294,k1,k5 -1 -3",
        r#"{"2":"y","last":"x"} 2"#,
        "0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,x",
        "32 30,x,y",
    ]);
    assert_eq!(run(ORDER, V38), expected);
    // V37 keeps its linear value17 dictionaries with the same behaviour.
    assert_eq!(run(ORDER, V37), expected);
}

#[test]
fn hashed_dictionaries_probe_past_collisions_and_tombstones() {
    let expected = lines(&[
        "0,1,2,3,4,5, undefined undefined",
        "01-34-67891011 1 false",
        "cE8ua,ca9oi,cE8ub,ca9oh,cE8uc,ca9oj,h31,h97,h158,h161,h217,h239,h266,h288,h330,h345,h363,h392",
        "53 18",
    ]);
    // The chosen keys collide under the shared hash.
    let hash = |key: &str| value::dictionary_hash(&key.encode_utf16().collect::<Vec<_>>());
    for (a, b) in [("ca9oj", "cE8ua"), ("ca9oi", "cE8ub"), ("ca9oh", "cE8uc")] {
        assert_eq!(hash(a), hash(b));
    }
    for key in ["h97", "h158", "h392"] {
        assert_eq!(hash(key) & 31, hash("h31") & 31);
    }
    assert_eq!(run(COLLISIONS, V38), expected);
    assert_eq!(run(COLLISIONS, V37), expected);
}

#[test]
fn hashed_dictionaries_serve_mixed_records_json_spreads_and_for_in() {
    let expected = lines(&[
        "78 2 3,12,p0,p1",
        r#"[["3",2],["5",5],["p1",2],["p2",4]] [["p39",78],["p0",-1]]"#,
        "43 100 p39,p0,extra",
        "41 41",
        "503 retry detail10,detail11,type",
        "213",
        "503 13 false",
        r#"{"the0":6,"quick0":2,"brown0":2,"fox0":4,"jumps0":2,"over0":2,"lazy0":2,"dog0":2,"end0":2,"the1":6,"quick1":2,"brown1":2,"fox1":4,"jumps1":2,"over1":2,"lazy1":2,"dog1":2,"end1":2,"the2":3,"quick2":1,"brown2":1,"fox2":2,"jumps2":1,"over2":1,"lazy2":1,"dog2":1,"end2":1}"#,
    ]);
    assert_eq!(run(MIXED, V38), expected);
    assert_eq!(run(MIXED, V37), expected);
}

/// The guest memory after `_start` of a v38 program that keeps an indexed
/// dictionary of 40 entries with one deleted (a hole and a tombstone) in a
/// module constant, with the module's bounds and the dictionary's handle.
fn indexed_dictionary() -> (Vec<u8>, value::ModuleBounds, u64) {
    let source = "const d: Record<string, number> = {};\
        export function main(): void { for (let i = 0; i < 40; i++) { d[\"key\" + String(i)] = i; }\
        delete d[\"key5\"]; console.log(String(d[\"key7\"])); }";
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile(source, V38)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    instance
        .get_func(&mut *store, "_start")
        .unwrap()
        .call(&mut *store, &[], &mut [])
        .unwrap();
    assert_eq!(store.data().output.lines, lines(&["7"]));
    let bounds = store.data().module;
    let memory = instance.get_memory(&mut *store, "memory").unwrap();
    let data = memory.data(&*store).to_vec();
    let version = value::HASHED_DICTIONARY_VALUE_VERSION;
    let fields = value::HASHED_DICTIONARY_FIELDS;
    let word = |at: usize| u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
    let length = value::record_bytes(version, fields) as u64;
    let handle = (abi::OWNED_STRING_BASE..data.len() - 8)
        .step_by(value::HEAP_ALIGNMENT)
        .filter(|ptr| {
            let descriptor = word(*ptr);
            word(ptr + 4) == fields
                && descriptor + 8 <= data.len()
                && word(descriptor) == version as usize
                && word(descriptor + 4) == fields
        })
        .map(|ptr| ptr as u64 | (length << i32::BITS))
        .find(|handle| {
            value::Dictionary::read_in(&data, *handle, version, bounds)
                .is_ok_and(|d| d.table.is_some() && d.keys.length == 40)
        })
        .expect("the indexed dictionary in guest memory");
    (data, bounds, handle)
}

/// One tampering of the guest memory image.
type Corruption<'a> = Box<dyn Fn(&mut Vec<u8>) + 'a>;

#[test]
fn corrupt_tables_are_rejected_at_the_json_and_log_boundaries() {
    use value::{ARRAY_META_BYTES, ARRAY_SLOT_BYTES, HASHED_DICTIONARY_VALUE_VERSION as V18};
    let (data, bounds, handle) = indexed_dictionary();
    let kind = value::ValueKind::Dictionary as u32;
    let render = |data: &[u8]| json::render_reference_in(data, kind, handle, V18, bounds);
    let log = |data: &[u8]| {
        let mut budget = varta_bigint::Budget::new(u64::from(u32::MAX), u64::MAX);
        json::render_diagnostic(data, kind, handle, V18, bounds, &mut budget)
    };
    let text = String::from_utf8(render(&data).unwrap()).unwrap();
    assert!(
        text.starts_with(r#"{"key0":0,"#) && !text.contains("key5"),
        "{text}"
    );
    assert!(log(&data).is_ok());
    let dictionary = value::Dictionary::read_in(&data, handle, V18, bounds).unwrap();
    let table = dictionary.table.unwrap();
    let slot = |index: usize| table.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES;
    let read = |data: &[u8], index: usize| {
        f64::from_le_bytes(data[slot(index)..slot(index) + 8].try_into().unwrap())
    };
    let bitmap = slot(table.capacity);
    let references: Vec<usize> = (0..table.length)
        .filter(|i| read(&data, *i) > 0.0)
        .collect();
    let tombstone = (0..table.length)
        .find(|i| data[bitmap + i / 8] & (1 << (i % 8)) != 0 && read(&data, *i) == 0.0)
        .expect("the deleted entry's tombstone");
    let empty = (0..table.length)
        .find(|i| data[bitmap + i / 8] & (1 << (i % 8)) == 0)
        .unwrap();
    assert_eq!(references.len(), 39);
    let record = handle as u32 as usize + value::RECORD_META_BYTES;
    let field = |slot: usize| record + slot * value::RECORD_SLOT_BYTES;
    let put = |data: &mut Vec<u8>, at: usize, bytes: &[u8]| {
        data[at..at + bytes.len()].copy_from_slice(bytes);
    };
    let corruptions: Vec<(&str, Corruption)> = vec![
        (
            "duplicate reference",
            Box::new(|d| {
                let other = d[slot(references[1])..slot(references[1]) + 8].to_vec();
                put(d, slot(references[0]), &other);
            }),
        ),
        (
            "live entry unreferenced",
            Box::new(|d| put(d, slot(references[2]), &0f64.to_bits().to_le_bytes())),
        ),
        (
            "fractional reference",
            Box::new(|d| put(d, slot(references[3]), &1.5f64.to_le_bytes())),
        ),
        (
            "reference past the entries",
            Box::new(|d| put(d, slot(references[3]), &41f64.to_le_bytes())),
        ),
        (
            "negative zero tombstone",
            Box::new(|d| put(d, slot(tombstone), &(-0f64).to_le_bytes())),
        ),
        (
            "reference to the hole",
            Box::new(|d| put(d, slot(tombstone), &6f64.to_le_bytes())),
        ),
        (
            "reference moved off its probe",
            Box::new(|d| {
                let moved = d[slot(references[4])..slot(references[4]) + 8].to_vec();
                put(d, slot(references[4]), &0f64.to_le_bytes());
                put(d, slot(empty), &moved);
                d[bitmap + empty / 8] |= 1 << (empty % 8);
            }),
        ),
        (
            "present slot made a hole",
            Box::new(|d| {
                let at = references[5];
                d[bitmap + at / 8] &= !(1 << (at % 8));
            }),
        ),
        (
            "hole count",
            Box::new(|d| {
                put(
                    d,
                    field(value::HASHED_DICTIONARY_HOLES_SLOT),
                    &0f64.to_le_bytes(),
                )
            }),
        ),
        (
            "hole without a table",
            Box::new(|d| {
                put(
                    d,
                    field(value::HASHED_DICTIONARY_TABLE_SLOT),
                    &0u64.to_le_bytes(),
                )
            }),
        ),
    ];
    for (name, corrupt) in corruptions {
        let mut bad = data.clone();
        corrupt(&mut bad);
        assert!(
            value::Dictionary::read_in(&bad, handle, V18, bounds).is_err(),
            "{name}"
        );
        let error = render(&bad).unwrap_err().to_string();
        assert!(
            error.contains("dictionary") || error.contains("array"),
            "{name}: {error}"
        );
        assert!(log(&bad).is_err(), "{name}");
        // The same bytes are never read as a value17 dictionary.
        assert!(
            value::Dictionary::read_in(&bad, handle, value::FUNCTION_FIELD_VALUE_VERSION, bounds)
                .is_err()
        );
    }
    assert!(
        value::Dictionary::read_in(&data, handle, value::FUNCTION_FIELD_VALUE_VERSION, bounds)
            .is_err()
    );
}

/// One measured program over `n` distinct keys `"k" + i`, built into an
/// array before the measured `loops` so that these allocate no keys.
fn workload(n: usize, loops: &[&str]) -> String {
    let body: String = loops
        .iter()
        .map(|operation| {
            format!("for (let i = 0; i < {n}; i++) {{ const key = keys[i]; {operation} }} ")
        })
        .collect();
    format!(
        "export function main(): void {{ const keys: string[] = []; \
         for (let i = 0; i < {n}; i++) {{ keys[i] = \"k\" + String(i); }} \
         const d: Record<string, number> = {{}}; let t = 0; {body}console.log(String(t)); }}"
    )
}
const INSERT: &str = "d[key] = i;";
const LOOKUP: &str = "const v = d[key]; if (v !== undefined) { t = t + v; }";
const DELETE: &str = "delete d[key];";

/// Fuel of inserts, lookups and deletes of `n` keys: the marginal fuel of
/// each loop over the previous program, under test-only fuel and the
/// production 8 MiB memory. A failure (the memory budget) is reported with
/// its fuel.
fn try_phases(n: usize, profile: P, fuel: u64) -> Result<[u64; 3], String> {
    let fuel = |loops: &[&str]| {
        let (result, used) = run_budgeted(
            &compile(&workload(n, loops), profile),
            fuel,
            abi::MAX_MEMORY_BYTES,
        );
        result
            .map(|_| used)
            .map_err(|e| format!("{loops:?}: {e} at {used} fuel"))
    };
    let empty = fuel(&[])?;
    let inserted = fuel(&[INSERT])?;
    let looked = fuel(&[INSERT, LOOKUP])?;
    let deleted = fuel(&[INSERT, DELETE])?;
    Ok([inserted - empty, looked - inserted, deleted - inserted])
}
fn phases(n: usize, profile: P, fuel: u64) -> [u64; 3] {
    try_phases(n, profile, fuel).unwrap_or_else(|e| panic!("{n}: {e}"))
}

#[test]
#[ignore = "measurement: prints the dictionary fuel table (--ignored --nocapture)"]
fn dictionary_fuel_measurements() {
    println!("| entries | inserts | per insert | lookups | per lookup | deletes | per delete |");
    for n in [1_000, 10_000, 20_000, 50_000, 100_000] {
        match try_phases(n, V38, 1_000_000) {
            Ok([inserts, lookups, deletes]) => {
                let per = |fuel: u64| fuel as f64 / n as f64;
                println!(
                    "| {n} | {inserts} | {:.0} | {lookups} | {:.0} | {deletes} | {:.0} |",
                    per(inserts),
                    per(lookups),
                    per(deletes)
                );
            }
            Err(error) => println!("| {n} | {error} |"),
        }
    }
    // The shape the data-limits stream measured at 251M fuel: 1000
    // insertions with 500 of the keys deleted again.
    let source = workload(1_000, &[INSERT, "if (i % 2 === 0) { delete d[key]; }"]);
    let (result, used) = run_budgeted(&compile(&source, V38), 1_000, abi::MAX_MEMORY_BYTES);
    result.unwrap();
    let (_, empty) = run_budgeted(
        &compile(&workload(1_000, &[]), V38),
        1_000,
        abi::MAX_MEMORY_BYTES,
    );
    println!("1000 insertions, 500 deleted: {} fuel", used - empty);
}

/// Fuel stays near linear in the entry count: ten times the keys cost well
/// under forty times the fuel per phase (the linear layout took 104–130 times;
/// a lookup's `T | undefined` cell adds collection work that grows with the
/// live heap).
#[test]
fn dictionary_operations_cost_amortized_constant_fuel() {
    let small = phases(1_000, V38, 1_000);
    let large = phases(10_000, V38, 1_000);
    for (phase, (small, large)) in ["inserts", "lookups", "deletes"]
        .into_iter()
        .zip(small.into_iter().zip(large))
    {
        let ratio = large as f64 / small as f64;
        assert!(
            ratio < 40.0,
            "{phase}: 1k keys {small} fuel, 10k keys {large} fuel ({ratio:.1}x)"
        );
    }
}

/// The docs/language-guide.md example (Node 24: `4999 false 2,row0`).
#[test]
fn hashed_dictionary_guide_example_matches_node() {
    let source = r#"export function main(): void {
  const index: Record<string, number> = {};
  for (let i = 0; i < 5000; i++) {
    index["row" + String(i)] = i;
  }
  delete index["row7"];
  index["2"] = -2;
  console.log(`${index["row4999"]} ${"row7" in index} ${Object.keys(index).slice(0, 2).join(",")}`);
}"#;
    assert_eq!(run(source, V38), lines(&["4999 false 2,row0"]));
}

#[test]
fn hashed_dictionaries_live_in_closures_generators_and_async_frames() {
    let expected = lines(&["1 -1 23", "sync 22", "settled 22 1 -1", "a=1;b=2;c=3; 0"]);
    assert_eq!(run(FRAMES, V38), expected);
    assert_eq!(run(FRAMES, V37), expected);
}

#[test]
fn hashed_dictionaries_survive_collections_at_large_sizes() {
    let expected = lines(&[
        "5000 5000 346114 true",
        "key1,key2,key4 key5982,key5988,key5994",
        "1000 14,28,42 2000 undefined",
    ]);
    // Past the v37 element limit, under the production 8 MiB memory budget.
    let (result, used) = run_budgeted(&compile(LARGE, V38), 2000, abi::MAX_MEMORY_BYTES);
    println!("large case: {used} fuel");
    assert_eq!(result.unwrap_or_else(|e| panic!("{e:?}")), expected);
}
