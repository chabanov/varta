//! V38 vectorised JSON scanning (`simd128`) in the compiled decoders: the
//! same outcomes as the scalar scanner on the existing corpora and on
//! strings whose escapes, controls, quotes and non-ASCII units straddle the
//! 16-byte lanes or the end of the input, and the fuel a scanned string byte
//! costs.
use super::*;
use crate::codegen_v2_tests::{Run, compile};
use crate::fuel_profile_tests::profiled;
use crate::json_decoder_tests::{literal_corpus_program, random_schema_programs};
use crate::linked_json_tests::{CORPUS, cause, run_test_budgets};
use varta_common::language::LanguageProfile as P;
use varta_compiler::json_decoder_test_hook as hook;

const V38: P = P::CheckedF64Utf16V38;

/// Whether a module loads any `v128` (every vector step does).
fn has_simd(bytes: &[u8]) -> bool {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .any(|payload| match payload.unwrap() {
            wasmparser::Payload::CodeSectionEntry(body) => body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .any(|op| matches!(op.unwrap(), wasmparser::Operator::V128Load { .. })),
            _ => false,
        })
}

/// The `simd128` decoders' run, the scalar decoders' run and the linked
/// decoder's run of `source`: identical lines and failure cause. Returns the
/// `simd128` and scalar runs.
fn same_as_scalar(source: &str, budgets: u64) -> (Run, Run) {
    let vector = compile(source, V38);
    let scalar = hook::scalar(|| compile(source, V38));
    let generic = hook::compile_generic(source, V38).unwrap_or_else(|e| panic!("{e}\n{source}"));
    assert!(has_simd(&vector), "no SIMD decoders compiled\n{source}");
    assert!(!has_simd(&scalar), "SIMD in the scalar reference\n{source}");
    let runs = [&vector, &scalar, &generic].map(|bytes| run_test_budgets(bytes, budgets));
    for (run, name) in runs[1..].iter().zip(["scalar", "linked"]) {
        assert_eq!(
            runs[0].lines, run.lines,
            "simd128 against {name}\n{source}\n{:?}\n{:?}",
            runs[0].error, run.error
        );
        assert_eq!(cause(&runs[0].error), cause(&run.error), "{name}\n{source}");
    }
    let [vector, scalar, _] = runs;
    (vector, scalar)
}

/// A JS string literal holding exactly `units` (lone surrogates included).
fn literal(units: &[u16]) -> String {
    let mut out = String::from("\"");
    for &unit in units {
        match char::from_u32(u32::from(unit)) {
            Some(c) if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' => out.push(c),
            _ => out.push_str(&format!("\\u{unit:04x}")),
        }
    }
    out.push('"');
    out
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// What may stop or break a plain run, as the units placed into string
/// bodies: quotes, every escape form (valid and malformed), controls, and
/// units whose low byte is special or which saturate when narrowed.
fn specials() -> Vec<Vec<u16>> {
    let mut specials: Vec<Vec<u16>> = [
        "\"",
        "\\\"",
        "\\\\",
        "\\/",
        "\\b",
        "\\f",
        "\\n",
        "\\r",
        "\\t",
        "\\u00e9",
        "\\uD83D\\uDE00",
        "\\u12g4",
        "\\x",
        "\\",
        "\\u00",
        "é",
        "😀",
        "\u{7f}",
    ]
    .iter()
    .map(|text| units(text))
    .collect();
    for unit in [
        0x00, 0x1f, 0x20, 0x21, 0x23, 0x5b, 0x5d, 0x80, 0xff, 0x100, 0x109, 0x120, 0x122, 0x15c,
        0x2022, 0x5c5c, 0x7fff, 0x8000, 0x8022, 0xd800, 0xdbff, 0xdc00, 0xdfff, 0xfffe, 0xffff,
    ] {
        specials.push(vec![unit]);
    }
    specials
}

/// Offsets of a special within a string body: every lane of the first
/// step, the step boundary and the second step.
const OFFSETS: [usize; 21] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 31, 32, 33,
];
/// Plain units after a special: ending before, at and after a step.
const TAILS: [usize; 5] = [0, 1, 15, 16, 17];
/// Specials per compiled program.
const SPECIALS_PER_PROGRAM: usize = 11;

