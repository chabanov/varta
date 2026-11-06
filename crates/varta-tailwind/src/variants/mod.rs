//! The variant registry (port of the `Variants` class and helpers of
//! Tailwind's `variants.ts`); the built-in variants are in `builtin`.
use crate::ast::{ContextValue, Node};
use crate::candidate::Variant;
use crate::compile::apply_variant;
use crate::design_system::DesignSystem;
use crate::theme::Theme;
use crate::utils::segment;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::rc::Rc;

mod builtin;

/// Whether a variant name is valid for `@custom-variant`
/// (`^@?[a-z0-9][a-zA-Z0-9_-]*(?<![_-])$`).
#[must_use]
pub fn is_valid_variant_name(name: &str) -> bool {
    let body = name.strip_prefix('@').unwrap_or(name);
    let mut chars = body.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    if !body
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return false;
    }
    !body.ends_with(['_', '-'])
}

/// Compound flags (`Compounds`).
pub mod compounds {
    pub const NEVER: u8 = 0;
    pub const AT_RULES: u8 = 1 << 0;
    pub const STYLE_RULES: u8 = 1 << 1;
}

/// The kind of a registered variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VariantKind {
    Static,
    Functional,
    Compound,
}

/// A variant's apply function: transforms the rule; `None` is Tailwind's
/// `null` (the variant cannot apply).
pub type ApplyFn = Rc<dyn Fn(&mut Node, &Variant, &DesignSystem) -> Option<()>>;
/// A group's compare function.
pub type CompareFn = Rc<dyn Fn(&Variant, &Variant) -> Ordering>;

/// One registered variant.
#[derive(Clone)]
pub struct VariantEntry {
    pub kind: VariantKind,
    pub order: u32,
    pub apply: ApplyFn,
    pub compounds_with: u8,
    pub compounds: u8,
}

/// The variant registry (port of `Variants`).
#[derive(Default, Clone)]
pub struct Variants {
    pub compare_fns: BTreeMap<u32, CompareFn>,
    variants: BTreeMap<String, VariantEntry>,
    /// Names in registration order.
    names: Vec<String>,
    group_order: Option<u32>,
    last_order: u32,
}

impl Variants {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a static variant.
    pub fn static_variant(
        &mut self,
        name: &str,
        apply: impl Fn(&mut Node, &Variant, &DesignSystem) -> Option<()> + 'static,
        compounds: Option<u8>,
        order: Option<u32>,
    ) {
        self.set(
            name,
            VariantKind::Static,
            Rc::new(apply),
            compounds::NEVER,
            compounds.unwrap_or(compounds::STYLE_RULES),
            order,
        );
    }

    /// Register a functional variant.
    pub fn functional(
        &mut self,
        name: &str,
        apply: impl Fn(&mut Node, &Variant, &DesignSystem) -> Option<()> + 'static,
        compounds: Option<u8>,
        order: Option<u32>,
    ) {
        self.set(
            name,
            VariantKind::Functional,
            Rc::new(apply),
            compounds::NEVER,
            compounds.unwrap_or(compounds::STYLE_RULES),
            order,
        );
    }

    /// Register a compound variant.
    pub fn compound(
        &mut self,
        name: &str,
        compounds_with: u8,
        apply: impl Fn(&mut Node, &Variant, &DesignSystem) -> Option<()> + 'static,
        compounds: Option<u8>,
        order: Option<u32>,
    ) {
        self.set(
            name,
            VariantKind::Compound,
            Rc::new(apply),
            compounds_with,
            compounds.unwrap_or(compounds::STYLE_RULES),
            order,
        );
    }

    /// Register a static variant from a `@custom-variant` body
    /// (port of `fromAst`).
    pub fn from_ast(&mut self, name: &str, ast: Vec<Node>) {
        let mut selectors = Vec::new();
        let mut uses_at_variant = false;
        let mut uses_at_scope = false;
        crate::ast::for_each(&ast, &mut |node, _| match node {
            Node::Rule { selector, .. } => selectors.push(selector.clone()),
            Node::AtRule { name, params, .. } if name == "@variant" => {
                let _ = params;
                uses_at_variant = true;
            }
            Node::AtRule { name, params, .. } if name != "@slot" => {
                if name == "@scope" {
                    uses_at_scope = true;
                }
                selectors.push(format!("{name} {params}"));
            }
            _ => {}
        });
        let compounds = compounds_for_selectors(&selectors);
        self.static_variant(
            name,
            move |rule, _, ds| {
                let mut body = ast.clone();
                if uses_at_variant {
                    substitute_at_variant(&mut body, ds).ok()?;
                }
                let children = rule.children_mut()?;
                if uses_at_scope {
                    let slotted = vec![Node::context(source("user"), std::mem::take(children))];
                    substitute_at_slot(&mut body, &slotted);
                    *children = vec![Node::context(source("variant"), body)];
                } else {
                    let slotted = std::mem::take(children);
                    substitute_at_slot(&mut body, &slotted);
                    *children = body;
                }
                Some(())
            },
            Some(compounds),
            None,
        );
    }

