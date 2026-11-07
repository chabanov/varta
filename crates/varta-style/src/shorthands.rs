//! Shorthands whose longhands are registered properties too (Cascade 5
//! §1.2): where a shorthand's declaration wins over a longhand's, the
//! longhand's computed value is the shorthand's component, so a host that
//! applies the computed values in registry order (the browser bridge sets
//! each inline, shorthands before their longhands) gets the cascade's
//! result. An invalid winning shorthand leaves the longhand unset.
use crate::Computed;
use crate::prelude::*;
use varta_common::view::CssProperty as P;
use varta_common::view::css::LINE_WIDTH_KEYWORDS;
use varta_common::view::mapped::split;

/// How a shorthand's value gives one longhand its component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Component {
    /// Index `i` of `n` longhands by the box rule (CSS Box 4 §2: top,
    /// right, bottom, left; two longhands take the first two values).
    Box(usize, usize),
    /// The whole value (`border` for a side's `border-top`).
    Whole,
    /// The `<line-width>` of `<line-width> || <line-style> || <color>`.
    Width,
    /// The `<line-style>` of a border or outline shorthand.
    Style,
    /// The `<color>` of a border or outline shorthand.
    Color,
    WrapMode,
    WrapStyle,
    WhiteSpaceMode,
}

const SIDES: [P; 4] = [P::BorderTop, P::BorderRight, P::BorderBottom, P::BorderLeft];
const WIDTHS: [P; 4] = [
    P::BorderTopWidth,
    P::BorderRightWidth,
    P::BorderBottomWidth,
    P::BorderLeftWidth,
];
const STYLES: [P; 4] = [
    P::BorderTopStyle,
    P::BorderRightStyle,
    P::BorderBottomStyle,
    P::BorderLeftStyle,
];
const COLORS: [P; 4] = [
    P::BorderTopColor,
    P::BorderRightColor,
    P::BorderBottomColor,
    P::BorderLeftColor,
];

/// The registered longhands (and nested shorthands) a shorthand sets, with
/// the component each takes.
fn covered(shorthand: P) -> Vec<(P, Component)> {
    let boxed = |longhands: &[P]| {
        longhands
            .iter()
            .enumerate()
            .map(|(index, longhand)| (*longhand, Component::Box(index, longhands.len())))
            .collect::<Vec<_>>()
    };
    match shorthand {
        P::TextWrap => vec![
            (P::TextWrapMode, Component::WrapMode),
            (P::TextWrapStyle, Component::WrapStyle),
        ],
        P::WhiteSpace => vec![(P::TextWrapMode, Component::WhiteSpaceMode)],
        P::Margin => boxed(&[P::MarginTop, P::MarginRight, P::MarginBottom, P::MarginLeft]),
        P::Padding => boxed(&[
            P::PaddingTop,
            P::PaddingRight,
            P::PaddingBottom,
            P::PaddingLeft,
        ]),
        P::Gap => boxed(&[P::RowGap, P::ColumnGap]),
        P::Overflow => boxed(&[P::OverflowX, P::OverflowY]),
        P::BorderWidth => boxed(&WIDTHS),
        P::BorderStyle => boxed(&STYLES),
        P::BorderColor => boxed(&COLORS),
        P::BorderRadius => boxed(&[
            P::BorderTopLeftRadius,
            P::BorderTopRightRadius,
            P::BorderBottomRightRadius,
            P::BorderBottomLeftRadius,
        ]),
        P::Border => {
            let mut out: Vec<(P, Component)> =
                SIDES.iter().map(|side| (*side, Component::Whole)).collect();
            out.extend([
                (P::BorderWidth, Component::Width),
                (P::BorderStyle, Component::Style),
                (P::BorderColor, Component::Color),
            ]);
            out.extend(WIDTHS.iter().map(|longhand| (*longhand, Component::Width)));
            out.extend(STYLES.iter().map(|longhand| (*longhand, Component::Style)));
            out.extend(COLORS.iter().map(|longhand| (*longhand, Component::Color)));
            out
        }
        P::BorderTop | P::BorderRight | P::BorderBottom | P::BorderLeft => {
            let side = SIDES
                .iter()
                .position(|side| *side == shorthand)
                .unwrap_or(0);
            vec![
                (WIDTHS[side], Component::Width),
                (STYLES[side], Component::Style),
                (COLORS[side], Component::Color),
            ]
        }
        P::Outline => vec![
            (P::OutlineWidth, Component::Width),
            (P::OutlineStyle, Component::Style),
            (P::OutlineColor, Component::Color),
        ],
        _ => Vec::new(),
    }
}

/// The shorthands of [`covered`], in registry order.
const SHORTHANDS: [P; 16] = [
    P::TextWrap,
    P::WhiteSpace,
    P::Margin,
    P::Padding,
    P::Gap,
    P::Overflow,
    P::Border,
    P::BorderWidth,
    P::BorderStyle,
    P::BorderColor,
    P::Outline,
    P::BorderRadius,
    P::BorderTop,
    P::BorderRight,
    P::BorderBottom,
    P::BorderLeft,
];

/// The `<line-style>` keywords (CSS Backgrounds 3 §3.2), and `auto`, which
/// only `outline-style` takes.
const LINE_STYLES: [&str; 11] = [
    "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset",
    "auto",
];

