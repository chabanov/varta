//! V38 fair host fuel (`varta_common::fuel`): the published charge of every
//! host import's work, deducted from the store's fuel like guest
//! instructions. The call charge is taken by a store call hook before any
//! host import runs; work charges are taken where the work is known: string
//! arguments before the effect, results before they are published.
use crate::RuntimeContext;
use varta_common::fuel;
use wasmtime::{CallHook, Caller, Store, StoreContextMut, Trap};

/// Charge [`fuel::CALL`] on every call of a host import.
pub(crate) fn install(store: &mut Store<RuntimeContext>) {
    store.call_hook(|mut context: StoreContextMut<'_, RuntimeContext>, hook| {
        if matches!(hook, CallHook::CallingHost) && context.data().fair_fuel {
            let remaining = context.get_fuel()?;
            take(remaining, fuel::CALL, |left| context.set_fuel(left))?;
        }
        Ok(())
    });
}

/// Deduct `amount` from `remaining`; an amount it cannot cover empties the
/// fuel and fails like guest code running out of fuel.
fn take(
    remaining: u64,
    amount: u64,
    mut set: impl FnMut(u64) -> wasmtime::Result<()>,
) -> wasmtime::Result<()> {
    match remaining.checked_sub(amount) {
        Some(left) => set(left),
        None => {
            set(0)?;
            Err(wasmtime::Error::new(Trap::OutOfFuel))
        }
    }
}

/// Charge `amount` of host work to the calling module, if it pays fair fuel.
pub(crate) fn charge(
    caller: &mut Caller<'_, RuntimeContext>,
    amount: u64,
) -> Result<(), wasmtime::Error> {
    if !caller.data().fair_fuel || amount == 0 {
        return Ok(());
    }
    let remaining = caller.get_fuel()?;
    take(remaining, amount, |left| caller.set_fuel(left))
}

/// Charge the bytes of a string argument or result.
pub(crate) fn bytes(
    caller: &mut Caller<'_, RuntimeContext>,
    len: usize,
) -> Result<(), wasmtime::Error> {
    charge(caller, fuel::work(len, fuel::BYTE))
}

#[cfg(test)]
#[path = "fair_fuel_tests.rs"]
mod tests;
