//! Properties the subset maps onto physical longhands when a stylesheet is
//! parsed: the flow-relative box, border and corner properties of CSS
//! Logical 1 (§4.1–§4.6) in the subset's one writing mode (`horizontal-tb`,
//! `ltr`: the inline start is the left side, the block start the top), the
//! shorthands `inset` (Positioned Layout 3 §3.1) and `flex` (Flexbox 1
//! §7.1), and the legacy name alias `-webkit-user-select` of `user-select`
//! (Cascade 5 §legacy name aliases, as Chrome implements it). A mapped declaration declares each of its longhands at its own
//! place in the cascade, as a shorthand does (Cascade 5 §1.2), so a later
//! or more specific `padding-left` and `padding-inline` win over each other
//! by the usual order (Logical 1 §4: they share computed values).
//!
//! A value with `var()` cannot be split until its references are
//! substituted: each longhand then holds the whole value and its
//! [`Part`], and is split at computed-value time (Variables 1 §3.1, the
//! pending-substitution value of a shorthand).
#[allow(unused_imports)]
use crate::view::prelude::*;

use super::CssProperty;
use super::syntax::{self, Token};

/// A mapped property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mapped {
    PaddingInline,
    PaddingBlock,
    PaddingInlineStart,
    PaddingInlineEnd,
    PaddingBlockStart,
    PaddingBlockEnd,
    MarginInline,
    MarginBlock,
    MarginInlineStart,
    MarginInlineEnd,
    MarginBlockStart,
    MarginBlockEnd,
    Inset,
    InsetInline,
    InsetBlock,
    InsetInlineStart,
    InsetInlineEnd,
    InsetBlockStart,
    InsetBlockEnd,
    Flex,
    BorderInline,
    BorderBlock,
    BorderInlineStart,
    BorderInlineEnd,
    BorderBlockStart,
    BorderBlockEnd,
    BorderInlineWidth,
    BorderInlineStyle,
    BorderInlineColor,
    BorderBlockWidth,
    BorderBlockStyle,
    BorderBlockColor,
    BorderInlineStartWidth,
    BorderInlineStartStyle,
    BorderInlineStartColor,
    BorderInlineEndWidth,
    BorderInlineEndStyle,
    BorderInlineEndColor,
    BorderBlockStartWidth,
    BorderBlockStartStyle,
    BorderBlockStartColor,
    BorderBlockEndWidth,
    BorderBlockEndStyle,
    BorderBlockEndColor,
    BorderStartStartRadius,
    BorderStartEndRadius,
    BorderEndStartRadius,
    BorderEndEndRadius,
    WebkitUserSelect,
    /// `font` (CSS Fonts 4 §2.8).
    Font,
    /// `list-style` (CSS Lists 3 §3.5).
    ListStyle,
    /// The legacy name alias `-webkit-appearance` (CSS UI 4 §7.1).
    WebkitAppearance,
    WebkitBackdropFilter,
    Animation,
    Transition,
}

/// One longhand of a mapped declaration whose value has `var()`: the mapped
/// property and the index of the longhand in [`Mapped::targets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub source: Mapped,
    pub index: usize,
}

/// How a mapped value's components spread over its longhands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spread {
    /// One component for one longhand.
    One,
    /// One or two: start and end (Logical 1 §4.2: `<'padding-left'>{1,2}`).
    Pair,
    /// One to four by the box rule: top, right, bottom, left.
    Box,
    /// `flex`: `none | [<'flex-grow'> <'flex-shrink'>? || <'flex-basis'>]`.
    Flex,
    /// The whole value for every longhand (a side's `border` shorthand, a
    /// corner's radii, a legacy alias).
    Whole,
    /// `font`: style, weight, size, line height and family, and the
    /// reset-only feature and variation settings.
    Font,
    /// `list-style`: type, position and image in any order.
    ListStyle,
    Animation,
    Transition,
}

use CssProperty as P;

