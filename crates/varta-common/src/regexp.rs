//! The v38 RegExp program format (format version 2): the contract between the
//! ahead-of-time pattern compiler (`varta-compiler` `regexp::backtrack`) and
//! the backtracking guest matcher (`varta-intrinsics` `regexp`), which the
//! v38 intrinsic library links into every module that matches.
//!
//! A program is a static string of UTF-16 code units: a fixed header, then
//! variable-length instructions whose first unit holds the opcode (low byte)
//! and the direction (bit [`BACKWARD`]), then the range tables of character
//! classes. Every operand that names a place in the program is an absolute
//! unit offset, so a program has at most [`MAX_PROGRAM_UNITS`] units.
//!
//! The v37 linear-time format (version 1) is a different, frozen format of
//! the compiler (`regexp::program`), run by the emitted Pike VM.

/// The program format version of this module.
pub const FORMAT_VERSION: u16 = 2;
/// The largest program: offsets are single code units.
pub const MAX_PROGRAM_UNITS: usize = u16::MAX as usize;

/// Header unit indices.
pub mod header {
    /// [`super::FORMAT_VERSION`].
    pub const FORMAT: usize = 0;
    /// [`super::flag`] bits.
    pub const FLAGS: usize = 1;
    /// Capture slots: two per group, group 0 included.
    pub const CAPTURES: usize = 2;
    /// Registers after the capture slots (quantifier counters and
    /// iteration starts).
    pub const REGISTERS: usize = 3;
    /// Unit offset of the first instruction.
    pub const CODE: usize = 4;
    /// Prefilter bitmap of the ASCII code units that can start a match.
    pub const FIRST_ASCII: usize = 5;
    pub const FIRST_ASCII_UNITS: usize = 8;
    /// Nonzero when a code unit above ASCII can start a match.
    pub const FIRST_OTHER: usize = FIRST_ASCII + FIRST_ASCII_UNITS;
    /// Header length in units.
    pub const UNITS: usize = FIRST_OTHER + 1;
}

/// `FLAGS` bits.
pub mod flag {
    /// The pattern reads code points (`u` or `v`): surrogate pairs are one
    /// character and the search advances by code point.
    pub const UNICODE: u16 = 1;
    /// Every match starts with `^` without `m`: only position 0 can match.
    pub const ANCHORED: u16 = 2;
    /// The `FIRST_*` units hold a prefilter: every match is non-empty and
    /// starts with a code unit in the set.
    pub const PREFILTER: u16 = 4;
}

/// The opcode is the low byte of an instruction's first unit.
pub const OP_MASK: u16 = 0xFF;
/// Consuming instructions and backreferences with this bit read the input
/// backward (inside lookbehind).
pub const BACKWARD: u16 = 0x100;

