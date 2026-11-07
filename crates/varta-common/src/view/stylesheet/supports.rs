//! `@supports` conditions (CSS Conditional 3 §6.1, `selector()` of
//! Conditional 4 §6.1), decided by this engine's own grammars: a
//! declaration is supported when the engine admits it, a selector when it is
//! in the selector subset. A `<general-enclosed>` is false.
use super::super::syntax::{self, Spanned, Token};

#[allow(unused_imports)]
use crate::view::prelude::*;

type Parsed<T> = Result<T, usize>;

struct Parser<'a> {
    tokens: &'a [Spanned],
    at: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|t| &t.token)
    }
    fn skip(&mut self) {
        while self.peek() == Some(&Token::Whitespace) {
            self.at += 1;
        }
    }
    fn keyword(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(found)) if found.eq_ignore_ascii_case(word))
    }
    /// The index just past the parenthesis closing the block opened at `at`.
    fn close(&self, at: usize) -> Parsed<usize> {
        let mut depth = 0usize;
        for (index, spanned) in self.tokens.iter().enumerate().skip(at) {
            match spanned.token {
                Token::OpenParen | Token::Function(_) | Token::OpenSquare | Token::OpenCurly => {
                    depth += 1;
                }
                Token::CloseParen | Token::CloseSquare | Token::CloseCurly => {
                    depth = depth.checked_sub(1).ok_or(index)?;
                    if depth == 0 {
                        return Ok(index + 1);
                    }
                }
                _ => {}
            }
        }
        Err(self.tokens.len())
    }
    /// `<supports-condition>`.
    fn condition(&mut self) -> Parsed<bool> {
        self.skip();
        if self.keyword("not") {
            self.at += 1;
            self.skip();
            return Ok(!self.in_parens()?);
        }
        let mut value = self.in_parens()?;
        let mut joiner: Option<bool> = None;
        loop {
            let save = self.at;
            self.skip();
            let and = self.keyword("and");
            if !and && !self.keyword("or") {
                self.at = save;
                return Ok(value);
            }
            if joiner.is_some_and(|j| j != and) {
                return Err(self.at);
            }
            joiner = Some(and);
            self.at += 1;
            self.skip();
            let next = self.in_parens()?;
            value = if and { value && next } else { value || next };
        }
    }
    /// `<supports-in-parens>`.
    fn in_parens(&mut self) -> Parsed<bool> {
        let start = self.at;
        match self.peek() {
            Some(Token::OpenParen) => {
                let end = self.close(start)?;
                let inner = &self.tokens[start + 1..end - 1];
                self.at = end;
                // A nested condition, a declaration, or general-enclosed.
                if self.depth >= super::media::MAX_CONDITION_DEPTH {
                    return Err(start);
                }
                let mut nested = Parser {
                    tokens: inner,
                    at: 0,
                    depth: self.depth + 1,
                };
                nested.skip();
                if matches!(nested.peek(), Some(Token::OpenParen)) || nested.keyword("not") {
                    let value = nested.condition()?;
                    nested.skip();
                    return if nested.at == inner.len() {
                        Ok(value)
                    } else {
                        Err(start + 1 + nested.at)
                    };
                }
                Ok(declaration(inner))
            }
            Some(Token::Function(name)) => {
                let selector = name.eq_ignore_ascii_case("selector");
                let end = self.close(start)?;
                let inner = &self.tokens[start + 1..end - 1];
                self.at = end;
                Ok(selector && compound_selector(inner))
            }
            _ => Err(start),
        }
    }
}

fn trimmed(tokens: &[Spanned]) -> &[Spanned] {
    let first = tokens
        .iter()
        .position(|t| t.token != Token::Whitespace)
        .unwrap_or(tokens.len());
    let last = tokens
        .iter()
        .rposition(|t| t.token != Token::Whitespace)
        .map_or(first, |last| last + 1);
    &tokens[first..last.max(first)]
}

