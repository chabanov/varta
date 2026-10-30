//! Ahead-of-time RegExp compilation (v37 `regexp()`). Regular expression
//! literals and `new RegExp` calls with constant arguments are parsed and
//! compiled to matcher programs at compile time; the guest matcher
//! (`emit::strings::regexp`) runs them in time linear in the input length.
pub(crate) mod case;
pub(crate) mod compile;
pub(crate) mod parse;
pub(crate) mod program;
pub(crate) mod set;
#[cfg(test)]
mod tests;

pub(crate) use compile::Compiled;
pub(crate) use parse::Flags;

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
}

/// A compiled regular expression with its observable `source` and `flags`.
#[derive(Debug)]
pub(crate) struct RegExp {
    pub source: String,
    pub flags: Flags,
    pub compiled: Compiled,
}

/// Compile a pattern written as a literal (`source` is its text).
pub(crate) fn literal(pattern: &str, flags: &str) -> Result<RegExp, RegExpError> {
    let flags = Flags::parse(flags)?;
    let parsed = parse::parse(pattern, flags)?;
    let compiled = compile::compile(&parsed.node, parsed.groups, &parsed.names, flags)?;
    Ok(RegExp {
        source: pattern.to_owned(),
        flags,
        compiled,
    })
}

/// Compile the pattern of `new RegExp(pattern, flags)`; `source` follows
/// EscapeRegExpPattern as V8 does (`/` and line terminators escaped).
pub(crate) fn constructed(pattern: &str, flags: &str) -> Result<RegExp, RegExpError> {
    let mut regexp = literal(pattern, flags)?;
    regexp.source = escape(pattern);
    Ok(regexp)
}

fn escape(pattern: &str) -> String {
    if pattern.is_empty() {
        return "(?:)".into();
    }
    let mut source = String::with_capacity(pattern.len());
    let (mut escaped, mut class) = (false, false);
    for ch in pattern.chars() {
        let text = match ch {
            '\n' => Some(if escaped { "n" } else { "\\n" }),
            '\r' => Some(if escaped { "r" } else { "\\r" }),
            '\u{2028}' => Some(if escaped { "u2028" } else { "\\u2028" }),
            '\u{2029}' => Some(if escaped { "u2029" } else { "\\u2029" }),
            '/' if !escaped && !class => Some("\\/"),
            _ => None,
        };
        match text {
            Some(text) => source.push_str(text),
            None => source.push(ch),
        }
        if escaped {
            escaped = false;
        } else {
            match ch {
                '\\' => escaped = true,
                '[' => class = true,
                ']' => class = false,
                _ => {}
            }
        }
    }
    source
}
