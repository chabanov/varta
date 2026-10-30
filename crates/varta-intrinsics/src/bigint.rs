//! BigInt operations (ABI27/28), the value13+ primitive conversions and the
//! primitive abstract comparisons over checked cells: one implementation for
//! the host imports of v28–v37 and for v38 linked code.
//!
//! Every function reads guest memory through [`Memory`] and charges `budget`
//! exactly as the host imports always did; callers choose the budget (the
//! host derives it from the remaining fuel, linked code is bounded by fuel
//! itself) and the [`Limits`] of their profile. A recoverable error
//! ([`Error::recoverable_class`]) enters the guest exception channel; every
//! other error is fatal.
use crate::{Memory, primitive};
use alloc::vec::Vec;
use core::cmp::Ordering;
use varta_bigint::{Budget, Error, Integer};
use varta_common::{
    bigint::{self, BigIntView, BinaryOp, UnaryOp},
    text,
    value::{PrimitiveBox, ValueKind as K, string_range},
};

mod comparison;
pub use comparison::{compare as primitive_compare, equal as primitive_equal};

/// Bit capacity of one BigInt result: the payload limit after its metadata.
pub const MAX_BITS: u64 = ((bigint::MAX_PAYLOAD_BYTES - bigint::META_BYTES) * 8) as u64;

/// What a caller admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Bytes of one string argument or string cell.
    pub string_bytes: usize,
    /// Bytes of one string result (the result buffer).
    pub result_bytes: usize,
}

/// The integer of the managed BigInt `handle`.
pub fn integer<M: Memory + ?Sized>(
    memory: &M,
    handle: u64,
    budget: &mut Budget,
) -> Result<Integer, Error> {
    let payload = BigIntView::read_owned(memory, handle).map_err(|_| Error::InvalidPayload)?;
    Integer::from_payload(&payload, budget)
}
/// The BigInt of a borrowed UTF-16 string `(pointer, byte length)`.
fn string_integer<M: Memory + ?Sized>(
    memory: &M,
    pointer: i32,
    length: i32,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Integer, Error> {
    let pointer = usize::try_from(pointer).map_err(|_| Error::InvalidPayload)?;
    let length = usize::try_from(length).map_err(|_| Error::InvalidPayload)?;
    if length > limits.string_bytes || !length.is_multiple_of(2) {
        return Err(Error::InvalidPayload);
    }
    let bytes = memory
        .borrow(pointer, length)
        .ok_or(Error::InvalidPayload)?;
    if (length / 2) as u64 > budget.remaining_work() {
        return Err(Error::WorkBudget);
    }
    let units = text::units(&bytes).map_err(|_| Error::InvalidPayload)?;
    Integer::from_utf16(&units, budget)
}
/// A canonical primitive or BigInt cell (value13 and later), charged one
/// unit plus a quarter unit per BigInt payload byte.
fn primitive_argument<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<PrimitiveBox, Error> {
    let value = value(memory, cell, limits)?;
    budget.consume_work(
        1 + if value.kind == K::BigInt {
            (value.payload >> 32) / 4
        } else {
            0
        },
    )?;
    Ok(value)
}
/// A canonical primitive cell or a cell holding a canonical managed BigInt;
/// every other cell is a noncanonical payload.
pub fn value<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    limits: Limits,
) -> Result<PrimitiveBox, Error> {
    let raw = PrimitiveBox::raw(memory, cell).map_err(|_| Error::InvalidPayload)?;
    if raw.kind == K::BigInt {
        BigIntView::read_owned(memory, raw.payload).map_err(|_| Error::InvalidPayload)?;
        return Ok(raw);
    }
    PrimitiveBox::read_limited(memory, cell, limits.string_bytes).map_err(|_| Error::InvalidPayload)
}
/// `ToNumber` of a non-BigInt primitive cell.
fn implicit_number<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    value: PrimitiveBox,
    budget: &mut Budget,
    limits: Limits,
) -> Result<f64, Error> {
    if value.kind == K::BigInt {
        return Err(Error::Type);
    }
    if value.kind == K::String {
        budget.consume_work(value.payload >> 32)?;
    }
    primitive::to_number(memory, cell, limits.string_bytes).map_err(|_| Error::InvalidPayload)
}
/// `ToBigInt` of a primitive or BigInt cell.
fn implicit_bigint<M: Memory + ?Sized>(
    memory: &M,
    value: PrimitiveBox,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Integer, Error> {
    match value.kind {
        K::BigInt => integer(memory, value.payload, budget),
        K::Boolean => Integer::from_bool(value.payload != 0, budget),
        K::String => {
            let (pointer, length) = string_range(value.payload);
            string_integer(memory, pointer as i32, length as i32, budget, limits)
        }
        K::Float64 | K::Null | K::Undefined => Err(Error::Type),
        _ => Err(Error::InvalidPayload),
    }
}
/// The UTF-16LE digits of `value` in `radix` (truncated; 2..=36).
fn formatted(
    value: &Integer,
    radix: f64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let radix = if radix.is_nan() { 0.0 } else { truncate(radix) };
    if !(2.0..=36.0).contains(&radix) {
        return Err(Error::InvalidRadix);
    }
    let text = value.to_string_radix(radix as u32, budget)?;
    if text.len() > limits.result_bytes / 2 {
        return Err(Error::BitCapacity);
    }
    // Formatting and encoding both traverse the output, including zero.
    budget.consume_work(text.len() as u64)?;
    Ok(text::encode(&text))
}
/// `f64::trunc` for a radix argument (`libm`, exact).
fn truncate(value: f64) -> f64 {
    libm::trunc(value)
}
fn width_of(signed: i32) -> Result<bool, Error> {
    match signed {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::InvalidPayload),
    }
}
fn as_width(
    value: &Integer,
    signed: bool,
    width: u64,
    budget: &mut Budget,
) -> Result<Integer, Error> {
    if signed {
        value.as_int_n(width, budget)
    } else {
        value.as_uint_n(width, budget)
    }
}

