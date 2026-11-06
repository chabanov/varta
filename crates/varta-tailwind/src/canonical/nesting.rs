//! Style rule flattening (CSS Nesting 1).
//!
//! * §2 (nesting style rules): a nested rule's selector is a relative
//!   selector; without `&` it is relative to the parent with a descendant
//!   combinator (or the combinator it starts with).
//! * §3 (the nesting selector): `&` stands for the parent's selector list
//!   with `:is()` semantics. The substitution is
//!   [`crate::optimize::substitute_nesting_selector`], which keeps `:is(…)`
//!   unless dropping it is provably equivalent (a single parent selector
//!   whose matching and specificity do not change). A compound that writes
//!   `&` before a type or universal selector (`&div`, valid nesting syntax)
//!   is first reordered to the equivalent `div&` (§3: the nesting selector
//!   may appear anywhere in a compound selector).
//! * §2.2 (mixing rules and declarations): declarations that follow a
//!   nested rule form a nested declarations rule, which matches the
//!   parent's elements with the parent's specificity. It is emitted as a
//!   separate rule with the parent's exact selector after the nested rule,
//!   so the cascade order is kept.
//! * §3.2 (nested group rules): `@media`, `@supports`, `@container`,
//!   `@layer`, `@scope` and `@starting-style` inside a style rule wrap a copy
//!   of the parent rule holding their declarations; their nested rules are
//!   relative to the parent. Any other at-rule inside a style rule is
//!   invalid there and rejected.
//! * `@scope` (CSS Cascade 6 §2.5, CSS Nesting 1 §3.3): a nested `@scope`'s
//!   `<scope-start>` is relative to the parent rule, `&` in `<scope-end>` and
//!   in the scope's own rules is the scoping root (`:where(:scope)`), and its
//!   bare declarations apply to `:where(:scope)`.
//! * A top-level `&` is the scoping root `:scope` (§3).
//!
//! Empty style rules and empty group rules other than `@layer` (which
//! orders layers) have no effect and are dropped.
use super::{CanonicalError, MAX_CANONICAL_DEPTH};
use crate::ast::Node;
use crate::css_parser::js_trim;
use crate::optimize::substitute_nesting_selector;
use crate::selector_parser::{self as selectors, SelectorNode};
use crate::utils::segment;
use crate::value_parser::{self, ValueNode};

/// At-rules allowed inside style rules (CSS Nesting 1 §3.2).
const NESTED_GROUP_RULES: &[&str] = &[
    "@media",
    "@supports",
    "@container",
    "@layer",
    "@scope",
    "@starting-style",
];
const AT_SCOPE: &str = "@scope";
const AT_LAYER: &str = "@layer";
const NESTING_SELECTOR: &str = "&";
const SCOPE_SELECTOR: &str = ":scope";
const WHERE_SCOPE_SELECTOR: &str = ":where(:scope)";
const SCOPE_END_KEYWORD: &str = "to";
const LIST_SEPARATOR: &str = ", ";

/// Where a list of nodes sits.
#[derive(Debug, Clone, Copy)]
struct Place<'a> {
    /// The resolved selector of the enclosing style rule.
    parent: Option<&'a str>,
    /// Directly inside an `@scope` body (no style rule in between).
    in_scope: bool,
}

/// Flatten every style rule of the stylesheet.
pub(super) fn flatten(nodes: Vec<Node>) -> Result<Vec<Node>, CanonicalError> {
    flatten_list(
        nodes,
        Place {
            parent: None,
            in_scope: false,
        },
        0,
    )
}

fn flush(pending: &mut Vec<Node>, place: Place<'_>, out: &mut Vec<Node>) {
    if pending.is_empty() {
        return;
    }
    let selector = match place.parent {
        Some(parent) => parent,
        None => WHERE_SCOPE_SELECTOR,
    };
    out.push(Node::rule(selector, std::mem::take(pending)));
}

