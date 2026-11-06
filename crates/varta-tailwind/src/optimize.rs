//! AST optimization: a port of `optimizeAst`, `handleNesting` and their
//! helpers from Tailwind CSS v4.3.5 `ast.ts` (MIT, Copyright (c) Tailwind
//! Labs, Inc.; see `LICENSE-TAILWIND`).
//!
//! Only Tailwind's `Polyfills.None` behaviour is ported: Varta resolves
//! feature support statically, so no `color-mix()` or `@property` fallback
//! blocks are emitted. Everything else is kept: unused theme variables and
//! theme keyframes are removed, at-root nodes are hoisted, duplicate
//! `@property` rules are dropped, context markers (`theme`, `keyframes`,
//! `reference`, `source`) are honoured and nesting is flattened.
//!
//! Tailwind mutates shared nodes by identity (`parent.indexOf(node)`); here
//! trees are owned, so the transformations give containers and tracked
//! declarations explicit ids, or address result containers by index paths
//! (the result only grows while it is built, so paths stay valid). Both
//! transformations walk the tree with explicit stacks and never recurse
//! over the CSS tree; selector helpers recurse over selector trees, whose
//! depth [`crate::selector_parser::MAX_SELECTOR_DEPTH`] bounds.
use crate::ast::{ContextValue, Node};
use crate::selector_parser::{self as selectors, SelectorNode};
use crate::theme::{Theme, options};
use crate::utils::segment;
use crate::utils::variables::extract_used_variables;
use crate::value_parser::{self, ValueNode};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// At-rules that can be hoisted above the style rules they are nested in:
/// they depend on the environment, not on the parent.
pub const HOISTABLE_AT_RULES: &[&str] = &[
    "@container",
    "@layer",
    "@media",
    "@page",
    "@scope",
    "@starting-style",
    "@supports",
    "@view-transition",
];

/// At-rules that can be dropped when they have no children.
pub const DROPPABLE_IF_EMPTY_AT_RULES: &[&str] = &[
    "@container",
    "@media",
    "@page",
    "@scope",
    "@starting-style",
    "@supports",
    "@view-transition",
];

/// At-rules `optimize_ast` keeps without children (statements).
const KEPT_IF_EMPTY_AT_RULES: &[&str] = &[
    "@layer",
    "@charset",
    "@custom-media",
    "@namespace",
    "@import",
    "@apply",
];

/// Standard pseudo-elements (CSS Pseudo 4, Forms 1, Overflow 5, Shadow
/// Parts 1, Scoping 1, View Transitions 2, Fullscreen, WebVTT), listed
/// without arguments. A combinator after one of them can never match.
const STANDARD_PSEUDO_ELEMENTS: &[&str] = &[
    "::after",
    "::backdrop",
    "::before",
    "::checkmark",
    "::column",
    "::cue",
    "::cue-region",
    "::details-content",
    "::file-selector-button",
    "::first-letter",
    "::first-line",
    "::grammar-error",
    "::highlight",
    "::marker",
    "::part",
    "::picker",
    "::picker-icon",
    "::placeholder",
    "::scroll-button",
    "::scroll-marker",
    "::scroll-marker-group",
    "::search-text",
    "::selection",
    "::slotted",
    "::spelling-error",
    "::target-text",
    "::view-transition",
    "::view-transition-group",
    "::view-transition-group-children",
    "::view-transition-image-pair",
    "::view-transition-new",
    "::view-transition-old",
];

/// Context keys.
const CONTEXT_THEME: &str = "theme";
const CONTEXT_KEYFRAMES: &str = "keyframes";
const CONTEXT_REFERENCE: &str = "reference";
const CONTEXT_SOURCE: &str = "source";
/// The source of a variant body.
const SOURCE_VARIANT: &str = "variant";

const AT_PROPERTY: &str = "@property";
const AT_KEYFRAMES: &str = "@keyframes";
const AT_SCOPE: &str = "@scope";
const AT_LAYER: &str = "@layer";

/// The internal sort key Tailwind never prints.
const SORT_PROPERTY: &str = "--tw-sort";
const ANIMATION_PROPERTY: &str = "animation";
const ANIMATE_NAMESPACE: &str = "--animate-";
const CUSTOM_PROPERTY_PREFIX: &str = "--";
const INITIAL: &str = "initial";
const VAR_FUNCTION: &str = "var(";

const NESTING_SELECTOR: &str = "&";
const SCOPE_SELECTOR: &str = ":scope";
const WHERE_SCOPE_SELECTOR: &str = ":where(:scope)";
const SCOPE_END_KEYWORD: &str = "to";
const NAMESPACE_SEPARATOR: char = '|';
const LIST_SEPARATOR: &str = ", ";

type Context = BTreeMap<String, ContextValue>;

