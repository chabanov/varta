//! Pattern trees to matcher programs (see `program`). The program is a
//! Thompson NFA run by a Pike VM: every thread advances in lockstep over the
//! input and a thread reaching an instruction another thread already reached
//! at the same position is dropped. That bounds a run by the program size
//! times the input length, so no pattern backtracks catastrophically.
//!
//! Leftmost-first priority reproduces the backtracking order of ECMAScript:
//! `SPLIT` prefers its first target, greedy quantifiers prefer another
//! iteration and lazy ones the exit. Each iteration of a quantified group
//! first clears its captures (`RESET`, RepeatMatcher step 4). An iteration of
//! an unbounded quantifier that consumes nothing returns to its loop
//! instruction at the same position and is dropped as already visited; an
//! optional iteration (`?`, `{n,m}` beyond `n`) of a body that can match
//! empty records its start in a register and fails an empty iteration with
//! `CHECK`, as RepeatMatcher's empty check does.
use super::RegExpError;
use super::case;
use super::parse::{Assertion, Flags, Node};
use super::program::{self, assertion, op};
use super::set::{self, Set};

/// Largest program, in code units of the data section.
pub(crate) const MAX_PROGRAM_UNITS: usize = 16 * 1024;
/// Largest matcher scratch allocation per run.
pub(crate) const MAX_SCRATCH_BYTES: u64 = 256 * 1024;
const ASCII: u32 = 0x80;
const UNIT_BITS: u32 = u16::BITS;

struct Builder {
    code: Vec<[u16; 3]>,
    classes: Vec<Set>,
    flags: Flags,
    captures: u16,
    depth: u16,
    registers: u16,
    /// Whether groups capture; the lean program records only slots 0 and 1.
    captures_on: bool,
}

fn too_large() -> RegExpError {
    RegExpError::Unsupported(format!(
        "RegExp program larger than {MAX_PROGRAM_UNITS} code units (counted repetitions are expanded)"
    ))
}
fn index(value: usize) -> Result<u16, RegExpError> {
    u16::try_from(value).map_err(|_| too_large())
}

