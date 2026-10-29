//! The v34 microtask scheduler (K4.4 step 3): one bounded queue per execution.
//!
//! The queue holds only resume tokens of live continuation frames; guest code
//! registers them through the scheduler imports and the runtime runs them only
//! through the exported `varta_resume` dispatcher. Pending reactions are keyed
//! by serials the runtime allocates, so it never retains a guest reference.
//! After `main` returns the checkpoint runs every queued job, including jobs
//! queued while it drains, in FIFO order; the per-execution fuel, deadline and
//! host-call ceilings keep running across the whole checkpoint. Afterwards a
//! rejection that is still unhandled terminates the execution.
use crate::{RuntimeContext, RuntimeError, exceptions::Channel};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use varta_common::HostOperation;
use varta_common::function::frame::{self, Completion, Outcome};
use varta_common::scheduler::{
    MAX_EXECUTION_JOBS, MAX_PENDING_CONTINUATIONS, MAX_PROMISE_SERIALS, MAX_REJECTION_REASON_UNITS,
    MAX_RETAINED_REJECTION_REASONS, RejectionReason, SchedulerError,
};
use wasmtime::{Caller, Extern, Instance, Linker, Store};

#[derive(Default)]
pub(crate) struct Scheduler {
    /// Ready microtasks in FIFO order.
    queue: VecDeque<i64>,
    /// Reactions of pending promises by serial, each in registration order.
    reactions: BTreeMap<u64, Vec<i64>>,
    /// Every token currently held, queued or in a reaction list.
    held: BTreeSet<i64>,
    next_serial: u64,
    dispatched: u64,
    /// Rejections without a handler: one bit per tracking id issued, so the
    /// bookkeeping never exceeds `MAX_PROMISE_SERIALS` bits.
    outstanding: Vec<u64>,
    unhandled: u64,
    /// Rendered reasons of at most `MAX_RETAINED_REJECTION_REASONS`
    /// outstanding reports, each bounded by the `RejectionReason` limits.
    reasons: BTreeMap<u64, RejectionReason>,
    next_tracking: u64,
}
const TRACKING_WORD_BITS: u64 = u64::BITS as u64;

/// A guest serial must be zero or a nonnegative integer the runtime issued.
fn serial(value: f64, issued: u64) -> Result<u64, SchedulerError> {
    if !(0.0..=MAX_PROMISE_SERIALS as f64).contains(&value) || value.fract() != 0.0 {
        return Err(SchedulerError::UnknownSerial);
    }
    let serial = value as u64;
    if serial > issued {
        return Err(SchedulerError::UnknownSerial);
    }
    Ok(serial)
}

