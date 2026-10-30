//! V38 `linked_intrinsics`: the pure intrinsics run as linked guest code and
//! compute exactly what the host implementations compute, on one corpus per
//! family; v38 modules import none of them; the private intrinsic memory is
//! bounded and never exported.
use super::*;
use crate::contract_tests::rewrite;
use varta_common::{HostOperation, language::LanguageProfile as P};
use varta_intrinsics::math::{self, Portable, Unary};

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn imports(bytes: &[u8]) -> Vec<HostOperation> {
    let mut operations = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ImportSection(reader) = payload.unwrap() {
            for import in reader {
                operations.push(HostOperation::from_name(import.unwrap().name).unwrap());
            }
        }
    }
    operations
}
/// Output lines and consumed fuel of `bytes` under `fuel` production budgets.
fn execute(bytes: &[u8], fuel: u64) -> (Vec<String>, u64) {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(fuel * abi::FUEL).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    crate::exceptions::finish(&instance, store).unwrap();
    crate::scheduler::checkpoint(&instance, store).unwrap();
    let used = fuel * abi::FUEL - store.get_fuel().unwrap();
    (store.data().output.lines.clone(), used)
}
fn run(source: &str, profile: P) -> Vec<String> {
    let bytes = compile(source, profile);
    if profile == V38 {
        let linked: Vec<_> = imports(&bytes)
            .into_iter()
            .filter(|op| op.linked())
            .collect();
        assert!(linked.is_empty(), "v38 imports {linked:?}");
    }
    execute(&bytes, 100).0
}
/// Both profiles print the same lines, and v37 really used host imports.
fn same_as_host(source: &str) -> Vec<String> {
    let host = run(source, V37);
    assert!(
        imports(&compile(source, V37)).iter().any(|op| op.linked()),
        "the corpus must exercise a host intrinsic:\n{source}"
    );
    assert_eq!(run(source, V38), host, "{source}");
    host
}

const NUMBER_TEXT: &str = r#"
export function main(): void {
  const values: number[] = [0, -0, 1, -1, 0.1, 0.5, 1.5, 2.5, -2.5, 123.456, 1e21, 1e-7, 5e-324,
    1.7976931348623157e308, 255, 4294967295, 0.000001234, 98765.4321, NaN, Infinity, -Infinity];
  for (const x of values) {
    console.log(x.toString(2) + " " + x.toString(16) + " " + x.toString(36) + " " + x.toString(7));
    console.log(x.toFixed(0) + " " + x.toFixed(2) + " " + x.toFixed(20));
    console.log(x.toExponential() + " " + x.toExponential(0) + " " + x.toExponential(5));
    console.log(x.toPrecision() + " " + x.toPrecision(1) + " " + x.toPrecision(7) + " " + x.toPrecision(21));
  }
}
"#;

const NUMBER_PARSE: &str = r#"
export function main(): void {
  const texts: string[] = ["42", "  -0x1F  ", "0b101", "0o17", "1e3", ".5", "-.5e-3", "Infinity",
    "-Infinity", "infinity", "12px", "", "   ", "0x", "1_000", "١٢", "  7 ﻿",
    "123456789012345678901234567890", "9007199254740993", "1.7976931348623159e308", "-0"];
  const radices: number[] = [0, 2, 8, 10, 16, 36, 37, 1, -1, 16.9];
  for (const t of texts) {
    let line = parseFloat(t) + " |";
    for (const r of radices) {
      line = line + " " + parseInt(t, r);
    }
    console.log(line + " | " + parseInt(t));
  }
}
"#;

