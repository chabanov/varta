//! The simulated platform side: trusted inputs, event dispatch (bubbling,
//! key filters, `preventDefault`/`stopPropagation`), default actions, and
//! the clipboard and system browser of the headless renderer.
use crate::tree::{Element, focus_records, uncheck_group};
use crate::{Access, Broker, EventRecord, InputError, View, ViewError};
use serde::Deserialize;
use varta_common::view::{
    ANY_KEY, Attribute, CommandStatus, EventFlag, EventKind, Key, ListenOption, MODIFIER_MASK,
    ResultOperation, Tag,
};

/// How the simulated permission prompt answers (headless: scripted; with no
/// answer configured an activation that needs a prompt is refused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptAnswer {
    Allow,
    Deny,
    Dismiss,
    #[default]
    None,
}

/// The headless platform's behaviour: scripted prompt answers and refusals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Platform {
    #[serde(default)]
    pub prompt: PromptAnswer,
    /// The clipboard write fails (`refused_by_platform`).
    #[serde(default)]
    pub clipboard_fails: bool,
    /// The system browser cannot be launched (`refused_by_platform`).
    #[serde(default)]
    pub browser_fails: bool,
}

/// What the simulated platform did: the clipboard, opened URLs, prompts.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Effects {
    pub clipboard: Option<String>,
    pub opened: Vec<String>,
    pub prompts: usize,
}

impl View {
    pub(crate) fn prompt_allows(&mut self) -> bool {
        self.effects.prompts += 1;
        self.platform.prompt == PromptAnswer::Allow
    }
    pub(crate) fn write_clipboard(&mut self, text: &str) -> CommandStatus {
        if self.platform.clipboard_fails {
            return CommandStatus::RefusedByPlatform;
        }
        self.effects.clipboard = Some(text.into());
        CommandStatus::Ok
    }
    fn open(&mut self, url: &str) -> CommandStatus {
        if self.platform.browser_fails {
            return CommandStatus::RefusedByPlatform;
        }
        self.effects.opened.push(url.into());
        CommandStatus::Ok
    }
}

/// A node an input addresses: a node id or `#` + an `id` attribute value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Node(u32),
    Ident(String),
}
impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(id) => write!(f, "{id}"),
            Self::Ident(ident) => write!(f, "{ident}"),
        }
    }
}

/// One trusted platform input of a headless script.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "input", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    /// A pointer activation of the element.
    Click {
        target: Target,
    },
    /// One edit that inserts `text` at the end of a text field.
    Type {
        target: Target,
        text: String,
    },
    /// A key press on the element (focused first).
    Key {
        target: Target,
        key: String,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        meta: bool,
        #[serde(default)]
        composing: bool,
    },
    Focus {
        target: Target,
    },
    /// Choose the option with `value` in a `select`.
    Select {
        target: Target,
        value: String,
    },
    Scroll {
        target: Target,
        top: f64,
    },
    /// Change how later prompts answer.
    Prompts {
        answer: PromptAnswer,
    },
}

/// Why a headless script was rejected.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("view script exceeds {} bytes", Script::MAX_BYTES)]
    TooLarge,
    #[error("view script has more than {} inputs", Script::MAX_INPUTS)]
    TooManyInputs,
    #[error("invalid view script: {0}")]
    Json(serde_json::Error),
}

/// A headless script: viewport, platform behaviour and inputs in order.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    #[serde(default)]
    pub viewport: Option<Viewport>,
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub inputs: Vec<Input>,
}
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub width: f64,
    #[serde(default)]
    pub dark: bool,
}
impl Script {
    /// Limits of a script file and of its inputs.
    pub const MAX_BYTES: usize = 1024 * 1024;
    pub const MAX_INPUTS: usize = 4096;
    /// Parse a bounded script.
    pub fn parse(bytes: &[u8]) -> Result<Self, ScriptError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(ScriptError::TooLarge);
        }
        let script: Self = serde_json::from_slice(bytes).map_err(ScriptError::Json)?;
        if script.inputs.len() > Self::MAX_INPUTS {
            return Err(ScriptError::TooManyInputs);
        }
        Ok(script)
    }
    #[must_use]
    pub fn environment(&self) -> crate::Environment {
        self.viewport
            .map_or_else(crate::Environment::default, |v| crate::Environment {
                width: v.width,
                dark: v.dark,
            })
    }
}

