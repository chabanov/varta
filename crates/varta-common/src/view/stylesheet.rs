//! The checked stylesheet grammar of draft v0, shared by the headless
//! renderer (`varta-view`, which applies it at patch time) and the compiler
//! (which checks a statically known `View.stylesheet` text at build time).
//! A violation reports the zero-based UTF-8 byte offset where it was found.
//!
//! The text is tokenized by CSS Syntax 3 ([`super::syntax`]: comments
//! dropped, escapes decoded) and parsed as a list of rules (§5.4): style
//! rules whose selectors are compound class selectors with the subset's
//! pseudo-classes or `:root` (Selectors 4 §6.6, §16), `@media` (Media
//! Queries 4/5, see [`media`]), `@layer` statements and blocks (Cascade 5
//! §6.4) and `@property` (Properties and Values API 1 §3). Anything else is
//! a violation: the subset fails closed instead of dropping what it does not
//! admit. Forbidden constructs are found on decoded tokens, so no escape can
//! hide them.
use super::{CssProperty, PSEUDO_CLASSES, css, syntax};
#[allow(unused_imports)]
use crate::view::prelude::*;
use syntax::{Spanned, Token};

pub mod media;
mod supports;
pub use media::{Media, MediaQueryList};

/// CSS pixels per `rem` (the initial font size).
pub const REM_PIXELS: f64 = 16.0;
/// Nesting depth of `@media` and `@layer` blocks.
pub const MAX_NESTING: usize = 8;
/// `@property` registrations of a document (all sheets together): the
/// native resolver seeds every one of them into each element's scope.
pub const MAX_REGISTERED_PROPERTIES: usize = 512;

/// A declared name: a subset property or a custom property (`--name`).
#[derive(Debug, Clone, PartialEq)]
pub enum Name {
    Property(CssProperty),
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pseudo {
    Hover,
    Active,
    FocusVisible,
    Disabled,
    Checked,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub root: bool,
    pub classes: Vec<String>,
    pub pseudo: Vec<Pseudo>,
}
impl Selector {
    /// Classes plus pseudo-classes; `:root` counts as one pseudo-class
    /// (Selectors 4 §17: all of them are the B component).
    #[must_use]
    pub fn specificity(&self) -> usize {
        self.classes.len() + self.pseudo.len() + usize::from(self.root)
    }
}

/// One segment of a layer name (Cascade 5 §6.4.2); each anonymous layer is
/// distinct.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayerName {
    Named(String),
    Anonymous(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<(Name, String)>,
    /// The media query lists of every enclosing `@media` block; all must
    /// match.
    pub media: Vec<MediaQueryList>,
    /// The rule's cascade layer: an index into [`ParsedStylesheet::layers`],
    /// or `None` for an unlayered rule.
    pub layer: Option<usize>,
    /// Whether these are the rule's `!important` declarations (Cascade 5
    /// §6.1, "Origin and Importance"); a rule with both kinds is parsed into
    /// a normal and an important record, in that order.
    pub important: bool,
}

/// One declaration of a layer name (Cascade 5 §6.4.3): an index into
/// [`ParsedStylesheet::layers`] and the media query lists of the conditional
/// rules around it. The layer order counts only the declarations whose
/// conditions currently hold.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerDeclaration {
    pub layer: usize,
    pub media: Vec<MediaQueryList>,
}

/// The `syntax` descriptor of a registered custom property (Properties and
/// Values API 1 §3.1, §5).
#[derive(Debug, Clone, PartialEq)]
pub enum Syntax {
    /// `"*"`: any token sequence.
    Universal,
    /// A syntax definition, as a value grammar of [`css::matches_engine`].
    Grammar(String),
}

/// An `@property` rule (Properties and Values API 1 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct Registration {
    /// The name without its leading `--`.
    pub name: String,
    pub syntax: Syntax,
    pub inherits: bool,
    /// The initial value; `None` (only with the universal syntax) is the
    /// guaranteed-invalid value.
    pub initial: Option<String>,
}
impl Registration {
    /// Whether the syntax has length components, whose computed values are
    /// absolute lengths (Properties and Values API 1 §2.4).
    #[must_use]
    pub fn has_lengths(&self) -> bool {
        matches!(&self.syntax, Syntax::Grammar(grammar) if grammar.contains("<length"))
    }
    /// Whether a computed value matches the registered syntax.
    #[must_use]
    pub fn admits(&self, value: &str) -> bool {
        match &self.syntax {
            Syntax::Universal => true,
            Syntax::Grammar(grammar) => css::matches_engine(grammar, value),
        }
    }
}

