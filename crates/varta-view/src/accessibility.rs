//! Host-owned semantic projection. No platform toolkit, I/O or guest execution.
mod geometry;
pub use geometry::AccessibilityGeometry;
mod scroll;
mod text;
use crate::name::Names;
use crate::style::Environment;
use crate::tree::{Content, Element, Tree};
use crate::{Broker, Input, InputError, NodeIdentity, Target, View};
pub use scroll::{
    AccessibilityScrollAction, AccessibilityScrollHint, AccessibilityScrollOffset,
    AccessibilityScrollUnit,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
pub use text::{
    ACCESSIBILITY_TEXT_RUN_CHARACTERS, AccessibilitySelection, AccessibilityText,
    AccessibilityTextRevision, AccessibilityTextRun,
};
use unicode_segmentation::UnicodeSegmentation;
use varta_common::view::{Attribute as A, EventKind, ROOT_NODE, Tag};

/// Host observer allowance; does not change guest admission or wire budgets.
pub const ACCESSIBILITY_TEXT_WORK_LIMIT: usize = varta_common::view::DRAFT_MAX_NAMING_WORK;
/// Native auxiliary text nodes, separate from guest live-node admission.
pub const ACCESSIBILITY_TEXT_RUN_LIMIT: usize = 4096;

/// Remaining work for one native observer publication, including its source
/// preflight. Pass one mutable allowance through all geometry queries.
#[derive(Debug, Clone)]
pub struct AccessibilityWork {
    remaining: usize,
}
impl Default for AccessibilityWork {
    fn default() -> Self {
        Self {
            remaining: ACCESSIBILITY_TEXT_WORK_LIMIT,
        }
    }
}
impl AccessibilityWork {
    pub(crate) fn charge(&mut self, work: usize) -> Result<(), AccessibilityError> {
        self.remaining = self
            .remaining
            .checked_sub(work)
            .ok_or(AccessibilityError::WorkLimit {
                limit: ACCESSIBILITY_TEXT_WORK_LIMIT,
            })?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AccessibilityError {
    #[error("accessibility text alternatives exceed the observer work limit ({limit})")]
    WorkLimit { limit: usize },
    #[error("accessibility logical text runs exceed the observer limit ({limit})")]
    TextRunLimit { limit: usize },
}

/// A boolean from a native control or a token from an explicit ARIA attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessibilityState {
    Boolean(bool),
    Token(String),
}

/// One visible semantic node. IDs are source node IDs, not platform object IDs.
#[derive(Debug, Clone)]
pub struct AccessibilityNode {
    pub id: u32,
    pub identity: NodeIdentity,
    pub role: String,
    pub author_id: Option<String>,
    pub name: String,
    /// Native name came from contents rather than an author/host label.
    pub name_from_content: bool,
    pub description: Option<String>,
    pub value: Option<String>,
    pub text: Option<AccessibilityText>,
    pub level: Option<u8>,
    pub checked: Option<bool>,
    pub selected: Option<AccessibilityState>,
    pub expanded: Option<String>,
    pub busy: Option<String>,
    pub invalid: Option<String>,
    pub current: Option<String>,
    pub live: Option<String>,
    /// Native atomic defaults for the admitted alert/status roles.
    pub live_atomic: bool,
    pub url: Option<String>,
    pub disabled: bool,
    legacy_disabled: bool,
    pub read_only: bool,
    pub password: bool,
    pub multiline: bool,
    pub modal: bool,
    pub focused: bool,
    pub focusable: bool,
    pub clickable: bool,
    pub editable: bool,
    pub children: Vec<u32>,
}

/// A flat snapshot: dropping deep guest trees never recurses through children.
/// As with View::snapshot, caller-owned copies are the embedder's responsibility.
#[derive(Debug, Clone)]
pub struct AccessibilityTree {
    pub root: u32,
    pub focus: Option<u32>,
    pub nodes: BTreeMap<u32, AccessibilityNode>,
    observer_work: AccessibilityWork,
}

/// Trusted assistive input, using the ordinary focus, activation and edit paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessibilityAction {
    Scroll(AccessibilityScrollAction),
    Focus,
    Click,
    SetValue(String),
    ReplaceSelectedText(String),
    SelectText {
        revision: AccessibilityTextRevision,
        anchor: usize,
        focus: usize,
    },
}

pub(crate) fn role(element: &Element) -> Option<&str> {
    element
        .attributes
        .get(&A::Role)
        .map(String::as_str)
        .or_else(|| {
            varta_common::view::accessibility::implicit_role(
                element.tag,
                element.input_type(),
                element.attributes.contains_key(&A::AriaLabel)
                    || element.attributes.contains_key(&A::AriaLabelledby),
            )
        })
}

struct Interaction {
    disabled: bool,
    focusable: bool,
    clickable: bool,
    editable: bool,
}
fn interaction(tree: &Tree, id: u32, element: &Element) -> Interaction {
    let disabled = element.disabled()
        || element.tag == Tag::Option
            && tree
                .select_owner(id)
                .and_then(|owner| tree.element(owner))
                .is_some_and(Element::disabled);
    Interaction {
        disabled,
        focusable: element.focusable() && !disabled,
        clickable: !disabled
            && (element.checkable()
                || matches!(
                    element.tag,
                    Tag::Button | Tag::A | Tag::Select | Tag::Option
                )
                || element
                    .listeners
                    .iter()
                    .any(|listener| listener.kind == EventKind::Click)),
        editable: element.text_field() && !disabled && !element.readonly(),
    }
}
fn native_container(tree: &Tree, id: u32, element: &Element, environment: &Environment) -> bool {
    element.attributes.contains_key(&A::AriaLive)
        || element.attributes.contains_key(&A::AriaBusy)
        || tree
            .computed_read(id, environment)
            .get(&varta_common::view::CssProperty::Overflow)
            .is_some_and(|value| matches!(value.as_str(), "auto" | "hidden"))
}
fn represented(tree: &Tree, id: u32, native: bool, environment: &Environment) -> bool {
    match &tree.nodes[&id].content {
        Content::Text(text) => !text.trim().is_empty(),
        Content::Element(element) => {
            id == ROOT_NODE
                || role(element).is_some()
                || native
                    && (native_container(tree, id, element, environment)
                        || element.focusable()
                        || element
                            .listeners
                            .iter()
                            .any(|listener| listener.kind == EventKind::Click))
        }
    }
}

fn semantic_node(
    tree: &Tree,
    id: u32,
    names: &Names<'_>,
    native: bool,
    environment: &Environment,
) -> Option<AccessibilityNode> {
    let entry = &tree.nodes[&id];
    let mut node = AccessibilityNode {
        id,
        identity: NodeIdentity {
            id,
            token: entry.identity.clone(),
        },
        role: String::new(),
        author_id: None,
        name: String::new(),
        name_from_content: false,
        description: None,
        value: None,
        text: None,
        level: None,
        checked: None,
        selected: None,
        expanded: None,
        busy: None,
        invalid: None,
        current: None,
        live: None,
        live_atomic: false,
        url: None,
        disabled: false,
        legacy_disabled: false,
        read_only: false,
        password: false,
        multiline: false,
        modal: false,
        focused: tree.focus == Some(id),
        focusable: false,
        clickable: false,
        editable: false,
        children: Vec::new(),
    };
    if id == ROOT_NODE {
        node.role = "application".into();
        if native && let Some(element) = tree.element(id) {
            node.live = element.attributes.get(&A::AriaLive).cloned();
            node.busy = element.attributes.get(&A::AriaBusy).cloned();
        }
        return Some(node);
    }
    let element = match &entry.content {
        Content::Text(text) if text.trim().is_empty() => return None,
        Content::Text(text) => {
            node.role = "text".into();
            node.name = text.clone();
            return Some(node);
        }
        Content::Element(element) => element,
    };
    let semantic_role = role(element);
    if semantic_role.is_none()
        && !(native && native_container(tree, id, element, environment))
        && !element.focusable()
        && !element
            .listeners
            .iter()
            .any(|listener| listener.kind == EventKind::Click)
    {
        return None;
    }
    node.role = semantic_role.unwrap_or("generic").into();
    node.author_id = element.attributes.get(&A::Id).cloned();
    if native {
        (node.name, node.name_from_content) = names.name_with_origin(id, &node.role);
    } else {
        node.name = names.accessible_name(id, &node.role);
    }
    if native {
        node.description = names.description(id);
    }
    node.level = match element.tag {
        Tag::H1 => Some(1),
        Tag::H2 => Some(2),
        Tag::H3 => Some(3),
        _ => None,
    };
    node.password = element.tag == Tag::Input && element.input_type() == "password";
    if element.text_field() || element.tag == Tag::Select {
        node.value = Some(if node.password {
            let masks = if native {
                element.value.graphemes(true).count()
            } else {
                // Preserve the established headless JSON representation.
                element.value.chars().count()
            };
            crate::text::field_display::PASSWORD_MASK.repeat(masks)
        } else {
            element.value.clone()
        });
    }
    if native && element.text_field() {
        node.text = Some(AccessibilityText::new(
            entry,
            element,
            node.value.as_deref().expect("text field value"),
        ));
    }
    node.multiline = element.tag == Tag::Textarea;
    node.checked = element.checkable().then_some(element.checked);
    let interaction = interaction(tree, id, element);
    node.disabled = interaction.disabled;
    node.legacy_disabled = element.disabled();
    node.read_only = element.readonly();
    if element.tag == Tag::Option {
        node.selected = Some(AccessibilityState::Boolean(element.editor.option_selected));
    }
    node.modal = element.tag == Tag::Dialog && element.open;
    node.expanded = element.attributes.get(&A::AriaExpanded).cloned();
    node.busy = element.attributes.get(&A::AriaBusy).cloned();
    node.invalid = element.attributes.get(&A::AriaInvalid).cloned();
    node.current = element.attributes.get(&A::AriaCurrent).cloned();
    node.live = element.attributes.get(&A::AriaLive).cloned();
    if native {
        node.live_atomic = matches!(node.role.as_str(), "alert" | "status");
        if node.live.is_none()
            && !tree.path(id).into_iter().skip(1).any(|ancestor| {
                tree.element(ancestor)
                    .is_some_and(|element| element.attributes.contains_key(&A::AriaLive))
            })
        {
            node.live = match node.role.as_str() {
                "alert" => Some("assertive".into()),
                "status" => Some("polite".into()),
                _ => None,
            };
        }
    }
    node.url = element.attributes.get(&A::Href).cloned();
    if let Some(value) = element.attributes.get(&A::AriaSelected) {
        node.selected = Some(AccessibilityState::Token(value.clone()));
    }
    node.focusable = interaction.focusable;
    node.clickable = interaction.clickable;
    node.editable = interaction.editable;
    Some(node)
}

pub(crate) fn project(tree: &Tree, environment: &Environment) -> AccessibilityTree {
    let names = Names::new(tree, environment);
    let order = visible_order(tree, &names);
    project_impl(tree, &names, order, false, environment)
}

fn visible_order(tree: &Tree, names: &Names<'_>) -> Vec<u32> {
    let mut order = Vec::new();
    let mut pending = vec![ROOT_NODE];
    while let Some(id) = pending.pop() {
        if id != ROOT_NODE && (names.hidden(id) || tree.blocked(id)) {
            continue;
        }
        order.push(id);
        pending.extend(tree.nodes[&id].children.iter().rev());
    }
    order
}

fn observer_preflight(
    tree: &Tree,
    names: &Names<'_>,
    order: &[u32],
    environment: &Environment,
) -> Result<AccessibilityWork, AccessibilityError> {
    let summaries = crate::naming_work::summaries(tree);
    let mut remaining = AccessibilityWork::default();
    let mut text_runs = 0usize;
    let mut charge = |work: usize| -> Result<(), AccessibilityError> { remaining.charge(work) };
    // Full atomic messages include nested live/off content. Charge their
    // possible output/traversal too, separately from guest admission. Root-name
    // costs include external label relations, not merely text descendants.
    let mut message_work = BTreeMap::<u32, usize>::new();
    for &id in order.iter().rev() {
        let mut work = if represented(tree, id, true, environment) {
            summaries
                .root(tree, id)
                .saturating_add(varta_common::view::DRAFT_NAMING_NODE_WORK)
                .saturating_add(
                    if tree
                        .element(id)
                        .is_some_and(|e| e.text_field() || e.tag == Tag::Select)
                    {
                        varta_common::view::DRAFT_FIELD_VALUE_RESERVATION_BYTES
                    } else {
                        0
                    },
                )
        } else {
            0
        };
        for child in &tree.nodes[&id].children {
            work = work
                .saturating_add(message_work.get(child).copied().unwrap_or(0))
                .min(ACCESSIBILITY_TEXT_WORK_LIMIT + 1);
        }
        message_work.insert(id, work);
    }
    for &id in order {
        if !represented(tree, id, true, environment) {
            continue;
        }
        charge(summaries.root(tree, id))?;
        if id != ROOT_NODE
            && tree
                .element(id)
                .and_then(role)
                .is_some_and(|role| matches!(role, "status" | "alert"))
        {
            charge(message_work[&id])?;
        }
        if let Some(field) = tree.element(id).filter(|field| field.text_field()) {
            let lines = text::run_count(&field.value, field.input_type() == "password");
            text_runs = text_runs.saturating_add(lines);
            if text_runs > ACCESSIBILITY_TEXT_RUN_LIMIT {
                return Err(AccessibilityError::TextRunLimit {
                    limit: ACCESSIBILITY_TEXT_RUN_LIMIT,
                });
            }
            charge(
                field.value.len().saturating_add(
                    (field.value.chars().count() + lines + 1)
                        .saturating_mul(varta_common::view::DRAFT_NAMING_NODE_WORK),
                ),
            )?;
        }
        if let Some(refs) = tree
            .element(id)
            .and_then(|e| e.attributes.get(&A::AriaDescribedby))
        {
            charge(refs.len())?;
            for ident in refs.split_ascii_whitespace() {
                charge(varta_common::view::DRAFT_NAMING_NODE_WORK + 1)?;
                if let Some(target) = names.resolve_id(ident) {
                    charge(summaries.descendants[&target])?;
                }
            }
        }
    }
    Ok(remaining)
}

fn project_impl(
    tree: &Tree,
    names: &Names<'_>,
    order: Vec<u32>,
    native: bool,
    environment: &Environment,
) -> AccessibilityTree {
    let mut nodes = BTreeMap::<u32, AccessibilityNode>::new();
    let mut roots = BTreeMap::<u32, Vec<u32>>::new();
    for id in order.into_iter().rev() {
        let nested: Vec<_> = tree.nodes[&id]
            .children
            .iter()
            .flat_map(|child| roots.remove(child).unwrap_or_default())
            .collect();
        if !represented(tree, id, native, environment) {
            roots.insert(id, nested);
            continue;
        }
        let mut node =
            semantic_node(tree, id, names, native, environment).expect("represented semantic node");
        let leaf_named = if native {
            // Collapse text only when the effective semantic role takes its
            // name from content. Tag-based folding loses overridden-role text
            // and duplicates names on author-assigned content roles.
            varta_common::view::accessibility::NAME_FROM_CONTENT.contains(&node.role.as_str())
        } else {
            // Preserve the frozen headless projection, including old folding.
            tree.element(id).is_some_and(|element| {
                matches!(
                    element.tag,
                    Tag::Button | Tag::A | Tag::Option | Tag::H1 | Tag::H2 | Tag::H3
                )
            })
        };
        if leaf_named && nested.iter().all(|child| nodes[child].role == "text") {
            for child in nested {
                nodes.remove(&child);
            }
        } else {
            node.children = nested;
        }
        nodes.insert(id, node);
        roots.insert(id, vec![id]);
    }
    AccessibilityTree {
        root: ROOT_NODE,
        focus: tree.focus.filter(|id| nodes.contains_key(id)),
        nodes,
        observer_work: AccessibilityWork::default(),
    }
}

impl AccessibilityTree {
    /// Start platform projection with the allowance left by native preflight,
    /// rather than granting another full allowance for each field.
    #[must_use]
    pub fn observer_work(&self) -> AccessibilityWork {
        self.observer_work.clone()
    }
    /// Established headless JSON shape; platform identity and action hints stay private.
    pub(crate) fn snapshot(&self) -> Value {
        let mut order = Vec::new();
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            order.push(id);
            pending.extend(self.nodes[&id].children.iter().rev());
        }
        let mut built = BTreeMap::<u32, Value>::new();
        for id in order.into_iter().rev() {
            let node = &self.nodes[&id];
            let children = node
                .children
                .iter()
                .map(|id| built.remove(id).unwrap())
                .collect();
            let mut out = Map::new();
            out.insert("role".into(), json!(node.role));
            if id == self.root {
                out.insert("children".into(), Value::Array(children));
            } else {
                if !node.name.is_empty() {
                    out.insert("name".into(), json!(node.name));
                }
                if let Some(value) = &node.value {
                    out.insert("value".into(), json!(value));
                }
                if let Some(value) = node.level {
                    out.insert("level".into(), json!(value));
                }
                if let Some(value) = node.checked {
                    out.insert("checked".into(), json!(value));
                }
                if let Some(value) = &node.selected {
                    out.insert(
                        "selected".into(),
                        match value {
                            AccessibilityState::Boolean(value) => json!(value),
                            AccessibilityState::Token(value) => json!(value),
                        },
                    );
                }
                if node.legacy_disabled {
                    out.insert("disabled".into(), json!(true));
                }
                for (key, present) in [
                    ("multiline", node.multiline),
                    ("modal", node.modal),
                    ("focused", node.focused),
                ] {
                    if present {
                        out.insert(key.into(), json!(true));
                    }
                }
                for (key, value) in [
                    ("expanded", &node.expanded),
                    ("busy", &node.busy),
                    ("invalid", &node.invalid),
                    ("current", &node.current),
                    ("live", &node.live),
                    ("url", &node.url),
                ] {
                    if let Some(value) = value {
                        out.insert(key.into(), json!(value));
                    }
                }
                if !node.children.is_empty() {
                    out.insert("children".into(), Value::Array(children));
                }
            }
            built.insert(id, Value::Object(out));
        }
        built.remove(&self.root).expect("projected root")
    }
}

