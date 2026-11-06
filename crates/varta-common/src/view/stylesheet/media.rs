//! `@media` preludes of Media Queries 4 (§2–§4) and the user-preference
//! features of Media Queries 5 (§12), evaluated against recorded host
//! inputs ([`Media`]) so that rendering stays deterministic.
//!
//! Admitted: query lists (`,`), `not`/`only` with the media types `all`,
//! `screen` (the renderer's type) and `print` (never matches), conditions
//! with `not`, `and`, `or` and parentheses (§3), and the features `width`,
//! `height` (range type, `min-`/`max-` prefixes and the range syntax of
//! §2.4.3), `orientation`, `prefers-color-scheme`, `prefers-reduced-motion`,
//! `prefers-contrast`, `forced-colors`, `hover`, `any-hover`, `pointer` and
//! `any-pointer`, in plain, boolean (§2.4.2) and range forms. Lengths are
//! `0`, absolute units, `em` and `rem` (16px, the initial font size, §1.3).
//! Any other feature, value or `<general-enclosed>` is a violation rather
//! than the `unknown` of §3.2: the subset fails closed.
use super::super::css::math;
use super::super::syntax::{Spanned, Token};
use super::REM_PIXELS;
#[allow(unused_imports)]
use crate::view::prelude::*;

/// `prefers-contrast` (Media Queries 5 §12.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Contrast {
    #[default]
    NoPreference,
    More,
    Less,
    Custom,
}
/// `hover`/`any-hover` (Media Queries 4 §5.3, §5.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Hover {
    None,
    #[default]
    Hover,
}
/// `pointer`/`any-pointer` (Media Queries 4 §5.2, §5.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Pointer {
    None,
    Coarse,
    #[default]
    Fine,
}

/// Recorded host inputs of the user-preference and interaction features.
/// The defaults are a desktop with a mouse and no stated preference, which
/// is what headless Chrome reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Preferences {
    pub reduced_motion: bool,
    pub contrast: Contrast,
    pub forced_colors: bool,
    pub hover: Hover,
    pub any_hover: Hover,
    pub pointer: Pointer,
    pub any_pointer: Pointer,
}

