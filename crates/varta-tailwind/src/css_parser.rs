//! CSS parser: a port of Tailwind CSS v4.3.5 `css-parser.ts` (MIT, Copyright
//! (c) Tailwind Labs, Inc.; see `LICENSE-TAILWIND`).
//!
//! The parser is deliberately loose, exactly like Tailwind's: it splits the
//! input into rules, at-rules and declarations, keeps custom property values
//! verbatim (including their comments), drops ordinary comments and hoists
//! license comments (`/*! … */`) to the front of the result. Source maps are
//! not ported. Every error carries the zero-based UTF-8 byte offset where it
//! was found, and block nesting is bounded by [`MAX_CSS_NESTING_DEPTH`].
use crate::ast::Node;

/// The deepest block nesting the parser accepts (Tailwind has no bound).
pub const MAX_CSS_NESTING_DEPTH: usize = 256;

const BACKSLASH: u8 = b'\\';
const SLASH: u8 = b'/';
const ASTERISK: u8 = b'*';
const DOUBLE_QUOTE: u8 = b'"';
const SINGLE_QUOTE: u8 = b'\'';
const COLON: u8 = b':';
const SEMICOLON: u8 = b';';
const LINE_BREAK: u8 = b'\n';
const CARRIAGE_RETURN: u8 = b'\r';
const SPACE: u8 = b' ';
const TAB: u8 = b'\t';
const OPEN_CURLY: u8 = b'{';
const CLOSE_CURLY: u8 = b'}';
const OPEN_PAREN: u8 = b'(';
const CLOSE_PAREN: u8 = b')';
const OPEN_BRACKET: u8 = b'[';
const CLOSE_BRACKET: u8 = b']';
const DASH: u8 = b'-';
const AT_SIGN: u8 = b'@';
const EXCLAMATION_MARK: u8 = b'!';

/// The byte-order mark Tailwind replaces by a space.
const BYTE_ORDER_MARK: char = '\u{FEFF}';
/// The `!important` flag at the end of a declaration value.
const IMPORTANT: &str = "!important";
/// CSS Syntax 3 §4.3.2 / §9: the CDO and CDC tokens skipped between
/// top-level nodes.
const CDO: &[u8] = b"<!--";
const CDC: &[u8] = b"-->";
/// The shortest standard at-rule name, `@page`, in UTF-16 code units: the
/// name of an at-rule is never split before this position (Tailwind's
/// documented assumption in `parseAtRule`).
const SHORTEST_AT_RULE_NAME: usize = "@page".len();

/// What went wrong while parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CssSyntaxErrorKind {
    #[error("Missing opening {{")]
    MissingOpeningCurly,
    #[error("Missing opening (")]
    MissingOpeningParen,
    #[error("Missing closing }} at {0}")]
    MissingClosingCurly(String),
    #[error("Unterminated string: {0}")]
    UnterminatedString(String),
    #[error("Invalid custom property, expected a value")]
    InvalidCustomProperty,
    #[error("Invalid declaration: `{0}`")]
    InvalidDeclaration(String),
    #[error("Nesting deeper than {MAX_CSS_NESTING_DEPTH} blocks")]
    NestingTooDeep,
}

/// A CSS syntax error (Tailwind's `CssSyntaxError`) and the zero-based UTF-8
/// byte offset where it was found.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} (at byte {offset})")]
pub struct CssSyntaxError {
    pub kind: CssSyntaxErrorKind,
    pub offset: usize,
}

impl CssSyntaxError {
    fn new(kind: CssSyntaxErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// Tailwind's message, without the location.
    #[must_use]
    pub fn message(&self) -> String {
        self.kind.to_string()
    }
}

/// JavaScript `String.prototype.trim` whitespace (ECMA-262 WhiteSpace and
/// LineTerminator), which differs from Rust's `char::is_whitespace` in
/// U+0085 and U+FEFF.
pub(crate) fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{D}'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// JavaScript `trim()`.
pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// An open block and the byte offset of its prelude.
struct Open {
    node: Node,
    start: usize,
}

/// Parser state: the input, the open blocks and the node being collected.
struct Parser<'a> {
    input: &'a [u8],
    ast: Vec<Node>,
    license_comments: Vec<Node>,
    stack: Vec<Open>,
    buffer: Vec<u8>,
    closing_bracket_stack: Vec<u8>,
    buffer_start: usize,
}

