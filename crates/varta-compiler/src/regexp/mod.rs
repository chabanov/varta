//! Ahead-of-time RegExp compilation (v37 `regexp()`). Regular expression
//! literals and `new RegExp` calls with constant arguments are parsed and
//! compiled to matcher programs at compile time. Under v37 the guest matcher
//! (`emit::strings::regexp`) runs the linear-time subset in time linear in
//! the input length (program format 1, `program`); v38 (`full_regexp`)
//! admits the whole ECMAScript 2025 pattern language and compiles it to
//! backtracking programs (format 2) for the linked matcher.
//!
//! The pattern parser and the v38 program compiler are the shared
//! `varta_intrinsics::regexp::pattern`, which v38 guests also link to
//! compile patterns built at run time; v37's format 1 stays here.
pub(crate) mod compile;
pub(crate) mod program;
#[cfg(test)]
mod tests;

use varta_intrinsics::regexp::pattern::{self as shared, RegExpError as PatternError};
pub(crate) use varta_intrinsics::regexp::pattern::{Compiled, Flags, case, parse, set};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegExpError {
    /// An early SyntaxError of the pattern or its flags.
    Syntax(String),
    /// A valid construct outside the linear-time subset, named precisely.
    Unsupported(String),
}

impl RegExpError {
    pub(crate) fn into_compiler(self) -> crate::CompilerError {
        match self {
            Self::Syntax(message) => crate::CompilerError::Invalid(message),
            Self::Unsupported(kind) => crate::CompilerError::Unsupported {
                kind,
                detail: "outside the checked-f64-utf16-v37 linear-time RegExp subset".into(),
            },
        }
    }
    /// The v38 compiler error: only program limits remain unsupported.
    pub(crate) fn into_full_compiler(self) -> crate::CompilerError {
        match self {
            Self::Syntax(message) => crate::CompilerError::Invalid(message),
            Self::Unsupported(kind) => crate::CompilerError::Unsupported {
                kind,
                detail: "beyond the checked-f64-utf16-v38 RegExp program limits".into(),
            },
        }
    }
    /// A shared pattern error with V8's message for `/pattern/flags`
    /// (`parsed`: the flags, once they parsed; a pattern is parsed only
    /// after its flags).
    fn from_pattern(error: PatternError, pattern: &str, flags: &str, parsed: Flags) -> Self {
        match error {
            PatternError::Pattern(message) => Self::Syntax(format!(
                "{}{pattern}/{}: {message}",
                shared::SYNTAX_PREFIX,
                parsed.text()
            )),
            PatternError::Flags => Self::Syntax(format!("{}{flags}'", shared::FLAGS_PREFIX)),
            PatternError::Unsupported(kind) => Self::Unsupported(kind.to_owned()),
            PatternError::Backreference(number) => Self::Unsupported(format!(
                "RegExp backreference \\{number} (backreferences need backtracking; v37 matches in linear time)"
            )),
            PatternError::TooLarge => Self::Unsupported(format!(
                "RegExp program larger than {} code units",
                varta_common::regexp::MAX_PROGRAM_UNITS
            )),
        }
    }
}

/// A compiled regular expression with its observable `source` and `flags`.
#[derive(Debug)]
pub(crate) struct RegExp {
    pub source: String,
    /// The pattern as written ([[OriginalSource]]); `source` escapes it.
    pub pattern: String,
    pub flags: Flags,
    pub compiled: Compiled,
}

/// Compile a pattern written as a literal (`source` is its text); `full`
/// selects the v38 language and backtracking program.
pub(crate) fn literal(pattern: &str, flags: &str, full: bool) -> Result<RegExp, RegExpError> {
    let flag_units: Vec<u16> = flags.encode_utf16().collect();
    let parsed = Flags::parse(&flag_units, full)
        .map_err(|error| RegExpError::from_pattern(error, pattern, flags, Flags::default()))?;
    let units: Vec<u16> = pattern.encode_utf16().collect();
    let error = |error| RegExpError::from_pattern(error, pattern, flags, parsed);
    let compiled = if full {
        shared::compile_full(&units, parsed).map_err(error)?
    } else {
        let tree = parse::parse(&units, parsed, false).map_err(error)?;
        compile::compile(&tree.node, tree.groups, &tree.names, parsed)?
    };
    Ok(RegExp {
        source: pattern.to_owned(),
        pattern: pattern.to_owned(),
        flags: parsed,
        compiled,
    })
}

/// Compile the pattern of `new RegExp(pattern, flags)`; `source` follows
/// EscapeRegExpPattern as V8 does (`/` and line terminators escaped).
pub(crate) fn constructed(pattern: &str, flags: &str, full: bool) -> Result<RegExp, RegExpError> {
    let mut regexp = literal(pattern, flags, full)?;
    let units: Vec<u16> = pattern.encode_utf16().collect();
    // Escaping inserts only ASCII before characters of `pattern`, so the
    // result of a `&str` pattern is valid UTF-16.
    regexp.source = String::from_utf16_lossy(&shared::escape_source(&units));
    Ok(regexp)
}
