//! The `kv_*` host operations against the Deno 2.9.4 fixture file
//! (`kv_cases/deno-2.9.4`): reads, writes, atomic commits, grants, pinning
//! and schema checks.
use super::*;
use crate::policy::FileRoots;
use std::path::{Path, PathBuf};
use varta_common::{CapabilityGrant, CapabilityValue, language::LanguageProfile};

const FIXTURE: &[u8] = include_bytes!("../kv_cases/deno-2.9.4/fixture.kv");
const MANIFEST: &str = include_str!("../kv_cases/deno-2.9.4/manifest.json");
const ROWS: &str = include_str!("../kv_cases/deno-2.9.4/rows.json");
const LOG: &str = "varta log: ";

/// A temporary host tree: `root/` (the `data` root) and `outside/`.
pub(super) struct Tree(PathBuf);
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Tree {
    pub(super) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "varta-kv-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(base.join("root/sub")).unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        Self(base)
    }
    pub(super) fn root(&self) -> PathBuf {
        self.0.join("root")
    }
    fn fixture(&self, name: &str) -> PathBuf {
        let path = self.root().join(name);
        std::fs::write(&path, FIXTURE).unwrap();
        path
    }
    pub(super) fn roots(&self) -> FileRoots {
        let mut roots = FileRoots::new();
        roots.open("data", &self.root()).unwrap();
        roots
    }
}

fn grant(id: &str, patterns: &[&str]) -> CapabilityGrant {
    CapabilityGrant {
        id: id.into(),
        value: CapabilityValue::PathGlobs(patterns.iter().map(|p| (*p).into()).collect()),
    }
}

pub(super) fn read_write() -> Vec<CapabilityGrant> {
    vec![
        grant(kv::READ_CAPABILITY, &["/data/**"]),
        grant(kv::WRITE_CAPABILITY, &["/data/**"]),
    ]
}