const PRIMITIVES: &str = r#"
function cell(i: number): string | number | boolean | null | undefined {
  if (i === 0) { return "  12.5  "; }
  if (i === 1) { return 0.1; }
  if (i === 2) { return true; }
  if (i === 3) { return null; }
  if (i === 4) { return undefined; }
  if (i === 5) { return "0x10"; }
  if (i === 6) { return "abc"; }
  if (i === 7) { return -0; }
  if (i === 8) { return "  -7e2 　"; }
  return 1e21;
}
export function main(): void {
  for (let i = 0; i < 10; i++) {
    const v = cell(i);
    console.log(String(v) + " -> " + isNaN(v) + " " + isFinite(v));
  }
}
"#;

const STRINGS: &str = r#"
export function main(): void {
  const words: string[] = ["Straße", "ΑΣ ΑΣΑ", "İstanbul", "ǆemal ﬁx", "abc", "", "\uD800x\uDC00", "😀ß"];
  for (const w of words) {
    console.log(w.toUpperCase() + " " + w.toLowerCase());
    let codes = "";
    for (let i = -1; i <= w.length; i++) {
      codes = codes + " " + w.charCodeAt(i) + w.charAt(i);
    }
    console.log(codes + " " + w.charCodeAt(0.9) + w.charAt(NaN));
  }
}
"#;

/// One program per base: `a ** b` and `a % b` with the sign of zeros.
fn arithmetic(base: &str) -> String {
    format!(
        r#"
export function main(): void {{
  const a = {base};
  const values: number[] = [0, -0, 1, -1, 2, -2, 0.5, -0.5, 3, -3, 1e308, 5e-324, NaN, Infinity, -Infinity];
  let line = "";
  for (const b of values) {{
    // A fractional exponent of an ordinary base is approximated (the
    // platform and libm may differ in the last bit); every other pair is
    // exact.
    const exact = b === Math.trunc(b) || !isFinite(b) || a !== a || a === 0 || !isFinite(a) || a === 1;
    const p = exact ? a ** b : 0;
    const r = a % b;
    line = line + " " + p + "/" + (p === 0 ? 1 / p : 0) + "," + r + "/" + (r === 0 ? 1 / r : 0);
  }}
  console.log(line);
}}
"#
    )
}

#[test]
fn linked_number_text_matches_the_host() {
    let lines = same_as_host(NUMBER_TEXT);
    assert_eq!(lines.len(), 21 * 4);
    assert!(lines.contains(&"varta log: 11111111 ff 73 513".to_owned()));
}
#[test]
fn linked_number_parsing_matches_the_host() {
    same_as_host(NUMBER_PARSE);
}
#[test]
fn linked_primitive_conversions_match_the_host() {
    same_as_host(PRIMITIVES);
}
#[test]
fn linked_string_access_and_case_mapping_match_the_host() {
    same_as_host(STRINGS);
}
#[test]
fn linked_power_and_remainder_match_the_host_on_exact_cases() {
    // Every pair here is an exact case (special values, integral powers of
    // small integers), where the platform and libm agree bit for bit.
    for base in [
        "0",
        "-0",
        "1",
        "-1",
        "2",
        "-2",
        "0.5",
        "-0.5",
        "3",
        "-3",
        "1e308",
        "5e-324",
        "NaN",
        "Infinity",
        "-Infinity",
    ] {
        same_as_host(&arithmetic(base));
    }
}

