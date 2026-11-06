//! CSS printer: a port of `toCss` from Tailwind CSS v4.3.5 `ast.ts` (MIT,
//! Copyright (c) Tailwind Labs, Inc.; see `LICENSE-TAILWIND`), without
//! source maps.
//!
//! The output is byte-identical to Tailwind's: two-space indentation, one
//! node per line, `prop: value;` declarations with ` !important`, and
//! at-rules without children printed as statements (`@layer x;`). Context
//! and at-root nodes print nothing (they must be removed by `optimize_ast`
//! first). A declaration without a value (Tailwind's `undefined`) is not
//! printed. The tree is walked with an explicit stack, so printing never
//! recurses.
use crate::ast::Node;

/// One level of indentation.
const INDENT: &str = "  ";

/// Pending printer work: a node to print, or the closing brace of a block.
enum Step<'a> {
    Node(&'a Node, usize),
    Close(usize),
}

/// Print an AST as CSS (Tailwind's `toCss`).
#[must_use]
pub fn to_css(ast: &[Node]) -> String {
    let mut css = String::new();
    let mut steps: Vec<Step<'_>> = ast.iter().rev().map(|node| Step::Node(node, 0)).collect();
    while let Some(step) = steps.pop() {
        let (node, depth) = match step {
            Step::Close(depth) => {
                push_indent(&mut css, depth);
                css.push_str("}\n");
                continue;
            }
            Step::Node(node, depth) => (node, depth),
        };
        let children = match node {
            Node::Decl {
                property,
                value: Some(value),
                important,
            } => {
                push_indent(&mut css, depth);
                css.push_str(property);
                css.push_str(": ");
                css.push_str(value);
                if *important {
                    css.push_str(" !important");
                }
                css.push_str(";\n");
                continue;
            }
            Node::Decl { value: None, .. } | Node::Context { .. } | Node::AtRoot(_) => continue,
            Node::Comment(value) => {
                push_indent(&mut css, depth);
                css.push_str("/*");
                css.push_str(value);
                css.push_str("*/\n");
                continue;
            }
            Node::Rule { selector, nodes } => {
                push_indent(&mut css, depth);
                css.push_str(selector);
                css.push_str(" {\n");
                nodes
            }
            Node::AtRule {
                name,
                params,
                nodes,
            } => {
                push_indent(&mut css, depth);
                css.push_str(name);
                if nodes.is_empty() {
                    css.push(' ');
                    css.push_str(params);
                    css.push_str(";\n");
                    continue;
                }
                if !params.is_empty() {
                    css.push(' ');
                    css.push_str(params);
                }
                css.push_str(" {\n");
                nodes
            }
        };
        steps.push(Step::Close(depth));
        steps.extend(
            children
                .iter()
                .rev()
                .map(|child| Step::Node(child, depth + 1)),
        );
    }
    css
}

fn push_indent(css: &mut String, depth: usize) {
    for _ in 0..depth {
        css.push_str(INDENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_parser::parse;

    #[test]
    fn pretty_prints_an_ast() {
        assert_eq!(
            to_css(&parse(".foo{color:red;&:hover{color:blue;}}").unwrap()),
            ".foo {\n  color: red;\n  &:hover {\n    color: blue;\n  }\n}\n"
        );
    }

    #[test]
    fn prints_statements_important_comments_and_skips_markers() {
        let ast = vec![
            Node::Comment("! License ".into()),
            Node::at_rule("@layer", "theme, base", vec![]),
            Node::at_rule("@tailwind", "", vec![]),
            Node::at_rule(
                "@starting-style",
                "",
                vec![Node::rule(
                    ".a",
                    vec![
                        Node::Decl {
                            property: "color".into(),
                            value: Some("red".into()),
                            important: true,
                        },
                        Node::decl_opt("--unset", None),
                    ],
                )],
            ),
            Node::context(Default::default(), vec![Node::decl("color", "blue")]),
            Node::at_root(vec![Node::decl("color", "blue")]),
        ];
        assert_eq!(
            to_css(&ast),
            "/*! License */\n@layer theme, base;\n@tailwind ;\n@starting-style {\n  .a {\n    \
             color: red !important;\n  }\n}\n"
        );
    }

    #[test]
    fn keeps_empty_rules_and_at_rules_like_tailwind() {
        let input = ".foo {\n}\n@media (min-width: 768px);\n@charset \"UTF-8\";\n";
        assert_eq!(to_css(&parse(input).unwrap()), input);
    }
}
