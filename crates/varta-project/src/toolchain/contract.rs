use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use varta_common::language::LanguageProfile;

pub const SCHEMA_VERSION: u32 = 1;
pub const TOKEN_ENV: &str = "VARTA_TOOLCHAIN_TOKEN";
pub const MAX_FILES: usize = 256;
pub const MAX_PATH_BYTES: usize = 512;
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_SOURCE_BYTES: usize = 4 * MAX_FILE_BYTES;
pub const MAX_REQUEST_BYTES: usize = 12 * MAX_FILE_BYTES;
pub const MAX_ARTIFACT_BYTES: usize = 32 * MAX_FILE_BYTES;
pub const MAX_RETAINED_BYTES: usize = 64 * MAX_FILE_BYTES;
pub const MAX_RECEIPTS: usize = 256;
pub const MAX_WORKER_REPLY: usize = 48 * MAX_FILE_BYTES;
pub const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;
pub const MAX_ACTIVE_JOBS: usize = 16;
pub const MAX_WORKERS: usize = 4;
pub const MAX_DEADLINE_MS: u64 = 30_000;
pub const MAX_CPU_SECONDS: u64 = 30;
pub const MIN_ADDRESS_SPACE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_ADDRESS_SPACE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAX_RETENTION_SECONDS: u64 = 300;
pub const MAX_ARTIFACT_FILES: usize = 64;
pub const TARGET: &str = "browser";
pub const DEPENDENCY_POLICY: &str = "local-only-v1";

#[derive(Debug, thiserror::Error)]
pub enum ToolchainError {
    #[error("invalid toolchain input: {0}")]
    Invalid(&'static str),
    #[error("toolchain I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid toolchain JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("toolchain worker resource limits unavailable: {0}")]
    ResourceLimits(&'static str),
}
pub type Result<T> = std::result::Result<T, ToolchainError>;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Check,
    Build,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolchainCeiling {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_origins: Option<Vec<String>>,
    pub schema_version: u32,
    pub listen: SocketAddr,
    pub operations: Vec<Operation>,
    pub profiles: Vec<LanguageProfile>,
    pub max_jobs: usize,
    pub max_workers: usize,
    pub deadline_ms: u64,
    pub cpu_seconds: u64,
    pub address_space_bytes: u64,
    pub retention_seconds: u64,
}
impl ToolchainCeiling {
    pub fn read(path: &std::path::Path) -> Result<Self> {
        use std::io::Read;
        const MAX_CEILING_BYTES: usize = 32 * 1024;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_CEILING_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_CEILING_BYTES {
            return Err(ToolchainError::Invalid("ceiling byte limit"));
        }
        let ceiling: Self = serde_json::from_slice(&bytes)?;
        ceiling.validate()?;
        Ok(ceiling)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || !self.listen.ip().is_loopback()
            || self.operations.is_empty()
            || self.operations.len() > 2
            || self.profiles.is_empty()
            || self.profiles.len() > 64
            || !(1..=MAX_ACTIVE_JOBS).contains(&self.max_jobs)
            || !(1..=MAX_WORKERS).contains(&self.max_workers)
            || self.max_workers > self.max_jobs
            || !(10..=MAX_DEADLINE_MS).contains(&self.deadline_ms)
            || !(1..=MAX_CPU_SECONDS).contains(&self.cpu_seconds)
            || !(MIN_ADDRESS_SPACE_BYTES..=MAX_ADDRESS_SPACE_BYTES)
                .contains(&self.address_space_bytes)
            || !(1..=MAX_RETENTION_SECONDS).contains(&self.retention_seconds)
            || self.address_space_bytes * self.max_workers as u64 > MAX_ADDRESS_SPACE_BYTES
        {
            return Err(ToolchainError::Invalid("ceiling"));
        }
        if let Some(origins) = &self.application_origins
            && (origins.is_empty()
                || origins.len() > 16
                || origins.windows(2).any(|p| p[0] >= p[1])
                || origins.iter().any(|origin| {
                    !varta_common::session::origin(origin, true)
                        || !origin.starts_with("http://localhost:")
                }))
        {
            return Err(ToolchainError::Invalid("application origins"));
        }
        for (i, op) in self.operations.iter().enumerate() {
            if self.operations[..i].contains(op) {
                return Err(ToolchainError::Invalid("duplicate operation"));
            }
        }
        for (i, profile) in self.profiles.iter().enumerate() {
            if self.profiles[..i].contains(profile) {
                return Err(ToolchainError::Invalid("duplicate profile"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CompileRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application: Option<crate::application::ApplicationRequest>,
    pub schema_version: u32,
    pub compiler_sha256: String,
    pub operation: Operation,
    pub target: String,
    pub language_profile: LanguageProfile,
    pub dependency_policy: String,
    pub entry: String,
    pub files: Vec<SourceFile>,
}

pub(crate) fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && !path.chars().any(|c| c.is_control() || "\\:%?#".contains(c))
        && path
            .split('/')
            .all(|part| !matches!(part, "" | "." | ".." | "node_modules"))
}
pub(crate) fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub(crate) fn idempotency_valid(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}
impl CompileRequest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || !digest_valid(&self.compiler_sha256)
            || self.target != TARGET
            || match (&self.application, self.dependency_policy.as_str()) {
                (None, DEPENDENCY_POLICY) => false,
                (Some(app), crate::application::DEPENDENCY_POLICY) => {
                    app.validate()?;
                    self.language_profile != LanguageProfile::CheckedF64Utf16V40
                }
                _ => true,
            }
            || !valid_path(&self.entry)
            || !(self.entry.ends_with(".ts") || self.entry.ends_with(".tsx"))
            || self.files.is_empty()
            || self.files.len() > MAX_FILES
        {
            return Err(ToolchainError::Invalid("request"));
        }
        let mut paths = std::collections::BTreeSet::new();
        let mut total = 0;
        for file in &self.files {
            if !valid_path(&file.path)
                || file.content.len() > MAX_FILE_BYTES
                || !paths.insert(file.path.as_str())
            {
                return Err(ToolchainError::Invalid("file path, size or duplicate"));
            }
            total += file.content.len();
            if total > MAX_SOURCE_BYTES {
                return Err(ToolchainError::Invalid("total source bytes"));
            }
        }
        if !paths.contains(self.entry.as_str()) {
            return Err(ToolchainError::Invalid("missing entry"));
        }
        Ok(())
    }
    pub(crate) fn identity(&self) -> Result<String> {
        let mut canonical = self.clone();
        canonical.files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(hash(&serde_json::to_vec(&canonical)?))
    }
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildDiagnostic {
    pub severity: String,
    pub code: String,
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CompilationResult {
    pub ok: bool,
    pub diagnostics: Vec<BuildDiagnostic>,
    pub artifact_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub id: String,
    pub compiler_sha256: String,
    pub request_sha256: String,
    pub state: JobState,
    pub error: Option<&'static str>,
    pub result: Option<CompilationResult>,
    pub artifact: Option<ArtifactDescription>,
    pub forgotten: bool,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
impl JobState {
    pub(crate) fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ArtifactFile {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct ArtifactDescription {
    pub sha256: String,
    pub files: Vec<ArtifactFile>,
}
