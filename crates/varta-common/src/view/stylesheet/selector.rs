//! Selectors of the view subset (Selectors 4): complex selectors with the
//! descendant, child, next-sibling and subsequent-sibling combinators
//! (§16); type and universal selectors (§5); id, class and attribute
//! selectors with every matcher and the `i`/`s` flags (§6); the logical
//! pseudo-classes `:is()`, `:where()`, `:not()` and `:has()` (§4); the
//! location, user-action, input, structural and tree pseudo-classes the
//! view's elements can be in (§7–§14); `:root`, `:scope` and `:host`, which
//! outside a shadow tree's own sheets matches nothing (CSS Scoping 1
//! §3.2.1); and the pseudo-elements of CSS Pseudo-Elements 4 the view's
//! elements have (`::before`, `::after`, `::placeholder`, `::marker`,
//! `::selection`, `::backdrop`, `::file-selector-button`, `::first-line`,
//! `::first-letter`, and the CSS 2 single-colon forms of the last four of
//! them that CSS 2 defined).
//!
//! Matching belongs to the style engine (`varta-style`), which walks the
//! host's tree; this module parses and computes specificity. Parsing is
//! bounded by [`MAX_SELECTOR_NESTING`] and [`MAX_COMPOUNDS`].
#[allow(unused_imports)]
use crate::view::prelude::*;

use super::super::{Tag, css};
use crate::view::syntax::{Spanned, Token};

/// The deepest nesting of `:is()`, `:where()`, `:not()` and `:has()`
/// arguments.
pub const MAX_SELECTOR_NESTING: usize = 4;
/// At most this many compound selectors in one complex selector.
pub const MAX_COMPOUNDS: usize = 16;
/// At most this many bytes in an attribute selector's name or value, or an
/// id selector's name.
pub const MAX_SELECTOR_NAME_BYTES: usize = 256;

/// A complex selector: compounds from left to right and the combinators
/// between them (`combinators.len() + 1 == compounds.len()`), and the
/// pseudo-element it selects of its subject, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub compounds: Vec<Compound>,
    pub combinators: Vec<Combinator>,
    pub pseudo_element: Option<PseudoElement>,
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

/// A compound selector: an optional type selector, then ids, classes,
/// attribute selectors and pseudo-classes. `*` is no type selector.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compound {
    pub tag: Option<Tag>,
    pub ids: Vec<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
    pub pseudo: Vec<Pseudo>,
}

/// An attribute selector (Selectors 4 §6.1–§6.3): the attribute name in
/// ASCII lowercase (HTML attribute names match ASCII case-insensitively),
/// and the matcher with its value and case mode, `None` for `[att]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeSelector {
    pub name: String,
    pub test: Option<(AttributeMatcher, String, AttributeCase)>,
}

/// The value matchers of Selectors 4 §6.1 and §6.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeMatcher {
    /// `=`: exactly the value.
    Equals,
    /// `~=`: one of its whitespace-separated words is the value.
    Includes,
    /// `|=`: the value, or the value followed by `-`.
    DashMatch,
    /// `^=`: begins with the value.
    Prefix,
    /// `$=`: ends with the value.
    Suffix,
    /// `*=`: contains the value.
    Substring,
}

/// How an attribute value compares (Selectors 4 §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeCase {
    /// The document language's rule: HTML compares the values of a fixed
    /// list of attributes ASCII case-insensitively, others exactly.
    Default,
    /// `i`: ASCII case-insensitively.
    Insensitive,
    /// `s`: exactly.
    Sensitive,
}

/// HTML's attributes whose values selectors match ASCII case-insensitively
/// on HTML elements (HTML "Case-sensitivity of selectors").
pub const CASE_INSENSITIVE_ATTRIBUTES: &[&str] = &[
    "accept",
    "accept-charset",
    "align",
    "alink",
    "axis",
    "bgcolor",
    "charset",
    "checked",
    "clear",
    "codetype",
    "color",
    "compact",
    "declare",
    "defer",
    "dir",
    "direction",
    "disabled",
    "enctype",
    "face",
    "frame",
    "hreflang",
    "http-equiv",
    "lang",
    "language",
    "link",
    "media",
    "method",
    "multiple",
    "nohref",
    "noresize",
    "noshade",
    "nowrap",
    "readonly",
    "rel",
    "rev",
    "rules",
    "scope",
    "scrolling",
    "selected",
    "shape",
    "target",
    "text",
    "type",
    "valign",
    "valuetype",
    "vlink",
];

