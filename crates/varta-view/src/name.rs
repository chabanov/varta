//! Accessible names: the steps of accname 1.2 that the L0 v1 subset admits
//! (architecture.md, "Accessible names"). The subset has no `title`
//! attribute, no `role="presentation"`/`none`, no generated content and no
//! layout, so steps 2I (tooltip) and the CSS parts of 2F do not apply.
use crate::style::Environment;
use crate::tree::{Content, Element, Tree};
use unicode_segmentation::UnicodeSegmentation;
use varta_common::view::{Attribute, CssProperty, Tag};

use std::collections::BTreeMap;
use varta_common::view::accessibility::NAME_FROM_CONTENT;

/// Snapshot-local indices. Visibility and id/label lookup are evaluated once,
/// rather than scanning the document for every repeated naming relation.
pub(crate) struct Names<'a> {
    tree: &'a Tree,
    mask_graphemes: bool,
    ids: BTreeMap<&'a str, u32>,
    labels: BTreeMap<&'a str, Vec<u32>>,
    hidden: BTreeMap<u32, bool>,
    hidden_in_tree: BTreeMap<u32, bool>,
    /// Nodes whose `visibility` is not `visible` (CSS Display 3 §4.1): their
    /// text names nothing (accname 1.2 §4.3.2 step 2A), visible descendants do.
    invisible: std::collections::BTreeSet<u32>,
}

impl<'a> Names<'a> {
    pub(crate) fn new(tree: &'a Tree, environment: &Environment) -> Self {
        let mut names = Self {
            tree,
            mask_graphemes: false,
            ids: BTreeMap::new(),
            labels: BTreeMap::new(),
            hidden: BTreeMap::new(),
            hidden_in_tree: BTreeMap::new(),
            invisible: std::collections::BTreeSet::new(),
        };
        let order = tree.document_order();
        for id in order {
            let own = tree
                .element(id)
                .is_some_and(|e| hidden_element(tree, id, e, environment));
            let parent_hidden = tree.nodes[&id]
                .parent
                .is_some_and(|p| names.hidden_in_tree[&p]);
            names.hidden.insert(id, own);
            names.hidden_in_tree.insert(id, own || parent_hidden);
            let parent_invisible = tree.nodes[&id]
                .parent
                .is_some_and(|p| names.invisible.contains(&p));
            let invisible = match tree
                .element(id)
                .and_then(|_| tree.computed_property(id, environment, CssProperty::Visibility))
            {
                Some(value) if value != crate::style::INHERIT => value != "visible",
                _ => parent_invisible,
            };
            if invisible {
                names.invisible.insert(id);
            }
            if let Some(element) = tree.element(id) {
                if let Some(ident) = element.attributes.get(&Attribute::Id) {
                    names.ids.insert(ident, id);
                }
                if element.tag == Tag::Label
                    && let Some(ident) = element.attributes.get(&Attribute::For)
                {
                    names.labels.entry(ident).or_default().push(id);
                }
            }
        }
        names
    }

    /// Native alternatives use the same masks as painted password fields;
    /// legacy snapshots keep their established scalar-mask representation.
    pub(crate) fn native(tree: &'a Tree, environment: &Environment) -> Self {
        Self {
            mask_graphemes: true,
            ..Self::new(tree, environment)
        }
    }

    pub(crate) fn hidden(&self, id: u32) -> bool {
        self.hidden[&id]
    }

    pub(crate) fn resolve_id(&self, ident: &str) -> Option<u32> {
        self.ids.get(ident).copied()
    }

    /// Description references use the same finite text-alternative traversal.
    /// Describedby itself is never followed recursively. Call only after the
    /// native observer's aggregate potential-work preflight succeeds.
    pub(crate) fn description(&self, id: u32) -> Option<String> {
        let refs = self
            .tree
            .element(id)?
            .attributes
            .get(&Attribute::AriaDescribedby)?;
        let parts: Vec<_> = refs
            .split_ascii_whitespace()
            .filter_map(|ident| self.resolve_id(ident))
            .map(|referenced| {
                compute(
                    self,
                    referenced,
                    None,
                    Traversal {
                        root: id,
                        labelledby: false,
                        include_hidden: self.hidden_in_tree[&referenced],
                    },
                    Step::Referenced,
                )
            })
            .collect();
        let description = flat(&parts.join(" "));
        (!description.is_empty()).then_some(description)
    }

