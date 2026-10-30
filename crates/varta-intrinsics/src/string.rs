//! UTF-16 string intrinsics: code-unit access and default case conversion.
//!
//! A string argument is read in place ([`Memory::borrow`]) after its range,
//! limit and length checks, and decoded unit by unit; only results are
//! copied. `limit` is the caller's string limit in bytes: the frozen
//! `abi::MAX_STRING_BYTES` for host imports of earlier profiles, the v38
//! data limit for linked code.
use crate::{IntrinsicError, Memory};
use alloc::{borrow::Cow, string::String, vec::Vec};

/// UTF-16LE code units per byte pair.
const UNIT_BYTES: usize = 2;

/// The UTF-16LE bytes of a borrowed guest string `(ptr, byte length)` of at
/// most `limit` bytes, in place where the memory lends them.
pub fn read<M: Memory + ?Sized>(
    memory: &M,
    ptr: i32,
    len: i32,
    limit: usize,
) -> Result<Cow<'_, [u8]>, IntrinsicError> {
    if ptr < 0 || len < 0 || len as usize > limit {
        return Err(IntrinsicError::Utf16Range);
    }
    let bytes = memory
        .borrow(ptr as usize, len as usize)
        .ok_or(IntrinsicError::Utf16Memory)?;
    if !bytes.len().is_multiple_of(UNIT_BYTES) {
        return Err(IntrinsicError::Utf16Length);
    }
    Ok(bytes)
}
/// The units of even-length UTF-16LE bytes.
pub fn units(bytes: &[u8]) -> impl Iterator<Item = u16> + '_ {
    bytes
        .as_chunks::<UNIT_BYTES>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
}
/// The unit at `index` of UTF-16LE bytes.
fn unit(bytes: &[u8], index: usize) -> u16 {
    u16::from_le_bytes([bytes[UNIT_BYTES * index], bytes[UNIT_BYTES * index + 1]])
}
/// The unit index `ToIntegerOrInfinity(position)` names, if in range.
#[must_use]
pub fn index(position: f64, length: usize) -> Option<usize> {
    let value = if position.is_nan() {
        0.0
    } else {
        libm::trunc(position)
    };
    if value < 0.0 || value >= length as f64 {
        None
    } else {
        Some(value as usize)
    }
}
/// `charAt` of UTF-16LE bytes: the unit's bytes, or empty out of range.
#[must_use]
pub fn char_at(bytes: &[u8], position: f64) -> Vec<u8> {
    index(position, bytes.len() / UNIT_BYTES)
        .map(|i| unit(bytes, i).to_le_bytes().to_vec())
        .unwrap_or_default()
}
/// `charCodeAt` of UTF-16LE bytes: the unit, or NaN out of range.
#[must_use]
pub fn char_code_at(bytes: &[u8], position: f64) -> f64 {
    index(position, bytes.len() / UNIT_BYTES).map_or(f64::NAN, |i| f64::from(unit(bytes, i)))
}
/// Default Unicode case conversion (`String.prototype.toUpperCase` and
/// `toLowerCase`, no locale): every run of well-formed UTF-16 is mapped as a
/// whole, so context rules such as the final sigma see the run; lone
/// surrogates are copied unchanged, as in ECMAScript.
#[must_use]
pub fn convert_case(units: &[u16], upper: bool) -> Vec<u16> {
    convert_units(units.iter().copied(), units.len(), upper)
}
fn convert_units(units: impl Iterator<Item = u16>, capacity: usize, upper: bool) -> Vec<u16> {
    let mut output = Vec::with_capacity(capacity);
    let mut run = String::new();
    let flush = |run: &mut String, output: &mut Vec<u16>| {
        let mapped = if upper {
            run.to_uppercase()
        } else {
            run.to_lowercase()
        };
        output.extend(mapped.encode_utf16());
        run.clear();
    };
    for decoded in char::decode_utf16(units) {
        match decoded {
            Ok(character) => run.push(character),
            Err(lone) => {
                flush(&mut run, &mut output);
                output.push(lone.unpaired_surrogate());
            }
        }
    }
    flush(&mut run, &mut output);
    output
}
/// [`convert_case`] of UTF-16LE bytes as result bytes of at most `limit`
/// bytes.
pub fn convert_case16(bytes: &[u8], upper: bool, limit: usize) -> Result<Vec<u8>, IntrinsicError> {
    let bytes: Vec<u8> = convert_units(units(bytes), bytes.len() / UNIT_BYTES, upper)
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    if bytes.len() > limit {
        return Err(IntrinsicError::CaseLimit);
    }
    Ok(bytes)
}
