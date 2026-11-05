//! The extent bounds of a KV value (`varta_kv_codec::Extent`) and the fuel
//! of decoding large values: every value Deno stores without sharing sets
//! and gets, a part shared within a value expands within the bounds or fails
//! by name on `set` and on `get`, keys keep Deno's own key-size limits, and
//! a read whose decoding costs more than its turn's fuel ends the turn with
//! the ordinary fatal fuel exhaustion.
use super::tests::{LOG, Tree, compile, open_at, read_write, waiting_runtime};
use super::*;
use crate::{BudgetExhausted, RuntimeError};
use denokv_proto::KeyPart;
use std::rc::Rc;
use std::time::{Duration, Instant};
use varta_common::{
    CapabilityGrant, CapabilityValue,
    budget::{BudgetKind, BudgetRequest, MAX_DEADLINE_PER_TURN_MS, MAX_FUEL_PER_TURN, TURN_FUEL},
    limits::Resource,
};
use varta_kv_codec::v8::{Key, Object, Value};

const PATH: &str = "/data/extent.kv";

/// The program's helpers: a timer turn, each set and get runs in its own.
const TURN: &str = "function turn(): Promise<void> { return new Promise<void>((done) => { setTimeout(() => done(), 0); }); }\n";

/// The KV grants and timers.
fn grants() -> Vec<CapabilityGrant> {
    let mut grants = read_write();
    grants.push(CapabilityGrant {
        id: "time.timer".into(),
        value: CapabilityValue::Boolean(true),
    });
    grants
}

/// Test-only budgets (CLAUDE.md §11), requested by the package and allowed
/// by the host: the engine's maximum fuel and deadline per turn, so the
/// largest values decode within one turn whatever the machine's load. The
/// memory stays the production one: a v38 module's own memory maximum is
/// that, and the browser host cannot enforce another (the browser KV
/// acceptance runs the same budgets).
fn largest_budgets() -> BudgetRequest {
    BudgetRequest {
        fuel_per_turn: Some(MAX_FUEL_PER_TURN),
        deadline_per_turn_ms: Some(MAX_DEADLINE_PER_TURN_MS),
        ..BudgetRequest::default()
    }
}

/// `helpers`, then `body` inside `main` with `kv` open at [`PATH`], every
/// failure logged as `<name>: <message>`.
fn program(helpers: &str, body: &str) -> String {
    format!(
        "{TURN}{helpers}\nexport async function main(): Promise<void> {{\n  const kv = await varta.kv.open('{PATH}');\n{body}\n  kv.close();\n  console.log('done');\n}}\n"
    )
}

/// Run `source` over `tree` under `budgets` (production when `None`).
fn run(
    source: &str,
    tree: &Tree,
    budgets: Option<BudgetRequest>,
) -> Result<Vec<String>, RuntimeError> {
    let bytes = compile(source);
    let report = crate::VartaRuntime::new().run_with(
        &bytes,
        crate::RunOptions {
            requested: grants(),
            file_roots: tree.roots(),
            budgets: budgets.unwrap_or_default(),
            budget_ceiling: budgets,
            ..crate::RunOptions::default()
        },
    );
    report.into_result().map(|output| {
        output
            .lines
            .into_iter()
            .map(|line| line.strip_prefix(LOG).unwrap_or(&line).to_owned())
            .collect()
    })
}

fn logs(source: &str, tree: &Tree, budgets: Option<BudgetRequest>) -> Vec<String> {
    run(source, tree, budgets).unwrap_or_else(|e| panic!("{e}\n{source}"))
}

/// The run ended with the ordinary fatal exhaustion of a production turn's
/// fuel.
fn assert_turn_fuel_exhausted(result: Result<Vec<String>, RuntimeError>) {
    match result {
        Err(RuntimeError::Budget(BudgetExhausted {
            budget: BudgetKind::Fuel,
            limit,
            ..
        })) => assert_eq!(limit, TURN_FUEL),
        other => panic!("{other:?}"),
    }
}