    /// Register variants that share one sort order (port of `group`).
    pub fn group(&mut self, register: impl FnOnce(&mut Self), compare: Option<CompareFn>) {
        let order = self.next_order();
        self.group_order = Some(order);
        if let Some(compare) = compare {
            self.compare_fns.insert(order, compare);
        }
        register(self);
        self.group_order = None;
    }

    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.variants.contains_key(name)
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&VariantEntry> {
        self.variants.get(name)
    }

    #[must_use]
    pub fn kind(&self, name: &str) -> Option<VariantKind> {
        self.variants.get(name).map(|v| v.kind)
    }

    /// Names in registration order.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Whether `parent` (a compound variant) compounds with `child`.
    #[must_use]
    pub fn compounds_with(&self, parent: &str, child: &Variant) -> bool {
        let Some(parent) = self.variants.get(parent) else {
            return false;
        };
        let child_compounds = match child {
            Variant::Arbitrary { selector, .. } => {
                compounds_for_selectors(std::slice::from_ref(selector))
            }
            other => match other.root().and_then(|root| self.variants.get(root)) {
                Some(entry) => entry.compounds,
                None => return false,
            },
        };
        if parent.kind != VariantKind::Compound {
            return false;
        }
        if child_compounds == compounds::NEVER || parent.compounds_with == compounds::NEVER {
            return false;
        }
        parent.compounds_with & child_compounds != 0
    }

