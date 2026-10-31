//! V38 linked serialization against the host imports: `JSON.stringify`
//! (`json_stringify_owned`, linked) and logging (the linked
//! `value_format_v12` text through `log_string16`) compared with modules
//! that import `json_stringify_owned` and `value_log_v12` from the host
//! (`host_import_test_hook`). Identical output and failures on the case
//! corpus, on generated value graphs and on the limit cases; collections
//! while the linked serializer runs; owned text results beyond the 1 MiB
//! result buffer; and the fuel of linked serialization against the fair
//! host rates (`--nocapture`, the development-plan table).
use super::*;
use crate::codegen_v2_tests::Run;
use varta_common::{HostOperation as H, language::LanguageProfile as P};

const V38: P = P::CheckedF64Utf16V38;
/// What the host-import builds import instead of linking.
const HOSTED: [H; 2] = [H::JsonStringifyOwned, H::ValueFormatV12];
thread_local! {
    /// Set while the case corpus is compared: programs v38 rejects are
    /// skipped.
    static CORPUS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn imports(bytes: &[u8]) -> Vec<H> {
    let mut operations = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ImportSection(reader) = payload.unwrap() {
            for import in reader {
                if let Some(operation) = H::from_name(import.unwrap().name) {
                    operations.push(operation);
                }
            }
        }
    }
    operations
}
/// The production v38 build: serialization and logging linked.
fn linked(source: &str) -> Vec<u8> {
    let bytes = crate::codegen_v2_tests::compile(source, V38);
    let imported = imports(&bytes);
    for operation in [H::JsonStringifyOwned, H::ValueLogV12, H::ValueFormatV12] {
        assert!(
            !imported.contains(&operation),
            "the linked module imports {}",
            operation.name()
        );
    }
    bytes
}
/// The v38 build importing the host serializer and value logging, if the
/// program uses either; a program v38 rejects is skipped only by the
/// corpus (`CORPUS`).
fn host(source: &str) -> Option<Vec<u8>> {
    let bytes =
        match varta_compiler::host_import_test_hook::compile_with_profile(source, V38, &HOSTED) {
            Ok(bytes) => bytes,
            Err(_) if CORPUS.with(std::cell::Cell::get) => return None,
            Err(error) => panic!("{error}\n{source}"),
        };
    let imported = imports(&bytes);
    if !imported.contains(&H::JsonStringifyOwned) && !imported.contains(&H::ValueLogV12) {
        return None;
    }
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    Some(bytes)
}
/// Run under `budgets` test-only fuel budgets and unbounded host calls,
/// host work charged as the profile charges it (`None`) or explicitly;
/// failures classified as the runtime reports them.
fn run(bytes: &[u8], budgets: u64, rated: Option<bool>) -> Run {
    const HOST_CALLS: u32 = 1 << 20;
    let runtime = VartaRuntime::new();
    let mut execution = execution::Execution::with_test_budgets(
        &runtime.engine,
        varta_common::budget::Budgets {
            fuel_per_turn: budgets * abi::FUEL,
            host_calls_per_turn: HOST_CALLS,
            ..varta_common::budget::Budgets::PRODUCTION
        },
    )
    .unwrap();
    if let Some(rated) = rated {
        execution.rate_host_work(rated);
    }
    crate::validate::host_import::ADMITTED.with(|admitted| admitted.set(true));
    let module = runtime.prepare_module(bytes);
    crate::validate::host_import::ADMITTED.with(|admitted| admitted.set(false));
    let module = module.unwrap();
    let budget = budgets * abi::FUEL;
    execution.store.set_fuel(budget).unwrap();
    execution.allow_test_wall_clock(std::time::Duration::from_secs(300));
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let result = (|| {
        instance
            .get_func(&mut *store, "_start")
            .ok_or_else(|| wasmtime::Error::msg("missing _start"))?
            .call(&mut *store, &[], &mut [])?;
        exceptions::finish(&instance, store)?;
        scheduler::checkpoint(&instance, store)?;
        Ok::<(), RuntimeError>(())
    })()
    .map_err(|error| resources::classify(&instance, store, scheduler::typed(error)));
    let error = result.err().map(|e| {
        let text = format!("{e:#}");
        text.lines().last().unwrap_or_default().trim().to_owned()
    });
    Run {
        lines: std::mem::take(&mut store.data_mut().output.lines),
        fuel: budget - store.get_fuel().unwrap(),
        error,
    }
}
/// The root cause of a failure, without the backtrace prefix.
fn cause(error: &Option<String>) -> Option<String> {
    const SERIALIZATION: &str = "JSON serialization: ";
    error.as_ref().map(|error| match error.find(SERIALIZATION) {
        Some(at) => error[at..].to_owned(),
        None => error.rsplit(": ").next().unwrap_or(error).to_owned(),
    })
}
/// The host-import run and the linked run of `source`: identical lines and
/// failure. `None` when the program neither serializes nor logs a value.
fn same(source: &str, budgets: u64) -> Option<(Run, Run)> {
    let host_bytes = host(source)?;
    let host = run(&host_bytes, budgets, None);
    let linked = run(&linked(source), budgets, None);
    assert_eq!(
        linked.lines, host.lines,
        "{source}\nlinked {:?}\nhost {:?}",
        linked.error, host.error
    );
    assert_eq!(cause(&linked.error), cause(&host.error), "{source}");
    Some((host, linked))
}

