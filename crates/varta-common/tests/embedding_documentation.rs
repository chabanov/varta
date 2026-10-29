//! The third-party embedding contract (`docs/embedding.md`) restates registry
//! facts: host operations, profile admission, memory layout, reserved names,
//! status codes and entry-point limits. Its tables are generated here from the
//! registries, so a registry change fails the gate until the document follows.
mod support;
#[path = "embedding_documentation/view.rs"]
mod view;

use std::collections::BTreeSet;
use support::{assert_block, profile_name, table, workspace_file};
use varta_common::{
    HostOperation, HostScalar, TOOL_COPY_STRING, TOOL_EXPORT_PREFIX, TOOL_RETAIN_STRING,
    TOOL_SECTION, ToolCatalog, ToolContract, ToolLimits, ToolParameter, ToolType,
    abi::{self, HostStatus},
    exceptions, function,
    function::admission,
    function::frame::{self, Completion, Outcome},
    http,
    language::{self, LanguageProfile},
    value,
};

const DOCUMENT: &str = "docs/embedding.md";
const HOST_REGISTRY: &str = "crates/varta-common/src/host.rs";

/// Every import name the runtime can link. Import validation recognizes a
/// name only through `HostOperation::from_name`, so its match arms are the
/// complete wire-name set; each must round-trip through `name()`.
fn registered_operations() -> Vec<HostOperation> {
    let (path, source) = workspace_file(HOST_REGISTRY);
    let start = source
        .find("pub fn from_name(")
        .unwrap_or_else(|| panic!("{} has no from_name", path.display()));
    let end = start
        + source[start..]
            .find("_ => return None")
            .unwrap_or_else(|| panic!("{} from_name has no fallback arm", path.display()));
    let mut seen = BTreeSet::new();
    let operations: Vec<HostOperation> = source[start..end]
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix('"')?;
            Some(&rest[..rest.find('"')?])
        })
        .map(|name| {
            let operation = HostOperation::from_name(name)
                .unwrap_or_else(|| panic!("from_name does not accept its own arm {name}"));
            assert_eq!(operation.name(), name, "{name} does not round-trip");
            assert!(seen.insert(operation), "{name} is registered twice");
            operation
        })
        .collect();
    assert!(!operations.is_empty(), "no host operations found");
    operations
}

/// The JSON imports bound to some profile; each is admitted only by the
/// profiles that select it (runtime preflight rejects any other pairing).
fn json_operations() -> BTreeSet<HostOperation> {
    language::SUPPORTED_PROFILES
        .iter()
        .flat_map(|profile| [profile.json_parse(), profile.json_stringify()])
        .collect()
}

/// Whether a module declaring `profile` may import `operation`: the import's
/// minimum ABI fits the profile's host ABI, the profile selects exactly this
/// encoding/layout variant, JSON imports match the profile's pair, and the
/// HTTP feature marker needs a profile that admits HTTP contracts.
fn admitted(
    operation: HostOperation,
    profile: LanguageProfile,
    json: &BTreeSet<HostOperation>,
) -> bool {
    if operation.minimum_abi() > profile.host_abi()
        || profile.host_operation(operation) != operation
    {
        return false;
    }
    if json.contains(&operation) {
        return operation == profile.json_parse() || operation == profile.json_stringify();
    }
    if operation == HostOperation::HttpService {
        return entry_points(profile);
    }
    true
}

/// Tool and HTTP contracts are rejected in every binary64 profile.
fn entry_points(profile: LanguageProfile) -> bool {
    !profile.binary64()
}

fn scalar(value: &HostScalar) -> &'static str {
    match value {
        HostScalar::I32 => "i32",
        HostScalar::I64 => "i64",
        HostScalar::F64 => "f64",
    }
}

fn signature(operation: HostOperation) -> String {
    let (params, results) = operation.scalar_signature();
    let list = |values: &[HostScalar]| values.iter().map(scalar).collect::<Vec<_>>().join(", ");
    format!("({}) → ({})", list(params), list(results))
}

