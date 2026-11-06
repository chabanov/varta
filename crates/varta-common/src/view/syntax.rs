//! CSS Syntax Module Level 3 tokenization (§4) for the engine's stylesheet
//! and value grammars, shared by the native renderer, the compiler's static
//! checks and (later) the browser host's Wasm shim.
//!
//! The tokenizer follows the specification's algorithm exactly: input
//! preprocessing (§3.3: CR, CR LF and FF are newlines, NUL is U+FFFD),
//! comments (§4.3.2), escapes (§4.3.7, decoded before any later check),
//! strings, hashes, numbers with exponents (§4.3.12), dimensions,
//! percentages, `url(` (§4.3.6) and every punctuation token. It never fails:
//! a parse error of the specification becomes a `BadString`/`BadUrl` token
//! or a `Delim`, which the checked grammars then refuse. Its work is linear
//! in the input, which callers bound and charge.
#[allow(unused_imports)]
use crate::view::prelude::*;

/// One token of §4 with the byte span of its source text.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub token: Token,
    /// Zero-based UTF-8 byte offset of the token's first code point.
    pub start: usize,
    /// Byte offset just past the token's last code point.
    pub end: usize,
}

/// A numeric value (§4.2: `type` flag, numeric value and representation).
#[derive(Debug, Clone, PartialEq)]
pub struct Numeric {
    pub value: f64,
    /// The specification's type flag: "integer" (no `.` and no exponent).
    pub integer: bool,
    /// The number's source representation, sign included.
    pub repr: String,
}

/// The tokens of CSS Syntax 3 §4.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Function(String),
    AtKeyword(String),
    Hash { value: String, id: bool },
    String(String),
    BadString,
    Url(String),
    BadUrl,
    Delim(char),
    Number(Numeric),
    Percentage(Numeric),
    Dimension(Numeric, String),
    Whitespace,
    Cdo,
    Cdc,
    Colon,
    Semicolon,
    Comma,
    OpenSquare,
    CloseSquare,
    OpenParen,
    CloseParen,
    OpenCurly,
    CloseCurly,
}

/// The maximum code point (§4.2).
const MAX_CODE_POINT: u32 = 0x10FFFF;
/// Hexadecimal digits of one escape (§4.3.7).
const MAX_ESCAPE_DIGITS: usize = 6;

/// Preprocessed input (§3.3) with the source byte offset of every code
/// point, plus the end offset.
struct Input {
    chars: Vec<char>,
    offsets: Vec<usize>,
}
impl Input {
    fn new(text: &str) -> Self {
        let mut chars = Vec::with_capacity(text.len());
        let mut offsets = Vec::with_capacity(text.len() + 1);
        let mut iter = text.char_indices().peekable();
        while let Some((offset, c)) = iter.next() {
            offsets.push(offset);
            match c {
                '\r' => {
                    if iter.peek().is_some_and(|(_, next)| *next == '\n') {
                        iter.next();
                    }
                    chars.push('\n');
                }
                '\x0C' => chars.push('\n'),
                '\0' => chars.push('\u{FFFD}'),
                _ => chars.push(c),
            }
        }
        offsets.push(text.len());
        Self { chars, offsets }
    }
}

/// Newline (§4.2, after preprocessing).
fn newline(c: char) -> bool {
    c == '\n'
}
/// Whitespace (§4.2): newline, tab or space.
#[must_use]
pub fn whitespace(c: char) -> bool {
    matches!(c, '\n' | '\t' | ' ')
}
/// Ident-start code point (§4.2): a letter, `_` or a non-ASCII code point.
#[must_use]
pub fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}
/// Ident code point (§4.2): an ident-start code point, a digit or `-`.
#[must_use]
pub fn ident_char(c: char) -> bool {
    ident_start(c) || c.is_ascii_digit() || c == '-'
}

struct Tokenizer {
    input: Input,
    at: usize,
}

