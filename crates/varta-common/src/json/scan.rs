//! The shared checked JSON decoder over UTF-16LE text (`no_std`).
//!
//! One implementation for every recoverable checked parse: the host imports
//! of the recoverable profiles (`json_parse_v13` and later) and the linked
//! v38 decoder (`json_parse_v19`) both run [`scan`]. It reads the guest's
//! UTF-16 text in place and produces a [`Tape`]: every value of the input in
//! document order, completely checked against the schema, before any guest
//! value is built. Consumers build from the tape: the host turns it into a
//! [`JsonValue`] tree ([`Tape::value`]), the linked decoder writes guest
//! blocks directly.
//!
//! The outcome is exactly that of the reference classification
//! (`JsonSchema::parse_json_classified` over the input's UTF-8 transcoding,
//! `json_input_limited`): the transcoding's byte limit and its lone-surrogate
//! syntax errors first, then the JSON grammar over the whole input, then the
//! whole-input budgets in document order (depth, visited values, array
//! elements, string units), then the schema in document order (a type
//! mismatch, or the decode-only budgets: dictionary entries and the values
//! that missing optional fields add). Grammar is `serde_json`'s strict
//! RFC 8259 grammar. Numbers convert as `serde_json` (`float_roundtrip`)
//! converts them: correctly rounded, an infinite result a type mismatch, and
//! `-0` a binary64 negative zero that no i32 slot accepts. Strings keep their
//! exact UTF-16 units, lone surrogates included; record keys must be well
//! formed.
//!
//! A first pass checks everything optimistically and stops at the first
//! anomaly; only then does a second, schema-free pass establish which
//! anomaly the reference order reports.
use super::*;
use crate::value::{
    MAX_WIDE_RECORD_FIELDS, ValueKind, array_index, dictionary_entry_limit, dictionary_hash,
};

const QUOTE: u16 = b'"' as u16;
const BACKSLASH: u16 = b'\\' as u16;
const COMMA: u16 = b',' as u16;
const COLON: u16 = b':' as u16;
const OPEN_OBJECT: u16 = b'{' as u16;
const CLOSE_OBJECT: u16 = b'}' as u16;
const OPEN_ARRAY: u16 = b'[' as u16;
const CLOSE_ARRAY: u16 = b']' as u16;
const MINUS: u16 = b'-' as u16;
const PLUS: u16 = b'+' as u16;
const DOT: u16 = b'.' as u16;
const ZERO: u16 = b'0' as u16;
const NINE: u16 = b'9' as u16;
/// The first unit no JSON string may hold unescaped.
const CONTROL_END: u16 = 0x20;
const SURROGATE_START: u16 = 0xd800;
const LOW_SURROGATE_START: u16 = 0xdc00;
const SURROGATE_END: u16 = 0xe000;
/// UTF-8 bytes of a lone surrogate in the reference transcoding, which
/// writes it as a `\uXXXX` escape.
const ESCAPED_SURROGATE_BYTES: usize = 6;
/// The most UTF-8 bytes one UTF-16 unit of input can become in the reference
/// transcoding.
const MAX_TRANSCODED_BYTES: usize = ESCAPED_SURROGATE_BYTES;
/// Units below 0x100 a string holds as themselves, the fast path of
/// [`Input::string`]: not `"`, `\\` or a control character.
static PLAIN: [bool; 256] = {
    let mut plain = [true; 256];
    let mut unit = 0;
    while unit < CONTROL_END as usize {
        plain[unit] = false;
        unit += 1;
    }
    plain[QUOTE as usize] = false;
    plain[BACKSLASH as usize] = false;
    plain
};
/// Values visited between two deadline checks.
const DEADLINE_STRIDE: usize = 1024;
/// Decimal digits of an integer literal that binary64 holds exactly
/// (`10^15 < 2^53`).
const EXACT_DIGITS: usize = 15;
/// Entries of a dictionary searched linearly for duplicate keys before a
/// hash index is built.
const LINEAR_KEYS: usize = 16;

/// The data limits of one decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonLimits {
    pub depth: usize,
    pub nodes: usize,
    /// UTF-8 bytes of the transcoded input.
    pub input_bytes: usize,
    /// UTF-16 units of one string or key.
    pub string_units: usize,
    pub array_elements: usize,
    pub dictionary_entries: usize,
}
impl JsonLimits {
    /// The limits of `data` for a schema materializing value layout
    /// `value_version`.
    #[must_use]
    pub fn of(data: crate::limits::DataLimits, value_version: u32) -> Self {
        Self {
            depth: data.json_depth,
            nodes: data.json_nodes,
            input_bytes: data.json_input_bytes,
            string_units: data.string_bytes / 2,
            array_elements: data.array_elements,
            dictionary_entries: dictionary_entry_limit(value_version, data),
        }
    }
    /// V38 (`data_limits`) limits of schema 18, as [`Self::of`] computes
    /// them from `DataLimits::V38`.
    pub const V38: Self = Self {
        depth: crate::limits::V38_MAX_JSON_DEPTH,
        nodes: usize::MAX,
        input_bytes: crate::limits::V38_MAX_STRING_BYTES,
        string_units: crate::limits::V38_MAX_STRING_BYTES / 2,
        array_elements: crate::limits::V38_MAX_ARRAY_ELEMENTS,
        dictionary_entries: crate::limits::V38_MAX_ARRAY_ELEMENTS / 2,
    };
}

/// A string or key of the input: units `start..end` between its quotes,
/// `units` units once decoded, `escaped` when it holds an escape (so its
/// units are not the raw range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    pub start: u32,
    pub end: u32,
    pub units: u32,
    pub escaped: bool,
}

/// One checked value of the input, in document order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Null,
    Boolean(bool),
    I32(i32),
    Float64(u64),
    String(Text),
    /// A union value of schema type `ty`: its member's value follows.
    Union {
        ty: u32,
        kind: ValueKind,
    },
    /// A record of schema type `ty` with `count` present fields, each a
    /// [`Node::Field`] followed by its value. Missing fields are optional.
    Record {
        ty: u32,
        count: u32,
    },
    Field {
        slot: u32,
    },
    /// An array of schema type `ty`: `count` element values follow.
    Array {
        ty: u32,
        count: u32,
    },
    /// A dictionary of schema type `ty`: `count` entries follow in input
    /// order, each a [`Node::Key`] and its value. `indexed` when a key is a
    /// canonical array index, so own-key order differs from input order.
    Dictionary {
        ty: u32,
        count: u32,
        indexed: bool,
    },
    Key(Text),
}

