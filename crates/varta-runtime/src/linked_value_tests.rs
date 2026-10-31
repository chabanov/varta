//! V38 `linked_intrinsics`, wave 3: the primitive conversions and
//! comparisons (`primitive_to_string_v6`, `primitive_to_number_v6`,
//! `primitive_compare`, `primitive_equal`) and the value18 readers
//! (`value_typeof_v11`, `value_truthy_v11`, `value_nullish_v11`,
//! `value_is_array_v11`, `value_strict_equal_v11`, `value_nullish_equal_v11`,
//! `value_is_error_class_v7`) are linked behind codegen-v2 fast paths.
//!
//! Each corpus runs three ways and prints the same lines: v37 (host imports),
//! v38 (fast paths with the linked code behind them) and v38 with every fast
//! path replaced by a call of its linked operation alone, so the linked code
//! itself answers every input. V38 imports none of these operations, never
//! costs more fuel than the v37 host calls on the measured loops, and keeps
//! its results across forced collections.
use super::*;
use crate::contract_tests::rewrite;
use varta_common::{HostOperation, language::LanguageProfile as P, profile};
use wasmparser::{Operator, Parser, Payload};

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

/// Every primitive kind at the conversion and comparison boundaries:
/// signed zeros, NaN, infinities, the 2^53 and 1e21 boundaries, the
/// extremes and subnormals, numeric and non-numeric strings (lone
/// surrogates and supplementary characters included), booleans, `null`,
/// `undefined` and BigInts. Each line prints `String(a)`, `Number(a)` (and
/// the sign of a zero) and one character per right operand encoding `<`,
/// `<=`, `>`, `>=`, `==` and `===`.
const CONVERSIONS: &str = r#"
type P = number | string | boolean | null | undefined | bigint;
function values(): P[] {
  return [0, -0, 1, -1, 0.1, -2.5, 123.456, 9007199254740991, 9007199254740992,
    9007199254740993, -9007199254740992, 18014398509481984, 1e21, 999999999999999900000,
    1e21 - 65536, 123456789012345680000, 5e-324, -5e-324, 2.2250738585072014e-308,
    1.7976931348623157e308, -1.7976931348623157e308, NaN, Infinity, -Infinity, 1e-7,
    0.000001, 4294967295, -4294967296, 2 ** 31, 1 / 3,
    "", "0", "-0", " 12 ", "1e3", "0x1F", "-0x1F", "0b101", "0o17", "Infinity", "-Infinity",
    "abc", "\uD800", "\uDC00x", "😀", "12n", "1_000", " \t\n", "9007199254740993",
    "1e21", "5e-324", "NaN", "true", "null", "007", "123456789012345", "1234567890123456",
    "12a",
    true, false, null, undefined, 0n, -1n, 5n, 12345678901234567890n, -(1n << 64n)];
}
function sign(n: number): string {
  return n === 0 ? (1 / n > 0 ? "+0" : "-0") : "";
}
function pairs(a: P, b: P): number {
  return (a < b ? 1 : 0) + (a <= b ? 2 : 0) + (a > b ? 4 : 0) + (a >= b ? 8 : 0)
    + (a == b ? 16 : 0) + (a === b ? 32 : 0);
}
const DIGITS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_";
export function main(): void {
  const xs = values();
  for (const a of xs) {
    const n = Number(a);
    let line = String(a) + "|" + String(n) + sign(n) + "|" + ("v" + a) + "|";
    for (const b of xs) {
      line = line + DIGITS.charAt(pairs(a, b));
    }
    console.log(line);
  }
  // Plain Numbers boxed nowhere and safe integers around every digit count.
  let digits = "";
  let p = 1;
  for (let i = 0; i < 17; i++) {
    digits = digits + String(p - 1) + "," + String(p) + "," + String(-p) + "," + (p + 0.5) + ";";
    p = p * 10;
  }
  console.log(digits);
}
"#;

