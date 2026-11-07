//! V38 integrity levels (`integrity_levels`, K9 D2): `Object.freeze`,
//! `Object.seal`, `Object.preventExtensions` and their predicates over the
//! value18 integrity header bits, and the `TypeError` of every failed write,
//! addition and deletion (`integrity_cases/*.ts`). Every `*.expected` file
//! is the output of Node 24.20.0 running the same program with
//! `await main();` appended, recorded once and checked in. Every program
//! runs under the production budgets and again in collector stress builds;
//! v37 keeps refusing the members, and a program that names none of them
//! gets no write check.
use super::*;
use crate::codegen_v2_tests::run_test_budgets;
use crate::collector_stress_tests::stressed;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
/// Test-only fuel budgets of one stress run, in production fuel budgets
/// (as in `standard_library_tests`).
const STRESS_BUDGETS: u64 = 400;
/// Work units per allocation of the stress builds.
const STRESS_UNITS: [u32; 2] = [2, 64];

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("integrity_cases/", $name, ".ts")),
            include_str!(concat!("integrity_cases/", $name, ".expected")),
        )
    };
}

const CASES: &[(&str, &str, &str)] = &[
    case!("records"),
    case!("arrays"),
    case!("objects"),
    case!("members"),
    case!("guide"),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn printed(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .flat_map(|line| {
            line.strip_prefix("varta log: ")
                .unwrap_or_else(|| panic!("not a log line: {line}"))
                .split('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn logs(source: &str) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_in(V38, source), &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    printed(&output.lines)
}

fn matches_node(name: &str) {
    let (_, source, expected) = CASES
        .iter()
        .find(|(case, ..)| *case == name)
        .unwrap_or_else(|| panic!("missing case {name}"));
    let expected: Vec<&str> = expected.lines().collect();
    assert_eq!(logs(source), expected, "{name}");
}

fn refused(profile: LanguageProfile, source: &str, diagnostic: &str) {
    let error = varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} compiled\n{source}"))
        .to_string();
    assert!(error.contains(diagnostic), "{profile:?}: {error}\n{source}");
}

#[test]
fn record_integrity_matches_node() {
    matches_node("records");
}

#[test]
fn array_integrity_matches_node() {
    matches_node("arrays");
}

#[test]
fn object_integrity_matches_node() {
    matches_node("objects");
}

#[test]
fn member_integrity_matches_node() {
    matches_node("members");
}

/// The `docs/language-guide.md` example, verbatim.
#[test]
fn integrity_guide_example_matches_node() {
    matches_node("guide");
}

/// Integrity bits survive every collection and never reach a fresh block:
/// the stress builds print what the normal build prints.
#[test]
fn integrity_survives_collector_stress() {
    for (name, source, expected) in CASES {
        let expected: Vec<String> = expected.lines().map(str::to_owned).collect();
        for units in STRESS_UNITS {
            let run = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
            assert_eq!(
                (&printed(&run.lines), &run.error),
                (&expected, &None),
                "{name}: stress build ({units} units per allocation)"
            );
        }
    }
}

/// V37 keeps refusing the six members.
#[test]
fn integrity_members_stay_refused_by_v37() {
    for member in [
        "freeze",
        "seal",
        "preventExtensions",
        "isFrozen",
        "isSealed",
        "isExtensible",
    ] {
        refused(
            V37,
            &format!(
                "export function main(): void {{ const o = {{ a: 1 }}; console.log(String(Object.{member}(o))); }}"
            ),
            "unsupported call",
        );
    }
}

/// Accessor cache refreshes are internal writes even when the object is frozen.
#[test]
fn frozen_accessor_caches_refresh_without_admitting_user_writes() {
    let source = r#"
export function main(): void {
  let stored = 1;
  const accessor = {
    get value(): number { return stored; },
    set value(next: number) { stored = next; },
  };
  Object.seal(accessor);
  console.log(Object.isFrozen(accessor));
  console.log(JSON.stringify(accessor));
  Object.freeze(accessor);
  accessor.value = 5;
  console.log(JSON.stringify(accessor));
  const getterOnly = { get value(): number { return stored; } };
  Object.freeze(getterOnly);
  try { (getterOnly as { value: number }).value = 10; } catch (error) {
    console.log(error instanceof TypeError);
  }
  console.log(JSON.stringify(getterOnly));
}
"#;
    let expected = [
        "true",
        r#"{"value":1}"#,
        r#"{"value":5}"#,
        "true",
        r#"{"value":5}"#,
    ];
    assert_eq!(logs(source), expected);
    for units in STRESS_UNITS {
        let run = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
        assert_eq!(printed(&run.lines), expected);
        assert!(run.error.is_none(), "{:?}", run.error);
    }
}
