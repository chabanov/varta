//! The v38 RegExp matcher: a backtracking interpreter of the program format
//! `varta_common::regexp` (version 2) over an immutable UTF-16 input.
//!
//! It follows the ECMAScript 2025 pattern semantics (§22.2.2) as a
//! continuation machine: alternatives and quantifiers push choice points,
//! every capture or register write is undone when matching backtracks past
//! it, lookarounds open a marker that their end resolves (a positive one
//! keeps its captures and drops its choice points, a negative one undoes
//! everything), and RepeatMatcher's empty check fails an optional iteration
//! that consumed nothing. Matching may take time exponential in the input:
//! linked into a guest module, every step is ordinary guest code metered as
//! fuel, so a catastrophic pattern ends with the turn's fuel exhaustion, and
//! the backtracking stack is arena memory under the memory budget.
//!
//! The program is data from the module: a malformed program is a fatal
//! [`IntrinsicError::RegExpProgram`], never undefined behaviour.
use crate::IntrinsicError;
use alloc::vec::Vec;
pub use varta_common::regexp::canonicalize;
use varta_common::regexp::{
    BACKWARD, FORMAT_VERSION, OP_MASK, RANGE_UNITS, UNBOUNDED, UNSET, assertion, flag, header,
    line_terminator, look, op, word_character,
};

const UNIT_BYTES: usize = 2;
type Unit = [u8; UNIT_BYTES];
/// Classes with more ranges than this are binary searched.
const LINEAR_RANGES: usize = 8;
const ASCII: u16 = 0x80;
/// A unit's word and bit in the 64-bit prefilter words.
const WORD_SHIFT: u16 = 6;
const WORD_MASK: u16 = 63;
const HIGH_FIRST: u16 = 0xD800;
const HIGH_LAST: u16 = 0xDBFF;
const LOW_FIRST: u16 = 0xDC00;
const LOW_LAST: u16 = 0xDFFF;
const SUPPLEMENTARY: u32 = 0x1_0000;
const SURROGATE_BITS: u32 = 10;

fn high(unit: u16) -> bool {
    (HIGH_FIRST..=HIGH_LAST).contains(&unit)
}
fn low(unit: u16) -> bool {
    (LOW_FIRST..=LOW_LAST).contains(&unit)
}
fn combine(high: u16, low: u16) -> u32 {
    SUPPLEMENTARY + ((u32::from(high - HIGH_FIRST)) << SURROGATE_BITS) + u32::from(low - LOW_FIRST)
}