impl Builder {
    fn here(&self) -> Result<u16, RegExpError> {
        index(self.code.len())
    }
    fn emit(&mut self, op: u16, a: u16, b: u16) -> Result<usize, RegExpError> {
        if (self.code.len() + 1) * program::INSTRUCTION_UNITS >= MAX_PROGRAM_UNITS {
            return Err(too_large());
        }
        self.code
            .push([op | self.depth << program::DEPTH_SHIFT, a, b]);
        Ok(self.code.len() - 1)
    }
    /// One iteration inside its empty check: one level deeper.
    fn checked_iteration(
        &mut self,
        node: &Node,
        reset: Option<(u16, u16)>,
        register: Option<u16>,
    ) -> Result<(), RegExpError> {
        if register.is_none() {
            return self.iteration(node, reset, None);
        }
        self.depth += 1;
        if self.depth > program::MAX_NESTING {
            return Err(RegExpError::Unsupported(format!(
                "RegExp quantified groups that can match empty nested deeper than {}",
                program::MAX_NESTING
            )));
        }
        self.registers = self.registers.max(self.depth);
        self.iteration(node, reset, register)?;
        self.depth -= 1;
        Ok(())
    }
    fn max(&self) -> u32 {
        if self.flags.unicode {
            set::MAX_CODE_POINT
        } else {
            set::MAX_UNIT
        }
    }
    fn set(&mut self, set: Set) -> Result<(), RegExpError> {
        match set.only() {
            Some(ch) if ch <= set::MAX_UNIT => {
                self.emit(op::CHAR, ch as u16, 0)?;
            }
            _ => {
                let class = match self.classes.iter().position(|known| *known == set) {
                    Some(class) => class,
                    None => {
                        self.classes.push(set);
                        self.classes.len() - 1
                    }
                };
                self.emit(op::CLASS, index(class)?, 0)?;
            }
        }
        Ok(())
    }
    fn class(&mut self, set: &Set, negated: bool) -> Result<(), RegExpError> {
        let mut set = if self.flags.ignore_case {
            case::close(set, self.flags.unicode)
        } else {
            set.clone()
        };
        if negated {
            set = set.complement(self.max());
        }
        self.set(set)
    }
    fn node(&mut self, node: &Node) -> Result<(), RegExpError> {
        match node {
            Node::Empty => {}
            Node::Char(ch) => self.class(&Set::single(*ch), false)?,
            Node::Class { set, negated } => self.class(set, *negated)?,
            Node::Dot if self.flags.dot_all => {
                self.emit(op::ANY, 0, 0)?;
            }
            Node::Dot => self.set(set::line_terminators().complement(self.max()))?,
            Node::Assert(kind) => {
                let kind = match (kind, self.flags.multiline) {
                    (Assertion::Start, false) => assertion::START,
                    (Assertion::Start, true) => assertion::LINE_START,
                    (Assertion::End, false) => assertion::END,
                    (Assertion::End, true) => assertion::LINE_END,
                    (Assertion::WordBoundary, _) => assertion::WORD_BOUNDARY,
                    (Assertion::NotWordBoundary, _) => assertion::NOT_WORD_BOUNDARY,
                };
                self.emit(op::ASSERT, kind, 0)?;
            }
            Node::Group(node, Some(_)) if !self.captures_on => self.node(node)?,
            Node::Group(node, Some(group)) => {
                let slot = index(2 * group)?;
                self.emit(op::SAVE, slot, 0)?;
                self.node(node)?;
                self.emit(op::SAVE, slot + 1, 0)?;
            }
            Node::Group(node, None) => self.node(node)?,
            Node::Concat(nodes) => {
                for node in nodes {
                    self.node(node)?;
                }
            }
            Node::Alternation(alternatives) => self.alternation(alternatives)?,
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => self.repeat(node, *min, *max, *greedy)?,
        }
        Ok(())
    }
    fn alternation(&mut self, alternatives: &[Node]) -> Result<(), RegExpError> {
        let mut jumps = Vec::new();
        let last = alternatives.len().saturating_sub(1);
        for (position, alternative) in alternatives.iter().enumerate() {
            if position == last {
                self.node(alternative)?;
                break;
            }
            let split = self.emit(op::SPLIT, self.here()? + 1, 0)?;
            self.node(alternative)?;
            jumps.push(self.emit(op::JUMP, 0, 0)?);
            self.code[split][2] = self.here()?;
        }
        let end = self.here()?;
        for jump in jumps {
            self.code[jump][1] = end;
        }
        Ok(())
    }
    /// One iteration: clear the body's captures, then match it, with an
    /// empty check when `register` is set.
    fn iteration(
        &mut self,
        node: &Node,
        reset: Option<(u16, u16)>,
        register: Option<u16>,
    ) -> Result<(), RegExpError> {
        if let Some(register) = register {
            self.emit(op::SAVE, register, 0)?;
        }
        if let Some((from, to)) = reset {
            self.emit(op::RESET, from, to)?;
        }
        self.node(node)?;
        if let Some(register) = register {
            self.emit(op::CHECK, register, 0)?;
        }
        Ok(())
    }
    fn split(&mut self, split: usize, body: u16, exit: u16, greedy: bool) {
        self.code[split][1] = if greedy { body } else { exit };
        self.code[split][2] = if greedy { exit } else { body };
    }
    fn repeat(
        &mut self,
        node: &Node,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    ) -> Result<(), RegExpError> {
        let reset = match node.groups() {
            Some((first, last)) if self.captures_on => {
                Some((index(2 * first)?, index(2 * last + 2)?))
            }
            _ => None,
        };
        for _ in 0..min {
            self.iteration(node, reset, None)?;
        }
        // Only captures can observe an empty iteration; an optional iteration
        // of a body that can match empty records its start in the register
        // of its depth and fails if it ends there.
        let register = (node.nullable() && self.captures_on).then_some(self.captures + self.depth);
        let Some(max) = max else {
            let head = self.emit(op::SPLIT, 0, 0)?;
            let body = self.here()?;
            self.checked_iteration(node, reset, register)?;
            self.emit(op::JUMP, index(head)?, 0)?;
            let exit = self.here()?;
            self.split(head, body, exit, greedy);
            return Ok(());
        };
        let mut splits = Vec::new();
        for _ in min..max {
            let split = self.emit(op::SPLIT, 0, 0)?;
            splits.push((split, self.here()?));
            self.checked_iteration(node, reset, register)?;
        }
        let exit = self.here()?;
        for (split, body) in splits {
            self.split(split, body, exit, greedy);
        }
        Ok(())
    }
}

