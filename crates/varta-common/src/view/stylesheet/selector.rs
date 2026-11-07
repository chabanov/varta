//! Selectors of the view subset (Selectors 4): complex selectors with the
//! descendant, child, next-sibling and subsequent-sibling combinators
//! (§16); type and universal selectors (§5); class selectors (§6.6); the
//! logical pseudo-classes `:is()`, `:where()` and `:not()` (§4); the
//! structural pseudo-classes (§14); the user-action and input pseudo-classes
//! the hosts record (`:hover`, `:active`, `:focus-visible`, `:disabled`,
//! `:enabled`, `:checked`); `:root`; and `:host`, which outside a shadow
//! tree's own sheets matches nothing (CSS Scoping 1 §3.2.1).
//!
//! Matching belongs to the style engine (`varta-style`), which walks the
//! host's tree; this module parses and computes specificity. Parsing is
//! bounded by [`MAX_SELECTOR_NESTING`] and [`MAX_COMPOUNDS`].
#[allow(unused_imports)]
use crate::view::prelude::*;

use super::super::{Tag, css};
use crate::view::syntax::{Spanned, Token};

/// The deepest nesting of `:is()`, `:where()` and `:not()` arguments.
pub const MAX_SELECTOR_NESTING: usize = 4;
/// At most this many compound selectors in one complex selector.
pub const MAX_COMPOUNDS: usize = 16;

/// A complex selector: compounds from left to right and the combinators
/// between them (`combinators.len() + 1 == compounds.len()`).
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub compounds: Vec<Compound>,
    pub combinators: Vec<Combinator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// ` ` (Selectors 4 §16.1).
    Descendant,
    /// `>` (§16.2).
    Child,
    /// `+` (§16.3).
    NextSibling,
    /// `~` (§16.4).
    SubsequentSibling,
}

/// A compound selector: an optional type selector, then classes and
/// pseudo-classes. `*` is no type selector.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compound {
    pub tag: Option<Tag>,
    pub classes: Vec<String>,
    pub pseudo: Vec<Pseudo>,
}

