//! CSS Animations shorthand and keyframe selectors, shared by both hosts.
#[allow(unused_imports)]
use super::prelude::*;
use super::{
    CssProperty as P, css, mapped,
    syntax::{self, Spanned, Token},
};
pub const MAX_ANIMATIONS: usize = 8;
pub const PROPERTIES: [P; 8] = [
    P::AnimationName,
    P::AnimationDuration,
    P::AnimationTimingFunction,
    P::AnimationDelay,
    P::AnimationIterationCount,
    P::AnimationDirection,
    P::AnimationFillMode,
    P::AnimationPlayState,
];
pub const INITIAL: [&str; 8] = ["none", "0s", "ease", "0s", "1", "normal", "none", "running"];
/// Top-level comma entries; commas inside functions are not separators.
pub fn list(value: &str) -> Option<Vec<&str>> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut from = 0;
    for token in syntax::tokenize(value) {
        match token.token {
            Token::Function(_) | Token::OpenParen => depth += 1,
            Token::CloseParen => depth = depth.checked_sub(1)?,
            Token::Comma if depth == 0 => {
                out.push(value[from..token.start].trim());
                from = token.end;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    out.push(value[from..].trim());
    (!out.iter().any(|v| v.is_empty()) && out.len() <= MAX_ANIMATIONS).then_some(out)
}
pub fn components(value: &str) -> Option<Vec<String>> {
    let entries = list(value)?;
    let mut longhands: Vec<Vec<String>> = vec![Vec::new(); 8];
    for entry in entries {
        let mut values: Vec<String> = INITIAL.iter().map(|v| (*v).into()).collect();
        let mut used = [false; 8];
        for part in mapped::split(entry)? {
            let slot = if css::check(P::AnimationDuration, &part).is_ok()
                || css::check(P::AnimationDelay, &part).is_ok()
            {
                if !used[1] {
                    1
                } else if !used[3] {
                    3
                } else {
                    return None;
                }
            } else if !used[2] && css::check(P::AnimationTimingFunction, &part).is_ok() {
                2
            } else if !used[4] && css::check(P::AnimationIterationCount, &part).is_ok() {
                4
            } else if !used[5] && css::check(P::AnimationDirection, &part).is_ok() {
                5
            } else if !used[6] && css::check(P::AnimationFillMode, &part).is_ok() {
                6
            } else if !used[7] && css::check(P::AnimationPlayState, &part).is_ok() {
                7
            } else if !used[0] && css::check(P::AnimationName, &part).is_ok() {
                0
            } else {
                return None;
            };
            if css::CSS_WIDE_KEYWORDS
                .iter()
                .any(|v| part.eq_ignore_ascii_case(v))
            {
                return None;
            }
            used[slot] = true;
            values[slot] = part;
        }
        for (index, value) in values.into_iter().enumerate() {
            longhands[index].push(value);
        }
    }
    Some(
        longhands
            .into_iter()
            .map(|parts| parts.join(", "))
            .collect(),
    )
}
pub(super) fn offsets(tokens: &[Spanned]) -> Option<Vec<f64>> {
    let mut offsets = Vec::new();
    let mut expect = true;
    for token in tokens {
        match &token.token {
            Token::Whitespace => {}
            Token::Comma if !expect => expect = true,
            Token::Percentage(n) if expect && (0.0..=100.0).contains(&n.value) => {
                offsets.push(if n.value == 0.0 { 0.0 } else { n.value / 100.0 });
                expect = false;
            }
            Token::Ident(n)
                if expect && (n.eq_ignore_ascii_case("from") || n.eq_ignore_ascii_case("to")) =>
            {
                offsets.push(if n.eq_ignore_ascii_case("from") {
                    0.0
                } else {
                    1.0
                });
                expect = false;
            }
            _ => return None,
        }
    }
    (!expect).then_some(offsets)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shorthand_lists_and_function_commas() {
        let c = components(
            "pulse 2s cubic-bezier(.4,0,.6,1) infinite, spin 1s linear -1s reverse both paused",
        )
        .unwrap();
        assert_eq!(c[0], "pulse, spin");
        assert_eq!(c[3], "0s, -1s");
        assert_eq!(c[7], "running, paused");
        assert!(components("spin -1s").is_some()); // Longhand validation refuses the negative duration.
        assert!(components("spin 1s 2s 3s").is_none());
        assert!(css::check(P::AnimationName, "inherit, pulse").is_err());
        assert!(css::check(P::AnimationName, "none, pulse").is_ok());
    }
}
