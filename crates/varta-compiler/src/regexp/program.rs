//! The matcher program format shared by the pattern compiler and the guest
//! matcher (`emit::strings::regexp`). A program is a static string of UTF-16
//! code units in the data section: a fixed header, three-unit instructions
//! `[op, a, b]` and the range tables of character classes.

/// Header unit indices.
pub(crate) const FORMAT: usize = 0;
pub(crate) const FLAGS: usize = 1;
/// Capture slots returned to the guest: two per group, group 0 included.
pub(crate) const CAPTURES: usize = 2;
/// Thread slots: the capture slots followed by empty-check registers.
pub(crate) const SLOTS: usize = 3;
pub(crate) const INSTRUCTIONS: usize = 4;
/// Most threads one list holds: consuming instructions plus `MATCH`.
pub(crate) const THREADS: usize = 5;
/// Most entries of the follow stack during one thread addition.
pub(crate) const STACK: usize = 6;
/// Prefilter bitmap of the ASCII code units that can start a match.
pub(crate) const FIRST_ASCII: usize = 7;
pub(crate) const FIRST_ASCII_UNITS: usize = 8;
/// Nonzero when a code unit above ASCII can start a match.
pub(crate) const FIRST_OTHER: usize = FIRST_ASCII + FIRST_ASCII_UNITS;
/// Unit offset of the closure index: one unit per instruction, the unit
/// offset of that instruction's precomputed closure or 0 when the matcher
/// follows it at run time (an assertion or empty check on the way).
pub(crate) const CLOSURE_INDEX: usize = FIRST_OTHER + 1;
/// When at most two code units (all ASCII) can start a match: their count
/// and the units, scanned for directly.
pub(crate) const FIRST_COUNT: usize = CLOSURE_INDEX + 1;
pub(crate) const FIRST_UNIT: usize = FIRST_COUNT + 1;
pub(crate) const FIRST_UNITS: usize = 2;
const HEADER_NESTING: usize = FIRST_UNIT + FIRST_UNITS;
pub(crate) const HEADER_UNITS: usize = HEADER_NESTING + 1;
/// The opcode is the low byte of an instruction's first unit; the high byte
/// is its nesting depth (the empty-checked iterations enclosing it).
pub(crate) const OP_MASK: u16 = 0xFF;
pub(crate) const DEPTH_SHIFT: u16 = 8;
/// Most nested empty-checked iterations.
pub(crate) const MAX_NESTING: u16 = 4;

/// The program format version.
pub(crate) const FORMAT_VERSION: u16 = 1;

/// `FLAGS` bits.
pub(crate) const UNICODE: u16 = 1;
/// Every match starts with `^` without `m`: only position 0 can match.
pub(crate) const ANCHORED: u16 = 2;
/// The `FIRST_*` units hold a prefilter: every match is non-empty.
pub(crate) const PREFILTER: u16 = 4;
/// `\b` and `\B` count U+017F and U+212A as word characters (`u` and `i`).
pub(crate) const EXTRA_WORD: u16 = 8;
/// The deepest nesting of empty-checked iterations: the matcher keys its
/// visited set by instruction and by which enclosing iterations started at
/// the current position (`2^NESTING` states per instruction).
pub(crate) const NESTING: usize = HEADER_NESTING;

/// A closure is `[count, entries…]`; an entry is `[target, writes,
/// (a, b)…]`: the thread continues at consuming instruction (or `MATCH`)
/// `target` after its slot writes in order, `b == SET` storing the position
/// in slot `a` and otherwise clearing slots `a..b`.
pub(crate) const SET: u16 = u16::MAX;
/// Units of one closure write.
pub(crate) const WRITE_UNITS: usize = 2;

/// Units per instruction.
pub(crate) const INSTRUCTION_UNITS: usize = 3;
/// Units per class range: two 32-bit bounds, low half first.
pub(crate) const RANGE_UNITS: usize = 4;

/// Opcodes; `CHAR`, `ANY` and `CLASS` consume one character.
pub(crate) mod op {
    /// The thread matched.
    pub(crate) const MATCH: u16 = 0;
    /// `a` equals the character.
    pub(crate) const CHAR: u16 = 1;
    /// Any character (`.` with `s`).
    pub(crate) const ANY: u16 = 2;
    /// The character is in the class at unit offset `a`: `[count, ranges…]`.
    pub(crate) const CLASS: u16 = 3;
    /// Continue at instruction `a`.
    pub(crate) const JUMP: u16 = 4;
    /// Continue at `a`, then (lower priority) at `b`.
    pub(crate) const SPLIT: u16 = 5;
    /// Store the position in slot `a`.
    pub(crate) const SAVE: u16 = 6;
    /// Clear slots `a..b` (captures of a repeated group).
    pub(crate) const RESET: u16 = 7;
    /// Fail when slot `a` equals the position (an empty iteration).
    pub(crate) const CHECK: u16 = 8;
    /// Continue when assertion `a` holds.
    pub(crate) const ASSERT: u16 = 9;
}

/// `ASSERT` kinds.
pub(crate) mod assertion {
    pub(crate) const START: u16 = 0;
    pub(crate) const END: u16 = 1;
    pub(crate) const LINE_START: u16 = 2;
    pub(crate) const LINE_END: u16 = 3;
    pub(crate) const WORD_BOUNDARY: u16 = 4;
    pub(crate) const NOT_WORD_BOUNDARY: u16 = 5;
}

/// Bytes of one 32-bit slot, stack word or list word in matcher scratch memory.
pub(crate) const WORD_BYTES: u32 = 4;
/// Words per follow-stack entry: a tag (instruction, or `-(slot + 1)` for a
/// slot restore) and the restored value.
pub(crate) const STACK_WORDS: u32 = 2;

/// Scratch bytes the matcher allocates for one run: returned captures,
/// working slots, the visited stamps (`2^nesting` per instruction), two
/// thread lists and the stack.
pub(crate) fn scratch_bytes(
    captures: u32,
    slots: u32,
    instructions: u32,
    nesting: u32,
    threads: u32,
    stack: u32,
) -> u64 {
    let thread = u64::from(1 + slots);
    u64::from(WORD_BYTES)
        * (u64::from(captures)
            + u64::from(slots)
            + (u64::from(instructions) << nesting)
            + 2 * u64::from(threads) * thread
            + u64::from(STACK_WORDS) * u64::from(stack))
}
