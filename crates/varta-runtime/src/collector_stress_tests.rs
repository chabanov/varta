//! The v38 incremental collector (`incremental_collection`) under stress:
//! collector stress builds (`varta_compiler::with_collector_stress`) run
//! cycles back to back with a few work units per allocation, so the program
//! mutates its graph while nearly every block is being marked; they poison
//! every block a sweep frees and verify every finished cycle with a complete
//! collection whose exact marking traps on a reachable freed block. A store
//! without its write barrier therefore traps or changes the program's
//! results.
use super::*;
use crate::codegen_v2_tests::{Run, run_test_budgets};
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
/// Test-only fuel budgets (CLAUDE.md §11) of one stress run, in production
/// fuel budgets: stress builds collect continuously.
const STRESS_BUDGETS: u64 = 400;
/// Work units per allocation of the stress builds: a short quantum keeps
/// many mutations inside each marking, a longer one completes more cycles.
const STRESS_UNITS: [u32; 3] = [2, 16, 64];

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V38)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
pub(crate) fn stressed(source: &str, units: u32) -> Vec<u8> {
    varta_compiler::with_collector_stress(units, || compile(source))
}

/// The run of a normal build and of the stress builds of each of `units`
/// agree.
fn agree(name: &str, source: &str, units: &[u32]) -> Run {
    let normal = run_test_budgets(&compile(source), STRESS_BUDGETS);
    for &units in units {
        let stress = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
        assert_eq!(
            (&stress.lines, &stress.error),
            (&normal.lines, &normal.error),
            "{name}: stress build ({units} units per allocation) differs"
        );
    }
    normal
}

