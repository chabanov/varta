//! The built-in variants: a port of the body of `createVariants` and of
//! `quoteAttributeValue` in Tailwind CSS v4.3.5 `variants.ts` (MIT,
//! Copyright (c) Tailwind Labs, Inc.; see `LICENSE-TAILWIND`). The attribute
//! selector check of `aria-*`/`data-*` is in `attribute_selector`.
//!
//! Registration order is Tailwind's: it decides the variant sort order.
//! Suggestions (`variants.suggest`) are not ported.
use super::{CompareFn, Variants, compounds, compounds_for_selectors};
use crate::ast::Node;
use crate::candidate::{CandidateModifier, Variant, VariantValue};
use crate::design_system::DesignSystem;
use crate::theme::Theme;
use crate::utils::compare_breakpoints::{Direction, compare_breakpoints};
use crate::utils::infer_data_type::is_positive_integer;
use crate::utils::js_number::{is_js_whitespace, js_trim};
use crate::utils::segment;
use crate::value_parser::{self, ValueNode, WalkAction};
use std::cmp::Ordering;
use std::rc::Rc;

mod attribute_selector;

/// Conditional at-rules that `not-*` can negate (`conditionalRules`).
const CONDITIONAL_RULES: &[&str] = &["@media", "@supports", "@container"];
/// The at-rule of container queries.
const CONTAINER: &str = "@container";
/// The negation keyword of media, supports and container conditions.
const NOT: &str = "not";
/// The `supports-*` functions whose name gains surrounding spaces
/// (Chrome rejects `(a)or(b)` but accepts `(a) or (b)`).
const SUPPORTS_OPERATORS: &[&str] = &["and", "or", "not"];
/// The `supports-*` function kept as-is (`selector(a:not(b))`).
const SUPPORTS_SELECTOR: &str = "selector";
/// The theme namespace of breakpoints.
const BREAKPOINT_NAMESPACE: &str = "--breakpoint";
/// The theme namespace of container sizes.
const CONTAINER_NAMESPACE: &str = "--container";

/// Static variants of fixed selectors, registered with `staticVariant`, in
/// order; each table is registered where Tailwind registers it.
const PSEUDO_ELEMENTS: &[(&str, &[&str])] = &[
    ("first-letter", &["&::first-letter"]),
    ("first-line", &["&::first-line"]),
    (
        "marker",
        &[
            "& *::marker",
            "&::marker",
            "& *::-webkit-details-marker",
            "&::-webkit-details-marker",
        ],
    ),
    ("selection", &["& *::selection", "&::selection"]),
    ("file", &["&::file-selector-button"]),
    ("placeholder", &["&::placeholder"]),
    ("backdrop", &["&::backdrop"]),
    ("details-content", &["&::details-content"]),
];

/// Positional, state, form, content and interactive pseudo-classes before
/// `hover`.
const PSEUDO_CLASSES_BEFORE_HOVER: &[(&str, &[&str])] = &[
    // Positional
    ("first", &["&:first-child"]),
    ("last", &["&:last-child"]),
    ("only", &["&:only-child"]),
    ("odd", &["&:nth-child(odd)"]),
    ("even", &["&:nth-child(even)"]),
    ("first-of-type", &["&:first-of-type"]),
    ("last-of-type", &["&:last-of-type"]),
    ("only-of-type", &["&:only-of-type"]),
    // State
    ("visited", &["&:visited"]),
    ("target", &["&:target"]),
    ("open", &["&:is([open], :popover-open, :open)"]),
    // Forms
    ("default", &["&:default"]),
    ("checked", &["&:checked"]),
    ("indeterminate", &["&:indeterminate"]),
    ("placeholder-shown", &["&:placeholder-shown"]),
    ("autofill", &["&:autofill"]),
    ("optional", &["&:optional"]),
    ("required", &["&:required"]),
    ("valid", &["&:valid"]),
    ("invalid", &["&:invalid"]),
    ("user-valid", &["&:user-valid"]),
    ("user-invalid", &["&:user-invalid"]),
    ("in-range", &["&:in-range"]),
    ("out-of-range", &["&:out-of-range"]),
    ("read-only", &["&:read-only"]),
    // Content
    ("empty", &["&:empty"]),
    // Interactive
    ("focus-within", &["&:focus-within"]),
];

/// Interactive pseudo-classes after `hover`.
const PSEUDO_CLASSES_AFTER_HOVER: &[(&str, &[&str])] = &[
    ("focus", &["&:focus"]),
    ("focus-visible", &["&:focus-visible"]),
    ("active", &["&:active"]),
    ("enabled", &["&:enabled"]),
    ("disabled", &["&:disabled"]),
    ("inert", &["&:is([inert], [inert] *)"]),
];

/// Media variants after `supports-*`.
const PREFERENCE_MEDIA: &[(&str, &[&str])] = &[
    (
        "motion-safe",
        &["@media (prefers-reduced-motion: no-preference)"],
    ),
    (
        "motion-reduce",
        &["@media (prefers-reduced-motion: reduce)"],
    ),
    ("contrast-more", &["@media (prefers-contrast: more)"]),
    ("contrast-less", &["@media (prefers-contrast: less)"]),
];

