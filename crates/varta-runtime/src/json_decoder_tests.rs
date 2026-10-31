//! V38 compiled JSON decoders (`json_decoders`) against the path they
//! replace, the linked `json_parse_v19` alone (the compiler's
//! `json_decoder_test_hook::compile_generic`), and the host import: the same
//! values, `SyntaxError`/`TypeError` messages and typed limits on the case
//! corpus, a hand corpus, seeded random schemas, inputs and mutations, wide
//! unions, dictionaries, deep nesting and the limits; collections before
//! every decoder allocation; failed parses that leave nothing behind; and
//! the fuel the decoders cost.
use super::*;
use crate::codegen_v2_tests::{Run, compile};
use crate::linked_json_tests::{cause, host, run_rated, run_test_budgets};
use varta_common::language::LanguageProfile as P;
use varta_compiler::json_decoder_test_hook as hook;

const V38: P = P::CheckedF64Utf16V38;

/// A v38 module whose checked parses call the linked decoder alone.
fn generic(source: &str) -> Vec<u8> {
    let bytes = hook::compile_generic(source, V38).unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
/// A v38 module whose decoders collect before every block they allocate.
fn collecting(source: &str) -> Vec<u8> {
    let bytes = hook::compile_collecting(source, V38).unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
/// Whether a module defines more functions than its generic build (the
/// decoders).
fn defined_functions(bytes: &[u8]) -> u32 {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .filter_map(|payload| match payload.unwrap() {
            wasmparser::Payload::FunctionSection(reader) => Some(reader.count()),
            _ => None,
        })
        .sum()
}

/// The decoders' run and the generic run of `source`: identical lines and
/// failure cause. Returns (decoders, generic).
fn same(source: &str, budgets: u64) -> (Run, Run) {
    let decoded_bytes = compile(source, V38);
    let generic_bytes = generic(source);
    assert!(
        defined_functions(&decoded_bytes) > defined_functions(&generic_bytes),
        "no decoders compiled\n{source}"
    );
    let decoded = run_test_budgets(&decoded_bytes, budgets);
    let generic = run_test_budgets(&generic_bytes, budgets);
    assert_eq!(
        decoded.lines, generic.lines,
        "{source}\ndecoders {:?}\ngeneric {:?}",
        decoded.error, generic.error
    );
    assert_eq!(cause(&decoded.error), cause(&generic.error), "{source}");
    (decoded, generic)
}

/// Every runtime case program that parses checked JSON behaves identically
/// with the decoders, with the linked decoder alone and with the host import
/// (the data-limit cases, which parse megabytes, run in their own test).
#[test]
fn decoders_match_the_linked_decoder_on_the_case_corpus() {
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
        if !source.contains("JSON.parse")
            && !source.contains("json.parse")
            && !source.contains(".json")
        {
            continue;
        }
        let Ok(bytes) = varta_compiler::compile_typescript_with_profile(&source, V38) else {
            continue;
        };
        if http_contract::read(&bytes).unwrap().is_some()
            || defined_functions(&bytes) == defined_functions(&generic(&source))
        {
            continue;
        }
        let (decoded, _) = same(&source, 400);
        let reference = run_test_budgets(&host(&source), 400);
        assert_eq!(decoded.lines, reference.lines, "{}", path.display());
        assert_eq!(
            cause(&decoded.error),
            cause(&reference.error),
            "{}",
            path.display()
        );
        compared += 1;
    }
    assert!(compared >= 15, "only {compared} case programs compared");
}

/// The linked-JSON hand corpus and more: escapes, surrogates, numbers on
/// both conversion paths and near the overflow, whitespace, duplicates,
/// unknown and missing fields, keys the decoders leave to the fallback.
#[test]
fn decoders_match_the_linked_decoder_on_a_hand_corpus() {
    let (decoded, _) = same(crate::linked_json_tests::CORPUS, 100);
    assert_eq!(decoded.error, None);
    assert_eq!(decoded.lines.len(), 24);
    let texts = [
        r#"{"items":[{"id":1,"name":"a\"b\\c\/d\b\f\n\r\tAé😀","tags":["\u0000"]}],"meta":{},"any":"x"}"#,
        r#"{"items":[{"id":0.30000000000000004,"name":"","tags":[""]}],"meta":{"a":12345678901234567890},"any":-1.7976931348623157e308}"#,
        r#"{"items":[],"meta":{"a":1e308},"any":1.8e308}"#,
        r#"{"items":[],"meta":{"a":9.999999999999999e307},"any":2.5e-324}"#,
        r#"{"items":[],"meta":{"a":0e999999999},"any":-0.0e-999999}"#,
        r#"{"items":[],"meta":{"a":1E+2,"b":1e-2,"c":-12.5E0},"any":[0,-0,1.0,100000000000000000000000]}"#,
        r#"{"items":[],"meta":{"a":01},"any":0}"#,
        r#"{"items":[],"meta":{"a":1.},"any":0}"#,
        r#"{"items":[],"meta":{"a":.5},"any":0}"#,
        r#"{"items":[],"meta":{"a":-},"any":0}"#,
        r#"{"items":[],"meta":{"a":1e},"any":0}"#,
        r#"{"items":[],"meta":{"a":+1},"any":0}"#,
        r#"{"items":[],"meta":{"a":1},"any":0}"#,
        r#"{"items":[],"meta":{"1":1,"0":2,"b":3},"any":{"10":1,"9":2,"x":3}}"#,
        r#"{"items":[],"meta":{},"any":{"a":1,"a":2}}"#,
        r#"{"items":[],"meta":{},"any":0,"items":[]}"#,
        r#"{"items":[],"meta":{},"any":0}"#,
        r#"{"items":[{"id":1,"name":"a","tags":[],"note":null}],"meta":{},"any":0}"#,
        r#"{"items":[{"id":1,"name":"a","tags":[],"score":true}],"meta":{},"any":0}"#,
        "{\"items\":[],\"meta\":{},\"any\":\"\u{1}\"}",
        " \t\r\n{ \"items\" : [ { \"id\" : 1 , \"name\" : \"a\" , \"tags\" : [ \"x\" , \"y\" ] } ] , \"meta\" : { \"k\" : 2 } , \"any\" : [ true , false , null ] } \n",
        r#"{"items":[],"meta":{},"any":tru}"#,
        r#"{"items":[],"meta":{},"any":nulll}"#,
        r#"{"items":[],"meta":{},"any":[1,2]]"#,
        r#"{"items":[],"meta":{},"any":"\x"}"#,
        r#"{"items":[],"meta":{},"any":"\u12g4"}"#,
        r#"{"items":[],"meta":{},"any":"unterminated}"#,
        r#"{"items":[],"meta":{},"any":0}  x"#,
        r#"{"items":[],"meta":{},"any":{"":1,"":2}}"#,
        r#"{"items":[],"meta":{"":0},"any":{"":[{"":{}}]}}"#,
    ];
    let program = corpus_program(
        "interface Item { id: number; name: string; tags: string[]; note?: string; score?: number | null }\n\
         interface Box { items: Item[]; meta: Record<string, number>; any: unknown }",
        "Box",
        &texts.map(str::to_owned),
    );
    let (decoded, _) = same(&program, 100);
    assert_eq!(decoded.error, None);
    assert_eq!(decoded.lines.len(), texts.len());
    let reference = run_test_budgets(&host(&program), 100);
    assert_eq!(decoded.lines, reference.lines);
}

/// A program printing, for each of `texts` (as JS string literals), the
/// stringified `JSON.parse(text) as target` and the stringified untyped
/// parse, or their error classes and messages.
fn corpus_program(declarations: &str, target: &str, texts: &[String]) -> String {
    let literals: Vec<String> = texts
        .iter()
        .map(|text| serde_json::to_string(text).unwrap())
        .collect();
    format!(
        r#"{declarations}
function show(text: string): string {{
  try {{
    return JSON.stringify(JSON.parse(text) as {target});
  }} catch (e) {{
    if (e instanceof SyntaxError) return "SyntaxError " + e.message;
    if (e instanceof TypeError) return "TypeError " + e.message;
    return "other";
  }}
}}
function raw(text: string): string {{
  try {{
    return JSON.stringify(JSON.parse(text));
  }} catch (e) {{
    return e instanceof SyntaxError ? "SyntaxError" : e instanceof TypeError ? "TypeError" : "other";
  }}
}}
export function main(): void {{
  const texts: string[] = [{}];
  for (const text of texts) console.log(show(text) + " | " + raw(text));
}}
"#,
        literals.join(",\n    ")
    )
}

/// A small deterministic generator (xorshift64*).
struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// A random checked target type: its TypeScript spelling, the interfaces it
/// declares and a value generator.
#[derive(Clone)]
enum Shape {
    String,
    Number,
    Boolean,
    Null,
    Unknown,
    Array(Box<Shape>),
    Dictionary(Box<Shape>),
    Nullable(Box<Shape>),
    Scalars,
    Record(usize, Vec<(String, bool, Shape)>),
}
const NAMES: [&str; 12] = [
    "id",
    "name",
    "type",
    "seq",
    "a",
    "b",
    "value",
    "items",
    "é",
    "with space",
    "x1",
    "longer_field_name",
];
fn shape(random: &mut Random, depth: usize, records: &mut usize) -> Shape {
    let leaf = depth == 0 || random.below(3) == 0;
    if leaf {
        return match random.below(6) {
            0 => Shape::String,
            1 => Shape::Number,
            2 => Shape::Boolean,
            3 => Shape::Null,
            4 => Shape::Scalars,
            _ => Shape::Unknown,
        };
    }
    match random.below(5) {
        0 => Shape::Array(Box::new(shape(random, depth - 1, records))),
        1 => Shape::Dictionary(Box::new(shape(random, depth - 1, records))),
        2 => Shape::Nullable(Box::new(record(random, depth - 1, records))),
        _ => record(random, depth - 1, records),
    }
}
fn record(random: &mut Random, depth: usize, records: &mut usize) -> Shape {
    let id = *records;
    *records += 1;
    let count = random.below(6);
    let mut fields = Vec::new();
    for name in NAMES {
        if fields.len() == count {
            break;
        }
        if random.below(2) == 0 {
            fields.push((
                name.to_owned(),
                random.below(3) == 0,
                shape(random, depth, records),
            ));
        }
    }
    Shape::Record(id, fields)
}
impl Shape {
    fn spell(&self, declarations: &mut Vec<String>) -> String {
        match self {
            Self::String => "string".into(),
            Self::Number => "number".into(),
            Self::Boolean => "boolean".into(),
            Self::Null => "null".into(),
            Self::Unknown => "unknown".into(),
            Self::Scalars => "string | number | boolean | null".into(),
            Self::Array(element) => format!("({})[]", element.spell(declarations)),
            Self::Dictionary(value) => format!("Record<string, {}>", value.spell(declarations)),
            Self::Nullable(inner) => format!("{} | null", inner.spell(declarations)),
            Self::Record(id, fields) => {
                let name = format!("R{id}");
                if !declarations
                    .iter()
                    .any(|d| d.starts_with(&format!("interface {name} ")))
                {
                    let body: Vec<String> = fields
                        .iter()
                        .map(|(field, optional, ty)| {
                            format!(
                                "{}{}: {}",
                                serde_json::to_string(field).unwrap(),
                                if *optional { "?" } else { "" },
                                ty.spell(declarations)
                            )
                        })
                        .collect();
                    declarations.push(format!("interface {name} {{ {} }}", body.join("; ")));
                }
                name
            }
        }
    }
    /// A JSON value of this shape, sometimes with whitespace.
    fn value(&self, random: &mut Random, depth: usize, out: &mut String) {
        let space = |random: &mut Random, out: &mut String| {
            if random.below(8) == 0 {
                out.push_str(random.pick(&[" ", "\n", "\t ", "\r\n"]));
            }
        };
        space(random, out);
        match self {
            Self::String => string(random, out),
            Self::Number => out.push_str(random.pick(&[
                "0",
                "-0",
                "1",
                "-17",
                "3.5",
                "1e3",
                "2.5E-3",
                "0.1",
                "123456789012345678",
                "0.30000000000000004",
                "1e-320",
                "6.02214076e23",
                "-9007199254740993",
            ])),
            Self::Boolean => out.push_str(random.pick(&["true", "false"])),
            Self::Null => out.push_str("null"),
            Self::Scalars => match random.below(4) {
                0 => string(random, out),
                1 => out.push_str("42.25"),
                2 => out.push_str("false"),
                _ => out.push_str("null"),
            },
            Self::Unknown => {
                let options = if depth > 3 { 4 } else { 6 };
                match random.below(options) {
                    0 => out.push_str("null"),
                    1 => out.push_str("-12.5e1"),
                    2 => string(random, out),
                    3 => out.push_str("true"),
                    4 => {
                        out.push('[');
                        for index in 0..random.below(4) {
                            if index > 0 {
                                out.push(',');
                            }
                            Self::Unknown.value(random, depth + 1, out);
                        }
                        out.push(']');
                    }
                    _ => {
                        out.push('{');
                        for index in 0..random.below(4) {
                            if index > 0 {
                                out.push(',');
                            }
                            out.push_str(
                                &serde_json::to_string(NAMES[index * 3 % NAMES.len()]).unwrap(),
                            );
                            out.push(':');
                            Self::Unknown.value(random, depth + 1, out);
                        }
                        out.push('}');
                    }
                }
            }
            Self::Array(element) => {
                out.push('[');
                for index in 0..random.below(5) {
                    if index > 0 {
                        out.push(',');
                        space(random, out);
                    }
                    element.value(random, depth + 1, out);
                }
                out.push(']');
            }
            Self::Dictionary(value) => {
                out.push('{');
                let count = random.below(5);
                for index in 0..count {
                    if index > 0 {
                        out.push(',');
                    }
                    let key = random.pick(&["k", "key", "a b", "ü", "zz", "", "k2", "7", "x\\n"]);
                    out.push('"');
                    out.push_str(key);
                    out.push_str("\":");
                    value.value(random, depth + 1, out);
                }
                out.push('}');
            }
            Self::Nullable(inner) => {
                if random.below(3) == 0 {
                    out.push_str("null");
                } else {
                    inner.value(random, depth, out);
                }
            }
            Self::Record(_, fields) => {
                let mut order: Vec<usize> = (0..fields.len()).collect();
                for index in (1..order.len()).rev() {
                    order.swap(index, random.below(index + 1));
                }
                out.push('{');
                let mut first = true;
                for index in order {
                    let (name, optional, ty) = &fields[index];
                    if *optional && random.below(2) == 0 {
                        continue;
                    }
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    space(random, out);
                    out.push_str(&serde_json::to_string(name).unwrap());
                    space(random, out);
                    out.push(':');
                    ty.value(random, depth + 1, out);
                }
                out.push('}');
            }
        }
        space(random, out);
    }
}
fn string(random: &mut Random, out: &mut String) {
    out.push_str(random.pick(&[
        r#""""#,
        r#""plain""#,
        r#""é😀""#,
        r#""a\"b""#,
        r#""é\n\t""#,
        r#""\ud800""#,
        r#""\\\/""#,
        r#""longer text with several words in it""#,
    ]));
}
/// Mutate a text: drop, duplicate or replace a unit, or splice a fragment.
fn mutate(random: &mut Random, text: &str) -> String {
    let mut units: Vec<char> = text.chars().collect();
    if units.is_empty() {
        return "x".into();
    }
    let at = random.below(units.len());
    match random.below(5) {
        0 => {
            units.remove(at);
        }
        1 => units.insert(at, units[at]),
        2 => units[at] = *random.pick(&['"', ',', '}', ']', '1', 'x', ' ', '\\', ':']),
        3 => {
            let fragment = random.pick(&[",\"extra\":1", "null", "[]", "{}", "\"s\"", "-"]);
            for (offset, c) in fragment.chars().enumerate() {
                units.insert(at + offset, c);
            }
        }
        _ => units.truncate(at),
    }
    units.into_iter().collect()
}

/// Seeded random schemas, each with valid inputs and mutations of them:
/// the decoders agree with the linked decoder on every outcome.
#[test]
fn decoders_match_the_linked_decoder_on_random_schemas_and_inputs() {
    let mut random = Random(0x5eed_1234_abcd_0042);
    let mut accepted = 0;
    let mut rejected = 0;
    for _ in 0..24 {
        let mut records = 0;
        let target = shape(&mut random, 3, &mut records);
        let mut declarations = Vec::new();
        let spelled = target.spell(&mut declarations);
        let mut texts = Vec::new();
        for _ in 0..10 {
            let mut text = String::new();
            target.value(&mut random, 0, &mut text);
            texts.push(mutate(&mut random, &text));
            texts.push(text);
        }
        let program = corpus_program(&declarations.join("\n"), &spelled, &texts);
        let (decoded, _) = same(&program, 200);
        assert_eq!(decoded.error, None, "{program}");
        accepted += decoded
            .lines
            .iter()
            .filter(|l| !l.contains("Error "))
            .count();
        rejected += decoded
            .lines
            .iter()
            .filter(|l| l.contains("Error "))
            .count();
    }
    assert!(
        accepted > 100 && rejected > 50,
        "{accepted} accepted, {rejected} rejected"
    );
}

/// Wide discriminated unions (through `unknown` and a checked conversion),
/// nesting beyond the decoders' own depth and to the v38 limit, large
/// documents and dictionaries beyond the decoders' entries.
#[test]
fn decoders_match_the_linked_decoder_on_unions_depth_dictionaries_and_size() {
    same(include_str!("strict_member_cases/events.ts"), 400);
    for levels in [100, 200, 250] {
        same(
            &format!(
                "interface Node {{ c: Node[] }} export function main(): void {{
                   const t = '{{\"c\":['.repeat({levels}) + ']}}'.repeat({levels});
                   const v = varta.json.parse<Node>(t);
                   let d = 0; let n: Node = v;
                   while (n.c.length > 0) {{ n = n.c[0]; d++; }}
                   console.log(String(d) + ' ' + String(JSON.stringify(v).length === t.length));
                   const u = JSON.parse('['.repeat({levels}) + ']'.repeat({levels}));
                   console.log(JSON.stringify(u).length);
                 }}"
            ),
            400,
        );
    }
    let levels = varta_common::limits::V38_MAX_JSON_DEPTH.div_ceil(2) + 1;
    let (decoded, _) = same(
        &format!(
            "interface Node{{c:Node[]}}export function main():void{{\
             const t='{{\"c\":['.repeat({levels})+']}}'.repeat({levels});\
             console.log(String(varta.json.parse<Node>(t).c.length));}}"
        ),
        400,
    );
    assert_eq!(
        cause(&decoded.error).as_deref(),
        Some("checked JSON depth limit")
    );
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
    // Dictionaries at, beyond and far beyond the decoders' own entries.
    for entries in [63, 64, 65, 300] {
        same(
            &format!(
                "export function main(): void {{
                   let t = '{{';
                   for (let i = 0; i < {entries}; i++) {{ if (i > 0) t += ','; t += '\"k' + String(i) + '\":' + String(i); }}
                   t += '}}';
                   const d = varta.json.parse<Record<string, number>>(t);
                   const u = JSON.parse(t) as Record<string, unknown>;
                   console.log(String(Object.keys(d).length) + ' ' + JSON.stringify(u).length + ' ' + String(d['k' + String({entries} - 1)]));
                   try {{ varta.json.parse<Record<string, number>>(t.slice(0, t.length - 1) + ',\"k0\":1}}'); }} catch (e) {{ console.log(e instanceof TypeError ? 'TypeError' : 'other'); }}
                 }}"
            ),
            400,
        );
    }
    // A 40-field record (wide) and a 70-field record (beyond one mask).
    for width in [40, 70] {
        let fields: Vec<String> = (0..width).map(|i| format!("f{i}: number")).collect();
        let members: Vec<String> = (0..width).map(|i| format!("\\\"f{i}\\\":{i}")).collect();
        same(
            &format!(
                "interface W {{ {} }} export function main(): void {{
                   const w = JSON.parse(\"{{{}}}\") as W;
                   console.log(JSON.stringify(w));
                 }}",
                fields.join("; "),
                members.join(",")
            ),
            400,
        );
    }
}

