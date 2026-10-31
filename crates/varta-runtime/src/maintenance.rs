//! Idle collection between turns (v38 `incremental_collection`, wave 3 (f)).
//!
//! When no task of a long-lived application instance is runnable, the
//! runtime gives the guest's collector one idle period of maintenance: it
//! calls the host-free [`IDLE_EXPORT`] repeatedly with
//! [`collector::IDLE_STEP_UNITS`] work units under a fresh maintenance
//! allowance ([`Budgets::maintenance_fuel`], [`Budgets::maintenance_deadline`]),
//! never a turn's fuel, until the guest reports that no work is due or that
//! its cycle completed, or until less than [`MAINTENANCE_STEP_FUEL`] remains.
//! The guest decides from its heap state alone, so the same turns always
//! give the same maintenance. The deadline is a hard bound only: reaching it
//! (or exhausting the fuel, which a well-formed module never does) terminates
//! the instance like an exhausted turn budget. Bounded runs, earlier profiles
//! and modules without the export run no maintenance.
//!
//! [`Budgets::maintenance_fuel`]: varta_common::budget::Budgets::maintenance_fuel
//! [`Budgets::maintenance_deadline`]: varta_common::budget::Budgets::maintenance_deadline
use crate::turns::{self, MaintenanceUsage, TurnKind};
use crate::{RuntimeContext, RuntimeError};
use std::time::Instant;
use varta_common::budget::MAINTENANCE_STEP_FUEL;
use varta_common::collector::{self, IDLE_DONE, IDLE_EXPORT, IDLE_MORE, IDLE_NONE};
use wasmtime::{Instance, Store};

/// Run one idle period's maintenance. An error terminates the instance.
pub(crate) fn idle(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
) -> Result<(), RuntimeError> {
    let budgets = store.data().budgets;
    if !store.data().turns.renew || budgets.maintenance_fuel == 0 {
        return Ok(());
    }
    let Ok(step) = instance.get_typed_func::<i32, i32>(&mut *store, IDLE_EXPORT) else {
        return Ok(());
    };
    let started = Instant::now();
    // The turn before the idle period is closed; its remaining fuel is not
    // carried over (a fuel profile keeps attributing across the boundary).
    let remaining = store.get_fuel()?;
    if let Some(profiler) = store.data_mut().profiler.as_mut() {
        profiler.renew(remaining, budgets.maintenance_fuel);
    }
    store.set_fuel(budgets.maintenance_fuel)?;
    let context = store.data_mut();
    let kind = context.turns.kind();
    context.turns.set_kind(TurnKind::Maintenance);
    let mut deadline = started + budgets.maintenance_deadline;
    if let Some(lifetime) = context.lifetime {
        deadline = deadline.min(lifetime);
    }
    context.deadline = deadline;
    context.rearm_deadline();
    store.set_epoch_deadline(1);
    let units = i32::try_from(step_units()).unwrap_or(i32::MAX);
    let mut steps = 0;
    let mut completed = false;
    let result = loop {
        if store.get_fuel()? < MAINTENANCE_STEP_FUEL {
            break Ok(());
        }
        match step.call(&mut *store, units) {
            Ok(IDLE_NONE) => break Ok(()),
            Ok(IDLE_MORE) => steps += 1,
            Ok(IDLE_DONE) => {
                steps += 1;
                completed = true;
                break Ok(());
            }
            Ok(_) => {
                break Err(RuntimeError::Execution(wasmtime::Error::msg(
                    "invalid idle collection status",
                )));
            }
            Err(error) => {
                steps += 1;
                break Err(turns::classify(error.into(), store.data()));
            }
        }
    };
    let memory_bytes = turns::memory_bytes(Some(instance), store);
    let fuel = budgets.maintenance_fuel - store.get_fuel().unwrap_or(0);
    let context = store.data_mut();
    context.turns.record_maintenance(MaintenanceUsage {
        after_turn: context.turns.current(),
        fuel,
        steps,
        completed,
        wall: started.elapsed(),
        memory_bytes,
    });
    if result.is_ok() {
        context.turns.set_kind(kind);
    }
    result
}

#[cfg(not(test))]
fn step_units() -> u32 {
    collector::IDLE_STEP_UNITS
}
#[cfg(test)]
thread_local! {
    static STEP_UNITS: std::cell::Cell<u32> = const { std::cell::Cell::new(collector::IDLE_STEP_UNITS) };
}
#[cfg(test)]
fn step_units() -> u32 {
    STEP_UNITS.with(std::cell::Cell::get)
}
/// Test-only: run `body` with idle steps of `units` work units.
#[cfg(test)]
pub(crate) fn with_step_units<T>(units: u32, body: impl FnOnce() -> T) -> T {
    let previous = STEP_UNITS.with(|cell| cell.replace(units));
    let result = body();
    STEP_UNITS.with(|cell| cell.set(previous));
    result
}