/// JavaScript truthiness of a context value.
fn truthy(context: &Context, key: &str) -> bool {
    match context.get(key) {
        Some(ContextValue::Flag(flag)) => *flag,
        Some(ContextValue::Text(text)) => !text.is_empty(),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// optimizeAst
// ---------------------------------------------------------------------------

/// A container or declaration identity (Tailwind's object identity).
type Id = usize;
/// The id of the result root.
const ROOT_ID: Id = 0;

/// The tree `optimize_ast` builds before the removal passes.
enum Built {
    Plain(Node),
    ThemeDecl {
        node: Node,
        id: Id,
    },
    Rule {
        selector: String,
        nodes: Vec<Built>,
        id: Id,
    },
    AtRule {
        name: String,
        params: String,
        nodes: Vec<Built>,
        id: Id,
    },
    Context {
        context: Context,
        nodes: Vec<Built>,
        id: Id,
    },
}

impl Built {
    fn id(&self) -> Option<Id> {
        match self {
            Built::Plain(_) => None,
            Built::ThemeDecl { id, .. }
            | Built::Rule { id, .. }
            | Built::AtRule { id, .. }
            | Built::Context { id, .. } => Some(*id),
        }
    }

    fn children(&self) -> Option<&Vec<Built>> {
        match self {
            Built::Rule { nodes, .. }
            | Built::AtRule { nodes, .. }
            | Built::Context { nodes, .. } => Some(nodes),
            Built::Plain(_) | Built::ThemeDecl { .. } => None,
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Built>> {
        match self {
            Built::Rule { nodes, .. }
            | Built::AtRule { nodes, .. }
            | Built::Context { nodes, .. } => Some(nodes),
            Built::Plain(_) | Built::ThemeDecl { .. } => None,
        }
    }
}

/// What a transform frame becomes when its children are done.
enum FrameKind {
    Root,
    Rule {
        selector: String,
    },
    AtRule {
        name: String,
        params: String,
        always: bool,
        track_keyframes: bool,
    },
    /// A context that tracks a source subtree; kept as a node.
    ContextCopy {
        context: Context,
    },
    /// A transparent context: its children join the parent.
    Flatten,
    /// An at-root node: each child is transformed into its own array.
    AtRoot,
    /// One child of an at-root node; its result is hoisted.
    AtRootChild,
}

struct Frame {
    kind: FrameKind,
    children: std::vec::IntoIter<Node>,
    out: Vec<Built>,
    /// The id of the container `out` stands for.
    id: Id,
    context: Rc<Context>,
    depth: usize,
}

/// What transforming one node gives.
enum Step {
    Skip,
    Leaf(Built),
    Open(Frame),
}

struct Optimizer<'t> {
    theme: &'t mut Theme,
    next_id: Id,
    at_roots: Vec<Built>,
    seen_at_properties: BTreeSet<String>,
    /// Theme variable declarations per container, in first-seen order.
    theme_variables: Vec<(Id, Vec<Id>)>,
    theme_variable_index: BTreeMap<Id, usize>,
    /// `(property, value)` of every tracked theme declaration.
    declarations: BTreeMap<Id, (String, String)>,
    /// Theme `@keyframes` copies and their names, in order.
    keyframes: Vec<(Id, String)>,
    used_keyframe_names: BTreeSet<String>,
    variable_dependencies: BTreeMap<String, BTreeSet<String>>,
}

/// The names in an `animation` value: JavaScript's
/// `value.split(/[\s,]+/)`, which keeps an empty first or last piece but no
/// empty piece between separators.
fn keyframe_names(value: &str) -> Vec<&str> {
    let pieces: Vec<&str> = value
        .split(|c: char| crate::css_parser::is_js_whitespace(c) || c == ',')
        .collect();
    let last = pieces.len().saturating_sub(1);
    pieces
        .into_iter()
        .enumerate()
        .filter(|(index, piece)| !piece.is_empty() || *index == 0 || *index == last)
        .map(|(_, piece)| piece)
        .collect()
}

impl Optimizer<'_> {
    fn fresh(&mut self) -> Id {
        self.next_id += 1;
        self.next_id
    }

    fn frame(
        &mut self,
        kind: FrameKind,
        nodes: Vec<Node>,
        context: Rc<Context>,
        depth: usize,
    ) -> Frame {
        let id = self.fresh();
        Frame {
            kind,
            children: nodes.into_iter(),
            out: Vec::new(),
            id,
            context,
            depth,
        }
    }

    fn track_theme_variable(&mut self, container: Id, property: &str, value: &str) -> Id {
        let id = self.fresh();
        let index = *self
            .theme_variable_index
            .entry(container)
            .or_insert_with(|| {
                self.theme_variables.push((container, Vec::new()));
                self.theme_variables.len() - 1
            });
        self.theme_variables[index].1.push(id);
        self.declarations
            .insert(id, (property.to_owned(), value.to_owned()));
        id
    }

    fn declaration(&mut self, node: Node, parent: Id, context: &Context) -> Step {
        let Node::Decl {
            property,
            value: Some(value),
            ..
        } = &node
        else {
            return Step::Skip;
        };
        if property == SORT_PROPERTY {
            return Step::Skip;
        }
        let theme_variable =
            truthy(context, CONTEXT_THEME) && property.starts_with(CUSTOM_PROPERTY_PREFIX);
        // Variables resolving to `initial` are never emitted.
        if theme_variable && value == INITIAL {
            return Step::Skip;
        }
        let tracked = theme_variable && !truthy(context, CONTEXT_KEYFRAMES);
        if value.contains(VAR_FUNCTION) {
            // Declaring another theme variable is a dependency, not a use.
            for variable in extract_used_variables(value) {
                if theme_variable {
                    self.variable_dependencies
                        .entry(variable)
                        .or_default()
                        .insert(property.clone());
                } else {
                    self.theme.mark_used_variable(&variable);
                }
            }
        }
        if property == ANIMATION_PROPERTY {
            for name in keyframe_names(value) {
                self.used_keyframe_names.insert(name.to_owned());
            }
        }
        if tracked {
            let (property, value) = (property.clone(), value.clone());
            let id = self.track_theme_variable(parent, &property, &value);
            return Step::Leaf(Built::ThemeDecl { node, id });
        }
        Step::Leaf(Built::Plain(node))
    }

    fn transform(&mut self, node: Node, parent: Id, context: &Rc<Context>, depth: usize) -> Step {
        match node {
            Node::Decl { .. } => self.declaration(node, parent, context),
            Node::Comment(_) => Step::Leaf(Built::Plain(node)),
            Node::Rule { selector, nodes } => Step::Open(self.frame(
                FrameKind::Rule { selector },
                nodes,
                Rc::clone(context),
                depth + 1,
            )),
            Node::AtRule {
                name,
                params,
                nodes,
            } if name == AT_PROPERTY && depth == 0 => {
                // Duplicate `@property` rules are dropped.
                if !self.seen_at_properties.insert(params.clone()) {
                    return Step::Skip;
                }
                let kind = FrameKind::AtRule {
                    name,
                    params,
                    always: true,
                    track_keyframes: false,
                };
                Step::Open(self.frame(kind, nodes, Rc::clone(context), depth + 1))
            }
            Node::AtRule {
                name,
                params,
                nodes,
            } => {
                let context = if name == AT_KEYFRAMES {
                    let mut inner = (**context).clone();
                    inner.insert(CONTEXT_KEYFRAMES.into(), ContextValue::Flag(true));
                    Rc::new(inner)
                } else {
                    Rc::clone(context)
                };
                // Only `@keyframes` defined in a `@theme` can be removed.
                let track_keyframes = name == AT_KEYFRAMES && truthy(&context, CONTEXT_THEME);
                let always = KEPT_IF_EMPTY_AT_RULES.contains(&name.as_str());
                let kind = FrameKind::AtRule {
                    name,
                    params,
                    always,
                    track_keyframes,
                };
                Step::Open(self.frame(kind, nodes, context, depth + 1))
            }
            Node::AtRoot(nodes) => {
                Step::Open(self.frame(FrameKind::AtRoot, nodes, Rc::clone(context), depth))
            }
            Node::Context {
                context: own,
                nodes,
            } => {
                // Reference imports are never printed.
                if truthy(&own, CONTEXT_REFERENCE) {
                    return Step::Skip;
                }
                let source = truthy(&own, CONTEXT_SOURCE);
                let mut merged = (**context).clone();
                merged.extend(own.clone());
                let merged = Rc::new(merged);
                if source {
                    // Context nodes that track their source are kept for
                    // `handle_nesting` (`@scope` composition).
                    Step::Open(self.frame(
                        FrameKind::ContextCopy { context: own },
                        nodes,
                        merged,
                        depth,
                    ))
                } else {
                    let mut frame = self.frame(FrameKind::Flatten, nodes, merged, depth);
                    frame.id = parent;
                    Step::Open(frame)
                }
            }
        }
    }

    /// A finished frame joins its parent (or the at-roots).
    fn complete(&mut self, frame: Frame, parent: Option<&mut Frame>) -> Option<Vec<Built>> {
        let Frame { kind, out, id, .. } = frame;
        let built = match kind {
            FrameKind::Root => return Some(out),
            FrameKind::AtRoot => return None,
            FrameKind::AtRootChild => {
                self.at_roots.extend(out);
                return None;
            }
            FrameKind::Flatten => {
                if let Some(parent) = parent {
                    parent.out.extend(out);
                }
                return None;
            }
            FrameKind::Rule { selector } => {
                if out.is_empty() {
                    return None;
                }
                Built::Rule {
                    selector,
                    nodes: out,
                    id,
                }
            }
            FrameKind::AtRule {
                name,
                params,
                always,
                track_keyframes,
            } => {
                if track_keyframes {
                    self.keyframes.push((id, params.clone()));
                }
                if out.is_empty() && !always {
                    return None;
                }
                Built::AtRule {
                    name,
                    params,
                    nodes: out,
                    id,
                }
            }
            FrameKind::ContextCopy { context } => {
                if out.is_empty() {
                    return None;
                }
                Built::Context {
                    context,
                    nodes: out,
                    id,
                }
            }
        };
        if let Some(parent) = parent {
            parent.out.push(built);
        }
        None
    }

    /// Transform the whole AST (Tailwind's recursive `transform`).
    fn run(&mut self, ast: Vec<Node>) -> Vec<Built> {
        let mut frames = vec![Frame {
            kind: FrameKind::Root,
            children: ast.into_iter(),
            out: Vec::new(),
            id: ROOT_ID,
            context: Rc::new(Context::new()),
            depth: 0,
        }];
        loop {
            let Some(frame) = frames.last_mut() else {
                return Vec::new();
            };
            let Some(child) = frame.children.next() else {
                let Some(done) = frames.pop() else {
                    return Vec::new();
                };
                if let Some(result) = self.complete(done, frames.last_mut()) {
                    return result;
                }
                continue;
            };
            if matches!(frame.kind, FrameKind::AtRoot) {
                let context = Rc::clone(&frame.context);
                let child = self.frame(FrameKind::AtRootChild, vec![child], context, 0);
                frames.push(child);
                continue;
            }
            let (parent, context, depth) = (frame.id, Rc::clone(&frame.context), frame.depth);
            match self.transform(child, parent, &context, depth) {
                Step::Skip => {}
                Step::Leaf(built) => {
                    if let Some(frame) = frames.last_mut() {
                        frame.out.push(built);
                    }
                }
                Step::Open(frame) => frames.push(frame),
            }
        }
    }

    /// Whether a variable, or any variable depending on it, is used
    /// (Tailwind's `isVariableUsed`; a variable reached twice counts as
    /// used, which breaks cycles).
    fn is_variable_used(&self, variable: &str) -> bool {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut stack = vec![variable];
        while let Some(variable) = stack.pop() {
            if !seen.insert(variable) {
                return true;
            }
            if self.theme.get_options(variable) & (options::STATIC | options::USED) != 0 {
                return true;
            }
            if let Some(dependencies) = self.variable_dependencies.get(variable) {
                stack.extend(dependencies.iter().map(String::as_str));
            }
        }
        false
    }

    /// Remove unused theme variables and the emptied rules (with the
    /// at-rules above them) from the main tree.
    fn remove_unused_theme_variables(&mut self, ast: &mut Vec<Built>) {
        let animate = self.theme.prefix_key(ANIMATE_NAMESPACE);
        let groups = std::mem::take(&mut self.theme_variables);
        for (container, ids) in groups {
            for id in ids {
                let Some((property, value)) = self.declarations.get(&id).cloned() else {
                    continue;
                };
                if self.is_variable_used(&property) {
                    if property.starts_with(&animate) {
                        for name in keyframe_names(&value) {
                            self.used_keyframe_names.insert(name.to_owned());
                        }
                    }
                    continue;
                }
                // Containers outside the trees (at-root scratch arrays,
                // detached rules) change invisibly.
                let Some((in_main, path)) = locate(ast, &self.at_roots, container) else {
                    continue;
                };
                let tree = if in_main {
                    &mut *ast
                } else {
                    &mut self.at_roots
                };
                let Some(nodes) = built_children_at(tree, &path) else {
                    continue;
                };
                nodes
                    .retain(|node| !matches!(node, Built::ThemeDecl { id: own, .. } if *own == id));
                if !nodes.is_empty() {
                    continue;
                }
                // An emptied rule in the main tree is removed together with
                // the at-rules above it.
                let is_rule = in_main
                    && !path.is_empty()
                    && matches!(built_node_at(ast, &path), Some(Built::Rule { .. }));
                if is_rule {
                    remove_upwards(ast, path);
                }
                break;
            }
        }
    }

    /// Remove theme `@keyframes` whose names are never used.
    fn remove_unused_keyframes(&mut self) {
        for (id, name) in std::mem::take(&mut self.keyframes) {
            if self.used_keyframe_names.contains(&name) {
                continue;
            }
            match self.at_roots.iter().position(|node| node.id() == Some(id)) {
                Some(index) => {
                    self.at_roots.remove(index);
                }
                // Tailwind's `splice(indexOf(…), 1)` removes the last
                // at-root when the keyframes are not one of them.
                None => {
                    self.at_roots.pop();
                }
            }
        }
    }
}

/// Find a container by id: `(in the main tree, index path)`; the root id is
/// the main tree itself.
fn locate(ast: &[Built], at_roots: &[Built], id: Id) -> Option<(bool, Vec<usize>)> {
    if id == ROOT_ID {
        return Some((true, Vec::new()));
    }
    if let Some(path) = find_built(ast, id) {
        return Some((true, path));
    }
    find_built(at_roots, id).map(|path| (false, path))
}

/// The index path of the node with `id` (pre-order, explicit stack).
fn find_built(nodes: &[Built], id: Id) -> Option<Vec<usize>> {
    let mut stack: Vec<(&[Built], Vec<usize>)> = vec![(nodes, Vec::new())];
    while let Some((nodes, path)) = stack.pop() {
        for (index, node) in nodes.iter().enumerate().rev() {
            let mut child = path.clone();
            child.push(index);
            if node.id() == Some(id) {
                return Some(child);
            }
            if let Some(children) = node.children() {
                stack.push((children, child));
            }
        }
    }
    None
}

fn built_node_at<'a>(nodes: &'a [Built], path: &[usize]) -> Option<&'a Built> {
    let (&first, rest) = path.split_first()?;
    let mut node = nodes.get(first)?;
    for &index in rest {
        node = node.children()?.get(index)?;
    }
    Some(node)
}

