//! Value18 serialization: `JSON.stringify` (v38 `json_stringify_owned`) and
//! the diagnostic text of a logged value (`value_format_v12`, which v38 logs
//! through `log_string16`), one traversal for both, generic over [`Memory`]
//! and an output [`Sink`] of UTF-16 code units.
//!
//! The behavior is the host serializer's for value layout 18 (the runtime's
//! `json_stringify_owned` and `value_log_v12`): the same output, the same
//! order of checks over the graph and the same typed failures, with private
//! state hidden from JSON and shown in diagnostics. The traversal is
//! iterative, so any nesting within the profile's JSON depth limit needs no
//! native stack, and it counts the UTF-8 bytes of its output against the
//! host's output limits: the memory budget for JSON, the result buffer for
//! the diagnostic text of an aggregate.
//!
//! Every value is read with the shared `no_std` readers of the value core
//! under v38 `bounds`; a noncanonical layout is [`EncodeError::Layout`], a
//! fatal error (a trap in linked code). A cycle, the depth limit, the output
//! limit and a callable `toJSON` field end the execution with the host's
//! message ([`EncodeError::linked_code`]); a BigInt inside `JSON.stringify`
//! is the recoverable `TypeError` the host throws, reported after the whole
//! graph was validated.
use crate::{Memory, number::Shortest};
use alloc::{collections::BTreeSet, vec::Vec};
use varta_bigint::Budget;
use varta_common::{
    abi,
    bigint::BigIntView,
    function::check_closure_envelope,
    value::{
        ARRAY_META_BYTES, ARRAY_SLOT_BYTES, Dictionary, ErrorObject,
        HASHED_DICTIONARY_VALUE_VERSION as VERSION, ModuleBounds, PRIVATE_KEY_PREFIX,
        PROMISE_REASON_FIELD, PROMISE_STATE_FIELD, PROMISE_VALUE_FIELD, PrimitiveBox, PromiseState,
        RecordShape, ReferenceBox, ValueKind as Kind, Vector, function_field_layout,
        read_record_with, record_descriptor, string_range,
    },
};

/// How a logged function value reads: as Node shows an anonymous function,
/// since closures carry no source name.
pub const FUNCTION_DISPLAY: &str = "[Function (anonymous)]";
/// The field a record would need to call for `JSON.stringify` (not
/// supported: guest call dispatch from the serializer).
const TO_JSON: &[u8] = b"toJSON";
/// UTF-16 code units buffered before a plain run is passed to the sink.
const RUN_UNITS: usize = 64;
/// The second halves of surrogate pairs.
const LOW_SURROGATES: core::ops::Range<u16> = 0xDC00..0xE000;
const HEX: &[u8; 16] = b"0123456789abcdef";

/// Where serialized UTF-16 code units go: a counter in a measuring pass, the
/// result string in a writing pass.
pub trait Sink {
    /// Append code units.
    fn units(&mut self, units: &[u16]);
    /// Append ASCII bytes, one code unit each.
    fn ascii(&mut self, bytes: &[u8]) {
        let mut buffer = [0u16; RUN_UNITS];
        for chunk in bytes.chunks(RUN_UNITS) {
            for (unit, byte) in buffer.iter_mut().zip(chunk) {
                *unit = u16::from(*byte);
            }
            self.units(&buffer[..chunk.len()]);
        }
    }
}
/// A sink that only counts code units.
#[derive(Debug, Default, Clone, Copy)]
pub struct Count(pub usize);
impl Sink for Count {
    fn units(&mut self, units: &[u16]) {
        self.0 += units.len();
    }
    fn ascii(&mut self, bytes: &[u8]) {
        self.0 += bytes.len();
    }
}
/// A sink collecting code units (host-side tests and small results).
impl Sink for Vec<u16> {
    fn units(&mut self, units: &[u16]) {
        self.extend_from_slice(units);
    }
}

/// What the traversal produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `JSON.stringify`, private state hidden (`json_stringify_owned`).
    Json,
    /// The notation of a logged aggregate (`value_log_v12`): BigInts with an
    /// `n`, functions and promises shown, private state shown.
    Diagnostic,
}
impl Mode {
    /// The host's limit on the UTF-8 bytes of the output: the memory budget
    /// for the owned JSON result, the result buffer for a diagnostic.
    fn output_limit(self) -> usize {
        match self {
            Self::Json => abi::MAX_MEMORY_BYTES,
            Self::Diagnostic => abi::MAX_RESULT_BYTES,
        }
    }
}

