use std::path::PathBuf;

/// Source dependency graph metadata; paths identify compiler inputs, not grants.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModuleInfo {
    pub path: PathBuf,
    pub imports: Vec<PathBuf>,
}
