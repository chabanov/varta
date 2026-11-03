//! Render-only memoization. Never attach these reads to the mutable live tree.
use crate::{style::Environment, tree::Tree};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use varta_common::view::CssProperty;

const MAX_VALIDATION_ENTRIES: usize = 4096;
const MAX_VALIDATION_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Default)]
struct Validation {
    values: BTreeMap<CssProperty, BTreeMap<String, bool>>,
    literals: BTreeMap<String, bool>,
    entries: usize,
    bytes: usize,
}
#[derive(Debug)]
struct Prepared {
    environment: Environment,
    layout_environment: Option<Environment>,
    values: BTreeMap<u32, BTreeMap<CssProperty, String>>,
    validation: Arc<Mutex<Validation>>,
    geometry: Mutex<GeometryReads>,
}
impl Prepared {
    fn accepts(&self, env: &Environment) -> bool {
        self.environment == *env || self.layout_environment == Some(*env)
    }
}
#[derive(Debug, Default)]
struct GeometryReads {
    properties: BTreeMap<u32, BTreeMap<CssProperty, String>>,
    borders: BTreeMap<u32, [f32; 4]>,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct ReadCache(Option<Arc<Prepared>>);
// Memoized reads are not model state.
impl PartialEq for ReadCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl ReadCache {
    pub fn layout(&self, id: u32, env: &Environment) -> Option<BTreeMap<CssProperty, String>> {
        let prepared = self.0.as_ref().filter(|p| p.accepts(env))?;
        prepared
            .geometry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .properties
            .get(&id)
            .cloned()
    }
    pub fn remember_layout(
        &self,
        id: u32,
        env: &Environment,
        values: &BTreeMap<CssProperty, String>,
    ) {
        if let Some(prepared) = self
            .0
            .as_ref()
            .filter(|p| p.accepts(env) && p.values.contains_key(&id))
        {
            prepared
                .geometry
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .properties
                .insert(id, values.clone());
        }
    }
    pub fn borders(&self, id: u32, env: &Environment) -> Option<[f32; 4]> {
        let prepared = self.0.as_ref().filter(|p| p.accepts(env))?;
        prepared
            .geometry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .borders
            .get(&id)
            .copied()
    }
    pub fn remember_borders(&self, id: u32, env: &Environment, widths: [f32; 4]) {
        if let Some(prepared) = self
            .0
            .as_ref()
            .filter(|p| p.accepts(env) && p.values.contains_key(&id))
        {
            prepared
                .geometry
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .borders
                .insert(id, widths);
        }
    }
    pub fn get(&self, id: u32, env: &Environment) -> Option<&BTreeMap<CssProperty, String>> {
        let prepared = self.0.as_ref().filter(|p| p.accepts(env))?;
        prepared.values.get(&id)
    }
    pub fn contains_variable(&self, env: &Environment, value: &str) -> bool {
        let Some(prepared) = self.0.as_ref().filter(|p| p.accepts(env)) else {
            return varta_common::view::css::contains_variable(value);
        };
        let mut memo = prepared
            .validation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(found) = memo.literals.get(value) {
            return *found;
        }
        let found = varta_common::view::css::contains_variable(value);
        if memo.entries < MAX_VALIDATION_ENTRIES
            && memo.bytes.saturating_add(value.len()) <= MAX_VALIDATION_BYTES
        {
            memo.entries += 1;
            memo.bytes += value.len();
            memo.literals.insert(value.into(), found);
        }
        found
    }
    pub fn validate(&self, env: &Environment, property: CssProperty, value: &str) -> bool {
        let Some(prepared) = self.0.as_ref().filter(|p| p.accepts(env)) else {
            return varta_common::view::css::property_value(property, value);
        };
        let mut memo = prepared
            .validation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(valid) = memo
            .values
            .get(&property)
            .and_then(|values| values.get(value))
        {
            return *valid;
        }
        let valid = varta_common::view::css::property_value(property, value);
        if memo.entries < MAX_VALIDATION_ENTRIES
            && memo.bytes.saturating_add(value.len()) <= MAX_VALIDATION_BYTES
        {
            memo.bytes += value.len();
            memo.entries += 1;
            memo.values
                .entry(property)
                .or_default()
                .insert(value.to_owned(), valid);
        }
        valid
    }
}

pub(crate) fn same_sources(a: &Tree, b: &Tree) -> bool {
    a.focus == b.focus
        && a.hovered == b.hovered
        && a.active == b.active
        && a.sheets == b.sheets
        && a.nodes.len() == b.nodes.len()
        && a.nodes.iter().all(|(id, node)| {
            b.nodes.get(id).is_some_and(|other| {
                if node.parent != other.parent || !Arc::ptr_eq(&node.identity, &other.identity) {
                    return false;
                }
                match (a.element(*id), b.element(*id)) {
                    (Some(x), Some(y)) => {
                        x.tag == y.tag
                            && x.class == y.class
                            && x.style == y.style
                            && x.border_inline == y.border_inline
                            && x.layout_inline == y.layout_inline
                            && x.outline_inline == y.outline_inline
                            && x.checked == y.checked
                            && x.disabled() == y.disabled()
                    }
                    (None, None) => true,
                    _ => false,
                }
            })
        })
}

pub(crate) fn prepare(
    tree: &Tree,
    env: Environment,
    previous: Option<&Tree>,
) -> Result<Tree, crate::LayoutError> {
    let mut prepared = tree.clone();
    if let Some(previous) = previous
        && previous
            .render_reads
            .0
            .as_ref()
            .is_some_and(|p| p.environment == env)
        && same_sources(tree, previous)
    {
        prepared.render_reads = previous.render_reads.clone();
        return Ok(prepared);
    }
    // Layout takes an f32 viewport while painting retains its f64 media width.
    // Reuse raw style reads at the rounded width only when every media decision
    // agrees. A breakpoint between those widths must keep the reference split.
    let layout_env = Environment {
        width: f64::from(env.width as f32),
        ..env
    };
    let layout_environment = (layout_env != env
        && tree.sheets.values().all(|sheet| {
            sheet.rules.iter().all(|rule| {
                rule.applies(env.width, env.dark) == rule.applies(layout_env.width, layout_env.dark)
            })
        }))
    .then_some(layout_env);
    prepared.render_reads = ReadCache(Some(Arc::new(Prepared {
        environment: env,
        layout_environment,
        values: BTreeMap::new(),
        validation: previous
            .and_then(|previous| previous.render_reads.0.as_ref())
            .map_or_else(|| Arc::new(Mutex::default()), |p| Arc::clone(&p.validation)),
        geometry: Mutex::default(),
    })));
    let mut pending = vec![(varta_common::view::ROOT_NODE, 0usize)];
    let mut visited = 0usize;
    while let Some((id, depth)) = pending.pop() {
        visited += 1;
        if visited > 4096 || depth > 128 {
            return Err(crate::LayoutError::Limit);
        }
        let values = prepared.computed(id, &env);
        let hidden = id != varta_common::view::ROOT_NODE
            && values
                .get(&CssProperty::Display)
                .is_some_and(|v| v.eq_ignore_ascii_case("none"));
        // A fresh preparation has one owner, and computed reads do not clone it.
        if let Some(cache) = prepared.render_reads.0.as_mut().and_then(Arc::get_mut) {
            cache.values.insert(id, values);
        }
        if !hidden {
            pending.extend(
                tree.nodes[&id]
                    .children
                    .iter()
                    .rev()
                    .map(|&child| (child, depth + 1)),
            );
        }
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounded_layout_width_reuses_styles_when_media_decisions_agree() {
        let env = Environment {
            width: 130.4,
            ..Environment::default()
        };
        let rounded = Environment {
            width: f64::from(env.width as f32),
            ..env
        };
        assert_ne!(env, rounded);
        let tree = prepare(&Tree::default(), env, None).unwrap();
        assert!(
            tree.render_reads
                .get(varta_common::view::ROOT_NODE, &rounded)
                .is_some()
        );
        let dark = Environment {
            dark: true,
            ..rounded
        };
        assert!(
            tree.render_reads
                .get(varta_common::view::ROOT_NODE, &dark)
                .is_none()
        );
    }
}