/// Contiguous runs of admitting profiles, in registry order.
fn admitting_ranges(operation: HostOperation, json: &BTreeSet<HostOperation>) -> String {
    let profiles = language::SUPPORTED_PROFILES;
    let mut ranges: Vec<String> = Vec::new();
    let mut index = 0;
    while index < profiles.len() {
        if !admitted(operation, profiles[index], json) {
            index += 1;
            continue;
        }
        let first = index;
        while index + 1 < profiles.len() && admitted(operation, profiles[index + 1], json) {
            index += 1;
        }
        ranges.push(if first == index {
            format!("`{}`", profile_name(profiles[first]))
        } else {
            format!(
                "`{}` – `{}`",
                profile_name(profiles[first]),
                profile_name(profiles[index])
            )
        });
        index += 1;
    }
    if ranges.is_empty() {
        "—".to_owned()
    } else {
        ranges.join(", ")
    }
}

fn host_operations() -> String {
    let json = json_operations();
    let mut operations = registered_operations();
    operations.sort_by_key(|operation| (operation.minimum_abi(), operation.name()));
    let flag = |set: bool| if set { "yes" } else { "—" }.to_owned();
    table(
        &[
            "Import `varta.<name>`",
            "Signature",
            "Min host ABI",
            "Capability",
            "Result globals",
            "String in host buffer",
            "Admitted under",
        ],
        operations.into_iter().map(|operation| {
            let capability = match operation.capability() {
                Some(id) => format!("`{id}`"),
                None if operation.scheduler() => "none (scheduler)".to_owned(),
                None => "none".to_owned(),
            };
            vec![
                format!("`{}`", operation.name()),
                signature(operation),
                operation.minimum_abi().to_string(),
                capability,
                flag(operation.needs_result_globals()),
                flag(operation.returns_string()),
                admitting_ranges(operation, &json),
            ]
        }),
    )
}

fn profiles() -> String {
    let json = json_operations();
    let operations = registered_operations();
    table(
        &[
            "Profile",
            "Host ABI",
            "Value ABI",
            "Numbers",
            "Strings",
            "Tool/HTTP entry points",
            "Exception channel",
            "Frames",
            "Scheduler",
            "Admitted imports",
        ],
        language::SUPPORTED_PROFILES.iter().map(|&profile| {
            let exceptions = if !profile.exceptions() {
                "—".to_owned()
            } else if profile.errors() {
                let (export, version) = profile.exception_channel();
                format!("`{export}` = {version}")
            } else {
                "pending/value only".to_owned()
            };
            let frames = if profile.resumable_frames() {
                let catalog = if profile.frame_liveness() {
                    frame::FRAME_LIVENESS_CATALOG_VERSION
                } else {
                    frame::FRAME_CATALOG_VERSION
                };
                format!("`{}`, catalog {catalog}", frame::RESUME_EXPORT)
            } else {
                "—".to_owned()
            };
            let imports = operations
                .iter()
                .filter(|&&operation| admitted(operation, profile, &json))
                .count();
            vec![
                format!("`{}`", profile_name(profile)),
                profile.host_abi().to_string(),
                profile.value_abi().to_string(),
                if profile.binary64() {
                    "binary64"
                } else {
                    "i32"
                }
                .to_owned(),
                if profile.utf16() { "UTF-16LE" } else { "UTF-8" }.to_owned(),
                if entry_points(profile) { "yes" } else { "no" }.to_owned(),
                exceptions,
                frames,
                if profile.promises() { "yes" } else { "—" }.to_owned(),
                imports.to_string(),
            ]
        }),
    )
}

/// Compiler-helper functions a profile may define beyond user functions.
fn helper_allowance(profile: LanguageProfile) -> usize {
    if profile.array_mutations() {
        abi::MAX_INTERNAL_FUNCTIONS
    } else if profile.sparse_arrays() {
        abi::SPARSE_INTERNAL_FUNCTIONS
    } else {
        abi::LEGACY_INTERNAL_FUNCTIONS
    }
}

