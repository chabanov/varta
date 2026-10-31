//! A checked matcher for the CSS value grammars of [`super::CssProperty`]
//! (draft v0). Appliers and the headless renderer validate every `set_style`
//! value and every stylesheet declaration with it before any effect, so one
//! source decides which values are in the subset.
//!
//! Grammars are the static value definitions of [`super::CssProperty::grammar`]
//! (`a | b` choice, `a{1,4}` repetition, `a || b` any order, `a#` comma list,
//! `[ … ]` grouping, `name( … )` functions) over the [`super::CssValue`] types.
//! Values come from guest memory: the lexer rejects any character outside the
//! value syntax, and a value longer than [`MAX_VALUE_TOKENS`] tokens fails
//! closed, which bounds the matcher's work.
use super::{CssProperty, CssValue, FORBIDDEN_CSS, FORBIDDEN_STYLE_VALUE};

/// Tokens of one property value (placeholder budget, like the `DRAFT_*`
/// limits): every subset grammar needs far fewer.
pub const MAX_VALUE_TOKENS: usize = 128;
/// Hexadecimal digits a `<hex>` colour admits: `#rgb`, `#rrggbb`, `#rrggbbaa`.
pub const HEX_COLOR_DIGITS: [usize; 3] = [3, 6, 8];

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

/// One token of a value.
#[derive(Debug, Clone, PartialEq)]
enum V {
    Word(String),
    Function(String),
    Close,
    Comma,
    Slash,
    Hash(String),
    Str,
}

/// Lex a value; `None` for any character outside the value syntax or an
/// unterminated string.
fn lex_value(text: &str) -> Option<Vec<V>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if out.len() > MAX_VALUE_TOKENS {
            return None;
        }
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == ',' {
            out.push(V::Comma);
            i += 1;
        } else if c == '/' {
            out.push(V::Slash);
            i += 1;
        } else if c == ')' {
            out.push(V::Close);
            i += 1;
        } else if c == '"' {
            let end = i + 1 + chars[i + 1..].iter().position(|&d| d == '"')?;
            out.push(V::Str);
            i = end + 1;
        } else if c == '#' {
            let start = i + 1;
            i = start;
            while i < chars.len() && chars[i].is_ascii_hexdigit() {
                i += 1;
            }
            out.push(V::Hash(chars[start..i].iter().collect()));
        } else {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || "-_.%".contains(chars[i])) {
                i += 1;
            }
            if i == start {
                return None;
            }
            let word: String = chars[start..i].iter().collect();
            if chars.get(i) == Some(&'(') {
                out.push(V::Function(word));
                i += 1;
            } else {
                out.push(V::Word(word));
            }
        }
    }
    (out.len() <= MAX_VALUE_TOKENS).then_some(out)
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
fn composite(name: &str) -> Option<&'static str> {
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

fn leaf(name: &str, token: &V) -> bool {
    let V::Word(word) = token else {
        return match (name, token) {
            ("<hex>", V::Hash(hex)) => HEX_COLOR_DIGITS.contains(&hex.len()),
            ("<string>", V::Str) => true,
            _ => false,
        };
    };
    match name {
        "<number>" => number(word),
        "<integer>" => {
            let digits = word.strip_prefix('-').unwrap_or(word);
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
        }
        "<length>" => word == "0" || unit(word, &["px", "rem"]),
        "<percentage>" => unit(word, &["%"]),
        "<flex>" => unit(word, &["fr"]),
        "<time>" => unit(word, &["ms", "s"]),
        "<ident>" => ident(word),
        _ => false,
    }
}

/// `[A-Za-z_][A-Za-z0-9_-]*`
#[must_use]
pub fn ident(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
}

/// `var(--name)` or `var(--name, <one token>)` stands for any whole component.
fn var(tokens: &[V], at: usize) -> Vec<usize> {
    if tokens.get(at) != Some(&V::Function("var".into())) {
        return vec![];
    }
    let Some(V::Word(name)) = tokens.get(at + 1) else {
        return vec![];
    };
    if !name.strip_prefix("--").is_some_and(ident) {
        return vec![];
    }
    match tokens.get(at + 2) {
        Some(V::Close) => vec![at + 3],
        Some(V::Comma)
            if tokens.get(at + 4) == Some(&V::Close)
                && !matches!(tokens.get(at + 3), Some(V::Close | V::Comma)) =>
        {
            vec![at + 5]
        }
        _ => vec![],
    }
}