/// The checked values of one input.
#[derive(Debug, Default)]
pub struct Tape {
    pub nodes: Vec<Node>,
}
impl Tape {
    /// The values on the tape (fields and keys only name the next one).
    #[must_use]
    pub fn values(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| !matches!(node, Node::Field { .. } | Node::Key(_)))
            .count()
    }
}

/// Stops the decoder once the host's deadline passed (checked every
/// [`DEADLINE_STRIDE`] values); linked code is bounded by fuel instead.
pub trait Deadline {
    /// Whether this deadline is ever checked.
    const TIMED: bool = true;
    fn overdue(&mut self) -> bool;
}
/// No deadline.
pub struct Unbounded;
impl Deadline for Unbounded {
    const TIMED: bool = false;
    fn overdue(&mut self) -> bool {
        false
    }
}

/// Decode UTF-16LE `text` (an even byte count) against `schema` within
/// `limits` into `tape`. The schema must already be validated.
pub fn scan<D: Deadline>(
    text: &[u8],
    schema: &JsonSchema,
    limits: JsonLimits,
    tape: &mut Tape,
    deadline: &mut D,
) -> Result<(), JsonDecodeError> {
    let (units, rest) = text.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(JsonDecodeError::Syntax { line: 1, column: 1 });
    }
    let input = Input { units };
    if units.len().saturating_mul(MAX_TRANSCODED_BYTES) > limits.input_bytes {
        transcoding(units, limits.input_bytes)?;
    }
    tape.nodes.clear();
    let stop = match (Decoder {
        input,
        schema,
        limits,
        nodes: 0,
        missing: 0,
        tape,
        frames: Vec::new(),
        keys: Vec::new(),
        open_keys: 0,
        deadline,
        digits: Vec::new(),
        decoded: Vec::new(),
    })
    .run()
    {
        Ok(()) => return Ok(()),
        Err(stop) => stop,
    };
    tape.nodes.clear();
    match stop {
        Stop::Syntax(pos) => Err(input.syntax(pos)),
        // Only the grammar can still precede a whole-input budget.
        Stop::Walk(limit) => match Checker::new(input, limits, false, deadline).run() {
            Err(Stop::Syntax(pos)) => Err(input.syntax(pos)),
            _ => Err(JsonDecodeError::Limit(limit)),
        },
        // The grammar and every whole-input budget precede the schema.
        Stop::Mismatch | Stop::Counted(_) => {
            let error = match stop {
                Stop::Counted(limit) => JsonDecodeError::Limit(limit),
                _ => JsonDecodeError::Type,
            };
            match Checker::new(input, limits, true, deadline).run() {
                Ok(()) | Err(Stop::Mismatch) => Err(error),
                Err(Stop::Syntax(pos)) => Err(input.syntax(pos)),
                Err(Stop::Walk(limit) | Stop::Counted(limit)) => Err(JsonDecodeError::Limit(limit)),
            }
        }
    }
}

/// The reference transcoding's own failures, in its scan order: a lone
/// surrogate outside a string or right after a backslash is a syntax error,
/// and output beyond `limit` bytes the input byte limit. Needed only when
/// the input could transcode past the limit; otherwise the grammar reports
/// every such surrogate itself.
fn transcoding(units: &[[u8; 2]], limit: usize) -> Result<(), JsonDecodeError> {
    let (mut output, mut quoted, mut escaped) = (0usize, false, false);
    for value in char::decode_utf16(units.iter().map(|unit| u16::from_le_bytes(*unit))) {
        match value {
            Err(_) if !quoted || escaped => {
                return Err(JsonDecodeError::Syntax { line: 1, column: 1 });
            }
            Err(_) => output += ESCAPED_SURROGATE_BYTES,
            Ok(c) => {
                if quoted {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        quoted = false;
                    }
                } else if c == '"' {
                    quoted = true;
                }
                output += c.len_utf8();
            }
        }
        if output > limit {
            return Err(JsonDecodeError::Limit(JsonInputLimit::Bytes));
        }
    }
    Ok(())
}

/// Why a pass stopped (small and `Copy`: every check returns it).
#[derive(Clone, Copy)]
enum Stop {
    /// The grammar failed at this unit.
    Syntax(usize),
    /// A whole-input budget.
    Walk(JsonInputLimit),
    /// The value does not match the schema.
    Mismatch,
    /// A budget only the schema counts (dictionary entries, the values of
    /// missing optional fields).
    Counted(JsonInputLimit),
}
#[inline]
fn mismatch<T>() -> Result<T, Stop> {
    Err(Stop::Mismatch)
}

