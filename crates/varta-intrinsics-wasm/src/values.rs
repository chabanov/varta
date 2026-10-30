//! The value18 readers and primitive conversions/comparisons of v38 as
//! linked exports (`varta_intrinsics::value`, `varta_intrinsics::bigint`).
//!
//! Cells are read under value layout 18 within this module's bounds
//! ([`crate::guest::bounds`]), exactly as the host readers read them; a
//! noncanonical cell traps. The compiler's inline fast paths (codegen v2)
//! answer the common primitive cases before these are called.
use crate::{
    arena::entry,
    guest::{Guest, bounds, fatal_bigint, fatal_value, result},
};
use varta_bigint::{Budget, Error};
use varta_common::{
    abi::MAX_RESULT_BYTES,
    limits::V38_MAX_STRING_BYTES,
    text,
    value::{ErrorClass, HASHED_DICTIONARY_VALUE_VERSION as VERSION, ReferenceBox},
};
use varta_intrinsics::{
    bigint::{self as shared, Limits, MAX_BITS},
    value::{self, ValueError},
};

/// Linked code is v38 code: v38 string arguments, results in the result
/// buffer.
const LINKED: Limits = Limits {
    string_bytes: V38_MAX_STRING_BYTES,
    result_bytes: MAX_RESULT_BYTES,
};

fn budget() -> Budget {
    Budget::new(MAX_BITS, u64::MAX)
}
fn fatal(_error: ValueError) -> ! {
    core::arch::wasm32::unreachable()
}
fn checked<T>(outcome: Result<T, ValueError>) -> T {
    outcome.unwrap_or_else(|error| fatal(error))
}
fn numeric<T>(outcome: Result<T, Error>) -> T {
    outcome.unwrap_or_else(|error| fatal_bigint(error))
}
/// The canonical value18 cell `handle`.
fn cell(handle: i64) -> ReferenceBox {
    value::read(&Guest, handle as u64, VERSION, bounds()).unwrap_or_else(|error| fatal_value(error))
}

#[unsafe(no_mangle)]
pub extern "C" fn value_typeof_v11(handle: i64) -> i32 {
    entry(|| result(&text::encode(checked(value::type_of(cell(handle))))))
}
#[unsafe(no_mangle)]
pub extern "C" fn value_truthy_v11(handle: i64) -> i32 {
    entry(|| i32::from(checked(value::truthy(&Guest, cell(handle)))))
}
#[unsafe(no_mangle)]
pub extern "C" fn value_nullish_v11(handle: i64) -> i32 {
    entry(|| i32::from(value::nullish(cell(handle))))
}
#[unsafe(no_mangle)]
pub extern "C" fn value_is_array_v11(handle: i64) -> i32 {
    entry(|| i32::from(value::is_array(cell(handle))))
}
#[unsafe(no_mangle)]
pub extern "C" fn value_strict_equal_v11(left: i64, right: i64) -> i32 {
    entry(|| {
        let (left, right) = (cell(left), cell(right));
        i32::from(checked(value::strict_equal(
            &Guest,
            left,
            right,
            &mut budget(),
        )))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn value_nullish_equal_v11(left: i64, right: i64) -> i32 {
    entry(|| {
        let (left, right) = (cell(left), cell(right));
        i32::from(value::nullish_equal(left, right))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn value_is_error_class_v7(handle: i64, class: i32) -> i32 {
    entry(|| {
        // The class selector is checked before the cell, as the host does.
        if ErrorClass::from_tag(class as u32).is_none() {
            fatal(ValueError::Class);
        }
        let value = cell(handle);
        i32::from(checked(value::is_error_class(
            &Guest,
            value,
            class as u32,
            VERSION,
            bounds(),
        )))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn primitive_to_string_v6(handle: i64) -> i32 {
    entry(|| {
        result(&numeric(shared::primitive_to_string(
            &Guest,
            handle as u64,
            &mut budget(),
            LINKED,
        )))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn primitive_to_number_v6(handle: i64) -> f64 {
    entry(|| {
        numeric(shared::primitive_to_number(
            &Guest,
            handle as u64,
            &mut budget(),
            LINKED,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn primitive_compare(left: i64, right: i64) -> f64 {
    entry(|| {
        numeric(shared::primitive_compare(
            &Guest,
            left as u64,
            right as u64,
            &mut budget(),
            LINKED,
        ))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn primitive_equal(strict: i32, left: i64, right: i64) -> i32 {
    entry(|| {
        i32::from(numeric(shared::primitive_equal(
            &Guest,
            strict,
            left as u64,
            right as u64,
            &mut budget(),
            LINKED,
        )))
    })
}
