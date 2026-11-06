//! The CSS AST of the compiler (port of Tailwind's `ast.ts` node types).
//!
//! Nodes are owned trees; Tailwind's in-place mutations through shared
//! references become explicit transformations.
use std::collections::BTreeMap;

/// A value carried by a [`Node::Context`] marker.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextValue {
    Flag(bool),
    Text(String),
}

/// One CSS AST node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A style rule: `selector { nodes }`.
    Rule {
        selector: String,
        nodes: Vec<Node>,
    },
    /// An at-rule; `name` keeps its `@` (e.g. `@media`).
    AtRule {
        name: String,
        params: String,
        nodes: Vec<Node>,
    },
    /// A declaration; `None` value is Tailwind's `undefined` (dropped on print).
    Decl {
        property: String,
        value: Option<String>,
        important: bool,
    },
    Comment(String),
    /// A transparent marker carrying context (`theme`, `reference`, …).
    Context {
        context: BTreeMap<String, ContextValue>,
        nodes: Vec<Node>,
    },
    /// Nodes hoisted to the root of the stylesheet (`@keyframes`, `@property`).
    AtRoot(Vec<Node>),
}

impl Node {
    /// A style rule.
    #[must_use]
    pub fn rule(selector: impl Into<String>, nodes: Vec<Node>) -> Node {
        Node::Rule {
            selector: selector.into(),
            nodes,
        }
    }
    /// An at-rule; `name` includes the `@`.
    #[must_use]
    pub fn at_rule(name: impl Into<String>, params: impl Into<String>, nodes: Vec<Node>) -> Node {
        Node::AtRule {
            name: name.into(),
            params: params.into(),
            nodes,
        }
    }
    /// A rule from a prelude: `@name params` becomes an at-rule, anything
    /// else a style rule (port of `rule()`).
    #[must_use]
    pub fn from_prelude(prelude: &str, nodes: Vec<Node>) -> Node {
        if prelude.starts_with('@') {
            let (name, params) = split_at_rule_prelude(prelude);
            Node::at_rule(name, params, nodes)
        } else {
            Node::rule(prelude, nodes)
        }
    }
    /// A declaration.
    #[must_use]
    pub fn decl(property: impl Into<String>, value: impl Into<String>) -> Node {
        Node::Decl {
            property: property.into(),
            value: Some(value.into()),
            important: false,
        }
    }
    /// A declaration whose value may be absent (`undefined`).
    #[must_use]
    pub fn decl_opt(property: impl Into<String>, value: Option<String>) -> Node {
        Node::Decl {
            property: property.into(),
            value,
            important: false,
        }
    }
    /// A context marker.
    #[must_use]
    pub fn context(context: BTreeMap<String, ContextValue>, nodes: Vec<Node>) -> Node {
        Node::Context { context, nodes }
    }
    /// An at-root wrapper.
    #[must_use]
    pub fn at_root(nodes: Vec<Node>) -> Node {
        Node::AtRoot(nodes)
    }
    /// The children of a node with children.
    #[must_use]
    pub fn children(&self) -> Option<&Vec<Node>> {
        match self {
            Node::Rule { nodes, .. }
            | Node::AtRule { nodes, .. }
            | Node::Context { nodes, .. }
            | Node::AtRoot(nodes) => Some(nodes),
            Node::Decl { .. } | Node::Comment(_) => None,
        }
    }
    /// The mutable children of a node with children.
    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::Rule { nodes, .. }
            | Node::AtRule { nodes, .. }
            | Node::Context { nodes, .. }
            | Node::AtRoot(nodes) => Some(nodes),
            Node::Decl { .. } | Node::Comment(_) => None,
        }
    }
    /// Whether this is a style rule or an at-rule (Tailwind's `Rule`).
    #[must_use]
    pub fn is_rule_like(&self) -> bool {
        matches!(self, Node::Rule { .. } | Node::AtRule { .. })
    }
}

/// Split an at-rule prelude `@name params` into `("@name", "params")`.
#[must_use]
pub fn split_at_rule_prelude(prelude: &str) -> (String, String) {
    let prelude = prelude.trim();
    let end = prelude
        .char_indices()
        .skip(1)
        .find(|(_, c)| c.is_whitespace() || *c == '(')
        .map_or(prelude.len(), |(i, _)| i);
    let name = &prelude[..end];
    let params = prelude[end..].trim();
    (name.to_owned(), params.to_owned())
}

/// Visit every node depth-first (pre-order), with the depth.
pub fn for_each<'a>(nodes: &'a [Node], visit: &mut dyn FnMut(&'a Node, usize)) {
    fn go<'a>(nodes: &'a [Node], depth: usize, visit: &mut dyn FnMut(&'a Node, usize)) {
        for node in nodes {
            visit(node, depth);
            if let Some(children) = node.children() {
                go(children, depth + 1, visit);
            }
        }
    }
    go(nodes, 0, visit);
}

/// Visit every node mutably depth-first (pre-order).
pub fn for_each_mut(nodes: &mut [Node], visit: &mut dyn FnMut(&mut Node)) {
    for node in nodes {
        visit(node);
        if let Some(children) = node.children_mut() {
            for_each_mut(children, visit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_splits_name_and_params() {
        assert_eq!(
            split_at_rule_prelude("@media (hover: hover)"),
            ("@media".into(), "(hover: hover)".into())
        );
        assert_eq!(
            split_at_rule_prelude("@supports(display:grid)"),
            ("@supports".into(), "(display:grid)".into())
        );
        assert_eq!(
            split_at_rule_prelude("@slot"),
            ("@slot".into(), String::new())
        );
    }
}
