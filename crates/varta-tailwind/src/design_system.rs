//! The design system: theme, utilities and variants (port of Tailwind's
//! `design-system.ts`).
use crate::ast::Node;
use crate::candidate::{Candidate, Variant, parse_candidate, parse_variant};
use crate::compile::{CompiledRule, compile_ast_nodes};
use crate::css_functions::substitute_functions;
use crate::theme::{Theme, options};
use crate::utilities::{Utilities, create_utilities, with_alpha};
use crate::variants::{Variants, create_variants, substitute_at_variant};
use std::collections::BTreeMap;

pub use crate::utilities::UtilityKind;
pub use crate::variants::VariantKind;

/// The compiled theme, utilities and variants of one stylesheet.
pub struct DesignSystem {
    pub theme: Theme,
    pub utilities: Utilities,
    pub variants: Variants,
    /// `@import "tailwindcss" important`: every utility declaration is important.
    pub important: bool,
    /// Candidates known to be invalid (`@source not inline(…)`).
    pub invalid_candidates: std::collections::BTreeSet<String>,
}

impl DesignSystem {
    /// Build the design system of `theme` (port of `buildDesignSystem`).
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        let utilities = create_utilities(&theme);
        let variants = create_variants(&theme);
        Self {
            theme,
            utilities,
            variants,
            important: false,
            invalid_candidates: Default::default(),
        }
    }

    /// Every interpretation of a class token.
    #[must_use]
    pub fn parse_candidate(&self, candidate: &str) -> Vec<Candidate> {
        parse_candidate(candidate, self)
    }

    /// One variant, or `None` when it is not a known variant.
    #[must_use]
    pub fn parse_variant(&self, variant: &str) -> Option<Variant> {
        parse_variant(variant, self)
    }

    /// Compile one candidate into rules; empty when it produces no CSS.
    /// Functions in arbitrary values (`--theme(…)`, `--spacing(…)`) are
    /// substituted, and a candidate whose substitution fails produces nothing.
    #[must_use]
    pub fn compile_ast_nodes(
        &self,
        candidate: &Candidate,
        respect_important: bool,
    ) -> Vec<CompiledRule> {
        let mut rules = compile_ast_nodes(candidate, self, respect_important);
        for rule in &mut rules {
            let mut nodes = vec![std::mem::replace(&mut rule.node, Node::AtRoot(Vec::new()))];
            if substitute_functions(&mut nodes, self).is_err() {
                return Vec::new();
            }
            if substitute_at_variant(&mut nodes, self).is_err() {
                return Vec::new();
            }
            rule.node = nodes.pop().unwrap_or(Node::AtRoot(Vec::new()));
        }
        rules
    }

    /// Resolve a theme path with an optional `/ alpha` modifier
    /// (port of `resolveThemeValue`).
    #[must_use]
    pub fn resolve_theme_value(&self, path: &str, force_inline: bool) -> Option<String> {
        let (path, modifier) = match path.rfind('/') {
            Some(index) => (path[..index].trim(), Some(path[index + 1..].trim())),
            None => (path, None),
        };
        let opts = if force_inline {
            options::INLINE
        } else {
            options::NONE
        };
        let value = self.theme.resolve(None, &[path], opts)?;
        match modifier {
            Some(modifier) if !modifier.is_empty() => Some(with_alpha(&value, modifier)),
            _ => Some(value),
        }
    }

    /// The sort group of every distinct variant among `variants`, by
    /// [`Variants::compare`] (port of `getVariantOrder`).
    #[must_use]
    pub fn variant_order<'a>(
        &self,
        variants: impl IntoIterator<Item = &'a Variant>,
    ) -> BTreeMap<Variant, usize> {
        let mut distinct: Vec<&Variant> = variants.into_iter().collect();
        distinct.sort();
        distinct.dedup();
        distinct.sort_by(|a, z| self.variants.compare(Some(a), Some(z)));
        let mut order = BTreeMap::new();
        let mut previous: Option<&Variant> = None;
        let mut index = 0usize;
        for variant in distinct {
            if let Some(previous) = previous
                && self.variants.compare(Some(previous), Some(variant)) != std::cmp::Ordering::Equal
            {
                index += 1;
            }
            order.insert(variant.clone(), index);
            previous = Some(variant);
        }
        order
    }
}
