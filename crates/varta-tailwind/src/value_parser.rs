//! CSS value tokenizer (port of Tailwind's `value-parser.ts`): words,
//! functions and separators, with strings and escapes kept verbatim.

/// One value node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueNode {
    Word(String),
    Function { name: String, nodes: Vec<ValueNode> },
    Separator(String),
}

impl ValueNode {
    /// The word or function name, or the separator text.
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            ValueNode::Word(value) | ValueNode::Separator(value) => value,
            ValueNode::Function { name, .. } => name,
        }
    }
}

/// Print value nodes back to text.
#[must_use]
pub fn to_css(nodes: &[ValueNode]) -> String {
    let mut out = String::new();
    write_css(nodes, &mut out);
    out
}

fn write_css(nodes: &[ValueNode], out: &mut String) {
    for node in nodes {
        match node {
            ValueNode::Word(value) | ValueNode::Separator(value) => out.push_str(value),
            ValueNode::Function { name, nodes } => {
                out.push_str(name);
                out.push('(');
                write_css(nodes, out);
                out.push(')');
            }
        }
    }
}

fn is_separator(c: char) -> bool {
    matches!(c, ':' | ',' | '=' | '>' | '<' | '\n' | ' ' | '\t')
}

/// Parse a value. Never fails: unbalanced input is tokenized as Tailwind does.
#[must_use]
pub fn parse(input: &str) -> Vec<ValueNode> {
    let input = input.replace("\r\n", "\n");
    let chars: Vec<char> = input.chars().collect();
    // Each open function is a path of indexes into the tree; children are
    // appended through the path.
    let mut root: Vec<ValueNode> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut buffer = String::new();

    fn target<'a>(root: &'a mut Vec<ValueNode>, stack: &[usize]) -> &'a mut Vec<ValueNode> {
        let mut nodes = root;
        for &index in stack {
            match &mut nodes[index] {
                ValueNode::Function { nodes: children, .. } => nodes = children,
                // Invariant: the stack only records function nodes.
                _ => unreachable!("value parser stack points at a function"),
            }
        }
        nodes
    }

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                buffer.push(c);
                if let Some(next) = chars.get(i + 1) {
                    buffer.push(*next);
                }
                i += 1;
            }
            '/' => {
                let nodes = target(&mut root, &stack);
                if !buffer.is_empty() {
                    nodes.push(ValueNode::Word(std::mem::take(&mut buffer)));
                }
                nodes.push(ValueNode::Word("/".into()));
            }
            c if is_separator(c) => {
                let nodes = target(&mut root, &stack);
                if !buffer.is_empty() {
                    nodes.push(ValueNode::Word(std::mem::take(&mut buffer)));
                }
                let start = i;
                let mut end = i + 1;
                while end < chars.len() && is_separator(chars[end]) {
                    end += 1;
                }
                i = end - 1;
                nodes.push(ValueNode::Separator(chars[start..end].iter().collect()));
            }
            '\'' | '"' => {
                let start = i;
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == '\\' {
                        j += 1;
                    } else if chars[j] == c {
                        i = j;
                        break;
                    }
                    j += 1;
                }
                buffer.extend(&chars[start..=i.min(chars.len() - 1)]);
            }
            '(' => {
                let name = std::mem::take(&mut buffer);
                let nodes = target(&mut root, &stack);
                nodes.push(ValueNode::Function {
                    name,
                    nodes: Vec::new(),
                });
                let index = nodes.len() - 1;
                stack.push(index);
            }
            ')' => {
                if !buffer.is_empty() {
                    // Tailwind pushes the trailing word into the closing
                    // function only (`tail?.nodes.push`), dropping it at top level.
                    if !stack.is_empty() {
                        let nodes = target(&mut root, &stack);
                        nodes.push(ValueNode::Word(std::mem::take(&mut buffer)));
                    } else {
                        buffer.clear();
                    }
                }
                stack.pop();
            }
            _ => buffer.push(c),
        }
        i += 1;
    }
    if !buffer.is_empty() {
        root.push(ValueNode::Word(buffer));
    }
    root
}

/// Visit every value node mutably, depth-first. The callback may replace the
/// node; children of a replaced node are visited afterwards.
pub fn walk_mut(nodes: &mut [ValueNode], visit: &mut dyn FnMut(&mut ValueNode) -> WalkAction) {
    for node in nodes.iter_mut() {
        match visit(node) {
            WalkAction::Continue => {
                if let ValueNode::Function { nodes, .. } = node {
                    walk_mut(nodes, visit);
                }
            }
            WalkAction::Skip => {}
        }
    }
}

/// Visit every value node, depth-first.
pub fn walk(nodes: &[ValueNode], visit: &mut dyn FnMut(&ValueNode) -> WalkAction) {
    for node in nodes {
        match visit(node) {
            WalkAction::Continue => {
                if let ValueNode::Function { nodes, .. } = node {
                    walk(nodes, visit);
                }
            }
            WalkAction::Skip => {}
        }
    }
}

/// Whether a walk descends into the children of the visited node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkAction {
    Continue,
    Skip,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ValueNode::*;

    fn w(s: &str) -> ValueNode {
        Word(s.into())
    }
    fn sep(s: &str) -> ValueNode {
        Separator(s.into())
    }

    #[test]
    fn parses_words_functions_and_separators() {
        assert_eq!(
            parse("calc(1px + 2px)"),
            vec![Function {
                name: "calc".into(),
                nodes: vec![w("1px"), sep(" "), w("+"), sep(" "), w("2px")]
            }]
        );
        assert_eq!(parse("a, b"), vec![w("a"), sep(", "), w("b")]);
        assert_eq!(parse("a/b"), vec![w("a"), w("/"), w("b")]);
        assert_eq!(
            parse("var(--a, 'x y')"),
            vec![Function {
                name: "var".into(),
                nodes: vec![w("--a"), sep(", "), w("'x y'")]
            }]
        );
    }

    #[test]
    fn round_trips() {
        for input in [
            "theme(colors.red.500/10)",
            "foo(bar(baz, 1px) 2px)",
            "url(\"a b\")",
            "1px\\ 2",
        ] {
            assert_eq!(to_css(&parse(input)), input);
        }
    }
}