/// The children of the node at `path` (the tree itself for an empty path).
fn built_children_at<'a>(nodes: &'a mut Vec<Built>, path: &[usize]) -> Option<&'a mut Vec<Built>> {
    let mut nodes = nodes;
    for &index in path {
        nodes = nodes.get_mut(index)?.children_mut()?;
    }
    Some(nodes)
}

/// Remove the node at `path`, then each parent from its own parent, while
/// the parent is an at-rule (or the root).
fn remove_upwards(ast: &mut Vec<Built>, mut path: Vec<usize>) {
    while let Some((&last, parent)) = path.split_last() {
        let removable =
            parent.is_empty() || matches!(built_node_at(ast, parent), Some(Built::AtRule { .. }));
        if !removable {
            break;
        }
        let Some(nodes) = built_children_at(ast, parent) else {
            break;
        };
        if last < nodes.len() {
            nodes.remove(last);
        }
        path.pop();
    }
}

/// Convert the built tree back to nodes (explicit stack).
fn into_nodes(built: Vec<Built>) -> Vec<Node> {
    enum Header {
        Root,
        Rule(String),
        AtRule(String, String),
        Context(Context),
    }
    let mut stack: Vec<(Header, std::vec::IntoIter<Built>, Vec<Node>)> =
        vec![(Header::Root, built.into_iter(), Vec::new())];
    loop {
        let Some((_, children, _)) = stack.last_mut() else {
            return Vec::new();
        };
        match children.next() {
            Some(Built::Plain(node) | Built::ThemeDecl { node, .. }) => {
                if let Some((_, _, out)) = stack.last_mut() {
                    out.push(node);
                }
            }
            Some(Built::Rule {
                selector, nodes, ..
            }) => {
                stack.push((Header::Rule(selector), nodes.into_iter(), Vec::new()));
            }
            Some(Built::AtRule {
                name,
                params,
                nodes,
                ..
            }) => {
                stack.push((Header::AtRule(name, params), nodes.into_iter(), Vec::new()));
            }
            Some(Built::Context { context, nodes, .. }) => {
                stack.push((Header::Context(context), nodes.into_iter(), Vec::new()));
            }
            None => {
                let Some((header, _, out)) = stack.pop() else {
                    return Vec::new();
                };
                let node = match header {
                    Header::Root => return out,
                    Header::Rule(selector) => Node::rule(selector, out),
                    Header::AtRule(name, params) => Node::at_rule(name, params, out),
                    Header::Context(context) => Node::context(context, out),
                };
                if let Some((_, _, parent)) = stack.last_mut() {
                    parent.push(node);
                }
            }
        }
    }
}

/// Optimize the AST for printing (Tailwind's `optimizeAst` with
/// `Polyfills.None`): afterwards printing is a 1-to-1 transformation.
///
/// Theme variable usage is tracked through `theme`
/// ([`Theme::mark_used_variable`] for every variable of a non-theme value,
/// Tailwind's `designSystem.trackUsedVariables`).
#[must_use]
pub fn optimize_ast(ast: Vec<Node>, theme: &mut Theme) -> Vec<Node> {
    let mut optimizer = Optimizer {
        theme,
        next_id: ROOT_ID,
        at_roots: Vec::new(),
        seen_at_properties: BTreeSet::new(),
        theme_variables: Vec::new(),
        theme_variable_index: BTreeMap::new(),
        declarations: BTreeMap::new(),
        keyframes: Vec::new(),
        used_keyframe_names: BTreeSet::new(),
        variable_dependencies: BTreeMap::new(),
    };
    let mut built = optimizer.run(ast);
    optimizer.remove_unused_theme_variables(&mut built);
    optimizer.remove_unused_keyframes();
    built.append(&mut optimizer.at_roots);
    handle_nesting(into_nodes(built))
}

// ---------------------------------------------------------------------------
// handleNesting
// ---------------------------------------------------------------------------

/// A selector stack entry: the resolved selector, and whether it can never
/// match anything.
#[derive(Debug, Clone)]
struct Entry {
    selector: String,
    dead: bool,
}

/// Where emitted nodes currently go (Tailwind's `nodes`).
#[derive(Debug, Clone)]
enum Target {
    /// No open container: the next emit builds one.
    None,
    /// The children of the result node at this index path.
    Path(Vec<usize>),
    /// A detached array: Tailwind keeps pushing into an array that is no
    /// longer part of the result, so these nodes are lost.
    Detached,
}

/// What to do when a node's subtree is done.
enum Exit {
    None,
    Rule,
    AtRule { masked: Option<Vec<Entry>> },
    Context { pushed: bool },
}

struct Nesting {
    selector_stack: Vec<Entry>,
    at_rule_stack: Vec<(String, String)>,
    target: Target,
    seen_declaration_properties: BTreeSet<String>,
    dedupe: BTreeSet<Vec<usize>>,
    result: Vec<Node>,
    source_stack: Vec<ContextValue>,
}

/// Whether any node below is an `@scope` at-rule.
fn contains_at_scope(nodes: &[Node]) -> bool {
    let mut stack: Vec<&Node> = nodes.iter().collect();
    while let Some(node) = stack.pop() {
        if let Node::AtRule { name, .. } = node
            && name == AT_SCOPE
        {
            return true;
        }
        if let Some(children) = node.children() {
            stack.extend(children);
        }
    }
    false
}

/// The children of the result node at `path`.
fn children_at<'a>(nodes: &'a mut Vec<Node>, path: &[usize]) -> Option<&'a mut Vec<Node>> {
    let mut nodes = nodes;
    for &index in path {
        nodes = nodes.get_mut(index)?.children_mut()?;
    }
    Some(nodes)
}

/// Wrap runs of bare declarations and comments inside `@scope` (and the
/// context nodes in it) in `&` rules.
fn wrap_bare_declarations(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::new();
    let mut grouping = false;
    for child in nodes {
        match child {
            Node::Decl { .. } | Node::Comment(_) => {
                if grouping && let Some(Node::Rule { nodes, .. }) = out.last_mut() {
                    nodes.push(child);
                } else {
                    out.push(Node::rule(NESTING_SELECTOR, vec![child]));
                    grouping = true;
                }
            }
            Node::Context { context, nodes } => {
                grouping = false;
                out.push(Node::Context {
                    context,
                    nodes: wrap_bare_declarations(nodes),
                });
            }
            other => {
                grouping = false;
                out.push(other);
            }
        }
    }
    out
}

impl Nesting {
    fn last_dead(&self) -> bool {
        self.selector_stack.last().is_some_and(|entry| entry.dead)
    }

    fn enter(&mut self, node: Node) -> Option<(Vec<Node>, Exit)> {
        match node {
            Node::Rule { selector, nodes } => self.enter_rule(selector, nodes),
            Node::AtRule {
                name,
                params,
                nodes,
            } => self.enter_at_rule(name, params, nodes),
            Node::Decl { .. } | Node::Comment(_) => {
                // The contents of a rule that can never match are dropped.
                if !self.last_dead() {
                    self.emit(node);
                }
                None
            }
            Node::Context { context, nodes } => {
                let source = context
                    .get(CONTEXT_SOURCE)
                    .filter(|_| truthy(&context, CONTEXT_SOURCE));
                let pushed = source.is_some();
                if let Some(source) = source {
                    self.source_stack.push(source.clone());
                }
                Some((nodes, Exit::Context { pushed }))
            }
            Node::AtRoot(nodes) => Some((nodes, Exit::None)),
        }
    }

