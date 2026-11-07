//! Node 24's console notation of value18 values (v38 `node_console`):
//! `util.formatWithOptions` of a logged argument list with the default
//! `util.inspect` options (depth 2, `compact: 3`, `breakLength: 80`,
//! `maxArrayLength: 100`, `maxStringLength: 10000`, no colors).
//!
//! The algorithm is Node's `lib/internal/util/inspect.js` step for step
//! (`formatValue`, `formatRaw`, `formatList`, `groupArrayElements`,
//! `reduceToSingleString`, `strEscape`, `formatError`, `improveStack`), so
//! line breaking, column grouping, quoting, circular references and depth
//! elision print exactly as Node prints them. Values Varta has no source for
//! print as in Node run with `--stack-trace-limit=0`: an Error has no stack
//! frames, so it prints as `[Name: message]`. Functions carry no name and
//! print as Node prints an anonymous function.
//!
//! Recursion is bounded by the inspection depth (at most [`HIDDEN_DEPTH`]
//! levels of aggregates below an argument); every read goes through the
//! shared `no_std` value readers under the module's bounds, so a
//! noncanonical layout is [`EncodeError::Layout`].
use crate::{Memory, json::EncodeError, number::Shortest};

mod console;
use alloc::{borrow::ToOwned, string::String, vec::Vec};
pub use console::render;
use varta_bigint::Budget;
use varta_common::{
    bigint::BigIntView,
    function::check_closure_envelope,
    value::{
        ARRAY_META_BYTES, ARRAY_SLOT_BYTES, Dictionary, ErrorObject,
        HASHED_DICTIONARY_VALUE_VERSION as VERSION, ModuleBounds, PRIVATE_KEY_PREFIX,
        PROMISE_REASON_FIELD, PROMISE_STATE_FIELD, PROMISE_VALUE_FIELD, PrimitiveBox, PromiseState,
        RecordDisplay, RecordShape, ReferenceBox, ValueKind as Kind, Vector, byte_at, byte_vector,
        read_record_with, record_descriptor, record_display, string_range, symbol_record,
    },
};

/// UTF-16 text.
type Text = Vec<u16>;

/// `util.inspect`'s `breakLength`.
const BREAK_LENGTH: usize = 80;
/// `util.inspect`'s `compact`.
const COMPACT: usize = 3;
/// `util.inspect`'s default `depth`.
const DEFAULT_DEPTH: usize = 2;
/// The depth of `%o` (`showHidden`).
pub const HIDDEN_DEPTH: usize = 4;
/// `util.inspect`'s `maxArrayLength`.
const MAX_ARRAY_LENGTH: usize = 100;
/// `util.inspect`'s `maxStringLength`.
const MAX_STRING_LENGTH: usize = 10_000;
/// The `maxArrayLength` of a `console.table` cell.
const TABLE_ARRAY_LENGTH: usize = 3;
/// `kMinLineLength`: shorter strings never split into lines.
const MIN_LINE_LENGTH: usize = 16;
/// Arrays with more entries than this group into columns.
const GROUP_ENTRIES: usize = 6;
/// The most columns of a grouped array.
const MAX_COLUMNS: usize = 15;
/// `approxCharHeights` of `groupArrayElements`.
const CHAR_HEIGHTS: f64 = 2.5;
/// A comma and a space between grouped entries.
const SEPARATOR_SPACE: usize = 2;
/// Added to an entry list's start before its single-line check.
const LINE_START: usize = 10;
/// The fields of an Error object record (an Error class instance).
const ERROR_NAME: &[u8] = b"#name";
const ERROR_MESSAGE: &[u8] = b"#message";
const ERROR_CAUSE: &[u8] = b"#cause";
/// `AggregateError`'s own non-enumerable `errors` property.
const AGGREGATE_ERRORS: &[u8] = b"errors";
/// The same property kept as prelude state.
const AGGREGATE_ERRORS_STATE: &[u8] = b"#errors";
/// The Map and Set storage vectors of the collections prelude.
const STORE_KEYS: &[u8] = b"keys";
const STORE_VALUES: &[u8] = b"values";
/// The `Date` prelude's time value.
const DATE_TIME: &[u8] = b"#time";
/// The private state this notation reads.
const STATE: [&[u8]; 5] = [
    ERROR_NAME,
    ERROR_MESSAGE,
    ERROR_CAUSE,
    DATE_TIME,
    AGGREGATE_ERRORS_STATE,
];
/// Error properties `formatError` never lists (the stack contains them).
const ERROR_HIDDEN: [&[u8]; 3] = [b"name", b"message", b"stack"];
/// Milliseconds per day.
const MS_PER_DAY: f64 = 86_400_000.0;

/// `util.inspect`'s `compact`: a number of inner levels joined on one line
/// (`false` is 0), or `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compact {
    Levels(usize),
    Single,
}

/// The `util.inspect` options a console method sets (`console.dir`, the
/// cells of `console.table`); [`Options::DEFAULT`] are Node's defaults. An
/// unbounded `depth`, `breakLength`, `maxArrayLength` or `maxStringLength`
/// (`Infinity`, `null`) is the type's maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// `depth`, compared with the recursion level as Node compares it
    /// (`-1` elides even the top-level object).
    pub depth: i64,
    pub show_hidden: bool,
    pub break_length: usize,
    pub compact: Compact,
    pub max_array_length: usize,
    pub max_string_length: usize,
    /// `sorted: true`: object entries in code unit order.
    pub sorted: bool,
    /// `numericSeparator`: `_` between digit groups of Numbers and BigInts.
    pub numeric_separator: bool,
}

impl Options {
    pub const DEFAULT: Self = Self {
        depth: DEFAULT_DEPTH as i64,
        show_hidden: false,
        break_length: BREAK_LENGTH,
        compact: Compact::Levels(COMPACT),
        max_array_length: MAX_ARRAY_LENGTH,
        max_string_length: MAX_STRING_LENGTH,
        sorted: false,
        numeric_separator: false,
    };
    /// The options of a `console.table` cell (`_inspect`): `depth` 0 (`-1`
    /// for a non-array object with more than two own keys),
    /// `maxArrayLength` 3, unbounded `breakLength`.
    #[must_use]
    pub const fn table_cell(elided: bool) -> Self {
        Self {
            depth: if elided { -1 } else { 0 },
            max_array_length: TABLE_ARRAY_LENGTH,
            break_length: usize::MAX,
            ..Self::DEFAULT
        }
    }
}

/// `util.formatWithOptions` of the logged argument array held by the
/// `unknown[]` cell `cell`: `%` directives of a leading string, then every
/// remaining argument, strings as they are and other values inspected, all
/// joined with spaces. As in Node the text has no length limit of its own:
/// the execution's fuel, memory and output budgets bound it.
pub fn format_arguments<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    bounds: ModuleBounds,
    budget: &mut Budget,
) -> Result<Text, EncodeError> {
    let array = ReferenceBox::read_envelope(memory, cell, VERSION).map_err(layout)?;
    if array.kind != Kind::Array {
        return Err(EncodeError::Layout);
    }
    let vector = Vector::read_in(memory, array.payload, VERSION, bounds).map_err(layout)?;
    let mut inspector = Inspector::new(memory, bounds, budget);
    let mut args = Vec::with_capacity(vector.length);
    for at in 0..vector.length {
        if !vector.is_present(memory, at).map_err(layout)? {
            return Err(EncodeError::Layout);
        }
        let slot = inspector.slot(vector.storage + ARRAY_META_BYTES + at * ARRAY_SLOT_BYTES)?;
        args.push(inspector.resolve(vector.kind as u32, slot)?);
    }
    inspector.format(&args)
}