/// Why serialization stopped. [`Self::BigInt`] is `JSON.stringify`'s
/// recoverable `TypeError`; every other error is fatal, with the host
/// serializer's message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    /// A noncanonical value layout (a trap in linked code).
    #[error("JSON serialization: noncanonical value layout")]
    Layout,
    #[error("JSON cannot serialize BigInt")]
    BigInt,
    #[error("JSON serialization: cyclic object")]
    Cyclic,
    /// Nesting beyond the depth limit, in diagnostic text.
    #[error("JSON serialization: depth or node limit")]
    Depth,
    #[error("JSON serialization: depth or node limit")]
    Nodes,
    /// Nesting beyond the depth limit, in `JSON.stringify`: the host's JSON
    /// depth ceiling.
    #[error("JSON serialization: nesting exceeds the host depth ceiling of {0}")]
    DepthCeiling(usize),
    #[error("JSON serialization: output byte limit")]
    Output,
    #[error("JSON serialization: toJSON hooks require guest call dispatch")]
    ToJson,
    /// A diagnostic BigInt's decimal text failed (its work budget).
    #[error(transparent)]
    BigIntText(#[from] varta_bigint::Error),
}
/// V38 linked serialization: the code a fatal failure stores in the module's
/// `limits::RESOURCE_FAILURE` global before it traps (above the
/// `limits::Resource` codes and the linked JSON input limits), so the runtime
/// reports exactly the host serializer's error.
pub const LINKED_FAILURE_BASE: i32 = 0x200;
impl EncodeError {
    /// The failures a linked serializer reports by code, in code order.
    const LINKED: [Self; 6] = [
        Self::Cyclic,
        Self::Depth,
        Self::Nodes,
        Self::DepthCeiling(0),
        Self::Output,
        Self::ToJson,
    ];
    /// This failure's linked code and request word (the depth ceiling), or
    /// `None` for a failure linked code does not report by code.
    #[must_use]
    pub fn linked_code(self) -> Option<(i32, u32)> {
        let index = Self::LINKED
            .iter()
            .position(|error| core::mem::discriminant(error) == core::mem::discriminant(&self))?;
        let requested = match self {
            Self::DepthCeiling(depth) => u32::try_from(depth).unwrap_or(u32::MAX),
            _ => 0,
        };
        Some((LINKED_FAILURE_BASE + index as i32, requested))
    }
    /// The failure a linked code and its request word name.
    #[must_use]
    pub fn from_linked_code(code: i32, requested: u32) -> Option<Self> {
        let index = usize::try_from(code.checked_sub(LINKED_FAILURE_BASE)?).ok()?;
        Some(match Self::LINKED.get(index).copied()? {
            Self::DepthCeiling(_) => Self::DepthCeiling(requested as usize),
            other => other,
        })
    }
}
fn layout<T>(_: T) -> EncodeError {
    EncodeError::Layout
}

/// `JSON.stringify` of `(kind, payload)` into `sink`: `Ok(None)` when the
/// result is `undefined` (nothing written), otherwise the values visited.
pub fn stringify<M: Memory + ?Sized, S: Sink + ?Sized>(
    memory: &M,
    kind: u32,
    payload: u64,
    bounds: ModuleBounds,
    sink: &mut S,
) -> Result<Option<usize>, EncodeError> {
    let mut encoder = Encoder::new(memory, bounds, Mode::Json, sink, None);
    let (kind, payload) = encoder.resolve(kind, payload)?;
    if kind == Kind::Undefined as u32 {
        if payload != 0 {
            return Err(EncodeError::Layout);
        }
        encoder.visit(0)?;
        return Ok(None);
    }
    encoder.run(kind, payload).map_err(|error| match error {
        EncodeError::Depth => EncodeError::DepthCeiling(bounds.limits.data.json_depth),
        other => other,
    })?;
    if encoder.bigint {
        return Err(EncodeError::BigInt);
    }
    Ok(Some(encoder.nodes))
}

/// The diagnostic text `value_log_v12` logs for the value18 cell `cell`
/// (before the `varta log: ` prefix): scalars, Error objects and functions
/// as the host shows them, strings as their own code units, aggregates in
/// the diagnostic notation within the result-buffer output limit.
pub fn format<M: Memory + ?Sized, S: Sink + ?Sized>(
    memory: &M,
    cell: u64,
    bounds: ModuleBounds,
    budget: &mut Budget,
    sink: &mut S,
) -> Result<(), EncodeError> {
    // An aggregate is validated once, by the encoder; every other cell as
    // `ReferenceBox::read_in` validates it.
    let header = ReferenceBox::read_envelope(memory, cell, VERSION).map_err(layout)?;
    let value = match header.kind {
        Kind::Record | Kind::Array | Kind::Dictionary | Kind::Promise => header,
        _ => ReferenceBox::read_in(memory, cell, VERSION, bounds).map_err(layout)?,
    };
    match value.kind {
        Kind::BigInt => {
            let text = crate::bigint::integer(memory, value.payload, budget)?
                .to_string_radix(10, budget)?;
            budget.consume_work(text.len() as u64)?;
            sink.ascii(text.as_bytes());
        }
        Kind::Null => sink.ascii(b"null"),
        Kind::Undefined => sink.ascii(b"undefined"),
        Kind::Boolean => sink.ascii(if value.payload == 0 { b"0" } else { b"1" }),
        Kind::Float64 => {
            let number = f64::from_bits(value.payload);
            if number.is_finite() {
                sink.ascii(Shortest::new().format_finite(number));
            } else {
                sink.ascii(ryu_js::Buffer::new().format(number).as_bytes());
            }
        }
        Kind::String => raw(memory, value.payload, sink)?,
        Kind::Error => {
            let object =
                ErrorObject::read_in(memory, value.payload, VERSION, bounds).map_err(layout)?;
            sink.ascii(object.class.name().as_bytes());
            if string_range(object.message).1 != 0 {
                sink.ascii(b": ");
                raw(memory, object.message, sink)?;
            }
        }
        Kind::Function => sink.ascii(FUNCTION_DISPLAY.as_bytes()),
        Kind::Record | Kind::Array | Kind::Dictionary | Kind::Promise => {
            let mut encoder = Encoder::new(memory, bounds, Mode::Diagnostic, sink, Some(budget));
            let (kind, payload) = encoder.resolve(value.kind as u32, value.payload)?;
            if kind == Kind::Undefined as u32 {
                return Err(EncodeError::Layout);
            }
            encoder.run(kind, payload)?;
        }
        _ => return Err(EncodeError::Layout),
    }
    Ok(())
}
/// The code units of the string `handle`, unescaped.
fn raw<M: Memory + ?Sized, S: Sink + ?Sized>(
    memory: &M,
    handle: u64,
    sink: &mut S,
) -> Result<(), EncodeError> {
    let (ptr, len) = string_range(handle);
    let bytes = memory.borrow(ptr, len).ok_or(EncodeError::Layout)?;
    let mut buffer = [0u16; RUN_UNITS];
    for chunk in bytes.as_chunks::<2>().0.chunks(RUN_UNITS) {
        for (unit, pair) in buffer.iter_mut().zip(chunk) {
            *unit = u16::from_le_bytes(*pair);
        }
        sink.units(&buffer[..chunk.len()]);
    }
    Ok(())
}

