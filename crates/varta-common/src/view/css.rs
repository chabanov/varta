//! A checked matcher for the CSS value grammars of [`super::CssProperty`]
//! (draft v0). Appliers and the headless renderer validate every `set_style`
//! value and every stylesheet declaration with it before any effect, so one
//! source decides which values are in the subset.
//!
//! Grammars are the static value definitions of [`super::CssProperty::grammar`]
//! (`a | b` choice, `a{1,4}` repetition, `a || b` any order, `a#` comma list,
//! `[ … ]` grouping, `name( … )` functions) over the [`super::CssValue`] types.
//! Values are tokenized by CSS Syntax 3 ([`super::syntax`]): comments are
//! dropped and escapes decoded before any check, and a value longer than
//! [`MAX_VALUE_TOKENS`] tokens fails closed, which bounds the matcher's work.
//!
//! Two semantics share the grammar engine:
//!
//! - [`matches`] is the host contract of the L0 browser host (the registry's
//!   grammars over the registry's leaf types, unchanged until the browser
//!   host runs this engine through Wasm);
//! - [`property_value`] is the engine: the [`grammar`] of each property
//!   (the registry grammar plus the values the native renderer supports),
//!   whose leaf types follow CSS Values 4 (every length unit of §6, math
//!   functions of §10 wherever a numeric type is allowed) and CSS Color 4/5
//!   ([`color`]). A valid engine value is then lowered ([`lower`]) at
//!   computed-value time to the host grammar the renderer's consumers read.
use super::{CssProperty, CssValue, FORBIDDEN_CSS, FORBIDDEN_STYLE_VALUE, syntax};
#[allow(unused_imports)]
use crate::view::prelude::*;

pub mod color;
pub mod math;

/// Tokens of one property value (placeholder budget, like the `DRAFT_*`
/// limits): every subset grammar needs far fewer.
pub const MAX_VALUE_TOKENS: usize = 128;
/// Hexadecimal digits a `<hex>` colour of the host grammar admits: `#rgb`,
/// `#rrggbb`, `#rrggbbaa`. The engine also admits `#rgba` (Color 4 §5.2).
pub const HEX_COLOR_DIGITS: [usize; 3] = [3, 6, 8];
/// CSS-wide keywords, shared by every keyword check of the subset. A family
/// name is a custom identifier, which excludes them, and the subset does not
/// admit them as `font-family` values: the browser would roll back to its own
/// font, which no native host can supply.
pub const CSS_WIDE_KEYWORDS: &[&str] = &["inherit", "initial", "unset", "revert", "revert-layer"];
/// The longest class name (UTF-8 bytes) of the HTML class model the engine
/// admits in `set_class`, TSX and selectors.
pub const MAX_CLASS_NAME_BYTES: usize = 256;
/// Functions that never appear in an engine stylesheet or value, wherever
/// they occur and however their names are escaped: fetches (`url()` and the
/// url token, `image-set()`), legacy script (`expression()`) and document
/// reads (`attr()`, `element()`).
pub const FORBIDDEN_FUNCTIONS: &[&str] = &[
    "url",
    "image-set",
    "-webkit-image-set",
    "expression",
    "attr",
    "element",
    "-moz-element",
];
/// The magnitude to which an infinite or overlarge lowered length or number
/// is clamped (CSS Values 4 §10.12 leaves the finite range to the UA); it is
/// the native renderer's length range.
pub const MAX_LOWERED_MAGNITUDE: f64 = 1_000_000.0;

/// The CSS-wide keyword an unquoted `font-family` component of `value`
/// names (ASCII case-insensitively), if any: the diagnostic for a value
/// [`property_value`] refuses for that reason. Native `set_style`, the
/// stylesheet checker and the compiler name it in their errors.
#[must_use]
pub fn font_family_css_wide_keyword(value: &str) -> Option<&'static str> {
    lex_value(value)?.iter().find_map(|token| match token {
        V::Ident(word) => CSS_WIDE_KEYWORDS
            .iter()
            .copied()
            .find(|keyword| word.eq_ignore_ascii_case(keyword)),
        _ => None,
    })
}

/// Whether `token` is a class name of the HTML class model: one
/// space-separated token, any code points but ASCII whitespace and
/// controls, at most [`MAX_CLASS_NAME_BYTES`] bytes.
#[must_use]
pub fn class_name(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_CLASS_NAME_BYTES
        && !token
            .chars()
            .any(|c| c.is_ascii_whitespace() || c.is_control())
}

/// Whether a `class` attribute value is a list of [`class_name`] tokens
/// separated by ASCII whitespace.
#[must_use]
pub fn class_list(value: &str) -> bool {
    value.split_ascii_whitespace().all(class_name)
}

#[derive(Debug, Clone, PartialEq)]
enum G {
    Token(Separator),
    Keyword(String),
    Type(String),
    Function(String, Box<G>),
    Seq(Vec<G>),
    Alt(Vec<G>),
    AnyOrder(Vec<G>),
    Repeat(Box<G>, usize, usize, bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separator {
    Comma,
    Slash,
}

fn lex_grammar(text: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '|' && chars.get(i + 1) == Some(&'|') {
            out.push("||".into());
            i += 2;
        } else if "[](),/?+#|".contains(c) {
            out.push(c.to_string());
            i += 1;
        } else if c == '{' || c == '<' {
            let close = if c == '{' { '}' } else { '>' };
            let end = i + chars[i..].iter().position(|&d| d == close)?;
            out.push(chars[i..=end].iter().collect());
            i = end + 1;
        } else {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || "-_.".contains(chars[i])) {
                i += 1;
            }
            if i == start {
                return None;
            }
            let word: String = chars[start..i].iter().collect();
            if chars.get(i) == Some(&'(') {
                out.push(format!("{word}("));
                i += 1;
            } else {
                out.push(word);
            }
        }
    }
    Some(out)
}

struct Parser {
    tokens: Vec<String>,
    at: usize,
}
impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }
    fn alt(&mut self) -> Option<G> {
        let mut items = vec![self.any_order()?];
        while self.peek() == Some("|") {
            self.at += 1;
            items.push(self.any_order()?);
        }
        Some(if items.len() == 1 {
            items.pop()?
        } else {
            G::Alt(items)
        })
    }
    fn any_order(&mut self) -> Option<G> {
        let mut items = vec![self.seq()?];
        while self.peek() == Some("||") {
            self.at += 1;
            items.push(self.seq()?);
        }
        Some(if items.len() == 1 {
            items.pop()?
        } else {
            G::AnyOrder(items)
        })
    }
    fn seq(&mut self) -> Option<G> {
        let mut items = Vec::new();
        while let Some(token) = self.peek() {
            if matches!(token, "|" | "||" | "]" | ")") {
                break;
            }
            let atom = self.atom()?;
            items.push(self.multiplier(atom)?);
        }
        match items.len() {
            0 => None,
            1 => items.pop(),
            _ => Some(G::Seq(items)),
        }
    }
    fn atom(&mut self) -> Option<G> {
        let token = self.tokens.get(self.at)?.clone();
        self.at += 1;
        if token == "[" {
            let inner = self.alt()?;
            (self.peek() == Some("]")).then_some(())?;
            self.at += 1;
            Some(inner)
        } else if let Some(name) = token.strip_suffix('(') {
            let inner = self.alt()?;
            (self.peek() == Some(")")).then_some(())?;
            self.at += 1;
            Some(G::Function(name.to_owned(), Box::new(inner)))
        } else if token.starts_with('<') {
            Some(G::Type(token))
        } else if token == "," {
            Some(G::Token(Separator::Comma))
        } else if token == "/" {
            Some(G::Token(Separator::Slash))
        } else {
            Some(G::Keyword(token))
        }
    }
    fn multiplier(&mut self, atom: G) -> Option<G> {
        let (min, max, comma) = match self.peek() {
            Some("?") => (0, 1, false),
            Some("+") => (1, usize::MAX, false),
            Some("#") => (1, usize::MAX, true),
            Some(token) if token.starts_with('{') => {
                let inner = token.strip_prefix('{')?.strip_suffix('}')?;
                let (low, high) = inner.split_once(',')?;
                (low.parse().ok()?, high.parse().ok()?, false)
            }
            _ => return Some(atom),
        };
        self.at += 1;
        Some(G::Repeat(Box::new(atom), min, max, comma))
    }
}

