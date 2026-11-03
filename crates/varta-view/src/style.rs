//! Stylesheets of draft v0: the checked stylesheet grammar and the cascade
//! subset (sheets by ascending id, then specificity, then source order;
//! inline `set_style` wins; custom properties inherit and `var()` is
//! substituted). Only custom properties inherit in the headless output; a
//! property's computed value is its cascaded value.
use std::collections::BTreeMap;

/// Host media conditions and native reference metrics, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Viewport width in CSS pixels.
    pub width: f64,
    pub dark: bool,
    /// Reference document-root font size for `rem`, independent of the view
    /// root's own font size and the shaper's default text size.
    pub root_font_size: f64,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            width: DEFAULT_VIEWPORT_WIDTH,
            dark: false,
            root_font_size: varta_common::view::stylesheet::REM_PIXELS,
        }
    }
}
impl Environment {
    /// Set the host reference font. Zero is valid for geometry; text shaping
    /// still applies its independent minimum font size. No OS lookup occurs.
    pub fn with_root_font_size(mut self, size: f64) -> Result<Self, crate::LayoutError> {
        self.root_font_size = size;
        crate::units::Units::new(&self).ok_or(crate::LayoutError::RootFont)?;
        Ok(self)
    }
}
/// Default headless viewport width in CSS pixels.
pub const DEFAULT_VIEWPORT_WIDTH: f64 = 1024.0;
pub(crate) use varta_common::view::stylesheet::{Name, Rule, Subject};

/// Whether `rule`'s media conditions hold in `environment`.
pub(crate) fn applies(rule: &Rule, environment: &Environment) -> bool {
    rule.applies(environment.width, environment.dark)
}

/// Parse a whole stylesheet with the shared grammar
/// (`varta_common::view::stylesheet`), retaining its typed work-limit error.
pub(crate) fn parse(
    text: &str,
    allowance: usize,
) -> Result<
    varta_common::view::stylesheet::ParsedStylesheet,
    varta_common::view::stylesheet::StylesheetError,
> {
    varta_common::view::stylesheet::parse_with_work_limit(text, allowance)
}

/// Rule anchors preserve the shared matcher's semantics, including comma
/// specificity. Each rule is considered once even with duplicate class names.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RuleIndex {
    roots: Vec<usize>,
    classes: BTreeMap<String, Vec<usize>>,
    pub has_custom: bool,
    pub costs: Vec<RuleCost>,
    pub expansion: Vec<Vec<(Name, varta_common::view::expansion::Value)>>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RuleCost {
    pub work: usize,
    pub variables: bool,
}
impl RuleIndex {
    pub fn new(rules: &[Rule]) -> Self {
        let mut out = Self::default();
        for (index, rule) in rules.iter().enumerate() {
            out.expansion.push(
                rule.declarations
                    .iter()
                    .map(|(name, value)| {
                        (
                            name.clone(),
                            varta_common::view::expansion::Value::new(value),
                        )
                    })
                    .collect(),
            );
            out.costs.push(RuleCost {
                work: rule.cascade_work(),
                variables: rule
                    .declarations
                    .iter()
                    .any(|(_, value)| varta_common::view::css::contains_variable(value)),
            });
            out.has_custom |= rule
                .declarations
                .iter()
                .any(|(name, _)| matches!(name, Name::Custom(_)));
            for selector in &rule.selectors {
                let list = if selector.root {
                    &mut out.roots
                } else {
                    let Some(class) = selector.classes.first() else {
                        continue;
                    };
                    out.classes.entry(class.clone()).or_default()
                };
                if list.last() != Some(&index) {
                    list.push(index);
                }
            }
        }
        out
    }
}

/// One read's variable map is materialized only if a value actually uses var().
pub(crate) struct Variables<'a> {
    tree: &'a crate::tree::Tree,
    id: u32,
    environment: &'a Environment,
    custom: std::cell::OnceCell<crate::variables::Scope<'a>>,
}
impl<'a> Variables<'a> {
    pub fn new(tree: &'a crate::tree::Tree, id: u32, environment: &'a Environment) -> Self {
        Self {
            tree,
            id,
            environment,
            custom: std::cell::OnceCell::new(),
        }
    }
    pub fn property(
        &self,
        property: varta_common::view::CssProperty,
        value: &str,
    ) -> Option<String> {
        let result = self.substitute(value)?;
        varta_common::view::css::property_value(property, &result).then_some(result)
    }
    pub fn substitute(&self, value: &str) -> Option<String> {
        if !varta_common::view::css::contains_variable(value) {
            return Some(value.to_owned());
        }
        self.custom
            .get_or_init(|| self.tree.custom_properties(self.id, self.environment))
            .substitute(value)
    }
}

/// Incremental cross-sheet anchors avoid scanning unrelated (or empty) sheets.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CascadeIndex {
    roots: std::collections::BTreeSet<(u32, usize)>,
    classes: BTreeMap<String, std::collections::BTreeSet<(u32, usize)>>,
    custom_sheets: usize,
}
impl CascadeIndex {
    pub fn insert(&mut self, sheet: u32, index: &RuleIndex) {
        self.custom_sheets += usize::from(index.has_custom);
        self.roots
            .extend(index.roots.iter().map(|rule| (sheet, *rule)));
        for (class, rules) in &index.classes {
            self.classes
                .entry(class.clone())
                .or_default()
                .extend(rules.iter().map(|rule| (sheet, *rule)));
        }
    }
    pub fn remove(&mut self, sheet: u32, index: &RuleIndex) {
        self.custom_sheets -= usize::from(index.has_custom);
        for rule in &index.roots {
            self.roots.remove(&(sheet, *rule));
        }
        for (class, rules) in &index.classes {
            let empty = if let Some(entries) = self.classes.get_mut(class) {
                for rule in rules {
                    entries.remove(&(sheet, *rule));
                }
                entries.is_empty()
            } else {
                false
            };
            if empty {
                self.classes.remove(class);
            }
        }
    }
    pub fn has_custom(&self) -> bool {
        self.custom_sheets != 0
    }
    pub fn candidates(
        &self,
        root: bool,
        classes: &std::collections::BTreeSet<&str>,
    ) -> Vec<(u32, usize)> {
        if root {
            return self.roots.iter().copied().collect();
        }
        let mut candidates = std::collections::BTreeSet::new();
        for class in classes {
            if let Some(rules) = self.classes.get(*class) {
                candidates.extend(rules);
            }
        }
        candidates.into_iter().collect()
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use std::collections::BTreeSet;
    #[test]
    fn unrelated_and_duplicate_anchors_do_not_expand_candidate_lists() {
        let mut index = CascadeIndex::default();
        for sheet in 1..1001 {
            let parsed = parse(&format!(".unrelated{sheet} {{ color:#abc }}"), usize::MAX).unwrap();
            index.insert(sheet, &RuleIndex::new(&parsed.rules));
        }
        let rules = parse(".a, .a.b, .b, :root { color:#fff }", usize::MAX)
            .unwrap()
            .rules;
        let anchors = RuleIndex::new(&rules);
        index.insert(2000, &anchors);
        assert_eq!(
            index.candidates(false, &BTreeSet::from(["a", "b"])),
            vec![(2000, 0)]
        );
        assert_eq!(index.candidates(true, &BTreeSet::new()), vec![(2000, 0)]);
        index.remove(2000, &anchors);
        assert!(
            index
                .candidates(false, &BTreeSet::from(["a", "b"]))
                .is_empty()
        );
        assert!(index.candidates(true, &BTreeSet::new()).is_empty());
    }
}