/// Every store kind mutates the graph while cycles mark: records and class
/// fields, dense, sparse and resizable arrays (element writes, `push`,
/// `pop`, `reverse`, `fill`, length writes and deletion), dictionaries and their hashed tables, captured binding cells
/// and closure environments, `Map`/`Set`, async frames and promises, and
/// generator frames. Each workload checks its own results.
const STORE_KINDS: &[(&str, &str)] = &[
    (
        "record and class fields",
        r#"class Node { constructor(public label: string, public next: Node | undefined) {} }
interface Box { item: string; other: string[] }
export function main(): void {
  let head: Node | undefined = undefined;
  const boxes: Box[] = [];
  for (let i = 0; i < 300; i++) {
    head = new Node(`n${i}`, head);
    const box: Box = { item: `i${i}`, other: [`a${i}`] };
    boxes.push(box);
    // Overwrite older fields with fresh values while marking runs.
    const j = (i * 7) % boxes.length;
    boxes[j].item = `r${i}`;
    boxes[j].other = [`b${i}`, `c${i}`];
    if (head.next !== undefined && i % 3 === 0) { head.next.label = `m${i}`; }
  }
  let count = 0; let text = 0;
  for (let n = head; n !== undefined; n = n.next) { count++; text += n.label.length; }
  for (const b of boxes) { text += b.item.length + b.other.join('').length; }
  console.log(`${count} ${text}`);
}"#,
    ),
    (
        "dense and resizable arrays",
        r#"export function main(): void {
  const rows: string[][] = [];
  for (let i = 0; i < 200; i++) {
    const row: string[] = [];
    for (let j = 0; j < 6; j++) { row.push(`${i}:${j}`); }
    rows.push(row);
    const target = rows[(i * 13) % rows.length];
    if (target.length > 0) { target[i % target.length] = `w${i}`; }
    if (i % 5 === 0) { const last = target.pop(); if (last !== undefined) { target.push(`${last}+`); } }
    if (i % 7 === 0) { target.reverse(); }
    if (i % 19 === 0 && target.length >= 2) { target.fill(`f${i}`, 0, 2); }
    if (i % 23 === 0) { target.length = 3; }
    if (i % 29 === 0) { const n = target.length; target.length = n + 2; target.fill(`l${i}`, n); }
  }
  let text = 0;
  for (const row of rows) { text += row.join(',').length; }
  console.log(`${rows.length} ${text}`);
}"#,
    ),
    (
        "sparse arrays and deletion",
        r#"export function main(): void {
  const xs: string[] = [];
  for (let i = 0; i < 400; i++) {
    xs[i * 3] = `v${i}`;
    if (i % 4 === 0) { delete xs[(i >> 1) * 3]; }
    if (i % 9 === 0) { xs[(i >> 2) * 3] = `u${i}`; }
  }
  let present = 0; let text = 0;
  for (let i = 0; i < xs.length; i++) { const x = xs[i]; if (x !== undefined) { present++; text += x.length; } }
  console.log(`${xs.length} ${present} ${text}`);
}"#,
    ),
    (
        "dictionaries",
        r#"export function main(): void {
  const index: { [key: string]: string[] } = {};
  for (let i = 0; i < 400; i++) {
    const key = `k${i % 97}`;
    const list = index[key];
    if (list === undefined) { index[key] = [`e${i}`]; } else { list.push(`e${i}`); }
    if (i % 5 === 0) { delete index[`k${(i * 3) % 97}`]; }
    if (i % 7 === 0) { index[`k${(i * 5) % 97}`] = [`r${i}`]; }
  }
  let keys = 0; let text = 0;
  for (const key of Object.keys(index)) { const entry = index[key]; keys++; if (entry !== undefined) { text += key.length + entry.join('').length; } }
  console.log(`${keys} ${text}`);
}"#,
    ),
    (
        "captured bindings and environments",
        r#"function counter(prefix: string): () => string {
  let last = prefix;
  let n = 0;
  return () => { n++; last = `${prefix}${n}${last.length}`; return last; };
}
export function main(): void {
  const makers: (() => string)[] = [];
  let shared = "s";
  const bump = (x: string): void => { shared = shared.length > 40 ? x : shared + x; };
  let text = 0;
  for (let i = 0; i < 200; i++) {
    makers.push(counter(`p${i}`));
    const f = makers[(i * 3) % makers.length];
    text += f().length;
    bump(`${i}`);
  }
  console.log(`${makers.length} ${text} ${shared.length}`);
}"#,
    ),
    (
        "Map and Set",
        r#"export function main(): void {
  const map = new Map<string, string[]>();
  const set = new Set<string>();
  for (let i = 0; i < 300; i++) {
    const key = `k${i % 53}`;
    const list = map.get(key);
    if (list === undefined) { map.set(key, [`v${i}`]); } else { list.push(`v${i}`); map.set(key, list.slice(0)); }
    set.add(`s${i % 71}`);
    if (i % 4 === 0) { map.delete(`k${(i * 7) % 53}`); set.delete(`s${(i * 3) % 71}`); }
  }
  let text = 0;
  map.forEach((value, key) => { text += key.length + value.join('').length; });
  set.forEach((value) => { text += value.length; });
  console.log(`${map.size} ${set.size} ${text}`);
}"#,
    ),
    (
        "async frames and promises",
        r#"async function produce(label: string, n: number): Promise<string[]> {
  const out: string[] = [];
  for (let i = 0; i < n; i++) {
    out.push(`${label}-${i}`);
    await Promise.resolve(0);
    out[i] = out[i] + '!';
  }
  return out;
}
export async function main(): Promise<void> {
  const pending: Promise<string[]>[] = [];
  for (let i = 0; i < 12; i++) { pending.push(produce(`t${i}`, 10 + i)); }
  const results = await Promise.all(pending);
  let text = 0;
  for (const r of results) { text += r.join('').length; }
  console.log(`${results.length} ${text}`);
}"#,
    ),
    (
        "generator frames",
        r#"function* words(prefix: string, n: number): Generator<string, void, void> {
  let carried = prefix;
  for (let i = 0; i < n; i++) {
    carried = `${carried.slice(0, 6)}${i}`;
    yield carried;
  }
}
export function main(): void {
  let text = 0; let count = 0;
  const kept: string[] = [];
  for (let g = 0; g < 20; g++) {
    for (const w of words(`g${g}`, 25)) { count++; text += w.length; if (count % 9 === 0) { kept.push(w); } }
  }
  console.log(`${count} ${text} ${kept.join('').length}`);
}"#,
    ),
];

#[test]
fn every_store_kind_survives_mutation_during_incremental_marking() {
    for (name, source) in STORE_KINDS {
        let run = agree(name, source, &STRESS_UNITS);
        assert_eq!(run.error, None, "{name}");
        assert_eq!(run.lines.len(), 1, "{name}: {:?}", run.lines);
    }
}

/// Case directories whose programs need the lifted v38 data limits (their
/// runs are large under stress builds).
const SKIPPED_CASES: [&str; 2] = ["data_limit_cases", "json_limit_cases"];

/// Work units per allocation of the corpus's stress builds (one setting
/// keeps the corpus run short; `STORE_KINDS` runs every setting).
const CORPUS_UNITS: u32 = 32;

/// Every runtime case program behaves the same in stress builds as in normal
/// v38 builds.
#[test]
fn stress_builds_match_normal_builds_over_the_case_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("_cases"))
        .filter(|entry| !SKIPPED_CASES.contains(&&*entry.file_name().to_string_lossy()))
        .flat_map(|dir| std::fs::read_dir(dir.path()).unwrap().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "ts"))
        .collect();
    files.sort();
    let mut compared = 0;
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        let Ok(bytes) = varta_compiler::compile_typescript_with_profile(&source, V38) else {
            continue;
        };
        if http_contract::read(&bytes).unwrap().is_some() {
            continue;
        }
        agree(&path.display().to_string(), &source, &[CORPUS_UNITS]);
        compared += 1;
    }
    assert!(compared >= 120, "only {compared} case programs compared");
}