impl Mapped {
    pub const ALL: [Self; 55] = [
        Self::PaddingInline,
        Self::PaddingBlock,
        Self::PaddingInlineStart,
        Self::PaddingInlineEnd,
        Self::PaddingBlockStart,
        Self::PaddingBlockEnd,
        Self::MarginInline,
        Self::MarginBlock,
        Self::MarginInlineStart,
        Self::MarginInlineEnd,
        Self::MarginBlockStart,
        Self::MarginBlockEnd,
        Self::Inset,
        Self::InsetInline,
        Self::InsetBlock,
        Self::InsetInlineStart,
        Self::InsetInlineEnd,
        Self::InsetBlockStart,
        Self::InsetBlockEnd,
        Self::Flex,
        Self::BorderInline,
        Self::BorderBlock,
        Self::BorderInlineStart,
        Self::BorderInlineEnd,
        Self::BorderBlockStart,
        Self::BorderBlockEnd,
        Self::BorderInlineWidth,
        Self::BorderInlineStyle,
        Self::BorderInlineColor,
        Self::BorderBlockWidth,
        Self::BorderBlockStyle,
        Self::BorderBlockColor,
        Self::BorderInlineStartWidth,
        Self::BorderInlineStartStyle,
        Self::BorderInlineStartColor,
        Self::BorderInlineEndWidth,
        Self::BorderInlineEndStyle,
        Self::BorderInlineEndColor,
        Self::BorderBlockStartWidth,
        Self::BorderBlockStartStyle,
        Self::BorderBlockStartColor,
        Self::BorderBlockEndWidth,
        Self::BorderBlockEndStyle,
        Self::BorderBlockEndColor,
        Self::BorderStartStartRadius,
        Self::BorderStartEndRadius,
        Self::BorderEndStartRadius,
        Self::BorderEndEndRadius,
        Self::WebkitUserSelect,
        Self::Font,
        Self::ListStyle,
        Self::WebkitAppearance,
        Self::WebkitBackdropFilter,
        Self::Animation,
        Self::Transition,
    ];

