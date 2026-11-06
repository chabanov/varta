//! A coarse tokenizer for at-rule conditions (CSS Syntax 3 §4): enough to
//! recognise the keywords, parenthesized blocks and functions of the
//! `@supports`, `@media` and `@container` grammars. Text inside blocks is
//! kept verbatim; strings and escapes never close a block.

/// One condition token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token<'a> {
    /// A run of whitespace.
    Space,
    /// An identifier (CSS Syntax 3 §4.3.11, without escapes decoding).
    Ident(&'a str),
    /// A function: its name and the text between its parentheses.
    Function(&'a str, &'a str),
    /// A `( … )` block: the text between the parentheses.
    Block(&'a str),
    /// Anything else, up to the next whitespace, parenthesis or comma.
    Other(&'a str),
    /// A top-level comma.
    Comma,
}

/// CSS Syntax 3 §4.2 "ident-start code point" (plus `-` and `\`, which start
/// an ident sequence).
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '-' || c == '\\' || !c.is_ascii()
}

/// CSS Syntax 3 §4.2 "ident code point".
fn is_ident_char(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

/// The byte index of the `)` closing the `(` at `open`, skipping strings and
/// escapes; `None` when unbalanced.
pub(super) fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut chars = text[open..]
        .char_indices()
        .map(|(index, c)| (open + index, c));
    while let Some((index, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '"' | '\'' => {
                let quote = c;
                loop {
                    match chars.next() {
                        Some((_, '\\')) => {
                            chars.next();
                        }
                        Some((_, next)) if next == quote => break,
                        Some(_) => {}
                        None => return None,
                    }
                }
            }
            '(' => depth += 1,
            ')' => {
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

/// Tokenize `text`; `None` when a parenthesis or string is unbalanced.
pub(super) fn tokenize(text: &str) -> Option<Vec<Token<'_>>> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while let Some(c) = text[index..].chars().next() {
        if c.is_whitespace() {
            let end = text[index..]
                .find(|c: char| !c.is_whitespace())
                .map_or(text.len(), |offset| index + offset);
            tokens.push(Token::Space);
            index = end;
        } else if c == '(' {
            let close = matching_paren(text, index)?;
            tokens.push(Token::Block(&text[index + 1..close]));
            index = close + 1;
        } else if c == ')' {
            return None;
        } else if c == ',' {
            tokens.push(Token::Comma);
            index += 1;
        } else if is_ident_start(c) && !starts_number(&text[index..]) {
            let mut end = index;
            let mut chars = text[index..].char_indices();
            while let Some((offset, c)) = chars.next() {
                if c == '\\' {
                    end = index + offset + 1;
                    if let Some((offset, escaped)) = chars.next() {
                        end = index + offset + escaped.len_utf8();
                    }
                    continue;
                }
                if !is_ident_char(c) {
                    break;
                }
                end = index + offset + c.len_utf8();
            }
            if text[end..].starts_with('(') {
                let close = matching_paren(text, end)?;
                tokens.push(Token::Function(&text[index..end], &text[end + 1..close]));
                index = close + 1;
            } else {
                tokens.push(Token::Ident(&text[index..end]));
                index = end;
            }
        } else {
            let end = text[index..]
                .find(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ','))
                .map_or(text.len(), |offset| index + offset);
            tokens.push(Token::Other(&text[index..end]));
            index = end;
        }
    }
    Some(tokens)
}

/// Whether `text` starts a number (`-1`, `-.5`): a `-` followed by a digit
/// or a `.` is not an identifier (CSS Syntax 3 §4.3.10).
fn starts_number(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next() == Some('-') && chars.next().is_some_and(|c| c.is_ascii_digit() || c == '.')
}

/// The tokens without whitespace.
pub(super) fn significant<'a>(tokens: &[Token<'a>]) -> Vec<Token<'a>> {
    tokens
        .iter()
        .filter(|token| **token != Token::Space)
        .cloned()
        .collect()
}

/// Collapse whitespace runs to one space and trim (CSS Syntax 3 §4.3.1:
/// consecutive whitespace is one token).
pub(super) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_keywords_blocks_and_functions() {
        assert_eq!(
            tokenize("not (a: b) and selector(.x:is(.y))").unwrap(),
            vec![
                Token::Ident("not"),
                Token::Space,
                Token::Block("a: b"),
                Token::Space,
                Token::Ident("and"),
                Token::Space,
                Token::Function("selector", ".x:is(.y)"),
            ]
        );
        assert_eq!(
            tokenize("(content: \")\")").unwrap(),
            vec![Token::Block("content: \")\"")]
        );
        assert_eq!(
            tokenize("-1px,a").unwrap(),
            vec![Token::Other("-1px"), Token::Comma, Token::Ident("a")]
        );
        assert_eq!(tokenize("(a"), None);
        assert_eq!(tokenize("a)"), None);
        assert_eq!(collapse_whitespace("  a \n b  "), "a b");
    }
}
