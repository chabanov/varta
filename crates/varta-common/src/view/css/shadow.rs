//! CSS Backgrounds 3 §6.1: shadow lists, after variable substitution and
//! ordinary computed-value lowering. No resource or document lookups.
use super::*;
/// The third length of a shadow is its nonnegative blur radius.
pub const BLUR_POSITION: usize = 2;
const MAX_LENGTHS: usize = 4;

/// One computed shadow, lengths in CSS pixels. A missing color is
/// `currentcolor`, resolved by the painter for the originating element.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub spread: f64,
    pub color: String,
    pub inset: bool,
}

pub(super) fn valid_blurs(tokens: &[V]) -> bool {
    let mut depth = 0;
    let mut lengths = 0;
    for token in tokens {
        match token {
            V::Function(_) => {
                if depth == 0 && math::is_math(Some(token)) {
                    lengths += 1;
                }
                // Colors and var() are not positional lengths. A var() can
                // expand to several lengths; validate again after substitution.
                depth += 1;
            }
            V::Close => depth -= 1,
            V::Comma if depth == 0 => lengths = 0,
            V::Num { value, .. } if depth == 0 => {
                if lengths == BLUR_POSITION && *value < 0.0 {
                    return false;
                }
                lengths += 1;
            }
            _ => {}
        }
    }
    true
}

/// Parse the lowered list. Its grammar and token budget are checked here too,
/// so callers cannot accidentally accept an unchecked composite value.
#[must_use]
pub fn parse(value: &str) -> Option<Vec<Shadow>> {
    let tokens = lex_value(value)?;
    if !matches_tokens(CssProperty::BoxShadow.grammar(), &tokens, Mode::Lowered)
        || !valid_blurs(&tokens)
    {
        return None;
    }
    if matches!(tokens.as_slice(), [V::Ident(name)] if name.eq_ignore_ascii_case("none")) {
        return Some(Vec::new());
    }
    let mut result = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let mut lengths = [0.0; MAX_LENGTHS];
        let mut count = 0;
        let mut inset = false;
        let mut color = "currentcolor".to_owned();
        while let Some(token) = tokens.get(at) {
            match token {
                V::Comma => {
                    at += 1;
                    break;
                }
                V::Ident(name) if name.eq_ignore_ascii_case("inset") => {
                    inset = true;
                    at += 1;
                }
                V::Num { value, unit, .. } => {
                    if count == MAX_LENGTHS
                        || !value.is_finite()
                        || !matches!(unit, NumUnit::None | NumUnit::Dimension(_))
                    {
                        return None;
                    }
                    if !matches!(unit, NumUnit::None)
                        && !matches!(unit, NumUnit::Dimension(name) if name == "px")
                    {
                        return None;
                    }
                    lengths[count] = *value;
                    count += 1;
                    at += 1;
                }
                _ => {
                    let (_, end) = color::parse(&tokens, at)?;
                    color = join(&tokens[at..end].iter().map(token_text).collect::<Vec<_>>());
                    at = end;
                }
            }
        }
        result.push(Shadow {
            x: lengths[0],
            y: lengths[1],
            blur: lengths[2],
            spread: lengths[3],
            color,
            inset,
        });
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_lists_keep_positions_colors_and_nonnegative_blurs() {
        let value = "inset oklch(50% 0.1 30) 1em -2px calc(3px - 8px) -1px, 0 0 2px";
        assert!(property_value(CssProperty::BoxShadow, value));
        let lowered = lower(CssProperty::BoxShadow, value, &Context::NOMINAL).unwrap();
        let shadows = parse(&lowered).unwrap();
        assert_eq!(shadows.len(), 2);
        assert!(shadows[0].inset);
        assert_eq!(
            (shadows[0].y, shadows[0].blur, shadows[0].spread),
            (-2.0, 0.0, -1.0)
        );
        assert_eq!(shadows[1].color, "currentcolor");
        for value in [
            "red",
            "inset",
            "1px",
            "0 0 -1px",
            "0 0 1px red blue",
            "inset 0 0 inset",
            "0 0,",
            "0 0 2%",
        ] {
            assert!(!property_value(CssProperty::BoxShadow, value), "{value}");
        }
        assert!(property_value(CssProperty::BoxShadow, "0 0 -0px"));
        assert!(parse("none").unwrap().is_empty());
    }
}