impl Parser<'_> {
    fn byte(&self, index: usize) -> Option<u8> {
        self.input.get(index).copied()
    }

    /// The buffer as text. The buffer only ever holds whole UTF-8 sequences
    /// copied in order from the input, so the conversion is lossless.
    fn buffer_text(&self) -> String {
        String::from_utf8_lossy(&self.buffer).into_owned()
    }

    fn push_node(&mut self, node: Node) {
        match self.stack.last_mut() {
            Some(open) => {
                if let Some(children) = open.node.children_mut() {
                    children.push(node);
                }
            }
            None => self.ast.push(node),
        }
    }

    fn top_closing(&self) -> Option<u8> {
        self.closing_bracket_stack.last().copied()
    }
}

/// Parse a stylesheet (Tailwind's `parse`).
///
/// # Errors
/// A [`CssSyntaxError`] for unbalanced blocks, unterminated strings, invalid
/// declarations or blocks nested deeper than [`MAX_CSS_NESTING_DEPTH`].
pub fn parse(input: &str) -> Result<Vec<Node>, CssSyntaxError> {
    // Tailwind replaces the BOM by one space; three spaces keep byte offsets.
    let replaced;
    let input = match input.strip_prefix(BYTE_ORDER_MARK) {
        Some(rest) => {
            replaced = format!("{}{rest}", " ".repeat(BYTE_ORDER_MARK.len_utf8()));
            replaced.as_str()
        }
        None => input,
    };
    let mut p = Parser {
        input: input.as_bytes(),
        ast: Vec::new(),
        license_comments: Vec::new(),
        stack: Vec::new(),
        buffer: Vec::new(),
        closing_bracket_stack: Vec::new(),
        buffer_start: 0,
    };
    let len = p.input.len();
    let mut i = 0;
    while i < len {
        let current = p.input[i];
        // Skip the CR of a CRLF pair.
        if current == CARRIAGE_RETURN && p.byte(i + 1) == Some(LINE_BREAK) {
            i += 1;
            continue;
        }
        if current == BACKSLASH {
            // An escape consumes the next (whole) character.
            if p.buffer.is_empty() {
                p.buffer_start = i;
            }
            let next_len = input[i + 1..].chars().next().map_or(0, char::len_utf8);
            p.buffer.extend_from_slice(&p.input[i..i + 1 + next_len]);
            i += 1 + next_len;
            continue;
        } else if current == SLASH && p.byte(i + 1) == Some(ASTERISK) {
            i = comment(&mut p, i);
        } else if current == SINGLE_QUOTE || current == DOUBLE_QUOTE {
            let end = parse_string(input, i, current)?;
            p.buffer.extend_from_slice(&p.input[i..=end.min(len - 1)]);
            i = end;
        } else if is_space(current) && next_is_space(p.input, i) {
            // Collapse runs of whitespace.
        } else if current == LINE_BREAK {
            if let Some(&last) = p.buffer.last()
                && !is_space(last)
            {
                p.buffer.push(SPACE);
            }
        } else if p.stack.is_empty()
            && p.buffer.is_empty()
            && (p.input[i..].starts_with(CDC) || p.input[i..].starts_with(CDO))
        {
            let token = if current == DASH {
                CDC.len()
            } else {
                CDO.len()
            };
            i += token - 1;
        } else if current == DASH && p.byte(i + 1) == Some(DASH) && p.buffer.is_empty() {
            i = custom_property(&mut p, input, i)?;
        } else if current == SEMICOLON && p.buffer.first() == Some(&AT_SIGN) {
            let node = parse_at_rule(&p.buffer_text(), Vec::new());
            p.push_node(node);
            p.buffer.clear();
        } else if current == SEMICOLON && p.top_closing() != Some(CLOSE_PAREN) {
            match parse_declaration(&p.buffer_text(), None, None) {
                Some(declaration) => {
                    p.push_node(declaration);
                    p.buffer.clear();
                }
                None if p.buffer.is_empty() => {}
                None => {
                    return Err(CssSyntaxError::new(
                        CssSyntaxErrorKind::InvalidDeclaration(js_trim(&p.buffer_text()).into()),
                        p.buffer_start,
                    ));
                }
            }
        } else if current == OPEN_CURLY && p.top_closing() != Some(CLOSE_PAREN) {
            if p.stack.len() >= MAX_CSS_NESTING_DEPTH {
                return Err(CssSyntaxError::new(CssSyntaxErrorKind::NestingTooDeep, i));
            }
            p.closing_bracket_stack.push(CLOSE_CURLY);
            let node = rule(js_trim(&p.buffer_text()));
            p.stack.push(Open {
                node,
                start: p.buffer_start,
            });
            p.buffer.clear();
        } else if current == CLOSE_CURLY && p.top_closing() != Some(CLOSE_PAREN) {
            close_block(&mut p, i)?;
        } else if current == OPEN_PAREN {
            p.closing_bracket_stack.push(CLOSE_PAREN);
            p.buffer.push(OPEN_PAREN);
        } else if current == CLOSE_PAREN {
            if p.top_closing() != Some(CLOSE_PAREN) {
                return Err(CssSyntaxError::new(
                    CssSyntaxErrorKind::MissingOpeningParen,
                    i,
                ));
            }
            p.closing_bracket_stack.pop();
            p.buffer.push(CLOSE_PAREN);
        } else {
            // Skip whitespace at the start of a new node.
            if p.buffer.is_empty() && is_space(current) {
                i += 1;
                continue;
            }
            if p.buffer.is_empty() {
                p.buffer_start = i;
            }
            p.buffer.push(current);
        }
        i += 1;
    }

    finish(p)
}

