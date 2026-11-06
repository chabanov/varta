//! Port of Tailwind CSS v4.3.5 `utils/brace-expansion.ts` (Tailwind Labs,
//! MIT): expand `{a,b}` groups and `{1..9..2}` numeric ranges.
//!
//! Unlike Tailwind, the expansion is bounded: more than
//! [`MAX_BRACE_EXPANSION_RESULTS`] strings or recursion deeper than
//! [`MAX_BRACE_EXPANSION_DEPTH`] is an error rather than unbounded work.

use super::js_number::{number_to_string, to_number};
use super::segment::segment;

/// The most strings one expansion (or any of its sub-expansions) may produce.
pub const MAX_BRACE_EXPANSION_RESULTS: usize = 65_536;
/// The deepest recursion of an expansion: nested groups and successive
/// groups each add a level.
pub const MAX_BRACE_EXPANSION_DEPTH: usize = 256;

/// Errors of [`expand`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BraceExpansionError {
    /// A `{` without a matching `}`.
    #[error("The pattern `{pattern}` is not balanced.")]
    Unbalanced { pattern: String },
    /// A numeric range with a zero step (`{0..5..0}`).
    #[error("Step cannot be zero in sequence expansion.")]
    ZeroStep,
    /// The expansion would produce more than [`MAX_BRACE_EXPANSION_RESULTS`]
    /// strings.
    #[error("brace expansion produces more than {MAX_BRACE_EXPANSION_RESULTS} results")]
    TooManyResults,
    /// The pattern nests or chains groups deeper than
    /// [`MAX_BRACE_EXPANSION_DEPTH`].
    #[error("brace expansion is nested deeper than {MAX_BRACE_EXPANSION_DEPTH} levels")]
    TooDeep,
}

/// Expand every brace group of `pattern`. Results vary the first group
/// fastest within each expansion of the rest (`{a,b}/{x,y}` gives `a/x`,
/// `b/x`, `a/y`, `b/y`).
pub fn expand(pattern: &str) -> Result<Vec<String>, BraceExpansionError> {
    expand_at(pattern, 0)
}

fn expand_at(pattern: &str, depth: usize) -> Result<Vec<String>, BraceExpansionError> {
    let Some(index) = pattern.find('{') else {
        return Ok(vec![pattern.to_owned()]);
    };
    if depth >= MAX_BRACE_EXPANSION_DEPTH {
        return Err(BraceExpansionError::TooDeep);
    }

    let pre = &pattern[..index];
    let rest = &pattern[index..];

    // The matching closing brace, or the last one when the braces never
    // balance out.
    let mut end = rest.rfind('}');
    let mut level = 0_usize;
    for (i, c) in rest.char_indices() {
        if c == '{' {
            level += 1;
        } else if c == '}' {
            level = level.saturating_sub(1);
            if level == 0 {
                end = Some(i);
                break;
            }
        }
    }
    let Some(end) = end else {
        return Err(BraceExpansionError::Unbalanced {
            pattern: pattern.to_owned(),
        });
    };

    let inside = &rest[1..end];
    let post = &rest[end + 1..];

    let alternatives = match parse_sequence(inside) {
        Some(sequence) => expand_sequence(sequence)?,
        None => segment(inside, ','),
    };
    let mut parts = Vec::new();
    for alternative in alternatives {
        let expanded = expand_at(&alternative, depth + 1)?;
        if parts.len() + expanded.len() > MAX_BRACE_EXPANSION_RESULTS {
            return Err(BraceExpansionError::TooManyResults);
        }
        parts.extend(expanded);
    }

    let tails = expand_at(post, depth + 1)?;
    if parts.len().saturating_mul(tails.len()) > MAX_BRACE_EXPANSION_RESULTS {
        return Err(BraceExpansionError::TooManyResults);
    }

    let mut result = Vec::with_capacity(parts.len() * tails.len());
    for tail in &tails {
        for part in &parts {
            result.push(format!("{pre}{part}{tail}"));
        }
    }
    Ok(result)
}

/// A numeric range `start..end` or `start..end..step`, each `-?\d+`.
struct Sequence<'a> {
    start: &'a str,
    end: &'a str,
    step: Option<&'a str>,
}

/// Whether `text` is a signed decimal integer (`-?\d+`).
fn is_integer_text(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Match Tailwind's `/^(-?\d+)\.\.(-?\d+)(?:\.\.(-?\d+))?$/`.
fn parse_sequence(text: &str) -> Option<Sequence<'_>> {
    let fields: Vec<&str> = text.split("..").collect();
    if !matches!(fields.len(), 2 | 3) || !fields.iter().all(|field| is_integer_text(field)) {
        return None;
    }
    Some(Sequence {
        start: fields[0],
        end: fields[1],
        step: fields.get(2).copied(),
    })
}