fn parse(text: &str) -> Option<G> {
    let mut parser = Parser {
        tokens: lex_grammar(text)?,
        at: 0,
    };
    let grammar = parser.alt()?;
    (parser.at == parser.tokens.len()).then_some(grammar)
}

/// The unit of a numeric value token.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NumUnit {
    None,
    Percent,
    Dimension(String),
}

/// One component token of a value (CSS Syntax 3 tokens without
/// whitespace; a `( … )` simple block is a function with an empty name).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum V {
    Ident(String),
    Num {
        /// The source text: representation plus unit or `%`.
        text: String,
        value: f64,
        integer: bool,
        unit: NumUnit,
    },
    Function(String),
    Close,
    Comma,
    Slash,
    Hash(String),
    Str(String),
    /// `+`, `-` or `*` (only meaningful inside math functions).
    Delim(char),
}

/// Lex a value with the CSS Syntax 3 tokenizer; `None` for a token outside
/// the value syntax (`;`, braces, brackets, `!`, at-keywords, bad strings
/// and urls), a forbidden function or url, unbalanced parentheses or more
/// than [`MAX_VALUE_TOKENS`] tokens.
pub(crate) fn lex_value(text: &str) -> Option<Vec<V>> {
    use syntax::Token as T;
    let mut out = Vec::new();
    let mut nesting = 0usize;
    for spanned in syntax::tokenize(text) {
        if spanned.token == syntax::Token::Whitespace {
            continue;
        }
        if out.len() >= MAX_VALUE_TOKENS {
            return None;
        }
        let number = |n: &syntax::Numeric, unit: NumUnit, suffix: &str| V::Num {
            text: format!("{}{suffix}", n.repr),
            value: n.value,
            integer: n.integer,
            unit,
        };
        out.push(match spanned.token {
            T::Ident(name) => V::Ident(name),
            T::Function(name) => {
                if FORBIDDEN_FUNCTIONS
                    .iter()
                    .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
                {
                    return None;
                }
                nesting += 1;
                V::Function(name)
            }
            T::OpenParen => {
                nesting += 1;
                V::Function(String::new())
            }
            T::CloseParen => {
                nesting = nesting.checked_sub(1)?;
                V::Close
            }
            T::Comma => V::Comma,
            T::Delim('/') => V::Slash,
            T::Delim(c @ ('+' | '-' | '*')) => V::Delim(c),
            T::Hash { value, .. } => V::Hash(value),
            T::String(value) => V::Str(value),
            T::Number(n) => number(&n, NumUnit::None, ""),
            T::Percentage(n) => number(&n, NumUnit::Percent, "%"),
            T::Dimension(n, unit) => {
                let suffix = unit.clone();
                number(&n, NumUnit::Dimension(unit), &suffix)
            }
            _ => return None,
        });
    }
    (nesting == 0).then_some(out)
}

/// Format a number without an exponent (Rust's `Display` of `f64`), with
/// negative zero as `0`.
#[must_use]
pub fn format_number(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else {
        value.to_string()
    }
}

fn number(word: &str) -> bool {
    let digits = word.strip_prefix('-').unwrap_or(word);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let all = |part: &str| part.chars().all(|c| c.is_ascii_digit());
    !digits.is_empty()
        && all(whole)
        && all(fraction)
        && (!whole.is_empty() || !fraction.is_empty())
        && (!digits.contains('.') || !fraction.is_empty())
}
fn unit(word: &str, units: &[&str]) -> bool {
    units
        .iter()
        .any(|unit| word.strip_suffix(unit).is_some_and(number))
}

/// Composite types are themselves grammars over the leaf types.
/// Generated hosts use these same definitions instead of duplicating grammars.
#[must_use]
pub fn composite(name: &str) -> Option<&'static str> {
    let value = CssValue::ALL.iter().find(|value| value.name() == name)?;
    match value {
        CssValue::LengthPercentage => Some("<length> | <percentage>"),
        CssValue::Color => Some(
            "<hex> | transparent | currentcolor | rgb(<integer>, <integer>, <integer>) | rgba(<integer>, <integer>, <integer>, <number>)",
        ),
        CssValue::Track => Some(
            "<length-percentage> | <flex> | auto | minmax(<track>, <track>) | repeat(<integer>, <track>+)",
        ),
        CssValue::Line => Some("auto | <integer> | span <integer>"),
        _ => None,
    }
}

/// Which semantics the leaf types have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The registry's host contract.
    Host,
    /// The host contract plus the layout-time math functions [`lower`]
    /// emits for the properties of [`layout_math_property`].
    Lowered,
    /// CSS Values 4 and Color 4/5 leaf types.
    Engine,
}

fn host_leaf(name: &str, token: &V) -> bool {
    match (name, token) {
        ("<hex>", V::Hash(hex)) => {
            HEX_COLOR_DIGITS.contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit())
        }
        ("<string>", V::Str(_)) => true,
        ("<ident>", V::Ident(word)) => ident(word),
        ("<custom-ident>", V::Ident(_)) => true,
        (_, V::Num { text, .. }) => match name {
            "<number>" => number(text),
            "<integer>" => {
                let digits = text.strip_prefix('-').unwrap_or(text);
                !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
            }
            "<length>" => text == "0" || unit(text, &["px", "rem"]),
            "<percentage>" => unit(text, &["%"]),
            "<flex>" => unit(text, &["fr"]),
            "<time>" => unit(text, &["ms", "s"]),
            _ => false,
        },
        _ => false,
    }
}

/// The properties whose native consumers resolve a layout-time `calc()`
/// against their percentage basis: box sizes, margins, padding, gaps, the
/// flex basis, insets and `vertical-align`. Elsewhere such a value is not
/// renderable yet.
#[must_use]
pub fn layout_math_property(property: CssProperty) -> bool {
    use CssProperty as P;
    matches!(
        property,
        P::Width
            | P::Height
            | P::MinWidth
            | P::MinHeight
            | P::MaxWidth
            | P::MaxHeight
            | P::Margin
            | P::MarginTop
            | P::MarginRight
            | P::MarginBottom
            | P::MarginLeft
            | P::Padding
            | P::PaddingTop
            | P::PaddingRight
            | P::PaddingBottom
            | P::PaddingLeft
            | P::Gap
            | P::RowGap
            | P::ColumnGap
            | P::FlexBasis
            | P::Top
            | P::Right
            | P::Bottom
            | P::Left
            | P::VerticalAlign
    )
}

/// A layout-time math function of the host grammar: a `<length-percentage>`
/// that sums lengths with percentages, lowered by [`lower`] to `px`,
/// percentage and number leaves (CSS Values 4 §10.10: resolved at layout).
fn host_math(name: &str, tokens: &[V], at: usize) -> Option<usize> {
    if name != "<length-percentage>" || !math::is_math(tokens.get(at)) {
        return None;
    }
    let (expression, end) = math::parse(tokens, at)?;
    (expression.layout_ready()
        && expression.resolved_type(Some(math::Base::Length))
            == Some(math::Type::of(math::Base::Length)))
    .then_some(end)
}

