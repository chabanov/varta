//! Selector parser: a port of Tailwind CSS v4.3.5 `selector-parser.ts`
//! (MIT, Copyright (c) Tailwind Labs, Inc.; see `LICENSE-TAILWIND`).
//!
//! The parser splits a selector into lists, complex and compound selectors,
//! combinators, simple selectors and functions. Only the selector-taking
//! pseudo-classes (`:is`, `:where`, `:not`, `:has` and the legacy `any`
//! forms) are parsed recursively; every other function keeps its arguments
//! as one opaque value, except the `of <selector-list>` clause of
//! `:nth-child()`/`:nth-last-child()`. Parsing never fails. Function nesting
//! deeper than [`MAX_SELECTOR_DEPTH`] is kept opaque, which bounds the depth
//! of every tree, so the recursive helpers here and in the optimizer are
//! bounded too.

/// The deepest nesting of parsed selector functions; deeper arguments are
/// kept as an opaque value (Tailwind has no bound).
pub const MAX_SELECTOR_DEPTH: usize = 128;

/// Pseudo-classes whose arguments are parsed as selectors.
const SELECTOR_FUNCTIONS: &[&str] = &[":not", ":where", ":has", ":is", ":-moz-any", ":-webkit-any"];
/// Functions with an optional `of <selector-list>` clause (Selectors 4 §14.4).
const NTH_OF_FUNCTIONS: &[&str] = &[":nth-child", ":nth-last-child"];
/// The keyword that starts the selector list of an `:nth-child()`.
const NTH_OF: &str = "of ";
/// Pseudo-elements from CSS 1 and 2 that browsers still accept with a single
/// colon (Selectors 4 §16.1).
const LEGACY_PSEUDO_ELEMENTS: &[&str] = &[":before", ":after", ":first-line", ":first-letter"];
/// The pseudo-element prefix.
const PSEUDO_ELEMENT_PREFIX: &str = "::";

/// One selector AST node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorNode {
    /// A selector list: `a, b`.
    List(Vec<SelectorNode>),
    /// A combinator: ` `, `>`, `+` or `~` (any trimmed run of them).
    Combinator(String),
    /// A complex selector: compounds joined by combinators.
    Complex(Vec<SelectorNode>),
    /// A compound selector: simple selectors without combinators.
    Compound(Vec<SelectorNode>),
    /// A simple selector (`.a`, `#b`, `div`, `[x]`, `:hover`, `&`, `*`).
    Selector(String),
    /// A functional pseudo-class or pseudo-element.
    Function {
        value: String,
        nodes: Vec<SelectorNode>,
    },
    /// Opaque function arguments.
    Value(String),
}

impl SelectorNode {
    /// The children of a node with children.
    #[must_use]
    pub fn children(&self) -> Option<&Vec<SelectorNode>> {
        match self {
            SelectorNode::List(nodes)
            | SelectorNode::Complex(nodes)
            | SelectorNode::Compound(nodes)
            | SelectorNode::Function { nodes, .. } => Some(nodes),
            SelectorNode::Combinator(_) | SelectorNode::Selector(_) | SelectorNode::Value(_) => {
                None
            }
        }
    }

    /// The mutable children of a node with children.
    pub fn children_mut(&mut self) -> Option<&mut Vec<SelectorNode>> {
        match self {
            SelectorNode::List(nodes)
            | SelectorNode::Complex(nodes)
            | SelectorNode::Compound(nodes)
            | SelectorNode::Function { nodes, .. } => Some(nodes),
            SelectorNode::Combinator(_) | SelectorNode::Selector(_) | SelectorNode::Value(_) => {
                None
            }
        }
    }

    /// The value of a simple selector or function.
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        match self {
            SelectorNode::Selector(value)
            | SelectorNode::Function { value, .. }
            | SelectorNode::Value(value)
            | SelectorNode::Combinator(value) => Some(value),
            SelectorNode::List(_) | SelectorNode::Complex(_) | SelectorNode::Compound(_) => None,
        }
    }
}

fn simple_starts_with(node: &SelectorNode, prefix: char) -> bool {
    matches!(node, SelectorNode::Selector(value) if value.starts_with(prefix))
}

/// `*`
#[must_use]
pub fn is_universal_selector(node: &SelectorNode) -> bool {
    simple_starts_with(node, '*')
}