    fn enter_rule(&mut self, selector: String, nodes: Vec<Node>) -> Option<(Vec<Node>, Exit)> {
        self.target = Target::None;
        if self.selector_stack.is_empty() {
            // A top-level `&` is the scoping root.
            let mut resolved = selector.clone();
            if selector.contains(NESTING_SELECTOR) {
                let in_scope = self.at_rule_stack.iter().any(|(name, _)| name == AT_SCOPE);
                let replacement = if in_scope {
                    WHERE_SCOPE_SELECTOR
                } else {
                    SCOPE_SELECTOR
                };
                let mut ast = selectors::parse(&selector);
                if replace_nesting(&mut ast, replacement) {
                    resolved = selectors::to_css(&ast, false);
                }
            }
            self.selector_stack.push(Entry {
                selector: resolved,
                dead: false,
            });
        } else {
            // A rule with just `&` is replaced by its children.
            if selector == NESTING_SELECTOR {
                return Some((nodes, Exit::None));
            }
            let parent = self
                .selector_stack
                .last()
                .map(|entry| entry.selector.clone())
                .unwrap_or_default();
            let resolved: Vec<String> = segment(&selector, ',')
                .iter()
                .map(|part| substitute_nesting_selector(part, &parent))
                .collect();
            let live: Vec<&String> = resolved
                .iter()
                .filter(|part| !can_never_match(&selectors::parse(part)))
                .collect();
            // No selector can match, but nested rules still might
            // (`:is(&, .fallback)`): keep walking with a dead entry.
            if live.is_empty() {
                self.selector_stack.push(Entry {
                    selector: resolved.join(LIST_SEPARATOR),
                    dead: true,
                });
                return Some((nodes, Exit::Rule));
            }
            let live: Vec<&str> = live.into_iter().map(String::as_str).collect();
            self.selector_stack.push(Entry {
                selector: live.join(LIST_SEPARATOR),
                dead: false,
            });
        }
        // A rule with a declaration stops the flattening: its children are
        // emitted as-is (unless an `@scope` must be hoisted from it).
        if nodes.iter().any(|child| matches!(child, Node::Decl { .. }))
            && !contains_at_scope(&nodes)
        {
            for child in nodes {
                self.emit(child);
            }
            return Some((Vec::new(), Exit::Rule));
        }
        Some((nodes, Exit::Rule))
    }

    fn enter_at_rule(
        &mut self,
        name: String,
        params: String,
        nodes: Vec<Node>,
    ) -> Option<(Vec<Node>, Exit)> {
        self.target = Target::None;
        let hoistable = HOISTABLE_AT_RULES.contains(&name.as_str());
        if self.last_dead() {
            // An empty `@layer` establishes the layer order: keep it, without
            // the selector that can never match.
            if name == AT_LAYER && nodes.is_empty() {
                let dead = std::mem::take(&mut self.selector_stack);
                self.emit(Node::at_rule(name, params, nodes));
                self.selector_stack = dead;
                return None;
            }
            if nodes.is_empty() || !hoistable {
                return None;
            }
        }
        if nodes.is_empty() && !DROPPABLE_IF_EMPTY_AT_RULES.contains(&name.as_str()) {
            self.emit(Node::at_rule(name, params, nodes));
            return None;
        }
        if !hoistable {
            self.emit(Node::at_rule(name, params, nodes));
            return None;
        }
        let mut params = params;
        let mut nodes = nodes;
        let mut masked = None;
        if name == AT_SCOPE {
            nodes = wrap_bare_declarations(nodes);
            match self.resolve_scope_prelude(&params) {
                ScopePrelude::Unchanged => {}
                ScopePrelude::Dead => return None,
                ScopePrelude::Resolved { prelude, mask } => {
                    params = prelude;
                    if mask {
                        masked = Some(std::mem::take(&mut self.selector_stack));
                    }
                }
            }
        }
        self.at_rule_stack.push((name, params));
        Some((nodes, Exit::AtRule { masked }))
    }

    /// Resolve the `@scope` prelude against the parent rule (variant
    /// `@scope` rules attach the parent to the rules inside; user-authored
    /// ones keep the native CSS nesting semantics, CSS Nesting 1 §3.3).
    fn resolve_scope_prelude(&self, params: &str) -> ScopePrelude {
        let is_variant = matches!(self.source_stack.last(), Some(ContextValue::Text(source)) if source == SOURCE_VARIANT);
        let has_parent = !self.selector_stack.is_empty();
        if !(params.contains(NESTING_SELECTOR) || (!is_variant && has_parent)) {
            return ScopePrelude::Unchanged;
        }
        let parent = self
            .selector_stack
            .last()
            .map(|entry| entry.selector.clone());
        let mut ast = value_parser::parse(params);
        let mut is_scope_end = false;
        let mut consumed_parent = false;
        let mut scope_start: Option<String> = None;
        for node in &mut ast {
            if matches!(node, ValueNode::Word(word) if word == SCOPE_END_KEYWORD) {
                is_scope_end = true;
                continue;
            }
            // A `(…)` group parses as a function without a name.
            let ValueNode::Function { name, nodes } = node else {
                continue;
            };
            if !name.is_empty() {
                continue;
            }
            let selector = value_parser::to_css(nodes);
            if let Some(parent) = parent.as_deref().filter(|_| !is_scope_end && !is_variant) {
                let resolved: Vec<String> = segment(&selector, ',')
                    .iter()
                    .map(|part| {
                        substitute_nesting_selector(crate::css_parser::js_trim(part), parent)
                    })
                    .collect();
                *nodes = vec![ValueNode::Word(resolved.join(LIST_SEPARATOR))];
                consumed_parent = true;
            } else if selector.contains(NESTING_SELECTOR) {
                let replacement = if is_scope_end {
                    WHERE_SCOPE_SELECTOR
                } else {
                    parent.as_deref().unwrap_or(SCOPE_SELECTOR)
                };
                *nodes = vec![ValueNode::Word(substitute_nesting_selector(
                    &selector,
                    replacement,
                ))];
                if !is_scope_end && has_parent {
                    consumed_parent = true;
                }
            }
            if !is_scope_end {
                scope_start = Some(value_parser::to_css(nodes));
            }
        }
        // Nothing can be in scope when the scope start consumed a parent
        // that can never match.
        if consumed_parent
            && let Some(start) = &scope_start
            && self.last_dead()
            && can_never_match(&selectors::parse(start))
        {
            return ScopePrelude::Dead;
        }
        ScopePrelude::Resolved {
            prelude: value_parser::to_css(&ast),
            mask: consumed_parent,
        }
    }

    fn exit(&mut self, exit: Exit) {
        match exit {
            Exit::None => {}
            Exit::Rule => {
                self.target = Target::None;
                self.selector_stack.pop();
            }
            Exit::AtRule { masked } => {
                self.target = Target::None;
                self.at_rule_stack.pop();
                if let Some(masked) = masked {
                    self.selector_stack = masked;
                }
            }
            Exit::Context { pushed } => {
                if pushed {
                    self.source_stack.pop();
                }
            }
        }
    }

    fn track_declaration(&mut self, node: &Node, path: Option<&Vec<usize>>) {
        if let Node::Decl { property, .. } = node
            && !self.seen_declaration_properties.insert(property.clone())
            && let Some(path) = path
        {
            self.dedupe.insert(path.clone());
        }
    }

