//! CSS Images 3/4 linear gradients, over the existing bounded token grammar.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Angle(f64),
    Corner([i8; 2]),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    pub color: String,
    pub positions: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub direction: Direction,
    pub space: color::Space,
    pub stops: Vec<Stop>,
}

fn segments(tokens: &[V]) -> Option<Vec<&[V]>> {
    if !matches!(tokens.first(),Some(V::Function(name)) if name.eq_ignore_ascii_case("linear-gradient"))
        || !matches!(tokens.last(), Some(V::Close))
    {
        return None;
    }
    let body = &tokens[1..tokens.len() - 1];
    let mut depth = 0;
    let mut start = 0;
    let mut parts = Vec::new();
    for (at, token) in body.iter().enumerate() {
        match token {
            V::Function(_) => depth += 1,
            V::Close => depth -= 1,
            V::Comma if depth == 0 => {
                parts.push(&body[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    Some(parts)
}
pub(super) fn valid_count(tokens: &[V]) -> bool {
    if matches!(tokens,[V::Ident(name)] if name.eq_ignore_ascii_case("none")) {
        return true;
    }
    if tokens
        .iter()
        .any(|t| matches!(t,V::Function(n) if n.eq_ignore_ascii_case("var")))
    {
        return true;
    }
    let Some(parts) = segments(tokens) else {
        return false;
    };
    let colors = parts
        .first()
        .is_some_and(|first| color::parse(first, 0).is_some());
    parts.len() >= if colors { 2 } else { 3 }
}
/// Parse lowered gradients; absent/invalid variables have already resolved.
#[must_use]
pub fn parse(value: &str) -> Option<Option<Gradient>> {
    let tokens = lex_value(value)?;
    if matches!(tokens.as_slice(),[V::Ident(name)] if name.eq_ignore_ascii_case("none")) {
        return Some(None);
    }
    if !matches_tokens(
        CssProperty::BackgroundImage.grammar(),
        &tokens,
        Mode::Lowered,
    ) || !valid_count(&tokens)
    {
        return None;
    }
    let mut parts = segments(&tokens)?.into_iter().peekable();
    let mut direction = Direction::Angle(180.0);
    let mut space = color::Space::Srgb;
    if parts.peek().is_some_and(|p| color::parse(p, 0).is_none()) {
        let header = parts.next()?;
        let mut at = 0;
        if let Some(V::Num {
            value,
            unit: NumUnit::Dimension(unit),
            ..
        }) = header.first()
        {
            direction = Direction::Angle(*value * math::angle_unit(unit)?);
            at = 1;
        } else if matches!(header.first(),Some(V::Ident(n)) if n.eq_ignore_ascii_case("to")) {
            let mut corner = [0; 2];
            at = 1;
            while let Some(V::Ident(n)) = header.get(at) {
                let (axis, sign) = match n.to_ascii_lowercase().as_str() {
                    "left" => (0, -1),
                    "right" => (0, 1),
                    "top" => (1, -1),
                    "bottom" => (1, 1),
                    _ => break,
                };
                corner[axis] = sign;
                at += 1;
            }
            direction = Direction::Corner(corner);
        }
        if let Some(V::Ident(n)) = header.get(at)
            && n.eq_ignore_ascii_case("in")
        {
            let V::Ident(n) = header.get(at + 1)? else {
                return None;
            };
            space = match n.to_ascii_lowercase().as_str() {
                "srgb" => color::Space::Srgb,
                "srgb-linear" => color::Space::SrgbLinear,
                "oklab" => color::Space::Oklab,
                _ => return None,
            };
            at += 2;
        }
        if at != header.len() {
            return None;
        }
    }
    let mut stops = Vec::new();
    for part in parts {
        let (_, end) = color::parse(part, 0)?;
        let color = join(&part[..end].iter().map(token_text).collect::<Vec<_>>());
        let positions = part[end..]
            .iter()
            .map(|token| matches!(token, V::Num { .. }).then(|| token_text(token)))
            .collect::<Option<_>>()?;
        stops.push(Stop { color, positions });
    }
    Some(Some(Gradient {
        direction,
        space,
        stops,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradients_require_two_stops_and_preserve_directions_positions_and_interpolation() {
        for value in [
            "linear-gradient(red)",
            "linear-gradient(to bottom,red)",
            "linear-gradient(to left right,red,blue)",
            "linear-gradient(red 1px 2px 3px,blue)",
        ] {
            assert!(
                !property_value(CssProperty::BackgroundImage, value),
                "{value}"
            );
        }
        let value = "linear-gradient(0.25turn in oklab,red 10% 20%,transparent,blue 5rem)";
        assert!(property_value(CssProperty::BackgroundImage, value));
        let lowered = lower(CssProperty::BackgroundImage, value, &Context::NOMINAL).unwrap();
        let gradient = parse(&lowered).unwrap().unwrap();
        assert_eq!(gradient.direction, Direction::Angle(90.0));
        assert_eq!(gradient.space, color::Space::Oklab);
        assert_eq!(gradient.stops.len(), 3);
        assert_eq!(gradient.stops[0].positions, ["10%", "20%"]);
    }
}