/// Run `program` over `input` (both UTF-16LE bytes) from unit `start`, as
/// RegExpBuiltinExec's matcher loop: only at `start` when `sticky`,
/// otherwise at every later position until a match. The capture slots of
/// the match (two per group, group 0 first, [`UNSET`] for a group that did
/// not take part), or `None`.
pub fn exec(
    program: &[u8],
    input: &[u8],
    start: f64,
    sticky: bool,
) -> Result<Option<Vec<i32>>, IntrinsicError> {
    let code = program.as_chunks::<UNIT_BYTES>().0;
    let input = input.as_chunks::<UNIT_BYTES>().0;
    let unit = |index: usize| {
        code.get(index)
            .map(|pair| u16::from_le_bytes(*pair))
            .ok_or(IntrinsicError::RegExpProgram)
    };
    if unit(header::FORMAT)? != FORMAT_VERSION {
        return Err(IntrinsicError::RegExpProgram);
    }
    let flags = unit(header::FLAGS)?;
    let captures = usize::from(unit(header::CAPTURES)?);
    let registers = usize::from(unit(header::REGISTERS)?);
    let entry = usize::from(unit(header::CODE)?);
    if captures < 2 {
        return Err(IntrinsicError::RegExpProgram);
    }
    // The ASCII prefilter as two 64-bit words: bit `u` for code unit `u`.
    let mut first = [0u64; 2];
    for index in 0..header::FIRST_ASCII_UNITS {
        let word = index * u16::BITS as usize / u64::BITS as usize;
        let shift = (index * u16::BITS as usize) as u32 % u64::BITS;
        first[word] |= u64::from(unit(header::FIRST_ASCII + index)?) << shift;
    }
    let other = unit(header::FIRST_OTHER)? != 0;
    let can_start = |code_unit: u16| {
        if code_unit < ASCII {
            first[usize::from(code_unit >> WORD_SHIFT)] >> (code_unit & WORD_MASK) & 1 != 0
        } else {
            other
        }
    };
    let mut matcher = Matcher {
        code,
        input,
        unicode: flags & flag::UNICODE != 0,
        slots: alloc::vec![UNSET; captures + registers],
        captures,
        stack: Vec::new(),
        points: 0,
    };
    let len = input.len();
    let mut at = if start.is_nan() || start <= 0.0 {
        0
    } else if start > len as f64 {
        return Ok(None);
    } else {
        start as usize
    };
    // A start inside a surrogate pair backs up to the pair under `u`/`v`.
    if matcher.unicode && at > 0 && at < len && low(matcher.unit(at)) && high(matcher.unit(at - 1))
    {
        at -= 1;
    }
    let anchored = flags & flag::ANCHORED != 0;
    let prefilter = flags & flag::PREFILTER != 0;
    loop {
        if anchored && at > 0 {
            return Ok(None);
        }
        if prefilter {
            // Every match is non-empty and starts with a unit of the set:
            // skip the positions that cannot start one without a run.
            if sticky {
                if at >= len || !can_start(matcher.unit(at)) {
                    return Ok(None);
                }
            } else {
                while let Some(pair) = input.get(at)
                    && !can_start(u16::from_le_bytes(*pair))
                {
                    at += 1;
                }
                if at >= len {
                    return Ok(None);
                }
                // Under `u`/`v` no match starts inside a surrogate pair.
                if matcher.unicode && at > 0 && low(matcher.unit(at)) && high(matcher.unit(at - 1))
                {
                    at += 1;
                    continue;
                }
            }
        }
        if let Some(found) = matcher.attempt(entry, at)? {
            return Ok(Some(found));
        }
        if sticky || at >= len {
            return Ok(None);
        }
        at = matcher.after(at);
    }
}

/// One backtracking point or undo record.
#[derive(Clone, Copy)]
enum Entry {
    /// Resume at `pc` with the position `pos`.
    Choice { pc: u32, pos: u32 },
    /// Restore slot `slot` to `value`.
    Undo { slot: u32, value: i32 },
    /// An open lookaround of `kind` that started at `pos` and continues at
    /// `next`.
    Look { kind: u16, pos: u32, next: u32 },
    /// A greedy `STAR` at `pc` that can give back characters down to
    /// `floor`, currently at `pos`.
    Greedy { pc: u32, floor: u32, pos: u32 },
    /// A lazy `STAR` at `pc` that has taken `count` characters, at `pos`.
    Lazy { pc: u32, pos: u32, count: u32 },
}

struct Matcher<'a> {
    code: &'a [Unit],
    input: &'a [Unit],
    unicode: bool,
    /// Capture slots, then registers.
    slots: Vec<i32>,
    captures: usize,
    stack: Vec<Entry>,
    /// Entries on the stack that matching can return to (everything but
    /// `Undo`): without one, a write needs no undo record.
    points: usize,
}