/// The typed input limits and the data-limit cases decode identically.
#[test]
fn decoders_match_the_linked_decoder_on_the_limit_cases() {
    for source in [
        include_str!("json_limit_cases/depth.ts"),
        include_str!("json_limit_cases/dictionary.ts"),
        include_str!("json_limit_cases/large_string.ts"),
        include_str!("json_limit_cases/round_trip.ts"),
    ] {
        same(source, 400);
    }
}

/// A full collection before every block the decoders allocate (the tape,
/// each container, string and cell): results kept across many parses amid
/// garbage stay intact, and failed parses leave nothing behind.
#[test]
fn decoders_survive_a_collection_before_every_allocation() {
    let source = r#"
interface Item { id: number; name: string; tags: string[]; note?: string; extra?: number | null; any?: unknown }
export function main(): void {
  const kept: Item[][] = [];
  const dictionaries: Record<string, Item>[] = [];
  let failures = 0;
  for (let round = 0; round < 6; round++) {
    let garbage = "g";
    for (let j = 0; j < 8; j++) { garbage = garbage + garbage; }
    let text = "[";
    for (let i = 0; i < 6; i++) {
      if (i > 0) text += ",";
      text += '{"id":' + String(round * 100 + i) + ',"name":"n\\u00e9' + String(i) + garbage.charAt(i) + '","tags":["a","b' + String(round) + '"],"any":{"k":[1,"two",null,{"x":0.30000000000000004}]}}';
    }
    text += "]";
    kept.push(JSON.parse(text) as Item[]);
    dictionaries.push(JSON.parse('{"a":' + text.slice(1, text.indexOf("}}") + 2) + '}') as Record<string, Item>);
    try { JSON.parse(text.slice(0, text.length - 2)) as Item[]; } catch (e) { failures++; }
    try { JSON.parse(text.replace('"id":', '"id":"')) as Item[]; } catch (e) { failures++; }
  }
  let sum = 0; let names = "";
  for (const items of kept) { for (const item of items) { sum += item.id; } names += items[5].name + items[3].tags[1] + JSON.stringify(items[2].any); }
  for (const d of dictionaries) { const e = d["a"]; if (e !== undefined) names += e.name; }
  console.log(String(sum) + " " + String(failures) + " " + String(names.length) + " " + names.slice(0, 60));
}
"#;
    let collected = run_test_budgets(&collecting(source), 4000);
    let reference = run_test_budgets(&generic(source), 400);
    assert_eq!(collected.error, None);
    assert_eq!(collected.lines, reference.lines);
    let (decoded, _) = same(source, 400);
    assert_eq!(decoded.lines, reference.lines);
}