impl Scheduler {
    fn hold(&mut self, token: i64) -> Result<(), SchedulerError> {
        if self.held.len() >= MAX_PENDING_CONTINUATIONS {
            return Err(SchedulerError::QueueFull);
        }
        if !self.held.insert(token) {
            return Err(SchedulerError::DuplicateToken);
        }
        Ok(())
    }
    /// Register `token` as a reaction of the pending promise `serial`,
    /// allocating the promise's serial on its first reaction.
    pub(crate) fn react(&mut self, value: f64, token: i64) -> Result<f64, SchedulerError> {
        let mut serial = serial(value, self.next_serial)?;
        if serial == 0 {
            if self.next_serial >= MAX_PROMISE_SERIALS {
                return Err(SchedulerError::SerialBudget);
            }
            self.next_serial += 1;
            serial = self.next_serial;
            self.reactions.insert(serial, Vec::new());
        }
        if !self.reactions.contains_key(&serial) {
            return Err(SchedulerError::UnknownSerial);
        }
        self.hold(token)?;
        self.reactions
            .get_mut(&serial)
            .ok_or(SchedulerError::UnknownSerial)?
            .push(token);
        Ok(serial as f64)
    }
    /// The promise `serial` settled: queue its reactions in order.
    pub(crate) fn settle(&mut self, value: f64) -> Result<(), SchedulerError> {
        let serial = serial(value, self.next_serial)?;
        let reactions = self
            .reactions
            .remove(&serial)
            .ok_or(SchedulerError::UnknownSerial)?;
        self.queue.extend(reactions);
        Ok(())
    }
    pub(crate) fn enqueue(&mut self, token: i64) -> Result<(), SchedulerError> {
        self.hold(token)?;
        self.queue.push_back(token);
        Ok(())
    }
    /// Track a rejection without a handler. Its reason is kept only while
    /// fewer than `MAX_RETAINED_REJECTION_REASONS` reasons are kept.
    pub(crate) fn unhandled(&mut self, reason: RejectionReason) -> Result<f64, SchedulerError> {
        if self.next_tracking >= MAX_PROMISE_SERIALS {
            return Err(SchedulerError::SerialBudget);
        }
        self.next_tracking += 1;
        let id = self.next_tracking;
        let (word, bit) = Self::tracking(id);
        if self.outstanding.len() <= word {
            self.outstanding.resize(word + 1, 0);
        }
        self.outstanding[word] |= bit;
        self.unhandled += 1;
        if self.reasons.len() < MAX_RETAINED_REJECTION_REASONS {
            self.reasons.insert(id, reason);
        }
        Ok(id as f64)
    }
    /// Whether the next report's rendered reason would be kept.
    pub(crate) fn retains_reason(&self) -> bool {
        self.reasons.len() < MAX_RETAINED_REJECTION_REASONS
    }
    pub(crate) fn handled(&mut self, value: f64) -> Result<(), SchedulerError> {
        let id = serial(value, self.next_tracking)?;
        let (word, bit) = Self::tracking(id);
        match self.outstanding.get_mut(word) {
            Some(bits) if *bits & bit != 0 => *bits &= !bit,
            _ => return Err(SchedulerError::UnknownSerial),
        }
        self.unhandled -= 1;
        self.reasons.remove(&id);
        Ok(())
    }
    fn tracking(id: u64) -> (usize, u64) {
        (
            (id / TRACKING_WORD_BITS) as usize,
            1 << (id % TRACKING_WORD_BITS),
        )
    }
    /// The earliest outstanding tracking id.
    fn earliest(&self) -> Option<u64> {
        self.outstanding
            .iter()
            .enumerate()
            .find(|(_, bits)| **bits != 0)
            .map(|(word, bits)| word as u64 * TRACKING_WORD_BITS + u64::from(bits.trailing_zeros()))
    }
    /// The next microtask, counted against the per-execution job budget.
    pub(crate) fn next(&mut self) -> Result<Option<i64>, SchedulerError> {
        let Some(token) = self.queue.pop_front() else {
            return Ok(None);
        };
        self.held.remove(&token);
        if self.dispatched >= MAX_EXECUTION_JOBS {
            return Err(SchedulerError::JobBudget);
        }
        self.dispatched += 1;
        Ok(Some(token))
    }
    /// The termination diagnostic for rejections still without a handler.
    pub(crate) fn unhandled_rejection(&self) -> Option<SchedulerError> {
        let earliest = self.earliest()?;
        Some(SchedulerError::UnhandledRejection {
            count: self.unhandled,
            reason: self
                .reasons
                .get(&earliest)
                .cloned()
                .unwrap_or(RejectionReason::NotRetained),
        })
    }
}

fn fatal(error: SchedulerError) -> wasmtime::Error {
    wasmtime::Error::new(error)
}

/// Scheduler bookkeeping is no host effect: it consumes guest fuel and the
/// scheduler's own named limits, and observes the execution deadline, but it
/// is not charged to the host-call budget.
fn deadline(context: &RuntimeContext) -> Result<(), wasmtime::Error> {
    if std::time::Instant::now() >= context.deadline {
        return Err(wasmtime::Error::msg("host operation budget exhausted"));
    }
    Ok(())
}

pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::PromiseReact.name(),
        |mut caller: Caller<'_, RuntimeContext>, serial: f64, token: i64| {
            deadline(caller.data())?;
            caller
                .data_mut()
                .scheduler
                .react(serial, token)
                .map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::PromiseSettle.name(),
        |mut caller: Caller<'_, RuntimeContext>, serial: f64| {
            deadline(caller.data())?;
            caller.data_mut().scheduler.settle(serial).map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::SchedulerEnqueue.name(),
        |mut caller: Caller<'_, RuntimeContext>, token: i64| {
            deadline(caller.data())?;
            caller.data_mut().scheduler.enqueue(token).map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::PromiseUnhandled.name(),
        |mut caller: Caller<'_, RuntimeContext>, reason: i64| -> Result<f64, wasmtime::Error> {
            deadline(caller.data())?;
            // Validate and render the reason now, with the uncaught-exception
            // readers bounded per string field, so the diagnostic never needs a
            // guest reference later and no host copy grows with guest text.
            let channel =
                Channel::detect(|name| matches!(caller.get_export(name), Some(Extern::Global(_))));
            let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                return Err(wasmtime::Error::msg("missing exception memory"));
            };
            let (value, truncated) = channel
                .render_bounded(
                    memory.data(&caller),
                    reason as u64,
                    MAX_REJECTION_REASON_UNITS,
                )
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            let reason = if caller.data().scheduler.retains_reason() {
                RejectionReason::render(&value, truncated)
            } else {
                RejectionReason::NotRetained
            };
            caller.data_mut().scheduler.unhandled(reason).map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::PromiseHandled.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            caller.data_mut().scheduler.handled(id).map_err(fatal)
        },
    )?;
    Ok(())
}

