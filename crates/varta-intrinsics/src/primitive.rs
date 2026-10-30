//! Primitive `ToNumber`/`ToString` of checked primitive cells, and the
//! numeric parsers' `ToString` input.
//!
//! Cells are read by `varta_common::value::PrimitiveBox`, the one reader of
//! the cell layout. `string_bytes` is the caller's string limit: the frozen
//! `abi::MAX_STRING_BYTES` for host imports of earlier profiles, the v38 data
//! limit for linked code.
use crate::{IntrinsicError, Memory, number::canonical_display, parse};
use alloc::vec::Vec;
use varta_common::{
    text,
    value::{PrimitiveBox, ValueKind as Kind, string_range},
};

/// A canonical primitive cell, or the shared noncanonical-cell failure.
fn cell<M: Memory + ?Sized>(
    memory: &M,
    handle: u64,
    string_bytes: usize,
) -> Result<PrimitiveBox, IntrinsicError> {
    PrimitiveBox::read_limited(memory, handle, string_bytes)
        .map_err(|_| IntrinsicError::PrimitiveCell)
}
/// The UTF-16LE bytes of a string cell's payload.
fn string_bytes<M: Memory + ?Sized>(memory: &M, payload: u64) -> Result<Vec<u8>, IntrinsicError> {
    let (ptr, len) = string_range(payload);
    // `PrimitiveBox::read_limited` already proved the range is inside memory.
    memory.bytes(ptr, len).ok_or(IntrinsicError::PrimitiveCell)
}
/// The ECMAScript spelling of a non-string primitive, if `kind` is one.
fn spelling(cell: PrimitiveBox) -> Option<Vec<u8>> {
    let text = match cell.kind {
        Kind::Float64 => canonical_display(f64::from_bits(cell.payload)),
        Kind::Boolean => if cell.payload == 0 { "false" } else { "true" }.into(),
        Kind::Null => "null".into(),
        Kind::Undefined => "undefined".into(),
        _ => return None,
    };
    Some(text::encode(&text))
}

/// `ToString` of a primitive cell as UTF-16LE bytes (`primitive_to_string*`).
pub fn to_string<M: Memory + ?Sized>(
    memory: &M,
    handle: u64,
    limit: usize,
) -> Result<Vec<u8>, IntrinsicError> {
    let cell = cell(memory, handle, limit)?;
    if cell.kind == Kind::String {
        return string_bytes(memory, cell.payload);
    }
    spelling(cell).ok_or(IntrinsicError::StringCell)
}
/// `ToNumber` of a primitive cell (`primitive_to_number*`).
pub fn to_number<M: Memory + ?Sized>(
    memory: &M,
    handle: u64,
    limit: usize,
) -> Result<f64, IntrinsicError> {
    let cell = cell(memory, handle, limit)?;
    Ok(match cell.kind {
        Kind::Float64 => f64::from_bits(cell.payload),
        Kind::Boolean => cell.payload as f64,
        Kind::Null => 0.0,
        Kind::Undefined => f64::NAN,
        Kind::String => parse::string_number(&string_bytes(memory, cell.payload)?),
        _ => return Err(IntrinsicError::NumberCell),
    })
}
/// The `ToString` input of `parseInt`/`parseFloat` as UTF-16LE bytes.
pub fn parse_input<M: Memory + ?Sized>(
    memory: &M,
    handle: u64,
    limit: usize,
) -> Result<Vec<u8>, IntrinsicError> {
    let cell = cell(memory, handle, limit)?;
    if cell.kind == Kind::String {
        return string_bytes(memory, cell.payload);
    }
    spelling(cell).ok_or(IntrinsicError::ParseCell)
}
/// `parseInt(input, radix)` of two primitive cells (`number_parse_int`).
pub fn parse_int<M: Memory + ?Sized>(
    memory: &M,
    input: u64,
    radix: u64,
    limit: usize,
) -> Result<f64, IntrinsicError> {
    let input = parse_input(memory, input, limit)?;
    let radix = parse::int32(to_number(memory, radix, limit)?);
    Ok(parse::integer(&input, radix))
}
/// `parseFloat(input)` of a primitive cell (`number_parse_float`).
pub fn parse_float<M: Memory + ?Sized>(
    memory: &M,
    input: u64,
    limit: usize,
) -> Result<f64, IntrinsicError> {
    Ok(parse::float(&parse_input(memory, input, limit)?))
}
