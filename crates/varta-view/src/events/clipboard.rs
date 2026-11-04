//! Host-owned clipboard defaults. The headless view describes bounded work;
//! the platform performs I/O and later completes it between guest turns.
use super::{insert_text, *};
use crate::{AccessibilityTextRevision, NodeIdentity};
use std::{borrow::Cow, sync::Arc};
use varta_common::view::DRAFT_FIELD_VALUE_RESERVATION_BYTES;

const MAX_CLIPBOARD_KEY_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAction {
    Copy,
    Cut,
    Paste,
}

/// A physical host shortcut with the original logical key for keydown filters.
/// Menu commands can later use a distinct input path; no synthetic key is used.
#[derive(Debug, Clone)]
pub struct ClipboardKey {
    pub action: ClipboardAction,
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Opaque content/selection binding, never clipboard text or password bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardTarget {
    identity: NodeIdentity,
    revision: AccessibilityTextRevision,
    anchor: usize,
    focus: usize,
}

/// Exactly one outstanding operation per view. Private identity prevents
/// replayed, foreign and duplicate completions from changing a field.
#[derive(Debug, Clone)]
pub struct ClipboardToken {
    nonce: Arc<()>,
    target: ClipboardTarget,
    action: ClipboardAction,
}

/// An authorized platform operation. Own the task until I/O completes; do not
/// run clipboard I/O or wait for it on the UI/application event-loop thread.
pub enum ClipboardTask {
    Read {
        token: ClipboardToken,
        max_bytes: usize,
    },
    Write {
        token: ClipboardToken,
        text: String,
    },
}
impl ClipboardTask {
    #[must_use]
    pub fn token(&self) -> &ClipboardToken {
        match self {
            Self::Read { token, .. } | Self::Write { token, .. } => token,
        }
    }
}

/// The embedder must cap a retained read before returning it. The view also
/// validates byte and UTF-16 limits without truncating clipboard contents.
pub enum ClipboardData {
    Text(Option<String>),
    Written,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard is unavailable")]
    Unavailable,
    #[error("clipboard operation failed")]
    Failed,
    #[error("clipboard text exceeds the host limit")]
    TooLarge,
    #[error("clipboard data is invalid")]
    InvalidData,
}

impl View {
    /// Capture only a current displayed text-field selection. This performs no
    /// clipboard I/O and does not require an accessibility projection.
    #[must_use]
    pub fn clipboard_target(&self) -> Option<ClipboardTarget> {
        let id = self.tree.focus?;
        let node = self.tree.nodes.get(&id)?;
        let field = self.tree.element(id)?;
        if !field.text_field()
            || field.disabled()
            || field.editor.preedit.is_some()
            || !self.tree.displayed(id, &self.environment)
            || self.tree.path(id).into_iter().any(|id| {
                self.tree
                    .element(id)
                    .is_some_and(|e| e.tag == Tag::Dialog && !e.open)
            })
            || !field.value.is_char_boundary(field.editor.anchor)
            || !field.value.is_char_boundary(field.editor.focus)
        {
            return None;
        }
        Some(ClipboardTarget {
            identity: NodeIdentity {
                id,
                token: node.identity.clone(),
            },
            revision: AccessibilityTextRevision::new(node, field),
            anchor: field.editor.anchor,
            focus: field.editor.focus,
        })
    }
    #[must_use]
    pub fn clipboard_pending(&self) -> bool {
        self.pending_clipboard.is_some()
    }