/// Inputs covering the ECMAScript special cases and ordinary finite values.
const MATH_INPUTS: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    -0.5,
    2.0,
    10.0,
    0.1,
    1e-300,
    1e300,
    std::f64::consts::PI,
    std::f64::consts::FRAC_PI_2,
    100.25,
    -7.75,
    0.999999,
    710.0,
    -745.0,
    1e-17,
    123456.789,
];
const UNARY: &[(&str, Unary)] = &[
    ("acos", Unary::Acos),
    ("acosh", Unary::Acosh),
    ("asin", Unary::Asin),
    ("asinh", Unary::Asinh),
    ("atan", Unary::Atan),
    ("atanh", Unary::Atanh),
    ("cbrt", Unary::Cbrt),
    ("cos", Unary::Cos),
    ("cosh", Unary::Cosh),
    ("exp", Unary::Exp),
    ("expm1", Unary::Expm1),
    ("log", Unary::Log),
    ("log10", Unary::Log10),
    ("log2", Unary::Log2),
    ("log1p", Unary::Log1p),
    ("sin", Unary::Sin),
    ("sinh", Unary::Sinh),
    ("tan", Unary::Tan),
    ("tanh", Unary::Tanh),
];
fn literal(value: f64) -> String {
    if value == 0.0 && value.is_sign_negative() {
        "-0".into()
    } else {
        varta_intrinsics::number::canonical_display(value)
    }
}
/// The exact text a guest logs for `value`: canonical, with zero's sign.
fn exact(value: f64) -> String {
    let zero = if value == 0.0 { 1.0 / value } else { 0.0 };
    format!(
        "{}/{}",
        varta_intrinsics::number::canonical_display(value),
        varta_intrinsics::number::canonical_display(zero)
    )
}

#[test]
fn linked_math_is_bit_identical_to_portable_native_code() {
    // One function and one log line per operation (or atan2/power row)
    // keeps bodies within the compile budget and v37 within its host calls.
    let mut source = String::new();
    let mut calls = String::new();
    let mut expected = Vec::new();
    let mut check = |name: &str, cases: Vec<(String, f64)>| {
        source.push_str(&format!(
            "function check_{name}(): void {{\n  let r = 0;\n  let line = \"\";\n"
        ));
        calls.push_str(&format!("  check_{name}();\n"));
        let mut line = String::from("varta log: ");
        for (expression, value) in cases {
            source.push_str(&format!(
                "  r = {expression}; line = line + \" \" + r.toString() + \"/\" + (r === 0 ? 1 / r : 0).toString();\n"
            ));
            line.push(' ');
            line.push_str(&exact(value));
        }
        source.push_str("  console.log(line);\n}\n");
        expected.push(line);
    };
    for &(name, operation) in UNARY {
        check(
            name,
            MATH_INPUTS
                .iter()
                .map(|&x| {
                    (
                        format!("Math.{name}({})", literal(x)),
                        math::unary::<Portable>(operation, x),
                    )
                })
                .collect(),
        );
    }
    for (row, &y) in MATH_INPUTS.iter().enumerate() {
        let (mut atan2, mut power) = (Vec::new(), Vec::new());
        for &x in MATH_INPUTS {
            atan2.push((
                format!("Math.atan2({}, {})", literal(y), literal(x)),
                math::atan2::<Portable>(y, x),
            ));
            power.push((
                format!("({}) ** ({})", literal(y), literal(x)),
                math::power::<Portable>(y, x),
            ));
        }
        check(&format!("atan2_{row}"), atan2);
        check(&format!("power_{row}"), power);
    }
    source.push_str(&format!("export function main(): void {{\n{calls}}}\n"));
    let v38 = run(&source, V38);
    assert_eq!(v38, expected);
    // The ECMAScript-required cases (here: signed zeros) also equal the
    // platform-backed v37 host bit for bit.
    let mut zeros = String::new();
    let mut calls = String::new();
    for (name, _) in UNARY {
        zeros.push_str(&format!(
            "function zero_{name}(): void {{\n  let r = 0;\n  let line = \"\";\n"
        ));
        calls.push_str(&format!("  zero_{name}();\n"));
        for x in ["0", "-0"] {
            zeros.push_str(&format!(
                "  r = Math.{name}({x}); line = line + \" \" + r.toString() + \"/\" + (r === 0 ? 1 / r : 0).toString();\n"
            ));
        }
        zeros.push_str("  console.log(line);\n}\n");
    }
    zeros.push_str(&format!("export function main(): void {{\n{calls}}}\n"));
    assert_eq!(run(&zeros, V38), run(&zeros, V37));
}