/// Strings with a special at every lane of a step, followed by plain units
/// that end before, at and after a step, in arrays, as top-level strings (so
/// the input ends right after, or inside, the string) and as keys: the
/// `simd128`, scalar and linked decoders agree.
#[test]
fn simd_matches_scalar_on_specials_at_every_lane() {
    for specials in specials().chunks(SPECIALS_PER_PROGRAM) {
        let mut array = Vec::new();
        let mut top = Vec::new();
        let mut keys = Vec::new();
        for special in specials {
            for offset in OFFSETS {
                for tail in TAILS {
                    let mut body = units(&"a".repeat(offset));
                    body.extend(special);
                    body.extend(units(&"é".repeat(tail % 3)));
                    body.extend(units(&"b".repeat(tail)));
                    let quoted = [units("\""), body.clone(), units("\"")].concat();
                    array.push(literal(
                        &[units("["), quoted.clone(), units(",\"x\"]")].concat(),
                    ));
                    top.push(literal(&quoted));
                    // Unterminated: the input ends inside the string.
                    top.push(literal(&[units("\""), body].concat()));
                    keys.push(literal(
                        &[units("{\"s\":\"v\","), quoted, units(":1,\"t\":\"w\"}")].concat(),
                    ));
                }
            }
        }
        same_as_scalar(&literal_corpus_program("", "string[]", &array), 400);
        same_as_scalar(&literal_corpus_program("", "string", &top), 400);
        same_as_scalar(
            &literal_corpus_program("interface R { s: string; t: string }", "R", &keys),
            400,
        );
        same_as_scalar(
            &literal_corpus_program("", "Record<string, unknown>", &keys),
            400,
        );
    }
}

/// Whitespace runs of every length to 40 (each JSON whitespace unit, and
/// units whose low byte is whitespace), before and after values, at the
/// input's end and inside it.
#[test]
fn simd_matches_scalar_on_whitespace_runs() {
    let mut texts = Vec::new();
    for length in 0..=40 {
        for space in [" ", "\t", "\n", "\r", " \n\t\r"] {
            let run: String = space.chars().cycle().take(length).collect();
            texts.push(literal(&units(&format!(
                "{run}[{run}1{run},{run}\"x\"{run}]{run}"
            ))));
            texts.push(literal(&units(&format!(
                "{{{run}\"a\"{run}:{run}[{run}]{run}}}"
            ))));
        }
        for unit in [0x0109u16, 0x010a, 0x0120, 0x2020, 0x0b, 0x0c, 0xa0] {
            let mut text = units(&format!("[{}", " ".repeat(length)));
            text.push(unit);
            text.extend(units("1]"));
            texts.push(literal(&text));
            let mut text = units(&format!("[1{}", " ".repeat(length)));
            text.push(unit);
            texts.push(literal(&text));
        }
    }
    same_as_scalar(&literal_corpus_program("", "unknown", &texts), 400);
    same_as_scalar(
        &literal_corpus_program("interface A { a: number[] }", "A", &texts),
        400,
    );
}

/// Every prefix of documents with escapes, non-ASCII text and whitespace:
/// inputs that end in the middle of a vector, a string or an escape.
#[test]
fn simd_matches_scalar_on_every_truncation() {
    let documents = [
        r#"{"text":"line one\nline two \"quoted\" and a path C:\\dir\\file","n":[1,2.5,-3e2],"u":"\u00e9\ud83d\ude00 tail of thirty-two units ......"}"#,
        "[ \"Звіт сесії: усі перевірки пройшли 😀\" ,\n  \"done\\t\" ]",
    ];
    for document in documents {
        let whole = units(document);
        let texts: Vec<String> = (0..=whole.len())
            .map(|end| literal(&whole[..end]))
            .collect();
        same_as_scalar(&literal_corpus_program("", "unknown", &texts), 400);
    }
}