/// `BigInt.asIntN`/`asUintN` over cells (`bigint_width_v2`).
pub fn width_v2<M: Memory + ?Sized>(
    memory: &M,
    signed: i32,
    width: u64,
    value: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let signed = width_of(signed)?;
    // Validate both transports before allowing a recoverable error.
    // Semantic conversions still occur in ToIndex/ToBigInt order.
    let width_arg = primitive_argument(memory, width, budget, limits)?;
    let value = primitive_argument(memory, value, budget, limits)?;
    let width =
        Integer::index_from_number(implicit_number(memory, width, width_arg, budget, limits)?)?;
    let value = implicit_bigint(memory, value, budget, limits)?;
    as_width(&value, signed, width, budget)?.to_payload(budget)
}
/// `BigInt.prototype.toString(radix)` with a radix cell (`bigint_to_string_v2`).
pub fn to_string_v2<M: Memory + ?Sized>(
    memory: &M,
    receiver: u64,
    radix: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let value = integer(memory, receiver, budget)?;
    let arg = primitive_argument(memory, radix, budget, limits)?;
    let radix = if arg.kind == K::Undefined {
        10.0
    } else {
        implicit_number(memory, radix, arg, budget, limits)?
    };
    formatted(&value, radix, budget, limits)
}
/// `BigInt.asIntN`/`asUintN` with a Number width (`bigint_width`).
pub fn width<M: Memory + ?Sized>(
    memory: &M,
    signed: i32,
    value: u64,
    width: f64,
    budget: &mut Budget,
) -> Result<Vec<u8>, Error> {
    let signed = width_of(signed)?;
    let width = Integer::index_from_number(width)?;
    let value = integer(memory, value, budget)?;
    as_width(&value, signed, width, budget)?.to_payload(budget)
}
/// One binary BigInt operator (`bigint_binary`).
pub fn binary<M: Memory + ?Sized>(
    memory: &M,
    op: i32,
    left: u64,
    right: u64,
    budget: &mut Budget,
) -> Result<Vec<u8>, Error> {
    let op = BinaryOp::from_tag(op as u32).ok_or(Error::InvalidPayload)?;
    let left = integer(memory, left, budget)?;
    let right = integer(memory, right, budget)?;
    left.binary(op, &right, budget)?.to_payload(budget)
}
/// One unary BigInt operator (`bigint_unary`).
pub fn unary<M: Memory + ?Sized>(
    memory: &M,
    op: i32,
    value: u64,
    budget: &mut Budget,
) -> Result<Vec<u8>, Error> {
    let op = UnaryOp::from_tag(op as u32).ok_or(Error::InvalidPayload)?;
    integer(memory, value, budget)?
        .unary(op, budget)?
        .to_payload(budget)
}
/// `BigInt(number)` (`bigint_from_number`).
pub fn from_number(value: f64, budget: &mut Budget) -> Result<Vec<u8>, Error> {
    Integer::from_number(value, budget)?.to_payload(budget)
}
/// `BigInt(string)` of a borrowed string (`bigint_from_string`).
pub fn from_string<M: Memory + ?Sized>(
    memory: &M,
    pointer: i32,
    length: i32,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    string_integer(memory, pointer, length, budget, limits)?.to_payload(budget)
}
/// `BigInt.prototype.toString(radix)` with a Number radix (`bigint_to_string`).
pub fn to_string<M: Memory + ?Sized>(
    memory: &M,
    value: u64,
    radix: f64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let value = integer(memory, value, budget)?;
    formatted(&value, radix, budget, limits)
}
/// `Number(bigint)` (`bigint_to_number`).
pub fn to_number<M: Memory + ?Sized>(
    memory: &M,
    value: u64,
    budget: &mut Budget,
) -> Result<f64, Error> {
    integer(memory, value, budget)?.to_number(budget)
}
/// The order of `-1`, `0` and `1` as a Number.
fn order(value: Ordering) -> f64 {
    value as i8 as f64
}
/// BigInt against a borrowed string; NaN when the string is no BigInt
/// (`bigint_compare_string`).
pub fn compare_string<M: Memory + ?Sized>(
    memory: &M,
    left: u64,
    pointer: i32,
    length: i32,
    budget: &mut Budget,
    limits: Limits,
) -> Result<f64, Error> {
    let left = integer(memory, left, budget)?;
    match string_integer(memory, pointer, length, budget, limits) {
        Ok(right) => Ok(order(left.compare(&right, budget)?)),
        Err(Error::Syntax) => Ok(f64::NAN),
        Err(error) => Err(error),
    }
}
/// Two BigInts (`bigint_compare`).
pub fn compare<M: Memory + ?Sized>(
    memory: &M,
    left: u64,
    right: u64,
    budget: &mut Budget,
) -> Result<f64, Error> {
    let left = integer(memory, left, budget)?;
    let right = integer(memory, right, budget)?;
    Ok(order(left.compare(&right, budget)?))
}
/// A BigInt against a Number; NaN when unordered (`bigint_compare_number`).
pub fn compare_number<M: Memory + ?Sized>(
    memory: &M,
    left: u64,
    right: f64,
    budget: &mut Budget,
) -> Result<f64, Error> {
    Ok(integer(memory, left, budget)?
        .compare_number(right, budget)?
        .map_or(f64::NAN, order))
}
/// `String(value)` of a primitive or BigInt cell (`primitive_to_string_v2`
/// to `_v5`).
pub fn primitive_to_string<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let value = value(memory, cell, limits)?;
    if value.kind == K::BigInt {
        let text = integer(memory, value.payload, budget)?.to_string_radix(10, budget)?;
        budget.consume_work(text.len() as u64)?;
        return Ok(text::encode(&text));
    }
    let length = (value.payload >> 32) as usize;
    if value.kind == K::String && length as u64 > budget.remaining_work() {
        return Err(Error::WorkBudget);
    }
    let bytes = primitive::to_string(memory, cell, limits.string_bytes)
        .map_err(|_| Error::InvalidPayload)?;
    budget.consume_work(bytes.len() as u64)?;
    Ok(bytes.into_owned())
}
/// `Number(value)` of a primitive or BigInt cell (`primitive_to_number_v2`
/// to `_v5`).
pub fn primitive_to_number<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<f64, Error> {
    let value = value(memory, cell, limits)?;
    if value.kind == K::BigInt {
        return integer(memory, value.payload, budget)?.to_number(budget);
    }
    if value.kind == K::String {
        budget.consume_work(value.payload >> 32)?;
    }
    primitive::to_number(memory, cell, limits.string_bytes).map_err(|_| Error::InvalidPayload)
}
/// `BigInt(value)` of a primitive or BigInt cell (`bigint_from_value` to
/// `_v4`).
pub fn from_value<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let value = value(memory, cell, limits)?;
    let value = match value.kind {
        K::BigInt => integer(memory, value.payload, budget)?,
        K::Float64 => Integer::from_number(f64::from_bits(value.payload), budget)?,
        K::Boolean => Integer::from_bool(value.payload != 0, budget)?,
        K::String => {
            let (pointer, length) = string_range(value.payload);
            string_integer(memory, pointer as i32, length as i32, budget, limits)?
        }
        K::Null | K::Undefined => return Err(Error::Type),
        _ => return Err(Error::InvalidPayload),
    };
    value.to_payload(budget)
}