#[test]
fn linked_intrinsics_keep_results_across_forced_collections() {
    // Each iteration doubles a filler string to about 120 KiB, so the run
    // allocates far more than the managed heap holds and the collector runs
    // repeatedly while results of earlier linked calls are still rooted.
    let source = r#"
export function main(): void {
  let kept: string[] = [];
  let total = 0;
  for (let i = 0; i < 60; i++) {
    const text = (i * 1.25).toFixed(3) + "|" + (i * 7).toString(36) + "|" + ("Item ß" + i).toUpperCase();
    total = total + parseFloat(text) + parseInt(text.substring(text.indexOf("|") + 1), 36);
    let filler = text;
    for (let j = 0; j < 12; j++) { filler = filler + filler; }
    const lower = filler.toLowerCase();
    if (i % 10 === 0) { kept.push(text + lower.charAt(3) + lower.length); }
  }
  for (const k of kept) { console.log(k); }
  console.log(total);
}
"#;
    let host = execute(&compile(source, V37), 1000).0;
    assert_eq!(execute(&compile(source, V38), 1000).0, host);
    assert_eq!(host.len(), 7);
}

#[test]
fn v38_validation_rejects_imported_pure_intrinsics_and_exposed_intrinsic_memory() {
    let runtime = VartaRuntime::new();
    // A v37 module relabelled as v38 still imports host intrinsics.
    let v37 = compile(
        "export function main(): void { console.log(Math.sin(0.5)); }",
        V37,
    );
    let relabelled = rewrite(&v37, |id, data| {
        let mut data = data.to_vec();
        let name = |section: &str| {
            data.get(1..1 + section.len()) == Some(section.as_bytes())
                && data[0] as usize == section.len()
        };
        if id == 0 && name(varta_common::language::SECTION) {
            let prefix = 1 + varta_common::language::SECTION.len();
            let mut section = data[..prefix].to_vec();
            let contract = varta_common::language::LanguageContract {
                schema_version: varta_common::language::SCHEMA_VERSION,
                profile: V38,
            };
            section.extend_from_slice(&contract.to_bytes().unwrap());
            return vec![section];
        }
        if id == 0 && name(abi::SECTION) {
            let prefix = 1 + abi::SECTION.len();
            data.truncate(prefix);
            data.extend_from_slice(&V38.host_abi().to_le_bytes());
        }
        vec![data]
    });
    let error = runtime
        .validate_module(&relabelled)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("pure intrinsic must be linked into the module, not imported: number_sin"),
        "{error}"
    );
    // A second memory is v38's alone, bounded and private.
    let v38 = compile(
        "export function main(): void { console.log(Math.sin(0.5)); }",
        V38,
    );
    runtime.validate_module(&v38).unwrap();
    let mut two_memories = v37.clone();
    two_memories = rewrite(&two_memories, |id, data| {
        let mut data = data.to_vec();
        if id == 5 {
            data[0] += 1;
            data.extend_from_slice(&[0x01, 0x00, 0x01]);
        }
        vec![data]
    });
    let error = runtime
        .validate_module(&two_memories)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("only one memory is supported"), "{error}");
    let oversized = rewrite(&v38, |id, data| {
        let mut data = data.to_vec();
        if id == 5 {
            // The private memory's maximum is the last LEB of the section.
            let maximum = (abi::MAX_INTRINSIC_MEMORY_BYTES / abi::WASM_PAGE_BYTES) as u8;
            let at = data.iter().rposition(|&b| b == maximum).unwrap();
            data[at] = maximum + 1;
        }
        vec![data]
    });
    let error = runtime
        .validate_module(&oversized)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("oversized intrinsic memory"), "{error}");
    let exported = rewrite(&v38, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            data[0] += 1;
            data.extend_from_slice(&[1, b'm', 2, 1]);
        }
        vec![data]
    });
    let error = runtime
        .validate_module(&exported)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("intrinsic memory must not be exported"),
        "{error}"
    );
}