/// The existing corpora: the linked-JSON hand corpus, the seeded random
/// schemas with their mutations, the case programs and the limit cases.
#[test]
fn simd_matches_scalar_on_the_json_corpora() {
    same_as_scalar(CORPUS, 100);
    for program in random_schema_programs() {
        let (vector, _) = same_as_scalar(&program, 200);
        assert_eq!(vector.error, None, "{program}");
    }
    for source in [
        include_str!("json_limit_cases/large_string.ts"),
        include_str!("json_limit_cases/round_trip.ts"),
        include_str!("strict_member_cases/events.ts"),
    ] {
        same_as_scalar(source, 400);
    }
    // A transcript-like document of about 64 KB, compact and indented.
    for indent in [false, true] {
        let (vector, scalar) =
            same_as_scalar(&transcript_program(&transcript(64 * 1024, indent)), 400);
        assert!(vector.fuel < scalar.fuel, "{} {}", vector.fuel, scalar.fuel);
    }
}

/// A full collection before every block the decoders allocate, over strings
/// the vector steps decode and copy: the results stay intact.
#[test]
fn simd_decoders_survive_a_collection_before_every_allocation() {
    let source = transcript_program(&transcript(16 * 1024, true));
    let collected = run_test_budgets(&hook::compile_collecting(&source, V38).unwrap(), 4000);
    let scalar = run_test_budgets(&hook::scalar(|| compile(&source, V38)), 400);
    assert_eq!(collected.error, None);
    assert_eq!(collected.lines, scalar.lines);
}

/// Paragraphs of a session transcript: prose, code, Cyrillic, emoji, and
/// the escapes `JSON.stringify` writes (quotes, backslashes, newlines,
/// tabs).
const PARAGRAPHS: [&str; 6] = [
    "The build finished in 41 s. Next step: run the scoped checks for the compiler and then the runtime feature filter, and record the measured fuel in the plan.",
    "Here is the patch:\n\n```rust\nfn scan(text: &[u16]) -> usize {\n\tlet mut at = 0;\n\twhile at < text.len() && text[at] != b'\"' as u16 { at += 1; }\n\tat\n}\n```\n\nIt keeps the scalar tail for the last \"partial\" vector.",
    "Звіт сесії: усі перевірки пройшли, крім однієї — тест «fuel determinism» очікує нові числа для v38. Причина: векторне сканування рядків.",
    "Path C:\\Users\\agent\\work\\varta and the URL https://example.com/a/b?c=d&e=f; the quote \"as written\" stays.",
    "Emoji and symbols: 😀 🚀 ✅ — mixed with ASCII text so that non-ASCII units fall on every lane of a vector.",
    "A short reply.",
];

/// A transcript-like document of about `bytes` UTF-16 bytes: an array of
/// session events (nested records, long string fields with escapes),
/// compact or indented as `JSON.stringify(value, null, 2)` writes it.
fn transcript(bytes: usize, indent: bool) -> String {
    let mut events = Vec::new();
    let mut size = 0;
    let mut seq = 0usize;
    while size < bytes / 2 {
        let text: String = (0..1 + seq % 4)
            .map(|k| PARAGRAPHS[(seq + k) % PARAGRAPHS.len()])
            .collect::<Vec<_>>()
            .join("\n\n");
        let event = serde_json::json!({
            "seq": seq,
            "role": if seq.is_multiple_of(2) { "user" } else { "assistant" },
            "kind": "message",
            "createdAt": format!("2026-11-05T10:{:02}:{:02}.000Z", seq / 60 % 60, seq % 60),
            "text": text,
            "meta": { "model": "model-a", "tokens": 120 + seq * 7, "latencyMs": 1500.5 },
            "parts": [
                { "type": "text", "text": PARAGRAPHS[seq % PARAGRAPHS.len()] },
                { "type": "tool", "text": "ls -la crates/varta-compiler/src/emit/decoders" },
            ],
        });
        size += event.to_string().encode_utf16().count();
        events.push(event);
        seq += 1;
    }
    let value = serde_json::Value::Array(events);
    if indent {
        serde_json::to_string_pretty(&value).unwrap()
    } else {
        value.to_string()
    }
}

/// The UTF-16 bytes between the quotes of every string and key of `text`.
fn string_bytes(text: &str) -> usize {
    let (mut inside, mut escaped, mut units) = (false, false, 0);
    for unit in text.encode_utf16() {
        if inside {
            if escaped {
                escaped = false;
            } else if unit == u16::from(b'\\') {
                escaped = true;
            } else if unit == u16::from(b'"') {
                inside = false;
                continue;
            }
            units += 1;
        } else if unit == u16::from(b'"') {
            inside = true;
        }
    }
    units * size_of::<u16>()
}