#[derive(Clone, Copy)]
struct Input<'a> {
    units: &'a [[u8; 2]],
}
impl Input<'_> {
    #[inline]
    fn at(self, pos: usize) -> Option<u16> {
        self.units.get(pos).map(|unit| u16::from_le_bytes(*unit))
    }
    #[inline]
    fn len(self) -> usize {
        self.units.len()
    }
    /// The first non-whitespace position at or after `pos`.
    #[inline]
    fn skip(self, mut pos: usize) -> usize {
        while let Some(unit) = self.at(pos) {
            if !matches!(unit, 0x20 | 0x0a | 0x09 | 0x0d) {
                break;
            }
            pos += 1;
        }
        pos
    }
    /// One-based line and column (in units) of `pos`, for diagnostics.
    fn syntax(self, pos: usize) -> JsonDecodeError {
        let (mut line, mut column) = (1, 1);
        for index in 0..pos.min(self.len()) {
            if self.at(index) == Some(0x0a) {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        JsonDecodeError::Syntax { line, column }
    }
    /// The string whose opening quote is at `pos`: its text, the position
    /// after the closing quote, and whether it holds a surrogate (raw or
    /// escaped), so that a record key needs a well-formedness check.
    #[inline]
    fn string(self, pos: usize) -> Result<(Text, usize, bool), usize> {
        let start = pos + 1;
        let mut at = start;
        // Plain units first: below 0x100 and neither a quote, a backslash
        // nor a control character.
        while let Some(&[low, 0]) = self.units.get(at) {
            if !PLAIN[usize::from(low)] {
                break;
            }
            at += 1;
        }
        if self.units.get(at) == Some(&[b'"', 0]) {
            let end = at as u32;
            return Ok((
                Text {
                    start: start as u32,
                    end,
                    units: end - start as u32,
                    escaped: false,
                },
                at + 1,
                false,
            ));
        }
        let mut units = at - start;
        let mut escaped = false;
        let mut surrogate = false;
        loop {
            let unit = self.at(at).ok_or(at)?;
            match unit {
                QUOTE => break,
                BACKSLASH => {
                    escaped = true;
                    let (code, next) = self.escape(at + 1)?;
                    surrogate |= (SURROGATE_START..SURROGATE_END).contains(&code);
                    at = next;
                }
                _ if unit < CONTROL_END => return Err(at),
                _ => {
                    surrogate |= (SURROGATE_START..SURROGATE_END).contains(&unit);
                    at += 1;
                }
            }
            units += 1;
        }
        Ok((
            Text {
                start: start as u32,
                end: at as u32,
                units: units as u32,
                escaped,
            },
            at + 1,
            surrogate,
        ))
    }
    /// The escape after a backslash at `pos`: its code unit and the next
    /// position.
    #[inline]
    fn escape(self, pos: usize) -> Result<(u16, usize), usize> {
        let unit = self.at(pos).ok_or(pos)?;
        let code = match unit {
            0x22 => QUOTE,
            0x5c => BACKSLASH,
            0x2f => u16::from(b'/'),
            0x62 => 0x08,
            0x66 => 0x0c,
            0x6e => 0x0a,
            0x72 => 0x0d,
            0x74 => 0x09,
            0x75 => {
                let mut code = 0u16;
                for offset in 1..=4 {
                    let digit = self.at(pos + offset).ok_or(pos + offset)?;
                    let value = match digit {
                        0x30..=0x39 => digit - 0x30,
                        0x41..=0x46 => digit - 0x41 + 10,
                        0x61..=0x66 => digit - 0x61 + 10,
                        _ => return Err(pos + offset),
                    };
                    code = code << 4 | value;
                }
                return Ok((code, pos + 5));
            }
            _ => return Err(pos),
        };
        Ok((code, pos + 1))
    }
    /// The number literal at `pos`: its end and whether it is an integer
    /// literal (no fraction, no exponent).
    fn number(self, pos: usize) -> Result<(usize, bool), usize> {
        let mut at = pos;
        if self.at(at) == Some(MINUS) {
            at += 1;
        }
        match self.at(at) {
            Some(ZERO) => {
                at += 1;
                if self.at(at).is_some_and(is_digit) {
                    return Err(at);
                }
            }
            Some(unit) if is_digit(unit) => {
                while self.at(at).is_some_and(is_digit) {
                    at += 1;
                }
            }
            _ => return Err(at),
        }
        let mut integer = true;
        if self.at(at) == Some(DOT) {
            integer = false;
            at += 1;
            if !self.at(at).is_some_and(is_digit) {
                return Err(at);
            }
            while self.at(at).is_some_and(is_digit) {
                at += 1;
            }
        }
        if matches!(self.at(at), Some(0x45 | 0x65)) {
            integer = false;
            at += 1;
            if matches!(self.at(at), Some(PLUS | MINUS)) {
                at += 1;
            }
            if !self.at(at).is_some_and(is_digit) {
                return Err(at);
            }
            while self.at(at).is_some_and(is_digit) {
                at += 1;
            }
        }
        Ok((at, integer))
    }
    /// The literal `word` (ASCII) at `pos`: the position after it.
    #[inline]
    fn literal(self, pos: usize, word: &[u8]) -> Result<usize, usize> {
        for (offset, byte) in word.iter().enumerate() {
            if self.at(pos + offset) != Some(u16::from(*byte)) {
                return Err(pos + offset);
            }
        }
        Ok(pos + word.len())
    }
    /// Append the decoded units of `text` to `out`.
    pub(crate) fn decode(self, text: Text, out: &mut Vec<u16>) {
        self.pieces(text, |piece| match piece {
            Piece::Units(start, end) => {
                out.extend(
                    self.units[start..end]
                        .iter()
                        .map(|unit| u16::from_le_bytes(*unit)),
                );
            }
            Piece::Escape(code) => out.push(code),
        });
    }
    /// The decoded units of `text` as runs of raw units and single escapes.
    fn pieces(self, text: Text, mut each: impl FnMut(Piece)) {
        let mut at = text.start as usize;
        let end = (text.end as usize).min(self.len());
        let mut run = at;
        while at < end {
            if self.at(at) == Some(BACKSLASH) {
                if run < at {
                    each(Piece::Units(run, at));
                }
                match self.escape(at + 1) {
                    Ok((code, next)) => {
                        each(Piece::Escape(code));
                        at = next;
                    }
                    // Unreachable for a checked text.
                    Err(_) => at += 1,
                }
                run = at;
            } else {
                at += 1;
            }
        }
        if run < end {
            each(Piece::Units(run, end));
        }
    }
}
/// A run of a string's decoded units: raw units `start..end` of the text,
/// or one escape's unit.
#[derive(Debug, Clone, Copy)]
pub enum Piece {
    Units(usize, usize),
    Escape(u16),
}
/// The decoded units of `text` in UTF-16LE `input`, as [`Piece`]s.
pub fn pieces(input: &[u8], text: Text, each: impl FnMut(Piece)) {
    Input {
        units: input.as_chunks::<2>().0,
    }
    .pieces(text, each);
}
#[inline]
fn is_digit(unit: u16) -> bool {
    (ZERO..=NINE).contains(&unit)
}

/// The decoded units of `text` in `input`, appended to `out`.
pub fn decode_text(input: &[u8], text: Text, out: &mut Vec<u16>) {
    Input {
        units: input.as_chunks::<2>().0,
    }
    .decode(text, out);
}

/// A set of record slots (at most [`MAX_WIDE_RECORD_FIELDS`]) as two words:
/// wasm32 has no native 128-bit shifts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slots([u64; 2]);
impl Slots {
    pub(crate) const EMPTY: Self = Self([0; 2]);
    /// Presence word `word` (32 slots) of a record trailer.
    #[inline]
    pub(crate) fn word(self, word: usize) -> u32 {
        (self.0[word / 2] >> (32 * (word % 2))) as u32
    }
    #[inline]
    pub(crate) fn contains(self, slot: usize) -> bool {
        self.0[slot / 64] & 1 << (slot % 64) != 0
    }
    #[inline]
    pub(crate) fn insert(&mut self, slot: usize) {
        self.0[slot / 64] |= 1 << (slot % 64);
    }
    #[inline]
    fn count(self) -> u32 {
        self.0[0].count_ones() + self.0[1].count_ones()
    }
    /// Every slot of a record with `count` fields.
    #[inline]
    pub(crate) fn all(count: usize) -> Self {
        let word = |bits: usize| match bits {
            0 => 0,
            64.. => u64::MAX,
            bits => (1u64 << bits) - 1,
        };
        Self([word(count.min(64)), word(count.saturating_sub(64))])
    }
}
const _: () = assert!(MAX_WIDE_RECORD_FIELDS <= 128);

/// An open container of the first pass.
enum Frame {
    Array {
        node: usize,
        element: u32,
        count: usize,
        depth: usize,
    },
    Record {
        node: usize,
        ty: u32,
        present: Slots,
        depth: usize,
    },
    /// A dictionary; its keys are the innermost of `Decoder::keys`.
    Dictionary {
        node: usize,
        value: u32,
        depth: usize,
    },
}

/// The distinct keys of one dictionary: hashes and texts, with a hash index
/// once it outgrows a linear search.
#[derive(Default)]
struct Keys {
    entries: Vec<(u32, Text)>,
    index: Vec<u32>,
    indexed: bool,
}

struct Decoder<'a, 'b, D> {
    input: Input<'a>,
    schema: &'a JsonSchema,
    limits: JsonLimits,
    /// Values visited by the whole-input budget.
    nodes: usize,
    /// Missing optional fields, which the schema counts as values too.
    missing: usize,
    tape: &'b mut Tape,
    frames: Vec<Frame>,
    /// The key sets of the open dictionaries, innermost last; retired sets
    /// are reused.
    keys: Vec<Keys>,
    open_keys: usize,
    deadline: &'b mut D,
    /// Scratch for number literals and decoded keys.
    digits: Vec<u8>,
    decoded: Vec<u16>,
}

/// What follows a value's first token.
enum Next {
    /// The value is complete.
    Done,
    /// A container opened: its first child has this type and depth.
    Child(u32, usize),
}

impl<D: Deadline> Decoder<'_, '_, D> {
    fn run(&mut self) -> Result<(), Stop> {
        let input = self.input;
        let mut ty = self.schema.root;
        let mut depth = 0;
        let mut pos = 0;
        loop {
            pos = input.skip(pos);
            // Arrays of plain numbers or strings: one element after another
            // without the general dispatch (the same checks, in order).
            if let Some(Frame::Array { element, .. }) = self.frames.last()
                && *element == ty
                && matches!(
                    self.schema.types[ty as usize],
                    JsonType::Float64 | JsonType::String
                )
            {
                let (after, done) = self.elements(ty, depth, pos)?;
                pos = after;
                if done {
                    // The last element taken is complete; its separator
                    // follows.
                    if self.close_loop(&mut ty, &mut depth, &mut pos)? {
                        return Ok(());
                    }
                    continue;
                }
            }
            let (next, after) = self.value(ty, depth, pos)?;
            pos = after;
            if let Next::Child(child, level) = next {
                (ty, depth) = (child, level);
                continue;
            }
            // Close containers and find the next value.
            if self.close_loop(&mut ty, &mut depth, &mut pos)? {
                return Ok(());
            }
        }
    }

    /// After a complete value at `pos`: close every finished container and
    /// move to the next value (its type, depth and position); `true` once
    /// the root is complete.
    fn close_loop(
        &mut self,
        ty: &mut u32,
        depth: &mut usize,
        pos: &mut usize,
    ) -> Result<bool, Stop> {
        let input = self.input;
        loop {
            *pos = input.skip(*pos);
            let Some(frame) = self.frames.last_mut() else {
                return if *pos == input.len() {
                    Ok(true)
                } else {
                    Err(Stop::Syntax(*pos))
                };
            };
            let unit = input.at(*pos);
            match frame {
                Frame::Array {
                    node,
                    element,
                    count,
                    depth: level,
                } => {
                    *count += 1;
                    if *count > self.limits.array_elements {
                        return Err(Stop::Walk(JsonInputLimit::ArrayElements));
                    }
                    match unit {
                        Some(COMMA) => {
                            *pos += 1;
                            *ty = *element;
                            *depth = *level + 1;
                            return Ok(false);
                        }
                        Some(CLOSE_ARRAY) => {
                            *pos += 1;
                            let (node, count) = (*node, *count as u32);
                            if let Some(Node::Array { count: slot, .. }) =
                                self.tape.nodes.get_mut(node)
                            {
                                *slot = count;
                            }
                            self.frames.pop();
                        }
                        _ => return Err(Stop::Syntax(*pos)),
                    }
                }
                Frame::Record { .. } | Frame::Dictionary { .. } => match unit {
                    Some(COMMA) => {
                        let (child, level, after) = self.member(input.skip(*pos + 1))?;
                        (*ty, *depth, *pos) = (child, level, after);
                        return Ok(false);
                    }
                    Some(CLOSE_OBJECT) => {
                        *pos += 1;
                        self.close(*pos)?;
                    }
                    _ => return Err(Stop::Syntax(*pos)),
                },
            }
        }
    }

    /// Elements of plain type `ty` (a number or a string) of the innermost
    /// array from `pos`, with the checks of [`Self::value`] and of the
    /// separator in the same order: the position reached, and whether an
    /// element taken there is complete (its separator not consumed).
    /// Anything else is left to the general path at the returned position.
    fn elements(&mut self, ty: u32, depth: usize, mut pos: usize) -> Result<(usize, bool), Stop> {
        let input = self.input;
        let number = matches!(self.schema.types[ty as usize], JsonType::Float64);
        loop {
            let Some(unit) = input.at(pos) else {
                return Ok((pos, false));
            };
            let node = if number && (unit == MINUS || is_digit(unit)) {
                self.visit(depth)?;
                self.counted()?;
                let (end, integer) = input.number(pos).map_err(Stop::Syntax)?;
                let bits = self.float(pos, end, integer)?;
                pos = end;
                Node::Float64(bits)
            } else if !number && unit == QUOTE {
                self.visit(depth)?;
                self.counted()?;
                let (text, next, _) = input.string(pos).map_err(Stop::Syntax)?;
                if text.units as usize > self.limits.string_units {
                    return Err(Stop::Walk(JsonInputLimit::StringBytes));
                }
                pos = next;
                Node::String(text)
            } else {
                return Ok((pos, false));
            };
            self.tape.nodes.push(node);
            let after = input.skip(pos);
            if input.at(after) != Some(COMMA) {
                return Ok((after, true));
            }
            if let Some(Frame::Array { count, .. }) = self.frames.last_mut() {
                *count += 1;
                if *count > self.limits.array_elements {
                    return Err(Stop::Walk(JsonInputLimit::ArrayElements));
                }
            }
            pos = input.skip(after + 1);
        }
    }

    /// The whole-input budget of one visited value at `depth`.
    #[inline(always)]
    fn visit(&mut self, depth: usize) -> Result<(), Stop> {
        if depth > self.limits.depth {
            return Err(Stop::Walk(JsonInputLimit::Depth));
        }
        self.nodes += 1;
        if D::TIMED && self.nodes.is_multiple_of(DEADLINE_STRIDE) && self.deadline.overdue() {
            return Err(Stop::Walk(JsonInputLimit::Deadline));
        }
        if self.nodes > self.limits.nodes {
            return Err(Stop::Walk(JsonInputLimit::Nodes));
        }
        Ok(())
    }
    /// The schema's own count of visited values (missing optional fields
    /// included).
    #[inline(always)]
    fn counted(&self) -> Result<(), Stop> {
        if self.nodes.saturating_add(self.missing) > self.limits.nodes {
            return Err(Stop::Counted(JsonInputLimit::Nodes));
        }
        Ok(())
    }

    /// Check and record the value at `pos` of schema type `ty`: what
    /// follows and the position after the token; a container is opened and
    /// its first child left to the caller.
    #[inline]
    fn value(&mut self, ty: u32, depth: usize, pos: usize) -> Result<(Next, usize), Stop> {
        self.visit(depth)?;
        let input = self.input;
        let unit = input.at(pos).ok_or(Stop::Syntax(pos))?;
        let schema = self.schema;
        let mut ty = ty;
        if let JsonType::PrimitiveUnion { members } | JsonType::ReferenceUnion { members } =
            &schema.types[ty as usize]
        {
            let kind = match unit {
                0x6e => ValueKind::Null,
                0x74 | 0x66 => ValueKind::Boolean,
                QUOTE => ValueKind::String,
                OPEN_OBJECT => ValueKind::Record,
                OPEN_ARRAY => ValueKind::Array,
                _ => ValueKind::Float64,
            };
            let Some(member) = members.iter().find(|member| {
                let member = schema.types[**member as usize].kind();
                member == kind || kind == ValueKind::Record && member == ValueKind::Dictionary
            }) else {
                return mismatch();
            };
            let member_kind = schema.types[*member as usize].kind();
            self.tape.nodes.push(Node::Union {
                ty,
                kind: member_kind,
            });
            ty = *member;
        }
        self.counted()?;
        let expected = &schema.types[ty as usize];
        let (node, after) = match unit {
            OPEN_OBJECT => {
                let frame = match expected {
                    JsonType::Record { .. } => {
                        self.tape.nodes.push(Node::Record { ty, count: 0 });
                        Frame::Record {
                            node: self.tape.nodes.len() - 1,
                            ty,
                            present: Slots::EMPTY,
                            depth,
                        }
                    }
                    JsonType::Dictionary { value, .. } => {
                        self.tape.nodes.push(Node::Dictionary {
                            ty,
                            count: 0,
                            indexed: false,
                        });
                        if self.open_keys == self.keys.len() {
                            self.keys.push(Keys::default());
                        }
                        let keys = &mut self.keys[self.open_keys];
                        keys.entries.clear();
                        keys.index.clear();
                        keys.indexed = false;
                        self.open_keys += 1;
                        Frame::Dictionary {
                            node: self.tape.nodes.len() - 1,
                            value: *value,
                            depth,
                        }
                    }
                    _ => return mismatch(),
                };
                self.frames.push(frame);
                let pos = input.skip(pos + 1);
                if input.at(pos) == Some(CLOSE_OBJECT) {
                    self.close(pos + 1)?;
                    return Ok((Next::Done, pos + 1));
                }
                let (child, level, after) = self.member(pos)?;
                return Ok((Next::Child(child, level), after));
            }
            OPEN_ARRAY => {
                let JsonType::Array { element } = expected else {
                    return mismatch();
                };
                self.tape.nodes.push(Node::Array { ty, count: 0 });
                let pos = input.skip(pos + 1);
                if input.at(pos) == Some(CLOSE_ARRAY) {
                    return Ok((Next::Done, pos + 1));
                }
                self.frames.push(Frame::Array {
                    node: self.tape.nodes.len() - 1,
                    element: *element,
                    count: 0,
                    depth,
                });
                return Ok((Next::Child(*element, depth + 1), pos));
            }
            QUOTE => {
                if !matches!(expected, JsonType::String) {
                    return mismatch();
                }
                let (text, next, _) = input.string(pos).map_err(Stop::Syntax)?;
                if text.units as usize > self.limits.string_units {
                    return Err(Stop::Walk(JsonInputLimit::StringBytes));
                }
                (Node::String(text), next)
            }
            0x74 | 0x66 => {
                if !matches!(expected, JsonType::Boolean) {
                    return mismatch();
                }
                let value = unit == 0x74;
                let word: &[u8] = if value { b"true" } else { b"false" };
                let next = input.literal(pos, word).map_err(Stop::Syntax)?;
                (Node::Boolean(value), next)
            }
            0x6e => {
                if !matches!(expected, JsonType::Null) {
                    return mismatch();
                }
                let next = input.literal(pos, b"null").map_err(Stop::Syntax)?;
                (Node::Null, next)
            }
            _ => {
                let (end, integer) = input.number(pos).map_err(Stop::Syntax)?;
                let node = match expected {
                    JsonType::I32 if integer => self.integer(pos, end).ok_or(Stop::Mismatch)?,
                    JsonType::Float64 => Node::Float64(self.float(pos, end, integer)?),
                    _ => return mismatch(),
                };
                (node, end)
            }
        };
        self.tape.nodes.push(node);
        Ok((Next::Done, after))
    }
    /// The key at `pos` of the next member of the innermost object and its
    /// `:`: the type and depth of its value and the position after the `:`.
    fn member(&mut self, pos: usize) -> Result<(u32, usize, usize), Stop> {
        let input = self.input;
        if input.at(pos) != Some(QUOTE) {
            return Err(Stop::Syntax(pos));
        }
        let (text, next, surrogate) = input.string(pos).map_err(Stop::Syntax)?;
        if text.units as usize > self.limits.string_units {
            return Err(Stop::Walk(JsonInputLimit::StringBytes));
        }
        let schema = self.schema;
        let (ty, depth) = match self.frames.last_mut() {
            Some(Frame::Record {
                ty, present, depth, ..
            }) => {
                // A record key is a well-formed string.
                if surrogate {
                    self.decoded.clear();
                    input.decode(text, &mut self.decoded);
                    if char::decode_utf16(self.decoded.iter().copied()).any(|c| c.is_err()) {
                        return mismatch();
                    }
                }
                let JsonType::Record { fields, .. } = &schema.types[*ty as usize] else {
                    return mismatch();
                };
                let slot = field(input, text, fields, &mut self.decoded).ok_or(Stop::Mismatch)?;
                if present.contains(slot) {
                    return mismatch();
                }
                present.insert(slot);
                self.tape.nodes.push(Node::Field { slot: slot as u32 });
                (fields[slot].ty, *depth + 1)
            }
            Some(Frame::Dictionary { value, depth, .. }) => {
                let (value, depth) = (*value, *depth + 1);
                let Some(keys) = self
                    .open_keys
                    .checked_sub(1)
                    .and_then(|open| self.keys.get_mut(open))
                else {
                    return Err(Stop::Syntax(pos));
                };
                if keys.entries.len() == self.limits.dictionary_entries {
                    return Err(Stop::Counted(JsonInputLimit::DictionaryEntries));
                }
                if !keys.insert(input, text, &mut self.decoded) {
                    return mismatch();
                }
                self.tape.nodes.push(Node::Key(text));
                (value, depth)
            }
            _ => return Err(Stop::Syntax(pos)),
        };
        let pos = input.skip(next);
        if input.at(pos) != Some(COLON) {
            return Err(Stop::Syntax(pos));
        }
        Ok((ty, depth, pos + 1))
    }
    /// Close the innermost object (`pos` is after its `}`): its missing
    /// fields in slot order (an optional one counts as a visited value, a
    /// required one fails).
    fn close(&mut self, pos: usize) -> Result<(), Stop> {
        match self.frames.pop() {
            Some(Frame::Record {
                node, ty, present, ..
            }) => {
                let JsonType::Record { fields, .. } = &self.schema.types[ty as usize] else {
                    return mismatch();
                };
                if present != Slots::all(fields.len()) {
                    for (slot, field) in fields.iter().enumerate() {
                        if present.contains(slot) {
                            continue;
                        }
                        if !field.optional {
                            return mismatch();
                        }
                        self.missing += 1;
                        self.counted()?;
                    }
                }
                if let Some(Node::Record { count, .. }) = self.tape.nodes.get_mut(node) {
                    *count = present.count();
                }
                Ok(())
            }
            Some(Frame::Dictionary { node, .. }) => {
                self.open_keys = self.open_keys.saturating_sub(1);
                let keys = &self.keys[self.open_keys];
                if let Some(Node::Dictionary { count, indexed, .. }) = self.tape.nodes.get_mut(node)
                {
                    *count = keys.entries.len() as u32;
                    *indexed = keys.indexed;
                }
                Ok(())
            }
            _ => Err(Stop::Syntax(pos)),
        }
    }
    /// An integer literal `start..end` as an i32: `None` outside the range
    /// and for `-0`, which `serde_json` reads as a binary64.
    fn integer(&self, start: usize, end: usize) -> Option<Node> {
        let negative = self.input.at(start) == Some(MINUS);
        let digits = start + usize::from(negative);
        if end - digits > EXACT_DIGITS {
            return None;
        }
        let mut value = 0i64;
        for at in digits..end {
            value = value * 10 + i64::from(self.input.at(at)? - ZERO);
        }
        if negative {
            if value == 0 {
                return None;
            }
            value = -value;
        }
        i32::try_from(value).ok().map(Node::I32)
    }
    /// A number literal as correctly rounded binary64 bits; an infinite
    /// value is a mismatch. Up to [`EXACT_DIGITS`] significant digits and a
    /// decimal exponent within [`EXACT_POWERS`] take the exact path (one
    /// correctly rounded multiplication or division by an exact power of
    /// ten); every other literal is parsed by `core`.
    fn float(&mut self, start: usize, end: usize, integer: bool) -> Result<u64, Stop> {
        let input = self.input;
        let negative = input.at(start) == Some(MINUS);
        let mut at = start + usize::from(negative);
        let mut mantissa = 0u64;
        let mut digits = 0usize;
        let mut scale = 0i64;
        let mut exact = true;
        while let Some(unit) = input.at(at).filter(|unit| is_digit(*unit)) {
            if digits < EXACT_DIGITS {
                mantissa = mantissa * 10 + u64::from(unit - ZERO);
                if mantissa != 0 {
                    digits += 1;
                }
            } else {
                exact = false;
            }
            at += 1;
        }
        if !integer {
            if input.at(at) == Some(DOT) {
                at += 1;
                while let Some(unit) = input.at(at).filter(|unit| is_digit(*unit)) {
                    if digits < EXACT_DIGITS {
                        mantissa = mantissa * 10 + u64::from(unit - ZERO);
                        if mantissa != 0 {
                            digits += 1;
                        }
                        scale -= 1;
                    } else {
                        exact = false;
                    }
                    at += 1;
                }
            }
            if matches!(input.at(at), Some(0x45 | 0x65)) {
                at += 1;
                let sign = match input.at(at) {
                    Some(MINUS) => {
                        at += 1;
                        -1
                    }
                    Some(PLUS) => {
                        at += 1;
                        1
                    }
                    _ => 1,
                };
                let mut exponent = 0i64;
                while let Some(unit) = input.at(at).filter(|unit| is_digit(*unit)) {
                    exponent = (exponent * 10 + i64::from(unit - ZERO)).min(EXPONENT_CAP);
                    at += 1;
                }
                scale += sign * exponent;
            }
        }
        if exact && (-EXACT_POWERS..=EXACT_POWERS).contains(&scale) {
            let value = mantissa as f64;
            let power = POWERS[scale.unsigned_abs() as usize];
            let value = if scale < 0 {
                value / power
            } else {
                value * power
            };
            if value.is_finite() {
                return Ok(if negative { -value } else { value }.to_bits());
            }
        }
        self.digits.clear();
        for at in start..end {
            // Number literals are ASCII.
            self.digits.push(input.at(at).unwrap_or(ZERO) as u8);
        }
        let value = core::str::from_utf8(&self.digits)
            .ok()
            .and_then(|text| text.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .ok_or(Stop::Mismatch)?;
        Ok(value.to_bits())
    }
}

/// Decimal exponents whose power of ten binary64 holds exactly.
const EXACT_POWERS: i64 = 22;
/// The exact powers of ten `10^0` to `10^22`.
static POWERS: [f64; EXACT_POWERS as usize + 1] = {
    let mut powers = [1.0; EXACT_POWERS as usize + 1];
    let mut index = 1;
    while index < powers.len() {
        powers[index] = powers[index - 1] * 10.0;
        index += 1;
    }
    powers
};
/// Exponent digits beyond this cap cannot change an exact-path decision.
const EXPONENT_CAP: i64 = 1 << 32;

/// The slot of the record field named by the well-formed key `text`, by
/// binary search over the schema's name-sorted fields (UTF-8 byte order,
/// which is code point order).
fn field(
    input: Input<'_>,
    text: Text,
    fields: &[JsonField],
    scratch: &mut Vec<u16>,
) -> Option<usize> {
    let units: &[u16] = if text.escaped {
        scratch.clear();
        input.decode(text, scratch);
        scratch
    } else {
        &[]
    };
    let unit = |index: usize| -> u16 {
        if text.escaped {
            units[index]
        } else {
            input.at(text.start as usize + index).unwrap_or(0)
        }
    };
    let (mut low, mut high) = (0, fields.len());
    while low < high {
        let middle = (low + high) / 2;
        match compare(&fields[middle].name, text.units as usize, unit) {
            core::cmp::Ordering::Equal => return Some(middle),
            core::cmp::Ordering::Less => low = middle + 1,
            core::cmp::Ordering::Greater => high = middle,
        }
    }
    None
}
/// Compare a field name with `len` well-formed UTF-16 key units by code
/// point: unit by byte while both are ASCII, by character after that.
#[inline]
fn compare(name: &str, len: usize, unit: impl Fn(usize) -> u16) -> core::cmp::Ordering {
    let bytes = name.as_bytes();
    let mut index = 0;
    while index < len && index < bytes.len() {
        let (key, byte) = (unit(index), bytes[index]);
        if key >= 0x80 || byte >= 0x80 {
            return compare_chars(&name[index..], index, len, unit);
        }
        if key != u16::from(byte) {
            return u16::from(byte).cmp(&key);
        }
        index += 1;
    }
    bytes.len().cmp(&len)
}
/// [`compare`] from key unit `index` on, where `name` starts.
fn compare_chars(
    name: &str,
    mut index: usize,
    len: usize,
    unit: impl Fn(usize) -> u16,
) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let mut chars = name.chars();
    loop {
        let key = if index < len {
            let high = unit(index);
            index += 1;
            let code = if (SURROGATE_START..LOW_SURROGATE_START).contains(&high) && index < len {
                let low = unit(index);
                index += 1;
                0x10000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
            } else {
                u32::from(high)
            };
            Some(code)
        } else {
            None
        };
        match (chars.next().map(u32::from), key) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) if a != b => return a.cmp(&b),
            _ => {}
        }
    }
}