/// A record field as serialization reads it: its UTF-8 key in static data,
/// its (then resolved) kind and payload, and its presence.
#[derive(Debug, Clone, Copy)]
struct Field {
    key: usize,
    len: usize,
    kind: u32,
    payload: u64,
    present: bool,
    optional: bool,
}
/// An aggregate whose members are being written.
enum Frame {
    Array {
        ptr: usize,
        vector: Vector,
        index: usize,
        depth: usize,
    },
    Dictionary {
        ptr: usize,
        dictionary: alloc::boxed::Box<Dictionary>,
        index: usize,
        depth: usize,
        written: bool,
    },
    /// `fields` by slot; `order` the present slots in output order.
    Record {
        ptr: usize,
        fields: Vec<Field>,
        order: Vec<usize>,
        index: usize,
        depth: usize,
        written: bool,
    },
    /// A diagnostic promise whose settled value is being written.
    Promise { ptr: usize },
}

/// Aggregates on the current path, for the cycle check: scanned while the
/// path is short, also kept in a set beyond [`Path::SCANNED`] so a check
/// costs the same at any depth.
#[derive(Default)]
struct Path {
    stack: Vec<usize>,
    set: Option<BTreeSet<usize>>,
}
impl Path {
    const SCANNED: usize = 32;
    #[inline]
    fn contains(&self, ptr: usize) -> bool {
        match &self.set {
            Some(set) => set.contains(&ptr),
            None => self.stack.contains(&ptr),
        }
    }
    /// Enter `ptr`; false when it is already on the path.
    #[inline]
    fn insert(&mut self, ptr: usize) -> bool {
        if self.contains(ptr) {
            return false;
        }
        self.stack.push(ptr);
        match &mut self.set {
            Some(set) => {
                set.insert(ptr);
            }
            None if self.stack.len() > Self::SCANNED => {
                self.set = Some(self.stack.iter().copied().collect());
            }
            None => {}
        }
        true
    }
    /// Leave `ptr`, the innermost aggregate.
    #[inline]
    fn remove(&mut self, ptr: usize) {
        if self.stack.last() == Some(&ptr) {
            self.stack.pop();
            if let Some(set) = &mut self.set {
                set.remove(&ptr);
            }
        }
    }
}

/// Output code units staged before they are passed to the sink.
const STAGED_UNITS: usize = 32;