/// The variants after the breakpoints and container queries.
const TRAILING_VARIANTS: &[(&str, &[&str])] = &[
    ("portrait", &["@media (orientation: portrait)"]),
    ("landscape", &["@media (orientation: landscape)"]),
    (
        "ltr",
        &["&:where(:dir(ltr), [dir=\"ltr\"], [dir=\"ltr\"] *)"],
    ),
    (
        "rtl",
        &["&:where(:dir(rtl), [dir=\"rtl\"], [dir=\"rtl\"] *)"],
    ),
    ("dark", &["@media (prefers-color-scheme: dark)"]),
    ("starting", &["@starting-style"]),
    ("print", &["@media print"]),
    ("forced-colors", &["@media (forced-colors: active)"]),
    ("inverted-colors", &["@media (inverted-colors: inverted)"]),
    ("pointer-none", &["@media (pointer: none)"]),
    ("pointer-coarse", &["@media (pointer: coarse)"]),
    ("pointer-fine", &["@media (pointer: fine)"]),
    ("any-pointer-none", &["@media (any-pointer: none)"]),
    ("any-pointer-coarse", &["@media (any-pointer: coarse)"]),
    ("any-pointer-fine", &["@media (any-pointer: fine)"]),
    ("noscript", &["@media (scripting: none)"]),
];

/// The structural pseudo-class of each `nth-*` variant.
const NTH_VARIANTS: &[(&str, &str)] = &[
    ("nth", ":nth-child"),
    ("nth-last", ":nth-last-child"),
    ("nth-of-type", ":nth-of-type"),
    ("nth-last-of-type", ":nth-last-of-type"),
];

/// The relation of `group-*` and `peer-*` targets to the marked element.
#[derive(Clone, Copy)]
enum Relation {
    /// `group`: the target is a descendant (`:is(<group> &)`).
    Group,
    /// `peer`: the target is a following sibling (`:is(<peer> ~ &)`).
    Peer,
}

impl Relation {
    fn class(self) -> &'static str {
        match self {
            Relation::Group => "group",
            Relation::Peer => "peer",
        }
    }

    fn combinator(self) -> &'static str {
        match self {
            Relation::Group => " ",
            Relation::Peer => " ~ ",
        }
    }
}

/// Register the built-in variants (port of `createVariants`).
pub(super) fn register(variants: &mut Variants, theme: &Theme) {
    static_variant(variants, "*", &[":is(& > *)"], Some(compounds::NEVER));
    static_variant(variants, "**", &[":is(& *)"], Some(compounds::NEVER));

    variants.compound(
        "not",
        compounds::STYLE_RULES | compounds::AT_RULES,
        apply_not,
        None,
        None,
    );
    variants.compound(
        "group",
        compounds::STYLE_RULES,
        |rule, variant, ds| apply_relation(rule, variant, ds, Relation::Group),
        None,
        None,
    );
    variants.compound(
        "peer",
        compounds::STYLE_RULES,
        |rule, variant, ds| apply_relation(rule, variant, ds, Relation::Peer),
        None,
        None,
    );

    static_variants(variants, PSEUDO_ELEMENTS);
    variants.static_variant(
        "before",
        |rule, _, _| wrap_pseudo_content(rule, "&::before"),
        Some(compounds::NEVER),
        None,
    );
    variants.static_variant(
        "after",
        |rule, _, _| wrap_pseudo_content(rule, "&::after"),
        Some(compounds::NEVER),
        None,
    );

    static_variants(variants, PSEUDO_CLASSES_BEFORE_HOVER);
    variants.static_variant(
        "hover",
        |rule, _, _| {
            wrap_children(rule, |nodes| {
                Node::rule(
                    "&:hover",
                    vec![Node::at_rule("@media", "(hover: hover)", nodes)],
                )
            })
        },
        None,
        None,
    );
    static_variants(variants, PSEUDO_CLASSES_AFTER_HOVER);

    variants.compound("in", compounds::STYLE_RULES, apply_in, None, None);
    variants.compound("has", compounds::STYLE_RULES, apply_has, None, None);

    variants.functional("aria", apply_aria, None, None);
    variants.functional("data", apply_data, None, None);
    for &(name, pseudo) in NTH_VARIANTS {
        variants.functional(
            name,
            move |rule, variant, _| apply_nth(rule, variant, pseudo),
            None,
            None,
        );
    }
    variants.functional("supports", apply_supports, Some(compounds::AT_RULES), None);

    static_variants(variants, PREFERENCE_MEDIA);

    // The compare functions see the theme the variants were created with.
    let creation_theme = Rc::new(theme.clone());
    register_breakpoints(variants, theme, &creation_theme);
    register_container_queries(variants, &creation_theme);

    static_variants(variants, TRAILING_VARIANTS);
}

/// Register a variant that nests the rule in each of `selectors`
/// (port of the `staticVariant` helper).
fn static_variant(
    variants: &mut Variants,
    name: &str,
    selectors: &'static [&'static str],
    compounds: Option<u8>,
) {
    let compounds = compounds.unwrap_or_else(|| {
        let owned: Vec<String> = selectors.iter().map(|s| (*s).to_owned()).collect();
        compounds_for_selectors(&owned)
    });
    variants.static_variant(
        name,
        move |rule, _, _| {
            let children = rule.children_mut()?;
            let nodes = std::mem::take(children);
            *children = selectors
                .iter()
                .map(|selector| Node::from_prelude(selector, nodes.clone()))
                .collect();
            Some(())
        },
        Some(compounds),
        None,
    );
}