/// What an element exposes to selector matching.
pub struct Subject<'a> {
    pub root: bool,
    pub classes: &'a [&'a str],
    pub focused: bool,
    pub hovered: bool,
    pub active: bool,
    pub disabled: bool,
    pub checked: bool,
}

impl Rule {
    /// The number of media feature tests that guard this rule.
    #[must_use]
    pub fn media_features(&self) -> usize {
        self.media.iter().map(MediaQueryList::features).sum()
    }
    /// Canonical syntax/source cost of considering this rule, independent of
    /// media or pseudo-class state. Uses the parsed stylesheet node weight.
    #[must_use]
    pub fn cascade_work(&self) -> usize {
        let weight = super::DRAFT_STYLESHEET_NODE_WORK;
        let mut work = weight.saturating_mul(1 + self.media_features());
        for selector in &self.selectors {
            work = work.saturating_add(weight.saturating_mul(1 + selector.pseudo.len()));
            for class in &selector.classes {
                work = work.saturating_add(weight + class.len());
            }
        }
        for (name, value) in &self.declarations {
            work = work.saturating_add(weight + value.len());
            if let Name::Custom(name) = name {
                work = work.saturating_add(name.len());
            }
        }
        work
    }
    /// Whether every enclosing media query list matches `media`.
    #[must_use]
    pub fn applies(&self, media: &Media) -> bool {
        self.media.iter().all(|list| list.matches(media))
    }
    /// The highest specificity among the selectors that match `subject`.
    #[must_use]
    pub fn matching(&self, subject: &Subject<'_>) -> Option<usize> {
        self.matching_with_classes(subject, |class| subject.classes.contains(&class))
    }
    /// The same matcher with a caller-owned class membership index.
    #[must_use]
    pub fn matching_with_classes(
        &self,
        subject: &Subject<'_>,
        contains: impl Fn(&str) -> bool,
    ) -> Option<usize> {
        self.selectors
            .iter()
            .filter(|selector| {
                selector.root == subject.root
                    && (selector.root || !selector.classes.is_empty())
                    && selector.classes.iter().all(|class| contains(class))
                    && selector.pseudo.iter().all(|pseudo| match pseudo {
                        Pseudo::Hover => subject.hovered,
                        Pseudo::Active => subject.active,
                        Pseudo::FocusVisible => subject.focused,
                        Pseudo::Disabled => subject.disabled,
                        Pseudo::Checked => subject.checked,
                    })
            })
            .map(Selector::specificity)
            .max()
    }
}

/// What a stylesheet violates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    #[error(
        "forbidden CSS construct (`url()`, `image-set()`, `expression()`, `attr()`, `element()`, `@import` or an HTML comment token)"
    )]
    Forbidden,
    #[error(
        "expected a selector: compound class selectors (`.a.b:hover`) or `:root`; combinators, type, id, attribute and other pseudo selectors are outside the subset"
    )]
    Selector,
    #[error("expected `{0}`")]
    Expected(&'static str),
    #[error(
        "invalid @media query (media types all/screen/print; features width, height, orientation, prefers-color-scheme, prefers-reduced-motion, prefers-contrast, forced-colors, hover, any-hover, pointer, any-pointer)"
    )]
    Media,
    #[error("invalid declaration: unknown property or value outside the subset grammar")]
    Declaration,
    #[error(
        "the value is valid CSS that the native renderer cannot render yet (for example `em` outside `font-size`, `ch`/`lh`, a sum of a percentage and a length, or `currentcolor` inside `color-mix()`)"
    )]
    Unrenderable,
    #[error("`font-family` names the CSS-wide keyword `{0}`, which the subset does not admit")]
    FontFamilyKeyword(&'static str),
    #[error("empty declaration")]
    EmptyDeclaration,
    #[error("stylesheet work limit exceeded")]
    WorkLimit,
    #[error("unterminated rule")]
    Unterminated,
    #[error(
        "at-rule outside the subset (admitted: @media, @supports, @layer, @property) or in a context that does not admit it"
    )]
    AtRule,
    #[error("invalid @layer name list")]
    Layer,
    #[error(
        "invalid @property rule (a custom property name; `syntax`, `inherits` and, unless the syntax is `\"*\"`, a computationally independent `initial-value`)"
    )]
    Property,
    #[error("@media, @supports and @layer blocks nest deeper than the subset admits")]
    Nesting,
    #[error("invalid @supports condition")]
    Supports,
}

/// A stylesheet violation and the zero-based UTF-8 byte offset where it was
/// found in the stylesheet text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{violation} at byte {offset}")]
pub struct StylesheetError {
    pub violation: Violation,
    pub offset: usize,
}