/// A rejected input never builds: a document that fails at its very end
/// costs the decoders' scan and the linked decoder's, and allocates no
/// value (the heap after many failed parses is what it was after one).
#[test]
fn failed_parses_build_nothing() {
    let source = |rounds: usize| {
        format!(
            r#"interface Item {{ id: number; name: string }}
export function main(): void {{
  let text = "[";
  for (let i = 0; i < 200; i++) {{ if (i > 0) text += ","; text += '{{"id":' + String(i) + ',"name":"item' + String(i) + '"}}'; }}
  const bad = text + ",]";
  let failures = 0;
  for (let round = 0; round < {rounds}; round++) {{
    try {{ JSON.parse(bad) as Item[]; }} catch (e) {{ if (e instanceof SyntaxError) failures++; }}
  }}
  console.log(String(failures));
}}"#
        )
    };
    let (one, _) = same(&source(1), 400);
    let (many, _) = same(&source(20), 400);
    assert_eq!(one.lines, ["varta log: 1"]);
    assert_eq!(many.lines, ["varta log: 20"]);
}

/// Fuel of the measured JSON workloads (`--nocapture`): the decoders, the
/// linked decoder alone, and v37 with its host work at the fair rates.
#[test]
#[ignore]
fn json_decoder_fuel_measurement() {
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
        let fuel = |bytes: &[u8], rated| run_rated(bytes, 400, rated).fuel;
        let delta = |build: &dyn Fn(&str) -> Vec<u8>, rated| {
            fuel(&build(&program(true)), rated) - fuel(&build(&program(false)), rated)
        };
        let v37 = delta(&|s| compile(s, P::CheckedF64Utf16V37), Some(true));
        let linked = delta(&generic, None);
        let decoded = delta(&|s| compile(s, V38), None);
        println!(
            "| {label} | {} | {v37} | {linked} | {decoded} | {:.1}x |",
            text.len(),
            linked as f64 / decoded as f64
        );
    };
    println!(
        "| workload | text units | v37 at fair rates | v38 linked decoder | v38 decoders | speed-up |"
    );
    let prelude = "interface Item { id: number; name: string; tags: string[] }";
    measure("1 record", prelude, &items(1), "Item[]");
    measure("40 records (~2 KB)", prelude, &items(40), "Item[]");
    let numbers = vec!["1.5"; 1000].join(",");
    measure("1000 numbers", "", &format!("[{numbers}]"), "number[]");
    let event = r#"{\"seq\":9,\"detail\":{\"code\":165,\"reason\":\"r15\"},\"type\":\"metric\"}"#;
    measure("RunEventV1-like member (unknown)", "", event, "unknown");
    let long = vec!["0.30000000000000004"; 100].join(",");
    measure("100 17-digit numbers", "", &format!("[{long}]"), "number[]");
}