/// Per-profile acceptance details that module validation applies.
fn profile_acceptance() -> String {
    table(
        &[
            "Profile",
            "Accepted `varta.values`",
            "`varta.functions` versions",
            "Helper allowance",
            "Max defined functions",
        ],
        language::SUPPORTED_PROFILES.iter().map(|&profile| {
            let values: Vec<String> = (value::LEGACY_VALUE_VERSION
                ..=value::FUNCTION_FIELD_VALUE_VERSION)
                .filter(|&version| value::compatible_version(version, profile.value_abi()))
                .map(|version| version.to_string())
                .collect();
            let catalog = if !profile.property_deletion() {
                "—".to_owned()
            } else {
                match admission::frame_catalog_version(profile) {
                    Some(frames) => {
                        format!(
                            "{} (no frames), {frames} (frames)",
                            function::CATALOG_VERSION
                        )
                    }
                    None => function::CATALOG_VERSION.to_string(),
                }
            };
            let helpers = helper_allowance(profile);
            vec![
                format!("`{}`", profile_name(profile)),
                values.join(", "),
                catalog,
                helpers.to_string(),
                (abi::MAX_USER_FUNCTIONS + helpers + abi::MAX_HTTP_HANDLERS).to_string(),
            ]
        }),
    )
}

fn layout() -> String {
    let range = |start: usize, end: usize| format!("[{start}, {end})");
    table(
        &["Region or unit", "Bytes", "Source constants"],
        [
            (
                "Static data (literals, descriptors, schemas)",
                range(0, abi::RESULT_BASE),
                "`abi::RESULT_BASE`",
            ),
            (
                "Reusable host result buffer",
                range(abi::RESULT_BASE, abi::RESULT_BASE + abi::MAX_RESULT_BYTES),
                "`abi::RESULT_BASE`, `abi::MAX_RESULT_BYTES`",
            ),
            (
                "Exact reference root stack",
                range(abi::STRING_ROOT_BASE, abi::STRING_ROOT_END),
                "`abi::STRING_ROOT_BASE`, `abi::STRING_ROOT_END`",
            ),
            (
                "Exception root slot (start of the root stack)",
                range(
                    exceptions::ROOT,
                    exceptions::ROOT + exceptions::RESERVED_ROOT_BYTES,
                ),
                "`exceptions::ROOT`, `exceptions::RESERVED_ROOT_BYTES`",
            ),
            (
                "Managed string/record/array heap",
                range(abi::OWNED_STRING_BASE, abi::MAX_MEMORY_BYTES),
                "`abi::OWNED_STRING_BASE`, `abi::MAX_MEMORY_BYTES`",
            ),
            (
                "Wasm page",
                abi::WASM_PAGE_BYTES.to_string(),
                "`abi::WASM_PAGE_BYTES`",
            ),
            (
                "Root slot",
                abi::STRING_ROOT_SLOT_BYTES.to_string(),
                "`abi::STRING_ROOT_SLOT_BYTES`",
            ),
            (
                "Heap block header / alignment",
                format!("{} / {}", abi::STRING_HEADER_BYTES, value::HEAP_ALIGNMENT),
                "`abi::STRING_HEADER_BYTES`, `value::HEAP_ALIGNMENT`",
            ),
            (
                "Host input string (stored bytes)",
                abi::MAX_INPUT_BYTES.to_string(),
                "`abi::MAX_INPUT_BYTES`",
            ),
            (
                "One host result / one string value",
                abi::MAX_STRING_BYTES.to_string(),
                "`abi::MAX_STRING_BYTES`",
            ),
        ]
        .into_iter()
        .map(|(region, bytes, constants)| vec![region.to_owned(), bytes, constants.to_owned()]),
    )
}