/// Store `values` at `["planted", label]` as Deno (or an earlier build)
/// writes them, straight into the file.
fn plant(tree: &Tree, values: &[(String, Vec<u8>)]) {
    let runtime = waiting_runtime();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut database = open_at(tree, PATH, true);
    let mutations = values
        .iter()
        .map(|(label, bytes)| denokv_proto::Mutation {
            key: denokv_proto::encode_key(&denokv_proto::Key(vec![
                KeyPart::String("planted".into()),
                KeyPart::String(label.clone()),
            ]))
            .unwrap(),
            kind: MutationKind::Set(KvValue::V8(bytes.clone())),
            expire_at: None,
        })
        .collect();
    assert!(
        database
            .commit(&runtime, deadline, Vec::new(), mutations)
            .unwrap()
            .is_some()
    );
}

/// The most elements of an array of one-byte values (`true`) whose V8 form
/// fits Deno's value size.
fn most_booleans() -> usize {
    let booleans = |count: usize| {
        Value::Object(Rc::new(Object::Dense(vec![
            Some(Value::Boolean(true));
            count
        ])))
    };
    (0..kv::MAX_VALUE_BYTES)
        .rev()
        .find(|count| v8::encode(&booleans(*count), kv::MAX_VALUE_BYTES).is_ok())
        .unwrap()
}

/// The first nine-digit array index: from here on, an index key and its
/// one-byte value take six encoded bytes for nine code units of text.
const FIRST_DENSE_INDEX: u32 = 100_000_000;

/// The most entries `FIRST_DENSE_INDEX + i: true` of an object whose V8 form
/// fits Deno's value size: the densest text Deno stores.
fn most_dense_keys() -> usize {
    let keyed = |count: u32| {
        Value::Object(Rc::new(Object::Plain(
            (FIRST_DENSE_INDEX..FIRST_DENSE_INDEX + count)
                .map(|index| (Key::Index(index), Value::Boolean(true)))
                .collect(),
        )))
    };
    let mut count = u32::try_from(kv::MAX_VALUE_BYTES / kv::DENSEST_TEXT_BYTES).unwrap();
    while v8::encode(&keyed(count), kv::MAX_VALUE_BYTES).is_err() {
        count -= 1;
    }
    count as usize
}

/// Values Deno and the earlier v38 build store without sharing set and get
/// under production budgets (each in its own turn): arrays of 2048 and 3000
/// numbers and 500 records of three fields, past the former, fuel-derived
/// value bound.
#[test]
fn unshared_values_deno_stores_set_and_get_in_a_production_turn() {
    let tree = Tree::new();
    let helpers = "interface Run { id: string; n: number; done: boolean }";
    let body = "    for (const n of [2048, 3000]) {\n\
         const numbers: number[] = [];\n\
         for (let i = 0; i < n; i++) { numbers.push(i); }\n\
         await turn();\n\
         await kv.set(['numbers', n], numbers);\n\
         await turn();\n\
         const got = await kv.get<number[]>(['numbers', n]);\n\
         let sum = 0;\n\
         if (got.value !== null) { for (const x of got.value) { sum = sum + x; } }\n\
         console.log('numbers ' + String(got.value === null ? 0 : got.value.length) + ' ' + String(sum));\n\
         }\n\
         const runs: Run[] = [];\n\
         for (let i = 0; i < 500; i++) { runs.push({ id: 'r' + String(i), n: i, done: i % 2 === 0 }); }\n\
         await turn();\n\
         await kv.set(['runs'], runs);\n\
         await turn();\n\
         const got = await kv.get<Run[]>(['runs']);\n\
         console.log('runs ' + String(got.value === null ? 0 : got.value.length) + ' ' + (got.value === null ? '' : got.value[499].id));";
    let sum = |n: usize| n * (n - 1) / 2;
    assert_eq!(
        logs(&program(helpers, body), &tree, None),
        [
            format!("numbers 2048 {}", sum(2048)),
            format!("numbers 3000 {}", sum(3000)),
            "runs 500 r499".into(),
            "done".into(),
        ]
    );
}