/// A compiled pattern.
#[derive(Debug)]
pub(crate) struct Compiled {
    /// The full program, with every capture.
    pub units: Vec<u16>,
    /// The program without group captures, empty checks or resets: the same
    /// match bounds (slots 0 and 1) for operations that need nothing else.
    pub lean: Vec<u16>,
    pub groups: usize,
    /// Named groups with their one-based indices, in index order.
    pub names: Vec<(String, usize)>,
}

pub(crate) fn compile(
    node: &Node,
    groups: usize,
    names: &[Option<String>],
    flags: Flags,
) -> Result<Compiled, RegExpError> {
    let units = assemble(node, groups, flags, true)?;
    let lean = if groups == 0 {
        units.clone()
    } else {
        assemble(node, 0, flags, false)?
    };
    Ok(Compiled {
        units,
        lean,
        groups,
        names: names
            .iter()
            .enumerate()
            .filter_map(|(group, name)| Some((name.clone()?, group)))
            .collect(),
    })
}

fn assemble(
    node: &Node,
    groups: usize,
    flags: Flags,
    captures_on: bool,
) -> Result<Vec<u16>, RegExpError> {
    let captures = index(2 * (groups + 1))?;
    let mut builder = Builder {
        code: Vec::new(),
        classes: Vec::new(),
        flags,
        captures,
        depth: 0,
        registers: 0,
        captures_on,
    };
    builder.emit(op::SAVE, 0, 0)?;
    builder.node(node)?;
    builder.emit(op::SAVE, 1, 0)?;
    builder.emit(op::MATCH, 0, 0)?;
    let Builder {
        mut code,
        classes,
        registers,
        ..
    } = builder;
    let (first, other) = prefilter(&code, &classes, flags);
    // Class tables follow the instructions; CLASS operands become offsets.
    let mut offsets = Vec::with_capacity(classes.len());
    let mut offset = program::HEADER_UNITS + code.len() * program::INSTRUCTION_UNITS;
    for class in &classes {
        offsets.push(index(offset)?);
        offset += 1 + class.ranges().len() * program::RANGE_UNITS;
    }
    if offset > MAX_PROGRAM_UNITS {
        return Err(too_large());
    }
    for instruction in &mut code {
        if instruction[0] & program::OP_MASK == op::CLASS {
            instruction[1] = offsets[usize::from(instruction[1])];
        }
    }
    let threads = code
        .iter()
        .filter(|[op, ..]| consuming_or_match(op & program::OP_MASK))
        .count();
    // Each (instruction, freshness) state is followed at most once per
    // thread addition.
    let stack = (1 + code
        .iter()
        .map(|[op, a, b]| match op & program::OP_MASK {
            op::SPLIT | op::SAVE => 1,
            op::RESET => usize::from(b - a),
            _ => 0,
        })
        .sum::<usize>())
        << registers;
    let slots = captures + registers;
    let scratch = program::scratch_bytes(
        u32::from(captures),
        u32::from(slots),
        code.len() as u32,
        u32::from(registers),
        threads as u32,
        stack as u32,
    );
    if scratch > MAX_SCRATCH_BYTES {
        return Err(too_large());
    }
    let mut header = vec![0u16; program::HEADER_UNITS];
    header[program::FORMAT] = program::FORMAT_VERSION;
    let mut bits = 0;
    if flags.unicode {
        bits |= program::UNICODE;
    }
    if anchored(&code) {
        bits |= program::ANCHORED;
    }
    if let Some(first) = first {
        bits |= program::PREFILTER;
        header[program::FIRST_ASCII..program::FIRST_OTHER].copy_from_slice(&first);
        header[program::FIRST_OTHER] = u16::from(other);
        let units: Vec<u16> = (0..ASCII as u16)
            .filter(|unit| {
                first[usize::from(unit / UNIT_BITS as u16)] >> (unit % UNIT_BITS as u16) & 1 != 0
            })
            .collect();
        if !other && (1..=program::FIRST_UNITS).contains(&units.len()) {
            header[program::FIRST_COUNT] = units.len() as u16;
            header[program::FIRST_UNIT] = units[0];
            header[program::FIRST_UNIT + 1] = units[units.len() - 1];
        }
    }
    if flags.unicode && flags.ignore_case {
        bits |= program::EXTRA_WORD;
    }
    header[program::NESTING] = registers;
    header[program::FLAGS] = bits;
    header[program::CAPTURES] = captures;
    header[program::SLOTS] = slots;
    header[program::INSTRUCTIONS] = index(code.len())?;
    header[program::THREADS] = index(threads)?;
    header[program::STACK] = index(stack)?;
    let mut units = header;
    units.extend(code.iter().flatten());
    for class in &classes {
        units.push(index(class.ranges().len())?);
        for &(low, high) in class.ranges() {
            for bound in [low, high] {
                units.push(bound as u16);
                units.push((bound >> UNIT_BITS) as u16);
            }
        }
    }
    // Precomputed closures, when they fit; otherwise every follow is dynamic.
    let closures = closures(&code, units.len());
    if units.len() + closures.len() <= MAX_PROGRAM_UNITS {
        units[program::CLOSURE_INDEX] = index(units.len())?;
        units.extend(closures);
    }
    Ok(units)
}