impl Keys {
    /// Record a key; `false` when it is a duplicate.
    fn insert(&mut self, input: Input<'_>, text: Text, scratch: &mut Vec<u16>) -> bool {
        scratch.clear();
        input.decode(text, scratch);
        let hash = dictionary_hash(scratch);
        if !self.indexed && array_index(scratch).is_some() {
            self.indexed = true;
        }
        let same = |other: &Text| {
            if other.units != text.units {
                return false;
            }
            let mut decoded = Vec::with_capacity(other.units as usize);
            input.decode(*other, &mut decoded);
            decoded == *scratch
        };
        if self.entries.len() < LINEAR_KEYS {
            if self
                .entries
                .iter()
                .any(|(other, key)| *other == hash && same(key))
            {
                return false;
            }
            self.entries.push((hash, text));
            if self.entries.len() == LINEAR_KEYS {
                self.rebuild();
            }
            return true;
        }
        let mask = self.index.len() - 1;
        let mut slot = hash as usize & mask;
        while let Some(entry) = self.index[slot].checked_sub(1) {
            let (other, key) = &self.entries[entry as usize];
            if *other == hash && same(key) {
                return false;
            }
            slot = (slot + 1) & mask;
        }
        self.index[slot] = self.entries.len() as u32 + 1;
        self.entries.push((hash, text));
        if self.entries.len() * 2 > self.index.len() {
            self.rebuild();
        }
        true
    }
    /// Rebuild the hash index at twice the entries, at least.
    fn rebuild(&mut self) {
        let capacity = (self.entries.len() * 4).next_power_of_two();
        self.index.clear();
        self.index.resize(capacity, 0);
        let mask = capacity - 1;
        for (position, (hash, _)) in self.entries.iter().enumerate() {
            let mut slot = *hash as usize & mask;
            while self.index[slot] != 0 {
                slot = (slot + 1) & mask;
            }
            self.index[slot] = position as u32 + 1;
        }
    }
}

