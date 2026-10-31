//! Fuel is a property of the module and its inputs, never of the host: the
//! same bytes consume exactly the same fuel on every platform, so budgets
//! mean the same everywhere. The golden numbers below are checked on every
//! CI host (macOS arm64 and Linux); a host-dependent Wasmtime setting (such
//! as copy-on-write memory images, which skip or page-round the metered
//! data-segment copy) or host-dependent compiler output changes them.
use super::*;
use crate::codegen_v2_tests::{compile, instantiation_fuel, run_rated, run_with};
use varta_common::language::{LanguageProfile as P, SUPPORTED_PROFILES};

const V1: P = P::CheckedI32Utf8V1;
const V20: P = P::CheckedF64Utf16V20;
const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

/// A program with static data and a loop that every profile admits.
const PORTABLE: &str = r#"export function main(): void {
  let total = 0;
  let text = "";
  let i = 0;
  while (i < 200) {
    total = total + i;
    if (i < 20) { text = text + "ab"; }
    i = i + 1;
  }
  if (total === 19900) { console.log("static data for the initial memory image " + text); }
}"#;

/// One string literal of `units` code units, which v38 may spill into its
/// static extension.
fn literal(units: usize) -> String {
    let literal = "q".repeat(units);
    format!(r#"export function main(): void {{ const a = "{literal}"; console.log("x"); }}"#)
}

/// A keyed list built from records, strings and arrays.
const RECORDS: &str = r#"interface Row { id: number; label: string; }
export function main(): void {
  const rows: Row[] = [];
  for (let i = 0; i < 40; i++) { rows.push({ id: i, label: "row " + String(i) }); }
  let out = "";
  for (const row of rows) { if (row.id % 7 === 0) { out = out + row.label + ";"; } }
  console.log(out);
}"#;

/// Checked JSON decoding, whose host work v38 charges at the fair rates.
const JSON: &str = r#"interface Item { name: string; count: number; }
export function main(): void {
  const items = JSON.parse('[{"name":"a","count":1},{"name":"bb","count":22},{"name":"ccc","count":333}]') as Item[];
  let total = 0;
  for (const item of items) { total = total + item.count + item.name.length; }
  console.log(JSON.stringify(items) + " " + String(total));
}"#;

/// The measured runs: instantiation alone, and whole runs under one
/// production budget (`run`) and with an earlier profile's host work at the
/// v38 fair rates (`rated`, equal to `run` under v38).
fn measure(name: &str, source: &str, profile: P) -> [(String, u64); 3] {
    let bytes = compile(source, profile);
    let run = run_with(&bytes, &[], 1);
    let rated = run_rated(&bytes, &[], 1);
    assert_eq!(run.error, None, "{name} {profile:?}");
    assert_eq!(rated.error, None, "{name} {profile:?}");
    let key = |what: &str| format!("{name} {profile:?} {what}");
    [
        (key("instantiation"), instantiation_fuel(&bytes)),
        (key("run"), run.fuel),
        (key("rated"), rated.fuel),
    ]
}

/// Exact fuel of representative programs, identical on every host
/// (`fuel_is_identical_on_every_host --nocapture` prints the table).
/// Instantiation is 0 in every profile; when v1–v37 were charged for it,
/// every v1–v37 entry differed by host: macOS arm64 charged the copy over
/// 16 KiB pages (`literal 1` v37 instantiation 16385) and Linux skipped it.
const GOLDEN: &[(&str, u64)] = &[
    ("portable CheckedI32Utf8V1 instantiation", 0),
    ("portable CheckedI32Utf8V1 run", 18255),
    ("portable CheckedI32Utf8V1 rated", 18386),
    ("portable CheckedF64Utf16V20 instantiation", 0),
    ("portable CheckedF64Utf16V20 run", 18785),
    ("portable CheckedF64Utf16V20 rated", 18997),
    ("portable CheckedF64Utf16V37 instantiation", 0),
    ("portable CheckedF64Utf16V37 run", 23693),
    ("portable CheckedF64Utf16V37 rated", 23905),
    ("portable CheckedF64Utf16V38 instantiation", 0),
    ("portable CheckedF64Utf16V38 run", 6151),
    ("portable CheckedF64Utf16V38 rated", 6151),
    ("literal 1 CheckedF64Utf16V37 instantiation", 0),
    ("literal 1 CheckedF64Utf16V37 run", 77),
    ("literal 1 CheckedF64Utf16V37 rated", 129),
    ("literal 30000 CheckedF64Utf16V37 instantiation", 0),
    ("literal 30000 CheckedF64Utf16V37 run", 77),
    ("literal 30000 CheckedF64Utf16V37 rated", 129),
    ("records CheckedF64Utf16V37 instantiation", 0),
    ("records CheckedF64Utf16V37 run", 84395),
    ("records CheckedF64Utf16V37 rated", 88665),
    ("json CheckedF64Utf16V37 instantiation", 0),
    ("json CheckedF64Utf16V37 run", 4602),
    ("json CheckedF64Utf16V37 rated", 57680),
    ("unknown assertions CheckedF64Utf16V37 instantiation", 0),
    ("unknown assertions CheckedF64Utf16V37 run", 22680),
    ("unknown assertions CheckedF64Utf16V37 rated", 24938),
    ("literal 1 CheckedF64Utf16V38 instantiation", 0),
    ("literal 1 CheckedF64Utf16V38 run", 102),
    ("literal 1 CheckedF64Utf16V38 rated", 102),
    ("literal 30000 CheckedF64Utf16V38 instantiation", 0),
    ("literal 30000 CheckedF64Utf16V38 run", 102),
    ("literal 30000 CheckedF64Utf16V38 rated", 102),
    ("records CheckedF64Utf16V38 instantiation", 0),
    ("records CheckedF64Utf16V38 run", 32636),
    ("records CheckedF64Utf16V38 rated", 32636),
    ("json CheckedF64Utf16V38 instantiation", 0),
    ("json CheckedF64Utf16V38 run", 51759),
    ("json CheckedF64Utf16V38 rated", 51759),
    ("unknown assertions CheckedF64Utf16V38 instantiation", 0),
    ("unknown assertions CheckedF64Utf16V38 run", 26166),
    ("unknown assertions CheckedF64Utf16V38 rated", 26166),
    ("literal 90000 CheckedF64Utf16V38 instantiation", 0),
    ("literal 90000 CheckedF64Utf16V38 run", 102),
    ("literal 90000 CheckedF64Utf16V38 rated", 102),
];

#[test]
fn fuel_is_identical_on_every_host() {
    let mut measured = Vec::new();
    for profile in [V1, V20, V37, V38] {
        measured.extend(measure("portable", PORTABLE, profile));
    }
    for profile in [V37, V38] {
        measured.extend(measure("literal 1", &literal(1), profile));
        measured.extend(measure("literal 30000", &literal(30_000), profile));
        measured.extend(measure("records", RECORDS, profile));
        measured.extend(measure("json", JSON, profile));
        measured.extend(measure(
            "unknown assertions",
            include_str!("unknown_cases/assertions.ts"),
            profile,
        ));
    }
    // Only v38 holds 90000 units (in its static extension).
    measured.extend(measure("literal 90000", &literal(90_000), V38));
    for (name, fuel) in &measured {
        println!("    (\"{name}\", {fuel}),");
    }
    let mismatches: Vec<_> = measured
        .iter()
        .filter(|(name, fuel)| {
            GOLDEN
                .iter()
                .find(|(golden, _)| golden == name)
                .is_none_or(|(_, golden)| golden != fuel)
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "fuel differs from the golden numbers: {mismatches:?}"
    );
    assert_eq!(GOLDEN.len(), measured.len());
}

/// A compiled module with a Wasm start function calling `_start` (legal
/// before the exception profiles; the compiler never emits one).
fn with_start_function(bytes: &[u8]) -> Vec<u8> {
    let (index, end) = wasmparser::Parser::new(0)
        .parse_all(bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::ExportSection(reader) => {
                let end = reader.range().end;
                let index = reader
                    .into_iter()
                    .map(Result::unwrap)
                    .find(|export| export.name == abi::START_EXPORT)
                    .map(|export| export.index);
                Some((index.unwrap(), end))
            }
            _ => None,
        })
        .unwrap();
    let index = u8::try_from(index).unwrap();
    assert!(index < 0x80, "a one-byte LEB128 index");
    let mut patched = bytes.to_vec();
    // The start section (id 8, one byte of contents: the function index)
    // directly follows the export section.
    patched.splice(end..end, [8, 1, index]);
    patched
}

/// Owner decision: instantiation never touches guest fuel, in every
/// profile, however much static data the module initializes and however
/// little fuel the guest has. A module with a Wasm start function runs guest
/// code during instantiation and keeps paying for it, exactly: without
/// copy-on-write memory images its data-segment copy is the same on every
/// host (with them, 34641 on macOS arm64 against 18257 on Linux).
#[test]
fn instantiation_never_touches_guest_fuel() {
    let mut modules: Vec<_> = SUPPORTED_PROFILES
        .iter()
        .map(|profile| compile(PORTABLE, *profile))
        .collect();
    modules.push(compile(&literal(30_000), V37));
    modules.push(compile(&literal(90_000), V38));
    let runtime = VartaRuntime::new();
    for bytes in &modules {
        assert_eq!(instantiation_fuel(bytes), 0);
        // One unit of guest fuel suffices and is left untouched.
        let module = runtime.prepare_module(bytes).unwrap();
        let mut execution = execution::Execution::new(&runtime.engine, &[], None).unwrap();
        execution.store.set_fuel(1).unwrap();
        execution.instantiate_prepared(&module).unwrap();
        assert_eq!(execution.store.get_fuel().unwrap(), 1);
    }
    let started = with_start_function(&compile(PORTABLE, V1));
    assert!(runtime.prepare_module(&started).unwrap().start_function);
    assert_eq!(instantiation_fuel(&started), 18_303);
    // Exception profiles reject a start function before compilation.
    let refused = runtime.prepare_module(&with_start_function(&compile(PORTABLE, V20)));
    assert!(
        refused
            .err()
            .is_some_and(|e| e.to_string().contains("forbids a Wasm start function"))
    );
}

/// The engine configuration that fuel determinism rests on: a module with a
/// start function pays its instantiation, which copy-on-write memory images
/// would make host-dependent.
#[test]
fn engine_never_uses_copy_on_write_images() {
    let runtime = VartaRuntime::new();
    assert!(!runtime.engine.get_memory_init_cow());
}

/// Compiling the same source twice yields the same bytes, for every case
/// program under v37 and v38 and a portable program under every profile.
#[test]
fn compilation_is_byte_identical_across_runs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("_cases"))
        .flat_map(|dir| std::fs::read_dir(dir.path()).unwrap().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "ts"))
        .collect();
    files.sort();
    let mut compared = 0;
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        for profile in [V37, V38] {
            let first = varta_compiler::compile_typescript_with_profile(&source, profile);
            let second = varta_compiler::compile_typescript_with_profile(&source, profile);
            match (first, second) {
                (Ok(first), Ok(second)) => {
                    assert!(first == second, "{} {profile:?}", path.display());
                    compared += 1;
                }
                (Err(first), Err(second)) => {
                    assert_eq!(first.to_string(), second.to_string(), "{}", path.display());
                }
                _ => panic!("{} {profile:?}: acceptance differs", path.display()),
            }
        }
    }
    assert!(compared >= 300, "only {compared} modules compared");
    for profile in SUPPORTED_PROFILES {
        let first = varta_compiler::compile_typescript_with_profile(PORTABLE, *profile).unwrap();
        let second = varta_compiler::compile_typescript_with_profile(PORTABLE, *profile).unwrap();
        assert!(first == second, "{profile:?}");
    }
}
