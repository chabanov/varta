//! The permission broker of v37: the one decision point for the capability
//! checks of operations that complete asynchronously (`net.request` behind
//! `fetch`, `time.timer` behind the timers). Every decision is taken before
//! the operation has any effect (no name resolution, socket or armed timer
//! precedes it), and a denial is delivered to the guest as a recoverable
//! `NotAllowedError` instead of ending the execution.
//!
//! Today the broker answers synchronously from the package's requests
//! intersected with the host ceiling. It is the seam for a later consent
//! prompt: a third answer, "pending until the user decides", would leave the
//! operation armed in its slot (a request with no transfer yet, a timer that
//! is not armed yet) while its frame keeps waiting on its engine-internal
//! promise through the scheduler; the user's answer then completes it as
//! granted or denied. A user grant is checked against the host ceiling like
//! any other, so a prompt can never exceed the ceiling. No prompt exists yet.
use crate::policy::Policy;

/// A broker answer.
pub(crate) enum Decision<T> {
    /// The operation may proceed, with what the check produced (the parsed
    /// URL of a request).
    Granted(T),
    /// The operation is refused; the guest receives a recoverable denial.
    Denied,
}

/// Arming a timer (`time.timer`, an exact boolean grant).
pub(crate) fn timer(policy: &Policy) -> Decision<()> {
    if policy.enabled(varta_common::scheduler::TIMER_CAPABILITY) {
        Decision::Granted(())
    } else {
        Decision::Denied
    }
}

/// One network request (`net.request`: URL scopes and an optional method
/// list); a granted request carries the URL the transport must use.
pub(crate) fn request(policy: &Policy, url: &str, method: &str) -> Decision<url::Url> {
    match policy.request(url, method) {
        Some(url) => Decision::Granted(url),
        None => Decision::Denied,
    }
}
