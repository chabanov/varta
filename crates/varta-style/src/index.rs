//! Rule anchors, cross-sheet candidate sets, cascade layer ranks and the
//! document's registered custom properties.
use crate::prelude::*;
use varta_common::view::expansion;
use varta_common::view::stylesheet::{
    LayerDeclaration, LayerName, Media, Name, ParsedStylesheet, Registration, Rule,
};

/// Rule anchors preserve the shared matcher's semantics, including comma
/// specificity. Each rule is considered once even with duplicate class names.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleIndex {
    roots: Vec<usize>,
    classes: BTreeMap<String, Vec<usize>>,
    pub has_custom: bool,
    pub costs: Vec<RuleCost>,
    pub expansion: Vec<Vec<(Name, expansion::Value)>>,
    /// The sheet's layer names, which rules refer to by index.
    pub layers: Vec<Vec<LayerName>>,
    /// The sheet's layer declarations in source order (Cascade 5 §6.4.3).
    pub layer_declarations: Vec<LayerDeclaration>,
    /// The sheet's `@property` registrations in sheet order.
    pub properties: Vec<Registration>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuleCost {
    pub work: usize,
    pub variables: bool,
}
impl RuleIndex {
    /// The index of a parsed sheet: its rules, layers and registrations.
    #[must_use]
    pub fn parsed(parsed: &ParsedStylesheet) -> Self {
        let mut index = Self::new(&parsed.rules);
        index.layers.clone_from(&parsed.layers);
        index
            .layer_declarations
            .clone_from(&parsed.layer_declarations);
        index.properties.clone_from(&parsed.properties);
        index.has_custom |= !index.properties.is_empty();
        index
    }
    #[must_use]
    pub fn new(rules: &[Rule]) -> Self {
        let mut out = Self::default();
        for (index, rule) in rules.iter().enumerate() {
            out.expansion.push(
                rule.declarations
                    .iter()
                    .map(|(name, value)| (name.clone(), expansion::Value::new(value)))
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

/// A layer path across sheets: named segments are shared by every sheet of
/// the document, anonymous ones belong to their sheet (Cascade 5 §6.4.2).
type LayerKey = Vec<(Option<u32>, LayerName)>;

/// One sheet's layers as the ranking needs them.
#[derive(Debug, Clone, PartialEq, Default)]
struct SheetLayers {
    layers: Vec<Vec<LayerName>>,
    declarations: Vec<LayerDeclaration>,
}

/// The cascade rank of every sheet-local layer (Cascade 5 §6.4.3): layers
/// in order of first declaration across sheets by ascending id, counting a
/// declaration inside a conditional rule only while its condition holds, a
/// layer's own rules after its sublayers, unlayered rules after every layer.
fn layer_ranks(
    sheets: &BTreeMap<u32, SheetLayers>,
    media: Option<&Media>,
) -> BTreeMap<(u32, usize), usize> {
    #[derive(Default)]
    struct Node {
        children: Vec<((Option<u32>, LayerName), Node)>,
        index: BTreeMap<(Option<u32>, LayerName), usize>,
    }
    let key = |sheet: u32, path: &[LayerName]| -> LayerKey {
        path.iter()
            .map(|segment| match segment {
                LayerName::Anonymous(_) => (Some(sheet), segment.clone()),
                LayerName::Named(_) => (None, segment.clone()),
            })
            .collect()
    };
    let mut root = Node::default();
    for (sheet, layers) in sheets {
        for declaration in &layers.declarations {
            let active =
                media.is_none_or(|media| declaration.media.iter().all(|list| list.matches(media)));
            let Some(path) = layers.layers.get(declaration.layer) else {
                continue;
            };
            if !active {
                continue;
            }
            let mut node = &mut root;
            for segment in key(*sheet, path) {
                let position = match node.index.get(&segment) {
                    Some(position) => *position,
                    None => {
                        node.index.insert(segment.clone(), node.children.len());
                        node.children.push((segment, Node::default()));
                        node.children.len() - 1
                    }
                };
                node = &mut node.children[position].1;
            }
        }
    }
    // Post-order numbering without recursion.
    let mut ranks: BTreeMap<LayerKey, usize> = BTreeMap::new();
    let mut stack: Vec<(&Node, LayerKey, usize)> = vec![(&root, Vec::new(), 0)];
    let mut next = 0usize;
    while let Some((node, path, child)) = stack.pop() {
        if let Some((segment, inner)) = node.children.get(child) {
            stack.push((node, path.clone(), child + 1));
            let mut inner_path = path;
            inner_path.push(segment.clone());
            stack.push((inner, inner_path, 0));
        } else if !path.is_empty() {
            ranks.insert(path, next);
            next += 1;
        }
    }
    let mut out = BTreeMap::new();
    for (sheet, layers) in sheets {
        for (local, path) in layers.layers.iter().enumerate() {
            if let Some(rank) = ranks.get(&key(*sheet, path)) {
                out.insert((*sheet, local), *rank);
            }
        }
    }
    out
}

/// Layer ranks for one media environment.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerRanks {
    ranks: BTreeMap<(u32, usize), usize>,
}
impl LayerRanks {
    /// The cascade rank of a rule's layer; unlayered rules outrank every
    /// layer (Cascade 5 §6.4.3, normal declarations).
    #[must_use]
    pub fn rank(&self, sheet: u32, layer: Option<usize>) -> usize {
        layer.map_or(usize::MAX, |local| {
            self.ranks
                .get(&(sheet, local))
                .copied()
                .unwrap_or(usize::MAX - 1)
        })
    }
}

/// Incremental cross-sheet anchors avoid scanning unrelated (or empty) sheets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CascadeIndex {
    roots: BTreeSet<(u32, usize)>,
    classes: BTreeMap<String, BTreeSet<(u32, usize)>>,
    custom_sheets: usize,
    layers: BTreeMap<u32, SheetLayers>,
    /// Ranks when no layer declaration is conditional (else `None`).
    static_ranks: Option<BTreeMap<(u32, usize), usize>>,
    registrations: BTreeMap<u32, Vec<Registration>>,
    registered: BTreeMap<String, Registration>,
}
impl CascadeIndex {
    /// The layer ranks under `media`.
    #[must_use]
    pub fn ranks(&self, media: &Media) -> LayerRanks {
        LayerRanks {
            ranks: self
                .static_ranks
                .clone()
                .unwrap_or_else(|| layer_ranks(&self.layers, Some(media))),
        }
    }
    /// Whether the layer order depends on media conditions.
    #[must_use]
    pub fn conditional_layers(&self) -> bool {
        self.static_ranks.is_none()
    }
    /// The registered custom properties of the document (Properties and
    /// Values API 1 §2.1: the last registration in sheet order wins).
    #[must_use]
    pub fn registered(&self) -> &BTreeMap<String, Registration> {
        &self.registered
    }
    fn rebuild(&mut self) {
        let conditional = self
            .layers
            .values()
            .any(|sheet| sheet.declarations.iter().any(|d| !d.media.is_empty()));
        self.static_ranks = (!conditional).then(|| layer_ranks(&self.layers, None));
        self.registered = self
            .registrations
            .values()
            .flatten()
            .map(|registration| (registration.name.clone(), registration.clone()))
            .collect();
    }
    pub fn insert(&mut self, sheet: u32, index: &RuleIndex) {
        if !index.layers.is_empty() {
            self.layers.insert(
                sheet,
                SheetLayers {
                    layers: index.layers.clone(),
                    declarations: index.layer_declarations.clone(),
                },
            );
        }
        if !index.properties.is_empty() {
            self.registrations.insert(sheet, index.properties.clone());
        }
        self.rebuild();
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
        if self.layers.remove(&sheet).is_some() | self.registrations.remove(&sheet).is_some() {
            self.rebuild();
        }
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
    #[must_use]
    pub fn has_custom(&self) -> bool {
        self.custom_sheets != 0
    }
    #[must_use]
    pub fn candidates(&self, root: bool, classes: &BTreeSet<&str>) -> Vec<(u32, usize)> {
        if root {
            return self.roots.iter().copied().collect();
        }
        let mut candidates = BTreeSet::new();
        for class in classes {
            if let Some(rules) = self.classes.get(*class) {
                candidates.extend(rules);
            }
        }
        candidates.into_iter().collect()
    }
}
