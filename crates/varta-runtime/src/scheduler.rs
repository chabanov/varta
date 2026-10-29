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
use varta_common::exceptions::ThrownValue;
use varta_common::function::frame::{self, Completion, Outcome};
use varta_common::scheduler::{
    MAX_EXECUTION_JOBS, MAX_PENDING_CONTINUATIONS, MAX_PROMISE_SERIALS, SchedulerError,
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
    /// Rejections without a handler by tracking id, with their rendered reason.
    unhandled: BTreeMap<u64, ThrownValue>,
    next_tracking: u64,
}

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
    pub(crate) fn unhandled(&mut self, reason: ThrownValue) -> Result<f64, SchedulerError> {
        if self.next_tracking >= MAX_PROMISE_SERIALS {
            return Err(SchedulerError::SerialBudget);
        }
        self.next_tracking += 1;
        self.unhandled.insert(self.next_tracking, reason);
        Ok(self.next_tracking as f64)
    }
    pub(crate) fn handled(&mut self, value: f64) -> Result<(), SchedulerError> {
        let id = serial(value, self.next_tracking)?;
        self.unhandled
            .remove(&id)
            .map(|_| ())
            .ok_or(SchedulerError::UnknownSerial)
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
        let (_, reason) = self.unhandled.first_key_value()?;
        Some(SchedulerError::UnhandledRejection {
            count: self.unhandled.len() as u64,
            reason: reason.clone(),
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
            // Render the reason now, with the uncaught-exception readers, so the
            // diagnostic never needs a guest reference later.
            let channel =
                Channel::detect(|name| matches!(caller.get_export(name), Some(Extern::Global(_))));
            let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                return Err(wasmtime::Error::msg("missing exception memory"));
            };
            let rendered = channel
                .render(memory.data(&caller), reason as u64)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
            caller
                .data_mut()
                .scheduler
                .unhandled(rendered)
                .map_err(fatal)
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
        let id = tracking
            .unhandled(ThrownValue::Number(1.0f64.to_bits()))
            .unwrap();
        tracking.unhandled(ThrownValue::Null).unwrap();
        assert!(matches!(
            tracking.unhandled_rejection(),
            Some(SchedulerError::UnhandledRejection {
                count: 2,
                reason: ThrownValue::Number(_)
            })
        ));
        tracking.handled(id).unwrap();
        assert_eq!(tracking.handled(id), Err(SchedulerError::UnknownSerial));
        assert!(matches!(
            tracking.unhandled_rejection(),
            Some(SchedulerError::UnhandledRejection {
                count: 1,
                reason: ThrownValue::Null
            })
        ));
    }
}
