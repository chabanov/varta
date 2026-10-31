//! Permission broker v1 of profile v38 (`turn_budgets`, increment P5): the
//! contract shared by the manifest, the compiler's `varta.permissions`
//! prelude, the runtime broker and the headless decision file.
//!
//! Every requested grant is in one of three states for an instance:
//! `granted`, `denied` or `prompt`. The state of a concrete resource is
//! `denied` whenever the package request intersected with the host ceiling
//! does not cover it (deny by default, exact matching, unchanged); inside
//! that intersection it is the state of the grant:
//!
//! - a **required** grant (the default) is granted by policy at start, unless
//!   a stored decision denies it; then the instance cannot run;
//! - an **optional** grant (`"required": false`) starts from the stored
//!   decision for this application identity (the artifact hash); without one
//!   it is in `prompt` state when the host has a prompt seam and `denied`
//!   otherwise (headless and server hosts never prompt).
//!
//! A user decision only confirms or refuses a grant the package requested
//! within the host ceiling; it never widens a pattern or the ceiling.
use serde::{Deserialize, Serialize};

/// The state of a permission (W3C Permissions API names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(i32)]
pub enum PermissionState {
    Granted = 1,
    Denied = 2,
    Prompt = 3,
}
impl PermissionState {
    pub const ALL: [Self; 3] = [Self::Granted, Self::Denied, Self::Prompt];
    /// The guest-visible state name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Prompt => "prompt",
        }
    }
    #[must_use]
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// `permission_query` and `permission_request` answer this code (and
/// `permission_request` the id 0) for a descriptor that names no capability
/// or a resource the capability cannot have; the prelude throws `TypeError`.
pub const INVALID_DESCRIPTOR: i32 = 0;
/// The first line of every permission descriptor.
pub const DESCRIPTOR_VERSION: &str = "varta-permission-1";
/// Separates descriptor fields: version, name, resource flag, resource.
pub const DESCRIPTOR_SEPARATOR: char = '\n';
/// The fields of one descriptor; the resource, the last, may contain the
/// separator.
pub const DESCRIPTOR_FIELDS: usize = 4;
/// The resource flag of a descriptor without and with a resource.
pub const DESCRIPTOR_NO_RESOURCE: &str = "0";
pub const DESCRIPTOR_RESOURCE: &str = "1";
/// UTF-16 bytes of one descriptor.
pub const MAX_DESCRIPTOR_BYTES: usize = 16 * 1024;

/// Bytes of a grant's `purpose` text.
pub const MAX_PURPOSE_BYTES: usize = 256;
/// Audit entries the broker keeps per instance; older ones are dropped and
/// counted.
pub const MAX_AUDIT_ENTRIES: usize = 256;
/// Bytes of the resource text kept per audit entry.
pub const MAX_AUDIT_RESOURCE_BYTES: usize = 512;
/// Prompts that may be open at once per instance.
pub const MAX_OPEN_PROMPTS: usize = 1;
/// Dismissed prompts of one grant before it becomes a stored denial.
pub const MAX_DISMISSALS: u32 = 3;
/// Applications and decisions per application in one decision file.
pub const MAX_DECISION_APPLICATIONS: usize = 64;
pub const MAX_DECISIONS_PER_APPLICATION: usize = 128;
/// Application identities before C3 provenance: `sha256:` and the lowercase
/// hexadecimal SHA-256 digest of the portable Wasm bytes.
pub const ARTIFACT_PREFIX: &str = "sha256:";
pub const ARTIFACT_DIGEST_HEX: usize = 64;
const DECISION_SCHEMA_VERSION: u32 = 1;

/// Per-grant consent metadata from the package manifest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Consent {
    /// Whether the application cannot run without the grant (default true).
    pub required: bool,
    /// Untrusted package text shown as a quote in every prompt.
    pub purpose: Option<String>,
}
impl Consent {
    #[must_use]
    pub fn required() -> Self {
        Self {
            required: true,
            purpose: None,
        }
    }
}

/// Validate a `purpose` text: nonblank, bounded, no control characters.
pub fn validate_purpose(id: &str, purpose: &str) -> crate::Result<()> {
    if purpose.trim().is_empty()
        || purpose.len() > MAX_PURPOSE_BYTES
        || purpose.chars().any(char::is_control)
    {
        return Err(crate::VartaError::InvalidManifest(format!(
            "invalid purpose for {id}: at most {MAX_PURPOSE_BYTES} bytes of text without control characters"
        )));
    }
    Ok(())
}

/// A stored user decision for one grant of one application identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoredDecision {
    Allow,
    Deny,
}

/// Where a broker decision came from (audit log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionSource {
    /// The package request intersected with the host ceiling.
    Policy,
    /// A stored decision for this application identity.
    Stored,
    /// The user's answer to a prompt.
    Prompt,
}

/// One audit-log entry: every broker decision of an instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    /// Monotonic per instance, starting at 1.
    pub sequence: u64,
    /// Milliseconds since the instance started.
    pub at_ms: u64,
    pub source: DecisionSource,
    /// The capability id (`namespace.operation`).
    pub operation: String,
    /// The concrete resource, bounded by [`MAX_AUDIT_RESOURCE_BYTES`];
    /// empty for a grant-level decision.
    pub resource: String,
    pub outcome: PermissionState,
}

/// The answer of a prompt seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAnswer {
    /// Allow the grant for this application identity.
    Allow,
    /// Refuse it; stored as a denial.
    Deny,
    /// Closed without an answer; the state stays `prompt`.
    Dismiss,
}

