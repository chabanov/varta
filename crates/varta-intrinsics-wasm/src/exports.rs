//! One export per linked host operation, named and typed exactly like the
//! `varta.<name>` import it replaces (`HostOperation::name`/`scalar_signature`).
//!
//! Linked code is v38 code: string arguments and string cells are read
//! within the v38 data limit (`limits::V38_MAX_STRING_BYTES`); string
//! results are owned guest strings ([`crate::text`]), not the result buffer.
use crate::{
    arena::entry,
    guest::{self, Guest, checked},
    text::{filled, owned},
};
use varta_common::limits::V38_MAX_STRING_BYTES as STRING_BYTES;
use varta_intrinsics::json::Sink;
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
        owned(&varta_common::text::encode(&text))
    })
}
fn decimal(mode: Mode, value: f64, digits: f64) -> i32 {
    entry(|| owned(&checked(format::format16(mode, value, digits))))
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
        owned(&checked(primitive::to_string(
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
        owned(&string::char_at(&units, position))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn string_char_code_at16(ptr: i32, len: i32, position: f64) -> f64 {
    entry(|| {
        let units = checked(string::read(&Guest, ptr, len, STRING_BYTES));
        string::char_code_at(&units, position)
    })
}
/// Result units of case conversion kept in the reserved arena: a result up
/// to this length is mapped once and copied, a longer one mapped again into
/// its owned string. (8 KiB, half the reserved arena: the arena never
/// continues above the heap, which the result's allocation may claim.)
const CASE_BUFFERED_UNITS: usize = 4096;

/// Default case conversion into an owned string, streamed over the input
/// read in place: the first pass sizes the result (keeping up to
/// [`CASE_BUFFERED_UNITS`] of it), a second one fills a longer result.
/// Nothing proportional to the input is allocated besides the result, and
/// the input is rooted while the result is allocated.
fn case(ptr: i32, len: i32, upper: bool) -> i32 {
    entry(|| {
        let input = checked(string::read(&Guest, ptr, len, STRING_BYTES));
        let mut buffer = alloc::vec::Vec::new();
        if !input.is_empty() {
            buffer.reserve_exact(CASE_BUFFERED_UNITS);
        }
        let mut units = 0;
        string::map_case(&input, upper, &mut |mapped| {
            units += mapped.len();
            if units <= CASE_BUFFERED_UNITS {
                buffer.extend_from_slice(mapped);
            }
        });
        let roots = guest::root_top();
        guest::retain_root(((len as u64) << u32::BITS) | ptr as u32 as u64);
        let handle = filled(units, |fill| {
            if units <= CASE_BUFFERED_UNITS {
                fill.units(&buffer);
                return;
            }
            string::map_case(&input, upper, &mut |mapped| fill.units(mapped));
        });
        guest::reset_roots(roots);
        let (result, bytes) = varta_common::value::string_range(handle);
        guest::set_result_len(bytes as i32);
        result as i32
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