/// Opcodes and their operands (after the opcode unit).
pub mod op {
    /// The whole pattern matched.
    pub const MATCH: u16 = 0;
    /// `[unit]`: the character equals `unit` (a code unit, or a BMP code
    /// point under `UNICODE`).
    pub const CHAR: u16 = 1;
    /// `[high, low]`: the character is the code point `high << 16 | low`.
    pub const CHAR32: u16 = 2;
    /// Any character.
    pub const ANY: u16 = 3;
    /// `[table]`: the character is in the class at unit offset `table`:
    /// `[count, ranges…]`, sorted ranges of two 32-bit bounds, low half first.
    pub const CLASS: u16 = 4;
    /// `[target]`: continue at `target`.
    pub const JUMP: u16 = 5;
    /// `[first, second]`: continue at `first`; on failure at `second`.
    pub const SPLIT: u16 = 6;
    /// `[slot]`: store the position in capture slot `slot`.
    pub const SAVE: u16 = 7;
    /// `[from, to]`: clear capture slots `from..to` (the captures of a
    /// repeated group, RepeatMatcher step 4).
    pub const RESET: u16 = 8;
    /// `[kind]`: continue when [`super::assertion`] `kind` holds.
    pub const ASSERT: u16 = 9;
    /// `[ignore_case, count, groups…]`: the input continues with the text
    /// of the first of `groups` that took part (empty when none did),
    /// compared by Canonicalize when `ignore_case` is nonzero.
    pub const BACKREF: u16 = 10;
    /// `[kind, next]`: a lookaround ([`super::look`] bits) whose body
    /// follows and ends with `LOOK_END`; the pattern continues at `next`.
    pub const LOOK: u16 = 11;
    /// The body of the innermost open lookaround matched.
    pub const LOOK_END: u16 = 12;
    /// `[register]`: a quantifier starts: its iteration count is 0.
    pub const REPEAT_INIT: u16 = 13;
    /// `[register, min (2 units), max (2 units), greedy, exit]`: the loop
    /// head of a quantifier whose body (starting with `ENTER`) follows;
    /// `max` [`super::UNBOUNDED`] is unbounded.
    pub const REPEAT: u16 = 14;
    /// `[register]`: an iteration starts: record the position in
    /// `register + 1` for the empty check.
    pub const ENTER: u16 = 15;
    /// `[register, head]`: an iteration ended: fail an optional iteration
    /// that consumed nothing, count it and continue at `head`.
    pub const REPEAT_END: u16 = 16;
    /// `[min (2 units), max (2 units), greedy]` followed by one `CHAR`,
    /// `CHAR32`, `ANY` or `CLASS`: a quantifier of a single character
    /// without captures, run without per-iteration state.
    pub const STAR: u16 = 17;
}

/// Units of each instruction, the opcode unit included; `None` for an
/// unknown opcode.
#[must_use]
pub fn width(code: u16, operands: &[u16]) -> Option<usize> {
    Some(match code & OP_MASK {
        op::MATCH | op::ANY | op::LOOK_END => 1,
        op::CHAR | op::CLASS | op::JUMP | op::SAVE | op::ASSERT | op::REPEAT_INIT | op::ENTER => 2,
        op::CHAR32 | op::SPLIT | op::RESET | op::LOOK | op::REPEAT_END => 3,
        op::BACKREF => 3 + usize::from(*operands.get(1)?),
        op::REPEAT => 8,
        op::STAR => 6,
        _ => return None,
    })
}

/// An unbounded quantifier maximum.
pub const UNBOUNDED: u32 = u32::MAX;

/// `ASSERT` kinds.
pub mod assertion {
    /// `^` without `m`.
    pub const START: u16 = 0;
    /// `$` without `m`.
    pub const END: u16 = 1;
    /// `^` with `m`.
    pub const LINE_START: u16 = 2;
    /// `$` with `m`.
    pub const LINE_END: u16 = 3;
    /// `\b`.
    pub const WORD_BOUNDARY: u16 = 4;
    /// `\B`.
    pub const NOT_WORD_BOUNDARY: u16 = 5;
    /// `\b` counting U+017F and U+212A as word characters (`u`/`v` with `i`).
    pub const WORD_BOUNDARY_EXTRA: u16 = 6;
    /// `\B` counting U+017F and U+212A as word characters.
    pub const NOT_WORD_BOUNDARY_EXTRA: u16 = 7;
}

/// `LOOK` kind bits.
pub mod look {
    /// Lookbehind (the body reads backward); otherwise lookahead.
    pub const BEHIND: u16 = 1;
    /// Negative: the pattern continues when the body cannot match.
    pub const NEGATIVE: u16 = 2;
}

/// Units per class range: two 32-bit bounds, low half first.
pub const RANGE_UNITS: usize = 4;
/// A capture slot or register that holds no position.
pub const UNSET: i32 = -1;

