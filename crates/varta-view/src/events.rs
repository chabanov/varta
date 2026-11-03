//! The simulated platform side: trusted inputs, event dispatch (bubbling,
//! key filters, `preventDefault`/`stopPropagation`), default actions, and
//! the clipboard and system browser of the headless renderer.
use crate::tree::{Element, Preedit, focus_records};
use crate::{Access, Broker, EventRecord, InputError, View, ViewError};
use serde::Deserialize;
use unicode_segmentation::UnicodeSegmentation;
use varta_common::scheduler::TIMER_MAX_DELAY_MS;
use varta_common::view::{
    ANY_KEY, Attribute, CommandStatus, DRAFT_MAX_VALUE_BYTES, EventFlag, EventKind, Key,
    ListenOption, MODIFIER_MASK, ResultOperation, Tag,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAction {
    Undo,
    Redo,
}

#[derive(Debug, Clone)]
struct EditSnapshot {
    value: String,
    anchor: usize,
    focus: usize,
    preferred_x: Option<f32>,
    upstream: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryEntry {
    target: crate::NodeIdentity,
    epoch: std::sync::Arc<()>,
    before: EditSnapshot,
    after: EditSnapshot,
    undone: bool,
}

// Shared by all fields in a view, including snapshots awaiting redo.
const MAX_HISTORY_BYTES: usize = 1024 * 1024;
const MAX_HISTORY_EDITS: usize = 64;

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
    pub opened: std::collections::VecDeque<String>,
    opened_bytes: usize,
    pub opened_dropped: u64,
    pub prompts: usize,
}

impl Effects {
    fn opened(&mut self, url: &str) {
        use varta_common::view::{
            DRAFT_MAX_OPENED_URL_BYTES as BYTES, DRAFT_MAX_OPENED_URLS as COUNT,
        };
        if url.len() > BYTES {
            self.opened_dropped = self.opened_dropped.saturating_add(1);
            return;
        }
        while self.opened.len() >= COUNT || self.opened_bytes + url.len() > BYTES {
            let previous = self
                .opened
                .pop_front()
                .expect("retained opens exceed a nonzero allowance");
            self.opened_bytes -= previous.len();
            self.opened_dropped = self.opened_dropped.saturating_add(1);
        }
        self.opened_bytes += url.len();
        self.opened.push_back(url.into());
    }
}

fn previous_grapheme(value: &str, offset: usize) -> usize {
    value
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .take_while(|index| *index < offset)
        .last()
        .unwrap_or(0)
}

fn next_grapheme(value: &str, offset: usize) -> usize {
    value
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .find(|index| *index > offset)
        .unwrap_or(value.len())
}

fn insert_text(
    value: &mut String,
    anchor: &mut usize,
    focus: &mut usize,
    text: &str,
    limit: Option<usize>,
) {
    let start = (*anchor).min(*focus);
    let end = (*anchor).max(*focus);
    let retained = value[..start].encode_utf16().count() + value[end..].encode_utf16().count();
    let available = limit.unwrap_or(usize::MAX).saturating_sub(retained);
    let mut units = 0usize;
    let accepted: String = text
        .chars()
        .take_while(|ch| {
            units = units.saturating_add(ch.len_utf16());
            units <= available
        })
        .collect();
    if accepted.is_empty() && !text.is_empty() {
        return;
    }
    value.replace_range(start..end, &accepted);
    *focus = start + accepted.len();
    *anchor = *focus;
}

impl View {
    /// Release history for removed fields and guest-replaced values after commit.
    pub(crate) fn prune_history(&mut self) {
        let tree = &self.tree;
        self.history.retain(|entry| {
            tree.nodes.get(&entry.target.id).is_some_and(|node| {
                std::sync::Arc::ptr_eq(&node.identity, &entry.target.token)
                    && tree.element(entry.target.id).is_some_and(|element| {
                        std::sync::Arc::ptr_eq(&element.editor.history_epoch, &entry.epoch)
                    })
            })
        });
    }

    pub(crate) fn prompt_allows(&mut self) -> bool {
        self.effects.prompts = self.effects.prompts.saturating_add(1);
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
        self.effects.opened(url);
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
    /// Activate a text field with a selection expressed as UTF-8 byte offsets.
    TextSelect {
        target: Target,
        anchor: usize,
        focus: usize,
        activate: bool,
    },
    /// A frame-derived visual movement, applied only if keydown is not prevented.
    TextNavigate {
        target: Target,
        key: String,
        position: usize,
        preferred_x: Option<f32>,
        #[serde(default)]
        upstream: bool,
        #[serde(default)]
        shift: bool,
    },
    /// Replace the current transient IME preedit without changing field value.
    ImePreedit {
        target: Target,
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// Commit IME text into the current selection as one input edit.
    ImeCommit {
        target: Target,
        text: String,
    },
    /// One edit that replaces the current selection with `text`.
    Type {
        target: Target,
        text: String,
    },
    /// A character keydown followed by default insertion or select typeahead.
    /// A select can use the logical key when no text was committed; text fields
    /// still require `text` for insertion.
    TextKey {
        target: Target,
        key: String,
        text: String,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        meta: bool,
        /// Host shortcut mapping for the physical select-all key.
        #[serde(default)]
        select_all: bool,
        #[serde(default)]
        history: Option<HistoryAction>,
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
    /// Pick a concrete option, including when several options have equal values.
    SelectOption {
        target: Target,
        option: u32,
    },
    /// Dismiss the native list without changing the committed selection.
    SelectClose {
        target: Target,
    },
    /// Native list scrolling does not generate a guest scroll event.
    SelectScroll {
        target: Target,
        first: usize,
    },
    Scroll {
        target: Target,
        top: f64,
    },
    /// A scroll from measured host geometry, all values in CSS pixels.
    ScrollMeasured {
        target: Target,
        top: f64,
        scroll_height: f64,
        client_height: f64,
    },
    /// Change how later prompts answer.
    Prompts {
        answer: PromptAnswer,
    },
}

/// One step of a headless script: a trusted platform input, or a `wait`
/// that advances the session's virtual clock (`{"input":"wait","ms":N}`).
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptStep {
    Input(Input),
    /// Advance virtual loop time by `ms` milliseconds, firing every timer due
    /// by then, each as its own turn.
    Wait {
        ms: f64,
    },
    /// Wait at most `ms` wall-clock milliseconds for armed network work to
    /// settle. Virtual loop time stays fixed during this explicit step.
    WaitNetwork {
        ms: f64,
    },
}

/// The control steps that share the `input` tag with [`Input`].
#[derive(Deserialize)]
#[serde(tag = "input", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Wait { ms: f64 },
    WaitNetwork { ms: f64 },
}
/// The `input` tag of a wait step.
const WAIT_STEP: &str = "wait";
const NETWORK_WAIT_STEP: &str = "wait_network";

impl<'de> Deserialize<'de> for ScriptStep {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(deserializer)?;
        if matches!(
            value.get("input").and_then(serde_json::Value::as_str),
            Some(WAIT_STEP | NETWORK_WAIT_STEP)
        ) {
            return Control::deserialize(value)
                .map(|control| match control {
                    Control::Wait { ms } => Self::Wait { ms },
                    Control::WaitNetwork { ms } => Self::WaitNetwork { ms },
                })
                .map_err(D::Error::custom);
        }
        Input::deserialize(value)
            .map(Self::Input)
            .map_err(D::Error::custom)
    }
}

/// Why a headless script was rejected.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("view script exceeds {} bytes", Script::MAX_BYTES)]
    TooLarge,
    #[error("view script has more than {} inputs", Script::MAX_INPUTS)]
    TooManyInputs,
    #[error(
        "view script step {step} waits {ms} ms; a wait is a number of milliseconds in 0..={}",
        TIMER_MAX_DELAY_MS
    )]
    Wait { step: usize, ms: f64 },
    #[error(
        "view script step {step} network wait {ms} ms is outside 0 < ms <= {}",
        Script::MAX_NETWORK_WAIT_MS
    )]
    NetworkWait { step: usize, ms: f64 },
    #[error("invalid view script: {0}")]
    Json(serde_json::Error),
}

