//! The live node tree and the semantic rules of every operation. A batch is
//! applied to a copy of the tree; only a batch whose every operation and
//! final invariant holds replaces the live tree (fail closed, atomic).
use crate::style::{self, Environment, Name, Rule, Subject};
use crate::wire::{Operation, Value};
use crate::{ViewError, Violation};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use varta_common::view::attributes as values;
use varta_common::view::{
    ANY_KEY, Attribute, AttributeValue, Command, CssProperty, DRAFT_MAX_LIVE_LISTENERS,
    DRAFT_MAX_LIVE_NODES, DRAFT_MAX_PENDING_COMMANDS, DRAFT_MAX_STYLESHEET_BYTES, EventKind,
    ListenOption, MODIFIER_MASK, NO_NODE, Opcode, ROOT_NODE, Tag, Tags,
};

/// One `listen` registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listener {
    pub kind: EventKind,
    pub handler: u32,
    pub filter: crate::Filter,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct EditorState {
    pub anchor: usize,
    pub focus: usize,
    pub preedit: Option<Preedit>,
    pub history_epoch: Arc<()>,
    pub manual_text_scroll: bool,
    pub preferred_x: Option<f32>,
    pub upstream: bool,
    pub option_selected: bool,
    pub option_dirty: bool,
    pub select_popup: Option<crate::select::SelectPopup>,
    /// Sticky across type changes: a local/deferred edit must not alter capacity.
    pub value_reserved: bool,
}
impl EditorState {
    pub fn reset_navigation(&mut self) {
        self.preferred_x = None;
        self.upstream = false;
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct OutlineStyle {
    pub values: BTreeMap<CssProperty, (CssProperty, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Element {
    pub tag: Tag,
    pub attributes: BTreeMap<Attribute, String>,
    pub class: String,
    pub style: BTreeMap<CssProperty, String>,
    /// CSSOM-like last writer for each geometry longhand, kept separately so
    /// shorthand updates and removals do not depend on enum sorting.
    pub layout_inline: BTreeMap<CssProperty, (CssProperty, String, usize)>,
    /// Last inline writer for each border side and component (width/style).
    pub border_inline: BTreeMap<(u8, bool), (CssProperty, String)>,
    /// Last inline writer for each border color, including shorthand resets.
    pub border_color_inline: BTreeMap<u8, (CssProperty, String)>,
    pub outline_inline: Box<OutlineStyle>,
    pub listeners: Vec<Listener>,
    /// The live value of `input`, `textarea` and `select` (`set_value`).
    pub value: String,
    /// Selection endpoints are UTF-8 byte offsets in the current value.
    pub editor: Box<EditorState>,
    pub checked: bool,
    /// The field edit counter (`seq`).
    pub seq: u32,
    pub scroll_top: f64,
    /// `dialog`: shown modally.
    pub open: bool,
}
impl Element {
    fn new(tag: Tag) -> Self {
        Self {
            tag,
            attributes: BTreeMap::new(),
            class: String::new(),
            style: BTreeMap::new(),
            layout_inline: BTreeMap::new(),
            border_inline: BTreeMap::new(),
            border_color_inline: BTreeMap::new(),
            outline_inline: Box::default(),
            listeners: Vec::new(),
            value: String::new(),
            editor: Box::default(),
            checked: false,
            seq: 0,
            scroll_top: 0.0,
            open: false,
        }
    }
    pub(crate) fn input_type(&self) -> &str {
        self.attributes
            .get(&Attribute::Type)
            .map_or("text", String::as_str)
    }
    pub(crate) fn checkable(&self) -> bool {
        self.tag == Tag::Input && matches!(self.input_type(), "checkbox" | "radio")
    }
    /// A field with a live text value (`set_value`).
    pub(crate) fn text_field(&self) -> bool {
        (self.tag == Tag::Input && !self.checkable()) || self.tag == Tag::Textarea
    }
    pub(crate) fn disabled(&self) -> bool {
        self.attributes.contains_key(&Attribute::Disabled)
    }
    pub(crate) fn readonly(&self) -> bool {
        self.attributes.contains_key(&Attribute::Readonly)
    }
    pub(crate) fn focusable(&self) -> bool {
        matches!(
            self.tag,
            Tag::Button | Tag::Input | Tag::Select | Tag::Textarea | Tag::A
        ) || self.attributes.contains_key(&Attribute::Tabindex)
    }
}

pub(crate) fn geometry_sides(property: CssProperty) -> Option<&'static [CssProperty]> {
    use CssProperty as P;
    Some(match property {
        P::Margin => &[P::MarginTop, P::MarginRight, P::MarginBottom, P::MarginLeft],
        P::Padding => &[
            P::PaddingTop,
            P::PaddingRight,
            P::PaddingBottom,
            P::PaddingLeft,
        ],
        P::Gap => &[P::RowGap, P::ColumnGap],
        P::MarginTop => &[P::MarginTop],
        P::MarginRight => &[P::MarginRight],
        P::MarginBottom => &[P::MarginBottom],
        P::MarginLeft => &[P::MarginLeft],
        P::PaddingTop => &[P::PaddingTop],
        P::PaddingRight => &[P::PaddingRight],
        P::PaddingBottom => &[P::PaddingBottom],
        P::PaddingLeft => &[P::PaddingLeft],
        P::RowGap => &[P::RowGap],
        P::ColumnGap => &[P::ColumnGap],
        _ => return None,
    })
}

pub(crate) fn border_components(property: CssProperty) -> Vec<(u8, bool)> {
    use CssProperty as P;
    let (sides, width, style): (&[u8], bool, bool) = match property {
        P::Border => (&[0, 1, 2, 3], true, true),
        P::BorderTop => (&[0], true, true),
        P::BorderRight => (&[1], true, true),
        P::BorderBottom => (&[2], true, true),
        P::BorderLeft => (&[3], true, true),
        P::BorderWidth => (&[0, 1, 2, 3], true, false),
        P::BorderStyle => (&[0, 1, 2, 3], false, true),
        _ => return Vec::new(),
    };
    sides
        .iter()
        .flat_map(|side| [(width, (*side, false)), (style, (*side, true))])
        .filter_map(|(enabled, key)| enabled.then_some(key))
        .collect()
}

pub(crate) fn border_color_sides(property: CssProperty) -> &'static [u8] {
    use CssProperty as P;
    match property {
        P::Border | P::BorderColor => &[0, 1, 2, 3],
        P::BorderTop => &[0],
        P::BorderRight => &[1],
        P::BorderBottom => &[2],
        P::BorderLeft => &[3],
        _ => &[],
    }
}

pub(crate) fn outline_components(property: CssProperty) -> &'static [CssProperty] {
    use CssProperty as P;
    match property {
        P::Outline => &[P::OutlineWidth, P::OutlineStyle, P::OutlineColor],
        P::OutlineWidth => &[P::OutlineWidth],
        P::OutlineStyle => &[P::OutlineStyle],
        P::OutlineColor => &[P::OutlineColor],
        P::OutlineOffset => &[P::OutlineOffset],
        _ => &[],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub identity: Arc<()>,
    pub content: Content,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
}

/// A host-side consequence of an applied batch, performed after the swap.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Effect {
    Copy {
        request: u32,
        text: String,
    },
    /// A record caused by a guest command (`focusin`, `focusout`, `close`).
    Record(crate::EventRecord),
    /// A queued scroll belongs to this particular element, even if its ID is reused.
    ScrollRecord {
        record: crate::EventRecord,
        identity: Arc<()>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sheet {
    pub text: String,
    pub rules: Vec<Rule>,
    pub index: style::RuleIndex,
    pub work: usize,
    pub text_units: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tree {
    pub nodes: BTreeMap<u32, Node>,
    /// Live handler ids and the node holding each.
    pub handlers: BTreeMap<u32, u32>,
    pub sheets: BTreeMap<u32, Sheet>,
    pub stylesheet_bytes: usize,
    pub stylesheet_work: usize,
    pub rule_index: style::CascadeIndex,
    pub title: String,
    pub focus: Option<u32>,
    /// `copy_text` requests whose `command_result` is not yet delivered.
    pub pending_requests: BTreeSet<u32>,
}
impl Default for Tree {
    fn default() -> Self {
        let root = Node {
            identity: Arc::new(()),
            content: Content::Element(Element::new(Tag::Div)),
            parent: None,
            children: Vec::new(),
        };
        Self {
            nodes: BTreeMap::from([(ROOT_NODE, root)]),
            handlers: BTreeMap::new(),
            sheets: BTreeMap::new(),
            stylesheet_bytes: 0,
            stylesheet_work: 0,
            rule_index: style::CascadeIndex::default(),
            title: String::new(),
            focus: None,
            pending_requests: BTreeSet::new(),
        }
    }
}

/// What a commit needs from its host: the patch-time external-link check
/// against the package request.
pub(crate) struct Context<'a> {
    pub link_requested: &'a dyn Fn(&str) -> bool,
    pub environment: Environment,
    pub layout: Option<LayoutContext<'a>>,
}

pub(crate) struct LayoutContext<'a> {
    pub width: f32,
    pub height: f32,
    pub font: Option<(&'a mut crate::TextMeasurer, f32)>,
}

struct Apply<'tree, 'context, 'input> {
    tree: &'tree mut Tree,
    context: &'context mut Context<'input>,
    effects: Vec<Effect>,
    index: usize,
    layout_commands: usize,
    select_work: usize,
    radio_work: usize,
    stylesheet_work: usize,
    last_select_value: Option<(u32, u32, String)>,
}

/// A per-navigation partition; group members retain document order. Building it
/// once avoids one complete tree scan per distinct radio name during Tab.
#[derive(Default)]
pub(crate) struct RadioGroups {
    by_node: BTreeMap<u32, usize>,
    members: Vec<Vec<u32>>,
}
impl RadioGroups {
    pub(crate) fn group(&self, id: u32) -> &[u32] {
        self.by_node
            .get(&id)
            .map_or(&[], |index| self.members[*index].as_slice())
    }
}

impl Tree {
    fn validate_retained_text(&mut self) -> Result<(), Violation> {
        use varta_common::view::{
            DRAFT_FIELD_VALUE_RESERVATION_BYTES, DRAFT_MAX_RETAINED_TEXT_BYTES,
        };
        let mut total = 0usize;
        let mut charge = |bytes: usize| {
            total = total.saturating_add(bytes);
            if total > DRAFT_MAX_RETAINED_TEXT_BYTES {
                Err(Violation::RetainedTextLimit)
            } else {
                Ok(())
            }
        };
        charge(self.title.len())?;
        for sheet in self.sheets.values() {
            charge(sheet.text.len())?;
        }
        for node in self.nodes.values_mut() {
            match &mut node.content {
                Content::Text(text) => charge(text.len())?,
                Content::Element(element) => {
                    let value_field = element.text_field() || element.tag == Tag::Select;
                    element.editor.value_reserved |= value_field;
                    charge(element.class.len())?;
                    for text in element.attributes.values().chain(element.style.values()) {
                        charge(text.len())?;
                    }
                    if element.editor.value_reserved {
                        charge(DRAFT_FIELD_VALUE_RESERVATION_BYTES)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn radio_groups(&self) -> RadioGroups {
        let mut groups = RadioGroups::default();
        let mut named = BTreeMap::new();
        let mut stack = vec![(ROOT_NODE, ROOT_NODE)];
        while let Some((id, inherited_owner)) = stack.pop() {
            let node = &self.nodes[&id];
            let owner = if self
                .element(id)
                .is_some_and(|element| element.tag == Tag::Form)
            {
                id
            } else {
                inherited_owner
            };
            if let Some(element) = self.element(id)
                && element.checkable()
                && element.input_type() == "radio"
            {
                let name = element
                    .attributes
                    .get(&Attribute::Name)
                    .filter(|name| !name.is_empty());
                let index = match name {
                    Some(name) => *named.entry((owner, name.as_str())).or_insert_with(|| {
                        groups.members.push(Vec::new());
                        groups.members.len() - 1
                    }),
                    None => {
                        groups.members.push(Vec::new());
                        groups.members.len() - 1
                    }
                };
                groups.members[index].push(id);
                groups.by_node.insert(id, index);
            }
            stack.extend(node.children.iter().rev().map(|child| (*child, owner)));
        }
        groups
    }

    pub(crate) fn element(&self, id: u32) -> Option<&Element> {
        match self.nodes.get(&id).map(|node| &node.content) {
            Some(Content::Element(element)) => Some(element),
            _ => None,
        }
    }
    pub(crate) fn element_mut(&mut self, id: u32) -> Option<&mut Element> {
        match self.nodes.get_mut(&id).map(|node| &mut node.content) {
            Some(Content::Element(element)) => Some(element),
            _ => None,
        }
    }
    /// Whether `id` is attached under the root (or is the root).
    pub(crate) fn attached(&self, mut id: u32) -> bool {
        loop {
            if id == ROOT_NODE {
                return true;
            }
            match self.nodes.get(&id).and_then(|node| node.parent) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }
    pub(crate) fn displayed(&self, id: u32, environment: &Environment) -> bool {
        self.attached(id)
            && !self.path(id).into_iter().any(|ancestor| {
                self.computed_property(ancestor, environment, CssProperty::Display)
                    .is_some_and(|value| value == "none")
            })
    }
    /// The node and its ancestors, target first.
    pub(crate) fn path(&self, id: u32) -> Vec<u32> {
        let mut path = vec![id];
        let mut current = id;
        while let Some(parent) = self.nodes.get(&current).and_then(|node| node.parent) {
            path.push(parent);
            current = parent;
        }
        path
    }
    /// Concatenated text of a subtree.
    pub(crate) fn text_content(&self, id: u32) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out);
        out
    }
    fn collect_text(&self, id: u32, out: &mut String) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        match &node.content {
            Content::Text(text) => out.push_str(text),
            Content::Element(_) => {
                for child in &node.children {
                    self.collect_text(*child, out);
                }
            }
        }
    }
    /// Live attached elements in document order.
    pub(crate) fn document_order(&self) -> Vec<u32> {
        self.subtree_order(ROOT_NODE)
    }
    fn subtree_order(&self, root: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(node) = self.nodes.get(&id) {
                stack.extend(node.children.iter().rev());
            }
        }
        out
    }
    /// A nonempty-name radio group shares its tree and closest form owner.
    /// Walk once from that owner, skipping nested forms, rather than resolving
    /// every candidate's ancestor path separately.
    pub(crate) fn radio_group(&self, id: u32) -> Vec<u32> {
        self.radio_group_with_work(id, usize::MAX)
            .expect("one group cannot overflow usize")
            .0
    }
    fn radio_group_with_work(
        &self,
        id: u32,
        allowance: usize,
    ) -> Result<(Vec<u32>, usize), Violation> {
        let mut work = 0usize;
        let mut charge = |amount: usize| {
            work = work.saturating_add(amount);
            if work > allowance {
                Err(Violation::RadioWorkLimit)
            } else {
                Ok(())
            }
        };
        let Some(element) = self.element(id) else {
            return Ok((Vec::new(), 0));
        };
        if !element.checkable() || element.input_type() != "radio" {
            return Ok((Vec::new(), 0));
        }
        let Some(name) = element
            .attributes
            .get(&Attribute::Name)
            .filter(|name| !name.is_empty())
        else {
            charge(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            return Ok((vec![id], work));
        };
        let mut owner = id;
        let mut at = Some(id);
        while let Some(current) = at {
            charge(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            owner = current;
            let node = &self.nodes[&current];
            if self
                .element(current)
                .is_some_and(|element| element.tag == Tag::Form)
            {
                break;
            }
            at = node.parent;
        }
        let mut group = Vec::new();
        let mut stack = vec![owner];
        while let Some(current) = stack.pop() {
            charge(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            let node = &self.nodes[&current];
            if let Content::Element(candidate) = &node.content {
                if current != owner && candidate.tag == Tag::Form {
                    continue;
                }
                if candidate.checkable() && candidate.input_type() == "radio" {
                    let candidate_name = candidate.attributes.get(&Attribute::Name);
                    charge(candidate_name.map_or(0, String::len))?;
                    if candidate_name == Some(name) {
                        group.push(current);
                    }
                }
            }
            stack.extend(node.children.iter().rev());
        }
        Ok((group, work))
    }
    pub(crate) fn by_ident(&self, ident: &str) -> Option<u32> {
        self.nodes
            .iter()
            .find_map(|(id, node)| match &node.content {
                Content::Element(element)
                    if element.attributes.get(&Attribute::Id).map(String::as_str)
                        == Some(ident) =>
                {
                    Some(*id)
                }
                _ => None,
            })
    }
    /// Resolve inherited custom properties, copying only the winning bindings.
    pub(crate) fn custom_properties(
        &self,
        id: u32,
        environment: &Environment,
    ) -> BTreeMap<String, String> {
        if !self.rule_index.has_custom() {
            return BTreeMap::new();
        }
        let mut raw = BTreeMap::new();
        for ancestor in self.path(id).into_iter().rev() {
            for (name, value) in self.cascade_refs(ancestor, environment) {
                if let Name::Custom(name) = name {
                    raw.insert(name.as_str(), value);
                }
            }
        }
        raw.into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect()
    }
    pub(crate) fn cascade_refs(&self, id: u32, environment: &Environment) -> Vec<(&Name, &str)> {
        let Some(element) = self.element(id) else {
            return Vec::new();
        };
        let classes: BTreeSet<_> = element.class.split_ascii_whitespace().collect();
        let class_list: Vec<_> = classes.iter().copied().collect();
        let subject = Subject {
            root: id == ROOT_NODE,
            classes: &class_list,
            focused: self.focus == Some(id),
            disabled: element.disabled(),
            checked: element.checked,
        };
        let mut matched = Vec::new();
        for (sheet, index) in self.rule_index.candidates(subject.root, &classes) {
            let rule = &self.sheets[&sheet].rules[index];
            if style::applies(rule, environment)
                && let Some(specificity) =
                    rule.matching_with_classes(&subject, |class| classes.contains(class))
            {
                matched.push((specificity, sheet, index, rule));
            }
        }
        matched.sort_by_key(|(specificity, sheet, index, _)| (*specificity, *sheet, *index));
        matched
            .into_iter()
            .flat_map(|(_, _, _, rule)| {
                rule.declarations
                    .iter()
                    .map(|(name, value)| (name, value.as_str()))
            })
            .collect()
    }
    /// Select one winning property without resolving unrelated values/variables.
    pub(crate) fn computed_property(
        &self,
        id: u32,
        environment: &Environment,
        property: CssProperty,
    ) -> Option<String> {
        let inline = self
            .element(id)
            .and_then(|element| element.style.get(&property));
        let raw = inline.map(String::as_str).or_else(|| {
            self.cascade_refs(id, environment)
                .into_iter()
                .rev()
                .find_map(|(name, value)| {
                    matches!(name,Name::Property(p) if *p==property).then_some(value)
                })
        })?;
        style::Variables::new(self, id, environment).substitute(raw)
    }
    pub(crate) fn computed(
        &self,
        id: u32,
        environment: &Environment,
    ) -> BTreeMap<CssProperty, String> {
        let mut declared = BTreeMap::new();
        for (name, value) in self.cascade_refs(id, environment) {
            if let Name::Property(property) = name {
                declared.insert(*property, value);
            }
        }
        if let Some(element) = self.element(id) {
            for (property, value) in &element.style {
                declared.insert(*property, value.as_str());
            }
        }
        let variables = style::Variables::new(self, id, environment);
        declared
            .into_iter()
            .filter_map(|(property, value)| {
                variables.substitute(value).map(|value| (property, value))
            })
            .collect()
    }
    /// Document order with hidden subtrees skipped once, rather than checking
    /// the complete ancestor path for every prospective focus stop.
    pub(crate) fn displayed_order(&self, environment: &Environment) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![ROOT_NODE];
        while let Some(id) = stack.pop() {
            if self
                .computed_property(id, environment, CssProperty::Display)
                .is_some_and(|display| display == "none")
            {
                continue;
            }
            out.push(id);
            stack.extend(self.nodes[&id].children.iter().rev());
        }
        out
    }

    /// Apply a decoded batch to this (copied) tree.
    pub(crate) fn apply(
        &mut self,
        operations: &[Operation],
        context: &mut Context<'_>,
    ) -> Result<Vec<Effect>, ViewError> {
        let mut apply = Apply {
            tree: self,
            context,
            effects: Vec::new(),
            index: 0,
            layout_commands: 0,
            select_work: 0,
            radio_work: 0,
            stylesheet_work: 0,
            last_select_value: None,
        };
        for (index, operation) in operations.iter().enumerate() {
            apply.index = index;
            apply.operation(operation)?;
        }
        apply.finish()?;
        Ok(apply.effects)
    }
}

impl Apply<'_, '_, '_> {
    fn fail(&self, violation: Violation) -> ViewError {
        ViewError::Invalid {
            operation: Some(self.index),
            violation,
        }
    }
    fn live(&self, id: u32) -> Result<&Node, ViewError> {
        self.tree
            .nodes
            .get(&id)
            .ok_or_else(|| self.fail(Violation::UnknownNode(id)))
    }
    fn element(&mut self, id: u32) -> Result<&mut Element, ViewError> {
        let operation = Some(self.index);
        match self.tree.nodes.get_mut(&id).map(|node| &mut node.content) {
            Some(Content::Element(element)) => Ok(element),
            Some(Content::Text(_)) => Err(ViewError::Invalid {
                operation,
                violation: Violation::NotAnElement(id),
            }),
            None => Err(ViewError::Invalid {
                operation,
                violation: Violation::UnknownNode(id),
            }),
        }
    }
    fn tag(&mut self, id: u32) -> Result<Tag, ViewError> {
        Ok(self.element(id)?.tag)
    }

    fn charge_select_work(
        &mut self,
        work: usize,
        operation: Option<usize>,
    ) -> Result<(), ViewError> {
        self.select_work = self.select_work.saturating_add(work);
        if self.select_work > varta_common::view::DRAFT_MAX_SELECT_WORK {
            return Err(ViewError::Invalid {
                operation,
                violation: Violation::SelectWorkLimit,
            });
        }
        Ok(())
    }

    fn charge_radio_work(&mut self, work: usize) -> Result<(), ViewError> {
        self.radio_work = self.radio_work.saturating_add(work);
        if self.radio_work > varta_common::view::DRAFT_MAX_RADIO_WORK {
            return Err(self.fail(Violation::RadioWorkLimit));
        }
        Ok(())
    }

    // Meter independently of checked/seq: a browser edit can still be queued
    // for Worker delivery. Group membership is authoritative in either state.
    fn reconcile_radio(&mut self, id: u32, apply: bool) -> Result<(), ViewError> {
        let (group, work) = self
            .tree
            .radio_group_with_work(
                id,
                varta_common::view::DRAFT_MAX_RADIO_WORK.saturating_sub(self.radio_work),
            )
            .map_err(|violation| self.fail(violation))?;
        self.charge_radio_work(work)?;
        if apply {
            for peer in group.into_iter().filter(|peer| *peer != id) {
                if let Some(element) = self.tree.element_mut(peer) {
                    element.checked = false;
                }
            }
        }
        Ok(())
    }

    fn operation(&mut self, operation: &Operation) -> Result<(), ViewError> {
        let node = operation.id(0);
        let select_value = operation.opcode == Opcode::SetValue
            && self
                .tree
                .element(node)
                .is_some_and(|element| element.tag == Tag::Select);
        // The counter must not depend on locally edited value/seq: a browser
        // Worker can still be waiting to deliver that edit. Consecutive identical
        // writes are idempotent even when their unmatched value becomes "".
        if select_value
            && self
                .last_select_value
                .as_ref()
                .is_some_and(|(id, seq, value)| {
                    *id == node && *seq == operation.id(2) && value.as_str() == operation.text()
                })
        {
            return Ok(());
        }
        self.last_select_value = None;
        self.charge_select_work(self.tree.select_operation_work(operation), Some(self.index))?;
        if select_value {
            self.last_select_value = Some((node, operation.id(2), operation.text().to_owned()));
        }
        match operation.opcode {
            Opcode::CreateElement => {
                let Some(Value::Tag(tag)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownTag));
                };
                self.create(node, Content::Element(Element::new(*tag)))
            }
            Opcode::CreateText => self.create(node, Content::Text(operation.text().into())),
            Opcode::SetText => {
                let text = operation.text().to_owned();
                match self.tree.nodes.get_mut(&node).map(|node| &mut node.content) {
                    Some(Content::Text(current)) => {
                        *current = text;
                        self.tree.sync_select_for(node);
                        Ok(())
                    }
                    Some(Content::Element(_)) => Err(self.fail(Violation::NotAText(node))),
                    None => Err(self.fail(Violation::UnknownNode(node))),
                }
            }
            Opcode::SetAttribute => {
                let Some(Value::Attribute(attribute)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownAttribute));
                };
                self.set_attribute(node, *attribute, operation.text())
            }
            Opcode::RemoveAttribute => {
                let Some(Value::Attribute(attribute)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownAttribute));
                };
                let tag = self.tag(node)?;
                self.attribute_admitted(*attribute, tag)?;
                if self.element(node)?.attributes.remove(attribute).is_some() {
                    self.tree.select_attribute(node, *attribute, false);
                }
                Ok(())
            }
            Opcode::SetClass => {
                let class = operation.text();
                if !class
                    .split_ascii_whitespace()
                    .all(varta_common::view::css::ident)
                {
                    return Err(self.fail(Violation::InvalidClass));
                }
                self.element(node)?.class = class.into();
                Ok(())
            }
            Opcode::SetStyle => {
                let Some(Value::Property(property)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownProperty));
                };
                let tag = self.tag(node)?;
                if matches!(property, CssProperty::FieldSizing | CssProperty::Resize)
                    && tag != Tag::Textarea
                {
                    return Err(self.fail(Violation::PropertyNotAllowed));
                }
                if !varta_common::view::css::property_value(*property, operation.text()) {
                    return Err(self.fail(Violation::InvalidStyleValue));
                }
                let element = self.element(node)?;
                element.style.insert(*property, operation.text().into());
                if let Some(sides) = geometry_sides(*property) {
                    for (index, side) in sides.iter().enumerate() {
                        element
                            .layout_inline
                            .insert(*side, (*property, operation.text().into(), index));
                    }
                }
                for component in border_components(*property) {
                    element
                        .border_inline
                        .insert(component, (*property, operation.text().into()));
                }
                for side in border_color_sides(*property) {
                    element
                        .border_color_inline
                        .insert(*side, (*property, operation.text().into()));
                }
                for component in outline_components(*property) {
                    element
                        .outline_inline
                        .values
                        .insert(*component, (*property, operation.text().into()));
                }
                Ok(())
            }
            Opcode::RemoveStyle => {
                let Some(Value::Property(property)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownProperty));
                };
                let element = self.element(node)?;
                element.style.remove(property);
                if let Some(sides) = geometry_sides(*property) {
                    for side in sides {
                        element.layout_inline.remove(side);
                    }
                }
                for component in border_components(*property) {
                    element.border_inline.remove(&component);
                }
                for side in border_color_sides(*property) {
                    element.border_color_inline.remove(side);
                }
                for component in outline_components(*property) {
                    element.outline_inline.values.remove(component);
                }
                Ok(())
            }
            Opcode::Insert => self.insert(node, operation.id(1), operation.id(2)),
            Opcode::Remove => self.remove(node),
            Opcode::Listen => self.listen(node, operation),
            Opcode::Unlisten => {
                let (Some(Value::Event(kind)), handler) =
                    (operation.operands.get(1), operation.id(2))
                else {
                    return Err(self.fail(Violation::UnknownEvent));
                };
                let kind = *kind;
                let element = self.element(node)?;
                let Some(position) = element
                    .listeners
                    .iter()
                    .position(|listener| listener.handler == handler && listener.kind == kind)
                else {
                    return Err(self.fail(Violation::NotListening(handler)));
                };
                element.listeners.remove(position);
                self.tree.handlers.remove(&handler);
                Ok(())
            }
            Opcode::SetValue => self.set_value(node, operation),
            Opcode::SetChecked => self.set_checked(node, operation),
            Opcode::Command => self.command(node, operation),
            Opcode::Stylesheet => self.stylesheet(node, operation.text()),
            Opcode::SetTitle => {
                self.tree.title = operation.text().into();
                Ok(())
            }
            Opcode::CopyText => {
                let request = node;
                if request == 0 || self.tree.pending_requests.contains(&request) {
                    return Err(self.fail(Violation::DuplicateRequest(request)));
                }
                if self.tree.pending_requests.len() >= DRAFT_MAX_PENDING_COMMANDS {
                    return Err(self.fail(Violation::TooManyPendingCommands));
                }
                self.tree.pending_requests.insert(request);
                self.effects.push(Effect::Copy {
                    request,
                    text: operation.text().into(),
                });
                Ok(())
            }
        }
    }

    fn create(&mut self, id: u32, content: Content) -> Result<(), ViewError> {
        if id == NO_NODE || self.tree.nodes.contains_key(&id) {
            return Err(self.fail(Violation::DuplicateNode(id)));
        }
        if self.tree.nodes.len() >= DRAFT_MAX_LIVE_NODES {
            return Err(self.fail(Violation::TooManyNodes));
        }
        self.tree.nodes.insert(
            id,
            Node {
                identity: Arc::new(()),
                content,
                parent: None,
                children: Vec::new(),
            },
        );
        Ok(())
    }

    fn attribute_admitted(&self, attribute: Attribute, tag: Tag) -> Result<(), ViewError> {
        let admitted = match attribute.tags() {
            Tags::All => true,
            Tags::Focusable => false,
            Tags::Only(tags) => tags.contains(&tag),
        };
        if admitted {
            Ok(())
        } else {
            Err(self.fail(Violation::AttributeNotAllowed(attribute.name())))
        }
    }

    fn set_attribute(
        &mut self,
        node: u32,
        attribute: Attribute,
        value: &str,
    ) -> Result<(), ViewError> {
        let tag = self.tag(node)?;
        self.attribute_admitted(attribute, tag)?;
        let grammar = attribute.value();
        let valid = if grammar == AttributeValue::Href && values::external(value) {
            (self.context.link_requested)(value)
        } else {
            values::valid(grammar, tag, value)
        };
        if !valid {
            return Err(self.fail(Violation::InvalidAttributeValue(attribute.name())));
        }
        let element = self.element(node)?;
        let changed = element.attributes.get(&attribute).map(String::as_str) != Some(value);
        element.attributes.insert(attribute, value.into());
        if matches!(attribute, Attribute::Disabled | Attribute::Readonly) {
            element.editor.preedit = None;
            element.editor.select_popup = None;
        }
        let reconcile = matches!(attribute, Attribute::Name | Attribute::Type)
            && element.checkable()
            && element.input_type() == "radio";
        let checked = element.checked;
        if reconcile {
            self.reconcile_radio(node, checked)?;
        }
        if changed {
            self.tree.select_attribute(node, attribute, true);
        }
        Ok(())
    }

    fn insert(&mut self, parent: u32, node: u32, before: u32) -> Result<(), ViewError> {
        if node == ROOT_NODE {
            return Err(self.fail(Violation::RootNotMovable));
        }
        let parent_tag = self.tag(parent)?;
        let child = self.live(node)?;
        let child_tag = match &child.content {
            Content::Element(element) => Some(element.tag),
            Content::Text(_) => None,
        };
        if !parent_tag.admits_child(child_tag) {
            return Err(self.fail(Violation::ChildNotAllowed));
        }
        if self.tree.path(parent).contains(&node) {
            return Err(self.fail(Violation::Cycle));
        }
        if before != NO_NODE
            && (before == node
                || self.tree.nodes.get(&before).and_then(|n| n.parent) != Some(parent))
        {
            return Err(self.fail(Violation::InvalidBefore(before)));
        }
        let old_select = self.tree.select_owner(node);
        let moved_option = self
            .tree
            .element(node)
            .is_some_and(|element| element.tag == Tag::Option);
        self.detach(node);
        let children = &mut self
            .tree
            .nodes
            .get_mut(&parent)
            .ok_or(ViewError::Invalid {
                operation: Some(self.index),
                violation: Violation::UnknownNode(parent),
            })?
            .children;
        let at = if before == NO_NODE {
            children.len()
        } else {
            children
                .iter()
                .position(|child| *child == before)
                .unwrap_or(children.len())
        };
        children.insert(at, node);
        if let Some(child) = self.tree.nodes.get_mut(&node) {
            child.parent = Some(parent);
        }
        let mut stack = vec![node];
        while let Some(radio) = stack.pop() {
            self.charge_radio_work(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            if let Some(element) = self.tree.element(radio)
                && element.checkable()
                && element.input_type() == "radio"
            {
                let checked = element.checked;
                self.reconcile_radio(radio, checked)?;
            }
            stack.extend(self.tree.nodes[&radio].children.iter().rev());
        }
        if let Some(select) = old_select {
            if moved_option {
                self.tree.normalize_select(select, None);
            } else {
                self.tree.sync_select_value(select);
            }
        }
        if let Some(select) = self.tree.select_owner(node) {
            let preferred = self
                .tree
                .element(node)
                .filter(|element| element.tag == Tag::Option && element.editor.option_selected)
                .map(|_| node);
            if self
                .tree
                .element(node)
                .is_some_and(|element| element.tag == Tag::Option)
            {
                self.tree.normalize_select(select, preferred);
            } else {
                self.tree.sync_select_for(node);
            }
        }
        Ok(())
    }

    fn detach(&mut self, node: u32) {
        let Some(parent) = self.tree.nodes.get(&node).and_then(|n| n.parent) else {
            return;
        };
        if let Some(parent) = self.tree.nodes.get_mut(&parent) {
            parent.children.retain(|child| *child != node);
        }
        if let Some(child) = self.tree.nodes.get_mut(&node) {
            child.parent = None;
        }
    }

    fn remove(&mut self, node: u32) -> Result<(), ViewError> {
        if node == ROOT_NODE {
            return Err(self.fail(Violation::RootNotMovable));
        }
        self.live(node)?;
        let select = self.tree.select_owner(node);
        let option = self
            .tree
            .element(node)
            .is_some_and(|element| element.tag == Tag::Option);
        self.detach(node);
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            let Some(removed) = self.tree.nodes.remove(&id) else {
                continue;
            };
            if let Content::Element(element) = &removed.content {
                for listener in &element.listeners {
                    self.tree.handlers.remove(&listener.handler);
                }
            }
            if self.tree.focus == Some(id) {
                self.tree.focus = None;
            }
            stack.extend(removed.children);
        }
        if let Some(select) = select {
            if option {
                self.tree.normalize_select(select, None);
            } else {
                self.tree.sync_select_value(select);
            }
        }
        Ok(())
    }

    fn listen(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let (Some(Value::Event(kind)), handler) = (operation.operands.get(1), operation.id(2))
        else {
            return Err(self.fail(Violation::UnknownEvent));
        };
        let kind = *kind;
        let Some(Value::Key(key)) = operation.operands.get(4) else {
            return Err(self.fail(Violation::UnknownKey));
        };
        let filter = crate::Filter {
            options: operation.id(3),
            key: *key,
            required: operation.id(5),
            forbidden: operation.id(6),
        };
        let option_bits = ListenOption::ALL
            .iter()
            .fold(0, |bits, option| bits | *option as u32);
        if kind == EventKind::CommandResult {
            return Err(self.fail(Violation::NotListenable));
        }
        if filter.options & !option_bits != 0
            || filter.required & !MODIFIER_MASK != 0
            || filter.forbidden & !MODIFIER_MASK != 0
        {
            return Err(self.fail(Violation::InvalidFilter));
        }
        let key_filter = filter.key != ANY_KEY
            || filter.required != 0
            || filter.forbidden != 0
            || filter.options & ListenOption::NotComposing as u32 != 0;
        if kind != EventKind::Keydown && key_filter {
            return Err(self.fail(Violation::InvalidFilter));
        }
        if handler == 0 || self.tree.handlers.contains_key(&handler) {
            return Err(self.fail(Violation::DuplicateHandler(handler)));
        }
        if self.tree.handlers.len() >= DRAFT_MAX_LIVE_LISTENERS {
            return Err(self.fail(Violation::TooManyListeners));
        }
        let same_filter = |listener: &Listener| {
            listener.kind == kind
                && listener.filter.key == filter.key
                && listener.filter.required == filter.required
                && listener.filter.forbidden == filter.forbidden
                && (listener.filter.options & ListenOption::NotComposing as u32)
                    == (filter.options & ListenOption::NotComposing as u32)
        };
        let element = self.element(node)?;
        if element.listeners.iter().any(same_filter) {
            return Err(self.fail(Violation::DuplicateFilter));
        }
        element.listeners.push(Listener {
            kind,
            handler,
            filter,
        });
        self.tree.handlers.insert(handler, node);
        Ok(())
    }

    fn set_value(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let base = operation.id(2);
        let value = operation.text().to_owned();
        let element = self.element(node)?;
        if !(element.text_field() || element.tag == Tag::Select) {
            return Err(self.fail(Violation::NotAField(node)));
        }
        let element = self.element(node)?;
        element.editor.value_reserved = true;
        // A stale patch (newer local edits exist) is dropped, not an error.
        if element.seq == base {
            if element.tag == Tag::Select {
                if element.value != value {
                    self.tree.set_select_value(node, &value);
                }
                return Ok(());
            }
            if element.value != value {
                element.editor.anchor = value.len();
                element.editor.focus = value.len();
                element.editor.preedit = None;
                element.editor.history_epoch = Arc::new(());
                element.editor.manual_text_scroll = false;
                element.editor.reset_navigation();
            }
            element.value = value;
        }
        Ok(())
    }

    fn set_checked(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let (checked, base) = (operation.id(1), operation.id(2));
        if checked > 1 {
            return Err(self.fail(Violation::InvalidChecked));
        }
        let element = self.element(node)?;
        if !element.checkable() {
            return Err(self.fail(Violation::NotAField(node)));
        }
        let accepted = element.seq == base;
        let radio = element.input_type() == "radio";
        if accepted {
            element.checked = checked == 1;
        }
        if checked == 1 && radio {
            self.reconcile_radio(node, accepted)?;
        }
        Ok(())
    }

    fn command(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let (Some(Value::Command(command)), Some(Value::Number(argument))) =
            (operation.operands.get(1), operation.operands.get(2))
        else {
            return Err(self.fail(Violation::UnknownCommand));
        };
        let (command, argument) = (*command, *argument);
        let element = self.element(node)?;
        let admitted = match command.tags() {
            Tags::All => true,
            Tags::Focusable => element.focusable(),
            Tags::Only(tags) => tags.contains(&element.tag),
        };
        if !admitted {
            return Err(self.fail(Violation::CommandNotAllowed(command.name())));
        }
        let valid_argument = if command.takes_argument() {
            argument.is_finite() && argument >= 0.0
        } else {
            argument.to_bits() == 0
        };
        if !valid_argument {
            return Err(self.fail(Violation::InvalidArgument));
        }
        match command {
            Command::Focus => {
                let element = self.element(node)?;
                if !element.disabled() && self.tree.displayed(node, &self.context.environment) {
                    let records = focus_records(self.tree, Some(node));
                    self.effects.extend(records.into_iter().map(Effect::Record));
                }
            }
            Command::Blur => {
                if self.tree.focus == Some(node) {
                    let records = focus_records(self.tree, None);
                    self.effects.extend(records.into_iter().map(Effect::Record));
                }
            }
            Command::Select => {
                let focus = !self.element(node)?.disabled()
                    && self.tree.displayed(node, &self.context.environment);
                if focus {
                    let records = focus_records(self.tree, Some(node));
                    self.effects.extend(records.into_iter().map(Effect::Record));
                }
                let element = self.element(node)?;
                if element.text_field()
                    && (element.tag == Tag::Textarea || element.input_type() != "number")
                {
                    element.editor.anchor = 0;
                    element.editor.focus = element.value.len();
                    element.editor.preedit = None;
                    element.editor.manual_text_scroll = false;
                    element.editor.reset_navigation();
                }
            }
            Command::ScrollIntoView => {
                if self.context.layout.is_some() {
                    self.layout_commands += 1;
                    if self.layout_commands > DRAFT_MAX_PENDING_COMMANDS {
                        return Err(self.fail(Violation::TooManyPendingCommands));
                    }
                }
                if let Some(layout) = self.context.layout.as_mut() {
                    let geometry = match layout.font.as_mut() {
                        Some((font, size)) => crate::layout::geometry_with_text(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                            font,
                            *size,
                        ),
                        None => crate::layout::geometry(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                        ),
                    }
                    .map_err(|_| self.fail(Violation::ScrollLayout))?;
                    let plan = crate::layout::scroll_into_view_plan(
                        self.tree,
                        &self.context.environment,
                        &geometry,
                        node,
                    )
                    .map_err(|_| self.fail(Violation::ScrollLayout))?;
                    for (id, top) in plan {
                        self.element(id)?.scroll_top = top;
                        let scroll = geometry[&id]
                            .programmatic_scroll
                            .ok_or_else(|| self.fail(Violation::ScrollLayout))?;
                        let mut record = crate::EventRecord::occurrence(EventKind::Scroll, id);
                        record.scroll_top = top;
                        record.client_height = f64::from(scroll.viewport.height);
                        record.scroll_height = f64::from(scroll.viewport.height + scroll.max);
                        let identity = self.tree.nodes[&id].identity.clone();
                        self.effects.push(Effect::ScrollRecord { record, identity });
                    }
                }
            }
            Command::ScrollTo => {
                if self.context.layout.is_some() {
                    self.layout_commands += 1;
                    if self.layout_commands > DRAFT_MAX_PENDING_COMMANDS {
                        return Err(self.fail(Violation::TooManyPendingCommands));
                    }
                }
                if let Some(layout) = self.context.layout.as_mut() {
                    let geometry = match layout.font.as_mut() {
                        Some((font, size)) => crate::layout::geometry_with_text(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                            font,
                            *size,
                        ),
                        None => crate::layout::geometry(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                        ),
                    }
                    .map_err(|_| self.fail(Violation::ScrollLayout))?;
                    if let Some(scroll) = geometry
                        .get(&node)
                        .and_then(|item| item.programmatic_scroll)
                    {
                        let top = argument.clamp(0.0, f64::from(scroll.max));
                        if top != f64::from(scroll.top) {
                            self.element(node)?.scroll_top = top;
                            let mut record =
                                crate::EventRecord::occurrence(EventKind::Scroll, node);
                            record.scroll_top = top;
                            record.client_height = f64::from(scroll.viewport.height);
                            record.scroll_height = f64::from(scroll.viewport.height + scroll.max);
                            let identity = self.tree.nodes[&node].identity.clone();
                            self.effects.push(Effect::ScrollRecord { record, identity });
                        }
                    }
                    return Ok(());
                }
                let scrolls = self
                    .tree
                    .computed(node, &self.context.environment)
                    .get(&CssProperty::Overflow)
                    .is_some_and(|overflow| overflow == "auto");
                if scrolls {
                    self.element(node)?.scroll_top = argument;
                }
            }
            Command::ShowModal => {
                if !self.tree.attached(node) {
                    return Err(self.fail(Violation::Detached(node)));
                }
                self.element(node)?.open = true;
            }
            Command::Close => {
                let element = self.element(node)?;
                if element.open {
                    element.open = false;
                    self.effects
                        .push(Effect::Record(crate::EventRecord::occurrence(
                            EventKind::Close,
                            node,
                        )));
                }
            }
        }
        Ok(())
    }

    fn stylesheet(&mut self, sheet: u32, text: &str) -> Result<(), ViewError> {
        if sheet == 0 {
            return Err(self.fail(Violation::InvalidSheet));
        }
        if text.is_empty() {
            if let Some(old) = self.tree.sheets.remove(&sheet) {
                self.tree.rule_index.remove(sheet, &old.index);
                self.tree.stylesheet_bytes -= old.text_units;
                self.tree.stylesheet_work -= old.work;
            }
            return Ok(());
        }
        let parsed = style::parse(
            text,
            varta_common::view::DRAFT_MAX_STYLESHEET_WORK.saturating_sub(self.stylesheet_work),
        )
        .map_err(|error| {
            self.fail(
                if error.violation == varta_common::view::stylesheet::Violation::WorkLimit {
                    Violation::StylesheetWorkLimit
                } else {
                    Violation::InvalidStylesheet
                },
            )
        })?;
        self.stylesheet_work = self.stylesheet_work.saturating_add(parsed.work);
        let text_units = text.encode_utf16().count() * 2;
        let old = self.tree.sheets.get(&sheet);
        let bytes =
            self.tree.stylesheet_bytes - old.map_or(0, |sheet| sheet.text_units) + text_units;
        if bytes > DRAFT_MAX_STYLESHEET_BYTES {
            return Err(self.fail(Violation::StylesheetsTooLarge));
        }
        let work = self.tree.stylesheet_work - old.map_or(0, |sheet| sheet.work) + parsed.work;
        if work > varta_common::view::DRAFT_MAX_STYLESHEET_WORK {
            return Err(self.fail(Violation::StylesheetWorkLimit));
        }
        let index = style::RuleIndex::new(&parsed.rules);
        if let Some(old) = self.tree.sheets.get(&sheet) {
            self.tree.rule_index.remove(sheet, &old.index);
        }
        self.tree.rule_index.insert(sheet, &index);
        self.tree.sheets.insert(
            sheet,
            Sheet {
                text: text.into(),
                rules: parsed.rules,
                index,
                work: parsed.work,
                text_units,
            },
        );
        self.tree.stylesheet_bytes = bytes;
        self.tree.stylesheet_work = work;
        Ok(())
    }

    /// Invariants of the whole batch: `id` values unique among live elements.
    fn finish(&mut self) -> Result<(), ViewError> {
        self.charge_select_work(self.tree.select_inventory_work(), None)?;
        self.tree
            .validate_retained_text()
            .map_err(|violation| ViewError::Invalid {
                operation: None,
                violation,
            })?;
        let mut seen = BTreeSet::new();
        for (id, node) in &self.tree.nodes {
            if self
                .tree
                .element(*id)
                .is_some_and(|element| element.tag == Tag::Option)
                && self
                    .tree
                    .option_value(*id)
                    .encode_utf16()
                    .take(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
                    .count()
                    > varta_common::view::DRAFT_MAX_VALUE_BYTES / 2
            {
                return Err(ViewError::Invalid {
                    operation: None,
                    violation: Violation::StringTooLong,
                });
            }
            if let Content::Element(element) = &node.content
                && element.tag == Tag::Select
                && element
                    .value
                    .encode_utf16()
                    .take(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
                    .count()
                    > varta_common::view::DRAFT_MAX_VALUE_BYTES / 2
            {
                return Err(ViewError::Invalid {
                    operation: None,
                    violation: Violation::StringTooLong,
                });
            }
            if let Content::Element(element) = &node.content
                && let Some(ident) = element.attributes.get(&Attribute::Id)
                && !seen.insert(ident.as_str())
            {
                return Err(ViewError::Invalid {
                    operation: None,
                    violation: Violation::DuplicateIdent(ident.clone()),
                });
            }
        }
        Ok(())
    }
}

/// Move focus to `next` and return the `focusout`/`focusin` records of the
/// change (the records bubble through the scheduler like any other).
pub(crate) fn focus_records(tree: &mut Tree, next: Option<u32>) -> Vec<crate::EventRecord> {
    let previous = tree.focus;
    if previous == next {
        return Vec::new();
    }
    if let Some(previous) = previous
        && let Some(element) = tree.element_mut(previous)
    {
        element.editor.preedit = None;
        element.editor.select_popup = None;
    }
    tree.focus = next;
    let mut records = Vec::new();
    if let Some(previous) = previous {
        records.push(crate::EventRecord::occurrence(
            EventKind::Focusout,
            previous,
        ));
    }
    if let Some(next) = next {
        records.push(crate::EventRecord::occurrence(EventKind::Focusin, next));
    }
    records
}