impl View {
    /// Build complete native text alternatives after a bounded observer preflight.
    /// Refusal leaves guest state and the established headless snapshot unchanged.
    pub fn accessibility_tree(&self) -> Result<AccessibilityTree, AccessibilityError> {
        let names = Names::native(&self.tree, &self.environment);
        let order = visible_order(&self.tree, &names);
        let observer_work = observer_preflight(&self.tree, &names, &order, &self.environment)?;
        let mut projected = project_impl(&self.tree, &names, order, true, &self.environment);
        projected.observer_work = observer_work;
        Ok(projected)
    }

    /// Ignore stale, hidden, disabled or unsupported requests before any effect.
    /// Accepted requests follow ordinary trusted input limits and event delivery.
    pub fn accessibility_input(
        &mut self,
        identity: &NodeIdentity,
        action: &AccessibilityAction,
        broker: &mut dyn Broker,
    ) -> Result<bool, InputError> {
        let Some(id) = self.resolve_node_identity(identity) else {
            return Ok(false);
        };
        let names = Names::new(&self.tree, &self.environment);
        if self.tree.inert(id) || self.tree.path(id).into_iter().any(|id| names.hidden(id)) {
            return Ok(false);
        }
        let Some(element) = self.tree.element(id) else {
            return Ok(false);
        };
        // Action validation never evaluates an accessible name or description.
        let node = interaction(&self.tree, id, element);
        let target = Target::Node(id);
        let input = match action {
            AccessibilityAction::SelectText {
                revision,
                anchor,
                focus,
            } if element.text_field()
                && !element.disabled()
                && element.editor.preedit.is_none() =>
            {
                let current = AccessibilityTextRevision::new(&self.tree.nodes[&id], element);
                let password = element.input_type() == "password";
                if *revision != current
                    || !text::boundary(&element.value, *anchor, password)
                    || !text::boundary(&element.value, *focus, password)
                {
                    return Ok(false);
                }
                Input::TextSelect {
                    target,
                    anchor: *anchor,
                    focus: *focus,
                    activate: false,
                }
            }
            AccessibilityAction::Focus if node.focusable => Input::Focus { target },
            AccessibilityAction::Click if node.clickable => {
                if self.tree.element(id).is_some_and(|e| e.tag == Tag::Option) {
                    let Some(owner) = self.tree.select_owner(id) else {
                        return Ok(false);
                    };
                    Input::SelectOption {
                        target: Target::Node(owner),
                        option: id,
                    }
                } else {
                    Input::Click { target }
                }
            }
            AccessibilityAction::SetValue(text)
            | AccessibilityAction::ReplaceSelectedText(text)
                if node.editable =>
            {
                if text
                    .encode_utf16()
                    .take(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
                    .count()
                    > varta_common::view::DRAFT_MAX_VALUE_BYTES / 2
                {
                    return Ok(false);
                }
                if matches!(action, AccessibilityAction::SetValue(_)) {
                    let end = self.tree.element(id).expect("validated field").value.len();
                    self.input(
                        &Input::TextSelect {
                            target: target.clone(),
                            anchor: 0,
                            focus: end,
                            activate: false,
                        },
                        broker,
                    )?;
                }
                Input::Type {
                    target,
                    text: text.clone(),
                }
            }
            _ => return Ok(false),
        };
        self.input(&input, broker)?;
        Ok(true)
    }
}

#[cfg(test)]
mod work_tests {
    use super::*;
    #[test]
    fn native_preflight_cost_is_carried_into_platform_geometry_allowance() {
        let source = View::default().accessibility_tree().unwrap();
        let mut work = source.observer_work();
        assert_eq!(
            work.charge(ACCESSIBILITY_TEXT_WORK_LIMIT),
            Err(AccessibilityError::WorkLimit {
                limit: ACCESSIBILITY_TEXT_WORK_LIMIT
            })
        );
        // A failed projection and a fresh publication have independent copies;
        // neither mutates the established semantic snapshot or guest model.
        assert!(source.observer_work().charge(1).is_ok());
    }
}
