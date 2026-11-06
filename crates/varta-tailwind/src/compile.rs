//! Candidate compilation (port of Tailwind's `compile.ts`): utility lookup,
//! variant application, `!important`, and the utility sort order.
use crate::ast::{ContextValue, Node};
use crate::candidate::{Candidate, CandidateKind, Variant};
use crate::design_system::DesignSystem;
use crate::property_order;
use crate::utilities::{Compiled, Utility, as_color};
use crate::utils::compare::compare;
use crate::utils::escape;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// The deepest compound variant chain applied (`not-group-has-…`).
pub const MAX_VARIANT_DEPTH: usize = 64;

/// The sort key of one compiled rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertySort {
    /// Positions in the property order of the declared properties, ascending.
    pub order: Vec<usize>,
    /// The number of declarations.
    pub count: usize,
}

/// One rule compiled from a candidate.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub node: Node,
    pub property_sort: PropertySort,
}

/// The rules of a set of class tokens, sorted as Tailwind sorts utilities.
#[derive(Debug, Clone, Default)]
pub struct CompiledCandidates {
    pub nodes: Vec<Node>,
    /// Tokens that produced no CSS, in input order.
    pub invalid: Vec<String>,
}

/// Compile class tokens (port of `compileCandidates`).
#[must_use]
pub fn compile_candidates<'a>(
    raw_candidates: impl IntoIterator<Item = &'a str>,
    ds: &DesignSystem,
    respect_important: bool,
) -> CompiledCandidates {
    let mut out = CompiledCandidates::default();
    let mut matches: Vec<(&str, Vec<Candidate>)> = Vec::new();
    for raw in raw_candidates {
        if ds.invalid_candidates.contains(raw) {
            out.invalid.push(raw.to_owned());
            continue;
        }
        let candidates = ds.parse_candidate(raw);
        if candidates.is_empty() {
            out.invalid.push(raw.to_owned());
            continue;
        }
        if let Some(existing) = matches.iter_mut().find(|(r, _)| *r == raw) {
            existing.1 = candidates;
        } else {
            matches.push((raw, candidates));
        }
    }

    let variant_order = if matches.len() > 1 {
        Some(
            ds.variant_order(
                matches
                    .iter()
                    .flat_map(|(_, candidates)| candidates.iter())
                    .flat_map(|candidate| candidate.variants.iter()),
            ),
        )
    } else {
        None
    };

    struct Sorting<'s> {
        node: Node,
        properties: PropertySort,
        /// Set variant-order bits, highest first (a big-integer bitmask).
        variants: Vec<usize>,
        candidate: &'s str,
    }
    let mut sorted: Vec<Sorting<'_>> = Vec::new();
    for (raw, candidates) in &matches {
        let mut found = false;
        for candidate in candidates {
            let rules = ds.compile_ast_nodes(candidate, respect_important);
            if rules.is_empty() {
                continue;
            }
            found = true;
            for rule in rules {
                let mut bits: Vec<usize> = match &variant_order {
                    Some(order) => candidate
                        .variants
                        .iter()
                        .filter_map(|variant| order.get(variant).copied())
                        .collect(),
                    None => Vec::new(),
                };
                bits.sort_unstable_by(|a, z| z.cmp(a));
                bits.dedup();
                sorted.push(Sorting {
                    node: rule.node,
                    properties: rule.property_sort,
                    variants: bits,
                    candidate: raw,
                });
            }
        }
        if !found {
            out.invalid.push((*raw).to_owned());
        }
    }

    sorted.sort_by(|a, z| {
        a.variants
            .cmp(&z.variants)
            .then_with(|| {
                let offset = a
                    .properties
                    .order
                    .iter()
                    .zip(&z.properties.order)
                    .take_while(|(x, y)| x == y)
                    .count();
                let a_order = a.properties.order.get(offset).copied();
                let z_order = z.properties.order.get(offset).copied();
                match (a_order, z_order) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            })
            .then_with(|| z.properties.count.cmp(&a.properties.count))
            .then_with(|| compare(a.candidate, z.candidate))
    });
    out.nodes = sorted.into_iter().map(|s| s.node).collect();
    out
}

/// Compile one candidate into its rules (port of `compileAstNodes`).
#[must_use]
pub fn compile_ast_nodes(
    candidate: &Candidate,
    ds: &DesignSystem,
    respect_important: bool,
) -> Vec<CompiledRule> {
    let asts = compile_base_utility(candidate, ds);
    if asts.is_empty() {
        return Vec::new();
    }
    let respect_important = ds.important && respect_important;
    let selector = format!(".{}", escape(&candidate.raw));
    let mut rules = Vec::new();
    for mut nodes in asts {
        let property_sort = get_property_sort(&nodes);
        if candidate.important || respect_important {
            apply_important(&mut nodes);
        }
        let mut node = Node::rule(selector.clone(), nodes);
        for variant in &candidate.variants {
            if apply_variant(&mut node, variant, ds, 0).is_none() {
                return Vec::new();
            }
        }
        rules.push(CompiledRule {
            node,
            property_sort,
        });
    }
    rules
}