    /// Start one trusted host shortcut. Static preventDefault is honored before
    /// a task exists; guest keydown callbacks run later in their ordinary turns.
    pub fn begin_clipboard(
        &mut self,
        target: Option<&ClipboardTarget>,
        key: &ClipboardKey,
    ) -> Result<Option<ClipboardTask>, InputError> {
        if key.key.is_empty()
            || key.key.len() > MAX_CLIPBOARD_KEY_BYTES
            || key.key.chars().any(char::is_control)
        {
            return Err(InputError::Key(key.key.clone()));
        }
        if self.pending_clipboard.is_some()
            || target.is_some_and(|target| self.clipboard_target().as_ref() != Some(target))
        {
            return Ok(None);
        }
        let Some(id) = self.tree.focus else {
            return Ok(None);
        };
        let Some(field) = self.tree.element(id) else {
            return Ok(None);
        };
        if field.disabled()
            || !self.tree.displayed(id, &self.environment)
            || self.tree.path(id).into_iter().any(|id| {
                self.tree
                    .element(id)
                    .is_some_and(|e| e.tag == Tag::Dialog && !e.open)
            })
        {
            return Ok(None);
        }
        let mut occurrence = EventRecord::occurrence(EventKind::Keydown, id);
        occurrence.key = key.key.clone();
        occurrence.seq = field.seq;
        if field.checked {
            occurrence.flags |= EventFlag::Checked as u32;
        }
        if field.editor.preedit.is_some() {
            occurrence.flags |= EventFlag::Composing as u32;
        }
        for (held, flag) in [
            (key.shift, EventFlag::Shift),
            (key.ctrl, EventFlag::Ctrl),
            (key.alt, EventFlag::Alt),
            (key.meta, EventFlag::Meta),
        ] {
            if held {
                occurrence.flags |= flag as u32;
            }
        }
        if self.dispatch(occurrence, true)? {
            return Ok(None);
        }
        // A key on a button/select still bubbles normally, but an absent field
        // binding never authorizes a clipboard operation or text typeahead.
        let Some(target) = target else {
            return Ok(None);
        };
        let field = self.tree.element(id).expect("retained clipboard target");
        if key.action != ClipboardAction::Copy && field.readonly()
            || key.action != ClipboardAction::Paste && field.input_type() == "password"
        {
            return Ok(None);
        }
        let token = ClipboardToken {
            nonce: Arc::new(()),
            target: target.clone(),
            action: key.action,
        };
        let task = if key.action == ClipboardAction::Paste {
            ClipboardTask::Read {
                token: token.clone(),
                max_bytes: DRAFT_FIELD_VALUE_RESERVATION_BYTES,
            }
        } else {
            let start = target.anchor.min(target.focus);
            let end = target.anchor.max(target.focus);
            if start == end {
                return Ok(None);
            }
            ClipboardTask::Write {
                token: token.clone(),
                text: field.value[start..end].into(),
            }
        };
        self.pending_clipboard = Some(token);
        Ok(Some(task))
    }

    /// Finish between turns. A stale source/focus/selection or a failed write
    /// cannot delete text. Completion produces no fresh gesture authority.
    pub fn complete_clipboard(
        &mut self,
        token: &ClipboardToken,
        result: Result<ClipboardData, ClipboardError>,
    ) -> Result<bool, InputError> {
        if self
            .pending_clipboard
            .as_ref()
            .is_none_or(|pending| !Arc::ptr_eq(&pending.nonce, &token.nonce))
        {
            return Ok(false);
        }
        self.pending_clipboard = None;
        if self.clipboard_target().as_ref() != Some(&token.target) {
            return Ok(false);
        }
        let field = self
            .tree
            .element(token.target.identity.id)
            .expect("current clipboard field");
        if token.action != ClipboardAction::Copy && field.readonly()
            || token.action != ClipboardAction::Paste && field.input_type() == "password"
        {
            return Ok(false);
        }
        let id = token.target.identity.id;
        match (token.action, result) {
            (ClipboardAction::Copy, Ok(ClipboardData::Written)) => Ok(true),
            (ClipboardAction::Cut, Ok(ClipboardData::Written)) => {
                self.edit(id, |value, anchor, focus, limit| {
                    insert_text(value, anchor, focus, "", limit)
                })?;
                Ok(true)
            }
            (ClipboardAction::Paste, Ok(ClipboardData::Text(Some(text)))) => {
                if text.len() > DRAFT_FIELD_VALUE_RESERVATION_BYTES
                    || text
                        .encode_utf16()
                        .take(DRAFT_MAX_VALUE_BYTES / 2 + 1)
                        .count()
                        > DRAFT_MAX_VALUE_BYTES / 2
                {
                    return Ok(false);
                }
                let text = normalize_paste(&text, field.tag == Tag::Textarea);
                if !text.is_empty() {
                    self.edit(id, |value, anchor, focus, limit| {
                        insert_text(value, anchor, focus, &text, limit)
                    })?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn normalize_paste(text: &str, multiline: bool) -> Cow<'_, str> {
    let text = if multiline {
        text
    } else {
        text.trim_end_matches(['\r', '\n'])
    };
    if !text.contains('\r') && (multiline || !text.contains('\n')) {
        return Cow::Borrowed(text);
    }
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                result.push(if multiline { '\n' } else { ' ' });
            }
            '\n' if !multiline => result.push(' '),
            _ => result.push(ch),
        }
    }
    Cow::Owned(result)
}