impl Matcher<'_> {
    fn unit(&self, index: usize) -> u16 {
        u16::from_le_bytes(self.input[index])
    }
    fn op(&self, pc: usize) -> Result<u16, IntrinsicError> {
        self.code
            .get(pc)
            .map(|pair| u16::from_le_bytes(*pair))
            .ok_or(IntrinsicError::RegExpProgram)
    }
    fn wide(&self, pc: usize) -> Result<u32, IntrinsicError> {
        Ok(u32::from(self.op(pc)?) | u32::from(self.op(pc + 1)?) << u16::BITS)
    }
    /// The next search position: one character on.
    fn after(&self, at: usize) -> usize {
        match self.next(at) {
            Some((_, next)) => next,
            None => at + 1,
        }
    }
    /// The character at `pos` reading forward, and the position after it.
    fn next(&self, pos: usize) -> Option<(u32, usize)> {
        let unit = u16::from_le_bytes(*self.input.get(pos)?);
        if self.unicode
            && high(unit)
            && let Some(pair) = self.input.get(pos + 1)
        {
            let second = u16::from_le_bytes(*pair);
            if low(second) {
                return Some((combine(unit, second), pos + 2));
            }
        }
        Some((u32::from(unit), pos + 1))
    }
    /// The character before `pos`, and the position before it.
    fn previous(&self, pos: usize) -> Option<(u32, usize)> {
        let before = pos.checked_sub(1)?;
        let unit = self.unit(before);
        if self.unicode && low(unit) && before > 0 {
            let first = self.unit(before - 1);
            if high(first) {
                return Some((combine(first, unit), before - 1));
            }
        }
        Some((u32::from(unit), before))
    }
    fn read(&self, pos: usize, backward: bool) -> Option<(u32, usize)> {
        if backward {
            self.previous(pos)
        } else {
            self.next(pos)
        }
    }
    fn set(&mut self, slot: usize, value: i32) -> Result<(), IntrinsicError> {
        let old = *self.slots.get(slot).ok_or(IntrinsicError::RegExpProgram)?;
        if old != value {
            if self.points > 0 {
                self.stack.push(Entry::Undo {
                    slot: slot as u32,
                    value: old,
                });
            }
            self.slots[slot] = value;
        }
        Ok(())
    }
    fn push(&mut self, entry: Entry) {
        self.points += 1;
        self.stack.push(entry);
    }
    fn class(&self, table: usize, ch: u32) -> Result<bool, IntrinsicError> {
        let count = usize::from(self.op(table)?);
        let range = |index: usize| -> Result<(u32, u32), IntrinsicError> {
            let at = table + 1 + index * RANGE_UNITS;
            Ok((self.wide(at)?, self.wide(at + 2)?))
        };
        if count <= LINEAR_RANGES {
            for index in 0..count {
                let (low, high) = range(index)?;
                if ch < low {
                    return Ok(false);
                }
                if ch <= high {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let (mut lo, mut hi) = (0, count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (low, high) = range(mid)?;
            if ch < low {
                hi = mid;
            } else if ch > high {
                lo = mid + 1;
            } else {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// One single-character instruction at `pc` against the input at
    /// `pos`: the position after the character when it matches.
    fn atom(&self, pc: usize, pos: usize) -> Result<Option<usize>, IntrinsicError> {
        let code = self.op(pc)?;
        let Some((ch, next)) = self.read(pos, code & BACKWARD != 0) else {
            return Ok(None);
        };
        let matched = match code & OP_MASK {
            op::CHAR => ch == u32::from(self.op(pc + 1)?),
            op::CHAR32 => ch == self.wide_char(pc + 1)?,
            op::ANY => true,
            op::CLASS => self.class(usize::from(self.op(pc + 1)?), ch)?,
            _ => return Err(IntrinsicError::RegExpProgram),
        };
        Ok(matched.then_some(next))
    }
    /// A `CHAR32` operand: the high half first.
    fn wide_char(&self, at: usize) -> Result<u32, IntrinsicError> {
        Ok(u32::from(self.op(at)?) << u16::BITS | u32::from(self.op(at + 1)?))
    }
    fn atom_width(&self, pc: usize) -> Result<usize, IntrinsicError> {
        Ok(match self.op(pc)? & OP_MASK {
            op::ANY => 1,
            op::CHAR | op::CLASS => 2,
            op::CHAR32 => 3,
            _ => return Err(IntrinsicError::RegExpProgram),
        })
    }
    fn assertion(&self, kind: u16, pos: usize) -> Result<bool, IntrinsicError> {
        let len = self.input.len();
        let before = |test: fn(u32) -> bool| pos > 0 && test(u32::from(self.unit(pos - 1)));
        let at = |test: fn(u32) -> bool| pos < len && test(u32::from(self.unit(pos)));
        let boundary = |extra: bool| {
            let word = |unit: u16| word_character(u32::from(unit), extra);
            let a = pos > 0 && word(self.unit(pos - 1));
            let b = pos < len && word(self.unit(pos));
            a != b
        };
        Ok(match kind {
            assertion::START => pos == 0,
            assertion::END => pos == len,
            assertion::LINE_START => pos == 0 || before(line_terminator),
            assertion::LINE_END => pos == len || at(line_terminator),
            assertion::WORD_BOUNDARY => boundary(false),
            assertion::NOT_WORD_BOUNDARY => !boundary(false),
            assertion::WORD_BOUNDARY_EXTRA => boundary(true),
            assertion::NOT_WORD_BOUNDARY_EXTRA => !boundary(true),
            _ => return Err(IntrinsicError::RegExpProgram),
        })
    }
    /// BackreferenceMatcher: the position after the referenced text, read
    /// in the instruction's direction.
    fn backreference(&self, pc: usize, pos: usize) -> Result<Option<usize>, IntrinsicError> {
        let code = self.op(pc)?;
        let backward = code & BACKWARD != 0;
        let ignore_case = self.op(pc + 1)? != 0;
        let count = usize::from(self.op(pc + 2)?);
        let mut range = None;
        for index in 0..count {
            let group = usize::from(self.op(pc + 3 + index)?);
            let start = *self
                .slots
                .get(2 * group)
                .ok_or(IntrinsicError::RegExpProgram)?;
            let end = *self
                .slots
                .get(2 * group + 1)
                .ok_or(IntrinsicError::RegExpProgram)?;
            if start != UNSET && end != UNSET {
                range = Some((start as usize, end as usize));
                break;
            }
        }
        let Some((start, end)) = range else {
            return Ok(Some(pos));
        };
        let length = end.saturating_sub(start);
        if !ignore_case {
            let from = if backward {
                let Some(from) = pos.checked_sub(length) else {
                    return Ok(None);
                };
                from
            } else {
                pos
            };
            if from + length > self.input.len()
                || self.input[start..end] != self.input[from..from + length]
            {
                return Ok(None);
            }
            return Ok(Some(if backward { from } else { from + length }));
        }
        // Characters compare by Canonicalize, in the instruction's direction.
        let mut at = pos;
        let mut reference = if backward { end } else { start };
        while if backward {
            reference > start
        } else {
            reference < end
        } {
            let Some((expected, next_reference)) = self.read(reference, backward) else {
                return Ok(None);
            };
            let Some((actual, next)) = self.read(at, backward) else {
                return Ok(None);
            };
            if canonicalize(expected, self.unicode) != canonicalize(actual, self.unicode) {
                return Ok(None);
            }
            reference = next_reference;
            at = next;
        }
        Ok(Some(at))
    }

    /// Match from `start`: the capture slots, or `None`.
    fn attempt(&mut self, entry: usize, start: usize) -> Result<Option<Vec<i32>>, IntrinsicError> {
        self.slots.fill(UNSET);
        self.stack.clear();
        self.points = 0;
        let mut pc = entry;
        let mut pos = start;
        loop {
            let code = self.op(pc)?;
            let advanced = match code & OP_MASK {
                op::MATCH => {
                    self.slots[0] = start as i32;
                    self.slots[1] = pos as i32;
                    return Ok(Some(self.slots[..self.captures].to_vec()));
                }
                op::CHAR | op::CHAR32 | op::ANY | op::CLASS => {
                    let width = self.atom_width(pc)?;
                    self.atom(pc, pos)?.map(|next| (pc + width, next))
                }
                op::JUMP => Some((usize::from(self.op(pc + 1)?), pos)),
                op::SPLIT => {
                    let second = self.op(pc + 2)?;
                    self.push(Entry::Choice {
                        pc: u32::from(second),
                        pos: pos as u32,
                    });
                    Some((usize::from(self.op(pc + 1)?), pos))
                }
                op::SAVE => {
                    self.set(usize::from(self.op(pc + 1)?), pos as i32)?;
                    Some((pc + 2, pos))
                }
                op::RESET => {
                    let (from, to) = (self.op(pc + 1)?, self.op(pc + 2)?);
                    for slot in from..to {
                        self.set(usize::from(slot), UNSET)?;
                    }
                    Some((pc + 3, pos))
                }
                op::ASSERT => self
                    .assertion(self.op(pc + 1)?, pos)?
                    .then_some((pc + 2, pos)),
                op::BACKREF => {
                    let count = usize::from(self.op(pc + 2)?);
                    self.backreference(pc, pos)?
                        .map(|next| (pc + 3 + count, next))
                }
                op::LOOK => {
                    let kind = self.op(pc + 1)?;
                    let next = self.op(pc + 2)?;
                    self.push(Entry::Look {
                        kind,
                        pos: pos as u32,
                        next: u32::from(next),
                    });
                    Some((pc + 3, pos))
                }
                op::LOOK_END => self.look_end()?,
                op::REPEAT_INIT => {
                    self.set(self.register(pc)?, 0)?;
                    Some((pc + 2, pos))
                }
                op::REPEAT => Some(self.repeat(pc, pos)?),
                op::ENTER => {
                    self.set(self.register(pc)? + 1, pos as i32)?;
                    Some((pc + 2, pos))
                }
                op::REPEAT_END => {
                    let register = self.register(pc)?;
                    let head = usize::from(self.op(pc + 2)?);
                    let count = self.slots[register] as u32;
                    let min = self.wide(head + 2)?;
                    if count >= min && self.slots[register + 1] == pos as i32 {
                        None
                    } else {
                        self.set(register, count.saturating_add(1) as i32)?;
                        Some((head, pos))
                    }
                }
                op::STAR => self.star(pc, pos)?,
                _ => return Err(IntrinsicError::RegExpProgram),
            };
            match advanced {
                Some((next_pc, next_pos)) => {
                    pc = next_pc;
                    pos = next_pos;
                }
                None => match self.backtrack()? {
                    Some((next_pc, next_pos)) => {
                        pc = next_pc;
                        pos = next_pos;
                    }
                    None => return Ok(None),
                },
            }
        }
    }
    fn register(&self, pc: usize) -> Result<usize, IntrinsicError> {
        let register = self.captures + usize::from(self.op(pc + 1)?);
        if register + 1 >= self.slots.len() {
            return Err(IntrinsicError::RegExpProgram);
        }
        Ok(register)
    }
    /// The loop head of a quantifier (RepeatMatcher): another iteration,
    /// the exit, or both in priority order.
    fn repeat(&mut self, pc: usize, pos: usize) -> Result<(usize, usize), IntrinsicError> {
        let register = self.register(pc)?;
        let count = self.slots[register] as u32;
        let min = self.wide(pc + 2)?;
        let max = self.wide(pc + 4)?;
        let greedy = self.op(pc + 6)? != 0;
        let exit = usize::from(self.op(pc + 7)?);
        let body = pc + 8;
        Ok(if max != UNBOUNDED && count >= max {
            (exit, pos)
        } else if count < min {
            (body, pos)
        } else if greedy {
            self.push(Entry::Choice {
                pc: exit as u32,
                pos: pos as u32,
            });
            (body, pos)
        } else {
            self.push(Entry::Choice {
                pc: body as u32,
                pos: pos as u32,
            });
            (exit, pos)
        })
    }
    /// The operands of the `STAR` at `pc`: its minimum, maximum, greed, its
    /// atom and the continuation after it.
    fn star_operands(&self, pc: usize) -> Result<(u32, u32, bool, usize, usize), IntrinsicError> {
        let min = self.wide(pc + 1)?;
        let max = self.wide(pc + 3)?;
        let greedy = self.op(pc + 5)? != 0;
        let atom = pc + 6;
        Ok((min, max, greedy, atom, atom + self.atom_width(atom)?))
    }
    fn star(&mut self, pc: usize, pos: usize) -> Result<Option<(usize, usize)>, IntrinsicError> {
        let (min, max, greedy, atom, next) = self.star_operands(pc)?;
        let mut at = pos;
        let mut count = 0;
        while count < min {
            match self.atom(atom, at)? {
                Some(after) => at = after,
                None => return Ok(None),
            }
            count += 1;
        }
        if greedy {
            let floor = at;
            while max == UNBOUNDED || count < max {
                match self.atom(atom, at)? {
                    Some(after) => at = after,
                    None => break,
                }
                count += 1;
            }
            if at != floor {
                self.push(Entry::Greedy {
                    pc: pc as u32,
                    floor: floor as u32,
                    pos: at as u32,
                });
            }
        } else if max == UNBOUNDED || count < max {
            self.push(Entry::Lazy {
                pc: pc as u32,
                pos: at as u32,
                count,
            });
        }
        Ok(Some((next, at)))
    }
    /// The body of the innermost open lookaround matched.
    fn look_end(&mut self) -> Result<Option<(usize, usize)>, IntrinsicError> {
        let Some(marker) = self
            .stack
            .iter()
            .rposition(|entry| matches!(entry, Entry::Look { .. }))
        else {
            return Err(IntrinsicError::RegExpProgram);
        };
        let Entry::Look { kind, pos, next } = self.stack[marker] else {
            return Err(IntrinsicError::RegExpProgram);
        };
        if kind & look::NEGATIVE != 0 {
            // The assertion fails: undo the body and backtrack before it.
            while self.stack.len() > marker {
                if let Some(Entry::Undo { slot, value }) = self.stack.pop() {
                    self.slots[slot as usize] = value;
                } else {
                    self.points -= 1;
                }
            }
            return Ok(None);
        }
        // A positive lookaround is atomic: its choice points go, the undo
        // records of its captures stay for backtracking before it.
        let mut kept = marker;
        for index in marker..self.stack.len() {
            match self.stack[index] {
                Entry::Undo { .. } => {
                    self.stack[kept] = self.stack[index];
                    kept += 1;
                }
                _ => self.points -= 1,
            }
        }
        self.stack.truncate(kept);
        Ok(Some((next as usize, pos as usize)))
    }
    /// Return to the latest choice point, undoing writes on the way.
    fn backtrack(&mut self) -> Result<Option<(usize, usize)>, IntrinsicError> {
        while let Some(entry) = self.stack.pop() {
            match entry {
                Entry::Undo { slot, value } => {
                    self.slots[slot as usize] = value;
                    continue;
                }
                Entry::Choice { pc, pos } => {
                    self.points -= 1;
                    return Ok(Some((pc as usize, pos as usize)));
                }
                Entry::Look { kind, pos, next } => {
                    self.points -= 1;
                    if kind & look::NEGATIVE != 0 {
                        // The body cannot match: the assertion holds.
                        return Ok(Some((next as usize, pos as usize)));
                    }
                }
                Entry::Greedy { pc, floor, pos } => {
                    self.points -= 1;
                    let (_, _, _, atom, next) = self.star_operands(pc as usize)?;
                    let backward = self.op(atom)? & BACKWARD != 0;
                    // Give back one character, toward the floor.
                    let Some((_, back)) = self.read(pos as usize, !backward) else {
                        return Err(IntrinsicError::RegExpProgram);
                    };
                    if back != floor as usize {
                        self.push(Entry::Greedy {
                            pc,
                            floor,
                            pos: back as u32,
                        });
                    }
                    return Ok(Some((next, back)));
                }
                Entry::Lazy { pc, pos, count } => {
                    self.points -= 1;
                    let (_, max, _, atom, next) = self.star_operands(pc as usize)?;
                    if let Some(after) = self.atom(atom, pos as usize)? {
                        let count = count + 1;
                        if max == UNBOUNDED || count < max {
                            self.push(Entry::Lazy {
                                pc,
                                pos: after as u32,
                                count,
                            });
                        }
                        return Ok(Some((next, after)));
                    }
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
