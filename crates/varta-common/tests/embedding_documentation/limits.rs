//! The module-structure, scheduler, network-request, permission-broker and
//! host-event limits of `docs/embedding.md` are generated from
//! `varta_common::{abi, language, limits, scheduler, net, permission,
//! events}`, so a host written from the document enforces the registry's
//! exact values.
use super::DOCUMENT;
use crate::support::{assert_block, profile_name, table};
use varta_common::{
    abi, events,
    language::{self, LanguageProfile},
    limits, net, permission, scheduler,
};

/// One row of a limit table.
type Row = (String, String, String);

fn row(limit: &str, value: impl ToString, constant: &str) -> Row {
    (limit.to_owned(), value.to_string(), format!("`{constant}`"))
}

fn limit_table(rows: Vec<Row>) -> String {
    table(
        &["Limit", "Value", "Source constant"],
        rows.into_iter()
            .map(|(limit, value, constant)| vec![limit, value, constant]),
    )
}

/// The profiles for which `admits` holds, grouped into contiguous runs by the
/// value `of` takes, in profile order: each run is `(span, value)`.
fn spans<T: PartialEq + Copy>(
    admits: impl Fn(LanguageProfile) -> bool,
    of: impl Fn(LanguageProfile) -> T,
) -> Vec<(String, T)> {
    let profiles: Vec<LanguageProfile> = language::SUPPORTED_PROFILES
        .iter()
        .copied()
        .filter(|&profile| admits(profile))
        .collect();
    let mut runs: Vec<(LanguageProfile, LanguageProfile, T)> = Vec::new();
    for profile in profiles {
        match runs.last_mut() {
            Some((_, last, value)) if *value == of(profile) => *last = profile,
            _ => runs.push((profile, profile, of(profile))),
        }
    }
    // Each value must form one run, so the spans stay readable.
    for (index, (_, _, value)) in runs.iter().enumerate() {
        assert!(
            runs[index + 1..].iter().all(|(_, _, other)| other != value),
            "a limit value recurs after another value"
        );
    }
    runs.into_iter()
        .map(|(first, last, value)| {
            let span = if first == last {
                format!("`{}`", profile_name(first))
            } else {
                format!("`{}` – `{}`", profile_name(first), profile_name(last))
            };
            (span, value)
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn module_limits() -> String {
    // The frozen module limit is also the limit of unspecified generic Wasm.
    assert_eq!(limits::FROZEN.wasm_bytes, abi::MAX_WASM_BYTES);
    assert_eq!(
        LanguageProfile::default().limits().wasm_bytes,
        abi::MAX_WASM_BYTES
    );
    let mut rows = vec![
        row(
            "Binary header (magic and version 1)",
            format!("`{}`", hex(abi::WASM_HEADER)),
            "abi::WASM_HEADER",
        ),
        row(
            "`varta.language` metadata (bytes)",
            language::MAX_METADATA_BYTES,
            "language::MAX_METADATA_BYTES",
        ),
    ];
    for (span, bytes) in spans(|_| true, |profile| profile.limits().wasm_bytes) {
        let constant = if bytes == abi::MAX_WASM_BYTES {
            "abi::MAX_WASM_BYTES"
        } else {
            assert_eq!(bytes, limits::WASM.wasm_bytes);
            "limits::WASM"
        };
        rows.push(row(
            &format!("Wasm module (bytes), {span}"),
            bytes,
            constant,
        ));
    }
    rows.extend([
        row(
            "Parameters of one function type",
            abi::MAX_WASM_PARAMETERS,
            "abi::MAX_WASM_PARAMETERS",
        ),
        row(
            "Results of one function type",
            abi::MAX_WASM_RESULTS,
            "abi::MAX_WASM_RESULTS",
        ),
        row("Imports", abi::MAX_IMPORTS, "abi::MAX_IMPORTS"),
        row("Memories", abi::MAX_MEMORIES, "abi::MAX_MEMORIES"),
        row(
            "Initial and maximum memory pages",
            abi::MAX_MEMORY_BYTES / abi::WASM_PAGE_BYTES,
            "abi::MAX_MEMORY_BYTES / abi::WASM_PAGE_BYTES",
        ),
        row("Tables", abi::MAX_TABLES, "abi::MAX_TABLES"),
        row(
            "Initial elements of the table",
            abi::MAX_TABLE_ELEMENTS,
            "abi::MAX_TABLE_ELEMENTS",
        ),
        row("Globals", abi::MAX_GLOBALS, "abi::MAX_GLOBALS"),
        row(
            "User functions, v1–v37",
            abi::MAX_USER_FUNCTIONS,
            "abi::MAX_USER_FUNCTIONS",
        ),
        row(
            "HTTP adapters, v1–v37",
            abi::MAX_HTTP_HANDLERS,
            "abi::MAX_HTTP_HANDLERS",
        ),
    ]);
    limit_table(rows)
}

fn scheduler_limits() -> String {
    let mut rows = Vec::new();
    for (span, held) in spans(LanguageProfile::promises, |profile| {
        profile.data().pending_continuations
    }) {
        let constant = if held == scheduler::MAX_PENDING_CONTINUATIONS {
            "scheduler::MAX_PENDING_CONTINUATIONS"
        } else {
            assert_eq!(held, limits::V38_MAX_PENDING_CONTINUATIONS);
            "limits::V38_MAX_PENDING_CONTINUATIONS"
        };
        rows.push(row(
            &format!("Tokens held at once (queued and registered), {span}"),
            held,
            constant,
        ));
    }
    for (span, timers) in spans(LanguageProfile::timers, |profile| {
        profile.data().pending_timers
    }) {
        let constant = if timers == scheduler::MAX_PENDING_TIMERS {
            "scheduler::MAX_PENDING_TIMERS"
        } else {
            assert_eq!(timers, limits::V38_MAX_PENDING_TIMERS);
            "limits::V38_MAX_PENDING_TIMERS"
        };
        rows.push(row(
            &format!("Timers armed or awaiting their take at once, {span}"),
            timers,
            constant,
        ));
    }
    rows.extend([
        row(
            "Tokens taken from the queue per execution (v38: per turn)",
            scheduler::MAX_EXECUTION_JOBS,
            "scheduler::MAX_EXECUTION_JOBS",
        ),
        row(
            "Promise serials, and separately tracking ids and timer ids, per execution",
            scheduler::MAX_PROMISE_SERIALS,
            "scheduler::MAX_PROMISE_SERIALS",
        ),
        row(
            "Unhandled-rejection reasons kept",
            scheduler::MAX_RETAINED_REJECTION_REASONS,
            "scheduler::MAX_RETAINED_REJECTION_REASONS",
        ),
        row(
            "UTF-16 units per rendered reason field",
            scheduler::MAX_REJECTION_REASON_UNITS,
            "scheduler::MAX_REJECTION_REASON_UNITS",
        ),
        row(
            "Bytes per rendered reason",
            scheduler::MAX_REJECTION_REASON_BYTES,
            "scheduler::MAX_REJECTION_REASON_BYTES",
        ),
    ]);
    limit_table(rows)
}

fn net_limits() -> String {
    limit_table(vec![
        row(
            "Requests holding a runtime slot at once",
            net::MAX_PENDING_REQUESTS,
            "net::MAX_PENDING_REQUESTS",
        ),
        row(
            "Request descriptor, body included (UTF-16 bytes)",
            net::MAX_REQUEST_BYTES,
            "net::MAX_REQUEST_BYTES",
        ),
        row(
            "Request header fields",
            net::MAX_REQUEST_HEADERS,
            "net::MAX_REQUEST_HEADERS",
        ),
        row(
            "Request header name and value bytes",
            net::MAX_REQUEST_HEADER_BYTES,
            "net::MAX_REQUEST_HEADER_BYTES",
        ),
        row(
            "Response header fields",
            net::MAX_RESPONSE_HEADERS,
            "net::MAX_RESPONSE_HEADERS",
        ),
        row(
            "Response header name and value bytes",
            net::MAX_RESPONSE_HEADER_BYTES,
            "net::MAX_RESPONSE_HEADER_BYTES",
        ),
        row(
            "Body bytes of one `net_body_read` completion",
            net::MAX_CHUNK_BYTES,
            "net::MAX_CHUNK_BYTES",
        ),
        row(
            "Body bytes of one response",
            net::MAX_RESPONSE_BODY_BYTES,
            "net::MAX_RESPONSE_BODY_BYTES",
        ),
        (
            "First descriptor line: `net_request_start` / `net_request_start_v2`".to_owned(),
            format!(
                "`{}` / `{}`",
                net::DESCRIPTOR_VERSION,
                net::DESCRIPTOR_VERSION_BYTES
            ),
            "`net::DESCRIPTOR_VERSION`, `net::DESCRIPTOR_VERSION_BYTES`".to_owned(),
        ),
    ])
}

fn permission_limits() -> String {
    limit_table(vec![
        row(
            "Descriptor (UTF-16 bytes)",
            permission::MAX_DESCRIPTOR_BYTES,
            "permission::MAX_DESCRIPTOR_BYTES",
        ),
        row(
            "Descriptor fields (version, name, resource flag, resource)",
            permission::DESCRIPTOR_FIELDS,
            "permission::DESCRIPTOR_FIELDS",
        ),
        row(
            "First descriptor line",
            format!("`{}`", permission::DESCRIPTOR_VERSION),
            "permission::DESCRIPTOR_VERSION",
        ),
        row(
            "Prompts open at once per instance",
            permission::MAX_OPEN_PROMPTS,
            "permission::MAX_OPEN_PROMPTS",
        ),
        row(
            "Dismissals of one grant before a stored denial",
            permission::MAX_DISMISSALS,
            "permission::MAX_DISMISSALS",
        ),
        row(
            "Audit entries kept per instance (older are dropped and counted)",
            permission::MAX_AUDIT_ENTRIES,
            "permission::MAX_AUDIT_ENTRIES",
        ),
        row(
            "Resource bytes kept per audit entry",
            permission::MAX_AUDIT_RESOURCE_BYTES,
            "permission::MAX_AUDIT_RESOURCE_BYTES",
        ),
        row(
            "Applications in one decision file",
            permission::MAX_DECISION_APPLICATIONS,
            "permission::MAX_DECISION_APPLICATIONS",
        ),
        row(
            "Decisions per application",
            permission::MAX_DECISIONS_PER_APPLICATION,
            "permission::MAX_DECISIONS_PER_APPLICATION",
        ),
    ])
}

fn event_limits() -> String {
    limit_table(vec![
        row(
            "Sources open at once per instance (named sources and pending permission answers)",
            events::MAX_EVENT_SOURCES,
            "events::MAX_EVENT_SOURCES",
        ),
        row(
            "Host tasks queued at once per instance",
            events::MAX_PENDING_HOST_TASKS,
            "events::MAX_PENDING_HOST_TASKS",
        ),
        row(
            "Event record (UTF-8 bytes)",
            events::MAX_EVENT_RECORD_BYTES,
            "events::MAX_EVENT_RECORD_BYTES",
        ),
        row(
            "Source name (bytes)",
            events::MAX_EVENT_NAME_BYTES,
            "events::MAX_EVENT_NAME_BYTES",
        ),
    ])
}

#[test]
fn module_and_instance_limits_match_registry() {
    assert_block(DOCUMENT, "embedding-module-limits", &module_limits());
    assert_block(DOCUMENT, "embedding-scheduler-limits", &scheduler_limits());
    assert_block(DOCUMENT, "embedding-net-limits", &net_limits());
    assert_block(
        DOCUMENT,
        "embedding-permission-limits",
        &permission_limits(),
    );
    assert_block(DOCUMENT, "embedding-event-limits", &event_limits());
}
