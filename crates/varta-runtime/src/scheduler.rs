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
//!
//! Timers (v36 `timers`, K4.4 step 5) make the checkpoint an event loop: once
//! the microtask queue is empty and no rejection is unhandled, the loop
//! advances its loop time to the earliest timer, ordered by (due time,
//! sequence), waits until the wall clock reaches it and wakes the timer's
//! frame, then drains the microtasks again. Loop time starts at zero with the
//! execution and moves only to the due time of the timer that fires, so the
//! order of timers never depends on how fast guest code ran.
use crate::turns::TurnKind;
use crate::{RuntimeContext, RuntimeError, exceptions::Channel};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};
use varta_common::HostOperation;
use varta_common::function::frame::{self, Completion, Outcome};
use varta_common::scheduler::{
    MAX_EXECUTION_JOBS, MAX_PENDING_CONTINUATIONS, MAX_PENDING_TIMERS, MAX_PROMISE_SERIALS,
    MAX_REJECTION_REASON_UNITS, MAX_RETAINED_REJECTION_REASONS, RejectionReason, SchedulerError,
    TIMER_CAPABILITY, TIMER_INTERVAL, TIMER_UNREF,
};
use wasmtime::{Caller, Extern, Instance, Linker, Store};

/// One timer: it wakes the single reaction (the timer frame) of the
/// engine-internal pending promise with `serial`.
struct Timer {
    serial: u64,
    /// The normalized repeat delay of an interval.
    interval: Option<f64>,
    keep_alive: bool,
    /// The (due time, sequence) key while the timer is armed.
    armed: Option<(f64, u64)>,
    /// Fired and not yet taken by its frame.
    fired: bool,
    cleared: bool,
}

