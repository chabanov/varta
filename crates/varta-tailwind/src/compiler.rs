//! Stylesheet compilation (port of `parseCss`, `compileAst` and `build` of
//! Tailwind's `index.ts`, and `at-import.ts` restricted to the embedded
//! `tailwindcss` package): `@import`, `@theme`, `@utility`,
//! `@custom-variant`, `@variant`, `@source inline()`, `@apply`,
//! `@tailwind utilities`, then candidates into canonical CSS.
use crate::apply::{apply_dependencies, substitute_apply};
use crate::ast::{ContextValue, Node};
use crate::canonical::{Supports, canonicalize};
use crate::compile::compile_candidates;
use crate::css_functions::substitute_functions;
use crate::css_parser;
use crate::design_system::DesignSystem;
use crate::error::TailwindError;
use crate::optimize::optimize_ast;
use crate::printer::to_css;
use crate::stylesheet::{INDEX_CSS, PREFLIGHT_CSS, THEME_CSS, UTILITIES_CSS, theme_options};
use crate::theme::{Theme, options};
use crate::utilities::css_utility::create_css_utility;
use crate::utils::brace_expansion::expand;
use crate::utils::topological_sort::topological_sort;
use crate::utils::{escape, segment, unescape};
use crate::value_parser::{self, ValueNode};
use crate::variants::{compounds_for_selectors, is_valid_variant_name, substitute_at_variant};
use std::collections::{BTreeMap, BTreeSet};

/// The largest Tailwind entry stylesheet, in bytes.
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
/// Tailwind's own `@import` recursion bound.
pub const MAX_IMPORT_DEPTH: usize = 100;
/// The most class tokens one build compiles.
pub const MAX_CANDIDATES: usize = 65_536;
/// The longest class token, in bytes.
pub const MAX_CANDIDATE_BYTES: usize = 4096;
/// The largest generated stylesheet, in bytes: the bound of a text resource
/// file, since the stylesheet is served as one.
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Context key marking the slot `@tailwind utilities` fills.
const UTILITIES_SLOT: &str = "utilities";
const CONTEXT_THEME: &str = "theme";
const CONTEXT_REFERENCE: &str = "reference";
const THEME_RULE_SELECTOR: &str = ":root, :host";

/// The embedded stylesheet an `@import` id names (the package `exports`
/// map of `tailwindcss` v4.3.5).
fn embedded(id: &str) -> Option<&'static str> {
    match id {
        "tailwindcss" | "tailwindcss/index" | "tailwindcss/index.css" => Some(INDEX_CSS),
        "tailwindcss/theme" | "tailwindcss/theme.css" | "./theme.css" => Some(THEME_CSS),
        "tailwindcss/preflight" | "tailwindcss/preflight.css" | "./preflight.css" => {
            Some(PREFLIGHT_CSS)
        }
        "tailwindcss/utilities" | "tailwindcss/utilities.css" | "./utilities.css" => {
            Some(UTILITIES_CSS)
        }
        _ => None,
    }
}

struct ImportParams {
    uri: String,
    layer: Option<String>,
    media: Option<String>,
    supports: Option<String>,
}

/// Port of `parseImportParams`; `None` leaves the `@import` for CSS.
fn parse_import_params(params: &[ValueNode]) -> Result<Option<ImportParams>, TailwindError> {
    let mut uri: Option<String> = None;
    let mut layer = None;
    let mut media = None;
    let mut supports: Option<String> = None;
    for (i, node) in params.iter().enumerate() {
        if matches!(node, ValueNode::Separator(_)) {
            continue;
        }
        if let ValueNode::Word(word) = node
            && uri.is_none()
        {
            if !(word.starts_with('"') || word.starts_with('\'')) || word.len() < 2 {
                return Ok(None);
            }
            uri = Some(word[1..word.len() - 1].to_owned());
            continue;
        }
        if let ValueNode::Function { name, .. } = node
            && name.eq_ignore_ascii_case("url")
        {
            return Ok(None);
        }
        if uri.is_none() {
            return Ok(None);
        }
        if node.value().eq_ignore_ascii_case("layer") && !matches!(node, ValueNode::Separator(_)) {
            if layer.is_some() {
                return Ok(None);
            }
            if supports.is_some() {
                return Err(TailwindError::ImportLayerOrder);
            }
            layer = Some(match node {
                ValueNode::Function { nodes, .. } => value_parser::to_css(nodes),
                _ => String::new(),
            });
            continue;
        }
        if let ValueNode::Function { name, nodes } = node
            && name.eq_ignore_ascii_case("supports")
        {
            if supports.is_some() {
                return Ok(None);
            }
            supports = Some(value_parser::to_css(nodes));
            continue;
        }
        media = Some(value_parser::to_css(&params[i..]));
        break;
    }
    Ok(uri.map(|uri| ImportParams {
        uri,
        layer,
        media,
        supports,
    }))
}