/// Every value18 reference-cell kind held by `unknown`: Numbers, strings,
/// booleans, `null`, `undefined`, BigInts, arrays (dense and grown), records,
/// small and hashed dictionaries, Error objects of every built-in class, a
/// function and a promise. Each line prints `typeof`, truthiness, the
/// nullish checks, `Array.isArray` and `instanceof` of each error class, then
/// one character per right operand for `===` and `== null`.
const READERS: &str = r#"
function values(): unknown[] {
  const grown: number[] = [];
  for (let i = 0; i < 40; i++) { grown.push(i); }
  const hashed: Record<string, number> = {};
  for (let i = 0; i < 20; i++) { hashed["k" + i] = i; }
  delete hashed["k3"];
  const small: Record<string, number> = { a: 1, b: 2 };
  const empty: number[] = [];
  const f = (x: number): number => x + 1;
  return [0, -0, NaN, 1, -1.5, Infinity, "", "a", "\uD800", true, false, null, undefined,
    0n, 7n, -7n, empty, [1, "x"], grown, {}, { a: 1, b: [2] }, small, hashed,
    new Error("e"), new TypeError("t"), new RangeError("r"), new SyntaxError("s"),
    f, Promise.resolve(1), "a", 1, 7n];
}
function pick(i: number): string | number[] | bigint | null | undefined {
  if (i === 0) { return ""; }
  if (i === 1) { return "s"; }
  if (i === 2) { return []; }
  if (i === 3) { return 0n; }
  if (i === 4) { return 3n; }
  if (i === 5) { return null; }
  return undefined;
}
export function main(): void {
  // `||` and `??` of a reference union (guest truth since v23, so no
  // truthiness or nullish reader is called).
  let logical = "";
  for (let i = 0; i < 7; i++) {
    const r = pick(i);
    const o = r || "or";
    const q = r ?? "nullish";
    logical = logical + (typeof o === "string" ? o : typeof o) + "/"
      + (typeof q === "string" ? q : typeof q) + ";";
  }
  console.log(logical);
  const xs = values();
  for (const a of xs) {
    let line = typeof a + "|" + (a ? "T" : "F") + (a == null ? "N" : "-")
      + (a === null ? "n" : "-") + (a === undefined ? "u" : "-") + (Array.isArray(a) ? "A" : "-")
      + (a instanceof Error ? "E" : "-") + (a instanceof TypeError ? "t" : "-")
      + (a instanceof RangeError ? "r" : "-") + (a instanceof SyntaxError ? "s" : "-") + "|";
    for (const b of xs) {
      line = line + (a === b ? "=" : a == null && b == null ? "~" : ".");
    }
    console.log(line);
  }
}
"#;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn imports(bytes: &[u8]) -> Vec<HostOperation> {
    let mut operations = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(reader) = payload.unwrap() {
            for import in reader {
                if let Some(operation) = HostOperation::from_name(import.unwrap().name) {
                    operations.push(operation);
                }
            }
        }
    }
    operations
}
/// The operations of this stream, in both their v37 and v38 forms.
fn stream_operation(operation: HostOperation) -> bool {
    use HostOperation as H;
    let operation = V38.host_operation(operation);
    matches!(
        operation,
        H::PrimitiveStringV6
            | H::PrimitiveNumberV6
            | H::PrimitiveCompare
            | H::PrimitiveEqual
            | H::ValueTypeOfV11
            | H::ValueTruthyV11
            | H::ValueNullishV11
            | H::ValueIsArrayV11
            | H::ValueStrictEqualV11
            | H::ValueNullishEqualV11
            | H::ValueIsErrorClassV7
    )
}
/// Test-only fuel budgets of a corpus run (CLAUDE.md §11): the conversion
/// corpus compares every pair of its 67 values six ways.
const CORPUS_BUDGETS: u64 = 400;
/// Output and fuel under `budgets` production fuel budgets (test-only
/// budgets, CLAUDE.md §11, with unbounded host calls).
fn execute(bytes: &[u8], budgets: u64) -> crate::codegen_v2_tests::Run {
    let run = crate::codegen_v2_tests::run_test_budgets(bytes, budgets);
    assert_eq!(run.error, None);
    run
}

/// The v37 output of `source` (host imports of this stream), checked to be
/// what v38 prints and v38 to import none of them.
fn same_as_host(source: &str) -> Vec<String> {
    let old = compile(source, V37);
    assert!(
        imports(&old).into_iter().any(stream_operation),
        "the corpus must exercise a host import of this stream"
    );
    let new = compile(source, V38);
    let linked: Vec<_> = imports(&new)
        .into_iter()
        .filter(|operation| operation.linked())
        .collect();
    assert!(linked.is_empty(), "v38 imports {linked:?}");
    let host = execute(&old, CORPUS_BUDGETS).lines;
    assert_eq!(execute(&new, CORPUS_BUDGETS).lines, host, "{source}");
    host
}

