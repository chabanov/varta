//! Accessible names: the steps of accname 1.2 that the L0 v1 subset admits
//! (architecture.md, "Accessible names"). The subset has no `title`
//! attribute, no `role="presentation"`/`none`, no generated content and no
//! layout, so steps 2I (tooltip) and the CSS parts of 2F do not apply.
use crate::style::Environment;
use crate::tree::{Content, Element, Tree};
use varta_common::view::{Attribute, CssProperty, Tag};

/// Roles of the subset whose name may come from their content (ARIA 1.2
/// "Name From: contents"). Every other role is named by its author only.
const NAME_FROM_CONTENT: &[&str] = &[
    "button",
    "link",
    "heading",
    "option",
    "cell",
    "columnheader",
    "row",
    "tab",
    "checkbox",
    "radio",
];

/// Where the current node sits in the traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// The element whose name is computed.
    Root,
    /// The node directly referenced by `aria-labelledby` or a native `label`.
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

/// The accessible name of `id` with role `role`, as a flat string (ASCII
/// whitespace runs collapsed to one space, trimmed).
pub(crate) fn accessible_name(
    tree: &Tree,
    id: u32,
    role: &str,
    environment: &Environment,
) -> String {
    let traversal = Traversal {
        root: id,
        labelledby: false,
        include_hidden: false,
    };
    flat(&compute(
        tree,
        id,
        Some(role),
        traversal,
        Step::Root,
        environment,
    ))
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
            .computed(id, environment)
            .get(&CssProperty::Display)
            .is_some_and(|display| display == "none")
        || (element.tag == Tag::Dialog && !element.open)
}

/// Whether the node or any ancestor is hidden.
fn hidden_in_tree(tree: &Tree, id: u32, environment: &Environment) -> bool {
    tree.path(id).into_iter().any(|node| {
        tree.element(node)
            .is_some_and(|element| hidden_element(tree, node, element, environment))
    })
}

fn compute(
    tree: &Tree,
    id: u32,
    role: Option<&str>,
    traversal: Traversal,
    step: Step,
    environment: &Environment,
) -> String {
    let Some(node) = tree.nodes.get(&id) else {
        return String::new();
    };
    let element = match &node.content {
        // 2G: a text node contributes its text.
        Content::Text(text) => return text.clone(),
        Content::Element(element) => element,
    };
    // The root met again inside its own label or content is not "another
    // widget" and contributes nothing (this also ends every cycle).
    if step == Step::Descendant && id == traversal.root {
        return String::new();
    }
    // 2A: hidden nodes are skipped unless the relation started at a hidden node.
    if !traversal.include_hidden && hidden_element(tree, id, element, environment) {
        return String::new();
    }
    // 2B: aria-labelledby, once per traversal.
    if !traversal.labelledby
        && let Some(name) = labelledby(tree, element, traversal, environment)
    {
        return name;
    }
    let recursion = step != Step::Root;
    // 2C: a control embedded in the label of another widget gives its value.
    if recursion && let Some(value) = embedded_value(tree, id, element) {
        return value;
    }
    // 2D: a nonblank aria-label.
    if let Some(label) = element.attributes.get(&Attribute::AriaLabel)
        && !flat(label).is_empty()
    {
        return label.clone();
    }
    // 2E: the host language label (`alt`; `label for` only for the root).
    if let Some(name) = host_label(tree, element, step, traversal, environment) {
        return name;
    }
    // 2F (root with a name-from-content role) and 2H (every recursion).
    let role = role.or_else(|| crate::snapshot::role(element));
    if recursion || role.is_some_and(|role| NAME_FROM_CONTENT.contains(&role)) {
        let content: String = node
            .children
            .iter()
            .map(|child| compute(tree, *child, None, traversal, Step::Descendant, environment))
            .collect();
        if recursion || !flat(&content).is_empty() {
            return content;
        }
    }
    // HTML-AAM's last resort for text fields (after the absent `title`).
    if element.text_field()
        && let Some(placeholder) = element.attributes.get(&Attribute::Placeholder)
    {
        return placeholder.clone();
    }
    String::new()
}

/// 2B: the names of the attached referenced nodes, joined by one space; `None`
/// when no reference resolves or the result is blank.
fn labelledby(
    tree: &Tree,
    element: &Element,
    traversal: Traversal,
    environment: &Environment,
) -> Option<String> {
    let ids = element.attributes.get(&Attribute::AriaLabelledby)?;
    let parts: Vec<String> = ids
        .split_ascii_whitespace()
        .filter_map(|ident| tree.by_ident(ident))
        .filter(|referenced| tree.attached(*referenced))
        .map(|referenced| {
            let nested = Traversal {
                root: traversal.root,
                labelledby: true,
                include_hidden: hidden_in_tree(tree, referenced, environment),
            };
            compute(
                tree,
                referenced,
                None,
                nested,
                Step::Referenced,
                environment,
            )
        })
        .collect();
    let joined = parts.join(" ");
    (!parts.is_empty() && !flat(&joined).is_empty()).then_some(joined)
}

/// 2C: the value of an embedded textbox, spinbutton or combobox (a password
/// field contributes its masked value, as the snapshot shows it).
fn embedded_value(tree: &Tree, id: u32, element: &Element) -> Option<String> {
    if element.text_field() {
        return Some(if element.input_type() == "password" {
            "•".repeat(element.value.chars().count())
        } else {
            element.value.clone()
        });
    }
    if element.tag != Tag::Select {
        return None;
    }
    let chosen = tree.nodes.get(&id)?.children.iter().find(|option| {
        tree.element(**option)
            .and_then(|option| option.attributes.get(&Attribute::Value))
            == Some(&element.value)
    });
    Some(chosen.map_or_else(String::new, |option| tree.text_content(*option)))
}

/// 2E: `alt` of an image, or, for the root only, the labels of a labelable
/// element in document order, each computed as a traversal of its own. Inside
/// a recursion a field gave its value at 2C already, and skipping the `label`
/// relation of other controls there keeps every traversal finite.
fn host_label(
    tree: &Tree,
    element: &Element,
    step: Step,
    traversal: Traversal,
    environment: &Environment,
) -> Option<String> {
    if element.tag == Tag::Img {
        return element.attributes.get(&Attribute::Alt).cloned();
    }
    if step != Step::Root
        || !matches!(
            element.tag,
            Tag::Input | Tag::Textarea | Tag::Select | Tag::Progress
        )
    {
        return None;
    }
    let ident = element.attributes.get(&Attribute::Id)?;
    let parts: Vec<String> = tree
        .document_order()
        .into_iter()
        .filter(|label| {
            tree.element(*label).is_some_and(|candidate| {
                candidate.tag == Tag::Label
                    && candidate.attributes.get(&Attribute::For) == Some(ident)
            })
        })
        .map(|label| {
            let nested = Traversal {
                root: traversal.root,
                labelledby: traversal.labelledby,
                include_hidden: hidden_in_tree(tree, label, environment),
            };
            compute(tree, label, None, nested, Step::Referenced, environment)
        })
        .collect();
    let joined = parts.join(" ");
    (!flat(&joined).is_empty()).then_some(joined)
}

/// accname's flat string: ASCII whitespace runs become one space, trimmed.
fn flat(text: &str) -> String {
    text.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}