impl Tokenizer {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.input.chars.get(self.at + ahead).copied()
    }
    /// §4.3.8: whether two code points are a valid escape.
    fn valid_escape(first: Option<char>, second: Option<char>) -> bool {
        first == Some('\\') && second.is_some_and(|c| !newline(c))
    }
    /// §4.3.9: whether three code points would start an ident sequence.
    fn starts_ident(first: Option<char>, second: Option<char>, third: Option<char>) -> bool {
        match first {
            Some('-') => {
                second.is_some_and(|c| ident_start(c) || c == '-')
                    || Self::valid_escape(second, third)
            }
            Some('\\') => Self::valid_escape(first, second),
            Some(c) => ident_start(c),
            None => false,
        }
    }
    /// §4.3.10: whether three code points would start a number.
    fn starts_number(first: Option<char>, second: Option<char>, third: Option<char>) -> bool {
        match first {
            Some('+' | '-') => {
                second.is_some_and(|c| c.is_ascii_digit())
                    || (second == Some('.') && third.is_some_and(|c| c.is_ascii_digit()))
            }
            Some('.') => second.is_some_and(|c| c.is_ascii_digit()),
            Some(c) => c.is_ascii_digit(),
            None => false,
        }
    }
    /// §4.3.7, after the reverse solidus has been consumed.
    fn escape(&mut self) -> char {
        let Some(c) = self.peek(0) else {
            return '\u{FFFD}';
        };
        self.at += 1;
        if !c.is_ascii_hexdigit() {
            return c;
        }
        let mut value = c.to_digit(16).unwrap_or(0);
        let mut digits = 1;
        while digits < MAX_ESCAPE_DIGITS
            && let Some(digit) = self.peek(0).and_then(|d| d.to_digit(16))
        {
            value = value * 16 + digit;
            digits += 1;
            self.at += 1;
        }
        if self.peek(0).is_some_and(whitespace) {
            self.at += 1;
        }
        if value == 0 || value > MAX_CODE_POINT {
            return '\u{FFFD}';
        }
        // Surrogates are not scalar values, so `from_u32` refuses them.
        char::from_u32(value).unwrap_or('\u{FFFD}')
    }
    /// §4.3.11: an ident sequence.
    fn ident_sequence(&mut self) -> String {
        let mut out = String::new();
        loop {
            match self.peek(0) {
                Some(c) if ident_char(c) => {
                    out.push(c);
                    self.at += 1;
                }
                first if Self::valid_escape(first, self.peek(1)) => {
                    self.at += 1;
                    out.push(self.escape());
                }
                _ => return out,
            }
        }
    }
    /// §4.3.12: a number.
    fn number(&mut self) -> Numeric {
        let start = self.at;
        let mut integer = true;
        if matches!(self.peek(0), Some('+' | '-')) {
            self.at += 1;
        }
        let digits = |this: &mut Self| {
            while this.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                this.at += 1;
            }
        };
        digits(self);
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
            digits(self);
            integer = false;
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let signed = matches!(self.peek(1), Some('+' | '-'));
            let digit = if signed { self.peek(2) } else { self.peek(1) };
            if digit.is_some_and(|c| c.is_ascii_digit()) {
                self.at += if signed { 2 } else { 1 };
                digits(self);
                integer = false;
            }
        }
        let repr: String = self.input.chars[start..self.at].iter().collect();
        // §4.3.13: the representation is a valid Rust float literal, with an
        // optional leading `+`; an overflow becomes an infinity, which the
        // value grammars refuse or clamp.
        let value = repr.parse::<f64>().unwrap_or(f64::NAN);
        Numeric {
            value,
            integer,
            repr,
        }
    }
    /// §4.3.3: a numeric token.
    fn numeric(&mut self) -> Token {
        let number = self.number();
        if Self::starts_ident(self.peek(0), self.peek(1), self.peek(2)) {
            let unit = self.ident_sequence();
            Token::Dimension(number, unit)
        } else if self.peek(0) == Some('%') {
            self.at += 1;
            Token::Percentage(number)
        } else {
            Token::Number(number)
        }
    }
    /// §4.3.4: an ident-like token.
    fn ident_like(&mut self) -> Token {
        let name = self.ident_sequence();
        if name.eq_ignore_ascii_case("url") && self.peek(0) == Some('(') {
            self.at += 1;
            while self.peek(0).is_some_and(whitespace) && self.peek(1).is_some_and(whitespace) {
                self.at += 1;
            }
            let quoted = |c: Option<char>| matches!(c, Some('"' | '\''));
            if quoted(self.peek(0))
                || (self.peek(0).is_some_and(whitespace) && quoted(self.peek(1)))
            {
                return Token::Function(name);
            }
            return self.url();
        }
        if self.peek(0) == Some('(') {
            self.at += 1;
            return Token::Function(name);
        }
        Token::Ident(name)
    }
    /// §4.3.6: a url token, after `url(`.
    fn url(&mut self) -> Token {
        let mut out = String::new();
        while self.peek(0).is_some_and(whitespace) {
            self.at += 1;
        }
        loop {
            let Some(c) = self.peek(0) else {
                return Token::Url(out);
            };
            self.at += 1;
            match c {
                ')' => return Token::Url(out),
                c if whitespace(c) => {
                    while self.peek(0).is_some_and(whitespace) {
                        self.at += 1;
                    }
                    match self.peek(0) {
                        None => return Token::Url(out),
                        Some(')') => {
                            self.at += 1;
                            return Token::Url(out);
                        }
                        Some(_) => return self.bad_url(),
                    }
                }
                '"' | '\'' | '(' => return self.bad_url(),
                c if non_printable(c) => return self.bad_url(),
                '\\' => {
                    if Self::valid_escape(Some('\\'), self.peek(0)) {
                        out.push(self.escape());
                    } else {
                        return self.bad_url();
                    }
                }
                c => out.push(c),
            }
        }
    }
    /// §4.3.14: the remnants of a bad url.
    fn bad_url(&mut self) -> Token {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            if c == ')' {
                break;
            }
            if Self::valid_escape(Some(c), self.peek(0)) {
                self.escape();
            }
        }
        Token::BadUrl
    }
    /// §4.3.5: a string token, after its opening quote.
    fn string(&mut self, ending: char) -> Token {
        let mut out = String::new();
        loop {
            let Some(c) = self.peek(0) else {
                return Token::String(out);
            };
            if newline(c) {
                return Token::BadString;
            }
            self.at += 1;
            if c == ending {
                return Token::String(out);
            }
            if c == '\\' {
                match self.peek(0) {
                    None => {}
                    Some(next) if newline(next) => self.at += 1,
                    Some(_) => out.push(self.escape()),
                }
            } else {
                out.push(c);
            }
        }
    }
    /// §4.3.2: comments.
    fn comments(&mut self) {
        while self.peek(0) == Some('/') && self.peek(1) == Some('*') {
            self.at += 2;
            loop {
                match self.peek(0) {
                    None => return,
                    Some('*') if self.peek(1) == Some('/') => {
                        self.at += 2;
                        break;
                    }
                    Some(_) => self.at += 1,
                }
            }
        }
    }
    /// §4.3.1: the next token.
    fn token(&mut self) -> Option<Token> {
        self.comments();
        let c = self.peek(0)?;
        Some(match c {
            c if whitespace(c) => {
                while self.peek(0).is_some_and(whitespace) {
                    self.at += 1;
                }
                Token::Whitespace
            }
            '"' | '\'' => {
                self.at += 1;
                self.string(c)
            }
            '#' => {
                if self.peek(1).is_some_and(ident_char)
                    || Self::valid_escape(self.peek(1), self.peek(2))
                {
                    self.at += 1;
                    let id = Self::starts_ident(self.peek(0), self.peek(1), self.peek(2));
                    Token::Hash {
                        value: self.ident_sequence(),
                        id,
                    }
                } else {
                    self.at += 1;
                    Token::Delim('#')
                }
            }
            '+' | '.' if Self::starts_number(Some(c), self.peek(1), self.peek(2)) => self.numeric(),
            '-' if Self::starts_number(Some(c), self.peek(1), self.peek(2)) => self.numeric(),
            '-' if self.peek(1) == Some('-') && self.peek(2) == Some('>') => {
                self.at += 3;
                Token::Cdc
            }
            '-' if Self::starts_ident(Some(c), self.peek(1), self.peek(2)) => self.ident_like(),
            '<' if self.peek(1) == Some('!')
                && self.peek(2) == Some('-')
                && self.peek(3) == Some('-') =>
            {
                self.at += 4;
                Token::Cdo
            }
            '@' if Self::starts_ident(self.peek(1), self.peek(2), self.peek(3)) => {
                self.at += 1;
                Token::AtKeyword(self.ident_sequence())
            }
            '\\' if Self::valid_escape(Some(c), self.peek(1)) => self.ident_like(),
            c if c.is_ascii_digit() => self.numeric(),
            c if ident_start(c) => self.ident_like(),
            _ => {
                self.at += 1;
                match c {
                    '(' => Token::OpenParen,
                    ')' => Token::CloseParen,
                    '[' => Token::OpenSquare,
                    ']' => Token::CloseSquare,
                    '{' => Token::OpenCurly,
                    '}' => Token::CloseCurly,
                    ',' => Token::Comma,
                    ':' => Token::Colon,
                    ';' => Token::Semicolon,
                    other => Token::Delim(other),
                }
            }
        })
    }
}