/// A profiled v38 build of `source` whose fast-path helpers each only call
/// the linked operation behind them (the first linked call in their body:
/// the BigInt-capable conversion where a helper has two), so the linked
/// code answers every input. The profile build names every guest body
/// (`profile::NAMES_SECTION`); linked library functions follow the guest's.
fn linked_only(source: &str) -> (Vec<u8>, Vec<String>) {
    let bytes = crate::fuel_profile_tests::profiled(source, V38);
    let mut imported = 0;
    let mut names = Vec::new();
    let mut result_len = None;
    let mut globals = 0;
    // The parameter types of every function, by function index.
    let mut types = Vec::new();
    let mut params: Vec<Vec<wasmparser::ValType>> = Vec::new();
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            Payload::TypeSection(reader) => {
                for ty in reader.into_iter_err_on_gc_types() {
                    types.push(ty.unwrap().params().to_vec());
                }
            }
            Payload::ImportSection(reader) => {
                imported = reader.count();
                for import in reader {
                    if let wasmparser::TypeRef::Func(ty) = import.unwrap().ty {
                        params.push(types[ty as usize].clone());
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for ty in reader {
                    params.push(types[ty.unwrap() as usize].clone());
                }
            }
            Payload::GlobalSection(reader) => globals = reader.count(),
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.unwrap();
                    if export.name == abi::RESULT_LEN {
                        result_len = Some(export.index);
                    }
                }
            }
            Payload::CustomSection(section) if section.name() == profile::NAMES_SECTION => {
                names = profile::decode_names(section.data()).unwrap();
            }
            _ => {}
        }
    }
    let result_len = result_len.unwrap();
    assert!(result_len < globals);
    let helper = |name: &str| {
        names
            .iter()
            .position(|label| label == &format!("<helper {name}>"))
            .map(|position| imported + position as u32)
    };
    let snapshot = helper("Snapshot");
    let guest_end = imported + names.len() as u32;
    let mut replaced = Vec::new();
    let bytes = rewrite(&bytes, |id, section| {
        const CODE_SECTION: u8 = 10;
        if id != CODE_SECTION {
            return vec![section.to_vec()];
        }
        let reader =
            wasmparser::CodeSectionReader::new(wasmparser::BinaryReader::new(section, 0)).unwrap();
        let mut content = Vec::new();
        leb(&mut content, reader.count());
        for (position, body) in reader.into_iter().enumerate() {
            let body = body.unwrap();
            let name = names.get(position).map_or("", String::as_str);
            let forwarded: &[u32] = match name {
                "<helper PrimitiveText(true)>"
                | "<helper PrimitiveText(false)>"
                | "<helper PrimitiveNumber(true)>"
                | "<helper PrimitiveNumber(false)>"
                | "<helper ValueTypeOf>"
                | "<helper ValueTruthy>"
                | "<helper ValueNullish>"
                | "<helper ValueIsArray>" => &[0],
                "<helper PrimitiveOrder(Cells { bigint: true, string: true })>"
                | "<helper ValueStrictEqual>"
                | "<helper ValueNullishEqual>"
                | "<helper ValueIsErrorClass>" => &[0, 1],
                "<helper PrimitiveEquals(Cells { bigint: true, string: true })>" => &[0, 1, 2],
                _ => {
                    let original = &section[body.range()];
                    leb(&mut content, original.len() as u32);
                    content.extend_from_slice(original);
                    continue;
                }
            };
            // The helper's own operation: the first linked call taking the
            // helper's parameters (a comparison also calls the linked
            // `ToNumber` of one string).
            let own = &params[imported as usize + position];
            let linked = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .find_map(|operator| match operator.unwrap() {
                    Operator::Call { function_index }
                        if function_index >= guest_end
                            && params.get(function_index as usize) == Some(own) =>
                    {
                        Some(function_index)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name} calls no linked operation"));
            replaced.push(name.to_owned());
            // No locals; the parameters, the linked call and, for a string
            // result, its snapshot.
            let mut function = vec![0];
            for &param in forwarded {
                function.push(LOCAL_GET);
                leb(&mut function, param);
            }
            function.push(CALL);
            leb(&mut function, linked);
            if matches!(
                name,
                "<helper ValueTypeOf>"
                    | "<helper PrimitiveText(true)>"
                    | "<helper PrimitiveText(false)>"
            ) {
                function.push(GLOBAL_GET);
                leb(&mut function, result_len);
                function.push(CALL);
                leb(&mut function, snapshot.unwrap());
            }
            function.push(END);
            leb(&mut content, function.len() as u32);
            content.extend(function);
        }
        vec![content]
    });
    replaced.sort();
    (bytes, replaced)
}
const LOCAL_GET: u8 = 0x20;
const GLOBAL_GET: u8 = 0x23;
const CALL: u8 = 0x10;
const END: u8 = 0x0b;
fn leb(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        bytes.push(byte | if value != 0 { 0x80 } else { 0 });
        if value == 0 {
            break;
        }
    }
}
/// Output of the linked-only build, which replaced exactly the fast paths
/// `helpers`.
fn linked_lines(source: &str, helpers: &[&str]) -> Vec<String> {
    let (bytes, replaced) = linked_only(source);
    let mut expected: Vec<String> = helpers.iter().map(|h| format!("<helper {h}>")).collect();
    expected.sort();
    assert_eq!(replaced, expected);
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    let report = VartaRuntime::new().run_with(
        &bytes,
        RunOptions {
            fuel_profile: true,
            budgets: varta_common::budget::BudgetRequest {
                fuel_per_turn: Some(CORPUS_BUDGETS * abi::FUEL),
                host_calls_per_turn: Some(1 << 20),
                ..Default::default()
            },
            budget_ceiling: Some(varta_common::budget::BudgetRequest {
                fuel_per_turn: Some(CORPUS_BUDGETS * abi::FUEL),
                host_calls_per_turn: Some(1 << 20),
                ..Default::default()
            }),
            ..RunOptions::default()
        },
    );
    assert!(report.failure.is_none(), "{:?}", report.failure);
    report.output.lines
}