impl AttributeSelector {
    /// Whether an element whose attribute has `value` (`None`: absent)
    /// matches.
    #[must_use]
    pub fn matches(&self, value: Option<&str>) -> bool {
        let Some(value) = value else {
            return false;
        };
        let Some((matcher, expected, case)) = &self.test else {
            return true;
        };
        let insensitive = match case {
            AttributeCase::Insensitive => true,
            AttributeCase::Sensitive => false,
            AttributeCase::Default => CASE_INSENSITIVE_ATTRIBUTES.contains(&self.name.as_str()),
        };
        let fold = |text: &str| -> String {
            if insensitive {
                text.to_ascii_lowercase()
            } else {
                text.to_owned()
            }
        };
        let (value, expected) = (fold(value), fold(expected));
        match matcher {
            AttributeMatcher::Equals => value == expected,
            AttributeMatcher::Includes => {
                !expected.is_empty()
                    && !expected.contains(is_css_whitespace)
                    && value.split(is_css_whitespace).any(|word| word == expected)
            }
            AttributeMatcher::DashMatch => {
                value == expected
                    || value
                        .strip_prefix(expected.as_str())
                        .is_some_and(|rest| rest.starts_with('-'))
            }
            AttributeMatcher::Prefix => !expected.is_empty() && value.starts_with(&expected),
            AttributeMatcher::Suffix => !expected.is_empty() && value.ends_with(&expected),
            AttributeMatcher::Substring => !expected.is_empty() && value.contains(&expected),
        }
    }
}

/// CSS whitespace (CSS Syntax 3 §4.2): space, tab and newlines.
fn is_css_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// The pseudo-elements of the view's elements (CSS Pseudo-Elements 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PseudoElement {
    /// `::before` (§4.1).
    Before,
    /// `::after` (§4.1).
    After,
    /// `::placeholder` (§3.3).
    Placeholder,
    /// `::marker` (§4.2).
    Marker,
    /// `::selection` (§3.1).
    Selection,
    /// `::backdrop` (CSS Position 4 §3.2).
    Backdrop,
    /// `::file-selector-button` (§3.5).
    FileSelectorButton,
    /// `::first-line` (§2.1).
    FirstLine,
    /// `::first-letter` (§2.2).
    FirstLetter,
}
impl PseudoElement {
    pub const ALL: [Self; 9] = [
        Self::Before,
        Self::After,
        Self::Placeholder,
        Self::Marker,
        Self::Selection,
        Self::Backdrop,
        Self::FileSelectorButton,
        Self::FirstLine,
        Self::FirstLetter,
    ];
    /// The name without its colons.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
            Self::Placeholder => "placeholder",
            Self::Marker => "marker",
            Self::Selection => "selection",
            Self::Backdrop => "backdrop",
            Self::FileSelectorButton => "file-selector-button",
            Self::FirstLine => "first-line",
            Self::FirstLetter => "first-letter",
        }
    }
    /// Whether the renderers render it, so that its rules may be admitted:
    /// the subset fails closed instead of ignoring a pseudo-element's
    /// declarations. `::file-selector-button` has no originating element (the
    /// view has no file inputs), so its rules apply to nothing.
    #[must_use]
    pub fn rendered(self) -> bool {
        matches!(self, Self::FileSelectorButton)
    }
    /// Whether CSS 2 defined it, so it also takes one colon (Selectors 4
    /// §3.7).
    fn legacy(self) -> bool {
        matches!(
            self,
            Self::Before | Self::After | Self::FirstLine | Self::FirstLetter
        )
    }
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

/// A relative selector of `:has()` (Selectors 4 §3.5): a complex selector
/// anchored at the `:has()` element by its leading combinator.
#[derive(Debug, Clone, PartialEq)]
pub struct Relative {
    pub leading: Combinator,
    pub selector: Selector,
}

