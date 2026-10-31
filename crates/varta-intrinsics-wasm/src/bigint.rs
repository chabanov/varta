//! The BigInt operations (`varta_intrinsics::bigint`) as linked exports.
//!
//! An operation whose registry signature returns two i32 values (a result
//! pointer, or 0 and a recoverable error class) is exported returning one
//! i64 with the first value in its low word: stable Rust has no multi-value
//! C ABI, and the compiler's linker unpacks the pair in a synthesized adapter
//! of the registry signature. Every error without a recoverable class traps,
//! as it is fatal in the host. Work is bounded by fuel: the BigInt budget
//! only caps the result bits.
use crate::{
    arena::entry,
    guest::{Guest, fatal_bigint, result},
};
use varta_bigint::{Budget, Error};
use varta_common::{abi::MAX_RESULT_BYTES, limits::V38_MAX_STRING_BYTES};
use varta_intrinsics::bigint::{self as shared, Limits, MAX_BITS};

/// Linked code is v38 code: v38 string arguments, results in the result
/// buffer.
const LINKED: Limits = Limits {
    string_bytes: V38_MAX_STRING_BYTES,
    result_bytes: MAX_RESULT_BYTES,
};

fn budget() -> Budget {
    Budget::new(MAX_BITS, u64::MAX)
}
/// The packed `(pointer, 0)` or `(0, error class)` pair of a BigInt result
/// in the result buffer.
fn pair(outcome: Result<alloc::vec::Vec<u8>, Error>) -> i64 {
    packed(outcome, result)
}
/// [`pair`] of BigInt text, an owned guest string ([`crate::text`]); text
/// longer than the result buffer stays the `RangeError` it always was.
fn text_pair(outcome: Result<alloc::vec::Vec<u8>, Error>) -> i64 {
    packed(outcome, crate::text::owned)
}
fn packed(outcome: Result<alloc::vec::Vec<u8>, Error>, publish: impl FnOnce(&[u8]) -> i32) -> i64 {
    let (first, second) = match outcome {
        Ok(bytes) => (publish(&bytes), 0),
        Err(error) => match error.recoverable_class() {
            Some(class) => (0, class as i32),
            None => fatal_bigint(error),
        },
    };
    (u64::from(first as u32) | (u64::from(second as u32) << u32::BITS)) as i64
}
fn checked<T>(outcome: Result<T, Error>) -> T {
    outcome.unwrap_or_else(|error| fatal_bigint(error))
}

#[unsafe(no_mangle)]
pub extern "C" fn bigint_binary(op: i32, left: i64, right: i64) -> i64 {
    entry(|| {
        pair(shared::binary(
            &Guest,
            op,
            left as u64,
            right as u64,
            &mut budget(),
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_unary(op: i32, value: i64) -> i64 {
    entry(|| pair(shared::unary(&Guest, op, value as u64, &mut budget())))
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_width(signed: i32, value: i64, width: f64) -> i64 {
    entry(|| {
        pair(shared::width(
            &Guest,
            signed,
            value as u64,
            width,
            &mut budget(),
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_width_v2(signed: i32, width: i64, value: i64) -> i64 {
    entry(|| {
        pair(shared::width_v2(
            &Guest,
            signed,
            width as u64,
            value as u64,
            &mut budget(),
            LINKED,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_from_number(value: f64) -> i64 {
    entry(|| pair(shared::from_number(value, &mut budget())))
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_from_string(pointer: i32, length: i32) -> i64 {
    entry(|| {
        pair(shared::from_string(
            &Guest,
            pointer,
            length,
            &mut budget(),
            LINKED,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_to_string(value: i64, radix: f64) -> i64 {
    entry(|| {
        text_pair(shared::to_string(
            &Guest,
            value as u64,
            radix,
            &mut budget(),
            LINKED,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_to_string_v2(receiver: i64, radix: i64) -> i64 {
    entry(|| {
        text_pair(shared::to_string_v2(
            &Guest,
            receiver as u64,
            radix as u64,
            &mut budget(),
            LINKED,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_from_value_v5(cell: i64) -> i64 {
    entry(|| {
        pair(shared::from_value(
            &Guest,
            cell as u64,
            &mut budget(),
            LINKED,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_to_number(value: i64) -> f64 {
    entry(|| checked(shared::to_number(&Guest, value as u64, &mut budget())))
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_compare(left: i64, right: i64) -> f64 {
    entry(|| {
        checked(shared::compare(
            &Guest,
            left as u64,
            right as u64,
            &mut budget(),
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_compare_number(left: i64, right: f64) -> f64 {
    entry(|| {
        checked(shared::compare_number(
            &Guest,
            left as u64,
            right,
            &mut budget(),
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn bigint_compare_string(left: i64, pointer: i32, length: i32) -> f64 {
    entry(|| {
        checked(shared::compare_string(
            &Guest,
            left as u64,
            pointer,
            length,
            &mut budget(),
            LINKED,
        ))
    })
}
