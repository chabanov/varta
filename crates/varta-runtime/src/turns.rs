//! Event turns and per-turn budgets (v38 `turn_budgets`, increment P5).
//!
//! The first turn of an instance covers instantiation, `_start` and the
//! microtask checkpoint; every later turn takes exactly one task (a fired
//! timer, a network completion, a host event or a permission answer) and
//! drains the microtasks it queued. In a v38 instance each turn begins with a
//! fresh fuel, host-call and microtask allowance and, in a long-lived
//! application instance, a fresh deadline; memory and output stay per
//! instance. Earlier profiles keep one allowance for the whole execution and
//! report it as a single `execution` entry.
use crate::{RuntimeContext, RuntimeError};
use std::time::{Duration, Instant};
use varta_common::budget::{BudgetKind, Budgets, CappedBudget, MAX_REPORTED_TURNS};
use wasmtime::{Instance, Store};

/// What a turn ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnKind {
    /// Instantiation, `_start` and its microtasks.
    Main,
    /// A fired timer.
    Timer,
    /// A network completion.
    Network,
    /// A host event delivered by the embedder.
    HostEvent,
    /// The answer of a permission prompt.
    Permission,
    /// A whole execution of an earlier profile, whose budgets are not
    /// renewed per turn.
    Execution,
}
impl TurnKind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Timer => "timer",
            Self::Network => "network",
            Self::HostEvent => "event",
            Self::Permission => "permission",
            Self::Execution => "execution",
        }
    }
}

/// What one turn used. Per-function attribution of the fuel (a profiler) is
/// a later extension of this record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnUsage {
    /// 1-based turn number within the instance.
    pub turn: u64,
    pub kind: TurnKind,
    /// Wasm fuel (instructions) consumed by the turn.
    pub fuel: u64,
    /// Host calls charged to the turn.
    pub host_calls: u32,
    /// Microtasks dispatched by the turn.
    pub microtasks: u64,
    /// Wall-clock time of the turn.
    pub wall: Duration,
    /// Linear memory size at the end of the turn.
    pub memory_bytes: usize,
}

/// The budget report of an instance: its effective budgets, the requests the
/// host ceiling lowered, and the usage of each turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetReport {
    pub budgets: Budgets,
    pub capped: Vec<CappedBudget>,
    /// Whether fuel, host calls and microtasks are renewed per turn (v38).
    pub per_turn: bool,
    /// Whether the deadline is renewed per turn (a long-lived instance).
    pub deadline_per_turn: bool,
    /// The first [`MAX_REPORTED_TURNS`] turns.
    pub turns: Vec<TurnUsage>,
    pub total_turns: u64,
    pub total_fuel: u64,
    pub peak_memory_bytes: usize,
    pub output_bytes: usize,
}

/// A turn exhausted one of its budgets; the instance is terminated (a trap
/// kills the instance, and it is never restarted automatically).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "turn {turn} ({kind}) exhausted its {budget} budget ({limit}{unit}); the instance was terminated",
    kind = kind.name(),
    budget = budget.label(),
    unit = unit(*budget)
)]
pub struct BudgetExhausted {
    pub turn: u64,
    pub kind: TurnKind,
    pub budget: BudgetKind,
    pub limit: u64,
}
fn unit(budget: BudgetKind) -> &'static str {
    match budget {
        BudgetKind::Fuel => " instructions per turn",
        BudgetKind::Deadline => " ms per turn",
        BudgetKind::Memory => " bytes per instance",
        BudgetKind::HostCalls => " calls per turn",
        BudgetKind::Output => " bytes per instance",
    }
}

/// The legacy (earlier profile) texts of exhausted host-side budgets.
pub(crate) const HOST_BUDGET_EXHAUSTED: &str = "host operation budget exhausted";
pub(crate) const DEADLINE_EXCEEDED: &str = "execution deadline exceeded";
pub(crate) const OUTPUT_BUDGET_EXHAUSTED: &str = "output budget exhausted";

