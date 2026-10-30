//! The shared UTF-16 decoder against the reference classification
//! (`json_input_limited` followed by the `serde_json` classifier): identical
//! trees and identical outcome classes on a hand-written corpus and on
//! seeded random inputs and mutations under small limits.
use super::*;
use crate::{limits::DataLimits, value::HASHED_DICTIONARY_VALUE_VERSION};

/// The reference outcome of the host path before the shared decoder.
fn reference(schema: &JsonSchema, units: &[u16], data: DataLimits) -> Result<JsonValue, JsonDecodeError> {
    let bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    if bytes.len() > data.json_input_bytes {
        return Err(JsonDecodeError::Limit(JsonInputLimit::Bytes));
    }
    match crate::text::json_input_limited(&bytes, data.json_input_bytes) {
        Ok(encoded) => schema.parse_json_classified_limited(&encoded, data),
        Err(crate::text::JsonTextError::Syntax) => Err(JsonDecodeError::Syntax { line: 1, column: 1 }),
        Err(crate::text::JsonTextError::Limit) => Err(JsonDecodeError::Limit(JsonInputLimit::Bytes)),
        Err(crate::text::JsonTextError::Encoding) => unreachable!("even input"),
    }
}
fn shared(schema: &JsonSchema, units: &[u16], data: DataLimits) -> Result<JsonValue, JsonDecodeError> {
    let bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    if bytes.len() > data.json_input_bytes {
        return Err(JsonDecodeError::Limit(JsonInputLimit::Bytes));
    }
    let mut tape = Tape::default();
    scan(
        &bytes,
        schema,
        JsonLimits::of(data, schema.value_version()),
        &mut tape,
        &mut Unbounded,
    )?;
    Ok(tape.value(&bytes, schema))
}
/// The outcome class: the tree, `Syntax`, `Type` or the limit.
fn class(outcome: &Result<JsonValue, JsonDecodeError>) -> String {
    match outcome {
        Ok(value) => format!("{value:?}"),
        Err(JsonDecodeError::Syntax { .. }) => "Syntax".into(),
        Err(JsonDecodeError::Type) => "Type".into(),
        Err(JsonDecodeError::Limit(limit)) => format!("Limit({limit:?})"),
        Err(JsonDecodeError::Schema(error)) => format!("Schema({error})"),
    }
}
fn agree(schema: &JsonSchema, units: &[u16], data: DataLimits) -> String {
    let expected = class(&reference(schema, units, data));
    let actual = class(&shared(schema, units, data));
    assert_eq!(
        actual,
        expected,
        "input {:?}",
        String::from_utf16_lossy(units)
    );
    expected
}
fn agree_text(schema: &JsonSchema, text: &str, data: DataLimits) -> String {
    agree(schema, &text.encode_utf16().collect::<Vec<_>>(), data)
}

