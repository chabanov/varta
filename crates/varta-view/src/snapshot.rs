//! Deterministic headless output: the node tree and the accessibility tree
//! as JSON, the basis of conformance snapshots.
use crate::events::Effects;
use crate::style::Environment;
use crate::tree::{Content, Element, Tree};
use serde_json::{Map, Value, json};
use varta_common::view::{Attribute, CssProperty, ROOT_NODE, Tag};

pub(crate) fn snapshot(tree: &Tree, environment: &Environment, platform: &Effects) -> Value {
    json!({
        "title": tree.title,
        "focus": tree.focus,
        "tree": node(tree, ROOT_NODE, environment),
        "accessibility": accessibility_root(tree, environment),
        "stylesheets": tree.sheets.iter().map(|(id, sheet)| json!({"id": id, "text": sheet.text})).collect::<Vec<_>>(),
        "clipboard": platform.clipboard,
        "opened": platform.opened,
        "prompts": platform.prompts,
    })
}

fn node(tree: &Tree, id: u32, environment: &Environment) -> Value {
    let Some(entry) = tree.nodes.get(&id) else {
        return Value::Null;
    };
    match &entry.content {
        Content::Text(text) => json!({ "id": id, "text": text }),
        Content::Element(element) => {
            let mut out = Map::new();
            out.insert("id".into(), json!(id));
            out.insert(
                "tag".into(),
                json!(if id == ROOT_NODE {
                    "root"
                } else {
                    element.tag.name()
                }),
            );
            if !element.attributes.is_empty() {
                out.insert(
                    "attributes".into(),
                    Value::Object(
                        element
                            .attributes
                            .iter()
                            .map(|(attribute, value)| (attribute.name().into(), json!(value)))
                            .collect(),
                    ),
                );
            }
            if !element.class.is_empty() {
                out.insert("class".into(), json!(element.class));
            }
            let computed = tree.computed(id, environment);
            if !computed.is_empty() {
                out.insert(
                    "style".into(),
                    Value::Object(
                        computed
                            .iter()
                            .map(|(property, value)| (property.name().into(), json!(value)))
                            .collect(),
                    ),
                );
            }
            if element.text_field() || element.tag == Tag::Select {
                out.insert("value".into(), json!(element.value));
                out.insert("seq".into(), json!(element.seq));
            }
            if element.checkable() {
                out.insert("checked".into(), json!(element.checked));
                out.insert("seq".into(), json!(element.seq));
            }
            if element.tag == Tag::Dialog {
                out.insert("open".into(), json!(element.open));
            }
            if element.scroll_top != 0.0 {
                out.insert("scrollTop".into(), json!(element.scroll_top));
            }
            if !element.listeners.is_empty() {
                out.insert(
                    "listeners".into(),
                    Value::Array(
                        element
                            .listeners
                            .iter()
                            .map(|listener| json!(listener.kind.name()))
                            .collect(),
                    ),
                );
            }
            if !entry.children.is_empty() {
                out.insert(
                    "children".into(),
                    Value::Array(
                        entry
                            .children
                            .iter()
                            .map(|child| node(tree, *child, environment))
                            .collect(),
                    ),
                );
            }
            Value::Object(out)
        }
    }
}

/// The implicit role of an element, before a `role` attribute overrides it;
/// `None` for generic containers, which are flattened into their parent.
fn implicit_role(element: &Element) -> Option<&'static str> {
    Some(match element.tag {
        Tag::Button => "button",
        Tag::A => "link",
        Tag::H1 | Tag::H2 | Tag::H3 => "heading",
        Tag::Ul | Tag::Ol => "list",
        Tag::Li => "listitem",
        Tag::Nav => "navigation",
        Tag::Main => "main",
        Tag::Header => "banner",
        Tag::Footer => "contentinfo",
        Tag::Aside => "complementary",
        Tag::Form => "form",
        Tag::P => "paragraph",
        Tag::Dialog => "dialog",
        Tag::Progress => "progressbar",
        Tag::Img => "image",
        Tag::Textarea => "textbox",
        Tag::Select => "combobox",
        Tag::Option => "option",
        Tag::Input => match element.input_type() {
            "checkbox" => "checkbox",
            "radio" => "radio",
            "search" => "searchbox",
            "number" => "spinbutton",
            _ => "textbox",
        },
        Tag::Section if element.attributes.contains_key(&Attribute::AriaLabel) => "region",
        _ => return None,
    })
}