fn flatten_list(
    nodes: Vec<Node>,
    place: Place<'_>,
    depth: usize,
) -> Result<Vec<Node>, CanonicalError> {
    if depth > MAX_CANONICAL_DEPTH {
        return Err(CanonicalError::TooDeep);
    }
    let wraps_declarations = place.parent.is_some() || place.in_scope;
    let mut out = Vec::new();
    let mut pending = Vec::new();
    for node in nodes {
        match node {
            Node::Decl { .. } if wraps_declarations => pending.push(node),
            Node::Decl { .. } | Node::Comment(_) => out.push(node),
            Node::Rule { selector, nodes } => {
                flush(&mut pending, place, &mut out);
                let resolved = resolve_selector(&selector, place);
                let inner = Place {
                    parent: Some(&resolved),
                    in_scope: false,
                };
                out.extend(flatten_list(nodes, inner, depth + 1)?);
            }
            Node::AtRule {
                name,
                params,
                nodes,
            } if NESTED_GROUP_RULES.contains(&name.as_str()) => {
                flush(&mut pending, place, &mut out);
                let (params, inner) = if name == AT_SCOPE {
                    let params = scope_prelude(&params, place.parent);
                    let inner = Place {
                        parent: None,
                        in_scope: true,
                    };
                    (params, flatten_list(nodes, inner, depth + 1)?)
                } else {
                    (params, flatten_list(nodes, place, depth + 1)?)
                };
                if inner.is_empty() && name != AT_LAYER {
                    continue;
                }
                out.push(Node::at_rule(name, params, inner));
            }
            Node::AtRule { name, .. } if place.parent.is_some() => {
                return Err(CanonicalError::NestedAtRule(name));
            }
            Node::AtRule { .. } => out.push(node),
            Node::Context { .. } => return Err(CanonicalError::Marker("context")),
            Node::AtRoot(_) => return Err(CanonicalError::Marker("at-root")),
        }
    }
    flush(&mut pending, place, &mut out);
    Ok(out)
}

/// The absolute selector of a style rule.
fn resolve_selector(selector: &str, place: Place<'_>) -> String {
    match place.parent {
        Some(parent) => segment(selector, ',')
            .iter()
            .map(|part| desugar(js_trim(part), parent))
            .collect::<Vec<_>>()
            .join(LIST_SEPARATOR),
        None if selector.contains(NESTING_SELECTOR) => {
            let replacement = if place.in_scope {
                WHERE_SCOPE_SELECTOR
            } else {
                SCOPE_SELECTOR
            };
            let mut ast = selectors::parse(selector);
            if replace_nesting(&mut ast, replacement) {
                selectors::to_css(&ast, false)
            } else {
                selector.to_owned()
            }
        }
        None => selector.to_owned(),
    }
}

/// One relative selector of a nested rule, made absolute.
fn desugar(selector: &str, parent: &str) -> String {
    if !selector.contains(NESTING_SELECTOR) {
        return substitute_nesting_selector(selector, parent);
    }
    let mut ast = selectors::parse(selector);
    let reordered = type_selectors_first(&mut ast);
    let selector = if reordered {
        selectors::to_css(&ast, false)
    } else {
        selector.to_owned()
    };
    substitute_nesting_selector(&selector, parent)
}

/// Move a type or universal selector written after `&` to the front of its
/// compound (`&div` → `div&`); true when something moved.
fn type_selectors_first(nodes: &mut [SelectorNode]) -> bool {
    let mut moved = false;
    for node in nodes {
        if let SelectorNode::Compound(parts) = node
            && parts.iter().any(selectors::is_nesting_selector)
            && let Some(position) = parts.iter().position(|part| {
                selectors::is_type_selector(part) || selectors::is_universal_selector(part)
            })
            && position > 0
        {
            let part = parts.remove(position);
            parts.insert(0, part);
            moved = true;
        }
        if let Some(children) = node.children_mut() {
            moved |= type_selectors_first(children);
        }
    }
    moved
}

/// Replace every `&` simple selector; true when one was found.
fn replace_nesting(nodes: &mut [SelectorNode], replacement: &str) -> bool {
    let mut changed = false;
    for node in nodes {
        match node {
            SelectorNode::Selector(value) if value == NESTING_SELECTOR => {
                *value = replacement.to_owned();
                changed = true;
            }
            _ => {
                if let Some(children) = node.children_mut() {
                    changed |= replace_nesting(children, replacement);
                }
            }
        }
    }
    changed
}