    /// The property name.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.row().0
    }

    /// The mapped property named `name` (ASCII case-insensitively).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|mapped| mapped.name().eq_ignore_ascii_case(name))
    }

    /// The physical longhands, in component order.
    #[must_use]
    pub fn targets(self) -> &'static [CssProperty] {
        self.row().2
    }

    fn row(self) -> (&'static str, Spread, &'static [CssProperty]) {
        const INLINE_PADDING: &[CssProperty] = &[P::PaddingLeft, P::PaddingRight];
        const BLOCK_PADDING: &[CssProperty] = &[P::PaddingTop, P::PaddingBottom];
        const INLINE_MARGIN: &[CssProperty] = &[P::MarginLeft, P::MarginRight];
        const BLOCK_MARGIN: &[CssProperty] = &[P::MarginTop, P::MarginBottom];
        const INSETS: &[CssProperty] = &[P::Top, P::Right, P::Bottom, P::Left];
        const INLINE_INSETS: &[CssProperty] = &[P::Left, P::Right];
        const BLOCK_INSETS: &[CssProperty] = &[P::Top, P::Bottom];
        const FLEX: &[CssProperty] = &[P::FlexGrow, P::FlexShrink, P::FlexBasis];
        const FONT: &[CssProperty] = &[
            P::FontStyle,
            P::FontWeight,
            P::FontSize,
            P::LineHeight,
            P::FontFamily,
            P::FontFeatureSettings,
            P::FontVariationSettings,
            P::FontVariantNumeric,
        ];
        match self {
            Self::PaddingInline => ("padding-inline", Spread::Pair, INLINE_PADDING),
            Self::PaddingBlock => ("padding-block", Spread::Pair, BLOCK_PADDING),
            Self::PaddingInlineStart => ("padding-inline-start", Spread::One, &[P::PaddingLeft]),
            Self::PaddingInlineEnd => ("padding-inline-end", Spread::One, &[P::PaddingRight]),
            Self::PaddingBlockStart => ("padding-block-start", Spread::One, &[P::PaddingTop]),
            Self::PaddingBlockEnd => ("padding-block-end", Spread::One, &[P::PaddingBottom]),
            Self::MarginInline => ("margin-inline", Spread::Pair, INLINE_MARGIN),
            Self::MarginBlock => ("margin-block", Spread::Pair, BLOCK_MARGIN),
            Self::MarginInlineStart => ("margin-inline-start", Spread::One, &[P::MarginLeft]),
            Self::MarginInlineEnd => ("margin-inline-end", Spread::One, &[P::MarginRight]),
            Self::MarginBlockStart => ("margin-block-start", Spread::One, &[P::MarginTop]),
            Self::MarginBlockEnd => ("margin-block-end", Spread::One, &[P::MarginBottom]),
            Self::Inset => ("inset", Spread::Box, INSETS),
            Self::InsetInline => ("inset-inline", Spread::Pair, INLINE_INSETS),
            Self::InsetBlock => ("inset-block", Spread::Pair, BLOCK_INSETS),
            Self::InsetInlineStart => ("inset-inline-start", Spread::One, &[P::Left]),
            Self::InsetInlineEnd => ("inset-inline-end", Spread::One, &[P::Right]),
            Self::InsetBlockStart => ("inset-block-start", Spread::One, &[P::Top]),
            Self::InsetBlockEnd => ("inset-block-end", Spread::One, &[P::Bottom]),
            Self::Flex => ("flex", Spread::Flex, FLEX),
            // Logical 1 §4.4: the flow-relative borders, as the physical
            // side shorthands and longhands.
            Self::BorderInline => (
                "border-inline",
                Spread::Whole,
                &[P::BorderLeft, P::BorderRight],
            ),
            Self::BorderBlock => (
                "border-block",
                Spread::Whole,
                &[P::BorderTop, P::BorderBottom],
            ),
            Self::BorderInlineStart => ("border-inline-start", Spread::Whole, &[P::BorderLeft]),
            Self::BorderInlineEnd => ("border-inline-end", Spread::Whole, &[P::BorderRight]),
            Self::BorderBlockStart => ("border-block-start", Spread::Whole, &[P::BorderTop]),
            Self::BorderBlockEnd => ("border-block-end", Spread::Whole, &[P::BorderBottom]),
            Self::BorderInlineWidth => (
                "border-inline-width",
                Spread::Pair,
                &[P::BorderLeftWidth, P::BorderRightWidth],
            ),
            Self::BorderInlineStyle => (
                "border-inline-style",
                Spread::Pair,
                &[P::BorderLeftStyle, P::BorderRightStyle],
            ),
            Self::BorderInlineColor => (
                "border-inline-color",
                Spread::Pair,
                &[P::BorderLeftColor, P::BorderRightColor],
            ),
            Self::BorderBlockWidth => (
                "border-block-width",
                Spread::Pair,
                &[P::BorderTopWidth, P::BorderBottomWidth],
            ),
            Self::BorderBlockStyle => (
                "border-block-style",
                Spread::Pair,
                &[P::BorderTopStyle, P::BorderBottomStyle],
            ),
            Self::BorderBlockColor => (
                "border-block-color",
                Spread::Pair,
                &[P::BorderTopColor, P::BorderBottomColor],
            ),
            Self::BorderInlineStartWidth => (
                "border-inline-start-width",
                Spread::One,
                &[P::BorderLeftWidth],
            ),
            Self::BorderInlineStartStyle => (
                "border-inline-start-style",
                Spread::One,
                &[P::BorderLeftStyle],
            ),
            Self::BorderInlineStartColor => (
                "border-inline-start-color",
                Spread::One,
                &[P::BorderLeftColor],
            ),
            Self::BorderInlineEndWidth => (
                "border-inline-end-width",
                Spread::One,
                &[P::BorderRightWidth],
            ),
            Self::BorderInlineEndStyle => (
                "border-inline-end-style",
                Spread::One,
                &[P::BorderRightStyle],
            ),
            Self::BorderInlineEndColor => (
                "border-inline-end-color",
                Spread::One,
                &[P::BorderRightColor],
            ),
            Self::BorderBlockStartWidth => (
                "border-block-start-width",
                Spread::One,
                &[P::BorderTopWidth],
            ),
            Self::BorderBlockStartStyle => (
                "border-block-start-style",
                Spread::One,
                &[P::BorderTopStyle],
            ),
            Self::BorderBlockStartColor => (
                "border-block-start-color",
                Spread::One,
                &[P::BorderTopColor],
            ),
            Self::BorderBlockEndWidth => (
                "border-block-end-width",
                Spread::One,
                &[P::BorderBottomWidth],
            ),
            Self::BorderBlockEndStyle => (
                "border-block-end-style",
                Spread::One,
                &[P::BorderBottomStyle],
            ),
            Self::BorderBlockEndColor => (
                "border-block-end-color",
                Spread::One,
                &[P::BorderBottomColor],
            ),
            // Logical 1 §4.6: the flow-relative corners.
            Self::BorderStartStartRadius => (
                "border-start-start-radius",
                Spread::Whole,
                &[P::BorderTopLeftRadius],
            ),
            Self::BorderStartEndRadius => (
                "border-start-end-radius",
                Spread::Whole,
                &[P::BorderTopRightRadius],
            ),
            Self::BorderEndStartRadius => (
                "border-end-start-radius",
                Spread::Whole,
                &[P::BorderBottomLeftRadius],
            ),
            Self::BorderEndEndRadius => (
                "border-end-end-radius",
                Spread::Whole,
                &[P::BorderBottomRightRadius],
            ),
            Self::WebkitUserSelect => ("-webkit-user-select", Spread::Whole, &[P::UserSelect]),
            Self::Font => ("font", Spread::Font, FONT),
            Self::ListStyle => (
                "list-style",
                Spread::ListStyle,
                &[P::ListStyleType, P::ListStylePosition, P::ListStyleImage],
            ),
            Self::WebkitAppearance => ("-webkit-appearance", Spread::Whole, &[P::Appearance]),
            Self::Transition => (
                "transition",
                Spread::Transition,
                &[
                    P::TransitionProperty,
                    P::TransitionDuration,
                    P::TransitionTimingFunction,
                    P::TransitionDelay,
                ],
            ),
            Self::Animation => (
                "animation",
                Spread::Animation,
                &[
                    P::AnimationName,
                    P::AnimationDuration,
                    P::AnimationTimingFunction,
                    P::AnimationDelay,
                    P::AnimationIterationCount,
                    P::AnimationDirection,
                    P::AnimationFillMode,
                    P::AnimationPlayState,
                ],
            ),
            Self::WebkitBackdropFilter => (
                "-webkit-backdrop-filter",
                Spread::Whole,
                &[P::BackdropFilter],
            ),
        }
    }

    /// The component of `value` (without `var()`) for each longhand, in
    /// [`Self::targets`] order; `None` outside the property's grammar of
    /// component counts. Each component still needs its longhand's grammar.
    /// A CSS-wide keyword is every longhand's value.
    #[must_use]
    pub fn components(self, value: &str) -> Option<Vec<String>> {
        let parts = split(value)?;
        let (_, spread, targets) = self.row();
        if spread == Spread::Whole {
            return (!parts.is_empty()).then(|| vec![parts.join(" "); targets.len()]);
        }
        if let [single] = parts.as_slice()
            && super::css::CSS_WIDE_KEYWORDS
                .iter()
                .any(|keyword| single.eq_ignore_ascii_case(keyword))
        {
            return Some(vec![single.clone(); targets.len()]);
        }
        match (spread, parts.as_slice()) {
            (Spread::One, [a]) => Some(vec![a.clone()]),
            (Spread::Pair, [a]) => Some(vec![a.clone(), a.clone()]),
            (Spread::Pair, [a, b]) => Some(vec![a.clone(), b.clone()]),
            (Spread::Box, [a]) => Some(vec![a.clone(), a.clone(), a.clone(), a.clone()]),
            (Spread::Box, [a, b]) => Some(vec![a.clone(), b.clone(), a.clone(), b.clone()]),
            (Spread::Box, [a, b, c]) => Some(vec![a.clone(), b.clone(), c.clone(), b.clone()]),
            (Spread::Box, [a, b, c, d]) => Some(vec![a.clone(), b.clone(), c.clone(), d.clone()]),
            (Spread::Flex, parts) => flex(parts),
            (Spread::Font, _) => font(value),
            (Spread::Transition, _) => transition(value),
            (Spread::Animation, _) => super::animation::components(value),
            (Spread::ListStyle, parts) => list_style(parts),
            _ => None,
        }
    }

    /// The component of `value` for longhand `index` (see
    /// [`Self::components`]).
    #[must_use]
    pub fn component(self, value: &str, index: usize) -> Option<String> {
        self.components(value)?.into_iter().nth(index)
    }
}