fn version() -> u32 {
    HASHED_DICTIONARY_SCHEMA_VERSION
}
fn field(name: &str, ty: u32, optional: bool) -> JsonField {
    JsonField {
        name: name.into(),
        ty,
        optional,
    }
}
/// The JSON value graph of `unknown` (recursive): null, undefined, boolean,
/// number, string, `unknown[]` and `Record<string, unknown>`.
fn unknown() -> JsonSchema {
    JsonSchema {
        version: version(),
        root: 0,
        types: vec![
            JsonType::ReferenceUnion {
                members: vec![1, 2, 3, 4, 5, 6, 7],
            },
            JsonType::Null,
            JsonType::Undefined,
            JsonType::Boolean,
            JsonType::Float64,
            JsonType::String,
            JsonType::Array { element: 0 },
            JsonType::Dictionary {
                descriptor: 0,
                value: 0,
            },
        ],
    }
}
/// A record with every field kind, optional fields and nested shapes.
fn records() -> JsonSchema {
    JsonSchema {
        version: version(),
        root: 0,
        types: vec![
            JsonType::Record {
                descriptor: 0,
                fields: vec![
                    field("a", 1, false),
                    field("b", 2, true),
                    field("c", 5, false),
                    field("d", 14, true),
                    field("e\u{e9}", 15, true),
                    field("f", 9, true),
                    field("\u{1f600}", 2, true),
                ],
            },
            JsonType::Float64,
            JsonType::PrimitiveUnion {
                members: vec![3, 4, 7],
            },
            JsonType::Undefined,
            JsonType::String,
            JsonType::Array { element: 10 },
            JsonType::Dictionary {
                descriptor: 0,
                value: 11,
            },
            JsonType::Boolean,
            JsonType::Float64,
            JsonType::ReferenceUnion {
                members: vec![3, 12, 13],
            },
            JsonType::PrimitiveUnion {
                members: vec![12, 1, 4],
            },
            JsonType::Array { element: 1 },
            JsonType::Null,
            JsonType::Record {
                descriptor: 0,
                fields: vec![field("x", 1, false), field("y", 3, true)],
            },
            JsonType::ReferenceUnion {
                members: vec![3, 6],
            },
            JsonType::PrimitiveUnion {
                members: vec![3, 8],
            },
        ],
    }
}
fn limits() -> DataLimits {
    DataLimits {
        json_depth: 4,
        json_nodes: 40,
        string_bytes: 16,
        array_elements: 6,
        json_input_bytes: 400,
        ..DataLimits::V38
    }
}

/// Limits where the visited values run out first.
fn counted() -> DataLimits {
    DataLimits {
        json_nodes: 9,
        json_depth: 100,
        array_elements: 100,
        string_bytes: 1000,
        json_input_bytes: 4000,
        ..DataLimits::V38
    }
}