/// A headless script: viewport, platform behaviour and its steps (inputs
/// and waits) in order.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    #[serde(default)]
    pub viewport: Option<Viewport>,
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub inputs: Vec<ScriptStep>,
}
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub width: f64,
    #[serde(default)]
    pub dark: bool,
}
impl Script {
    /// Limits of a script file and of its steps (inputs and waits).
    pub const MAX_BYTES: usize = 1024 * 1024;
    pub const MAX_INPUTS: usize = 4096;
    /// The most turns one step may take to settle (the tasks runnable before
    /// an input, or the timers a wait reaches); more fails the session, so a
    /// timer that keeps re-arming with the minimum delay cannot hang it.
    pub const MAX_STEP_TURNS: u64 = 100_000;
    /// A network wait's maximum wall-clock bound; virtual waits keep their
    /// independent timer-delay bound. This does not raise execution budgets.
    pub const MAX_NETWORK_WAIT_MS: f64 = 5_000.0;
    /// Parse a bounded script.
    pub fn parse(bytes: &[u8]) -> Result<Self, ScriptError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(ScriptError::TooLarge);
        }
        let script: Self = serde_json::from_slice(bytes).map_err(ScriptError::Json)?;
        script.validate()?;
        Ok(script)
    }
    /// Validate parsed or programmatically constructed control steps before
    /// starting guest execution. Byte-size validation belongs to `parse`.
    pub fn validate(&self) -> Result<(), ScriptError> {
        if self.inputs.len() > Self::MAX_INPUTS {
            return Err(ScriptError::TooManyInputs);
        }
        for (step, entry) in self.inputs.iter().enumerate() {
            if let ScriptStep::Wait { ms } = *entry
                && !(0.0..=TIMER_MAX_DELAY_MS).contains(&ms)
            {
                return Err(ScriptError::Wait { step: step + 1, ms });
            }
            if let ScriptStep::WaitNetwork { ms } = *entry
                && !(ms > 0.0 && ms <= Self::MAX_NETWORK_WAIT_MS)
            {
                return Err(ScriptError::NetworkWait { step: step + 1, ms });
            }
        }
        Ok(())
    }
    #[must_use]
    pub fn environment(&self) -> crate::Environment {
        self.viewport
            .map_or_else(crate::Environment::default, |v| crate::Environment {
                width: v.width,
                dark: v.dark,
                ..Default::default()
            })
    }
}