fn static_variants(
    variants: &mut Variants,
    table: &'static [(&'static str, &'static [&'static str])],
) {
    for (name, selectors) in table {
        static_variant(variants, name, selectors, None);
    }
}

/// `before`/`after`: the pseudo-element with `content: var(--tw-content)`.
fn wrap_pseudo_content(rule: &mut Node, selector: &str) -> Option<()> {
    let children = rule.children_mut()?;
    let mut nodes = vec![
        Node::AtRoot(vec![Node::at_rule(
            "@property",
            "--tw-content",
            vec![
                Node::decl("syntax", "\"*\""),
                Node::decl("initial-value", "\"\""),
                Node::decl("inherits", "false"),
            ],
        )]),
        Node::decl("content", "var(--tw-content)"),
    ];
    nodes.append(children);
    *children = vec![Node::rule(selector, nodes)];
    Some(())
}

/// The value and modifier of a functional variant.
fn functional_parts(
    variant: &Variant,
) -> Option<(Option<&VariantValue>, Option<&CandidateModifier>)> {
    match variant {
        Variant::Functional {
            value, modifier, ..
        } => Some((value.as_ref(), modifier.as_ref())),
        _ => None,
    }
}

/// The value of a functional variant without a modifier (Tailwind's
/// `if (!variant.value || variant.modifier) return null`).
fn unmodified_value(variant: &Variant) -> Option<&VariantValue> {
    match functional_parts(variant)? {
        (Some(value), None) => Some(value),
        _ => None,
    }
}

/// Nest the children of `rule` in one new node.
fn wrap_children(rule: &mut Node, wrap: impl FnOnce(Vec<Node>) -> Node) -> Option<()> {
    let children = rule.children_mut()?;
    let nodes = std::mem::take(children);
    *children = vec![wrap(nodes)];
    Some(())
}

/// The sub-variant of a compound variant is a relative arbitrary selector
/// (`group-[>img]`), which the selector-rewriting compounds reject.
fn has_relative_sub_variant(variant: &Variant) -> bool {
    matches!(
        variant,
        Variant::Compound { variant, .. }
            if matches!(**variant, Variant::Arbitrary { relative: true, .. })
    )
}

/* ---------------------------------- not ---------------------------------- */

/// One rule on the path from the compound's rule to an empty leaf.
#[derive(Clone)]
enum PathRule {
    Style(String),
    At { name: String, params: String },
}

/// The rule paths to every empty style rule or at-rule, in walk order.
fn leaf_paths(nodes: &[Node], path: &mut Vec<PathRule>, out: &mut Vec<Vec<PathRule>>) {
    for node in nodes {
        let entry = match node {
            Node::Rule { selector, .. } => Some(PathRule::Style(selector.clone())),
            Node::AtRule { name, params, .. } => Some(PathRule::At {
                name: name.clone(),
                params: params.clone(),
            }),
            _ => None,
        };
        let children = node.children().map_or(&[][..], Vec::as_slice);
        match entry {
            Some(entry) if children.is_empty() => {
                let mut leaf = path.clone();
                leaf.push(entry);
                out.push(leaf);
            }
            Some(entry) => {
                path.push(entry);
                leaf_paths(children, path, out);
                path.pop();
            }
            None => leaf_paths(children, path, out),
        }
    }
}

/// `not-*` (the `not` compound of `createVariants`): replace the rule with
/// `&` holding the negation of each style rule and at-rule on the path to an
/// empty leaf.
fn apply_not(rule: &mut Node, variant: &Variant, _: &DesignSystem) -> Option<()> {
    if has_relative_sub_variant(variant) || variant.modifier().is_some() {
        return None;
    }
    let mut paths = Vec::new();
    leaf_paths(std::slice::from_ref(rule), &mut Vec::new(), &mut paths);

    let mut replacement: Option<Node> = None;
    let mut did_apply = false;
    for mut path in paths {
        // Tailwind replaces the rule in place, so later leaves see the
        // replaced `&` rule as their outermost ancestor.
        if replacement.is_some()
            && let Some(first) = path.first_mut()
        {
            *first = PathRule::Style("&".to_owned());
        }
        let style_count = path
            .iter()
            .filter(|entry| matches!(entry, PathRule::Style(_)))
            .count();
        if style_count > 1 || path.len() - style_count > 1 {
            break;
        }
        let Some(rules) = negate_path(&path) else {
            did_apply = false;
            break;
        };
        replacement = Some(Node::rule("&", rules));
        did_apply = true;
    }
    if let Some(replacement) = replacement {
        *rule = replacement;
    }
    if let Node::Rule { selector, nodes } = rule
        && selector == "&"
        && nodes.len() == 1
        && let Some(only) = nodes.pop()
    {
        *rule = only;
    }
    did_apply.then_some(())
}

/// The negated rules of one path: style rules first, then at-rules.
fn negate_path(path: &[PathRule]) -> Option<Vec<Node>> {
    let mut rules = Vec::new();
    for entry in path {
        if let PathRule::Style(selector) = entry {
            rules.push(Node::rule(negate_selector(selector)?, Vec::new()));
        }
    }
    for entry in path {
        if let PathRule::At { name, params } = entry {
            rules.push(negate_at_rule(name, params)?);
        }
    }
    Some(rules)
}