#[test]
fn hand_written_corpus_agrees() {
    for schema in [unknown(), records()] {
        schema.validate_within(&crate::limits::WASM).unwrap();
    }
    let unknown = unknown();
    let records = records();
    let wide = DataLimits::V38;
    let small = limits();
    let corpus = [
        "", " ", "null", " null ", "nul", "nulll", "true", "false", "tru", "1", "-1", "-0", "0",
        "01", "-01", "1.", "1.5", ".5", "-", "+1", "1e5", "1E+5", "1e-5", "1e", "1e+", "1.5e3",
        "1e400", "-1e400", "1e-400", "123456789012345678901234567890", "9007199254740993",
        "0.1", "-0.0", "\"\"", "\"a\"", "\"a", "\"\\u0041\"", "\"\\u00\"", "\"\\x\"",
        "\"\\ud800\"", "\"\\udc00\\ud800\"", "\"\\ud83d\\ude00\"", "\"\u{1}\"", "\"\t\"",
        "\"\u{7f}\"", "[]", "[ ]", "[1,]", "[,1]", "[1 2]", "[1,2,3]", "[[[]]]", "[[[[[]]]]]",
        "[[[[[1]]]]]", "{}", "{ }", "{\"a\":1}", "{\"a\":1,}", "{\"a\" 1}", "{a:1}",
        "{\"a\":1,\"a\":2}", "{\"1\":1,\"0\":2,\"b\":3,\"a\":4}", "{\"10\":1,\"9\":2,\"01\":3}",
        "{\"\\ud800\":1}", "[1,2,3,4,5,6,7]", "[1,2,3,4,5,6]", "\"123456789\"", "\"12345678\"",
        "{\"123456789\":1}", "[true, false, null, \"x\", 1.5, [], {}]", "1 2", "[] []", "{}}",
        "[\"a\", \"b\"", "{\"a\":{\"b\":{\"c\":{\"d\":{}}}}}", "{\"a\":{\"b\":{\"c\":{\"d\":[]}}}}",
        "{\"a\":{\"b\":{\"c\":{\"d\":[1]}}}}", "[1,2,3,4,5,6,7,x]", "[1, \"\\q\"]",
        "{\"a\":1, \"b\":2, \"c\":3}", "{\"a\":[1,2,3,4,5,6,7], \"b\":}",
    ];
    for text in corpus {
        agree_text(&unknown, text, wide);
        agree_text(&unknown, text, small);
    }
    let record_corpus = [
        r#"{"a":1,"c":[]}"#,
        r#"{"c":[],"a":1}"#,
        r#"{"a":1}"#,
        r#"{"c":[]}"#,
        r#"{"a":1,"c":[],"z":1}"#,
        r#"{"a":1,"c":[],"a":1}"#,
        r#"{"a":"1","c":[]}"#,
        r#"{"a":1,"c":[null,1,"x"]}"#,
        r#"{"a":1,"c":[null,1,true]}"#,
        r#"{"a":1,"b":"s","c":[]}"#,
        r#"{"a":1,"b":true,"c":[]}"#,
        r#"{"a":1,"b":null,"c":[]}"#,
        r#"{"a":1,"b":1,"c":[]}"#,
        r#"{"a":1,"c":[],"d":{"k":[1,2]}}"#,
        r#"{"a":1,"c":[],"d":{"k":[1,2],"k":[]}}"#,
        r#"{"a":1,"c":[],"d":{"2":[],"1":[1],"x":[]}}"#,
        r#"{"a":1,"c":[],"d":{"a":[],"b":[],"c":[],"d":[]}}"#,
        r#"{"a":1,"c":[],"é":5}"#,
        r#"{"a":1,"c":[],"\u00e9":5}"#,
        r#"{"a":1,"c":[],"é":5.5}"#,
        r#"{"a":1,"c":[],"é":-0}"#,
        r#"{"a":1,"c":[],"é":2147483648}"#,
        r#"{"a":1,"c":[],"é":-2147483648}"#,
        r#"{"a":1,"c":[],"é":1e2}"#,
        r#"{"a":1,"c":[],"f":{"x":1}}"#,
        r#"{"a":1,"c":[],"f":{"x":1,"y":true}}"#,
        r#"{"a":1,"c":[],"f":{"y":true}}"#,
        r#"{"a":1,"c":[],"f":null}"#,
        r#"{"a":1,"c":[],"f":[]}"#,
        r#"{"a":1,"c":[],"😀":true}"#,
        r#"{"a":1,"c":[],"\ud83d\ude00":true}"#,
        r#"{"a":1,"c":[],"\ud83d":true}"#,
        r#"{"a":1,"c":[],"\ud83d😀":true}"#,
        r#"{"a":1,"c":[1,2,3,4,5,6,7],"z":1}"#,
        r#"{"z":1,"c":[1,2,3,4,5,6,7]}"#,
        r#"{"z":1,"c":[1,2,3,4,5,6,7],"a":"#,
        r#"{"a":1e400,"c":[1,2,3,4,5,6,7]}"#,
        r#"{"a":1,"c":[],"d":{"a":[],"b":[],"c":[]},"q":1}"#,
        r#"{"a":1,"c":[],"d":{"a":[],"b":[],"c":[]}"#,
    ];
    for text in record_corpus {
        agree_text(&records, text, wide);
        agree_text(&records, text, small);
    }
}