/// Every runtime case program that serializes or logs a value behaves
/// identically with linked serialization (the JSON case directories, the
/// data-limit cases and the view cases included).
#[test]
fn linked_serialization_matches_the_host_imports_on_the_case_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("_cases"))
        .flat_map(|dir| std::fs::read_dir(dir.path()).unwrap().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "ts" || e == "tsx"))
        .collect();
    files.sort();
    let mut compared = 0;
    CORPUS.with(|corpus| corpus.set(true));
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        let tsx = path.extension().is_some_and(|e| e == "tsx");
        if tsx || source.contains("varta.http") || source.contains("import ") {
            continue;
        }
        if same(&source, 400).is_some() {
            compared += 1;
        }
    }
    assert!(compared >= 60, "only {compared} case programs compared");
}

/// TypeScript helpers of the generated programs: values widened to
/// `unknown`, dictionaries with hashed tables and holes, sparse arrays.
const HELPERS: &str = r#"
function u(x: unknown): unknown { return x; }
function arr(...xs: unknown[]): unknown[] { return xs; }
function dict(keys: string[], values: unknown[], drop: number): Record<string, unknown> {
  const d: Record<string, unknown> = {};
  for (let i = 0; i < keys.length; i++) d[keys[i]] = values[i % values.length];
  if (drop >= 0 && drop < keys.length) delete d[keys[drop]];
  return d;
}
function sparse(length: number, at: number, value: unknown): unknown[] {
  const a: unknown[] = [];
  a[length - 1] = 0;
  a[at] = value;
  return a;
}
function show(v: unknown): void {
  try { console.log(JSON.stringify(v)); } catch (e) { console.log(e instanceof TypeError ? "TypeError " + e.message : "other"); }
  console.log(v);
}
"#;
/// A small deterministic generator (xorshift64).
struct Generator(u64);
impl Generator {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// A TypeScript expression of a random value graph of at most `depth`
    /// levels, typed `unknown`.
    fn value(&mut self, depth: u32) -> String {
        const SCALARS: &[&str] = &[
            "0",
            "-0",
            "1",
            "-1",
            "2.5",
            "1e21",
            "1e-7",
            "123456789",
            "0.1",
            "NaN",
            "Infinity",
            "-Infinity",
            "2147483647",
            "-2147483648",
            "true",
            "false",
            "null",
            "undefined",
            "\"\"",
            "\"a\"",
            "\"10\"",
            "\"\\ud800\"",
            "\"x\\udc00y\\ud83d\\ude00\"",
            "\"é😀\"",
            "\"\\\"\\\\\\n\\t\\u0001\\u001f\\u007f\"",
            "\"#p\"",
            "5n",
            "-12345678901234567890123n",
        ];
        const KEYS: &[&str] = &[
            "a",
            "b",
            "10",
            "2",
            "0",
            "4294967294",
            "4294967295",
            "01",
            "#p",
            "é",
            "\\ud800",
            "\\\"q",
            "z",
            "toString",
        ];
        if depth == 0 {
            return format!("u({})", self.pick(SCALARS));
        }
        match self.below(9) {
            0..=2 => format!("u({})", self.pick(SCALARS)),
            3 => {
                let items: Vec<String> =
                    (0..self.below(5)).map(|_| self.value(depth - 1)).collect();
                format!("u(arr({}))", items.join(", "))
            }
            4 => {
                // Record keys never start with `#` (private names); dictionary keys may.
                let mut keys: Vec<&str> = (0..self.below(5))
                    .map(|_| self.pick(KEYS))
                    .filter(|key| !key.starts_with('#') && !key.starts_with('\\'))
                    .collect();
                keys.sort_unstable();
                keys.dedup();
                let fields: Vec<String> = keys
                    .iter()
                    .map(|key| format!("\"{key}\": {}", self.value(depth - 1)))
                    .collect();
                format!("u({{ {} }})", fields.join(", "))
            }
            5 => {
                let count = self.below(14);
                let keys: Vec<String> = (0..count)
                    .map(|i| format!("\"{}{i}\"", self.pick(KEYS)))
                    .collect();
                let values: Vec<String> = (0..1 + self.below(3))
                    .map(|_| self.value(depth - 1))
                    .collect();
                format!(
                    "u(dict([{}], [{}], {}))",
                    keys.join(", "),
                    values.join(", "),
                    self.below(count + 2) as i64 - 1
                )
            }
            6 => {
                let length = 1 + self.below(40);
                format!(
                    "u(sparse({length}, {}, {}))",
                    self.below(length),
                    self.value(depth - 1)
                )
            }
            7 => format!(
                "u({{ fn: (x: number): number => x + 1, err: new {}(\"m\\ud800\"), v: {} }})",
                self.pick(&["Error", "TypeError", "RangeError", "SyntaxError"]),
                self.value(depth - 1)
            ),
            // A promise settles with a scalar (one held by `unknown` cannot
            // be adopted).
            _ => format!("u(Promise.resolve({}))", self.value(0)),
        }
    }
}

