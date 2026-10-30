//! ECMAScript Math and Number::exponentiate over a chosen elementary library.
//!
//! The special cases ECMAScript requires (NaN, signed zeros, infinities and
//! domain boundaries) are decided here, before the library is called, so they
//! are exact for every backend. Ordinary finite results come from the
//! [`Elementary`] backend: the native runtime keeps the platform library for
//! earlier profiles (approximated, documented as host-dependent), and
//! [`Portable`] (`libm`, pure Rust) gives v38 the same bits on every host.
use core::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// The elementary functions an ECMAScript Math needs for finite inputs.
pub trait Elementary {
    fn acos(x: f64) -> f64;
    fn acosh(x: f64) -> f64;
    fn asin(x: f64) -> f64;
    fn asinh(x: f64) -> f64;
    fn atan(x: f64) -> f64;
    fn atanh(x: f64) -> f64;
    fn atan2(y: f64, x: f64) -> f64;
    fn cbrt(x: f64) -> f64;
    fn cos(x: f64) -> f64;
    fn cosh(x: f64) -> f64;
    fn exp(x: f64) -> f64;
    fn expm1(x: f64) -> f64;
    fn ln(x: f64) -> f64;
    fn log10(x: f64) -> f64;
    fn log2(x: f64) -> f64;
    fn ln_1p(x: f64) -> f64;
    fn sin(x: f64) -> f64;
    fn sinh(x: f64) -> f64;
    fn tan(x: f64) -> f64;
    fn tanh(x: f64) -> f64;
    fn pow(x: f64, y: f64) -> f64;
}

/// The pure-Rust `libm` backend: identical bits on every host and in Wasm.
pub struct Portable;
impl Elementary for Portable {
    fn acos(x: f64) -> f64 {
        libm::acos(x)
    }
    fn acosh(x: f64) -> f64 {
        libm::acosh(x)
    }
    fn asin(x: f64) -> f64 {
        libm::asin(x)
    }
    fn asinh(x: f64) -> f64 {
        libm::asinh(x)
    }
    fn atan(x: f64) -> f64 {
        libm::atan(x)
    }
    fn atanh(x: f64) -> f64 {
        libm::atanh(x)
    }
    fn atan2(y: f64, x: f64) -> f64 {
        libm::atan2(y, x)
    }
    fn cbrt(x: f64) -> f64 {
        libm::cbrt(x)
    }
    fn cos(x: f64) -> f64 {
        libm::cos(x)
    }
    fn cosh(x: f64) -> f64 {
        libm::cosh(x)
    }
    fn exp(x: f64) -> f64 {
        libm::exp(x)
    }
    fn expm1(x: f64) -> f64 {
        libm::expm1(x)
    }
    fn ln(x: f64) -> f64 {
        libm::log(x)
    }
    fn log10(x: f64) -> f64 {
        libm::log10(x)
    }
    fn log2(x: f64) -> f64 {
        libm::log2(x)
    }
    fn ln_1p(x: f64) -> f64 {
        libm::log1p(x)
    }
    fn sin(x: f64) -> f64 {
        libm::sin(x)
    }
    fn sinh(x: f64) -> f64 {
        libm::sinh(x)
    }
    fn tan(x: f64) -> f64 {
        libm::tan(x)
    }
    fn tanh(x: f64) -> f64 {
        libm::tanh(x)
    }
    fn pow(x: f64, y: f64) -> f64 {
        libm::pow(x, y)
    }
}

/// The unary Math functions with a host import (`number_<name>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unary {
    Acos,
    Acosh,
    Asin,
    Asinh,
    Atan,
    Atanh,
    Cbrt,
    Cos,
    Cosh,
    Exp,
    Expm1,
    Log,
    Log10,
    Log2,
    Log1p,
    Sin,
    Sinh,
    Tan,
    Tanh,
}

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