/// Non-printable code point (§4.2).
fn non_printable(c: char) -> bool {
    matches!(c, '\0'..='\x08' | '\x0B' | '\x0E'..='\x1F' | '\x7F')
}

/// Tokenize `text` (§4.3), dropping comments. The work is linear in the
/// text; callers bound the text and charge it.
#[must_use]
pub fn tokenize(text: &str) -> Vec<Spanned> {
    let mut tokenizer = Tokenizer {
        input: Input::new(text),
        at: 0,
    };
    let mut out = Vec::new();
    loop {
        tokenizer.comments();
        let start = tokenizer.input.offsets[tokenizer.at];
        let Some(token) = tokenizer.token() else {
            return out;
        };
        out.push(Spanned {
            token,
            start,
            end: tokenizer.input.offsets[tokenizer.at],
        });
    }
}

/// Whether `text` contains a comment or an escape, so that its stored form
/// must be the token serialization rather than the source text.
#[must_use]
pub fn needs_serialization(text: &str) -> bool {
    text.contains('\\') || text.contains("/*")
}

/// Serialize an identifier (CSSOM §2.1 "serialize an identifier").
pub fn serialize_identifier(name: &str, out: &mut String) {
    let chars: Vec<char> = name.chars().collect();
    if chars.as_slice() == ['-'] {
        out.push_str("\\-");
        return;
    }
    for (index, &c) in chars.iter().enumerate() {
        let leading_digit = c.is_ascii_digit() && (index == 0 || (index == 1 && chars[0] == '-'));
        if c == '\0' {
            out.push('\u{FFFD}');
        } else if matches!(c, '\x01'..='\x1F' | '\x7F') || leading_digit {
            out.push_str(&format!("\\{:x} ", u32::from(c)));
        } else if ident_char(c) {
            out.push(c);
        } else {
            out.push('\\');
            out.push(c);
        }
    }
}