/// Generated value graphs (records, arrays, hashed dictionaries with holes,
/// sparse arrays, unions, functions, errors, promises, BigInts, lone
/// surrogates, integer-like and private-looking keys) serialize and log
/// identically.
#[test]
fn linked_serialization_matches_the_host_imports_on_generated_graphs() {
    let mut generator = Generator(0x9E37_79B9_7F4A_7C15);
    for program in 0..12 {
        let shows: Vec<String> = (0..24)
            .map(|_| format!("  show({});", generator.value(4)))
            .collect();
        let source = format!(
            "{HELPERS}\nexport function main(): void {{\n{}\n}}\n",
            shows.join("\n")
        );
        let (host, _) = same(&source, 400).unwrap_or_else(|| panic!("program {program}"));
        assert_eq!(host.error, None, "program {program}\n{source}");
        assert_eq!(host.lines.len(), 48, "program {program}");
    }
}

/// Optional fields (absent, present `undefined`, present), presence order
/// and private class state, with the JSON round trip and logging of the
/// console's tool-call shape.
#[test]
fn linked_serialization_matches_the_host_imports_on_records() {
    let source = r#"
interface Call { name: string; arguments: Record<string, unknown>; id?: number; note?: string | null }
class Counter { #count = 2; label = "c"; constructor(public step: number) {} }
export function main(): void {
  const calls: Call[] = [
    { name: "list", arguments: { b: 1, a: [true, null, "x\"y"], "10": 2.5, "2": -0 } },
    { name: "get", arguments: {}, id: 7, note: undefined },
    { name: "set\ud800", arguments: { deep: { deeper: { deepest: [1, [2, [3]]] } } }, note: null },
  ];
  for (const call of calls) {
    const text = JSON.stringify(call.arguments);
    console.log(text);
    const back = JSON.parse(JSON.stringify(call)) as Call;
    console.log(JSON.stringify(back) + " " + String(back.id) + " " + String(back.note));
    const value: unknown = back;
    console.log(value);
    const args: unknown = call.arguments;
    console.log(args);
  }
  const counter = new Counter(3);
  console.log(JSON.stringify(counter));
  console.log(JSON.stringify([counter, counter]));
}
"#;
    let (host, _) = same(source, 100).unwrap();
    assert_eq!(host.error, None);
    assert_eq!(host.lines.len(), 14);
}

/// The host serializer's failures, word for word: a cycle, the depth
/// ceiling of `JSON.stringify` and the depth limit of logging, a BigInt
/// `TypeError` (recoverable, after the whole graph was validated), and the
/// output limit of a logged aggregate.
#[test]
fn linked_serialization_reports_the_host_failures() {
    use varta_common::limits::V38_MAX_JSON_DEPTH;
    let cycle = r#"
interface Node { next: unknown }
export function main(): void {
  const a: Node = { next: null };
  const b: Node = { next: a };
  a.next = b;
  console.log("before");
  console.log(JSON.stringify(a));
}
"#;
    let (host, _) = same(cycle, 10).unwrap();
    assert_eq!(
        cause(&host.error).as_deref(),
        Some("JSON serialization: cyclic object")
    );
    let logged_cycle = cycle.replace(
        "console.log(JSON.stringify(a));",
        "const v: unknown = a; console.log(v);",
    );
    let (host, _) = same(&logged_cycle, 10).unwrap();
    assert_eq!(
        cause(&host.error).as_deref(),
        Some("JSON serialization: cyclic object")
    );
    let levels = V38_MAX_JSON_DEPTH + 1;
    let deep = format!(
        "interface N {{ c: unknown }} export function main(): void {{
           let v: unknown = 0;
           for (let i = 0; i < {levels}; i++) {{ const n: N = {{ c: v }}; v = n; }}
           console.log(String(JSON.stringify(v).length));
         }}"
    );
    let (host, _) = same(&deep, 400).unwrap();
    assert_eq!(
        cause(&host.error),
        Some(format!(
            "JSON serialization: nesting exceeds the host depth ceiling of {V38_MAX_JSON_DEPTH}"
        ))
    );
    // The host renders a logged value recursively on the calling thread
    // (the linked renderer is iterative): compare on the JSON host stack.
    let logged = deep.replace(
        "console.log(String(JSON.stringify(v).length));",
        "console.log(v);",
    );
    let (host, _) = crate::json::deep(|| same(&logged, 400).unwrap()).unwrap();
    assert_eq!(
        cause(&host.error).as_deref(),
        Some("JSON serialization: depth or node limit")
    );
    // Within the ceiling the deep value serializes.
    let within = deep.replace(&format!("i < {levels}"), &format!("i < {}", levels - 2));
    let (host, _) = same(&within, 400).unwrap();
    assert_eq!(host.error, None);
    let bigint = r#"