fn hidden(tree: &Tree, id: u32, element: &Element, environment: &Environment) -> bool {
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

fn name(tree: &Tree, id: u32, element: &Element) -> Option<String> {
    if let Some(label) = element.attributes.get(&Attribute::AriaLabel) {
        return Some(label.clone());
    }
    if let Some(ids) = element.attributes.get(&Attribute::AriaLabelledby) {
        let text: Vec<String> = ids
            .split(' ')
            .filter_map(|ident| tree.by_ident(ident))
            .map(|labelled| tree.text_content(labelled))
            .collect();
        return Some(text.join(" "));
    }
    if element.tag == Tag::Img {
        return element.attributes.get(&Attribute::Alt).cloned();
    }
    if matches!(
        element.tag,
        Tag::Input | Tag::Textarea | Tag::Select | Tag::Progress
    ) {
        let ident = element.attributes.get(&Attribute::Id)?;
        let label = tree
            .nodes
            .iter()
            .find_map(|(label, node)| match &node.content {
                Content::Element(candidate)
                    if candidate.tag == Tag::Label
                        && candidate.attributes.get(&Attribute::For) == Some(ident) =>
                {
                    Some(*label)
                }
                _ => None,
            })?;
        return Some(tree.text_content(label));
    }
    if matches!(
        element.tag,
        Tag::Button | Tag::A | Tag::H1 | Tag::H2 | Tag::H3 | Tag::Option | Tag::Li
    ) {
        return Some(tree.text_content(id));
    }
    None
}

fn accessibility_root(tree: &Tree, environment: &Environment) -> Value {
    json!({
        "role": "application",
        "children": children(tree, ROOT_NODE, environment),
    })
}

fn children(tree: &Tree, id: u32, environment: &Environment) -> Vec<Value> {
    let Some(entry) = tree.nodes.get(&id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in &entry.children {
        out.extend(accessible(tree, *child, environment));
    }
    out
}

/// The accessible nodes of one subtree: a node with a role, or the flattened
/// children of a generic container, or a text leaf.
fn accessible(tree: &Tree, id: u32, environment: &Environment) -> Vec<Value> {
    let Some(entry) = tree.nodes.get(&id) else {
        return Vec::new();
    };
    let element = match &entry.content {
        Content::Text(text) if text.trim().is_empty() => return Vec::new(),
        Content::Text(text) => return vec![json!({ "role": "text", "name": text })],
        Content::Element(element) => element,
    };
    if hidden(tree, id, element, environment) {
        return Vec::new();
    }
    let role = element
        .attributes
        .get(&Attribute::Role)
        .map(String::as_str)
        .or_else(|| implicit_role(element));
    let Some(role) = role else {
        return children(tree, id, environment);
    };
    let mut out = Map::new();
    out.insert("role".into(), json!(role));
    if let Some(name) = name(tree, id, element) {
        out.insert("name".into(), json!(name));
    }
    if let Some(level) = match element.tag {
        Tag::H1 => Some(1),
        Tag::H2 => Some(2),
        Tag::H3 => Some(3),
        _ => None,
    } {
        out.insert("level".into(), json!(level));
    }
    if element.text_field() || element.tag == Tag::Select {
        let masked = element.input_type() == "password";
        out.insert(
            "value".into(),
            json!(if masked {
                "•".repeat(element.value.chars().count())
            } else {
                element.value.clone()
            }),
        );
    }
    if element.tag == Tag::Textarea {
        out.insert("multiline".into(), json!(true));
    }
    if element.checkable() {
        out.insert("checked".into(), json!(element.checked));
    }
    if element.disabled() {
        out.insert("disabled".into(), json!(true));
    }
    if tree.focus == Some(id) {
        out.insert("focused".into(), json!(true));
    }
    if element.tag == Tag::Dialog && element.open {
        out.insert("modal".into(), json!(true));
    }
    for (attribute, key) in [
        (Attribute::AriaExpanded, "expanded"),
        (Attribute::AriaSelected, "selected"),
        (Attribute::AriaBusy, "busy"),
        (Attribute::AriaInvalid, "invalid"),
        (Attribute::AriaCurrent, "current"),
        (Attribute::AriaLive, "live"),
        (Attribute::Href, "url"),
    ] {
        if let Some(value) = element.attributes.get(&attribute) {
            out.insert(key.into(), json!(value));
        }
    }
    let leaf_named = matches!(
        element.tag,
        Tag::Button | Tag::A | Tag::Option | Tag::H1 | Tag::H2 | Tag::H3
    );
    let nested = children(tree, id, environment);
    if !nested.is_empty() && !(leaf_named && nested.iter().all(|n| n["role"] == "text")) {
        out.insert("children".into(), Value::Array(nested));
    }
    vec![Value::Object(out)]
}
