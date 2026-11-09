use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, net::SocketAddr};
use varta_common::budget::{BudgetKind, BudgetRequest};

pub const SCHEMA_VERSION: u32 = 1;
pub const TOKEN_ENV: &str = "VARTA_PREVIEW_TOKEN";
pub const BUNDLE_POLICY: &str = "official-browser-v1";
pub const POLICY_ID: &str = "deny-all-v1";
pub const TARGET: &str = "browser";
pub const MAX_FILES: usize = 64;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 64 * 1024 * 1024;
pub const CHUNK_BYTES: usize = 8192;
pub const MAX_JSON_BYTES: usize = 32 * 1024;
pub const MAX_WORKER_INPUT: usize = 12 * 1024 * 1024;
pub const MAX_WORKER_OUTPUT: usize = 24 * 1024 * 1024;
pub const MAX_SLOTS: usize = 4;
pub const MAX_RECEIPTS: usize = 128;
pub const MAX_RUN_SECONDS: u64 = 300;
pub const MAX_IMPORT_SECONDS: u64 = 60;
pub const MAX_RETENTION_SECONDS: u64 = 300;
pub const TICKET_MS: u64 = 30_000;
pub const LEASE_MS: u64 = 3000;
pub const POLL_MS: u64 = 1000;
pub const COOKIE_NAME: &str = "varta_preview";

#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("invalid preview input: {0}")]
    Invalid(&'static str),
    #[error("preview I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("preview JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("preview worker: {0}")]
    Worker(#[from] crate::toolchain::ToolchainError),
    #[error("preview artifact: {0}")]
    Artifact(String),
}
pub type Result<T> = std::result::Result<T, PreviewError>;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PreviewCeiling {
    pub schema_version: u32,
    pub control_listen: SocketAddr,
    pub public_listen: SocketAddr,
    pub protected_origins: Vec<String>,
    pub max_imports: usize,
    pub max_runs: usize,
    pub max_run_seconds: u64,
    pub import_seconds: u64,
    pub retention_seconds: u64,
    pub validation_deadline_ms: u64,
    pub cpu_seconds: u64,
    pub address_space_bytes: u64,
    pub budgets: BudgetRequest,
}
impl PreviewCeiling {
    pub fn read(path: &std::path::Path) -> Result<Self> {
        use std::io::Read;
        let mut b = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_JSON_BYTES + 1) as u64)
            .read_to_end(&mut b)?;
        if b.len() > MAX_JSON_BYTES {
            return Err(PreviewError::Invalid("ceiling size"));
        }
        let c: Self = serde_json::from_slice(&b)?;
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || !self.control_listen.ip().is_loopback()
            || !self.public_listen.ip().is_loopback()
            || self.public_listen.ip() == self.control_listen.ip()
            || self.protected_origins.is_empty()
            || self.protected_origins.len() > 16
            || !(1..=MAX_SLOTS).contains(&self.max_imports)
            || !(1..=MAX_SLOTS).contains(&self.max_runs)
            || !(1..=MAX_RUN_SECONDS).contains(&self.max_run_seconds)
            || !(1..=MAX_IMPORT_SECONDS).contains(&self.import_seconds)
            || !(1..=MAX_RETENTION_SECONDS).contains(&self.retention_seconds)
            || !(10..=crate::toolchain::MAX_DEADLINE_MS).contains(&self.validation_deadline_ms)
            || !(1..=crate::toolchain::MAX_CPU_SECONDS).contains(&self.cpu_seconds)
            || !(crate::toolchain::MIN_ADDRESS_SPACE_BYTES
                ..=crate::toolchain::MAX_ADDRESS_SPACE_BYTES)
                .contains(&self.address_space_bytes)
        {
            return Err(PreviewError::Invalid("ceiling"));
        }
        self.budgets
            .validate()
            .map_err(|_| PreviewError::Invalid("budgets"))?;
        if BudgetKind::ALL
            .into_iter()
            .any(|k| self.budgets.value(k).is_some_and(|v| v > k.production()))
        {
            return Err(PreviewError::Invalid("budgets above production"));
        }
        let public_url = url::Url::parse(&format!("http://{}", self.public_listen))
            .map_err(|_| PreviewError::Invalid("public origin"))?;
        for origin in &self.protected_origins {
            if !varta_common::session::origin(origin, true) {
                return Err(PreviewError::Invalid("protected origin"));
            }
            let parsed =
                url::Url::parse(origin).map_err(|_| PreviewError::Invalid("protected origin"))?;
            if parsed.host_str() == public_url.host_str() {
                return Err(PreviewError::Invalid("hostname isolation"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleFile {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ImportRequest {
    pub schema_version: u32,
    pub host_sha256: String,
    pub artifact_sha256: String,
    pub files: Vec<BundleFile>,
}
impl ImportRequest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || !digest(&self.host_sha256)
            || !digest(&self.artifact_sha256)
            || self.files.is_empty()
            || self.files.len() > MAX_FILES
        {
            return Err(PreviewError::Invalid("bundle descriptor"));
        }
        let mut names = BTreeSet::new();
        let mut previous = "";
        let mut total = 0;
        for file in &self.files {
            if !path_valid(&file.path)
                || !digest(&file.sha256)
                || file.bytes > MAX_BYTES
                || file.path.as_str() <= previous
                || !names.insert(file.path.to_ascii_lowercase())
            {
                return Err(PreviewError::Invalid("bundle file"));
            }
            previous = &file.path;
            total += file.bytes;
            if total > MAX_BYTES {
                return Err(PreviewError::Invalid("bundle bytes"));
            }
        }
        Ok(())
    }
}
pub(crate) fn path_valid(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 256
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        && path
            .split('/')
            .all(|p| !p.is_empty() && !p.starts_with('.'))
}
pub(crate) use crate::host_control::{digest, hash, identity, key_valid, now_ms, random};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StartRequest {
    pub schema_version: u32,
    pub host_sha256: String,
    pub bundle_sha256: String,
    pub target: String,
    pub policy_id: String,
    pub ttl_seconds: u64,
}
impl StartRequest {
    pub fn validate(&self, max: u64) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || !digest(&self.host_sha256)
            || !digest(&self.bundle_sha256)
            || self.target != TARGET
            || self.policy_id != POLICY_ID
            || !(1..=max).contains(&self.ttl_seconds)
        {
            return Err(PreviewError::Invalid("start"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportState {
    Uploading,
    Sealing,
    Ready,
    Cancelled,
    Expired,
    Failed,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStatus {
    pub id: String,
    pub host_sha256: String,
    pub request_sha256: String,
    pub artifact_sha256: String,
    pub state: ImportState,
    pub error: Option<&'static str>,
    pub expires_at_ms: u64,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Ready,
    Active,
    Stopped,
    Expired,
    Failed,
}
impl RunState {
    pub(crate) fn terminal(self) -> bool {
        !matches!(self, Self::Ready | Self::Active)
    }
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuestState {
    Pending,
    Ready,
    Closed,
    Failed,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatus {
    pub id: String,
    pub host_sha256: String,
    pub request_sha256: String,
    pub bundle_sha256: String,
    pub state: RunState,
    pub guest_state: GuestState,
    pub error: Option<&'static str>,
    pub expires_at_ms: u64,
}
