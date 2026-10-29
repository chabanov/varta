//! Versioned inventory, strict baselines and observations have separate types.
use serde::{Deserialize, Serialize};
use varta_common::language::LanguageProfile;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inventory {
    pub schema_version: u32,
    pub id: String,
    pub edition: String,
    pub source_model: String,
    pub profile: LanguageProfile,
    pub features: Vec<Feature>,
    pub cases: Vec<Case>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Feature {
    pub id: String,
    pub title: String,
    pub core: Vec<String>,
    pub origin: Origin,
    pub reference: String,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Ecmascript2025,
    VartaContract,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub feature: String,
    pub path: String,
    pub sha256: String,
    pub stdout: Vec<String>,
    /// Reviewed boolean capability grants the case runs with (also the host
    /// ceiling); only [`CASE_GRANTS`] are admitted, and an omitted list runs
    /// the case without any grant.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grants: Vec<String>,
    pub baseline: Baseline,
}
/// Capabilities a case may be granted: the v36 `time.timer` grant only arms
/// timers on the runtime event loop, has no OS effect beyond waiting and is
/// bounded by the execution deadline.
pub const CASE_GRANTS: &[&str] = &[varta_common::scheduler::TIMER_CAPABILITY];
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Baseline {
    pub status: BaselineStatus,
    pub diagnostic: Option<CompileCode>,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaselineStatus {
    Supported,
    KnownUnsupported,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompileCode {
    SyntaxError,
    TypeError,
    ModuleError,
    UnsupportedSyntax,
    InvalidProgram,
    InternalError,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    pub profile: LanguageProfile,
    pub source: String,
    #[serde(default)]
    pub grants: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Observation {
    Executed {
        lines: Vec<String>,
        exit_code: i32,
    },
    CompileRejected {
        code: CompileCode,
        detail: String,
    },
    RuntimeFailed {
        category: RuntimeFailure,
        detail: String,
    },
    Infrastructure {
        category: InfrastructureFailure,
        detail: String,
    },
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFailure {
    UncaughtException,
    Execution,
    Validation,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureFailure {
    Launch,
    Timeout,
    OutputLimit,
    Process,
    Protocol,
    InternalCompiler,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Passed,
    KnownUnsupported,
    UnexpectedSupport,
    Regression,
    Infrastructure,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseReport {
    pub id: String,
    pub feature: String,
    pub path: String,
    pub source_sha256: String,
    pub status: CaseStatus,
    pub baseline_reason: Option<String>,
    pub observation: Observation,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub total: usize,
    pub passed: usize,
    pub known_unsupported: usize,
    pub unexpected_support: usize,
    pub regressions: usize,
    pub infrastructure_failures: usize,
}
impl Counts {
    pub(crate) fn add(&mut self, status: CaseStatus) {
        self.total += 1;
        match status {
            CaseStatus::Passed => self.passed += 1,
            CaseStatus::KnownUnsupported => self.known_unsupported += 1,
            CaseStatus::UnexpectedSupport => self.unexpected_support += 1,
            CaseStatus::Regression => self.regressions += 1,
            CaseStatus::Infrastructure => self.infrastructure_failures += 1,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureReport {
    pub feature: String,
    pub title: String,
    pub reference: String,
    pub origin: Origin,
    pub core: Vec<String>,
    pub counts: Counts,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub suite: String,
    pub edition: String,
    pub source_model: String,
    pub profile: LanguageProfile,
    pub inventory_sha256: String,
    pub dependency_lock_sha256: String,
    pub worker_timeout_ms: u64,
    pub runtime_budgets: RuntimeBudgets,
    pub package_version: String,
    pub os: String,
    pub arch: String,
    pub counts: Counts,
    pub baseline_matches: bool,
    pub untested_features: Vec<String>,
    pub features: Vec<FeatureReport>,
    pub cases: Vec<CaseReport>,
}
impl Report {
    /// Only the selected checked-case inventory; never a whole-edition readiness claim.
    pub fn all_cases_supported(&self) -> bool {
        self.baseline_matches
            && self.counts.passed == self.counts.total
            && self.untested_features.is_empty()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBudgets {
    pub fuel: u64,
    pub memory_bytes: usize,
    pub host_calls: u32,
    pub output_bytes: usize,
    pub deadline_seconds: u64,
}