#[cfg(test)]
mod script_tests {
    use super::*;

    #[test]
    fn oversized_open_diagnostics_preserve_retained_entries_and_saturate_counters() {
        let mut effects = Effects::default();
        effects.opened("https://example.test/kept");
        effects.opened(&"x".repeat(varta_common::view::DRAFT_MAX_OPENED_URL_BYTES + 1));
        assert_eq!(effects.opened.len(), 1);
        assert_eq!(effects.opened_dropped, 1);
        assert_eq!(effects.opened_bytes, "https://example.test/kept".len());
        effects.opened_dropped = u64::MAX;
        effects.opened(&"x".repeat(varta_common::view::DRAFT_MAX_OPENED_URL_BYTES + 1));
        assert_eq!(effects.opened_dropped, u64::MAX);
        let mut view = View::default();
        view.effects.prompts = usize::MAX;
        assert!(!view.prompt_allows());
        assert_eq!(view.effects.prompts, usize::MAX);
    }

    struct Deny;
    impl Broker for Deny {
        fn link_requested(&self, _: &str) -> bool {
            false
        }
        fn open_url(&mut self, _: &str) -> crate::Access {
            crate::Access::Denied
        }
        fn clipboard_write(&mut self) -> crate::Access {
            crate::Access::Denied
        }
    }

    #[test]
    fn committed_value_changes_and_removal_release_history_but_failed_batches_do_not() {
        let mut view = View::default();
        let mut setup = crate::BatchWriter::new();
        setup.create_element(2, Tag::Input).insert(1, 2, 0);
        view.commit(&setup.finish(), false, &mut Deny).unwrap();
        let edit = Input::Type {
            target: Target::Node(2),
            text: "private".into(),
        };
        view.input(&edit, &mut Deny).unwrap();
        assert_eq!(view.history.len(), 1);
        let mut invalid = crate::BatchWriter::new();
        invalid
            .remove(2)
            .command(2, varta_common::view::Command::Focus, 0.0);
        assert!(view.commit(&invalid.finish(), false, &mut Deny).is_err());
        assert_eq!(view.history.len(), 1);
        let mut replace = crate::BatchWriter::new();
        replace.set_value(2, "replacement", 1);
        view.commit(&replace.finish(), false, &mut Deny).unwrap();
        assert!(view.history.is_empty());
        view.input(&edit, &mut Deny).unwrap();
        assert_eq!(view.history.len(), 1);
        let mut remove = crate::BatchWriter::new();
        remove.remove(2);
        view.commit(&remove.finish(), false, &mut Deny).unwrap();
        assert!(view.history.is_empty());
    }

    #[test]
    fn undo_history_evicts_large_old_edits_within_the_shared_byte_limit() {
        let mut view = View::default();
        let mut batch = crate::BatchWriter::new();
        batch
            .create_element(2, Tag::Input)
            .insert(1, 2, 0)
            .set_value(2, &"a".repeat(32_000), 0);
        view.commit(&batch.finish(), false, &mut Deny).unwrap();
        for _ in 0..32 {
            view.input(
                &Input::Type {
                    target: Target::Node(2),
                    text: "a".into(),
                },
                &mut Deny,
            )
            .unwrap();
        }
        for _ in 0..32 {
            view.restore_history(2, HistoryAction::Undo).unwrap();
        }
        assert_eq!(view.tree.element(2).unwrap().value.len(), 32_016);
    }

    #[test]
    fn ime_rejected_overflow_and_commit_clear_previous_preedit() {
        let mut view = View::default();
        let mut batch = crate::BatchWriter::new();
        let value = "a".repeat(DRAFT_MAX_VALUE_BYTES / 2 - 1);
        batch
            .create_element(2, Tag::Input)
            .insert(1, 2, 0)
            .set_value(2, &value, 0);
        view.commit(&batch.finish(), false, &mut Deny).unwrap();
        let target = Target::Node(2);
        view.input(
            &Input::Click {
                target: target.clone(),
            },
            &mut Deny,
        )
        .unwrap();
        let preedit = |text: &str| Input::ImePreedit {
            target: target.clone(),
            text: text.into(),
            cursor: None,
        };
        view.input(&preedit("a"), &mut Deny).unwrap();
        assert!(view.tree.element(2).unwrap().editor.preedit.is_some());
        view.input(&preedit("aa"), &mut Deny).unwrap();
        assert!(view.tree.element(2).unwrap().editor.preedit.is_none());
        assert_eq!(view.tree.element(2).unwrap().value, value);
        view.input(&preedit("a"), &mut Deny).unwrap();
        view.input(
            &Input::ImeCommit {
                target,
                text: "\n".into(),
            },
            &mut Deny,
        )
        .unwrap();
        let element = view.tree.element(2).unwrap();
        assert!(element.editor.preedit.is_none());
        assert_eq!(element.value, value);
        assert_eq!(element.seq, 0);
    }