    fn emit(&mut self, node: Node) {
        match self.target.clone() {
            Target::Path(path) => {
                self.track_declaration(&node, Some(&path));
                if let Some(nodes) = children_at(&mut self.result, &path) {
                    nodes.push(node);
                }
                return;
            }
            Target::Detached => {
                self.track_declaration(&node, None);
                return;
            }
            Target::None => {}
        }

        // No parent rules or at-rules: emit as-is, ignoring a repeated
        // body-less at-rule.
        if self.selector_stack.is_empty() && self.at_rule_stack.is_empty() {
            if let (
                Some(Node::AtRule {
                    name: last_name,
                    params: last_params,
                    nodes: last_nodes,
                }),
                Node::AtRule {
                    name,
                    params,
                    nodes,
                },
            ) = (self.result.last(), &node)
                && last_nodes.is_empty()
                && nodes.is_empty()
                && last_name == name
                && last_params == params
            {
                return;
            }
            self.result.push(node);
            return;
        }

        self.seen_declaration_properties.clear();
        if let Node::Decl { property, .. } = &node {
            self.seen_declaration_properties.insert(property.clone());
        }
        let nodes = vec![node];

        // Merge into the last emitted at-rules when they match the stack.
        let mut target_path: Vec<usize> = Vec::new();
        let mut offset = 0;
        {
            let mut target: &Vec<Node> = &self.result;
            if matches!(target.last(), Some(Node::AtRule { .. })) {
                for (name, params) in &self.at_rule_stack {
                    let Some(Node::AtRule {
                        name: last_name,
                        params: last_params,
                        nodes: children,
                    }) = target.last()
                    else {
                        break;
                    };
                    if last_name != name || last_params != params {
                        break;
                    }
                    offset += 1;
                    target_path.push(target.len() - 1);
                    target = children;
                }
            }
        }

        let mut root: Option<Node> = None;
        let mut nodes = Some(nodes);
        if let Some(entry) = self.selector_stack.last() {
            // Merge into an adjacent rule with the same selector.
            if self.at_rule_stack.len() <= offset
                && let Some(target) = children_at(&mut self.result, &target_path)
                && let Some(Node::Rule {
                    selector,
                    nodes: last_nodes,
                }) = target.last_mut()
                && *selector == entry.selector
            {
                last_nodes.extend(nodes.take().unwrap_or_default());
                let mut path = target_path;
                path.push(target.len() - 1);
                self.dedupe.insert(path.clone());
                self.target = Target::Path(path);
                return;
            }
            root = Some(Node::rule(
                entry.selector.clone(),
                nodes.take().unwrap_or_default(),
            ));
        }
        // The container of `nodes` is this many first-child steps below the
        // pushed root: the rule below every wrapper, or else the innermost
        // wrapper itself.
        let wrappers = &self.at_rule_stack[offset.min(self.at_rule_stack.len())..];
        let steps = if root.is_some() {
            wrappers.len()
        } else {
            wrappers.len().saturating_sub(1)
        };
        for (name, params) in wrappers.iter().rev() {
            let children = match root.take() {
                Some(root) => vec![root],
                None => nodes.take().unwrap_or_default(),
            };
            root = Some(Node::at_rule(name.clone(), params.clone(), children));
        }
        let Some(target) = children_at(&mut self.result, &target_path) else {
            return;
        };
        match root {
            Some(root) => {
                target.push(root);
                let mut path = target_path;
                path.push(target.len() - 1);
                path.extend(std::iter::repeat_n(0, steps));
                self.target = Target::Path(path);
            }
            None => {
                target.extend(nodes.take().unwrap_or_default());
                self.target = Target::Detached;
            }
        }
    }

    /// Drop repeated declarations (same property, value and importance),
    /// keeping the last one, in every container that may hold them. Deeper
    /// containers go first so removals never shift a pending path.
    fn dedupe(&mut self) {
        let paths = std::mem::take(&mut self.dedupe);
        for path in paths.iter().rev() {
            let Some(nodes) = children_at(&mut self.result, path) else {
                continue;
            };
            let mut seen: BTreeSet<(String, Option<String>, bool)> = BTreeSet::new();
            let mut index = nodes.len();
            while index > 0 {
                index -= 1;
                if let Node::Decl {
                    property,
                    value,
                    important,
                } = &nodes[index]
                    && !seen.insert((property.clone(), value.clone(), *important))
                {
                    nodes.remove(index);
                }
            }
        }
    }
}

/// The outcome of resolving an `@scope` prelude.
enum ScopePrelude {
    Unchanged,
    /// The scope can never match: drop the rule and its subtree.
    Dead,
    /// The resolved prelude; `mask` when the parent selector was consumed,
    /// so the rules inside resolve against the scoping root.
    Resolved {
        prelude: String,
        mask: bool,
    },
}

/// Replace every `&` simple selector by `replacement`; true when one was
/// found.
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

/// Flatten nested style rules into the rules and at-rules Tailwind prints
/// (Tailwind's `handleNesting`). Rules that contain declarations keep their
/// nested children as-is; at-rules that depend only on the environment are
/// hoisted above the style rules; `@scope` rules follow CSS Nesting 1 §3.3
/// (or attach the parent to their contents when a variant generated them).
#[must_use]
pub fn handle_nesting(ast: Vec<Node>) -> Vec<Node> {
    let mut nesting = Nesting {
        selector_stack: Vec::new(),
        at_rule_stack: Vec::new(),
        target: Target::None,
        seen_declaration_properties: BTreeSet::new(),
        dedupe: BTreeSet::new(),
        result: Vec::new(),
        source_stack: Vec::new(),
    };
    let mut stack: Vec<(std::vec::IntoIter<Node>, Exit)> = vec![(ast.into_iter(), Exit::None)];
    while let Some((children, _)) = stack.last_mut() {
        match children.next() {
            Some(node) => {
                if let Some((children, exit)) = nesting.enter(node) {
                    stack.push((children.into_iter(), exit));
                }
            }
            None => {
                if let Some((_, exit)) = stack.pop() {
                    nesting.exit(exit);
                }
            }
        }
    }
    nesting.dedupe();
    nesting.result
}

// ---------------------------------------------------------------------------
// Selector helpers
// ---------------------------------------------------------------------------

/// The kind of a selector node with children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentKind {
    List,
    Complex,
    Compound,
    Function,
}

fn parent_kind(node: &SelectorNode) -> Option<ParentKind> {
    match node {
        SelectorNode::List(_) => Some(ParentKind::List),
        SelectorNode::Complex(_) => Some(ParentKind::Complex),
        SelectorNode::Compound(_) => Some(ParentKind::Compound),
        SelectorNode::Function { .. } => Some(ParentKind::Function),
        SelectorNode::Combinator(_) | SelectorNode::Selector(_) | SelectorNode::Value(_) => None,
    }
}

/// The walk context of a selector node: its parent's kind, and the kind of
/// the grandparent with whether the parent is its first child.
#[derive(Debug, Clone, Copy)]
struct Parent {
    kind: ParentKind,
    grandparent: Option<(ParentKind, bool)>,
}

/// The parent selector, parsed once.
struct NestingParent<'a> {
    selector: &'a str,
    ast: Vec<SelectorNode>,
}

impl NestingParent<'_> {
    fn is_list(&self) -> bool {
        matches!(self.ast.as_slice(), [SelectorNode::List(_)])
    }
    fn first_is_complex(&self) -> bool {
        matches!(self.ast.first(), Some(SelectorNode::Complex(_)))
    }
    fn first_is_type_or_universal(&self) -> bool {
        self.ast.first().is_some_and(|node| {
            selectors::is_universal_selector(node) || selectors::is_type_selector(node)
        })
    }
}

/// Substitute `&` by the parent selector with `:is(…)` semantics, dropping
/// the `:is(…)` where that is provably equivalent (CSS Nesting 1 §3); a
/// selector without `&` is prefixed with the parent and a descendant
/// combinator (Tailwind's `substituteNestingSelector`).
#[must_use]
pub fn substitute_nesting_selector(selector: &str, parent_selector: &str) -> String {
    let parent = NestingParent {
        selector: parent_selector,
        ast: selectors::parse(parent_selector),
    };
    if selector.contains(NESTING_SELECTOR) {
        let mut ast = selectors::parse(selector);
        let mut changed = false;
        substitute_in(&mut ast, None, &parent, &mut changed);
        if changed {
            return selectors::to_css(&ast, false);
        }
    }
    if parent.is_list() {
        format!(":is({parent_selector}) {selector}")
    } else {
        format!("{parent_selector} {selector}")
    }
}

/// Walk `nodes` like Tailwind's `walk` (enter, children, exit), replacing
/// `&` on enter and dropping a redundant leading `*` of a compound on exit.
fn substitute_in(
    nodes: &mut Vec<SelectorNode>,
    parent: Option<Parent>,
    nesting: &NestingParent<'_>,
    changed: &mut bool,
) {
    let mut index = 0;
    while index < nodes.len() {
        if matches!(&nodes[index], SelectorNode::Selector(value) if value == NESTING_SELECTOR) {
            *changed = true;
            let value = nesting_replacement(nodes, index, parent, nesting);
            nodes[index] = SelectorNode::Selector(value);
        }
        if let Some(kind) = parent_kind(&nodes[index]) {
            let context = Parent {
                kind,
                grandparent: parent.map(|parent| (parent.kind, index == 0)),
            };
            if let Some(children) = nodes[index].children_mut()
                && !children.is_empty()
            {
                substitute_in(children, Some(context), nesting, changed);
            }
        }
        // `*:hover` → `:hover`, except a namespace (`*|div`).
        if index == 0
            && nodes.len() > 1
            && parent.is_some_and(|parent| parent.kind == ParentKind::Compound)
            && selectors::is_universal_selector(&nodes[index])
            && !matches!(&nodes[1], SelectorNode::Selector(next) if next.starts_with(NAMESPACE_SEPARATOR))
        {
            nodes.remove(index);
            continue;
        }
        index += 1;
    }
}

