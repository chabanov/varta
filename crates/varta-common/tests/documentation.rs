//! Documentation tables that restate registry facts are generated from the
//! registries and compared here, so a stale count or version fails the gate.
mod support;

use support::{assert_block, block, profile_name, table, workspace_file};
use varta_common::{abi, json, language, launcher::LauncherInfo, limits, scheduler, value};

fn version_matrix() -> String {
    let mut rows = Vec::new();
    for &profile in language::SUPPORTED_PROFILES {
        let exception = if profile.exceptions() {
            let (export, channel) = profile.exception_channel();
            format!("`{export}` = {channel}")
        } else {
            "—".to_owned()
        };
        rows.push(vec![
            format!("`{}`", profile_name(profile)),
            profile.host_abi().to_string(),
            profile.value_abi().to_string(),
            exception,
            format!("`{}`", profile.json_stringify().name()),
            format!("`{}`", profile.json_parse().name()),
        ]);
    }
    table(
        &[
            "Profile",
            "Host ABI",
            "Value ABI",
            "Exception export",
            "JSON stringify",
            "JSON parse",
        ],
        rows,
    )
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
            "Linked intrinsic stack in memory 0 (v38 `linked_intrinsics`)",
            kib(abi::INTRINSIC_STACK_BYTES),
            "abi::INTRINSIC_STACK_BYTES",
        ),
        (
            "Guest instructions (fuel), v1–v37 per execution",
            abi::FUEL.to_string(),
            "abi::FUEL",
        ),
        (
            "Guest instructions (fuel), v38 per turn (`turn_budgets`)",
            varta_common::budget::TURN_FUEL.to_string(),
            "budget::TURN_FUEL",
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
    table(
        &["Resource", "Limit", "Source constant"],
        rows.into_iter().map(|(resource, limit, constant)| {
            vec![resource.to_owned(), limit, format!("`{constant}`")]
        }),
    )
}

/// Structural program limits per profile family, from `varta_common::limits`.
fn program_limits() -> String {
    fn count(n: usize) -> String {
        n.to_string()
    }
    fn bytes(n: usize) -> String {
        format!("{n} bytes")
    }
    type Row = (
        &'static str,
        fn(&limits::ProgramLimits) -> String,
        &'static str,
    );
    let rows: Vec<Row> = vec![
        (
            "User functions",
            |l| count(l.user_functions),
            "user_functions",
        ),
        (
            "Wasm functions (defined and imported; frozen: defined, maximum helper allowance)",
            |l| count(l.wasm_functions),
            "wasm_functions",
        ),
        (
            "Distinct Wasm signatures",
            |l| count(l.wasm_signatures),
            "wasm_signatures",
        ),
        ("Wasm module", |l| bytes(l.wasm_bytes), "wasm_bytes"),
        (
            "Wasm function body",
            |l| bytes(l.function_body_bytes),
            "function_body_bytes",
        ),
        (
            "Record shapes / classes",
            |l| count(l.record_shapes),
            "record_shapes",
        ),
        ("Union types", |l| count(l.union_types), "union_types"),
        (
            "Structural members of one union",
            |l| count(l.union_members),
            "union_members",
        ),
        (
            "Array element types",
            |l| count(l.array_types),
            "array_types",
        ),
        (
            "`Promise<T>` types",
            |l| count(l.promise_types),
            "promise_types",
        ),
        (
            "Generator types",
            |l| count(l.generator_types),
            "generator_types",
        ),
        (
            "Function types",
            |l| count(l.function_types),
            "function_types",
        ),
        (
            "JSON schema types (legacy / current schemas)",
            |l| format!("{} / {}", l.schema_types(false), l.schema_types(true)),
            "schema_types()",
        ),
        ("JSON schema", |l| bytes(l.schema_bytes), "schema_bytes"),
        (
            "Members of one JSON schema union",
            |l| count(l.json_union_members),
            "json_union_members",
        ),
        (
            "Function capture types",
            |l| count(l.capture_types()),
            "capture_types()",
        ),
        (
            "Static extension pages",
            |l| count(l.static_extension_pages as usize),
            "static_extension_pages",
        ),
    ];
    table(
        &[
            "Limit",
            "v1–v37 (`limits::FROZEN`)",
            "v38 (`limits::WASM`)",
            "Field",
        ],
        rows.into_iter().map(|(name, value, field)| {
            vec![
                name.to_owned(),
                value(&limits::FROZEN),
                value(&limits::WASM),
                format!("`{field}`"),
            ]
        }),
    )
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
    assert_block("docs/architecture.md", "program-limits", &program_limits());
}

#[test]
fn profile_indexes_list_every_registered_profile() {
    assert_profile_index("README.md");
    assert_profile_index("CLAUDE.md");
}