struct Encoder<'m, 's, 'b, M: Memory + ?Sized, S: Sink + ?Sized> {
    memory: &'m M,
    bounds: ModuleBounds,
    mode: Mode,
    sink: &'s mut S,
    /// The work budget of diagnostic BigInt text.
    budget: Option<&'b mut Budget>,
    /// [`Mode::output_limit`].
    limit: usize,
    /// Output not yet passed to the sink.
    staged: [u16; STAGED_UNITS],
    filled: usize,
    /// UTF-8 bytes of the output so far, against [`Mode::output_limit`].
    utf8: usize,
    nodes: usize,
    /// A BigInt was replaced by `null` (`JSON.stringify` then throws).
    bigint: bool,
    path: Path,
    stack: Vec<Frame>,
    /// Field lists of closed records, reused (no allocation per record).
    spare_fields: Vec<Vec<Field>>,
    spare_order: Vec<Vec<usize>>,
    /// The record descriptors validated during this call (static data does
    /// not change while it runs): each record of a known shape checks only
    /// its own block, presence, order and link word.
    shapes: Vec<RecordShape>,
}
impl<'m, 's, 'b, M: Memory + ?Sized, S: Sink + ?Sized> Encoder<'m, 's, 'b, M, S> {
    fn new(
        memory: &'m M,
        bounds: ModuleBounds,
        mode: Mode,
        sink: &'s mut S,
        budget: Option<&'b mut Budget>,
    ) -> Self {
        Self {
            memory,
            bounds,
            mode,
            sink,
            budget,
            limit: mode.output_limit(),
            staged: [0; STAGED_UNITS],
            filled: 0,
            utf8: 0,
            nodes: 0,
            bigint: false,
            path: Path::default(),
            stack: Vec::new(),
            spare_fields: Vec::new(),
            spare_order: Vec::new(),
            shapes: Vec::new(),
        }
    }
    fn run(&mut self, kind: u32, payload: u64) -> Result<(), EncodeError> {
        self.value(kind, payload, 0)?;
        while !self.stack.is_empty() {
            self.step()?;
        }
        self.flush();
        Ok(())
    }
    #[inline]
    fn visit(&mut self, depth: usize) -> Result<(), EncodeError> {
        self.nodes += 1;
        let limits = self.bounds.limits.data;
        if depth > limits.json_depth {
            return Err(EncodeError::Depth);
        }
        if self.nodes > limits.json_nodes {
            return Err(EncodeError::Nodes);
        }
        Ok(())
    }
    #[inline]
    fn slot(&self, at: usize) -> Result<u64, EncodeError> {
        self.memory.u64_at(at).ok_or(EncodeError::Layout)
    }
    /// Count `utf8` output bytes against the output limit.
    #[inline]
    fn count(&mut self, utf8: usize) -> Result<(), EncodeError> {
        if utf8 > self.limit - self.utf8 {
            return Err(EncodeError::Output);
        }
        self.utf8 += utf8;
        Ok(())
    }
    /// Pass the staged output to the sink.
    fn flush(&mut self) {
        if self.filled != 0 {
            self.sink.units(&self.staged[..self.filled]);
            self.filled = 0;
        }
    }
    /// Stage one code unit (counted by the caller).
    #[inline]
    fn put(&mut self, unit: u16) {
        if self.filled == STAGED_UNITS {
            self.flush();
        }
        self.staged[self.filled] = unit;
        self.filled += 1;
    }
    /// One ASCII punctuation character.
    #[inline]
    fn punct(&mut self, byte: u8) -> Result<(), EncodeError> {
        self.count(1)?;
        self.put(u16::from(byte));
        Ok(())
    }
    fn ascii(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.count(bytes.len())?;
        if self.filled + bytes.len() > STAGED_UNITS {
            self.flush();
        }
        if bytes.len() > STAGED_UNITS {
            for byte in bytes {
                self.put(u16::from(*byte));
            }
            return Ok(());
        }
        for (unit, byte) in self.staged[self.filled..].iter_mut().zip(bytes) {
            *unit = u16::from(*byte);
        }
        self.filled += bytes.len();
        Ok(())
    }
    /// Stage `\u` and four lowercase hex digits of `unit` (six UTF-8 bytes,
    /// counted by the caller).
    fn escape(&mut self, unit: u16) {
        self.put(u16::from(b'\\'));
        self.put(u16::from(b'u'));
        for shift in [12, 8, 4, 0] {
            self.put(u16::from(HEX[usize::from((unit >> shift) & 0xF)]));
        }
    }
    /// The JSON escape of a character below U+0080 that needs one.
    fn short_escape(unit: u16) -> Option<&'static [u8]> {
        Some(match unit {
            0x22 => b"\\\"",
            0x5C => b"\\\\",
            0x08 => b"\\b",
            0x0C => b"\\f",
            0x0A => b"\\n",
            0x0D => b"\\r",
            0x09 => b"\\t",
            _ => return None,
        })
    }
    /// Stage the escape of an ASCII `unit` that needs one; its UTF-8 bytes.
    fn escape_ascii(&mut self, unit: u16) -> usize {
        match Self::short_escape(unit) {
            Some(text) => {
                for byte in text {
                    self.put(u16::from(*byte));
                }
                text.len()
            }
            None => {
                self.escape(unit);
                ESCAPE_BYTES
            }
        }
    }
    /// A JSON string of UTF-16LE `bytes`: escapes as the host writes them,
    /// lone surrogates as `\udxxx`, every other character as its own units.
    /// The output limit is checked once for the whole string (nothing else
    /// is checked within it).
    fn string16(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        let units = bytes.as_chunks::<2>().0;
        let mut utf8 = 2;
        self.put(u16::from(b'"'));
        let mut index = 0;
        while index < units.len() {
            let unit = u16::from_le_bytes(units[index]);
            index += 1;
            match unit {
                0x20..0x80 if unit != 0x22 && unit != 0x5C => {
                    self.put(unit);
                    utf8 += 1;
                }
                0..0x80 => utf8 += self.escape_ascii(unit),
                0x80..0x800 => {
                    self.put(unit);
                    utf8 += 2;
                }
                0xD800..0xDC00
                    if units.get(index).is_some_and(|next| {
                        LOW_SURROGATES.contains(&u16::from_le_bytes(*next))
                    }) =>
                {
                    self.put(unit);
                    self.put(u16::from_le_bytes(units[index]));
                    index += 1;
                    utf8 += 4;
                }
                0xD800..0xE000 => {
                    // A lone surrogate.
                    self.escape(unit);
                    utf8 += ESCAPE_BYTES;
                }
                _ => {
                    self.put(unit);
                    utf8 += 3;
                }
            }
        }
        self.put(u16::from(b'"'));
        self.count(utf8)
    }
    /// A JSON string of a UTF-8 record key from static data.
    fn key(&mut self, field: Field) -> Result<(), EncodeError> {
        let memory = self.memory;
        let bytes = memory
            .borrow(field.key, field.len)
            .ok_or(EncodeError::Layout)?;
        let mut utf8 = 2;
        self.put(u16::from(b'"'));
        if bytes.is_ascii() {
            for byte in bytes.iter() {
                let unit = u16::from(*byte);
                if unit >= 0x20 && unit != 0x22 && unit != 0x5C {
                    self.put(unit);
                    utf8 += 1;
                } else {
                    utf8 += self.escape_ascii(unit);
                }
            }
        } else {
            let text = core::str::from_utf8(&bytes).map_err(layout)?;
            let mut pair = [0u16; 2];
            for character in text.chars() {
                let code = character as u32;
                if code < 0x80 && (code < 0x20 || code == 0x22 || code == 0x5C) {
                    utf8 += self.escape_ascii(code as u16);
                    continue;
                }
                for unit in character.encode_utf16(&mut pair) {
                    self.put(*unit);
                }
                utf8 += character.len_utf8();
            }
        }
        self.put(u16::from(b'"'));
        self.count(utf8)
    }
    /// The kind and payload a slot of `kind` denotes: a union cell's member,
    /// and in JSON a function as `undefined` (after its envelope check). A
    /// reference cell is read by its header: the member it selects is
    /// validated by its own reader when it is written.
    fn resolve(&mut self, kind: u32, payload: u64) -> Result<(u32, u64), EncodeError> {
        if kind == Kind::Function as u32 {
            check_closure_envelope(self.memory, payload, &self.bounds.limits).map_err(layout)?;
            if function_field_layout(VERSION) && self.mode == Mode::Diagnostic {
                return Ok((kind, payload));
            }
            return Ok((Kind::Undefined as u32, 0));
        }
        if kind == Kind::ReferenceUnion as u32 {
            let cell =
                ReferenceBox::read_envelope(self.memory, payload, VERSION).map_err(layout)?;
            return self.resolve(cell.kind as u32, cell.payload);
        }
        if kind != Kind::PrimitiveUnion as u32 {
            return Ok((kind, payload));
        }
        let cell =
            PrimitiveBox::read_limited(self.memory, payload, self.bounds.limits.data.string_bytes)
                .map_err(layout)?;
        Ok((cell.kind as u32, cell.payload))
    }
    /// Write one value at `depth`: a scalar completely, an aggregate's
    /// opening (its members follow from [`Self::step`]).
    fn value(&mut self, kind: u32, payload: u64, depth: usize) -> Result<(), EncodeError> {
        self.visit(depth)?;
        let (kind, payload) = self.resolve(kind, payload)?;
        let kind = Kind::from_tag(kind)
            .filter(|kind| kind.supported_by(VERSION))
            .ok_or(EncodeError::Layout)?;
        match kind {
            Kind::BigInt => self.bigint(payload),
            Kind::Undefined | Kind::Null if payload == 0 => self.ascii(b"null"),
            Kind::Float64 => {
                let number = f64::from_bits(payload);
                if number.is_finite() {
                    self.ascii(Shortest::new().format_finite(number))
                } else {
                    self.ascii(b"null")
                }
            }
            Kind::I32 => {
                let value = i32::try_from(payload as i64).map_err(layout)?;
                self.integer(value)
            }
            Kind::Boolean if payload <= 1 => {
                self.ascii(if payload == 0 { b"false" } else { b"true" })
            }
            Kind::String => {
                let (ptr, len) = string_range(payload);
                if len > self.bounds.limits.data.string_bytes || !len.is_multiple_of(2) {
                    return Err(EncodeError::Layout);
                }
                let memory = self.memory;
                let bytes = memory.borrow(ptr, len).ok_or(EncodeError::Layout)?;
                self.string16(&bytes)
            }
            Kind::Error => {
                ErrorObject::read_in(self.memory, payload, VERSION, self.bounds).map_err(layout)?;
                self.visit(depth + 1)?;
                self.ascii(b"{}")
            }
            Kind::Record => self.record(payload, depth),
            Kind::Array => self.array(payload, depth),
            Kind::Dictionary => self.dictionary(payload, depth),
            Kind::Promise => self.promise(payload, depth),
            Kind::Function if self.mode == Mode::Diagnostic => {
                self.ascii(FUNCTION_DISPLAY.as_bytes())
            }
            _ => Err(EncodeError::Layout),
        }
    }
    fn integer(&mut self, value: i32) -> Result<(), EncodeError> {
        let mut digits = [0u8; 11];
        let mut at = digits.len();
        let mut rest = value.unsigned_abs();
        loop {
            at -= 1;
            digits[at] = b'0' + (rest % 10) as u8;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        if value < 0 {
            at -= 1;
            digits[at] = b'-';
        }
        self.ascii(&digits[at..])
    }
    fn bigint(&mut self, payload: u64) -> Result<(), EncodeError> {
        BigIntView::check(self.memory, payload).map_err(layout)?;
        let Some(budget) = self.budget.as_deref_mut() else {
            // `JSON.stringify` validates the rest of the graph before it
            // throws: corrupt slots, cycles and limits are not hidden.
            self.bigint = true;
            return self.ascii(b"null");
        };
        let text =
            crate::bigint::integer(self.memory, payload, budget)?.to_string_radix(10, budget)?;
        budget.consume_work(text.len() as u64)?;
        self.ascii(text.as_bytes())?;
        self.ascii(b"n")
    }
    fn enter(&mut self, ptr: usize) -> Result<(), EncodeError> {
        if !self.path.insert(ptr) {
            return Err(EncodeError::Cyclic);
        }
        Ok(())
    }
    fn array(&mut self, payload: u64, depth: usize) -> Result<(), EncodeError> {
        let vector = Vector::read_in(self.memory, payload, VERSION, self.bounds).map_err(layout)?;
        self.enter(vector.ptr)?;
        self.punct(b'[')?;
        self.stack.push(Frame::Array {
            ptr: vector.ptr,
            vector,
            index: 0,
            depth,
        });
        Ok(())
    }
    /// A dictionary: an object of its entries in own-key order; entries
    /// whose value is `undefined` are omitted, as for record fields.
    fn dictionary(&mut self, payload: u64, depth: usize) -> Result<(), EncodeError> {
        let dictionary =
            Dictionary::read_in(self.memory, payload, VERSION, self.bounds).map_err(layout)?;
        let ptr = payload as u32 as usize;
        self.enter(ptr)?;
        self.punct(b'{')?;
        self.stack.push(Frame::Dictionary {
            ptr,
            dictionary: alloc::boxed::Box::new(dictionary),
            index: 0,
            depth,
            written: false,
        });
        Ok(())
    }
    /// A record, validated as `validate_aggregate_in` validates it: its
    /// fields by slot (unions resolved, a callable `toJSON` refused), every
    /// absent or `undefined` slot visited, then its present fields in own-key
    /// order (canonical array indices ascending, then insertion order).
    fn record(&mut self, payload: u64, depth: usize) -> Result<(), EncodeError> {
        const UNSET: Field = Field {
            key: 0,
            len: 0,
            kind: u32::MAX,
            payload: 0,
            present: false,
            optional: false,
        };
        let ptr = payload as u32 as usize;
        // One validating pass collects the fields by slot and the present
        // slots in insertion order.
        let mut fields = self.spare_fields.pop().unwrap_or_default();
        fields.clear();
        let mut order = self.spare_order.pop().unwrap_or_default();
        order.clear();
        let memory = self.memory;
        let shape = self.shape(payload)?;
        read_record_with(
            memory,
            payload,
            VERSION,
            &self.shapes[shape],
            |entry| {
                if fields.len() <= entry.slot {
                    fields.resize(entry.slot + 1, UNSET);
                }
                fields[entry.slot] = Field {
                    key: entry.key,
                    len: entry.len,
                    kind: entry.kind as u32,
                    payload: entry.payload,
                    present: entry.present,
                    optional: entry.optional,
                };
            },
            |slot| order.push(slot),
        )
        .map_err(layout)?;
        if self.path.contains(ptr) {
            return Err(EncodeError::Cyclic);
        }
        let mut indexed = false;
        for field in fields.iter_mut() {
            if field.present
                && field.len == TO_JSON.len()
                && memory.borrow(field.key, field.len).as_deref() == Some(TO_JSON)
            {
                let callable = if field.kind == Kind::ReferenceUnion as u32 {
                    ReferenceBox::read_envelope(memory, field.payload, VERSION)
                        .map_err(layout)?
                        .kind
                        == Kind::Function
                } else {
                    field.kind == Kind::Function as u32
                };
                if callable {
                    return Err(EncodeError::ToJson);
                }
            }
            let (kind, payload) = self.resolve(field.kind, field.payload)?;
            if !field.present && (!field.optional || kind != Kind::Undefined as u32 || payload != 0)
            {
                return Err(EncodeError::Layout);
            }
            field.kind = kind;
            field.payload = payload;
            indexed |= field.present && self.index_key(*field).is_some();
        }
        // Integer-index keys move ahead of the others, ascending (a stable
        // sort keeps the rest in insertion order).
        if indexed {
            order.sort_by_key(|slot| {
                let field = fields[*slot];
                let index = memory
                    .borrow(field.key, field.len)
                    .and_then(|key| index_key(&key));
                (index.is_none(), index.unwrap_or(0))
            });
        }
        self.path.insert(ptr);
        self.punct(b'{')?;
        // Every reserved slot, absent ones included, consumes the traversal
        // budget.
        for field in &fields {
            if !field.present || field.kind == Kind::Undefined as u32 {
                self.visit(depth + 1)?;
                if field.payload != 0 {
                    return Err(EncodeError::Layout);
                }
            }
        }
        self.stack.push(Frame::Record {
            ptr,
            fields,
            order,
            index: 0,
            depth,
            written: false,
        });
        Ok(())
    }
    /// The index in [`Self::shapes`] of the validated shape of the record
    /// `payload`.
    fn shape(&mut self, payload: u64) -> Result<usize, EncodeError> {
        let (descriptor, count) = record_descriptor(self.memory, payload).map_err(layout)?;
        if let Some(index) = self
            .shapes
            .iter()
            .position(|shape| shape.descriptor() == descriptor && shape.count() == count)
        {
            return Ok(index);
        }
        let shape = RecordShape::read(self.memory, descriptor, count, VERSION, &self.bounds)
            .map_err(layout)?;
        if self.shapes.len() == CACHED_SHAPES {
            self.shapes.remove(0);
        }
        self.shapes.push(shape);
        Ok(self.shapes.len() - 1)
    }
    /// The canonical array index a field's key names, if any.
    fn index_key(&self, field: Field) -> Option<u32> {
        if field.len == 0 || field.len > MAX_INDEX_DIGITS {
            return None;
        }
        index_key(&self.memory.borrow(field.key, field.len)?)
    }
    /// A promise: `{}` in JSON; in diagnostics `Promise { <pending> }`,
    /// `Promise { value }` or `Promise { <rejected> reason }`.
    fn promise(&mut self, payload: u64, depth: usize) -> Result<(), EncodeError> {
        // One validating pass finds the promise record's state and settled
        // fields (the first descriptor entry of each name).
        const NAMES: [&str; 3] = [
            PROMISE_STATE_FIELD,
            PROMISE_VALUE_FIELD,
            PROMISE_REASON_FIELD,
        ];
        let memory = self.memory;
        let diagnostic = self.mode == Mode::Diagnostic;
        let mut fields: [Option<(u32, u64)>; 3] = [None; 3];
        let shape = self.shape(payload)?;
        read_record_with(
            memory,
            payload,
            VERSION,
            &self.shapes[shape],
            |entry| {
                if !diagnostic {
                    return;
                }
                for (field, name) in fields.iter_mut().zip(NAMES) {
                    if field.is_none()
                        && entry.len == name.len()
                        && memory.borrow(entry.key, entry.len).as_deref() == Some(name.as_bytes())
                    {
                        *field = Some((entry.kind as u32, entry.payload));
                    }
                }
            },
            |_| {},
        )
        .map_err(layout)?;
        self.visit(depth + 1)?;
        if !diagnostic {
            return self.ascii(b"{}");
        }
        let ptr = payload as u32 as usize;
        if self.path.contains(ptr) {
            return Err(EncodeError::Cyclic);
        }
        let [state, value, reason] = fields;
        let (kind, state) = state.ok_or(EncodeError::Layout)?;
        if kind != Kind::Float64 as u32 {
            return Err(EncodeError::Layout);
        }
        let state = f64::from_bits(state);
        self.path.insert(ptr);
        let (field, opening): (Option<(u32, u64)>, &[u8]) =
            if state == f64::from(PromiseState::Pending as i32) {
                self.ascii(b"Promise { <pending> }")?;
                self.path.remove(ptr);
                return Ok(());
            } else if state == f64::from(PromiseState::Fulfilled as i32) {
                (value, b"Promise { ")
            } else if state == f64::from(PromiseState::Rejected as i32) {
                (reason, b"Promise { <rejected> ")
            } else {
                return Err(EncodeError::Layout);
            };
        let (kind, payload) = field.ok_or(EncodeError::Layout)?;
        self.ascii(opening)?;
        self.stack.push(Frame::Promise { ptr });
        // The settled value or reason; `undefined` prints as in Node.
        let (kind, payload) = self.resolve(kind, payload)?;
        if kind == Kind::Undefined as u32 {
            if payload != 0 {
                return Err(EncodeError::Layout);
            }
            self.visit(depth + 2)?;
            return self.ascii(b"undefined");
        }
        self.value(kind, payload, depth + 2)
    }
    /// Write the next member of the innermost aggregate, or close it.
    fn step(&mut self) -> Result<(), EncodeError> {
        match self.stack.last_mut() {
            Some(Frame::Array {
                ptr,
                vector,
                index,
                depth,
            }) => {
                let (ptr, at, depth) = (*ptr, *index, *depth);
                let (storage, length, kind) = (vector.storage, vector.length, vector.kind);
                if at == length {
                    return self.close(ptr, b']');
                }
                *index += 1;
                let present = vector.is_present(self.memory, at).map_err(layout)?;
                if at != 0 {
                    self.punct(b',')?;
                }
                if !present {
                    return self.value(Kind::Undefined as u32, 0, depth + 1);
                }
                let element = self.slot(storage + ARRAY_META_BYTES + at * ARRAY_SLOT_BYTES)?;
                self.value(kind as u32, element, depth + 1)
            }
            Some(Frame::Dictionary { .. }) => self.dictionary_step(),
            Some(Frame::Record { .. }) => self.record_step(),
            Some(Frame::Promise { ptr }) => {
                let ptr = *ptr;
                self.punct(b' ')?;
                self.close(ptr, b'}')
            }
            None => Ok(()),
        }
    }
    /// Pop the innermost aggregate after writing its closing `text`.
    fn close(&mut self, ptr: usize, closing: u8) -> Result<(), EncodeError> {
        // A record's field lists serve the next record.
        if let Some(Frame::Record { fields, order, .. }) = self.stack.pop() {
            self.spare_fields.push(fields);
            self.spare_order.push(order);
        }
        self.punct(closing)?;
        self.path.remove(ptr);
        Ok(())
    }
    fn dictionary_step(&mut self) -> Result<(), EncodeError> {
        loop {
            let Some(Frame::Dictionary {
                ptr,
                dictionary,
                index,
                depth,
                written,
            }) = self.stack.last_mut()
            else {
                return Err(EncodeError::Layout);
            };
            let (ptr, at, depth, was_written) = (*ptr, *index, *depth, *written);
            if at == dictionary.keys.length {
                return self.close(ptr, b'}');
            }
            *index += 1;
            // Holes are deleted entries of an indexed dictionary.
            let Some((key, value)) = dictionary.entry(self.memory, at).map_err(layout)? else {
                continue;
            };
            let values = dictionary.values.kind as u32;
            let (kind, value) = self.resolve(values, value)?;
            if kind == Kind::Undefined as u32 {
                self.visit(depth + 1)?;
                if value != 0 {
                    return Err(EncodeError::Layout);
                }
                continue;
            }
            if let Some(Frame::Dictionary { written, .. }) = self.stack.last_mut() {
                *written = true;
            }
            if was_written {
                self.punct(b',')?;
            }
            let (start, len) = string_range(key);
            let memory = self.memory;
            let bytes = memory.borrow(start, len).ok_or(EncodeError::Layout)?;
            if !len.is_multiple_of(2) {
                return Err(EncodeError::Layout);
            }
            self.string16(&bytes)?;
            self.punct(b':')?;
            return self.value(kind, value, depth + 1);
        }
    }
    fn record_step(&mut self) -> Result<(), EncodeError> {
        loop {
            let Some(Frame::Record {
                ptr,
                fields,
                order,
                index,
                depth,
                written,
            }) = self.stack.last_mut()
            else {
                return Err(EncodeError::Layout);
            };
            let (ptr, at, depth, was_written) = (*ptr, *index, *depth, *written);
            let Some(&slot) = order.get(at) else {
                return self.close(ptr, b'}');
            };
            let field = fields[slot];
            *index += 1;
            if field.kind == Kind::Undefined as u32 {
                continue;
            }
            // Private state (ES private names, prelude state) is no property.
            if self.mode == Mode::Json && self.private(field)? {
                self.visit(depth + 1)?;
                continue;
            }
            if let Some(Frame::Record { written, .. }) = self.stack.last_mut() {
                *written = true;
            }
            if was_written {
                self.punct(b',')?;
            }
            self.key(field)?;
            self.punct(b':')?;
            return self.value(field.kind, field.payload, depth + 1);
        }
    }
    fn private(&self, field: Field) -> Result<bool, EncodeError> {
        if field.len == 0 {
            return Ok(false);
        }
        let first = self
            .memory
            .borrow(field.key, 1)
            .ok_or(EncodeError::Layout)?;
        Ok(first[0] == PRIVATE_KEY_PREFIX as u8)
    }
}