/// Every value Deno can store without sharing within its value size is
/// within the extent bounds: the most values (an array of `true`, one byte
/// each) and the densest text (nine-digit index keys) set and get whole;
/// one element more fails Deno's own size check, never the extent. Their
/// decoding is past a production turn's fuel, so the test-only budgets
/// allow the engine's maximum.
#[test]
fn the_largest_unshared_values_set_and_get_whole() {
    let tree = Tree::new();
    let (booleans, keys) = (most_booleans(), most_dense_keys());
    assert!(booleans < kv::MAX_VALUE_NODES, "the array and its elements");
    let units = keys * kv::DENSEST_TEXT_UNITS;
    assert!(units > kv::MAX_VALUE_BYTES && units <= kv::MAX_VALUE_UNITS);
    let body = format!(
        "    for (const n of [{booleans}, {}]) {{\n\
         const values: boolean[] = [];\n\
         for (let i = 0; i < n; i++) {{ values.push(true); }}\n\
         await turn();\n\
         try {{ await kv.set(['booleans', n], values); }} catch (e) {{ if (e instanceof Error) {{ console.log(e.name + ': ' + e.message); }} }}\n\
         await turn();\n\
         const got = await kv.get<boolean[]>(['booleans', n]);\n\
         console.log('booleans ' + String(got.value === null ? 0 : got.value.length));\n\
         }}\n\
         for (const n of [{keys}, {}]) {{\n\
         const values: Record<string, boolean> = {{}};\n\
         for (let i = 0; i < n; i++) {{ values[String({FIRST_DENSE_INDEX} + i)] = true; }}\n\
         await turn();\n\
         try {{ await kv.set(['keys', n], values); }} catch (e) {{ if (e instanceof Error) {{ console.log(e.name + ': ' + e.message); }} }}\n\
         await turn();\n\
         const got = await kv.get<Record<string, boolean>>(['keys', n]);\n\
         let text = 0;\n\
         if (got.value !== null) {{ for (const key of Object.keys(got.value)) {{ text = text + key.length; }} }}\n\
         console.log('keys ' + String(text));\n\
         }}",
        booleans + 1,
        keys + 1
    );
    let too_large = format!(
        "TypeError: Value too large (max {} bytes)",
        kv::MAX_VALUE_BYTES
    );
    assert_eq!(
        logs(&program("", &body), &tree, Some(largest_budgets())),
        [
            format!("booleans {booleans}"),
            too_large.clone(),
            "booleans 0".into(),
            format!("keys {units}"),
            too_large,
            "keys 0".into(),
            "done".into(),
        ]
    );
}

/// Levels of the largest diamond of `D` (`interface D { a?: D; b?: D }`,
/// each level `{ a: x, b: x }` of the one below, `{}` at the bottom) within
/// [`kv::MAX_VALUE_NODES`] stored values expanded.
fn diamond_fits() -> usize {
    (kv::MAX_VALUE_NODES + 1).ilog2() as usize - 1
}

/// The stored values of a `D` diamond expanded: every reach of every level.
fn diamond_values(levels: usize) -> usize {
    (1 << (levels + 1)) - 1
}

/// What a read of a `D` diamond builds and its program counts: every reach
/// and both absent fields of every `{}` at the bottom.
fn diamond_built(levels: usize) -> usize {
    (1 << (levels + 2)) - 1
}

/// A diamond far beyond the bound: millions of values expanded from about
/// two hundred stored bytes. Read before the extent bound, its decode ran
/// past the turn deadline and grew the native host to about 1 GB.
const FAR_DIAMOND: usize = 22;

/// The V8 bytes of a `D` diamond of `levels` as Deno (or an earlier Varta)
/// writes it: level `k` (object id `levels - k`) back-references level
/// `k - 1` in `b`.
fn diamond_bytes(levels: usize) -> Vec<u8> {
    fn body(bytes: &mut Vec<u8>, level: usize, levels: usize) {
        bytes.push(b'o');
        if level == 0 {
            bytes.extend([b'{', 0]);
            return;
        }
        bytes.extend([b'"', 1, b'a']);
        body(bytes, level - 1, levels);
        // One-byte varint: the ids stay below 0x80 here.
        let id = u8::try_from(levels - level + 1).unwrap();
        assert!(id < 0x80);
        bytes.extend([b'"', 1, b'b', b'^', id, b'{', 2]);
    }
    let mut bytes = vec![0xFF, u8::try_from(v8::WRITE_VERSION).unwrap()];
    body(&mut bytes, levels, levels);
    bytes
}

/// The diamond helpers (the browser KV acceptance, scripts/test-browser-kv.mjs
/// `diamond`, runs the same program).
const DIAMOND: &str = "interface D { a?: D; b?: D }\n\
     function diamond(levels: number): D {\n\
     let x: D = {};\n\
     for (let i = 0; i < levels; i++) { x = { a: x, b: x }; }\n\
     return x;\n\
     }\n\
     function size(d: D | undefined): number { return d === undefined ? 1 : 1 + size(d.a) + size(d.b); }";