/// Modifier flags of a key input.
#[derive(Clone, Copy, Default)]
struct Modifiers {
    flags: u32,
}

impl View {
    /// Perform one trusted platform input: update host-owned state (focus,
    /// field buffers, edit counters), dispatch its events and run the
    /// default actions that were not prevented.
    pub fn input(&mut self, input: &Input, broker: &mut dyn Broker) -> Result<(), InputError> {
        match input {
            Input::Click { target } => {
                let node = self.resolve(target)?;
                self.click(node, broker)?;
            }
            Input::Type { target, text } => {
                let node = self.resolve(target)?;
                self.focus_input(node)?;
                self.edit(node, |value| value.push_str(text))?;
            }
            Input::Key {
                target,
                key,
                shift,
                ctrl,
                alt,
                meta,
                composing,
            } => {
                let node = self.resolve(target)?;
                let key = Key::ALL
                    .iter()
                    .copied()
                    .find(|candidate| candidate.name() == key || candidate.dom_key() == key)
                    .ok_or_else(|| InputError::Key(key.clone()))?;
                let mut flags = 0;
                for (held, flag) in [
                    (*shift, EventFlag::Shift),
                    (*ctrl, EventFlag::Ctrl),
                    (*alt, EventFlag::Alt),
                    (*meta, EventFlag::Meta),
                    (*composing, EventFlag::Composing),
                ] {
                    if held {
                        flags |= flag as u32;
                    }
                }
                self.focus_input(node)?;
                self.key(node, key, Modifiers { flags }, broker)?;
            }
            Input::Focus { target } => {
                let node = self.resolve(target)?;
                self.focus_input(node)?;
            }
            Input::Select { target, value } => {
                let node = self.resolve(target)?;
                if self
                    .tree
                    .element(node)
                    .is_some_and(|e| e.tag == Tag::Select)
                {
                    self.focus_input(node)?;
                    let value = value.clone();
                    self.edit(node, move |current| *current = value)?;
                    self.fire(node, EventKind::Change, true)?;
                }
            }
            Input::Scroll { target, top } => {
                let node = self.resolve(target)?;
                if let Some(element) = self.tree.element_mut(node) {
                    element.scroll_top = top.max(0.0);
                }
                self.fire(node, EventKind::Scroll, true)?;
            }
            Input::Prompts { answer } => self.platform.prompt = *answer,
        }
        Ok(())
    }

    fn focus_input(&mut self, node: u32) -> Result<(), ViewError> {
        let focusable = self
            .tree
            .element(node)
            .is_some_and(|element| element.focusable() && !element.disabled());
        if focusable {
            for record in focus_records(&mut self.tree, Some(node)) {
                self.dispatch(record, true)?;
            }
        }
        Ok(())
    }

    /// One local edit of a field: apply it, bump the edit counter and fire
    /// `input`. Disabled and read-only fields take no edits.
    fn edit(&mut self, node: u32, change: impl FnOnce(&mut String)) -> Result<(), ViewError> {
        let Some(element) = self.tree.element_mut(node) else {
            return Ok(());
        };
        if !(element.text_field() || element.tag == Tag::Select)
            || element.disabled()
            || element.readonly()
        {
            return Ok(());
        }
        change(&mut element.value);
        if let Some(limit) = element
            .attributes
            .get(&Attribute::Maxlength)
            .and_then(|limit| limit.parse::<usize>().ok())
        {
            let units: Vec<u16> = element.value.encode_utf16().take(limit).collect();
            element.value = String::from_utf16_lossy(&units);
        }
        element.seq = element.seq.wrapping_add(1);
        self.fire(node, EventKind::Input, true)
    }

