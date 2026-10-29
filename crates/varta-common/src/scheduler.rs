//! Microtask scheduler contract of profile v34 (`promises`, K4.4 step 3).
//!
//! The runtime owns one bounded microtask queue per execution (one isolate per
//! execution). A queue entry is the resume token of a live continuation frame;
//! the runtime runs it only through the exported `varta_resume` dispatcher with
//! a `Normal` completion and an `undefined` input, so the generation in the
//! token rejects late or duplicate resumptions. Promise objects stay in guest
//! memory: the runtime keys pending reactions by a serial it allocates and never
//! retains a guest reference. These operations grant no capability.
use crate::exceptions::ThrownValue;
use crate::function::frame;

/// Resume tokens the scheduler may hold at once, in the queue and in pending
/// reactions together. Every token names a distinct live frame, so a correct
/// module never reaches this bound before its live-frame table is full.
pub const MAX_PENDING_CONTINUATIONS: usize = frame::MAX_LIVE_FRAMES;
/// Microtasks dispatched per execution, across the whole checkpoint.
pub const MAX_EXECUTION_JOBS: u64 = 1 << 16;
/// Reaction serials, and separately rejection-tracking ids, allocated per
/// execution. Zero means "none" in the guest's promise record.
pub const MAX_PROMISE_SERIALS: u64 = 1 << 20;
/// Outstanding unhandled-rejection reports whose rendered reason the runtime
/// keeps at once. Later reports keep only their tracking bit (one bit per id
/// issued, at most [`MAX_PROMISE_SERIALS`] bits) until a kept one is withdrawn.
pub const MAX_RETAINED_REJECTION_REASONS: usize = 16;
/// UTF-16 code units copied out of guest memory per string field of a
/// reason (a String reason, an Error name or message); longer text is cut.
pub const MAX_REJECTION_REASON_UNITS: usize = crate::exceptions::MAX_DIAGNOSTIC_UNITS;
/// Bytes of rendered reason text kept per report, before the marker.
pub const MAX_REJECTION_REASON_BYTES: usize = 2048;
/// Marks a reason cut at [`MAX_REJECTION_REASON_UNITS`] or
/// [`MAX_REJECTION_REASON_BYTES`] in the termination diagnostic.
pub const REJECTION_TRUNCATION_MARKER: &str = " [reason truncated]";
/// Names the earliest remaining report when its reason was not kept.
pub const REJECTION_NOT_RETAINED: &str = "<reason not retained>";

/// The bounded rendering of an unhandled rejection's reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionReason {
    /// The rendered reason, at most [`MAX_REJECTION_REASON_BYTES`] bytes;
    /// `truncated` when a bound cut it.
    Retained { text: String, truncated: bool },
    /// Reported while [`MAX_RETAINED_REJECTION_REASONS`] reasons were kept.
    NotRetained,
}
impl RejectionReason {
    /// Render `value` (whose string fields the reader already cut when
    /// `truncated`) and bound the text at a character boundary.
    #[must_use]
    pub fn render(value: &ThrownValue, truncated: bool) -> Self {
        let mut text = value.to_string();
        let mut truncated = truncated;
        if text.len() > MAX_REJECTION_REASON_BYTES {
            let mut end = MAX_REJECTION_REASON_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            truncated = true;
        }
        Self::Retained { text, truncated }
    }
}
impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained { text, truncated } => {
                f.write_str(text)?;
                if *truncated {
                    f.write_str(REJECTION_TRUNCATION_MARKER)?;
                }
                Ok(())
            }
            Self::NotRetained => f.write_str(REJECTION_NOT_RETAINED),
        }
    }
}

/// Private fields of the guest's promise record that value16 readers
/// (v35 `promise_values`) inspect to log a promise: its state and its
/// settled value or reason. The compiler names the record's fields by these.
pub const PROMISE_STATE_FIELD: &str = "state";
pub const PROMISE_VALUE_FIELD: &str = "value";
pub const PROMISE_REASON_FIELD: &str = "reason";

/// Promise states as the guest stores them in its private promise record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum PromiseState {
    Pending = 0,
    Fulfilled = 1,
    Rejected = 2,
}

/// Fatal scheduler failures. None of them is ever delivered to the guest as a
/// rejection: each one terminates the execution.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchedulerError {
    #[error("microtask queue exceeds {MAX_PENDING_CONTINUATIONS} pending continuations")]
    QueueFull,
    #[error("execution exceeds {MAX_EXECUTION_JOBS} microtasks")]
    JobBudget,
    #[error("execution exceeds {MAX_PROMISE_SERIALS} promise serials")]
    SerialBudget,
    #[error("resume token is already scheduled")]
    DuplicateToken,
    #[error("unknown promise serial")]
    UnknownSerial,
    #[error("scheduled frame reported an invalid outcome")]
    Outcome,
    #[error(
        "unhandled promise rejection: {reason} ({count} rejected promise(s) had no handler after the microtask checkpoint)"
    )]
    UnhandledRejection {
        count: u64,
        /// The earliest remaining report's bounded reason.
        reason: RejectionReason,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_reasons_are_bounded_and_marked() {
        let short =
            RejectionReason::render(&ThrownValue::String("ok".encode_utf16().collect()), false);
        assert_eq!(short.to_string(), "\"ok\"");
        let cut = RejectionReason::render(&ThrownValue::String(vec![u16::from(b'a'); 3]), true);
        assert_eq!(
            cut.to_string(),
            format!("\"aaa\"{REJECTION_TRUNCATION_MARKER}")
        );
        // Escaped text longer than the byte bound is cut at a char boundary.
        let wide = ThrownValue::ErrorInstance {
            name: vec![0x00e9; MAX_REJECTION_REASON_UNITS],
            message: vec![0x0001; MAX_REJECTION_REASON_UNITS],
        };
        let RejectionReason::Retained { text, truncated } = RejectionReason::render(&wide, false)
        else {
            panic!("retained");
        };
        assert!(truncated && text.len() <= MAX_REJECTION_REASON_BYTES);
        assert_eq!(
            RejectionReason::NotRetained.to_string(),
            REJECTION_NOT_RETAINED
        );
    }
}