/// Port of `negateSelector`.
fn negate_selector(selector: &str) -> Option<String> {
    if selector.contains("::") {
        return None;
    }
    // `&:hover` becomes `&:not(*:hover)`; the `*` is often optimized away.
    let selectors: Vec<String> = segment(selector, ',')
        .iter()
        .map(|sel| sel.replace('&', "*"))
        .collect();
    Some(format!("&:not({})", selectors.join(", ")))
}

/// Port of `negateAtRule`.
fn negate_at_rule(name: &str, params: &str) -> Option<Node> {
    if !CONDITIONAL_RULES.contains(&name) {
        return None;
    }
    let conditions = segment(params, ',');
    // `@media screen, print` would negate to a conjunction; not supported.
    if conditions.len() > 1 {
        return None;
    }
    let negated: Vec<String> = conditions
        .iter()
        .map(|condition| negate_condition(name, condition))
        .collect();
    Some(Node::at_rule(name, negated.join(", "), Vec::new()))
}

/// Port of `negateConditions` for one condition.
fn negate_condition(rule_name: &str, condition: &str) -> String {
    if rule_name == CONTAINER {
        return negate_container_condition(condition);
    }
    let condition = js_trim(condition);
    let parts = segment(condition, ' ');
    // `@media not {query}`, `@supports not {query}`
    if parts.first().is_some_and(|part| part == NOT) {
        return parts[1..].join(" ");
    }
    format!("{NOT} {condition}")
}

fn negate_container_condition(condition: &str) -> String {
    let mut ast = value_parser::parse(js_trim(condition));
    let is_function = |node: &ValueNode| matches!(node, ValueNode::Function { .. });
    let is_word = |node: &ValueNode| matches!(node, ValueNode::Word(_));
    let is_not = |node: &ValueNode| matches!(node, ValueNode::Word(word) if word == NOT);
    // `@container {query}`
    if ast.first().is_some_and(is_function) {
        return format!("{NOT} {condition}");
    }
    // `@container not {query}`: drop the `not` and its separator.
    if ast.len() >= 3 && is_not(&ast[0]) && is_function(&ast[2]) {
        ast.drain(0..2);
        return value_parser::to_css(&ast);
    }
    // `@container {name} not {query}`: drop the `not` and its separator.
    if ast.len() >= 5 && is_word(&ast[0]) && is_not(&ast[2]) && is_function(&ast[4]) {
        ast.drain(2..4);
        return value_parser::to_css(&ast);
    }
    // `@container {name} {query}`: insert `not` after the name.
    if ast.len() >= 3 && is_word(&ast[0]) && !is_not(&ast[0]) && is_function(&ast[2]) {
        ast.splice(
            1..1,
            [
                ValueNode::Separator(" ".to_owned()),
                ValueNode::Word(NOT.to_owned()),
            ],
        );
        return value_parser::to_css(&ast);
    }
    format!("{NOT} {condition}")
}

/* ----------------------------- selector walks ---------------------------- */

/// Rewrite the selector of every style rule, depth-first; `false` (Tailwind's
/// `didApply = false` and `WalkAction.Stop`) as soon as a style rule is
/// nested in another style rule.
fn rewrite_style_rules(
    nodes: &mut [Node],
    inside_style_rule: bool,
    rewrite: &dyn Fn(&str) -> String,
    did_apply: &mut bool,
) -> bool {
    for node in nodes {
        let is_style_rule = matches!(node, Node::Rule { .. });
        if let Node::Rule { selector, .. } = node {
            if inside_style_rule {
                *did_apply = false;
                return false;
            }
            *selector = rewrite(selector);
            *did_apply = true;
        }
        if let Some(children) = node.children_mut()
            && !rewrite_style_rules(
                children,
                inside_style_rule || is_style_rule,
                rewrite,
                did_apply,
            )
        {
            return false;
        }
    }
    true
}

/// Apply a selector rewrite to the compound's rule; `None` when nothing was
/// rewritten or a style rule is nested.
fn rewrite_rule(rule: &mut Node, rewrite: &dyn Fn(&str) -> String) -> Option<()> {
    let mut did_apply = false;
    rewrite_style_rules(std::slice::from_mut(rule), false, rewrite, &mut did_apply);
    did_apply.then_some(())
}

/// `group-*` and `peer-*`: CSS nesting cannot make `&` refer to only the
/// `.group`/`.peer` class, so `&` is replaced in the selector.
fn apply_relation(
    rule: &mut Node,
    variant: &Variant,
    ds: &DesignSystem,
    relation: Relation,
) -> Option<()> {
    if has_relative_sub_variant(variant) {
        return None;
    }
    let prefix = match ds.theme.prefix.as_deref() {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}\\:"),
        _ => String::new(),
    };
    let class = relation.class();
    // Name the group or peer by appending the modifier to its class.
    let variant_selector = match variant.modifier() {
        Some(modifier) => format!(":where(.{prefix}{class}\\/{})", modifier.value()),
        None => format!(":where(.{prefix}{class})"),
    };
    let combinator = relation.combinator();
    rewrite_rule(rule, &|selector| {
        let mut selector = selector.replace('&', &variant_selector);
        // Keep a selector list's matching behavior.
        if segment(&selector, ',').len() > 1 {
            selector = format!(":is({selector})");
        }
        // The target stays `&` inside `:is` (limits style invalidation, works
        // with `@apply`, and keeps `has-group-*` from anchoring the group).
        format!(":is({selector}{combinator}&)")
    })
}

