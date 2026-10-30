//! UTF-16 string intrinsics: code-unit access and default case conversion.
use crate::{IntrinsicError, Memory, layout, text};
use alloc::{string::String, vec::Vec};

/// The units of a borrowed guest string `(ptr, byte length)`.
pub fn read<M: Memory + ?Sized>(
    memory: &M,
    ptr: i32,
    len: i32,
) -> Result<Vec<u16>, IntrinsicError> {
    if ptr < 0 || len < 0 || len as usize > layout::MAX_STRING_BYTES {
        return Err(IntrinsicError::Utf16Range);
    }
    let bytes = memory
        .bytes(ptr as usize, len as usize)
        .ok_or(IntrinsicError::Utf16Memory)?;
    text::units(&bytes)
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
/// `charAt`: the unit's UTF-16LE bytes, or empty out of range.
#[must_use]
pub fn char_at(units: &[u16], position: f64) -> Vec<u8> {
    index(position, units.len())
        .map(|i| units[i].to_le_bytes().to_vec())
        .unwrap_or_default()
}
/// `charCodeAt`: the unit, or NaN out of range.
#[must_use]
pub fn char_code_at(units: &[u16], position: f64) -> f64 {
    index(position, units.len()).map_or(f64::NAN, |i| f64::from(units[i]))
}
/// Default Unicode case conversion (`String.prototype.toUpperCase` and
/// `toLowerCase`, no locale): every run of well-formed UTF-16 is mapped as a
/// whole, so context rules such as the final sigma see the run; lone
/// surrogates are copied unchanged, as in ECMAScript.
#[must_use]
pub fn convert_case(units: &[u16], upper: bool) -> Vec<u16> {
    let mut output = Vec::with_capacity(units.len());
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
    for decoded in char::decode_utf16(units.iter().copied()) {
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
/// [`convert_case`] as UTF-16LE result bytes within the string limit.
pub fn convert_case16(units: &[u16], upper: bool) -> Result<Vec<u8>, IntrinsicError> {
    let bytes: Vec<u8> = convert_case(units, upper)
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    if bytes.len() > layout::MAX_STRING_BYTES {
        return Err(IntrinsicError::CaseLimit);
    }
    Ok(bytes)
}