/// The writing direction of `:dir()` (Selectors 4 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
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
    /// `:scope` (§10.2): in a stylesheet, the root.
    Scope,
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
    /// `:empty` (§14.2).
    Empty,
    /// `:link`, `:any-link` (§8.1, §8.2): a hyperlink; `:visited` never
    /// matches (no history is exposed).
    Link,
    AnyLink,
    Visited,
    /// `:target`, `:target-within` (§8.6): no fragment navigation.
    Target,
    TargetWithin,
    /// The input pseudo-classes (§12).
    ReadOnly,
    ReadWrite,
    PlaceholderShown,
    Default,
    Indeterminate,
    Valid,
    Invalid,
    UserValid,
    UserInvalid,
    InRange,
    OutOfRange,
    Required,
    Optional,
    Autofill,
    /// `:open` (§13.2), `:modal` (§13.3): open and modal dialogs.
    Open,
    Modal,
    /// `:defined` (§13.1): every element of the view is defined.
    Defined,
    /// `:dir()` (§7.1): the view is left-to-right.
    Dir(Direction),
    /// `:lang()` (§7.2): the view has no content language, so none match.
    Lang,
    /// `:is()`: the arguments that parsed (a forgiving list, §4.2).
    Is(Vec<Selector>),
    /// `:where()`: the same with zero specificity (§4.3).
    Where(Vec<Selector>),
    /// `:not()` (§4.4).
    Not(Vec<Selector>),
    /// `:has()` (§4.5).
    Has(Vec<Relative>),
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
    ("scope", Pseudo::Scope),
    ("first-child", Pseudo::FirstChild),
    ("last-child", Pseudo::LastChild),
    ("only-child", Pseudo::OnlyChild),
    ("first-of-type", Pseudo::FirstOfType),
    ("last-of-type", Pseudo::LastOfType),
    ("only-of-type", Pseudo::OnlyOfType),
    ("empty", Pseudo::Empty),
    ("link", Pseudo::Link),
    ("any-link", Pseudo::AnyLink),
    ("visited", Pseudo::Visited),
    ("target", Pseudo::Target),
    ("target-within", Pseudo::TargetWithin),
    ("read-only", Pseudo::ReadOnly),
    ("read-write", Pseudo::ReadWrite),
    ("placeholder-shown", Pseudo::PlaceholderShown),
    ("default", Pseudo::Default),
    ("indeterminate", Pseudo::Indeterminate),
    ("valid", Pseudo::Valid),
    ("invalid", Pseudo::Invalid),
    ("user-valid", Pseudo::UserValid),
    ("user-invalid", Pseudo::UserInvalid),
    ("in-range", Pseudo::InRange),
    ("out-of-range", Pseudo::OutOfRange),
    ("required", Pseudo::Required),
    ("optional", Pseudo::Optional),
    ("autofill", Pseudo::Autofill),
    ("open", Pseudo::Open),
    ("modal", Pseudo::Modal),
    ("defined", Pseudo::Defined),
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
    ":scope",
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
    ":empty",
    ":link",
    ":any-link",
    ":visited",
    ":target",
    ":target-within",
    ":read-only",
    ":read-write",
    ":placeholder-shown",
    ":default",
    ":indeterminate",
    ":valid",
    ":invalid",
    ":user-valid",
    ":user-invalid",
    ":in-range",
    ":out-of-range",
    ":required",
    ":optional",
    ":autofill",
    ":open",
    ":modal",
    ":defined",
    ":dir()",
    ":lang()",
    ":is()",
    ":where()",
    ":not()",
    ":has()",
];

/// Specificity (Selectors 4 §17): `(ids, classes, types)`, compared in
/// that order.
pub type Specificity = (u32, u32, u32);

fn count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

impl Selector {
    /// The rightmost (subject) compound.
    #[must_use]
    pub fn subject(&self) -> &Compound {
        // A parsed selector has at least one compound.
        &self.compounds[self.compounds.len() - 1]
    }
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        let own = (0, 0, u32::from(self.pseudo_element.is_some()));
        self.compounds
            .iter()
            .map(Compound::specificity)
            .fold(own, add)
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
                        || matches!(
                            pseudo,
                            Pseudo::Is(_) | Pseudo::Where(_) | Pseudo::Not(_) | Pseudo::Has(_)
                        )
                })
            })
    }
    /// The relative selectors of every `:has()` in this selector (nested
    /// arguments included), whose matching searches a subtree.
    #[must_use]
    pub fn has_arguments(&self) -> Vec<&Relative> {
        let mut out = Vec::new();
        for compound in &self.compounds {
            for pseudo in &compound.pseudo {
                match pseudo {
                    Pseudo::Has(list) => out.extend(list.iter()),
                    Pseudo::Is(list) | Pseudo::Where(list) | Pseudo::Not(list) => {
                        out.extend(list.iter().flat_map(Selector::has_arguments));
                    }
                    _ => {}
                }
            }
        }
        out
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
            count(self.ids.len()),
            count(self.classes.len()).saturating_add(count(self.attributes.len())),
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
                Pseudo::Has(list) => list.iter().map(|relative| relative.selector.cost()).sum(),
                _ => 0,
            })
            .sum::<usize>()
    }
}