    #[test]
    fn explicit_network_waits_are_strict_positive_and_bounded() {
        let script = Script::parse(
            br#"{"inputs":[{"input":"wait_network","ms":0.5},{"input":"wait","ms":0}]}"#,
        )
        .unwrap();
        assert_eq!(
            script.inputs,
            [
                ScriptStep::WaitNetwork { ms: 0.5 },
                ScriptStep::Wait { ms: 0.0 }
            ]
        );
        for input in [
            r#"{"input":"wait_network"}"#,
            r#"{"input":"wait_network","ms":0}"#,
            r#"{"input":"wait_network","ms":-1}"#,
            r#"{"input":"wait_network","ms":5001}"#,
            r#"{"input":"wait_network","ms":"100"}"#,
            r#"{"input":"wait_network","ms":100,"target":"x"}"#,
            r#"{"input":"wait","ms":100,"network":true}"#,
        ] {
            assert!(
                Script::parse(format!("{{\"inputs\":[{input}]}}").as_bytes()).is_err(),
                "{input}"
            );
        }
        for ms in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -1.0,
            Script::MAX_NETWORK_WAIT_MS + 1.0,
        ] {
            let script = Script {
                inputs: vec![ScriptStep::WaitNetwork { ms }],
                ..Script::default()
            };
            assert!(matches!(
                script.validate(),
                Err(ScriptError::NetworkWait { step: 1, .. })
            ));
        }
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
            Input::TextSelect {
                target,
                anchor,
                focus,
                activate,
            } => {
                let node = self.resolve(target)?;
                let element = self
                    .tree
                    .element(node)
                    .ok_or_else(|| InputError::Target(target.to_string()))?;
                if !element.text_field()
                    || *anchor > element.value.len()
                    || *focus > element.value.len()
                    || !element.value.is_char_boundary(*anchor)
                    || !element.value.is_char_boundary(*focus)
                {
                    return Err(InputError::Selection);
                }
                if element.disabled() {
                    return Ok(());
                }
                let element = self.tree.element_mut(node).expect("validated element");
                element.editor.anchor = *anchor;
                element.editor.focus = *focus;
                element.editor.manual_text_scroll = false;
                element.editor.reset_navigation();
                element.editor.preedit = None;
                if *activate {
                    self.click(node, broker)?;
                } else {
                    self.focus_input(node)?;
                }
            }
            Input::TextNavigate {
                target,
                key,
                position,
                preferred_x,
                upstream,
                shift,
            } => {
                let node = self.resolve(target)?;
                if !matches!(key.as_str(), "ArrowUp" | "ArrowDown" | "Home" | "End") {
                    return Err(InputError::Key(key.clone()));
                }
                let field = self.tree.element(node).ok_or(InputError::Selection)?;
                if !field.text_field()
                    || *position > field.value.len()
                    || !field.value.is_char_boundary(*position)
                    || preferred_x.is_some_and(|x| !x.is_finite() || x.abs() > 1_000_000.0)
                {
                    return Err(InputError::Selection);
                }
                self.focus_input(node)?;
                let mut record = EventRecord::occurrence(EventKind::Keydown, node);
                record.key = key.clone();
                record.seq = self.tree.element(node).expect("validated field").seq;
                if *shift {
                    record.flags |= EventFlag::Shift as u32;
                }
                if !self.dispatch(record, true)? {
                    let field = self.tree.element_mut(node).expect("validated field");
                    if !field.disabled() {
                        field.editor.focus = *position;
                        if !shift {
                            field.editor.anchor = *position;
                        }
                        field.editor.preferred_x = *preferred_x;
                        field.editor.upstream = *upstream;
                        field.editor.preedit = None;
                        field.editor.manual_text_scroll = false;
                    }
                }
            }
            Input::ImePreedit {
                target,
                text,
                cursor,
            } => {
                let node = self.resolve(target)?;
                let Some(element) = self.tree.element(node) else {
                    return Err(InputError::Selection);
                };
                if self.tree.focus != Some(node)
                    || !element.text_field()
                    || element.disabled()
                    || element.readonly()
                {
                    return Ok(());
                }
                if text.chars().any(char::is_control)
                    || text
                        .encode_utf16()
                        .take(DRAFT_MAX_VALUE_BYTES / 2 + 1)
                        .count()
                        > DRAFT_MAX_VALUE_BYTES / 2
                    || cursor.is_some_and(|(start, end)| {
                        start > end
                            || end > text.len()
                            || !text.is_char_boundary(start)
                            || !text.is_char_boundary(end)
                    })
                {
                    return Err(InputError::Selection);
                }
                let start = element.editor.anchor.min(element.editor.focus);
                let end = element.editor.anchor.max(element.editor.focus);
                let units = element.value[..start].encode_utf16().count()
                    + text.encode_utf16().count()
                    + element.value[end..].encode_utf16().count();
                if units > DRAFT_MAX_VALUE_BYTES / 2 {
                    // Do not leave an earlier, shorter preedit on screen when
                    // the current composition no longer fits the value bound.
                    self.cancel_preedit();
                    return Ok(());
                }
                if let Some(element) = self.tree.element_mut(node) {
                    element.editor.preedit = (!text.is_empty()).then(|| Preedit {
                        text: text.clone(),
                        cursor: *cursor,
                    });
                    element.editor.manual_text_scroll = false;
                    element.editor.reset_navigation();
                }
            }
            Input::ImeCommit { target, text } => {
                let node = self.resolve(target)?;
                let Some(element) = self.tree.element(node) else {
                    return Err(InputError::Selection);
                };
                if self.tree.focus != Some(node)
                    || !element.text_field()
                    || element.disabled()
                    || element.readonly()
                {
                    return Ok(());
                }
                if text.chars().any(char::is_control)
                    || text
                        .encode_utf16()
                        .take(DRAFT_MAX_VALUE_BYTES / 2 + 1)
                        .count()
                        > DRAFT_MAX_VALUE_BYTES / 2
                {
                    self.cancel_preedit();
                    return Ok(());
                }
                if let Some(element) = self.tree.element_mut(node) {
                    element.editor.preedit = None;
                }
                if !text.is_empty() {
                    self.edit(node, |value, anchor, focus, limit| {
                        insert_text(value, anchor, focus, text, limit)
                    })?;
                }
            }
            Input::Type { target, text } => {
                let node = self.resolve(target)?;
                if text
                    .encode_utf16()
                    .take(DRAFT_MAX_VALUE_BYTES / 2 + 1)
                    .count()
                    > DRAFT_MAX_VALUE_BYTES / 2
                {
                    return Ok(());
                }
                self.focus_input(node)?;
                if let Some(element) = self.tree.element_mut(node) {
                    element.editor.preedit = None;
                }
                self.edit(node, |value, anchor, focus, limit| {
                    insert_text(value, anchor, focus, text, limit)
                })?;
            }
            Input::TextKey {
                target,
                key,
                text,
                shift,
                ctrl,
                alt,
                meta,
                select_all,
                history,
            } => {
                let node = self.resolve(target)?;
                if key.is_empty()
                    || key.len() > 4096
                    || text.len() > 4096
                    || key.chars().any(char::is_control)
                    || text.chars().any(char::is_control)
                {
                    return Err(InputError::Key(key.clone()));
                }
                self.focus_input(node)?;
                let mut occurrence = EventRecord::occurrence(EventKind::Keydown, node);
                occurrence.key = key.clone();
                for (held, flag) in [
                    (*shift, EventFlag::Shift),
                    (*ctrl, EventFlag::Ctrl),
                    (*alt, EventFlag::Alt),
                    (*meta, EventFlag::Meta),
                ] {
                    if held {
                        occurrence.flags |= flag as u32;
                    }
                }
                if let Some(element) = self.tree.element(node) {
                    occurrence.seq = element.seq;
                    if element.checked {
                        occurrence.flags |= EventFlag::Checked as u32;
                    }
                }
                if self.dispatch(occurrence, true)? {
                    return Ok(());
                }
                let select_prefix = if text.is_empty() { key } else { text };
                if (!*ctrl || *alt) && !*meta && self.select_text(node, select_prefix)? {
                    return Ok(());
                }
                let history = history.or_else(|| {
                    if key.eq_ignore_ascii_case("z") {
                        Some(if *shift {
                            HistoryAction::Redo
                        } else {
                            HistoryAction::Undo
                        })
                    } else if key.eq_ignore_ascii_case("y") {
                        Some(HistoryAction::Redo)
                    } else {
                        None
                    }
                });
                if let Some(action) = history.filter(|_| (*ctrl || *meta) && !*alt) {
                    self.restore_history(node, action)?;
                } else if (*ctrl || *meta)
                    && !*alt
                    && (*select_all || key.eq_ignore_ascii_case("a"))
                {
                    if let Some(element) = self.tree.element_mut(node)
                        && element.text_field()
                        && !element.disabled()
                    {
                        element.editor.anchor = 0;
                        element.editor.focus = element.value.len();
                        element.editor.manual_text_scroll = false;
                        element.editor.reset_navigation();
                        element.editor.preedit = None;
                    }
                } else if !text.is_empty() && (!*ctrl || *alt) && !*meta {
                    if let Some(element) = self.tree.element_mut(node) {
                        element.editor.preedit = None;
                    }
                    self.edit(node, |value, anchor, focus, limit| {
                        insert_text(value, anchor, focus, text, limit)
                    })?;
                }
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
                let option = self
                    .tree
                    .select_options(node)
                    .into_iter()
                    .find(|option| {
                        self.tree.option_value(*option) == *value
                            && self
                                .tree
                                .element(*option)
                                .is_some_and(|element| !element.disabled())
                    })
                    .ok_or(InputError::Choice)?;
                self.choose_option(node, option)?;
            }
            Input::SelectOption { target, option } => {
                let node = self.resolve(target)?;
                self.choose_option(node, *option)?;
            }
            Input::SelectClose { target } => {
                let node = self.resolve(target)?;
                self.close_select(node);
            }
            Input::SelectScroll { target, first } => {
                let node = self.resolve(target)?;
                if *first > self.tree.select_options(node).len() {
                    return Err(InputError::Choice);
                }
                let popup = self
                    .tree
                    .element_mut(node)
                    .and_then(|element| element.editor.select_popup.as_mut())
                    .ok_or(InputError::Choice)?;
                popup.first = *first;
                popup.manual_scroll = true;
            }
            Input::Scroll { target, top } => {
                let node = self.resolve(target)?;
                if let Some(element) = self.tree.element_mut(node) {
                    element.scroll_top = top.max(0.0);
                    if element.tag == Tag::Textarea {
                        element.editor.manual_text_scroll = true;
                    }
                }
                self.fire(node, EventKind::Scroll, true)?;
            }
            Input::ScrollMeasured {
                target,
                top,
                scroll_height,
                client_height,
            } => {
                let node = self.resolve(target)?;
                if [*top, *scroll_height, *client_height]
                    .into_iter()
                    .any(|value| !value.is_finite() || value < 0.0)
                    || scroll_height < client_height
                    || *top > scroll_height - client_height
                {
                    return Err(InputError::ScrollGeometry);
                }
                let element = self
                    .tree
                    .element_mut(node)
                    .ok_or(InputError::ScrollGeometry)?;
                element.scroll_top = *top;
                if element.tag == Tag::Textarea {
                    element.editor.manual_text_scroll = true;
                }
                let mut record = EventRecord::occurrence(EventKind::Scroll, node);
                record.scroll_top = *top;
                record.scroll_height = *scroll_height;
                record.client_height = *client_height;
                self.dispatch(record, true)?;
            }
            Input::Prompts { answer } => self.platform.prompt = *answer,
        }
        Ok(())
    }

    pub(crate) fn focus_input(&mut self, node: u32) -> Result<(), ViewError> {
        let focusable = self
            .tree
            .element(node)
            .is_some_and(|element| element.focusable() && !element.disabled())
            && self.tree.displayed(node, &self.environment);
        if focusable {
            for record in focus_records(&mut self.tree, Some(node)) {
                self.dispatch(record, true)?;
            }
        }
        Ok(())
    }

    /// One local edit of a field: apply it, bump the edit counter and fire
    /// `input`. Disabled and read-only fields take no edits.
    fn edit(
        &mut self,
        node: u32,
        change: impl FnOnce(&mut String, &mut usize, &mut usize, Option<usize>),
    ) -> Result<(), ViewError> {
        let identity = self.node_identity(node);
        let Some(element) = self.tree.element_mut(node) else {
            return Ok(());
        };
        if !element.text_field() || element.disabled() || element.readonly() {
            return Ok(());
        }
        let mut value = element.value.clone();
        let mut anchor = element.editor.anchor;
        let mut focus = element.editor.focus;
        let limit = element
            .attributes
            .get(&Attribute::Maxlength)
            .and_then(|limit| limit.parse::<usize>().ok());
        change(&mut value, &mut anchor, &mut focus, limit);
        if value
            .encode_utf16()
            .take(DRAFT_MAX_VALUE_BYTES / 2 + 1)
            .count()
            > DRAFT_MAX_VALUE_BYTES / 2
        {
            return Ok(());
        }
        if value == element.value
            && anchor == element.editor.anchor
            && focus == element.editor.focus
        {
            return Ok(());
        }
        if element.text_field()
            && element.value != value
            && let Some(target) = identity
        {
            self.history.retain(|entry| {
                entry.target != target
                    || (!entry.undone
                        && std::sync::Arc::ptr_eq(&entry.epoch, &element.editor.history_epoch))
            });
            self.history.push_back(HistoryEntry {
                target,
                epoch: element.editor.history_epoch.clone(),
                before: EditSnapshot {
                    value: element.value.clone(),
                    anchor: element.editor.anchor,
                    focus: element.editor.focus,
                    preferred_x: element.editor.preferred_x,
                    upstream: element.editor.upstream,
                },
                after: EditSnapshot {
                    value: value.clone(),
                    anchor,
                    focus,
                    preferred_x: None,
                    upstream: false,
                },
                undone: false,
            });
            while self.history.len() > MAX_HISTORY_EDITS
                || self
                    .history
                    .iter()
                    .map(|entry| entry.before.value.len() + entry.after.value.len())
                    .sum::<usize>()
                    > MAX_HISTORY_BYTES
            {
                self.history.pop_front();
            }
        }
        element.value = value;
        element.editor.anchor = anchor;
        element.editor.focus = focus;
        element.editor.manual_text_scroll = false;
        element.editor.reset_navigation();
        element.seq = element.seq.wrapping_add(1);
        self.fire(node, EventKind::Input, true)
    }

    fn restore_history(&mut self, node: u32, action: HistoryAction) -> Result<(), ViewError> {
        let Some(target) = self.node_identity(node) else {
            return Ok(());
        };
        let Some(element) = self.tree.element(node) else {
            return Ok(());
        };
        if !element.text_field() || element.disabled() || element.readonly() {
            return Ok(());
        }
        let redo = action == HistoryAction::Redo;
        let matches = |entry: &HistoryEntry| {
            entry.target == target
                && entry.undone == redo
                && std::sync::Arc::ptr_eq(&entry.epoch, &element.editor.history_epoch)
        };
        let index = if redo {
            self.history.iter().position(matches)
        } else {
            self.history.iter().rposition(matches)
        };
        let Some(index) = index else {
            return Ok(());
        };
        let entry = &self.history[index];
        let expected = if redo { &entry.before } else { &entry.after };
        if element.value != expected.value {
            return Ok(());
        }
        let replacement = if redo {
            entry.after.clone()
        } else {
            entry.before.clone()
        };
        self.history[index].undone = !redo;
        let element = self.tree.element_mut(node).expect("validated field");
        element.value = replacement.value;
        element.editor.anchor = replacement.anchor;
        element.editor.focus = replacement.focus;
        element.editor.preedit = None;
        element.editor.manual_text_scroll = false;
        element.editor.reset_navigation();
        element.editor.preferred_x = replacement.preferred_x;
        element.editor.upstream = replacement.upstream;
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
        if self.tree.element(node).is_some_and(Element::checkable) {
            return self.click_checkable(node);
        }
        let prevented = self.dispatch(EventRecord::occurrence(EventKind::Click, node), true)?;
        if prevented {
            return Ok(());
        }
        self.activate(node, broker)
    }

    fn click_checkable(&mut self, node: u32) -> Result<(), ViewError> {
        let element = self.tree.element(node).expect("validated checkable");
        let radio = element.input_type() == "radio";
        let changed = !radio || !element.checked;
        let old: Vec<_> = if radio {
            self.tree.radio_group(node)
        } else {
            vec![node]
        }
        .into_iter()
        .map(|id| (id, self.tree.element(id).expect("radio group").checked))
        .collect();
        if changed {
            let field = self.tree.element_mut(node).expect("validated checkable");
            field.checked = !field.checked;
            if radio {
                for (peer, _) in &old {
                    if *peer != node {
                        self.tree.element_mut(*peer).expect("radio peer").checked = false;
                    }
                }
            }
        }
        let field = self.tree.element(node).expect("validated checkable");
        let mut record = EventRecord::occurrence(EventKind::Click, node);
        record.seq = field.seq;
        if field.checked {
            record.flags |= EventFlag::Checked as u32;
        }
        // Click listeners observe preactivation. Cancellation restores the entire
        // group before any input/change event or edit-counter increment.
        if self.dispatch(record, true)? {
            for (id, checked) in old {
                self.tree.element_mut(id).expect("retained group").checked = checked;
            }
        } else if changed {
            let field = self.tree.element_mut(node).expect("validated checkable");
            field.seq = field.seq.wrapping_add(1);
            self.fire(node, EventKind::Input, true)?;
            self.fire(node, EventKind::Change, true)?;
        }
        Ok(())
    }

    /// The default action of an activation: submit the enclosing form from
    /// a submit button, open an external link, or
    /// forward a label's activation to its control.
    fn activate(&mut self, node: u32, broker: &mut dyn Broker) -> Result<(), ViewError> {
        let Some(element) = self.tree.element(node).cloned() else {
            return Ok(());
        };
        if element.tag == Tag::Select {
            if element.editor.select_popup.is_some() {
                self.close_select(node);
            } else {
                self.open_select(node);
            }
            return Ok(());
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
                Access::Prompt => {
                    let allowed = self.prompt_allows();
                    broker.prompted(ResultOperation::OpenUrl, &href, allowed);
                    if allowed {
                        self.open(&href)
                    } else {
                        CommandStatus::NotAllowed
                    }
                }
                Access::Denied => CommandStatus::NotAllowed,
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
    ) -> Result<(), InputError> {
        let mut occurrence = EventRecord::occurrence(EventKind::Keydown, node);
        occurrence.flags = modifiers.flags;
        occurrence.key = key.dom_key().into();
        if let Some(element) = self.tree.element(node) {
            occurrence.seq = element.seq;
            if element.checked {
                occurrence.flags |= EventFlag::Checked as u32;
            }
        }
        let prevented = self.dispatch(occurrence, true)?;
        if prevented || modifiers.flags & EventFlag::Composing as u32 != 0 {
            return Ok(());
        }
        let Some(element) = self.tree.element(node).cloned() else {
            return Ok(());
        };
        if element.tag == Tag::Select && !element.disabled() {
            if modifiers.flags & EventFlag::Alt as u32 != 0
                && matches!(key, Key::ArrowDown | Key::ArrowUp)
            {
                self.open_select(node);
                return Ok(());
            }
            if modifiers.flags
                & (EventFlag::Ctrl as u32 | EventFlag::Alt as u32 | EventFlag::Meta as u32)
                == 0
                && self.select_key(node, key)?
            {
                return Ok(());
            }
        }
        let result = match key {
            Key::Enter if element.tag == Tag::Textarea => self
                .edit(node, |value, anchor, focus, limit| {
                    insert_text(value, anchor, focus, "\n", limit)
                }),
            Key::Enter if element.text_field() && !element.disabled() => {
                match self.ancestor(node, Tag::Form) {
                    Some(form) => self.fire(form, EventKind::Submit, true),
                    None => Ok(()),
                }
            }
            Key::Space
                if element.text_field()
                    && modifiers.flags & (EventFlag::Ctrl as u32 | EventFlag::Meta as u32) == 0 =>
            {
                self.edit(node, |value, anchor, focus, limit| {
                    insert_text(value, anchor, focus, " ", limit)
                })
            }
            Key::Enter | Key::Space if element.tag == Tag::Button => self.click(node, broker),
            Key::Enter if element.tag == Tag::A => self.click(node, broker),
            Key::Space if element.checkable() => self.click(node, broker),
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
                if element.checkable()
                    && element.input_type() == "radio"
                    && !element.disabled()
                    && modifiers.flags
                        & (EventFlag::Ctrl as u32
                            | EventFlag::Alt as u32
                            | EventFlag::Meta as u32)
                        == 0 =>
            {
                let group: Vec<_> = self
                    .tree
                    .radio_group(node)
                    .into_iter()
                    .filter(|id| {
                        self.tree
                            .element(*id)
                            .is_some_and(|field| !field.disabled())
                            && self.tree.displayed(*id, &self.environment)
                    })
                    .collect();
                if let Some(at) = group.iter().position(|id| *id == node) {
                    let next = if matches!(key, Key::ArrowLeft | Key::ArrowUp) {
                        (at + group.len() - 1) % group.len()
                    } else {
                        (at + 1) % group.len()
                    };
                    self.click(group[next], broker)?;
                }
                Ok(())
            }
            Key::Backspace | Key::Delete
                if element.text_field()
                    && modifiers.flags
                        & (EventFlag::Ctrl as u32
                            | EventFlag::Alt as u32
                            | EventFlag::Meta as u32)
                        == 0 =>
            {
                self.edit(node, |value, anchor, focus, _| {
                    if *anchor == *focus {
                        if key == Key::Backspace {
                            *anchor = previous_grapheme(value, *focus);
                        } else {
                            *focus = next_grapheme(value, *focus);
                        }
                    }
                    insert_text(value, anchor, focus, "", None);
                })
            }
            Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End
                if element.text_field()
                    && !element.disabled()
                    && modifiers.flags
                        & (EventFlag::Ctrl as u32
                            | EventFlag::Alt as u32
                            | EventFlag::Meta as u32)
                        == 0 =>
            {
                if let Some(field) = self.tree.element_mut(node) {
                    let shift = modifiers.flags & EventFlag::Shift as u32 != 0;
                    let next = match key {
                        Key::ArrowLeft if !shift && field.editor.anchor != field.editor.focus => {
                            field.editor.anchor.min(field.editor.focus)
                        }
                        Key::ArrowRight if !shift && field.editor.anchor != field.editor.focus => {
                            field.editor.anchor.max(field.editor.focus)
                        }
                        Key::ArrowLeft => previous_grapheme(&field.value, field.editor.focus),
                        Key::ArrowRight => next_grapheme(&field.value, field.editor.focus),
                        Key::Home if field.tag == Tag::Input => 0,
                        Key::End if field.tag == Tag::Input => field.value.len(),
                        _ => field.editor.focus,
                    };
                    field.editor.focus = next;
                    field.editor.manual_text_scroll = false;
                    field.editor.reset_navigation();
                    if !shift {
                        field.editor.anchor = next;
                    }
                }
                Ok(())
            }
            Key::Escape => self.escape(node),
            Key::Tab => self.tab(node, modifiers.flags & EventFlag::Shift as u32 != 0),
            _ => Ok(()),
        };
        result.map_err(Into::into)
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
            .displayed_order(&self.environment)
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
        let groups = self.tree.radio_groups();
        let eligible: std::collections::BTreeSet<_> = order.iter().copied().collect();
        let mut seen = std::collections::BTreeSet::new();
        let mut stops = std::collections::BTreeSet::new();
        for id in &order {
            if !seen.insert(*id) {
                continue;
            }
            let group = groups.group(*id);
            seen.extend(group.iter().copied());
            let chosen = group
                .iter()
                .copied()
                .find(|member| {
                    eligible.contains(member)
                        && self
                            .tree
                            .element(*member)
                            .is_some_and(|field| field.checked)
                })
                .unwrap_or(*id);
            stops.insert(chosen);
        }
        let current_group: std::collections::BTreeSet<_> =
            groups.group(node).iter().copied().collect();
        let start = order
            .iter()
            .position(|id| *id == node)
            .map_or(0, |at| at + 1);
        let next = order
            .iter()
            .cycle()
            .skip(start)
            .take(order.len())
            .find(|id| stops.contains(id) && !current_group.contains(id))
            .or_else(|| order.iter().find(|id| stops.contains(id)))
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
