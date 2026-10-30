//! V38 checked JSON parsed by linked guest code (`json_parse_v19` linked
//! through the compiler's test switch, `linked_json_test_hook`) against the
//! host import: the same shared decoder and builder, identical output,
//! exceptions and typed failures, collections during the build, and the
//! fuel the linked decoder costs (why production keeps the host import).
use super::*;
use crate::codegen_v2_tests::{Run, compile};
use varta_common::{HostOperation, language::LanguageProfile as P};

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

/// A v38 module whose checked JSON parse is linked guest code.
fn linked(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::linked_json_test_hook::compile_with_profile(source, V38)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    assert!(
        !imports(&bytes).contains(&HostOperation::JsonParseV19),
        "the linked module imports json_parse_v19"
    );
    bytes
}
fn imports(bytes: &[u8]) -> Vec<HostOperation> {
    let mut operations = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ImportSection(reader) = payload.unwrap() {
            for import in reader {
                if let Some(operation) = HostOperation::from_name(import.unwrap().name) {
                    operations.push(operation);
                }
            }
        }
    }
    operations
}
/// Run with test-only budgets (CLAUDE.md §11): `budgets` production fuel
/// budgets and unbounded host calls; failures are classified as the
/// runtime reports them (typed resources and linked JSON limits).
fn run_test_budgets(bytes: &[u8], budgets: u64) -> Run {
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
    let module = runtime.prepare_module(bytes).unwrap();
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
    error
        .as_ref()
        .map(|error| error.rsplit(": ").next().unwrap_or(error).to_owned())
}

/// The host-import run and the linked run of `source` under `budgets`
/// test-only fuel budgets: identical lines and failure.
fn same(source: &str, budgets: u64) -> (Run, Run) {
    let host_bytes = compile(source, V38);
    assert!(imports(&host_bytes).contains(&HostOperation::JsonParseV19));
    let host = run_test_budgets(&host_bytes, budgets);
    let linked = run_test_budgets(&linked(source), budgets);
    assert_eq!(
        linked.lines, host.lines,
        "{source}\nlinked {:?}\nhost {:?}",
        linked.error, host.error
    );
    assert_eq!(cause(&linked.error), cause(&host.error), "{source}");
    (host, linked)
}

/// Every runtime case program that parses checked JSON behaves identically
/// with the linked decoder (the data-limit cases, which parse megabytes,
/// are measured separately).
#[test]
fn linked_json_matches_the_host_import_on_the_case_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("_cases"))
        .filter(|entry| entry.file_name() != "data_limit_cases")
        .flat_map(|dir| std::fs::read_dir(dir.path()).unwrap().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "ts"))
        .collect();
    files.sort();
    let mut compared = 0;
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        if !source.contains("JSON.parse") && !source.contains("json.parse") {
            continue;
        }
        let Ok(host) = varta_compiler::compile_typescript_with_profile(&source, V38) else {
            continue;
        };
        if !imports(&host).contains(&HostOperation::JsonParseV19)
            || http_contract::read(&host).unwrap().is_some()
        {
            continue;
        }
        same(&source, 400);
        compared += 1;
    }
    assert!(compared >= 15, "only {compared} case programs compared");
}

