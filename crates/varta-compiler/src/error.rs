use varta_common::{Diagnostic, DiagnosticCode};
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompilerError {
    #[error("{0}")]
    Parse(Box<varta_common::Diagnostic>),
    #[error("unsupported {kind}: {detail}")]
    Unsupported { kind: String, detail: String },
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Type(String),
    #[error("{0}")]
    Module(String),
    #[error("{diagnostic}")]
    Located {
        diagnostic: Box<varta_common::Diagnostic>,
        source: Box<CompilerError>,
    },
    #[error("internal error: {0}")]
    Internal(String),
}

impl CompilerError {
    #[must_use]
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        match self {
            Self::Parse(diagnostic) | Self::Located { diagnostic, .. } => Some(diagnostic),
            _ => None,
        }
    }
    pub(crate) fn code(&self) -> DiagnosticCode {
        match self {
            Self::Parse(_) => DiagnosticCode::SyntaxError,
            Self::Type(_) => DiagnosticCode::TypeError,
            Self::Module(_) => DiagnosticCode::ModuleError,
            Self::Unsupported { .. } => DiagnosticCode::UnsupportedSyntax,
            Self::Invalid(_) => DiagnosticCode::InvalidProgram,
            Self::Internal(_) => DiagnosticCode::InternalError,
            Self::Located { source, .. } => source.code(),
        }
    }
}