/// Whether `byte` is a space, line break or tab.
fn is_space(byte: u8) -> bool {
    byte == SPACE || byte == LINE_BREAK || byte == TAB
}

/// Whether whitespace follows position `i` (a CRLF counts as whitespace).
fn next_is_space(input: &[u8], i: usize) -> bool {
    match input.get(i + 1).copied() {
        Some(next) if is_space(next) => true,
        Some(CARRIAGE_RETURN) => input.get(i + 2).copied() == Some(LINE_BREAK),
        _ => false,
    }
}

/// Skip a comment starting at `i`; license comments are collected. Returns
/// the index of the comment's last byte (or `i` when it is unterminated,
/// as in Tailwind).
fn comment(p: &mut Parser<'_>, start: usize) -> usize {
    let mut end = start;
    let mut j = start + 2;
    while j < p.input.len() {
        if p.input[j] == ASTERISK && p.byte(j + 1) == Some(SLASH) {
            end = j + 1;
            break;
        }
        j += 1;
    }
    let text = &p.input[start..=end];
    if text.get(2) == Some(&EXCLAMATION_MARK) {
        let value = String::from_utf8_lossy(&text[2..text.len() - 2]).into_owned();
        p.license_comments.push(Node::Comment(value));
    }
    end
}

/// A custom property starting at `start`: its value is kept verbatim and may
/// contain any balanced brackets. Returns the index of the last consumed
/// byte.
fn custom_property(p: &mut Parser<'_>, input: &str, start: usize) -> Result<usize, CssSyntaxError> {
    let bytes = p.input;
    let mut closing: Vec<u8> = Vec::new();
    let mut colon: Option<usize> = None;
    // The end of the value without trailing whitespace and comments.
    let mut value_end = 0;
    let mut i = start;
    let mut j = start + 2;
    while j < bytes.len() {
        let peek = bytes[j];
        if peek == BACKSLASH {
            j += 1;
        } else if peek == SINGLE_QUOTE || peek == DOUBLE_QUOTE {
            j = parse_string(input, j, peek)?;
        } else if peek == SLASH && bytes.get(j + 1) == Some(&ASTERISK) {
            let mut k = j + 2;
            while k < bytes.len() {
                if bytes[k] == ASTERISK && bytes.get(k + 1) == Some(&SLASH) {
                    j = k + 1;
                    break;
                }
                k += 1;
            }
            j += 1;
            continue;
        } else if colon.is_none() && peek == COLON {
            colon = Some(j - start);
        } else if peek == SEMICOLON && closing.is_empty() {
            p.buffer.extend_from_slice(&bytes[start..j]);
            i = j;
            break;
        } else if peek == OPEN_PAREN {
            closing.push(CLOSE_PAREN);
        } else if peek == OPEN_BRACKET {
            closing.push(CLOSE_BRACKET);
        } else if peek == OPEN_CURLY {
            closing.push(CLOSE_CURLY);
        } else if peek == CLOSE_CURLY && closing.is_empty() {
            i = j - 1;
            p.buffer.extend_from_slice(&bytes[start..j]);
            break;
        } else if (peek == CLOSE_PAREN || peek == CLOSE_BRACKET || peek == CLOSE_CURLY)
            && closing.last() == Some(&peek)
        {
            closing.pop();
        }
        if !is_space(peek) && peek != CARRIAGE_RETURN {
            value_end = j + 1 - start;
        }
        j += 1;
    }
    // No `;` or `}` ended the property: the rest of the input belongs to it.
    if p.buffer.is_empty() {
        p.buffer.extend_from_slice(&bytes[start..]);
        i = bytes.len();
    }
    let buffer = p.buffer_text();
    let declaration = colon
        .and_then(|colon| parse_declaration(&buffer, Some(colon), Some(value_end)))
        .ok_or_else(|| CssSyntaxError::new(CssSyntaxErrorKind::InvalidCustomProperty, start))?;
    p.push_node(declaration);
    p.buffer.clear();
    Ok(i)
}