/// The timer queue of one execution, ordered by (due time, sequence).
#[derive(Default)]
struct Timers {
    entries: BTreeMap<u64, Timer>,
    by_serial: BTreeMap<u64, u64>,
    /// Armed timers: (due-time bits, sequence) -> id. Due times are finite
    /// and nonnegative, so their bit patterns order like the values.
    order: BTreeMap<(u64, u64), u64>,
    next_id: u64,
    next_sequence: u64,
    /// Loop time in milliseconds since the execution started.
    now: f64,
}
impl Timers {
    fn arm(&mut self, id: u64, due: f64) -> Result<(), SchedulerError> {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        let timer = self
            .entries
            .get_mut(&id)
            .ok_or(SchedulerError::TimerState)?;
        timer.armed = Some((due, sequence));
        self.order.insert((due.to_bits(), sequence), id);
        Ok(())
    }
    fn cleared(&self, serial: u64) -> bool {
        self.by_serial
            .get(&serial)
            .and_then(|id| self.entries.get(id))
            .is_some_and(|timer| timer.cleared)
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only virtual clock: when set, the event loop advances loop time
    /// without sleeping, so timer tests are fast and deterministic.
    /// Production builds always wait for the wall clock.
    static VIRTUAL_CLOCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Test-only: run `body` with the virtual clock.
#[cfg(test)]
pub(crate) fn with_virtual_clock<T>(body: impl FnOnce() -> T) -> T {
    VIRTUAL_CLOCK.with(|flag| flag.set(true));
    let result = body();
    VIRTUAL_CLOCK.with(|flag| flag.set(false));
    result
}
fn virtual_clock() -> bool {
    #[cfg(test)]
    {
        VIRTUAL_CLOCK.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// The profile's host tracking bounds (v38 `data_limits` derives them from
/// the memory budget); earlier profiles keep 64 tokens and 32 timers with
/// their frozen errors.
#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    continuations: usize,
    timers: usize,
}
impl Default for Bounds {
    fn default() -> Self {
        Self::from(varta_common::limits::DataLimits::LEGACY)
    }
}
impl From<varta_common::limits::DataLimits> for Bounds {
    fn from(limits: varta_common::limits::DataLimits) -> Self {
        Self {
            continuations: limits.pending_continuations,
            timers: limits.pending_timers,
        }
    }
}
impl Bounds {
    fn queue_full(self) -> SchedulerError {
        if self.continuations == MAX_PENDING_CONTINUATIONS {
            SchedulerError::QueueFull
        } else {
            SchedulerError::ContinuationLimit {
                limit: self.continuations,
            }
        }
    }
    fn timers_full(self) -> SchedulerError {
        if self.timers == MAX_PENDING_TIMERS {
            SchedulerError::TimerBudget
        } else {
            SchedulerError::TimerLimit { limit: self.timers }
        }
    }
}

#[derive(Default)]
pub(crate) struct Scheduler {
    /// Host tracking bounds of the instantiated module's profile.
    bounds: Bounds,
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
    timers: Timers,
}
const TRACKING_WORD_BITS: u64 = u64::BITS as u64;
/// Loop time is kept in milliseconds, like JavaScript timer delays.
const MILLISECONDS_PER_SECOND: f64 = 1000.0;

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
    /// Adopt the instantiated module's profile bounds (before any guest code).
    pub(crate) fn set_bounds(&mut self, limits: varta_common::limits::DataLimits) {
        self.bounds = limits.into();
    }
    fn hold(&mut self, token: i64) -> Result<(), SchedulerError> {
        if self.held.len() >= self.bounds.continuations {
            return Err(self.bounds.queue_full());
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
        // A cleared timer releases its frame at once when the frame awaits
        // again (an interval cleared by its own callback).
        if serial != 0 && !self.reactions.contains_key(&serial) && self.timers.cleared(serial) {
            self.enqueue(token)?;
            return Ok(serial as f64);
        }
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
    /// Microtasks dispatched in the current allowance.
    pub(crate) fn dispatched(&self) -> u64 {
        self.dispatched
    }
    /// A v38 turn begins with a fresh microtask allowance.
    pub(crate) fn renew(&mut self) {
        self.dispatched = 0;
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
    /// Arm a timer for the one reaction of the pending promise `serial`.
    /// The caller has checked the `time.timer` grant.
    pub(crate) fn timer_start(
        &mut self,
        serial: f64,
        delay: f64,
        flags: i32,
    ) -> Result<f64, SchedulerError> {
        if flags & !(TIMER_INTERVAL | TIMER_UNREF) != 0 {
            return Err(SchedulerError::TimerState);
        }
        let serial = self::serial(serial, self.next_serial)?;
        if serial == 0
            || self.reactions.get(&serial).map(Vec::len) != Some(1)
            || self.timers.by_serial.contains_key(&serial)
        {
            return Err(SchedulerError::TimerState);
        }
        if self.timers.entries.len() >= self.bounds.timers {
            return Err(self.bounds.timers_full());
        }
        if self.timers.next_id >= MAX_PROMISE_SERIALS {
            return Err(SchedulerError::SerialBudget);
        }
        self.timers.next_id += 1;
        let id = self.timers.next_id;
        let delay = varta_common::scheduler::timer_delay(delay);
        self.timers.entries.insert(
            id,
            Timer {
                serial,
                interval: (flags & TIMER_INTERVAL != 0).then_some(delay),
                keep_alive: flags & TIMER_UNREF == 0,
                armed: None,
                fired: false,
                cleared: false,
            },
        );
        self.timers.by_serial.insert(serial, id);
        let due = self.timers.now + delay;
        self.timers.arm(id, due)?;
        Ok(id as f64)
    }
    /// A v37 timer start the broker denied: nothing is armed and no id is
    /// issued; the frame suspended on `serial` is released at once, exactly
    /// like a cleared timer (`timer_take` answers 0), so the callback never
    /// runs and the guest's denial is a recoverable exception.
    pub(crate) fn timer_denied(&mut self, serial: f64) -> Result<(), SchedulerError> {
        let serial = self::serial(serial, self.next_serial)?;
        if serial == 0
            || self.reactions.get(&serial).map(Vec::len) != Some(1)
            || self.timers.by_serial.contains_key(&serial)
        {
            return Err(SchedulerError::TimerState);
        }
        if self.timers.entries.len() >= self.bounds.timers {
            return Err(self.bounds.timers_full());
        }
        // Released entries live under ids above every issued one and are
        // forgotten when taken, so they never collide with a timer id.
        let key = MAX_PROMISE_SERIALS + serial;
        self.timers.entries.insert(
            key,
            Timer {
                serial,
                interval: None,
                keep_alive: false,
                armed: None,
                fired: false,
                cleared: true,
            },
        );
        self.timers.by_serial.insert(serial, key);
        self.wake(serial)
    }
    /// Clear the timer `id`: an armed timer wakes its frame for release.
    /// Ids that name no armed timer are ignored, as in JavaScript.
    pub(crate) fn timer_clear(&mut self, id: f64) -> Result<(), SchedulerError> {
        if !(1.0..=MAX_PROMISE_SERIALS as f64).contains(&id) || id.fract() != 0.0 {
            return Ok(());
        }
        let Some(timer) = self.timers.entries.get_mut(&(id as u64)) else {
            return Ok(());
        };
        let Some((due, sequence)) = timer.armed.take() else {
            return Ok(());
        };
        timer.cleared = true;
        let serial = timer.serial;
        self.timers.order.remove(&(due.to_bits(), sequence));
        // A frame running its interval callback has no reaction yet; it is
        // released when it awaits again (see `react`).
        if let Some(reactions) = self.reactions.remove(&serial) {
            self.queue.extend(reactions);
        }
        Ok(())
    }
    /// A woken timer frame asks whether its timer fired (1) or was cleared
    /// (0). A taken timeout and a cleared timer are forgotten.
    pub(crate) fn timer_take(&mut self, serial: f64) -> Result<i32, SchedulerError> {
        let serial = self::serial(serial, self.next_serial)?;
        let id = *self
            .timers
            .by_serial
            .get(&serial)
            .ok_or(SchedulerError::TimerState)?;
        let timer = self
            .timers
            .entries
            .get_mut(&id)
            .ok_or(SchedulerError::TimerState)?;
        let fired = if timer.fired {
            timer.fired = false;
            if timer.interval.is_some() && !timer.cleared {
                return Ok(1);
            }
            1
        } else if timer.cleared {
            0
        } else {
            return Err(SchedulerError::TimerState);
        };
        self.timers.entries.remove(&id);
        self.timers.by_serial.remove(&serial);
        Ok(fired)
    }
    /// The serial of an engine-internal pending promise with exactly one
    /// reaction that no timer owns: what a v37 network request may wake.
    pub(crate) fn watchable(&self, serial: f64) -> Result<u64, SchedulerError> {
        let serial = self::serial(serial, self.next_serial)?;
        if serial == 0
            || self.reactions.get(&serial).map(Vec::len) != Some(1)
            || self.timers.by_serial.contains_key(&serial)
        {
            return Err(SchedulerError::UnknownSerial);
        }
        Ok(serial)
    }
    /// Queue the reactions of `serial` without settling its promise (a
    /// network completion, like a fired timer).
    pub(crate) fn wake(&mut self, serial: u64) -> Result<(), SchedulerError> {
        let reactions = self
            .reactions
            .remove(&serial)
            .ok_or(SchedulerError::UnknownSerial)?;
        self.queue.extend(reactions);
        Ok(())
    }
    /// Loop time follows the wall clock once an external completion (a v37
    /// network response) arrived, so later timers are due relative to it.
    fn observe(&mut self, elapsed_ms: f64) {
        self.timers.now = self.timers.now.max(elapsed_ms);
    }
    /// The due time of the next timer to fire, while a timer that keeps the
    /// loop alive is armed, or any armed timer while other work (`busy`: a
    /// network request in flight) keeps it alive; unreferenced timers alone
    /// end the execution.
    pub(crate) fn next_due(&self, busy: bool) -> Option<f64> {
        let alive = busy
            || self
                .timers
                .order
                .values()
                .any(|id| self.timers.entries.get(id).is_some_and(|t| t.keep_alive));
        if !alive {
            return None;
        }
        self.timers
            .order
            .keys()
            .next()
            .map(|(bits, _)| f64::from_bits(*bits))
    }
    /// Fire the earliest timer: advance loop time to its due time, re-arm an
    /// interval and wake its frame.
    pub(crate) fn fire(&mut self) -> Result<(), SchedulerError> {
        let Some(((bits, _), id)) = self.timers.order.pop_first() else {
            return Err(SchedulerError::TimerState);
        };
        let due = f64::from_bits(bits);
        self.timers.now = self.timers.now.max(due);
        let timer = self
            .timers
            .entries
            .get_mut(&id)
            .ok_or(SchedulerError::TimerState)?;
        timer.armed = None;
        timer.fired = true;
        let (serial, interval) = (timer.serial, timer.interval);
        let reactions = self
            .reactions
            .remove(&serial)
            .ok_or(SchedulerError::TimerState)?;
        self.queue.extend(reactions);
        if let Some(interval) = interval {
            // The frame registers again after its callback, under the same
            // serial; Node re-arms an interval at its fire time plus its delay.
            self.reactions.insert(serial, Vec::new());
            self.timers.arm(id, due + interval)?;
        }
        Ok(())
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

/// Scheduler failures inside guest calls keep their typed error, and so does
/// an exception that escaped a timer callback (v36).
pub(crate) fn typed(error: RuntimeError) -> RuntimeError {
    let RuntimeError::Execution(source) = error else {
        return error;
    };
    let source = match source.downcast::<SchedulerError>() {
        Ok(error) => return RuntimeError::Scheduler(error),
        Err(source) => source,
    };
    let source = match source.downcast::<varta_common::net::NetError>() {
        Ok(error) => return RuntimeError::Net(error),
        Err(source) => source,
    };
    let source = match crate::view::typed(source) {
        Ok(error) => return error,
        Err(source) => source,
    };
    match source.downcast::<varta_common::exceptions::UncaughtException>() {
        Ok(error) => RuntimeError::Uncaught(error),
        Err(source) => RuntimeError::Execution(source),
    }
}

fn fatal(error: SchedulerError) -> wasmtime::Error {
    wasmtime::Error::new(error)
}

/// Render an exception that escaped a timer callback with the
/// uncaught-exception readers, as for one escaping `main`.
fn uncaught(caller: &mut Caller<'_, RuntimeContext>, reason: i64) -> wasmtime::Error {
    let channel =
        Channel::detect(|name| matches!(caller.get_export(name), Some(Extern::Global(_))))
            .in_module(caller.data().module);
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return wasmtime::Error::msg("missing exception memory");
    };
    match channel.render(memory.data(&*caller), reason as u64) {
        Ok(value) => wasmtime::Error::new(varta_common::exceptions::UncaughtException { value }),
        Err(error) => wasmtime::Error::msg(error.to_string()),
    }
}

/// Scheduler bookkeeping is no host effect: it consumes guest fuel and the
/// scheduler's own named limits, and observes the execution deadline, but it
/// is not charged to the host-call budget.
fn deadline(context: &RuntimeContext) -> Result<(), wasmtime::Error> {
    context.check_deadline()
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
                Channel::detect(|name| matches!(caller.get_export(name), Some(Extern::Global(_))))
                    .in_module(caller.data().module);
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
    register_timers(linker)
}

/// The v36 timer imports. Arming a timer is a host effect: it is charged to
/// the host-call budget and requires the exact `time.timer` grant, checked
/// before the timer exists. Clearing, taking and reporting are bookkeeping.
fn register_timers(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::TimerStart.name(),
        |mut caller: Caller<'_, RuntimeContext>, serial: f64, delay: f64, flags: i32| {
            caller.data_mut().tick()?;
            if !caller.data().policy.enabled(TIMER_CAPABILITY) {
                return Err(fatal(SchedulerError::TimerDenied));
            }
            caller
                .data_mut()
                .scheduler
                .timer_start(serial, delay, flags)
                .map_err(fatal)
        },
    )?;
    // V37: a denial is recoverable. The broker decides before anything is
    // armed; a denied start releases the frame, logs the denial and returns
    // 0, and the guest throws a `NotAllowedError`.
    linker.func_wrap(
        "varta",
        HostOperation::TimerStartV2.name(),
        |mut caller: Caller<'_, RuntimeContext>, serial: f64, delay: f64, flags: i32| {
            caller.data_mut().tick()?;
            if flags & !(TIMER_INTERVAL | TIMER_UNREF) != 0 {
                return Err(fatal(SchedulerError::TimerState));
            }
            let context = caller.data_mut();
            match context.broker.timer(&context.policy) {
                crate::permission::Decision::Granted(()) => context
                    .scheduler
                    .timer_start(serial, delay, flags)
                    .map_err(fatal),
                crate::permission::Decision::Denied => {
                    context.scheduler.timer_denied(serial).map_err(fatal)?;
                    context.emit(format!(
                        "varta log: <capability denied: {TIMER_CAPABILITY}> (timer)"
                    ))?;
                    Ok(0.0)
                }
            }
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::TimerClear.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            caller.data_mut().scheduler.timer_clear(id).map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::TimerTake.name(),
        |mut caller: Caller<'_, RuntimeContext>, serial: f64| {
            deadline(caller.data())?;
            caller
                .data_mut()
                .scheduler
                .timer_take(serial)
                .map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::TimerUncaught.name(),
        |mut caller: Caller<'_, RuntimeContext>, reason: i64| -> Result<(), wasmtime::Error> {
            deadline(caller.data())?;
            Err(uncaught(&mut caller, reason))
        },
    )?;
    Ok(())
}

/// The wall-clock instant of loop time `due` (milliseconds after the
/// execution started); now under the test-only virtual clock.
pub(crate) fn target(context: &RuntimeContext, due: f64) -> Instant {
    if virtual_clock() {
        return Instant::now();
    }
    context.started + Duration::from_secs_f64(due / MILLISECONDS_PER_SECOND)
}

/// Wait until the wall clock reaches loop time `due`, never past the
/// execution deadline.
fn wait(context: &RuntimeContext, due: f64) -> Result<(), wasmtime::Error> {
    let target = target(context, due);
    let now = Instant::now();
    if target > now {
        std::thread::sleep(target.min(context.deadline) - now);
    }
    if Instant::now() >= context.deadline {
        return Err(context.deadline_error());
    }
    Ok(())
}

/// Advance loop time to the wall clock after a network completion (v37); the
/// virtual clock of the runtime tests keeps loop time program-defined.
pub(crate) fn observed_wall_clock(context: &mut RuntimeContext) {
    if virtual_clock() {
        return;
    }
    let elapsed = Instant::now().saturating_duration_since(context.started);
    context
        .scheduler
        .observe(elapsed.as_secs_f64() * MILLISECONDS_PER_SECOND);
}

/// One turn while network requests are in flight (v37): a due timer fires
/// first (Node's timers phase precedes its poll phase); otherwise the loop
/// drives the transfers until a completion arrives, the next timer is due or
/// the deadline passes. A timer due beyond the deadline cannot fire, but a
/// completion may still clear it, so the wait continues until the deadline.
/// Returns the kind of task taken, if any.
fn network_turn(
    store: &mut Store<RuntimeContext>,
    due: Option<f64>,
) -> Result<Option<TurnKind>, RuntimeError> {
    let context = store.data_mut();
    let deadline = context.deadline;
    let timer = due.map(|due| (due, target(context, due)));
    let fires = |instant: Instant| instant < deadline;
    if let Some((_, at)) = timer
        && fires(at)
        && at <= Instant::now()
    {
        context.scheduler.fire().map_err(fatal)?;
        return Ok(Some(TurnKind::Timer));
    }
    let until = match timer {
        Some((_, at)) if fires(at) => at,
        _ => deadline,
    };
    if crate::net::wait(context, until)? {
        return Ok(Some(TurnKind::Network));
    }
    if Instant::now() >= deadline {
        return Err(context.deadline_error().into());
    }
    if timer.is_some_and(|(_, at)| fires(at) && at <= Instant::now()) {
        context.scheduler.fire().map_err(fatal)?;
        return Ok(Some(TurnKind::Timer));
    }
    Ok(None)
}

/// Run the event loop after the entry point returned: the microtask
/// checkpoint, the unhandled-rejection policy, then the earliest timer, until
/// no microtask and no timer that keeps the loop alive remains. A timer due
/// at or beyond the execution deadline can never fire and ends the execution
/// at once with a budget error instead of waiting for the deadline.
pub(crate) fn checkpoint(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
) -> Result<(), RuntimeError> {
    loop {
        drain(instance, store)?;
        if let Some(error) = store.data().scheduler.unhandled_rejection() {
            return Err(error.into());
        }
        let busy = store.data().net.alive();
        let due = store.data().scheduler.next_due(busy);
        if busy {
            if let Some(kind) = network_turn(store, due)? {
                crate::turns::boundary(instance, store, kind)?;
            }
            continue;
        }
        let Some(due) = due else {
            return Ok(());
        };
        if let Some(error) = beyond_deadline(store.data(), due) {
            return Err(error.into());
        }
        wait(store.data(), due)?;
        store.data_mut().scheduler.fire().map_err(fatal)?;
        crate::turns::boundary(instance, store, TurnKind::Timer)?;
    }
}

/// The budget error of a next timer due at loop time `due` (milliseconds) at
/// or beyond the end of a bounded execution; it reports the due time rounded
/// up to a whole millisecond. A long-lived instance without a lifetime bound
/// waits for any timer.
fn beyond_deadline(context: &RuntimeContext, due: f64) -> Option<SchedulerError> {
    let lifetime = context.lifetime?.saturating_duration_since(context.started);
    beyond_lifetime(lifetime, due)
}
fn beyond_lifetime(lifetime: Duration, due: f64) -> Option<SchedulerError> {
    (due >= lifetime.as_millis() as f64).then(|| SchedulerError::TimerDeadline {
        due_ms: due.ceil() as u64,
        deadline_seconds: lifetime.as_secs_f64().ceil() as u64,
    })
}

/// The microtask checkpoint: every job through `varta_resume` with a
/// `Normal` completion and an `undefined` input, until the queue is empty.
pub(crate) fn drain(
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::abi;
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

    /// V38 bounds come from the memory budget; earlier profiles keep 64/32
    /// and their frozen errors.
    #[test]
    fn v38_bounds_track_the_memory_budget_with_their_own_errors() {
        use varta_common::limits::{
            DataLimits, V38_MAX_PENDING_CONTINUATIONS, V38_MAX_PENDING_TIMERS,
        };
        let mut wide = Scheduler::default();
        wide.set_bounds(DataLimits::V38);
        for token in 0..V38_MAX_PENDING_CONTINUATIONS as i64 {
            wide.enqueue(token).unwrap();
        }
        assert_eq!(
            wide.enqueue(V38_MAX_PENDING_CONTINUATIONS as i64),
            Err(SchedulerError::ContinuationLimit {
                limit: V38_MAX_PENDING_CONTINUATIONS
            })
        );
        assert_eq!(
            SchedulerError::ContinuationLimit { limit: 131_072 }.to_string(),
            "microtask queue exceeds 131072 pending continuations"
        );
        let bounds = Bounds::from(DataLimits::V38);
        assert_eq!(bounds.timers, V38_MAX_PENDING_TIMERS);
        assert_eq!(
            bounds.timers_full(),
            SchedulerError::TimerLimit {
                limit: V38_MAX_PENDING_TIMERS
            }
        );
        let legacy = Bounds::default();
        assert_eq!(legacy.queue_full(), SchedulerError::QueueFull);
        assert_eq!(legacy.timers_full(), SchedulerError::TimerBudget);
        assert_eq!(
            SchedulerError::QueueFull.to_string(),
            "microtask queue exceeds 64 pending continuations"
        );
    }

    /// The deadline error reports the next due time rounded up to a whole
    /// millisecond, and only a due time at or past the deadline fails.
    #[test]
    fn timer_deadline_rounds_the_due_time_up() {
        let lifetime = Duration::from_secs(abi::DEADLINE_SECONDS);
        let deadline = lifetime.as_millis() as f64;
        assert_eq!(beyond_lifetime(lifetime, deadline - 0.25), None);
        assert_eq!(
            beyond_lifetime(lifetime, 5999.25),
            Some(SchedulerError::TimerDeadline {
                due_ms: 6000,
                deadline_seconds: abi::DEADLINE_SECONDS,
            })
        );
        assert_eq!(
            beyond_lifetime(lifetime, deadline),
            Some(SchedulerError::TimerDeadline {
                due_ms: deadline as u64,
                deadline_seconds: abi::DEADLINE_SECONDS,
            })
        );
    }

    /// Double faults report the first check in the documented order: a
    /// timer start validates its flags before its serial, and a reaction
    /// checks the queue bound before a duplicate token.
    #[test]
    fn double_faults_report_the_first_check() {
        let mut scheduler = Scheduler::default();
        let unissued = 7.0;
        let bad_flags = !(TIMER_INTERVAL | TIMER_UNREF);
        assert_eq!(
            scheduler.timer_start(unissued, 1.0, bad_flags),
            Err(SchedulerError::TimerState)
        );
        assert_eq!(
            scheduler.timer_start(unissued, 1.0, 0),
            Err(SchedulerError::UnknownSerial)
        );
        let mut full = Scheduler::default();
        let held: i64 = 0;
        let serial = full.react(0.0, held).unwrap();
        for token in 1..MAX_PENDING_CONTINUATIONS as i64 {
            full.enqueue(token).unwrap();
        }
        // `held` is already held and the queue is full: QueueFull wins.
        assert_eq!(full.react(serial, held), Err(SchedulerError::QueueFull));
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
