//! Native GUI backlog; distinct from the bounded runtime event queue.
use std::collections::VecDeque;
use winit::keyboard::ModifiersState;

pub(super) const WORKER_QUEUE_CAPACITY: usize = 8;
pub(super) const MAX_PENDING_KEYS: usize = 64;
pub(super) const MAX_PENDING_TEXT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_KEY_TEXT_BYTES: usize = 4096;

pub(super) enum PendingKey {
    SelectClose(u32),
    Key(&'static str, ModifiersState),
    Character {
        key: String,
        text: String,
        modifiers: ModifiersState,
        select_all: bool,
        history: Option<varta_view::HistoryAction>,
    },
    ImePreedit {
        node: u32,
        commits: usize,
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit {
        node: u32,
        commits: usize,
        text: String,
    },
}

impl PendingKey {
    fn text_bytes(&self) -> usize {
        match self {
            Self::Character { key, text, .. } => key.len().saturating_add(text.len()),
            Self::ImePreedit { text, .. } | Self::ImeCommit { text, .. } => text.len(),
            Self::SelectClose(_) | Self::Key(..) => 0,
        }
    }
    fn compact(&mut self) {
        fn compact(text: &mut String) {
            if text.capacity() != text.len() {
                *text = std::mem::take(text).into_boxed_str().into_string();
            }
        }
        match self {
            Self::Character { key, text, .. } => {
                compact(key);
                compact(text);
            }
            Self::ImePreedit { text, .. } | Self::ImeCommit { text, .. } => compact(text),
            Self::SelectClose(_) | Self::Key(..) => {}
        }
    }
    fn owned_bytes(&self) -> usize {
        match self {
            Self::Character { key, text, .. } => key.capacity().saturating_add(text.capacity()),
            Self::ImePreedit { text, .. } | Self::ImeCommit { text, .. } => text.capacity(),
            Self::SelectClose(_) | Self::Key(..) => 0,
        }
    }
    fn replaces(&self, previous: &Self) -> bool {
        matches!((self,previous),(Self::ImePreedit {node:a,commits:b,..},Self::ImePreedit {node:c,commits:d,..}) if a==c && b==d)
    }
}

#[derive(Default)]
pub(super) struct PendingKeys {
    entries: VecDeque<PendingKey>,
    bytes: usize,
}
impl PendingKeys {
    pub(super) fn push(&mut self, mut pending: PendingKey) -> Result<(), &'static str> {
        let replace = self
            .entries
            .back()
            .is_some_and(|last| pending.replaces(last));
        if !replace && self.entries.len() >= MAX_PENDING_KEYS {
            return Err("native keyboard queue is full");
        }
        let refund = if replace {
            self.entries.back().unwrap().owned_bytes()
        } else {
            0
        };
        let Some(bytes) = (self.bytes - refund)
            .checked_add(pending.text_bytes())
            .filter(|bytes| *bytes <= MAX_PENDING_TEXT_BYTES)
        else {
            return Err("native keyboard text queue is full");
        };
        // Validate before compacting/copying. Normalization releases oversized
        // spare platform capacity without rejecting otherwise valid short input.
        pending.compact();
        debug_assert_eq!(pending.owned_bytes(), pending.text_bytes());
        if replace {
            self.entries.pop_back();
        }
        self.entries.push_back(pending);
        self.bytes = bytes;
        Ok(())
    }
    pub(super) fn pop_front(&mut self) -> Option<PendingKey> {
        let pending = self.entries.pop_front()?;
        self.bytes -= pending.owned_bytes();
        Some(pending)
    }
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub(super) fn retain(&mut self, mut keep: impl FnMut(&PendingKey) -> bool) {
        self.entries.retain(|entry| {
            if keep(entry) {
                true
            } else {
                self.bytes -= entry.owned_bytes();
                false
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(capacity: usize) -> String {
        "x".repeat(capacity)
    }
    fn preedit(node: u32, commits: usize, text: String) -> PendingKey {
        PendingKey::ImePreedit {
            node,
            commits,
            text,
            cursor: None,
        }
    }
    #[test]
    fn exact_owned_capacity_and_one_byte_excess_preserve_fifo() {
        let mut keys = PendingKeys::default();
        keys.push(PendingKey::ImeCommit {
            node: 1,
            commits: 0,
            text: text(MAX_PENDING_TEXT_BYTES),
        })
        .unwrap();
        assert_eq!(keys.bytes, MAX_PENDING_TEXT_BYTES);
        assert_eq!(
            keys.push(PendingKey::ImeCommit {
                node: 1,
                commits: 0,
                text: "x".into()
            }),
            Err("native keyboard text queue is full")
        );
        assert_eq!(keys.entries.len(), 1);
        assert!(
            matches!(keys.pop_front(),Some(PendingKey::ImeCommit{text,..}) if text.len()==MAX_PENDING_TEXT_BYTES&&text.capacity()==MAX_PENDING_TEXT_BYTES)
        );
        assert_eq!(keys.bytes, 0);
        assert!(keys.is_empty());
        keys.push(PendingKey::ImeCommit {
            node: 1,
            commits: 0,
            text: text(MAX_PENDING_TEXT_BYTES),
        })
        .unwrap();
    }
    #[test]
    fn rejected_preedit_replacement_keeps_old_value_and_success_refunds_old_capacity() {
        let mut keys = PendingKeys::default();
        keys.push(preedit(1, 0, "abc".into())).unwrap();
        keys.push(preedit(2, 0, text(MAX_PENDING_TEXT_BYTES - 3)))
            .unwrap();
        assert!(
            keys.push(preedit(2, 0, text(MAX_PENDING_TEXT_BYTES)))
                .is_err()
        );
        assert_eq!(keys.entries.len(), 2);
        assert_eq!(keys.bytes, MAX_PENDING_TEXT_BYTES);
        assert!(
            matches!(keys.pop_front(),Some(PendingKey::ImePreedit{node:1,text,..}) if text=="abc")
        );
        keys.push(preedit(2, 0, text(MAX_PENDING_TEXT_BYTES)))
            .unwrap();
        assert_eq!(keys.entries.len(), 1);
        assert_eq!(keys.bytes, MAX_PENDING_TEXT_BYTES);
        keys.push(preedit(2, 0, "small".into())).unwrap();
        assert_eq!(keys.bytes, 5);
    }
    #[test]
    fn preedit_coalescing_requires_the_same_binding_and_never_crosses_a_commit() {
        let mut keys = PendingKeys::default();
        keys.push(preedit(1, 0, "a".into())).unwrap();
        keys.push(preedit(1, 1, "b".into())).unwrap();
        keys.push(preedit(2, 1, "c".into())).unwrap();
        keys.push(PendingKey::ImeCommit {
            node: 2,
            commits: 1,
            text: "committed".into(),
        })
        .unwrap();
        keys.push(preedit(2, 1, "d".into())).unwrap();
        keys.push(preedit(2, 1, "e".into())).unwrap();
        assert_eq!(keys.entries.len(), 5);
        let texts = std::iter::from_fn(|| keys.pop_front())
            .map(|entry| match entry {
                PendingKey::ImePreedit { text, .. } | PendingKey::ImeCommit { text, .. } => text,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(texts, ["a", "b", "c", "committed", "e"]);
        assert_eq!(keys.bytes, 0);
    }
    #[test]
    fn entry_limit_allows_only_a_valid_preedit_replacement_at_capacity() {
        let mut keys = PendingKeys::default();
        for _ in 0..MAX_PENDING_KEYS - 1 {
            keys.push(PendingKey::Key("Tab", ModifiersState::SHIFT))
                .unwrap();
        }
        keys.push(preedit(1, 0, "old".into())).unwrap();
        keys.push(preedit(1, 0, "new".into())).unwrap();
        assert_eq!(
            keys.push(PendingKey::SelectClose(1)),
            Err("native keyboard queue is full")
        );
        assert!(matches!(keys.pop_front(),Some(PendingKey::Key("Tab",m)) if m.shift_key()));
        keys.push(PendingKey::SelectClose(1)).unwrap();
        assert_eq!(keys.entries.len(), MAX_PENDING_KEYS);
    }
    #[test]
    fn retain_and_clear_release_ime_and_character_allocations() {
        let mut keys = PendingKeys::default();
        keys.push(preedit(1, 0, text(1000))).unwrap();
        keys.push(PendingKey::Character {
            key: text(1000),
            text: text(2000),
            modifiers: ModifiersState::empty(),
            select_all: false,
            history: None,
        })
        .unwrap();
        keys.push(preedit(2, 0, text(4000))).unwrap();
        assert_eq!(keys.bytes, 8000);
        keys.retain(|entry| !matches!(entry, PendingKey::ImePreedit { node: 1, .. }));
        assert_eq!(keys.bytes, 7000);
        keys.clear();
        assert_eq!(keys.bytes, 0);
        assert!(keys.is_empty());
        keys.push(preedit(1, 0, text(MAX_PENDING_TEXT_BYTES)))
            .unwrap();
    }
    #[test]
    fn full_unicode_commits_are_kept_whole_until_aggregate_capacity_is_exhausted() {
        let value = "界".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2);
        let count = MAX_PENDING_TEXT_BYTES / value.capacity();
        let mut keys = PendingKeys::default();
        for _ in 0..count {
            keys.push(PendingKey::ImeCommit {
                node: 1,
                commits: 0,
                text: value.clone(),
            })
            .unwrap();
        }
        assert!(
            keys.push(PendingKey::ImeCommit {
                node: 1,
                commits: 0,
                text: value.clone()
            })
            .is_err()
        );
        assert_eq!(keys.entries.len(), count);
        while let Some(PendingKey::ImeCommit { text, .. }) = keys.pop_front() {
            assert_eq!(text, value);
        }
        assert_eq!(keys.bytes, 0);
    }
}