fn layout<T>(_: T) -> EncodeError {
    EncodeError::Layout
}
fn ascii(text: &str) -> Text {
    text.encode_utf16().collect()
}
/// The parts one after another.
fn concat(parts: &[&[u16]]) -> Text {
    parts.concat()
}
/// The decimal digits of `value`, zero-padded to `width` (no `core::fmt`:
/// linked intrinsics have no function tables).
fn digits(value: u64, width: usize) -> Text {
    radix_digits(value, width, 10, false)
}
/// The hexadecimal digits of `value`, zero-padded to `width`.
fn hex(value: u32, width: usize, upper: bool) -> Text {
    radix_digits(u64::from(value), width, 16, upper)
}
fn radix_digits(mut value: u64, width: usize, radix: u64, upper: bool) -> Text {
    let letters: &[u8; 16] = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut text = Text::new();
    loop {
        text.push(u16::from(letters[(value % radix) as usize]));
        value /= radix;
        if value == 0 && text.len() >= width {
            break;
        }
    }
    text.reverse();
    text
}
fn push(text: &mut Text, ascii: &str) {
    text.extend(ascii.encode_utf16());
}
fn spaces(count: usize) -> Text {
    alloc::vec![u16::from(b' '); count]
}
fn contains(text: &[u16], needle: &[u16]) -> bool {
    needle.is_empty() || text.windows(needle.len()).any(|window| window == needle)
}
fn index_of(text: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    (from..=text.len().saturating_sub(needle.len()))
        .find(|at| text.get(*at..*at + needle.len()) == Some(needle))
}
fn join(parts: &[Text], separator: &[u16]) -> Text {
    let mut text = Text::new();
    for (index, part) in parts.iter().enumerate() {
        if index != 0 {
            text.extend_from_slice(separator);
        }
        text.extend_from_slice(part);
    }
    text
}
fn pad_start(text: &[u16], width: usize) -> Text {
    let mut padded = spaces(width.saturating_sub(text.len()));
    padded.extend_from_slice(text);
    padded
}
fn pad_end(text: &[u16], width: usize) -> Text {
    let mut padded = text.to_vec();
    padded.resize(width.max(text.len()), u16::from(b' '));
    padded
}
/// `remainingText`.
fn remaining_text(remaining: usize) -> Text {
    let plural = if remaining > 1 { "s" } else { "" };
    concat(&[
        &ascii("... "),
        &digits(remaining as u64, 1),
        &ascii(" more item"),
        &ascii(plural),
    ])
}
/// `<n empty items>`.
fn empty_items(count: usize) -> Text {
    let plural = if count > 1 { "s" } else { "" };
    concat(&[
        &ascii("<"),
        &digits(count as u64, 1),
        &ascii(" empty item"),
        &ascii(plural),
        &ascii(">"),
    ])
}

/// `formatNumber`: `-0` keeps its sign.
fn number(value: f64) -> Text {
    if value == 0.0 && value.is_sign_negative() {
        return ascii("-0");
    }
    if value.is_finite() {
        Shortest::new()
            .format_finite(value)
            .iter()
            .map(|byte| u16::from(*byte))
            .collect()
    } else {
        ascii(ryu_js::Buffer::new().format(value))
    }
}

/// The escape `strEscape` writes for `unit` below `0xA0`, if any (Node's
/// `meta` table): C0 controls, `'` in single quotes, `\`, DEL and C1.
fn meta(unit: u16, single: bool) -> Option<Text> {
    Some(match unit {
        0x08 => ascii("\\b"),
        0x09 => ascii("\\t"),
        0x0A => ascii("\\n"),
        0x0C => ascii("\\f"),
        0x0D => ascii("\\r"),
        0x00..0x20 | 0x7F..0xA0 => concat(&[&ascii("\\x"), &hex(u32::from(unit), 2, true)]),
        0x27 if single => ascii("\\'"),
        0x5C => ascii("\\\\"),
        _ => return None,
    })
}
/// `strEscape`: the quoted string, quoted with `'` unless it holds one
/// (then `"`, or `` ` `` when it also holds `"`), with control characters,
/// the quote, `\` and lone surrogates escaped.
fn quoted(units: &[u16]) -> Text {
    const SINGLE: u16 = b'\'' as u16;
    const DOUBLE: u16 = b'"' as u16;
    const BACKTICK: u16 = b'`' as u16;
    let mut quote = SINGLE;
    if units.contains(&SINGLE) {
        if !units.contains(&DOUBLE) {
            quote = DOUBLE;
        } else if !units.contains(&BACKTICK) && !contains(units, &ascii("${")) {
            quote = BACKTICK;
        }
    }
    let mut text = alloc::vec![quote];
    let mut at = 0;
    while at < units.len() {
        let unit = units[at];
        if let Some(escape) = meta(unit, quote == SINGLE) {
            text.extend(escape);
        } else if (0xD800..0xE000).contains(&unit) {
            let paired = unit < 0xDC00
                && units
                    .get(at + 1)
                    .is_some_and(|next| (0xDC00..0xE000).contains(next));
            if paired {
                text.extend_from_slice(&units[at..at + 2]);
                at += 1;
            } else {
                push(&mut text, "\\u");
                text.extend(hex(u32::from(unit), 1, false));
            }
        } else {
            text.push(unit);
        }
        at += 1;
    }
    text.push(quote);
    text
}
/// A property key: an identifier as it is (`__proto__` quoted in brackets),
/// any other key quoted.
fn key(units: &[u16]) -> Text {
    let identifier = units.first().is_some_and(|first| {
        *first == u16::from(b'_') || u8::try_from(*first).is_ok_and(|b| b.is_ascii_alphabetic())
    }) && units.iter().all(|unit| {
        *unit == u16::from(b'_') || u8::try_from(*unit).is_ok_and(|b| b.is_ascii_alphanumeric())
    });
    if !identifier {
        return quoted(units);
    }
    if units == ascii("__proto__").as_slice() {
        return ascii("['__proto__']");
    }
    units.to_vec()
}
/// `getStringWidth` without colors: ASCII by its printable units; from the
/// first non-ASCII unit on, by code point, wide East Asian and emoji code
/// points two columns and zero-width ones none (Node's tables).
fn width(text: &[u16]) -> usize {
    let mut width = 0;
    for (at, unit) in text.iter().enumerate() {
        if *unit >= 127 {
            return width
                + char::decode_utf16(text[at..].iter().copied())
                    .map(|c| c.map_or(1, |c| code_point_width(c as u32)))
                    .sum::<usize>();
        }
        width += usize::from(*unit >= 32);
    }
    width
}
fn code_point_width(code: u32) -> usize {
    let zero = code <= 0x1F
        || (0x7F..=0x9F).contains(&code)
        || (0x300..=0x36F).contains(&code)
        || (0x200B..=0x200F).contains(&code)
        || (0x20D0..=0x20FF).contains(&code)
        || (0xFE00..=0xFE0F).contains(&code)
        || (0xFE20..=0xFE2F).contains(&code)
        || (0xE0100..=0xE01EF).contains(&code);
    let full = code >= 0x1100
        && (code <= 0x115F
            || code == 0x2329
            || code == 0x232A
            || ((0x2E80..=0x3247).contains(&code) && code != 0x303F)
            || (0x3250..=0x4DBF).contains(&code)
            || (0x4E00..=0xA4C6).contains(&code)
            || (0xA960..=0xA97C).contains(&code)
            || (0xAC00..=0xD7A3).contains(&code)
            || (0xF900..=0xFAFF).contains(&code)
            || (0xFE10..=0xFE19).contains(&code)
            || (0xFE30..=0xFE6B).contains(&code)
            || (0xFF01..=0xFF60).contains(&code)
            || (0xFFE0..=0xFFE6).contains(&code)
            || (0x1B000..=0x1B001).contains(&code)
            || (0x1F200..=0x1F251).contains(&code)
            || (0x1F300..=0x1F64F).contains(&code)
            || (0x20000..=0x3FFFD).contains(&code));
    if full {
        2
    } else if zero {
        0
    } else {
        1
    }
}

/// A resolved value: a scalar or an aggregate's handle.
#[derive(Debug, Clone, Copy)]
struct Value {
    kind: Kind,
    payload: u64,
}

/// A record slot before it is resolved: key, kind tag, payload, presence.
type RawField = (Vec<u8>, u32, u64, bool);
/// A record field as the inspector reads it.
#[derive(Debug, Clone)]
struct Field {
    key: Vec<u8>,
    value: Value,
    present: bool,
}
/// A record's fields: its display trailer and its present fields in own-key
/// order.
struct Record {
    display: RecordDisplay,
    name: String,
    fields: Vec<Field>,
}
impl Record {
    fn field(&self, name: &[u8]) -> Option<&Field> {
        self.fields.iter().find(|field| field.key == name)
    }
}

/// An aggregate's `formatRaw` parts.
struct Raw {
    base: Text,
    braces: [Text; 2],
    /// `kArrayExtrasType`: entries may group into columns.
    array: bool,
    output: Vec<Text>,
    /// For grouping: whether element `i` of the array is a Number or BigInt.
    numeric: Vec<bool>,
}