#[test]
fn linked_conversions_and_comparisons_match_the_host() {
    let host = same_as_host(CONVERSIONS);
    assert_eq!(host.len(), 68);
    assert!(host[1].starts_with("varta log: 0|0-0|v0|"), "{}", host[1]);
    assert_eq!(
        linked_lines(
            CONVERSIONS,
            &[
                "PrimitiveText(true)",
                "PrimitiveNumber(true)",
                "PrimitiveOrder(Cells { bigint: true, string: true })",
                "PrimitiveEquals(Cells { bigint: true, string: true })"
            ]
        ),
        host
    );
}

#[test]
fn linked_value_readers_match_the_host() {
    let host = same_as_host(READERS);
    assert_eq!(host.len(), 33);
    assert!(
        host.iter().any(|line| line.contains("object|T----E-r-|")),
        "{host:?}"
    );
    assert_eq!(
        linked_lines(
            READERS,
            &[
                "ValueTypeOf",
                "ValueIsArray",
                "ValueIsErrorClass",
                "ValueNullishEqual",
                "ValueStrictEqual"
            ]
        ),
        host
    );
}

/// One measured operation: a loop body over `i` (a Number), `u` (a
/// `number | string` union holding 2), `s` (a numeric string), `e` (a digit
/// string), `d` (a `string | number` union holding a digit string), `w` (one
/// holding a non-numeric string), `b` (a `bigint | number` union holding 5n)
/// and `v` (`unknown`, a small record), and whether it never costs more than the
/// v37 host call (`fast`): the inline fast paths, except `Number` of a union
/// cell holding a digit string, whose digit reader (about 22 fuel per digit)
/// costs a few fuel more than the uncharged host parse of a short string.
struct Operation {
    name: &'static str,
    body: &'static str,
    fast: bool,
}
const OPERATIONS: &[Operation] = &[
    Operation {
        name: "String(i)",
        body: "t = t + String(i).length;",
        fast: true,
    },
    Operation {
        name: "\"k\" + i",
        body: "t = t + (\"k\" + i).length;",
        fast: true,
    },
    Operation {
        name: "String(u)",
        body: "t = t + String(u).length;",
        fast: true,
    },
    Operation {
        name: "u < 3, u == 2, u === 2",
        body: "if (u < 3) { t++; } if (u == 2) { t++; } if (u === 2) { t++; }",
        fast: true,
    },
    Operation {
        name: "Number(u)",
        body: "t = t + Number(u);",
        fast: true,
    },
    Operation {
        name: "Number(e) (digit string)",
        body: "t = t + Number(e);",
        fast: true,
    },
    Operation {
        name: "Number(d) (union, digit string)",
        body: "t = t + Number(d);",
        fast: false,
    },
    Operation {
        name: "d == 1024 (digit string)",
        body: "if (d == 1024) { t++; }",
        fast: true,
    },
    Operation {
        name: "d < u (digit string)",
        body: "if (d < u) { t++; }",
        fast: true,
    },
    Operation {
        name: "b < 3, b == 5 (small BigInt)",
        body: "if (b < 3) { t++; } if (b == 5) { t++; }",
        fast: true,
    },
    Operation {
        name: "typeof v, v === null, Array.isArray(v), v instanceof Error",
        body: "if (typeof v === \"object\") { t++; } if (v === null) { t++; } if (Array.isArray(v)) { t++; } if (v instanceof Error) { t++; }",
        fast: true,
    },
    Operation {
        name: "String(i / 8) (linked text)",
        body: "t = t + String(i / 8).length;",
        fast: false,
    },
    Operation {
        name: "Number(s) (linked parse)",
        body: "t = t + Number(s);",
        fast: false,
    },
    Operation {
        name: "s < u (linked comparison)",
        body: "if (s < u) { t++; }",
        fast: false,
    },
    Operation {
        name: "w < 3 (linked comparison, NaN)",
        body: "if (w < 3) { t++; }",
        fast: false,
    },
    Operation {
        name: "Number(w) (linked parse, NaN)",
        body: "t = t + (Number(w) === 0 ? 1 : 0);",
        fast: false,
    },
];
fn operation_loop(body: &str, n: usize) -> String {
    format!(
        "export function main(): void {{\n  const u: number | string = Math.random() > 2 ? \"x\" : 2;\n  \
         const s: string = Math.random() > 2 ? \"x\" : \"12.5\";\n  const v: unknown = {{ a: 1 }};\n  \
         const d: string | number = Math.random() > 2 ? 1 : \"1024\";\n  \
         const e: string = Math.random() > 2 ? \"x\" : \"1024\";\n  \
         const w: string | number = Math.random() > 2 ? 1 : \"abc\";\n  \
         const b: bigint | number = Math.random() > 2 ? 1 : 5n;\n  \
         let t = 0;\n  for (let i = 0; i < {n}; i++) {{ {body} }}\n  console.log(t > 0 ? \"ok\" : \"none\");\n}}\n"
    )
}
/// Fuel per iteration of `body` under `profile` (the marginal cost
/// between 100 and 200 iterations).
fn per_iteration(body: &str, profile: P) -> f64 {
    let fuel = |n| execute(&compile(&operation_loop(body, n), profile), 10).fuel;
    (fuel(200) - fuel(100)) as f64 / 100.0
}

