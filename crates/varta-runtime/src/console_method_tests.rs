//! V38 console methods (`console_methods`, K9 D1): standard error through
//! `log_stderr16` and the rest of Node's console (`console_method_cases/*.ts`).
//! Every `*.stdout`/`*.stderr` pair is what Node 24.20.0 prints running the
//! same program with `main();` appended, under `--stack-trace-limit=0` and
//! `TZ=UTC`, recorded once and checked in. Node prints its process warnings
//! (`countReset` of an unknown label, the timer warnings) asynchronously
//! with its process id; Varta prints them synchronously as `Warning: …`
//! lines on standard error, which the tests check on their own.
use super::*;
use varta_common::console::{LOG_PREFIX, StderrLevel};
use varta_common::language::LanguageProfile;
use varta_common::{CapabilityGrant, HostOperation};

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
/// Varta's form of a Node process warning.
const WARNING: &str = "Warning: ";

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("console_method_cases/", $name, ".ts")),
            include_str!(concat!("console_method_cases/", $name, ".stdout")),
            include_str!(concat!("console_method_cases/", $name, ".stderr")),
        )
    };
}

const CASES: &[(&str, &str, &str, &str)] = &[
    case!("streams"),
    case!("groups"),
    // The `docs/language-guide.md` example, verbatim.
    case!("guide"),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

/// The printed lines of a run: standard output, standard error with each
/// line's level, in order.
#[derive(Debug, Default)]
struct Printed {
    stdout: Vec<String>,
    stderr: Vec<(StderrLevel, String)>,
}

fn printed(lines: &[String]) -> Printed {
    let mut printed = Printed::default();
    for line in lines {
        let split = |text: &str| text.split('\n').map(str::to_owned).collect::<Vec<_>>();
        if let Some(text) = line.strip_prefix(LOG_PREFIX) {
            printed.stdout.extend(split(text));
            continue;
        }
        let level = StderrLevel::ALL
            .into_iter()
            .find(|level| line.starts_with(level.prefix()))
            .unwrap_or_else(|| panic!("not a console line: {line}"));
        for text in split(&line[level.prefix().len()..]) {
            printed.stderr.push((level, text));
        }
    }
    printed
}

fn run_in(profile: LanguageProfile, source: &str, grants: &[CapabilityGrant]) -> Printed {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_in(profile, source), grants)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    printed(&output.lines)
}

fn case(name: &str) -> (&'static str, &'static str, &'static str) {
    let (_, source, stdout, stderr) = CASES
        .iter()
        .find(|(case, ..)| *case == name)
        .unwrap_or_else(|| panic!("missing case {name}"));
    (source, stdout, stderr)
}

/// The standard error text without Varta's process warnings, and the
/// warnings.
fn without_warnings(printed: &Printed) -> (Vec<&str>, Vec<&str>) {
    let (warnings, lines): (Vec<_>, Vec<_>) = printed
        .stderr
        .iter()
        .map(|(_, text)| text.as_str())
        .partition(|text| text.starts_with(WARNING));
    (lines, warnings)
}

#[test]
fn console_methods_print_as_node_on_both_streams() {
    for (name, ..) in CASES {
        let (source, stdout, stderr) = case(name);
        let printed = run_in(V38, source, &[]);
        assert_eq!(printed.stdout, stdout.lines().collect::<Vec<_>>(), "{name}");
        let (lines, _) = without_warnings(&printed);
        assert_eq!(lines, stderr.lines().collect::<Vec<_>>(), "{name}");
    }
}

#[test]
fn standard_error_lines_carry_the_method_level() {
    let printed = run_in(V38, case("streams").0, &[]);
    let level_of = |text: &str| {
        printed
            .stderr
            .iter()
            .find(|(_, line)| line == text)
            .map(|(level, _)| *level)
    };
    assert_eq!(level_of("err 1 { a: [ 1, 2 ] }"), Some(StderrLevel::Error));
    assert_eq!(level_of("warn x"), Some(StderrLevel::Warn));
    assert_eq!(level_of("Trace: x 1"), Some(StderrLevel::Error));
    assert_eq!(level_of("Assertion failed: yes 5"), Some(StderrLevel::Warn));
}

