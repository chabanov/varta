//! Typed errors of the Tailwind compiler. Messages follow Tailwind's own
//! errors where one exists.
use crate::canonical::CanonicalError;
use crate::css_functions::CssFunctionError;
use crate::css_parser::CssSyntaxError;
use crate::theme::ThemeError;
use crate::utils::brace_expansion::BraceExpansionError;
use crate::variants::AtVariantError;

/// A Tailwind compilation error.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TailwindError {
    #[error("{}", .0.message())]
    Syntax(#[from] CssSyntaxError),
    #[error(transparent)]
    Theme(#[from] ThemeError),
    #[error(transparent)]
    Function(#[from] CssFunctionError),
    #[error(transparent)]
    Variant(#[from] AtVariantError),
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
    #[error("invalid `@source inline(…)` pattern: {0}")]
    BraceExpansion(#[from] BraceExpansionError),
    /// `@plugin` and `@config` load JavaScript, which Varta never runs.
    #[error(
        "`{0}` loads JavaScript and is not supported: Varta compiles Tailwind without a JavaScript runtime; use CSS-first `@theme`, `@utility` and `@custom-variant`"
    )]
    JavaScriptConfig(String),
    /// A filesystem source: classes come only from the module graph.
    #[error(
        "`{0}` scans files, which Varta never does: classes are extracted from the program's module graph; list extra classes with `@source inline(\"…\")`"
    )]
    FileSource(String),
    #[error(
        "`@import {0}` cannot be resolved: a stylesheet imports the embedded `tailwindcss` package (`tailwindcss`, `tailwindcss/theme`, `tailwindcss/preflight`, `tailwindcss/utilities`) or a project file by a relative `./` or `../` path; URLs are never fetched"
    )]
    Import(String),
    #[error("`@import \"{specifier}\"`: {reason}")]
    ImportLoad { specifier: String, reason: String },
    #[error("`@import` cycle through {0}")]
    ImportCycle(String),
    #[error("`layer(…)` in an `@import` should come before any other functions or conditions")]
    ImportLayerOrder,
    #[error("exceeded the maximum `@import` depth while resolving `{0}`")]
    ImportDepth(String),
    /// A misplaced or malformed directive, with Tailwind's message.
    #[error("{0}")]
    Directive(String),
    #[error("cannot apply unknown utility class `{0}`")]
    UnknownApply(String),
    #[error("{0}")]
    Apply(String),
    #[error("circular dependency detected in {0}")]
    Circular(String),
    #[error("the Tailwind input exceeds the {limit} limit of {max}")]
    Limit { limit: &'static str, max: usize },
}
