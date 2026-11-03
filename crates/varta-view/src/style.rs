//! Stylesheets of draft v0: the checked stylesheet grammar and the cascade
//! subset (sheets by ascending id, then specificity, then source order;
//! inline `set_style` wins; custom properties inherit and `var()` is
//! substituted). Only custom properties inherit in the headless output; a
//! property's computed value is its cascaded value.
use std::collections::BTreeMap;

/// The renderer environment media conditions are evaluated against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Viewport width in CSS pixels.
    pub width: f64,
    pub dark: bool,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            width: DEFAULT_VIEWPORT_WIDTH,
            dark: false,
        }
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

/// Substitute `var(--name)` and `var(--name, fallback)` with the custom
/// properties in scope; `None` when a referenced property is missing and
/// has no fallback (the declaration is then invalid at computed-value time).
pub(crate) fn substitute(value: &str, custom: &BTreeMap<String, String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "var(".len()..];
        let end = after.find(')')?;
        let inner = &after[..end];
        let (name, fallback) = match inner.split_once(',') {
            Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
            None => (inner.trim(), None),
        };
        let name = name.strip_prefix("--")?;
        out.push_str(custom.get(name).map(String::as_str).or(fallback)?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_substitution_uses_fallbacks() {
        let custom = BTreeMap::from([("fg".to_string(), "#111".to_string())]);
        assert_eq!(
            substitute("1px solid var(--fg)", &custom).as_deref(),
            Some("1px solid #111")
        );
        assert_eq!(
            substitute("var(--bg, #fff)", &custom).as_deref(),
            Some("#fff")
        );
        assert_eq!(substitute("var(--bg)", &custom), None);
    }
}

/// Rule anchors preserve the shared matcher's semantics, including comma
/// specificity. Each rule is considered once even with duplicate class names.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RuleIndex {
    roots: Vec<usize>,
    classes: BTreeMap<String, Vec<usize>>,
    pub has_custom: bool,
}
impl RuleIndex {
    pub fn new(rules: &[Rule]) -> Self {
        let mut out = Self::default();
        for (index, rule) in rules.iter().enumerate() {
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
    custom: std::cell::OnceCell<BTreeMap<String, String>>,
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
    pub fn substitute(&self, value: &str) -> Option<String> {
        if !value.contains("var(") {
            return Some(value.to_owned());
        }
        substitute(
            value,
            self.custom
                .get_or_init(|| self.tree.custom_properties(self.id, self.environment)),
        )
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
