//! Documentation tables that restate registry facts are generated from the
//! registries and compared here, so a stale count or version fails the gate.
use std::path::{Path, PathBuf};
use varta_common::{abi, json, language, launcher::LauncherInfo, scheduler, value};

fn workspace_file(relative: &str) -> (PathBuf, String) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    // Test-only read of a checked-in document; a missing file is a test failure.
    let text = std::fs::read_to_string(&path).unwrap();
    (path, text)
}

/// Returns the text between `<!-- generated:NAME:begin ... -->` and its end marker.
fn block<'a>(text: &'a str, name: &str) -> &'a str {
    let begin = format!("<!-- generated:{name}:begin");
    let end = format!("<!-- generated:{name}:end -->");
    let start = text
        .find(&begin)
        .unwrap_or_else(|| panic!("missing {begin}"));
    let body = start + text[start..].find("-->").unwrap() + "-->".len();
    let stop = text[body..]
        .find(&end)
        .unwrap_or_else(|| panic!("missing {end}"));
    text[body..body + stop].trim()
}

fn assert_block(relative: &str, name: &str, expected: &str) {
    let (path, text) = workspace_file(relative);
    let actual = block(&text, name);
    assert!(
        actual == expected.trim(),
        "{} block `{name}` is stale; replace it with:\n\n{}\n",
        path.display(),
        expected.trim()
    );
}