/// `&`
#[must_use]
pub fn is_nesting_selector(node: &SelectorNode) -> bool {
    simple_starts_with(node, '&')
}

/// `.class`
#[must_use]
pub fn is_class_selector(node: &SelectorNode) -> bool {
    simple_starts_with(node, '.')
}

/// `#id`
#[must_use]
pub fn is_id_selector(node: &SelectorNode) -> bool {
    simple_starts_with(node, '#')
}

/// `:pseudo` (non-functional pseudo-classes and pseudo-elements).
#[must_use]
pub fn is_pseudo_selector(node: &SelectorNode) -> bool {
    simple_starts_with(node, ':')
}

/// `::pseudo-element`, or a legacy single-colon pseudo-element.
#[must_use]
pub fn is_pseudo_element(node: &SelectorNode) -> bool {
    let (SelectorNode::Selector(value) | SelectorNode::Function { value, .. }) = node else {
        return false;
    };
    value.starts_with(PSEUDO_ELEMENT_PREFIX)
        || LEGACY_PSEUDO_ELEMENTS
            .iter()
            .any(|legacy| value.eq_ignore_ascii_case(legacy))
}

/// `[attribute]`
#[must_use]
pub fn is_attribute_selector(node: &SelectorNode) -> bool {
    simple_starts_with(node, '[')
}

/// A type selector: any simple selector that is none of the others (not
/// verified further, as in Tailwind).
#[must_use]
pub fn is_type_selector(node: &SelectorNode) -> bool {
    match node {
        SelectorNode::Selector(value) => !matches!(
            value.chars().next(),
            Some('*' | '&' | '.' | '#' | ':' | '[')
        ),
        _ => false,
    }
}

/// Print a selector AST (Tailwind's `toCss`); `minify` drops the spaces
/// around non-descendant combinators and after list commas.
#[must_use]
pub fn to_css(ast: &[SelectorNode], minify: bool) -> String {
    let mut css = String::new();
    write_css(ast, minify, &mut css);
    css
}

fn write_css(ast: &[SelectorNode], minify: bool, css: &mut String) {
    for node in ast {
        match node {
            SelectorNode::Selector(value) | SelectorNode::Value(value) => css.push_str(value),
            SelectorNode::Combinator(value) => {
                if minify || value == " " {
                    css.push_str(value);
                } else {
                    css.push(' ');
                    css.push_str(value);
                    css.push(' ');
                }
            }
            SelectorNode::Function { value, nodes } => {
                css.push_str(value);
                css.push('(');
                write_css(nodes, minify, css);
                css.push(')');
            }
            SelectorNode::Complex(nodes) | SelectorNode::Compound(nodes) => {
                write_css(nodes, minify, css);
            }
            SelectorNode::List(nodes) => {
                for (index, node) in nodes.iter().enumerate() {
                    if index > 0 {
                        css.push_str(if minify { "," } else { ", " });
                    }
                    write_css(std::slice::from_ref(node), minify, css);
                }
            }
        }
    }
}

/// One level of the parser: the open function (none at the root), its
/// nodes, the selector list being collected and whether a combinator was
/// seen in the current list item.
struct Frame {
    function: Option<String>,
    target: Vec<SelectorNode>,
    list: Option<Vec<SelectorNode>>,
    contains_combinator: bool,
}

impl Frame {
    fn new(function: Option<String>) -> Self {
        Self {
            function,
            target: Vec::new(),
            list: None,
            contains_combinator: false,
        }
    }

    /// The current list item (Tailwind's `current`).
    fn current(nodes: Vec<SelectorNode>, contains_combinator: bool) -> SelectorNode {
        if nodes.len() == 1 {
            nodes
                .into_iter()
                .next()
                .unwrap_or(SelectorNode::Compound(Vec::new()))
        } else if contains_combinator {
            SelectorNode::Complex(nodes)
        } else {
            SelectorNode::Compound(nodes)
        }
    }

    /// Attach a simple selector or function to the current compound.
    fn append(&mut self, node: SelectorNode) {
        match self.target.last_mut() {
            Some(SelectorNode::Compound(nodes)) => nodes.push(node),
            Some(existing)
                if !matches!(
                    existing,
                    SelectorNode::List(_) | SelectorNode::Combinator(_)
                ) =>
            {
                let previous = std::mem::replace(existing, SelectorNode::Compound(Vec::new()));
                *existing = SelectorNode::Compound(vec![previous, node]);
            }
            _ => self.target.push(node),
        }
    }