/// One timer turn per step: `set <levels>` stores a diamond at
/// `["diamond", levels]`, `get <levels>` reads it back and `planted
/// <label>` reads `["planted", label]`, each logging what it built or why
/// it failed.
fn diamond_body(steps: &[(&str, usize)]) -> String {
    steps
        .iter()
        .map(|(step, levels)| {
            let label = levels.to_string();
            let read = |key: &str| {
                format!(
                    "    await turn();\n\
                     try {{ const got = await kv.get<D>([{key}, '{label}']); console.log('{step} {label} ' + (got.value === null ? 'absent' : String(size(got.value)))); }}\n\
                     catch (e) {{ if (e instanceof Error) {{ console.log('{step} {label} ' + e.name + ': ' + e.message); }} }}"
                )
            };
            match *step {
                "set" => format!(
                    "    await turn();\n\
                     try {{ await kv.set(['diamond', '{label}'], diamond({levels})); console.log('set {label}'); }}\n\
                     catch (e) {{ if (e instanceof Error) {{ console.log('set {label} ' + e.name + ': ' + e.message); }} }}"
                ),
                "get" => read("'diamond'"),
                _ => read("'planted'"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Levels of the largest diamond whose read a guest's memory builds
/// whole (measured): one level below the bound, half its values.
fn built_diamond() -> usize {
    diamond_fits() - 1
}

/// A part shared within a value counts toward the extent bounds at every
/// reach, on `set` and on `get`: the largest diamond within the bound
/// stores, and the next one (a few dozen stored bytes) is refused by name
/// before anything is stored; diamonds written by Deno or an earlier build
/// are refused by name on read past the bound, the far one without
/// building it.
///
/// Within the bound a read pays for what it builds, like the read of any
/// large value: a diamond reads back whole, stored by Varta or planted,
/// when its turn's fuel and the instance's memory pay for it; under a
/// production turn's fuel the decoding of the diamond at the bound ends the
/// turn with the fatal fuel exhaustion, and with the engine's maximum fuel
/// it builds past the guest's memory and ends the run with the fatal memory
/// exhaustion.
#[test]
fn shared_parts_expand_within_the_value_bound() {
    let tree = Tree::new();
    let (fits, built) = (diamond_fits(), built_diamond());
    assert!(diamond_values(fits) <= kv::MAX_VALUE_NODES);
    assert!(diamond_values(fits + 1) > kv::MAX_VALUE_NODES);
    plant(
        &tree,
        &[built, fits, fits + 1, FAR_DIAMOND]
            .map(|levels| (levels.to_string(), diamond_bytes(levels))),
    );
    let refused = format!("TypeError: {}", varta_kv_codec::too_expanded());
    let mismatch = format!(
        "TypeError: KV value does not match the checked type: {}",
        varta_kv_codec::too_expanded()
    );
    let steps = [
        ("set", built),
        ("set", fits),
        ("set", fits + 1),
        ("get", fits + 1),
        ("planted", fits + 1),
        ("planted", FAR_DIAMOND),
    ];
    assert_eq!(
        logs(&program(DIAMOND, &diamond_body(&steps)), &tree, None),
        [
            format!("set {built}"),
            format!("set {fits}"),
            format!("set {} {refused}", fits + 1),
            format!("get {} absent", fits + 1),
            format!("planted {} {mismatch}", fits + 1),
            format!("planted {FAR_DIAMOND} {mismatch}"),
            "done".into(),
        ]
    );
    let steps = [("get", built), ("planted", built)];
    let values = diamond_built(built);
    assert_eq!(
        logs(
            &program(DIAMOND, &diamond_body(&steps)),
            &tree,
            Some(largest_budgets())
        ),
        [
            format!("get {built} {values}"),
            format!("planted {built} {values}"),
            "done".into(),
        ]
    );
    for step in ["get", "planted"] {
        let source = program(DIAMOND, &diamond_body(&[(step, fits)]));
        assert_turn_fuel_exhausted(run(&source, &tree, None));
        match run(&source, &tree, Some(largest_budgets())) {
            Err(RuntimeError::Resource(failure)) => {
                assert_eq!(failure.resource, Resource::Memory);
            }
            other => panic!("{other:?}"),
        }
    }
}

/// Keys and range selectors never meet a value's bounds: whatever their
/// parts, Deno's own key-size messages name them, on read and on write,
/// also for a key whose V8 form is past Deno's value size.
#[test]
fn keys_keep_denos_key_size_limits() {
    let tree = Tree::new();
    // A double part takes a tag and eight bytes in V8 and in the key.
    let doubles = kv::MAX_VALUE_BYTES / size_of::<f64>();
    let body = format!(
        "    for (const n of [1000, 2047, 2048, 3000, {doubles}]) {{\n\
         const key: KvKey = [];\n\
         for (let i = 0; i < n; i++) {{ key.push(n === {doubles} ? 0.5 : 0); }}\n\
         await turn();\n\
         try {{ await kv.get<number>(key); }} catch (e) {{ if (e instanceof Error) {{ console.log('get ' + String(n) + ' ' + e.message); }} }}\n\
         await turn();\n\
         try {{ for await (const e of kv.list<number>({{ prefix: key }})) {{ console.log(e.versionstamp); }} }} catch (e) {{ if (e instanceof Error) {{ console.log('list ' + String(n) + ' ' + e.message); }} }}\n\
         await turn();\n\
         try {{ await kv.set(key, 1); }} catch (e) {{ if (e instanceof Error) {{ console.log('set ' + String(n) + ' ' + e.message); }} }}\n\
         }}"
    );
    let read = format!(
        "Key too large for read (max {} bytes)",
        kv::MAX_READ_KEY_BYTES
    );
    let write = format!(
        "Key too large for write (max {} bytes)",
        kv::MAX_WRITE_KEY_BYTES
    );
    let mut expected: Vec<String> = [1000, 2047, 2048, 3000, doubles]
        .iter()
        .flat_map(|n| {
            [
                format!("get {n} {read}"),
                format!("list {n} {read}"),
                format!("set {n} {write}"),
            ]
        })
        .collect();
    expected.push("done".into());
    assert_eq!(logs(&program("", &body), &tree, None), expected);
}

/// A read pays the fuel of every value it decodes as it decodes it, in the
/// turn that reads it: one `get` of a value whose decoding fits a turn
/// succeeds; a `getMany` or a `list` batch decoding several of them in one
/// turn, or one `get` of a value whose decoding alone is past a turn's
/// fuel, ends the turn with the ordinary fatal fuel exhaustion, as any
/// other too expensive work does. Spreading the reads over turns reads
/// them all.
#[test]
fn reads_past_their_turns_fuel_end_with_fatal_fuel_exhaustion() {
    let tree = Tree::new();
    // Each value 3000 numbers; one over a turn's decoding alone.
    let store = "    const sizes: number[] = [3000, 3000, 3000, 6000];\n\
         for (let k = 0; k < sizes.length; k++) {\n\
         const numbers: number[] = [];\n\
         for (let i = 0; i < sizes[k]; i++) { numbers.push(i); }\n\
         await turn();\n\
         await kv.set(['big', sizes[k], k], numbers);\n\
         }";
    assert_eq!(logs(&program("", store), &tree, None), ["done"]);
    let each = "    const keys: KvKey[] = [];\n\
         for await (const e of kv.list<number[]>({ prefix: ['big', 3000] }, { batchSize: 1 })) { keys.push(e.key); await turn(); }\n\
         let total = 0;\n\
         for (const key of keys) { await turn(); const got = await kv.get<number[]>(key); if (got.value !== null) { total = total + got.value.length; } }\n\
         console.log('read ' + String(keys.length) + ' ' + String(total));";
    assert_eq!(
        logs(&program("", each), &tree, None),
        ["read 3 9000", "done"]
    );
    let together = [
        "    const keys: KvKey[] = [];\n\
         for await (const e of kv.list<number[]>({ prefix: ['big', 3000] }, { batchSize: 1 })) { keys.push(e.key); await turn(); }\n\
         await turn();\n\
         await kv.getMany<number[]>(keys);",
        "    for await (const e of kv.list<number[]>({ prefix: ['big', 3000] })) { console.log(e.versionstamp); }",
        "    for await (const e of kv.list<number[]>({ prefix: ['big', 6000] })) { console.log(e.versionstamp); }",
    ];
    for body in together {
        assert_turn_fuel_exhausted(run(&program("", body), &tree, None));
    }
}