/// CSS Transitions 1: the first time is duration, the second delay; easing
/// keywords are consumed before an otherwise arbitrary property identifier.
fn transition(value: &str) -> Option<Vec<String>> {
    let entries = super::animation::list(value)?;
    let mut longhands: [Vec<String>; 4] = core::array::from_fn(|_| Vec::new());
    for entry in &entries {
        let mut values = [
            "all".to_owned(),
            "0s".to_owned(),
            "ease".to_owned(),
            "0s".to_owned(),
        ];
        let mut used = [false; 4];
        for part in split(entry)? {
            if super::css::CSS_WIDE_KEYWORDS
                .iter()
                .any(|v| part.eq_ignore_ascii_case(v))
            {
                return None;
            }
            let slot = if super::css::check(P::TransitionDelay, &part).is_ok() {
                if !used[1] {
                    1
                } else if !used[3] {
                    3
                } else {
                    return None;
                }
            } else if !used[2] && super::css::check(P::TransitionTimingFunction, &part).is_ok() {
                2
            } else if !used[0] && super::css::check(P::TransitionProperty, &part).is_ok() {
                0
            } else {
                return None;
            };
            if slot == 0 && part.eq_ignore_ascii_case("none") && entries.len() != 1 {
                return None;
            }
            used[slot] = true;
            values[slot] = part;
        }
        for (slot, value) in values.into_iter().enumerate() {
            longhands[slot].push(value);
        }
    }
    Some(longhands.into_iter().map(|v| v.join(", ")).collect())
}