export function main(): void {
  const values: unknown[] = [1, { a: [2n] }, "s"];
  try { console.log(JSON.stringify(values)); } catch (e) { console.log(e instanceof TypeError ? "TypeError: " + e.message : "other"); }
  const cyclic: { self: unknown; n: bigint } = { self: null, n: 1n };
  cyclic.self = cyclic;
  console.log(JSON.stringify(cyclic));
}
"#;
    let (host, _) = same(bigint, 10).unwrap();
    assert_eq!(
        host.lines[0],
        "varta log: TypeError: invalid BigInt operation"
    );
    assert_eq!(
        cause(&host.error).as_deref(),
        Some("JSON serialization: cyclic object")
    );
    // A logged aggregate whose text exceeds the result buffer's 1 MiB of
    // UTF-8 (the host renderer's output limit).
    let output = r#"
export function main(): void {
  const parts: string[] = [];
  const chunk = "x".repeat(4096);
  for (let i = 0; i < 300; i++) parts.push(chunk);
  const v: unknown = parts;
  console.log("logging");
  console.log(v);
}
"#;
    let (host, _) = same(output, 100).unwrap();
    assert_eq!(
        cause(&host.error).as_deref(),
        Some("JSON serialization: output byte limit")
    );
}

/// Collections while the linked serializer runs (its scratch, the value and
/// the result rooted): serialized and logged values kept amid garbage stay
/// intact, and a failed serialization leaves nothing behind.
#[test]
fn linked_serialization_survives_collections() {
    let source = r#"
interface Item { id: number; name: string; tags: string[]; extra: unknown }
export function main(): void {
  const kept: string[] = [];
  let failures = 0;
  for (let round = 0; round < 40; round++) {
    let garbage = "g";
    for (let j = 0; j < 11; j++) { garbage = garbage + garbage; }
    const items: Item[] = [];
    const extra: Record<string, unknown> = {};
    for (let i = 0; i < 30; i++) {
      extra["k" + String(i)] = [i, "v" + garbage.charAt(i), { n: i }];
      items.push({ id: round * 100 + i, name: "n" + String(i), tags: ["a", "b" + String(round)], extra: i % 2 === 0 ? extra : null });
    }
    kept.push(JSON.stringify(items));
    const big: unknown = { items: items, n: 5n };
    try { JSON.stringify(big); } catch (e) { failures++; }
    if (round % 10 === 0) { const v: unknown = items[0]; console.log(v); }
  }
  let total = 0;
  for (const text of kept) total += text.length;
  console.log(String(total) + " " + String(failures) + " " + kept[39].slice(0, 80));
  console.log(String(kept[0] === kept[0].slice(0)) + " " + String(kept[17].length));
}
"#;
    let (host, _) = same(source, 2000).unwrap();
    assert_eq!(host.error, None);
    assert_eq!(host.lines.len(), 6);
}

