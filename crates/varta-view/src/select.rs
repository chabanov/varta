//! Single-select state and transient native popup editing, independent of clocks/I/O.
use crate::tree::Tree;
use crate::{EventRecord, InputError, View};
use varta_common::view::{Attribute, EventKind, Key, Tag};

pub(crate) const POPUP_ROWS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SelectPopup {
    pub active: Option<u32>,
    pub first: usize,
    pub manual_scroll: bool,
}

impl Tree {
    pub(crate) fn select_owner(&self, mut id: u32) -> Option<u32> {
        loop {
            if self
                .element(id)
                .is_some_and(|element| element.tag == Tag::Select)
            {
                return Some(id);
            }
            id = self.nodes.get(&id)?.parent?;
        }
    }
    pub(crate) fn select_options(&self, id: u32) -> Vec<u32> {
        self.nodes.get(&id).map_or_else(Vec::new, |node| {
            node.children
                .iter()
                .copied()
                .filter(|id| {
                    self.element(*id)
                        .is_some_and(|element| element.tag == Tag::Option)
                })
                .collect()
        })
    }
    pub(crate) fn option_label(&self, option: u32) -> String {
        self.text_content(option)
            .split_ascii_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub(crate) fn option_value(&self, option: u32) -> String {
        self.element(option)
            .and_then(|element| element.attributes.get(&Attribute::Value))
            .cloned()
            .unwrap_or_else(|| self.option_label(option))
    }
    pub(crate) fn selected_option(&self, id: u32) -> Option<u32> {
        self.select_options(id).into_iter().find(|option| {
            self.element(*option)
                .is_some_and(|element| element.editor.option_selected)
        })
    }
    pub(crate) fn sync_select_for(&mut self, id: u32) {
        if let Some(select) = self.select_owner(id) {
            self.sync_select_value(select);
        }
    }
    pub(crate) fn sync_select_value(&mut self, id: u32) {
        let selected = self.selected_option(id);
        let value = selected.map_or_else(String::new, |option| self.option_value(option));
        if let Some(element) = self
            .element_mut(id)
            .filter(|element| element.tag == Tag::Select)
        {
            element.value = value;
            element.editor.select_popup = None;
        }
    }
    pub(crate) fn normalize_select(&mut self, id: u32, preferred: Option<u32>) {
        let options = self.select_options(id);
        let selected = preferred
            .filter(|preferred| options.contains(preferred))
            .or_else(|| {
                options.iter().rev().copied().find(|option| {
                    self.element(*option)
                        .is_some_and(|element| element.editor.option_selected)
                })
            })
            .or_else(|| {
                options.iter().copied().find(|option| {
                    self.element(*option)
                        .is_some_and(|element| !element.disabled())
                })
            });
        for option in options {
            self.element_mut(option)
                .expect("option")
                .editor
                .option_selected = Some(option) == selected;
        }
        self.sync_select_value(id);
    }
    pub(crate) fn select_attribute(&mut self, id: u32, attribute: Attribute, present: bool) {
        let Some(element) = self.element_mut(id) else {
            return;
        };
        if element.tag != Tag::Option {
            return;
        }
        if attribute == Attribute::Selected && !element.editor.option_dirty {
            element.editor.option_selected = present;
            if let Some(select) = self.select_owner(id) {
                self.normalize_select(select, present.then_some(id));
            }
        } else if matches!(attribute, Attribute::Value | Attribute::Disabled) {
            self.sync_select_for(id);
        }
    }
    pub(crate) fn set_select_value(&mut self, id: u32, value: &str) {
        let selected = self
            .select_options(id)
            .into_iter()
            .find(|option| self.option_value(*option) == value);
        self.set_select_option(id, selected);
    }
    pub(crate) fn set_select_option(&mut self, id: u32, selected: Option<u32>) {
        for option in self.select_options(id) {
            let editor = &mut self.element_mut(option).expect("option").editor;
            editor.option_selected = Some(option) == selected;
            if editor.option_selected {
                editor.option_dirty = true;
            }
        }
        self.sync_select_value(id);
    }
}

impl View {
    pub(crate) fn select_text(&mut self, id: u32, text: &str) -> Result<bool, InputError> {
        if self
            .tree
            .element(id)
            .is_none_or(|element| element.tag != Tag::Select || element.disabled())
        {
            return Ok(false);
        }
        if text.is_empty() {
            return Ok(true);
        }
        let options: Vec<_> = self
            .tree
            .select_options(id)
            .into_iter()
            .filter(|option| {
                self.tree
                    .element(*option)
                    .is_some_and(|element| !element.disabled())
                    && self
                        .tree
                        .computed(*option, &self.environment)
                        .get(&varta_common::view::CssProperty::Display)
                        .is_none_or(|value| value != "none")
            })
            .collect();
        let popup = self
            .tree
            .element(id)
            .and_then(|element| element.editor.select_popup.as_ref());
        let active = popup
            .and_then(|popup| popup.active)
            .or_else(|| self.tree.selected_option(id));
        let start = active
            .and_then(|active| options.iter().position(|option| *option == active))
            .map_or(0, |index| index + 1);
        let prefix = text.to_lowercase();
        let chosen = options
            .iter()
            .cycle()
            .skip(start)
            .take(options.len())
            .copied()
            .find(|option| {
                self.tree
                    .option_label(*option)
                    .to_lowercase()
                    .starts_with(&prefix)
            });
        if let Some(option) = chosen {
            if let Some(popup) = self
                .tree
                .element_mut(id)
                .expect("select")
                .editor
                .select_popup
                .as_mut()
            {
                popup.active = Some(option);
                popup.manual_scroll = false;
            } else {
                self.choose_option(id, option)?;
            }
        }
        Ok(true)
    }
    pub(crate) fn choose_option(&mut self, id: u32, option: u32) -> Result<(), InputError> {
        if !self
            .tree
            .element(id)
            .is_some_and(|element| element.tag == Tag::Select)
            || !self.tree.select_options(id).contains(&option)
            || self
                .tree
                .element(option)
                .is_none_or(|element| element.disabled())
        {
            return Err(InputError::Choice);
        }
        if self
            .tree
            .option_value(option)
            .encode_utf16()
            .take(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
            .count()
            > varta_common::view::DRAFT_MAX_VALUE_BYTES / 2
        {
            return Err(InputError::Choice);
        }
        if self
            .tree
            .element(id)
            .is_none_or(|element| element.disabled())
        {
            return Ok(());
        }
        self.focus_input(id)?;
        self.close_select(id);
        if self.tree.selected_option(id) == Some(option) {
            return Ok(());
        }
        self.tree.set_select_option(id, Some(option));
        let element = self.tree.element_mut(id).expect("validated select");
        element.seq = element.seq.wrapping_add(1);
        for kind in [EventKind::Input, EventKind::Change] {
            let mut record = EventRecord::occurrence(kind, id);
            record.seq = self.tree.element(id).expect("select").seq;
            record.value = self.tree.element(id).expect("select").value.clone();
            self.dispatch(record, true)?;
        }
        Ok(())
    }
    pub(crate) fn close_select(&mut self, id: u32) {
        if let Some(element) = self.tree.element_mut(id) {
            element.editor.select_popup = None;
        }
    }
    pub(crate) fn open_select(&mut self, id: u32) {
        if self
            .tree
            .element(id)
            .is_none_or(|element| element.tag != Tag::Select || element.disabled())
        {
            return;
        }
        if !self.tree.select_options(id).into_iter().any(|option| {
            self.tree
                .computed(option, &self.environment)
                .get(&varta_common::view::CssProperty::Display)
                .is_none_or(|value| value != "none")
        }) {
            return;
        }
        let active = self
            .tree
            .selected_option(id)
            .filter(|option| {
                self.tree
                    .element(*option)
                    .is_some_and(|element| !element.disabled())
            })
            .or_else(|| {
                self.tree.select_options(id).into_iter().find(|option| {
                    self.tree
                        .element(*option)
                        .is_some_and(|element| !element.disabled())
                })
            });
        self.tree
            .element_mut(id)
            .expect("select")
            .editor
            .select_popup = Some(SelectPopup {
            active,
            first: 0,
            manual_scroll: false,
        });
    }
    pub(crate) fn select_key(&mut self, id: u32, key: Key) -> Result<bool, InputError> {
        if self
            .tree
            .element(id)
            .is_none_or(|element| element.tag != Tag::Select || element.disabled())
        {
            return Ok(false);
        }
        let popup = self
            .tree
            .element(id)
            .and_then(|element| element.editor.select_popup.clone());
        if key == Key::Escape && popup.is_some() {
            self.close_select(id);
            return Ok(true);
        }
        if matches!(key, Key::Enter | Key::Space) {
            if let Some(popup) = popup {
                if let Some(option) = popup.active.filter(|option| {
                    self.tree.select_options(id).contains(option)
                        && self
                            .tree
                            .element(*option)
                            .is_some_and(|element| !element.disabled())
                        && self
                            .tree
                            .computed(*option, &self.environment)
                            .get(&varta_common::view::CssProperty::Display)
                            .is_none_or(|value| value != "none")
                }) {
                    self.choose_option(id, option)?;
                } else {
                    self.close_select(id);
                }
            } else {
                self.open_select(id);
            }
            return Ok(true);
        }
        if key == Key::Tab {
            self.close_select(id);
            return Ok(false);
        }
        if !matches!(
            key,
            Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::PageUp | Key::PageDown
        ) {
            return Ok(false);
        }
        let options: Vec<_> = self
            .tree
            .select_options(id)
            .into_iter()
            .filter(|option| {
                self.tree
                    .element(*option)
                    .is_some_and(|element| !element.disabled())
                    && self
                        .tree
                        .computed(*option, &self.environment)
                        .get(&varta_common::view::CssProperty::Display)
                        .is_none_or(|value| value != "none")
            })
            .collect();
        if options.is_empty() {
            return Ok(true);
        }
        let active = popup
            .as_ref()
            .and_then(|popup| popup.active)
            .or_else(|| self.tree.selected_option(id));
        let at = active.and_then(|active| options.iter().position(|option| *option == active));
        let last = options.len() - 1;
        let index = match key {
            Key::Home => 0,
            Key::End => last,
            Key::ArrowUp => at.unwrap_or(1).saturating_sub(1),
            Key::ArrowDown => at.map_or(0, |at| (at + 1).min(last)),
            Key::PageUp => at.unwrap_or(POPUP_ROWS).saturating_sub(POPUP_ROWS),
            Key::PageDown => at.map_or(0, |at| (at + POPUP_ROWS).min(last)),
            _ => return Ok(false),
        };
        if popup.is_some() {
            let popup = self
                .tree
                .element_mut(id)
                .expect("select")
                .editor
                .select_popup
                .as_mut()
                .expect("popup");
            popup.active = Some(options[index]);
            popup.manual_scroll = false;
        } else {
            self.choose_option(id, options[index])?;
        }
        Ok(true)
    }
}