/// Expand a numeric range with JavaScript number arithmetic: padding is not
/// kept (`{00..05}` gives `0` … `5`) and the sign of the step follows the
/// direction of the range.
fn expand_sequence(sequence: Sequence<'_>) -> Result<Vec<String>, BraceExpansionError> {
    let start = to_number(sequence.start);
    let end = to_number(sequence.end);
    let mut step = match sequence.step {
        Some(step) => to_number(step),
        None if start <= end => 1.0,
        None => -1.0,
    };
    if step == 0.0 {
        return Err(BraceExpansionError::ZeroStep);
    }

    let increasing = start < end;
    if (increasing && step < 0.0) || (!increasing && step > 0.0) {
        step = -step;
    }

    let mut result = Vec::new();
    let mut i = start;
    while if increasing { i <= end } else { i >= end } {
        // A step too small to change `i` would loop forever in Tailwind;
        // the bound turns it into an error.
        if result.len() == MAX_BRACE_EXPANSION_RESULTS {
            return Err(BraceExpansionError::TooManyResults);
        }
        result.push(number_to_string(i));
        i += step;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut values: Vec<String>) -> Vec<String> {
        values.sort();
        values
    }

    #[test]
    fn expands_patterns() {
        let cases: &[(&str, &[&str])] = &[
            ("a/b/c", &["a/b/c"]),
            // Groups.
            ("a/{x,y,z}/b", &["a/x/b", "a/y/b", "a/z/b"]),
            ("{a,b}/{x,y}", &["a/x", "a/y", "b/x", "b/y"]),
            (
                "{{xs,sm,md,lg}:,}hidden",
                &["xs:hidden", "sm:hidden", "md:hidden", "lg:hidden", "hidden"],
            ),
            // Numeric ranges.
            (
                "a/{0..5}/b",
                &["a/0/b", "a/1/b", "a/2/b", "a/3/b", "a/4/b", "a/5/b"],
            ),
            (
                "a/{-5..0}/b",
                &["a/-5/b", "a/-4/b", "a/-3/b", "a/-2/b", "a/-1/b", "a/0/b"],
            ),
            (
                "a/{0..-5}/b",
                &["a/0/b", "a/-1/b", "a/-2/b", "a/-3/b", "a/-4/b", "a/-5/b"],
            ),
            ("a/{0..10..5}/b", &["a/0/b", "a/5/b", "a/10/b"]),
            ("a/{0..10..-5}/b", &["a/10/b", "a/5/b", "a/0/b"]),
            ("a/{10..0..5}/b", &["a/10/b", "a/5/b", "a/0/b"]),
            ("a/{10..0..-5}/b", &["a/0/b", "a/5/b", "a/10/b"]),
            // Padding is not supported.
            (
                "a/{00..05}/b",
                &["a/0/b", "a/1/b", "a/2/b", "a/3/b", "a/4/b", "a/5/b"],
            ),
            (
                "a{001..9}b",
                &[
                    "a1b", "a2b", "a3b", "a4b", "a5b", "a6b", "a7b", "a8b", "a9b",
                ],
            ),
            // Ranges with a step.
            ("a/{0..5..2}/b", &["a/0/b", "a/2/b", "a/4/b"]),
            (
                "bg-red-{100..900..100}",
                &[
                    "bg-red-100",
                    "bg-red-200",
                    "bg-red-300",
                    "bg-red-400",
                    "bg-red-500",
                    "bg-red-600",
                    "bg-red-700",
                    "bg-red-800",
                    "bg-red-900",
                ],
            ),
            // Nested braces.
            ("a{b,c,/{x,y}}/e", &["ab/e", "ac/e", "a/x/e", "a/y/e"]),
            (
                "a{b,c,/{x,y},{z,w}}/e",
                &["ab/e", "ac/e", "a/x/e", "a/y/e", "az/e", "aw/e"],
            ),
            (
                "a{b,c,/{x,y},{0..2}}/e",
                &["ab/e", "ac/e", "a/x/e", "a/y/e", "a0/e", "a1/e", "a2/e"],
            ),
            (
                "bg-red-{50,{100..900..100},950}",
                &[
                    "bg-red-50",
                    "bg-red-100",
                    "bg-red-200",
                    "bg-red-300",
                    "bg-red-400",
                    "bg-red-500",
                    "bg-red-600",
                    "bg-red-700",
                    "bg-red-800",
                    "bg-red-900",
                    "bg-red-950",
                ],
            ),
            // Ranges with decimals are not expanded.
            ("{1.1..2.2}", &["1.1..2.2"]),
        ];
        for (input, expected) in cases {
            let expected: Vec<String> = expected.iter().map(|s| (*s).to_owned()).collect();
            assert_eq!(expand(input).map(sorted), Ok(sorted(expected)), "{input}");
        }
    }

    #[test]
    fn keeps_tailwind_result_order() {
        assert_eq!(
            expand("{a,b}/{x,y}"),
            Ok(vec!["a/x".into(), "b/x".into(), "a/y".into(), "b/y".into()])
        );
    }

    #[test]
    fn rejects_unbalanced_braces() {
        let error = expand("a{b,c{d,e},{f,g}h}x{y,z");
        assert_eq!(
            error,
            Err(BraceExpansionError::Unbalanced {
                pattern: "x{y,z".into()
            })
        );
        assert_eq!(
            error.map_err(|error| error.to_string()),
            Err("The pattern `x{y,z` is not balanced.".into())
        );
    }

    #[test]
    fn rejects_a_zero_step() {
        let error = expand("a{0..5..0}/b");
        assert_eq!(error, Err(BraceExpansionError::ZeroStep));
        assert_eq!(
            error.map_err(|error| error.to_string()),
            Err("Step cannot be zero in sequence expansion.".into())
        );
    }

    #[test]
    fn bounds_the_expansion() {
        assert_eq!(
            expand("{0..65536}"),
            Err(BraceExpansionError::TooManyResults)
        );
        assert_eq!(expand("{1..65536}").map(|values| values.len()), Ok(65_536));
        // 2^17 combinations.
        let product = "{a,b}".repeat(17);
        assert_eq!(expand(&product), Err(BraceExpansionError::TooManyResults));
        // A step that cannot advance past 2^53.
        assert_eq!(
            expand("{9007199254740992..9007199254740999}"),
            Err(BraceExpansionError::TooManyResults)
        );
        let deep = format!("{}{}", "{a".repeat(300), "}".repeat(300));
        assert_eq!(expand(&deep), Err(BraceExpansionError::TooDeep));
    }
}
