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
    UnhandledRejection { count: u64, reason: ThrownValue },
}
