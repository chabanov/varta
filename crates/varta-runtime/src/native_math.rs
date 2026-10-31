//! ABI19 pure native math: the shared exact edge cases of `varta-intrinsics`
//! over the platform library's approximated finite values.
use crate::RuntimeContext;
use varta_common::native_math::NativeMath;
use varta_intrinsics::math::{self, Elementary, Unary};
use wasmtime::{Caller, Linker};

/// The platform `f64` library: earlier profiles keep its approximated finite
/// results; v38 links the portable `libm` backend instead.
pub(crate) struct Platform;
impl Elementary for Platform {
    fn acos(x: f64) -> f64 {
        x.acos()
    }
    fn acosh(x: f64) -> f64 {
        x.acosh()
    }
    fn asin(x: f64) -> f64 {
        x.asin()
    }
    fn asinh(x: f64) -> f64 {
        x.asinh()
    }
    fn atan(x: f64) -> f64 {
        x.atan()
    }
    fn atanh(x: f64) -> f64 {
        x.atanh()
    }
    fn atan2(y: f64, x: f64) -> f64 {
        y.atan2(x)
    }
    fn cbrt(x: f64) -> f64 {
        x.cbrt()
    }
    fn cos(x: f64) -> f64 {
        x.cos()
    }
    fn cosh(x: f64) -> f64 {
        x.cosh()
    }
    fn exp(x: f64) -> f64 {
        x.exp()
    }
    fn expm1(x: f64) -> f64 {
        x.exp_m1()
    }
    fn ln(x: f64) -> f64 {
        x.ln()
    }
    fn log10(x: f64) -> f64 {
        x.log10()
    }
    fn log2(x: f64) -> f64 {
        x.log2()
    }
    fn ln_1p(x: f64) -> f64 {
        x.ln_1p()
    }
    fn sin(x: f64) -> f64 {
        x.sin()
    }
    fn sinh(x: f64) -> f64 {
        x.sinh()
    }
    fn tan(x: f64) -> f64 {
        x.tan()
    }
    fn tanh(x: f64) -> f64 {
        x.tanh()
    }
    fn pow(x: f64, y: f64) -> f64 {
        x.powf(y)
    }
}
fn unary(operation: NativeMath, x: f64) -> Result<f64, wasmtime::Error> {
    use NativeMath::*;
    let operation = match operation {
        Acos => Unary::Acos,
        Acosh => Unary::Acosh,
        Asin => Unary::Asin,
        Asinh => Unary::Asinh,
        Atan => Unary::Atan,
        Atanh => Unary::Atanh,
        Cbrt => Unary::Cbrt,
        Cos => Unary::Cos,
        Cosh => Unary::Cosh,
        Exp => Unary::Exp,
        Expm1 => Unary::Expm1,
        Log => Unary::Log,
        Log10 => Unary::Log10,
        Log2 => Unary::Log2,
        Log1p => Unary::Log1p,
        Sin => Unary::Sin,
        Sinh => Unary::Sinh,
        Tan => Unary::Tan,
        Tanh => Unary::Tanh,
        Atan2 => {
            return Err(wasmtime::Error::msg(
                "binary native Math operation used as unary",
            ));
        }
    };
    Ok(math::unary::<Platform>(operation, x))
}
fn atan2(y: f64, x: f64) -> f64 {
    math::atan2::<Platform>(y, x)
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    for &operation in NativeMath::ALL {
        if operation == NativeMath::Atan2 {
            linker.func_wrap(
                varta_common::abi::HOST_MODULE,
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
                varta_common::abi::HOST_MODULE,
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