/// The schema-free second pass: the grammar of the whole input and, when
/// `walk`, its whole-input budgets in document order. The first exceeded
/// budget is reported only once the grammar of the rest holds.
struct Checker<'a, 'b, D> {
    input: Input<'a>,
    limits: JsonLimits,
    walk: bool,
    nodes: usize,
    deadline: &'b mut D,
    exceeded: Option<JsonInputLimit>,
}
impl<'a, 'b, D: Deadline> Checker<'a, 'b, D> {
    fn new(input: Input<'a>, limits: JsonLimits, walk: bool, deadline: &'b mut D) -> Self {
        Self {
            input,
            limits,
            walk,
            nodes: 0,
            deadline,
            exceeded: None,
        }
    }
    fn run(mut self) -> Result<(), Stop> {
        self.grammar()?;
        match self.exceeded {
            Some(limit) => Err(Stop::Walk(limit)),
            None => Ok(()),
        }
    }
    /// Record the first exceeded budget; only the grammar is checked after it.
    fn exceed(&mut self, limit: JsonInputLimit) {
        self.exceeded = Some(limit);
        self.walk = false;
    }
    fn grammar(&mut self) -> Result<(), Stop> {
        // Open containers: `true` for an object, with its element count.
        let mut open: Vec<(bool, usize)> = Vec::new();
        let input = self.input;
        let mut pos = input.skip(0);
        loop {
            // A value at `pos`, at depth `open.len()`.
            if self.walk {
                self.visit(open.len());
            }
            match input.at(pos).ok_or(Stop::Syntax(pos))? {
                OPEN_OBJECT | OPEN_ARRAY => {
                    let object = input.at(pos) == Some(OPEN_OBJECT);
                    pos = input.skip(pos + 1);
                    open.push((object, 0));
                    let close = if object { CLOSE_OBJECT } else { CLOSE_ARRAY };
                    if input.at(pos) == Some(close) {
                        pos += 1;
                        open.pop();
                    } else {
                        if object {
                            pos = self.key(pos)?;
                        }
                        continue;
                    }
                }
                QUOTE => pos = self.text(pos)?,
                0x74 => pos = input.literal(pos, b"true").map_err(Stop::Syntax)?,
                0x66 => pos = input.literal(pos, b"false").map_err(Stop::Syntax)?,
                0x6e => pos = input.literal(pos, b"null").map_err(Stop::Syntax)?,
                _ => pos = input.number(pos).map_err(Stop::Syntax)?.0,
            }
            loop {
                pos = input.skip(pos);
                let Some((object, count)) = open.last_mut() else {
                    return if pos == input.len() {
                        Ok(())
                    } else {
                        Err(Stop::Syntax(pos))
                    };
                };
                *count += 1;
                let object = *object;
                if self.walk && !object && *count > self.limits.array_elements {
                    self.exceed(JsonInputLimit::ArrayElements);
                }
                match input.at(pos) {
                    Some(COMMA) => {
                        pos = input.skip(pos + 1);
                        if object {
                            pos = self.key(pos)?;
                        }
                        break;
                    }
                    Some(CLOSE_OBJECT) if object => {
                        pos += 1;
                        open.pop();
                    }
                    Some(CLOSE_ARRAY) if !object => {
                        pos += 1;
                        open.pop();
                    }
                    _ => return Err(Stop::Syntax(pos)),
                }
            }
        }
    }
    fn visit(&mut self, depth: usize) {
        if depth > self.limits.depth {
            return self.exceed(JsonInputLimit::Depth);
        }
        self.nodes += 1;
        if D::TIMED && self.nodes.is_multiple_of(DEADLINE_STRIDE) && self.deadline.overdue() {
            return self.exceed(JsonInputLimit::Deadline);
        }
        if self.nodes > self.limits.nodes {
            self.exceed(JsonInputLimit::Nodes);
        }
    }
    /// A string at `pos` with its budget: the position after it.
    fn text(&mut self, pos: usize) -> Result<usize, Stop> {
        let (text, next, _) = self.input.string(pos).map_err(Stop::Syntax)?;
        if self.walk && text.units as usize > self.limits.string_units {
            self.exceed(JsonInputLimit::StringBytes);
        }
        Ok(next)
    }
    /// A member key and its `:` at `pos`: the position of its value.
    fn key(&mut self, pos: usize) -> Result<usize, Stop> {
        if self.input.at(pos) != Some(QUOTE) {
            return Err(Stop::Syntax(pos));
        }
        let pos = self.input.skip(self.text(pos)?);
        if self.input.at(pos) != Some(COLON) {
            return Err(Stop::Syntax(pos));
        }
        Ok(self.input.skip(pos + 1))
    }
}

