//! Static `@supports` resolution (CSS Conditional 3 §2, Conditional 4 §2).
//!
//! The prelude is parsed with the `<supports-condition>` grammar
//! (Conditional 3 §2.1):
//!
//! ```text
//! <supports-condition> = not <supports-in-parens>
//!                      | <supports-in-parens> [ and <supports-in-parens> ]*
//!                      | <supports-in-parens> [ or <supports-in-parens> ]*
//! <supports-in-parens> = ( <supports-condition> ) | <supports-feature> | <general-enclosed>
//! <supports-feature>   = <supports-decl> | <supports-selector-fn>
//! <supports-decl>      = ( <declaration> )
//! <supports-selector-fn> = selector( <complex-selector> )      (Conditional 4)
//! ```
//!
//! A `<general-enclosed>` (any other parenthesized text or function) is
//! false, and a prelude that does not match the grammar makes the rule
//! invalid, which is also false (Conditional 3 §2: an invalid `@supports`
//! rule is ignored). A true rule is replaced by its contents in place, a
//! false one is dropped with its contents.
use super::tokens::{Token, significant, tokenize};
use super::{CanonicalError, MAX_CANONICAL_DEPTH, MAX_CONDITION_DEPTH, Supports};
use crate::ast::Node;
use crate::css_parser::js_trim;
use crate::utils::segment;

const AT_SUPPORTS: &str = "@supports";
const NOT: &str = "not";
const AND: &str = "and";
const OR: &str = "or";
const SELECTOR_FUNCTION: &str = "selector";
/// A trailing `!important` is part of `<declaration>` (CSS Syntax 3 §5.4.6)
/// but not of the property value whose support is asked.
const IMPORTANT: &str = "!important";

/// A parsed `<supports-condition>`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Condition {
    Not(Box<Condition>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
    Declaration { property: String, value: String },
    Selector(String),
    GeneralEnclosed,
}

impl Condition {
    fn evaluate(&self, supports: &dyn Supports) -> bool {
        match self {
            Condition::Not(inner) => !inner.evaluate(supports),
            Condition::And(all) => all.iter().all(|inner| inner.evaluate(supports)),
            Condition::Or(any) => any.iter().any(|inner| inner.evaluate(supports)),
            Condition::Declaration { property, value } => supports.declaration(property, value),
            // A `<complex-selector>` is one selector: a list does not match
            // the grammar, so it is a `<general-enclosed>` (false).
            Condition::Selector(selector) => {
                segment(selector, ',').len() == 1 && supports.selector(selector)
            }
            Condition::GeneralEnclosed => false,
        }
    }
}

/// Parse a `<supports-condition>`; `Ok(None)` when it does not match the
/// grammar.
fn parse_condition(text: &str, depth: usize) -> Result<Option<Condition>, CanonicalError> {
    if depth > MAX_CONDITION_DEPTH {
        return Err(CanonicalError::ConditionTooDeep(text.to_owned()));
    }
    let Some(tokens) = tokenize(text) else {
        return Ok(None);
    };
    let tokens = significant(&tokens);
    let Some((first, rest)) = tokens.split_first() else {
        return Ok(None);
    };
    if let Token::Ident(keyword) = first
        && keyword.eq_ignore_ascii_case(NOT)
    {
        let [operand] = rest else {
            return Ok(None);
        };
        return Ok(in_parens(operand, depth)?.map(|inner| Condition::Not(Box::new(inner))));
    }
    let Some(head) = in_parens(first, depth)? else {
        return Ok(None);
    };
    if rest.is_empty() {
        return Ok(Some(head));
    }
    // `a and b or c` mixes operators without parentheses: invalid.
    let Some(Token::Ident(operator)) = rest.first() else {
        return Ok(None);
    };
    let is_and = operator.eq_ignore_ascii_case(AND);
    if !is_and && !operator.eq_ignore_ascii_case(OR) {
        return Ok(None);
    }
    let mut operands = vec![head];
    for pair in rest.chunks(2) {
        let [Token::Ident(keyword), operand] = pair else {
            return Ok(None);
        };
        if !keyword.eq_ignore_ascii_case(operator) {
            return Ok(None);
        }
        let Some(operand) = in_parens(operand, depth)? else {
            return Ok(None);
        };
        operands.push(operand);
    }
    Ok(Some(if is_and {
        Condition::And(operands)
    } else {
        Condition::Or(operands)
    }))
}

/// Parse a `<supports-in-parens>`.
fn in_parens(token: &Token<'_>, depth: usize) -> Result<Option<Condition>, CanonicalError> {
    match token {
        Token::Block(inner) => {
            if let Some(condition) = parse_condition(inner, depth + 1)? {
                return Ok(Some(condition));
            }
            Ok(Some(
                declaration(inner).unwrap_or(Condition::GeneralEnclosed),
            ))
        }
        Token::Function(name, inner) if name.eq_ignore_ascii_case(SELECTOR_FUNCTION) => {
            Ok(Some(Condition::Selector(js_trim(inner).to_owned())))
        }
        Token::Function(..) => Ok(Some(Condition::GeneralEnclosed)),
        Token::Space | Token::Ident(_) | Token::Other(_) | Token::Comma => Ok(None),
    }
}