/// The turn ledger of one instance.
pub(crate) struct Turns {
    /// Fuel, host calls and microtasks renew per turn (v38).
    pub renew: bool,
    /// The deadline renews per turn (a long-lived application instance).
    pub renew_deadline: bool,
    pub capped: Vec<CappedBudget>,
    current: u64,
    kind: TurnKind,
    started: Instant,
    open: bool,
    report: Vec<TurnUsage>,
    last: Option<TurnUsage>,
    total_turns: u64,
    total_fuel: u64,
    peak_memory: usize,
}
impl Turns {
    pub fn new(renew: bool, renew_deadline: bool, capped: Vec<CappedBudget>) -> Self {
        Self {
            renew,
            renew_deadline,
            capped,
            current: 1,
            kind: if renew {
                TurnKind::Main
            } else {
                TurnKind::Execution
            },
            started: Instant::now(),
            open: true,
            report: Vec::new(),
            last: None,
            total_turns: 0,
            total_fuel: 0,
            peak_memory: 0,
        }
    }
    /// The number of the current (or last) turn.
    pub fn current(&self) -> u64 {
        self.current
    }
    /// The error for an exhausted budget of the current turn: typed in a v38
    /// instance, the frozen legacy text otherwise.
    pub fn exhausted(
        &self,
        budget: BudgetKind,
        budgets: &Budgets,
        legacy: &str,
    ) -> wasmtime::Error {
        if !self.renew {
            return wasmtime::Error::msg(legacy.to_owned());
        }
        wasmtime::Error::new(BudgetExhausted {
            turn: self.current,
            kind: self.kind,
            budget,
            limit: budgets.value(budget),
        })
    }
    fn record(&mut self, usage: TurnUsage) {
        self.total_turns += 1;
        self.total_fuel = self.total_fuel.saturating_add(usage.fuel);
        self.peak_memory = self.peak_memory.max(usage.memory_bytes);
        if self.report.len() < MAX_REPORTED_TURNS {
            self.report.push(usage.clone());
        }
        self.last = Some(usage);
    }
    /// The usage of the most recently closed turn.
    pub fn last(&self) -> Option<TurnUsage> {
        self.last.clone()
    }
    pub fn report(&self, budgets: Budgets, output_bytes: usize) -> BudgetReport {
        BudgetReport {
            budgets,
            capped: self.capped.clone(),
            per_turn: self.renew,
            deadline_per_turn: self.renew_deadline,
            turns: self.report.clone(),
            total_turns: self.total_turns,
            total_fuel: self.total_fuel,
            peak_memory_bytes: self.peak_memory,
            output_bytes,
        }
    }
}

fn memory_bytes(instance: Option<&Instance>, store: &mut Store<RuntimeContext>) -> usize {
    instance
        .and_then(|instance| instance.get_memory(&mut *store, "memory"))
        .map_or(0, |memory| memory.data_size(&*store))
}

/// Close the current turn and record its usage. Earlier profiles record one
/// entry when the execution ends.
pub(crate) fn close(instance: Option<&Instance>, store: &mut Store<RuntimeContext>) {
    if !store.data().turns.open {
        return;
    }
    let memory = memory_bytes(instance, store);
    let remaining = store.get_fuel().unwrap_or(0);
    let context = store.data_mut();
    let usage = TurnUsage {
        turn: context.turns.current,
        kind: context.turns.kind,
        fuel: context.budgets.fuel_per_turn.saturating_sub(remaining),
        host_calls: context.host_calls,
        microtasks: context.scheduler.dispatched(),
        wall: context.turns.started.elapsed(),
        memory_bytes: memory,
    };
    context.turns.open = false;
    context.turns.record(usage);
}

/// A task was taken: close the turn that ran before it and begin the next
/// one. Earlier profiles keep one allowance and record nothing here.
pub(crate) fn boundary(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
    kind: TurnKind,
) -> Result<(), RuntimeError> {
    if !store.data().turns.renew {
        return Ok(());
    }
    close(Some(instance), store);
    begin(store, kind)
}

/// Begin a turn of `kind` with a fresh allowance.
pub(crate) fn begin(store: &mut Store<RuntimeContext>, kind: TurnKind) -> Result<(), RuntimeError> {
    let fuel = store.data().budgets.fuel_per_turn;
    store.set_fuel(fuel)?;
    let context = store.data_mut();
    context.turns.current += 1;
    context.turns.kind = kind;
    context.turns.started = Instant::now();
    context.turns.open = true;
    context.host_calls = 0;
    context.scheduler.renew();
    if context.turns.renew_deadline {
        let mut deadline = Instant::now() + context.budgets.deadline_per_turn;
        if let Some(lifetime) = context.lifetime {
            deadline = deadline.min(lifetime);
        }
        context.deadline = deadline;
        context.rearm_deadline();
    }
    store.set_epoch_deadline(1);
    Ok(())
}

/// Give exhausted per-turn budgets of a v38 instance their typed error.
pub(crate) fn classify(error: RuntimeError, context: &RuntimeContext) -> RuntimeError {
    if !context.turns.renew {
        return error;
    }
    let RuntimeError::Execution(source) = error else {
        return error;
    };
    let source = match source.downcast::<BudgetExhausted>() {
        Ok(exhausted) => return RuntimeError::Budget(exhausted),
        Err(source) => source,
    };
    let budget = match source.downcast_ref::<wasmtime::Trap>() {
        Some(wasmtime::Trap::OutOfFuel) => BudgetKind::Fuel,
        Some(wasmtime::Trap::Interrupt) => BudgetKind::Deadline,
        _ => return RuntimeError::Execution(source),
    };
    RuntimeError::Budget(BudgetExhausted {
        turn: context.turns.current,
        kind: context.turns.kind,
        budget,
        limit: context.budgets.value(budget),
    })
}

/// How long a turn may still run (for transfers it starts): the end of the
/// current turn in a bounded execution, a fresh per-turn allowance from now
/// in a long-lived instance (a request outlives the turn that started it).
pub(crate) fn operation_deadline(context: &RuntimeContext) -> Instant {
    if context.turns.renew_deadline {
        let deadline = Instant::now() + context.budgets.deadline_per_turn;
        return context
            .lifetime
            .map_or(deadline, |lifetime| deadline.min(lifetime));
    }
    context.deadline
}