impl Pseudo {
    fn specificity(&self) -> Specificity {
        match self {
            // §17: `:where()` adds nothing; `:is()`, `:not()` and `:has()`
            // add their most specific argument.
            Self::Where(_) => (0, 0, 0),
            Self::Is(list) | Self::Not(list) => list
                .iter()
                .map(Selector::specificity)
                .max()
                .unwrap_or((0, 0, 0)),
            Self::Has(list) => list
                .iter()
                .map(|relative| relative.selector.specificity())
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
                | Self::Empty
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
    .list(tokens.len(), Context::TOP)
}

/// Where a selector list is parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Context {
    depth: usize,
    /// Drop invalid items (Selectors 4 §4.1) instead of failing.
    forgiving: bool,
    /// Whether a pseudo-element may end a selector (only at the top level,
    /// §3.6.1: none in logical pseudo-classes).
    pseudo_elements: bool,
    /// Whether `:has()` is admitted (none inside `:has()`, §4.5).
    has: bool,
    /// A list of relative selectors (`:has()`'s argument).
    relative: bool,
}
impl Context {
    const TOP: Self = Self {
        depth: 0,
        forgiving: false,
        pseudo_elements: true,
        has: true,
        relative: false,
    };
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
    fn token_at(&self, at: usize, end: usize) -> Option<&Token> {
        (at < end).then(|| self.tokens.get(at).map(|t| &t.token))?
    }
    fn skip_whitespace(&mut self, end: usize) -> bool {
        let start = self.at;
        while self.at < end && self.token() == Some(&Token::Whitespace) {
            self.at += 1;
        }
        self.at > start
    }
    /// A comma-separated list up to `end`.
    fn list(&mut self, end: usize, context: Context) -> Result<Vec<Selector>, Failure> {
        Ok(self
            .relative_list(end, context)?
            .into_iter()
            .map(|relative| relative.selector)
            .collect())
    }
    /// A comma-separated list up to `end`, each item with its leading
    /// combinator (a descendant one unless the context is relative).
    fn relative_list(&mut self, end: usize, context: Context) -> Result<Vec<Relative>, Failure> {
        let mut out = Vec::new();
        loop {
            let item_end = self.item_end(end);
            match self.relative(item_end, context) {
                Ok(selector) => out.push(selector),
                Err(failure) if !context.forgiving || self.limit => return Err(failure),
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
    fn combinator(&self) -> Option<Combinator> {
        match self.token() {
            Some(Token::Delim('>')) => Some(Combinator::Child),
            Some(Token::Delim('+')) => Some(Combinator::NextSibling),
            Some(Token::Delim('~')) => Some(Combinator::SubsequentSibling),
            _ => None,
        }
    }
    fn relative(&mut self, end: usize, context: Context) -> Result<Relative, Failure> {
        self.skip_whitespace(end);
        let mut leading = Combinator::Descendant;
        if context.relative
            && let Some(combinator) = self.combinator()
        {
            leading = combinator;
            self.at += 1;
            self.skip_whitespace(end);
        }
        let selector = self.complex(end, context)?;
        Ok(Relative { leading, selector })
    }
    fn complex(&mut self, end: usize, context: Context) -> Result<Selector, Failure> {
        let mut selector = Selector {
            compounds: Vec::new(),
            combinators: Vec::new(),
            pseudo_element: None,
        };
        selector
            .compounds
            .push(self.compound(end, context, &mut selector.pseudo_element)?);
        loop {
            let spaced = self.skip_whitespace(end);
            if self.at >= end {
                break;
            }
            // Nothing follows a pseudo-element (§3.6.3).
            if selector.pseudo_element.is_some() {
                return Err(self.at);
            }
            let combinator = match self.combinator() {
                Some(combinator) => combinator,
                None if spaced => Combinator::Descendant,
                None => return Err(self.at),
            };
            if combinator != Combinator::Descendant {
                self.at += 1;
                self.skip_whitespace(end);
            }
            if selector.compounds.len() == MAX_COMPOUNDS {
                self.limit = true;
                return Err(self.at);
            }
            selector.combinators.push(combinator);
            selector
                .compounds
                .push(self.compound(end, context, &mut selector.pseudo_element)?);
        }
        Ok(selector)
    }
    fn compound(
        &mut self,
        end: usize,
        context: Context,
        pseudo_element: &mut Option<PseudoElement>,
    ) -> Result<Compound, Failure> {
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
                Some(Token::Delim('.')) => match self.token_at(self.at + 1, end) {
                    Some(Token::Ident(class)) if css::class_name(class) => {
                        compound.classes.push(class.clone());
                        self.at += 2;
                    }
                    _ => return Err(self.at + 1),
                },
                // §6.7: an id selector is a hash token with the id type.
                Some(Token::Hash { value, id: true }) => {
                    if value.len() > MAX_SELECTOR_NAME_BYTES {
                        return Err(self.at);
                    }
                    compound.ids.push(value.clone());
                    self.at += 1;
                }
                Some(Token::OpenSquare) => {
                    let attribute = self.attribute(end)?;
                    compound.attributes.push(attribute);
                }
                Some(Token::Colon) => match self.token_at(self.at + 1, end) {
                    Some(Token::Colon) => {
                        let element = self.pseudo_element(end, context, 2)?;
                        *pseudo_element = Some(element);
                    }
                    _ => {
                        if let Some(element) = self.legacy_pseudo_element(end, context) {
                            *pseudo_element = Some(element);
                        } else {
                            let pseudo = self.pseudo(end, context)?;
                            compound.pseudo.push(pseudo);
                        }
                    }
                },
                _ => break,
            }
            if pseudo_element.is_some() {
                // A pseudo-element ends the compound and the selector.
                break;
            }
        }
        if self.at == start {
            return Err(self.at);
        }
        Ok(compound)
    }
    /// `::name` (`colons` 2) at the token after the colons.
    fn pseudo_element(
        &mut self,
        end: usize,
        context: Context,
        colons: usize,
    ) -> Result<PseudoElement, Failure> {
        let at = self.at;
        let element = match self.token_at(at + colons, end) {
            Some(Token::Ident(name)) => PseudoElement::ALL
                .into_iter()
                .find(|element| element.name().eq_ignore_ascii_case(name))
                .ok_or(at)?,
            _ => return Err(at),
        };
        if !context.pseudo_elements {
            return Err(at);
        }
        self.at = at + colons + 1;
        Ok(element)
    }
    /// The single-colon form of a CSS 2 pseudo-element, if the tokens are
    /// one.
    fn legacy_pseudo_element(&mut self, end: usize, context: Context) -> Option<PseudoElement> {
        let Some(Token::Ident(name)) = self.token_at(self.at + 1, end) else {
            return None;
        };
        let element = PseudoElement::ALL
            .into_iter()
            .find(|element| element.legacy() && element.name().eq_ignore_ascii_case(name))?;
        if !context.pseudo_elements {
            return None;
        }
        self.at += 2;
        Some(element)
    }
    /// `[name]`, `[name matcher value flag?]` (Selectors 4 §6.1–§6.3); no
    /// namespace prefixes (the view has one namespace).
    fn attribute(&mut self, end: usize) -> Result<AttributeSelector, Failure> {
        let open = self.at;
        let close = (open + 1..end)
            .find(|at| self.tokens[*at].token == Token::CloseSquare)
            .ok_or(open)?;
        self.at = open + 1;
        self.skip_whitespace(close);
        let name = match self.token_at(self.at, close) {
            Some(Token::Ident(name)) if name.len() <= MAX_SELECTOR_NAME_BYTES => {
                name.to_ascii_lowercase()
            }
            _ => return Err(self.at),
        };
        self.at += 1;
        self.skip_whitespace(close);
        if self.at == close {
            self.at = close + 1;
            return Ok(AttributeSelector { name, test: None });
        }
        let matcher = match (
            self.token_at(self.at, close),
            self.token_at(self.at + 1, close),
        ) {
            (Some(Token::Delim('=')), _) => {
                self.at += 1;
                AttributeMatcher::Equals
            }
            (Some(Token::Delim(prefix)), Some(Token::Delim('='))) => {
                let matcher = match prefix {
                    '~' => AttributeMatcher::Includes,
                    '|' => AttributeMatcher::DashMatch,
                    '^' => AttributeMatcher::Prefix,
                    '$' => AttributeMatcher::Suffix,
                    '*' => AttributeMatcher::Substring,
                    _ => return Err(self.at),
                };
                self.at += 2;
                matcher
            }
            _ => return Err(self.at),
        };
        self.skip_whitespace(close);
        let value = match self.token_at(self.at, close) {
            Some(Token::Ident(value) | Token::String(value))
                if value.len() <= MAX_SELECTOR_NAME_BYTES =>
            {
                value.clone()
            }
            _ => return Err(self.at),
        };
        self.at += 1;
        self.skip_whitespace(close);
        let case = match self.token_at(self.at, close) {
            None => AttributeCase::Default,
            Some(Token::Ident(flag)) if flag.eq_ignore_ascii_case("i") => {
                self.at += 1;
                AttributeCase::Insensitive
            }
            Some(Token::Ident(flag)) if flag.eq_ignore_ascii_case("s") => {
                self.at += 1;
                AttributeCase::Sensitive
            }
            Some(_) => return Err(self.at),
        };
        self.skip_whitespace(close);
        if self.at != close {
            return Err(self.at);
        }
        self.at = close + 1;
        Ok(AttributeSelector {
            name,
            test: Some((matcher, value, case)),
        })
    }
    fn pseudo(&mut self, end: usize, context: Context) -> Result<Pseudo, Failure> {
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
                let inner_end = trim_end(self.tokens, self.at, close);
                let pseudo = match name.as_str() {
                    "is" | "where" | "not" | "has" => {
                        if context.depth + 1 > MAX_SELECTOR_NESTING {
                            self.limit = true;
                            return Err(open);
                        }
                        if name == "has" && !context.has {
                            return Err(open);
                        }
                        let inner = Context {
                            depth: context.depth + 1,
                            // `:is()` and `:where()` take forgiving lists;
                            // `:not()` and `:has()` do not (§4.4, §4.5).
                            forgiving: matches!(name.as_str(), "is" | "where"),
                            pseudo_elements: false,
                            has: context.has && name != "has",
                            relative: name == "has",
                        };
                        self.skip_whitespace(inner_end);
                        match name.as_str() {
                            "is" => Pseudo::Is(self.list(inner_end, inner)?),
                            "where" => Pseudo::Where(self.list(inner_end, inner)?),
                            "not" => Pseudo::Not(self.list(inner_end, inner)?),
                            _ => Pseudo::Has(self.relative_list(inner_end, inner)?),
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
                    "dir" => {
                        self.skip_whitespace(inner_end);
                        let direction = match self.token_at(self.at, inner_end) {
                            Some(Token::Ident(word)) if word.eq_ignore_ascii_case("ltr") => {
                                Direction::Ltr
                            }
                            Some(Token::Ident(word)) if word.eq_ignore_ascii_case("rtl") => {
                                Direction::Rtl
                            }
                            _ => return Err(open),
                        };
                        self.at += 1;
                        if self.at != inner_end {
                            return Err(open);
                        }
                        Pseudo::Dir(direction)
                    }
                    // §7.2: a list of language ranges (idents or strings).
                    "lang" => {
                        self.lang_ranges(inner_end).ok_or(open)?;
                        Pseudo::Lang
                    }
                    _ => return Err(colon),
                };
                self.at = close + 1;
                Ok(pseudo)
            }
            _ => Err(colon),
        }
    }
    /// The comma-separated language ranges of `:lang()` up to `end`.
    fn lang_ranges(&mut self, end: usize) -> Option<()> {
        loop {
            self.skip_whitespace(end);
            match self.token_at(self.at, end) {
                Some(Token::Ident(range) | Token::String(range))
                    if !range.is_empty() && range.len() <= MAX_SELECTOR_NAME_BYTES =>
                {
                    self.at += 1;
                }
                _ => return None,
            }
            self.skip_whitespace(end);
            match self.token_at(self.at, end) {
                None => return Some(()),
                Some(Token::Comma) => self.at += 1,
                Some(_) => return None,
            }
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
            "frob",
            ".a:hovered",
            ".a .",
            ":nth-child(2n+)",
            ".a::after .b",
            ".a::after:hover",
            "::frob",
            "[a=]",
            "[a b]",
            "[a=b c]",
            "[a=1]",
            "[ns|a]",
            "[a=b x]",
            ":has(:has(.a))",
            ":has(::before)",
            ":has()",
            ":dir(up)",
            ":lang()",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
        let deep =
            ":is(".repeat(MAX_SELECTOR_NESTING + 1) + ".a" + &")".repeat(MAX_SELECTOR_NESTING + 1);
        assert!(parse(&deep).is_err());
    }

    /// Selectors 4 §6 and §17.
    #[test]
    fn ids_attributes_and_pseudo_elements_parse_with_their_specificity() {
        let [selector] = parse("#a.b[data-x~=\"y\" i]::before")
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(selector.specificity(), (1, 2, 1));
        assert_eq!(selector.pseudo_element, Some(PseudoElement::Before));
        let compound = selector.subject();
        assert_eq!(compound.ids, vec!["a"]);
        assert_eq!(
            compound.attributes,
            vec![AttributeSelector {
                name: "data-x".into(),
                test: Some((
                    AttributeMatcher::Includes,
                    "y".into(),
                    AttributeCase::Insensitive
                )),
            }]
        );
        let preflight = parse("*, ::after, ::before, ::backdrop, ::file-selector-button").unwrap();
        assert_eq!(preflight[3].pseudo_element, Some(PseudoElement::Backdrop));
        assert_eq!(
            parse("p:first-line").unwrap()[0].pseudo_element,
            Some(PseudoElement::FirstLine)
        );
        assert!(parse(":placeholder").is_err());
        let hidden = parse("[hidden]:where(:not([hidden='until-found']))").unwrap();
        assert_eq!(hidden[0].specificity(), (0, 1, 0));
        let [has] = parse(".a:has(> .b, + .c ~ div)")
            .unwrap()
            .try_into()
            .unwrap();
        let Pseudo::Has(list) = &has.subject().pseudo[0] else {
            panic!("{has:?}");
        };
        assert_eq!(list[0].leading, Combinator::Child);
        assert_eq!(list[1].leading, Combinator::NextSibling);
        assert_eq!(has.specificity(), (0, 2, 1));
        for matcher in ["=", "~=", "|=", "^=", "$=", "*="] {
            assert!(parse(&format!("[a{matcher}b s]")).is_ok(), "{matcher}");
        }
        assert_eq!(
            parse(":dir(rtl):lang(en, \"de-*\")").unwrap()[0].specificity(),
            (0, 2, 0)
        );
    }

    /// Selectors 4 §6.1–§6.3 with HTML's case-insensitive attribute values.
    #[test]
    fn attribute_matchers_follow_the_specification() {
        let test = |text: &str, value: Option<&str>| {
            let selector = parse(text).unwrap();
            selector[0].subject().attributes[0].matches(value)
        };
        assert!(test("[a]", Some("")));
        assert!(!test("[a]", None));
        assert!(test("[a=x]", Some("x")) && !test("[a=x]", Some("X")));
        assert!(test("[a=x i]", Some("X")));
        assert!(test("[type=TEXT]", Some("text")) && !test("[type=TEXT s]", Some("text")));
        assert!(test("[a~=b]", Some("a b\tc")) && !test("[a~=\"b c\"]", Some("b c")));
        assert!(!test("[a~=\"\"]", Some("")));
        assert!(test("[a|=en]", Some("en-US")) && test("[a|=en]", Some("en")));
        assert!(!test("[a|=en]", Some("english")));
        assert!(test("[a^=ab]", Some("abc")) && !test("[a^=\"\"]", Some("abc")));
        assert!(test("[a$=bc]", Some("abc")) && !test("[a$=\"\"]", Some("abc")));
        assert!(test("[a*=b]", Some("abc")) && !test("[a*=\"\"]", Some("abc")));
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
