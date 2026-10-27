//! Scalar numeric ABI v2 and undefined logging in v3. No legacy layout is reinterpreted.
use crate::{GuestExit, HostFailure, RuntimeContext, host};
use varta_common::HostOperation as Host;
use wasmtime::{Caller, Linker};

fn checked_exit(value: f64) -> Result<i32, HostFailure> {
    if !value.is_finite()
        || value.fract() != 0.0
        || value < f64::from(i32::MIN)
        || value > f64::from(i32::MAX)
    {
        return Err(HostFailure::invalid());
    }
    Ok(value as i32)
}
pub(super) fn display(value: f64) -> String {
    if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        value.to_string()
    }
}
pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        Host::LogUndefined.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i32| {
            caller.data_mut().tick()?;
            if value != 0 {
                return Err(wasmtime::Error::msg("noncanonical undefined"));
            }
            caller.data_mut().emit("varta log: undefined".into())
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::LogFloat.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: f64| {
            caller.data_mut().tick()?;
            let value = display(value);
            caller.data_mut().emit(format!("varta log: {value}"))
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::FloatRemainder.name(),
        |mut caller: Caller<'_, RuntimeContext>, a: f64, b: f64| -> Result<f64, wasmtime::Error> {
            caller.data_mut().tick()?;
            // Truncating floating remainder, not IEEE nearest-integer remainder.
            // std's fmod semantics preserve the dividend's zero/sign and special cases.
            Ok(a % b)
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::ExitFloat.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: f64| -> Result<(), wasmtime::Error> {
            host::reset(&mut caller)?;
            if !caller.data().policy.enabled("process.exit") {
                return host::failure(&mut caller, HostFailure::denied("process.exit", "process"));
            }
            match checked_exit(value) {
                Ok(code) => Err(wasmtime::Error::new(GuestExit(code))),
                Err(error) => host::failure(&mut caller, error),
            }
        },
    )?;
    Ok(())
}
