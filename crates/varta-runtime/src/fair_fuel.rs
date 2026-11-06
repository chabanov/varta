//! V38 fair host fuel (`varta_common::fuel`): the published charge of every
//! host import's work, deducted from the store's fuel like guest
//! instructions. The call charge is taken by a store call hook before any
//! host import runs; work charges are taken where the work is known: string
//! arguments before the effect, results before they are published.
use crate::RuntimeContext;
use varta_common::fuel;
use wasmtime::{CallHook, Caller, Store, StoreContextMut, Trap};

/// The call charge the hook took for the host call in progress.
///
/// Wasmtime runs the call hook around libcalls as well as host imports,
/// among them the epoch check, which an engine's ticker or another Store's
/// deadline or cancellation can reach at any instruction. The epoch
/// callback refunds the charge ([`refund_libcall`]), so fuel never depends
/// on when epochs advance. Fuel that cannot cover the charge does not fail
/// at once (the call may be that libcall): it is emptied, a counted host
/// call fails before its effect ([`RuntimeContext::tick`]) and any other
/// call when it returns, like guest code running out of fuel.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallCharge {
    taken: u64,
    short: bool,
}
impl CallCharge {
    /// Whether the fuel could not cover the charge.
    pub(crate) fn short(self) -> bool {
        self.short
    }
}

/// Charge [`fuel::CALL`] on every call of a host import.
pub(crate) fn install(store: &mut Store<RuntimeContext>) {
    store.call_hook(|mut context: StoreContextMut<'_, RuntimeContext>, hook| {
        if !context.data().fair_fuel {
            return Ok(());
        }
        match hook {
            CallHook::CallingHost => {
                let remaining = context.get_fuel()?;
                let taken = remaining.min(fuel::CALL);
                context.set_fuel(remaining - taken)?;
                context.data_mut().call_charge = Some(CallCharge {
                    taken,
                    short: taken < fuel::CALL,
                });
            }
            CallHook::ReturningFromHost => {
                if context
                    .data_mut()
                    .call_charge
                    .take()
                    .is_some_and(CallCharge::short)
                {
                    return Err(wasmtime::Error::new(Trap::OutOfFuel));
                }
            }
            CallHook::CallingWasm | CallHook::ReturningFromWasm => {}
        }
        Ok(())
    });
}

/// The epoch callback runs inside the epoch libcall: give back the call
/// charge the hook took for it, which is no host import.
pub(crate) fn refund_libcall(
    context: &mut StoreContextMut<'_, RuntimeContext>,
) -> wasmtime::Result<()> {
    if let Some(charge) = context.data_mut().call_charge.take() {
        let remaining = context.get_fuel()?;
        context.set_fuel(remaining.saturating_add(charge.taken))?;
    }
    Ok(())
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