/// Owned text results: case mapping, serialization and the logged text of
/// a value beyond the 1 MiB result buffer succeed within the memory budget
/// (they trapped before), and an exhausted budget is the typed memory
/// resource, not a trap.
#[test]
fn linked_text_results_beyond_the_result_buffer() {
    // 600,000 units (1.2 MB of UTF-16) through each text operation.
    // Each in its own function, so the earlier values are garbage by the
    // time the next one is built.
    let source = r#"
function cases(): void {
  const lower = "é".repeat(600000);
  const upper = lower.toUpperCase();
  console.log(String(upper.length) + " " + upper.slice(0, 3) + " " + String(upper.toLowerCase() === lower));
}
function serialized(): void {
  const numbers: number[] = [];
  for (let i = 0; i < 120000; i++) numbers.push(i % 10);
  const text = JSON.stringify(numbers);
  console.log(String(text.length) + " " + text.slice(0, 12));
}
function logged(): void {
  const words: unknown = ["y".repeat(300000), "z".repeat(300000)];
  console.log(words);
}
export function main(): void {
  cases();
  serialized();
  logged();
}
"#;
    let (host, owned) = same(source, 2000).unwrap();
    assert_eq!(owned.error, None);
    assert_eq!(host.lines[0], "varta log: 600000 ÉÉÉ true");
    assert_eq!(host.lines[1], "varta log: 240001 [0,1,2,3,4,5");
    assert_eq!(owned.lines[2].len(), "varta log: ".len() + 600_000 + 7);
    // A result the memory budget cannot hold: the typed memory resource.
    let exhausted = r#"
export function main(): void {
  const lower = "é".repeat(2000000);
  console.log(String(lower.toUpperCase().length));
}
"#;
    let failed = run(&linked(exhausted), 2000, None);
    let error = failed.error.unwrap_or_default();
    assert!(error.contains("memory budget"), "{error}");
}