/// `in-*`: `&:hover` becomes `:where(*:hover) &`.
fn apply_in(rule: &mut Node, variant: &Variant, _: &DesignSystem) -> Option<()> {
    if variant.modifier().is_some() {
        return None;
    }
    rewrite_rule(rule, &|selector| {
        format!(":where({}) &", selector.replace('&', "*"))
    })
}

/// `has-*`: `&:hover` becomes `&:has(*:hover)`.
fn apply_has(rule: &mut Node, variant: &Variant, _: &DesignSystem) -> Option<()> {
    if variant.modifier().is_some() {
        return None;
    }
    rewrite_rule(rule, &|selector| {
        format!("&:has({})", selector.replace('&', "*"))
    })
}

/* ------------------------------- functional ------------------------------ */

fn apply_aria(rule: &mut Node, variant: &Variant, _: &DesignSystem) -> Option<()> {
    let selector = match unmodified_value(variant)? {
        VariantValue::Arbitrary(value) => format!("[aria-{}]", quote_attribute_value(value)),
        VariantValue::Named(value) => format!("[aria-{value}=\"true\"]"),
    };
    attribute_selector::parse(&selector)?;
    wrap_children(rule, |nodes| Node::rule(format!("&{selector}"), nodes))
}

fn apply_data(rule: &mut Node, variant: &Variant, _: &DesignSystem) -> Option<()> {
    let value = unmodified_value(variant)?;
    let selector = format!("[data-{}]", quote_attribute_value(value.value()));
    attribute_selector::parse(&selector)?;
    wrap_children(rule, |nodes| Node::rule(format!("&{selector}"), nodes))
}

fn apply_nth(rule: &mut Node, variant: &Variant, pseudo: &str) -> Option<()> {
    let value = unmodified_value(variant)?;
    // Only numeric bare values are allowed.
    if let VariantValue::Named(value) = value
        && !is_positive_integer(value)
    {
        return None;
    }
    let selector = format!("&{pseudo}({})", value.value());
    wrap_children(rule, |nodes| Node::rule(selector, nodes))
}

/// Whether `value` starts with a function call (`/^[\w-]*\s*\(/`).
fn starts_with_function(value: &str) -> bool {
    let rest =
        value.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    rest.trim_start_matches(is_js_whitespace).starts_with('(')
}

fn apply_supports(rule: &mut Node, variant: &Variant, _: &DesignSystem) -> Option<()> {
    let mut value = unmodified_value(variant)?.value().to_owned();

    // `not()`, `selector()`, `font-tech()` and other functions are used
    // as-is, with spaces around `and`/`or`/`not` functions.
    if starts_with_function(&value) {
        let mut changed = false;
        let mut ast = value_parser::parse(&value);
        value_parser::walk_mut(&mut ast, &mut |node| {
            let ValueNode::Function { name, .. } = node else {
                return WalkAction::Continue;
            };
            // `selector(a:not(b))` keeps its `not` without spaces.
            if name == SUPPORTS_SELECTOR {
                return WalkAction::Skip;
            }
            if SUPPORTS_OPERATORS.contains(&name.as_str()) {
                changed = true;
                *name = format!(" {name} ");
            }
            WalkAction::Continue
        });
        let query = if changed {
            value_parser::to_css(&ast)
        } else {
            value
        };
        return wrap_children(rule, |nodes| Node::at_rule("@supports", query, nodes));
    }

    // `supports-[display]` is shorthand for `(display: var(--tw))`.
    if !value.contains(':') {
        value = format!("{value}: var(--tw)");
    }
    // `supports-[display:flex]` becomes `(display:flex)`.
    if !value.starts_with('(') || !value.ends_with(')') {
        value = format!("({value})");
    }
    wrap_children(rule, |nodes| Node::at_rule("@supports", value, nodes))
}

/* ---------------------- breakpoints and container queries ---------------- */

/// A resolved breakpoint or container width.
type Lookup = fn(&Variant, &Theme) -> Option<String>;

/// A resolved value usable in a media or container query: not empty and
/// without `var()`.
fn usable(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty() && !value.contains("var("))
}

/// The breakpoint of `sm`, `min-sm`, `max-[600px]` (port of
/// `resolvedBreakpoints`).
fn resolved_breakpoint(variant: &Variant, theme: &Theme) -> Option<String> {
    match variant {
        Variant::Static { root } => theme.resolve_value(Some(root), &[BREAKPOINT_NAMESPACE]),
        Variant::Functional { .. } => match unmodified_value(variant)? {
            VariantValue::Arbitrary(value) => usable(Some(value.clone())),
            VariantValue::Named(value) => {
                usable(theme.resolve_value(Some(value), &[BREAKPOINT_NAMESPACE]))
            }
        },
        Variant::Arbitrary { .. } | Variant::Compound { .. } => None,
    }
}

/// The width of `@md`, `@min-[400px]/name` (port of `resolvedWidths`).
fn resolved_width(variant: &Variant, theme: &Theme) -> Option<String> {
    match functional_parts(variant)?.0? {
        VariantValue::Arbitrary(value) => usable(Some(value.clone())),
        VariantValue::Named(value) => {
            usable(theme.resolve_value(Some(value), &[CONTAINER_NAMESPACE]))
        }
    }
}