fn consuming_or_match(op: u16) -> bool {
    matches!(op, op::MATCH | op::CHAR | op::ANY | op::CLASS)
}

/// One closure entry: the consuming instruction (or `MATCH`) reached and the
/// slot writes on the way, as `(a, b)` pairs (see `program::SET`).
struct Entry {
    target: u16,
    writes: Vec<(u16, u16)>,
}

/// The epsilon closure of instruction `start` in priority order, as the
/// matcher's follow would compute it: a split explores its first target
/// first, and an instruction seen before on any path is not followed again.
/// `None` when an assertion or empty check is reachable, whose outcome
/// depends on the position and the thread.
fn closure(code: &[[u16; 3]], start: usize) -> Option<Vec<Entry>> {
    let mut seen = vec![false; code.len()];
    let mut pending: Vec<(usize, Vec<(u16, u16)>)> = vec![(start, Vec::new())];
    let mut entries = Vec::new();
    while let Some((mut at, mut writes)) = pending.pop() {
        loop {
            if at >= code.len() || std::mem::replace(&mut seen[at], true) {
                break;
            }
            let [op, a, b] = code[at];
            match op & program::OP_MASK {
                op::JUMP => at = usize::from(a),
                op::SPLIT => {
                    pending.push((usize::from(b), writes.clone()));
                    at = usize::from(a);
                }
                op::SAVE => {
                    writes.push((a, program::SET));
                    at += 1;
                }
                op::RESET => {
                    writes.push((a, b));
                    at += 1;
                }
                op::CHECK | op::ASSERT => return None,
                _ => {
                    entries.push(Entry {
                        target: at as u16,
                        writes: std::mem::take(&mut writes),
                    });
                    break;
                }
            }
        }
    }
    Some(entries)
}

