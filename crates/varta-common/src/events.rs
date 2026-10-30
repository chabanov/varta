//! Host event sources of profile v38 (`turn_budgets`, increment P5): the
//! contract shared by the compiler's `varta.events` prelude and the runtime's
//! host task queue.
//!
//! A guest opens a **source** by name (`event_source_open`); the embedder of a
//! long-lived application instance delivers records to it as **host tasks**.
//! Each task is one event turn: it wakes the one reaction of the
//! engine-internal pending promise the source's frame registered with
//! `event_watch` (like a timer or a network completion), and the frame takes
//! the record with `event_take` and `event_text`. A permission request's answer
//! uses a one-shot source of the same kind. The runtime keeps source ids,
//! generations and serials, never a guest reference.

/// Sources open at once per instance (named sources and pending permission
/// answers together).
pub const MAX_EVENT_SOURCES: usize = 32;
/// Host tasks queued at once per instance. Overflow drops a coalescible
/// record and fails the instance for any other.
pub const MAX_PENDING_HOST_TASKS: usize = 256;
/// Bytes of one event record (UTF-8).
pub const MAX_EVENT_RECORD_BYTES: usize = 64 * 1024;
/// Bytes of a source name.
pub const MAX_EVENT_NAME_BYTES: usize = 64;
/// `event_source_open` answers this id for an invalid or duplicate name; the
/// prelude throws `TypeError`.
pub const INVALID_SOURCE: f64 = 0.0;

/// What `event_take` reports. The prelude mirrors these codes as named
/// constants; a compiler test keeps both in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TakeKind {
    /// A delivered record; `event_text` returns it.
    Event = 1,
    /// The source was closed; `event_text` returns the empty string.
    Closed = 2,
}

/// Whether `name` is a valid source name: 1 to [`MAX_EVENT_NAME_BYTES`]
/// bytes of ASCII letters, digits, `.`, `_` and `-`, starting with a letter.
#[must_use]
pub fn valid_source_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_EVENT_NAME_BYTES
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Typed failures of host event delivery reported to the embedder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EventError {
    #[error("no open host event source is named {0}")]
    NoListener(String),
    #[error("event record exceeds {MAX_EVENT_RECORD_BYTES} bytes")]
    RecordTooLarge,
    #[error("host task queue is full ({MAX_PENDING_HOST_TASKS} tasks)")]
    QueueFull,
    #[error("invalid host event source state")]
    State,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_names_are_bounded_identifiers() {
        for good in ["ui", "ui.click", "a-b_c.9"] {
            assert!(valid_source_name(good), "{good}");
        }
        for bad in [
            "",
            "9ui",
            ".ui",
            "ui click",
            "ui/x",
            "é",
            &"a".repeat(MAX_EVENT_NAME_BYTES + 1),
        ] {
            assert!(!valid_source_name(bad), "{bad:?}");
        }
    }
}
