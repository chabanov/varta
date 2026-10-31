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
/// [`convert_case`] of UTF-16LE `bytes`, streamed: the converted code
/// units are passed to `emit` in order, and nothing proportional to the
/// input is allocated. V38 linked case mapping calls it twice, to size its
/// owned result and to fill it. Characters map one by one, as
/// `str::to_uppercase`/`to_lowercase` map them; the one contextual mapping,
/// a lowercased `Σ` at the end of a word (`Final_Sigma`), looks at the
/// characters around it within its run of well-formed UTF-16, as
/// [`convert_case`] does.
pub fn map_case(bytes: &[u8], upper: bool, emit: &mut impl FnMut(&[u16])) {
    let units = bytes.as_chunks::<UNIT_BYTES>().0;
    let mut index = 0;
    let mut encoded = [0u16; 2];
    while index < units.len() {
        let current = u16::from_le_bytes(units[index]);
        if current < 0x80 {
            let byte = current as u8;
            let mapped = if upper {
                byte.to_ascii_uppercase()
            } else {
                byte.to_ascii_lowercase()
            };
            emit(&[u16::from(mapped)]);
            index += 1;
            continue;
        }
        let Some((character, width)) = decode_at(units, index) else {
            // A lone surrogate is copied unchanged.
            emit(&[current]);
            index += 1;
            continue;
        };
        if !upper && character == SIGMA {
            let final_sigma = cased_before(units, index) && !cased_after(units, index + width);
            let mapped = if final_sigma {
                FINAL_SMALL_SIGMA
            } else {
                SMALL_SIGMA
            };
            emit(mapped.encode_utf16(&mut encoded));
        } else if upper {
            for mapped in character.to_uppercase() {
                emit(mapped.encode_utf16(&mut encoded));
            }
        } else {
            for mapped in character.to_lowercase() {
                emit(mapped.encode_utf16(&mut encoded));
            }
        }
        index += width;
    }
}
const SIGMA: char = '\u{3a3}';
const SMALL_SIGMA: char = '\u{3c3}';
const FINAL_SMALL_SIGMA: char = '\u{3c2}';
const HIGH_SURROGATES: core::ops::Range<u16> = 0xD800..0xDC00;
const LOW_SURROGATES: core::ops::Range<u16> = 0xDC00..0xE000;
/// The character starting at unit `index` and its width in units, or `None`
/// at a lone surrogate.
fn decode_at(units: &[[u8; UNIT_BYTES]], index: usize) -> Option<(char, usize)> {
    let first = u16::from_le_bytes(*units.get(index)?);
    if HIGH_SURROGATES.contains(&first) {
        let second = u16::from_le_bytes(*units.get(index + 1)?);
        if !LOW_SURROGATES.contains(&second) {
            return None;
        }
        return char::decode_utf16([first, second])
            .next()?
            .ok()
            .map(|character| (character, 2));
    }
    char::from_u32(u32::from(first)).map(|character| (character, 1))
}
/// The character ending just before unit `end` and its width, or `None` at
/// the start or a lone surrogate (the end of the run).
fn decode_before(units: &[[u8; UNIT_BYTES]], end: usize) -> Option<(char, usize)> {
    let last = u16::from_le_bytes(*units.get(end.checked_sub(1)?)?);
    if LOW_SURROGATES.contains(&last)
        && let Some(start) = end.checked_sub(2)
        && HIGH_SURROGATES.contains(&u16::from_le_bytes(units[start]))
    {
        return decode_at(units, start);
    }
    decode_at(units, end - 1).filter(|(_, width)| *width == 1)
}
/// How the `Final_Sigma` condition sees a character.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SigmaContext {
    /// `Case_Ignorable`: skipped.
    Ignorable,
    /// `Cased` and not case-ignorable.
    Cased,
    Other,
}
/// The character's [`SigmaContext`], as `str::to_lowercase` decides it: a
/// `Σ` after the character alone lowercases to `ς` exactly when it is cased
/// and not case-ignorable, and after `A` and the character when it is
/// case-ignorable or cased. (The Unicode properties themselves are private
/// to the standard library.)
fn sigma_context(character: char) -> SigmaContext {
    let final_after = |prefix: &[char]| {
        let mut text: String = prefix.iter().collect();
        text.push(SIGMA);
        text.to_lowercase().ends_with(FINAL_SMALL_SIGMA)
    };
    if final_after(&[character]) {
        SigmaContext::Cased
    } else if final_after(&['A', character]) {
        SigmaContext::Ignorable
    } else {
        SigmaContext::Other
    }
}
/// Whether a cased character precedes unit `end` across case-ignorable ones.
fn cased_before(units: &[[u8; UNIT_BYTES]], mut end: usize) -> bool {
    while let Some((character, width)) = decode_before(units, end) {
        match sigma_context(character) {
            SigmaContext::Ignorable => end -= width,
            context => return context == SigmaContext::Cased,
        }
    }
    false
}
/// Whether a cased character follows from unit `start` across
/// case-ignorable ones.
fn cased_after(units: &[[u8; UNIT_BYTES]], mut start: usize) -> bool {
    while let Some((character, width)) = decode_at(units, start) {
        match sigma_context(character) {
            SigmaContext::Ignorable => start += width,
            context => return context == SigmaContext::Cased,
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes(units: &[u16]) -> Vec<u8> {
        units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
    }
    fn streamed(units: &[u16], upper: bool) -> Vec<u16> {
        let mut out = Vec::new();
        map_case(&bytes(units), upper, &mut |mapped| {
            out.extend_from_slice(mapped)
        });
        out
    }
    fn check(units: &[u16]) {
        for upper in [true, false] {
            assert_eq!(
                streamed(units, upper),
                convert_case(units, upper),
                "{units:x?} upper={upper}"
            );
        }
    }
    #[test]
    fn streamed_case_mapping_matches_whole_run_conversion() {
        // Every BMP code unit alone, and around a capital sigma.
        for unit in 0..=u16::MAX {
            check(&[unit]);
            check(&[0x41, unit, 0x3a3]);
            check(&[0x3a3, unit, 0x41]);
            check(&[0x41, 0x3a3, unit]);
        }
        for text in [
            "ΣΑΣ ΑΣ. ΑΣ'Σ Σ",
            "ὈΔΥΣΣΕΎΣ",
            "Α\u{301}Σ\u{301} x",
            "AΣ\u{345}b",
            "İstanbul ß ﬃ ǅ",
            "a\u{10428}Σ \u{10400}Σ\u{1d165}",
        ] {
            check(&text.encode_utf16().collect::<Vec<_>>());
        }
        // Lone surrogates end runs; pairs and halves around sigmas.
        check(&[0x41, 0xd800, 0x3a3]);
        check(&[0x41, 0x3a3, 0xdc00, 0x41]);
        check(&[0xd801, 0xdc00, 0x3a3, 0xd801]);
        check(&[0xdc00, 0xd801, 0x3a3]);
        // Random mixes of letters, marks, ignorables, sigmas and surrogates.
        let alphabet: [u16; 16] = [
            0x41, 0x61, 0x3a3, 0x3c3, 0x301, 0x345, 0x27, 0x2e, 0x20, 0x31, 0xd801, 0xdc00, 0xdf00,
            0x130, 0xdf, 0x1c5,
        ];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let len = (state % 9) as usize;
            let units: Vec<u16> = (0..len)
                .map(|i| alphabet[((state >> (4 + 4 * i)) & 15) as usize])
                .collect();
            check(&units);
        }
    }
}