/// A layout-time length of the host grammar (see [`host_math`]): `None`
/// unless `value` is one lowered math function.
#[must_use]
pub fn layout_math(value: &str) -> Option<math::Expression> {
    let tokens = lex_value(value)?;
    let end = host_math("<length-percentage>", &tokens, 0)?;
    // Only a lowered value reaches layout: its own text is `layout_ready`.
    (end == tokens.len()).then(|| math::parse(&tokens, 0).map(|(expression, _)| expression))?
}

/// The math base a numeric leaf type resolves to, with its percent hint.
fn math_target(name: &str) -> Option<(math::Type, Option<math::Base>)> {
    use math::{Base, Type};
    Some(match name {
        "<number>" | "<integer>" => (Type::NUMBER, None),
        "<length>" => (Type::of(Base::Length), None),
        "<percentage>" => (Type::of(Base::Percent), None),
        "<length-percentage>" => (Type::of(Base::Length), Some(Base::Length)),
        "<time>" => (Type::of(Base::Time), None),
        "<angle>" => (Type::of(Base::Angle), None),
        _ => return None,
    })
}

/// Engine leaf types: CSS Values 4 numerics and math functions, Color 4/5
/// colours. Returns the ends of every match at `at`.
fn engine_leaf(name: &str, tokens: &[V], at: usize) -> Option<Vec<usize>> {
    let token = tokens.get(at)?;
    if math::is_math(Some(token)) {
        let (target, percent_as) = math_target(name)?;
        let (expression, end) = math::parse(tokens, at)?;
        return (expression.resolved_type(percent_as) == Some(target)).then(|| vec![end]);
    }
    let numeric = |token: &V| match token {
        V::Num {
            value,
            integer,
            unit,
            ..
        } if value.is_finite() => Some((*value, *integer, unit.clone())),
        _ => None,
    };
    let found = match name {
        "<color>" => return color::parse(tokens, at).map(|(_, end)| vec![end]),
        "<hex>" => {
            matches!(token, V::Hash(hex) if color::HEX_DIGITS.contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit()))
        }
        "<number>" => matches!(numeric(token), Some((_, _, NumUnit::None))),
        "<integer>" => matches!(numeric(token), Some((_, true, NumUnit::None))),
        "<length>" => match numeric(token) {
            Some((value, _, NumUnit::None)) => value == 0.0,
            Some((_, _, NumUnit::Dimension(unit))) => math::length_unit(&unit).is_some(),
            _ => false,
        },
        "<percentage>" => matches!(numeric(token), Some((_, _, NumUnit::Percent))),
        "<length-percentage>" => {
            return Some(
                ["<length>", "<percentage>"]
                    .iter()
                    .filter_map(|leaf| engine_leaf(leaf, tokens, at))
                    .flatten()
                    .collect(),
            );
        }
        "<time>" => {
            matches!(numeric(token), Some((_, _, NumUnit::Dimension(unit))) if math::time_unit(&unit).is_some())
        }
        "<angle>" => {
            matches!(numeric(token), Some((_, _, NumUnit::Dimension(unit))) if math::angle_unit(&unit).is_some())
        }
        "<flex>" => {
            matches!(numeric(token), Some((_, _, NumUnit::Dimension(unit))) if unit.eq_ignore_ascii_case("fr"))
        }
        "<ident>" => matches!(token, V::Ident(word) if ident(word)),
        // CSS Values 4 §3.2: any identifier; callers exclude reserved words.
        "<custom-ident>" => matches!(token, V::Ident(_)),
        "<string>" => matches!(token, V::Str(_)),
        _ => return None,
    };
    Some(if found { vec![at + 1] } else { vec![] })
}

/// `[A-Za-z_][A-Za-z0-9_-]*`
#[must_use]
pub fn ident(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
}