/// The closure index and lists, for a program whose tables end at unit
/// `base`: closures start at instruction 0 and after every consuming
/// instruction.
fn closures(code: &[[u16; 3]], base: usize) -> Vec<u16> {
    let mut index = vec![0u16; code.len()];
    let mut lists = Vec::new();
    let starts = std::iter::once(0).chain(
        code.iter()
            .enumerate()
            .filter(|(_, [op, ..])| matches!(op & program::OP_MASK, op::CHAR | op::ANY | op::CLASS))
            .map(|(at, _)| at + 1),
    );
    for start in starts {
        let Some(entries) = closure(code, start) else {
            continue;
        };
        let Ok(offset) = u16::try_from(base + code.len() + lists.len()) else {
            break;
        };
        index[start] = offset;
        lists.push(entries.len() as u16);
        for entry in entries {
            lists.push(entry.target);
            lists.push(entry.writes.len() as u16);
            for (a, b) in entry.writes {
                lists.push(a);
                lists.push(b);
            }
        }
    }
    index.extend(lists);
    index
}

/// Instructions reachable from instruction 0 without consuming a character;
/// `stop` ends a path at an instruction.
fn reachable(code: &[[u16; 3]], stop: impl Fn(&[u16; 3]) -> bool) -> Vec<usize> {
    let mut seen = vec![false; code.len()];
    let mut pending = vec![0usize];
    let mut reached = Vec::new();
    while let Some(at) = pending.pop() {
        if at >= code.len() || std::mem::replace(&mut seen[at], true) {
            continue;
        }
        let instruction = &code[at];
        if stop(instruction) {
            continue;
        }
        match instruction[0] & program::OP_MASK {
            op::JUMP => pending.push(usize::from(instruction[1])),
            op::SPLIT => {
                pending.push(usize::from(instruction[1]));
                pending.push(usize::from(instruction[2]));
            }
            op::SAVE | op::RESET | op::CHECK | op::ASSERT => pending.push(at + 1),
            _ => reached.push(at),
        }
    }
    reached
}

/// Whether every match must start at position 0 (`^` without `m`).
fn anchored(code: &[[u16; 3]]) -> bool {
    reachable(code, |[op, a, _]| {
        op & program::OP_MASK == op::ASSERT && *a == assertion::START
    })
    .is_empty()
}

/// The code units that can start a non-empty match, or `None` when a match
/// can be empty or start with any unit.
fn prefilter(
    code: &[[u16; 3]],
    classes: &[Set],
    flags: Flags,
) -> (Option<[u16; program::FIRST_ASCII_UNITS]>, bool) {
    let mut first = [0u16; program::FIRST_ASCII_UNITS];
    let mut other = false;
    let mut mark = |low: u32, high: u32, other: &mut bool| {
        for unit in low.min(ASCII)..high.saturating_add(1).min(ASCII) {
            first[(unit / UNIT_BITS) as usize] |= 1 << (unit % UNIT_BITS);
        }
        if high >= ASCII {
            *other = true;
        }
    };
    for at in reachable(code, |_| false) {
        let [op, a, _] = code[at];
        match op & program::OP_MASK {
            op::CHAR => mark(u32::from(a), u32::from(a), &mut other),
            op::CLASS => {
                let class = &classes[usize::from(a)];
                for &(low, high) in class.ranges() {
                    // Astral code points start with a high surrogate unit.
                    let high = if flags.unicode {
                        high.min(set::MAX_UNIT + 1)
                    } else {
                        high
                    };
                    mark(low.min(set::MAX_UNIT + 1), high, &mut other);
                }
            }
            _ => return (None, false),
        }
    }
    if other && first.iter().all(|bits| *bits == u16::MAX) {
        return (None, false);
    }
    (Some(first), other)
}