/// The flex factors when `flex` omits them, and the basis (Flexbox 1 §7.1:
/// "When omitted from the flex shorthand, its specified value is 0", which
/// Chrome, the oracle, computes as `0%`).
const OMITTED_FACTOR: &str = "1";
const OMITTED_BASIS: &str = "0%";

/// `flex`'s longhands: grow, shrink, basis.
fn flex(parts: &[String]) -> Option<Vec<String>> {
    let triple = |grow: &str, shrink: &str, basis: &str| {
        Some(vec![grow.to_owned(), shrink.to_owned(), basis.to_owned()])
    };
    if let [single] = parts {
        if single.eq_ignore_ascii_case("none") {
            return triple("0", "0", "auto");
        }
        if single.eq_ignore_ascii_case("auto") {
            return triple("1", "1", "auto");
        }
    }
    let number = |text: &str| {
        matches!(
            syntax::tokenize(text).as_slice(),
            [syntax::Spanned {
                token: Token::Number(_),
                ..
            }]
        )
    };
    // A unitless zero not preceded by two factors is a factor (§7.1).
    let (factors, basis): (Vec<&String>, Vec<&String>) = match parts {
        [a, b, c] if number(a) && number(b) && number(c) => (vec![a, b], vec![c]),
        _ => parts.iter().partition(|part| number(part)),
    };
    let contiguous = match parts {
        [first, ..] if basis.first() == Some(&first) => {
            factors.iter().zip(&parts[1..]).all(|(a, b)| *a == b)
        }
        _ => factors.iter().zip(parts).all(|(a, b)| *a == b),
    };
    if factors.len() > 2 || basis.len() > 1 || factors.is_empty() && basis.is_empty() || !contiguous
    {
        return None;
    }
    triple(
        factors
            .first()
            .map_or(OMITTED_FACTOR, |factor| factor.as_str()),
        factors
            .get(1)
            .map_or(OMITTED_FACTOR, |factor| factor.as_str()),
        basis.first().map_or(OMITTED_BASIS, |basis| basis.as_str()),
    )
}

/// The keywords of `font`'s optional prefix (Fonts 4 §2.8): `normal` sets
/// nothing of its own; the style, weight and the CSS 2 variant and CSS 3
/// width keywords the renderer draws (`normal` only for the last two).
const FONT_STYLES: &[&str] = &["italic", "oblique"];
const FONT_WEIGHTS: &[&str] = &["bold", "bolder", "lighter"];
/// The absolute and relative `font-size` keywords (Fonts 4 §2.5).
const FONT_SIZE_KEYWORDS: &[&str] = &[
    "xx-small",
    "x-small",
    "small",
    "medium",
    "large",
    "x-large",
    "xx-large",
    "xxx-large",
    "larger",
    "smaller",
];
/// The reset-only longhands' initial values (§2.8: reset by the shorthand).
const FONT_RESET: &str = "normal";