#[test]
fn intrinsic_layout_matches_common() {
    use varta_common::value;
    use varta_intrinsics::layout as l;
    assert_eq!(l::RESULT_BASE, abi::RESULT_BASE);
    assert_eq!(l::MAX_RESULT_BYTES, abi::MAX_RESULT_BYTES);
    assert_eq!(l::MAX_STRING_BYTES, abi::MAX_STRING_BYTES);
    assert_eq!(l::OWNED_STRING_BASE, abi::OWNED_STRING_BASE);
    assert_eq!(l::STRING_HEADER_BYTES, abi::STRING_HEADER_BYTES);
    assert_eq!(l::WASM_PAGE_BYTES, abi::WASM_PAGE_BYTES);
    assert_eq!(l::HEAP_ALIGNMENT, value::HEAP_ALIGNMENT);
    assert_eq!(l::HEAP_FLAGS_OFFSET, value::HEAP_FLAGS_OFFSET);
    assert_eq!(l::HEAP_LINK_OFFSET, value::HEAP_LINK_OFFSET);
    assert_eq!(l::HEAP_ALLOCATED_FLAG, value::HEAP_ALLOCATED_FLAG);
    assert_eq!(l::HEAP_UNION_FLAG, value::HEAP_UNION_FLAG as u32);
    assert_eq!(l::UNION_META_BYTES, value::UNION_META_BYTES);
    assert_eq!(l::UNION_BYTES, value::UNION_BYTES);
    for tag in 0..=u8::MAX as u32 {
        let common = value::ValueKind::from_tag(tag);
        let shared = l::Kind::from_tag(tag);
        assert_eq!(
            common.map(|k| k as u32),
            shared.map(|k| k as u32),
            "tag {tag}"
        );
        if let (Some(common), Some(shared)) = (common, shared) {
            assert_eq!(common.is_reference(), shared.is_reference(), "tag {tag}");
        }
    }
    for unit in 0..=u16::MAX {
        assert_eq!(
            varta_common::text::ecmascript_whitespace(unit),
            varta_intrinsics::text::ecmascript_whitespace(unit)
        );
    }
}

#[test]
fn shared_primitive_cell_reader_agrees_with_common() {
    // Canonical cells of every primitive kind, then every single-byte
    // corruption of their headers and payloads: both readers accept or
    // reject each the same way and agree on accepted contents.
    let source = r#"
function cell(i: number): string | number | boolean | null | undefined {
  if (i === 0) { return "text"; }
  if (i === 1) { return 2.5; }
  if (i === 2) { return false; }
  if (i === 3) { return null; }
  return undefined;
}
export function main(): void {
  for (let i = 0; i < 5; i++) { console.log(String(cell(i))); }
}
"#;
    let bytes = compile(source, V37);
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    instance
        .get_func(&mut *store, "_start")
        .unwrap()
        .call(&mut *store, &[], &mut [])
        .unwrap();
    let memory = instance.get_memory(&mut *store, "memory").unwrap();
    let data = memory.data(&*store).to_vec();
    // Every aligned heap address is a candidate handle of a 16-byte cell.
    let mut accepted = 0;
    for ptr in
        (abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES..data.len()).step_by(value_alignment())
    {
        let handle = ((varta_common::value::UNION_BYTES as u64) << 32) | ptr as u64;
        let common = varta_common::value::PrimitiveBox::read(&data, handle);
        let shared = varta_intrinsics::cell::Cell::read(&data[..], handle);
        assert_eq!(common.is_ok(), shared.is_ok(), "handle {handle:#x}");
        if let (Ok(common), Ok(shared)) = (common, shared) {
            accepted += 1;
            assert_eq!(common.kind as u32, shared.kind as u32);
            assert_eq!(common.payload, shared.payload);
            let mut corrupt = data.clone();
            for offset in ptr - abi::STRING_HEADER_BYTES..ptr + varta_common::value::UNION_BYTES {
                corrupt[offset] ^= 0x41;
                assert_eq!(
                    varta_common::value::PrimitiveBox::read(&corrupt, handle).is_ok(),
                    varta_intrinsics::cell::Cell::read(&corrupt[..], handle).is_ok(),
                    "corruption at {offset:#x}"
                );
                corrupt[offset] ^= 0x41;
            }
        }
    }
    assert!(accepted >= 5, "found {accepted} primitive cells");
}
fn value_alignment() -> usize {
    varta_common::value::HEAP_ALIGNMENT
}