/// Raw lone surrogates, which the reference transcodes to escapes inside
/// strings and rejects elsewhere.
#[test]
fn raw_surrogates_agree() {
    let unknown = unknown();
    let records = records();
    let q = u16::from(b'"');
    let cases: Vec<Vec<u16>> = vec![
        vec![q, 0xd800, q],
        vec![q, 0xdc00, q],
        vec![q, 0xd83d, 0xde00, q],
        vec![0xd800],
        vec![u16::from(b'['), 0xd800, u16::from(b']')],
        vec![q, u16::from(b'\\'), 0xd800, q],
        vec![q, 0xd800, u16::from(b'\\'), u16::from(b'u'), u16::from(b'D'), u16::from(b'C'), u16::from(b'0'), u16::from(b'0'), q],
        "{\"".encode_utf16().chain([0xd83d]).chain("\":1}".encode_utf16()).collect(),
        "{\"a\":1,\"c\":[],\"".encode_utf16().chain([0xd83d, 0xde00]).chain("\":true}".encode_utf16()).collect(),
        "{\"a\":1,\"c\":[],\"".encode_utf16().chain([0xd83d]).chain("\\ude00\":true}".encode_utf16()).collect(),
        "{\"a\":1,\"c\":[],\"\\ud83d".encode_utf16().chain([0xde00]).chain("\":true}".encode_utf16()).collect(),
        "{\"a\":1,\"c\":[],\"".encode_utf16().chain([0xde00]).chain("\":true}".encode_utf16()).collect(),
    ];
    for units in &cases {
        for data in [DataLimits::V38, limits()] {
            agree(&unknown, units, data);
            agree(&records, units, data);
        }
    }
}

/// The transcoded byte limit precedes everything, in transcoding order.
#[test]
fn transcoded_byte_limit_agrees() {
    let unknown = unknown();
    let data = DataLimits {
        json_input_bytes: 24,
        ..DataLimits::V38
    };
    for text in [
        "[\"\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\"]",
        "[\"\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\"]",
        "[1,2,3,x,\"\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\"]",
    ] {
        agree_text(&unknown, text, data);
    }
    let q = u16::from(b'"');
    let mut early = vec![0xd800];
    early.extend("[\"\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\"]".encode_utf16());
    agree(&unknown, &early, data);
    let mut late: Vec<u16> = "[\"\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}\"".encode_utf16().collect();
    late.extend([0xd800, u16::from(b']')]);
    agree(&unknown, &late, data);
    agree(&unknown, &[q, 0xd800, 0xd800, 0xd800, 0xd800, 0xd800, q], data);
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
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}
/// A random JSON-like text: mostly valid, near the small limits.
fn generate(random: &mut Random, depth: usize, out: &mut String) {
    let space = |random: &mut Random, out: &mut String| {
        if random.below(6) == 0 {
            out.push_str(random.pick(&[" ", "\n", "\t", "\r", "  "]));
        }
    };
    space(random, out);
    match random.below(if depth > 5 { 6 } else { 9 }) {
        0 => out.push_str(random.pick(&["null", "true", "false"])),
        1 | 2 => out.push_str(random.pick(&[
            "0", "1", "-1", "-0", "1.5", "2147483647", "2147483648", "-2147483649", "1e3",
            "1e400", "0.5e-3", "12345678901234567890",
        ])),
        3 | 4 | 5 => {
            out.push('"');
            for _ in 0..random.below(10) {
                out.push_str(random.pick(&[
                    "a", "b", "x", "1", "0", "\\n", "\\u00e9", "\\ud800", "\\udc00", "\u{e9}",
                    "\u{1f600}", "\\\"", "\\\\", "9",
                ]));
            }
            out.push('"');
        }
        6 | 7 => {
            out.push('[');
            let count = random.below(8);
            for index in 0..count {
                if index > 0 {
                    out.push(',');
                }
                generate(random, depth + 1, out);
            }
            out.push(']');
        }
        _ => {
            out.push('{');
            let count = random.below(6);
            for index in 0..count {
                if index > 0 {
                    out.push(',');
                }
                space(random, out);
                out.push_str(random.pick(&[
                    "\"a\"", "\"b\"", "\"c\"", "\"d\"", "\"e\u{e9}\"", "\"f\"", "\"x\"", "\"y\"",
                    "\"z\"", "\"0\"", "\"1\"", "\"10\"", "\"01\"", "\"\\ud83d\\ude00\"",
                    "\"\\ud800\"", "\"k\"",
                ]));
                space(random, out);
                out.push(':');
                generate(random, depth + 1, out);
            }
            out.push('}');
        }
    }
    space(random, out);
}
/// Corrupt a text: delete, duplicate or replace a unit.
fn mutate(random: &mut Random, units: &mut Vec<u16>) {
    if units.is_empty() {
        return;
    }
    let at = random.below(units.len() as u64) as usize;
    match random.below(4) {
        0 => {
            units.remove(at);
        }
        1 => units.insert(at, units[at]),
        2 => {
            units[at] = *random.pick(&[
                u16::from(b'"'),
                u16::from(b','),
                u16::from(b'}'),
                u16::from(b']'),
                u16::from(b'\\'),
                u16::from(b'1'),
                0xd800,
                0xdc00,
                0x01,
            ]);
        }
        _ => units.truncate(at),
    }
}

