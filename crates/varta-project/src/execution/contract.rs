pub(crate) use crate::host_control::{digest, hash, identity, key_valid, now_ms, random};
pub use crate::preview::{BundleFile, ImportRequest, ImportState, ImportStatus};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use varta_common::budget::{BudgetKind, BudgetRequest};
pub const TOKEN_ENV: &str = "VARTA_EXECUTION_TOKEN";
pub const TARGET: &str = "native-http";
pub const POLICY_ID: &str = "loopback-http-v1";
pub const ARTIFACT_POLICY: &str = "native-http-v1";
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RETAINED: usize = 64 * 1024 * 1024;
pub const CHUNK_BYTES: usize = 8192;
pub const MAX_RECEIPTS: usize = 128;
pub const MAX_IPC: usize = 12 * 1024 * 1024;
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("invalid execution input: {0}")]
    Invalid(&'static str),
    #[error("execution I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("execution JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("execution process guard: {0}")]
    Guard(#[from] crate::toolchain::ToolchainError),
    #[error("execution artifact: {0}")]
    Artifact(String),
}
pub type Result<T> = std::result::Result<T, ExecutionError>;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExecutionCeiling {
    pub schema_version: u32,
    pub listen: SocketAddr,
    pub max_runs: usize,
    pub max_imports: usize,
    pub max_run_seconds: u64,
    pub import_seconds: u64,
    pub retention_seconds: u64,
    pub startup_deadline_ms: u64,
    pub validation_deadline_ms: u64,
    pub cpu_seconds: u64,
    pub address_space_bytes: u64,
    pub budgets: BudgetRequest,
}
impl ExecutionCeiling {
    pub fn read(path: &std::path::Path) -> Result<Self> {
        use std::io::Read;
        let mut b = Vec::new();
        std::fs::File::open(path)?.take(32769).read_to_end(&mut b)?;
        if b.len() > 32768 {
            return Err(ExecutionError::Invalid("ceiling size"));
        }
        let c: Self = serde_json::from_slice(&b)?;
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || !self.listen.ip().is_loopback()
            || !(1..=2).contains(&self.max_runs)
            || !(1..=4).contains(&self.max_imports)
            || !(1..=300).contains(&self.max_run_seconds)
            || !(1..=60).contains(&self.import_seconds)
            || !(1..=300).contains(&self.retention_seconds)
            || !(10..=30000).contains(&self.startup_deadline_ms)
            || !(10..=30000).contains(&self.validation_deadline_ms)
            || !(1..=30).contains(&self.cpu_seconds)
            || !(crate::toolchain::MIN_ADDRESS_SPACE_BYTES
                ..=crate::toolchain::MAX_ADDRESS_SPACE_BYTES)
                .contains(&self.address_space_bytes)
        {
            return Err(ExecutionError::Invalid("ceiling"));
        }
        self.budgets
            .validate()
            .map_err(|_| ExecutionError::Invalid("budgets"))?;
        if BudgetKind::ALL
            .into_iter()
            .any(|k| self.budgets.value(k).is_some_and(|v| v > k.production()))
        {
            return Err(ExecutionError::Invalid("budgets above production"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StartRequest {
    pub schema_version: u32,
    pub host_sha256: String,
    pub compiler_sha256: String,
    pub artifact_sha256: String,
    pub target: String,
    pub policy_id: String,
    pub ttl_seconds: u64,
}
impl StartRequest {
    pub fn validate(&self, max: u64) -> Result<()> {
        if self.schema_version != 1
            || !digest(&self.host_sha256)
            || !digest(&self.compiler_sha256)
            || !digest(&self.artifact_sha256)
            || self.target != TARGET
            || self.policy_id != POLICY_ID
            || !(1..=max).contains(&self.ttl_seconds)
        {
            return Err(ExecutionError::Invalid("start request"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Starting,
    Ready,
    Stopping,
    Stopped,
    Expired,
    Exited,
    Failed,
}
impl RunState {
    pub(crate) fn terminal(self) -> bool {
        matches!(
            self,
            Self::Stopped | Self::Expired | Self::Exited | Self::Failed
        )
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatus {
    pub id: String,
    pub host_sha256: String,
    pub compiler_sha256: String,
    pub request_sha256: String,
    pub artifact_sha256: String,
    pub state: RunState,
    pub address: Option<SocketAddr>,
    pub error: Option<&'static str>,
    pub expires_at_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkerInput {
    pub request: ImportRequest,
    pub files: std::collections::BTreeMap<String, String>,
    pub budgets: BudgetRequest,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "event", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum WorkerEvent {
    Validated,
    Listening { address: SocketAddr },
    Exited { ok: bool, exit_code: i32 },
}
pub(super) fn import_valid(r: &ImportRequest) -> bool {
    r.validate().is_ok()
        && r.files
            .iter()
            .try_fold(0usize, |sum, f| sum.checked_add(f.bytes))
            .is_some_and(|n| n <= MAX_BYTES)
        && r.files.len() == 2
        && r.files[0].path == "app.manifest.json"
        && r.files[1].path == "app.wasm"
}