struct Inspector<'m, 'b, M: Memory + ?Sized> {
    memory: &'m M,
    bounds: ModuleBounds,
    budget: &'b mut Budget,
    options: Options,
    indentation: usize,
    /// Aggregates being formatted (`ctx.seen`).
    seen: Vec<u32>,
    /// Aggregates found to be circular; reference `n` is entry `n - 1`.
    circular: Vec<u32>,
    /// `ctx.currentDepth`.
    current_depth: usize,
    shapes: Vec<RecordShape>,
}

impl<'m, 'b, M: Memory + ?Sized> Inspector<'m, 'b, M> {
    fn new(memory: &'m M, bounds: ModuleBounds, budget: &'b mut Budget) -> Self {
        Self {
            memory,
            bounds,
            budget,
            options: Options::DEFAULT,
            indentation: 0,
            seen: Vec::new(),
            circular: Vec::new(),
            current_depth: 0,
            shapes: Vec::new(),
        }
    }
    fn slot(&self, at: usize) -> Result<u64, EncodeError> {
        self.memory.u64_at(at).ok_or(EncodeError::Layout)
    }
    /// The member a slot of `kind` holds: a union cell's member, a function
    /// after its envelope check.
    fn resolve(&self, kind: u32, payload: u64) -> Result<Value, EncodeError> {
        let kind = Kind::from_tag(kind)
            .filter(|kind| kind.supported_by(VERSION))
            .ok_or(EncodeError::Layout)?;
        match kind {
            Kind::Function => {
                check_closure_envelope(self.memory, payload, &self.bounds.limits, VERSION)
                    .map_err(layout)?;
                Ok(Value { kind, payload })
            }
            Kind::ReferenceUnion => {
                let cell =
                    ReferenceBox::read_envelope(self.memory, payload, VERSION).map_err(layout)?;
                self.resolve(cell.kind as u32, cell.payload)
            }
            Kind::PrimitiveUnion => {
                let cell = PrimitiveBox::read_limited(
                    self.memory,
                    payload,
                    self.bounds.limits.data.string_bytes,
                )
                .map_err(layout)?;
                self.resolve(cell.kind as u32, cell.payload)
            }
            Kind::Undefined | Kind::Null if payload != 0 => Err(EncodeError::Layout),
            Kind::Boolean if payload > 1 => Err(EncodeError::Layout),
            _ => Ok(Value { kind, payload }),
        }
    }
    /// The code units of a string payload.
    fn string(&self, payload: u64) -> Result<Text, EncodeError> {
        let (ptr, len) = string_range(payload);
        if len > self.bounds.limits.data.string_bytes || !len.is_multiple_of(2) {
            return Err(EncodeError::Layout);
        }
        let bytes = self.memory.borrow(ptr, len).ok_or(EncodeError::Layout)?;
        Ok(bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect())
    }
    fn bigint(&mut self, payload: u64) -> Result<Text, EncodeError> {
        BigIntView::check(self.memory, payload).map_err(layout)?;
        let text = crate::bigint::integer(self.memory, payload, self.budget)?
            .to_string_radix(10, self.budget)?;
        self.budget.consume_work(text.len() as u64)?;
        let mut text = ascii(&text);
        text.push(u16::from(b'n'));
        Ok(text)
    }