/// One program decoding a corpus of texts against several schemas, catching
/// and printing every failure: values, `SyntaxError`s and `TypeError`s are
/// identical.
const CORPUS: &str = r#"
interface Item { id: number; name: string; tags: string[]; note?: string; score?: number | null }
interface Box { items: Item[]; meta: Record<string, number>; any: unknown }
function show(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text) as Box);
  } catch (e) {
    if (e instanceof SyntaxError) return "SyntaxError " + e.message;
    if (e instanceof TypeError) return "TypeError " + e.message;
    return "other";
  }
}
function raw(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text));
  } catch (e) {
    return e instanceof SyntaxError ? "SyntaxError" : e instanceof TypeError ? "TypeError" : "other";
  }
}
export function main(): void {
  const texts: string[] = [
    '{"items":[],"meta":{},"any":null}',
    '{"items":[{"id":1,"name":"a","tags":[]}],"meta":{"x":1},"any":[1,"2",{"3":true}]}',
    '{"any":{"b":1,"a":2,"10":3,"2":4},"meta":{"9":9,"1":1,"z":0},"items":[{"tags":["t"],"name":"n","id":-0,"score":null,"note":"é😀"}]}',
    '{"items":[{"id":1e400,"name":"a","tags":[]}],"meta":{},"any":0}',
    '{"items":[{"id":1,"name":"a","tags":[],"extra":1}],"meta":{},"any":0}',
    '{"items":[{"id":1,"name":"a"}],"meta":{},"any":0}',
    '{"items":[{"id":1,"name":"a","tags":[1]}],"meta":{},"any":0}',
    '{"items":[],"meta":{"a":1,"a":2},"any":0}',
    '{"items":[],"meta":{},"any":0',
    '{"items":[],"meta":{},"any":0,}',
    '{"items":[],"meta":{},"any":[1,2,]}',
    '{"items":[],"meta":{},"any":"\ud800"}',
    '{"items":[],"meta":{},"any":"\\ud800\\udc00\\ud800"}',
    '{"items":[],"meta":{},"any":"tab\there"}',
    '{"items":[],"meta":{},"any":01}',
    '{"items":[],"meta":{},"any":-}',
    '{"items":[],"meta":{},"any":1.5e-7}',
    '{"items":[],"meta":{},"any":123456789012345678901234567890}',
    '{"items":[],"meta":{},"any":0.1}',
    '  {"items" : [ ] , "meta" : { } , "any" : true }  ',
    '{"items":[],"meta":{},"any":nul}',
    '[]',
    '',
    '{"items":[{"id":2,"name":"b","tags":["x","y"],"score":3.25}],"meta":{"k":1},"any":{"nested":[[[]]]}}',
  ];
  for (const text of texts) console.log(show(text) + " | " + raw(text));
}
"#;

#[test]
fn linked_json_matches_the_host_import_on_a_differential_corpus() {
    let (host, _) = same(CORPUS, 100);
    assert_eq!(host.error, None);
    assert_eq!(host.lines.len(), 24);
    assert!(host.lines.iter().any(|line| line.contains("SyntaxError")));
    assert!(host.lines.iter().any(|line| line.contains("TypeError")));
}

/// Wide discriminated unions (24 members through `unknown` and a checked
/// conversion), deep nesting and large documents decode identically.
#[test]
fn linked_json_matches_the_host_import_on_unions_depth_and_size() {
    same(include_str!("strict_member_cases/events.ts"), 400);
    // 500 levels of nesting, beyond the frozen depth of 64.
    same(
        "interface Node { c: Node[] } export function main(): void {
           const t = '{\"c\":['.repeat(250) + ']}'.repeat(250);
           const v = varta.json.parse<Node>(t);
           let d = 0; let n: Node = v;
           while (n.c.length > 0) { n = n.c[0]; d++; }
           console.log(String(d) + ' ' + String(JSON.stringify(v).length === t.length));
         }",
        400,
    );
    // 20,000 numbers and 2,000 records.
    same(
        "interface R { k: string; v: number[] } export function main(): void {
           let t = '[';
           for (let i = 0; i < 2000; i++) { if (i > 0) t += ','; t += '{\"k\":\"key' + String(i) + '\",\"v\":[1,2.5,-3,4e2,5,6,7,8,9,10]}'; }
           t += ']';
           const rs = varta.json.parse<R[]>(t);
           let s = 0; for (const r of rs) { for (const x of r.v) s += x; }
           console.log(String(rs.length) + ' ' + String(s) + ' ' + rs[1999].k);
         }",
        4000,
    );
}

/// The depth ceiling is the host's typed limit error, word for word.
#[test]
fn linked_json_reports_the_host_limit_errors() {
    use varta_common::limits::V38_MAX_JSON_DEPTH;
    let levels = V38_MAX_JSON_DEPTH.div_ceil(2) + 1;
    let source = format!(
        "interface Node{{c:Node[]}}export function main():void{{\
         const t='{{\"c\":['.repeat({levels})+']}}'.repeat({levels});\
         console.log(String(varta.json.parse<Node>(t).c.length));}}"
    );
    let (host, linked) = same(&source, 400);
    assert_eq!(
        cause(&host.error).as_deref(),
        Some("checked JSON depth limit")
    );
    assert_eq!(cause(&linked.error), cause(&host.error));
}