type Parsed<T> = Result<T, StylesheetError>;

/// Checked rules and their canonical parsed-work charge.
#[derive(Debug, Clone)]
pub struct ParsedStylesheet {
    pub rules: Vec<Rule>,
    pub work: usize,
    /// Every layer name declared, each as its full path from the top level
    /// (rules refer to them by index).
    pub layers: Vec<Vec<LayerName>>,
    /// The layer declarations in source order, with their conditions.
    pub layer_declarations: Vec<LayerDeclaration>,
    /// `@property` registrations in stylesheet order.
    pub properties: Vec<Registration>,
}

/// Where a rule list is parsed.
#[derive(Debug, Clone)]
struct Context {
    media: Vec<MediaQueryList>,
    /// The enclosing layer's full path (empty at the top level).
    layer: Vec<LayerName>,
    depth: usize,
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Spanned>,
    at: usize,
    work: usize,
    allowance: usize,
    rules: Vec<Rule>,
    layers: Vec<Vec<LayerName>>,
    /// Each declared layer path's index in `layers`.
    layer_index: BTreeMap<Vec<LayerName>, usize>,
    layer_declarations: Vec<LayerDeclaration>,
    /// Layers already declared without a condition.
    unconditional: BTreeSet<usize>,
    anonymous: usize,
    properties: Vec<Registration>,
}

