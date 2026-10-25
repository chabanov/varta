//! Shared invocation values and stable boundary errors, independent of transport.
mod arguments;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ToolValue {
    Integer(i32),
    Boolean(bool),
    String(String),
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolResult {
    pub value: ToolValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolInvocationOutput {
    pub result: ToolResult,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationCode {
    InvalidArguments,
    UnknownTool,
    InvalidArtifact,
    InvalidPolicy,
    InvalidResult,
    ExecutionFailed,
    GuestExit,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolInvocationError {
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
    #[error("unknown published tool: {0}")]
    UnknownTool(String),
    #[error("invalid invocation artifact: {0}")]
    InvalidArtifact(String),
    #[error("invalid invocation policy: {0}")]
    InvalidPolicy(String),
    #[error("invalid tool result: {0}")]
    InvalidResult(String),
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("tool exited without returning a result (code {0})")]
    GuestExit(i32),
}
impl ToolInvocationError {
    #[must_use]
    pub fn code(&self) -> ToolInvocationCode {
        match self {
            Self::InvalidArguments(_) => ToolInvocationCode::InvalidArguments,
            Self::UnknownTool(_) => ToolInvocationCode::UnknownTool,
            Self::InvalidArtifact(_) => ToolInvocationCode::InvalidArtifact,
            Self::InvalidPolicy(_) => ToolInvocationCode::InvalidPolicy,
            Self::InvalidResult(_) => ToolInvocationCode::InvalidResult,
            Self::ExecutionFailed(_) => ToolInvocationCode::ExecutionFailed,
            Self::GuestExit(_) => ToolInvocationCode::GuestExit,
        }
    }
}