fn names() -> String {
    let mut rows: Vec<(String, &str, &str, &str)> = vec![
        (
            abi::SECTION.into(),
            "custom section, u32 LE",
            "Host ABI version; mandatory for every declared profile",
            "`abi::SECTION`",
        ),
        (
            language::SECTION.into(),
            "custom section, JSON",
            "Source profile; mandatory above host ABI 1",
            "`language::SECTION`",
        ),
        (
            value::VALUE_SECTION.into(),
            "custom section, u32 LE",
            "Structured value ABI; required by JSON, union and exception imports",
            "`value::VALUE_SECTION`",
        ),
        (
            TOOL_SECTION.into(),
            "custom section, JSON",
            "Tool catalog; present only when tools are published",
            "`TOOL_SECTION`",
        ),
        (
            http::SECTION.into(),
            "custom section, JSON",
            "HTTP service contract; pairs with the `http_service` import",
            "`http::SECTION`",
        ),
        (
            function::CATALOG_SECTION.into(),
            "custom section, binary",
            "Function capture and frame catalog (v13+ closures, v33+ frames)",
            "`function::CATALOG_SECTION`",
        ),
        (
            abi::RESULT_LEN.into(),
            "export, mutable i32 global",
            "Required when any import needs result globals",
            "`abi::RESULT_LEN`",
        ),
        (
            abi::RESULT_STATUS.into(),
            "export, mutable i32 global",
            "Required when any import needs result globals",
            "`abi::RESULT_STATUS`",
        ),
        (
            frame::RESUME_EXPORT.into(),
            "export, function",
            "Closed resume dispatcher of frame profiles",
            "`frame::RESUME_EXPORT`",
        ),
        (
            format!("{TOOL_EXPORT_PREFIX}<name>"),
            "export, function",
            "One handler per catalog entry",
            "`TOOL_EXPORT_PREFIX`",
        ),
        (
            TOOL_COPY_STRING.into(),
            "export, function",
            "Tool string transfer (any string parameter or result)",
            "`TOOL_COPY_STRING`",
        ),
        (
            TOOL_RETAIN_STRING.into(),
            "export, function",
            "Tool string retention",
            "`TOOL_RETAIN_STRING`",
        ),
        (
            http::HANDLER.into(),
            "export, function",
            "HTTP adapter",
            "`http::HANDLER`",
        ),
        (
            http::COPY.into(),
            "export, function",
            "HTTP string transfer",
            "`http::COPY`",
        ),
        (
            http::RETAIN.into(),
            "export, function",
            "HTTP string retention",
            "`http::RETAIN`",
        ),
        (
            value::VALUE_ALLOCATE.into(),
            "export, function",
            "Guest allocator called back by checked JSON parsing",
            "`value::VALUE_ALLOCATE`",
        ),
        (
            value::VALUE_RETAIN.into(),
            "export, function",
            "Guest root retention called back by checked JSON parsing",
            "`value::VALUE_RETAIN`",
        ),
        (
            exceptions::PENDING.into(),
            "export, mutable i32 global",
            "Exception profiles: 1 while an exception is pending",
            "`exceptions::PENDING`",
        ),
        (
            exceptions::VALUE.into(),
            "export, mutable i64 global",
            "Exception profiles: the boxed thrown value",
            "`exceptions::VALUE`",
        ),
    ];
    for (name, constant) in [
        (exceptions::VERSION, "`exceptions::VERSION`"),
        (exceptions::FAMILY_VERSION, "`exceptions::FAMILY_VERSION`"),
        (exceptions::BIGINT_VERSION, "`exceptions::BIGINT_VERSION`"),
        (
            exceptions::CLASS_ERROR_VERSION,
            "`exceptions::CLASS_ERROR_VERSION`",
        ),
        (exceptions::DYNAMIC_VERSION, "`exceptions::DYNAMIC_VERSION`"),
        (exceptions::PROMISE_VERSION, "`exceptions::PROMISE_VERSION`"),
        (
            exceptions::FUNCTION_FIELD_VERSION,
            "`exceptions::FUNCTION_FIELD_VERSION`",
        ),
    ] {
        rows.push((
            name.into(),
            "export, immutable i32 global",
            "Exception channel version of Error profiles (one per module)",
            constant,
        ));
    }
    table(
        &["Name", "Kind", "Presence", "Constant"],
        rows.into_iter().map(|(name, kind, presence, constant)| {
            vec![
                format!("`{name}`"),
                kind.to_owned(),
                presence.to_owned(),
                constant.to_owned(),
            ]
        }),
    )
}

