//! The manifest, host-policy and v38 storage limits of `docs/embedding.md` are
//! generated from `varta_common::{budget, files, kv, net, permission}`, so a
//! host written from the document validates manifests with the registry's
//! exact values and root-name grammar.
use super::DOCUMENT;
use crate::support::{assert_block, profile_name, table};
use varta_common::{
    NORMALIZED_METHODS, abi,
    budget::{self, BudgetKind, Budgets},
    files, kv,
    language::{self, LanguageProfile},
    net, permission,
};

/// The one contiguous run of supported profiles for which `admits` holds.
fn profile_span(admits: impl Fn(LanguageProfile) -> bool) -> String {
    let profiles = language::SUPPORTED_PROFILES;
    let first = profiles.iter().position(|&p| admits(p)).unwrap();
    let last = profiles.iter().rposition(|&p| admits(p)).unwrap();
    assert!(
        profiles[first..=last].iter().all(|&p| admits(p)),
        "the profiles are not one contiguous run"
    );
    if first == last {
        format!("`{}`", profile_name(profiles[first]))
    } else {
        format!(
            "`{}` – `{}`",
            profile_name(profiles[first]),
            profile_name(profiles[last])
        )
    }
}

/// Each budget's engine maximum, v38 default and earlier-profile value, by
/// the constants that define them (asserted against the registry).
fn budget_constants(kind: BudgetKind) -> [(u64, &'static str); 3] {
    let deadline_ms = abi::DEADLINE_SECONDS * budget::MILLISECONDS_PER_SECOND;
    match kind {
        BudgetKind::Fuel => [
            (budget::MAX_FUEL_PER_TURN, "budget::MAX_FUEL_PER_TURN"),
            (budget::TURN_FUEL, "budget::TURN_FUEL"),
            (abi::FUEL, "abi::FUEL"),
        ],
        BudgetKind::Deadline => [
            (
                budget::MAX_DEADLINE_PER_TURN_MS,
                "budget::MAX_DEADLINE_PER_TURN_MS",
            ),
            (deadline_ms, "abi::DEADLINE_SECONDS"),
            (deadline_ms, "abi::DEADLINE_SECONDS"),
        ],
        BudgetKind::Memory => [
            (budget::MAX_MEMORY_BYTES, "budget::MAX_MEMORY_BYTES"),
            (abi::MAX_MEMORY_BYTES as u64, "abi::MAX_MEMORY_BYTES"),
            (abi::MAX_MEMORY_BYTES as u64, "abi::MAX_MEMORY_BYTES"),
        ],
        BudgetKind::HostCalls => [
            (
                u64::from(budget::MAX_HOST_CALLS_PER_TURN),
                "budget::MAX_HOST_CALLS_PER_TURN",
            ),
            (u64::from(abi::MAX_HOST_CALLS), "abi::MAX_HOST_CALLS"),
            (u64::from(abi::MAX_HOST_CALLS), "abi::MAX_HOST_CALLS"),
        ],
        BudgetKind::Output => [
            (budget::MAX_OUTPUT_BYTES, "budget::MAX_OUTPUT_BYTES"),
            (abi::MAX_OUTPUT_BYTES as u64, "abi::MAX_OUTPUT_BYTES"),
            (abi::MAX_OUTPUT_BYTES as u64, "abi::MAX_OUTPUT_BYTES"),
        ],
    }
}

fn budgets() -> String {
    // The prose says only turn-budget profiles can carry `budgets`: a package
    // manifest's `abiVersion` equals its module's host ABI.
    for &profile in language::SUPPORTED_PROFILES {
        assert_eq!(
            profile.host_abi() >= abi::PROGRAM_LIMITS_VERSION,
            profile.turn_budgets(),
            "{}",
            profile_name(profile)
        );
    }
    let v38 = profile_span(LanguageProfile::turn_budgets);
    let earlier = profile_span(|p| !p.turn_budgets());
    let header = [
        "`budgets` member".to_owned(),
        "Accepted request or ceiling value".to_owned(),
        format!("{v38}: default, and cap without a ceiling (per turn)"),
        format!("{earlier}: fixed (per execution)"),
        "Source constants".to_owned(),
    ];
    let header: Vec<&str> = header.iter().map(String::as_str).collect();
    table(
        &header,
        BudgetKind::ALL.into_iter().map(|kind| {
            let [(maximum, max_name), (turn, turn_name), (fixed, fixed_name)] =
                budget_constants(kind);
            assert_eq!(kind.maximum(), maximum, "{}", kind.wire_name());
            assert_eq!(Budgets::TURN.value(kind), turn, "{}", kind.wire_name());
            assert_eq!(kind.production(), turn, "{}", kind.wire_name());
            assert_eq!(
                Budgets::PRODUCTION.value(kind),
                fixed,
                "{}",
                kind.wire_name()
            );
            let mut names = vec![max_name, turn_name, fixed_name];
            names.dedup();
            vec![
                format!("`{}`", kind.wire_name()),
                format!("1 to {maximum}"),
                turn.to_string(),
                fixed.to_string(),
                names
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            ]
        }),
    )
}

fn quoted_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn manifest_limits() -> String {
    let rows: Vec<(&str, String, &str)> = vec![
        (
            "Grants in one manifest or host policy",
            varta_common::MAX_POLICY_GRANTS.to_string(),
            "`MAX_POLICY_GRANTS`",
        ),
        (
            "Patterns, listen endpoints and request URLs across all grants",
            varta_common::MAX_POLICY_PATTERNS.to_string(),
            "`MAX_POLICY_PATTERNS`",
        ),
        (
            "Patterns, listen endpoints or request URLs in one grant",
            varta_common::MAX_GRANT_PATTERNS.to_string(),
            "`MAX_GRANT_PATTERNS`",
        ),
        (
            "Name, path or URL pattern (bytes)",
            varta_common::MAX_PATTERN_BYTES.to_string(),
            "`MAX_PATTERN_BYTES`",
        ),
        (
            "Pattern bytes × input bytes of one comparison (more fails closed)",
            varta_common::MAX_PATTERN_MATCH_WORK.to_string(),
            "`MAX_PATTERN_MATCH_WORK`",
        ),
        (
            "`target` values",
            quoted_list(&varta_common::MANIFEST_TARGETS),
            "`MANIFEST_TARGETS`",
        ),
        (
            "Manifest, host ceiling or decision file the reference CLI and launcher read (bytes)",
            varta_common::MAX_MANIFEST_BYTES.to_string(),
            "`MAX_MANIFEST_BYTES`",
        ),
        (
            "`abiVersion` that admits `budgets`, grant `required` and `purpose`, and `fsRoots`",
            format!("{} or later", abi::PROGRAM_LIMITS_VERSION),
            "`abi::PROGRAM_LIMITS_VERSION`",
        ),
        (
            "Grant `purpose` (UTF-8 bytes)",
            permission::MAX_PURPOSE_BYTES.to_string(),
            "`permission::MAX_PURPOSE_BYTES`",
        ),
        (
            "`fsRoots` entries (package names or host mappings)",
            files::MAX_FILE_ROOTS.to_string(),
            "`files::MAX_FILE_ROOTS`",
        ),
        (
            "Root name (bytes)",
            format!("1 to {}", files::MAX_ROOT_NAME_BYTES),
            "`files::MAX_ROOT_NAME_BYTES`",
        ),
        (
            "Host root directory / guest virtual path (bytes)",
            files::MAX_PATH_BYTES.to_string(),
            "`files::MAX_PATH_BYTES`",
        ),
        (
            "Methods in one `net.request` grant",
            net::MAX_GRANT_METHODS.to_string(),
            "`net::MAX_GRANT_METHODS`",
        ),
        (
            "Methods never grantable (in any case)",
            quoted_list(&net::FORBIDDEN_METHODS),
            "`net::FORBIDDEN_METHODS`",
        ),
        (
            "Methods a grant must spell in uppercase",
            quoted_list(&NORMALIZED_METHODS),
            "`NORMALIZED_METHODS`",
        ),
        (
            "Guest URL of `net_get*`, `net_fetch*` and `net_request_*` (bytes; a longer URL is denied)",
            net::MAX_URL_BYTES.to_string(),
            "`net::MAX_URL_BYTES`",
        ),
        (
            "Response head of `net_get*` and `net_fetch*` (bytes; a larger head is an I/O failure)",
            net::MAX_FETCH_HEADER_BYTES.to_string(),
            "`net::MAX_FETCH_HEADER_BYTES`",
        ),
        (
            "Environment variable name of `env_*` (bytes; nonempty, a longer name is denied)",
            varta_common::MAX_ENV_NAME_BYTES.to_string(),
            "`MAX_ENV_NAME_BYTES`",
        ),
        (
            "Absolute host path of `fs_*` before v38 (bytes; a longer path is denied)",
            varta_common::MAX_HOST_PATH_BYTES.to_string(),
            "`MAX_HOST_PATH_BYTES`",
        ),
    ];
    table(
        &["Limit", "Value", "Source constant"],
        rows.into_iter()
            .map(|(limit, value, constant)| vec![limit.to_owned(), value, constant.to_owned()]),
    )
}

/// One documented root-name example and the validity the document states.
struct RootNameExample {
    shown: String,
    name: String,
    valid: bool,
    reason: &'static str,
}

fn example(name: &str, valid: bool, reason: &'static str) -> RootNameExample {
    RootNameExample {
        shown: format!("`{name}`"),
        name: name.to_owned(),
        valid,
        reason,
    }
}

fn repeated(count: usize, valid: bool, reason: &'static str) -> RootNameExample {
    RootNameExample {
        shown: format!("`a` repeated {count} times"),
        name: "a".repeat(count),
        valid,
        reason,
    }
}

fn root_name_examples() -> Vec<RootNameExample> {
    vec![
        example("project", true, "lowercase letters"),
        example("a", true, "one letter"),
        example(
            "data-2_cache",
            true,
            "digits, `-` and `_` after the first letter",
        ),
        repeated(files::MAX_ROOT_NAME_BYTES, true, "the longest name"),
        RootNameExample {
            shown: "(empty)".to_owned(),
            name: String::new(),
            valid: false,
            reason: "no first letter",
        },
        repeated(files::MAX_ROOT_NAME_BYTES + 1, false, "too long"),
        example("Project", false, "uppercase letter"),
        example("2fa", false, "starts with a digit"),
        example("_cache", false, "starts with `_`"),
        example("-x", false, "starts with `-`"),
        example("my.root", false, "`.` is not admitted"),
        example("a/b", false, "the path separator"),
        example("a b", false, "white space"),
        example("café", false, "non-ASCII letter"),
    ]
}

fn root_names() -> String {
    table(
        &["Name", "Valid", "Why"],
        root_name_examples().into_iter().map(|example| {
            assert_eq!(
                files::valid_root_name(&example.name),
                example.valid,
                "documented root name {}",
                example.shown
            );
            vec![
                example.shown,
                if example.valid { "yes" } else { "no" }.to_owned(),
                example.reason.to_owned(),
            ]
        }),
    )
}

fn kv_limits() -> String {
    let rows: Vec<(&str, usize, &str)> = vec![
        (
            "Databases open at once per instance",
            kv::MAX_OPEN_DATABASES,
            "`kv::MAX_OPEN_DATABASES`",
        ),
        (
            "Read or commit descriptor, or database path (UTF-16 bytes; longer is fatal)",
            kv::MAX_DESCRIPTOR_BYTES,
            "`kv::MAX_DESCRIPTOR_BYTES`",
        ),
        (
            "Encoded key to write (bytes)",
            kv::MAX_WRITE_KEY_BYTES,
            "`kv::MAX_WRITE_KEY_BYTES`",
        ),
        (
            "Encoded key of a range selector (bytes)",
            kv::MAX_READ_KEY_BYTES,
            "`kv::MAX_READ_KEY_BYTES`",
        ),
        (
            "Encoded value (bytes)",
            kv::MAX_VALUE_BYTES,
            "`kv::MAX_VALUE_BYTES`",
        ),
        (
            "Ranges per read",
            kv::MAX_READ_RANGES,
            "`kv::MAX_READ_RANGES`",
        ),
        (
            "Entries per read",
            kv::MAX_READ_ENTRIES,
            "`kv::MAX_READ_ENTRIES`",
        ),
        ("Checks per commit", kv::MAX_CHECKS, "`kv::MAX_CHECKS`"),
        (
            "Mutations per commit",
            kv::MAX_MUTATIONS,
            "`kv::MAX_MUTATIONS`",
        ),
        (
            "Mutation key and value bytes per commit",
            kv::MAX_TOTAL_MUTATION_BYTES,
            "`kv::MAX_TOTAL_MUTATION_BYTES`",
        ),
        (
            "Key bytes per commit",
            kv::MAX_TOTAL_KEY_BYTES,
            "`kv::MAX_TOTAL_KEY_BYTES`",
        ),
        (
            "Values staged at once (more is fatal)",
            kv::MAX_STAGED,
            "`kv::MAX_STAGED`",
        ),
        (
            "Encoded bytes staged at once",
            kv::MAX_STAGED_BYTES,
            "`kv::MAX_STAGED_BYTES`",
        ),
    ];
    let timing: Vec<(&str, u64, &str)> = vec![
        (
            "Commit cutoff before the turn deadline (ms)",
            kv::COMMIT_RESERVE_MS,
            "`kv::COMMIT_RESERVE_MS`",
        ),
        (
            "Longest wait for the write lock (ms, never past the cutoff)",
            kv::LOCK_WAIT_MS,
            "`kv::LOCK_WAIT_MS`",
        ),
    ];
    table(
        &["Limit", "Value", "Source constant"],
        rows.into_iter()
            .map(|(limit, value, constant)| (limit, value.to_string(), constant))
            .chain(
                timing
                    .into_iter()
                    .map(|(limit, value, constant)| (limit, value.to_string(), constant)),
            )
            .map(|(limit, value, constant)| vec![limit.to_owned(), value, constant.to_owned()]),
    )
}

#[test]
fn manifest_limits_match_registry() {
    assert_block(DOCUMENT, "embedding-manifest-budgets", &budgets());
    assert_block(DOCUMENT, "embedding-manifest-limits", &manifest_limits());
    assert_block(DOCUMENT, "embedding-kv-limits", &kv_limits());
}

#[test]
fn documented_root_names_match_valid_root_name() {
    assert_block(DOCUMENT, "embedding-root-names", &root_names());
}

#[test]
fn documented_purpose_rule_matches_validate_purpose() {
    // Each clause of the purpose rule stated next to the manifest limits.
    let longest = "x".repeat(permission::MAX_PURPOSE_BYTES);
    for valid in ["Sync your items", "Синхронізація", longest.as_str()] {
        assert!(
            permission::validate_purpose("net.request", valid).is_ok(),
            "{valid:?}"
        );
    }
    let too_long = "x".repeat(permission::MAX_PURPOSE_BYTES + 1);
    for invalid in [
        "",
        " \t ",
        "\u{3000}",
        "line\nbreak",
        "bell\u{7}",
        "next\u{85}line",
        &too_long,
    ] {
        assert!(
            permission::validate_purpose("net.request", invalid).is_err(),
            "{invalid:?}"
        );
    }
}