/// A `<declaration>`: `<ident> : <value>` with a non-empty value, or an
/// empty one for a custom property (CSS Variables 1 §2).
fn declaration(text: &str) -> Option<Condition> {
    let (property, value) = text.split_once(':')?;
    let property = js_trim(property);
    let tokens = tokenize(property)?;
    if !matches!(tokens.as_slice(), [Token::Ident(_)]) {
        return None;
    }
    let mut value = js_trim(value);
    if value.len() >= IMPORTANT.len() {
        let split = value.len() - IMPORTANT.len();
        if value.is_char_boundary(split) && value[split..].eq_ignore_ascii_case(IMPORTANT) {
            value = js_trim(&value[..split]);
        }
    }
    if value.is_empty() && !property.starts_with("--") {
        return None;
    }
    Some(Condition::Declaration {
        property: property.to_owned(),
        value: value.to_owned(),
    })
}

/// Evaluate an `@supports` prelude; an invalid prelude is false.
fn evaluate(prelude: &str, supports: &dyn Supports) -> Result<bool, CanonicalError> {
    Ok(parse_condition(prelude, 0)?.is_some_and(|condition| condition.evaluate(supports)))
}

/// Replace every true `@supports` rule by its contents and drop every false
/// one, at any depth.
pub(super) fn resolve(
    nodes: Vec<Node>,
    supports: &dyn Supports,
    depth: usize,
) -> Result<Vec<Node>, CanonicalError> {
    if depth > MAX_CANONICAL_DEPTH {
        return Err(CanonicalError::TooDeep);
    }
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Node::AtRule {
                name,
                params,
                nodes,
            } if name == AT_SUPPORTS => {
                if evaluate(&params, supports)? {
                    out.extend(resolve(nodes, supports, depth + 1)?);
                }
            }
            Node::AtRule {
                name,
                params,
                nodes,
            } => {
                out.push(Node::at_rule(
                    name,
                    params,
                    resolve(nodes, supports, depth + 1)?,
                ));
            }
            Node::Rule { selector, nodes } => {
                out.push(Node::rule(selector, resolve(nodes, supports, depth + 1)?));
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Supports `display`, `color` and selectors without `:has(`.
    struct Fixed;

    impl Supports for Fixed {
        fn declaration(&self, property: &str, value: &str) -> bool {
            matches!(property, "display" | "color" | "--x") && !value.contains("in lab")
        }
        fn selector(&self, selector: &str) -> bool {
            !selector.contains(":has(")
        }
    }

    fn eval(prelude: &str) -> bool {
        evaluate(prelude, &Fixed).unwrap()
    }

    #[test]
    fn declarations_and_selectors() {
        // Conditional 3 §2.1 `( <declaration> )`, Conditional 4 `selector()`.
        assert!(eval("(display:grid)"));
        assert!(eval("( display : grid !important )"));
        assert!(eval("(--x:)"));
        assert!(!eval("(width: 1px)"));
        assert!(!eval("(color: lab(in lab))"));
        assert!(eval("selector(.a > .b)"));
        assert!(!eval("selector(.a:has(.b))"));
        // A list is not a `<complex-selector>`.
        assert!(!eval("selector(.a, .b)"));
    }

    #[test]
    fn logic_follows_the_grammar() {
        // Conditional 3 §2.1: not / and / or, parentheses, no mixing.
        assert!(eval("not (width: 1px)"));
        assert!(eval("(display: grid) and (color: red)"));
        assert!(!eval("(display: grid) and (width: 1px)"));
        assert!(eval("(width: 1px) or (color: red)"));
        assert!(eval("((width: 1px) or (color: red)) and (display: grid)"));
        assert!(eval("NOT (width: 1px)"));
        assert!(!eval("(display: grid) and (color: red) or (display: grid)"));
        assert!(!eval("not(display: grid)"));
        assert!(!eval("display: grid"));
        assert!(!eval(""));
    }

    #[test]
    fn general_enclosed_is_false() {
        // Conditional 3 §2.1: `<general-enclosed>` evaluates to false.
        assert!(!eval("(not a declaration)"));
        assert!(!eval("font-tech(color-COLRv1)"));
        assert!(eval("not (unknown stuff)"));
    }

    #[test]
    fn deep_conditions_are_rejected() {
        let deep = "(".repeat(MAX_CONDITION_DEPTH + 2) + &")".repeat(MAX_CONDITION_DEPTH + 2);
        assert!(matches!(
            evaluate(&deep, &Fixed),
            Err(CanonicalError::ConditionTooDeep(_))
        ));
    }

    #[test]
    fn resolves_in_place() {
        let rule = |nodes| Node::rule(".a", nodes);
        let ast = vec![rule(vec![
            Node::decl("--p", "0deg"),
            Node::at_rule("@supports", "(color: red)", vec![Node::decl("--p", "1deg")]),
            Node::at_rule("@supports", "(width: 1px)", vec![Node::decl("--p", "2deg")]),
            Node::decl("color", "var(--p)"),
        ])];
        assert_eq!(
            resolve(ast, &Fixed, 0).unwrap(),
            vec![rule(vec![
                Node::decl("--p", "0deg"),
                Node::decl("--p", "1deg"),
                Node::decl("color", "var(--p)"),
            ])]
        );
    }
}
