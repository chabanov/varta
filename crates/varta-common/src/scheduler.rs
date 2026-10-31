//! Microtask scheduler contract of profile v34 (`promises`, K4.4 step 3).
//!
//! The runtime owns one bounded microtask queue per execution (one isolate per
//! execution). A queue entry is the resume token of a live continuation frame;
//! the runtime runs it only through the exported `varta_resume` dispatcher with
//! a `Normal` completion and an `undefined` input, so the generation in the
//! token rejects late or duplicate resumptions. Promise objects stay in guest
//! memory: the runtime keys pending reactions by a serial it allocates and never
//! retains a guest reference. These operations grant no capability.
//!
//! Timers (v36 `timers`, K4.4 step 5) extend the same queue: a timer wakes
//! the one reaction of an engine-internal pending promise when it fires or is
//! cleared. Only starting a timer takes a capability (`time.timer`).
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

/// Timers armed or awaiting release at once (v36 `timers`). Every pending
/// timer also holds one resume token (counted against
/// [`MAX_PENDING_CONTINUATIONS`]) and one live continuation frame.
pub const MAX_PENDING_TIMERS: usize = frame::MAX_LIVE_FRAMES / 2;
/// The capability a timer start requires: an exact boolean grant.
pub const TIMER_CAPABILITY: &str = "time.timer";
/// `timer_start` flag: re-arm after every firing (`setInterval`).
pub const TIMER_INTERVAL: i32 = 1;
/// `timer_start` flag: the timer does not keep the event loop alive
/// (Node's `unref`, used by `AbortSignal.timeout`).
pub const TIMER_UNREF: i32 = 2;
/// Node's `TIMEOUT_MAX` (2^31 - 1 ms). A delay that is not a number in
/// `1..=TIMEOUT_MAX` becomes [`TIMER_MIN_DELAY_MS`], as in Node.
pub const TIMER_MAX_DELAY_MS: f64 = 2_147_483_647.0;
pub const TIMER_MIN_DELAY_MS: f64 = 1.0;
// Every pending timer holds a token; the timer flags are distinct bits.
const _: () =
    assert!(MAX_PENDING_TIMERS <= MAX_PENDING_CONTINUATIONS && TIMER_INTERVAL & TIMER_UNREF == 0);

/// Node's delay normalization (`lib/internal/timers.js`): a delay that is not
/// a number in `1..=TIMEOUT_MAX` (NaN, zero, negative or too large) is 1 ms;
/// fractions are kept.
#[must_use]
pub fn timer_delay(ms: f64) -> f64 {
    if (TIMER_MIN_DELAY_MS..=TIMER_MAX_DELAY_MS).contains(&ms) {
        ms
    } else {
        TIMER_MIN_DELAY_MS
    }
}

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

/// The guest's private promise record (its fields and states), owned by the
/// `no_std` value core, which the linked v38 renderer reads too.
pub use crate::value::{
    PROMISE_REASON_FIELD, PROMISE_STATE_FIELD, PROMISE_VALUE_FIELD, PromiseState,
};

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
    #[error("execution exceeds {MAX_PENDING_TIMERS} pending timers")]
    TimerBudget,
    /// V38 (`data_limits`): the memory-derived continuation bound
    /// (`limits::V38_MAX_PENDING_CONTINUATIONS`) instead of [`Self::QueueFull`].
    #[error("microtask queue exceeds {limit} pending continuations")]
    ContinuationLimit { limit: usize },
    /// V38 (`data_limits`): the memory-derived timer bound instead of
    /// [`Self::TimerBudget`].
    #[error("execution exceeds {limit} pending timers")]
    TimerLimit { limit: usize },
    #[error(
        "<capability denied: {TIMER_CAPABILITY}> (setTimeout and setInterval require an exact {TIMER_CAPABILITY} grant)"
    )]
    TimerDenied,
    #[error("invalid timer operation")]
    TimerState,
    #[error(
        "execution deadline exceeded: the next timer is due at least {due_ms} ms after the execution started, beyond the {deadline_seconds} s deadline"
    )]
    TimerDeadline { due_ms: u64, deadline_seconds: u64 },
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
    fn timer_delays_follow_node() {
        for (input, expected) in [
            (0.0, 1.0),
            (-5.0, 1.0),
            (f64::NAN, 1.0),
            (f64::INFINITY, 1.0),
            (TIMER_MAX_DELAY_MS + 1.0, 1.0),
            (0.5, 1.0),
            (1.5, 1.5),
            (250.0, 250.0),
            (TIMER_MAX_DELAY_MS, TIMER_MAX_DELAY_MS),
        ] {
            assert_eq!(timer_delay(input).to_bits(), expected.to_bits(), "{input}");
        }
    }

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
