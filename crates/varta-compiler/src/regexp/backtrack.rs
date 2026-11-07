//! Pattern trees to v38 backtracking programs (`varta_common::regexp`,
//! format version 2), run by the linked guest matcher
//! (`varta_intrinsics::regexp`).
//!
//! The program follows the ECMAScript 2025 pattern semantics (§22.2.2):
//! alternatives try left to right, greedy quantifiers prefer another
//! iteration and lazy ones the exit (RepeatMatcher with an iteration counter
//! and the empty check of optional iterations), every iteration first clears
//! its captures, backreferences match the first named group that took part,
//! lookarounds are atomic and lookbehind bodies (and their captures) are
//! matched backward. Case-insensitive characters and classes are widened
//! at compile time (`case`), so only backreferences compare by Canonicalize
//! at run time. Pattern modifiers change `i`, `m` and `s` for their body.
use super::RegExpError;
use super::case;
use super::compile::Compiled;
use super::parse::{Assertion, Flags, Node};
use super::set::{self, Set};
use varta_common::regexp::{
    BACKWARD, FORMAT_VERSION, MAX_PROGRAM_UNITS, UNBOUNDED, assertion, flag, header, look, op,
};

const ASCII: u32 = 0x80;
const UNIT_BITS: u32 = u16::BITS;
const HIGH_SURROGATE: u32 = 0xD800;
const SUPPLEMENTARY: u32 = 0x1_0000;
const SURROGATE_BITS: u32 = 10;

fn too_large() -> RegExpError {
    RegExpError::Unsupported(format!(
        "RegExp program larger than {MAX_PROGRAM_UNITS} code units"
    ))
}
fn unit(value: usize) -> Result<u16, RegExpError> {
    u16::try_from(value).map_err(|_| too_large())
}