fn status_meaning(status: HostStatus) -> &'static str {
    // Exhaustive on purpose: a new status fails compilation here.
    match status {
        HostStatus::Ok => "Success",
        HostStatus::Denied => "Capability denied (checked before any effect)",
        HostStatus::InvalidInput => "Invalid input, range or encoding",
        HostStatus::Io => "I/O failure",
        HostStatus::TooLarge => "Result too large",
    }
}

fn completion_meaning(completion: Completion) -> &'static str {
    match completion {
        Completion::Normal => "Resume normally with the input value",
        Completion::Throw => "Resume by throwing the input value",
        Completion::Return => "Resume by returning the input value",
    }
}

fn outcome_meaning(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Suspended => "The frame suspended again",
        Outcome::Returned => "The frame completed",
        Outcome::Threw => "The frame completed by throwing",
    }
}

fn codes() -> String {
    let mut rows = Vec::new();
    for status in [
        HostStatus::Ok,
        HostStatus::Denied,
        HostStatus::InvalidInput,
        HostStatus::Io,
        HostStatus::TooLarge,
    ] {
        rows.push(vec![
            "`result_status`".to_owned(),
            (status as i32).to_string(),
            format!("`{status:?}`"),
            status_meaning(status).to_owned(),
        ]);
    }
    for completion in [Completion::Normal, Completion::Throw, Completion::Return] {
        rows.push(vec![
            format!("`{}` completion", frame::RESUME_EXPORT),
            (completion as i32).to_string(),
            format!("`{completion:?}`"),
            completion_meaning(completion).to_owned(),
        ]);
    }
    for outcome in [Outcome::Suspended, Outcome::Returned, Outcome::Threw] {
        assert_eq!(Outcome::from_code(outcome as i32), Some(outcome));
        rows.push(vec![
            format!("`{}` outcome", frame::RESUME_EXPORT),
            (outcome as i32).to_string(),
            format!("`{outcome:?}`"),
            outcome_meaning(outcome).to_owned(),
        ]);
    }
    table(&["Channel", "Code", "Name", "Meaning"], rows)
}