/// The replacement of the `&` at `index` among `siblings`.
fn nesting_replacement(
    siblings: &[SelectorNode],
    index: usize,
    parent: Option<Parent>,
    nesting: &NestingParent<'_>,
) -> String {
    let wrapped = format!(":is({})", nesting.selector);
    let plain = nesting.selector.to_owned();
    let Some(parent) = parent else {
        return wrapped;
    };
    // Several parent selectors always need `:is(…)`.
    if nesting.is_list() {
        return wrapped;
    }
    match parent.kind {
        // `& [after]` never needs `:is(…)`; `[before] &` and
        // `[before] & [after]` need it for a complex parent.
        ParentKind::Complex => {
            if index == 0 || !nesting.first_is_complex() {
                plain
            } else {
                wrapped
            }
        }
        ParentKind::Compound => {
            // A complex parent inside a compound that is not the first of a
            // complex selector keeps `:is(…)` (`.system &:focus`).
            if nesting.first_is_complex()
                && parent.grandparent == Some((ParentKind::Complex, false))
            {
                return wrapped;
            }
            // `&*` and `&div` must stay invalid.
            if siblings[index + 1..].iter().any(|sibling| {
                selectors::is_universal_selector(sibling) || selectors::is_type_selector(sibling)
            }) {
                return wrapped;
            }
            if index == 0 {
                return plain;
            }
            if nesting.first_is_complex() || nesting.first_is_type_or_universal() {
                wrapped
            } else {
                plain
            }
        }
        // `:not(&)`: a single parent selector is a valid argument.
        ParentKind::Function => plain,
        ParentKind::List => wrapped,
    }
}

/// Whether a parsed selector can never match: a combinator after a
/// standard pseudo-element (Tailwind's `canNeverMatch`).
#[must_use]
pub fn can_never_match(ast: &[SelectorNode]) -> bool {
    !ast.is_empty() && ast.iter().all(is_dead_selector)
}

fn is_standard_pseudo_element(node: &SelectorNode) -> bool {
    if !matches!(
        node,
        SelectorNode::Selector(_) | SelectorNode::Function { .. }
    ) {
        return false;
    }
    if !selectors::is_pseudo_element(node) {
        return false;
    }
    let value = node.value().unwrap_or_default().to_lowercase();
    // Legacy single-colon and vendor-prefixed pseudo-elements.
    if !value.starts_with("::") || value.starts_with("::-") {
        return true;
    }
    STANDARD_PSEUDO_ELEMENTS.contains(&value.as_str())
}

