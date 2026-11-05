//! Deterministic headless output: the node tree and the accessibility tree
//! as JSON, the basis of conformance snapshots.
use crate::events::Effects;
use crate::style::Environment;
use crate::tree::{Content, Tree};
use serde_json::{Map, Value, json};
use varta_common::view::{ROOT_NODE, Tag};

pub(crate) fn snapshot(tree: &Tree, environment: &Environment, platform: &Effects) -> Value {
    let mut out = json!({
        "title": tree.title,
        "focus": tree.focus,
        "stylesheets": tree.sheets.iter().map(|(id, sheet)| json!({"id": id, "text": sheet.text})).collect::<Vec<_>>(),
        "clipboard": platform.clipboard,
        "opened": platform.opened,
        "prompts": platform.prompts,
    });
    if platform.opened_dropped != 0 {
        out["openedDropped"] = json!(platform.opened_dropped);
    }
    // Insert already-built values without serde recursively reserializing them.
    out["tree"] = node_tree(tree, environment);
    out["accessibility"] = crate::accessibility::project(tree, environment).snapshot();
    out
}

/// Project explicit cascaded `display` values through flex item blockification.
/// This is deliberately separate from the established cascade snapshot: it is
/// one layout rule, not a complete browser computed-style implementation. The
/// opt-in geometry section reports the used display of every laid-out element
/// instead, without rewriting the tree ([`crate::View::snapshot_with_geometry`]).
pub fn project_flex_blockification(snapshot: &mut Value) {
    let Some(tree) = snapshot.get_mut("tree") else {
        return;
    };
    let mut pending = vec![(tree, false)];
    while let Some((node, parent_flex)) = pending.pop() {
        let Some(object) = node.as_object_mut() else {
            continue;
        };
        let style = object.get_mut("style").and_then(Value::as_object_mut);
        let mut own_flex = false;
        if let Some(style) = style
            && let Some(display) = style.get_mut("display")
        {
            if parent_flex
                && display
                    .as_str()
                    .is_some_and(|v| v.eq_ignore_ascii_case("inline"))
            {
                *display = Value::String("block".into());
            }
            own_flex = display
                .as_str()
                .is_some_and(|v| v.eq_ignore_ascii_case("flex"));
        }
        if let Some(children) = object.get_mut("children").and_then(Value::as_array_mut) {
            for child in children {
                pending.push((child, own_flex));
            }
        }
    }
}

fn node_tree(tree: &Tree, environment: &Environment) -> Value {
    let mut built = std::collections::BTreeMap::new();
    for id in tree.document_order().into_iter().rev() {
        let children = tree.nodes[&id]
            .children
            .iter()
            .filter_map(|child| built.remove(child))
            .collect();
        built.insert(id, node(tree, id, environment, children));
    }
    built.remove(&ROOT_NODE).unwrap_or(Value::Null)
}

fn node(tree: &Tree, id: u32, environment: &Environment, children: Vec<Value>) -> Value {
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
            if element.tag == Tag::Option {
                out.insert("selected".into(), json!(element.editor.option_selected));
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
                out.insert("children".into(), Value::Array(children));
            }
            Value::Object(out)
        }
    }
}

#[cfg(test)]
mod flex_tests {
    use super::*;
    #[test]
    fn flex_projection_only_blockifies_explicit_inline_children() {
        let mut snapshot = json!({"tree":{"tag":"root","children":[
            {"tag":"div","style":{"display":"flex"},"children":[
                {"tag":"span","style":{"display":"inline","color":"red"}},
                {"tag":"span","style":{"display":"none"}},
                {"tag":"span"},
                {"tag":"div","style":{"display":"block"},"children":[{"tag":"span","style":{"display":"inline"}}]}
            ]}
        ]},"accessibility":{"unchanged":true}});
        project_flex_blockification(&mut snapshot);
        let children = &snapshot["tree"]["children"][0]["children"];
        assert_eq!(children[0]["style"]["display"], "block");
        assert_eq!(children[0]["style"]["color"], "red");
        assert_eq!(children[1]["style"]["display"], "none");
        assert!(children[2].get("style").is_none());
        assert_eq!(children[3]["children"][0]["style"]["display"], "inline");
        assert_eq!(snapshot["accessibility"]["unchanged"], true);
    }
}