/// Record descriptors one serialization keeps validated.
const CACHED_SHAPES: usize = 16;
/// UTF-8 bytes of a `\uxxxx` escape.
const ESCAPE_BYTES: usize = 6;
/// Digits of the largest canonical array index (2^32 - 2).
const MAX_INDEX_DIGITS: usize = 10;

/// The canonical array index a UTF-8 key names (`0` or a decimal without
/// leading zeros below `u32::MAX`).
fn index_key(key: &[u8]) -> Option<u32> {
    if key.is_empty()
        || key.len() > 10
        || !key.iter().all(u8::is_ascii_digit)
        || (key.len() > 1 && key[0] == b'0')
    {
        return None;
    }
    let value = key
        .iter()
        .fold(0u64, |value, digit| value * 10 + u64::from(digit - b'0'));
    u32::try_from(value).ok().filter(|value| *value < u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn index_keys_are_canonical_array_indices() {
        for (key, index) in [
            ("0", Some(0)),
            ("10", Some(10)),
            ("4294967294", Some(4_294_967_294)),
            ("4294967295", None),
            ("01", None),
            ("+1", None),
            ("", None),
            ("1a", None),
            ("99999999999", None),
        ] {
            assert_eq!(index_key(key.as_bytes()), index, "{key}");
        }
    }
    #[test]
    fn linked_codes_round_trip() {
        for error in [
            EncodeError::Cyclic,
            EncodeError::Depth,
            EncodeError::Nodes,
            EncodeError::DepthCeiling(10_000),
            EncodeError::Output,
            EncodeError::ToJson,
        ] {
            let (code, requested) = error.linked_code().unwrap();
            assert_eq!(EncodeError::from_linked_code(code, requested), Some(error));
        }
        assert_eq!(EncodeError::BigInt.linked_code(), None);
        assert_eq!(EncodeError::Layout.linked_code(), None);
        assert_eq!(
            EncodeError::from_linked_code(LINKED_FAILURE_BASE - 1, 0),
            None
        );
        assert_eq!(
            EncodeError::from_linked_code(LINKED_FAILURE_BASE + 6, 0),
            None
        );
    }
    #[test]
    fn strings_escape_as_the_host_writes_them() {
        let encode = |units: &[u16]| {
            let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
            let mut out = Vec::new();
            let mut encoder = Encoder::new(
                &[][..],
                ModuleBounds::v38(abi::StaticRegion::SINGLE_PAGE),
                Mode::Json,
                &mut out,
                None,
            );
            encoder.string16(&bytes).unwrap();
            encoder.flush();
            let utf8 = encoder.utf8;
            (alloc::string::String::from_utf16(&out).unwrap(), utf8)
        };
        let text: Vec<u16> = "a\"\\\u{8}\u{c}\n\r\t\u{1}\u{1f} é😀\u{7f}"
            .encode_utf16()
            .collect();
        let (json, utf8) = encode(&text);
        assert_eq!(json, "\"a\\\"\\\\\\b\\f\\n\\r\\t\\u0001\\u001f é😀\u{7f}\"");
        assert_eq!(utf8, json.len());
        let (json, utf8) = encode(&[0xD800, 0x61, 0xDC00, 0xD83D, 0xDE00, 0xDBFF]);
        assert_eq!(json, "\"\\ud800a\\udc00😀\\udbff\"");
        assert_eq!(utf8, json.len());
    }
}