fn is_dead_selector(node: &SelectorNode) -> bool {
    match node {
        SelectorNode::List(nodes) => nodes.iter().all(is_dead_selector),
        SelectorNode::Complex(nodes) => {
            for (index, child) in nodes.iter().enumerate() {
                if matches!(child, SelectorNode::Combinator(_)) {
                    let Some(previous) = index
                        .checked_sub(1)
                        .and_then(|previous| nodes.get(previous))
                    else {
                        continue;
                    };
                    let after_pseudo_element = is_standard_pseudo_element(previous)
                        || matches!(previous, SelectorNode::Compound(parts) if parts.iter().any(is_standard_pseudo_element));
                    if after_pseudo_element {
                        return true;
                    }
                } else if is_dead_selector(child) {
                    return true;
                }
            }
            false
        }
        SelectorNode::Compound(nodes) => nodes.iter().any(is_dead_selector),
        // `:is(…)` and `:where(…)` take forgiving selector lists.
        SelectorNode::Function { value, nodes } if value == ":is" || value == ":where" => {
            can_never_match(nodes)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_parser::parse;
    use crate::printer::to_css;

    /// Strip the common indentation of an inline snapshot.
    fn unindent(text: &str) -> String {
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let indent = lines
            .iter()
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);
        lines
            .iter()
            .map(|line| &line[indent..])
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Replace `@in-variant` at-rules by `source: variant` contexts (the
    /// test hook of Tailwind's `ast.test.ts`).
    fn mark_variants(nodes: Vec<Node>) -> Vec<Node> {
        nodes
            .into_iter()
            .map(|node| match node {
                Node::AtRule { name, nodes, .. } if name == "@in-variant" => Node::context(
                    BTreeMap::from([(
                        CONTEXT_SOURCE.to_owned(),
                        ContextValue::Text(SOURCE_VARIANT.into()),
                    )]),
                    mark_variants(nodes),
                ),
                Node::Rule { selector, nodes } => Node::rule(selector, mark_variants(nodes)),
                Node::AtRule {
                    name,
                    params,
                    nodes,
                } => Node::at_rule(name, params, mark_variants(nodes)),
                other => other,
            })
            .collect()
    }

    fn optimize(input: &str) -> String {
        to_css(&handle_nesting(mark_variants(parse(input).unwrap())))
    }

    fn check(input: &str, expected: &str) {
        assert_eq!(
            optimize(input).trim(),
            unindent(expected).trim(),
            "input: {input}"
        );
    }

    fn optimize_with(input: &str, theme: &mut Theme) -> String {
        to_css(&optimize_ast(parse(input).unwrap(), theme))
    }

    #[test]
    fn does_not_emit_empty_rules_once_optimized() {
        let input = r#"
            /* Empty rule */
            .foo {
            }
            .foo {
              .bar {
              }
              .baz {
              }
            }
            .foo {
              & {
                &:hover {
                }
                &:focus {
                }
              }
            }
            @media (min-width: 768px) {
            }
            @media (min-width: 768px) {
              .foo {
              }
              @media (min-width: 1024px) {
                .bar {
                }
              }
            }
            @charset "UTF-8";
            @layer foo, bar, baz;
            @layer foo, bar, baz; /* Will be deduped */
            @custom-media --modern (color), (hover);
            @namespace 'http://www.w3.org/1999/xhtml';
            @import url('https://fonts.googleapis.com/css2?family=Cedarville+Cursive&display=swap');
        "#;
        assert_eq!(
            optimize_with(input, &mut Theme::new()).trim(),
            unindent(
                r#"
                @charset "UTF-8";
                @layer foo, bar, baz;
                @custom-media --modern (color), (hover);
                @namespace 'http://www.w3.org/1999/xhtml';
                @import url('https://fonts.googleapis.com/css2?family=Cedarville+Cursive&display=swap');
                "#
            )
        );
    }

    #[test]
    fn does_not_emit_exact_duplicate_declarations() {
        let input = "
            .foo {
              color: red;
              .bar {
                color: green;
                color: blue;
                color: green;
              }
              color: red;
            }
            .foo {
              color: red;
              & {
                color: green;
                & {
                  color: red;
                  color: green;
                  color: blue;
                }
                color: red;
              }
              background: blue;
              .bar {
                color: green;
                color: blue;
                color: green;
              }
              caret-color: orange;
            }
        ";
        assert_eq!(
            optimize_with(input, &mut Theme::new()).trim(),
            unindent(
                "
                .foo {
                  .bar {
                    color: green;
                    color: blue;
                    color: green;
                  }
                  color: red;
                  & {
                    color: green;
                    & {
                      color: red;
                      color: green;
                      color: blue;
                    }
                    color: red;
                  }
                  background: blue;
                  .bar {
                    color: green;
                    color: blue;
                    color: green;
                  }
                  caret-color: orange;
                }
                "
            )
        );
    }

    #[test]
    fn keeps_keyframes_and_tracks_used_variables() {
        let input = "
            @keyframes my-animation {
              0% {
                color: color-mix(in oklab, var(--color-emerald-600) 0%, transparent);
              }
            }
        ";
        let mut theme = Theme::new();
        theme
            .add(
                "--color-emerald-600",
                "oklch(59.6% 0.145 163.225)",
                options::NONE,
            )
            .unwrap();
        let css = optimize_with(input, &mut theme);
        assert!(css.starts_with("@keyframes my-animation {\n  0% {\n    color: color-mix("));
        assert_ne!(theme.get_options("--color-emerald-600") & options::USED, 0);
    }

    fn theme_context(nodes: Vec<Node>) -> Node {
        Node::context(
            BTreeMap::from([(CONTEXT_THEME.to_owned(), ContextValue::Flag(true))]),
            nodes,
        )
    }

    #[test]
    fn removes_unused_theme_variables_and_keyframes() {
        let mut theme = Theme::new();
        for (key, value) in [
            ("--color-red", "red"),
            ("--color-blue", "blue"),
            ("--color-alias", "var(--color-blue)"),
            ("--animate-spin", "spin 1s linear infinite"),
        ] {
            theme.add(key, value, options::NONE).unwrap();
        }
        theme
            .add("--font-static", "serif", options::STATIC)
            .unwrap();
        let keyframes = |name: &str| {
            Node::at_rule(
                "@keyframes",
                name,
                vec![Node::rule(
                    "to",
                    vec![Node::decl("transform", "rotate(360deg)")],
                )],
            )
        };
        let ast = vec![
            theme_context(vec![
                Node::at_rule(
                    "@layer",
                    "theme",
                    vec![Node::rule(
                        ":root, :host",
                        vec![
                            Node::decl("--color-red", "red"),
                            Node::decl("--color-blue", "blue"),
                            Node::decl("--color-alias", "var(--color-blue)"),
                            Node::decl("--animate-spin", "spin 1s linear infinite"),
                            Node::decl("--font-static", "serif"),
                            Node::decl("--unset", "initial"),
                        ],
                    )],
                ),
                Node::at_root(vec![keyframes("spin"), keyframes("ping")]),
            ]),
            Node::rule(
                ".a",
                vec![
                    Node::decl("color", "var(--color-alias)"),
                    Node::decl("animation", "var(--animate-spin)"),
                    Node::decl("--tw-sort", "1"),
                ],
            ),
        ];
        let css = to_css(&optimize_ast(ast, &mut theme));
        assert_eq!(
            css.trim(),
            unindent(
                "
                @layer theme {
                  :root, :host {
                    --color-blue: blue;
                    --color-alias: var(--color-blue);
                    --animate-spin: spin 1s linear infinite;
                    --font-static: serif;
                  }
                }
                .a {
                  color: var(--color-alias);
                  animation: var(--animate-spin);
                }
                @keyframes spin {
                  to {
                    transform: rotate(360deg);
                  }
                }
                "
            )
        );
    }

    #[test]
    fn removes_emptied_theme_rules_with_their_layers() {
        let mut theme = Theme::new();
        theme.add("--color-red", "red", options::NONE).unwrap();
        let ast = vec![
            theme_context(vec![Node::at_rule(
                "@layer",
                "theme",
                vec![Node::rule(":root", vec![Node::decl("--color-red", "red")])],
            )]),
            Node::rule(".b", vec![Node::decl("color", "blue")]),
            Node::context(
                BTreeMap::from([(CONTEXT_REFERENCE.to_owned(), ContextValue::Flag(true))]),
                vec![Node::rule(".hidden", vec![Node::decl("color", "red")])],
            ),
            Node::at_rule("@property", "--x", vec![Node::decl("syntax", "'*'")]),
            Node::at_rule("@property", "--x", vec![Node::decl("syntax", "'*'")]),
        ];
        assert_eq!(
            to_css(&optimize_ast(ast, &mut theme)).trim(),
            unindent(
                "
                .b {
                  color: blue;
                }
                @property --x {
                  syntax: '*';
                }
                "
            )
        );
    }

    #[test]
    fn uses_the_descendant_combinator_by_default() {
        check(
            "
            .a {
              element { --x: 1; }
              .class { --x: 2; }
              #id { --x: 3; }
              :pseudo-class { --x: 4; }
              ::pseudo-element { --x: 5; }
              [attribute] { --x: 6; }
              * { --x: 7; }
            }
            ",
            "
            .a element {
              --x: 1;
            }
            .a .class {
              --x: 2;
            }
            .a #id {
              --x: 3;
            }
            .a :pseudo-class {
              --x: 4;
            }
            .a ::pseudo-element {
              --x: 5;
            }
            .a [attribute] {
              --x: 6;
            }
            .a * {
              --x: 7;
            }
            ",
        );
        check(
            ".a { + .b { --x: 1; } > .c { --x: 2; } ~ .d { --x: 3; } }",
            "
            .a + .b {
              --x: 1;
            }
            .a > .c {
              --x: 2;
            }
            .a ~ .d {
              --x: 3;
            }
            ",
        );
    }

    #[test]
    fn replaces_a_top_level_nesting_selector_with_scope() {
        check(
            "
            & { --x: 1; }
            @supports (--y: 1) { & { --x: 2; } }
            :is(&) { --x: 3; }
            &, .a { --x: 5; }
            @supports (--y: 4) { :is(&), .d { --x: 8; } }
            ",
            "
            :scope {
              --x: 1;
            }
            @supports (--y: 1) {
              :scope {
                --x: 2;
              }
            }
            :is(:scope) {
              --x: 3;
            }
            :scope, .a {
              --x: 5;
            }
            @supports (--y: 4) {
              :is(:scope), .d {
                --x: 8;
              }
            }
            ",
        );
    }

    #[test]
    fn substitutes_the_nesting_selector() {
        check(
            "
            .a {
              & + .b { --x: 1; }
              .c > & { --x: 2; }
              & { --x: 3; }
              :is(:is(:is(&))) { --x: 4; }
              .b-\\& { --x: 5; }
              [data-b='c&d'] { --x: 6; }
              & *|div { --x: 7; }
              & .b & .c & .d { --x: 8; }
              &&& { --x: 9; }
              :not(&) { --x: 10; }
            }
            ",
            "
            .a + .b {
              --x: 1;
            }
            .c > .a {
              --x: 2;
            }
            .a {
              --x: 3;
            }
            :is(:is(:is(.a))) {
              --x: 4;
            }
            .a .b-\\& {
              --x: 5;
            }
            .a [data-b='c&d'] {
              --x: 6;
            }
            .a *|div {
              --x: 7;
            }
            .a .b .a .c .a .d {
              --x: 8;
            }
            .a.a.a {
              --x: 9;
            }
            :not(.a) {
              --x: 10;
            }
            ",
        );
    }

    #[test]
    fn handles_parent_selector_lists_and_deep_nesting() {
        check(
            ".a,\n.b {\n  .c,\n  .d & {\n    --x: 1;\n    &:hover {\n      --x: 2;\n    }\n  }\n}",
            "
            :is(.a, .b) .c, .d :is(.a, .b) {
              --x: 1;
              &:hover {
                --x: 2;
              }
            }
            ",
        );
        check(
            ".a {\n  --before: 1;\n  &:hover {\n    --inside: 1;\n  }\n  --after: 1;\n}",
            "
            .a {
              --before: 1;
              &:hover {
                --inside: 1;
              }
              --after: 1;
            }
            ",
        );
    }

    /// `'%s { %s }' → '%s'`: one rule with one declaration.
    fn check_pair(root: &str, nested: &str, expected: &str) {
        let input = to_css(&[Node::rule(
            root,
            vec![Node::rule(nested, vec![Node::decl("--x", "0")])],
        )]);
        let ast = parse(&optimize(&input)).unwrap();
        let mut count = 0;
        crate::ast::for_each(&ast, &mut |_, _| count += 1);
        assert_eq!(count, 2, "{root} {{ {nested} }}");
        let Some(Node::Rule { selector, .. }) = ast.first() else {
            panic!("expected a rule for {root} {{ {nested} }}");
        };
        assert_eq!(selector, expected, "{root} {{ {nested} }}");
    }

    #[test]
    fn substitutes_into_compound_selectors() {
        let cases: &[(&str, &[(&str, &str)])] = &[
            (
                "element",
                &[
                    ("&element", ":is(element)element"),
                    ("element&", "element:is(element)"),
                    ("&.class", "element.class"),
                    (".class&", ".class:is(element)"),
                    ("&#id", "element#id"),
                    ("#id&", "#id:is(element)"),
                    ("&:hover", "element:hover"),
                    (":hover&", ":hover:is(element)"),
                    ("&::before", "element::before"),
                    ("::before&", "::before:is(element)"),
                    ("&:not(.x)", "element:not(.x)"),
                    (":not(.x)&", ":not(.x):is(element)"),
                    ("&[attribute]", "element[attribute]"),
                    ("[attribute]&", "[attribute]:is(element)"),
                    ("&*", ":is(element)*"),
                    ("*&", "element"),
                ],
            ),
            (
                ".class",
                &[
                    ("&element", ":is(.class)element"),
                    ("element&", "element.class"),
                    ("&.class", ".class.class"),
                    (".class&", ".class.class"),
                    ("#id&", "#id.class"),
                    (":hover&", ":hover.class"),
                    ("::before&", "::before.class"),
                    (":not(.x)&", ":not(.x).class"),
                    ("[attribute]&", "[attribute].class"),
                    ("&*", ":is(.class)*"),
                    ("*&", ".class"),
                ],
            ),
            (
                "::before",
                &[
                    ("&element", ":is(::before)element"),
                    ("&:hover", "::before:hover"),
                    (":hover&", ":hover::before"),
                    ("*&", "::before"),
                ],
            ),
            (
                "*",
                &[
                    ("&element", ":is(*)element"),
                    ("element&", "element:is(*)"),
                    ("&.class", ".class"),
                    (".class&", ".class:is(*)"),
                    ("&#id", "#id"),
                    ("&:hover", ":hover"),
                    (":hover&", ":hover:is(*)"),
                    ("&::before", "::before"),
                    ("&:not(.x)", ":not(.x)"),
                    ("&[attribute]", "[attribute]"),
                    ("[attribute]&", "[attribute]:is(*)"),
                    ("&*", ":is(*)*"),
                    ("*&", "*"),
                ],
            ),
            (
                "&",
                &[
                    ("&element", ":is(:scope)element"),
                    ("element&", "element:scope"),
                    ("&.class", ":scope.class"),
                    ("::before&", "::before:scope"),
                    ("&*", ":is(:scope)*"),
                    ("*&", ":scope"),
                ],
            ),
        ];
        for (root, pairs) in cases {
            for (nested, expected) in *pairs {
                check_pair(root, nested, expected);
            }
        }
    }

    #[test]
    fn splits_rules_to_guarantee_specificity() {
        for (root, nested, expected) in [
            ("div", "&", "div"),
            ("div", "[before]&", "[before]:is(div)"),
            ("div", "&[after]", "div[after]"),
            ("div", "[before]&[after]", "[before]:is(div)[after]"),
            ("div", "[before] &", "[before] div"),
            ("div", "& [after]", "div [after]"),
            ("div", "[before] & [after]", "[before] div [after]"),
            (".parent", "[before]&", "[before].parent"),
            (".parent", "[before]&[after]", "[before].parent[after]"),
            (".parent", "[before] & [after]", "[before] .parent [after]"),
            (".a > .b", "&", ".a > .b"),
            (".a > .b", "[before]&", "[before]:is(.a > .b)"),
            (".a > .b", "&[after]", ".a > .b[after]"),
            (".a > .b", "[before]&[after]", "[before]:is(.a > .b)[after]"),
            (".a > .b", "[before] &", "[before] :is(.a > .b)"),
            (".a > .b", "& [after]", ".a > .b [after]"),
            (
                ".a > .b",
                "[before] & [after]",
                "[before] :is(.a > .b) [after]",
            ),
        ] {
            check_pair(root, nested, expected);
        }
    }

    #[test]
    fn hoists_at_rules() {
        check(
            "
            @layer utilities {
              .a,
              .b {
                @media (print) {
                  --x: 1;
                  .c {
                    @media (min-width: 123px) {
                      --x: 2;
                    }
                  }
                }
              }
              .d {
                @media (print) {
                  @media (min-width: 123px) {
                    --x: 3;
                  }
                }
              }
            }
            @property --foo {
              syntax: '*';
            }
            @layer utilities {
              .e {
                @media (print) {
                  --x: 4;
                }
              }
            }
            ",
            "
            @layer utilities {
              @media (print) {
                .a, .b {
                  --x: 1;
                }
                @media (min-width: 123px) {
                  :is(.a, .b) .c {
                    --x: 2;
                  }
                  .d {
                    --x: 3;
                  }
                }
              }
            }
            @property --foo {
              syntax: '*';
            }
            @layer utilities {
              @media (print) {
                .e {
                  --x: 4;
                }
              }
            }
            ",
        );
    }

    #[test]
    fn hoists_variant_scope_rules() {
        check(
            "@layer utilities { .parent { @in-variant { @scope (.from) to (.to) { --x: 1; } } } }",
            "
            @layer utilities {
              @scope (.from) to (.to) {
                .parent {
                  --x: 1;
                }
              }
            }
            ",
        );
        check(
            "@layer utilities { .parent { --x: 1; @in-variant { @scope (.from) { .inside { --y: 2; } } } } }",
            "
            @layer utilities {
              .parent {
                --x: 1;
              }
              @scope (.from) {
                .parent .inside {
                  --y: 2;
                }
              }
            }
            ",
        );
        check(
            ".parent { @in-variant { @scope (.from) { --x: 1; } } }",
            "
            @scope (.from) {
              .parent {
                --x: 1;
              }
            }
            ",
        );
    }

    #[test]
    fn resolves_the_scope_prelude() {
        check(
            ".parent { @scope (& > .scope) to (& .limit) { .content { --x: 1; } } }",
            "
            @scope (.parent > .scope) to (:where(:scope) .limit) {
              .content {
                --x: 1;
              }
            }
            ",
        );
        check(
            ".parent-a,\n.parent-b { @scope (& > .from) { .inside { --x: 1; } } }",
            "
            @scope (:is(.parent-a, .parent-b) > .from) {
              .inside {
                --x: 1;
              }
            }
            ",
        );
        check(
            ".parent { @scope to (& .to) { .inside { --x: 1; } } }",
            "
            @scope to (:where(:scope) .to) {
              .parent .inside {
                --x: 1;
              }
            }
            ",
        );
        check(
            "main { @scope (.card&) { .inside { --x: 1; } } }",
            "
            @scope (.card:is(main)) {
              .inside {
                --x: 1;
              }
            }
            ",
        );
        check(
            ".parent { @scope (.a, & > .b) { .inside { --x: 1; } } }",
            "
            @scope (.parent .a, .parent > .b) {
              .inside {
                --x: 1;
              }
            }
            ",
        );
        check(
            ".parent { @scope (.from) to (.to) { .inside { --x: 1; } & > .child { --x: 2; } } }",
            "
            @scope (.parent .from) to (.to) {
              .inside {
                --x: 1;
              }
              :where(:scope) > .child {
                --x: 2;
              }
            }
            ",
        );
        check(
            ".parent { @scope (.from) { --x: 1; } }",
            "
            @scope (.parent .from) {
              :where(:scope) {
                --x: 1;
              }
            }
            ",
        );
    }

    #[test]
    fn wraps_bare_declarations_in_scope() {
        check(
            "@scope (.from) to (.to) { --x: 1; .inside { --x: 2; } }",
            "
            @scope (.from) to (.to) {
              :where(:scope) {
                --x: 1;
              }
              .inside {
                --x: 2;
              }
            }
            ",
        );
        check(
            "@scope (.from) { &, .inside { --x: 1; &:hover { --x: 2; } } }",
            "
            @scope (.from) {
              :where(:scope), .inside {
                --x: 1;
                &:hover {
                  --x: 2;
                }
              }
            }
            ",
        );
    }

    #[test]
    fn leaves_property_and_apply_alone_and_merges_layers() {
        check(
            ".foo { .bar { @apply text-red-500; } }\n.baz { @property --tw-content { syntax: '*'; } }",
            "
            .foo .bar {
              @apply text-red-500;
            }
            .baz {
              @property --tw-content {
                syntax: '*';
              }
            }
            ",
        );
        check(
            "@layer a;\n@layer a { @layer b { .x { color: red; } } }",
            "
            @layer a {
              @layer b {
                .x {
                  color: red;
                }
              }
            }
            ",
        );
        check(
            ".foo .bar { .system &:focus { --x: 1; } }\n.foo:hover { .system &:focus { --x: 2; } }",
            "
            .system :is(.foo .bar):focus {
              --x: 1;
            }
            .system .foo:hover:focus {
              --x: 2;
            }
            ",
        );
    }

    #[test]
    fn drops_rules_that_can_never_match() {
        check(
            "
            .a::selection { & > * { --x: 1; } & .b, & + .c { --x: 2; } }
            .a ::marker { & > * { --x: 3; } }
            .a:before { & > * { --x: 4; } }
            .a::part(foo) { & > * { --x: 5; } }
            .a::backdrop { :where(& > :not(:last-child)) { --x: 6; } }
            .a::-webkit-details-marker { & > * { --x: 7; } }
            ",
            "",
        );
        check(
            "
            .a::before, .b { & > * { --x: 1; } }
            .c::after { & > *, &:hover { --x: 2; } }
            .d::backdrop { :where(& > *, .e) { --x: 3; } }
            ",
            "
            :is(.a::before, .b) > * {
              --x: 1;
            }
            .c::after:hover {
              --x: 2;
            }
            :where(.d::backdrop > *, .e) {
              --x: 3;
            }
            ",
        );
        check(
            "
            .a::selection {
              :where(& > *) {
                --x: 1;
                :is(&, .b) { --x: 2; }
                &:hover { --x: 3; }
                @media (width >= 40rem) {
                  --x: 4;
                  :is(&, .c) { --x: 5; }
                }
              }
            }
            ",
            "
            :is(:is(:where(.a::selection > *)), .b) {
              --x: 2;
            }
            @media (width >= 40rem) {
              :is(:is(:where(.a::selection > *)), .c) {
                --x: 5;
              }
            }
            ",
        );
        check(
            "
            .a::selection {
              & > * {
                @font-face { font-family: 'A'; }
                @scope (.from) { color: red; }
                @layer base, utilities;
                @layer components { --x: 1; :is(&, .b) { --x: 2; } }
              }
            }
            ",
            "
            @layer base, utilities;
            @layer components {
              :is(:is(.a::selection > *), .b) {
                --x: 2;
              }
            }
            ",
        );
        check(
            ".a::selection { & > * { @scope (.x) { .y { --x: 1; } } } }",
            "",
        );
        check(
            ".a::selection { & > * { @scope (:is(&, .fallback)) { .y { --x: 1; } } } }",
            "
            @scope (:is(:is(.a::selection > *), .fallback)) {
              .y {
                --x: 1;
              }
            }
            ",
        );
    }

    #[test]
    fn keeps_unknown_pseudo_elements_and_distinct_bodies() {
        check(
            ".a { ::ng-deep .b { --x: 1; } }\n.h::unknown { & > * { --x: 5; } }",
            "
            .a ::ng-deep .b {
              --x: 1;
            }
            .h::unknown > * {
              --x: 5;
            }
            ",
        );
        check(
            "@font-face { font-family: 'A'; }\n@font-face { font-family: 'B'; }",
            "
            @font-face {
              font-family: 'A';
            }
            @font-face {
              font-family: 'B';
            }
            ",
        );
    }

    #[test]
    fn deep_trees_do_not_recurse() {
        // Far deeper than the parser admits; bounded only because dropping
        // a `Node` tree recurses.
        let depth = 2_000;
        let mut node = Node::decl("color", "red");
        for _ in 0..depth {
            node = Node::at_rule("@media", "print", vec![node]);
        }
        let css = to_css(&optimize_ast(vec![node], &mut Theme::new()));
        assert_eq!(css.matches("@media print").count(), depth);
    }
}
