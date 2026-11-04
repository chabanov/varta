//! Logical field text and exact source-byte selection mapping, without geometry.
use crate::tree::{Element, Node};
use std::{ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

/// Maximum selectable units per native run; word navigation also casts the
/// end position to u8, so that endpoint must fit as well as word starts.
pub const ACCESSIBILITY_TEXT_RUN_CHARACTERS: usize = u8::MAX as usize;

/// Opaque content generation and masking mode. Selection alone does not change it.
#[derive(Debug, Clone)]
pub struct AccessibilityTextRevision {
    token: Arc<()>,
    password: bool,
}
impl PartialEq for AccessibilityTextRevision {
    fn eq(&self, other: &Self) -> bool {
        self.password == other.password && Arc::ptr_eq(&self.token, &other.token)
    }
}
impl Eq for AccessibilityTextRevision {}
impl AccessibilityTextRevision {
    pub(crate) fn new(node: &Node, field: &Element) -> Self {
        Self {
            token: field
                .editor
                .text_epoch
                .as_ref()
                .unwrap_or(&node.identity)
                .clone(),
            password: field.input_type() == "password",
        }
    }
}

/// Directional endpoints in source UTF-8 bytes, not UTF-16 or displayed masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilitySelection {
    pub anchor: usize,
    pub focus: usize,
}

/// Scalar selection units (CRLF is one unit), or rendered grapheme masks for
/// passwords, and logical LF/CRLF lines.
/// Offsets are host-owned and never sent to the platform for password fields.
#[derive(Debug, Clone)]
pub struct AccessibilityText {
    pub revision: AccessibilityTextRevision,
    pub source_offsets: Vec<usize>,
    pub character_lengths: Vec<u8>,
    pub lines: Vec<Range<usize>>,
    pub runs: Vec<AccessibilityTextRun>,
    pub word_starts: Vec<usize>,
    /// None during composition or when a legacy selection splits a CRLF pair
    /// or a rendered password grapheme.
    /// Such selection is not rounded and guest state is never changed by a read.
    pub selection: Option<AccessibilitySelection>,
    pub selectable: bool,
}

pub(super) fn boundary(value: &str, offset: usize, password: bool) -> bool {
    value.is_char_boundary(offset)
        && (offset == 0
            || offset == value.len()
            || if password {
                value.grapheme_indices(true).any(|(byte, _)| byte == offset)
            } else {
                value.as_bytes()[offset - 1] != b'\r' || value.as_bytes()[offset] != b'\n'
            })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibilityTextRun {
    pub characters: Range<usize>,
    pub line: usize,
}

fn units(value: &str, password: bool) -> impl Iterator<Item = (usize, u8, bool)> + '_ {
    let mut chars = value.char_indices().peekable();
    let mut masks = value.grapheme_indices(true);
    std::iter::from_fn(move || {
        if password {
            return masks.next().map(|(offset, grapheme)| {
                (
                    offset + grapheme.len(),
                    crate::text::field_display::PASSWORD_MASK.len() as u8,
                    false,
                )
            });
        }
        let (offset, character) = chars.next()?;
        let mut end = offset + character.len_utf8();
        if character == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
            chars.next();
            end += 1;
        }
        // A scalar is at most four bytes; CRLF two.
        let length = end - offset;
        Some((end, length as u8, value.as_bytes()[end - 1] == b'\n'))
    })
}

pub(super) fn run_count(value: &str, password: bool) -> usize {
    let mut runs = 1;
    let mut used = 0;
    for (_, _, newline) in units(value, password) {
        if used == ACCESSIBILITY_TEXT_RUN_CHARACTERS {
            runs += 1;
            used = 0;
        }
        used += 1;
        if newline {
            runs += 1;
            used = 0;
        }
    }
    runs
}

impl AccessibilityText {
    pub(super) fn new(node: &Node, field: &Element, display: &str) -> Self {
        let password = field.input_type() == "password";
        let mut source_offsets = vec![0];
        let mut character_lengths = Vec::new();
        let mut lines = Vec::new();
        let mut line_start = 0;
        for (end, length, newline) in units(&field.value, password) {
            character_lengths.push(length);
            source_offsets.push(end);
            if newline {
                lines.push(line_start..character_lengths.len());
                line_start = character_lengths.len();
            }
        }
        lines.push(line_start..character_lengths.len());
        let mut runs = Vec::new();
        for (line, range) in lines.iter().enumerate() {
            if range.is_empty() {
                runs.push(AccessibilityTextRun {
                    characters: range.clone(),
                    line,
                });
            }
            for start in (range.start..range.end).step_by(ACCESSIBILITY_TEXT_RUN_CHARACTERS) {
                runs.push(AccessibilityTextRun {
                    characters: start..(start + ACCESSIBILITY_TEXT_RUN_CHARACTERS).min(range.end),
                    line,
                });
            }
        }
        // No original password word structure is exposed.
        let word_starts = if password {
            Vec::new()
        } else {
            display
                .unicode_word_indices()
                .filter_map(|(byte, _)| source_offsets.binary_search(&byte).ok())
                .collect()
        };
        let selectable = !field.disabled() && field.editor.preedit.is_none();
        let selection = (selectable
            && boundary(&field.value, field.editor.anchor, password)
            && boundary(&field.value, field.editor.focus, password))
        .then_some(AccessibilitySelection {
            anchor: field.editor.anchor,
            focus: field.editor.focus,
        });
        Self {
            revision: AccessibilityTextRevision::new(node, field),
            source_offsets,
            character_lengths,
            lines,
            runs,
            word_starts,
            selection,
            selectable,
        }
    }
}