/// The word characters of `\w` and `\b`.
#[must_use]
pub fn word_character(unit: u32, extra: bool) -> bool {
    matches!(unit, 0x30..=0x39 | 0x41..=0x5A | 0x5F | 0x61..=0x7A)
        || (extra && (unit == LONG_S || unit == KELVIN))
}
/// U+017F LATIN SMALL LETTER LONG S and U+212A KELVIN SIGN: the characters
/// outside `\w` whose simple case folding is in `\w`.
pub const LONG_S: u32 = 0x017F;
pub const KELVIN: u32 = 0x212A;
/// LineTerminator: LF, CR, LS and PS.
#[must_use]
pub fn line_terminator(unit: u32) -> bool {
    matches!(unit, 0x0A | 0x0D | 0x2028 | 0x2029)
}

/// The flag bits of a RegExp object (the `RegExp` preludes' `bits`), in the
/// `dgimsuvy` order of its getters.
pub mod bits {
    pub const GLOBAL: u32 = 1;
    pub const IGNORE_CASE: u32 = 2;
    pub const MULTILINE: u32 = 4;
    pub const DOT_ALL: u32 = 8;
    pub const UNICODE: u32 = 16;
    pub const STICKY: u32 = 32;
    /// V38: `d`.
    pub const HAS_INDICES: u32 = 64;
    /// V38: `v`.
    pub const UNICODE_SETS: u32 = 128;
}

/// The record the linked `regexp_compile` returns to the v38 RegExp prelude
/// (`prelude/regexp_v38.ts` mirrors these names): 32-bit little-endian
/// words read like capture slots, then UTF-16 texts.
pub mod construction {
    /// Word: [`OK`] or [`SYNTAX_ERROR`].
    pub const STATUS: usize = 0;
    pub const OK: i32 = 0;
    pub const SYNTAX_ERROR: i32 = 1;
    /// After [`SYNTAX_ERROR`]: the SyntaxError message, from this unit on.
    pub const MESSAGE_UNIT: usize = 2;
    /// Word: the [`super::bits`] of the flags.
    pub const BITS: usize = 1;
    /// Word: the captures, the whole match included.
    pub const COUNT: usize = 2;
    /// Words: the unit lengths of the texts, in their order.
    pub const PROGRAM_UNITS: usize = 3;
    pub const LEAN_UNITS: usize = 4;
    pub const NAMES_UNITS: usize = 5;
    pub const SOURCE_UNITS: usize = 6;
    pub const FLAGS_UNITS: usize = 7;
    /// The first unit of the texts: the program, the lean program, the
    /// comma-separated names of groups 1.. (empty without named groups),
    /// `source` and the canonical `flags`.
    pub const TEXT_UNIT: usize = 16;
}

/// The first code point outside ASCII.
const ASCII: u32 = 0x80;

/// Canonicalize(rer, ch) (§22.2.2.7.3): `toUppercase` per code unit without
/// `u`/`v`, simple case folding with them (the lowercase of the
/// single-character uppercase mapping, the Turkic dotted and dotless I
/// folding to themselves), from the Unicode tables of `core`. The compiler
/// widens characters and classes with it (`varta-compiler` `regexp::case`)
/// and the linked matcher compares case-insensitive backreferences with it.
#[must_use]
pub fn canonicalize(ch: u32, unicode: bool) -> u32 {
    /// U+0130 and U+0131 have only Turkic (`T`) or full (`F`) foldings.
    const TURKIC: [u32; 2] = [0x0130, 0x0131];
    const MAX_UNIT: u32 = 0xFFFF;
    fn single(mut mapped: impl Iterator<Item = char>) -> Option<char> {
        let first = mapped.next()?;
        mapped.next().is_none().then_some(first)
    }
    let Some(character) = char::from_u32(ch) else {
        return ch;
    };
    if unicode {
        if TURKIC.contains(&ch) {
            return ch;
        }
        let upper = single(character.to_uppercase()).unwrap_or(character);
        return single(upper.to_lowercase())
            .or_else(|| single(character.to_lowercase()))
            .map_or(ch, u32::from);
    }
    let Some(upper) = single(character.to_uppercase()).map(u32::from) else {
        return ch;
    };
    if upper > MAX_UNIT || ch >= ASCII && upper < ASCII {
        return ch;
    }
    upper
}