#[test]
fn random_inputs_and_mutations_agree() {
    let schemas = [unknown(), records()];
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let mut outcomes = std::collections::BTreeMap::new();
    for round in 0..6000 {
        let mut text = String::new();
        if round % 2 == 1 {
            // Records near the field shapes.
            text.push_str(random.pick(&[
                r#"{"a":1,"c":["#,
                r#"{"c":[],"a":"#,
                r#"{"a":2,"c":[1,null,"x"],"d":{"#,
            ]));
            generate(&mut random, 1, &mut text);
            text.push_str(random.pick(&["]}", "}}", "]", "}"]));
        } else {
            generate(&mut random, 0, &mut text);
        }
        let mut units: Vec<u16> = text.encode_utf16().collect();
        if random.below(3) == 0 {
            mutate(&mut random, &mut units);
        }
        for schema in &schemas {
            for data in [limits(), counted(), DataLimits::V38] {
                let outcome = agree(schema, &units, data);
                let key = if outcome.starts_with("Limit") || outcome.len() < 10 {
                    outcome
                } else {
                    "Ok".into()
                };
                *outcomes.entry(key).or_insert(0) += 1;
            }
        }
    }
    // The corpus reaches every outcome class.
    for class in [
        "Ok",
        "Syntax",
        "Type",
        "Limit(Depth)",
        "Limit(Nodes)",
        "Limit(ArrayElements)",
        "Limit(StringBytes)",
        "Limit(DictionaryEntries)",
    ] {
        assert!(outcomes.contains_key(class), "{class}: {outcomes:?}");
    }
}

/// The schema reader of the linked decoder reads every schema the `serde`
/// reader reads (and its serialization) to the same value, and rejects what
/// it rejects.
#[test]
fn the_no_std_schema_reader_agrees_with_serde() {
    let bounds = schema::SchemaBounds::WASM;
    let limit = crate::limits::WASM.schema_bytes;
    for schema in [unknown(), records()] {
        let bytes = schema.to_bytes_within(&crate::limits::WASM).unwrap();
        assert_eq!(JsonSchema::read(&bytes, bounds, limit).unwrap(), schema);
        let spaced = String::from_utf8(bytes.clone())
            .unwrap()
            .replace(',', " ,\n\t")
            .replace(':', " : ");
        assert_eq!(
            JsonSchema::read(spaced.as_bytes(), bounds, limit).unwrap(),
            JsonSchema::from_bytes_within(spaced.as_bytes(), &crate::limits::WASM).unwrap()
        );
    }
    let reordered = r#"{"types":[{"fields":[{"ty":1,"optional":false,"name":"\u0061"}],"descriptor":7,"kind":"record"},{"kind":"float64"}],"root":0,"version":18}"#;
    assert_eq!(
        JsonSchema::read(reordered.as_bytes(), bounds, limit).unwrap(),
        JsonSchema::from_bytes_within(reordered.as_bytes(), &crate::limits::WASM).unwrap()
    );
    for rejected in [
        r#"{"version":18,"root":0}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64"}],"extra":1}"#,
        r#"{"version":18,"version":18,"root":0,"types":[{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float"}]}"#,

        r#"{"version":18,"root":0,"types":[{"kind":"array"}]}"#,
        r#"{"version":18,"root":-0,"types":[{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0.0,"types":[{"kind":"float64"}]}"#,
        r#"{"version":18,"root":4294967296,"types":[{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64"}]} x"#,
        r#"{"version":18,"root":1,"types":[{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"record","descriptor":0,"fields":[{"name":"\ud800","ty":0}]}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"record","descriptor":0,"fields":[{"name":"b","ty":0},{"name":"a","ty":0}]}]}"#,
    ] {
        assert!(
            JsonSchema::from_bytes_within(rejected.as_bytes(), &crate::limits::WASM).is_err(),
            "{rejected}"
        );
        assert!(JsonSchema::read(rejected.as_bytes(), bounds, limit).is_err(), "{rejected}");
    }
}

