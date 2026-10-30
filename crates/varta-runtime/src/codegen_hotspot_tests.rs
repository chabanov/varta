//! V38 `codegen_v2` hotspots: `push` without argument arrays, checked element
//! reads without union cells, quiet function-value calls, word-stored literal
//! backings and the exact-size free lists of the allocator. Every program runs
//! under v37 and v38 with identical output; the stress programs force many
//! collections with live aliases, captured arrays and mixed block sizes.
use super::*;
use crate::codegen_v2_tests::{compile, lines, run_test_budgets};
use varta_common::language::LanguageProfile as P;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

/// Output and failure under v38 equal v37's, with no more fuel; returns the
/// v38 output lines. `budgets` production fuel budgets and an unbounded
/// host-call count (test-only, since v37 spends host calls on text).
fn same_as_v37(source: &str, budgets: u64) -> Vec<String> {
    let old = run_test_budgets(&compile(source, V37), budgets);
    let new = run_test_budgets(&compile(source, V38), budgets);
    assert_eq!(old.lines, new.lines, "{source}");
    assert_eq!(old.error, new.error, "{source}");
    assert!(
        new.fuel <= old.fuel,
        "v38 {} > v37 {}\n{source}",
        new.fuel,
        old.fuel
    );
    new.lines
}

/// `push` with one to four arguments appends without an argument array, and
/// a longer list keeps the argument array; arguments are evaluated before the
/// append (also when they push to the same array), the result is the new
/// length, aliases share the array, and reference slots survive collections
/// forced by every growth.
#[test]
fn push_appends_static_argument_lists_with_javascript_order() {
    let source = r#"interface Row { key: string; tags: string[] }
function churn(): void { let g = "g"; for (let j = 0; j < 7; j++) { g = g + g; } }
function present(xs: number[], i: number): boolean { try { return xs[i] === xs[i]; } catch (e) { return false; } }
function grow(xs: string[], text: string): string { xs.push(text + "!"); churn(); return text; }
export function main(): void {
  const a: number[] = [];
  const b = a;
  let lengths = "";
  lengths += String(a.push(1)) + ",";
  lengths += String(a.push(2, 3)) + ",";
  lengths += String(a.push(a.length, a.length + 1, 6)) + ",";
  lengths += String(a.push(7, 8, 9, 10)) + ",";
  lengths += String(a.push(11, 12, 13, 14, 15)) + ",";
  console.log(lengths + " " + b.join(" "));
  const words: string[] = [];
  for (let i = 0; i < 300; i++) {
    words.push(grow(words, "w" + String(i % 10)));
    if (i % 50 === 0) { churn(); }
  }
  let check = 0;
  for (let i = 0; i < words.length; i += 2) {
    if (words[i] !== "w" + String((i / 2) % 10) + "!" || words[i + 1] !== "w" + String((i / 2) % 10)) { check++; }
  }
  console.log(String(words.length) + " " + String(check));
  const rows: Row[] = [];
  for (let i = 0; i < 200; i++) {
    const row: Row = { key: "k" + String(i), tags: [] };
    rows.push(row, row);
    row.tags.push("t", row.key);
    churn();
  }
  let same = 0;
  for (let i = 0; i < rows.length; i += 2) { if (rows[i] === rows[i + 1] && rows[i].tags[1] === rows[i].key) { same++; } }
  console.log(String(rows.length) + " " + String(same));
  const flags: boolean[] = [];
  flags.push(true, false);
  const empty: number[] = [];
  empty.length = 3;
  empty.push(4);
  console.log(String(flags[0]) + String(flags[1]) + " " + String(empty.length) + " " + String(empty.indexOf(4)) + " " + String(present(empty, 0)));
}"#;
    assert_eq!(
        same_as_v37(source, 20),
        lines(&[
            "1,3,6,10,15, 1 2 3 3 4 6 7 8 9 10 11 12 13 14 15",
            "600 0",
            "400 200",
            "truefalse 4 3 false",
        ])
    );
}

/// Checked element reads load the slot without a union cell, and every missing
/// read (hole, negative, fractional, NaN or out-of-range index, iterated
/// hole) still throws the same catchable `TypeError`, also inside quiet
/// arithmetic, conditionals and call arguments.
#[test]
fn checked_element_reads_unbox_and_throw_like_v37() {
    let source = r#"interface P { x: number }
function id(n: number): number { return n; }
function attempt(xs: number[], i: number): string {
  try { let t = 0; t = t + xs[i] * 2; return String(t); } catch (e) { return e instanceof TypeError ? "T" : "?"; }
}
export function main(): void {
  const xs: number[] = [1, 2, 3];
  xs.length = 5;
  xs[4] = 5;
  let out = "";
  for (const i of [0, 1, 2, 3, 4, 5, -1, 1.5, NaN, -0]) { out += attempt(xs, i) + " "; }
  console.log(out);
  const words: string[] = ["a", "b"];
  const flags: boolean[] = [true];
  const points: P[] = [{ x: 7 }];
  console.log(words[1] + String(flags[0]) + String(points[0].x) + String(id(xs[2]) + (xs[0] > 0 ? xs[1] : xs[2])));
  let iterated = 0;
  try { for (const v of xs) { iterated += v; } } catch (e) { console.log("hole " + String(iterated) + " " + String(e instanceof TypeError)); }
  const mixed: (number | string)[] = [1, "two"];
  console.log(String(mixed[1]) + String(mixed[0]));
}"#;
    assert_eq!(
        same_as_v37(source, 1),
        lines(&["2 4 6 T 10 T T T T 2 ", "btrue75", "hole 6 true", "two1"])
    );
}