    /// `formatWithOptionsInternal`.
    fn format(&mut self, args: &[Value]) -> Result<Text, EncodeError> {
        let mut text = Text::new();
        let mut next = 0;
        let mut joiner: &str = "";
        if let Some(first) = args.first().filter(|first| first.kind == Kind::String) {
            let first = self.string(first.payload)?;
            if args.len() == 1 {
                return Ok(first);
            }
            let mut last = 0;
            let mut at = 0;
            while at + 1 < first.len() {
                if first[at] == u16::from(b'%') {
                    at += 1;
                    let directive = first[at];
                    if next + 1 != args.len() {
                        let replacement = match u8::try_from(directive).unwrap_or(0) {
                            b's' => {
                                next += 1;
                                self.directive_string(args[next])?
                            }
                            b'j' => {
                                next += 1;
                                self.directive_json(args[next])?
                            }
                            b'd' => {
                                next += 1;
                                self.directive_number(args[next], Numeric::Number)?
                            }
                            b'O' => {
                                next += 1;
                                self.inspect(args[next], DEFAULT_DEPTH, false)?
                            }
                            b'o' => {
                                next += 1;
                                self.inspect(args[next], HIDDEN_DEPTH, true)?
                            }
                            b'i' => {
                                next += 1;
                                self.directive_number(args[next], Numeric::Integer)?
                            }
                            b'f' => {
                                next += 1;
                                self.directive_number(args[next], Numeric::Float)?
                            }
                            b'c' => {
                                next += 1;
                                Text::new()
                            }
                            b'%' => {
                                text.extend_from_slice(&first[last..at]);
                                last = at + 1;
                                at += 1;
                                continue;
                            }
                            _ => {
                                at += 1;
                                continue;
                            }
                        };
                        if last != at - 1 {
                            text.extend_from_slice(&first[last..at - 1]);
                        }
                        text.extend(replacement);
                        last = at + 1;
                    } else if directive == u16::from(b'%') {
                        text.extend_from_slice(&first[last..at]);
                        last = at + 1;
                    }
                }
                at += 1;
            }
            if last != 0 {
                next += 1;
                joiner = " ";
                if last < first.len() {
                    text.extend_from_slice(&first[last..]);
                }
            }
        }
        while next < args.len() {
            push(&mut text, joiner);
            let value = args[next];
            if value.kind == Kind::String {
                text.extend(self.string(value.payload)?);
            } else {
                text.extend(self.inspect(value, DEFAULT_DEPTH, false)?);
            }
            joiner = " ";
            next += 1;
        }
        Ok(text)
    }
    /// `util.inspect(value, { depth, showHidden })` from the top level.
    fn inspect(
        &mut self,
        value: Value,
        depth: usize,
        show_hidden: bool,
    ) -> Result<Text, EncodeError> {
        self.inspect_with(
            value,
            Options {
                depth: depth as i64,
                show_hidden,
                ..Options::DEFAULT
            },
        )
    }
    /// `util.inspect(value, options)` from the top level.
    fn inspect_with(&mut self, value: Value, options: Options) -> Result<Text, EncodeError> {
        self.options = options;
        self.indentation = 0;
        self.seen.clear();
        self.circular.clear();
        self.current_depth = 0;
        self.value(value, 0)
    }
    /// `%s`: Numbers and BigInts as inspected, other primitives as
    /// `String(value)`, objects inspected at depth 0.
    fn directive_string(&mut self, value: Value) -> Result<Text, EncodeError> {
        match value.kind {
            Kind::Float64 | Kind::I32 | Kind::BigInt => self.primitive(value),
            Kind::String => self.string(value.payload),
            Kind::Undefined | Kind::Null | Kind::Boolean => self.primitive(value),
            _ => self.inspect(value, 0, false),
        }
    }
    /// `%j`: `JSON.stringify`, `undefined` for no result and `[Circular]`
    /// for a cyclic value.
    fn directive_json(&mut self, value: Value) -> Result<Text, EncodeError> {
        let mut text = Text::new();
        match crate::json::stringify(
            self.memory,
            value.kind as u32,
            value.payload,
            self.bounds,
            &mut text,
        ) {
            Ok(Some(_)) => Ok(text),
            Ok(None) => Ok(ascii("undefined")),
            Err(EncodeError::Cyclic) => Ok(ascii("[Circular]")),
            Err(error) => Err(error),
        }
    }
    /// `%d`, `%i` and `%f`: a BigInt as inspected, otherwise the Number of
    /// `Number(value)`, `parseInt(value)` or `parseFloat(value)`.
    fn directive_number(&mut self, value: Value, numeric: Numeric) -> Result<Text, EncodeError> {
        if value.kind == Kind::BigInt && numeric != Numeric::Float {
            return self.bigint(value.payload);
        }
        let result = match numeric {
            Numeric::Number => self.number_of(value)?,
            Numeric::Integer | Numeric::Float => {
                let text = self.string_of(value)?;
                let bytes: Vec<u8> = text.iter().flat_map(|unit| unit.to_le_bytes()).collect();
                if numeric == Numeric::Integer {
                    crate::parse::integer(&bytes, 0)
                } else {
                    crate::parse::float(&bytes)
                }
            }
        };
        Ok(number(result))
    }
    /// `Number(value)` of the values whose conversion runs no program code.
    fn number_of(&mut self, value: Value) -> Result<f64, EncodeError> {
        Ok(match value.kind {
            Kind::Float64 => f64::from_bits(value.payload),
            Kind::I32 => f64::from(value.payload as i64 as i32),
            Kind::Boolean => value.payload as f64,
            Kind::Null => 0.0,
            Kind::Record
                if self.record(value.payload, Some(&[]))?.display == RecordDisplay::Date =>
            {
                self.date_time(value.payload)?
            }
            _ => {
                let text = self.string_of(value)?;
                let bytes: Vec<u8> = text.iter().flat_map(|unit| unit.to_le_bytes()).collect();
                crate::parse::string_number(&bytes)
            }
        })
    }
    /// `String(value)` for Number conversion: primitives, arrays joined
    /// with commas (`null` and `undefined` elements empty), Error objects
    /// as `Error.prototype.toString`, other objects `[object Object]`.
    fn string_of(&mut self, value: Value) -> Result<Text, EncodeError> {
        Ok(match value.kind {
            Kind::String => self.string(value.payload)?,
            Kind::BigInt => {
                let mut text = self.bigint(value.payload)?;
                text.pop();
                text
            }
            Kind::Float64 => {
                let number = f64::from_bits(value.payload);
                if number == 0.0 {
                    ascii("0")
                } else {
                    self::number(number)
                }
            }
            Kind::I32 | Kind::Boolean | Kind::Undefined | Kind::Null => self.primitive(value)?,
            Kind::Array => {
                let vector = Vector::read_in(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?;
                if self.seen.contains(&(vector.ptr as u32)) {
                    return Ok(Text::new());
                }
                self.seen.push(vector.ptr as u32);
                let mut parts = Vec::new();
                for at in 0..vector.length {
                    let element = self.element(&vector, at)?;
                    parts.push(match element {
                        Some(element) if !matches!(element.kind, Kind::Undefined | Kind::Null) => {
                            self.string_of(element)?
                        }
                        _ => Text::new(),
                    });
                }
                self.seen.pop();
                join(&parts, &ascii(","))
            }
            Kind::Bytes => {
                let vector = byte_vector(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?;
                let mut parts = Vec::new();
                for at in 0..vector.length {
                    let byte = byte_at(self.memory, &vector, at).map_err(layout)?;
                    parts.push(digits(u64::from(byte), 1));
                }
                join(&parts, &ascii(","))
            }
            Kind::Error => {
                let object = ErrorObject::read_in(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?;
                error_string(&ascii(object.class.name()), &self.string(object.message)?)
            }
            _ => ascii("[object Object]"),
        })
    }

    /// `formatValue`.
    fn value(&mut self, value: Value, recurse: usize) -> Result<Text, EncodeError> {
        let id = match value.kind {
            Kind::Record | Kind::Dictionary | Kind::Promise | Kind::Error => value.payload as u32,
            Kind::Array => {
                Vector::read_in(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?
                    .ptr as u32
            }
            Kind::Bytes => {
                byte_vector(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?
                    .ptr as u32
            }
            _ => return self.primitive(value),
        };
        if self.seen.contains(&id) {
            let index = match self.circular.iter().position(|seen| *seen == id) {
                Some(index) => index + 1,
                None => {
                    self.circular.push(id);
                    self.circular.len()
                }
            };
            return Ok(concat(&[
                &ascii("[Circular *"),
                &digits(index as u64, 1),
                &ascii("]"),
            ]));
        }
        self.raw(value, id, recurse)
    }
    /// `formatPrimitive` and the functions (which have no properties here).
    fn primitive(&mut self, value: Value) -> Result<Text, EncodeError> {
        Ok(match value.kind {
            Kind::String => {
                let units = self.string(value.payload)?;
                self.string_text(&units)
            }
            Kind::Float64 => self.number_text(f64::from_bits(value.payload)),
            Kind::I32 => {
                let value = i32::try_from(value.payload as i64).map_err(layout)?;
                self.number_text(f64::from(value))
            }
            Kind::BigInt => {
                let text = self.bigint(value.payload)?;
                if self.options.numeric_separator {
                    let mut grouped = numeric_separators(&text[..text.len() - 1]);
                    grouped.push(u16::from(b'n'));
                    grouped
                } else {
                    text
                }
            }
            Kind::Boolean => ascii(if value.payload == 0 { "false" } else { "true" }),
            Kind::Undefined => ascii("undefined"),
            Kind::Null => ascii("null"),
            Kind::Function => ascii(crate::json::FUNCTION_DISPLAY),
            // `Symbol(description)` (v38 `symbol_values`), as
            // `Symbol.prototype.toString` writes it.
            Kind::Symbol => {
                let record = symbol_record(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?;
                let mut text = ascii(crate::json::SYMBOL_OPEN);
                if let Some(description) = record.description {
                    text.extend(self.string(description)?);
                }
                text.extend(ascii(crate::json::SYMBOL_CLOSE));
                text
            }
            _ => return Err(EncodeError::Layout),
        })
    }
    /// `formatNumber`, with `numericSeparator` digit groups.
    fn number_text(&self, value: f64) -> Text {
        if !self.options.numeric_separator {
            return number(value);
        }
        let integer = libm::trunc(value);
        let whole = number(integer);
        // `String(integer)`: a negative zero prints `0`.
        let whole = if whole == ascii("-0") {
            ascii("0")
        } else {
            whole
        };
        if integer == value {
            if !value.is_finite() || contains(&whole, &ascii("e")) {
                return whole;
            }
            return numeric_separators(&whole);
        }
        if value.is_nan() {
            return whole;
        }
        let full = number(value);
        let fraction = &full[(whole.len() + 1).min(full.len())..];
        let mut text = numeric_separators(&whole);
        text.push(u16::from(b'.'));
        text.extend(numeric_separators_end(fraction));
        text
    }
    /// A nested string: quoted, cut at [`MAX_STRING_LENGTH`] units, and a
    /// long one split after its line breaks.
    fn string_text(&self, units: &[u16]) -> Text {
        let max = self.options.max_string_length;
        let (units, trailer) = if units.len() > max {
            let remaining = units.len() - max;
            let plural = if remaining > 1 { "s" } else { "" };
            (
                &units[..max],
                concat(&[
                    &ascii("... "),
                    &digits(remaining as u64, 1),
                    &ascii(" more character"),
                    &ascii(plural),
                ]),
            )
        } else {
            (units, Text::new())
        };
        let mut text = if self.options.compact != Compact::Single
            && units.len() > MIN_LINE_LENGTH
            && units.len().saturating_add(self.indentation + 4) > self.options.break_length
        {
            let mut separator = ascii(" +\n");
            separator.extend(spaces(self.indentation + 2));
            let lines: Vec<Text> = units
                .split_inclusive(|unit| *unit == u16::from(b'\n'))
                .map(quoted)
                .collect();
            join(&lines, &separator)
        } else {
            quoted(units)
        };
        text.extend(trailer);
        text
    }

    /// `formatRaw` of an aggregate `id` not on the current path.
    fn raw(&mut self, value: Value, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        match value.kind {
            Kind::Array => self.array(value.payload, id, recurse),
            Kind::Bytes => self.bytes(value.payload, id, recurse),
            Kind::Dictionary => self.dictionary(value.payload, id, recurse),
            Kind::Promise => self.promise(value.payload, id, recurse),
            Kind::Error => {
                let object = ErrorObject::read_in(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?;
                let class = ascii(object.class.name());
                let message = self.string(object.message)?;
                let base = self.error_base(&class, &class, &message);
                if !self.options.show_hidden {
                    return Ok(base);
                }
                let label = class.clone();
                self.finish(id, recurse, &label, base, ascii("{"), |inspector, _| {
                    Ok(inspector.hidden_error_keys(&message))
                })
            }
            Kind::Record => self.record_value(value.payload, id, recurse),
            _ => Err(EncodeError::Layout),
        }
    }
    /// The rest of `formatRaw` once an aggregate has entries: the depth
    /// check (`[label]`), then its entries from `body` with the aggregate on
    /// the path, its `<ref *n>` marker and `reduceToSingleString`.
    fn finish(
        &mut self,
        id: u32,
        recurse: usize,
        label: &[u16],
        base: Text,
        opening: Text,
        body: impl FnOnce(&mut Self, usize) -> Result<(Vec<Text>, Vec<bool>), EncodeError>,
    ) -> Result<Text, EncodeError> {
        if recurse as i64 > self.options.depth {
            let mut text = alloc::vec![u16::from(b'[')];
            text.extend_from_slice(label);
            text.push(u16::from(b']'));
            return Ok(text);
        }
        // Arrays and typed arrays (`kArrayExtrasType`) open with `[`.
        let array = opening.last() == Some(&u16::from(b'['));
        let recurse = recurse + 1;
        self.seen.push(id);
        self.current_depth = recurse;
        let (mut output, numeric) = body(self, recurse)?;
        let mut base = base;
        let mut opening = opening;
        if let Some(index) = self.circular.iter().position(|seen| *seen == id) {
            let mut reference =
                concat(&[&ascii("<ref *"), &digits(index as u64 + 1, 1), &ascii(">")]);
            if self.options.compact == Compact::Single {
                // `compact: true` puts the reference before the brace.
                reference.push(u16::from(b' '));
                reference.extend(opening);
                opening = reference;
            } else {
                if !base.is_empty() {
                    reference.push(u16::from(b' '));
                    reference.extend(base);
                }
                base = reference;
            }
        }
        self.seen.pop();
        // `sorted`: the entries of an object in code unit order (an array's
        // extra keys are at most its hidden `length`, so they keep theirs).
        if self.options.sorted && !array {
            output.sort();
        }
        let closing = if array { ascii("]") } else { ascii("}") };
        Ok(self.reduce(
            Raw {
                base,
                braces: [opening, closing],
                array,
                output,
                numeric,
            },
            recurse,
        ))
    }

    /// A value formatted as a property or entry: indented two more.
    fn nested(&mut self, value: Value, recurse: usize) -> Result<Text, EncodeError> {
        self.indentation += 2;
        let text = self.value(value, recurse);
        self.indentation -= 2;
        text
    }
    /// `formatProperty` of a `kObjectType` property, or of an array's
    /// hidden `length` (`array`, `kArrayExtrasType`).
    fn property(
        &mut self,
        name: &[u16],
        hidden: bool,
        value: Value,
        recurse: usize,
        array: bool,
    ) -> Result<Text, EncodeError> {
        let mut text = Text::new();
        if hidden {
            text.push(u16::from(b'['));
        }
        text.extend(key(name));
        if hidden {
            text.push(u16::from(b']'));
        }
        text.push(u16::from(b':'));
        // `compact: true` indents an object's property values by three and
        // moves one wider than `breakLength` to its own line.
        let diff = if self.options.compact == Compact::Single && !array {
            3
        } else {
            2
        };
        self.indentation += diff;
        let value = self.value(value, recurse);
        let wide =
            matches!(&value, Ok(value) if diff == 3 && self.options.break_length < width(value));
        if wide {
            text.push(u16::from(b'\n'));
            text.extend(spaces(self.indentation));
        } else {
            text.push(u16::from(b' '));
        }
        self.indentation -= diff;
        text.extend(value?);
        Ok(text)
    }

    /// The element `at` of `vector`, or `None` for a hole.
    fn element(&self, vector: &Vector, at: usize) -> Result<Option<Value>, EncodeError> {
        if !vector.is_present(self.memory, at).map_err(layout)? {
            return Ok(None);
        }
        let slot = self.slot(vector.storage + ARRAY_META_BYTES + at * ARRAY_SLOT_BYTES)?;
        self.resolve(vector.kind as u32, slot).map(Some)
    }
    fn array(&mut self, payload: u64, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        let vector = Vector::read_in(self.memory, payload, VERSION, self.bounds).map_err(layout)?;
        if vector.length == 0 && !self.options.show_hidden {
            return Ok(ascii("[]"));
        }
        self.finish(
            id,
            recurse,
            &ascii("Array"),
            Text::new(),
            ascii("["),
            |inspector, recurse| {
                let mut output = inspector.array_entries(&vector, recurse)?;
                if inspector.options.show_hidden {
                    let length = Value {
                        kind: Kind::Float64,
                        payload: (vector.length as f64).to_bits(),
                    };
                    output.push(inspector.property(
                        &ascii("length"),
                        true,
                        length,
                        recurse,
                        true,
                    )?);
                }
                let mut numeric = Vec::new();
                let max = inspector.options.max_array_length.saturating_add(2);
                for at in 0..vector.length.min(max) {
                    numeric.push(inspector.element(&vector, at)?.is_some_and(|element| {
                        matches!(element.kind, Kind::Float64 | Kind::I32 | Kind::BigInt)
                    }));
                }
                Ok((output, numeric))
            },
        )
    }
    /// `formatArray` and `formatSpecialArray`.
    fn array_entries(&mut self, vector: &Vector, recurse: usize) -> Result<Vec<Text>, EncodeError> {
        let length = vector.length.min(self.options.max_array_length);
        let mut output = Vec::new();
        let mut at = 0;
        while at < length {
            let Some(element) = self.element(vector, at)? else {
                return self.sparse_entries(vector, recurse, length, output, at);
            };
            output.push(self.nested(element, recurse)?);
            at += 1;
        }
        if vector.length > length {
            output.push(remaining_text(vector.length - length));
        }
        Ok(output)
    }
    /// `formatSpecialArray` from the first hole `index`: runs of holes as
    /// `<n empty items>`, within `max` entries.
    fn sparse_entries(
        &mut self,
        vector: &Vector,
        recurse: usize,
        max: usize,
        mut output: Vec<Text>,
        mut index: usize,
    ) -> Result<Vec<Text>, EncodeError> {
        let mut at = index;
        while at < vector.length && output.len() < max {
            let Some(element) = self.element(vector, at)? else {
                at += 1;
                continue;
            };
            if at != index {
                output.push(empty_items(at - index));
                index = at;
                if output.len() == max {
                    break;
                }
            }
            output.push(self.nested(element, recurse)?);
            index += 1;
            at += 1;
        }
        let remaining = vector.length - index;
        if output.len() != max {
            if remaining > 0 {
                output.push(empty_items(remaining));
            }
        } else if remaining > 0 {
            output.push(remaining_text(remaining));
        }
        Ok(output)
    }
    /// A `Uint8Array`: `Uint8Array(n) [ … ]`.
    fn bytes(&mut self, payload: u64, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        let vector = byte_vector(self.memory, payload, VERSION, self.bounds).map_err(layout)?;
        let opening = concat(&[
            &ascii("Uint8Array("),
            &digits(vector.length as u64, 1),
            &ascii(") ["),
        ]);
        if vector.length == 0 && !self.options.show_hidden {
            return Ok(ascii("Uint8Array(0) []"));
        }
        self.finish(
            id,
            recurse,
            &ascii("Uint8Array"),
            Text::new(),
            opening,
            |inspector, _| {
                let length = vector.length.min(inspector.options.max_array_length);
                let mut output = Vec::with_capacity(length + 1);
                for at in 0..length {
                    let byte = byte_at(inspector.memory, &vector, at).map_err(layout)?;
                    output.push(digits(u64::from(byte), 1));
                }
                if vector.length > length {
                    output.push(remaining_text(vector.length - length));
                }
                Ok((
                    output,
                    alloc::vec![
                        true;
                        vector
                            .length
                            .min(inspector.options.max_array_length.saturating_add(2))
                    ],
                ))
            },
        )
    }
    /// A dictionary: an object of its entries in own-key order.
    fn dictionary(&mut self, payload: u64, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        let dictionary =
            Dictionary::read_in(self.memory, payload, VERSION, self.bounds).map_err(layout)?;
        let mut entries = Vec::new();
        for at in 0..dictionary.keys.length {
            let Some((key, value)) = dictionary.entry(self.memory, at).map_err(layout)? else {
                continue;
            };
            let value = self.resolve(dictionary.values.kind as u32, value)?;
            entries.push((self.string(key)?, value));
        }
        if entries.is_empty() {
            return Ok(ascii("{}"));
        }
        self.finish(
            id,
            recurse,
            &ascii("Object"),
            Text::new(),
            ascii("{"),
            |inspector, recurse| {
                let mut output = Vec::with_capacity(entries.len());
                for (name, value) in &entries {
                    output.push(inspector.property(name, false, *value, recurse, false)?);
                }
                Ok((output, Vec::new()))
            },
        )
    }
    /// A promise: `Promise { <pending> }`, `Promise { value }` or
    /// `Promise { <rejected> reason }`.
    fn promise(&mut self, payload: u64, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        // The settled value or reason is read only once the state selects
        // it: the other slot holds no value.
        let field = |inspector: &mut Self, name: &str| -> Result<Value, EncodeError> {
            let record = inspector.record(payload, Some(&[name.as_bytes()]))?;
            record
                .field(name.as_bytes())
                .map(|field| field.value)
                .ok_or(EncodeError::Layout)
        };
        let state = field(self, PROMISE_STATE_FIELD)?;
        if state.kind != Kind::Float64 {
            return Err(EncodeError::Layout);
        }
        let state = f64::from_bits(state.payload);
        let settled = if state == f64::from(PromiseState::Pending as i32) {
            None
        } else if state == f64::from(PromiseState::Fulfilled as i32) {
            Some((false, field(self, PROMISE_VALUE_FIELD)?))
        } else if state == f64::from(PromiseState::Rejected as i32) {
            Some((true, field(self, PROMISE_REASON_FIELD)?))
        } else {
            return Err(EncodeError::Layout);
        };
        self.finish(
            id,
            recurse,
            &ascii("Promise"),
            Text::new(),
            ascii("Promise {"),
            |inspector, recurse| {
                let entry = match settled {
                    None => ascii("<pending>"),
                    Some((rejected, value)) => {
                        let text = inspector.nested(value, recurse)?;
                        if rejected {
                            let mut entry = ascii("<rejected> ");
                            entry.extend(text);
                            entry
                        } else {
                            text
                        }
                    }
                };
                Ok((alloc::vec![entry], Vec::new()))
            },
        )
    }

    /// The record `payload`'s display and present fields in own-key order
    /// (canonical array indices ascending, then insertion order): its
    /// properties and the private state this notation reads ([`STATE`]), or
    /// only the fields `wanted` names. Other private state is not read.
    fn record(&mut self, payload: u64, wanted: Option<&[&[u8]]>) -> Result<Record, EncodeError> {
        let (descriptor, count) =
            record_descriptor(self.memory, payload, VERSION).map_err(layout)?;
        let shape = match self
            .shapes
            .iter()
            .position(|shape| shape.descriptor() == descriptor && shape.count() == count)
        {
            Some(index) => index,
            None => {
                let shape =
                    RecordShape::read(self.memory, descriptor, count, VERSION, &self.bounds)
                        .map_err(layout)?;
                self.shapes.push(shape);
                self.shapes.len() - 1
            }
        };
        let (display, name_at, name_len) =
            record_display(self.memory, descriptor, count, &self.bounds).map_err(layout)?;
        let name = self
            .memory
            .borrow(name_at, name_len)
            .ok_or(EncodeError::Layout)?;
        let name = core::str::from_utf8(&name).map_err(layout)?.to_owned();
        let mut slots: Vec<Option<RawField>> = alloc::vec![None; count];
        let mut order = Vec::with_capacity(count);
        let memory = self.memory;
        let mut missing = false;
        read_record_with(
            memory,
            payload,
            VERSION,
            &self.shapes[shape],
            |entry| match memory.borrow(entry.key, entry.len) {
                Some(key) => {
                    slots[entry.slot] = Some((
                        key.into_owned(),
                        entry.kind as u32,
                        entry.payload,
                        entry.present,
                    ));
                }
                None => missing = true,
            },
            |slot| order.push(slot),
        )
        .map_err(layout)?;
        if missing {
            return Err(EncodeError::Layout);
        }
        let index = |key: &[u8]| -> Option<u32> {
            let text = core::str::from_utf8(key).ok()?;
            let index: u32 = text.parse().ok()?;
            (index != u32::MAX && (text == "0" || !text.starts_with('0'))).then_some(index)
        };
        order.sort_by_key(|slot| {
            let position = slots[*slot].as_ref().and_then(|(key, ..)| index(key));
            (position.is_none(), position.unwrap_or(0))
        });
        let mut fields = Vec::with_capacity(count);
        for slot in order {
            let (key, kind, payload, present) = slots[slot].take().ok_or(EncodeError::Layout)?;
            let read = match wanted {
                Some(wanted) => wanted.contains(&key.as_slice()),
                None => {
                    key.first() != Some(&(PRIVATE_KEY_PREFIX as u8))
                        || STATE.contains(&key.as_slice())
                }
            };
            if !read {
                continue;
            }
            let value = self.resolve(kind, payload)?;
            fields.push(Field {
                key,
                value,
                present,
            });
        }
        Ok(Record {
            display,
            name,
            fields,
        })
    }
    /// A record by its display: an object or class instance, an Error
    /// object, a Map, a Set or a Date.
    fn record_value(&mut self, payload: u64, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        let record = self.record(payload, None)?;
        match record.display {
            RecordDisplay::Object | RecordDisplay::Class => self.object(record, id, recurse),
            RecordDisplay::Error | RecordDisplay::AggregateError => {
                self.error_record(record, id, recurse)
            }
            RecordDisplay::Map | RecordDisplay::Set => self.collection(record, id, recurse),
            RecordDisplay::Date => {
                let time = self.date_time(payload)?;
                Ok(date_text(time))
            }
        }
    }
    /// A `Date`'s time value.
    fn date_time(&mut self, payload: u64) -> Result<f64, EncodeError> {
        let record = self.record(payload, Some(&[DATE_TIME]))?;
        let time = record.field(DATE_TIME).ok_or(EncodeError::Layout)?.value;
        if time.kind != Kind::Float64 {
            return Err(EncodeError::Layout);
        }
        Ok(f64::from_bits(time.payload))
    }
    /// A record's own enumerable properties: present public fields.
    fn public(record: &Record) -> impl Iterator<Item = &Field> {
        record
            .fields
            .iter()
            .filter(|field| field.present && field.key.first() != Some(&(PRIVATE_KEY_PREFIX as u8)))
    }
    fn object(&mut self, record: Record, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        let prefix = match record.display {
            RecordDisplay::Class => record.name.clone() + " ",
            _ => String::new(),
        };
        let fields: Vec<(Text, Value)> = Self::public(&record)
            .map(|field| (utf16(&field.key), field.value))
            .collect();
        if fields.is_empty() {
            return Ok(ascii(&(prefix + "{}")));
        }
        let label = if prefix.is_empty() {
            "Object"
        } else {
            prefix.trim_end()
        };
        self.finish(
            id,
            recurse,
            &ascii(label),
            Text::new(),
            ascii(&(prefix.clone() + "{")),
            |inspector, recurse| {
                let mut output = Vec::with_capacity(fields.len());
                for (name, value) in &fields {
                    output.push(inspector.property(name, false, *value, recurse, false)?);
                }
                Ok((output, Vec::new()))
            },
        )
    }
    /// `formatError`'s text: `Error.prototype.toString` improved with the
    /// constructor name (`improveStack`), in brackets (no stack frames),
    /// with its line breaks indented.
    fn error_base(&self, constructor: &[u16], name: &[u16], message: &[u16]) -> Text {
        let mut stack = improve_stack(error_string(name, message), constructor, name);
        // `formatError` strips no frames: a message line holding the frame
        // marker reads as frames, and the text keeps no brackets.
        let after = if message.is_empty() {
            0
        } else {
            index_of(&stack, message, 0)
                .map_or(0, |at| if at == 0 { 0 } else { at + message.len() })
        };
        if index_of(&stack, &ascii("\n    at"), after).is_none() {
            stack.insert(0, u16::from(b'['));
            stack.push(u16::from(b']'));
        }
        if self.indentation != 0 {
            let mut indented = Text::with_capacity(stack.len());
            for unit in stack {
                indented.push(unit);
                if unit == u16::from(b'\n') {
                    indented.extend(spaces(self.indentation));
                }
            }
            stack = indented;
        }
        stack
    }
    /// `%o` of a native Error: its own `stack` accessor and `message`.
    fn hidden_error_keys(&mut self, message: &[u16]) -> (Vec<Text>, Vec<bool>) {
        let mut output = alloc::vec![ascii("[stack]: [Getter/Setter]")];
        if !message.is_empty() {
            self.indentation += 2;
            let message = self.string_text(message);
            self.indentation -= 2;
            let mut text = ascii("[message]: ");
            text.extend(message);
            output.push(text);
        }
        (output, Vec::new())
    }
    /// An Error class instance: its base, then its public fields, its
    /// `[cause]` and an `AggregateError`'s `[errors]`.
    fn error_record(
        &mut self,
        record: Record,
        id: u32,
        recurse: usize,
    ) -> Result<Text, EncodeError> {
        // An assigned `name` or `message` is an own property that shadows
        // the inherited state.
        let text = |inspector: &Self, own: &[u8], state: &[u8]| -> Result<Text, EncodeError> {
            let field = record
                .field(own)
                .filter(|field| field.present && field.value.kind == Kind::String)
                .or_else(|| record.field(state))
                .ok_or(EncodeError::Layout)?;
            match field.value.kind {
                Kind::String => inspector.string(field.value.payload),
                _ => Err(EncodeError::Layout),
            }
        };
        let name = text(self, &ERROR_NAME[1..], ERROR_NAME)?;
        let message = text(self, &ERROR_MESSAGE[1..], ERROR_MESSAGE)?;
        let aggregate = record.display == RecordDisplay::AggregateError;
        let mut keys: Vec<(Text, bool, Value)> = Vec::new();
        if self.options.show_hidden {
            keys.push((
                ascii("stack"),
                true,
                Value {
                    kind: Kind::Undefined,
                    payload: 0,
                },
            ));
            if !message.is_empty() {
                keys.push((
                    ascii("message"),
                    true,
                    Value {
                        kind: Kind::String,
                        payload: record
                            .field(ERROR_MESSAGE)
                            .ok_or(EncodeError::Layout)?
                            .value
                            .payload,
                    },
                ));
            }
        }
        for field in Self::public(&record) {
            if (aggregate && field.key == AGGREGATE_ERRORS)
                || (!self.options.show_hidden && ERROR_HIDDEN.contains(&field.key.as_slice()))
            {
                continue;
            }
            keys.push((utf16(&field.key), false, field.value));
        }
        if let Some(cause) = record.field(ERROR_CAUSE)
            && cause.value.kind != Kind::Undefined
        {
            keys.push((ascii("cause"), true, cause.value));
        }
        if aggregate
            && let Some(errors) = record
                .field(AGGREGATE_ERRORS)
                .or_else(|| record.field(AGGREGATE_ERRORS_STATE))
        {
            keys.push((utf16(AGGREGATE_ERRORS), true, errors.value));
        }
        let constructor = ascii(&record.name);
        let base = self.error_base(&constructor, &name, &message);
        if keys.is_empty() {
            return Ok(base);
        }
        self.finish(
            id,
            recurse,
            &constructor,
            base,
            ascii("{"),
            |inspector, recurse| {
                let mut output = Vec::with_capacity(keys.len());
                for (key, hidden, value) in &keys {
                    if inspector.options.show_hidden
                        && *hidden
                        && key.as_slice() == ascii("stack").as_slice()
                    {
                        output.push(ascii("[stack]: [Getter/Setter]"));
                        continue;
                    }
                    output.push(inspector.property(key, *hidden, *value, recurse, false)?);
                }
                Ok((output, Vec::new()))
            },
        )
    }
    /// A Map (`Map(n) { k => v, … }`) or Set (`Set(n) { v, … }`) from its
    /// prelude storage: the live entries are the present positions of its
    /// key vector, in insertion order.
    fn collection(&mut self, record: Record, id: u32, recurse: usize) -> Result<Text, EncodeError> {
        let map = record.display == RecordDisplay::Map;
        let vector = |inspector: &Self, name: &[u8]| -> Result<Vector, EncodeError> {
            let field = record.field(name).ok_or(EncodeError::Layout)?;
            if field.value.kind != Kind::Array {
                return Err(EncodeError::Layout);
            }
            Vector::read_in(
                inspector.memory,
                field.value.payload,
                VERSION,
                inspector.bounds,
            )
            .map_err(layout)
        };
        let keys = vector(self, STORE_KEYS)?;
        let values = if map {
            Some(vector(self, STORE_VALUES)?)
        } else {
            None
        };
        let mut entries = Vec::new();
        for at in 0..keys.length {
            let Some(key) = self.element(&keys, at)? else {
                continue;
            };
            let value = match &values {
                Some(values) => Some(self.element(values, at)?.ok_or(EncodeError::Layout)?),
                None => None,
            };
            entries.push((key, value));
        }
        let name = if map { "Map" } else { "Set" };
        let prefix = String::from_utf16_lossy(&concat(&[
            &ascii(name),
            &ascii("("),
            &digits(entries.len() as u64, 1),
            &ascii(") "),
        ]));
        if entries.is_empty() {
            return Ok(ascii(&(prefix + "{}")));
        }
        self.finish(
            id,
            recurse,
            &ascii(name),
            Text::new(),
            ascii(&(prefix.clone() + "{")),
            |inspector, recurse| {
                let length = entries.len().min(inspector.options.max_array_length);
                let mut output = Vec::with_capacity(length + 1);
                inspector.indentation += 2;
                for (key, value) in &entries[..length] {
                    let mut entry = inspector.value(*key, recurse)?;
                    if let Some(value) = value {
                        push(&mut entry, " => ");
                        entry.extend(inspector.value(*value, recurse)?);
                    }
                    output.push(entry);
                }
                inspector.indentation -= 2;
                if entries.len() > length {
                    output.push(remaining_text(entries.len() - length));
                }
                Ok((output, Vec::new()))
            },
        )
    }

    /// `reduceToSingleString`.
    fn reduce(&self, raw: Raw, recurse: usize) -> Text {
        let Raw {
            base,
            braces: [opening, closing],
            array,
            output,
            numeric,
        } = raw;
        let Compact::Levels(compact) = self.options.compact else {
            return self.reduce_single(base, [opening, closing], &output);
        };
        let entries = output.len();
        let output = if compact >= 1 && array && entries > GROUP_ENTRIES {
            self.group(output, &numeric, compact)
        } else {
            output
        };
        let mut text = Text::new();
        if !base.is_empty() {
            text.extend_from_slice(&base);
            text.push(u16::from(b' '));
        }
        if compact >= 1
            && self.current_depth.saturating_sub(recurse) < compact
            && entries == output.len()
        {
            let start = output.len() + self.indentation + opening.len() + base.len() + LINE_START;
            if below_break_length(&output, start, &base, self.options.break_length) {
                let joined = join(&output, &ascii(", "));
                if !joined.contains(&u16::from(b'\n')) {
                    text.extend(opening);
                    text.push(u16::from(b' '));
                    text.extend(joined);
                    text.push(u16::from(b' '));
                    text.extend(closing);
                    return text;
                }
            }
        }
        let mut indentation = alloc::vec![u16::from(b'\n')];
        indentation.extend(spaces(self.indentation));
        text.extend(opening);
        text.extend_from_slice(&indentation);
        push(&mut text, "  ");
        let mut separator = alloc::vec![u16::from(b',')];
        separator.extend_from_slice(&indentation);
        push(&mut separator, "  ");
        text.extend(join(&output, &separator));
        text.extend(indentation);
        text.extend(closing);
        text
    }
    /// `reduceToSingleString` with `compact: true`: entries on one line
    /// within `breakLength`, otherwise one per line after the brace.
    fn reduce_single(&self, base: Text, braces: [Text; 2], output: &[Text]) -> Text {
        let [opening, closing] = braces;
        let mut text = opening.clone();
        if below_break_length(output, 0, &base, self.options.break_length) {
            if !base.is_empty() {
                text.push(u16::from(b' '));
                text.extend_from_slice(&base);
            }
            text.push(u16::from(b' '));
            text.extend(join(output, &ascii(", ")));
            text.push(u16::from(b' '));
            text.extend(closing);
            return text;
        }
        let indentation = spaces(self.indentation);
        if base.is_empty() && opening.len() == 1 {
            text.push(u16::from(b' '));
        } else {
            if !base.is_empty() {
                text.push(u16::from(b' '));
                text.extend_from_slice(&base);
            }
            text.push(u16::from(b'\n'));
            text.extend_from_slice(&indentation);
            push(&mut text, "  ");
        }
        let mut separator = ascii(",\n");
        separator.extend_from_slice(&indentation);
        push(&mut separator, "  ");
        text.extend(join(output, &separator));
        text.push(u16::from(b' '));
        text.extend(closing);
        text
    }
    /// `groupArrayElements`: more than six entries in aligned columns when
    /// at least three fit side by side and no entry dominates.
    fn group(&self, output: Vec<Text>, numeric: &[bool], compact: usize) -> Vec<Text> {
        let break_length = self.options.break_length;
        let mut total = 0usize;
        let mut max = 0usize;
        let mut length = output.len();
        if self.options.max_array_length < output.len() {
            // The "... more items" entry stays out of the columns.
            length -= 1;
        }
        let lengths: Vec<usize> = output[..length].iter().map(|entry| width(entry)).collect();
        for len in &lengths {
            total += len + SEPARATOR_SPACE;
            max = max.max(*len);
        }
        let actual_max = max + SEPARATOR_SPACE;
        if !((actual_max * 3).saturating_add(self.indentation) < break_length
            && (total as f64 / actual_max as f64 > 5.0 || max <= 6))
        {
            return output;
        }
        let average_bias = libm::sqrt(actual_max as f64 - total as f64 / output.len() as f64);
        let biased_max = (actual_max as f64 - 3.0 - average_bias).max(1.0);
        let columns = [
            libm::floor(libm::sqrt(CHAR_HEIGHTS * biased_max * length as f64) / biased_max + 0.5),
            libm::floor(break_length.saturating_sub(self.indentation) as f64 / actual_max as f64),
            (compact * 4) as f64,
            MAX_COLUMNS as f64,
        ]
        .into_iter()
        .fold(f64::INFINITY, f64::min) as usize;
        if columns <= 1 {
            return output;
        }
        let line_widths: Vec<usize> = (0..columns)
            .map(|column| {
                (column..length)
                    .step_by(columns)
                    .map(|at| lengths[at])
                    .max()
                    .unwrap_or(0)
                    + SEPARATOR_SPACE
            })
            .collect();
        let pad_left = (0..output.len()).all(|at| numeric.get(at).copied().unwrap_or(false));
        let mut grouped = Vec::new();
        for start in (0..length).step_by(columns) {
            let end = (start + columns).min(length);
            let mut line = Text::new();
            for at in start..end - 1 {
                let mut entry = output[at].clone();
                push(&mut entry, ", ");
                let padding = line_widths[at - start] + output[at].len() - lengths[at];
                line.extend(if pad_left {
                    pad_start(&entry, padding)
                } else {
                    pad_end(&entry, padding)
                });
            }
            let last = end - 1;
            if pad_left {
                let padding = line_widths[last - start] + output[last].len()
                    - lengths[last]
                    - SEPARATOR_SPACE;
                line.extend(pad_start(&output[last], padding));
            } else {
                line.extend_from_slice(&output[last]);
            }
            grouped.push(line);
        }
        if self.options.max_array_length < output.len() {
            grouped.push(output[length].clone());
        }
        grouped
    }
}

/// How `%d`, `%i` and `%f` convert their argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Numeric {
    Number,
    Integer,
    Float,
}

fn utf16(bytes: &[u8]) -> Text {
    String::from_utf8_lossy(bytes).encode_utf16().collect()
}
/// `addNumericSeparator`: `_` before every group of three integer digits.
fn numeric_separators(integer: &[u16]) -> Text {
    let start = usize::from(integer.first() == Some(&u16::from(b'-')));
    let mut groups: Vec<&[u16]> = Vec::new();
    let mut at = integer.len();
    while at >= start + 4 {
        groups.push(&integer[at - 3..at]);
        at -= 3;
    }
    if at == integer.len() {
        return integer.to_vec();
    }
    let mut text = integer[..at].to_vec();
    for group in groups.iter().rev() {
        text.push(u16::from(b'_'));
        text.extend_from_slice(group);
    }
    text
}
/// `addNumericSeparatorEnd`: `_` after every group of three fraction
/// digits but the last.
fn numeric_separators_end(fraction: &[u16]) -> Text {
    let mut text = Text::new();
    let mut at = 0;
    while at + 3 < fraction.len() {
        text.extend_from_slice(&fraction[at..at + 3]);
        text.push(u16::from(b'_'));
        at += 3;
    }
    text.extend_from_slice(&fraction[at..]);
    text
}
/// `isBelowBreakLength`.
fn below_break_length(output: &[Text], start: usize, base: &[u16], break_length: usize) -> bool {
    let mut total = output.len() + start;
    if total + output.len() > break_length {
        return false;
    }
    for entry in output {
        total += entry.len();
        if total > break_length {
            return false;
        }
    }
    base.is_empty() || !base.contains(&u16::from(b'\n'))
}
/// `Error.prototype.toString`.
fn error_string(name: &[u16], message: &[u16]) -> Text {
    if name.is_empty() {
        return message.to_vec();
    }
    if message.is_empty() {
        return name.to_vec();
    }
    let mut text = name.to_vec();
    push(&mut text, ": ");
    text.extend_from_slice(message);
    text
}
/// `improveStack`: a stack starting with an Error name the constructor does
/// not contain names the constructor (`Plain [Error]: …`).
fn improve_stack(stack: Text, constructor: &[u16], name: &[u16]) -> Text {
    let len = name.len();
    let ends_with_error = name.ends_with(&ascii("Error"));
    let starts = stack.starts_with(name)
        && (stack.len() == len || stack[len] == u16::from(b':') || stack[len] == u16::from(b'\n'));
    if !(ends_with_error && starts) || name == constructor {
        return stack;
    }
    let mut text = constructor.to_vec();
    if contains(constructor, name) {
        if len == 0 {
            push(&mut text, ": ");
            text.extend(stack);
        } else {
            text.extend_from_slice(&stack[len..]);
        }
    } else {
        push(&mut text, " [");
        text.extend_from_slice(name);
        text.push(u16::from(b']'));
        text.extend_from_slice(&stack[len..]);
    }
    text
}
/// `Date.prototype.toISOString`, or `Invalid Date`.
fn date_text(time: f64) -> Text {
    if time.is_nan() {
        return ascii("Invalid Date");
    }
    let days = libm::floor(time / MS_PER_DAY);
    let within = (time - days * MS_PER_DAY) as i64;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let year = if (0..=9999).contains(&year) {
        digits(year as u64, 4)
    } else if year < 0 {
        concat(&[&ascii("-"), &digits((-year) as u64, 6)])
    } else {
        concat(&[&ascii("+"), &digits(year as u64, 6)])
    };
    let (hours, rest) = (within / 3_600_000, within % 3_600_000);
    let (minutes, rest) = (rest / 60_000, rest % 60_000);
    let (seconds, millis) = (rest / 1000, rest % 1000);
    concat(&[
        &year,
        &ascii("-"),
        &digits(month as u64, 2),
        &ascii("-"),
        &digits(day as u64, 2),
        &ascii("T"),
        &digits(hours as u64, 2),
        &ascii(":"),
        &digits(minutes as u64, 2),
        &ascii(":"),
        &digits(seconds as u64, 2),
        &ascii("."),
        &digits(millis as u64, 3),
        &ascii("Z"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_quote_and_escape_as_node() {
        let text = |s: &str| String::from_utf16(&quoted(&ascii(s))).unwrap();
        assert_eq!(text("a"), "'a'");
        assert_eq!(text("it's"), "\"it's\"");
        assert_eq!(text("it's \"x\""), "`it's \"x\"`");
        assert_eq!(text("it's \"x\" `y`"), "'it\\'s \"x\" `y`'");
        assert_eq!(text("a\nb\t\u{7f}\\"), "'a\\nb\\t\\x7F\\\\'");
        assert_eq!(String::from_utf16(&quoted(&[0xD800])).unwrap(), "'\\ud800'");
        assert_eq!(String::from_utf16(&key(&ascii("a-b"))).unwrap(), "'a-b'");
        assert_eq!(String::from_utf16(&key(&ascii("_a1"))).unwrap(), "_a1");
        assert_eq!(String::from_utf16(&key(&ascii("$a"))).unwrap(), "'$a'");
    }

    #[test]
    fn dates_print_their_iso_text() {
        let text = |t: f64| String::from_utf16(&date_text(t)).unwrap();
        assert_eq!(text(0.0), "1970-01-01T00:00:00.000Z");
        assert_eq!(text(-1.0), "1969-12-31T23:59:59.999Z");
        assert_eq!(text(8.64e15), "+275760-09-13T00:00:00.000Z");
        assert_eq!(text(-62_198_755_200_000.0), "-000001-01-01T00:00:00.000Z");
        assert_eq!(text(f64::NAN), "Invalid Date");
    }

    #[test]
    fn error_stacks_name_their_constructor() {
        let text = |c: &str, n: &str| {
            String::from_utf16(&improve_stack(
                error_string(&ascii(n), &ascii("m")),
                &ascii(c),
                &ascii(n),
            ))
            .unwrap()
        };
        assert_eq!(text("Plain", "Error"), "Plain [Error]: m");
        assert_eq!(text("MyError", "MyError"), "MyError: m");
        assert_eq!(text("TypeError", "TypeError"), "TypeError: m");
        assert_eq!(text("SpecialTypeError", "TypeError"), "SpecialTypeError: m");
        assert_eq!(text("Error", "Custom"), "Custom: m");
    }
}