/// Compile a parsed v38 pattern into its full and lean programs.
pub(crate) fn compile(
    node: &Node,
    groups: usize,
    names: &[Option<String>],
    flags: Flags,
) -> Result<Compiled, RegExpError> {
    let units = assemble(node, groups, flags, true)?;
    // Backreferences read captures, so their lean program is the full one.
    let lean = if groups == 0 || references(node) {
        units.clone()
    } else {
        assemble(node, groups, flags, false)?
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

fn references(node: &Node) -> bool {
    match node {
        Node::Backref(_) => true,
        Node::Group(inner, _)
        | Node::Repeat { node: inner, .. }
        | Node::Look { node: inner, .. }
        | Node::Modified(_, inner) => references(inner),
        Node::Concat(nodes) | Node::Alternation(nodes) => nodes.iter().any(references),
        _ => false,
    }
}

struct Assembler {
    /// Instructions; offsets in operands are absolute program units.
    code: Vec<u16>,
    classes: Vec<Set>,
    /// Code indices of `CLASS` operands and the class they name.
    patches: Vec<(usize, usize)>,
    registers: usize,
    unicode: bool,
    captures_on: bool,
}

impl Assembler {
    /// The absolute offset of the next instruction.
    fn here(&self) -> Result<u16, RegExpError> {
        unit(header::UNITS + self.code.len())
    }
    fn emit(&mut self, units: &[u16]) -> Result<usize, RegExpError> {
        if header::UNITS + self.code.len() + units.len() > MAX_PROGRAM_UNITS {
            return Err(too_large());
        }
        let at = self.code.len();
        self.code.extend_from_slice(units);
        Ok(at)
    }
    fn patch(&mut self, at: usize, value: u16) {
        self.code[at] = value;
    }
    fn wide(value: u32) -> [u16; 2] {
        [value as u16, (value >> UNIT_BITS) as u16]
    }
    fn max(&self) -> u32 {
        if self.unicode {
            set::MAX_CODE_POINT
        } else {
            set::MAX_UNIT
        }
    }
    /// The set a character class matches under `flags`.
    fn class_set(&self, set: &Set, negated: bool, flags: Flags) -> Set {
        let mut set = if flags.ignore_case {
            case::close(set, self.unicode)
        } else {
            set.clone()
        };
        if negated {
            set = set.complement(self.max());
        }
        set
    }
    /// One character of `set`.
    fn atom(&mut self, set: Set, backward: bool) -> Result<(), RegExpError> {
        let direction = if backward { BACKWARD } else { 0 };
        if set.ranges() == [(0, self.max())] {
            self.emit(&[op::ANY | direction])?;
            return Ok(());
        }
        match set.only() {
            Some(ch) if ch <= set::MAX_UNIT => {
                self.emit(&[op::CHAR | direction, ch as u16])?;
            }
            Some(ch) => {
                self.emit(&[op::CHAR32 | direction, (ch >> UNIT_BITS) as u16, ch as u16])?;
            }
            None => {
                let class = match self.classes.iter().position(|known| *known == set) {
                    Some(class) => class,
                    None => {
                        self.classes.push(set);
                        self.classes.len() - 1
                    }
                };
                let at = self.emit(&[op::CLASS | direction, 0])?;
                self.patches.push((at + 1, class));
            }
        }
        Ok(())
    }
    /// The single-character set of `node`, when it is one character without
    /// captures (for `STAR`).
    fn single(&self, node: &Node, flags: Flags) -> Option<Set> {
        match node {
            Node::Char(ch) => Some(self.class_set(&Set::single(*ch), false, flags)),
            Node::Class { set, negated } => Some(self.class_set(set, *negated, flags)),
            Node::Dot if flags.dot_all => Some(Set::from_ranges([(0, self.max())])),
            Node::Dot => Some(set::line_terminators().complement(self.max())),
            Node::Group(inner, None) => self.single(inner, flags),
            Node::Modified(modifiers, inner) => self.single(inner, modifiers.apply(flags)),
            _ => None,
        }
    }
    fn node(&mut self, node: &Node, flags: Flags, backward: bool) -> Result<(), RegExpError> {
        match node {
            Node::Empty => {}
            Node::Char(_) | Node::Class { .. } | Node::Dot => {
                let set = self.single(node, flags).unwrap_or_default();
                self.atom(set, backward)?;
            }
            Node::Assert(kind) => {
                let extra = self.unicode && flags.ignore_case;
                let kind = match (kind, flags.multiline, extra) {
                    (Assertion::Start, false, _) => assertion::START,
                    (Assertion::Start, true, _) => assertion::LINE_START,
                    (Assertion::End, false, _) => assertion::END,
                    (Assertion::End, true, _) => assertion::LINE_END,
                    (Assertion::WordBoundary, _, false) => assertion::WORD_BOUNDARY,
                    (Assertion::WordBoundary, _, true) => assertion::WORD_BOUNDARY_EXTRA,
                    (Assertion::NotWordBoundary, _, false) => assertion::NOT_WORD_BOUNDARY,
                    (Assertion::NotWordBoundary, _, true) => assertion::NOT_WORD_BOUNDARY_EXTRA,
                };
                self.emit(&[op::ASSERT, kind])?;
            }
            Node::Group(inner, Some(group)) if self.captures_on => {
                let (start, end) = (unit(2 * group)?, unit(2 * group + 1)?);
                let (first, last) = if backward { (end, start) } else { (start, end) };
                self.emit(&[op::SAVE, first])?;
                self.node(inner, flags, backward)?;
                self.emit(&[op::SAVE, last])?;
            }
            Node::Group(inner, _) => self.node(inner, flags, backward)?,
            Node::Modified(modifiers, inner) => {
                self.node(inner, modifiers.apply(flags), backward)?;
            }
            Node::Concat(nodes) => {
                if backward {
                    for inner in nodes.iter().rev() {
                        self.node(inner, flags, backward)?;
                    }
                } else {
                    for inner in nodes {
                        self.node(inner, flags, backward)?;
                    }
                }
            }
            Node::Alternation(alternatives) => {
                let mut exits = Vec::new();
                for (index, alternative) in alternatives.iter().enumerate() {
                    if index + 1 == alternatives.len() {
                        self.node(alternative, flags, backward)?;
                        break;
                    }
                    let split = self.emit(&[op::SPLIT, 0, 0])?;
                    let first = self.here()?;
                    self.patch(split + 1, first);
                    self.node(alternative, flags, backward)?;
                    exits.push(self.emit(&[op::JUMP, 0])? + 1);
                    let second = self.here()?;
                    self.patch(split + 2, second);
                }
                let end = self.here()?;
                for exit in exits {
                    self.patch(exit, end);
                }
            }
            Node::Repeat {
                node: inner,
                min,
                max,
                greedy,
            } => self.repeat(inner, *min, *max, *greedy, flags, backward)?,
            Node::Backref(groups) => {
                let direction = if backward { BACKWARD } else { 0 };
                let mut units = vec![
                    op::BACKREF | direction,
                    u16::from(flags.ignore_case),
                    unit(groups.len())?,
                ];
                for group in groups {
                    units.push(unit(*group)?);
                }
                self.emit(&units)?;
            }
            Node::Look {
                behind,
                negative,
                node: inner,
            } => {
                let kind = if *behind { look::BEHIND } else { 0 }
                    | if *negative { look::NEGATIVE } else { 0 };
                let at = self.emit(&[op::LOOK, kind, 0])?;
                self.node(inner, flags, *behind)?;
                self.emit(&[op::LOOK_END])?;
                let next = self.here()?;
                self.patch(at + 2, next);
            }
        }
        Ok(())
    }
    fn repeat(
        &mut self,
        inner: &Node,
        min: u32,
        max: Option<u32>,
        greedy: bool,
        flags: Flags,
        backward: bool,
    ) -> Result<(), RegExpError> {
        let max_value = max.unwrap_or(UNBOUNDED);
        if max_value == 0 {
            return Ok(());
        }
        if let Some(set) = self.single(inner, flags) {
            let [min_low, min_high] = Self::wide(min);
            let [max_low, max_high] = Self::wide(max_value);
            self.emit(&[
                op::STAR,
                min_low,
                min_high,
                max_low,
                max_high,
                u16::from(greedy),
            ])?;
            return self.atom(set, backward);
        }
        if min == 1 && max_value == 1 {
            return self.node(inner, flags, backward);
        }
        let register = unit(self.registers)?;
        self.registers += 2;
        self.emit(&[op::REPEAT_INIT, register])?;
        let head = self.here()?;
        let [min_low, min_high] = Self::wide(min);
        let [max_low, max_high] = Self::wide(max_value);
        let at = self.emit(&[
            op::REPEAT,
            register,
            min_low,
            min_high,
            max_low,
            max_high,
            u16::from(greedy),
            0,
        ])?;
        self.emit(&[op::ENTER, register])?;
        if self.captures_on
            && let Some((first, last)) = inner.groups()
        {
            self.emit(&[op::RESET, unit(2 * first)?, unit(2 * last + 2)?])?;
        }
        self.node(inner, flags, backward)?;
        self.emit(&[op::REPEAT_END, register, head])?;
        let exit = self.here()?;
        self.patch(at + 7, exit);
        Ok(())
    }
}

/// The code units that can start a non-empty match of `node`, or `None`
/// when any can (a backreference).
fn first(node: &Node, flags: Flags, unicode: bool) -> Option<Set> {
    let max = if unicode {
        set::MAX_CODE_POINT
    } else {
        set::MAX_UNIT
    };
    let characters = |set: Set| {
        // Characters to their first code units: astral code points start
        // with their high surrogate.
        let mut units = Set::default();
        for &(low, high) in set.ranges() {
            if low <= set::MAX_UNIT {
                units.add(low, high.min(set::MAX_UNIT));
            }
            if high >= SUPPLEMENTARY {
                let lead = |ch: u32| HIGH_SURROGATE + ((ch - SUPPLEMENTARY) >> SURROGATE_BITS);
                units.add(lead(low.max(SUPPLEMENTARY)), lead(high));
            }
        }
        units
    };
    match node {
        Node::Empty | Node::Assert(_) | Node::Look { .. } => Some(Set::default()),
        Node::Backref(_) => None,
        Node::Char(ch) => Some(characters(if flags.ignore_case {
            case::close(&Set::single(*ch), unicode)
        } else {
            Set::single(*ch)
        })),
        Node::Class { set, negated } => {
            let mut set = if flags.ignore_case {
                case::close(set, unicode)
            } else {
                set.clone()
            };
            if *negated {
                set = set.complement(max);
            }
            Some(characters(set))
        }
        Node::Dot => Some(characters(Set::from_ranges([(0, max)]))),
        Node::Group(inner, _) => first(inner, flags, unicode),
        Node::Modified(modifiers, inner) => first(inner, modifiers.apply(flags), unicode),
        Node::Repeat { max: Some(0), .. } => Some(Set::default()),
        Node::Repeat { node: inner, .. } => first(inner, flags, unicode),
        Node::Concat(nodes) => {
            let mut units = Set::default();
            for inner in nodes {
                units.union(&first(inner, flags, unicode)?);
                if !inner.nullable() {
                    break;
                }
            }
            Some(units)
        }
        Node::Alternation(nodes) => {
            let mut units = Set::default();
            for inner in nodes {
                units.union(&first(inner, flags, unicode)?);
            }
            Some(units)
        }
    }
}

/// Whether every match of `node` starts with `^` without `m`.
fn anchored(node: &Node, flags: Flags) -> bool {
    match node {
        Node::Assert(Assertion::Start) => !flags.multiline,
        Node::Group(inner, _) => anchored(inner, flags),
        Node::Modified(modifiers, inner) => anchored(inner, modifiers.apply(flags)),
        Node::Concat(nodes) => nodes.first().is_some_and(|inner| anchored(inner, flags)),
        Node::Alternation(nodes) => nodes.iter().all(|inner| anchored(inner, flags)),
        _ => false,
    }
}

fn assemble(
    node: &Node,
    groups: usize,
    flags: Flags,
    captures_on: bool,
) -> Result<Vec<u16>, RegExpError> {
    let unicode = flags.unicode || flags.unicode_sets;
    let mut assembler = Assembler {
        code: Vec::new(),
        classes: Vec::new(),
        patches: Vec::new(),
        registers: 0,
        unicode,
        captures_on,
    };
    assembler.node(node, flags, false)?;
    assembler.emit(&[op::MATCH])?;
    let mut program = vec![0u16; header::UNITS];
    program[header::FORMAT] = FORMAT_VERSION;
    program[header::CAPTURES] = unit(2 * (groups + 1))?;
    program[header::REGISTERS] = unit(assembler.registers)?;
    program[header::CODE] = unit(header::UNITS)?;
    let mut bits = if unicode { flag::UNICODE } else { 0 };
    if anchored(node, flags) {
        bits |= flag::ANCHORED;
    }
    if !node.nullable()
        && let Some(units) = first(node, flags, unicode)
    {
        bits |= flag::PREFILTER;
        for &(low, high) in units.ranges() {
            for ch in low..=high.min(ASCII - 1) {
                program[header::FIRST_ASCII + (ch / UNIT_BITS) as usize] |= 1 << (ch % UNIT_BITS);
            }
            if high >= ASCII {
                program[header::FIRST_OTHER] = 1;
            }
        }
    }
    program[header::FLAGS] = bits;
    // Class tables follow the code.
    let mut offsets = Vec::with_capacity(assembler.classes.len());
    let mut tables = Vec::new();
    let code_end = header::UNITS + assembler.code.len();
    for class in &assembler.classes {
        offsets.push(unit(code_end + tables.len())?);
        tables.push(unit(class.ranges().len())?);
        for &(low, high) in class.ranges() {
            tables.extend(Assembler::wide(low));
            tables.extend(Assembler::wide(high));
        }
    }
    for &(at, class) in &assembler.patches {
        assembler.code[at] = offsets[class];
    }
    program.extend(assembler.code);
    program.extend(tables);
    if program.len() > MAX_PROGRAM_UNITS {
        return Err(too_large());
    }
    Ok(program)
}