/// A `}`: complete a pending node and close the current block.
fn close_block(p: &mut Parser<'_>, i: usize) -> Result<(), CssSyntaxError> {
    if p.closing_bracket_stack.is_empty() {
        return Err(CssSyntaxError::new(
            CssSyntaxErrorKind::MissingOpeningCurly,
            i,
        ));
    }
    p.closing_bracket_stack.pop();
    if !p.buffer.is_empty() {
        if p.buffer.first() == Some(&AT_SIGN) {
            let node = parse_at_rule(&p.buffer_text(), Vec::new());
            p.push_node(node);
        } else if !p.stack.is_empty() {
            let buffer = p.buffer_text();
            let declaration = parse_declaration(&buffer, None, None).ok_or_else(|| {
                CssSyntaxError::new(
                    CssSyntaxErrorKind::InvalidDeclaration(js_trim(&buffer).into()),
                    p.buffer_start,
                )
            })?;
            p.push_node(declaration);
        }
    }
    if let Some(open) = p.stack.pop() {
        p.push_node(open.node);
    }
    p.buffer.clear();
    Ok(())
}

/// Complete the leftover buffer at the end of the input and check balance.
fn finish(mut p: Parser<'_>) -> Result<Vec<Node>, CssSyntaxError> {
    if p.buffer.first() == Some(&AT_SIGN) {
        let node = parse_at_rule(&p.buffer_text(), Vec::new());
        p.ast.push(node);
    } else if !p.buffer.is_empty() && p.closing_bracket_stack.is_empty() {
        // Unbalanced brackets (`color: rgb(1, 2`) leave an incomplete
        // declaration, which is dropped.
        if let Some(declaration) = parse_declaration(&p.buffer_text(), None, None) {
            p.push_node(declaration);
        }
    }
    if !p.closing_bracket_stack.is_empty()
        && let Some(open) = p.stack.last()
    {
        let at = match &open.node {
            Node::AtRule { name, params, .. } => format!("{name} {params}"),
            Node::Rule { selector, .. } => selector.clone(),
            _ => String::new(),
        };
        return Err(CssSyntaxError::new(
            CssSyntaxErrorKind::MissingClosingCurly(at),
            open.start,
        ));
    }
    let mut result = p.license_comments;
    result.extend(p.ast);
    Ok(result)
}

/// A rule from a prelude: `@name params` is an at-rule, anything else a
/// style rule (Tailwind's `rule`).
#[must_use]
pub fn rule(prelude: &str) -> Node {
    if prelude.starts_with('@') {
        parse_at_rule(prelude, Vec::new())
    } else {
        Node::rule(prelude, Vec::new())
    }
}

/// Split `@name params` into an at-rule (Tailwind's `parseAtRule`): the
/// name ends at the first space, tab or `(` after the first five UTF-16 code
/// units.
#[must_use]
pub fn parse_at_rule(buffer: &str, nodes: Vec<Node>) -> Node {
    let mut name = buffer;
    let mut params = "";
    let mut units = 0;
    for (index, c) in buffer.char_indices() {
        if units >= SHORTEST_AT_RULE_NAME && matches!(c, ' ' | '\t' | '(') {
            name = &buffer[..index];
            params = &buffer[index..];
            break;
        }
        units += c.len_utf16();
    }
    Node::at_rule(js_trim(name), js_trim(params), nodes)
}

/// Split a declaration at its colon (Tailwind's `parseDeclaration`).
/// `colon` defaults to the first `:` and `value_end` (the end of the value
/// without trailing comments) to the buffer's end.
fn parse_declaration(buffer: &str, colon: Option<usize>, value_end: Option<usize>) -> Option<Node> {
    let colon = colon.or_else(|| buffer.find(':'))?;
    let property = js_trim(&buffer[..colon]);
    let mut value_end = value_end
        .unwrap_or(buffer.len())
        .clamp(colon + 1, buffer.len());
    while !buffer.is_char_boundary(value_end) {
        value_end += 1;
    }
    let value = trim_value(&buffer[colon + 1..value_end]);
    if value.ends_with(IMPORTANT) {
        let bang = value.len() - IMPORTANT.len();
        if !is_escaped(value, bang) {
            return Some(Node::Decl {
                property: property.to_owned(),
                value: Some(trim_value(&value[..bang]).to_owned()),
                important: true,
            });
        }
    }
    Some(Node::decl(property, trim_value(&buffer[colon + 1..])))
}