/// Inline embedded `@import`/`@reference` rules (port of
/// `substituteAtImports`).
fn substitute_imports(ast: &mut Vec<Node>, depth: usize) -> Result<(), TailwindError> {
    let mut out = Vec::with_capacity(ast.len());
    for mut node in std::mem::take(ast) {
        let Node::AtRule { name, params, .. } = &node else {
            if let Some(children) = node.children_mut() {
                substitute_imports(children, depth)?;
            }
            out.push(node);
            continue;
        };
        if name != "@import" && name != "@reference" {
            if let Some(children) = node.children_mut() {
                substitute_imports(children, depth)?;
            }
            out.push(node);
            continue;
        }
        let Some(mut parsed) = parse_import_params(&value_parser::parse(params))? else {
            out.push(node);
            continue;
        };
        if name == "@reference" {
            parsed.media = Some("reference".into());
        }
        if parsed.uri.starts_with("data:")
            || parsed.uri.starts_with("http://")
            || parsed.uri.starts_with("https://")
        {
            return Err(TailwindError::Import(parsed.uri));
        }
        if depth > MAX_IMPORT_DEPTH {
            return Err(TailwindError::ImportDepth(parsed.uri));
        }
        let content =
            embedded(&parsed.uri).ok_or_else(|| TailwindError::Import(parsed.uri.clone()))?;
        let mut imported = css_parser::parse(content)?;
        substitute_imports(&mut imported, depth + 1)?;
        let mut root = imported;
        if let Some(layer) = parsed.layer {
            root = vec![Node::at_rule("@layer", layer, root)];
        }
        if let Some(media) = parsed.media {
            root = vec![Node::at_rule("@media", media, root)];
        }
        if let Some(supports) = parsed.supports {
            let supports = if supports.starts_with('(') {
                supports
            } else {
                format!("({supports})")
            };
            root = vec![Node::at_rule("@supports", supports, root)];
        }
        out.push(Node::context(BTreeMap::new(), root));
    }
    *ast = out;
    Ok(())
}

/// A `@custom-variant` registration.
enum CustomVariant {
    Selectors {
        at_rules: Vec<String>,
        style_rules: Vec<String>,
    },
    Body(Vec<Node>),
}

/// The parsed entry stylesheet.
struct Parsed {
    theme: Theme,
    important: bool,
    custom_variants: Vec<(String, CustomVariant, Vec<String>)>,
    utilities: Vec<Node>,
    inline_candidates: Vec<String>,
    ignored_candidates: Vec<String>,
    has_utilities_slot: bool,
    has_theme_rule: bool,
    features: bool,
}

fn source_param_error(param: &str) -> TailwindError {
    TailwindError::FileSource(param.to_owned())
}

fn strip_quotes(path: &str, what: &str) -> Result<String, TailwindError> {
    let quoted = (path.starts_with('"') && path.ends_with('"'))
        || (path.starts_with('\'') && path.ends_with('\''));
    if !quoted || path.len() < 2 {
        return Err(TailwindError::Directive(format!(
            "`{what}` paths must be quoted."
        )));
    }
    Ok(path[1..path.len() - 1].to_owned())
}

fn reference_context(nodes: Vec<Node>) -> Node {
    Node::context(
        BTreeMap::from([(CONTEXT_REFERENCE.to_owned(), ContextValue::Flag(true))]),
        nodes,
    )
}