#[test]
fn process_warnings_are_standard_error_lines_outside_groups() {
    let printed = run_in(V38, case("groups").0, &[]);
    let (_, warnings) = without_warnings(&printed);
    assert_eq!(warnings, ["Warning: Count for 'nope' does not exist"]);
    assert!(
        printed
            .stderr
            .iter()
            .any(|(level, text)| *level == StderrLevel::Warn && text.starts_with(WARNING))
    );
}

/// The timers read the `time.now` clock: durations vary, so the text is
/// checked by its form (`formatTime` of whole milliseconds).
#[test]
fn timers_print_labels_durations_and_warnings() {
    let source = r#"
function main(): void {
  console.time();
  console.time();
  console.time("t");
  console.timeLog("t", "extra", { a: 1 });
  console.timeLog();
  console.timeEnd("t");
  console.timeEnd("t");
  console.timeLog("nope");
  console.group();
  console.timeEnd();
}
"#;
    let grant = CapabilityGrant {
        id: "time.now".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    };
    let printed = run_in(V38, source, &[grant]);
    let duration = |line: &str, label: &str, rest: &str| {
        let text = line
            .strip_prefix(label)
            .and_then(|text| text.strip_suffix(rest))
            .unwrap_or_else(|| panic!("{line}"));
        let number = text.strip_suffix("ms").unwrap_or_else(|| panic!("{line}"));
        assert!(number.parse::<u64>().is_ok(), "{line}");
    };
    let [extra, default, end, nested] = printed.stdout.as_slice() else {
        panic!("{printed:?}");
    };
    duration(extra, "t: ", " extra { a: 1 }");
    duration(default, "default: ", "");
    duration(end, "t: ", "");
    duration(nested, "  default: ", "");
    let (lines, warnings) = without_warnings(&printed);
    assert!(lines.is_empty(), "{lines:?}");
    assert_eq!(
        warnings,
        [
            "Warning: Label 'default' already exists for console.time()",
            "Warning: No such label 't' for console.timeEnd()",
            "Warning: No such label 'nope' for console.timeLog()",
        ]
    );
}

#[test]
fn timers_need_the_time_grant() {
    let bytes = compile_in(V38, "function main(): void { console.time(); }");
    // The denied clock reports itself as `Date.now` does.
    let output = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    assert!(
        output
            .lines
            .iter()
            .any(|line| line.contains("capability denied: time.now")),
        "{output:?}"
    );
}

#[test]
fn programs_without_stateful_methods_write_directly() {
    // No console prelude: `console.error` and `console.warn` import
    // `log_stderr16` and the module defines no console state.
    let bytes = compile_in(
        V38,
        r#"function main(): void { console.error("e"); console.warn("w", 1); }"#,
    );
    let module = wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert!(
        module
            .imports()
            .any(|import| import.name() == HostOperation::LogStderr16.name())
    );
    let printed = run_in(
        V38,
        r#"function main(): void { console.error("e"); console.warn("w", 1); }"#,
        &[],
    );
    assert_eq!(
        printed.stderr,
        [
            (StderrLevel::Error, "e".to_owned()),
            (StderrLevel::Warn, "w 1".to_owned())
        ]
    );
}

#[test]
fn earlier_profiles_refuse_the_console_methods() {
    for method in [
        "error",
        "warn",
        "trace",
        "assert",
        "dir",
        "table",
        "group",
        "groupEnd",
        "count",
        "countReset",
        "time",
        "timeEnd",
        "timeLog",
        "dirxml",
    ] {
        let source = format!("function main(): void {{ console.{method}(\"x\"); }}");
        assert!(
            varta_compiler::compile_typescript_with_profile(&source, V37).is_err(),
            "{method}"
        );
    }
}
