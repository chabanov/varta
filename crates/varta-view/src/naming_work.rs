//! Potential accessible-name work without evaluating names or allocating text.
use crate::tree::{Content, Tree};
use std::collections::BTreeMap;
use varta_common::view::{
    Attribute as A, DRAFT_FIELD_VALUE_RESERVATION_BYTES as FIELD, DRAFT_MAX_NAMING_WORK as LIMIT,
    DRAFT_NAMING_NODE_WORK as NODE, ROOT_NODE,
    accessibility::{LABELABLE, NAME_FROM_CONTENT},
};

fn add(left: usize, right: usize) -> usize {
    left.saturating_add(right).min(LIMIT + 1)
}

pub(crate) fn potential_work(tree: &Tree) -> usize {
    let mut order = Vec::with_capacity(tree.nodes.len());
    let mut pending: Vec<_> = tree
        .nodes
        .iter()
        .filter_map(|(id, n)| n.parent.is_none().then_some(*id))
        .collect();
    while let Some(id) = pending.pop() {
        order.push(id);
        pending.extend(tree.nodes[&id].children.iter().rev());
    }
    let mut own = BTreeMap::new();
    let mut ids = BTreeMap::new();
    let mut inventory = 0;
    for id in &order {
        let mut cost = NODE;
        match &tree.nodes[id].content {
            Content::Text(text) => cost = add(cost, text.len()),
            Content::Element(element) => {
                for attr in [
                    A::Id,
                    A::For,
                    A::AriaLabel,
                    A::AriaLabelledby,
                    A::Alt,
                    A::Placeholder,
                ] {
                    cost = add(cost, element.attributes.get(&attr).map_or(0, String::len));
                }
                if let Some(ident) = element.attributes.get(&A::Id) {
                    ids.insert(ident.as_str(), *id);
                }
            }
        }
        inventory = add(inventory, NODE);
        own.insert(*id, cost);
    }
    // Referenced traversals disable labelledby, so their summaries depend only
    // on children. All hidden/local-state/precedence alternatives remain live.
    let mut no_relations = BTreeMap::new();
    for id in order.iter().rev() {
        let node = &tree.nodes[id];
        let mut cost = own[id];
        if tree
            .element(*id)
            .is_some_and(|e| e.text_field() || e.tag == varta_common::view::Tag::Select)
        {
            cost = add(cost, FIELD);
        }
        for child in &node.children {
            cost = add(cost, no_relations[child]);
        }
        no_relations.insert(*id, cost);
    }
    let mut relations = BTreeMap::new();
    for id in &order {
        let mut cost = 0;
        if let Some(refs) = tree
            .element(*id)
            .and_then(|e| e.attributes.get(&A::AriaLabelledby))
        {
            for ident in refs.split_ascii_whitespace() {
                cost = add(cost, NODE + 1);
                if let Some(target) = ids.get(ident) {
                    cost = add(cost, no_relations[target]);
                }
            }
        }
        relations.insert(*id, cost);
    }
    let mut descendants = BTreeMap::new();
    for id in order.iter().rev() {
        let mut cost = add(own[id], relations[id]);
        if tree
            .element(*id)
            .is_some_and(|e| e.text_field() || e.tag == varta_common::view::Tag::Select)
        {
            cost = add(cost, FIELD);
        }
        for child in &tree.nodes[id].children {
            cost = add(cost, descendants[child]);
        }
        descendants.insert(*id, cost);
    }
    // Aggregate label alternatives once, even if many roots query an absent id.
    let mut labels = BTreeMap::new();
    for id in &order {
        if let Some(e) = tree.element(*id)
            && e.tag == varta_common::view::Tag::Label
            && let Some(ident) = e.attributes.get(&A::For)
        {
            let cost = labels.entry(ident.as_str()).or_insert(0);
            *cost = add(*cost, add(NODE + 1, descendants[id]));
        }
    }
    for id in &order {
        let Some(element) = tree.element(*id) else {
            continue;
        };
        let Some(role) = crate::snapshot::role(element) else {
            continue;
        };
        if *id == ROOT_NODE {
            continue;
        }
        let mut cost = add(own[id], relations[id]);
        if NAME_FROM_CONTENT.contains(&role) {
            for child in &tree.nodes[id].children {
                cost = add(cost, descendants[child]);
            }
        }
        if LABELABLE.contains(&element.tag)
            && let Some(ident) = element.attributes.get(&A::Id)
        {
            cost = add(cost, labels.get(ident.as_str()).copied().unwrap_or(0));
        }
        inventory = add(inventory, cost);
        if inventory > LIMIT {
            break;
        }
    }
    inventory
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Access, BatchWriter as B, Broker, View};
    use varta_common::view::Tag;
    struct Denied;
    impl Broker for Denied {
        fn link_requested(&self, _: &str) -> bool {
            false
        }
        fn open_url(&mut self, _: &str) -> Access {
            Access::Denied
        }
        fn clipboard_write(&mut self) -> Access {
            Access::Denied
        }
    }
    #[test]
    fn potential_work_bounds_computed_element_names_across_144_relation_worlds() {
        for role in ["cell", "list", "status"] {
            for kind in ["text", "password", "checkbox"] {
                for refs in ["a", "b a", "missing", "a a"] {
                    for hidden in ["true", "false"] {
                        for text in ["", " é😀 \n"] {
                            let mut b = B::new();
                            b.create_element(2, Tag::Button)
                                .set_attribute(2, A::Role, role)
                                .set_attribute(2, A::AriaLabelledby, refs)
                                .set_attribute(2, A::AriaLabel, "fallback")
                                .insert(1, 2, 0)
                                .create_element(3, Tag::Input)
                                .set_attribute(3, A::Type, kind)
                                .set_attribute(3, A::Id, "field")
                                .create_element(4, Tag::Label)
                                .set_attribute(4, A::For, "field")
                                .set_attribute(4, A::AriaLabelledby, refs)
                                .insert(4, 3, 0)
                                .insert(1, 4, 0)
                                .create_element(5, Tag::Div)
                                .set_attribute(5, A::Id, "a")
                                .set_attribute(5, A::AriaLabelledby, "b")
                                .set_attribute(5, A::AriaHidden, hidden)
                                .insert(1, 5, 0)
                                .create_element(6, Tag::Div)
                                .set_attribute(6, A::Id, "b")
                                .set_attribute(6, A::AriaLabelledby, "a")
                                .insert(1, 6, 0)
                                .create_text(7, text)
                                .insert(5, 7, 0)
                                .create_text(8, " child ")
                                .insert(2, 8, 0);
                            if kind != "checkbox" {
                                b.set_value(3, " é😀 ", 0);
                            }
                            let mut view = View::default();
                            view.commit(&b.finish(), false, &mut Denied).unwrap();
                            let snapshot = view.snapshot();
                            let mut materialized = 0;
                            let mut pending = vec![&snapshot["accessibility"]];
                            while let Some(node) = pending.pop() {
                                if node["role"] != "text" {
                                    materialized += node["name"].as_str().map_or(0, str::len);
                                }
                                if let Some(children) = node["children"].as_array() {
                                    pending.extend(children);
                                }
                            }
                            assert!(
                                materialized <= potential_work(&view.tree),
                                "{role}/{kind}/{refs}/{hidden}/{text:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