/// `An+B` (CSS Syntax 3 §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nth {
    pub a: i32,
    pub b: i32,
}
impl Nth {
    /// Whether the one-based `index` is `An+B` for some `n >= 0`.
    #[must_use]
    pub fn matches(self, index: i32) -> bool {
        let offset = i64::from(index) - i64::from(self.b);
        match self.a {
            0 => offset == 0,
            a => {
                let a = i64::from(a);
                offset % a == 0 && offset / a >= 0
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pseudo {
    Hover,
    Active,
    Focus,
    FocusWithin,
    FocusVisible,
    Disabled,
    Enabled,
    Checked,
    Root,
    Host,
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthChild(Nth),
    NthLastChild(Nth),
    NthOfType(Nth),
    NthLastOfType(Nth),
    /// `:is()`: the arguments that parsed (a forgiving list, §4.2).
    Is(Vec<Selector>),
    /// `:where()`: the same with zero specificity (§4.3).
    Where(Vec<Selector>),
    /// `:not()` (§4.4).
    Not(Vec<Selector>),
}

/// The pseudo-classes without arguments, by name.
const KEYWORD_PSEUDO: &[(&str, Pseudo)] = &[
    ("hover", Pseudo::Hover),
    ("active", Pseudo::Active),
    ("focus", Pseudo::Focus),
    ("focus-within", Pseudo::FocusWithin),
    ("focus-visible", Pseudo::FocusVisible),
    ("disabled", Pseudo::Disabled),
    ("enabled", Pseudo::Enabled),
    ("checked", Pseudo::Checked),
    ("root", Pseudo::Root),
    ("host", Pseudo::Host),
    ("first-child", Pseudo::FirstChild),
    ("last-child", Pseudo::LastChild),
    ("only-child", Pseudo::OnlyChild),
    ("first-of-type", Pseudo::FirstOfType),
    ("last-of-type", Pseudo::LastOfType),
    ("only-of-type", Pseudo::OnlyOfType),
];

/// The names of the pseudo-classes the subset admits, with their colon.
pub const PSEUDO_CLASS_NAMES: &[&str] = &[
    ":hover",
    ":active",
    ":focus",
    ":focus-within",
    ":focus-visible",
    ":disabled",
    ":enabled",
    ":checked",
    ":root",
    ":host",
    ":first-child",
    ":last-child",
    ":only-child",
    ":first-of-type",
    ":last-of-type",
    ":only-of-type",
    ":nth-child()",
    ":nth-last-child()",
    ":nth-of-type()",
    ":nth-last-of-type()",
    ":is()",
    ":where()",
    ":not()",
];

/// Specificity (Selectors 4 §17): `(ids, classes, types)`, compared in
/// that order. The subset has no id selectors.
pub type Specificity = (u32, u32, u32);

impl Selector {
    /// The rightmost (subject) compound.
    #[must_use]
    pub fn subject(&self) -> &Compound {
        // A parsed selector has at least one compound.
        &self.compounds[self.compounds.len() - 1]
    }
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        self.compounds
            .iter()
            .map(Compound::specificity)
            .fold((0, 0, 0), add)
    }
    /// The number of compounds, nested arguments included: the selector's
    /// share of the matching work at one element.
    #[must_use]
    pub fn cost(&self) -> usize {
        self.compounds.iter().map(Compound::cost).sum()
    }
    /// Whether matching looks beyond the element's own state: a combinator,
    /// a structural pseudo-class or a logical pseudo-class's arguments.
    #[must_use]
    pub fn searches(&self) -> bool {
        self.compounds.len() > 1
            || self.compounds.iter().any(|compound| {
                compound.pseudo.iter().any(|pseudo| {
                    pseudo.structural()
                        || matches!(pseudo, Pseudo::Is(_) | Pseudo::Where(_) | Pseudo::Not(_))
                })
            })
    }
    /// Whether every combinator is a descendant or child combinator (no
    /// sibling scan) and no argument has one: the selector reads only the
    /// element's ancestors.
    #[must_use]
    pub fn reads_siblings(&self) -> bool {
        self.combinators
            .iter()
            .any(|c| matches!(c, Combinator::NextSibling | Combinator::SubsequentSibling))
            || self.compounds.iter().any(|compound| {
                compound.pseudo.iter().any(|pseudo| match pseudo {
                    Pseudo::Is(list) | Pseudo::Where(list) | Pseudo::Not(list) => {
                        list.iter().any(Selector::reads_siblings)
                    }
                    pseudo => pseudo.structural(),
                })
            })
    }
}

fn add(a: Specificity, b: Specificity) -> Specificity {
    (
        a.0.saturating_add(b.0),
        a.1.saturating_add(b.1),
        a.2.saturating_add(b.2),
    )
}

impl Compound {
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        let own = (
            0,
            u32::try_from(self.classes.len()).unwrap_or(u32::MAX),
            u32::from(self.tag.is_some()),
        );
        self.pseudo.iter().map(Pseudo::specificity).fold(own, add)
    }
    fn cost(&self) -> usize {
        1 + self
            .pseudo
            .iter()
            .map(|pseudo| match pseudo {
                Pseudo::Is(list) | Pseudo::Where(list) | Pseudo::Not(list) => {
                    list.iter().map(Selector::cost).sum()
                }
                _ => 0,
            })
            .sum::<usize>()
    }
}

impl Pseudo {
    fn specificity(&self) -> Specificity {
        match self {
            // §17: `:where()` adds nothing; `:is()` and `:not()` add their
            // most specific argument.
            Self::Where(_) => (0, 0, 0),
            Self::Is(list) | Self::Not(list) => list
                .iter()
                .map(Selector::specificity)
                .max()
                .unwrap_or((0, 0, 0)),
            _ => (0, 1, 0),
        }
    }
    /// Whether it reads the element's siblings.
    fn structural(&self) -> bool {
        matches!(
            self,
            Self::FirstChild
                | Self::LastChild
                | Self::OnlyChild
                | Self::FirstOfType
                | Self::LastOfType
                | Self::OnlyOfType
                | Self::NthChild(_)
                | Self::NthLastChild(_)
                | Self::NthOfType(_)
                | Self::NthLastOfType(_)
        )
    }
}

/// A selector parse failure: the index of the offending token.
pub type Failure = usize;

/// Parse a selector list (`tokens`, without surrounding whitespace) whose
/// items must all be valid; `source` is the text the tokens' offsets refer
/// to. Returns the selectors, or the index of the token where parsing
/// failed.
///
/// # Errors
/// The index of the first offending token.
pub fn parse_list(tokens: &[Spanned], source: &str) -> Result<Vec<Selector>, Failure> {
    Parser {
        tokens,
        source,
        at: 0,
        limit: false,
    }
    .list(tokens.len(), 0, false)
}

struct Parser<'a> {
    tokens: &'a [Spanned],
    source: &'a str,
    at: usize,
    /// A nesting or compound limit was exceeded: not forgivable.
    limit: bool,
}

impl Parser<'_> {
    fn token(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|t| &t.token)
    }
    fn skip_whitespace(&mut self, end: usize) -> bool {
        let start = self.at;
        while self.at < end && self.token() == Some(&Token::Whitespace) {
            self.at += 1;
        }
        self.at > start
    }
    /// A comma-separated list up to `end`; `forgiving` drops invalid items
    /// (Selectors 4 §4.1) instead of failing.
    fn list(
        &mut self,
        end: usize,
        depth: usize,
        forgiving: bool,
    ) -> Result<Vec<Selector>, Failure> {
        let mut out = Vec::new();
        loop {
            let item_end = self.item_end(end);
            match self.complex(item_end, depth) {
                Ok(selector) => out.push(selector),
                Err(failure) if !forgiving || self.limit => return Err(failure),
                Err(_) => {}
            }
            self.at = item_end;
            if self.at >= end {
                return Ok(out);
            }
            // The comma.
            self.at += 1;
        }
    }
    /// The index of the next top-level comma before `end`, or `end`.
    fn item_end(&self, end: usize) -> usize {
        let mut depth = 0usize;
        for index in self.at..end {
            match &self.tokens[index].token {
                Token::Function(_) | Token::OpenParen | Token::OpenSquare => depth += 1,
                Token::CloseParen | Token::CloseSquare => depth = depth.saturating_sub(1),
                Token::Comma if depth == 0 => return index,
                _ => {}
            }
        }
        end
    }
    fn complex(&mut self, end: usize, depth: usize) -> Result<Selector, Failure> {
        self.skip_whitespace(end);
        let mut compounds = vec![self.compound(end, depth)?];
        let mut combinators = Vec::new();
        loop {
            let spaced = self.skip_whitespace(end);
            if self.at >= end {
                break;
            }
            let combinator = match self.token() {
                Some(Token::Delim('>')) => Combinator::Child,
                Some(Token::Delim('+')) => Combinator::NextSibling,
                Some(Token::Delim('~')) => Combinator::SubsequentSibling,
                _ if spaced => Combinator::Descendant,
                _ => return Err(self.at),
            };
            if combinator != Combinator::Descendant {
                self.at += 1;
                self.skip_whitespace(end);
            }
            if compounds.len() == MAX_COMPOUNDS {
                self.limit = true;
                return Err(self.at);
            }
            combinators.push(combinator);
            compounds.push(self.compound(end, depth)?);
        }
        Ok(Selector {
            compounds,
            combinators,
        })
    }
    fn compound(&mut self, end: usize, depth: usize) -> Result<Compound, Failure> {
        let mut compound = Compound::default();
        let start = self.at;
        match self.token() {
            Some(Token::Delim('*')) if self.at < end => self.at += 1,
            Some(Token::Ident(name)) if self.at < end => {
                compound.tag = Some(
                    Tag::ALL
                        .iter()
                        .copied()
                        .find(|tag| tag.name().eq_ignore_ascii_case(name))
                        .ok_or(self.at)?,
                );
                self.at += 1;
            }
            _ => {}
        }
        while self.at < end {
            match self.token() {
                Some(Token::Delim('.')) => match self.tokens.get(self.at + 1).map(|t| &t.token) {
                    Some(Token::Ident(class)) if self.at + 1 < end && css::class_name(class) => {
                        compound.classes.push(class.clone());
                        self.at += 2;
                    }
                    _ => return Err(self.at + 1),
                },
                Some(Token::Colon) => {
                    let pseudo = self.pseudo(end, depth)?;
                    compound.pseudo.push(pseudo);
                }
                _ => break,
            }
        }
        if self.at == start {
            return Err(self.at);
        }
        Ok(compound)
    }
    fn pseudo(&mut self, end: usize, depth: usize) -> Result<Pseudo, Failure> {
        let colon = self.at;
        self.at += 1;
        match self.tokens.get(self.at).map(|t| &t.token) {
            Some(Token::Ident(name)) if self.at < end => {
                let found = KEYWORD_PSEUDO
                    .iter()
                    .find(|(known, _)| known.eq_ignore_ascii_case(name))
                    .map(|(_, pseudo)| pseudo.clone())
                    .ok_or(colon)?;
                self.at += 1;
                Ok(found)
            }
            Some(Token::Function(name)) if self.at < end => {
                let name = name.to_ascii_lowercase();
                let open = self.at;
                let close = self.close(open, end).ok_or(open)?;
                self.at = open + 1;
                let pseudo = match name.as_str() {
                    "is" | "where" | "not" => {
                        if depth + 1 > MAX_SELECTOR_NESTING {
                            self.limit = true;
                            return Err(open);
                        }
                        let forgiving = name != "not";
                        let inner_end = trim_end(self.tokens, self.at, close);
                        self.skip_whitespace(inner_end);
                        let list = self.list(inner_end, depth + 1, forgiving)?;
                        match name.as_str() {
                            "is" => Pseudo::Is(list),
                            "where" => Pseudo::Where(list),
                            _ => Pseudo::Not(list),
                        }
                    }
                    "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type" => {
                        let text = self.text(self.at, close);
                        let nth = nth(&text).ok_or(open)?;
                        match name.as_str() {
                            "nth-child" => Pseudo::NthChild(nth),
                            "nth-last-child" => Pseudo::NthLastChild(nth),
                            "nth-of-type" => Pseudo::NthOfType(nth),
                            _ => Pseudo::NthLastOfType(nth),
                        }
                    }
                    _ => return Err(colon),
                };
                self.at = close + 1;
                Ok(pseudo)
            }
            _ => Err(colon),
        }
    }
    /// The index of the `)` closing the function at `open`.
    fn close(&self, open: usize, end: usize) -> Option<usize> {
        let mut depth = 0usize;
        for index in open..end {
            match &self.tokens[index].token {
                Token::Function(_) | Token::OpenParen => depth += 1,
                Token::CloseParen => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => {}
            }
        }
        None
    }
    /// The source text of tokens `start..end`.
    fn text(&self, start: usize, end: usize) -> String {
        match (
            self.tokens.get(start),
            end.checked_sub(1).and_then(|last| self.tokens.get(last)),
        ) {
            (Some(first), Some(last)) if start < end => {
                self.source[first.start..last.end].to_owned()
            }
            _ => String::new(),
        }
    }
}