/// `( <declaration> )`: supported when the engine admits it.
fn declaration(tokens: &[Spanned]) -> bool {
    let tokens = trimmed(tokens);
    let Some(Token::Ident(name)) = tokens.first().map(|t| &t.token) else {
        return false;
    };
    let Some(colon) = tokens.iter().position(|t| t.token == Token::Colon) else {
        return false;
    };
    if tokens[1..colon]
        .iter()
        .any(|t| t.token != Token::Whitespace)
    {
        return false;
    }
    let value = syntax::serialize(trimmed(&tokens[colon + 1..]));
    super::declaration(name, &value).is_ok_and(|declared| !declared.is_empty())
}

/// `selector( … )`: a compound selector of the subset.
fn compound_selector(tokens: &[Spanned]) -> bool {
    // Conditional 4 §6.1: `selector(<complex-selector>)`, one selector.
    let text = syntax::serialize(trimmed(tokens));
    let tokens = syntax::tokenize(&text);
    super::selector::parse_list(&tokens, &text).is_ok_and(|list| {
        list.len() == 1
            && list[0]
                .pseudo_element
                // Unknown `-webkit-` pseudo-elements are valid but not
                // supported (Selectors 4 Appendix B).
                .is_none_or(|element| {
                    element.rendered() && element != super::selector::PseudoElement::UnknownWebkit
                })
    })
}

/// Evaluate an `@supports` prelude; the error is the index of the token
/// where it stops being a `<supports-condition>`.
pub(super) fn evaluate(tokens: &[Spanned]) -> Result<bool, usize> {
    let mut parser = Parser {
        tokens,
        at: 0,
        depth: 0,
    };
    let value = parser.condition()?;
    parser.skip();
    if parser.at == tokens.len() {
        Ok(value)
    } else {
        Err(parser.at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn holds(text: &str) -> Result<bool, usize> {
        evaluate(&syntax::tokenize(text))
    }
    #[test]
    fn conditions_follow_conditional_3() {
        assert_eq!(holds("(color: color-mix(in lab, red, red))"), Ok(true));
        assert_eq!(holds("(color: color(display-p3 1 0 0))"), Ok(false));
        assert_eq!(holds("(color: color-mix(in lab, #f00, #f00))"), Ok(true));
        assert_eq!(
            holds("(display: grid) and (not (display: inline-grid))"),
            Ok(true)
        );
        assert_eq!(
            holds("(-webkit-hyphens: none) or (margin-trim: block)"),
            Ok(false)
        );
        assert_eq!(holds("not (backdrop-filter: blur(1px))"), Ok(false));
        assert_eq!(holds("not (backdrop-filter: brightness(2))"), Ok(true));
        assert_eq!(holds("(--x: 1px)"), Ok(true));
        assert_eq!(holds("selector(.a:hover)"), Ok(true));
        assert_eq!(holds("selector(.a > .b)"), Ok(true));
        assert_eq!(holds("selector(.a, .b)"), Ok(false));
        assert_eq!(holds("selector(.a::after)"), Ok(true));
        assert_eq!(holds("selector(.a::marker)"), Ok(false));
        assert_eq!(holds("selector(::-webkit-search-decoration)"), Ok(false));
        assert_eq!(holds("(tab-size: 4)"), Ok(false));
        assert_eq!(holds("(-webkit-tap-highlight-color: red)"), Ok(false));
        assert_eq!(holds("selector(::file-selector-button)"), Ok(true));
        assert_eq!(holds("selector(.a::frob)"), Ok(false));
        assert_eq!(holds("font-tech(color-COLRv1)"), Ok(false));
        assert_eq!(holds("(width > 1px)"), Ok(false));
        assert!(holds("(a: b) and (c: d) or (e: f)").is_err());
        assert!(holds("display: grid").is_err());
        assert!(holds("(display: grid) junk").is_err());
        let deep = format!("{}(display: grid){}", "(".repeat(40), ")".repeat(40));
        assert!(holds(&deep).is_err());
    }
}