/// Calls of function values with quiet arguments take no marks or operand
/// roots: closures capturing arrays and strings keep them across collections
/// their bodies force, also through function fields and recursion.
#[test]
fn quiet_function_value_calls_keep_captures_across_collections() {
    let source = r#"interface Box { get: () => string; add: (n: number) => number }
function make(seed: string): Box {
  const items: string[] = [seed];
  let total = 0;
  return {
    get: (): string => { let g = "x"; for (let j = 0; j < 8; j++) { g = g + g; } items.push(seed + String(items.length)); return items[items.length - 1] + String(g.length); },
    add: (n: number): number => { total = total + n; return total; },
  };
}
export function main(): void {
  const boxes: Box[] = [];
  for (let i = 0; i < 20; i++) { boxes.push(make("s" + String(i))); }
  let text = "";
  let sum = 0;
  for (let round = 0; round < 10; round++) {
    for (const box of boxes) { const read = box.get; text = read(); sum = box.add(round); }
  }
  const twice = (f: (n: number) => number, n: number): number => f(f(n));
  const inc = (n: number): number => n + 1;
  console.log(text + " " + String(sum) + " " + String(twice(inc, 40)));
}"#;
    assert_eq!(same_as_v37(source, 20), lines(&["s1910256 45 42"]));
    // A named function expression reads its own value after the caller's
    // binding stopped holding it and after collections: such calls keep the
    // callee rooted.
    let named = r#"export function main(): void {
  let f: (n: number) => number = (n: number): number => n;
  f = function count(n: number): number {
    if (n <= 0) { return 0; }
    f = (m: number): number => m * 100;
    let g = "x";
    for (let j = 0; j < 12; j++) { g = g + g; }
    return 1 + count(n - 1);
  };
  const first = f(6);
  console.log(String(first) + " " + String(f(2)));
}"#;
    assert_eq!(same_as_v37(named, 20), lines(&["6 200"]));
}

/// The exact-size free lists reuse blocks of many sizes across many
/// collections without reclaiming live aliases: a live set of strings,
/// records and arrays of every class size is rebuilt while garbage of the same
/// sizes is dropped, and memory stays below the budget.
#[test]
fn free_lists_reuse_mixed_sizes_across_collections_without_losing_aliases() {
    let source = r#"interface Cell { text: string; parts: number[]; next: Cell | undefined }
export function main(): void {
  let live: Cell | undefined = undefined;
  const kept: string[] = [];
  let checksum = 0;
  for (let round = 0; round < 60; round++) {
    let filler = "f";
    for (let j = 0; j < 14; j++) { filler = filler + filler; }
    checksum += filler.length > 0 ? 0 : 1;
    let s = "";
    for (let i = 0; i < 40; i++) {
      s = s + ((round + i) % 3 === 0 ? "ab" : "c");
      const parts: number[] = [];
      for (let j = 0; j < (i % 9); j++) { parts.push(j); }
      const cell: Cell = { text: s.slice(0, (i % 33) + 1), parts, next: undefined };
      if (i % 7 === 0) { cell.next = live; live = cell; }
      if (i % 13 === 0) { kept.push(cell.text); }
    }
    if (round % 6 === 5 && round < 55) { live = undefined; }
  }
  let count = 0;
  let cursor = live;
  while (cursor !== undefined) { count++; checksum += cursor.text.length + cursor.parts.length; cursor = cursor.next; }
  let text = 0;
  for (const k of kept) { text += k.length; }
  console.log(String(count) + " " + String(checksum) + " " + String(kept.length) + " " + String(text));
}"#;
    let output = same_as_v37(source, 200);
    assert_eq!(output.len(), 1);
    // Collections ran: the profiled run calls the collector repeatedly.
    let budgets = varta_common::budget::BudgetRequest {
        fuel_per_turn: Some(200 * abi::FUEL),
        host_calls_per_turn: Some(1 << 16),
        ..Default::default()
    };
    let report = VartaRuntime::new().run_with(
        &crate::fuel_profile_tests::profiled(source, V38),
        RunOptions {
            budgets,
            budget_ceiling: Some(budgets),
            fuel_profile: true,
            ..RunOptions::default()
        },
    );
    assert!(report.failure.is_none(), "{:?}", report.failure);
    let collections = report
        .fuel_profile
        .unwrap()
        .functions
        .iter()
        .find(|row| row.name == "<helper Collect>")
        .map_or(0, |row| row.calls);
    assert!(collections >= 3, "{collections} collections");
}

/// Literal backings stored word by word read like filled ones: holes, spare
/// capacity, presence across the first bitmap word and the largest stored
/// literal, then a backing above the stored size.
#[test]
fn word_stored_literal_backings_keep_holes_and_spare_slots() {
    let source = r#"export function main(): void {
  const empty: number[] = [];
  const holes: number[] = [1, , 3, , 5];
  const present = (xs: number[], i: number): boolean => { try { return xs[i] === xs[i]; } catch (e) { return false; } };
  const full: number[] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  const big: number[] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
  let shown = "";
  for (let i = 0; i < 6; i++) { shown += String(present(holes, i)) + ","; }
  empty.length = 2;
  holes.push(6);
  full.push(16);
  console.log(shown + " " + String(empty.length) + String(present(empty, 0)) + " " + String(holes.length) + String(holes.indexOf(6)) + " " + String(full.length) + String(full[16]) + " " + String(big.length) + String(big[16]) + " " + JSON.stringify(holes.slice(2)));
}"#;
    assert_eq!(
        same_as_v37(source, 1),
        lines(&["true,false,true,false,true,false, 2false 65 1716 1716 [3,null,5,6]"])
    );
}
