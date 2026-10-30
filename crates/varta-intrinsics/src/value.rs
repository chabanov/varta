//! The structured-value readers (`value_typeof_*`, `value_truthy_*`,
//! `value_nullish_*`, `value_is_array_*`, `value_strict_equal_*`,
//! `value_nullish_equal_*`, `value_is_error_class_*`): one implementation for
//! the host imports of value layouts 8 to 17 and the linked value18 readers
//! of v38.
//!
//! Every operation first reads its cells with
//! `varta_common::value::ReferenceBox::read_in`, the one shallow reader of
//! reference cells (records, arrays, dictionaries with their value18 hash
//! table, Error objects, BigInts, closure envelopes and primitive cells),
//! under the caller's layout `version` and module bounds, so a noncanonical
//! cell is the same fatal error in every host. Strings are compared in place
//! where the memory lends them.
use crate::Memory;
use varta_bigint::{Budget, Error};
use varta_common::{
    bigint::BigIntView,
    value::{
        ErrorClass, ErrorObject, ModuleBounds, ReferenceBox, ValueAbiError, ValueKind as Kind,
        string_range,
    },
};

/// Why a value reader refused its input; every case is fatal.
#[derive(Debug)]
pub enum ValueError {
    /// A noncanonical cell or child.
    Abi(ValueAbiError),
    /// A failed BigInt comparison (its work budget, or a noncanonical leaf).
    BigInt(Error),
    /// A kind the operation does not define (never produced by a canonical
    /// cell of a supported layout).
    Kind,
    /// An unknown error class selector.
    Class,
}
impl From<ValueAbiError> for ValueError {
    fn from(error: ValueAbiError) -> Self {
        Self::Abi(error)
    }
}

/// The canonical reference cell `handle` of layout `version` in `bounds`.
pub fn read<M: Memory + ?Sized>(
    memory: &M,
    handle: u64,
    version: u32,
    bounds: ModuleBounds,
) -> Result<ReferenceBox, ValueAbiError> {
    ReferenceBox::read_in(memory, handle, version, bounds)
}
/// `typeof` of a read cell (`value_typeof_*`). Only value17 and later
/// readers accept function cells.
pub fn type_of(value: ReferenceBox) -> Result<&'static str, ValueError> {
    Ok(match value.kind {
        Kind::Error
        | Kind::Null
        | Kind::Record
        | Kind::Array
        | Kind::Dictionary
        | Kind::Promise => "object",
        Kind::Undefined => "undefined",
        Kind::String => "string",
        Kind::Float64 => "number",
        Kind::Boolean => "boolean",
        Kind::BigInt => "bigint",
        Kind::Function => "function",
        _ => return Err(ValueError::Kind),
    })
}
/// `null` or `undefined` (`value_nullish_*`).
#[must_use]
pub fn nullish(value: ReferenceBox) -> bool {
    matches!(value.kind, Kind::Null | Kind::Undefined)
}
/// `Array.isArray` (`value_is_array_*`).
#[must_use]
pub fn is_array(value: ReferenceBox) -> bool {
    value.kind == Kind::Array
}
/// ECMAScript `ToBoolean` of a read cell (`value_truthy_*`).
pub fn truthy<M: Memory + ?Sized>(memory: &M, value: ReferenceBox) -> Result<bool, ValueError> {
    Ok(match value.kind {
        Kind::Null | Kind::Undefined => false,
        Kind::Error
        | Kind::Record
        | Kind::Array
        | Kind::Dictionary
        | Kind::Function
        | Kind::Promise => true,
        Kind::BigInt => !BigIntView::check(memory, value.payload)?,
        Kind::Boolean => value.payload != 0,
        Kind::String => value.payload >> i32::BITS != 0,
        Kind::Float64 => {
            let n = f64::from_bits(value.payload);
            n != 0.0 && !n.is_nan()
        }
        _ => false,
    })
}
/// `a == null && b == null` (`value_nullish_equal_*`).
#[must_use]
pub fn nullish_equal(a: ReferenceBox, b: ReferenceBox) -> bool {
    nullish(a) && nullish(b)
}
/// `===` of two read cells (`value_strict_equal_*`): BigInts by value
/// (charging `budget`), Numbers by IEEE equality, strings by content and
/// every other kind by identity of its payload.
pub fn strict_equal<M: Memory + ?Sized>(
    memory: &M,
    a: ReferenceBox,
    b: ReferenceBox,
    budget: &mut Budget,
) -> Result<bool, ValueError> {
    if a.kind != b.kind {
        return Ok(false);
    }
    Ok(match a.kind {
        Kind::BigInt => {
            let left =
                crate::bigint::integer(memory, a.payload, budget).map_err(ValueError::BigInt)?;
            let right =
                crate::bigint::integer(memory, b.payload, budget).map_err(ValueError::BigInt)?;
            left.compare(&right, budget).map_err(ValueError::BigInt)? == core::cmp::Ordering::Equal
        }
        Kind::Float64 => f64::from_bits(a.payload) == f64::from_bits(b.payload),
        Kind::String => {
            let (left, left_length) = string_range(a.payload);
            let (right, right_length) = string_range(b.payload);
            // The checked cells proved both complete even-length ranges.
            left_length == right_length
                && memory.borrow(left, left_length).ok_or(ValueError::Kind)?
                    == memory.borrow(right, right_length).ok_or(ValueError::Kind)?
        }
        _ => a.payload == b.payload,
    })
}
/// `value instanceof C` for the built-in error class with tag `class`
/// (`value_is_error_class_*`): every Error object is an `Error`, and an
/// object of a subclass matches its own class.
pub fn is_error_class<M: Memory + ?Sized>(
    memory: &M,
    value: ReferenceBox,
    class: u32,
    version: u32,
    bounds: ModuleBounds,
) -> Result<bool, ValueError> {
    let class = ErrorClass::from_tag(class).ok_or(ValueError::Class)?;
    if value.kind != Kind::Error {
        return Ok(false);
    }
    let object = ErrorObject::read_in(memory, value.payload, version, bounds)?;
    Ok(class == ErrorClass::Error || object.class == class)
}