impl Parser<'_> {
    fn charge(&mut self, work: usize) -> Parsed<()> {
        self.work = self.work.saturating_add(work);
        if self.work > self.allowance {
            return self.fail(Violation::WorkLimit);
        }
        Ok(())
    }
    fn node(&mut self) -> Parsed<()> {
        self.charge(super::DRAFT_STYLESHEET_NODE_WORK)
    }
    fn offset(&self, at: usize) -> usize {
        self.tokens.get(at).map_or(self.text.len(), |t| t.start)
    }
    fn fail<T>(&self, violation: Violation) -> Parsed<T> {
        self.fail_at(self.at, violation)
    }
    fn fail_at<T>(&self, at: usize, violation: Violation) -> Parsed<T> {
        Err(StylesheetError {
            violation,
            offset: self.offset(at),
        })
    }
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|t| &t.token)
    }
    fn skip(&mut self) {
        while self.peek() == Some(&Token::Whitespace) {
            self.at += 1;
        }
    }
    /// The index of the token that closes the block opened just before
    /// `from` (a `{`), or the end.
    fn block_end(&self, from: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (index, spanned) in self.tokens.iter().enumerate().skip(from) {
            match spanned.token {
                Token::OpenCurly | Token::OpenParen | Token::OpenSquare | Token::Function(_) => {
                    depth += 1;
                }
                Token::CloseCurly if depth == 0 => return Some(index),
                Token::CloseCurly | Token::CloseParen | Token::CloseSquare => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        None
    }
    /// Parse rules until `end` (a closing `}` or the end of the tokens).
    fn rule_list(&mut self, end: usize, context: &Context) -> Parsed<()> {
        loop {
            self.skip();
            if self.at >= end {
                return Ok(());
            }
            match self.peek() {
                Some(Token::AtKeyword(name)) => {
                    let name = name.to_ascii_lowercase();
                    self.at_rule(&name, context)?;
                }
                Some(Token::Cdo | Token::Cdc) => return self.fail(Violation::Forbidden),
                _ => self.style_rule(context)?,
            }
        }
    }
    fn at_rule(&mut self, name: &str, context: &Context) -> Parsed<()> {
        let start = self.at;
        self.at += 1;
        match name {
            "media" => self.media_rule(context),
            "layer" => self.layer_rule(context),
            "property" if context.depth == 0 => self.property_rule(),
            "supports" => self.supports_rule(context),
            "import" => self.fail_at(start, Violation::Forbidden),
            _ => self.fail_at(start, Violation::AtRule),
        }
    }
    /// The prelude of an at-rule or a qualified rule: tokens up to the `{`
    /// (or `;` when `statement`), returning the index of that token.
    fn prelude(&mut self, statement: bool) -> Parsed<usize> {
        let mut index = self.at;
        while let Some(spanned) = self.tokens.get(index) {
            match spanned.token {
                Token::OpenCurly => return Ok(index),
                Token::Semicolon if statement => return Ok(index),
                Token::Semicolon | Token::CloseCurly => {
                    return self.fail_at(index, Violation::Expected("{"));
                }
                _ => index += 1,
            }
        }
        self.fail_at(index, Violation::Unterminated)
    }
    fn nested(&self, context: &Context) -> Parsed<()> {
        if context.depth >= MAX_NESTING {
            return self.fail(Violation::Nesting);
        }
        Ok(())
    }
    /// `@supports` (CSS Conditional 3 §6): the condition is decided by this
    /// engine's own grammar, so it does not depend on the host; a true block
    /// is parsed like the enclosing rule list, a false one is skipped (its
    /// rules never apply and its layers are not declared, Cascade 5 §6.4.3).
    fn supports_rule(&mut self, context: &Context) -> Parsed<()> {
        self.nested(context)?;
        self.node()?;
        let open = self.prelude(false)?;
        let holds =
            supports::evaluate(&self.tokens[self.at..open]).map_err(|index| StylesheetError {
                violation: Violation::Supports,
                offset: self.offset(self.at + index),
            })?;
        let end = self.block_end(open + 1).map_or_else(
            || self.fail_at(self.tokens.len(), Violation::Unterminated),
            Ok,
        )?;
        if holds {
            let mut inner = context.clone();
            inner.depth += 1;
            self.at = open + 1;
            self.rule_list(end, &inner)?;
        }
        self.at = end + 1;
        Ok(())
    }
    fn media_rule(&mut self, context: &Context) -> Parsed<()> {
        self.nested(context)?;
        let open = self.prelude(false)?;
        let (list, features) =
            media::parse(&self.tokens[self.at..open]).map_err(|index| StylesheetError {
                violation: Violation::Media,
                offset: self.offset(self.at + index),
            })?;
        // Charged per feature test, as the legacy `and` chain was.
        for _ in 0..features.max(1) {
            self.node()?;
        }
        let end = self.block_end(open + 1).map_or_else(
            || self.fail_at(self.tokens.len(), Violation::Unterminated),
            Ok,
        )?;
        let mut inner = context.clone();
        inner.media.push(list);
        inner.depth += 1;
        self.at = open + 1;
        self.rule_list(end, &inner)?;
        self.at = end + 1;
        Ok(())
    }
    /// A `<layer-name>`: identifiers joined by `.` without whitespace.
    fn layer_name(&mut self) -> Parsed<Vec<LayerName>> {
        let mut segments = Vec::new();
        loop {
            let Some(Token::Ident(name)) = self.peek() else {
                return self.fail(Violation::Layer);
            };
            if css::CSS_WIDE_KEYWORDS
                .iter()
                .any(|keyword| name.eq_ignore_ascii_case(keyword))
            {
                return self.fail(Violation::Layer);
            }
            segments.push(LayerName::Named(name.clone()));
            self.at += 1;
            self.node()?;
            if self.peek() == Some(&Token::Delim('.'))
                && matches!(
                    self.tokens.get(self.at + 1).map(|t| &t.token),
                    Some(Token::Ident(_))
                )
            {
                self.at += 1;
            } else {
                return Ok(segments);
            }
        }
    }
    /// Declare a layer path and every prefix, in order (Cascade 5 §6.4.3),
    /// under the conditions of `media`.
    fn declare(&mut self, path: &[LayerName], media: &[MediaQueryList]) -> usize {
        let mut index = 0;
        for length in 1..=path.len() {
            index = match self.layer_index.get(&path[..length]) {
                Some(index) => *index,
                None => {
                    let index = self.layers.len();
                    self.layer_index.insert(path[..length].to_vec(), index);
                    self.layers.push(path[..length].to_vec());
                    index
                }
            };
            // A declaration after an unconditional one cannot move the layer.
            if self.unconditional.contains(&index) {
                continue;
            }
            let repeated = self
                .layer_declarations
                .iter()
                .rev()
                .find(|declaration| declaration.layer == index)
                .is_some_and(|declaration| declaration.media.as_slice() == media);
            if !repeated {
                self.layer_declarations.push(LayerDeclaration {
                    layer: index,
                    media: media.to_vec(),
                });
            }
            if media.is_empty() {
                self.unconditional.insert(index);
            }
        }
        index
    }
    fn layer_rule(&mut self, context: &Context) -> Parsed<()> {
        self.nested(context)?;
        self.node()?;
        let end = self.prelude(true)?;
        let mut names = Vec::new();
        self.skip();
        while self.at < end {
            names.push(self.layer_name()?);
            self.skip();
            if self.at < end {
                if self.peek() != Some(&Token::Comma) {
                    return self.fail(Violation::Layer);
                }
                self.at += 1;
                self.skip();
                if self.at >= end {
                    return self.fail(Violation::Layer);
                }
            }
        }
        let full = |name: &[LayerName]| -> Vec<LayerName> {
            context
                .layer
                .iter()
                .cloned()
                .chain(name.iter().cloned())
                .collect()
        };
        if self.tokens[end].token == Token::Semicolon {
            if names.is_empty() {
                return self.fail_at(end, Violation::Layer);
            }
            for name in &names {
                self.declare(&full(name), &context.media);
            }
            self.at = end + 1;
            return Ok(());
        }
        let path = match names.as_slice() {
            [] => {
                self.anonymous += 1;
                full(&[LayerName::Anonymous(self.anonymous)])
            }
            [name] => full(name),
            _ => return self.fail_at(end, Violation::Layer),
        };
        self.declare(&path, &context.media);
        let close = self.block_end(end + 1).map_or_else(
            || self.fail_at(self.tokens.len(), Violation::Unterminated),
            Ok,
        )?;
        let mut inner = context.clone();
        inner.layer = path;
        inner.depth += 1;
        self.at = end + 1;
        self.rule_list(close, &inner)?;
        self.at = close + 1;
        Ok(())
    }
    fn property_rule(&mut self) -> Parsed<()> {
        self.node()?;
        let open = self.prelude(false)?;
        let start = self.at;
        self.skip();
        let name = match self.peek() {
            Some(Token::Ident(name)) => name.strip_prefix("--").filter(|n| css::ident(n)),
            _ => None,
        }
        .map(str::to_owned)
        .map_or_else(|| self.fail_at(start, Violation::Property), Ok)?;
        self.at += 1;
        self.skip();
        if self.at != open {
            return self.fail(Violation::Property);
        }
        let close = self.block_end(open + 1).map_or_else(
            || self.fail_at(self.tokens.len(), Violation::Unterminated),
            Ok,
        )?;
        let mut syntax = None;
        let mut inherits = None;
        let mut initial = None;
        for (descriptor, first, last, important) in self.declarations(open + 1, close)? {
            if important {
                return self.fail_at(first, Violation::Property);
            }
            self.node()?;
            let text = self.value_text(first, last);
            self.charge(text.len())?;
            let meaningful: Vec<Token> = self.tokens[first..last]
                .iter()
                .map(|t| t.token.clone())
                .filter(|t| *t != Token::Whitespace)
                .collect();
            match descriptor.to_ascii_lowercase().as_str() {
                "syntax" if syntax.is_none() => match meaningful.as_slice() {
                    [Token::String(definition)] => {
                        syntax = Some(
                            parse_syntax(definition)
                                .map_or_else(|| self.fail_at(first, Violation::Property), Ok)?,
                        );
                    }
                    _ => return self.fail_at(first, Violation::Property),
                },
                "inherits" if inherits.is_none() => match meaningful.as_slice() {
                    [Token::Ident(word)] if word.eq_ignore_ascii_case("true") => {
                        inherits = Some(true);
                    }
                    [Token::Ident(word)] if word.eq_ignore_ascii_case("false") => {
                        inherits = Some(false);
                    }
                    _ => return self.fail_at(first, Violation::Property),
                },
                "initial-value" if initial.is_none() => initial = Some((text, first)),
                _ => return self.fail_at(first, Violation::Property),
            }
        }
        let (Some(syntax), Some(inherits)) = (syntax, inherits) else {
            return self.fail_at(start, Violation::Property);
        };
        let initial = match (&syntax, initial) {
            (Syntax::Universal, None) => None,
            (Syntax::Universal, Some((value, first))) => {
                if !css::custom_value(&value) || css::contains_variable(&value) {
                    return self.fail_at(first, Violation::Property);
                }
                Some(value)
            }
            (Syntax::Grammar(grammar), Some((value, first))) => {
                if !css::matches_engine(grammar, &value) || !independent(&value) {
                    return self.fail_at(first, Violation::Property);
                }
                Some(value)
            }
            (Syntax::Grammar(_), None) => return self.fail_at(start, Violation::Property),
        };
        if self.properties.len() >= MAX_REGISTERED_PROPERTIES {
            return self.fail_at(start, Violation::WorkLimit);
        }
        self.properties.push(Registration {
            name,
            syntax,
            inherits,
            initial,
        });
        self.at = close + 1;
        Ok(())
    }
    /// The stored text of a declaration value: the trimmed source, or its
    /// serialization when it has comments or escapes.
    fn value_text(&self, first: usize, last: usize) -> String {
        if first >= last {
            return String::new();
        }
        let start = self.tokens[first].start;
        let end = self.tokens[last - 1].end;
        css::normalize(&self.text[start..end])
    }
    /// Split a declaration block (tokens `from..close`) into declarations:
    /// `(name, first value token, end of value tokens)`, values trimmed of
    /// whitespace. Only a final `;` may leave an empty declaration.
    fn declarations(&self, from: usize, close: usize) -> Parsed<Vec<(String, usize, usize, bool)>> {
        let mut out = Vec::new();
        let mut parts = Vec::new();
        let mut start = from;
        for index in from..close {
            match self.tokens[index].token {
                Token::Semicolon => {
                    parts.push((start, index));
                    start = index + 1;
                }
                Token::OpenCurly | Token::CloseCurly => {
                    return self.fail_at(index, Violation::Declaration);
                }
                _ => {}
            }
        }
        parts.push((start, close));
        let count = parts.len();
        for (index, (mut first, mut last)) in parts.into_iter().enumerate() {
            while first < last && self.tokens[first].token == Token::Whitespace {
                first += 1;
            }
            while last > first && self.tokens[last - 1].token == Token::Whitespace {
                last -= 1;
            }
            if first == last {
                if index + 1 == count && (index > 0 || count == 1) {
                    continue;
                }
                return Err(StylesheetError {
                    violation: Violation::EmptyDeclaration,
                    offset: self.offset(first).min(self.text.len()),
                });
            }
            let Token::Ident(name) = &self.tokens[first].token else {
                return self.fail_at(first, Violation::Declaration);
            };
            let mut colon = first + 1;
            while colon < last && self.tokens[colon].token == Token::Whitespace {
                colon += 1;
            }
            if colon >= last || self.tokens[colon].token != Token::Colon {
                return self.fail_at(first, Violation::Declaration);
            }
            let mut value = colon + 1;
            while value < last && self.tokens[value].token == Token::Whitespace {
                value += 1;
            }
            // Syntax 3 §5.4.6: a final `!` and `important` (ASCII
            // case-insensitively) mark the declaration important and are
            // not part of its value.
            let meaningful: Vec<usize> = (value..last)
                .filter(|&i| self.tokens[i].token != Token::Whitespace)
                .collect();
            let mut important = false;
            if let [.., bang, word] = meaningful.as_slice()
                && self.tokens[*bang].token == Token::Delim('!')
                && matches!(&self.tokens[*word].token, Token::Ident(w) if w.eq_ignore_ascii_case("important"))
            {
                important = true;
                last = *bang;
                while last > value && self.tokens[last - 1].token == Token::Whitespace {
                    last -= 1;
                }
            }
            out.push((name.clone(), value, last, important));
        }
        Ok(out)
    }
    fn style_rule(&mut self, context: &Context) -> Parsed<()> {
        self.charge(
            super::DRAFT_STYLESHEET_NODE_WORK.saturating_mul(
                context
                    .media
                    .iter()
                    .map(MediaQueryList::features)
                    .sum::<usize>()
                    .saturating_add(1),
            ),
        )?;
        let open = self.prelude(false)?;
        let mut selectors = Vec::new();
        let mut start = self.at;
        for index in self.at..=open {
            if index == open || self.tokens[index].token == Token::Comma {
                selectors.push(self.selector(start, index)?);
                start = index + 1;
            }
        }
        let close = self.block_end(open + 1).map_or_else(
            || self.fail_at(self.tokens.len(), Violation::Unterminated),
            Ok,
        )?;
        let mut declarations = Vec::new();
        let mut important = Vec::new();
        for (name, first, last, is_important) in self.declarations(open + 1, close)? {
            let offset = self.declaration_offset(first);
            let value = self.value_text(first, last);
            let entry = declaration(&name, &value)
                .map_err(|violation| StylesheetError { violation, offset })?;
            self.node()?;
            self.charge(
                entry.1.len()
                    + match &entry.0 {
                        Name::Custom(name) => name.len(),
                        Name::Property(_) => 0,
                    },
            )?;
            if is_important {
                important.push(entry);
            } else {
                declarations.push(entry);
            }
        }
        let layer =
            (!context.layer.is_empty()).then(|| self.declare(&context.layer, &context.media));
        let rule = Rule {
            selectors,
            declarations,
            media: context.media.clone(),
            layer,
            important: false,
        };
        if !important.is_empty() {
            self.charge(super::DRAFT_STYLESHEET_NODE_WORK)?;
            let important_rule = Rule {
                declarations: important,
                important: true,
                ..rule.clone()
            };
            self.rules.push(rule);
            self.rules.push(important_rule);
        } else {
            self.rules.push(rule);
        }
        self.at = close + 1;
        Ok(())
    }
    /// The offset of a declaration's name, given its first value token.
    fn declaration_offset(&self, value: usize) -> usize {
        let mut index = value.min(self.tokens.len());
        while index > 0 {
            index -= 1;
            if matches!(self.tokens[index].token, Token::Ident(_))
                && self.tokens[index + 1..value]
                    .iter()
                    .all(|t| matches!(t.token, Token::Whitespace | Token::Colon))
            {
                return self.tokens[index].start;
            }
        }
        self.offset(value)
    }
    /// One compound selector of tokens `start..end` (Selectors 4 §3.1, the
    /// subset: class selectors and pseudo-classes, or `:root` alone).
    fn selector(&mut self, start: usize, end: usize) -> Parsed<Selector> {
        self.node()?;
        let mut first = start;
        let mut last = end;
        while first < last && self.tokens[first].token == Token::Whitespace {
            first += 1;
        }
        while last > first && self.tokens[last - 1].token == Token::Whitespace {
            last -= 1;
        }
        let tokens: Vec<&Token> = self.tokens[first..last].iter().map(|t| &t.token).collect();
        if let [Token::Colon, Token::Ident(name)] = tokens.as_slice()
            && name.eq_ignore_ascii_case("root")
        {
            return Ok(Selector {
                root: true,
                classes: Vec::new(),
                pseudo: Vec::new(),
            });
        }
        let mut classes = Vec::new();
        let mut pseudo = Vec::new();
        let mut index = first;
        while index < last {
            let pair = (
                self.tokens[index].token.clone(),
                self.tokens.get(index + 1).map(|t| t.token.clone()),
            );
            match (&pair.0, pair.1.as_ref()) {
                (Token::Delim('.'), Some(Token::Ident(class))) if index + 1 < last => {
                    if !css::class_name(class) {
                        return self.fail_at(index + 1, Violation::Selector);
                    }
                    let class = class.clone();
                    self.node()?;
                    self.charge(class.len())?;
                    classes.push(class);
                }
                (Token::Colon, Some(Token::Ident(name))) if index + 1 < last => {
                    self.node()?;
                    let Some(found) = PSEUDO_CLASSES
                        .iter()
                        .position(|known| known[1..].eq_ignore_ascii_case(name))
                    else {
                        return self.fail_at(index, Violation::Selector);
                    };
                    pseudo.push(match &PSEUDO_CLASSES[found][1..] {
                        "hover" => Pseudo::Hover,
                        "active" => Pseudo::Active,
                        "focus-visible" => Pseudo::FocusVisible,
                        "disabled" => Pseudo::Disabled,
                        "checked" => Pseudo::Checked,
                        _ => return self.fail_at(index, Violation::Selector),
                    });
                }
                _ => return self.fail_at(index, Violation::Selector),
            }
            index += 2;
        }
        if classes.is_empty() {
            return self.fail_at(first, Violation::Selector);
        }
        Ok(Selector {
            root: false,
            classes,
            pseudo,
        })
    }
}

/// Whether an `initial-value` is computationally independent (Properties
/// and Values API 1 §3.3): no references and no relative units.
fn independent(value: &str) -> bool {
    !css::contains_variable(value)
        && syntax::tokenize(value).iter().all(|t| match &t.token {
            // Every length unit but the absolute ones is relative.
            Token::Dimension(_, unit) => !matches!(
                css::math::length_unit(unit),
                Some(unit) if !matches!(unit, css::math::LengthUnit::Absolute(_))
            ),
            Token::Ident(word) => !word.eq_ignore_ascii_case("currentcolor"),
            _ => true,
        })
}

/// The data types a syntax definition may name (Properties and Values API 1
/// §5.1), as engine leaf types.
const SYNTAX_TYPES: &[(&str, &str)] = &[
    ("length", "<length>"),
    ("number", "<number>"),
    ("percentage", "<percentage>"),
    ("length-percentage", "<length-percentage>"),
    ("color", "<color>"),
    ("integer", "<integer>"),
    ("angle", "<angle>"),
    ("time", "<time>"),
    ("custom-ident", "<custom-ident>"),
    ("string", "<string>"),
];

/// Parse a syntax string (Properties and Values API 1 §5): `*`, or `|`
/// separated components of a data type or an identifier, each with an
/// optional `+` or `#` multiplier.
#[must_use]
pub fn parse_syntax(definition: &str) -> Option<Syntax> {
    let definition = definition.trim_matches(syntax::whitespace);
    if definition == "*" {
        return Some(Syntax::Universal);
    }
    let mut components = Vec::new();
    for component in definition.split('|') {
        let component = component.trim_matches(syntax::whitespace);
        let (base, multiplier) = match component.strip_suffix(['+', '#']) {
            Some(base) => (base, &component[base.len()..]),
            None => (component, ""),
        };
        let leaf = if let Some(name) = base.strip_prefix('<').and_then(|b| b.strip_suffix('>')) {
            SYNTAX_TYPES
                .iter()
                .find(|(known, _)| *known == name)?
                .1
                .to_owned()
        } else {
            let mut chars = base.chars();
            let valid = chars.next().is_some_and(syntax::ident_start)
                && base.chars().all(syntax::ident_char)
                && !css::CSS_WIDE_KEYWORDS
                    .iter()
                    .any(|keyword| base.eq_ignore_ascii_case(keyword))
                && !base.eq_ignore_ascii_case("default");
            valid.then(|| base.to_owned())?
        };
        components.push(format!("{leaf}{multiplier}"));
    }
    Some(Syntax::Grammar(components.join(" | ")))
}

/// A declaration of a style rule: its checked name and stored value.
fn declaration(name: &str, value: &str) -> Result<(Name, String), Violation> {
    if let Some(custom) = name.strip_prefix("--") {
        return (css::ident(custom) && css::custom_value(value))
            .then(|| (Name::Custom(custom.into()), value.into()))
            .ok_or(Violation::Declaration);
    }
    let property = CssProperty::ALL
        .iter()
        .copied()
        .find(|property| property.name().eq_ignore_ascii_case(name))
        .ok_or(Violation::Declaration)?;
    match css::check(property, value) {
        Ok(()) => Ok((Name::Property(property), value.into())),
        Err(css::Refusal::Unrenderable) => Err(Violation::Unrenderable),
        Err(css::Refusal::Invalid) => Err(if property == CssProperty::FontFamily {
            css::font_family_css_wide_keyword(value)
                .map_or(Violation::Declaration, Violation::FontFamilyKeyword)
        } else {
            Violation::Declaration
        }),
    }
}

/// The first forbidden token (decoded, so escapes cannot hide one): a url
/// token, a forbidden function or `@import`.
fn forbidden(tokens: &[Spanned]) -> Option<usize> {
    tokens.iter().find_map(|spanned| {
        let found = match &spanned.token {
            Token::Url(_) | Token::BadUrl => true,
            Token::Function(name) => css::FORBIDDEN_FUNCTIONS
                .iter()
                .any(|forbidden| name.eq_ignore_ascii_case(forbidden)),
            Token::AtKeyword(name) => name.eq_ignore_ascii_case("import"),
            _ => false,
        };
        found.then_some(spanned.start)
    })
}

/// Parse a whole stylesheet: the grammar, forbidden tokens and every
/// declaration against its property grammar.
///
/// # Errors
/// The first violation, with its byte offset.
pub fn parse(text: &str) -> Result<Vec<Rule>, StylesheetError> {
    parse_with_work_limit(text, super::DRAFT_MAX_STYLESHEET_WORK).map(|parsed| parsed.rules)
}

/// Parse within a remaining transaction allowance, capped by the shared limit.
/// The counter is checked before copying media conditions into another rule.
pub fn parse_with_work_limit(
    text: &str,
    allowance: usize,
) -> Result<ParsedStylesheet, StylesheetError> {
    let allowance = allowance.min(super::DRAFT_MAX_STYLESHEET_WORK);
    if text.len() > allowance {
        return Err(StylesheetError {
            violation: Violation::WorkLimit,
            offset: 0,
        });
    }
    let tokens = syntax::tokenize(text);
    if let Some(offset) = forbidden(&tokens) {
        return Err(StylesheetError {
            violation: Violation::Forbidden,
            offset,
        });
    }
    let end = tokens.len();
    let mut parser = Parser {
        text,
        tokens,
        at: 0,
        work: text.len(),
        allowance,
        rules: Vec::new(),
        layers: Vec::new(),
        layer_index: BTreeMap::new(),
        layer_declarations: Vec::new(),
        unconditional: BTreeSet::new(),
        anonymous: 0,
        properties: Vec::new(),
    };
    let top = Context {
        media: Vec::new(),
        layer: Vec::new(),
        depth: 0,
    };
    parser.rule_list(end, &top)?;
    Ok(ParsedStylesheet {
        rules: parser.rules,
        work: parser.work,
        layers: parser.layers,
        layer_declarations: parser.layer_declarations,
        properties: parser.properties,
    })
}

#[cfg(test)]
mod tests;
