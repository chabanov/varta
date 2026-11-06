//! Varta's canonical stylesheet form.
//!
//! Varta's compiled stylesheet and the Tailwind oracle's output are both
//! brought into this form before they are compared, so that equal meaning
//! gives equal bytes. Every step is defined by a W3C specification:
//!
//! 1. Comments are removed (CSS Syntax 3 §4.3.2: comments are not tokens)
//!    and at-rule names are lowercased (CSS Syntax 3 §4.3.4 / §9: at-keyword
//!    names are ASCII case-insensitive).
//! 2. `@supports` is resolved statically ([`supports`], CSS Conditional 3 §2
//!    and Conditional 4 `selector()`) against a [`Supports`] answer: a true
//!    block is replaced by its contents in place, a false one is dropped.
//! 3. Style rule nesting is flattened ([`nesting`], CSS Nesting 1 §2–§3):
//!    no `&` and no style rule inside a style rule remain.
//! 4. Media and container query range syntax is rewritten to `min-`/`max-`
//!    features with canonical spacing, and directly nested single `@media`
//!    conditions are merged ([`media`], Media Queries 4 §2.4.4 and §3,
//!    Containment 3 §6, Conditional 3 §3).
//!
//! `@supports` is resolved before flattening: inside a style rule, the
//! declarations of a true `@supports` block then stay among the parent's
//! declarations, in source order, which is what the nested group rule means
//! once its condition is known (CSS Nesting 1 §2.2). The result is printed
//! with [`crate::printer::to_css`]; canonicalizing a printed canonical
//! stylesheet again gives the same tree.
//!
//! Work is bounded: trees deeper than [`MAX_CANONICAL_DEPTH`] and
//! conditions nested deeper than [`MAX_CONDITION_DEPTH`] are rejected with a
//! typed [`CanonicalError`].
use crate::ast::Node;

mod media;
mod nesting;
mod supports;
mod tokens;

/// The deepest tree [`canonicalize`] accepts (the parser's own bound).
pub const MAX_CANONICAL_DEPTH: usize = crate::css_parser::MAX_CSS_NESTING_DEPTH;

/// The deepest nesting of parentheses in an `@supports`, `@media` or
/// `@container` condition.
pub const MAX_CONDITION_DEPTH: usize = 64;

/// Why a stylesheet has no canonical form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanonicalError {
    #[error("the stylesheet nests deeper than {MAX_CANONICAL_DEPTH} levels")]
    TooDeep,
    #[error("a condition nests deeper than {MAX_CONDITION_DEPTH} parentheses: `{0}`")]
    ConditionTooDeep(String),
    #[error("`{0}` cannot be nested in a style rule (CSS Nesting 1 §3.2)")]
    NestedAtRule(String),
    #[error("{0} nodes are compiler markers, not CSS; run `optimize_ast` first")]
    Marker(&'static str),
}

/// What the target user agent supports, for static `@supports` resolution
/// (CSS Conditional 3 §2.1, Conditional 4 §2.1).
pub trait Supports {
    /// Whether the agent accepts `property: value` (`(<declaration>)`).
    fn declaration(&self, property: &str, value: &str) -> bool;
    /// Whether the agent accepts the complex selector
    /// (`selector(<complex-selector>)`).
    fn selector(&self, selector: &str) -> bool;
}

/// Varta's own answer: a declaration or selector is supported exactly when
/// Varta's stylesheet grammar accepts it (CSS Conditional 3 §2.1: a UA
/// supports a declaration when it would accept it).
#[derive(Debug, Clone, Copy, Default)]
pub struct VartaSupports;

impl Supports for VartaSupports {
    fn declaration(&self, property: &str, value: &str) -> bool {
        // One rule with exactly one declaration: a value cannot smuggle a
        // second declaration or rule into the probe.
        varta_common::view::stylesheet::parse(&format!(".x{{{property}:{value}}}")).is_ok_and(
            |rules| {
                matches!(rules.as_slice(), [rule] if rule.declarations.len() == 1 && rule.media.is_empty())
            },
        )
    }

    fn selector(&self, selector: &str) -> bool {
        varta_common::view::stylesheet::parse(&format!("{selector}{{}}")).is_ok_and(|rules| {
            matches!(rules.as_slice(), [rule] if rule.declarations.is_empty() && rule.media.is_empty())
        })
    }
}

/// Bring a parsed stylesheet into canonical form.
///
/// # Errors
/// [`CanonicalError::TooDeep`] beyond [`MAX_CANONICAL_DEPTH`],
/// [`CanonicalError::ConditionTooDeep`] beyond [`MAX_CONDITION_DEPTH`],
/// [`CanonicalError::Marker`] for context or at-root nodes, and
/// [`CanonicalError::NestedAtRule`] for an at-rule that CSS Nesting 1 does
/// not allow inside a style rule.
pub fn canonicalize(ast: Vec<Node>, supports: &dyn Supports) -> Result<Vec<Node>, CanonicalError> {
    check_depth(&ast)?;
    let ast = remove_comments(ast, 0)?;
    let ast = supports::resolve(ast, supports, 0)?;
    let ast = nesting::flatten(ast)?;
    media::normalize(ast, 0)
}

