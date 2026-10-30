//! Development fuel profiling (`varta dev --profile-fuel`,
//! [`varta_common::profile`]).
//!
//! The probes are linked only into a profiling execution. Each probe reads
//! the store's remaining fuel, refunds its own call sequence and attributes
//! the fuel consumed since the previous probe to the innermost open
//! activation. Probes are not host operations: they need no capability, do
//! not count against the host-call budget and never touch guest memory.
use crate::{RuntimeContext, RuntimeError};
use varta_common::profile::{self, FuelProfile, FunctionFuel, Probe};
use wasmtime::{Caller, Linker};

/// Fuel of one probe's `i32.const id; call probe` under Wasmtime's default
/// operator costs (one unit per operator); `fuel_profile_tests` proves the
/// refund exact against the uninstrumented build.
pub(crate) const PROBE_FUEL: u64 = 2;

#[derive(Clone, Copy)]
struct Frame {
    id: usize,
    start: u64,
    children: u64,
}

#[derive(Default)]
struct Row {
    calls: u64,
    self_fuel: u64,
    total_fuel: u64,
    /// Open activations, so recursion adds its total once.
    open: u32,
}

pub(crate) struct Profiler {
    /// Fuel of the current turn when it began (v38 renews fuel per turn).
    turn_budget: u64,
    /// Fuel consumed by the turns before the current one.
    spent: u64,
    /// The fuel allowance of every turn so far, reported as the budget.
    allowance: u64,
    names: Vec<String>,
    rows: Vec<Row>,
    stack: Vec<Frame>,
    /// Fuel consumed while no activation was open, and where the last
    /// outermost activation ended.
    outside: u64,
    idle_since: u64,
    /// A probe sequence that does not match the instrumentation.
    malformed: bool,
}
impl Profiler {
    pub fn new(budget: u64, names: Vec<String>) -> Self {
        Self {
            turn_budget: budget,
            spent: 0,
            allowance: budget,
            rows: names.iter().map(|_| Row::default()).collect(),
            names,
            stack: Vec::new(),
            outside: 0,
            idle_since: 0,
            malformed: false,
        }
    }
    /// Fuel consumed since the execution began, across turns.
    fn consumed(&self, remaining: u64) -> u64 {
        self.spent
            .saturating_add(self.turn_budget.saturating_sub(remaining))
    }
    /// A later v38 turn begins with `fuel`: the fuel consumed so far stays
    /// attributed and the next turn's consumption continues from it.
    pub fn renew(&mut self, remaining: u64, fuel: u64) {
        self.spent = self.consumed(remaining);
        self.turn_budget = fuel;
        self.allowance = self.allowance.saturating_add(fuel);
    }
    fn enter(&mut self, id: i32, consumed: u64) {
        let Some(row) = usize::try_from(id)
            .ok()
            .and_then(|id| self.rows.get_mut(id))
        else {
            self.malformed = true;
            return;
        };
        row.calls += 1;
        row.open += 1;
        if self.stack.is_empty() {
            self.outside += consumed.saturating_sub(self.idle_since);
        }
        self.stack.push(Frame {
            id: id as usize,
            start: consumed,
            children: 0,
        });
    }
    fn exit(&mut self, id: i32, consumed: u64) {
        match self.stack.last() {
            Some(frame) if usize::try_from(id).is_ok_and(|id| id == frame.id) => {
                self.close(consumed);
            }
            _ => self.malformed = true,
        }
    }
    fn close(&mut self, consumed: u64) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let total = consumed.saturating_sub(frame.start);
        let row = &mut self.rows[frame.id];
        row.self_fuel += total.saturating_sub(frame.children);
        row.open -= 1;
        if row.open == 0 {
            row.total_fuel += total;
        }
        match self.stack.last_mut() {
            Some(parent) => parent.children += total,
            None => self.idle_since = consumed,
        }
    }
    /// Close every open activation at `remaining` fuel and report.
    pub fn finish(mut self, remaining: u64) -> Result<FuelProfile, RuntimeError> {
        if self.malformed {
            return Err(wasmtime::Error::msg("fuel profile probes do not match the build").into());
        }
        let consumed = self.consumed(remaining);
        let unfinished = self.stack.len() as u64;
        if self.stack.is_empty() {
            self.outside += consumed.saturating_sub(self.idle_since);
        }
        while !self.stack.is_empty() {
            self.close(consumed);
        }
        let mut functions: Vec<_> = self
            .names
            .into_iter()
            .zip(self.rows)
            .filter(|(_, row)| row.calls > 0)
            .map(|(name, row)| FunctionFuel {
                name,
                calls: row.calls,
                self_fuel: row.self_fuel,
                total_fuel: row.total_fuel,
            })
            .collect();
        functions.sort_by(|a, b| {
            b.self_fuel
                .cmp(&a.self_fuel)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(FuelProfile {
            consumed,
            budget: self.allowance,
            functions,
            outside: self.outside,
            unfinished,
        })
    }
}

/// The names section of a profile build; ordinary builds have none.
pub(crate) fn names(bytes: &[u8]) -> Result<Vec<String>, RuntimeError> {
    let mut names = None;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::CustomSection(section) =
            payload.map_err(|e| wasmtime::Error::msg(e.to_string()))?
            && section.name() == profile::NAMES_SECTION
        {
            if names.is_some() {
                return Err(wasmtime::Error::msg("duplicate fuel profile names").into());
            }
            names = Some(profile::decode_names(section.data()).map_err(wasmtime::Error::new)?);
        }
    }
    names.ok_or_else(|| wasmtime::Error::msg("not a fuel profile build").into())
}

/// Close the profile of a profiling execution at its remaining fuel; `None`
/// for every other execution.
pub(crate) fn finish(
    store: &mut wasmtime::Store<RuntimeContext>,
) -> Option<Result<FuelProfile, RuntimeError>> {
    let profiler = store.data_mut().profiler.take()?;
    Some(
        store
            .get_fuel()
            .map_err(RuntimeError::from)
            .and_then(|remaining| profiler.finish(remaining)),
    )
}

/// Link the probes of a profiling execution.
pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    for probe in Probe::ALL {
        linker.func_wrap(
            profile::MODULE,
            probe.name(),
            move |mut caller: Caller<'_, RuntimeContext>, id: i32| {
                let remaining = caller.get_fuel()?.saturating_add(PROBE_FUEL);
                caller.set_fuel(remaining)?;
                if let Some(profiler) = caller.data_mut().profiler.as_mut() {
                    let consumed = profiler.consumed(remaining);
                    match probe {
                        Probe::Enter => profiler.enter(id, consumed),
                        Probe::Exit => profiler.exit(id, consumed),
                    }
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}