fn entry_limits() -> String {
    let ToolLimits {
        input_bytes,
        output_bytes,
        string_bytes,
        memory_bytes,
        fuel,
        host_calls,
        deadline_ms,
    } = ToolLimits::default();
    let rows: Vec<(&str, String, &str)> = vec![
        (
            "Tool contract / invocation ABI version",
            format!(
                "{} / {}",
                varta_common::TOOL_CONTRACT_VERSION,
                varta_common::TOOL_INVOCATION_ABI_VERSION
            ),
            "`TOOL_CONTRACT_VERSION`, `TOOL_INVOCATION_ABI_VERSION`",
        ),
        (
            "Tool catalog bytes",
            varta_common::MAX_TOOL_CATALOG_BYTES.to_string(),
            "`MAX_TOOL_CATALOG_BYTES`",
        ),
        (
            "Tools / parameters per tool",
            format!(
                "{} / {}",
                varta_common::MAX_TOOLS,
                varta_common::MAX_TOOL_PARAMETERS
            ),
            "`MAX_TOOLS`, `MAX_TOOL_PARAMETERS`",
        ),
        (
            "Tool or parameter name bytes / description bytes",
            format!(
                "{} / {}",
                varta_common::MAX_TOOL_NAME_BYTES,
                varta_common::MAX_TOOL_DESCRIPTION_BYTES
            ),
            "`MAX_TOOL_NAME_BYTES`, `MAX_TOOL_DESCRIPTION_BYTES`",
        ),
        (
            "Tool input / output JSON bytes",
            format!("{input_bytes} / {output_bytes}"),
            "`ToolLimits::default()`",
        ),
        (
            "Tool string bytes / memory bytes",
            format!("{string_bytes} / {memory_bytes}"),
            "`ToolLimits::default()`",
        ),
        (
            "Tool fuel / host calls / deadline",
            format!("{fuel} / {host_calls} / {deadline_ms} ms"),
            "`ToolLimits::default()`",
        ),
        (
            "HTTP contract version / metadata bytes",
            format!("{} / {}", http::VERSION, http::MAX_METADATA_BYTES),
            "`http::VERSION`, `http::MAX_METADATA_BYTES`",
        ),
        (
            "HTTP headers / header bytes / target bytes / body bytes",
            format!(
                "{} / {} / {} / {}",
                http::MAX_HEADERS,
                http::MAX_HEADER_BYTES,
                http::MAX_TARGET_BYTES,
                http::MAX_BODY_BYTES
            ),
            "`http::MAX_*`",
        ),
        (
            "HTTP header / connection timeout",
            format!(
                "{} s / {} s",
                http::HEADER_SECONDS,
                http::CONNECTION_SECONDS
            ),
            "`http::HEADER_SECONDS`, `http::CONNECTION_SECONDS`",
        ),
        (
            "Live continuation frames",
            frame::MAX_LIVE_FRAMES.to_string(),
            "`frame::MAX_LIVE_FRAMES`",
        ),
    ];
    table(
        &["Contract limit", "Value", "Source constants"],
        rows.into_iter()
            .map(|(limit, value, constant)| vec![limit.to_owned(), value, constant.to_owned()]),
    )
}

/// A one-tool catalog built and validated by the registry types themselves.
fn tool_catalog_example() -> String {
    let name = "text.echo";
    let catalog = ToolCatalog::new(vec![ToolContract {
        name: name.into(),
        description: "Return supplied text".into(),
        export: format!("{TOOL_EXPORT_PREFIX}{name}"),
        parameters: vec![ToolParameter {
            name: "value".into(),
            value_type: ToolType::String,
        }],
        result: ToolType::String,
        required_effects: Vec::new(),
        limits: ToolLimits::default(),
    }]);
    // The example must be a catalog the strict parser accepts.
    catalog.validate().unwrap();
    // Infallible: the catalog serializes to JSON.
    let json = serde_json::to_string_pretty(&catalog).unwrap();
    format!("```json\n{json}\n```")
}

#[test]
fn embedding_host_operations_match_registry() {
    assert_block(DOCUMENT, "embedding-host-operations", &host_operations());
}

#[test]
fn embedding_profiles_match_registry() {
    assert_block(DOCUMENT, "embedding-profiles", &profiles());
    assert_block(
        DOCUMENT,
        "embedding-profile-acceptance",
        &profile_acceptance(),
    );
}

#[test]
fn embedding_layout_names_and_codes_match_constants() {
    assert_block(DOCUMENT, "embedding-layout", &layout());
    assert_block(DOCUMENT, "embedding-names", &names());
    assert_block(DOCUMENT, "embedding-codes", &codes());
    assert_block(DOCUMENT, "embedding-entry-limits", &entry_limits());
    assert_block(
        DOCUMENT,
        "embedding-tool-catalog-example",
        &tool_catalog_example(),
    );
}

#[test]
fn every_profile_admits_its_own_json_pair() {
    // Guards the admission model the tables are derived from.
    let json = json_operations();
    for &profile in language::SUPPORTED_PROFILES {
        for operation in [profile.json_parse(), profile.json_stringify()] {
            assert!(
                admitted(operation, profile, &json),
                "{} does not admit {}",
                profile_name(profile),
                operation.name()
            );
        }
    }
}