/// Conservative reference probe for work admission, including quoted text.
#[must_use]
pub fn contains_variable(value: &str) -> bool {
    value
        .as_bytes()
        .windows(4)
        .any(|part| part.eq_ignore_ascii_case(b"var("))
}
fn function_end(tokens: &[V], at: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(at) {
        match token {
            V::Function(_) => depth += 1,
            V::Close => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}
fn is_var(token: Option<&V>) -> bool {
    matches!(token, Some(V::Function(name)) if name.eq_ignore_ascii_case("var"))
}
fn valid_variables(tokens: &[V]) -> bool {
    tokens
        .iter()
        .enumerate()
        .all(|(at, token)| !is_var(Some(token)) || !var(tokens, at).is_empty())
}
/// A reference or a reference with a balanced component fallback.
fn var(tokens: &[V], at: usize) -> Vec<usize> {
    if !is_var(tokens.get(at)) {
        return vec![];
    }
    let Some(V::Ident(name)) = tokens.get(at + 1) else {
        return vec![];
    };
    if !name.strip_prefix("--").is_some_and(ident) {
        return vec![];
    }
    let Some(end) = function_end(tokens, at) else {
        return vec![];
    };
    match tokens.get(at + 2) {
        Some(V::Close) => vec![end],
        Some(V::Comma) if end >= at + 4 && valid_variables(&tokens[at + 3..end - 1]) => vec![end],
        _ => vec![],
    }
}

fn ends(grammar: &G, tokens: &[V], at: usize, mode: Mode) -> Vec<usize> {
    let mut out = match grammar {
        G::Token(separator) => {
            let want = match separator {
                Separator::Comma => V::Comma,
                Separator::Slash => V::Slash,
            };
            if tokens.get(at) == Some(&want) {
                vec![at + 1]
            } else {
                vec![]
            }
        }
        G::Keyword(word) => {
            let mut out = var(tokens, at);
            let found = match tokens.get(at) {
                Some(V::Ident(found)) => found.eq_ignore_ascii_case(word),
                Some(V::Num {
                    text,
                    unit: NumUnit::None,
                    ..
                }) => text == word,
                _ => false,
            };
            if found {
                out.push(at + 1);
            }
            out
        }
        G::Type(name) => {
            let mut out = var(tokens, at);
            let engine = (mode == Mode::Engine)
                .then(|| engine_leaf(name, tokens, at))
                .flatten();
            if let Some(found) = engine {
                out.extend(found);
            } else if let Some(definition) = composite(name) {
                if let Some(grammar) = parse(definition) {
                    out.extend(ends(&grammar, tokens, at, mode));
                }
            } else if tokens.get(at).is_some_and(|token| host_leaf(name, token)) {
                out.push(at + 1);
            }
            if mode == Mode::Lowered
                && let Some(end) = host_math(name, tokens, at)
            {
                out.push(end);
            }
            out
        }
        G::Function(name, inner) => {
            if !matches!(tokens.get(at), Some(V::Function(found)) if found == name
                || (mode == Mode::Engine && found.eq_ignore_ascii_case(name)))
            {
                return vec![];
            }
            ends(inner, tokens, at + 1, mode)
                .into_iter()
                .filter(|&end| tokens.get(end) == Some(&V::Close))
                .map(|end| end + 1)
                .collect()
        }
        G::Seq(items) => items.iter().fold(vec![at], |starts, item| {
            let mut next: Vec<usize> = starts
                .into_iter()
                .flat_map(|start| ends(item, tokens, start, mode))
                .collect();
            next.sort_unstable();
            next.dedup();
            next
        }),
        G::Alt(items) => items
            .iter()
            .flat_map(|item| ends(item, tokens, at, mode))
            .collect(),
        G::AnyOrder(items) => any_order(items, &mut vec![false; items.len()], tokens, at, mode),
        G::Repeat(inner, min, max, comma) => {
            let mut out = if *min == 0 { vec![at] } else { vec![] };
            let mut frontier = vec![at];
            let mut count = 0;
            while !frontier.is_empty() && count < *max {
                let mut next = Vec::new();
                for start in frontier {
                    let start = if count > 0 && *comma {
                        if tokens.get(start) != Some(&V::Comma) {
                            continue;
                        }
                        start + 1
                    } else {
                        start
                    };
                    next.extend(
                        ends(inner, tokens, start, mode)
                            .into_iter()
                            .filter(|&e| e > start),
                    );
                }
                next.sort_unstable();
                next.dedup();
                count += 1;
                if count >= *min {
                    out.extend(next.iter().copied());
                }
                frontier = next;
            }
            out
        }
    };
    out.sort_unstable();
    out.dedup();
    out
}

fn any_order(items: &[G], used: &mut [bool], tokens: &[V], at: usize, mode: Mode) -> Vec<usize> {
    let mut out = Vec::new();
    for index in 0..items.len() {
        if used[index] {
            continue;
        }
        for end in ends(&items[index], tokens, at, mode) {
            out.push(end);
            used[index] = true;
            out.extend(any_order(items, used, tokens, end, mode));
            used[index] = false;
        }
    }
    out
}

fn matches_tokens(grammar: &str, tokens: &[V], mode: Mode) -> bool {
    parse(grammar).is_some_and(|grammar| ends(&grammar, tokens, 0, mode).contains(&tokens.len()))
}

/// Whether `value` matches the value-definition `grammar` exactly, with the
/// host contract's leaf types (the registry's [`CssValue`] definitions).
#[must_use]
pub fn matches(grammar: &str, value: &str) -> bool {
    lex_value(value).is_some_and(|tokens| matches_tokens(grammar, &tokens, Mode::Host))
}

/// Whether `value` matches `grammar` with the engine's leaf types (CSS
/// Values 4 numerics and math functions, Color 4/5 colours, `<angle>`).
#[must_use]
pub fn matches_engine(grammar: &str, value: &str) -> bool {
    lex_value(value).is_some_and(|tokens| matches_tokens(grammar, &tokens, Mode::Engine))
}

/// Whether every grammar of the registry parses (checked by the unit tests).
#[must_use]
pub fn grammar_parses(grammar: &str) -> bool {
    parse(grammar).is_some()
}

/// Whether `text` contains a forbidden CSS token of the host contract's
/// raw-text check, ASCII case-insensitively.
#[must_use]
pub fn forbidden(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    FORBIDDEN_CSS.iter().any(|token| lower.contains(token))
}

/// The engine's value grammar of `property`: the registry grammar, plus the
/// values of the CSS specifications the native renderer supports.
#[must_use]
pub fn grammar(property: CssProperty) -> &'static str {
    use CssProperty as P;
    match property {
        // CSS Fonts 4 §2.2: `<font-weight-absolute>`; numbers in [1, 1000].
        P::FontWeight => "normal | bold | <number>",
        // CSS Color 4 §11.2: `<alpha-value>`.
        P::Opacity => "<number> | <percentage>",
        // CSS Text 3 §3: `pre` is the user-agent default of `pre` elements.
        P::WhiteSpace => "normal | nowrap | pre | pre-wrap",
        // CSS Text 3 §7.1: `start`/`end` (the renderer is left-to-right).
        P::TextAlign => "left | center | right | start | end",
        // CSS Transitions 1 §2.1: any property name; unknown ones are kept
        // and ignored.
        P::TransitionProperty => "none | [all | <custom-ident>]#",
        // CSS Easing 1 §2.2 and §2.3.
        P::TransitionTimingFunction => {
            "[linear | ease | ease-in | ease-out | ease-in-out | step-start | step-end | cubic-bezier(<number>, <number>, <number>, <number>) | steps(<integer> [, [jump-start | jump-end | jump-none | jump-both | start | end]]?)]#"
        }
        _ => property.grammar(),
    }
}

/// Values of the CSS specifications the native layout does not support yet
/// (CSS Display 3 §2, Positioned Layout 3 §2, Sizing 3 §3.2, Overflow 3
/// §3, Text 3 §3 and §7.1, Backgrounds 3 §4.2): refused as unrenderable, with
/// a clearer diagnostic than an unknown value.
fn unrendered(property: CssProperty) -> Option<&'static str> {
    use CssProperty as P;
    Some(match property {
        P::Display => {
            "contents | list-item | inline-block | inline-flex | inline-grid | inline-table | flow-root | table | table-row-group | table-header-group | table-footer-group | table-row | table-cell | table-column-group | table-column | table-caption | -webkit-box"
        }
        P::Position => "sticky",
        P::Width | P::Height | P::MinWidth | P::MinHeight | P::MaxWidth | P::MaxHeight => {
            "min-content | max-content | fit-content | fit-content(<length-percentage>) | stretch"
        }
        P::Overflow => "[visible | hidden | clip | scroll | auto]{1,2}",
        P::WhiteSpace => "pre-line | break-spaces",
        P::TextAlign => "justify | match-parent",
        P::BorderStyle | P::OutlineStyle => {
            "[none | hidden | dotted | dashed | solid | double | groove | ridge | inset | outset]{1,4}"
        }
        _ => return None,
    })
}

/// Why the engine refuses a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Outside the property's grammar (or the specification).
    Invalid,
    /// Valid CSS the native renderer cannot render yet (for example `ch`
    /// without font metrics, a sum of a percentage and a length, or
    /// `currentcolor` inside `color-mix()`).
    Unrenderable,
}

/// The reference metrics of [`lower`]: the root font size for `rem`, the
/// viewport for viewport-percentage units and the element's font for the
/// other font-relative units, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Context {
    pub root_font_size: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    /// The element's font metrics; `None` leaves `em` and `lh` outside
    /// `font-size` unresolved.
    pub font: Option<FontContext>,
}
impl Context {
    /// Nominal metrics for a parse-time renderability check: whether a
    /// value lowers does not depend on them.
    pub const NOMINAL: Self = Self {
        root_font_size: super::stylesheet::REM_PIXELS,
        viewport_width: 1024.0,
        viewport_height: 768.0,
        font: Some(FontContext {
            em: super::stylesheet::REM_PIXELS,
            parent_em: super::stylesheet::REM_PIXELS,
            lh: Some(super::stylesheet::REM_PIXELS),
            parent_lh: Some(super::stylesheet::REM_PIXELS),
            rlh: Some(super::stylesheet::REM_PIXELS),
        }),
    };
}

/// The font-relative references of one element (CSS Values 4 §6.1.1), in
/// CSS pixels. In `font-size` and `line-height`, `em` (in `font-size`) and
/// `lh` refer to the parent element's metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontContext {
    /// The element's computed font size.
    pub em: f64,
    /// The parent element's computed font size.
    pub parent_em: f64,
    /// The element's computed line height; `None` where it is `normal`,
    /// which needs the metrics of the first available font.
    pub lh: Option<f64>,
    pub parent_lh: Option<f64>,
    /// The root element's computed line height.
    pub rlh: Option<f64>,
}

/// Properties whose numeric slots are nonnegative (CSS Box 3, Sizing 3,
/// Flexbox 1, Fonts 4, Backgrounds 3): a math result is clamped to zero
/// (Values 4 §10.12); a negative literal is refused where the host grammar's
/// note says so.
fn nonnegative(property: CssProperty) -> bool {
    use CssProperty as P;
    matches!(
        property,
        P::Width
            | P::Height
            | P::MinWidth
            | P::MinHeight
            | P::MaxWidth
            | P::MaxHeight
            | P::Padding
            | P::PaddingTop
            | P::PaddingRight
            | P::PaddingBottom
            | P::PaddingLeft
            | P::Gap
            | P::RowGap
            | P::ColumnGap
            | P::FlexGrow
            | P::FlexShrink
            | P::FlexBasis
            | P::BorderWidth
            | P::OutlineWidth
            | P::BorderRadius
            | P::FontSize
            | P::LineHeight
            | P::TransitionDuration
    )
}
/// Properties whose numbers are integers (Values 4 §10.12 rounds math).
fn integer_property(property: CssProperty) -> bool {
    matches!(
        property,
        CssProperty::ZIndex | CssProperty::GridColumn | CssProperty::GridRow
    )
}