fn ends(grammar: &G, tokens: &[V], at: usize) -> Vec<usize> {
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
        G::Keyword(word) => match tokens.get(at) {
            Some(V::Word(found)) if found.eq_ignore_ascii_case(word) => vec![at + 1],
            _ => vec![],
        },
        G::Type(name) => {
            let mut out = var(tokens, at);
            if let Some(definition) = composite(name) {
                if let Some(grammar) = parse(definition) {
                    out.extend(ends(&grammar, tokens, at));
                }
            } else if tokens.get(at).is_some_and(|token| leaf(name, token)) {
                out.push(at + 1);
            }
            out
        }
        G::Function(name, inner) => {
            if tokens.get(at) != Some(&V::Function(name.clone())) {
                return vec![];
            }
            ends(inner, tokens, at + 1)
                .into_iter()
                .filter(|&end| tokens.get(end) == Some(&V::Close))
                .map(|end| end + 1)
                .collect()
        }
        G::Seq(items) => items.iter().fold(vec![at], |starts, item| {
            let mut next: Vec<usize> = starts
                .into_iter()
                .flat_map(|start| ends(item, tokens, start))
                .collect();
            next.sort_unstable();
            next.dedup();
            next
        }),
        G::Alt(items) => items
            .iter()
            .flat_map(|item| ends(item, tokens, at))
            .collect(),
        G::AnyOrder(items) => any_order(items, &mut vec![false; items.len()], tokens, at),
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
                        ends(inner, tokens, start)
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

fn any_order(items: &[G], used: &mut [bool], tokens: &[V], at: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for index in 0..items.len() {
        if used[index] {
            continue;
        }
        for end in ends(&items[index], tokens, at) {
            out.push(end);
            used[index] = true;
            out.extend(any_order(items, used, tokens, end));
            used[index] = false;
        }
    }
    out
}

/// Whether `value` matches the value-definition `grammar` exactly.
#[must_use]
pub fn matches(grammar: &str, value: &str) -> bool {
    let (Some(grammar), Some(tokens)) = (parse(grammar), lex_value(value)) else {
        return false;
    };
    ends(&grammar, &tokens, 0).contains(&tokens.len())
}

/// Whether every grammar of the registry parses (checked by the unit tests).
#[must_use]
pub fn grammar_parses(grammar: &str) -> bool {
    parse(grammar).is_some()
}

/// Whether `text` contains a forbidden CSS token, ASCII case-insensitively.
#[must_use]
pub fn forbidden(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    FORBIDDEN_CSS.iter().any(|token| lower.contains(token))
}

/// Whether `value` is a valid value of `property`: no forbidden token or
/// structural character, the property grammar, and the restriction of its
/// note (nonnegative values; opacity between 0 and 1).
#[must_use]
pub fn property_value(property: CssProperty, value: &str) -> bool {
    if forbidden(value) || FORBIDDEN_STYLE_VALUE.iter().any(|c| value.contains(c)) {
        return false;
    }
    if !matches(property.grammar(), value) {
        return false;
    }
    let Some(tokens) = lex_value(value) else {
        return false;
    };
    let words = || {
        tokens.iter().filter_map(|token| match token {
            V::Word(word) if !word.starts_with("--") => Some(word.as_str()),
            _ => None,
        })
    };
    match property {
        CssProperty::Padding
        | CssProperty::PaddingTop
        | CssProperty::PaddingRight
        | CssProperty::PaddingBottom
        | CssProperty::PaddingLeft
        | CssProperty::FlexGrow
        | CssProperty::FlexShrink => words().all(|word| !word.starts_with('-')),
        CssProperty::Opacity => {
            words().all(|word| word.parse::<f64>().is_ok_and(|n| (0.0..=1.0).contains(&n)))
        }
        _ => true,
    }
}

/// Whether `value` is a valid custom-property value: one or more lexable
/// value tokens, without forbidden text or structural characters.
#[must_use]
pub fn custom_value(value: &str) -> bool {
    !forbidden(value)
        && !FORBIDDEN_STYLE_VALUE.iter().any(|c| value.contains(c))
        && lex_value(value).is_some_and(|tokens| !tokens.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_from_guest_memory_never_panic_and_fail_closed() {
        for value in ["\"open", "#", "!", "a +", "", "var(--)", "var(--a,)", "（"] {
            let _ = matches("<string> | <ident>", value);
        }
        assert!(!matches("<string>", "\"open"));
        let long = "1px ".repeat(MAX_VALUE_TOKENS + 1);
        assert!(!matches("<length>+", &long));
        assert!(matches("<length>+", &"1px ".repeat(MAX_VALUE_TOKENS)));
        assert!(!property_value(CssProperty::Padding, "-1px"));
        assert!(property_value(CssProperty::Padding, "var(--gap)"));
        assert!(!property_value(CssProperty::Opacity, "1.5"));
        assert!(property_value(CssProperty::Opacity, "0.5"));
        assert!(!property_value(CssProperty::Color, "#fff;"));
        assert!(!property_value(CssProperty::Color, "URL(x)"));
        assert!(custom_value("#0a0a0a"));
        assert!(!custom_value("a; b"));
        assert!(!custom_value(" "));
    }
}