    /// Fire a host event at `node` with the field state as its payload.
    fn fire(&mut self, node: u32, kind: EventKind, trusted: bool) -> Result<(), ViewError> {
        let mut record = EventRecord::occurrence(kind, node);
        if let Some(element) = self.tree.element(node) {
            record.seq = element.seq;
            if element.checked {
                record.flags |= EventFlag::Checked as u32;
            }
            if matches!(kind, EventKind::Input | EventKind::Change) {
                record.value = if element.checkable() {
                    String::new()
                } else {
                    element.value.clone()
                };
            }
            if kind == EventKind::Scroll {
                record.scroll_top = element.scroll_top;
            }
        }
        self.dispatch(record, trusted).map(|_| ())
    }

    /// Deliver an occurrence along its path, target first. Returns whether a
    /// delivered listener asked for `preventDefault`.
    pub(crate) fn dispatch(
        &mut self,
        occurrence: EventRecord,
        trusted: bool,
    ) -> Result<bool, ViewError> {
        let mut prevented = false;
        let path = self.tree.path(occurrence.target);
        let mut deliveries = Vec::new();
        'path: for node in path {
            let Some(element) = self.tree.element(node) else {
                continue;
            };
            for listener in &element.listeners {
                if listener.kind != occurrence.kind || !matches_filter(listener, &occurrence) {
                    continue;
                }
                let mut record = occurrence.clone();
                record.handler = listener.handler;
                deliveries.push(record);
                prevented |= listener.filter.options & ListenOption::PreventDefault as u32 != 0;
                if listener.filter.options & ListenOption::StopPropagation as u32 != 0 {
                    break 'path;
                }
            }
        }
        for record in deliveries {
            self.enqueue(record, trusted)?;
        }
        Ok(prevented)
    }

    fn click(&mut self, node: u32, broker: &mut dyn Broker) -> Result<(), ViewError> {
        if self.tree.element(node).is_some_and(Element::disabled) {
            return Ok(());
        }
        self.focus_input(node)?;
        let prevented = self.dispatch(EventRecord::occurrence(EventKind::Click, node), true)?;
        if prevented {
            return Ok(());
        }
        self.activate(node, broker)
    }

    /// The default action of an activation: toggle a checkable input, submit
    /// the enclosing form from a submit button, open an external link, or
    /// forward a label's activation to its control.
    fn activate(&mut self, node: u32, broker: &mut dyn Broker) -> Result<(), ViewError> {
        let Some(element) = self.tree.element(node).cloned() else {
            return Ok(());
        };
        if element.checkable() {
            let radio = element.input_type() == "radio";
            if radio && element.checked {
                return Ok(());
            }
            if let Some(target) = self.tree.element_mut(node) {
                target.checked = !target.checked;
                target.seq = target.seq.wrapping_add(1);
            }
            if radio {
                let name = element.attributes.get(&Attribute::Name).cloned();
                uncheck_group(&mut self.tree, node, name.as_deref());
            }
            self.fire(node, EventKind::Input, true)?;
            return self.fire(node, EventKind::Change, true);
        }
        if element.tag == Tag::Button
            && element.attributes.get(&Attribute::Type).map(String::as_str) != Some("button")
            && let Some(form) = self.ancestor(node, Tag::Form)
        {
            return self.fire(form, EventKind::Submit, true);
        }
        if element.tag == Tag::Label
            && let Some(control) = element
                .attributes
                .get(&Attribute::For)
                .and_then(|ident| self.tree.by_ident(ident))
        {
            return self.click(control, broker);
        }
        if let Some(link) = self.ancestor(node, Tag::A)
            && let Some(href) = self
                .tree
                .element(link)
                .and_then(|a| a.attributes.get(&Attribute::Href))
                .cloned()
            && varta_common::view::attributes::external(&href)
        {
            let status = match broker.open_url(&href) {
                Access::Granted => self.open(&href),
                Access::Prompt if self.prompt_allows() => self.open(&href),
                Access::Prompt | Access::Denied => CommandStatus::NotAllowed,
            };
            let mut record = EventRecord::new(EventKind::CommandResult, 0, link);
            record.status = status.code();
            record.key = ResultOperation::OpenUrl.name().into();
            self.enqueue(record, true)?;
        }
        Ok(())
    }

    /// The nearest element with `tag` on the path of `node`, `node` included.
    fn ancestor(&self, node: u32, tag: Tag) -> Option<u32> {
        self.tree
            .path(node)
            .into_iter()
            .find(|id| self.tree.element(*id).is_some_and(|e| e.tag == tag))
    }

    fn key(
        &mut self,
        node: u32,
        key: Key,
        modifiers: Modifiers,
        broker: &mut dyn Broker,
    ) -> Result<(), ViewError> {
        let mut occurrence = EventRecord::occurrence(EventKind::Keydown, node);
        occurrence.flags = modifiers.flags;
        occurrence.key = key.dom_key().into();
        if let Some(element) = self.tree.element(node) {
            occurrence.seq = element.seq;
        }
        let prevented = self.dispatch(occurrence, true)?;
        if prevented || modifiers.flags & EventFlag::Composing as u32 != 0 {
            return Ok(());
        }
        let Some(element) = self.tree.element(node).cloned() else {
            return Ok(());
        };
        match key {
            Key::Enter if element.tag == Tag::Textarea => self.edit(node, |value| value.push('\n')),
            Key::Enter if element.text_field() => match self.ancestor(node, Tag::Form) {
                Some(form) => self.fire(form, EventKind::Submit, true),
                None => Ok(()),
            },
            Key::Enter | Key::Space if element.tag == Tag::Button => self.click(node, broker),
            Key::Enter if element.tag == Tag::A => self.click(node, broker),
            Key::Space if element.checkable() => self.click(node, broker),
            Key::Backspace if element.text_field() => self.edit(node, |value| {
                value.pop();
            }),
            Key::Escape => self.escape(node),
            Key::Tab => self.tab(node, modifiers.flags & EventFlag::Shift as u32 != 0),
            _ => Ok(()),
        }
    }

    /// Esc in an open modal dialog: `cancel`, then close unless prevented.
    fn escape(&mut self, node: u32) -> Result<(), ViewError> {
        let Some(dialog) = self.ancestor(node, Tag::Dialog).or_else(|| {
            self.tree.document_order().into_iter().find(|id| {
                self.tree
                    .element(*id)
                    .is_some_and(|e| e.tag == Tag::Dialog && e.open)
            })
        }) else {
            return Ok(());
        };
        if !self.tree.element(dialog).is_some_and(|e| e.open) {
            return Ok(());
        }
        let prevented = self.dispatch(EventRecord::occurrence(EventKind::Cancel, dialog), true)?;
        if prevented {
            return Ok(());
        }
        if let Some(element) = self.tree.element_mut(dialog) {
            element.open = false;
        }
        self.fire(dialog, EventKind::Close, true)
    }

    /// Tab follows document order over focusable, enabled elements.
    fn tab(&mut self, node: u32, backwards: bool) -> Result<(), ViewError> {
        let mut order: Vec<u32> = self
            .tree
            .document_order()
            .into_iter()
            .filter(|id| {
                self.tree.element(*id).is_some_and(|e| {
                    e.focusable()
                        && !e.disabled()
                        && e.attributes.get(&Attribute::Tabindex).map(String::as_str) != Some("-1")
                })
            })
            .collect();
        if backwards {
            order.reverse();
        }
        let next = order
            .iter()
            .position(|id| *id == node)
            .and_then(|at| order.get(at + 1))
            .or_else(|| order.first())
            .copied();
        if let Some(next) = next {
            self.focus_input(next)?;
        }
        Ok(())
    }
}

/// A `keydown` listener's filter against an occurrence; every other kind's
/// filter is the any-key filter by construction.
fn matches_filter(listener: &crate::tree::Listener, occurrence: &EventRecord) -> bool {
    if listener.kind != EventKind::Keydown {
        return true;
    }
    let filter = listener.filter;
    let held = occurrence.flags & MODIFIER_MASK;
    let key_matches = filter.key == ANY_KEY
        || Key::from_code(filter.key).is_some_and(|key| key.dom_key() == occurrence.key);
    let composing = occurrence.flags & EventFlag::Composing as u32 != 0;
    key_matches
        && held & filter.required == filter.required
        && held & filter.forbidden == 0
        && !(composing && filter.options & ListenOption::NotComposing as u32 != 0)
}

impl EventRecord {
    /// An event at `target` before dispatch (no listener yet).
    pub(crate) fn occurrence(kind: EventKind, target: u32) -> Self {
        Self::new(kind, 0, target)
    }
}