/// `font`'s longhands (style, weight, size, line height, family, feature
/// settings, variation settings): `[<'font-style'> || <font-variant-css2>
/// || <'font-weight'> || <font-width-css3>]? <'font-size'> [/
/// <'line-height'>]? <'font-family'>#`. System font keywords and other
/// variant or width keywords are not expanded (`None`).
fn font(value: &str) -> Option<Vec<String>> {
    let tokens = syntax::tokenize(value);
    let meaningful: Vec<usize> = (0..tokens.len())
        .filter(|index| tokens[*index].token != Token::Whitespace)
        .collect();
    let mut style = None;
    let mut weight = None;
    let mut at = 0;
    // The prefix: at most four keywords or a weight number.
    while let Some(index) = meaningful.get(at).copied() {
        let token = &tokens[index].token;
        let keyword = match token {
            Token::Ident(word) => Some(word.to_ascii_lowercase()),
            _ => None,
        };
        let next_is_size = meaningful.get(at + 1).is_some();
        match (token, keyword.as_deref()) {
            (_, Some("normal")) => {}
            (_, Some(word)) if FONT_STYLES.contains(&word) && style.is_none() => {
                style = Some(word.to_owned());
            }
            (_, Some(word)) if FONT_WEIGHTS.contains(&word) && weight.is_none() => {
                weight = Some(word.to_owned());
            }
            (Token::Number(number), _) if weight.is_none() && next_is_size => {
                weight = Some(number.repr.clone());
            }
            _ => break,
        }
        at += 1;
        if at > 4 {
            return None;
        }
    }
    // The size: a length, percentage, keyword or math function, then an
    // optional `/ <'line-height'>`, then the family list.
    let size_start = *meaningful.get(at)?;
    let size_end = match &tokens[size_start].token {
        Token::Dimension(..) | Token::Percentage(_) | Token::Number(_) => size_start + 1,
        Token::Ident(word) if FONT_SIZE_KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) => {
            size_start + 1
        }
        Token::Function(_) => block_end(&tokens, size_start)? + 1,
        _ => return None,
    };
    let text = |from: usize, to: usize| -> String {
        value[tokens[from].start..tokens[to - 1].end].to_owned()
    };
    let size = text(size_start, size_end);
    let mut cursor = size_end;
    let skip = |cursor: &mut usize| {
        while tokens
            .get(*cursor)
            .is_some_and(|t| t.token == Token::Whitespace)
        {
            *cursor += 1;
        }
    };
    skip(&mut cursor);
    let mut line_height = FONT_RESET.to_owned();
    if tokens.get(cursor).map(|t| &t.token) == Some(&Token::Delim('/')) {
        cursor += 1;
        skip(&mut cursor);
        let start = cursor;
        let end = match &tokens.get(start)?.token {
            Token::Function(_) => block_end(&tokens, start)? + 1,
            _ => start + 1,
        };
        line_height = text(start, end);
        cursor = end;
        skip(&mut cursor);
    }
    if cursor >= tokens.len() {
        return None;
    }
    let family = text(cursor, tokens.len()).trim_end().to_owned();
    Some(vec![
        style.unwrap_or_else(|| FONT_RESET.to_owned()),
        weight.unwrap_or_else(|| FONT_RESET.to_owned()),
        size,
        line_height,
        family,
        FONT_RESET.to_owned(),
        FONT_RESET.to_owned(),
        FONT_RESET.to_owned(),
    ])
}