/// Size, compile-time and fuel measurements for the documentation; run with
/// `cargo test -p varta-runtime --lib linked_intrinsic_measurements -- --ignored --nocapture`.
#[test]
#[ignore = "measurement report, not a check"]
fn linked_intrinsic_measurements() {
    let families: &[(&str, &str)] = &[
        ("none", "export function main(): void { console.log(0.5); }"),
        (
            "Math.sin",
            "export function main(): void { console.log(Math.sin(0.5)); }",
        ),
        (
            "every Math",
            "export function main(): void { const x = 0.5; console.log(Math.sin(x)+Math.cos(x)+Math.tan(x)+Math.asin(x)+Math.acos(x)+Math.atan(x)+Math.atan2(x,2)+Math.sinh(x)+Math.cosh(x)+Math.tanh(x)+Math.asinh(x)+Math.acosh(2)+Math.atanh(x)+Math.cbrt(x)+Math.exp(x)+Math.expm1(x)+Math.log(x)+Math.log10(x)+Math.log2(x)+Math.log1p(x)+x**3+(7%3)); }",
        ),
        (
            "toFixed",
            "export function main(): void { console.log((1.5).toFixed(2)); }",
        ),
        (
            "toString(radix)",
            "export function main(): void { console.log((255.5).toString(16)); }",
        ),
        (
            "Number text (4)",
            "export function main(): void { console.log((255).toString(16) + (1.5).toFixed(2) + (12345).toExponential(1) + (0.5).toPrecision(3)); }",
        ),
        (
            "parseInt/parseFloat",
            "export function main(): void { console.log(parseInt(\"ff\", 16) + parseFloat(\"1.5e3\")); }",
        ),
        (
            "isNaN/isFinite(union)",
            "function p(f: boolean): string | number { return f ? \"7\" : 2.5; } export function main(): void { console.log(isNaN(p(true))); console.log(isFinite(p(false))); }",
        ),
        (
            "charAt/charCodeAt",
            "export function main(): void { const s = \"abc\"; console.log(s.charAt(1) + s.charCodeAt(2)); }",
        ),
        (
            "toUpperCase/toLowerCase",
            "export function main(): void { const s = \"Straße\"; console.log(s.toUpperCase() + s.toLowerCase()); }",
        ),
    ];
    let engine = wasmtime::Engine::default();
    println!("| Family | v37 bytes | v38 bytes | growth | v38 compile ms | fuel v37 | fuel v38 |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for (name, source) in families {
        let v37 = compile(source, V37);
        let v38 = compile(source, V38);
        let started = std::time::Instant::now();
        for _ in 0..5 {
            wasmtime::Module::new(&engine, &v38).unwrap();
        }
        let compile_ms = started.elapsed().as_secs_f64() * 1000.0 / 5.0;
        let (_, fuel37) = execute(&v37, 1);
        let (_, fuel38) = execute(&v38, 1);
        println!(
            "| {name} | {} | {} | {:+} | {compile_ms:.1} | {fuel37} | {fuel38} |",
            v37.len(),
            v38.len(),
            v38.len() as i64 - v37.len() as i64
        );
        // Modules for an external cold-start measurement (for example Node).
        for (profile, bytes) in [("v37", &v37), ("v38", &v38)] {
            let path = std::env::temp_dir().join(format!(
                "varta-{profile}-{}.wasm",
                name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            ));
            std::fs::write(path, bytes).unwrap();
        }
    }
}