/// A serialization workload: the value, typed `unknown`, a prelude that
/// builds it, and whether it is logged (`console.log`) or serialized
/// (`JSON.stringify`).
struct Workload {
    label: &'static str,
    prelude: String,
    value: String,
    log: bool,
}
/// The measured workloads of the development-plan table.
fn workloads() -> Vec<Workload> {
    let items = |n: usize| {
        let items: Vec<String> = (0..n)
            .map(|i| format!("{{ id: {i}, name: \"item{i}\", tags: [\"a\", \"b\"] }}"))
            .collect();
        format!("[{}]", items.join(", "))
    };
    let repeated = |item: &str| format!("[{}]", vec![item; 40].join(", "));
    let dictionary = |entries: usize| {
        format!(
            "function d(): Record<string, unknown> {{ const r: Record<string, unknown> = {{}}; \
             for (let i = 0; i < {entries}; i++) r[\"k\" + String(i)] = i; return r; }}"
        )
    };
    let workload = |label, prelude: &str, value: &str, log| Workload {
        label,
        prelude: prelude.to_owned(),
        value: value.to_owned(),
        log,
    };
    vec![
        workload("number", "", "2.5", false),
        workload("short string", "", "\"hello\"", false),
        workload(
            "1 record (3 fields)",
            "",
            "{ id: 1, name: \"n\", tags: [\"a\", \"b\"] }",
            false,
        ),
        workload("40 records (~1.8 KB)", "", &items(40), false),
        workload(
            "1000 numbers",
            "",
            &format!("[{}]", vec!["1.5"; 1000].join(", ")),
            false,
        ),
        workload("40 strings", "", &repeated("\"item12\""), false),
        workload("40 integers", "", &repeated("12"), false),
        workload(
            "40 arrays [\"a\",\"b\"]",
            "",
            &repeated("[\"a\", \"b\"]"),
            false,
        ),
        workload("40 empty arrays", "", &repeated("([] as number[])"), false),
        workload("40 empty records", "", &repeated("{}"), false),
        workload(
            "40 records {a,b,c}",
            "",
            &repeated("{ a: 1, b: 2, c: 3 }"),
            false,
        ),
        workload("dictionary, 5 entries", &dictionary(5), "d()", false),
        workload(
            "dictionary, 20 entries (hashed)",
            &dictionary(20),
            "d()",
            false,
        ),
        workload("log: number cell", "", "2.5", true),
        workload("log: string cell", "", "\"hello\"", true),
        workload(
            "log: 1 record",
            "",
            "{ id: 1, name: \"n\", tags: [\"a\", \"b\"] }",
            true,
        ),
        workload("log: 40 records", "", &items(40), true),
        workload("log: fulfilled promise", "", "Promise.resolve(7)", true),
    ]
}
/// Fuel of one serialization or logged value beyond the same program
/// without it: the host-import build uncharged and at the fair rates, and
/// the linked build.
fn workload_fuel(workload: &Workload) -> (i64, i64, i64) {
    let program = |serialize: bool| {
        let body = match (serialize, workload.log) {
            (true, false) => "console.log(String(JSON.stringify(v).length));",
            (true, true) => "console.log(v);",
            (false, _) => "console.log(String(typeof v));",
        };
        format!(
            "{}\nexport function main(): void {{ const v: unknown = {}; {body} }}",
            workload.prelude, workload.value
        )
    };
    let fuel = |bytes: &[u8], rated| run(bytes, 400, rated).fuel as i64;
    let delta = |linked_build: bool, rated: Option<bool>| {
        let build = |serialize: bool| {
            let source = program(serialize);
            if linked_build {
                linked(&source)
            } else {
                varta_compiler::host_import_test_hook::compile_with_profile(&source, V38, &HOSTED)
                    .unwrap()
            }
        };
        fuel(&build(true), rated) - fuel(&build(false), rated)
    };
    (
        delta(false, Some(false)),
        delta(false, Some(true)),
        delta(true, None),
    )
}

/// The development-plan table (`--nocapture`).
#[test]
#[ignore]
fn linked_serialization_fuel_measurement() {
    println!(
        "| workload | v38 import, uncharged | v38 import, fair rates | v38 linked | linked / fair |"
    );
    for workload in workloads() {
        let (free, fair, linked) = workload_fuel(&workload);
        println!(
            "| {} | {free} | {fair} | {linked} | {:.2} |",
            workload.label,
            linked as f64 / fair as f64
        );
    }
}

/// Linked serialization costs its instructions: at most these multiples of
/// the host import's fair charge for the same work, as measured (open in
/// the development plan: the stringify rates or further optimization). A
/// change that makes linked serialization costlier fails here.
#[test]
fn linked_serialization_stays_within_its_measured_cost() {
    const CEILINGS: &[(&str, f64)] = &[
        ("40 records (~1.8 KB)", 1.15),
        ("1000 numbers", 0.65),
        ("40 strings", 0.75),
        ("40 integers", 0.65),
        ("40 records {a,b,c}", 1.2),
        ("log: 40 records", 1.2),
        ("dictionary, 5 entries", 2.7),
        ("log: fulfilled promise", 3.6),
        ("number", 1.5),
    ];
    let workloads = workloads();
    for &(label, ceiling) in CEILINGS {
        let workload = workloads.iter().find(|w| w.label == label).unwrap();
        let (_, fair, linked) = workload_fuel(workload);
        assert!(
            linked as f64 <= fair as f64 * ceiling,
            "{label}: linked {linked} > {ceiling} x fair {fair}"
        );
    }
}