/// Reject trees deeper than [`MAX_CANONICAL_DEPTH`] (explicit stack), so the
/// recursive steps below are bounded.
fn check_depth(ast: &[Node]) -> Result<(), CanonicalError> {
    let mut stack: Vec<(&Node, usize)> = ast.iter().map(|node| (node, 1)).collect();
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_CANONICAL_DEPTH {
            return Err(CanonicalError::TooDeep);
        }
        if let Some(children) = node.children() {
            stack.extend(children.iter().map(|child| (child, depth + 1)));
        }
    }
    Ok(())
}

/// Step 1: drop comments, lowercase at-rule names, reject compiler markers.
fn remove_comments(nodes: Vec<Node>, depth: usize) -> Result<Vec<Node>, CanonicalError> {
    if depth > MAX_CANONICAL_DEPTH {
        return Err(CanonicalError::TooDeep);
    }
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Node::Comment(_) => {}
            Node::Decl { .. } => out.push(node),
            Node::Rule { selector, nodes } => {
                out.push(Node::rule(selector, remove_comments(nodes, depth + 1)?));
            }
            Node::AtRule {
                name,
                params,
                nodes,
            } => out.push(Node::at_rule(
                name.to_ascii_lowercase(),
                params,
                remove_comments(nodes, depth + 1)?,
            )),
            Node::Context { .. } => return Err(CanonicalError::Marker("context")),
            Node::AtRoot(_) => return Err(CanonicalError::Marker("at-root")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_parser::parse;
    use crate::printer::to_css;

    /// A fixed answer: declarations whose property is listed, selectors
    /// without `:has(`.
    struct Fixed(&'static [&'static str]);

    impl Supports for Fixed {
        fn declaration(&self, property: &str, _value: &str) -> bool {
            self.0.contains(&property)
        }
        fn selector(&self, selector: &str) -> bool {
            !selector.contains(":has(")
        }
    }

    fn canonical(input: &str, supports: &dyn Supports) -> String {
        to_css(&canonicalize(parse(input).unwrap(), supports).unwrap())
    }

    #[test]
    fn removes_comments_and_lowercases_at_rule_names() {
        // CSS Syntax 3 §4.3.2 (comments) and §4.3.4 (at-keywords).
        assert_eq!(
            canonical(
                "/*! License */ @MEDIA print { .a { color: red; } }",
                &Fixed(&[])
            ),
            "@media print {\n  .a {\n    color: red;\n  }\n}\n"
        );
    }

    #[test]
    fn rejects_markers_and_deep_trees() {
        let marker = vec![Node::at_root(vec![])];
        assert_eq!(
            canonicalize(marker, &Fixed(&[])),
            Err(CanonicalError::Marker("at-root"))
        );
        let mut node = Node::decl("color", "red");
        for _ in 0..=MAX_CANONICAL_DEPTH {
            node = Node::rule(".a", vec![node]);
        }
        assert_eq!(
            canonicalize(vec![node], &Fixed(&[])),
            Err(CanonicalError::TooDeep)
        );
    }

    #[test]
    fn steps_compose_and_reach_a_fixed_point() {
        let input = "
            .bg {
              --pos: 0deg;
              @supports (background-image: linear-gradient(in lab, red, red)) {
                --pos: 0deg in oklab;
              }
              background-image: linear-gradient(var(--pos));
            }
            .container {
              width: 100%;
              @media (width >= 40rem) {
                max-width: 40rem;
              }
            }
            @media not (width < 40rem) {
              @media (hover: hover) {
                .x:hover { & > .y { color: red; } }
              }
            }
        ";
        let supports = Fixed(&["background-image"]);
        let once = canonical(input, &supports);
        assert_eq!(
            once,
            ".bg {\n  --pos: 0deg;\n  --pos: 0deg in oklab;\n  background-image: linear-gradient(var(--pos));\n}\n\
             .container {\n  width: 100%;\n}\n\
             @media (min-width: 40rem) {\n  .container {\n    max-width: 40rem;\n  }\n}\n\
             @media (min-width: 40rem) and (hover: hover) {\n  .x:hover > .y {\n    color: red;\n  }\n}\n"
        );
        assert_eq!(canonical(&once, &supports), once);
    }

    #[test]
    fn varta_supports_asks_the_stylesheet_grammar() {
        let supports = VartaSupports;
        assert!(supports.declaration("display", "grid"));
        assert!(supports.declaration("--a", "1"));
        assert!(!supports.declaration("display", "grid;display:flex"));
        assert!(!supports.declaration("background-image", "linear-gradient(in lab, red, red)"));
        assert!(!supports.declaration("not-a-property", "1"));
        assert!(supports.selector(".a:hover"));
        assert!(!supports.selector(".a{}.b"));
    }
}