/// The component of a border or outline shorthand's part: its kind.
fn part_kind(part: &str) -> Component {
    let keyword = |list: &[&str]| list.iter().any(|word| part.eq_ignore_ascii_case(word));
    if keyword(&LINE_STYLES) {
        Component::Style
    } else if keyword(&LINE_WIDTH_KEYWORDS.map(|(name, _)| name))
        || part.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
        || ["calc(", "min(", "max(", "clamp("]
            .iter()
            .any(|f| part.len() >= f.len() && part[..f.len()].eq_ignore_ascii_case(f))
    {
        Component::Width
    } else {
        Component::Color
    }
}

/// One longhand's component of a computed shorthand value; `None` when the
/// value has no such component (the longhand is then unset).
fn component(value: &Computed, wanted: Component) -> Option<Computed> {
    let Computed::Value(text) = value else {
        return Some(value.clone());
    };
    let parts = split(text)?;
    match wanted {
        Component::Whole => Some(value.clone()),
        Component::WhiteSpaceMode => Some(Computed::Value(
            if matches!(text.to_ascii_lowercase().as_str(), "nowrap" | "pre") {
                "nowrap"
            } else {
                "wrap"
            }
            .into(),
        )),
        Component::WrapMode => Some(Computed::Value(
            parts
                .iter()
                .find(|p| matches!(p.to_ascii_lowercase().as_str(), "wrap" | "nowrap"))
                .map_or("wrap", String::as_str)
                .to_ascii_lowercase(),
        )),
        Component::WrapStyle => Some(Computed::Value(
            parts
                .iter()
                .find(|p| matches!(p.to_ascii_lowercase().as_str(), "auto" | "pretty"))
                .map_or("auto", String::as_str)
                .to_ascii_lowercase(),
        )),
        Component::Box(index, longhands) => {
            let at = match (longhands, parts.len()) {
                (_, 1) => 0,
                (2, _) => index.min(1),
                (_, 2) => index % 2,
                (_, 3) => [0, 1, 2, 1][index],
                _ => index,
            };
            parts.get(at).cloned().map(Computed::Value)
        }
        // CSS Backgrounds 3 §3.6: an omitted component is reset to its
        // initial value.
        kind => Some(
            parts
                .into_iter()
                .find(|part| part_kind(part) == kind)
                .map_or(Computed::Initial, Computed::Value),
        ),
    }
}

/// Give each longhand (or nested shorthand) the component of the
/// highest-ranked declared shorthand that covers it and wins over its own
/// declaration (Cascade 5 §1.2).
pub(crate) fn resolve(
    out: &mut BTreeMap<P, Computed>,
    ranks: &BTreeMap<P, usize>,
    wanted: &impl Fn(P) -> bool,
) {
    // Each covered property's best covering shorthand: (rank, shorthand,
    // component).
    let mut best: BTreeMap<P, (usize, P, Component)> = BTreeMap::new();
    for shorthand in SHORTHANDS {
        let Some(&rank) = ranks.get(&shorthand).filter(|_| wanted(shorthand)) else {
            continue;
        };
        for (longhand, how) in covered(shorthand) {
            if best.get(&longhand).is_none_or(|(other, ..)| *other < rank) {
                best.insert(longhand, (rank, shorthand, how));
            }
        }
    }
    for (longhand, (rank, shorthand, how)) in best {
        let text_wrap = matches!(
            how,
            Component::WrapMode | Component::WrapStyle | Component::WhiteSpaceMode
        );
        if ranks.get(&longhand).map_or(!text_wrap, |own| *own > rank) {
            continue;
        }
        match out.get(&shorthand).and_then(|value| component(value, how)) {
            Some(value) => {
                out.insert(longhand, value);
            }
            None => {
                out.remove(&longhand);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> Computed {
        Computed::Value(text.into())
    }

    #[test]
    fn border_components_pick_width_style_and_colour() {
        let shorthand = value("2px solid rgba(1, 2, 3, 1)");
        assert_eq!(component(&shorthand, Component::Width), Some(value("2px")));
        assert_eq!(
            component(&shorthand, Component::Style),
            Some(value("solid"))
        );
        assert_eq!(
            component(&shorthand, Component::Color),
            Some(value("rgba(1, 2, 3, 1)"))
        );
        // An omitted component is the longhand's initial value.
        assert_eq!(
            component(&value("solid"), Component::Width),
            Some(Computed::Initial)
        );
        assert_eq!(
            component(&value("1px 2px 3px"), Component::Box(3, 4)),
            Some(value("2px"))
        );
        assert_eq!(
            component(&value("hidden auto"), Component::Box(1, 2)),
            Some(value("auto"))
        );
    }

    #[test]
    fn the_highest_ranked_covering_shorthand_gives_the_component() {
        let mut out = BTreeMap::from([
            (P::Border, value("1px solid red")),
            (P::BorderTop, value("2px solid blue")),
            (P::BorderTopWidth, value("5px")),
            (P::BorderLeftColor, value("green")),
        ]);
        let ranks = BTreeMap::from([
            (P::BorderTop, 1),
            (P::Border, 5),
            (P::BorderLeftColor, 3),
            (P::BorderTopWidth, 10),
        ]);
        resolve(&mut out, &ranks, &|_| true);
        assert_eq!(out[&P::BorderTop], value("1px solid red"));
        assert_eq!(out[&P::BorderTopWidth], value("5px"));
        assert_eq!(out[&P::BorderLeftColor], value("red"));
    }
}