/// Trim a value but keep an escaped whitespace at its end.
fn trim_value(value: &str) -> &str {
    let value = value.trim_start_matches(is_js_whitespace);
    let trimmed = value.trim_end_matches(is_js_whitespace);
    if trimmed.len() < value.len() && is_escaped(value, trimmed.len()) {
        let kept = value[trimmed.len()..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        return &value[..trimmed.len() + kept];
    }
    trimmed
}

/// Whether the byte at `index` is escaped by an odd number of backslashes.
fn is_escaped(value: &str, index: usize) -> bool {
    let backslashes = value.as_bytes()[..index]
        .iter()
        .rev()
        .take_while(|&&byte| byte == BACKSLASH)
        .count();
    backslashes % 2 == 1
}

/// The index of the closing quote of the string starting at `start`, or
/// `start` when the input ends first (Tailwind's `parseString`).
fn parse_string(input: &str, start: usize, quote: u8) -> Result<usize, CssSyntaxError> {
    let bytes = input.as_bytes();
    let unterminated = |end: usize| {
        let mut text = String::from_utf8_lossy(&bytes[start..end]).into_owned();
        text.push(char::from(quote));
        CssSyntaxError::new(CssSyntaxErrorKind::UnterminatedString(text), start)
    };
    let mut i = start + 1;
    while i < bytes.len() {
        let peek = bytes[i];
        if peek == BACKSLASH {
            i += 1;
        } else if peek == quote {
            return Ok(i);
        } else if peek == SEMICOLON
            && (bytes.get(i + 1) == Some(&LINE_BREAK)
                || (bytes.get(i + 1) == Some(&CARRIAGE_RETURN)
                    && bytes.get(i + 2) == Some(&LINE_BREAK)))
        {
            return Err(unterminated(i + 1));
        } else if peek == LINE_BREAK
            || (peek == CARRIAGE_RETURN && bytes.get(i + 1) == Some(&LINE_BREAK))
        {
            return Err(unterminated(i));
        }
        i += 1;
    }
    Ok(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(property: &str, value: &str) -> Node {
        Node::decl(property, value)
    }

    fn important(property: &str, value: &str) -> Node {
        Node::Decl {
            property: property.into(),
            value: Some(value.into()),
            important: true,
        }
    }

    /// Run a case with Unix and Windows line endings (`describe.each`).
    fn both(input: &str) -> [Result<Vec<Node>, CssSyntaxError>; 2] {
        let unix = input.replace("\r\n", "\n");
        [parse(&unix), parse(&unix.replace('\n', "\r\n"))]
    }

    fn check(input: &str, expected: &[Node]) {
        for result in both(input) {
            assert_eq!(result.unwrap(), expected, "input: {input:?}");
        }
    }

    fn error(input: &str) -> String {
        parse(input).unwrap_err().message()
    }

    #[test]
    fn comments_are_dropped() {
        check("\n  /*Hello, world!*/\n", &[]);
        check(r"/*Hello, \*\/ world!*/", &[]);
        check(
            r"/* C:\temp\*/
            .foo {
              color: red;
            }",
            &[Node::rule(".foo", vec![decl("color", "red")])],
        );
        check(
            ".foo {\n  /*Example comment*/\n}",
            &[Node::rule(".foo", vec![])],
        );
        check(
            ".foo/*.bar*/.baz {\n}\n.foo/*.bar*//*.baz*/.qux\n{\n}\n.foo/*.bar*/ /*.baz*/.qux {\n}\n\
             .foo /*.bar*/.baz {\n}\n.foo/*.bar*/ .baz {\n}\n.foo/*.bar*/\n.baz {\n}",
            &[
                Node::rule(".foo.baz", vec![]),
                Node::rule(".foo.qux", vec![]),
                Node::rule(".foo .qux", vec![]),
                Node::rule(".foo .baz", vec![]),
                Node::rule(".foo .baz", vec![]),
                Node::rule(".foo .baz", vec![]),
            ],
        );
        check(
            ".dark /* comment */p {\n  color: black;\n}",
            &[Node::rule(".dark p", vec![decl("color", "black")])],
        );
    }

    #[test]
    fn license_comments_are_hoisted() {
        check(
            "/*! License #1 */\n.foo {\n  color: red; /*! License #1.5 */\n}\n/*! License #2 */\n\
             .bar {\n  /*! License #2.5 */\n  color: blue;\n}\n/*! License #3 */",
            &[
                Node::Comment("! License #1 ".into()),
                Node::Comment("! License #1.5 ".into()),
                Node::Comment("! License #2 ".into()),
                Node::Comment("! License #2.5 ".into()),
                Node::Comment("! License #3 ".into()),
                Node::rule(".foo", vec![decl("color", "red")]),
                Node::rule(".bar", vec![decl("color", "blue")]),
            ],
        );
    }

    #[test]
    fn declarations() {
        check("color: red;", &[decl("color", "red")]);
        check("color: red", &[decl("color", "red")]);
        check("color: red\n", &[decl("color", "red")]);
        check("color: red !important", &[important("color", "red")]);
        check("a: 1; b: 2", &[decl("a", "1"), decl("b", "2")]);
        check(
            "--a: 1;\ncolor: red",
            &[decl("--a", "1"), decl("color", "red")],
        );
        check("color: rgb(1, 2", &[]);
        check(
            "content: 'Hello, world!';",
            &[decl("content", "'Hello, world!'")],
        );
        check(
            r#"content: "It's a beautiful day!";"#,
            &[decl("content", r#""It's a beautiful day!""#)],
        );
        check(
            r#"content: 'These are not the end "\' of the string';"#,
            &[decl(
                "content",
                r#"'These are not the end "\' of the string'"#,
            )],
        );
        check(
            "grid-template-areas:\n  'header header header'\n  'sidebar main main';",
            &[decl(
                "grid-template-areas",
                "'header header header' 'sidebar main main'",
            )],
        );
        check(
            "color/* color: #f00; */: red;\nfont-weight:/* font-size: 12px */ bold;",
            &[decl("color", "red"), decl("font-weight", "bold")],
        );
        check(
            ".foo {\n  background: url(data:image/png;base64,abc==);\n  \
             background: url(https://x.org?q={query;limit=5}&ids=[1,2,3]);\n}",
            &[Node::rule(
                ".foo",
                vec![
                    decl("background", "url(data:image/png;base64,abc==)"),
                    decl(
                        "background",
                        "url(https://x.org?q={query;limit=5}&ids=[1,2,3])",
                    ),
                ],
            )],
        );
    }

    #[test]
    fn important_flags() {
        check("width: 123px !important;", &[important("width", "123px")]);
        check(
            "width: 123px !important /* Very important */;",
            &[important("width", "123px")],
        );
        check(
            "--foo: red !important /* Very important */;\n--bar: red /* not important */;",
            &[
                important("--foo", "red"),
                decl("--bar", "red /* not important */"),
            ],
        );
        check(
            "--foo: red /* !important /* example */;\n\
             --bar: red /* example */ !important /* example */ /* example */;\n\
             --baz: '/* example' !important;\n--quux: red !important\n  /* example */;",
            &[
                decl("--foo", "red /* !important /* example */"),
                important("--bar", "red /* example */"),
                important("--baz", "'/* example'"),
                important("--quux", "red"),
            ],
        );
        check(
            "content: 'a !important b';\nbackground: url('x!important.png') !important;\n\
             --foo: '!important';",
            &[
                decl("content", "'a !important b'"),
                important("background", "url('x!important.png')"),
                decl("--foo", "'!important'"),
            ],
        );
        check(
            "content: a\\ ;\nfont-family: a\\ !important;\n--foo: a\\ ;\n--bar: a\\ !important;\n\
             --baz: a\\\\ !important;",
            &[
                decl("content", "a\\ "),
                important("font-family", "a\\ "),
                decl("--foo", "a\\ "),
                important("--bar", "a\\ "),
                important("--baz", "a\\\\"),
            ],
        );
        check(
            "content: \\!important;\n--foo: red\\!important;\n--bar: red\\\\!important;",
            &[
                decl("content", "\\!important"),
                decl("--foo", "red\\!important"),
                important("--bar", "red\\\\"),
            ],
        );
    }

    #[test]
    fn custom_properties() {
        check("--foo: bar;", &[decl("--foo", "bar")]);
        check(
            ":root{--foo:bar;}",
            &[Node::rule(":root", vec![decl("--foo", "bar")])],
        );
        check(
            ":root{--foo:bar}",
            &[Node::rule(":root", vec![decl("--foo", "bar")])],
        );
        check("\n  --foo: bar\n", &[decl("--foo", "bar")]);
        check("\n  --foo: bar !important\n", &[important("--foo", "bar")]);
        check("--foo: var(--bar)", &[decl("--foo", "var(--bar)")]);
        check("--foo: {}", &[decl("--foo", "{}")]);
        check("--foo: bar /* baz */", &[decl("--foo", "bar /* baz */")]);
        check("--foo: [bar", &[decl("--foo", "[bar")]);
        check(
            "color: red; --foo: (bar",
            &[decl("color", "red"), decl("--foo", "(bar")],
        );
        check(
            "--foo: if(x > 5) this.width = 10;",
            &[decl("--foo", "if(x > 5) this.width = 10")],
        );
        check("--foo:;", &[decl("--foo", "")]);
        check("--foo: ;", &[decl("--foo", "")]);
        // Custom property blocks keep their line endings verbatim.
        let block = "{\n  background-color: red;\n  /* A comment ; */\n  content: 'x';\n}";
        for ending in ["\n", "\r\n"] {
            let block = block.replace('\n', ending);
            assert_eq!(
                parse(&format!("--foo: {block};")).unwrap(),
                vec![decl("--foo", &block)]
            );
        }
        check(
            r"--foo: This is not the end \;, but this is;",
            &[decl("--foo", r"This is not the end \;, but this is")],
        );
        check(
            r"--foo: /* C:\temp\*/ bar;",
            &[decl("--foo", r"/* C:\temp\*/ bar")],
        );
        check(
            "--foo: 'data:text/plain;base64,SGVsbG8=';",
            &[decl("--foo", "'data:text/plain;base64,SGVsbG8='")],
        );
    }

    #[test]
    fn selectors() {
        check(
            r".hover\:foo:hover {}
            .\32 xl\:foo {}",
            &[
                Node::rule(r".hover\:foo:hover", vec![]),
                Node::rule(r".\32 xl\:foo", vec![]),
            ],
        );
        check(".foo,\n.bar {\n}", &[Node::rule(".foo, .bar", vec![])]);
        check(
            ".foo {\n  color: red;\n  font-size: 16px !important\n}",
            &[Node::rule(
                ".foo",
                vec![decl("color", "red"), important("font-size", "16px")],
            )],
        );
        check(
            ".foo,\n.bar,\n.baz\t\n \n .qux\n{\ncolor:red;\n}",
            &[Node::rule(
                ".foo, .bar, .baz .qux",
                vec![decl("color", "red")],
            )],
        );
    }

    #[test]
    fn at_rules() {
        check(
            "@charset \"UTF-8\";",
            &[Node::at_rule("@charset", "\"UTF-8\"", vec![])],
        );
        for whitespace in [" ", "  ", "\t", " \t", "\n", " \n", "\r\n", "\r\n "] {
            assert_eq!(
                parse(&format!("@apply{whitespace}bg-red-500;")).unwrap(),
                vec![Node::at_rule("@apply", "bg-red-500", vec![])]
            );
        }
        check(
            "\n@tailwind utilities\n",
            &[Node::at_rule("@tailwind", "utilities", vec![])],
        );
        check(
            "@layer utilities {\n  @tailwind utilities\n}",
            &[Node::at_rule(
                "@layer",
                "utilities",
                vec![Node::at_rule("@tailwind", "utilities", vec![])],
            )],
        );
        check(
            "@tailwind;\n@tailwind base;",
            &[
                Node::at_rule("@tailwind", "", vec![]),
                Node::at_rule("@tailwind", "base", vec![]),
            ],
        );
        check(
            "@media (width >= 600px) {\n  .foo {\n    color: red;\n    @media (width >= 800px) {\n      \
             color: blue;\n    }\n  }\n}",
            &[Node::at_rule(
                "@media",
                "(width >= 600px)",
                vec![Node::rule(
                    ".foo",
                    vec![
                        decl("color", "red"),
                        Node::at_rule("@media", "(width >= 800px)", vec![decl("color", "blue")]),
                    ],
                )],
            )],
        );
        check(
            ".foo {\n  @apply hover:text-red-100\n         sm:hover:text-red-200;\n}",
            &[Node::rule(
                ".foo",
                vec![Node::at_rule(
                    "@apply",
                    "hover:text-red-100 sm:hover:text-red-200",
                    vec![],
                )],
            )],
        );
        check(
            r"@custom \{ { foo: bar; }",
            &[Node::at_rule("@custom", r"\{", vec![decl("foo", "bar")])],
        );
    }

    #[test]
    fn nesting() {
        check(
            ".foo {\n  font-weight: bold;\n  .bar {\n    color: red;\n  }\n  --in-between: 1;\n  \
             &:hover {\n    color: blue;\n  }\n  --at-the-end: 2;\n}",
            &[Node::rule(
                ".foo",
                vec![
                    decl("font-weight", "bold"),
                    Node::rule(".bar", vec![decl("color", "red")]),
                    decl("--in-between", "1"),
                    Node::rule("&:hover", vec![decl("color", "blue")]),
                    decl("--at-the-end", "2"),
                ],
            )],
        );
        check(
            ".foo{color:red;@media(width>=600px){.bar{color:blue;font-weight:bold}}}",
            &[Node::rule(
                ".foo",
                vec![
                    decl("color", "red"),
                    Node::at_rule(
                        "@media",
                        "(width>=600px)",
                        vec![Node::rule(
                            ".bar",
                            vec![decl("color", "blue"), decl("font-weight", "bold")],
                        )],
                    ),
                ],
            )],
        );
    }

    #[test]
    fn complex_and_semicolons() {
        check(
            r".foo:has(.bar /* instead \*\/ of .baz { */) { color: red; }",
            &[Node::rule(".foo:has(.bar )", vec![decl("color", "red")])],
        );
        check(";;;", &[]);
        check(
            "@plugin \"foo\" {} ;",
            &[Node::at_rule("@plugin", "\"foo\"", vec![])],
        );
        check(
            ".foo { color: red;;; }",
            &[Node::rule(".foo", vec![decl("color", "red")])],
        );
    }

    #[test]
    fn cdo_and_cdc_tokens() {
        check("<!--", &[]);
        check("-->", &[]);
        check("<!-- -->", &[]);
        check(
            "<!--a{color:red}-->b{color:blue}",
            &[
                Node::rule("a", vec![decl("color", "red")]),
                Node::rule("b", vec![decl("color", "blue")]),
            ],
        );
        check(
            "--foo: bar; --> --baz: qux;",
            &[decl("--foo", "bar"), decl("--baz", "qux")],
        );
        check(
            "a --> b{color:blue}",
            &[Node::rule("a --> b", vec![decl("color", "blue")])],
        );
        check("--foo: <!-- -->;", &[decl("--foo", "<!-- -->")]);
        check("---a: b;", &[decl("---a", "b")]);
        assert_eq!(
            error("a { --> }"),
            "Invalid custom property, expected a value"
        );
    }

    #[test]
    fn errors_carry_messages_and_offsets() {
        let input = ".foo {\n  color: red;\n}\n.bar\n  color: blue;\n}";
        let err = parse(input).unwrap_err();
        assert_eq!(err.message(), "Missing opening {");
        assert_eq!(err.offset, input.rfind('}').unwrap());

        let input = ".foo {\n  color: red;\n}\n.bar {\n  color: blue;\n";
        let err = parse(input).unwrap_err();
        assert_eq!(err.message(), "Missing closing } at .bar");
        assert_eq!(err.offset, input.find(".bar").unwrap());

        assert_eq!(
            error(".foo {\n  content: \"Hello world!\n  font-weight: bold;\n}"),
            "Unterminated string: \"Hello world!\""
        );
        assert_eq!(
            error(".foo {\n  content: \"Hello world!;\n  font-weight: bold;\n}"),
            "Unterminated string: \"Hello world!;\""
        );
        assert_eq!(error("--foo"), "Invalid custom property, expected a value");
        assert_eq!(
            error(".foo { --bar }"),
            "Invalid custom property, expected a value"
        );
        assert_eq!(parse(".foo { --bar }").unwrap_err().offset, 7);
        assert_eq!(error(".foo { bar }"), "Invalid declaration: `bar`");
        assert_eq!(parse(".foo { bar }").unwrap_err().offset, 7);
        assert_eq!(error(".a { b: c) }"), "Missing opening (");
    }

    #[test]
    fn nesting_is_bounded() {
        let ok = "a{".repeat(MAX_CSS_NESTING_DEPTH) + &"}".repeat(MAX_CSS_NESTING_DEPTH);
        assert!(parse(&ok).is_ok());
        let deep = "a{".repeat(MAX_CSS_NESTING_DEPTH + 1);
        let err = parse(&deep).unwrap_err();
        assert_eq!(err.kind, CssSyntaxErrorKind::NestingTooDeep);
        assert_eq!(err.offset, MAX_CSS_NESTING_DEPTH * 2 + 1);
    }

    #[test]
    fn ignores_the_byte_order_mark_and_keeps_utf8() {
        check(
            "\u{FEFF}@reference 'tailwindcss';",
            &[Node::at_rule("@reference", "'tailwindcss'", vec![])],
        );
        check(
            ".é\\ü { content: 'ö'; }",
            &[Node::rule(".é\\ü", vec![decl("content", "'ö'")])],
        );
    }
}