/// `JsonLimits::V38` is what [`JsonLimits::of`] computes for v38.
#[test]
fn v38_limits_are_the_data_limits() {
    assert_eq!(
        JsonLimits::of(DataLimits::V38, HASHED_DICTIONARY_VALUE_VERSION),
        JsonLimits::V38
    );
    assert_eq!(
        schema::SchemaBounds::of(&crate::limits::WASM),
        schema::SchemaBounds::WASM
    );
}
/// The `no_std` schema reader agrees with `serde` on the edge forms: unit
/// variants ignoring members, sequence forms, numbers out of range.
#[test]
fn the_schema_reader_agrees_on_edge_forms() {
    for text in [
        r#"{"version":18,"root":0,"types":[{"kind":"float64","element":1}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","foo":"x"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"array","element":0,"foo":1}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"array","element":0,"members":[1]}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"array","element":0,"kind":"array"}]}"#,
        r#"{"version":18,"root":0,"types":[{"element":0,"kind":"array"}]}"#,
        r#"{"version":18,"root":0,"types":[{"element":0,"element":0,"kind":"array"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[["float64"]]}"#,
        r#"[18,0,[{"kind":"float64"}]]"#,
        r#"{"version":18,"root":0,"types":[{"kind":"record","descriptor":0,"fields":[["a",1,false]]},{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"record","descriptor":0,"fields":[{"name":"a","ty":1,"optional":null}]},{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64"}],"root":0}"#,
        r#"{"version":18e0,"root":0,"types":[{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","element":"x"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","element":[1,{}]}]}"#,
        r#"{"version":18,"root":0,"types":[["float64", 1]]}"#,
        r#"{"version":18,"root":0,"types":[["array", 0]]}"#,
        r#"{"version":18,"root":0,"types":[["array"]]}"#,
        r#"{"version":18,"root":0,"types":[["array", 0, 1]]}"#,
        r#"{"version":18,"root":0,"types":[["record", 0, [["a",1]]],{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[["record", 0, [["a",1,false,5]]],{"kind":"float64"}]}"#,
        r#"[18,0,[{"kind":"float64"}],1]"#,
        r#"[18,0]"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","x":1e400}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","x":"\ud800"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","x":-1}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","x":{"a":1,"a":2}}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64","x":1,"x":2}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"float64"}],"x":1}"#,
        r#"{"version":18,"root":0,"types":[{"kind":"record","descriptor":0,"fields":[{"name":"a","ty":1,"x":1}]},{"kind":"float64"}]}"#,
        r#"{"version":18,"root":0,"types":[{"kind":3}]}"#,
    ] {
        let serde = JsonSchema::from_bytes_within(text.as_bytes(), &crate::limits::WASM).ok();
        let read = JsonSchema::read(
            text.as_bytes(),
            schema::SchemaBounds::WASM,
            crate::limits::WASM.schema_bytes,
        )
        .ok();
        assert_eq!(read, serde, "{text}");
    }
}