/// Port of `compareBreakpointVariants`.
fn compare_breakpoint_variants(
    a: &Variant,
    z: &Variant,
    direction: Direction,
    lookup: Lookup,
    theme: &Theme,
) -> Ordering {
    if a == z {
        return Ordering::Equal;
    }
    let unresolved_first = match direction {
        Direction::Asc => Ordering::Less,
        Direction::Desc => Ordering::Greater,
    };
    let Some(a_value) = lookup(a, theme) else {
        return unresolved_first;
    };
    let Some(z_value) = lookup(z, theme) else {
        return unresolved_first.reverse();
    };
    compare_breakpoints(&a_value, &z_value, direction)
}

fn compare_with(direction: Direction, lookup: Lookup, theme: &Rc<Theme>) -> CompareFn {
    let theme = Rc::clone(theme);
    Rc::new(move |a, z| compare_breakpoint_variants(a, z, direction, lookup, &theme))
}

/// Nest the rule in `@media (width <op> value)`.
fn wrap_media(rule: &mut Node, operator: &str, value: &str) -> Option<()> {
    let params = format!("(width {operator} {value})");
    wrap_children(rule, |nodes| Node::at_rule("@media", params, nodes))
}

/// `max-*`, the named breakpoints and `min-*`.
fn register_breakpoints(variants: &mut Variants, theme: &Theme, creation_theme: &Rc<Theme>) {
    variants.group(
        |variants| {
            variants.functional(
                "max",
                |rule, variant, ds| {
                    let value = resolved_breakpoint(variant, &ds.theme)?;
                    wrap_media(rule, "<", &value)
                },
                Some(compounds::AT_RULES),
                None,
            );
        },
        Some(compare_with(
            Direction::Desc,
            resolved_breakpoint,
            creation_theme,
        )),
    );

    let breakpoints = theme.namespace(BREAKPOINT_NAMESPACE);
    variants.group(
        |variants| {
            // `sm`, `md`, `lg`, …
            for (key, value) in breakpoints {
                let Some(key) = key else { continue };
                variants.static_variant(
                    &key,
                    move |rule, _, _| wrap_media(rule, ">=", &value),
                    Some(compounds::AT_RULES),
                    None,
                );
            }
            variants.functional(
                "min",
                |rule, variant, ds| {
                    let value = resolved_breakpoint(variant, &ds.theme)?;
                    wrap_media(rule, ">=", &value)
                },
                Some(compounds::AT_RULES),
                None,
            );
        },
        Some(compare_with(
            Direction::Asc,
            resolved_breakpoint,
            creation_theme,
        )),
    );
}

/// Nest the rule in `@container [name] (width <op> value)`.
fn wrap_container(
    rule: &mut Node,
    variant: &Variant,
    ds: &DesignSystem,
    operator: &str,
) -> Option<()> {
    let value = resolved_width(variant, &ds.theme)?;
    let params = match variant.modifier() {
        Some(name) => format!("{} (width {operator} {value})", name.value()),
        None => format!("(width {operator} {value})"),
    };
    wrap_children(rule, |nodes| Node::at_rule(CONTAINER, params, nodes))
}

/// `@max-*`, `@*` and `@min-*`.
fn register_container_queries(variants: &mut Variants, creation_theme: &Rc<Theme>) {
    variants.group(
        |variants| {
            variants.functional(
                "@max",
                |rule, variant, ds| wrap_container(rule, variant, ds, "<"),
                Some(compounds::AT_RULES),
                None,
            );
        },
        Some(compare_with(
            Direction::Desc,
            resolved_width,
            creation_theme,
        )),
    );
    variants.group(
        |variants| {
            variants.functional(
                "@",
                |rule, variant, ds| wrap_container(rule, variant, ds, ">="),
                Some(compounds::AT_RULES),
                None,
            );
            variants.functional(
                "@min",
                |rule, variant, ds| wrap_container(rule, variant, ds, ">="),
                Some(compounds::AT_RULES),
                None,
            );
        },
        Some(compare_with(Direction::Asc, resolved_width, creation_theme)),
    );
}

/* -------------------------------- helpers -------------------------------- */

/// Case-sensitivity flags of attribute selectors (`[a="b" i]`).
const SENSITIVITY_FLAGS: &[char] = &['i', 'I', 's', 'S'];

