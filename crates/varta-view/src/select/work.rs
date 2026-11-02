//! Admission accounting precedes selection mutations and text derivation.
use crate::tree::{Content, Tree};
use crate::wire::{Operation, Value};
use varta_common::view::{Attribute, DRAFT_SELECT_NODE_WORK, Opcode, Tag};

impl Tree {
    pub(super) fn select_owner_work(&self, mut id: u32) -> (Option<u32>, usize) {
        let mut work = 0_usize;
        while let Some(node) = self.nodes.get(&id) {
            work = work.saturating_add(DRAFT_SELECT_NODE_WORK);
            if self
                .element(id)
                .is_some_and(|element| element.tag == Tag::Select)
            {
                return (Some(id), work);
            }
            let Some(parent) = node.parent else { break };
            id = parent;
        }
        (None, work)
    }

    fn option_work(&self, id: u32) -> usize {
        let Some(element) = self
            .element(id)
            .filter(|element| element.tag == Tag::Option)
        else {
            return 0;
        };
        let mut work = DRAFT_SELECT_NODE_WORK.saturating_add(
            element
                .attributes
                .get(&Attribute::Value)
                .map_or(0, String::len),
        );
        for child in &self.nodes[&id].children {
            if let Some(Content::Text(text)) = self.nodes.get(child).map(|node| &node.content) {
                work = work
                    .saturating_add(DRAFT_SELECT_NODE_WORK)
                    .saturating_add(text.len());
            }
        }
        work
    }

    fn select_work(&self, id: u32) -> usize {
        self.nodes.get(&id).map_or(0, |node| {
            node.children.iter().fold(0_usize, |work, &child| {
                work.saturating_add(self.option_work(child))
            })
        })
    }

    pub(crate) fn select_inventory_work(&self) -> usize {
        self.nodes.keys().fold(0_usize, |work, &id| {
            work.saturating_add(self.option_work(id))
        })
    }

    pub(crate) fn select_operation_work(&self, operation: &Operation) -> usize {
        let node = operation.id(0);
        let (first, second, incoming) = match operation.opcode {
            Opcode::SetText | Opcode::Remove => (Some(node), None, 0),
            Opcode::Insert => {
                let child = operation.id(1);
                let incoming = match self.nodes.get(&child).map(|node| &node.content) {
                    Some(Content::Text(text)) => DRAFT_SELECT_NODE_WORK.saturating_add(text.len()),
                    _ => self.option_work(child),
                };
                (Some(child), Some(node), incoming)
            }
            Opcode::SetAttribute | Opcode::RemoveAttribute => {
                let Some(element) = self
                    .element(node)
                    .filter(|element| element.tag == Tag::Option)
                else {
                    return 0;
                };
                let Some(Value::Attribute(attribute)) = operation.operands.get(1) else {
                    return 0;
                };
                if !matches!(
                    attribute,
                    Attribute::Value | Attribute::Selected | Attribute::Disabled
                ) {
                    return 0;
                }
                let current = element.attributes.get(attribute);
                let changed = if operation.opcode == Opcode::SetAttribute {
                    current.map(String::as_str) != Some(operation.text())
                } else {
                    current.is_some()
                };
                if !changed {
                    return 0;
                }
                (Some(node), None, 0)
            }
            Opcode::SetValue => {
                if !self
                    .element(node)
                    .is_some_and(|element| element.tag == Tag::Select)
                {
                    return 0;
                }
                (Some(node), None, 0)
            }
            _ => return 0,
        };
        let (first_owner, first_work) = first.map_or((None, 0), |id| self.select_owner_work(id));
        let (second_owner, second_work) = second.map_or((None, 0), |id| self.select_owner_work(id));
        let mut work = first_work
            .saturating_add(second_work)
            .saturating_add(incoming);
        if let Some(owner) = first_owner {
            work = work.saturating_add(self.select_work(owner));
        }
        if let Some(owner) = second_owner.filter(|owner| Some(*owner) != first_owner) {
            work = work.saturating_add(self.select_work(owner));
        }
        work.saturating_add(operation.text().len())
    }
}