/// Run the microtask checkpoint after the entry point returned: every job
/// through `varta_resume` with a `Normal` completion and an `undefined` input,
/// until the queue is empty; then apply the unhandled-rejection policy.
pub(crate) fn checkpoint(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
) -> Result<(), RuntimeError> {
    let mut resume = None;
    while let Some(token) = store.data_mut().scheduler.next().map_err(fatal)? {
        if resume.is_none() {
            resume = Some(
                instance
                    .get_typed_func::<(i64, i32, i64), i32>(&mut *store, frame::RESUME_EXPORT)?,
            );
        }
        let Some(dispatcher) = &resume else {
            return Err(fatal(SchedulerError::Outcome).into());
        };
        let outcome = dispatcher.call(&mut *store, (token, Completion::Normal as i32, 0))?;
        // Scheduled frames settle their promises themselves: they report
        // suspension or completion and leave the exception channel empty.
        if !matches!(
            Outcome::from_code(outcome),
            Some(Outcome::Suspended | Outcome::Returned)
        ) {
            return Err(fatal(SchedulerError::Outcome).into());
        }
        crate::exceptions::finish(instance, store)?;
    }
    match store.data().scheduler.unhandled_rejection() {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::exceptions::ThrownValue;

    #[test]
    fn reactions_queue_in_registration_order_and_serials_are_checked() {
        let mut scheduler = Scheduler::default();
        let first = scheduler.react(0.0, 11).unwrap();
        assert_eq!(scheduler.react(first, 12).unwrap(), first);
        let second = scheduler.react(0.0, 13).unwrap();
        assert_ne!(first, second);
        scheduler.enqueue(14).unwrap();
        scheduler.settle(second).unwrap();
        scheduler.settle(first).unwrap();
        let mut order = Vec::new();
        while let Some(token) = scheduler.next().unwrap() {
            order.push(token);
        }
        assert_eq!(order, [14, 13, 11, 12]);
        // A settled serial has no reactions left, and unknown serials fail.
        assert_eq!(scheduler.settle(first), Err(SchedulerError::UnknownSerial));
        for bad in [0.5, -1.0, f64::NAN, 99.0] {
            assert_eq!(scheduler.react(bad, 20), Err(SchedulerError::UnknownSerial));
        }
    }

    #[test]
    fn duplicate_tokens_and_queue_bounds_are_fatal() {
        let mut scheduler = Scheduler::default();
        scheduler.enqueue(1).unwrap();
        assert_eq!(scheduler.enqueue(1), Err(SchedulerError::DuplicateToken));
        let serial = scheduler.react(0.0, 2).unwrap();
        assert_eq!(
            scheduler.react(serial, 1),
            Err(SchedulerError::DuplicateToken)
        );
        let mut full = Scheduler::default();
        for token in 0..MAX_PENDING_CONTINUATIONS as i64 {
            full.enqueue(token).unwrap();
        }
        assert_eq!(
            full.enqueue(MAX_PENDING_CONTINUATIONS as i64),
            Err(SchedulerError::QueueFull)
        );
        // A dispatched token may be scheduled again (its frame suspended anew).
        assert_eq!(full.next().unwrap(), Some(0));
        full.enqueue(0).unwrap();
    }

    #[test]
    fn job_budget_and_tracking_ids_are_bounded() {
        let mut scheduler = Scheduler {
            dispatched: MAX_EXECUTION_JOBS,
            ..Scheduler::default()
        };
        scheduler.enqueue(5).unwrap();
        assert_eq!(scheduler.next(), Err(SchedulerError::JobBudget));
        let mut tracking = Scheduler::default();
        let number = RejectionReason::render(&ThrownValue::Number(1.0f64.to_bits()), false);
        let id = tracking.unhandled(number.clone()).unwrap();
        let null = RejectionReason::render(&ThrownValue::Null, false);
        tracking.unhandled(null.clone()).unwrap();
        assert_eq!(
            tracking.unhandled_rejection(),
            Some(SchedulerError::UnhandledRejection {
                count: 2,
                reason: number
            })
        );
        tracking.handled(id).unwrap();
        assert_eq!(tracking.handled(id), Err(SchedulerError::UnknownSerial));
        assert_eq!(
            tracking.unhandled_rejection(),
            Some(SchedulerError::UnhandledRejection {
                count: 1,
                reason: null
            })
        );
        // Ids never issued and ids beyond the tracked words are unknown.
        assert_eq!(tracking.handled(5.0), Err(SchedulerError::UnknownSerial));
    }

    /// Host memory held by unhandled reports is bounded by count and bytes:
    /// at most `MAX_RETAINED_REJECTION_REASONS` rendered reasons, and one
    /// tracking bit per id for every other report.
    #[test]
    fn unhandled_reports_retain_bounded_reasons() {
        let mut scheduler = Scheduler::default();
        let reason =
            |n: u32| RejectionReason::render(&ThrownValue::Number(f64::from(n).to_bits()), false);
        let reports = (MAX_RETAINED_REJECTION_REASONS * 4) as u32;
        let ids: Vec<f64> = (0..reports)
            .map(|n| {
                let kept = scheduler.retains_reason();
                let reason = if kept {
                    reason(n)
                } else {
                    RejectionReason::NotRetained
                };
                scheduler.unhandled(reason).unwrap()
            })
            .collect();
        assert_eq!(scheduler.reasons.len(), MAX_RETAINED_REJECTION_REASONS);
        assert_eq!(scheduler.unhandled, u64::from(reports));
        assert_eq!(
            scheduler.outstanding.len(),
            (u64::from(reports) / TRACKING_WORD_BITS + 1) as usize
        );
        // Withdrawing a kept report frees room for a later one.
        scheduler.handled(ids[1]).unwrap();
        assert!(scheduler.retains_reason());
        // The earliest remaining report is still named with its reason.
        let Some(SchedulerError::UnhandledRejection {
            count,
            reason: first,
        }) = scheduler.unhandled_rejection()
        else {
            panic!("unhandled");
        };
        assert_eq!((count, first), (u64::from(reports) - 1, reason(0)));
        // Once every kept report is withdrawn, the earliest remaining one has
        // only its tracking bit.
        for id in &ids[..MAX_RETAINED_REJECTION_REASONS] {
            let _ = scheduler.handled(*id);
        }
        assert!(matches!(
            scheduler.unhandled_rejection(),
            Some(SchedulerError::UnhandledRejection {
                reason: RejectionReason::NotRetained,
                ..
            })
        ));
        for id in &ids[MAX_RETAINED_REJECTION_REASONS..] {
            scheduler.handled(*id).unwrap();
        }
        assert_eq!(scheduler.unhandled_rejection(), None);
        assert!(scheduler.reasons.is_empty());
    }
}
