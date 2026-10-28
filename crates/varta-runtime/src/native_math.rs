//! ABI19 pure native math: exact required edge cases, approximated finite values.
use crate::RuntimeContext;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use varta_common::native_math::NativeMath;
use wasmtime::{Caller, Linker};

fn logarithm(x: f64, operation: fn(f64) -> f64) -> f64 {
    if x == 0.0 {
        f64::NEG_INFINITY
    } else if x < 0.0 {
        f64::NAN
    } else if x == 1.0 {
        0.0
    } else if x == f64::INFINITY {
        x
    } else {
        operation(x)
    }
}
fn unary(operation: NativeMath, x: f64) -> Result<f64, wasmtime::Error> {
    if x.is_nan() {
        return Ok(x);
    }
    use NativeMath::*;
    let result = match operation {
        Acos => {
            if x.abs() > 1.0 {
                f64::NAN
            } else if x == 0.0 {
                FRAC_PI_2
            } else if x == 1.0 {
                0.0
            } else if x == -1.0 {
                PI
            } else {
                x.acos()
            }
        }
        Acosh => {
            if x < 1.0 {
                f64::NAN
            } else if x == 1.0 {
                0.0
            } else if x.is_infinite() {
                x
            } else {
                x.acosh()
            }
        }
        Asin => {
            if x.abs() > 1.0 {
                f64::NAN
            } else if x == 0.0 {
                x
            } else if x.abs() == 1.0 {
                FRAC_PI_2.copysign(x)
            } else {
                x.asin()
            }
        }
        Asinh => {
            if x == 0.0 || x.is_infinite() {
                x
            } else {
                x.asinh()
            }
        }
        Atan => {
            if x == 0.0 {
                x
            } else if x.is_infinite() {
                FRAC_PI_2.copysign(x)
            } else {
                x.atan()
            }
        }
        Atanh => {
            if x.abs() > 1.0 {
                f64::NAN
            } else if x == 0.0 {
                x
            } else if x.abs() == 1.0 {
                f64::INFINITY.copysign(x)
            } else {
                x.atanh()
            }
        }
        Cbrt => {
            if x == 0.0 || x.is_infinite() {
                x
            } else {
                x.cbrt()
            }
        }
        Cos => {
            if x.is_infinite() {
                f64::NAN
            } else if x == 0.0 {
                1.0
            } else {
                x.cos()
            }
        }
        Cosh => {
            if x.is_infinite() {
                f64::INFINITY
            } else if x == 0.0 {
                1.0
            } else {
                x.cosh()
            }
        }
        Exp => {
            if x == f64::NEG_INFINITY {
                0.0
            } else if x == f64::INFINITY {
                x
            } else if x == 0.0 {
                1.0
            } else {
                x.exp()
            }
        }
        Expm1 => {
            if x == f64::NEG_INFINITY {
                -1.0
            } else if x == 0.0 || x == f64::INFINITY {
                x
            } else {
                x.exp_m1()
            }
        }
        Log => logarithm(x, f64::ln),
        Log10 => logarithm(x, f64::log10),
        Log2 => logarithm(x, f64::log2),
        Log1p => {
            if x < -1.0 {
                f64::NAN
            } else if x == -1.0 {
                f64::NEG_INFINITY
            } else if x == 0.0 || x == f64::INFINITY {
                x
            } else {
                x.ln_1p()
            }
        }
        Sin => {
            if x.is_infinite() {
                f64::NAN
            } else if x == 0.0 {
                x
            } else {
                x.sin()
            }
        }
        Sinh => {
            if x == 0.0 || x.is_infinite() {
                x
            } else {
                x.sinh()
            }
        }
        Tan => {
            if x.is_infinite() {
                f64::NAN
            } else if x == 0.0 {
                x
            } else {
                x.tan()
            }
        }
        Tanh => {
            if x == 0.0 {
                x
            } else if x.is_infinite() {
                1.0_f64.copysign(x)
            } else {
                x.tanh()
            }
        }
        Atan2 => {
            return Err(wasmtime::Error::msg(
                "binary native Math operation used as unary",
            ));
        }
    };
    Ok(result)
}
fn atan2(y: f64, x: f64) -> f64 {
    if y.is_nan() || x.is_nan() {
        return f64::NAN;
    }
    if y == 0.0 {
        return if x.is_sign_negative() {
            PI.copysign(y)
        } else {
            y
        };
    }
    if x == 0.0 {
        return FRAC_PI_2.copysign(y);
    }
    if y.is_infinite() {
        let magnitude = if x.is_infinite() {
            if x.is_sign_negative() {
                3.0 * FRAC_PI_4
            } else {
                FRAC_PI_4
            }
        } else {
            FRAC_PI_2
        };
        return magnitude.copysign(y);
    }
    if x.is_infinite() {
        return if x.is_sign_negative() {
            PI.copysign(y)
        } else {
            0.0_f64.copysign(y)
        };
    }
    y.atan2(x)
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    for &operation in NativeMath::ALL {
        if operation == NativeMath::Atan2 {
            linker.func_wrap(
                "varta",
                operation.host().name(),
                |mut caller: Caller<'_, RuntimeContext>,
                 y: f64,
                 x: f64|
                 -> Result<f64, wasmtime::Error> {
                    caller.data_mut().tick()?;
                    let result = atan2(y, x);
                    caller.data().check_budget()?;
                    Ok(result)
                },
            )?;
        } else {
            linker.func_wrap(
                "varta",
                operation.host().name(),
                move |mut caller: Caller<'_, RuntimeContext>,
                      x: f64|
                      -> Result<f64, wasmtime::Error> {
                    caller.data_mut().tick()?;
                    let result = unary(operation, x)?;
                    caller.data().check_budget()?;
                    Ok(result)
                },
            )?;
        }
    }
    Ok(())
}
