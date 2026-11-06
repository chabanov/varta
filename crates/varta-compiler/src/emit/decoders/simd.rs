//! Fixed-width SIMD (`simd128`) steps of the scanner
//! ([`LanguageProfile::simd128`](varta_common::language::LanguageProfile::simd128)):
//! sixteen UTF-16 units per step, as two 16-byte loads narrowed to one byte
//! per unit.
//!
//! `i8x16.narrow_i16x8_s` saturates each unit to a signed byte: an ASCII
//! unit stays itself, a unit from `0x80` to `0x7fff` becomes `0x7f` and a
//! unit from `0x8000` up becomes `0x80`. Neither is a quote, a backslash, a
//! control character or whitespace, so the narrowed bytes classify the units
//! exactly: non-ASCII text needs no scalar step. Every SIMD instruction is
//! one ordinary Wasm operator, so its fuel is as deterministic as the scalar
//! scanner's.
use super::asm::{Asm, TEXT, at};
use wasm_encoder::Instruction as I;

/// Bytes of one `v128`.
pub(super) const VECTOR_BYTES: i32 = size_of::<u128>() as i32;
/// Input bytes of one step: two vectors, narrowed into one.
pub(super) const STEP_BYTES: i32 = 2 * VECTOR_BYTES;
/// The bits of an `i8x16.bitmask` whose sixteen lanes are all set.
pub(super) const ALL_LANES: i32 = (1 << VECTOR_BYTES) - 1;
/// `log2` of the bytes of one UTF-16 unit: a lane index to a byte offset.
pub(super) const UNIT_SHIFT: i32 = super::scan::UNIT_BYTES.trailing_zeros() as i32;

/// Bytes the quote test flips: `unit ^ QUOTE_FLIP` maps every control
/// character into `0..0x20` and the quote onto `0x20`, so one unsigned
/// comparison finds both.
const QUOTE_FLIP: u8 = b'"' ^ CONTROL_END;
const CONTROL_END: u8 = 0x20;
const BACKSLASH: u8 = b'\\';
const SPACE: u8 = b' ';
/// The JSON whitespace below 16 (`\t`, `\n`, `\r`), as an `i8x16.swizzle`
/// table: a lane of all ones at each such index.
const WHITESPACE_TABLE: i128 = {
    let mut table = [0u8; VECTOR_BYTES as usize];
    table[b'\t' as usize] = u8::MAX;
    table[b'\n' as usize] = u8::MAX;
    table[b'\r' as usize] = u8::MAX;
    i128::from_le_bytes(table)
};

/// A `v128` with `byte` in every lane.
const fn bytes(byte: u8) -> i128 {
    i128::from_le_bytes([byte; VECTOR_BYTES as usize])
}

/// Push the sixteen units from the address in `pos`, narrowed to bytes.
pub(super) fn narrowed(a: &mut Asm, pos: u32) {
    a.get(pos)
        .i(I::V128Load(TEXT))
        .get(pos)
        .i(I::V128Load(at(TEXT, VECTOR_BYTES as u64)))
        .i(I::I8x16NarrowI16x8S);
}

/// Replace the narrowed units on the stack (kept in the `v128` local
/// `lanes`) by the lanes that end or break a plain string run: a quote, a
/// backslash or a control character.
pub(super) fn string_stops(a: &mut Asm, lanes: u32) {
    a.tee(lanes)
        .i(I::V128Const(bytes(QUOTE_FLIP)))
        .i(I::V128Xor)
        .i(I::V128Const(bytes(CONTROL_END + 1)))
        .i(I::I8x16LtU)
        .get(lanes)
        .i(I::V128Const(bytes(BACKSLASH)))
        .i(I::I8x16Eq)
        .i(I::V128Or);
}

/// Replace the narrowed units on the stack (kept in the `v128` local
/// `lanes`) by the lanes that hold JSON whitespace.
pub(super) fn whitespace(a: &mut Asm, lanes: u32) {
    a.set(lanes)
        .i(I::V128Const(WHITESPACE_TABLE))
        .get(lanes)
        .i(I::I8x16Swizzle)
        .get(lanes)
        .i(I::V128Const(bytes(SPACE)))
        .i(I::I8x16Eq)
        .i(I::V128Or);
}

/// Replace the narrowed units on the stack by the lanes that hold a
/// backslash.
pub(super) fn backslashes(a: &mut Asm) {
    a.i(I::V128Const(bytes(BACKSLASH))).i(I::I8x16Eq);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classification of a unit, as the scalar scanner decides it.
    fn stops(unit: u16) -> bool {
        unit == u16::from(b'"') || unit == u16::from(b'\\') || unit < u16::from(CONTROL_END)
    }
    fn space(unit: u16) -> bool {
        matches!(unit, 0x20 | 0x09 | 0x0a | 0x0d)
    }
    /// `i8x16.narrow_i16x8_s` of one lane.
    fn narrow(unit: u16) -> u8 {
        (unit as i16).clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8 as u8
    }

    /// The narrowed byte classifies every UTF-16 unit exactly as the scalar
    /// scanner does: string stops, whitespace and backslashes.
    #[test]
    fn narrowed_bytes_classify_every_unit_exactly() {
        let table = WHITESPACE_TABLE.to_le_bytes();
        for unit in 0..=u16::MAX {
            let byte = narrow(unit);
            let stop = (byte ^ QUOTE_FLIP) < CONTROL_END + 1 || byte == BACKSLASH;
            assert_eq!(stop, stops(unit), "{unit:#x}");
            let looked_up = table.get(usize::from(byte)).copied().unwrap_or(0) != 0;
            assert_eq!(looked_up || byte == SPACE, space(unit), "{unit:#x}");
            assert_eq!(byte == BACKSLASH, unit == u16::from(b'\\'), "{unit:#x}");
        }
    }
}