/// The environment media queries are evaluated against: the viewport in CSS
/// pixels, the colour scheme and the recorded preferences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Media {
    pub width: f64,
    pub height: f64,
    pub dark: bool,
    pub preferences: Preferences,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    All,
    Screen,
    Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureName {
    Width,
    Height,
    Orientation,
    PrefersColorScheme,
    PrefersReducedMotion,
    PrefersContrast,
    ForcedColors,
    Hover,
    AnyHover,
    Pointer,
    AnyPointer,
}
impl FeatureName {
    /// Every feature with its name and, for discrete features, its values.
    const ALL: &[(Self, &'static str, &'static [&'static str])] = &[
        (Self::Width, "width", &[]),
        (Self::Height, "height", &[]),
        (Self::Orientation, "orientation", &["portrait", "landscape"]),
        (
            Self::PrefersColorScheme,
            "prefers-color-scheme",
            &["light", "dark"],
        ),
        (
            Self::PrefersReducedMotion,
            "prefers-reduced-motion",
            &["no-preference", "reduce"],
        ),
        (
            Self::PrefersContrast,
            "prefers-contrast",
            &["no-preference", "more", "less", "custom"],
        ),
        (Self::ForcedColors, "forced-colors", &["none", "active"]),
        (Self::Hover, "hover", &["none", "hover"]),
        (Self::AnyHover, "any-hover", &["none", "hover"]),
        (Self::Pointer, "pointer", &["none", "coarse", "fine"]),
        (Self::AnyPointer, "any-pointer", &["none", "coarse", "fine"]),
    ];
    fn named(name: &str) -> Option<(Self, &'static [&'static str])> {
        Self::ALL
            .iter()
            .find(|(_, known, _)| name.eq_ignore_ascii_case(known))
            .map(|(feature, _, values)| (*feature, *values))
    }
    fn range(self) -> bool {
        matches!(self, Self::Width | Self::Height)
    }
    /// The discrete value the environment has.
    fn value(self, media: &Media) -> &'static str {
        let p = &media.preferences;
        let hover = |h: Hover| if h == Hover::Hover { "hover" } else { "none" };
        let pointer = |p: Pointer| match p {
            Pointer::None => "none",
            Pointer::Coarse => "coarse",
            Pointer::Fine => "fine",
        };
        match self {
            Self::Width | Self::Height => "",
            Self::Orientation => {
                if media.height >= media.width {
                    "portrait"
                } else {
                    "landscape"
                }
            }
            Self::PrefersColorScheme => {
                if media.dark {
                    "dark"
                } else {
                    "light"
                }
            }
            Self::PrefersReducedMotion => {
                if p.reduced_motion {
                    "reduce"
                } else {
                    "no-preference"
                }
            }
            Self::PrefersContrast => match p.contrast {
                Contrast::NoPreference => "no-preference",
                Contrast::More => "more",
                Contrast::Less => "less",
                Contrast::Custom => "custom",
            },
            Self::ForcedColors => {
                if p.forced_colors {
                    "active"
                } else {
                    "none"
                }
            }
            Self::Hover => hover(p.hover),
            Self::AnyHover => hover(p.any_hover),
            Self::Pointer => pointer(p.pointer),
            Self::AnyPointer => pointer(p.any_pointer),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
}
impl Comparison {
    fn holds(self, a: f64, b: f64) -> bool {
        match self {
            Self::Less => a < b,
            Self::LessOrEqual => a <= b,
            Self::Greater => a > b,
            Self::GreaterOrEqual => a >= b,
            Self::Equal => a == b,
        }
    }
    /// The comparison with its operands swapped (`v < f` is `f > v`).
    fn flipped(self) -> Self {
        match self {
            Self::Less => Self::Greater,
            Self::LessOrEqual => Self::GreaterOrEqual,
            Self::Greater => Self::Less,
            Self::GreaterOrEqual => Self::LessOrEqual,
            Self::Equal => Self::Equal,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Test {
    /// Boolean context (§2.4.2).
    Boolean,
    Keyword(&'static str),
    /// `feature <op> value`, every one of which must hold, in CSS pixels.
    Range(Vec<(Comparison, f64)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub name: FeatureName,
    pub test: Test,
}
impl Feature {
    fn matches(&self, media: &Media) -> bool {
        let size = match self.name {
            FeatureName::Width => Some(media.width),
            FeatureName::Height => Some(media.height),
            _ => None,
        };
        match (&self.test, size) {
            (Test::Boolean, Some(size)) => size != 0.0,
            (Test::Boolean, None) => !matches!(self.name.value(media), "none" | "no-preference"),
            (Test::Keyword(word), _) => self.name.value(media) == *word,
            (Test::Range(tests), Some(size)) => tests.iter().all(|(op, v)| op.holds(size, *v)),
            (Test::Range(_), None) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Feature(Feature),
    Not(Box<Condition>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}
impl Condition {
    fn matches(&self, media: &Media) -> bool {
        match self {
            Self::Feature(feature) => feature.matches(media),
            Self::Not(inner) => !inner.matches(media),
            Self::And(items) => items.iter().all(|item| item.matches(media)),
            Self::Or(items) => items.iter().any(|item| item.matches(media)),
        }
    }
    fn features(&self) -> usize {
        match self {
            Self::Feature(_) => 1,
            Self::Not(inner) => inner.features(),
            Self::And(items) | Self::Or(items) => items.iter().map(Self::features).sum(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaQuery {
    pub negated: bool,
    pub media_type: MediaType,
    pub condition: Option<Condition>,
}
impl MediaQuery {
    fn matches(&self, media: &Media) -> bool {
        let matched = self.media_type != MediaType::Print
            && self
                .condition
                .as_ref()
                .is_none_or(|condition| condition.matches(media));
        matched != self.negated
    }
}

/// A `<media-query-list>`: it matches when any query matches.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaQueryList(pub Vec<MediaQuery>);
impl MediaQueryList {
    #[must_use]
    pub fn matches(&self, media: &Media) -> bool {
        self.0.is_empty() || self.0.iter().any(|query| query.matches(media))
    }
    /// The number of feature tests, the unit of the parse-work charge.
    #[must_use]
    pub fn features(&self) -> usize {
        self.0
            .iter()
            .filter_map(|query| query.condition.as_ref())
            .map(Condition::features)
            .sum()
    }
}

/// Parenthesized nesting of a media condition (and of an `@supports`
/// condition): the parsers recurse once per level.
pub const MAX_CONDITION_DEPTH: usize = 32;

struct Parser<'a> {
    tokens: &'a [Spanned],
    at: usize,
    depth: usize,
}
type Parsed<T> = Result<T, usize>;

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|t| &t.token)
    }
    fn skip(&mut self) {
        while self.peek() == Some(&Token::Whitespace) {
            self.at += 1;
        }
    }
    fn fail<T>(&self) -> Parsed<T> {
        Err(self.at)
    }
    fn keyword(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(found)) if found.eq_ignore_ascii_case(word))
    }
    fn query(&mut self) -> Parsed<MediaQuery> {
        self.skip();
        if self.peek() == Some(&Token::OpenParen)
            || (self.keyword("not") && {
                let mut look = self.at + 1;
                while self.tokens.get(look).map(|t| &t.token) == Some(&Token::Whitespace) {
                    look += 1;
                }
                self.tokens.get(look).map(|t| &t.token) == Some(&Token::OpenParen)
            })
        {
            return Ok(MediaQuery {
                negated: false,
                media_type: MediaType::All,
                condition: Some(self.condition(true)?),
            });
        }
        let mut negated = false;
        if self.keyword("not") {
            negated = true;
            self.at += 1;
            self.skip();
        } else if self.keyword("only") {
            self.at += 1;
            self.skip();
        }
        let media_type = match self.peek() {
            Some(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
                "all" => MediaType::All,
                "screen" => MediaType::Screen,
                "print" => MediaType::Print,
                _ => return self.fail(),
            },
            _ => return self.fail(),
        };
        self.at += 1;
        self.skip();
        let condition = if self.keyword("and") {
            self.at += 1;
            self.skip();
            Some(self.condition(false)?)
        } else {
            None
        };
        Ok(MediaQuery {
            negated,
            media_type,
            condition,
        })
    }
    /// `<media-condition>` (or `<media-condition-without-or>`).
    fn condition(&mut self, or: bool) -> Parsed<Condition> {
        self.skip();
        if self.keyword("not") {
            self.at += 1;
            self.skip();
            return Ok(Condition::Not(Box::new(self.in_parens()?)));
        }
        let first = self.in_parens()?;
        let mut items = vec![first];
        let mut joiner: Option<bool> = None;
        loop {
            let save = self.at;
            self.skip();
            let and = self.keyword("and");
            let is_or = self.keyword("or");
            if !and && !is_or {
                self.at = save;
                break;
            }
            if is_or && !or || joiner.is_some_and(|j| j != and) {
                return self.fail();
            }
            joiner = Some(and);
            self.at += 1;
            self.skip();
            items.push(self.in_parens()?);
        }
        Ok(match joiner {
            None => items.remove(0),
            Some(true) => Condition::And(items),
            Some(false) => Condition::Or(items),
        })
    }
    /// `<media-in-parens>`: a nested condition or a feature.
    fn in_parens(&mut self) -> Parsed<Condition> {
        if self.peek() != Some(&Token::OpenParen) || self.depth >= MAX_CONDITION_DEPTH {
            return self.fail();
        }
        self.depth += 1;
        let result = self.in_parens_inner();
        self.depth -= 1;
        result
    }
    fn in_parens_inner(&mut self) -> Parsed<Condition> {
        self.at += 1;
        self.skip();
        let inner = if self.peek() == Some(&Token::OpenParen) || self.keyword("not") {
            self.condition(true)?
        } else {
            Condition::Feature(self.feature()?)
        };
        self.skip();
        if self.peek() != Some(&Token::CloseParen) {
            return self.fail();
        }
        self.at += 1;
        Ok(inner)
    }
    fn comparison(&mut self) -> Option<Comparison> {
        let op = match self.peek()? {
            Token::Delim('<') => Comparison::Less,
            Token::Delim('>') => Comparison::Greater,
            Token::Delim('=') => {
                self.at += 1;
                return Some(Comparison::Equal);
            }
            _ => return None,
        };
        self.at += 1;
        if self.peek() == Some(&Token::Delim('=')) {
            self.at += 1;
            return Some(match op {
                Comparison::Less => Comparison::LessOrEqual,
                _ => Comparison::GreaterOrEqual,
            });
        }
        Some(op)
    }
    /// A `<length>` value in CSS pixels.
    fn length(&mut self) -> Option<f64> {
        let value = match self.peek()? {
            Token::Number(number) if number.value == 0.0 => 0.0,
            Token::Dimension(number, unit) => {
                let scale = match math::length_unit(unit)? {
                    math::LengthUnit::Absolute(factor) => factor,
                    math::LengthUnit::Em | math::LengthUnit::Rem => REM_PIXELS,
                    _ => return None,
                };
                number.value * scale
            }
            _ => return None,
        };
        value.is_finite().then(|| self.at += 1).map(|()| value)
    }
    fn feature(&mut self) -> Parsed<Feature> {
        // `<mf-value> <op> <mf-name> [<op> <mf-value>]?`
        if let Some(value) = self.length() {
            self.skip();
            let first = self.comparison().ok_or(self.at)?;
            self.skip();
            let name = self.name()?;
            if !name.0.range() {
                return self.fail();
            }
            let mut tests = vec![(first.flipped(), value)];
            self.skip();
            if let Some(second) = self.comparison() {
                let increasing = matches!(first, Comparison::Less | Comparison::LessOrEqual)
                    && matches!(second, Comparison::Less | Comparison::LessOrEqual);
                let decreasing = matches!(first, Comparison::Greater | Comparison::GreaterOrEqual)
                    && matches!(second, Comparison::Greater | Comparison::GreaterOrEqual);
                if !increasing && !decreasing {
                    return self.fail();
                }
                self.skip();
                let value = self.length().ok_or(self.at)?;
                tests.push((second, value));
            }
            return Ok(Feature {
                name: name.0,
                test: Test::Range(tests),
            });
        }
        let start = self.at;
        let Some(Token::Ident(raw)) = self.peek() else {
            return self.fail();
        };
        let lower = raw.to_ascii_lowercase();
        let (prefix, base) = if let Some(base) = lower.strip_prefix("min-") {
            (Some(Comparison::GreaterOrEqual), base)
        } else if let Some(base) = lower.strip_prefix("max-") {
            (Some(Comparison::LessOrEqual), base)
        } else {
            (None, lower.as_str())
        };
        let (name, values) = FeatureName::named(base).ok_or(start)?;
        if prefix.is_some() && !name.range() {
            return Err(start);
        }
        self.at += 1;
        self.skip();
        match self.peek() {
            Some(Token::Colon) => {
                self.at += 1;
                self.skip();
                let test = if name.range() {
                    let value = self.length().ok_or(self.at)?;
                    Test::Range(vec![(prefix.unwrap_or(Comparison::Equal), value)])
                } else {
                    let Some(Token::Ident(word)) = self.peek() else {
                        return self.fail();
                    };
                    let found = values
                        .iter()
                        .find(|known| word.eq_ignore_ascii_case(known))
                        .ok_or(self.at)?;
                    self.at += 1;
                    Test::Keyword(found)
                };
                Ok(Feature { name, test })
            }
            Some(Token::CloseParen) if prefix.is_none() => Ok(Feature {
                name,
                test: Test::Boolean,
            }),
            _ if prefix.is_none() && name.range() => {
                let op = self.comparison().ok_or(self.at)?;
                self.skip();
                let value = self.length().ok_or(self.at)?;
                Ok(Feature {
                    name,
                    test: Test::Range(vec![(op, value)]),
                })
            }
            _ => self.fail(),
        }
    }
    fn name(&mut self) -> Parsed<(FeatureName, &'static [&'static str])> {
        let Some(Token::Ident(name)) = self.peek() else {
            return self.fail();
        };
        let found = FeatureName::named(name).ok_or(self.at)?;
        self.at += 1;
        Ok(found)
    }
}

/// Parse the prelude tokens of an `@media` rule. Returns the list and its
/// feature count, or the index of the offending token.
///
/// # Errors
/// The index of the first token outside the admitted grammar.
pub fn parse(tokens: &[Spanned]) -> Result<(MediaQueryList, usize), usize> {
    let mut queries = Vec::new();
    let mut parser = Parser {
        tokens,
        at: 0,
        depth: 0,
    };
    parser.skip();
    if parser.at >= tokens.len() {
        // An empty prelude is the empty list, which matches all (§2.1).
        return Ok((MediaQueryList(queries), 0));
    }
    loop {
        queries.push(parser.query()?);
        parser.skip();
        match parser.peek() {
            None => break,
            Some(Token::Comma) => parser.at += 1,
            Some(_) => return parser.fail(),
        }
    }
    let list = MediaQueryList(queries);
    let features = list.features();
    Ok((list, features))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::syntax::tokenize;

    fn media(width: f64, height: f64) -> Media {
        Media {
            width,
            height,
            dark: false,
            preferences: Preferences::default(),
        }
    }
    fn query(text: &str) -> MediaQueryList {
        parse(&tokenize(text))
            .unwrap_or_else(|at| panic!("{text}: {at}"))
            .0
    }

    #[test]
    fn range_syntax_and_prefixes_are_equivalent() {
        for text in [
            "(width >= 40rem)",
            "(min-width: 640px)",
            "(40rem <= width)",
            "screen and (min-width: 40rem)",
            "only screen and (width >= 640px)",
            "(640px <= width < 2000px)",
            "not (width < 640px)",
            "(width >= 640px) or (hover: none)",
        ] {
            let list = query(text);
            assert!(list.matches(&media(640.0, 400.0)), "{text}");
            assert!(!list.matches(&media(639.0, 400.0)), "{text}");
        }
        assert!(query("(width = 10px)").matches(&media(10.0, 1.0)));
        assert!(query("(orientation: portrait)").matches(&media(10.0, 10.0)));
        assert!(query("(orientation: landscape)").matches(&media(11.0, 10.0)));
        assert!(!query("print").matches(&media(10.0, 10.0)));
        assert!(query("not print").matches(&media(10.0, 10.0)));
        assert!(query("print, (hover)").matches(&media(10.0, 10.0)));
        assert!(query("").matches(&media(10.0, 10.0)));
    }

    #[test]
    fn preferences_are_recorded_inputs() {
        let mut env = media(100.0, 100.0);
        assert!(query("(hover: hover) and (pointer: fine)").matches(&env));
        assert!(query("(prefers-reduced-motion: no-preference)").matches(&env));
        assert!(!query("(prefers-reduced-motion)").matches(&env));
        assert!(!query("(forced-colors)").matches(&env));
        env.preferences.reduced_motion = true;
        env.preferences.hover = Hover::None;
        env.preferences.contrast = Contrast::More;
        assert!(query("(prefers-reduced-motion: reduce)").matches(&env));
        assert!(query("(hover: none)").matches(&env));
        assert!(query("(prefers-contrast)").matches(&env));
        env.dark = true;
        assert!(query("(prefers-color-scheme: dark)").matches(&env));
    }

    #[test]
    fn everything_else_is_a_violation() {
        for text in [
            "(orientation: sideways)",
            "(color)",
            "(min-hover: hover)",
            "(width >= 10vw)",
            "(width >= calc(1px))",
            "(10px < width > 20px)",
            "(width: 1px) and (height: 1px) or (hover)",
            "tv",
            "screen, ",
            "(hover: hover",
            "foo(bar)",
            &"(".repeat(64),
        ] {
            assert!(parse(&tokenize(text)).is_err(), "{text}");
        }
    }
}