    /// Complete the frame at its `)` or at the end of the input.
    fn finish(self) -> Vec<SelectorNode> {
        if let Some(mut list) = self.list {
            list.push(Frame::current(self.target, self.contains_combinator));
            vec![SelectorNode::List(list)]
        } else if self.contains_combinator {
            vec![SelectorNode::Complex(self.target)]
        } else {
            self.target
        }
    }

    /// The nodes of a frame left open at the end of the input: Tailwind
    /// never completes it, so a pending list item is lost.
    fn abandon(self) -> Vec<SelectorNode> {
        match self.list {
            Some(list) => vec![SelectorNode::List(list)],
            None => self.target,
        }
    }
}

fn is_combinator(c: char) -> bool {
    matches!(c, '>' | '\n' | ' ' | '+' | '\t' | '~')
}

/// Parse a selector (Tailwind's `parse`).
#[must_use]
pub fn parse(input: &str) -> Vec<SelectorNode> {
    parse_at_depth(input, 0)
}

fn flush(buffer: &mut String, frame: &mut Frame) {
    if !buffer.is_empty() {
        frame.append(SelectorNode::Selector(std::mem::take(buffer)));
    }
}

/// The index of the bracket closing the one at `open`, counting nesting.
fn closing(chars: &[char], open: usize, opening: char, closing: char) -> Option<usize> {
    let mut nesting = 0usize;
    for (j, &c) in chars.iter().enumerate().skip(open + 1) {
        if c == opening {
            nesting += 1;
        } else if c == closing {
            if nesting == 0 {
                return Some(j);
            }
            nesting -= 1;
        }
    }
    None
}

fn parse_at_depth(input: &str, depth: usize) -> Vec<SelectorNode> {
    let input = input.replace("\r\n", "\n");
    let chars: Vec<char> = input.chars().collect();
    let mut frames = vec![Frame::new(None)];
    let mut buffer = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let level = depth + frames.len();
        let Some(frame) = frames.last_mut() else {
            break;
        };
        match c {
            ',' => {
                flush(&mut buffer, frame);
                while chars
                    .get(i + 1)
                    .is_some_and(|&next| matches!(next, '\n' | ' ' | '\t'))
                {
                    i += 1;
                }
                let item =
                    Frame::current(std::mem::take(&mut frame.target), frame.contains_combinator);
                frame.list.get_or_insert_with(Vec::new).push(item);
                frame.contains_combinator = false;
            }
            c if is_combinator(c) => {
                flush(&mut buffer, frame);
                let start = i;
                let mut end = i + 1;
                while chars.get(end).is_some_and(|&next| is_combinator(next)) {
                    end += 1;
                }
                i = end - 1;
                let value: String = chars[start..end].iter().collect();
                let value = value.trim();
                if value.is_empty()
                    && (frame.target.is_empty() || end >= chars.len() || chars[end] == ',')
                {
                    i += 1;
                    continue;
                }
                let value = if value.is_empty() { " " } else { value };
                frame
                    .target
                    .push(SelectorNode::Combinator(value.to_owned()));
                frame.contains_combinator = true;
            }
            '(' => {
                let name = std::mem::take(&mut buffer);
                if SELECTOR_FUNCTIONS.contains(&name.as_str()) && level < MAX_SELECTOR_DEPTH {
                    frames.push(Frame::new(Some(name)));
                } else {
                    // An unclosed function keeps an empty value (JavaScript's
                    // `slice` with an end before its start).
                    let end = closing(&chars, i, '(', ')').unwrap_or(i);
                    let contents: String = chars[(i + 1).min(end)..end].iter().collect();
                    let mut nodes = Vec::new();
                    match contents.find(NTH_OF) {
                        Some(of)
                            if NTH_OF_FUNCTIONS.contains(&name.as_str())
                                && level + 1 < MAX_SELECTOR_DEPTH =>
                        {
                            let split = of + NTH_OF.len();
                            nodes.push(SelectorNode::Value(contents[..split].to_owned()));
                            nodes.extend(parse_at_depth(&contents[split..], level + 1));
                        }
                        _ => nodes.push(SelectorNode::Value(contents)),
                    }
                    frame.append(SelectorNode::Function { value: name, nodes });
                    i = end;
                }
            }
            ')' => {
                flush(&mut buffer, frame);
                if let Some(mut closed) = frames.pop() {
                    let function = closed.function.take();
                    let nodes = closed.finish();
                    match (function, frames.last_mut()) {
                        // Tailwind appends the function when it opens; the
                        // parent is untouched until the `)`, so appending
                        // it now gives the same tree.
                        (Some(value), Some(parent)) => {
                            parent.append(SelectorNode::Function { value, nodes });
                        }
                        // A stray `)` at the top level restarts the root
                        // with its completed nodes.
                        _ => {
                            let mut root = Frame::new(None);
                            root.target = nodes;
                            frames.push(root);
                        }
                    }
                }
            }
            '.' | ':' | '#' => {
                if c == ':' && buffer == ":" {
                    buffer.push(c);
                } else {
                    flush(&mut buffer, frame);
                    buffer.push(c);
                }
            }
            '[' => {
                flush(&mut buffer, frame);
                let end = closing(&chars, i, '[', ']').unwrap_or(i);
                frame.append(SelectorNode::Selector(chars[i..=end].iter().collect()));
                i = end;
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
                buffer.extend(&chars[start..=i]);
            }
            '&' | '*' => {
                flush(&mut buffer, frame);
                frame.append(SelectorNode::Selector(c.to_string()));
            }
            '\\' => {
                // Tailwind appends `input[i + 1]`, which is `undefined` at
                // the end of the input; a lone trailing `\` is kept as is.
                buffer.push(c);
                if let Some(&next) = chars.get(i + 1) {
                    buffer.push(next);
                }
                i += 1;
            }
            _ => buffer.push(c),
        }
        i += 1;
    }
    finish_input(frames, buffer)
}