impl Parsed {
    /// Walk the AST (port of the `parseCss` walk). `top` is true when the
    /// nodes have no style or at-rule parent; `reference` is the context flag.
    fn walk(
        &mut self,
        nodes: &mut Vec<Node>,
        top: bool,
        reference: bool,
    ) -> Result<(), TailwindError> {
        let mut out = Vec::with_capacity(nodes.len());
        for mut node in std::mem::take(nodes) {
            match &mut node {
                Node::Context { context, nodes } => {
                    let reference = reference
                        || matches!(
                            context.get(CONTEXT_REFERENCE),
                            Some(ContextValue::Flag(true))
                        );
                    self.walk(nodes, top, reference)?;
                    out.push(node);
                }
                Node::AtRule { .. } => {
                    if let Some(replacement) = self.at_rule(node, top, reference)? {
                        out.extend(replacement);
                    }
                }
                Node::Rule { nodes, .. } => {
                    self.walk(nodes, false, reference)?;
                    out.push(node);
                }
                _ => out.push(node),
            }
        }
        *nodes = out;
        Ok(())
    }

    /// Handle one at-rule; returns its replacement nodes.
    fn at_rule(
        &mut self,
        mut node: Node,
        top: bool,
        reference: bool,
    ) -> Result<Option<Vec<Node>>, TailwindError> {
        let Node::AtRule {
            name,
            params,
            nodes,
        } = &mut node
        else {
            return Ok(Some(vec![node]));
        };
        match name.as_str() {
            "@plugin" | "@config" => {
                return Err(TailwindError::JavaScriptConfig(format!("{name} {params}")));
            }
            "@tailwind" if params == "utilities" || params.starts_with("utilities") => {
                if self.has_utilities_slot || reference {
                    return Ok(Some(Vec::new()));
                }
                for param in segment(params, ' ') {
                    if param.starts_with("source(") && param != "source(none)" {
                        return Err(source_param_error(&param));
                    }
                }
                self.has_utilities_slot = true;
                self.features = true;
                return Ok(Some(vec![Node::context(
                    BTreeMap::from([(UTILITIES_SLOT.to_owned(), ContextValue::Flag(true))]),
                    Vec::new(),
                )]));
            }
            "@utility" => {
                if !top {
                    return Err(TailwindError::Directive(
                        "`@utility` cannot be nested.".into(),
                    ));
                }
                if nodes.is_empty() {
                    return Err(TailwindError::Directive(format!(
                        "`@utility {params}` is empty. Utilities should include at least one property."
                    )));
                }
                let params = params.clone();
                if create_css_utility(&node).is_none() {
                    return Err(invalid_utility_name(&params));
                }
                self.features = true;
                self.utilities.push(node.clone());
                // `@utility` rules are removed from the output.
                return Ok(Some(Vec::new()));
            }
            "@source" => {
                if !nodes.is_empty() {
                    return Err(TailwindError::Directive(
                        "`@source` cannot have a body.".into(),
                    ));
                }
                if !top {
                    return Err(TailwindError::Directive(
                        "`@source` cannot be nested.".into(),
                    ));
                }
                let mut path = params.as_str();
                let mut not = false;
                if let Some(rest) = path.strip_prefix("not ") {
                    not = true;
                    path = rest;
                }
                let Some(inline) = path
                    .strip_prefix("inline(")
                    .and_then(|p| p.strip_suffix(')'))
                else {
                    return Err(TailwindError::FileSource(format!("@source {params}")));
                };
                let source = strip_quotes(inline.trim(), "@source")?;
                for pattern in segment(&source, ' ') {
                    for candidate in expand(&pattern)? {
                        if not {
                            self.ignored_candidates.push(candidate);
                        } else {
                            self.inline_candidates.push(candidate);
                        }
                    }
                }
                return Ok(Some(Vec::new()));
            }
            "@variant" if top => {
                let has_slot = has_at_rule(nodes, "@slot");
                if nodes.is_empty() || has_slot {
                    *name = "@custom-variant".into();
                } else {
                    // Expanded later by `substitute_at_variant`.
                    let children = std::mem::take(nodes);
                    let mut children = children;
                    self.walk(&mut children, false, reference)?;
                    *nodes = children;
                    return Ok(Some(vec![node]));
                }
            }
            _ => {}
        }
        // Re-borrow after a possible rename.
        let Node::AtRule {
            name,
            params,
            nodes,
        } = &mut node
        else {
            return Ok(Some(vec![node]));
        };
        match name.as_str() {
            "@custom-variant" => {
                if !top {
                    return Err(TailwindError::Directive(
                        "`@custom-variant` cannot be nested.".into(),
                    ));
                }
                let parts = segment(params, ' ');
                let variant_name = parts.first().cloned().unwrap_or_default();
                let selector = parts.get(1).cloned().filter(|s| !s.is_empty());
                if !is_valid_variant_name(&variant_name) {
                    return Err(TailwindError::Directive(format!(
                        "`@custom-variant {variant_name}` defines an invalid variant name. Variants should only contain alphanumeric, dashes, or underscore characters and start with a lowercase letter or number."
                    )));
                }
                if !nodes.is_empty() && selector.is_some() {
                    return Err(TailwindError::Directive(format!(
                        "`@custom-variant {variant_name}` cannot have both a selector and a body."
                    )));
                }
                if nodes.is_empty() {
                    let Some(selector) = selector else {
                        return Err(TailwindError::Directive(format!(
                            "`@custom-variant {variant_name}` has no selector or body."
                        )));
                    };
                    let inner = selector
                        .strip_prefix('(')
                        .and_then(|s| s.strip_suffix(')'))
                        .unwrap_or_else(|| {
                            selector
                                .get(1..selector.len().saturating_sub(1))
                                .unwrap_or_default()
                        });
                    let selectors = segment(inner, ',');
                    if selectors.is_empty() || selectors.iter().any(|s| s.trim().is_empty()) {
                        return Err(TailwindError::Directive(format!(
                            "`@custom-variant {variant_name} ({})` selector is invalid.",
                            selectors.join(",")
                        )));
                    }
                    let (mut at_rules, mut style_rules) = (Vec::new(), Vec::new());
                    for selector in selectors {
                        let selector = selector.trim().to_owned();
                        if selector.starts_with('@') {
                            at_rules.push(selector);
                        } else {
                            style_rules.push(selector);
                        }
                    }
                    self.custom_variants.push((
                        variant_name,
                        CustomVariant::Selectors {
                            at_rules,
                            style_rules,
                        },
                        Vec::new(),
                    ));
                } else {
                    let mut dependencies = Vec::new();
                    crate::ast::for_each(nodes, &mut |child, _| {
                        if let Node::AtRule { name, params, .. } = child
                            && name == "@variant"
                        {
                            dependencies.push(params.clone());
                        }
                    });
                    self.custom_variants.push((
                        variant_name,
                        CustomVariant::Body(std::mem::take(nodes)),
                        dependencies,
                    ));
                }
                Ok(Some(Vec::new()))
            }
            "@media" => self.media(node, top, reference),
            "@theme" => {
                let mut opts = theme_options(params);
                self.features = true;
                if reference {
                    opts |= options::REFERENCE;
                }
                if let Some(prefix) = theme_prefix(params) {
                    if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_lowercase()) {
                        return Err(TailwindError::Directive(format!(
                            "The prefix \"{prefix}\" is invalid. Prefixes must be lowercase ASCII letters (a-z) only."
                        )));
                    }
                    self.theme.prefix = Some(prefix);
                }
                for child in nodes.iter() {
                    match child {
                        Node::AtRule { name, .. } if name == "@keyframes" => {
                            self.theme.add_keyframes(child.clone());
                        }
                        Node::Comment(_) => {}
                        Node::Decl {
                            property, value, ..
                        } if property.starts_with("--") => {
                            self.theme.add(
                                &unescape(property),
                                value.as_deref().unwrap_or_default(),
                                opts,
                            )?;
                        }
                        other => {
                            return Err(TailwindError::Directive(format!(
                                "`@theme` blocks must only contain custom properties or `@keyframes`, found:\n{}",
                                to_css(std::slice::from_ref(other))
                            )));
                        }
                    }
                }
                if self.has_theme_rule {
                    Ok(Some(Vec::new()))
                } else {
                    self.has_theme_rule = true;
                    Ok(Some(vec![Node::rule(THEME_RULE_SELECTOR, Vec::new())]))
                }
            }
            _ => {
                let nested_top = false;
                if let Some(children) = node.children_mut() {
                    let mut children = std::mem::take(children);
                    self.walk(&mut children, nested_top, reference)?;
                    if let Some(slot) = node.children_mut() {
                        *slot = children;
                    }
                }
                Ok(Some(vec![node]))
            }
        }
    }

    /// `@media` with Tailwind import parameters (`source()`, `theme()`,
    /// `prefix()`, `important`, `reference`).
    fn media(
        &mut self,
        mut node: Node,
        top: bool,
        reference: bool,
    ) -> Result<Option<Vec<Node>>, TailwindError> {
        let Node::AtRule { params, nodes, .. } = &mut node else {
            return Ok(Some(vec![node]));
        };
        let parts = segment(params, ' ');
        let mut unknown = Vec::new();
        let mut reference = reference;
        for param in &parts {
            if param.starts_with("source(") {
                if param != "source(none)" {
                    return Err(source_param_error(param));
                }
            } else if let Some(theme_params) = param
                .strip_prefix("theme(")
                .and_then(|p| p.strip_suffix(')'))
            {
                let has_reference = theme_params.contains("reference");
                append_theme_params(nodes, theme_params, has_reference)?;
            } else if let Some(prefix) = param
                .strip_prefix("prefix(")
                .and_then(|p| p.strip_suffix(')'))
            {
                append_theme_params(nodes, &format!("prefix({prefix})"), false)?;
            } else if param == "important" {
                self.important = true;
            } else if param == "reference" {
                reference = true;
                let children = std::mem::take(nodes);
                *nodes = vec![reference_context(children)];
            } else {
                unknown.push(param.clone());
            }
        }
        if !unknown.is_empty() {
            *params = unknown.join(" ");
            let mut children = std::mem::take(nodes);
            self.walk(&mut children, false, reference)?;
            *nodes = children;
            return Ok(Some(vec![node]));
        }
        let mut children = std::mem::take(nodes);
        if parts.is_empty() {
            self.walk(&mut children, false, reference)?;
            *nodes = children;
            return Ok(Some(vec![node]));
        }
        // Replaced by its children, which are walked in the parent's place.
        self.walk(&mut children, top, reference)?;
        Ok(Some(children))
    }
}