pub(super) fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn run_with(
    source: &str,
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
    roots: FileRoots,
) -> Result<Vec<String>, crate::RuntimeError> {
    let bytes = compile(source);
    let report = crate::VartaRuntime::new().run_with(
        &bytes,
        crate::RunOptions {
            requested,
            ceiling,
            file_roots: roots,
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

fn run(source: &str, requested: Vec<CapabilityGrant>, roots: FileRoots) -> Vec<String> {
    run_with(source, requested, None, roots).unwrap_or_else(|e| panic!("{e}\n{source}"))
}

/// `body` inside `main` with `kv` open at `path`.
fn program(path: &str, body: &str) -> String {
    format!(
        "export async function main(): Promise<void> {{\n  const kv = await varta.kv.open('{path}');\n{body}\n  kv.close();\n}}\n"
    )
}

/// A program that reports `<name>: <message>` of what opening or `body`
/// rejects with.
fn guarded(path: &str, body: &str) -> String {
    format!(
        "export async function main(): Promise<void> {{\n  try {{\n    const kv = await varta.kv.open('{path}');\n{body}\n    kv.close();\n    console.log('done');\n  }} catch (error) {{\n    if (error instanceof Error) {{ console.log(error.name + ': ' + error.message); }}\n  }}\n}}\n"
    )
}

fn manifest() -> Vec<(serde_json::Value, Option<String>)> {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
    manifest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["key"].clone(),
                entry["versionstamp"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// `(v, v_encoding)` of the Deno 2.9.4 row under `key`.
fn deno_row(key: &[u8]) -> (Vec<u8>, i64) {
    let rows: serde_json::Value = serde_json::from_str(ROWS).unwrap();
    let row = rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| hex(row["k"].as_str().unwrap()) == key)
        .unwrap_or_else(|| panic!("no Deno row for {key:02x?}"));
    (
        hex(row["v"].as_str().unwrap()),
        row["v_encoding"].as_i64().unwrap(),
    )
}

/// Every row of a database file: `(k, v, v_encoding, version)`.
pub(super) fn rows(path: &Path) -> Vec<(Vec<u8>, Vec<u8>, i64, i64)> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("select k, v, v_encoding, version from kv order by k")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn key_bytes(key: &serde_json::Value) -> Vec<u8> {
    let parts = key
        .as_array()
        .unwrap()
        .iter()
        .map(|part| match part {
            serde_json::Value::String(text) => KeyPart::String(text.clone()),
            other => KeyPart::Float(other.as_f64().unwrap()),
        })
        .collect();
    denokv_proto::encode_key(&denokv_proto::Key(parts)).unwrap()
}

#[test]
fn deno_written_versionstamps_read_back_for_every_manifest_key() {
    let tree = Tree::new();
    tree.fixture("fixture.kv");
    // `getMany` keeps the order of its keys, missing ones included.
    let source = program(
        "/data/fixture.kv",
        "  const entries = await kv.getMany<string>([['fixture', 'deleted'], ['fixture', 'key', 0], ['tenant', 't-1', 'run', 'r-1']]);\n\
           for (const e of entries) { console.log((e.value ?? 'missing') + ' ' + (e.versionstamp ?? '-')); }",
    );
    assert_eq!(
        run(&source, read_write(), tree.roots()),
        [
            "missing -",
            "k 00000000000000070000",
            "k 00000000000000220000"
        ]
    );
    // One program per key: a value of a kind without a checked type rejects.
    let entries = manifest();
    let mut lines = Vec::new();
    for (key, _) in &entries {
        let source = guarded(
            "/data/fixture.kv",
            &format!(
                "    const e = await kv.get<unknown>({key}); console.log(e.versionstamp ?? 'missing');"
            ),
        );
        lines.push(run(&source, read_write(), tree.roots()));
    }
    for ((key, versionstamp), lines) in entries.iter().zip(&lines) {
        let shown = &lines[0];
        // The key encodes to the bytes of Deno's row.
        if versionstamp.is_some() {
            deno_row(&key_bytes(key));
        }
        match versionstamp {
            Some(versionstamp) if shown.starts_with("TypeError: KV value") => {
                // Date, Map, Set, Uint8Array, BigInt, KvU64 and holes: no
                // checked `unknown` form, a recoverable TypeError.
                assert!(
                    shown.contains("no checked type")
                        || shown.contains("is a KvU64")
                        || shown.contains("is a Uint8Array")
                        || shown.contains("has holes"),
                    "{key}: {shown}"
                );
                assert!(!versionstamp.is_empty());
            }
            Some(versionstamp) => assert_eq!(shown, versionstamp, "{key}"),
            None => assert_eq!(shown, "missing", "{key}"),
        }
    }
    let unreadable = lines
        .iter()
        .filter(|lines| lines[0].starts_with("TypeError"))
        .count();
    // bigint, holes, bytes, bytes_in_obj, date, map, set, u64 plain, u64 sum.
    assert_eq!(unreadable, 9, "{lines:?}");
}

#[test]
fn deno_written_values_decode_strictly_into_their_types() {
    let tree = Tree::new();
    tree.fixture("fixture.kv");
    let source = format!(
        "interface Flat {{ a: number; b: string; c: boolean; d: null }}\n\
         interface Run {{ run_id: string; tenant_id: string; agent_id: string; status: string;\n\
           input: {{ message: string }}; output: {{ response: string }};\n\
           metrics: {{ input_tokens: number; output_tokens: number; total_cost_usd: number; duration_ms: number }};\n\
           metadata: Record<string, unknown>; step_seq: number; artifacts: unknown[]; created_at: string;\n\
           error_code?: string }}\n{}",
        program(
            "/data/fixture.kv",
            "  const flat = await kv.get<Flat>(['fixture', 'obj', 'flat']);\n\
             console.log(JSON.stringify(flat.value));\n\
             const utf16 = await kv.get<string>(['fixture', 'str', 'utf16']);\n\
             console.log(utf16.value ?? '');\n\
             const latin1 = await kv.get<string>(['fixture', 'str', 'latin1']);\n\
             console.log(latin1.value ?? '');\n\
             const long = await kv.get<string>(['fixture', 'str', 'long_utf16']);\n\
             console.log(String((long.value ?? '').length));\n\
             for (const name of ['neg_zero', 'nan', 'inf', 'beyond_smi', 'float']) {\n\
               const n = await kv.get<number>(['fixture', 'num', name]);\n\
               const v = n.value ?? 0;\n\
               console.log(Object.is(v, -0) ? '-0' : String(v));\n\
             }\n\
             const run = await kv.get<Run>(['tenant', 't-1', 'run', 'r-2']);\n\
             const value = run.value;\n\
             if (value !== null) { console.log(value.status + ' ' + String(value.metrics.total_cost_usd) + ' ' + String(value.error_code === undefined)); }\n\
             const dict = await kv.get<Record<string, unknown>>(['fixture', 'obj', 'flat']);\n\
             console.log(JSON.stringify(dict.value));\n\
             const numeric = await kv.get<Record<string, string>>(['fixture', 'obj', 'numeric_keys']);\n\
             console.log(JSON.stringify(numeric.value));\n\
             const u64 = await kv.get<KvU64>(['fixture', 'u64', 'sum']);\n\
             const counter = u64.value;\n\
             if (counter !== null) { console.log(counter.toString()); }\n\
             const bytes = await kv.get<Uint8Array>(['fixture', 'bytes']);\n\
             const raw = bytes.value;\n\
             if (raw !== null) { console.log(String(raw.length) + ' ' + String(raw[3])); }\n\
             const nested = await kv.get<unknown>(['fixture', 'obj', 'nested']);\n\
             console.log(JSON.stringify(nested.value));\n\
             const missing = await kv.get<string>(['fixture', 'deleted']);\n\
             console.log(String(missing.value === null && missing.versionstamp === null));"
        )
    );
    assert_eq!(
        run(&source, read_write(), tree.roots()),
        [
            r#"{"a":1,"b":"two","c":true,"d":null}"#,
            "Привіт 🌍",
            "café ñ ü",
            "30000",
            "-0",
            "NaN",
            "Infinity",
            "2147483648",
            "3.14159",
            "completed 0.000123 true",
            r#"{"a":1,"b":"two","c":true,"d":null}"#,
            r#"{"1":"one","10":"ten","x":"x"}"#,
            "15",
            "4 255",
            r#"{"a":{"b":{"c":[1,{"d":"e"}]}},"list":[],"o":{}}"#,
            "true",
        ]
    );
}

/// The shapes the production census found common, written by Deno 2.9.7
/// (`kv_cases/deno-2.9.7-probe/probe.kv`): arrays of a holey elements kind
/// that miss no index read as dense arrays, a genuinely missing index is a
/// `TypeError`, a sub-object referenced twice reads as two equal copies, and
/// an explicit `undefined` property fills a required `T | undefined` field.
#[test]
fn deno_written_holey_kinds_shared_objects_and_undefined_fields_read_strictly() {
    const PROBE: &[u8] = include_bytes!("../kv_cases/deno-2.9.7-probe/probe.kv");
    let tree = Tree::new();
    std::fs::write(tree.root().join("probe.kv"), PROBE).unwrap();
    let source = format!(
        "interface Meta {{ tag: string; n: number }}\n\
         interface Shared {{ first: Meta; second: Meta; list: Meta[] }}\n{}",
        guarded(
            "/data/probe.kv",
            "    const refilled = await kv.get<number[]>(['p', 'refilled']);\n\
             console.log(JSON.stringify(refilled.value));\n\
             const filled = await kv.get<number[]>(['p', 'newarray']);\n\
             console.log(JSON.stringify(filled.value));\n\
             const shared = (await kv.get<Shared>(['p', 'shared_record'])).value;\n\
             if (shared !== null) {\n\
               console.log(JSON.stringify(shared) + ' ' + String(shared.first === shared.second));\n\
               shared.first.n = 2;\n\
               console.log(String(shared.second.n));\n\
             }\n\
             const required = (await kv.get<{ a: number; b: number | undefined }>(['p', 'required_undefined'])).value;\n\
             if (required !== null) { console.log(String(required.b === undefined) + ' ' + String(Object.keys(required).length)); }\n\
             const optional = (await kv.get<{ a: number; b?: number }>(['p', 'required_undefined'])).value;\n\
             if (optional !== null) { console.log(String(optional.b === undefined)); }\n\
             const holey: string[] = ['holey_strings', 'holeend'];\n\
             for (const key of holey) {\n\
               try { await kv.get<unknown[]>(['p', key]); } catch (e) { if (e instanceof Error) { console.log(key + ': ' + e.message); } }\n\
             }\n\
             try { await kv.get<{ a: number; b: number }>(['p', 'required_undefined']); } catch (e) { if (e instanceof Error) { console.log(e.name); } }",
        )
    );
    assert_eq!(
        run(&source, read_write(), tree.roots()),
        [
            "[1,5,3]",
            "[1,2]",
            // Decoded records take the schema's field order, as checked JSON does.
            r#"{"first":{"n":1,"tag":"t"},"list":[{"n":1,"tag":"t"},{"n":1,"tag":"t"}],"second":{"n":1,"tag":"t"}} false"#,
            "1",
            "true 2",
            "true",
            "holey_strings: KV value does not match the checked type: a stored array has holes",
            "holeend: KV value does not match the checked type: a stored array has holes",
            "TypeError",
            "done",
        ]
    );
}

#[test]
fn mismatched_stored_values_are_catchable_type_errors() {
    let tree = Tree::new();
    tree.fixture("fixture.kv");
    for (ty, key, detail) in [
        (
            "{ a: number }",
            "['fixture', 'obj', 'flat']",
            "unknown field b",
        ),
        (
            "{ a: number; b: string; c: boolean; d: null; e: string }",
            "['fixture', 'obj', 'flat']",
            "lacks the field e",
        ),
        ("string", "['fixture', 'num', 'small']", "does not match"),
        ("number[]", "['fixture', 'arr', 'holes']", "has holes"),
        ("unknown", "['fixture', 'date']", "Date has no checked type"),
        ("string", "['fixture', 'u64', 'plain']", "is a KvU64"),
        (
            "string | boolean",
            "['fixture', 'null']",
            "matches no member",
        ),
    ] {
        let source = guarded(
            "/data/fixture.kv",
            &format!("    await kv.get<{ty}>({key});"),
        );
        let lines = run(&source, read_write(), tree.roots());
        assert!(
            lines[0].starts_with("TypeError: KV value does not match the checked type")
                && lines[0].contains(detail),
            "{ty}: {lines:?}"
        );
    }
    // A key of another kind than the program names fails the same way.
    let source = guarded(
        "/data/fixture.kv",
        "    const e = await kv.get<KvU64>(['fixture', 'str', 'ascii']);",
    );
    assert_eq!(
        run(&source, read_write(), tree.roots()),
        ["TypeError: the stored value is not a KvU64"]
    );
}

#[test]
fn lists_follow_deno_key_order_selectors_and_cursors() {
    let tree = Tree::new();
    tree.fixture("fixture.kv");
    let source = program(
        "/data/fixture.kv",
        "  for await (const e of kv.list<string>({ prefix: ['fixture', 'key'] })) {\n\
           console.log(JSON.stringify(e.key) + ' ' + e.versionstamp);\n\
         }\n\
         const page = kv.list<string>({ prefix: ['fixture', 'key'] }, { limit: 2, batchSize: 1 });\n\
         for await (const e of page) { console.log(JSON.stringify(e.key)); }\n\
         const rest = kv.list<string>({ prefix: ['fixture', 'key'] }, { cursor: page.cursor, reverse: false });\n\
         let more = 0;\n\
         for await (const e of rest) { more++; }\n\
         console.log(String(more));\n\
         const back = kv.list<string>({ prefix: ['fixture', 'key'] }, { reverse: true, limit: 1 });\n\
         for await (const e of back) { console.log(JSON.stringify(e.key)); }\n\
         const range = kv.list<unknown>({ start: ['fixture', 'num', 'a'], end: ['fixture', 'num', 'm'] });\n\
         for await (const e of range) { console.log(JSON.stringify(e.key)); }\n\
         const after = kv.list<string>({ prefix: ['fixture', 'key'], start: ['fixture', 'key', 0] });\n\
         for await (const e of after) { console.log('after ' + JSON.stringify(e.key)); }",
    );
    let versionstamps: std::collections::BTreeMap<String, String> = manifest()
        .into_iter()
        .filter_map(|(key, versionstamp)| Some((key.to_string(), versionstamp?)))
        .collect();
    let lines = run(&source, read_write(), tree.roots());
    // Strings before numbers, numbers ascending, as Deno orders keys.
    let order = [
        r#"["fixture","key","string"]"#,
        r#"["fixture","key","ünïcødé-ключ-🔑"]"#,
        r#"["fixture","key",-1]"#,
        r#"["fixture","key",0]"#,
        r#"["fixture","key",1.5]"#,
        r#"["fixture","key",9007199254740992]"#,
    ];
    for (line, key) in lines.iter().zip(order) {
        assert_eq!(line, &format!("{key} {}", versionstamps[key]));
    }
    assert_eq!(lines[6..8], [order[0], order[1]]);
    assert_eq!(lines[8], "4");
    assert_eq!(lines[9], order[5]);
    assert_eq!(
        lines[10..14],
        [
            r#"["fixture","num","beyond_smi"]"#,
            r#"["fixture","num","float"]"#,
            r#"["fixture","num","inf"]"#,
            r#"["fixture","num","int53"]"#,
        ]
    );
    assert_eq!(
        lines[14..],
        [
            format!("after {}", order[3]),
            format!("after {}", order[4]),
            format!("after {}", order[5]),
        ]
    );
}

#[test]
fn varta_writes_the_bytes_deno_writes_for_the_same_values() {
    let tree = Tree::new();
    let source = program(
        "/data/new.kv",
        "  await kv.set(['fixture', 'key', 'string'], 'k');\n\
           await kv.set(['fixture', 'key', 1.5], 'k');\n\
           await kv.set(['fixture', 'key', 'ünïcødé-ключ-🔑'], 'k');\n\
           await kv.set(['fixture', 'str', 'ascii'], 'hello');\n\
           await kv.set(['fixture', 'str', 'latin1'], 'café ñ ü');\n\
           await kv.set(['fixture', 'str', 'utf16'], 'Привіт 🌍');\n\
           await kv.set(['fixture', 'str', 'empty'], '');\n\
           await kv.set(['fixture', 'str', 'long'], 'x'.repeat(60000));\n\
           await kv.set(['fixture', 'str', 'long_utf16'], 'я'.repeat(30000));\n\
           await kv.set(['fixture', 'num', 'zero'], 0);\n\
           await kv.set(['fixture', 'num', 'small'], 42);\n\
           await kv.set(['fixture', 'num', 'neg'], -7);\n\
           await kv.set(['fixture', 'num', 'smi_max'], 2 ** 30 - 1);\n\
           await kv.set(['fixture', 'num', 'beyond_smi'], 2 ** 31);\n\
           await kv.set(['fixture', 'num', 'int53'], 2 ** 53 - 1);\n\
           await kv.set(['fixture', 'num', 'float'], 3.14159);\n\
           await kv.set(['fixture', 'num', 'neg_zero'], -0);\n\
           await kv.set(['fixture', 'num', 'nan'], NaN);\n\
           await kv.set(['fixture', 'num', 'inf'], Infinity);\n\
           await kv.set(['fixture', 'num', 'neg_inf'], -Infinity);\n\
           await kv.set(['fixture', 'bool', 'true'], true);\n\
           await kv.set(['fixture', 'bool', 'false'], false);\n\
           await kv.set(['fixture', 'null'], null);\n\
           await kv.set(['fixture', 'undefined'], undefined);\n\
           const empty: Record<string, number> = {};\n\
           await kv.set(['fixture', 'obj', 'empty'], empty);\n\
           await kv.set(['fixture', 'obj', 'flat'], { a: 1, b: 'two', c: true, d: null });\n\
           const list: number[] = [];\n\
           const o: Record<string, number> = {};\n\
           const c: unknown[] = [1, { d: 'e' }];\n\
           await kv.set(['fixture', 'obj', 'nested'], { a: { b: { c } }, list, o });\n\
           await kv.set(['fixture', 'obj', 'undefined_field'], { a: 1, b: undefined });\n\
           await kv.set(['fixture', 'obj', 'numeric_keys'], { '1': 'one', '10': 'ten', x: 'x' });\n\
           const none: number[] = [];\n\
           await kv.set(['fixture', 'arr', 'empty'], none);\n\
           const two: number[] = [2];\n\
           const dense: unknown[] = [1, 'two', true, null, { a: 1 }, two];\n\
           await kv.set(['fixture', 'arr', 'dense'], dense);\n\
           await kv.set(['fixture', 'bytes'], new Uint8Array([0, 1, 2, 255]));\n\
           await kv.set(['tenant', 't-1', 'run', 'r-2'], {\n\
             run_id: 'r-2', tenant_id: 't-1', agent_id: 'a-1', status: 'completed',\n\
             input: { message: 'hi' }, output: { response: 'hello' },\n\
             metrics: { input_tokens: 120, output_tokens: 34, total_cost_usd: 0.000123, duration_ms: 1500 },\n\
             metadata: o, step_seq: 3, artifacts: none, created_at: '2026-10-30T12:00:00.000Z',\n\
             error_code: undefined,\n\
           });\n\
           await kv.set(['fixture', 'u64', 'plain'], new KvU64(7n));\n\
           await kv.set(['fixture', 'expire'], 'lives a day', { expireIn: 86400000 });\n\
           await kv.set(['fixture', 'cas'], { by: 'check-and-set' });",
    );
    run(&source, read_write(), tree.roots());
    let written = rows(&tree.root().join("new.kv"));
    assert_eq!(written.len(), 36);
    for (key, value, encoding, _) in &written {
        let (deno, deno_encoding) = deno_row(key);
        assert_eq!((value, encoding), (&deno, &deno_encoding), "{key:02x?}");
    }
    // Versions only grow; Deno increments by a random step of 1 to 9 per
    // commit, so they are not Deno's.
    let mut versions: Vec<i64> = written.iter().map(|row| row.3).collect();
    versions.sort_unstable();
    versions.dedup();
    assert_eq!(versions.len(), 36);
    // The new file has Deno's schema.
    let connection = rusqlite::Connection::open(tree.root().join("new.kv")).unwrap();
    let version: i64 = connection
        .query_row("select version from migration_state where k = 0", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(version, kv::SCHEMA_VERSION);
}

#[test]
fn atomic_commits_check_versionstamps_and_count() {
    let tree = Tree::new();
    let path = tree.fixture("fixture.kv");
    let source = program(
        "/data/fixture.kv",
        "  const current = await kv.get<string>(['fixture', 'overwrite']);\n\
           const ok = await kv.atomic().check(current).set(['fixture', 'cas2'], 'x').sum(['fixture', 'u64', 'sum'], 5n).max(['fixture', 'u64', 'plain'], 9n).commit();\n\
           console.log(String(ok.ok === true));\n\
           if (ok.ok === true) { console.log(String(ok.versionstamp > (current.versionstamp ?? ''))); }\n\
           const stale = await kv.atomic().check({ key: ['fixture', 'overwrite'], versionstamp: '00000000000000000000' }).set(['fixture', 'cas3'], 1).commit();\n\
           console.log(String(stale.ok === false));\n\
           const absent = await kv.atomic().check({ key: ['fixture', 'none'], versionstamp: null }).delete(['fixture', 'cas2']).commit();\n\
           console.log(String(absent.ok === true));\n\
           const sum = await kv.get<KvU64>(['fixture', 'u64', 'sum']);\n\
           const s = sum.value; if (s !== null) { console.log(s.toString()); }\n\
           const max = await kv.get<KvU64>(['fixture', 'u64', 'plain']);\n\
           const m = max.value; if (m !== null) { console.log(m.toString()); }\n\
           try { kv.atomic().sum(['x'], -1n); } catch (e) { if (e instanceof Error) { console.log(e.name); } }\n\
           try { await kv.set([], 1); } catch (e) { if (e instanceof Error) { console.log(e.message); } }\n\
           try { await kv.set(['big'], 'x'.repeat(70000)); } catch (e) { if (e instanceof Error) { console.log(e.message); } }\n\
           try { await kv.set(['k'], 1, { expireIn: -1 }); } catch (e) { if (e instanceof Error) { console.log(e.name); } }\n\
           try { await kv.atomic().check({ key: ['k'], versionstamp: 'zz' }).commit(); } catch (e) { if (e instanceof Error) { console.log(e.message); } }",
    );
    assert_eq!(
        run(&source, read_write(), tree.roots()),
        [
            "true",
            "true",
            "true",
            "true",
            "20",
            "9",
            "RangeError",
            "key cannot be empty",
            "Value too large (max 65536 bytes)",
            "TypeError",
            "invalid versionstamp",
        ]
    );
    let keys: Vec<Vec<u8>> = rows(&path).into_iter().map(|row| row.0).collect();
    let key = |parts: &[&str]| {
        denokv_proto::encode_key(&denokv_proto::Key(
            parts.iter().map(|p| KeyPart::String((*p).into())).collect(),
        ))
        .unwrap()
    };
    assert!(!keys.contains(&key(&["fixture", "cas2"])));
    assert!(!keys.contains(&key(&["fixture", "cas3"])));
}

#[test]
fn grants_are_exact_and_checked_before_the_file_is_opened() {
    let tree = Tree::new();
    let path = tree.fixture("fixture.kv");
    let original = std::fs::read(&path).unwrap();
    let write = guarded("/data/fixture.kv", "    await kv.set(['fixture', 'x'], 1);");
    // No grant: a recoverable NotAllowedError, logged.
    let lines = run(&write, Vec::new(), tree.roots());
    assert_eq!(
        lines,
        [
            "<capability denied: kv.read> (/data/fixture.kv)",
            "NotAllowedError: <capability denied: kv.read> (/data/fixture.kv)"
        ]
    );
    // A write grant alone does not open; a read grant opens read-only and
    // the write is denied.
    let only_write = vec![grant(kv::WRITE_CAPABILITY, &["/data/**"])];
    assert!(run(&write, only_write, tree.roots())[1].starts_with("NotAllowedError"));
    let only_read = vec![grant(kv::READ_CAPABILITY, &["/data/**"])];
    let lines = run(&write, only_read.clone(), tree.roots());
    assert_eq!(
        lines.last().unwrap(),
        "NotAllowedError: <capability denied: kv.write> (/data/fixture.kv)"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    // A read-only grant never creates a file.
    let missing = guarded("/data/missing.kv", "");
    assert_eq!(
        run(&missing, only_read.clone(), tree.roots()),
        ["NotFoundError: no KV database at /data/missing.kv"]
    );
    assert!(!tree.root().join("missing.kv").exists());
    // Paths outside the request, traversal, unmapped roots and host paths.
    let narrow = vec![grant(kv::READ_CAPABILITY, &["/data/sub/*.kv"])];
    for denied in [
        "/data/fixture.kv",
        "/data/sub/../fixture.kv",
        "/other/fixture.kv",
        &path.display().to_string(),
    ] {
        let lines = run(&guarded(denied, ""), narrow.clone(), tree.roots());
        assert!(
            lines.last().unwrap().starts_with("NotAllowedError"),
            "{denied}: {lines:?}"
        );
    }
    // A ceiling narrows, never widens.
    let ceiling = Some(vec![grant(kv::READ_CAPABILITY, &["/data/sub/*.kv"])]);
    let lines = run_with(
        &guarded("/data/fixture.kv", ""),
        read_write(),
        ceiling,
        tree.roots(),
    )
    .unwrap();
    assert!(lines.last().unwrap().starts_with("NotAllowedError"));
    let lines = run_with(
        &guarded("/data/fixture.kv", "    await kv.set(['x'], 1);"),
        read_write(),
        Some(vec![grant(kv::READ_CAPABILITY, &["/data/**"])]),
        tree.roots(),
    )
    .unwrap();
    assert!(lines.last().unwrap().contains("kv.write"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    // The broker audits each decision.
    let bytes = compile(&guarded("/data/fixture.kv", ""));
    let report = crate::VartaRuntime::new().run_with(
        &bytes,
        crate::RunOptions {
            requested: read_write(),
            file_roots: tree.roots(),
            ..crate::RunOptions::default()
        },
    );
    assert!(report.audit.iter().any(
        |entry| entry.operation == kv::READ_CAPABILITY && entry.resource == "/data/fixture.kv"
    ));
}

#[test]
#[cfg(unix)]
fn symbolic_links_never_reach_a_database() {
    use std::os::unix::fs::symlink;
    let tree = Tree::new();
    let outside = tree.0.join("outside/secret.kv");
    std::fs::write(&outside, FIXTURE).unwrap();
    symlink(&outside, tree.root().join("link.kv")).unwrap();
    symlink(tree.0.join("outside"), tree.root().join("dir")).unwrap();
    for path in ["/data/link.kv", "/data/dir/secret.kv"] {
        let lines = run(
            &guarded(path, "    await kv.set(['x'], 1);"),
            read_write(),
            tree.roots(),
        );
        assert!(
            lines[0].starts_with("TypeError: cannot open"),
            "{path}: {lines:?}"
        );
    }
    assert_eq!(std::fs::read(&outside).unwrap(), FIXTURE);
    assert!(!tree.0.join("outside/secret.kv-wal").exists());
}

/// A swap of the verified path between pinning and SQLite's open makes
/// SQLite open another file, which the descriptor check rejects; a symbolic
/// link swapped in fails SQLite's no-follow open.
#[test]
#[cfg(unix)]
fn a_path_swapped_while_opening_is_rejected() {
    let tree = Tree::new();
    let path = tree.fixture("fixture.kv");
    let other = tree.root().join("other.kv");
    std::fs::write(&other, FIXTURE).unwrap();
    let roots = tree.roots();
    let policy = crate::policy::Policy::with_files(
        &read_write(),
        None,
        &crate::policy::FileMode::Virtual(roots),
    );
    let target = || {
        policy
            .database(kv::READ_CAPABILITY, "/data/fixture.kv")
            .unwrap()
    };
    let swap = |replacement: &Path| {
        let replacement = replacement.to_path_buf();
        move |host: &Path| {
            std::fs::rename(host, host.with_extension("moved")).unwrap();
            std::fs::rename(&replacement, host).unwrap();
        }
    };
    let error = store::open_with(target(), "/data/fixture.kv", true, &mut swap(&other))
        .err()
        .unwrap();
    assert_eq!(
        error.message,
        "the KV database /data/fixture.kv changed while it was opened"
    );
    // Restore, then swap in a symbolic link.
    std::fs::rename(&path, &other).unwrap();
    std::fs::rename(path.with_extension("moved"), &path).unwrap();
    let outside = tree.0.join("outside/secret.kv");
    std::fs::write(&outside, FIXTURE).unwrap();
    let mut link = |host: &Path| {
        std::fs::rename(host, host.with_extension("moved")).unwrap();
        std::os::unix::fs::symlink(&outside, host).unwrap();
    };
    let error = store::open_with(target(), "/data/fixture.kv", true, &mut link)
        .err()
        .unwrap();
    assert!(error.message.starts_with("cannot open"), "{error:?}");
    assert_eq!(std::fs::read(&outside).unwrap(), FIXTURE);
    // Without a swap the same target opens.
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(path.with_extension("moved"), &path).unwrap();
    assert!(store::open(target(), "/data/fixture.kv", true).is_ok());
}

#[test]
fn files_of_another_schema_are_rejected_before_any_write() {
    let tree = Tree::new();
    let cases: [(&str, &str); 4] = [
        ("plain.kv", "create table t (x integer);"),
        (
            "old.kv",
            "create table migration_state(k integer not null primary key, version integer not null);\
             insert into migration_state values (0, 2);\
             create table data_version (k integer primary key, version integer not null);\
             create table kv (k blob primary key, v blob not null, v_encoding integer not null, version integer not null) without rowid;\
             create table queue (ts integer); create table queue_running (deadline integer);",
        ),
        ("text.kv", ""),
        ("empty.kv", ""),
    ];
    for (name, sql) in cases {
        let path = tree.root().join(name);
        if name == "text.kv" {
            std::fs::write(&path, "not a database at all, just some text").unwrap();
        } else if !sql.is_empty() {
            rusqlite::Connection::open(&path)
                .unwrap()
                .execute_batch(sql)
                .unwrap();
        }
        let before = std::fs::read(&path).ok();
        let lines = run(
            &guarded(&format!("/data/{name}"), "    await kv.set(['x'], 1);"),
            read_write(),
            tree.roots(),
        );
        if name == "empty.kv" {
            // A missing file is a new database.
            assert_eq!(lines, ["done"]);
            continue;
        }
        assert!(lines[0].starts_with("TypeError"), "{name}: {lines:?}");
        assert_eq!(std::fs::read(&path).ok(), before, "{name}");
    }
    // An existing empty file opened read-only is no database.
    std::fs::write(tree.root().join("zero.kv"), b"").unwrap();
    let lines = run(
        &guarded("/data/zero.kv", ""),
        vec![grant(kv::READ_CAPABILITY, &["/data/**"])],
        tree.roots(),
    );
    assert!(lines[0].contains("has no KV tables"), "{lines:?}");
}

#[test]
fn a_file_locked_by_another_writer_fails_clearly() {
    let tree = Tree::new();
    let path = tree.fixture("fixture.kv");
    let holder = rusqlite::Connection::open(&path).unwrap();
    holder.execute_batch("begin exclusive;").unwrap();
    let lines = run(
        &guarded("/data/fixture.kv", "    await kv.set(['x'], 1);"),
        read_write(),
        tree.roots(),
    );
    assert_eq!(
        lines,
        ["TypeError: the KV database /data/fixture.kv is locked by another writer"]
    );
    holder.execute_batch("rollback;").unwrap();
    drop(holder);
    assert_eq!(
        run(
            &guarded("/data/fixture.kv", "    await kv.set(['x'], 1);"),
            read_write(),
            tree.roots()
        ),
        ["done"]
    );
}

/// A writable database of `tree`'s `fixture.kv`, opened directly.
fn open_fixture(tree: &Tree) -> store::Database {
    let policy = crate::policy::Policy::with_files(
        &read_write(),
        None,
        &crate::policy::FileMode::Virtual(tree.roots()),
    );
    let target = policy
        .database(kv::READ_CAPABILITY, "/data/fixture.kv")
        .unwrap();
    store::open(target, "/data/fixture.kv", true).unwrap()
}

/// One `set` of `["fenced", name]` to the string `"x"`.
fn fenced_set(name: &str) -> (Vec<denokv_proto::Check>, Vec<denokv_proto::Mutation>) {
    let key = denokv_proto::encode_key(&denokv_proto::Key(vec![
        KeyPart::String("fenced".into()),
        KeyPart::String(name.into()),
    ]))
    .unwrap();
    let value = v8::encode(
        &v8::Value::String(vec![u16::from(b'x')]),
        kv::MAX_VALUE_BYTES,
    )
    .unwrap();
    let mutation = denokv_proto::Mutation {
        key,
        kind: MutationKind::Set(KvValue::V8(value)),
        expire_at: None,
    };
    (Vec::new(), vec![mutation])
}

fn fenced_rows(path: &Path) -> usize {
    rows(path)
        .iter()
        .filter(|row| row.0.windows(6).any(|w| w == b"fenced"))
        .count()
}

pub(super) fn waiting_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
}

fn failed_message(result: Result<Option<denokv_proto::Versionstamp>, store::Wait>) -> String {
    match result {
        Err(store::Wait::Failed(error)) => {
            assert_eq!(error.name, kv::error::TYPE_ERROR);
            error.message
        }
        Err(store::Wait::Deadline) => panic!("deadline"),
        Ok(outcome) => panic!("committed: {outcome:?}"),
    }
}

/// A commit fails fast on a write lock another connection holds, and on a
/// turn with too little time left; nothing is applied either way.
#[test]
fn commits_fail_fast_on_a_held_lock_and_a_spent_turn() {
    let tree = Tree::new();
    let path = tree.fixture("fixture.kv");
    let runtime = waiting_runtime();
    let mut database = open_fixture(&tree);
    let holder = rusqlite::Connection::open(&path).unwrap();
    holder.execute_batch("begin immediate;").unwrap();
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_secs(5);
    let (checks, mutations) = fenced_set("busy");
    let message = failed_message(database.commit(&runtime, deadline, checks, mutations));
    assert_eq!(
        message,
        "the KV database /data/fixture.kv is locked by another writer; nothing was applied"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    holder.execute_batch("rollback;").unwrap();
    // Less than the commit reserve left: nothing is submitted.
    let soon =
        std::time::Instant::now() + std::time::Duration::from_millis(kv::COMMIT_RESERVE_MS / 2);
    let (checks, mutations) = fenced_set("late");
    let message = failed_message(database.commit(&runtime, soon, checks, mutations));
    assert!(
        message.starts_with("too little time is left in this turn"),
        "{message}"
    );
    // The database stays usable and commits once the lock is free.
    let (checks, mutations) = fenced_set("free");
    assert!(
        database
            .commit(&runtime, deadline, checks, mutations)
            .unwrap()
            .is_some()
    );
    drop(database);
    assert_eq!(fenced_rows(&path), 1);
}

/// A lock taken after the probe keeps the worker waiting: at the cutoff the
/// commit fails with a `TypeError`, and when the lock is released later the
/// worker's commit is refused at its commit point, so nothing is ever
/// applied and the database accepts nothing more.
#[test]
fn a_commit_that_misses_its_turn_is_never_applied() {
    let tree = Tree::new();
    let path = tree.fixture("fixture.kv");
    let runtime = waiting_runtime();
    let mut database = open_fixture(&tree);
    let holder = rusqlite::Connection::open(&path).unwrap();
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(kv::COMMIT_RESERVE_MS + 300);
    let (checks, mutations) = fenced_set("missed");
    let result = database.commit_with(&runtime, deadline, (checks, mutations), &mut || {
        holder.execute_batch("begin immediate;").unwrap();
    });
    let message = failed_message(result);
    assert_eq!(
        message,
        "the commit to the KV database /data/fixture.kv did not finish in this turn (another writer may hold its lock); nothing was applied"
    );
    assert!(std::time::Instant::now() < deadline);
    // Release the lock: the worker retries, reaches the commit point and is
    // refused there.
    holder.execute_batch("rollback;").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(fenced_rows(&path), 0);
    let later = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let (checks, mutations) = fenced_set("after");
    let message = failed_message(database.commit(&runtime, later, checks, mutations));
    assert!(
        message.contains("unavailable after an unfinished operation"),
        "{message}"
    );
    drop(database);
    drop(holder);
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(fenced_rows(&path), 0);
}

/// The environment variable that makes the ignored test below a lock holder
/// process (set only by the test that spawns it).
const LOCK_HOLDER: &str = "VARTA_KV_TEST_LOCK_HOLDER";

/// Run as a child process: hold the write lock of the file the variable
/// names until its `.release` marker exists.
#[test]
#[ignore = "a helper process of a_write_lock_held_by_another_process_fails_fast"]
fn kv_lock_holder_process() {
    let Some(path) = std::env::var_os(LOCK_HOLDER) else {
        return;
    };
    let path = PathBuf::from(path);
    let holder = rusqlite::Connection::open(&path).unwrap();
    holder.execute_batch("begin immediate;").unwrap();
    std::fs::write(path.with_extension("held"), b"").unwrap();
    let release = path.with_extension("release");
    let started = std::time::Instant::now();
    while !release.exists() && started.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    holder.execute_batch("rollback;").unwrap();
}

/// Another process holding the write lock fails the commit at once, with
/// nothing applied.
#[test]
#[cfg(unix)]
fn a_write_lock_held_by_another_process_fails_fast() {
    let tree = Tree::new();
    let path = tree.fixture("fixture.kv");
    let runtime = waiting_runtime();
    let mut database = open_fixture(&tree);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "kv::tests::kv_lock_holder_process",
            "--ignored",
            "--nocapture",
        ])
        .env(LOCK_HOLDER, &path)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    while !path.with_extension("held").exists() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "no lock holder"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let (checks, mutations) = fenced_set("process");
    let message = failed_message(database.commit(&runtime, deadline, checks, mutations));
    assert!(
        message.ends_with("is locked by another writer; nothing was applied"),
        "{message}"
    );
    std::fs::write(path.with_extension("release"), b"").unwrap();
    assert!(child.wait().unwrap().success());
    drop(database);
    assert_eq!(fenced_rows(&path), 0);
}

#[test]
fn values_outside_the_subset_are_rejected() {
    let tree = Tree::new();
    // Statically: types without a stored form.
    for ty in [
        "Map<string, number>",
        "Date",
        "bigint",
        "{ at: Date }",
        "() => void",
    ] {
        let source = format!(
            "export async function main(): Promise<void> {{ const kv = await varta.kv.open('/data/a.kv'); await kv.get<{ty}>(['x']); }}"
        );
        let error = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V38,
        )
        .err()
        .unwrap_or_else(|| panic!("{ty} compiled"));
        assert!(error.to_string().contains("KV value type"), "{ty}: {error}");
    }
    // At run time: what an `unknown` holds.
    let source = guarded(
        "/data/a.kv",
        "    const values: unknown[] = [new Error('x')];\n\
         for (const value of values) {\n\
           try { await kv.set(['v'], value); } catch (e) { if (e instanceof Error) { console.log(e.message); } }\n\
         }\n\
         const cyclic: { self: unknown } = { self: null };\n\
         cyclic.self = cyclic;\n\
         try { await kv.set(['v'], cyclic); } catch (e) { if (e instanceof Error) { console.log(e.message); } }",
    );
    let source = format!("class Box {{ #secret = 1; }}\n{source}");
    assert_eq!(
        run(&source, read_write(), tree.roots()),
        [
            "an Error cannot be stored in a KV value",
            "a cyclic value cannot be stored in a KV value",
            "done"
        ]
    );
}

#[test]
fn earlier_profiles_have_no_kv() {
    let source = "export async function main(): Promise<void> { const kv = await varta.kv.open('/data/a.kv'); }";
    for profile in [
        LanguageProfile::CheckedF64Utf16V37,
        LanguageProfile::CheckedF64Utf16V36,
    ] {
        assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
    }
    for operation in [
        HostOperation::KvOpen,
        HostOperation::KvRead,
        HostOperation::KvCommit,
        HostOperation::KvStage,
        HostOperation::KvEntry,
        HostOperation::KvKey,
        HostOperation::KvValue,
        HostOperation::KvText,
        HostOperation::KvClose,
    ] {
        assert_eq!(
            operation.minimum_abi(),
            varta_common::abi::PROGRAM_LIMITS_VERSION
        );
        assert!(operation.kv());
        assert_eq!(HostOperation::from_name(operation.name()), Some(operation));
    }
    assert_eq!(
        HostOperation::KvOpen.capability(),
        Some(kv::READ_CAPABILITY)
    );
    assert_eq!(
        HostOperation::KvRead.capability(),
        Some(kv::READ_CAPABILITY)
    );
    assert_eq!(
        HostOperation::KvCommit.capability(),
        Some(kv::WRITE_CAPABILITY)
    );
    assert_eq!(HostOperation::KvValue.capability(), None);
}

#[test]
fn reads_keep_denos_limits() {
    let tree = Tree::new();
    tree.fixture("fixture.kv");
    let source = guarded(
        "/data/fixture.kv",
        "    const keys: KvKey[] = [];\n\
         for (let i = 0; i < 11; i++) { keys.push(['k', i]); }\n\
         try { await kv.getMany<string>(keys); } catch (e) { if (e instanceof Error) { console.log(e.message); } }\n\
         try { for await (const e of kv.list<string>({ prefix: [] }, { batchSize: 1001 })) { console.log(e.versionstamp); } } catch (e) { if (e instanceof Error) { console.log(e.message); } }\n\
         try { kv.list<string>({ prefix: ['a'], start: ['a', 'b'], end: ['a', 'c'] }); } catch (e) { if (e instanceof Error) { console.log(e.name); } }\n\
         try { for await (const e of kv.list<string>({ prefix: ['fixture'], start: ['other'] })) { console.log(e.versionstamp); } } catch (e) { if (e instanceof Error) { console.log(e.message); } }\n\
         try { for await (const e of kv.list<string>({ prefix: ['fixture'] }, { cursor: '!!' })) { console.log(e.versionstamp); } } catch (e) { if (e instanceof Error) { console.log(e.message); } }\n\
         const big = 'x'.repeat(3000);\n\
         try { await kv.get<string>([big]); } catch (e) { if (e instanceof Error) { console.log(e.message); } }",
    );
    assert_eq!(
        run(&source, read_write(), tree.roots()),
        [
            "Too many ranges (max 10)",
            "Too many entries (max 1000)",
            "TypeError",
            "Start key is not in the keyspace defined by prefix",
            "invalid cursor",
            "Key too large for read (max 2049 bytes)",
            "done",
        ]
    );
}

#[test]
fn cursors_are_padded_base64url() {
    for bytes in [&b""[..], b"a", b"ab", b"abc", &[0xFF, 0xFE, 0x00, 0x01]] {
        let text = base64url_encode(bytes);
        assert_eq!(base64url_decode(&text).as_deref(), Some(bytes), "{text}");
    }
    assert_eq!(base64url_encode(&[0xFB, 0xFF]), "-_8=");
    for bad in ["a", "ab=", "a===", "Zg=a", "Zh==", "Z!=="] {
        assert_eq!(base64url_decode(bad), None, "{bad}");
    }
}

/// The KV example of the language guide runs as documented.
#[test]
fn language_guide_example_runs() {
    const GUIDE: &str = include_str!("../../../../docs/language-guide.md");
    let section = &GUIDE[GUIDE.find("#### Deno KV storage (opt-in v38)").unwrap()..];
    let start = section.find("```typescript\n").unwrap() + "```typescript\n".len();
    let source = &section[start..start + section[start..].find("```").unwrap()];
    let tree = Tree::new();
    let grants = vec![
        grant(kv::READ_CAPABILITY, &["/data/app.kv"]),
        grant(kv::WRITE_CAPABILITY, &["/data/app.kv"]),
    ];
    let lines = run(source, grants, tree.roots());
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert!(store::parse_versionstamp(&lines[0]).is_some(), "{lines:?}");
    assert_eq!(lines[1..3], ["completed", "tenant/t-1/run/r-1"]);
    assert!(lines[3].starts_with("committed ") && lines[3] > format!("committed {}", lines[0]));
    assert_eq!(lines[4], "1");
}
