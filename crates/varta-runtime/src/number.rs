//! Scalar numeric imports, undefined logging and versioned pure Number power.
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
/// Number::exponentiate: the shared special cases, then the platform's
/// approximated pow (v38 links the portable `libm` backend instead).
fn power(base: f64, exponent: f64) -> f64 {
    varta_intrinsics::math::power::<crate::native_math::Platform>(base, exponent)
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
pub(super) fn canonical_display(value: f64) -> String {
    varta_intrinsics::number::canonical_display(value)
}
pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        Host::FloatPower.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         base: f64,
         exponent: f64|
         -> Result<f64, wasmtime::Error> {
            caller.data_mut().tick()?;
            let value = power(base, exponent);
            // Recheck the deadline without charging a second host operation.
            caller.data().check_budget()?;
            Ok(value)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        Host::LogUndefined.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i32| {
            caller.data_mut().tick()?;
            if value != 0 {
                return Err(wasmtime::Error::msg("noncanonical undefined"));
            }
            caller.data_mut().emit("varta log: undefined".into())
        },
    )?;
    for operation in [Host::LogFloat, Host::LogFloatV2] {
        linker.func_wrap(
            varta_common::abi::HOST_MODULE,
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>, value: f64| {
                caller.data_mut().tick()?;
                let value = if operation == Host::LogFloatV2 {
                    canonical_display(value)
                } else {
                    display(value)
                };
                caller.data_mut().emit(format!("varta log: {value}"))
            },
        )?;
    }
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        Host::FloatRemainder.name(),
        |mut caller: Caller<'_, RuntimeContext>, a: f64, b: f64| -> Result<f64, wasmtime::Error> {
            caller.data_mut().tick()?;
            // Truncating floating remainder (C `fmod`), not IEEE
            // nearest-integer remainder; exact on every host.
            Ok(varta_intrinsics::math::remainder(a, b))
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
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
