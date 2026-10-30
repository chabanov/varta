//! UTF-16LE string units, as stored by the UTF-16 profiles.
use crate::IntrinsicError;
use alloc::vec::Vec;

/// UTF-16LE bytes of well-formed text.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
/// The `u16` units of an even-length UTF-16LE byte string.
pub fn units(bytes: &[u8]) -> Result<Vec<u16>, IntrinsicError> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(IntrinsicError::Utf16Length);
    }
    Ok(pairs.iter().map(|p| u16::from_le_bytes(*p)).collect())
}
/// ECMAScript `WhiteSpace` and `LineTerminator` code units.
#[must_use]
pub fn ecmascript_whitespace(unit: u16) -> bool {
    matches!(unit,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}