/// What a prompt shows: the broker's own description of the operation and
/// its exact requested scope, with the package's purpose only as a quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPrompt {
    pub artifact: String,
    pub capability: String,
    /// The exact patterns of the package request (empty for boolean grants).
    pub scope: Vec<String>,
    pub purpose: Option<String>,
    /// Whether the application cannot run without it (a first-run prompt).
    pub required: bool,
}

/// Typed broker failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PermissionError {
    #[error(
        "required permission {id} is denied for this application ({source_label}); the application cannot run"
    )]
    RequiredDenied { id: String, source_label: String },
    #[error("invalid permission decisions: {0}")]
    InvalidDecisions(String),
}

/// One stored decision in the headless decision file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    pub id: String,
    pub decision: StoredDecision,
}

/// The decisions of one application identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationDecisions {
    pub artifact: String,
    pub decisions: Vec<DecisionRecord>,
}

/// The headless/server decision file: stored user decisions keyed by
/// application identity. It never grants anything outside a package request
/// and the host ceiling; decisions for other identities are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DecisionFile {
    #[serde(default = "decision_schema")]
    pub schema_version: u32,
    pub applications: Vec<ApplicationDecisions>,
}
fn decision_schema() -> u32 {
    DECISION_SCHEMA_VERSION
}

/// Whether `artifact` is a well-formed application identity.
#[must_use]
pub fn valid_artifact(artifact: &str) -> bool {
    artifact.strip_prefix(ARTIFACT_PREFIX).is_some_and(|hex| {
        hex.len() == ARTIFACT_DIGEST_HEX
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

impl DecisionFile {
    pub fn parse(bytes: &[u8]) -> Result<Self, PermissionError> {
        let file: Self = serde_json::from_slice(bytes)
            .map_err(|e| PermissionError::InvalidDecisions(e.to_string()))?;
        file.validate()?;
        Ok(file)
    }
    pub fn validate(&self) -> Result<(), PermissionError> {
        let invalid = |m: String| Err(PermissionError::InvalidDecisions(m));
        if self.schema_version != DECISION_SCHEMA_VERSION {
            return invalid("unsupported schemaVersion".into());
        }
        if self.applications.len() > MAX_DECISION_APPLICATIONS {
            return invalid("too many applications".into());
        }
        let mut artifacts = std::collections::BTreeSet::new();
        for application in &self.applications {
            if !valid_artifact(&application.artifact) {
                return invalid(format!(
                    "artifact must be {ARTIFACT_PREFIX} and {ARTIFACT_DIGEST_HEX} lowercase hex digits"
                ));
            }
            if !artifacts.insert(&application.artifact) {
                return invalid(format!("duplicate artifact {}", application.artifact));
            }
            if application.decisions.len() > MAX_DECISIONS_PER_APPLICATION {
                return invalid("too many decisions for one artifact".into());
            }
            let mut ids = std::collections::BTreeSet::new();
            for record in &application.decisions {
                if crate::capability_kind(&record.id).is_none() {
                    return invalid(format!("unknown capability {}", record.id));
                }
                if !ids.insert(&record.id) {
                    return invalid(format!("duplicate decision for {}", record.id));
                }
            }
        }
        Ok(())
    }
    /// The stored decisions of `artifact`, by capability id.
    #[must_use]
    pub fn decisions_for(
        &self,
        artifact: &str,
    ) -> std::collections::BTreeMap<String, StoredDecision> {
        self.applications
            .iter()
            .filter(|application| application.artifact == artifact)
            .flat_map(|application| {
                application
                    .decisions
                    .iter()
                    .map(|record| (record.id.clone(), record.decision))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const HASH: &str = "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn decision_files_are_strict_and_keyed_by_artifact() {
        let file = DecisionFile::parse(
            format!(
                r#"{{"applications":[{{"artifact":"{HASH}","decisions":[{{"id":"net.request","decision":"allow"}},{{"id":"time.timer","decision":"deny"}}]}}]}}"#
            )
            .as_bytes(),
        )
        .unwrap();
        let decisions = file.decisions_for(HASH);
        assert_eq!(decisions["net.request"], StoredDecision::Allow);
        assert_eq!(decisions["time.timer"], StoredDecision::Deny);
        assert!(file.decisions_for(&HASH.replace('0', "1")).is_empty());
        for bad in [
            r#"{"applications":[{"artifact":"sha256:00","decisions":[]}]}"#.to_owned(),
            format!(
                r#"{{"applications":[{{"artifact":"{}","decisions":[]}}]}}"#,
                HASH.to_uppercase()
            ),
            format!(
                r#"{{"applications":[{{"artifact":"{HASH}","decisions":[{{"id":"net.unknown","decision":"allow"}}]}}]}}"#
            ),
            format!(
                r#"{{"applications":[{{"artifact":"{HASH}","decisions":[{{"id":"time.now","decision":"maybe"}}]}}]}}"#
            ),
            format!(
                r#"{{"applications":[{{"artifact":"{HASH}","decisions":[{{"id":"time.now","decision":"allow"}},{{"id":"time.now","decision":"deny"}}]}}]}}"#
            ),
            format!(
                r#"{{"applications":[{{"artifact":"{HASH}","decisions":[]}},{{"artifact":"{HASH}","decisions":[]}}]}}"#
            ),
            r#"{"schemaVersion":2,"applications":[]}"#.to_owned(),
            r#"{"applications":[],"extra":1}"#.to_owned(),
        ] {
            assert!(DecisionFile::parse(bad.as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn purposes_are_bounded_text() {
        validate_purpose("net.request", "Connect to your workspace").unwrap();
        for bad in ["", "   ", "line\nbreak", &"x".repeat(MAX_PURPOSE_BYTES + 1)] {
            assert!(validate_purpose("net.request", bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            PermissionState::ALL.map(PermissionState::code),
            [1, 2, 3],
            "codes are the prelude's constants"
        );
    }
}
