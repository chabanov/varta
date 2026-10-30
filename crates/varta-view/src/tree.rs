//! The live node tree and the semantic rules of every operation. A batch is
//! applied to a copy of the tree; only a batch whose every operation and
//! final invariant holds replaces the live tree (fail closed, atomic).
use crate::style::{self, Environment, Name, Rule, Subject};
use crate::wire::{Operation, Value};
use crate::{ViewError, Violation};
use std::collections::{BTreeMap, BTreeSet};
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
pub(crate) struct Element {
    pub tag: Tag,
    pub attributes: BTreeMap<Attribute, String>,
    pub class: String,
    pub style: BTreeMap<CssProperty, String>,
    pub listeners: Vec<Listener>,
    /// The live value of `input`, `textarea` and `select` (`set_value`).
    pub value: String,
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
            listeners: Vec::new(),
            value: String::new(),
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
    pub(crate) fn classes(&self) -> Vec<&str> {
        self.class.split_ascii_whitespace().collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
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
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sheet {
    pub text: String,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tree {
    pub nodes: BTreeMap<u32, Node>,
    /// Live handler ids and the node holding each.
    pub handlers: BTreeMap<u32, u32>,
    pub sheets: BTreeMap<u32, Sheet>,
    pub title: String,
    pub focus: Option<u32>,
    /// `copy_text` requests whose `command_result` is not yet delivered.
    pub pending_requests: BTreeSet<u32>,
}
impl Default for Tree {
    fn default() -> Self {
        let root = Node {
            content: Content::Element(Element::new(Tag::Div)),
            parent: None,
            children: Vec::new(),
        };
        Self {
            nodes: BTreeMap::from([(ROOT_NODE, root)]),
            handlers: BTreeMap::new(),
            sheets: BTreeMap::new(),
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
}

struct Apply<'a> {
    tree: &'a mut Tree,
    context: &'a Context<'a>,
    effects: Vec<Effect>,
    index: usize,
}

impl Tree {
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
        let mut out = Vec::new();
        let mut stack = vec![ROOT_NODE];
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(node) = self.nodes.get(&id) {
                stack.extend(node.children.iter().rev());
            }
        }
        out
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
    /// The custom properties in scope at `id` (they inherit), cascaded.
    pub(crate) fn custom_properties(
        &self,
        id: u32,
        environment: &Environment,
    ) -> BTreeMap<String, String> {
        let mut custom = BTreeMap::new();
        for ancestor in self.path(id).into_iter().rev() {
            for (name, value) in self.cascade(ancestor, environment) {
                if let Name::Custom(name) = name {
                    custom.insert(name, value);
                }
            }
        }
        custom
    }
    /// The declarations that apply to `id` in cascade order (later wins):
    /// sheets by ascending id, then specificity, then source order.
    pub(crate) fn cascade(&self, id: u32, environment: &Environment) -> Vec<(Name, String)> {
        let Some(element) = self.element(id) else {
            return Vec::new();
        };
        let classes = element.classes();
        let subject = Subject {
            root: id == ROOT_NODE,
            classes: &classes,
            focused: self.focus == Some(id),
            disabled: element.disabled(),
            checked: element.checked,
        };
        let mut matched = Vec::new();
        for (order, rule) in self
            .sheets
            .values()
            .flat_map(|sheet| sheet.rules.iter())
            .enumerate()
        {
            if !style::applies(rule, environment) {
                continue;
            }
            if let Some(specificity) = rule.matching(&subject) {
                matched.push((specificity, order, rule));
            }
        }
        matched.sort_by_key(|(specificity, order, _)| (*specificity, *order));
        matched
            .into_iter()
            .flat_map(|(_, _, rule)| rule.declarations.iter().cloned())
            .collect()
    }
    /// The computed value of each property set on `id`: the cascade, then
    /// inline styles, with `var()` substituted; an unresolvable `var()`
    /// leaves the property unset.
    pub(crate) fn computed(
        &self,
        id: u32,
        environment: &Environment,
    ) -> BTreeMap<CssProperty, String> {
        let mut declared = BTreeMap::new();
        for (name, value) in self.cascade(id, environment) {
            if let Name::Property(property) = name {
                declared.insert(property, value);
            }
        }
        if let Some(element) = self.element(id) {
            for (property, value) in &element.style {
                declared.insert(*property, value.clone());
            }
        }
        let custom = self.custom_properties(id, environment);
        declared
            .into_iter()
            .filter_map(|(property, value)| {
                style::substitute(&value, &custom).map(|value| (property, value))
            })
            .collect()
    }

    /// Apply a decoded batch to this (copied) tree.
    pub(crate) fn apply(
        &mut self,
        operations: &[Operation],
        context: &Context<'_>,
    ) -> Result<Vec<Effect>, ViewError> {
        let mut apply = Apply {
            tree: self,
            context,
            effects: Vec::new(),
            index: 0,
        };
        for (index, operation) in operations.iter().enumerate() {
            apply.index = index;
            apply.operation(operation)?;
        }
        apply.finish()?;
        Ok(apply.effects)
    }
}

impl Apply<'_> {
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

    fn operation(&mut self, operation: &Operation) -> Result<(), ViewError> {
        let node = operation.id(0);
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
                self.element(node)?.attributes.remove(attribute);
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
                self.element(node)?
                    .style
                    .insert(*property, operation.text().into());
                Ok(())
            }
            Opcode::RemoveStyle => {
                let Some(Value::Property(property)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownProperty));
                };
                self.element(node)?.style.remove(property);
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
        self.element(node)?
            .attributes
            .insert(attribute, value.into());
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
        if !admits_child(parent_tag, child_tag) {
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
        // A stale patch (newer local edits exist) is dropped, not an error.
        if element.seq == base {
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
        if element.seq == base {
            element.checked = checked == 1;
            if element.checked && element.input_type() == "radio" {
                let name = element.attributes.get(&Attribute::Name).cloned();
                uncheck_group(self.tree, node, name.as_deref());
            }
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
                if !element.disabled() && self.tree.attached(node) {
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
            Command::Select | Command::ScrollIntoView => {}
            Command::ScrollTo => {
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
            self.tree.sheets.remove(&sheet);
            return Ok(());
        }
        let rules = style::parse(text).ok_or_else(|| self.fail(Violation::InvalidStylesheet))?;
        self.tree.sheets.insert(
            sheet,
            Sheet {
                text: text.into(),
                rules,
            },
        );
        let bytes: usize = self
            .tree
            .sheets
            .values()
            .map(|sheet| sheet.text.encode_utf16().count() * 2)
            .sum();
        if bytes > DRAFT_MAX_STYLESHEET_BYTES {
            return Err(self.fail(Violation::StylesheetsTooLarge));
        }
        Ok(())
    }

    /// Invariants of the whole batch: `id` values unique among live elements.
    fn finish(&mut self) -> Result<(), ViewError> {
        let mut seen = BTreeSet::new();
        for node in self.tree.nodes.values() {
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

/// Uncheck every other radio of the group `name` (no group without a name).
pub(crate) fn uncheck_group(tree: &mut Tree, keep: u32, name: Option<&str>) {
    let Some(name) = name else {
        return;
    };
    let others: Vec<u32> = tree
        .nodes
        .iter()
        .filter_map(|(id, node)| match &node.content {
            Content::Element(element)
                if *id != keep
                    && element.checkable()
                    && element.input_type() == "radio"
                    && element.attributes.get(&Attribute::Name).map(String::as_str)
                        == Some(name) =>
            {
                Some(*id)
            }
            _ => None,
        })
        .collect();
    for id in others {
        if let Some(element) = tree.element_mut(id) {
            element.checked = false;
        }
    }
}

/// Move focus to `next` and return the `focusout`/`focusin` records of the
/// change (the records bubble through the scheduler like any other).
pub(crate) fn focus_records(tree: &mut Tree, next: Option<u32>) -> Vec<crate::EventRecord> {
    let previous = tree.focus;
    if previous == next {
        return Vec::new();
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

/// The content model of the subset: void elements and text nodes take no
/// children, `select` only `option`, `option` only text.
fn admits_child(parent: Tag, child: Option<Tag>) -> bool {
    match parent {
        Tag::Br | Tag::Input | Tag::Img | Tag::Textarea | Tag::Progress => false,
        Tag::Select => child == Some(Tag::Option),
        Tag::Option => child.is_none(),
        _ => true,
    }
}