impl Tape {
    /// The checked tree of a successful [`scan`] of `text` (the host's
    /// builders take this tree), built iteratively.
    #[must_use]
    pub fn value(&self, text: &[u8], schema: &JsonSchema) -> JsonValue {
        let input = Input {
            units: text.as_chunks::<2>().0,
        };
        /// A container being filled: its node, children and remaining count.
        struct Open {
            node: Node,
            remaining: u32,
            values: Vec<JsonValue>,
            /// Record slots of the values, or dictionary keys.
            slots: Vec<u32>,
            keys: Vec<Vec<u16>>,
        }
        let mut stack: Vec<Open> = Vec::new();
        // Pending union wrappers: (stack depth, union type) innermost last.
        let mut unions: Vec<(usize, u32)> = Vec::new();
        let mut nodes = self.nodes.iter();
        let mut finished: Option<JsonValue> = None;
        while finished.is_none() {
            let Some(node) = nodes.next() else { break };
            let mut done = match *node {
                Node::Null => JsonValue::Null,
                Node::Boolean(value) => JsonValue::Boolean(value),
                Node::I32(value) => JsonValue::I32(value),
                Node::Float64(bits) => JsonValue::Float64(bits),
                Node::String(text) => {
                    let mut units = Vec::with_capacity(text.units as usize);
                    input.decode(text, &mut units);
                    JsonValue::String16(units)
                }
                Node::Union { ty, .. } => {
                    unions.push((stack.len(), ty));
                    continue;
                }
                Node::Field { slot } => {
                    if let Some(open) = stack.last_mut() {
                        open.slots.push(slot);
                    }
                    continue;
                }
                Node::Key(text) => {
                    let mut units = Vec::with_capacity(text.units as usize);
                    input.decode(text, &mut units);
                    if let Some(open) = stack.last_mut() {
                        open.keys.push(units);
                    }
                    continue;
                }
                Node::Record { count, .. }
                | Node::Array { count, .. }
                | Node::Dictionary { count, .. } => {
                    stack.push(Open {
                        node: *node,
                        remaining: count,
                        values: Vec::with_capacity(count as usize),
                        slots: Vec::new(),
                        keys: Vec::new(),
                    });
                    if count > 0 {
                        continue;
                    }
                    let open = stack.pop().unwrap_or_else(|| unreachable!());
                    close(open.node, open.values, open.slots, open.keys, schema)
                }
            };
            // Wrap and deliver `done` upwards, closing finished containers.
            loop {
                while let Some(&(level, ty)) = unions.last() {
                    if level != stack.len() {
                        break;
                    }
                    unions.pop();
                    done = JsonValue::Union {
                        ty,
                        value: Box::new(done),
                    };
                }
                let Some(open) = stack.last_mut() else {
                    finished = Some(done);
                    break;
                };
                open.values.push(done);
                open.remaining -= 1;
                if open.remaining > 0 {
                    break;
                }
                let open = stack.pop().unwrap_or_else(|| unreachable!());
                done = close(open.node, open.values, open.slots, open.keys, schema);
            }
        }
        finished.unwrap_or(JsonValue::Null)
    }
}
/// A completed container's tree: record fields in slot order (missing
/// optional ones `undefined`), dictionary entries in own-key order.
fn close(
    node: Node,
    values: Vec<JsonValue>,
    slots: Vec<u32>,
    keys: Vec<Vec<u16>>,
    schema: &JsonSchema,
) -> JsonValue {
    match node {
        Node::Record { ty, .. } => {
            let JsonType::Record { fields, .. } = &schema.types[ty as usize] else {
                return JsonValue::Null;
            };
            let mut ordered: Vec<Option<JsonValue>> = (0..fields.len()).map(|_| None).collect();
            for (slot, value) in slots.into_iter().zip(values) {
                ordered[slot as usize] = Some(value);
            }
            JsonValue::Record {
                ty,
                fields: ordered
                    .into_iter()
                    .enumerate()
                    .map(|(slot, value)| {
                        value.unwrap_or_else(|| {
                            let ty = fields[slot].ty;
                            match schema.types[ty as usize] {
                                JsonType::Undefined => JsonValue::Undefined,
                                _ => JsonValue::Union {
                                    ty,
                                    value: Box::new(JsonValue::Undefined),
                                },
                            }
                        })
                    })
                    .collect(),
            }
        }
        Node::Array { ty, .. } => JsonValue::Array { ty, values },
        Node::Dictionary { ty, .. } => {
            let mut entries: Vec<(Vec<u16>, JsonValue)> = keys.into_iter().zip(values).collect();
            entries.sort_by_key(|(key, _)| match array_index(key) {
                Some(index) => (false, index),
                None => (true, 0),
            });
            JsonValue::Dictionary { ty, entries }
        }
        _ => JsonValue::Null,
    }
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