/// The decoders cost at least five times less fuel than the linked decoder
/// on small schema-heavy documents, and less on every measured shape.
#[test]
fn decoders_cost_less_fuel_than_the_linked_decoder() {
    let cases = [
        (
            "interface Item { id: number; name: string; tags: string[] }",
            r#"[{\"id\":1,\"name\":\"item1\",\"tags\":[\"a\",\"b\"]}]"#,
            "Item[]",
            5.0,
        ),
        (
            "",
            r#"{\"seq\":9,\"detail\":{\"code\":165,\"reason\":\"r15\"},\"type\":\"metric\"}"#,
            "unknown",
            5.0,
        ),
    ];
    for (prelude, text, ty, ratio) in cases {
        let program = |parse: bool| {
            format!(
                "{prelude}\nexport function main(): void {{ const t = \"{text}\"; {} }}",
                if parse {
                    format!("const v = JSON.parse(t) as {ty}; console.log(typeof v);")
                } else {
                    "console.log(\"object\");".to_owned()
                }
            )
        };
        let delta = |build: &dyn Fn(&str) -> Vec<u8>| {
            run_test_budgets(&build(&program(true)), 40).fuel
                - run_test_budgets(&build(&program(false)), 40).fuel
        };
        let linked = delta(&generic);
        let decoded = delta(&|s| compile(s, V38));
        assert!(
            linked as f64 >= ratio * decoded as f64,
            "{ty}: linked {linked}, decoders {decoded}"
        );
    }
}