/// Literal restrictions the grammar does not express, over the tokens that
/// are not inside a math function.
fn literal_notes(property: CssProperty, tokens: &[V], mode: Mode) -> bool {
    let mut depth = 0usize;
    let mut math_depth = None;
    for (index, token) in tokens.iter().enumerate() {
        match token {
            V::Function(_) => {
                if math_depth.is_none() && math::is_math(Some(token)) {
                    math_depth = Some(depth);
                }
                depth += 1;
                continue;
            }
            V::Close => {
                depth = depth.saturating_sub(1);
                if math_depth == Some(depth) {
                    math_depth = None;
                }
                continue;
            }
            _ => {}
        }
        if math_depth.is_some() {
            continue;
        }
        let ok = match (property, token) {
            (
                CssProperty::Padding
                | CssProperty::PaddingTop
                | CssProperty::PaddingRight
                | CssProperty::PaddingBottom
                | CssProperty::PaddingLeft
                | CssProperty::FlexGrow
                | CssProperty::FlexShrink,
                V::Num { value, text, .. },
            ) => *value >= 0.0 && !text.starts_with('-'),
            // Color 4 §11.2: an `<alpha-value>` outside [0, 1] is valid and
            // clamped at computed-value time; the host grammar refuses it.
            (CssProperty::Opacity, V::Num { value, unit, .. }) => match (unit, mode) {
                (NumUnit::Percent | NumUnit::None, Mode::Engine) => value.is_finite(),
                (NumUnit::None, Mode::Host | Mode::Lowered) => (0.0..=1.0).contains(value),
                _ => false,
            },
            (CssProperty::FontWeight, V::Num { value, .. }) => (1.0..=1000.0).contains(value),
            (CssProperty::TransitionProperty, V::Ident(word)) => {
                (tokens.len() == 1 || !word.eq_ignore_ascii_case("none"))
                    && !CSS_WIDE_KEYWORDS
                        .iter()
                        .any(|keyword| word.eq_ignore_ascii_case(keyword))
                    && !word.eq_ignore_ascii_case("default")
            }
            (CssProperty::TransitionTimingFunction, V::Num { value, .. }) => {
                timing_argument(tokens, index, *value)
            }
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    property != CssProperty::FontFamily || !tokens.iter().any(|token| matches!(token, V::Ident(word) if CSS_WIDE_KEYWORDS.iter().any(|k| word.eq_ignore_ascii_case(k))))
}

/// CSS Easing 1: the x coordinates of `cubic-bezier()` lie in [0, 1]; a
/// `steps()` count is positive (at least 2 with `jump-none`).
fn timing_argument(tokens: &[V], index: usize, value: f64) -> bool {
    let mut open = index;
    while open > 0 && !matches!(tokens[open], V::Function(_)) {
        open -= 1;
    }
    let position = index - open;
    match &tokens[open] {
        V::Function(name) if name.eq_ignore_ascii_case("cubic-bezier") => {
            !matches!(position, 1 | 5) || (0.0..=1.0).contains(&value)
        }
        V::Function(name) if name.eq_ignore_ascii_case("steps") => {
            let jump_none = matches!(tokens.get(index + 2), Some(V::Ident(word)) if word.eq_ignore_ascii_case("jump-none"));
            position != 1 || value >= if jump_none { 2.0 } else { 1.0 }
        }
        _ => true,
    }
}

/// The engine's verdict on `value` for `property`: forbidden text and
/// structural characters, the engine [`grammar`], literal restrictions and
/// whether the native renderer can render it ([`lower`]). A value with a
/// `var()` reference defers the whole grammar to computed-value time (CSS
/// Variables 1 §3).
///
/// # Errors
/// Why the value is refused.
pub fn check(property: CssProperty, value: &str) -> Result<(), Refusal> {
    if FORBIDDEN_STYLE_VALUE.iter().any(|c| value.contains(c)) {
        return Err(Refusal::Invalid);
    }
    let tokens = lex_value(value).ok_or(Refusal::Invalid)?;
    // A reference may substitute an empty or multiple-component sequence.
    // Its presence defers the whole property grammar, not just one atom.
    if tokens.iter().any(|token| is_var(Some(token))) {
        return valid_variables(&tokens)
            .then_some(())
            .ok_or(Refusal::Invalid);
    }
    // Cascade 5 §7.3: every property takes the CSS-wide keywords as its
    // whole value. `font-family: initial` names the user agent's own initial
    // font, which no native host can supply.
    if let [V::Ident(word)] = tokens.as_slice()
        && CSS_WIDE_KEYWORDS
            .iter()
            .any(|keyword| word.eq_ignore_ascii_case(keyword))
    {
        return if property == CssProperty::FontFamily && word.eq_ignore_ascii_case("initial") {
            Err(Refusal::Invalid)
        } else {
            Ok(())
        };
    }
    if !matches_tokens(grammar(property), &tokens, Mode::Engine) {
        let specified = unrendered(property)
            .is_some_and(|values| matches_tokens(values, &tokens, Mode::Engine));
        return Err(if specified {
            Refusal::Unrenderable
        } else {
            Refusal::Invalid
        });
    }
    if !literal_notes(property, &tokens, Mode::Engine) {
        return Err(Refusal::Invalid);
    }
    lower(property, value, &Context::NOMINAL)
        .map(|_| ())
        .ok_or(Refusal::Unrenderable)
}

/// Whether `value` is a valid engine value of `property` that the native
/// renderer can render ([`check`]).
#[must_use]
pub fn property_value(property: CssProperty, value: &str) -> bool {
    check(property, value).is_ok()
}

/// Whether `value` is a valid custom-property value (CSS Variables 1 §2:
/// any declaration value, here the value tokens of the subset, possibly
/// empty), without forbidden functions or structural characters.
#[must_use]
pub fn custom_value(value: &str) -> bool {
    !FORBIDDEN_STYLE_VALUE.iter().any(|c| value.contains(c))
        && lex_value(value).is_some_and(|tokens| valid_variables(&tokens))
}

/// The stored form of a source value: the source text, or its CSS Syntax 3
/// serialization when it has comments or escapes, so that later text scans
/// (`var()` substitution) never see either.
#[must_use]
pub fn normalize(value: &str) -> String {
    let value = value.trim_matches(|c: char| syntax::whitespace(c));
    if syntax::needs_serialization(value) {
        let tokens = syntax::tokenize(value);
        let first = tokens
            .iter()
            .position(|t| t.token != syntax::Token::Whitespace);
        let last = tokens
            .iter()
            .rposition(|t| t.token != syntax::Token::Whitespace);
        match (first, last) {
            (Some(first), Some(last)) => syntax::serialize(&tokens[first..=last]),
            _ => String::new(),
        }
    } else {
        value.to_owned()
    }
}

/// Serialize lowered tokens for the renderer's consumers.
fn join(parts: &[String]) -> String {
    let mut out = String::new();
    for (index, part) in parts.iter().enumerate() {
        let previous = index.checked_sub(1).map(|i| parts[i].as_str());
        let glue = !(previous.is_none_or(|p| p.ends_with('(')) || part == ")" || part == ",");
        if glue {
            out.push(' ');
        }
        out.push_str(part);
    }
    out
}

fn clamp_magnitude(value: f64) -> f64 {
    value.clamp(-MAX_LOWERED_MAGNITUDE, MAX_LOWERED_MAGNITUDE)
}

/// Lower one math function to a host-grammar component.
fn lower_math(
    property: CssProperty,
    tokens: &[V],
    at: usize,
    context: &Context,
) -> Option<(String, usize)> {
    use math::{Base, Type};
    let (expression, end) = math::parse(tokens, at)?;
    let font_size = property == CssProperty::FontSize;
    // Values 4 §6.1.1: in `font-size` and `line-height`, `lh` is the
    // parent's line height; in `font-size`, `em` is the parent's size.
    let parent_metrics = matches!(property, CssProperty::FontSize | CssProperty::LineHeight);
    let font = context.font;
    let resolution = math::Resolution {
        em: font.map(|font| if font_size { font.parent_em } else { font.em }),
        rem: context.root_font_size,
        lh: font.and_then(|font| {
            if parent_metrics {
                font.parent_lh
            } else {
                font.lh
            }
        }),
        rlh: font.and_then(|font| font.rlh),
        viewport_width: context.viewport_width,
        viewport_height: context.viewport_height,
    };
    let percent_as = Some(Base::Length);
    let kind = expression
        .resolved_type(None)
        .or_else(|| expression.resolved_type(percent_as))?;
    let mut evaluated = if font_size {
        // A sum of `em` and other lengths is an absolute size of the
        // parent's; `em` alone keeps the relative form (a percentage).
        match expression.evaluate_font_size(&resolution)? {
            mixed if mixed.mixed => expression.evaluate(&resolution)?,
            relative => relative,
        }
    } else {
        expression.evaluate(&resolution)?
    };
    if evaluated.mixed {
        // Values 4 §10.10: a sum of lengths and percentages resolves at
        // layout; its lengths become `px` now.
        if !layout_math_property(property) {
            return None;
        }
        let parts = tokens[at..end]
            .iter()
            .map(|token| match token {
                V::Num {
                    value,
                    unit: NumUnit::Dimension(name),
                    ..
                } => match math::length_unit(name) {
                    Some(unit) => absolute_length(*value, unit, context),
                    None => Some(token_text(token)),
                },
                token => Some(token_text(token)),
            })
            .collect::<Option<Vec<_>>>()?;
        let text = join(&parts);
        // §10.12: a nonnegative property clamps the used value at zero.
        let text = if nonnegative(property) {
            format!("max(0px, {text})")
        } else {
            text
        };
        return Some((text, end));
    }
    if nonnegative(property) {
        evaluated.value = evaluated.value.max(0.0);
    }
    let value = clamp_magnitude(evaluated.value);
    let text = if kind == Type::NUMBER {
        let value = if integer_property(property) {
            (value + 0.5).libm_floor()
        } else if property == CssProperty::Opacity {
            value.clamp(0.0, 1.0)
        } else if property == CssProperty::FontWeight {
            // Fonts 4 §2.2: a math result is clamped to [1, 1000].
            value.clamp(1.0, 1000.0)
        } else {
            value
        };
        format_number(value)
    } else if kind == Type::of(Base::Percent) {
        if property == CssProperty::Opacity {
            format_number((value / 100.0).clamp(0.0, 1.0))
        } else {
            format!("{}%", format_number(value))
        }
    } else if kind == Type::of(Base::Length) {
        if evaluated.percent {
            format!("{}%", format_number(value))
        } else {
            format!("{}px", format_number(value))
        }
    } else if kind == Type::of(Base::Time) {
        format!("{}ms", format_number(value))
    } else if kind == Type::of(Base::Angle) {
        format!("{}deg", format_number(value))
    } else {
        return None;
    };
    Some((text, end))
}

/// The computed value of a registered custom property's value whose
/// syntax has length components (Properties and Values API 1 §2.4): every
/// length becomes an absolute length in `px` (`em`, `rem`, `lh`, viewport
/// and absolute units, and math functions over lengths alone); a math
/// function that sums lengths with percentages stays as written, and every
/// other component is kept. `None` where a length cannot be resolved.
#[must_use]
pub fn absolutize_lengths(value: &str, context: &Context) -> Option<String> {
    let tokens = lex_value(value)?;
    let mut parts = Vec::with_capacity(tokens.len());
    let mut at = 0;
    while let Some(token) = tokens.get(at) {
        if math::is_math(Some(token)) {
            let (expression, end) = math::parse(&tokens, at)?;
            let font = context.font;
            let resolution = math::Resolution {
                em: font.map(|font| font.em),
                rem: context.root_font_size,
                lh: font.and_then(|font| font.lh),
                rlh: font.and_then(|font| font.rlh),
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
            };
            let length = expression.resolved_type(Some(math::Base::Length))
                == Some(math::Type::of(math::Base::Length));
            match expression.evaluate(&resolution) {
                Some(evaluated) if length && !evaluated.percent => {
                    parts.push(format!(
                        "{}px",
                        format_number(clamp_magnitude(evaluated.value))
                    ));
                }
                _ => parts.extend(tokens[at..end].iter().map(token_text)),
            }
            at = end;
            continue;
        }
        let lowered = match token {
            V::Num {
                value,
                unit: NumUnit::Dimension(name),
                ..
            } => match math::length_unit(name) {
                Some(unit) => Some(absolute_length(*value, unit, context)?),
                None => None,
            },
            _ => None,
        };
        parts.push(lowered.unwrap_or_else(|| token_text(token)));
        at += 1;
    }
    Some(join(&parts))
}

/// One value token as the renderer's consumers read it (numbers as written).
fn token_text(token: &V) -> String {
    match token {
        V::Ident(word) => word.to_ascii_lowercase(),
        V::Num { text, .. } => text.clone(),
        V::Function(name) => format!("{}(", name.to_ascii_lowercase()),
        V::Close => ")".into(),
        V::Comma => ",".into(),
        V::Slash => "/".into(),
        V::Hash(hex) => format!("#{hex}"),
        V::Str(text) => {
            let mut out = String::new();
            syntax::serialize_string(text, &mut out);
            out
        }
        V::Delim(c) => c.to_string(),
    }
}

/// One length in CSS pixels, outside `font-size` and `line-height`.
fn absolute_length(value: f64, unit: math::LengthUnit, context: &Context) -> Option<String> {
    use math::LengthUnit as L;
    let vw = context.viewport_width / 100.0;
    let vh = context.viewport_height / 100.0;
    let factor = match unit {
        L::Absolute(factor) => factor,
        L::Em => context.font?.em,
        L::Rem => context.root_font_size,
        L::Lh => context.font?.lh?,
        L::Rlh => context.font?.rlh?,
        L::ViewportWidth => vw,
        L::ViewportHeight => vh,
        L::ViewportMin => vw.min(vh),
        L::ViewportMax => vw.max(vh),
        L::Unresolved => return None,
    };
    font_length(value, factor)
}

/// A font-relative length in CSS pixels.
fn font_length(value: f64, reference: f64) -> Option<String> {
    Some(format!(
        "{}px",
        format_number(clamp_magnitude(value * reference))
    ))
}

/// Lower one numeric token whose unit the host grammar lacks.
fn lower_numeric(property: CssProperty, token: &V, context: &Context) -> Option<String> {
    let V::Num {
        text, value, unit, ..
    } = token
    else {
        return None;
    };
    match unit {
        NumUnit::Percent if property == CssProperty::Opacity => {
            Some(format_number((value / 100.0).clamp(0.0, 1.0)))
        }
        NumUnit::None if property == CssProperty::Opacity => {
            Some(format_number(value.clamp(0.0, 1.0)))
        }
        NumUnit::Dimension(name) => {
            let lower = name.to_ascii_lowercase();
            if matches!(lower.as_str(), "px" | "rem" | "fr" | "ms" | "s") {
                let repr = &text[..text.len() - name.len()];
                return Some(format!("{repr}{lower}"));
            }
            let parent_metrics =
                matches!(property, CssProperty::FontSize | CssProperty::LineHeight);
            match math::length_unit(&lower)? {
                math::LengthUnit::Em if property == CssProperty::FontSize => {
                    Some(format!("{}%", format_number(value * 100.0)))
                }
                math::LengthUnit::Em => font_length(*value, context.font?.em),
                math::LengthUnit::Lh => {
                    let font = context.font?;
                    font_length(
                        *value,
                        if parent_metrics {
                            font.parent_lh?
                        } else {
                            font.lh?
                        },
                    )
                }
                math::LengthUnit::Rlh => font_length(*value, context.font?.rlh?),
                math::LengthUnit::Unresolved => None,
                math::LengthUnit::Rem => Some(format!("{}rem", format_number(*value))),
                unit => {
                    let vw = context.viewport_width / 100.0;
                    let vh = context.viewport_height / 100.0;
                    let factor = match unit {
                        math::LengthUnit::Absolute(factor) => factor,
                        math::LengthUnit::ViewportWidth => vw,
                        math::LengthUnit::ViewportHeight => vh,
                        math::LengthUnit::ViewportMin => vw.min(vh),
                        _ => vw.max(vh),
                    };
                    Some(format!(
                        "{}px",
                        format_number(clamp_magnitude(value * factor))
                    ))
                }
            }
        }
        _ => Some(text.clone()),
    }
}

/// Lower a valid engine value of `property` (after `var()` substitution) to
/// the host grammar the native renderer's consumers read: math functions
/// are evaluated (CSS Values 4 §10.10) with lengths in `px` and the results
/// range-checked (§10.12), absolute and viewport units become `px`, `em`
/// in `font-size` a percentage, colours `rgba()` after gamut mapping, and
/// `opacity` percentages numbers. `None` when the native renderer cannot
/// render the value. A value already in the host grammar is returned as is.
#[must_use]
pub fn lower(property: CssProperty, value: &str, context: &Context) -> Option<String> {
    let tokens = lex_value(value)?;
    let target = grammar(property);
    if matches_tokens(target, &tokens, Mode::Host) && literal_notes(property, &tokens, Mode::Host) {
        return Some(value.to_owned());
    }
    let mut parts = Vec::with_capacity(tokens.len());
    let mut at = 0;
    while let Some(token) = tokens.get(at) {
        if math::is_math(Some(token)) {
            let (text, end) = lower_math(property, &tokens, at, context)?;
            parts.push(text);
            at = end;
            continue;
        }
        let colour = match token {
            V::Hash(hex) => !HEX_COLOR_DIGITS.contains(&hex.len()),
            // Color 4 §6.1 named colours, where the property takes a colour.
            V::Ident(name) => grammar(property).contains("<color>") && color::named(name).is_some(),
            V::Function(name) => [
                "rgb",
                "rgba",
                "hsl",
                "hsla",
                "hwb",
                "lab",
                "lch",
                "oklab",
                "oklch",
                "color-mix",
            ]
            .iter()
            .any(|f| name.eq_ignore_ascii_case(f)),
            _ => false,
        };
        if colour {
            let (parsed, end) = color::parse(&tokens, at)?;
            let color::Parsed::Color(color) = parsed else {
                return None;
            };
            parts.push(color::to_legacy(color));
            at = end;
            continue;
        }
        parts.push(match token {
            V::Num { .. } => lower_numeric(property, token, context)?,
            token => token_text(token),
        });
        at += 1;
    }
    let lowered = join(&parts);
    let tokens = lex_value(&lowered)?;
    (matches_tokens(target, &tokens, Mode::Lowered)
        && literal_notes(property, &tokens, Mode::Lowered))
    .then_some(lowered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_from_guest_memory_never_panic_and_fail_closed() {
        for value in ["\"open", "#", "!", "a +", "", "var(--)", "var(--a,)", "（"] {
            let _ = matches("<string> | <ident>", value);
        }
        assert!(!matches("<length>+", &"1px ".repeat(MAX_VALUE_TOKENS + 1)));
        assert!(matches("<length>+", &"1px ".repeat(MAX_VALUE_TOKENS)));
        assert!(!property_value(CssProperty::Padding, "-1px"));
        assert!(property_value(CssProperty::Padding, "var(--gap)"));
        assert!(property_value(CssProperty::Display, "var(--mode)"));
        assert!(property_value(CssProperty::Display, "var(--mode, none)"));
        assert!(property_value(CssProperty::Display, "var(--mode,)"));
        assert!(property_value(CssProperty::Opacity, "1.5"));
        assert_eq!(
            lower(CssProperty::Opacity, "1.5", &Context::NOMINAL).as_deref(),
            Some("1")
        );
        assert_eq!(
            lower(CssProperty::Opacity, "-20%", &Context::NOMINAL).as_deref(),
            Some("0")
        );
        assert_eq!(
            lower(CssProperty::Color, "Red", &Context::NOMINAL).as_deref(),
            Some("rgba(255, 0, 0, 1)")
        );
        assert_eq!(
            lower(CssProperty::Border, "1px solid tomato", &Context::NOMINAL).as_deref(),
            Some("1px solid rgba(255, 99, 71, 1)")
        );
        assert_eq!(
            lower(CssProperty::FontWeight, "calc(2000)", &Context::NOMINAL).as_deref(),
            Some("1000")
        );
        assert!(property_value(CssProperty::Opacity, "0.5"));
        assert!(!property_value(CssProperty::Color, "#fff;"));
        assert!(!property_value(CssProperty::Color, "URL(x)"));
        assert!(!property_value(CssProperty::Color, "\\75 rl(x)"));
        assert!(!property_value(CssProperty::Color, "#fff !important"));
        // Cascade 5 §7.3: a CSS-wide keyword is a whole value; inside a
        // family list it is invalid, and `initial` names the user agent's
        // own font, which no native host can supply.
        for value in ["INITIAL", "\"A\", unset", "revert-layer, serif"] {
            assert!(!property_value(CssProperty::FontFamily, value), "{value}");
            assert!(font_family_css_wide_keyword(value).is_some(), "{value}");
        }
        for value in ["inherit", "unset", "revert", "Revert-Layer"] {
            assert!(property_value(CssProperty::FontFamily, value), "{value}");
        }
        // A quoted name is a family; a keyword inside a longer name is not one.
        for value in ["\"inherit\"", "inheritance, serif", "monospace"] {
            assert!(property_value(CssProperty::FontFamily, value), "{value}");
            assert_eq!(font_family_css_wide_keyword(value), None, "{value}");
        }
        assert!(custom_value("#0a0a0a"));
        assert!(custom_value(""));
        assert!(!custom_value("a; b"));
        assert!(custom_value("calc(var(--spacing) * 4)"));
    }

    #[test]
    fn class_names_follow_the_html_class_model() {
        for name in [
            "sm:flex",
            "w-1/2",
            "p-0.5",
            "[mask:url(x)]",
            "!",
            "é",
            "2xl:x",
        ] {
            assert!(class_name(name), "{name}");
        }
        for name in ["", "a b", "a\u{7}", &"x".repeat(MAX_CLASS_NAME_BYTES + 1)] {
            assert!(!class_name(name), "{name:?}");
        }
        assert!(class_list(" sm:flex  hover:bg-x/50 "));
    }

    #[test]
    fn engine_values_follow_values_4_and_color_4() {
        use CssProperty as P;
        for (property, value) in [
            (P::Width, "calc(var(--spacing) * 4)"),
            (P::Width, "calc(0.25rem * 4)"),
            (P::Width, "50vw"),
            (P::Width, "calc(1/2 * 100%)"),
            (P::Width, "1in"),
            (P::Padding, "calc(1px - 4px) 2PX"),
            (P::MarginTop, "calc(1px * -1)"),
            (P::LineHeight, "calc(1.25 / 0.875)"),
            (P::FontSize, "1.5em"),
            (P::FontSize, "calc(2em * 0.5)"),
            (P::Color, "oklch(51.1% 0.262 276.966)"),
            (P::Color, "color-mix(in oklab, #000 50%, transparent)"),
            (P::BackgroundColor, "rgb(0 0 0 / 0.5)"),
            (P::BorderColor, "#0f08"),
            (P::Opacity, "50%"),
            (P::FontWeight, "350"),
            (P::ZIndex, "calc(1.5)"),
            (P::BorderRadius, "calc(infinity * 1px)"),
            (P::TransitionTimingFunction, "cubic-bezier(0.4, 0, 0.2, 1)"),
            (P::TransitionTimingFunction, "steps(4, jump-end)"),
            (
                P::TransitionProperty,
                "color, background-color, --tw-gradient-from, fill",
            ),
            (P::TransitionDuration, "calc(0.1s * 2)"),
            (P::WhiteSpace, "pre"),
            (P::TextAlign, "start"),
            // Values 4 §6.1.1: `em` and `lh` resolve from the element's font.
            (P::Width, "2em"),
            (P::Padding, "0.5em 1em"),
            (P::Height, "calc(2lh + 1em)"),
        ] {
            assert_eq!(
                check(property, value),
                Ok(()),
                "{}: {value}",
                property.name()
            );
        }
        for (property, value, refusal) in [
            (P::Width, "calc(1px + 1)", Refusal::Invalid),
            (P::BorderRadius, "calc(100% - 1rem)", Refusal::Unrenderable),
            (P::Width, "2ch", Refusal::Unrenderable),
            (P::Width, "round(1px, 2px)", Refusal::Invalid),
            (
                P::Color,
                "color-mix(in oklab, currentcolor 50%, transparent)",
                Refusal::Unrenderable,
            ),
            (P::Color, "notacolor", Refusal::Invalid),
            (P::Opacity, "150%x", Refusal::Invalid),
            (P::FontWeight, "1001", Refusal::Invalid),
            (P::TransitionProperty, "a, none", Refusal::Invalid),
            (
                P::TransitionTimingFunction,
                "cubic-bezier(2, 0, 0, 1)",
                Refusal::Invalid,
            ),
            (
                P::TransitionTimingFunction,
                "steps(1, jump-none)",
                Refusal::Invalid,
            ),
            (P::Display, "inline-block", Refusal::Unrenderable),
            (P::Display, "inline-blocks", Refusal::Invalid),
            (P::Position, "sticky", Refusal::Unrenderable),
            (P::Width, "fit-content", Refusal::Unrenderable),
        ] {
            assert_eq!(
                check(property, value),
                Err(refusal),
                "{}: {value}",
                property.name()
            );
        }
    }

    #[test]
    fn lowering_produces_the_host_grammar() {
        use CssProperty as P;
        let context = Context::NOMINAL;
        let lower = |property, value| lower(property, value, &context);
        assert_eq!(lower(P::Width, "12px").as_deref(), Some("12px"));
        assert_eq!(
            lower(P::Width, "calc(0.25rem * 4)").as_deref(),
            Some("16px")
        );
        assert_eq!(lower(P::Width, "calc(1/2 * 100%)").as_deref(), Some("50%"));
        assert_eq!(lower(P::Width, "10vw").as_deref(), Some("102.4px"));
        assert_eq!(lower(P::Height, "100dvh").as_deref(), Some("768px"));
        assert_eq!(
            lower(P::Padding, "calc(1px - 4px) 2PX").as_deref(),
            Some("0px 2px")
        );
        assert_eq!(
            lower(P::MarginTop, "calc(4px * -1)").as_deref(),
            Some("-4px")
        );
        assert_eq!(lower(P::FontSize, "1.5em").as_deref(), Some("150%"));
        assert_eq!(lower(P::Opacity, "50%").as_deref(), Some("0.5"));
        assert_eq!(lower(P::ZIndex, "calc(1.5)").as_deref(), Some("2"));
        assert_eq!(
            lower(P::BorderRadius, "calc(infinity * 1px)").as_deref(),
            Some("1000000px")
        );
        assert_eq!(
            lower(P::Color, "color-mix(in oklab, #ff0000 50%, transparent)").as_deref(),
            Some("rgba(255, 0, 0, 0.5)")
        );
        assert_eq!(
            lower(P::Border, "1px solid oklch(100% 0 0)").as_deref(),
            Some("1px solid rgba(255, 255, 255, 1)")
        );
        assert_eq!(
            lower(P::TransitionDuration, "calc(0.1s * 2)").as_deref(),
            Some("200ms")
        );
    }

    #[test]
    fn host_grammar_keeps_the_registry_semantics() {
        assert!(!matches("<length>", "1em"));
        assert!(!matches("<length>", "calc(1px)"));
        assert!(!matches("<color>", "oklch(0.5 0.1 10)"));
        assert!(!matches("<length>", "1PX"));
        assert!(!matches("<number>", "1e3"));
        assert!(matches("<color>", "rgba(0, 0, 0, 0.4)"));
        assert!(matches_engine("<length>", "1PX"));
        assert!(matches_engine("<number>", "1e3"));
        assert!(matches_engine("<angle>", "0.5turn"));
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;

    #[test]
    fn font_relative_units_lower_from_the_font_context() {
        let context = Context {
            font: Some(FontContext {
                em: 20.0,
                parent_em: 10.0,
                lh: Some(30.0),
                parent_lh: Some(12.0),
                rlh: None,
            }),
            ..Context::NOMINAL
        };
        let lower = |property, value| lower(property, value, &context);
        assert_eq!(lower(CssProperty::Width, "2em").as_deref(), Some("40px"));
        assert_eq!(lower(CssProperty::Width, "1lh").as_deref(), Some("30px"));
        // In `font-size` `em` is the parent's size; alone it stays relative.
        assert_eq!(
            lower(CssProperty::FontSize, "1.5em").as_deref(),
            Some("150%")
        );
        assert_eq!(
            lower(CssProperty::FontSize, "calc(1em + 2px)").as_deref(),
            Some("12px")
        );
        // In `line-height`, `lh` is the parent's; `em` the element's own.
        assert_eq!(
            lower(CssProperty::LineHeight, "2lh").as_deref(),
            Some("24px")
        );
        assert_eq!(
            lower(CssProperty::LineHeight, "calc(1em)").as_deref(),
            Some("20px")
        );
        // Without the root's line height or a font context, nothing lowers.
        assert_eq!(lower(CssProperty::Width, "1rlh"), None);
        let bare = Context {
            font: None,
            ..Context::NOMINAL
        };
        assert_eq!(super::lower(CssProperty::Width, "2em", &bare), None);
    }
}

#[cfg(test)]
mod layout_math_tests {
    use super::*;

    #[test]
    fn sums_of_lengths_and_percentages_lower_to_layout_time_math() {
        let lower = |property, value| lower(property, value, &Context::NOMINAL);
        assert_eq!(
            lower(CssProperty::Width, "calc(50% - 1rem)").as_deref(),
            Some("max(0px, calc(50% - 16px))")
        );
        assert_eq!(
            lower(CssProperty::MarginLeft, "calc(10% + 0.5em)").as_deref(),
            Some("calc(10% + 8px)")
        );
        assert_eq!(
            lower(CssProperty::Padding, "1px calc(5% + 1px)").as_deref(),
            Some("1px max(0px, calc(5% + 1px))")
        );
        for (property, value) in [
            (CssProperty::Width, "calc(50% - 20px)"),
            (CssProperty::Height, "calc(100% - 2rem)"),
            (CssProperty::MarginLeft, "calc(10% + 5px)"),
            (CssProperty::PaddingTop, "calc(5% + 1px)"),
            (CssProperty::MinWidth, "max(0px, calc(10% - 1000px))"),
        ] {
            assert_eq!(check(property, value), Ok(()), "{value}");
        }
        assert_eq!(
            check(CssProperty::BorderRadius, "calc(50% - 1px)"),
            Err(Refusal::Unrenderable)
        );
        assert!(layout_math("max(0px, calc(50% - 16px))").is_some());
        assert!(layout_math("calc(50% - 1em)").is_none());
    }
}
