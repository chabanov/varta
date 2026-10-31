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
/// (`fuel_is_identical_on_every_host --nocapture` prints the table). Before
/// copy-on-write images were disabled, every v1–v37 entry differed by host:
/// macOS arm64 charged the copy over 16 KiB pages (`literal 1` v37
/// instantiation 16385) and Linux skipped it (0).
const GOLDEN: &[(&str, u64)] = &[
    ("portable CheckedI32Utf8V1 instantiation", 47),
    ("portable CheckedI32Utf8V1 run", 18302),
    ("portable CheckedI32Utf8V1 rated", 18433),
    ("portable CheckedF64Utf16V20 instantiation", 90),
    ("portable CheckedF64Utf16V20 run", 18875),
    ("portable CheckedF64Utf16V20 rated", 19087),
    ("portable CheckedF64Utf16V37 instantiation", 90),
    ("portable CheckedF64Utf16V37 run", 23783),
    ("portable CheckedF64Utf16V37 rated", 23995),
    ("portable CheckedF64Utf16V38 instantiation", 0),
    ("portable CheckedF64Utf16V38 run", 6151),
    ("portable CheckedF64Utf16V38 rated", 6151),
    ("literal 1 CheckedF64Utf16V37 instantiation", 7),
    ("literal 1 CheckedF64Utf16V37 run", 84),
    ("literal 1 CheckedF64Utf16V37 rated", 136),
    ("literal 30000 CheckedF64Utf16V37 instantiation", 60005),
    ("literal 30000 CheckedF64Utf16V37 run", 60082),
    ("literal 30000 CheckedF64Utf16V37 rated", 60134),
    ("records CheckedF64Utf16V37 instantiation", 111),
    ("records CheckedF64Utf16V37 run", 84506),
    ("records CheckedF64Utf16V37 rated", 88776),
    ("json CheckedF64Utf16V37 instantiation", 498),
    ("json CheckedF64Utf16V37 run", 5100),
    ("json CheckedF64Utf16V37 rated", 58178),
    ("unknown assertions CheckedF64Utf16V37 instantiation", 1639),
    ("unknown assertions CheckedF64Utf16V37 run", 24319),
    ("unknown assertions CheckedF64Utf16V37 rated", 26577),
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

/// Earlier profiles pay exactly one fuel per byte of their data segments at
/// instantiation, on every host: the copy is never skipped (copy-on-write
/// images) nor rounded to host pages.
#[test]
fn frozen_instantiation_pays_exactly_the_segment_bytes() {
    let segment_bytes = |bytes: &[u8]| -> u64 {
        wasmparser::Parser::new(0)
            .parse_all(bytes)
            .filter_map(|payload| match payload.unwrap() {
                wasmparser::Payload::DataSection(reader) => Some(
                    reader
                        .into_iter()
                        .map(|data| data.unwrap().data.len() as u64)
                        .sum::<u64>(),
                ),
                _ => None,
            })
            .sum()
    };
    let small = compile(&literal(1), V37);
    let large = compile(&literal(30_000), V37);
    let growth = instantiation_fuel(&large) - instantiation_fuel(&small);
    assert_eq!(growth, segment_bytes(&large) - segment_bytes(&small));
    // 29999 more UTF-16 code units.
    assert_eq!(growth, 2 * 29_999);
    // V38 instantiation is charged to its own allowance, never the guest.
    assert_eq!(instantiation_fuel(&compile(&literal(30_000), V38)), 0);
}

/// The engine configuration that fuel determinism rests on.
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