/// Quote the value of `attribute=value` (port of `quoteAttributeValue`):
/// `state=open` becomes `state="open"`, `lang=en i` becomes `lang="en" i`.
fn quote_attribute_value(input: &str) -> String {
    if !input.contains('=') {
        return input.to_owned();
    }
    let parts = segment(input, '=');
    let attribute = parts.first().map_or("", String::as_str);
    let rest = parts.get(1..).unwrap_or_default().join("=");
    let value = js_trim(&rest);

    // Already quoted.
    if value.starts_with(['\'', '"']) {
        return input.to_owned();
    }

    // A case-sensitivity flag after an unquoted value.
    let mut chars = value.chars().rev();
    if let (Some(flag), Some(' ')) = (chars.next(), chars.next())
        && SENSITIVITY_FLAGS.contains(&flag)
    {
        let unflagged = &value[..value.len() - 2];
        return format!("{attribute}=\"{unflagged}\" {flag}");
    }

    format!("{attribute}=\"{value}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stylesheet::{candidates_to_css, default_design_system};

    /// The CSS of each candidate on its own (`None` when it produces none).
    fn css(candidates: &[&str]) -> Vec<Option<String>> {
        let mut ds = default_design_system();
        candidates_to_css(&mut ds, candidates)
    }

    fn one(candidate: &str) -> Option<String> {
        css(&[candidate]).pop().flatten()
    }

    #[test]
    fn quotes_attribute_values() {
        assert_eq!(quote_attribute_value("state=open"), "state=\"open\"");
        assert_eq!(quote_attribute_value("state='open'"), "state='open'");
        assert_eq!(quote_attribute_value("lang=en i"), "lang=\"en\" i");
        assert_eq!(quote_attribute_value("lang=en S"), "lang=\"en\" S");
        assert_eq!(quote_attribute_value("a=b=c"), "a=\"b=c\"");
        assert_eq!(quote_attribute_value("active"), "active");
    }

    #[test]
    fn negates_conditions() {
        assert_eq!(
            negate_condition("@media", "(hover: hover)"),
            "not (hover: hover)"
        );
        assert_eq!(negate_condition("@media", "not print"), "print");
        assert_eq!(
            negate_condition("@supports", " (display: grid)"),
            "not (display: grid)"
        );
        assert_eq!(
            negate_condition("@container", "(width >= 1px)"),
            "not (width >= 1px)"
        );
        assert_eq!(
            negate_condition("@container", "not (width >= 1px)"),
            "(width >= 1px)"
        );
        assert_eq!(
            negate_condition("@container", "main not (width >= 1px)"),
            "main (width >= 1px)"
        );
        assert_eq!(
            negate_condition("@container", "main (width >= 1px)"),
            "main not (width >= 1px)"
        );
        assert_eq!(
            negate_selector("&:hover, &:focus").as_deref(),
            Some("&:not(*:hover,  *:focus)")
        );
        assert_eq!(negate_selector("&::before"), None);
    }

    #[test]
    fn supports_function_detection() {
        assert!(starts_with_function("not(display:grid)"));
        assert!(starts_with_function("font-tech (color-COLRv1)"));
        assert!(starts_with_function("(display:grid)"));
        assert!(!starts_with_function("display:grid"));
    }

    // Ported from variants.test.ts: each case uses the arbitrary-property
    // utility so no utility port is needed.

    #[test]
    fn pseudo_class_and_element_variants() {
        assert_eq!(
            one("hover:[color:red]").as_deref(),
            Some(
                "@media (hover: hover) {\n  .hover\\:\\[color\\:red\\]:hover {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("*:[color:red]").as_deref(),
            Some(":is(.\\*\\:\\[color\\:red\\] > *) {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("first-letter:[color:red]").as_deref(),
            Some(".first-letter\\:\\[color\\:red\\]::first-letter {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("open:[color:red]").as_deref(),
            Some(
                ".open\\:\\[color\\:red\\]:is([open], :popover-open, :open) {\n  color: red;\n}\n"
            )
        );
    }

    #[test]
    fn before_registers_content_property() {
        assert_eq!(
            one("before:[color:red]").as_deref(),
            Some(
                ".before\\:\\[color\\:red\\]::before {\n  content: var(--tw-content);\n  color: red;\n}\n@property --tw-content {\n  syntax: \"*\";\n  initial-value: \"\";\n  inherits: false;\n}\n"
            )
        );
        assert_eq!(one("before/foo:[color:red]"), None);
    }

    #[test]
    fn group_and_peer() {
        assert_eq!(
            one("group-focus:[color:red]").as_deref(),
            Some(
                ":is(:where(.group):focus .group-focus\\:\\[color\\:red\\]) {\n  color: red;\n}\n"
            )
        );
        assert_eq!(
            one("group-hover/name:[color:red]").as_deref(),
            Some(
                "@media (hover: hover) {\n  :is(:where(.group\\/name):hover .group-hover\\/name\\:\\[color\\:red\\]) {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("peer-[&_p]:[color:red]").as_deref(),
            Some(
                ":is(:where(.peer) p ~ .peer-\\[\\&_p\\]\\:\\[color\\:red\\]) {\n  color: red;\n}\n"
            )
        );
        assert_eq!(one("peer-[@media_foo]:[color:red]"), None);
        assert_eq!(
            one("group-[&:hover,&:focus]:[color:red]").as_deref(),
            Some(
                ":is(:is(:where(.group):hover, :where(.group):focus) .group-\\[\\&\\:hover\\,\\&\\:focus\\]\\:\\[color\\:red\\]) {\n  color: red;\n}\n"
            )
        );
        assert_eq!(one("group-[>img]:[color:red]"), None);
        assert_eq!(one("group-not-hover:[color:red]"), None);
    }

    #[test]
    fn negation() {
        assert_eq!(
            one("not-first:[color:red]").as_deref(),
            Some(".not-first\\:\\[color\\:red\\]:not(:first-child) {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("not-print:[color:red]").as_deref(),
            Some(
                "@media not print {\n  .not-print\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("not-supports-[display:grid]:[color:red]").as_deref(),
            Some(
                "@supports not (display:grid) {\n  .not-supports-\\[display\\:grid\\]\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("not-hover:[color:red]").as_deref(),
            Some(
                ".not-hover\\:\\[color\\:red\\]:not(:hover) {\n  color: red;\n}\n@media not (hover: hover) {\n  .not-hover\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("not-@sm:[color:red]").as_deref(),
            Some(
                "@container not (width >= 24rem) {\n  .not-\\@sm\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("not-[&:hover,&:focus]:[color:red]").as_deref(),
            Some(
                ".not-\\[\\&\\:hover\\,\\&\\:focus\\]\\:\\[color\\:red\\]:not(:hover, :focus) {\n  color: red;\n}\n"
            )
        );
        assert_eq!(one("not-[@media_print,screen]:[color:red]"), None);
        assert_eq!(one("not-before:[color:red]"), None);
        assert_eq!(one("not-starting:[color:red]"), None);
        assert_eq!(one("not-hover/foo:[color:red]"), None);
    }

    #[test]
    fn in_and_has() {
        assert_eq!(
            one("in-focus:[color:red]").as_deref(),
            Some(":where(:focus) .in-focus\\:\\[color\\:red\\] {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("has-checked:[color:red]").as_deref(),
            Some(".has-checked\\:\\[color\\:red\\]:has(:checked) {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("in-[p]:[color:red]").as_deref(),
            Some(":where(:is(p)) .in-\\[p\\]\\:\\[color\\:red\\] {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("has-[>img]:[color:red]").as_deref(),
            Some(".has-\\[\\>img\\]\\:\\[color\\:red\\]:has( > img) {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("group-has-checked/parent-name:[color:red]").as_deref(),
            Some(
                ":is(:where(.group\\/parent-name):has(:checked) .group-has-checked\\/parent-name\\:\\[color\\:red\\]) {\n  color: red;\n}\n"
            )
        );
        assert_eq!(one("has-[@media_print]:[color:red]"), None);
        assert_eq!(one("has-checked/foo:[color:red]"), None);
    }

    #[test]
    fn attribute_variants() {
        assert_eq!(
            one("aria-checked:[color:red]").as_deref(),
            Some(".aria-checked\\:\\[color\\:red\\][aria-checked=\"true\"] {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("aria-[sort=ascending]:[color:red]").as_deref(),
            Some(
                ".aria-\\[sort\\=ascending\\]\\:\\[color\\:red\\][aria-sort=\"ascending\"] {\n  color: red;\n}\n"
            )
        );
        assert_eq!(
            one("data-[lang=en_i]:[color:red]").as_deref(),
            Some(
                ".data-\\[lang\\=en_i\\]\\:\\[color\\:red\\][data-lang=\"en\" i] {\n  color: red;\n}\n"
            )
        );
        assert_eq!(one("data-[foo_bar]:[color:red]"), None);
        assert_eq!(one("data-active/foo:[color:red]"), None);
    }

    #[test]
    fn nth_variants() {
        assert_eq!(
            one("nth-3:[color:red]").as_deref(),
            Some(".nth-3\\:\\[color\\:red\\]:nth-child(3) {\n  color: red;\n}\n")
        );
        assert_eq!(
            one("nth-last-of-type-[2n+1]:[color:red]").as_deref(),
            Some(
                ".nth-last-of-type-\\[2n\\+1\\]\\:\\[color\\:red\\]:nth-last-of-type(2n+1) {\n  color: red;\n}\n"
            )
        );
        assert_eq!(one("nth--3:[color:red]"), None);
        assert_eq!(one("nth-foo:[color:red]"), None);
    }

    #[test]
    fn supports() {
        assert_eq!(
            one("supports-gap:[color:red]").as_deref(),
            Some(
                "@supports (gap: var(--tw)) {\n  .supports-gap\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("supports-[not(display:grid)]:[color:red]").as_deref(),
            Some(
                "@supports  not (display:grid) {\n  .supports-\\[not\\(display\\:grid\\)\\]\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("supports-[selector(A>B)]:[color:red]").as_deref(),
            Some(
                "@supports selector(A>B) {\n  .supports-\\[selector\\(A\\>B\\)\\]\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
    }

    #[test]
    fn breakpoints_and_containers() {
        assert_eq!(
            one("sm:[color:red]").as_deref(),
            Some(
                "@media (width >= 40rem) {\n  .sm\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("max-[600px]:[color:red]").as_deref(),
            Some(
                "@media (width < 600px) {\n  .max-\\[600px\\]\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(
            one("@sm/main:[color:red]").as_deref(),
            Some(
                "@container main (width >= 24rem) {\n  .\\@sm\\/main\\:\\[color\\:red\\] {\n    color: red;\n  }\n}\n"
            )
        );
        assert_eq!(one("max-sm/foo:[color:red]"), None);
        assert_eq!(one("min-[var(--x)]:[color:red]"), None);
    }

    #[test]
    fn breakpoint_variants_sort_by_value() {
        let ds = default_design_system();
        let parse = |v: &str| ds.parse_variant(v).unwrap_or_else(|| panic!("{v}"));
        let compare = |a: &str, z: &str| ds.variants.compare(Some(&parse(a)), Some(&parse(z)));
        assert_eq!(compare("sm", "md"), Ordering::Less);
        assert_eq!(compare("min-[900px]", "min-[100px]"), Ordering::Greater);
        assert_eq!(compare("max-sm", "max-md"), Ordering::Greater);
        assert_eq!(compare("@sm", "@min-md"), Ordering::Less);
        assert_eq!(compare("@max-sm", "@max-md"), Ordering::Greater);
    }
}