fn profile_name(profile: language::LanguageProfile) -> String {
    // Infallible: the profile enum serializes to its registered string name.
    serde_json::to_value(profile)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn version_matrix() -> String {
    let mut rows = vec![
        "| Profile | Host ABI | Value ABI | Exception export | JSON stringify | JSON parse |"
            .to_owned(),
        "| --- | --- | --- | --- | --- | --- |".to_owned(),
    ];
    for &profile in language::SUPPORTED_PROFILES {
        let exception = if profile.exceptions() {
            let (export, channel) = profile.exception_channel();
            format!("`{export}` = {channel}")
        } else {
            "—".to_owned()
        };
        rows.push(format!(
            "| `{}` | {} | {} | {exception} | `{}` | `{}` |",
            profile_name(profile),
            profile.host_abi(),
            profile.value_abi(),
            profile.json_stringify().name(),
            profile.json_parse().name(),
        ));
    }
    rows.join("\n")
}

fn launcher_summary() -> String {
    let info = LauncherInfo::current();
    format!(
        "Launcher protocol **{}** advertises host ABI **{}**, value ABI **{}**, HTTP contract **{}** and **{}** language profiles (the rows above).",
        info.protocol_version,
        info.host_abi,
        info.value_abi,
        info.http_contract,
        info.language_profiles.len()
    )
}

fn limits() -> String {
    let mib = |bytes: usize| format!("{} MiB", bytes / (1024 * 1024));
    let kib = |bytes: usize| format!("{} KiB", bytes / 1024);
    let rows: Vec<(&str, String, &str)> = vec![
        (
            "Guest linear memory",
            mib(abi::MAX_MEMORY_BYTES),
            "abi::MAX_MEMORY_BYTES",
        ),
        (
            "Guest instructions (fuel)",
            abi::FUEL.to_string(),
            "abi::FUEL",
        ),
        (
            "Execution/HTTP deadline",
            format!("{} s", abi::DEADLINE_SECONDS),
            "abi::DEADLINE_SECONDS",
        ),
        (
            "Host calls per execution",
            abi::MAX_HOST_CALLS.to_string(),
            "abi::MAX_HOST_CALLS",
        ),
        (
            "Captured output",
            mib(abi::MAX_OUTPUT_BYTES),
            "abi::MAX_OUTPUT_BYTES",
        ),
        (
            "Host input string",
            kib(abi::MAX_INPUT_BYTES),
            "abi::MAX_INPUT_BYTES",
        ),
        (
            "One host result / one string value",
            mib(abi::MAX_RESULT_BYTES),
            "abi::MAX_RESULT_BYTES",
        ),
        ("Wasm file", mib(abi::MAX_WASM_BYTES), "abi::MAX_WASM_BYTES"),
        (
            "Wasm function body",
            format!("{} bytes", abi::MAX_WASM_FUNCTION_BODY_BYTES),
            "abi::MAX_WASM_FUNCTION_BODY_BYTES",
        ),
        (
            "Distinct Wasm signatures",
            abi::MAX_WASM_SIGNATURES.to_string(),
            "abi::MAX_WASM_SIGNATURES",
        ),
        (
            "Physical function parameters",
            abi::MAX_WASM_PARAMETERS.to_string(),
            "abi::MAX_WASM_PARAMETERS",
        ),
        (
            "User functions",
            abi::MAX_USER_FUNCTIONS.to_string(),
            "abi::MAX_USER_FUNCTIONS",
        ),
        (
            "HTTP adapters",
            abi::MAX_HTTP_HANDLERS.to_string(),
            "abi::MAX_HTTP_HANDLERS",
        ),
        (
            "Compiler helpers (legacy / sparse / maximum)",
            format!(
                "{} / {} / {}",
                abi::LEGACY_INTERNAL_FUNCTIONS,
                abi::SPARSE_INTERNAL_FUNCTIONS,
                abi::MAX_INTERNAL_FUNCTIONS
            ),
            "abi::*_INTERNAL_FUNCTIONS",
        ),
        (
            "Record fields",
            value::MAX_RECORD_FIELDS.to_string(),
            "value::MAX_RECORD_FIELDS",
        ),
        (
            "Record shapes per graph",
            value::MAX_RECORD_SHAPES.to_string(),
            "value::MAX_RECORD_SHAPES",
        ),
        (
            "Record key bytes",
            value::MAX_RECORD_KEY_BYTES.to_string(),
            "value::MAX_RECORD_KEY_BYTES",
        ),
        (
            "Array elements / spread arguments",
            value::MAX_ARRAY_ELEMENTS.to_string(),
            "value::MAX_ARRAY_ELEMENTS",
        ),
        (
            "Array element types per graph",
            value::MAX_ARRAY_TYPES.to_string(),
            "value::MAX_ARRAY_TYPES",
        ),
        (
            "Union types",
            value::MAX_UNION_TYPES.to_string(),
            "value::MAX_UNION_TYPES",
        ),
        (
            "JSON depth / visited values",
            format!("{} / {}", value::MAX_JSON_DEPTH, value::MAX_JSON_NODES),
            "value::MAX_JSON_*",
        ),
        (
            "JSON schema types (legacy / current)",
            format!(
                "{} / {}",
                json::LEGACY_MAX_SCHEMA_TYPES,
                json::MAX_SCHEMA_TYPES
            ),
            "json::*MAX_SCHEMA_TYPES",
        ),
        (
            "Language metadata",
            format!("{} bytes", language::MAX_METADATA_BYTES),
            "language::MAX_METADATA_BYTES",
        ),
        (
            "Live continuation frames",
            varta_common::function::frame::MAX_LIVE_FRAMES.to_string(),
            "frame::MAX_LIVE_FRAMES",
        ),
        (
            "Pending microtask continuations (v34)",
            scheduler::MAX_PENDING_CONTINUATIONS.to_string(),
            "scheduler::MAX_PENDING_CONTINUATIONS",
        ),
        (
            "Microtasks per execution (v34)",
            scheduler::MAX_EXECUTION_JOBS.to_string(),
            "scheduler::MAX_EXECUTION_JOBS",
        ),
        (
            "Promise serials / rejection ids per execution (v34)",
            scheduler::MAX_PROMISE_SERIALS.to_string(),
            "scheduler::MAX_PROMISE_SERIALS",
        ),
        (
            "Unhandled-rejection reasons kept / units per field / bytes each",
            format!(
                "{} / {} / {}",
                scheduler::MAX_RETAINED_REJECTION_REASONS,
                scheduler::MAX_REJECTION_REASON_UNITS,
                scheduler::MAX_REJECTION_REASON_BYTES
            ),
            "scheduler::MAX_*REJECTION_REASON*",
        ),
        (
            "Pending timers (v36)",
            scheduler::MAX_PENDING_TIMERS.to_string(),
            "scheduler::MAX_PENDING_TIMERS",
        ),
    ];
    let mut lines = vec![
        "| Resource | Limit | Source constant |".to_owned(),
        "| --- | --- | --- |".to_owned(),
    ];
    lines.extend(
        rows.into_iter()
            .map(|(resource, limit, constant)| format!("| {resource} | {limit} | `{constant}` |")),
    );
    lines.join("\n")
}

/// Profile indexes carry hand-written summaries, but must list exactly the
/// registered profiles, in registry order.
fn assert_profile_index(relative: &str) {
    let (path, text) = workspace_file(relative);
    let listed: Vec<String> = block(&text, "profile-index")
        .lines()
        .filter(|line| line.starts_with("| `"))
        .map(|line| line[3..].split('`').next().unwrap_or_default().to_owned())
        .collect();
    let registered: Vec<String> = language::SUPPORTED_PROFILES
        .iter()
        .map(|&profile| profile_name(profile))
        .collect();
    assert_eq!(listed, registered, "{} profile index", path.display());
}

#[test]
fn architecture_version_matrix_matches_registries() {
    assert_block("docs/architecture.md", "version-matrix", &version_matrix());
    assert_block(
        "docs/architecture.md",
        "launcher-summary",
        &launcher_summary(),
    );
}

#[test]
fn architecture_limits_match_constants() {
    assert_block("docs/architecture.md", "limits", &limits());
}

#[test]
fn profile_indexes_list_every_registered_profile() {
    assert_profile_index("README.md");
    assert_profile_index("CLAUDE.md");
}