/// Complete the parse at the end of the input. Only the innermost open
/// frame is completed, exactly as in Tailwind.
fn finish_input(mut frames: Vec<Frame>, mut buffer: String) -> Vec<SelectorNode> {
    let Some(mut innermost) = frames.pop() else {
        return Vec::new();
    };
    flush(&mut buffer, &mut innermost);
    let mut function = innermost.function.take();
    let mut nodes = innermost.finish();
    while let Some(mut parent) = frames.pop() {
        if let Some(value) = function.take() {
            parent.append(SelectorNode::Function { value, nodes });
        }
        function = parent.function.take();
        nodes = parent.abandon();
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use SelectorNode::*;

    fn s(value: &str) -> SelectorNode {
        Selector(value.into())
    }
    fn comb(value: &str) -> SelectorNode {
        Combinator(value.into())
    }
    fn fun(value: &str, nodes: Vec<SelectorNode>) -> SelectorNode {
        Function {
            value: value.into(),
            nodes,
        }
    }

    #[test]
    fn parses_simple_compound_and_lists() {
        assert_eq!(parse(".foo"), vec![s(".foo")]);
        assert_eq!(
            parse(".foo.bar:hover#id"),
            vec![Compound(vec![s(".foo"), s(".bar"), s(":hover"), s("#id")])]
        );
        assert_eq!(
            parse(".foo::before"),
            vec![Compound(vec![s(".foo"), s("::before")])]
        );
        for input in [".foo,.bar", ".foo, .bar", ".foo , .bar", ".foo ,.bar"] {
            assert_eq!(
                parse(input),
                vec![List(vec![s(".foo"), s(".bar")])],
                "{input}"
            );
        }
        assert_eq!(
            parse(".foo,"),
            vec![List(vec![s(".foo"), Compound(vec![])])]
        );
        assert_eq!(
            parse(",.foo"),
            vec![List(vec![Compound(vec![]), s(".foo")])]
        );
        assert_eq!(
            parse(".foo,,.bar"),
            vec![List(vec![s(".foo"), Compound(vec![]), s(".bar")])]
        );
    }

    #[test]
    fn parses_complex_selectors_in_lists() {
        assert_eq!(
            parse(".a+.b, .c .d[attr], .e"),
            vec![List(vec![
                Complex(vec![s(".a"), comb("+"), s(".b")]),
                Complex(vec![
                    s(".c"),
                    comb(" "),
                    Compound(vec![s(".d"), s("[attr]")])
                ]),
                s(".e"),
            ])]
        );
        assert_eq!(
            parse("#a.b > .c, .d"),
            vec![List(vec![
                Complex(vec![Compound(vec![s("#a"), s(".b")]), comb(">"), s(".c")]),
                s(".d"),
            ])]
        );
        assert_eq!(
            parse(".foo  \n\t .bar"),
            vec![Complex(vec![s(".foo"), comb(" "), s(".bar")])]
        );
    }

    #[test]
    fn parses_functions() {
        assert_eq!(
            parse(".foo:hover:not(.bar:focus)"),
            vec![Compound(vec![
                s(".foo"),
                s(":hover"),
                fun(":not", vec![Compound(vec![s(".bar"), s(":focus")])]),
            ])]
        );
        assert_eq!(
            parse(":is(.foo, .bar)"),
            vec![fun(":is", vec![List(vec![s(".foo"), s(".bar")])])]
        );
        assert_eq!(
            parse(":where(&, .foo)"),
            vec![fun(":where", vec![List(vec![s("&"), s(".foo")])])]
        );
        assert_eq!(
            parse(":nth-child(n+1)"),
            vec![fun(":nth-child", vec![Value("n+1".into())])]
        );
        assert_eq!(
            parse(":nth-child(2 of &)"),
            vec![fun(":nth-child", vec![Value("2 of ".into()), s("&")])]
        );
        assert_eq!(
            parse(":nth-child(2n + 1 of .foo, .bar)"),
            vec![fun(
                ":nth-child",
                vec![Value("2n + 1 of ".into()), List(vec![s(".foo"), s(".bar")])]
            )]
        );
        assert_eq!(
            parse("&:has(:nth-child(2))"),
            vec![Compound(vec![
                s("&"),
                fun(":has", vec![fun(":nth-child", vec![Value("2".into())])]),
            ])]
        );
    }

    #[test]
    fn parses_nesting_universal_attribute_and_type_selectors() {
        assert_eq!(
            parse("&[data-foo]"),
            vec![Compound(vec![s("&"), s("[data-foo]")])]
        );
        assert_eq!(
            parse("[data-foo]&"),
            vec![Compound(vec![s("[data-foo]"), s("&")])]
        );
        assert_eq!(
            parse("[data-foo]*"),
            vec![Compound(vec![s("[data-foo]"), s("*")])]
        );
        assert_eq!(
            parse("[data-foo]div"),
            vec![Compound(vec![s("[data-foo]"), s("div")])]
        );
        assert_eq!(
            parse(".foo[bar=\"baz\"]"),
            vec![Compound(vec![s(".foo"), s("[bar=\"baz\"]")])]
        );
        assert_eq!(parse("foo\\.bar"), vec![s("foo\\.bar")]);
        assert!(is_type_selector(&s("div")));
        assert!(!is_type_selector(&s("&")));
        assert!(is_universal_selector(&s("*")));
        assert!(is_pseudo_element(&s("::before")));
        assert!(is_pseudo_element(&s(":BEFORE")));
        assert!(!is_pseudo_element(&s(":hover")));
    }

    #[test]
    fn prints_selectors() {
        assert_eq!(
            to_css(&parse(".foo.bar:hover#id"), false),
            ".foo.bar:hover#id"
        );
        assert_eq!(to_css(&parse(".foo ,.bar"), false), ".foo, .bar");
        assert_eq!(to_css(&parse(".foo ,.bar"), true), ".foo,.bar");
        assert_eq!(
            to_css(&parse(":nth-child(2n + 1 of .foo,.bar)"), false),
            ":nth-child(2n + 1 of .foo, .bar)"
        );
        assert_eq!(
            to_css(&parse(".a+.b, .c .d[attr], .e"), false),
            ".a + .b, .c .d[attr], .e"
        );
        assert_eq!(
            to_css(&parse(".a+.b, .c .d[attr], .e"), true),
            ".a+.b,.c .d[attr],.e"
        );
    }

    #[test]
    fn unclosed_and_deep_input_is_bounded() {
        // Only the innermost open frame is completed, as in Tailwind.
        assert_eq!(parse(".a, :is(.b"), vec![List(vec![s(".a")])]);
        assert_eq!(parse(":is(.b"), vec![fun(":is", vec![s(".b")])]);
        let deep = ":is(".repeat(MAX_SELECTOR_DEPTH * 2) + &")".repeat(MAX_SELECTOR_DEPTH * 2);
        assert_eq!(to_css(&parse(&deep), false), deep);
    }
}