/// The `prefix(…)` option of `@theme` parameters.
fn theme_prefix(params: &str) -> Option<String> {
    segment(params, ' ').into_iter().find_map(|option| {
        option
            .strip_prefix("prefix(")
            .and_then(|p| p.strip_suffix(')'))
            .map(str::to_owned)
    })
}

/// Append `extra` to the params of each `@theme` among `nodes` (contexts are
/// transparent). With `reference_only`, any other node is an error.
fn append_theme_params(
    nodes: &mut [Node],
    extra: &str,
    reference_only: bool,
) -> Result<(), TailwindError> {
    for node in nodes {
        match node {
            Node::Context { nodes, .. } => append_theme_params(nodes, extra, reference_only)?,
            Node::AtRule { name, params, .. } if name == "@theme" => {
                params.push(' ');
                params.push_str(extra);
            }
            Node::AtRule { nodes, .. } => append_theme_params(nodes, extra, reference_only)?,
            _ if reference_only => {
                return Err(TailwindError::Directive(
                    "Files imported with `@import \"…\" theme(reference)` must only contain `@theme` blocks. Use `@reference \"…\";` instead.".into(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn has_at_rule(nodes: &[Node], wanted: &str) -> bool {
    let mut found = false;
    crate::ast::for_each(nodes, &mut |node, _| {
        if matches!(node, Node::AtRule { name, .. } if name == wanted) {
            found = true;
        }
    });
    found
}

fn invalid_utility_name(params: &str) -> TailwindError {
    let message = if !params.ends_with("-*") && params.ends_with('*') {
        format!(
            "`@utility {params}` defines an invalid utility name. A functional utility must end in `-*`."
        )
    } else if !params.ends_with("-*") && params.contains('*') {
        format!(
            "`@utility {params}` defines an invalid utility name. The dynamic portion marked by `-*` must appear once at the end."
        )
    } else {
        format!(
            "`@utility {params}` defines an invalid utility name. Utilities should be alphanumeric and start with a lowercase letter."
        )
    };
    TailwindError::Directive(message)
}

fn source(value: &str) -> BTreeMap<String, ContextValue> {
    BTreeMap::from([("source".to_owned(), ContextValue::Text(value.to_owned()))])
}

/// Register a selector-list custom variant (`@custom-variant x (sel, @media …)`).
fn register_selector_variant(
    ds: &mut DesignSystem,
    name: &str,
    at_rules: Vec<String>,
    style_rules: Vec<String>,
) {
    let uses_scope = at_rules.iter().any(|p| p.starts_with("@scope"));
    let all: Vec<String> = style_rules.iter().chain(&at_rules).cloned().collect();
    let compounds = compounds_for_selectors(&all);
    ds.variants.static_variant(
        name,
        move |rule, _, _| {
            let children = rule.children_mut()?;
            let original = std::mem::take(children);
            let slot = if uses_scope {
                vec![Node::context(source("user"), original)]
            } else {
                original
            };
            let mut nodes = Vec::new();
            if !style_rules.is_empty() {
                nodes.push(Node::rule(style_rules.join(", "), slot.clone()));
            }
            for selector in &at_rules {
                nodes.push(Node::from_prelude(selector, slot.clone()));
            }
            *children = if uses_scope {
                vec![Node::context(source("variant"), nodes)]
            } else {
                nodes
            };
            Some(())
        },
        Some(compounds),
        None,
    );
}

/// A compiled Tailwind entry stylesheet, ready to build with candidates.
pub struct Compiler {
    ds: DesignSystem,
    ast: Vec<Node>,
    has_utilities_slot: bool,
    features: bool,
    inline_candidates: Vec<String>,
}

impl std::fmt::Debug for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compiler")
            .field("has_utilities_slot", &self.has_utilities_slot)
            .field("features", &self.features)
            .field("inline_candidates", &self.inline_candidates)
            .finish_non_exhaustive()
    }
}

/// Compile a Tailwind entry stylesheet (port of `compile`/`parseCss`).
pub fn compile(css: &str) -> Result<Compiler, TailwindError> {
    if css.len() > MAX_INPUT_BYTES {
        return Err(TailwindError::Limit {
            limit: "entry stylesheet bytes",
            max: MAX_INPUT_BYTES,
        });
    }
    let mut ast = vec![Node::context(BTreeMap::new(), css_parser::parse(css)?)];
    substitute_imports(&mut ast, 0)?;

    let mut parsed = Parsed {
        theme: Theme::new(),
        important: false,
        custom_variants: Vec::new(),
        utilities: Vec::new(),
        inline_candidates: Vec::new(),
        ignored_candidates: Vec::new(),
        has_utilities_slot: false,
        has_theme_rule: false,
        features: false,
    };
    parsed.walk(&mut ast, true, false)?;

    let mut ds = DesignSystem::new(std::mem::take(&mut parsed.theme));
    ds.important = parsed.important;
    ds.invalid_candidates
        .extend(parsed.ignored_candidates.iter().cloned());

    // Custom variants: placeholders first, then in dependency order.
    for (name, _, _) in &parsed.custom_variants {
        ds.variants
            .static_variant(name, |_, _, _| Some(()), None, None);
    }
    let graph: Vec<(String, Vec<String>)> = parsed
        .custom_variants
        .iter()
        .map(|(name, _, deps)| (name.clone(), deps.clone()))
        .collect();
    let order = topological_sort(&graph).map_err(|cycle| {
        TailwindError::Circular(format!(
            "custom variants: {:?} -> {:?}",
            cycle.path, cycle.start
        ))
    })?;
    let mut variants: BTreeMap<String, CustomVariant> = BTreeMap::new();
    for (name, variant, _) in parsed.custom_variants.drain(..) {
        variants.insert(name, variant);
    }
    for name in order {
        match variants.remove(&name) {
            Some(CustomVariant::Selectors {
                at_rules,
                style_rules,
            }) => {
                register_selector_variant(&mut ds, &name, at_rules, style_rules);
            }
            Some(CustomVariant::Body(body)) => ds.variants.from_ast(&name, body),
            None => {}
        }
    }

    register_custom_utilities(&mut ds, std::mem::take(&mut parsed.utilities))?;

    if parsed.has_theme_rule {
        let mut declarations = Vec::new();
        for (key, value) in ds.theme.entries() {
            if value.options & options::REFERENCE != 0 {
                continue;
            }
            declarations.push(Node::decl(escape(&key), value.value));
        }
        let theme_context =
            || BTreeMap::from([(CONTEXT_THEME.to_owned(), ContextValue::Flag(true))]);
        fill_theme_rule(&mut ast, Node::context(theme_context(), declarations));
        for keyframes in ds.theme.keyframes().to_vec() {
            ast.push(Node::context(
                theme_context(),
                vec![Node::AtRoot(vec![keyframes])],
            ));
        }
    }

    substitute_at_variant(&mut ast, &ds)?;
    substitute_functions(&mut ast, &ds)?;
    for node in &mut ast {
        apply_below_top(node, &ds)?;
    }

    Ok(Compiler {
        ds,
        ast,
        has_utilities_slot: parsed.has_utilities_slot,
        features: parsed.features,
        inline_candidates: parsed.inline_candidates,
    })
}

/// Substitute `@apply` everywhere except at the top level (Tailwind leaves a
/// parentless `@apply` alone). Contexts are transparent.
fn apply_below_top(node: &mut Node, ds: &DesignSystem) -> Result<(), TailwindError> {
    match node {
        Node::Context { nodes, .. } | Node::AtRoot(nodes) => {
            for child in nodes {
                apply_below_top(child, ds)?;
            }
            Ok(())
        }
        Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } => {
            substitute_apply(nodes, ds)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Register `@utility` rules, substituting their `@apply` rules in
/// dependency order first (port of the `@utility` part of
/// `substituteAtApply`).
fn register_custom_utilities(
    ds: &mut DesignSystem,
    utilities: Vec<Node>,
) -> Result<(), TailwindError> {
    let name_of = |node: &Node| match node {
        Node::AtRule { params, .. } => params.strip_suffix("-*").unwrap_or(params).to_owned(),
        _ => String::new(),
    };
    // Dependencies by index: the utilities each one applies.
    let names: Vec<String> = utilities.iter().map(name_of).collect();
    let graph: Vec<(usize, Vec<usize>)> = utilities
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let mut roots = BTreeSet::new();
            crate::ast::for_each(std::slice::from_ref(node), &mut |child, _| {
                if let Node::AtRule { name, params, .. } = child
                    && name == "@apply"
                {
                    roots.extend(apply_dependencies(params, ds));
                }
            });
            let deps = names
                .iter()
                .enumerate()
                .filter(|(other, name)| *other != index && roots.contains(*name))
                .map(|(other, _)| other)
                .collect();
            (index, deps)
        })
        .collect();
    let order = topological_sort(&graph).map_err(|cycle| {
        let names: Vec<&str> = cycle.path.iter().map(|i| names[*i].as_str()).collect();
        TailwindError::Circular(format!(
            "`@utility` rules applying each other: {}",
            names.join(" -> ")
        ))
    })?;
    let mut utilities: Vec<Option<Node>> = utilities.into_iter().map(Some).collect();
    for index in order {
        let Some(mut node) = utilities.get_mut(index).and_then(Option::take) else {
            continue;
        };
        if let Some(children) = node.children_mut() {
            substitute_apply(children, ds)?;
        }
        if let Some(register) = create_css_utility(&node) {
            register(ds);
        }
    }
    Ok(())
}

/// Fill the first `:root, :host` theme rule placeholder.
fn fill_theme_rule(ast: &mut [Node], declarations: Node) -> bool {
    for node in ast {
        match node {
            Node::Rule { selector, nodes }
                if selector == THEME_RULE_SELECTOR && nodes.is_empty() =>
            {
                *nodes = vec![declarations];
                return true;
            }
            _ => {
                if let Some(children) = node.children_mut()
                    && fill_theme_rule(children, declarations.clone())
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Fill the `@tailwind utilities` slot.
fn fill_utilities(ast: &mut [Node], utilities: &[Node]) -> bool {
    for node in ast {
        if let Node::Context { context, nodes } = node
            && matches!(context.get(UTILITIES_SLOT), Some(ContextValue::Flag(true)))
        {
            *nodes = utilities.to_vec();
            return true;
        }
        if let Some(children) = node.children_mut()
            && fill_utilities(children, utilities)
        {
            return true;
        }
    }
    false
}

/// The result of a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// The canonical stylesheet.
    pub css: String,
    /// Tokens that matched no utility, sorted (most are ordinary strings).
    pub unmatched: Vec<String>,
}

impl Compiler {
    /// The design system (theme, utilities, variants).
    #[must_use]
    pub fn design_system(&self) -> &DesignSystem {
        &self.ds
    }

    /// Candidates from `@source inline(…)`.
    #[must_use]
    pub fn inline_candidates(&self) -> &[String] {
        &self.inline_candidates
    }

    /// Build the stylesheet for `candidates` plus the inline safelist, in
    /// canonical form (port of `build` with no polyfills). The candidate
    /// order does not matter: they are deduplicated and sorted.
    pub fn build<'a>(
        &mut self,
        candidates: impl IntoIterator<Item = &'a str>,
        supports: &dyn Supports,
    ) -> Result<Build, TailwindError> {
        let mut all: BTreeSet<String> = BTreeSet::new();
        let mut tokens: Vec<String> = self.inline_candidates.clone();
        tokens.extend(candidates.into_iter().map(str::to_owned));
        for candidate in tokens.iter().map(String::as_str) {
            if candidate.len() > MAX_CANDIDATE_BYTES
                || self.ds.invalid_candidates.contains(candidate)
            {
                continue;
            }
            if candidate.starts_with("--") {
                self.ds.theme.mark_used_variable(candidate);
            } else {
                all.insert(candidate.to_owned());
            }
            if all.len() > MAX_CANDIDATES {
                return Err(TailwindError::Limit {
                    limit: "class candidates",
                    max: MAX_CANDIDATES,
                });
            }
        }
        let mut ast = self.ast.clone();
        let mut unmatched = Vec::new();
        if self.features && self.has_utilities_slot {
            let compiled = compile_candidates(all.iter().map(String::as_str), &self.ds, true);
            unmatched = compiled.invalid;
            unmatched.sort();
            fill_utilities(&mut ast, &compiled.nodes);
        }
        let ast = optimize_ast(ast, &mut self.ds.theme);
        let ast = canonicalize(ast, supports)?;
        let css = to_css(&ast);
        if css.len() > MAX_OUTPUT_BYTES {
            return Err(TailwindError::Limit {
                limit: "generated stylesheet bytes",
                max: MAX_OUTPUT_BYTES,
            });
        }
        Ok(Build { css, unmatched })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::VartaSupports;

    fn build(css: &str, candidates: &[&str]) -> String {
        let mut compiler = compile(css).unwrap();
        compiler
            .build(candidates.iter().copied(), &VartaSupports)
            .unwrap()
            .css
    }

    #[test]
    fn utilities_only() {
        let css = build("@tailwind utilities;", &["flex", "p-4", "nope"]);
        assert_eq!(css, ".flex {\n  display: flex;\n}\n");
    }

    #[test]
    fn theme_variables_are_emitted_when_used() {
        let css = build(
            "@theme { --color-brand: #123456; --color-unused: red; }\n@tailwind utilities;",
            &["text-brand"],
        );
        assert!(css.contains("--color-brand: #123456;"), "{css}");
        assert!(!css.contains("--color-unused"), "{css}");
        assert!(css.contains("color: var(--color-brand);"), "{css}");
    }

    #[test]
    fn full_import_is_deterministic() {
        let a = build(
            "@import \"tailwindcss\";",
            &["flex", "bg-red-500", "hover:underline"],
        );
        let b = build(
            "@import \"tailwindcss\";",
            &["hover:underline", "bg-red-500", "flex"],
        );
        assert_eq!(a, b);
        assert!(
            a.contains("@layer theme, base, components, utilities;"),
            "{a}"
        );
        assert!(a.contains("--color-red-500"), "{a}");
    }

    #[test]
    fn javascript_configuration_is_rejected() {
        let error = compile("@plugin \"@tailwindcss/forms\";").unwrap_err();
        assert!(matches!(error, TailwindError::JavaScriptConfig(_)));
        let error = compile("@config \"./tailwind.config.js\";").unwrap_err();
        assert!(matches!(error, TailwindError::JavaScriptConfig(_)));
    }

    #[test]
    fn file_sources_are_rejected_and_inline_sources_safelist() {
        assert!(matches!(
            compile("@source \"../src\";").unwrap_err(),
            TailwindError::FileSource(_)
        ));
        let css = build(
            "@import \"tailwindcss/theme\";\n@source inline(\"p-{1,2}\");\n@source not inline(\"p-2\");\n@tailwind utilities;",
            &["p-2"],
        );
        assert!(css.contains(".p-1 {"), "{css}");
        assert!(!css.contains(".p-2 {"), "{css}");
    }

    #[test]
    fn custom_utilities_variants_and_apply() {
        let css = build(
            "@import \"tailwindcss/theme\";\n@custom-variant hocus (&:hover, &:focus);\n@utility tab-* { tab-size: --value(integer); }\n.btn { @apply px-4 font-bold; }\n@tailwind utilities;",
            &["hocus:tab-4"],
        );
        assert!(
            css.contains(".hocus\\:tab-4:hover, .hocus\\:tab-4:focus {\n  tab-size: 4;\n}"),
            "{css}"
        );
        assert!(
            css.contains(".btn {\n  padding-inline: calc(var(--spacing) * 4);"),
            "{css}"
        );
    }

    #[test]
    fn unknown_imports_are_rejected() {
        assert!(matches!(
            compile("@import \"./other.css\";").unwrap_err(),
            TailwindError::Import(_)
        ));
    }
}