/// `end` moved back over trailing whitespace tokens.
fn trim_end(tokens: &[Spanned], start: usize, mut end: usize) -> usize {
    while end > start && tokens[end - 1].token == Token::Whitespace {
        end -= 1;
    }
    end
}

/// `An+B` (CSS Syntax 3 §6.2) from its source text: `odd`, `even`, an
/// integer, or `An+B` with whitespace allowed around the sign of `B`.
fn nth(text: &str) -> Option<Nth> {
    let text: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "odd" => return Some(Nth { a: 2, b: 1 }),
        "even" => return Some(Nth { a: 2, b: 0 }),
        _ => {}
    }
    let integer = |text: &str| -> Option<i32> {
        let digits = text.strip_prefix('+').unwrap_or(text);
        if digits.is_empty()
            || !digits
                .trim_start_matches('-')
                .chars()
                .all(|c| c.is_ascii_digit())
        {
            return None;
        }
        digits.parse().ok()
    };
    let Some(position) = lower.find('n') else {
        return Some(Nth {
            a: 0,
            b: integer(&lower)?,
        });
    };
    let (a, b) = (&lower[..position], &lower[position + 1..]);
    let a = match a {
        "" | "+" => 1,
        "-" => -1,
        a => integer(a)?,
    };
    let b = match b {
        "" => 0,
        b if b.starts_with('+') || b.starts_with('-') => integer(b)?,
        _ => return None,
    };
    Some(Nth { a, b })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::syntax;

    fn parse(text: &str) -> Result<Vec<Selector>, Failure> {
        let tokens = syntax::tokenize(text);
        parse_list(&tokens, text)
    }

    #[test]
    fn complex_selectors_parse_with_their_specificity() {
        let list = parse(":root, :host").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].subject().pseudo, vec![Pseudo::Host]);
        let space = parse(":where(.space-x-2 > :not(:last-child))").unwrap();
        assert_eq!(space[0].specificity(), (0, 0, 0));
        let group = parse(":is(:where(.group):hover .group-hover\\:x)").unwrap();
        assert_eq!(group[0].specificity(), (0, 2, 0));
        let Pseudo::Is(inner) = &group[0].subject().pseudo[0] else {
            panic!("{group:?}");
        };
        assert_eq!(inner[0].combinators, vec![Combinator::Descendant]);
        let sibling = parse("div.a+ .b ~ button:first-child").unwrap();
        assert_eq!(
            sibling[0].combinators,
            vec![Combinator::NextSibling, Combinator::SubsequentSibling]
        );
        assert_eq!(sibling[0].specificity(), (0, 3, 2));
        assert_eq!(
            parse(".x:nth-child(2n + 1)").unwrap()[0].subject().pseudo,
            vec![Pseudo::NthChild(Nth { a: 2, b: 1 })]
        );
        // Forgiving `:is()` drops what it cannot parse; `:not()` does not.
        assert_eq!(
            parse(":is(.a, ::after)").unwrap()[0].subject().pseudo,
            vec![Pseudo::Is(parse(".a").unwrap())]
        );
        assert!(parse(":not(.a, ::after)").is_err());
        for invalid in [
            ".a >",
            "> .a",
            ".a::after",
            "frob",
            ".a:hovered",
            ".a .",
            ":nth-child(2n+)",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
        let deep =
            ":is(".repeat(MAX_SELECTOR_NESTING + 1) + ".a" + &")".repeat(MAX_SELECTOR_NESTING + 1);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn nth_follows_the_an_plus_b_microsyntax() {
        let parsed = |text| nth(text).unwrap();
        assert_eq!(parsed("odd"), Nth { a: 2, b: 1 });
        assert_eq!(parsed("EVEN"), Nth { a: 2, b: 0 });
        assert_eq!(parsed("-n+3"), Nth { a: -1, b: 3 });
        assert_eq!(parsed("n"), Nth { a: 1, b: 0 });
        assert_eq!(parsed("+5"), Nth { a: 0, b: 5 });
        assert_eq!(parsed("3n - 2"), Nth { a: 3, b: -2 });
        assert!(nth("n3").is_none() && nth("2x").is_none() && nth("").is_none());
        let first_three: Vec<i32> = (1..=6).filter(|i| parsed("-n+3").matches(*i)).collect();
        assert_eq!(first_three, vec![1, 2, 3]);
        assert!(parsed("odd").matches(5) && !parsed("odd").matches(4));
        assert!(parsed("3n-2").matches(1) && parsed("3n-2").matches(4));
    }
}