    /// Flat ASCII-whitespace-normalized name; no borrowed cache survives a frame.
    pub(crate) fn accessible_name(&self, id: u32, role: &str) -> String {
        self.name_with_origin(id, role).0
    }
    /// True only for the root's successful name-from-content fallback, not
    /// author labels, relations, host labels or placeholder fallback.
    pub(crate) fn name_with_origin(&self, id: u32, role: &str) -> (String, bool) {
        let (name, contents) = compute_with_origin(
            self,
            id,
            Some(role),
            Traversal {
                root: id,
                labelledby: false,
                include_hidden: false,
            },
            Step::Root,
        );
        (flat(&name), contents)
    }
}

/// Where the current node sits in the traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// The element whose name is computed.
    Root,
    /// The node directly referenced by `aria-labelledby`, `aria-describedby`
    /// or a native `label`.
    Referenced,
    /// A descendant reached through name from content.
    Descendant,
}

#[derive(Debug, Clone, Copy)]
struct Traversal {
    root: u32,
    /// Inside an `aria-labelledby` traversal: 2B is not applied again.
    labelledby: bool,
    /// The node that started the current relation was hidden, so hidden
    /// descendants take part (2A's exception).
    include_hidden: bool,
}

/// Whether the element itself is hidden under the L0 rules: `aria-hidden="true"`,
/// a computed `display: none`, or a `dialog` that is not open.
pub(crate) fn hidden_element(
    tree: &Tree,
    id: u32,
    element: &Element,
    environment: &Environment,
) -> bool {
    element
        .attributes
        .get(&Attribute::AriaHidden)
        .map(String::as_str)
        == Some("true")
        || tree
            .computed_property(id, environment, CssProperty::Display)
            .is_some_and(|display| display.eq_ignore_ascii_case("none"))
        || (element.tag == Tag::Dialog && !element.open)
}

fn compute_with_origin<'a>(
    names: &'a Names<'_>,
    id: u32,
    role: Option<&'a str>,
    traversal: Traversal,
    step: Step,
) -> (String, bool) {
    let mut root_contents = false;
    let tree = names.tree;
    enum Task<'a> {
        Visit(u32, Option<&'a str>, Step),
        Content {
            start: usize,
            recursion: bool,
            placeholder: Option<&'a str>,
        },
    }
    let mut pending = vec![Task::Visit(id, role, step)];
    let mut parts = Vec::<String>::new();
    while let Some(task) = pending.pop() {
        let Task::Visit(id, role, step) = task else {
            let Task::Content {
                start,
                recursion,
                placeholder,
            } = task
            else {
                unreachable!()
            };
            // Descendant content already occupies an ordered fragment range.
            // Do not concatenate that range again at every ancestor depth.
            if !recursion {
                root_contents = parts[start..]
                    .iter()
                    .any(|part| part.split_ascii_whitespace().next().is_some());
            }
            if !recursion && !root_contents {
                parts.truncate(start);
                parts.push(placeholder.unwrap_or_default().to_owned());
            }
            continue;
        };
        let Some(node) = tree.nodes.get(&id) else {
            parts.push(String::new());
            continue;
        };
        let element = match &node.content {
            // 2G: a text node contributes its text.
            Content::Text(text) => {
                if !traversal.include_hidden && names.invisible.contains(&id) {
                    parts.push(String::new());
                    continue;
                }
                parts.push(text.clone());
                continue;
            }
            Content::Element(element) => element,
        };
        if step == Step::Descendant && id == traversal.root {
            parts.push(String::new());
            continue;
        }
        // 2A: a relation starting at a hidden node includes its descendants.
        if !traversal.include_hidden && names.hidden(id) {
            parts.push(String::new());
            continue;
        }
        // Relation nesting remains bounded: labelledby is disabled in its
        // referenced traversal, and host labels apply only to the root step.
        if !traversal.labelledby
            && let Some(name) = labelledby(names, element, traversal)
        {
            parts.push(name);
            continue;
        }
        let recursion = step != Step::Root;
        if recursion && let Some(value) = embedded_value(names, id, element) {
            parts.push(value);
            continue;
        }
        if let Some(label) = element.attributes.get(&Attribute::AriaLabel)
            && !flat(label).is_empty()
        {
            parts.push(label.clone());
            continue;
        }
        if let Some(name) = host_label(names, element, step, traversal) {
            parts.push(name);
            continue;
        }
        let placeholder = element
            .text_field()
            .then(|| element.attributes.get(&Attribute::Placeholder))
            .flatten()
            .map(String::as_str);
        let role = role.or_else(|| crate::accessibility::role(element));
        if recursion || role.is_some_and(|role| NAME_FROM_CONTENT.contains(&role)) {
            pending.push(Task::Content {
                start: parts.len(),
                recursion,
                placeholder,
            });
            pending.extend(
                node.children
                    .iter()
                    .rev()
                    .map(|child| Task::Visit(*child, None, Step::Descendant)),
            );
        } else {
            parts.push(placeholder.unwrap_or_default().to_owned());
        }
    }
    (parts.concat(), root_contents)
}
fn compute<'a>(
    names: &'a Names<'_>,
    id: u32,
    role: Option<&'a str>,
    traversal: Traversal,
    step: Step,
) -> String {
    compute_with_origin(names, id, role, traversal, step).0
}

