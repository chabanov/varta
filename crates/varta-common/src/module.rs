use std::path::PathBuf;

/// Source dependency graph metadata; paths identify compiler inputs, not grants.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModuleInfo {
    pub path: PathBuf,
    pub imports: Vec<PathBuf>,
}

/// Validated bytecode metadata; callers need no backend-specific module types.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmInspection {
    pub language: crate::language::LanguageInspection,
    pub value_abi_version: Option<u32>,
    pub imports: Vec<WasmImport>,
    pub exports: Vec<String>,
    pub memories: usize,
    pub tool_catalog: Option<crate::ToolCatalog>,
    pub http_service: Option<crate::http::HttpContract>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WasmImport {
    pub module: String,
    pub name: String,
}
