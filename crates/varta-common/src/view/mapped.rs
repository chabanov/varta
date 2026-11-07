//! Properties the subset maps onto physical longhands when a stylesheet is
//! parsed: the flow-relative box properties of CSS Logical 1 (§4.1, §4.2,
//! §4.3) in the subset's one writing mode (`horizontal-tb`, `ltr`: the
//! inline start is the left side, the block start the top), and the
//! shorthands `inset` (Positioned Layout 3 §3.1) and `flex` (Flexbox 1
//! §7.1). A mapped declaration declares each of its longhands at its own
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
}

use CssProperty as P;

impl Mapped {
    pub const ALL: [Self; 20] = [
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

/// The top-level components of a value: whitespace-separated, functions
/// and blocks whole. `None` for unbalanced parentheses.
pub(crate) fn split(value: &str) -> Option<Vec<String>> {
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