/// 2B: the names of the attached referenced nodes, joined by one space; `None`
/// when no reference resolves or the result is blank.
fn labelledby(names: &Names<'_>, element: &Element, traversal: Traversal) -> Option<String> {
    let ids = element.attributes.get(&Attribute::AriaLabelledby)?;
    let parts: Vec<String> = ids
        .split_ascii_whitespace()
        .filter_map(|ident| names.ids.get(ident).copied())
        .map(|referenced| {
            let nested = Traversal {
                root: traversal.root,
                labelledby: true,
                include_hidden: names.hidden_in_tree[&referenced],
            };
            compute(names, referenced, None, nested, Step::Referenced)
        })
        .collect();
    let joined = parts.join(" ");
    (!parts.is_empty() && !flat(&joined).is_empty()).then_some(joined)
}

/// 2C: the value of an embedded textbox, spinbutton or combobox (a password
/// field contributes its masked value, as the snapshot shows it).
fn embedded_value(names: &Names<'_>, id: u32, element: &Element) -> Option<String> {
    let tree = names.tree;
    if element.text_field() {
        return Some(if element.input_type() == "password" {
            let count = if names.mask_graphemes {
                element.value.graphemes(true).count()
            } else {
                element.value.chars().count()
            };
            crate::text::field_display::PASSWORD_MASK.repeat(count)
        } else {
            element.value.clone()
        });
    }
    if element.tag != Tag::Select {
        return None;
    }
    Some(
        tree.selected_option(id)
            .map_or_else(String::new, |option| tree.option_label(option)),
    )
}

/// 2E: `alt` of an image, or, for the root only, the labels of a labelable
/// element in document order, each computed as a traversal of its own. Inside
/// a recursion a field gave its value at 2C already, and skipping the `label`
/// relation of other controls there keeps every traversal finite.
fn host_label(
    names: &Names<'_>,
    element: &Element,
    step: Step,
    traversal: Traversal,
) -> Option<String> {
    if element.tag == Tag::Img {
        return element.attributes.get(&Attribute::Alt).cloned();
    }
    if step != Step::Root || !varta_common::view::accessibility::LABELABLE.contains(&element.tag) {
        return None;
    }
    let ident = element.attributes.get(&Attribute::Id)?;
    let parts: Vec<String> = names
        .labels
        .get(ident.as_str())
        .into_iter()
        .flatten()
        .copied()
        .map(|label| {
            let nested = Traversal {
                root: traversal.root,
                labelledby: traversal.labelledby,
                include_hidden: names.hidden_in_tree[&label],
            };
            compute(names, label, None, nested, Step::Referenced)
        })
        .collect();
    let joined = parts.join(" ");
    (!flat(&joined).is_empty()).then_some(joined)
}

/// accname's flat string: ASCII whitespace runs become one space, trimmed.
fn flat(text: &str) -> String {
    text.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}