/// Apply one variant to a rule; `None` when the variant cannot apply
/// (port of `applyVariant`).
pub fn apply_variant(
    node: &mut Node,
    variant: &Variant,
    ds: &DesignSystem,
    depth: usize,
) -> Option<()> {
    if depth > MAX_VARIANT_DEPTH {
        return None;
    }
    match variant {
        Variant::Arbitrary { selector, relative } => {
            if *relative && depth == 0 {
                return None;
            }
            let children = std::mem::take(node.children_mut()?);
            let mut child = Node::from_prelude(selector, children);
            if let Node::AtRule { name, nodes, .. } = &mut child
                && name == "@scope"
            {
                let slotted = std::mem::take(nodes);
                *nodes = vec![Node::context(source_context("user"), slotted)];
                child = Node::context(source_context("variant"), vec![child]);
            }
            *node.children_mut()? = vec![child];
            Some(())
        }
        Variant::Compound {
            root, variant: sub, ..
        } => {
            let entry = ds.variants.get(root)?;
            let mut isolated = Node::at_rule("@slot", "", Vec::new());
            apply_variant(&mut isolated, sub, ds, depth + 1)?;
            let isolated_nodes = match &mut isolated {
                Node::AtRule { nodes, .. } => nodes,
                _ => return None,
            };
            if root == "not" && isolated_nodes.len() > 1 {
                return None;
            }
            for child in isolated_nodes.iter_mut() {
                if !child.is_rule_like() {
                    return None;
                }
                (entry.apply)(child, variant, ds)?;
            }
            // Replace the placeholder (the first empty rule on each path)
            // with the node's children.
            let replacement = std::mem::take(node.children_mut()?);
            fill_empty_rules(isolated_nodes, &replacement);
            *node.children_mut()? = std::mem::take(isolated_nodes);
            Some(())
        }
        Variant::Static { root } | Variant::Functional { root, .. } => {
            let entry = ds.variants.get(root)?;
            (entry.apply)(node, variant, ds)
        }
    }
}

/// Fill every empty style rule or at-rule with `replacement`, without
/// descending into filled rules (Tailwind's walk with `WalkAction.Skip`).
fn fill_empty_rules(nodes: &mut [Node], replacement: &[Node]) {
    for node in nodes {
        match node {
            Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } if nodes.is_empty() => {
                *nodes = replacement.to_vec();
            }
            _ => {
                if let Some(children) = node.children_mut() {
                    fill_empty_rules(children, replacement);
                }
            }
        }
    }
}

fn source_context(source: &str) -> BTreeMap<String, ContextValue> {
    BTreeMap::from([("source".to_owned(), ContextValue::Text(source.to_owned()))])
}

fn is_fallback_utility(utility: &Utility) -> bool {
    utility.types.len() > 1 && utility.types.iter().any(|t| t == "any")
}

fn compile_base_utility(candidate: &Candidate, ds: &DesignSystem) -> Vec<Vec<Node>> {
    if let CandidateKind::Arbitrary {
        property,
        value,
        modifier,
    } = &candidate.kind
    {
        let value = match modifier {
            Some(_) => as_color(value, modifier.as_ref(), &ds.theme),
            None => Some(value.clone()),
        };
        return match value {
            Some(value) => vec![vec![Node::decl(property.clone(), value)]],
            None => Vec::new(),
        };
    }
    let Some(kind) = candidate.utility_kind() else {
        return Vec::new();
    };
    let utilities = ds.utilities.get(candidate.root());
    let mut asts = Vec::new();
    for fallback in [false, true] {
        for utility in utilities
            .iter()
            .filter(|u| is_fallback_utility(u) == fallback)
        {
            if utility.kind != kind {
                continue;
            }
            match (utility.compile)(candidate, ds) {
                Compiled::Skip => {}
                Compiled::Invalid => {
                    if !utility.types.is_empty() {
                        return asts;
                    }
                }
                Compiled::Nodes(nodes) => asts.push(nodes),
            }
        }
        if !asts.is_empty() {
            return asts;
        }
    }
    asts
}

fn apply_important(nodes: &mut [Node]) {
    for node in nodes {
        match node {
            Node::AtRoot(_) => {}
            Node::Decl { important, .. } => *important = true,
            Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } => apply_important(nodes),
            Node::Comment(_) | Node::Context { .. } => {}
        }
    }
}

/// The property sort key of compiled nodes (port of `getPropertySort`).
#[must_use]
pub fn get_property_sort(nodes: &[Node]) -> PropertySort {
    let mut order = std::collections::BTreeSet::new();
    let mut count = 0usize;
    let mut queue: std::collections::VecDeque<&Node> = nodes.iter().collect();
    let mut seen_tw_sort = false;
    while let Some(node) = queue.pop_front() {
        match node {
            Node::Decl {
                property, value, ..
            } => {
                let Some(value) = value else { continue };
                count += 1;
                if seen_tw_sort {
                    continue;
                }
                if property == "--tw-sort"
                    && let Some(index) = property_order::position(value)
                {
                    order.insert(index);
                    seen_tw_sort = true;
                    continue;
                }
                if let Some(index) = property_order::position(property) {
                    order.insert(index);
                }
            }
            Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } => queue.extend(nodes.iter()),
            _ => {}
        }
    }
    PropertySort {
        order: order.into_iter().collect(),
        count,
    }
}

/// Mark every declaration important, except inside at-root nodes.
pub fn mark_important(nodes: &mut [Node]) {
    apply_important(nodes);
}