const TRANSCRIPT_TYPES: &str = "interface Meta { model: string; tokens: number; latencyMs: number }
interface Part { type: string; text: string }
interface Event { seq: number; role: string; kind: string; createdAt: string; text: string; meta: Meta; parts: Part[] }";

/// A program parsing `text` as `Event[]` once and printing the result.
fn transcript_program(text: &str) -> String {
    parse_program(
        text,
        "JSON.parse(t) as Event[]",
        "String(JSON.stringify(events).length) + \" \" + events[events.length - 1].text",
    )
}
/// A program that evaluates `parse` over the text `t` and prints `show`
/// of its result `events`.
fn parse_program(text: &str, parse: &str, show: &str) -> String {
    format!(
        "{TRANSCRIPT_TYPES}
export function main(): void {{
  const t = {};
  const events = {parse};
  console.log(String(events.length) + \" \" + {show});
}}",
        serde_json::to_string(text).unwrap()
    )
}

/// The fuel profile rows of one parse: the scanner's self fuel, the
/// whitespace skipper's, the escaped-string builder's and the whole entry's
/// (check and build).
struct ScanFuel {
    text: u64,
    whitespace: u64,
    decode: u64,
    entry: u64,
}
fn scan_fuel(bytes: &[u8]) -> ScanFuel {
    let run = VartaRuntime::new()
        .run_fuel_profile(bytes, &[], None)
        .unwrap();
    assert!(run.error.is_none(), "{:?}", run.error);
    let row = |prefix: &str, total: bool| {
        run.profile
            .functions
            .iter()
            .filter(|row| row.name.starts_with(prefix))
            .map(|row| if total { row.total_fuel } else { row.self_fuel })
            .sum::<u64>()
    };
    ScanFuel {
        text: row("<json Text>", false),
        whitespace: row("<json Whitespace>", false),
        decode: row("<json Decode>", false),
        entry: row("<json entry", true),
    }
}

/// Fuel per scanned string byte on a ~276 KB transcript-like document and
/// on one long plain string (`--nocapture`): the scalar scanner (before)
/// and the `simd128` scanner, the development-plan table.
#[test]
#[ignore]
fn json_scan_fuel_per_string_byte() {
    println!(
        "| document | scanner | text bytes | string bytes | scan fuel | scan per string byte | whitespace fuel | escaped-string build fuel | parse fuel | parse per byte |"
    );
    let long = format!("[\"{}\"]", "plain ascii text ".repeat(8 * 1024));
    let documents = [
        (
            "transcript, compact",
            transcript(276 * 1024, false),
            "Event[]",
        ),
        (
            "transcript, indented",
            transcript(276 * 1024, true),
            "Event[]",
        ),
        ("one plain string", long, "string[]"),
    ];
    for (label, text, ty) in &documents {
        let bytes = text.encode_utf16().count() * size_of::<u16>();
        let strings = string_bytes(text);
        let parse = format!("JSON.parse(t) as {ty}");
        let program = parse_program(text, &parse, "String(t.length)");
        // The linked decoder alone (the decoders' fallback), whole parse:
        // beyond the same program parsing an empty array.
        let without = parse_program(
            text,
            &format!("JSON.parse(\"[]\") as {ty}"),
            "String(t.length)",
        );
        let linked =
            |source: &str| run_test_budgets(&hook::compile_generic(source, V38).unwrap(), 40).fuel;
        let fallback = linked(&program) - linked(&without);
        println!(
            "| {label} | linked decoder | {bytes} | {strings} | – | – | – | – | {fallback} | {:.3} |",
            fallback as f64 / bytes as f64
        );
        for (scanner, bytes_of) in [
            ("scalar", hook::scalar(|| profiled(&program, V38))),
            ("simd128", profiled(&program, V38)),
        ] {
            let fuel = scan_fuel(&bytes_of);
            println!(
                "| {label} | {scanner} | {bytes} | {strings} | {} | {:.3} | {} | {} | {} | {:.3} |",
                fuel.text,
                fuel.text as f64 / strings as f64,
                fuel.whitespace,
                fuel.decode,
                fuel.entry,
                fuel.entry as f64 / bytes as f64,
            );
        }
    }
}