/// `Math.<operation>(x)` with the ECMAScript-required exact cases.
#[must_use]
pub fn unary<E: Elementary>(operation: Unary, x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    use Unary::*;
    match operation {
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
                E::acos(x)
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
                E::acosh(x)
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
                E::asin(x)
            }
        }
        Asinh => {
            if x == 0.0 || x.is_infinite() {
                x
            } else {
                E::asinh(x)
            }
        }
        Atan => {
            if x == 0.0 {
                x
            } else if x.is_infinite() {
                FRAC_PI_2.copysign(x)
            } else {
                E::atan(x)
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
                E::atanh(x)
            }
        }
        Cbrt => {
            if x == 0.0 || x.is_infinite() {
                x
            } else {
                E::cbrt(x)
            }
        }
        Cos => {
            if x.is_infinite() {
                f64::NAN
            } else if x == 0.0 {
                1.0
            } else {
                E::cos(x)
            }
        }
        Cosh => {
            if x.is_infinite() {
                f64::INFINITY
            } else if x == 0.0 {
                1.0
            } else {
                E::cosh(x)
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
                E::exp(x)
            }
        }
        Expm1 => {
            if x == f64::NEG_INFINITY {
                -1.0
            } else if x == 0.0 || x == f64::INFINITY {
                x
            } else {
                E::expm1(x)
            }
        }
        Log => logarithm(x, E::ln),
        Log10 => logarithm(x, E::log10),
        Log2 => logarithm(x, E::log2),
        Log1p => {
            if x < -1.0 {
                f64::NAN
            } else if x == -1.0 {
                f64::NEG_INFINITY
            } else if x == 0.0 || x == f64::INFINITY {
                x
            } else {
                E::ln_1p(x)
            }
        }
        Sin => {
            if x.is_infinite() {
                f64::NAN
            } else if x == 0.0 {
                x
            } else {
                E::sin(x)
            }
        }
        Sinh => {
            if x == 0.0 || x.is_infinite() {
                x
            } else {
                E::sinh(x)
            }
        }
        Tan => {
            if x.is_infinite() {
                f64::NAN
            } else if x == 0.0 {
                x
            } else {
                E::tan(x)
            }
        }
        Tanh => {
            if x == 0.0 {
                x
            } else if x.is_infinite() {
                1.0_f64.copysign(x)
            } else {
                E::tanh(x)
            }
        }
    }
}

/// `Math.atan2(y, x)` with the ECMAScript-required exact cases.
#[must_use]
pub fn atan2<E: Elementary>(y: f64, x: f64) -> f64 {
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
    E::atan2(y, x)
}

/// Whether `value` is an integer (finite with no fractional part).
fn integral(value: f64) -> bool {
    value.is_finite() && libm::trunc(value) == value
}

/// `Number::exponentiate`: its special cases precede the backend's `pow`.
#[must_use]
pub fn power<E: Elementary>(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() {
        return f64::NAN;
    }
    if exponent == 0.0 {
        return 1.0;
    }
    if base.is_nan() {
        return f64::NAN;
    }
    let odd = integral(exponent) && exponent.abs() % 2.0 == 1.0;
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
    if base < 0.0 && !integral(exponent) {
        return f64::NAN;
    }
    let magnitude = E::pow(base.abs(), exponent);
    if negative { -magnitude } else { magnitude }
}

/// The truncating remainder (`%`, C `fmod`), exact on every host.
#[must_use]
pub fn remainder(dividend: f64, divisor: f64) -> f64 {
    libm::fmod(dividend, divisor)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn required_cases_are_exact_for_the_portable_backend() {
        assert!(unary::<Portable>(Unary::Acos, 2.0).is_nan());
        assert_eq!(
            unary::<Portable>(Unary::Sin, -0.0).to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(unary::<Portable>(Unary::Log, 0.0), f64::NEG_INFINITY);
        assert_eq!(atan2::<Portable>(0.0, -0.0), PI);
        assert!(power::<Portable>(-8.0, 1.0 / 3.0).is_nan());
        assert_eq!(power::<Portable>(-2.0, 3.0), -8.0);
        assert!(power::<Portable>(1.0, f64::INFINITY).is_nan());
        assert_eq!(remainder(-5.5, 2.0).to_bits(), (-1.5_f64).to_bits());
        assert_eq!(remainder(-0.0, 2.0).to_bits(), (-0.0_f64).to_bits());
        assert!(remainder(1.0, 0.0).is_nan());
    }
}