/// Collections run while the linked decoder builds (its scratch and the
/// input string rooted, every block published before the next allocation):
/// results kept across many parses amid garbage stay intact, and failed
/// parses in between leave nothing behind.
#[test]
fn linked_json_survives_collections_and_failed_parses() {
    let source = r#"
interface Item { id: number; name: string; tags: string[] }
export function main(): void {
  const kept: Item[][] = [];
  let failures = 0;
  for (let round = 0; round < 60; round++) {
    let garbage = "g";
    for (let j = 0; j < 12; j++) { garbage = garbage + garbage; }
    let text = "[";
    for (let i = 0; i < 20; i++) {
      if (i > 0) text += ",";
      text += '{"id":' + String(round * 100 + i) + ',"name":"n' + String(i) + garbage.charAt(i) + '","tags":["a","b' + String(round) + '"]}';
    }
    text += "]";
    kept.push(JSON.parse(text) as Item[]);
    try { JSON.parse(text.slice(0, text.length - 2)) as Item[]; } catch (e) { failures++; }
    try { JSON.parse(text.replace('"id":', '"id":"')) as Item[]; } catch (e) { failures++; }
  }
  let sum = 0; let names = "";
  for (const items of kept) { for (const item of items) { sum += item.id; } names += items[19].name + items[7].tags[1]; }
  console.log(String(sum) + " " + String(failures) + " " + String(names.length) + " " + names.slice(0, 40));
}
"#;
    let (host, _) = same(source, 400);
    assert_eq!(host.error, None);
    // A full collection before each scheduled job is in
    // `the_24_member_union_survives_forced_collections`; here the parses
    // allocate far beyond the initial heap.
    assert_eq!(host.lines.len(), 1);
}

/// Fuel of the measured JSON workloads with the host import and with the
/// linked decoder (`--nocapture`): the development-plan table.
#[test]
#[ignore]
fn linked_json_fuel_measurement() {
    let items = |n: usize| {
        let mut text = String::from("[");
        for i in 0..n {
            if i > 0 {
                text.push(',');
            }
            text.push_str(&format!(
                "{{\\\"id\\\":{i},\\\"name\\\":\\\"item{i}\\\",\\\"tags\\\":[\\\"a\\\",\\\"b\\\"]}}"
            ));
        }
        text + "]"
    };
    let measure = |label: &str, prelude: &str, text: &str, ty: &str| {
        let program = |parse: bool| {
            format!(
                "{prelude}\nexport function main(): void {{ const t = \"{text}\"; {} }}",
                if parse {
                    format!(
                        "const v = JSON.parse(t) as {ty}; console.log(String(t.length) + typeof v);"
                    )
                } else {
                    "console.log(String(t.length) + \"object\");".to_owned()
                }
            )
        };
        let fuel = |bytes: &[u8]| run_test_budgets(bytes, 400).fuel;
        let base = fuel(&compile(&program(false), V38));
        let v37 = fuel(&compile(&program(true), V37)) - fuel(&compile(&program(false), V37));
        let host = fuel(&compile(&program(true), V38)) - base;
        let linked = fuel(&linked(&program(true))) - base;
        println!("| {label} | {} | {v37} | {host} | {linked} |", text.len());
    };
    println!("| workload | text units | v37 | v38 host import | v38 linked |");
    let prelude = "interface Item { id: number; name: string; tags: string[] }";
    measure("1 record", prelude, &items(1), "Item[]");
    measure("40 records (~2 KB)", prelude, &items(40), "Item[]");
    let numbers = vec!["1.5"; 1000].join(",");
    measure("1000 numbers", "", &format!("[{numbers}]"), "number[]");
    let event = r#"{\"seq\":9,\"detail\":{\"code\":165,\"reason\":\"r15\"},\"type\":\"metric\"}"#;
    measure("RunEventV1-like member (unknown)", "", event, "unknown");
}