/// The fast paths never cost more fuel than the v37 host calls they replace
/// (which charge one host call and no fuel for their own work).
#[test]
fn fast_paths_never_cost_more_fuel_than_host_calls() {
    for operation in OPERATIONS.iter().filter(|operation| operation.fast) {
        let (host, linked) = (
            per_iteration(operation.body, V37),
            per_iteration(operation.body, V38),
        );
        assert!(
            linked <= host,
            "{}: v38 {linked} > v37 {host} fuel per iteration",
            operation.name
        );
    }
}

/// The per-iteration table of the development plan:
/// `cargo test -p varta-runtime --lib linked_value_measurements -- --ignored --nocapture`.
#[test]
#[ignore]
fn linked_value_measurements() {
    println!("| Operation | v37 fuel / iteration | v38 fuel / iteration |");
    for operation in OPERATIONS {
        println!(
            "| {} | {:.1} | {:.1} |",
            operation.name,
            per_iteration(operation.body, V37),
            per_iteration(operation.body, V38)
        );
    }
}

/// The mixed-kind regression of the development plan: every pair of twelve
/// mixed primitives (strings, BigInts, fractions) compared four ways and
/// converted, under v37 (host imports unrated and at the fair rates) and
/// v38 (linked): `cargo test -p varta-runtime --lib mixed_kind_conversion_fuel -- --ignored --nocapture`.
#[test]
#[ignore]
fn mixed_kind_conversion_fuel() {
    let source = r#"
export function main(): void {
  const xs: (string | number | bigint)[] = [0, 1.5, -2, 1e21, "3", "12.5", "abc", "", "007", 5n, -70000000000n, 2];
  let s = 0;
  let text = 0;
  for (const a of xs) {
    text = text + String(a).length;
    for (const b of xs) {
      if (a < b) { s = s + 1; }
      if (a <= b) { s = s + 2; }
      if (a == b) { s = s + 4; }
      if (a === b) { s = s + 8; }
    }
  }
  console.log(String(s) + " " + String(text));
}
"#;
    let run = |profile, rated| {
        let run = crate::codegen_v2_tests::run_budgets(&compile(source, profile), 20, rated);
        assert_eq!(run.error, None);
        run
    };
    let (v37, v37_rated, v38) = (run(V37, false), run(V37, true), run(V38, false));
    assert_eq!(v37.lines, v38.lines);
    println!(
        "{:?}: v37 {} (host work unrated), v37 at fair rates {}, v38 {}",
        v38.lines, v37.fuel, v37_rated.fuel, v38.fuel
    );
}