/// Resolve an `@scope` prelude: `<scope-start>` relative to the parent rule
/// (or `&` as `:scope` at the top level), `&` in `<scope-end>` as the scoping
/// root. An unchanged prelude is kept verbatim.
fn scope_prelude(params: &str, parent: Option<&str>) -> String {
    if parent.is_none() && !params.contains(NESTING_SELECTOR) {
        return params.to_owned();
    }
    let mut ast = value_parser::parse(params);
    let mut is_scope_end = false;
    let mut changed = false;
    for node in &mut ast {
        if matches!(node, ValueNode::Word(word) if word == SCOPE_END_KEYWORD) {
            is_scope_end = true;
            continue;
        }
        let ValueNode::Function { name, nodes } = node else {
            continue;
        };
        if !name.is_empty() {
            continue;
        }
        let selector = value_parser::to_css(nodes);
        let resolved = match (is_scope_end, parent) {
            (false, Some(parent)) => segment(&selector, ',')
                .iter()
                .map(|part| desugar(js_trim(part), parent))
                .collect::<Vec<_>>()
                .join(LIST_SEPARATOR),
            (false, None) if selector.contains(NESTING_SELECTOR) => resolve_selector(
                &selector,
                Place {
                    parent: None,
                    in_scope: false,
                },
            ),
            (true, _) if selector.contains(NESTING_SELECTOR) => {
                substitute_nesting_selector(&selector, WHERE_SCOPE_SELECTOR)
            }
            _ => continue,
        };
        *nodes = vec![ValueNode::Word(resolved)];
        changed = true;
    }
    if changed {
        value_parser::to_css(&ast)
    } else {
        params.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_parser::parse;
    use crate::printer::to_css;

    fn flat(input: &str) -> String {
        to_css(&flatten(parse(input).unwrap()).unwrap())
    }

    #[test]
    fn nested_rules_are_relative_to_the_parent() {
        // CSS Nesting 1 §2: relative selectors; §3: `&` with `:is()` semantics.
        assert_eq!(
            flat(".a { .b { color: red; } > .c { color: blue; } }"),
            ".a .b {\n  color: red;\n}\n.a > .c {\n  color: blue;\n}\n"
        );
        assert_eq!(
            flat(".a, .b { &:hover { color: red; } .c & { color: blue; } }"),
            ":is(.a, .b):hover {\n  color: red;\n}\n.c :is(.a, .b) {\n  color: blue;\n}\n"
        );
        assert_eq!(
            flat(".a .b { .c > & { color: red; } }"),
            ".c > :is(.a .b) {\n  color: red;\n}\n"
        );
    }

    #[test]
    fn nesting_selector_before_a_type_selector() {
        // CSS Nesting 1 §3: `&div` is valid and equals `div&`.
        assert_eq!(
            flat(".a { &div { color: red; } }"),
            "div.a {\n  color: red;\n}\n"
        );
    }

    #[test]
    fn declarations_after_nested_rules_keep_their_order() {
        // CSS Nesting 1 §2.2: nested declarations rules.
        assert_eq!(
            flat(".a { color: red; &:hover { color: blue; } background: green; }"),
            ".a {\n  color: red;\n}\n.a:hover {\n  color: blue;\n}\n.a {\n  background: green;\n}\n"
        );
    }

    #[test]
    fn nested_group_rules_wrap_the_parent() {
        // CSS Nesting 1 §3.2: nested conditional group rules.
        assert_eq!(
            flat(".a { width: 1px; @media (x) { width: 2px; .b { color: red; } } }"),
            ".a {\n  width: 1px;\n}\n@media (x) {\n  .a {\n    width: 2px;\n  }\n  .a .b {\n    color: red;\n  }\n}\n"
        );
        assert_eq!(
            flat("@media (x) { .a { @layer l { color: red; } } }"),
            "@media (x) {\n  @layer l {\n    .a {\n      color: red;\n    }\n  }\n}\n"
        );
        assert!(matches!(
            flatten(parse(".a { @font-face { src: x; } }").unwrap()),
            Err(CanonicalError::NestedAtRule(name)) if name == "@font-face"
        ));
    }

    #[test]
    fn top_level_nesting_selector_is_the_scoping_root() {
        // CSS Nesting 1 §3: `&` outside a style rule is `:scope`.
        assert_eq!(
            flat("& > .a { color: red; }"),
            ":scope > .a {\n  color: red;\n}\n"
        );
    }

    #[test]
    fn scope_rules() {
        // CSS Cascade 6 §2.5 and CSS Nesting 1 §3.3.
        assert_eq!(
            flat(".p { @scope (.from) to (& .to) { color: red; & > .c { color: blue; } } }"),
            "@scope (.p .from) to (:where(:scope) .to) {\n  :where(:scope) {\n    color: red;\n  }\n  \
             :where(:scope) > .c {\n    color: blue;\n  }\n}\n"
        );
    }

    #[test]
    fn empty_rules_are_dropped_but_layers_kept() {
        assert_eq!(
            flat(".a { .b { } } @media (x) { .c { } } @layer l { }"),
            "@layer l;\n"
        );
    }

    #[test]
    fn keyframes_and_other_top_level_at_rules_are_opaque() {
        assert_eq!(
            flat("@keyframes k { to { color: red; } }"),
            "@keyframes k {\n  to {\n    color: red;\n  }\n}\n"
        );
    }
}
