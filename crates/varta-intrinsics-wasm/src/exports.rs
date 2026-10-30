//! One export per linked host operation, named and typed exactly like the
//! `varta.<name>` import it replaces (`HostOperation::name`/`scalar_signature`).
//!
//! Linked code is v38 code: string arguments and string cells are read
//! within the v38 data limit (`limits::V38_MAX_STRING_BYTES`); string
//! results still travel through the result buffer, whose size
//! (`abi::MAX_RESULT_BYTES`) bounds them.
use crate::{
    arena::entry,
    guest::{Guest, checked, result},
};
use varta_common::{abi::MAX_RESULT_BYTES, limits::V38_MAX_STRING_BYTES as STRING_BYTES};
use varta_intrinsics::{
    format::{self, Mode},
    math::{self, Portable, Unary},
    primitive, radix, string,
};

macro_rules! unary_math {
    ($($name:ident => $operation:ident),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: f64) -> f64 {
            math::unary::<Portable>(Unary::$operation, x)
        }
    )*};
}
unary_math! {
    number_acos => Acos,
    number_acosh => Acosh,
    number_asin => Asin,
    number_asinh => Asinh,
    number_atan => Atan,
    number_atanh => Atanh,
    number_cbrt => Cbrt,
    number_cos => Cos,
    number_cosh => Cosh,
    number_exp => Exp,
    number_expm1 => Expm1,
    number_log => Log,
    number_log10 => Log10,
    number_log2 => Log2,
    number_log1p => Log1p,
    number_sin => Sin,
    number_sinh => Sinh,
    number_tan => Tan,
    number_tanh => Tanh,
}

#[unsafe(no_mangle)]
pub extern "C" fn number_atan2(y: f64, x: f64) -> f64 {
    math::atan2::<Portable>(y, x)
}
#[unsafe(no_mangle)]
pub extern "C" fn number_power(base: f64, exponent: f64) -> f64 {
    math::power::<Portable>(base, exponent)
}
#[unsafe(no_mangle)]
pub extern "C" fn number_remainder(dividend: f64, divisor: f64) -> f64 {
    math::remainder(dividend, divisor)
}

#[unsafe(no_mangle)]
pub extern "C" fn number_to_string(value: f64, radix: f64) -> i32 {
    entry(|| {
        let text = checked(radix::format(value, radix));
        result(&varta_common::text::encode(&text))
    })
}
fn decimal(mode: Mode, value: f64, digits: f64) -> i32 {
    entry(|| result(&checked(format::format16(mode, value, digits))))
}
#[unsafe(no_mangle)]
pub extern "C" fn number_to_fixed(value: f64, digits: f64) -> i32 {
    decimal(Mode::Fixed, value, digits)
}
#[unsafe(no_mangle)]
pub extern "C" fn number_to_exponential(value: f64, digits: f64) -> i32 {
    decimal(Mode::Exponential, value, digits)
}
#[unsafe(no_mangle)]
pub extern "C" fn number_to_precision(value: f64, digits: f64) -> i32 {
    decimal(Mode::Precision, value, digits)
}

#[unsafe(no_mangle)]
pub extern "C" fn number_parse_float(input: i64) -> f64 {
    entry(|| checked(primitive::parse_float(&Guest, input as u64, STRING_BYTES)))
}
#[unsafe(no_mangle)]
pub extern "C" fn number_parse_int(input: i64, radix: i64) -> f64 {
    entry(|| {
        checked(primitive::parse_int(
            &Guest,
            input as u64,
            radix as u64,
            STRING_BYTES,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn primitive_to_string(cell: i64) -> i32 {
    entry(|| {
        result(&checked(primitive::to_string(
            &Guest,
            cell as u64,
            STRING_BYTES,
        )))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn primitive_to_number(cell: i64) -> f64 {
    entry(|| checked(primitive::to_number(&Guest, cell as u64, STRING_BYTES)))
}

#[unsafe(no_mangle)]
pub extern "C" fn string_char_at16(ptr: i32, len: i32, position: f64) -> i32 {
    entry(|| {
        let units = checked(string::read(&Guest, ptr, len, STRING_BYTES));
        result(&string::char_at(&units, position))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn string_char_code_at16(ptr: i32, len: i32, position: f64) -> f64 {
    entry(|| {
        let units = checked(string::read(&Guest, ptr, len, STRING_BYTES));
        string::char_code_at(&units, position)
    })
}
fn case(ptr: i32, len: i32, upper: bool) -> i32 {
    entry(|| {
        let units = checked(string::read(&Guest, ptr, len, STRING_BYTES));
        result(&checked(string::convert_case16(
            &units,
            upper,
            MAX_RESULT_BYTES,
        )))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn string_to_upper_case16(ptr: i32, len: i32) -> i32 {
    case(ptr, len, true)
}
#[unsafe(no_mangle)]
pub extern "C" fn string_to_lower_case16(ptr: i32, len: i32) -> i32 {
    case(ptr, len, false)
}