/// Conversions, comparisons and readers keep their results while
/// collections run between them: each iteration builds about 120 KiB of
/// garbage, far more than the managed heap holds.
#[test]
fn fast_paths_and_linked_fallbacks_survive_forced_collections() {
    let source = r#"
export function main(): void {
  const kept: string[] = [];
  const values: unknown[] = [1, "a", null, [1], { b: 2 }, 5n, new RangeError("r")];
  let total = 0;
  for (let i = 0; i < 60; i++) {
    const u: number | string | bigint = i % 3 === 0 ? i : i % 3 === 1 ? "s" + i : BigInt(i);
    const text = String(u) + "|" + String(i / 8) + "|" + ("n" + i) + "|" + String(Number("1" + i));
    let filler = text;
    for (let j = 0; j < 12; j++) { filler = filler + filler; }
    const v = values[i % values.length];
    if (typeof v === "object" && v !== null && !Array.isArray(v) && !(v instanceof Error)) { total = total + 1; }
    if (u < 30 || u == "s4") { total = total + filler.length % 7; }
    if (i % 10 === 0) { kept.push(text + typeof v + filler.length); }
  }
  for (const k of kept) { console.log(k); }
  console.log(String(total));
}
"#;
    let host = execute(&compile(source, V37), 1000).lines;
    assert_eq!(execute(&compile(source, V38), 1000).lines, host);
    assert_eq!(host.len(), 7);
}

/// `bytes` relabelled as a v38 module (language section, host ABI 34, value
/// layout 18) with each import named `from` renamed to `to`.
fn as_v38(bytes: &[u8], renames: &[(&str, &str)]) -> Vec<u8> {
    rewrite(bytes, |id, data| {
        let mut data = data.to_vec();
        let name = |data: &[u8], section: &str| {
            data.get(1..1 + section.len()) == Some(section.as_bytes())
                && data[0] as usize == section.len()
        };
        const CUSTOM_SECTION: u8 = 0;
        const IMPORT_SECTION: u8 = 2;
        if id == CUSTOM_SECTION && name(&data, varta_common::language::SECTION) {
            let prefix = 1 + varta_common::language::SECTION.len();
            let mut section = data[..prefix].to_vec();
            let contract = varta_common::language::LanguageContract {
                schema_version: varta_common::language::SCHEMA_VERSION,
                profile: V38,
            };
            section.extend_from_slice(&contract.to_bytes().unwrap());
            return vec![section];
        }
        for (section, version) in [
            (abi::SECTION, V38.host_abi()),
            (varta_common::value::VALUE_SECTION, V38.value_abi()),
        ] {
            if id == CUSTOM_SECTION && name(&data, section) {
                let prefix = 1 + section.len();
                let mut rewritten = data[..prefix].to_vec();
                rewritten.extend_from_slice(&version.to_le_bytes());
                return vec![rewritten];
            }
        }
        if id == IMPORT_SECTION {
            for (from, to) in renames {
                // Same-length names: the section layout is unchanged.
                assert_eq!(from.len(), to.len());
                let at = data
                    .windows(from.len())
                    .position(|window| window == from.as_bytes())
                    .unwrap_or_else(|| panic!("no import {from}"));
                data[at..at + to.len()].copy_from_slice(to.as_bytes());
            }
        }
        vec![data]
    })
}

