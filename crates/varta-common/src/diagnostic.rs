//! Serializable source diagnostics. Lines/columns are one-based Unicode scalar
//! positions; byte offsets are zero-based UTF-8 and range ends are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    SyntaxError,
    TypeError,
    ModuleError,
    UnsupportedSyntax,
    InvalidProgram,
    InternalError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
    pub byte_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SourceRange {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceSnippet {
    pub text: String,
    /// Display columns within the bounded snippet, not absolute source offsets.
    pub highlight_start: usize,
    pub highlight_width: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub message: String,
    pub file: String,
    pub range: SourceRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<SourceSnippet>,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.file.chars() {
            if c.is_control() {
                write!(f, "{}", c.escape_debug())?;
            } else {
                write!(f, "{c}")?;
            }
        }
        write!(
            f,
            ":{}:{}: {}",
            self.range.start.line, self.range.start.column, self.message
        )?;
        if let Some(snippet) = &self.snippet {
            write!(
                f,
                "\n  | {}\n  | {}^{}",
                snippet.text,
                " ".repeat(snippet.highlight_start),
                "~".repeat(snippet.highlight_width.saturating_sub(1))
            )?;
        }
        Ok(())
    }
}
