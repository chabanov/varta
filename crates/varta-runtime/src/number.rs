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
/// Number::exponentiate special cases precede the platform's approximated pow.
fn power(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() {
        return f64::NAN;
    }
    if exponent == 0.0 {
        return 1.0;
    }
    if base.is_nan() {
        return f64::NAN;
    }
    let odd = exponent.is_finite() && exponent.fract() == 0.0 && exponent.abs() % 2.0 == 1.0;
    let negative = base.is_sign_negative() && odd;
    if base.is_infinite() {
        let magnitude = if exponent > 0.0 { f64::INFINITY } else { 0.0 };
        return if negative { -magnitude } else { magnitude };
    }
    if base == 0.0 {
        let magnitude = if exponent > 0.0 { 0.0 } else { f64::INFINITY };
        return if negative { -magnitude } else { magnitude };
    }
    if exponent.is_infinite() {
        let magnitude = base.abs();
        if magnitude == 1.0 {
            return f64::NAN;
        }
        return if (magnitude > 1.0) == exponent.is_sign_positive() {
            f64::INFINITY
        } else {
            0.0
        };
    }
    if base < 0.0 && exponent.fract() != 0.0 {
        return f64::NAN;
    }
    let magnitude = base.abs().powf(exponent);
    if negative { -magnitude } else { magnitude }
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