/// A v38 module that imports one of these operations instead of linking it
/// is rejected before instantiation, operation by operation.
#[test]
fn v38_validation_rejects_imported_conversions_comparisons_and_readers() {
    let runtime = VartaRuntime::new();
    let union = "const u: number | string = Math.random() > 2 ? \"x\" : 2;";
    let unknown = "const v: unknown = Math.random() > 2 ? \"x\" : 2;";
    for (body, from, to) in [
        (
            format!("{union} console.log(String(u));"),
            "primitive_to_string_v5",
            "primitive_to_string_v6",
        ),
        (
            format!("{union} console.log(Number(u));"),
            "primitive_to_number_v5",
            "primitive_to_number_v6",
        ),
        (
            format!("{union} console.log(u < 3);"),
            "primitive_compare",
            "primitive_compare",
        ),
        (
            format!("{union} console.log(u === 2);"),
            "primitive_equal",
            "primitive_equal",
        ),
        (
            format!("{unknown} console.log(typeof v);"),
            "value_typeof_v10",
            "value_typeof_v11",
        ),
        (
            format!("{unknown} console.log(Array.isArray(v));"),
            "value_is_array_v10",
            "value_is_array_v11",
        ),
        (
            format!("{unknown} console.log(v === null);"),
            "value_strict_equal_v10",
            "value_strict_equal_v11",
        ),
        (
            format!("{unknown} console.log(v instanceof RangeError);"),
            "value_is_error_class_v6",
            "value_is_error_class_v7",
        ),
    ] {
        let source = format!("export function main(): void {{ {body} }}");
        let v37 = compile(&source, V37);
        let v38 = compile(&source, V38);
        assert!(imports(&v38).iter().all(|op| !op.linked()), "{source}");
        let error = runtime
            .validate_module(&as_v38(&v37, &[(from, to)]))
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains(&format!(
                "pure intrinsic must be linked into the module, not imported: {to}"
            )),
            "{source}: {error}"
        );
    }
}

/// Whether `operation` reads value layouts beyond primitive and BigInt cells
/// or converts primitive cells: the operations a contract-only host cannot
/// implement from `docs/embedding.md`.
fn layout_bound(operation: HostOperation) -> bool {
    operation.value17_counterpart().is_some()
        || matches!(
            operation,
            HostOperation::JsonStringifyOwned
                | HostOperation::PrimitiveCompare
                | HostOperation::PrimitiveEqual
        )
}

/// The generic v38 fixtures (the keyed view, the Markdown block parser, the
/// dictionary workload, the TSX application and a JSON round trip that logs
/// records, arrays, dictionaries and unions) import effects only: checked
/// JSON, serialization and the logged text of a value are linked, and
/// logging is `log_string16`.
#[test]
fn v38_fixtures_import_only_effects() {
    let dictionary = "export function main(): void { const d: Record<string, number> = {}; \
        for (let i = 0; i < 50; i++) { d[\"k\" + String(i)] = i; } let t = 0; \
        for (let i = 0; i < 50; i++) { const v = d[\"k\" + String(i)]; if (v !== undefined) { t = t + v; } } \
        console.log(String(t)); }";
    for (name, bytes) in [
        (
            "keyed view",
            compile(include_str!("codegen_v2_fixtures/keyed_view.ts"), V38),
        ),
        (
            "Markdown",
            compile(include_str!("codegen_v2_fixtures/markdown.ts"), V38),
        ),
        ("dictionary", compile(dictionary, V38)),
        (
            "JSON round trip and logged records",
            compile(include_str!("codegen_v2_fixtures/json_log.ts"), V38),
        ),
        (
            "TSX app",
            crate::view_tests::compile_tsx(
                crate::view_tests::APP,
                &[("signals.ts", crate::view_tests::SIGNALS)],
            ),
        ),
    ] {
        let imported = imports(&bytes);
        println!(
            "{name}: {}",
            imported
                .iter()
                .map(|operation| operation.name())
                .collect::<Vec<_>>()
                .join(", ")
        );
        for operation in &imported {
            assert!(!operation.linked(), "{name} imports {operation:?}");
            assert!(
                !layout_bound(*operation),
                "{name} imports the layout-bound {operation:?}"
            );
            assert!(
                !matches!(operation, HostOperation::ValueLogV12),
                "{name} logs through value_log_v12"
            );
        }
    }
}