/// Serialize a string (CSSOM §2.1 "serialize a string"), with the quote
/// and reverse solidus as hexadecimal escapes so that a naive scan for the
/// closing quote stays correct.
pub fn serialize_string(value: &str, out: &mut String) {
    out.push('"');
    for c in value.chars() {
        match c {
            '\0' => out.push('\u{FFFD}'),
            '\x01'..='\x1F' | '\x7F' | '"' | '\\' => {
                out.push_str(&format!("\\{:x} ", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Whether two adjacent tokens need a comment between them to keep their
/// boundary (CSS Syntax 3 §9, the serialization table).
fn needs_separator(a: &Token, b: &Token) -> bool {
    use Token as T;
    let numeric = |t: &Token| matches!(t, T::Number(_) | T::Percentage(_) | T::Dimension(..));
    let identish = |t: &Token| {
        matches!(
            t,
            T::Ident(_) | T::Function(_) | T::Url(_) | T::BadUrl | T::Cdc
        ) || matches!(t, T::Delim('-'))
            || numeric(t)
    };
    match a {
        T::Ident(_) => identish(b) || matches!(b, T::OpenParen),
        T::AtKeyword(_) | T::Hash { .. } | T::Dimension(..) => identish(b),
        T::Delim('#' | '-') => identish(b),
        T::Number(_) => identish(b) || matches!(b, T::Delim('%')),
        T::Delim('@') => {
            matches!(
                b,
                T::Ident(_) | T::Function(_) | T::Url(_) | T::BadUrl | T::Delim('-')
            )
        }
        T::Delim('.' | '+') => numeric(b),
        T::Delim('/') => matches!(b, T::Delim('*')),
        _ => false,
    }
}

/// Serialize tokens (CSS Syntax 3 §9), inserting an empty comment where two
/// tokens would otherwise merge. Whitespace serializes as one space.
#[must_use]
pub fn serialize(tokens: &[Spanned]) -> String {
    let mut out = String::new();
    let mut previous: Option<&Token> = None;
    for spanned in tokens {
        let token = &spanned.token;
        if previous.is_some_and(|previous| needs_separator(previous, token)) {
            out.push_str("/**/");
        }
        match token {
            Token::Ident(name) => serialize_identifier(name, &mut out),
            Token::Function(name) => {
                serialize_identifier(name, &mut out);
                out.push('(');
            }
            Token::AtKeyword(name) => {
                out.push('@');
                serialize_identifier(name, &mut out);
            }
            Token::Hash { value, .. } => {
                out.push('#');
                for c in value.chars() {
                    if ident_char(c) {
                        out.push(c);
                    } else {
                        serialize_identifier(&c.to_string(), &mut out);
                    }
                }
            }
            Token::String(value) => serialize_string(value, &mut out),
            Token::BadString | Token::BadUrl => {}
            Token::Url(value) => {
                out.push_str("url(");
                out.push_str(value);
                out.push(')');
            }
            Token::Delim(c) => out.push(*c),
            Token::Number(number) => out.push_str(&number.repr),
            Token::Percentage(number) => {
                out.push_str(&number.repr);
                out.push('%');
            }
            Token::Dimension(number, unit) => {
                out.push_str(&number.repr);
                // A unit that starts like an exponent needs an escape.
                if unit.starts_with(['e', 'E'])
                    && unit[1..].starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+')
                {
                    out.push_str(&format!(
                        "\\{:x} ",
                        u32::from(unit.chars().next().unwrap_or('e'))
                    ));
                    serialize_identifier(&unit[1..], &mut out);
                } else {
                    serialize_identifier(unit, &mut out);
                }
            }
            Token::Whitespace => out.push(' '),
            Token::Cdo => out.push_str("<!--"),
            Token::Cdc => out.push_str("-->"),
            Token::Colon => out.push(':'),
            Token::Semicolon => out.push(';'),
            Token::Comma => out.push(','),
            Token::OpenSquare => out.push('['),
            Token::CloseSquare => out.push(']'),
            Token::OpenParen => out.push('('),
            Token::CloseParen => out.push(')'),
            Token::OpenCurly => out.push('{'),
            Token::CloseCurly => out.push('}'),
        }
        previous = Some(token);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Token> {
        tokenize(text).into_iter().map(|t| t.token).collect()
    }
    fn ident(name: &str) -> Token {
        Token::Ident(name.into())
    }

    #[test]
    fn escapes_are_decoded_before_any_check() {
        assert_eq!(kinds(".sm\\:flex")[1], ident("sm:flex"));
        assert_eq!(kinds(".w-1\\/2")[1], ident("w-1/2"));
        assert_eq!(kinds(".p-0\\.5")[1], ident("p-0.5"));
        assert_eq!(kinds(".\\[x\\]")[1], ident("[x]"));
        assert_eq!(kinds(".\\!")[1], ident("!"));
        assert_eq!(kinds(".\\32 xl\\:flex")[1], ident("2xl:flex"));
        // Hex escapes: at most six digits, one trailing whitespace, NUL and
        // out-of-range values become U+FFFD.
        assert_eq!(kinds("\\41 b")[0], ident("Ab"));
        assert_eq!(kinds("\\0 a")[0], ident("\u{FFFD}a"));
        assert_eq!(kinds("\\110000 a")[0], ident("\u{FFFD}a"));
        assert_eq!(kinds("\\D800 a")[0], ident("\u{FFFD}a"));
        // An escaped `url(` is still a url token, and an escaped function
        // name is still that function.
        assert!(matches!(kinds("\\75 rl(x)")[0], Token::Url(_)));
        assert_eq!(kinds("u\\rl(\"x\")")[0], Token::Function("url".into()));
        assert_eq!(kinds("\\@import")[0], ident("@import"));
        assert_eq!(kinds("@\\69mport")[0], Token::AtKeyword("import".into()));
    }

    #[test]
    fn comments_numbers_and_preprocessing_follow_the_specification() {
        assert_eq!(kinds("/*! x */a/**/b"), vec![ident("a"), ident("b")]);
        assert_eq!(kinds("/* open"), vec![]);
        let Token::Dimension(number, unit) = &kinds("1.5e2px")[0] else {
            panic!()
        };
        assert_eq!(
            (number.value, number.integer, unit.as_str()),
            (150.0, false, "px")
        );
        assert!(matches!(&kinds("+.5%")[0], Token::Percentage(n) if n.value == 0.5));
        assert!(matches!(&kinds("-1")[0], Token::Number(n) if n.integer));
        assert_eq!(kinds("1e"), kinds("1e"));
        assert!(matches!(&kinds("1e")[0], Token::Dimension(_, unit) if unit == "e"));
        assert_eq!(kinds("a\r\nb").len(), 3);
        assert_eq!(kinds("\0")[0], ident("\u{FFFD}"));
        assert_eq!(
            kinds("<!-- -->"),
            vec![Token::Cdo, Token::Whitespace, Token::Cdc]
        );
        assert_eq!(
            kinds("\"a\nb\""),
            vec![
                Token::BadString,
                Token::Whitespace,
                ident("b"),
                Token::String(String::new())
            ]
        );
        assert_eq!(kinds("'a\\\nb'"), vec![Token::String("ab".into())]);
        assert!(matches!(kinds("url( a b )")[0], Token::BadUrl));
        assert_eq!(
            kinds("#-a")[0],
            Token::Hash {
                value: "-a".into(),
                id: true
            }
        );
        assert_eq!(
            kinds("#1a")[0],
            Token::Hash {
                value: "1a".into(),
                id: false
            }
        );
    }

    #[test]
    fn spans_are_utf8_byte_offsets_of_the_source() {
        let tokens = tokenize("é /* c */ .x");
        assert_eq!((tokens[0].start, tokens[0].end), (0, 2));
        // A comment between two whitespace runs leaves two whitespace tokens.
        assert_eq!(tokens[3].token, Token::Delim('.'));
        assert_eq!(tokens[3].start, "é /* c */ ".len());
    }

    #[test]
    fn serialization_round_trips_token_boundaries() {
        for source in [
            "a/**/b",
            "1/**/px",
            "sm\\:flex",
            "\"a\\\"b\"",
            "-\\31 a",
            "calc(1px + 2px)",
            "1\\65 3",
        ] {
            let tokens: Vec<_> = kinds(source);
            let serialized = serialize(&tokenize(source));
            assert_eq!(kinds(&serialized), tokens, "{source} -> {serialized}");
        }
    }
}