/// The index of the `)` closing the function or block opened at `open`.
fn block_end(tokens: &[syntax::Spanned], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, spanned) in tokens.iter().enumerate().skip(open) {
        match spanned.token {
            Token::Function(_) | Token::OpenParen => depth += 1,
            Token::CloseParen => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `list-style-position` keywords (CSS Lists 3 §3.4).
const LIST_POSITIONS: &[&str] = &["inside", "outside"];
const LIST_NONE: &str = "none";
/// The initial values of the three longhands.
const LIST_INITIAL: [&str; 3] = ["disc", "outside", "none"];

/// `list-style`'s longhands (type, position, image): `<'list-style-position'>
/// || <'list-style-image'> || <'list-style-type'>`, where `none` sets
/// whichever of the image and type is otherwise unset (Lists 3 §3.5).
fn list_style(parts: &[String]) -> Option<Vec<String>> {
    let mut kind = None;
    let mut position = None;
    let mut image = None;
    let mut nones = 0usize;
    for part in parts {
        let lower = part.to_ascii_lowercase();
        if LIST_POSITIONS.contains(&lower.as_str()) {
            if position.replace(part.clone()).is_some() {
                return None;
            }
        } else if lower == LIST_NONE {
            nones += 1;
        } else if part.ends_with(')') && !lower.starts_with("symbols(") {
            if image.replace(part.clone()).is_some() {
                return None;
            }
        } else if kind.replace(part.clone()).is_some() {
            return None;
        }
    }
    let none = || LIST_NONE.to_owned();
    match (nones, kind.is_some(), image.is_some()) {
        (0, ..) => {}
        (1, false, _) => kind = Some(none()),
        (1, true, false) => image = Some(none()),
        (2, false, false) => {
            kind = Some(none());
            image = Some(none());
        }
        _ => return None,
    }
    if parts.is_empty() {
        return None;
    }
    Some(vec![
        kind.unwrap_or_else(|| LIST_INITIAL[0].to_owned()),
        position.unwrap_or_else(|| LIST_INITIAL[1].to_owned()),
        image.unwrap_or_else(|| LIST_INITIAL[2].to_owned()),
    ])
}

/// The top-level components of a value: whitespace-separated, functions
/// and blocks whole. `None` for unbalanced parentheses.
pub fn split(value: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let mut end = 0;
    for spanned in syntax::tokenize(value) {
        match spanned.token {
            Token::Whitespace if depth == 0 => {
                if let Some(at) = start.take() {
                    parts.push(value[at..end].to_owned());
                }
                continue;
            }
            Token::Function(_) | Token::OpenParen => depth += 1,
            Token::CloseParen => depth = depth.checked_sub(1)?,
            _ => {}
        }
        start.get_or_insert(spanned.start);
        end = spanned.end;
    }
    if depth != 0 {
        return None;
    }
    if let Some(at) = start {
        parts.push(value[at..end].to_owned());
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(mapped: Mapped, value: &str) -> Option<Vec<String>> {
        mapped.components(value)
    }
    fn texts(values: &[&str]) -> Option<Vec<String>> {
        Some(values.iter().map(|value| (*value).to_owned()).collect())
    }

    #[test]
    fn logical_properties_map_onto_the_horizontal_ltr_sides() {
        assert_eq!(
            Mapped::PaddingInline.targets(),
            &[P::PaddingLeft, P::PaddingRight]
        );
        assert_eq!(Mapped::MarginBlockEnd.targets(), &[P::MarginBottom]);
        assert_eq!(Mapped::InsetInlineStart.targets(), &[P::Left]);
        assert_eq!(Mapped::parse("PADDING-INLINE"), Some(Mapped::PaddingInline));
        assert_eq!(Mapped::parse("padding-inline-x"), None);
        assert_eq!(
            parts(Mapped::PaddingInline, "calc(1px + 2px)  4%"),
            texts(&["calc(1px + 2px)", "4%"])
        );
        assert_eq!(parts(Mapped::MarginBlock, "auto"), texts(&["auto", "auto"]));
        assert_eq!(parts(Mapped::MarginInlineStart, "1px 2px"), None);
        assert_eq!(parts(Mapped::PaddingBlock, "1px 2px 3px"), None);
        assert_eq!(
            parts(Mapped::Inset, "1px 2px 3px"),
            texts(&["1px", "2px", "3px", "2px"])
        );
        assert_eq!(parts(Mapped::Inset, "inherit"), texts(&["inherit"; 4]));
        assert_eq!(parts(Mapped::Inset, "min(1px, 2px"), None);
    }

    /// Logical 1 §4.4 and §4.6, and the legacy alias of `user-select`.
    #[test]
    fn logical_borders_and_corners_map_onto_the_physical_sides() {
        assert_eq!(
            Mapped::BorderInline.targets(),
            &[P::BorderLeft, P::BorderRight]
        );
        assert_eq!(
            parts(Mapped::BorderBlock, "1px  solid red"),
            texts(&["1px solid red", "1px solid red"])
        );
        assert_eq!(
            parts(Mapped::BorderInlineWidth, "1px 2px"),
            texts(&["1px", "2px"])
        );
        assert_eq!(
            Mapped::BorderBlockEndColor.targets(),
            &[P::BorderBottomColor]
        );
        assert_eq!(
            parts(Mapped::BorderEndStartRadius, "4px 8px"),
            texts(&["4px 8px"])
        );
        assert_eq!(
            Mapped::BorderEndStartRadius.targets(),
            &[P::BorderBottomLeftRadius]
        );
        assert_eq!(
            Mapped::parse("-WEBKIT-USER-SELECT"),
            Some(Mapped::WebkitUserSelect)
        );
        assert_eq!(parts(Mapped::BorderInline, ""), None);
    }

    /// Fonts 4 §2.8: the shorthand's set and reset-only longhands.
    #[test]
    fn font_sets_and_resets_its_longhands() {
        assert_eq!(parts(Mapped::Font, "inherit"), texts(&["inherit"; 8]));
        assert_eq!(
            parts(
                Mapped::Font,
                "italic bold 12px/1.5 \"Helvetica Neue\", serif"
            ),
            texts(&[
                "italic",
                "bold",
                "12px",
                "1.5",
                "\"Helvetica Neue\", serif",
                "normal",
                "normal",
                "normal"
            ])
        );
        assert_eq!(
            parts(Mapped::Font, "600 calc(1rem + 2px) / 20px monospace"),
            texts(&[
                "normal",
                "600",
                "calc(1rem + 2px)",
                "20px",
                "monospace",
                "normal",
                "normal",
                "normal"
            ])
        );
        assert_eq!(
            parts(Mapped::Font, "normal 1em serif"),
            texts(&[
                "normal", "normal", "1em", "normal", "serif", "normal", "normal", "normal"
            ])
        );
        assert_eq!(parts(Mapped::Font, "12px"), None);
        assert_eq!(parts(Mapped::Font, "caption"), None);
    }

    /// Lists 3 §3.5: `none` sets whichever of the image and type is unset.
    #[test]
    fn list_style_resolves_none() {
        assert_eq!(
            parts(Mapped::ListStyle, "none"),
            texts(&["none", "outside", "none"])
        );
        assert_eq!(
            parts(Mapped::ListStyle, "square inside"),
            texts(&["square", "inside", "none"])
        );
        assert_eq!(
            parts(Mapped::ListStyle, "none none"),
            texts(&["none", "outside", "none"])
        );
        assert_eq!(
            parts(Mapped::ListStyle, "disc none"),
            texts(&["disc", "outside", "none"])
        );
        assert_eq!(parts(Mapped::ListStyle, "none none none"), None);
        assert_eq!(parts(Mapped::ListStyle, "inside outside"), None);
        assert_eq!(parts(Mapped::WebkitAppearance, "none"), texts(&["none"]));
    }

    #[test]
    fn transition_lists_preserve_function_commas_order_and_reset_defaults() {
        assert!(super::super::css::check(P::TransitionDuration, "-1s").is_err());
        assert_eq!(
            parts(
                Mapped::Transition,
                "box-shadow 100ms steps(2, jump-end) -20ms, opacity 1s"
            ),
            texts(&[
                "box-shadow, opacity",
                "100ms, 1s",
                "steps(2, jump-end), ease",
                "-20ms, 0s"
            ])
        );
        assert_eq!(
            parts(Mapped::Transition, "ease 1s"),
            texts(&["all", "1s", "ease", "0s"])
        );
        assert_eq!(
            parts(Mapped::Transition, "none"),
            texts(&["none", "0s", "ease", "0s"])
        );
        assert_eq!(parts(Mapped::Transition, "inherit"), texts(&["inherit"; 4]));
        assert_eq!(parts(Mapped::Transition, "none, opacity 1s"), None);
        assert_eq!(parts(Mapped::Transition, "all 1s 2s 3s"), None);
        assert_eq!(parts(Mapped::Transition, "box-shadow 1s linear ease"), None);
    }

    /// Flexbox 1 §7.1.1 and the shorthand's grammar.
    #[test]
    fn flex_expands_its_keywords_and_omitted_components() {
        assert_eq!(parts(Mapped::Flex, "1"), texts(&["1", "1", "0%"]));
        assert_eq!(parts(Mapped::Flex, "auto"), texts(&["1", "1", "auto"]));
        assert_eq!(parts(Mapped::Flex, "none"), texts(&["0", "0", "auto"]));
        assert_eq!(parts(Mapped::Flex, "2 3"), texts(&["2", "3", "0%"]));
        assert_eq!(parts(Mapped::Flex, "1 1 0"), texts(&["1", "1", "0"]));
        assert_eq!(parts(Mapped::Flex, "10px"), texts(&["1", "1", "10px"]));
        assert_eq!(parts(Mapped::Flex, "50% 2"), texts(&["2", "1", "50%"]));
        assert_eq!(parts(Mapped::Flex, "2 0 auto"), texts(&["2", "0", "auto"]));
        assert_eq!(parts(Mapped::Flex, "1 2 3 4"), None);
        assert_eq!(parts(Mapped::Flex, "1 10px 2"), None);
        assert_eq!(parts(Mapped::Flex, "1px 2px"), None);
    }
}