    /// Order two variants (port of `compare`).
    #[must_use]
    pub fn compare(&self, a: Option<&Variant>, z: Option<&Variant>) -> Ordering {
        let (a, z) = match (a, z) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(a), Some(z)) => (a, z),
        };
        if a == z {
            return Ordering::Equal;
        }
        match (a, z) {
            (Variant::Arbitrary { selector: x, .. }, Variant::Arbitrary { selector: y, .. }) => {
                return if utf16_less(x, y) {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            (Variant::Arbitrary { .. }, _) => return Ordering::Greater,
            (_, Variant::Arbitrary { .. }) => return Ordering::Less,
            _ => {}
        }
        let (Some(a_root), Some(z_root)) = (a.root(), z.root()) else {
            return Ordering::Equal;
        };
        let a_order = self.variants.get(a_root).map_or(0, |v| v.order);
        let z_order = self.variants.get(z_root).map_or(0, |v| v.order);
        if a_order != z_order {
            return a_order.cmp(&z_order);
        }
        if let (
            Variant::Compound {
                variant: av,
                modifier: am,
                ..
            },
            Variant::Compound {
                variant: zv,
                modifier: zm,
                ..
            },
        ) = (a, z)
        {
            let order = self.compare(Some(av), Some(zv));
            if order != Ordering::Equal {
                return order;
            }
            return match (am, zm) {
                (Some(x), Some(y)) => {
                    if utf16_less(x.value(), y.value()) {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            };
        }
        if let Some(compare) = self.compare_fns.get(&a_order) {
            return compare(a, z);
        }
        if a_root != z_root {
            return if utf16_less(a_root, z_root) {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let (Some(a_value), Some(z_value)) = (a.value(), z.value()) else {
            return if a.value().is_none() {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        };
        use crate::candidate::VariantValue::Arbitrary;
        match (a_value, z_value) {
            (Arbitrary(_), crate::candidate::VariantValue::Named(_)) => Ordering::Greater,
            (crate::candidate::VariantValue::Named(_), Arbitrary(_)) => Ordering::Less,
            _ => {
                if utf16_less(a_value.value(), z_value.value()) {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
        }
    }

    fn set(
        &mut self,
        name: &str,
        kind: VariantKind,
        apply: ApplyFn,
        compounds_with: u8,
        compounds: u8,
        order: Option<u32>,
    ) {
        if let Some(existing) = self.variants.get_mut(name) {
            existing.kind = kind;
            existing.apply = apply;
            existing.compounds = compounds;
            return;
        }
        let order = match order {
            Some(order) => order,
            None => {
                self.last_order = self.next_order();
                self.last_order
            }
        };
        self.names.push(name.to_owned());
        self.variants.insert(
            name.to_owned(),
            VariantEntry {
                kind,
                order,
                apply,
                compounds_with,
                compounds,
            },
        );
    }

    fn next_order(&self) -> u32 {
        self.group_order.unwrap_or(self.last_order + 1)
    }
}

/// JavaScript `<` on strings: UTF-16 code unit order.
fn utf16_less(a: &str, z: &str) -> bool {
    a.encode_utf16().lt(z.encode_utf16())
}

/// The compound flags of a variant's selectors (port of
/// `compoundsForSelectors`).
#[must_use]
pub fn compounds_for_selectors(selectors: &[String]) -> u8 {
    let mut flags = compounds::NEVER;
    for selector in selectors {
        if selector.starts_with('@') {
            if !selector.starts_with("@media")
                && !selector.starts_with("@supports")
                && !selector.starts_with("@container")
            {
                return compounds::NEVER;
            }
            flags |= compounds::AT_RULES;
            continue;
        }
        if selector.contains("::") {
            return compounds::NEVER;
        }
        flags |= compounds::STYLE_RULES;
    }
    flags
}

fn source(value: &str) -> BTreeMap<String, ContextValue> {
    BTreeMap::from([("source".to_owned(), ContextValue::Text(value.to_owned()))])
}

/// Replace `@slot` with `nodes`, and wrap `@keyframes`/`@property` in
/// at-root nodes (port of `substituteAtSlot`).
pub fn substitute_at_slot(ast: &mut Vec<Node>, nodes: &[Node]) {
    let mut out = Vec::with_capacity(ast.len());
    for mut node in std::mem::take(ast) {
        match &mut node {
            Node::AtRule { name, .. } if name == "@slot" => {
                out.extend(nodes.iter().cloned());
                continue;
            }
            Node::AtRule { name, .. } if name == "@keyframes" || name == "@property" => {
                out.push(Node::AtRoot(vec![node]));
                continue;
            }
            _ => {}
        }
        if let Some(children) = node.children_mut() {
            substitute_at_slot(children, nodes);
        }
        out.push(node);
    }
    *ast = out;
}

/// A failed `@variant` substitution.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AtVariantError {
    #[error("cannot use `@variant` with empty variant")]
    Empty,
    #[error("cannot use `@variant` with unknown variant: {0}")]
    Unknown(String),
    #[error("cannot use `@variant` with variant: {0}")]
    NotApplicable(String),
}

/// Expand `@variant name { … }` rules (port of `substituteAtVariant`).
/// Returns whether any was found.
pub fn substitute_at_variant(
    ast: &mut Vec<Node>,
    ds: &DesignSystem,
) -> Result<bool, AtVariantError> {
    let mut found = false;
    let mut out = Vec::with_capacity(ast.len());
    for mut node in std::mem::take(ast) {
        if let Node::AtRule {
            name,
            params,
            nodes,
        } = &mut node
            && name == "@variant"
        {
            found = true;
            let body = std::mem::take(nodes);
            for compound in segment(params, ',') {
                let mut rule = Node::rule("&", body.clone());
                let stacked = segment(&compound, ':');
                for variant in stacked.iter().rev() {
                    let variant = variant.trim();
                    if variant.is_empty() {
                        return Err(AtVariantError::Empty);
                    }
                    let parsed = ds
                        .parse_variant(variant)
                        .ok_or_else(|| AtVariantError::Unknown(variant.to_owned()))?;
                    apply_variant(&mut rule, &parsed, ds, 0)
                        .ok_or_else(|| AtVariantError::NotApplicable(variant.to_owned()))?;
                }
                match rule {
                    Node::Rule { selector, nodes } if selector == "&" => out.extend(nodes),
                    other => out.push(other),
                }
            }
            continue;
        }
        if let Some(children) = node.children_mut() {
            found |= substitute_at_variant(children, ds)?;
        }
        out.push(node);
    }
    *ast = out;
    Ok(found)
}

/// Create the built-in variants (port of `createVariants`).
#[must_use]
pub fn create_variants(theme: &Theme) -> Variants {
    let mut variants = Variants::new();
    builtin::register(&mut variants, theme);
    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_names() {
        assert!(is_valid_variant_name("hocus"));
        assert!(is_valid_variant_name("@lg"));
        assert!(!is_valid_variant_name("Hocus"));
        assert!(!is_valid_variant_name("hocus-"));
        assert!(!is_valid_variant_name(""));
    }

    #[test]
    fn compounds_of_selectors() {
        assert_eq!(
            compounds_for_selectors(&["&:hover".into()]),
            compounds::STYLE_RULES
        );
        assert_eq!(
            compounds_for_selectors(&["@media (hover: hover)".into(), "&:hover".into()]),
            compounds::STYLE_RULES | compounds::AT_RULES
        );
        assert_eq!(
            compounds_for_selectors(&["&::before".into()]),
            compounds::NEVER
        );
        assert_eq!(
            compounds_for_selectors(&["@starting-style".into()]),
            compounds::NEVER
        );
    }
}
