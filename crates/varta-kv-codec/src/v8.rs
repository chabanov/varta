//! Strict V8 `ValueSerializer` codec of Deno KV values (v38 `kv`).
//!
//! Deno KV stores a value written by `Deno.Kv` with `v_encoding` 1 as the
//! bytes of V8's `ValueSerializer` (`Deno.core.serialize(value, { forStorage:
//! true })`): the version header and one value, with no host-object or
//! wrapper framing. This module reads and writes exactly the subset of that
//! wire format the value kinds Deno KV stores can produce, as V8 15.0 (Deno
//! 2.9.4 and 2.9.7) chooses it:
//!
//! - numbers: an integral value in the 32-bit Smi range other than `-0` as
//!   `I` (zigzag varint), every other number as `N` (binary64, NaN written as
//!   the canonical quiet NaN); both read as a number;
//! - strings: `"` (Latin-1 bytes) when every code unit is at most 0xFF,
//!   otherwise `c` (UTF-16LE, lone surrogates kept) preceded by one padding
//!   byte when the code units would otherwise start at an odd offset; `S`
//!   (UTF-8) is read too;
//! - objects (`o`…`{`): own properties in V8's order, array-index keys as
//!   numbers (`I`, or `N` above the Smi range) ascending before the other
//!   keys in insertion order;
//! - arrays: dense (`A`…`$`) when every element is present, sparse
//!   (`a`…`@`) otherwise; named array properties are rejected;
//! - `undefined`, `null`, booleans, BigInt (`Z`), `Date` (`D`), `Map`
//!   (`;`…`:`), `Set` (`'`…`,`) and `Uint8Array` over its own `ArrayBuffer`
//!   (`B` then `V` with subtag `B`);
//! - back-references (`^`) to completed objects; a reference to an object
//!   still being read is a cycle and is rejected. A shared object keeps the
//!   depth limit at its deepest reach and counts toward the value's
//!   [`crate::Extent`] at every reach, read or written, as an expanding
//!   reader sees it.
//!
//! Every other tag (other typed arrays, standalone or shared buffers,
//! primitive wrappers, `RegExp`, errors, host objects, transfers), a
//! truncated or overlong field, a non-canonical BigInt, a duplicate key and
//! trailing bytes fail with a typed [`V8Error`]; a failed read yields no
//! partial value.
use alloc::{collections::BTreeMap, rc::Rc, vec::Vec};
use num_bigint::{BigInt, Sign};

/// The serializer version Deno 2.9.x (V8 15.0) writes.
pub const WRITE_VERSION: u32 = 16;
/// The oldest and newest serializer versions read. Deno KV files written by
/// earlier Deno releases carry earlier headers of the same subset.
pub const MIN_READ_VERSION: u32 = 13;
pub const MAX_READ_VERSION: u32 = 16;
/// Array buffer views carry a flags field from this version on.
const VIEW_FLAGS_VERSION: u32 = 14;
/// The largest array index (`2^32 - 2`).
const MAX_ARRAY_INDEX: u32 = u32::MAX - 1;
/// The canonical quiet NaN V8 stores.
const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;
/// Bytes of one BigInt digit on the wire.
const DIGIT_BYTES: usize = 8;

/// Wire tags (`v8::internal::SerializationTag`).
mod tag {
    pub const VERSION: u8 = 0xFF;
    pub const PADDING: u8 = 0x00;
    pub const VERIFY_OBJECT_COUNT: u8 = b'?';
    pub const THE_HOLE: u8 = b'-';
    pub const UNDEFINED: u8 = b'_';
    pub const NULL: u8 = b'0';
    pub const TRUE: u8 = b'T';
    pub const FALSE: u8 = b'F';
    pub const INT32: u8 = b'I';
    pub const UINT32: u8 = b'U';
    pub const DOUBLE: u8 = b'N';
    pub const BIGINT: u8 = b'Z';
    pub const UTF8_STRING: u8 = b'S';
    pub const ONE_BYTE_STRING: u8 = b'"';
    pub const TWO_BYTE_STRING: u8 = b'c';
    pub const OBJECT_REFERENCE: u8 = b'^';
    pub const BEGIN_OBJECT: u8 = b'o';
    pub const END_OBJECT: u8 = b'{';
    pub const BEGIN_SPARSE_ARRAY: u8 = b'a';
    pub const END_SPARSE_ARRAY: u8 = b'@';
    pub const BEGIN_DENSE_ARRAY: u8 = b'A';
    pub const END_DENSE_ARRAY: u8 = b'$';
    pub const DATE: u8 = b'D';
    pub const BEGIN_MAP: u8 = b';';
    pub const END_MAP: u8 = b':';
    pub const BEGIN_SET: u8 = b'\'';
    pub const END_SET: u8 = b',';
    pub const ARRAY_BUFFER: u8 = b'B';
    pub const ARRAY_BUFFER_VIEW: u8 = b'V';
    /// The `Uint8Array` subtag of an array buffer view.
    pub const UINT8_ARRAY: u8 = b'B';
}

/// The tags V8 can write that this subset rejects, with their kinds.
const UNSUPPORTED: [(u8, &str); 15] = [
    (b'y', "Boolean object"),
    (b'x', "Boolean object"),
    (b'n', "Number object"),
    (b'z', "BigInt object"),
    (b's', "String object"),
    (b'R', "RegExp"),
    (b'r', "Error"),
    (b'~', "resizable ArrayBuffer"),
    (b't', "transferred ArrayBuffer"),
    (b'u', "SharedArrayBuffer"),
    (b'p', "shared object"),
    (b'w', "WebAssembly module"),
    (b'm', "WebAssembly memory"),
    (b'\\', "host object"),
    (tag::VERIFY_OBJECT_COUNT, "object count check"),
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum V8Error {
    #[error("the value ends early")]
    Truncated,
    #[error("unsupported serializer version {0}")]
    Version(u32),
    #[error("missing serializer version header")]
    Header,
    #[error("unknown tag 0x{0:02x}")]
    UnknownTag(u8),
    #[error("unsupported value kind: {0}")]
    Unsupported(&'static str),
    #[error("malformed {0}")]
    Malformed(&'static str),
    #[error("trailing bytes after the value")]
    Trailing,
    #[error("a back-reference to an object that encloses it (cycles are not supported)")]
    Cycle,
    #[error("duplicate property key")]
    DuplicateKey,
    #[error("the value exceeds the {0} limit")]
    Limit(&'static str),
    /// Nesting below the depth limit ([`Limits::depth`], at most
    /// [`crate::MAX_VALUE_DEPTH`]); the text is [`crate::too_deep`]'s form.
    #[error("a KV value cannot nest deeper than {0} levels")]
    Depth(usize),
    /// An [`crate::Extent`] beyond [`crate::MAX_VALUE_NODES`] values or
    /// [`crate::MAX_VALUE_UNITS`] code units; the text is
    /// [`crate::too_expanded`]'s.
    #[error(
        "a KV value cannot expand to more than {} values or {} code units of text",
        crate::MAX_VALUE_NODES,
        crate::MAX_VALUE_UNITS
    )]
    Expanded,
}

/// A property key: an array index or any other string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Index(u32),
    Name(Vec<u16>),
}

impl Key {
    /// The canonical key of `units`: an array index when the text spells one.
    pub fn of(units: Vec<u16>) -> Self {
        match varta_common::value::array_index(&units) {
            Some(index) => Self::Index(index as u32),
            None => Self::Name(units),
        }
    }

    /// The code units of the key's text (an index as its decimal digits),
    /// as a value's [`crate::Extent`] counts it.
    #[must_use]
    pub fn units(&self) -> usize {
        match self {
            Self::Index(index) => crate::index_units(*index),
            Self::Name(units) => units.len(),
        }
    }
}

/// A JavaScript value of the stored subset.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    BigInt(BigInt),
    /// UTF-16 code units, lone surrogates included.
    String(Vec<u16>),
    /// An object; shared (`Rc`) references are one object written once and
    /// back-referenced after.
    Object(Rc<Object>),
}

/// The object kinds of the stored subset.
#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    /// Own enumerable properties in V8 order (indices first, ascending).
    Plain(Vec<(Key, Value)>),
    /// Every element present (`None` only for a hole V8 wrote densely).
    Dense(Vec<Option<Value>>),
    /// A holey array: its length and its present elements, ascending.
    Sparse {
        length: u32,
        elements: Vec<(u32, Value)>,
    },
    /// A `Date` time value.
    Date(f64),
    Map(Vec<(Value, Value)>),
    Set(Vec<Value>),
    /// A `Uint8Array` viewing `length` bytes at `offset` of its own buffer.
    Bytes {
        buffer: Vec<u8>,
        offset: u32,
        length: u32,
    },
}

impl Object {
    /// The kind name used in diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Plain(_) => "object",
            Self::Dense(_) | Self::Sparse { .. } => "array",
            Self::Date(_) => "Date",
            Self::Map(_) => "Map",
            Self::Set(_) => "Set",
            Self::Bytes { .. } => "Uint8Array",
        }
    }
}

/// Structural bounds of one read.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Nesting depth of objects; a read never goes below
    /// [`crate::MAX_VALUE_DEPTH`], whatever this says.
    pub depth: usize,
    /// Values read, back-references included.
    pub nodes: usize,
    /// Elements of one array (its length, holes included), entries of one
    /// object, `Map` or `Set`, and bytes of one `Uint8Array`.
    pub elements: usize,
    /// Code units of one string.
    pub string_units: usize,
}

/// Decode one stored value; the whole input must be exactly one value.
pub fn decode(bytes: &[u8], limits: Limits) -> Result<Value, V8Error> {
    let mut reader = Reader {
        bytes,
        at: 0,
        version: 0,
        objects: Vec::new(),
        nodes: 0,
        deepest: 0,
        extent: crate::Extent::default(),
        limits,
    };
    if reader.byte()? != tag::VERSION {
        return Err(V8Error::Header);
    }
    let version = reader.varint()?;
    if !(MIN_READ_VERSION..=MAX_READ_VERSION).contains(&version) {
        return Err(V8Error::Version(version));
    }
    reader.version = version;
    let value = reader.value(0)?;
    if reader.at != bytes.len() {
        return Err(V8Error::Trailing);
    }
    Ok(value)
}

/// An object id of the input.
enum Slot {
    /// Being read: a reference to it is a cycle.
    Open,
    /// A complete object, its height (the levels below it) and the extent
    /// below it.
    Done(Rc<Object>, usize, crate::Extent),
    /// The buffer of a `Uint8Array`, which has no value of its own here.
    Buffer,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    version: u32,
    /// Every object by its serializer id.
    objects: Vec<Slot>,
    nodes: usize,
    /// The deepest level reached inside the object being read.
    deepest: usize,
    /// The extent of the value read so far, shared objects at every reach.
    extent: crate::Extent,
    limits: Limits,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, V8Error> {
        let byte = *self.bytes.get(self.at).ok_or(V8Error::Truncated)?;
        self.at += 1;
        Ok(byte)
    }
    fn take(&mut self, len: usize) -> Result<&[u8], V8Error> {
        let end = self.at.checked_add(len).ok_or(V8Error::Truncated)?;
        let bytes = self.bytes.get(self.at..end).ok_or(V8Error::Truncated)?;
        self.at = end;
        Ok(bytes)
    }
    /// A base-128 varint of at most 32 bits, without overlong groups.
    fn varint(&mut self) -> Result<u32, V8Error> {
        let mut value = 0u64;
        for group in 0..5 {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7F) << (7 * group);
            if byte & 0x80 == 0 {
                if group > 0 && byte == 0 {
                    return Err(V8Error::Malformed("varint"));
                }
                return u32::try_from(value).map_err(|_| V8Error::Malformed("varint"));
            }
        }
        Err(V8Error::Malformed("varint"))
    }
    fn double(&mut self) -> Result<f64, V8Error> {
        let bytes = self.take(8)?;
        let mut word = [0; 8];
        word.copy_from_slice(bytes);
        Ok(f64::from_le_bytes(word))
    }
    /// The next tag, skipping padding as V8 does.
    fn tag(&mut self) -> Result<u8, V8Error> {
        loop {
            let tag = self.byte()?;
            if tag != tag::PADDING {
                return Ok(tag);
            }
        }
    }
    fn peek_tag(&self) -> Result<u8, V8Error> {
        self.bytes[self.at..]
            .iter()
            .copied()
            .find(|byte| *byte != tag::PADDING)
            .ok_or(V8Error::Truncated)
    }
    fn length(&mut self, what: &'static str, limit: usize) -> Result<usize, V8Error> {
        let length = self.varint()? as usize;
        if length > limit {
            return Err(V8Error::Limit(what));
        }
        Ok(length)
    }
    fn visit(&mut self, depth: usize) -> Result<(), V8Error> {
        self.nodes += 1;
        if self.nodes > self.limits.nodes {
            return Err(V8Error::Limit("node"));
        }
        self.reach(depth)
    }
    /// Whether a value may sit at level `depth`.
    fn reach(&mut self, depth: usize) -> Result<(), V8Error> {
        let limit = self.limits.depth.min(crate::MAX_VALUE_DEPTH);
        if depth > limit {
            return Err(V8Error::Depth(limit));
        }
        self.deepest = self.deepest.max(depth);
        Ok(())
    }
    /// Count `values` and `units` toward the value's extent.
    fn grow(&mut self, values: usize, units: usize) -> Result<(), V8Error> {
        if self.extent.grow(values, units) {
            Ok(())
        } else {
            Err(V8Error::Expanded)
        }
    }
    /// A value: an element, a property's value, a `Map` key or value or a
    /// `Set` value, one value of the extent (and a string's code units).
    fn value(&mut self, depth: usize) -> Result<Value, V8Error> {
        self.visit(depth)?;
        self.grow(1, 0)?;
        let value = self.item(depth)?;
        if let Value::String(units) = &value {
            self.grow(0, units.len())?;
        }
        Ok(value)
    }
    /// The value at the next tag, not itself one value of the extent (a
    /// property key, whose text its caller counts).
    fn item(&mut self, depth: usize) -> Result<Value, V8Error> {
        let tag = self.tag()?;
        Ok(match tag {
            tag::UNDEFINED => Value::Undefined,
            tag::NULL => Value::Null,
            tag::TRUE => Value::Boolean(true),
            tag::FALSE => Value::Boolean(false),
            tag::INT32 => Value::Number(f64::from(zigzag(self.varint()?))),
            tag::UINT32 => Value::Number(f64::from(self.varint()?)),
            tag::DOUBLE => Value::Number(self.double()?),
            tag::BIGINT => Value::BigInt(self.bigint()?),
            tag::ONE_BYTE_STRING | tag::TWO_BYTE_STRING | tag::UTF8_STRING => {
                Value::String(self.string(tag)?)
            }
            tag::OBJECT_REFERENCE => {
                let id = self.varint()? as usize;
                match self.objects.get(id) {
                    Some(Slot::Done(object, height, below)) => {
                        let (object, height, below) = (Rc::clone(object), *height, *below);
                        self.reach(depth + height)?;
                        self.grow(below.values, below.units)?;
                        Value::Object(object)
                    }
                    Some(Slot::Open) => return Err(V8Error::Cycle),
                    Some(Slot::Buffer) => {
                        return Err(V8Error::Unsupported("shared ArrayBuffer of views"));
                    }
                    None => return Err(V8Error::Malformed("object reference")),
                }
            }
            tag::BEGIN_OBJECT
            | tag::BEGIN_DENSE_ARRAY
            | tag::BEGIN_SPARSE_ARRAY
            | tag::DATE
            | tag::BEGIN_MAP
            | tag::BEGIN_SET
            | tag::ARRAY_BUFFER => Value::Object(self.object(tag, depth)?),
            tag::THE_HOLE | tag::ARRAY_BUFFER_VIEW => {
                return Err(V8Error::Malformed("value position"));
            }
            other => {
                return Err(UNSUPPORTED
                    .iter()
                    .find(|(tag, _)| *tag == other)
                    .map_or(V8Error::UnknownTag(other), |(_, kind)| {
                        V8Error::Unsupported(kind)
                    }));
            }
        })
    }
    fn bigint(&mut self) -> Result<BigInt, V8Error> {
        let bitfield = self.varint()?;
        let negative = bitfield & 1 == 1;
        let length = (bitfield >> 1) as usize;
        if !length.is_multiple_of(DIGIT_BYTES) {
            return Err(V8Error::Malformed("BigInt"));
        }
        let digits = self.take(length)?;
        // Canonical: a nonzero top digit, and zero is never negative.
        if length > 0 && digits[length - DIGIT_BYTES..].iter().all(|b| *b == 0)
            || length == 0 && negative
        {
            return Err(V8Error::Malformed("BigInt"));
        }
        let sign = match (length, negative) {
            (0, _) => Sign::NoSign,
            (_, true) => Sign::Minus,
            (_, false) => Sign::Plus,
        };
        Ok(BigInt::from_bytes_le(sign, digits))
    }
    fn string(&mut self, tag: u8) -> Result<Vec<u16>, V8Error> {
        let max_bytes = self.limits.string_units.saturating_mul(2);
        match tag {
            tag::ONE_BYTE_STRING => {
                let length = self.length("string", self.limits.string_units)?;
                Ok(self.take(length)?.iter().map(|b| u16::from(*b)).collect())
            }
            tag::TWO_BYTE_STRING => {
                let length = self.length("string", max_bytes)?;
                if length % 2 != 0 {
                    return Err(V8Error::Malformed("two-byte string"));
                }
                Ok(self
                    .take(length)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .collect())
            }
            _ => {
                let length = self.length("string", max_bytes)?;
                let text = core::str::from_utf8(self.take(length)?)
                    .map_err(|_| V8Error::Malformed("UTF-8 string"))?;
                let units: Vec<u16> = text.encode_utf16().collect();
                if units.len() > self.limits.string_units {
                    return Err(V8Error::Limit("string"));
                }
                Ok(units)
            }
        }
    }
    /// A property key: a string, or an array index written as a number.
    fn key(&mut self, depth: usize) -> Result<Key, V8Error> {
        self.visit(depth)?;
        match self.item(depth)? {
            Value::String(units) => Ok(Key::of(units)),
            Value::Number(number) => index_of(number).map(Key::Index),
            _ => Err(V8Error::Malformed("property key")),
        }
    }
    /// Read `key: value` pairs until `end`; returns them and checks the
    /// trailing property count. A plain object's keys (`text`) count toward
    /// the extent, as the text a read builds; an array's keys are indices.
    fn properties(
        &mut self,
        end: u8,
        depth: usize,
        text: bool,
    ) -> Result<Vec<(Key, Value)>, V8Error> {
        let mut properties = Vec::new();
        let mut seen = alloc::collections::BTreeSet::new();
        while self.peek_tag()? != end {
            if properties.len() == self.limits.elements {
                return Err(V8Error::Limit("entry"));
            }
            let key = self.key(depth + 1)?;
            if text {
                self.grow(0, key.units())?;
            }
            let value = self.value(depth + 1)?;
            if !seen.insert(key.clone()) {
                return Err(V8Error::DuplicateKey);
            }
            properties.push((key, value));
        }
        self.tag()?;
        if self.varint()? as usize != properties.len() {
            return Err(V8Error::Malformed("property count"));
        }
        Ok(properties)
    }
    /// Read an object begun by `tag`, registering its id first as V8 does,
    /// with its height once complete.
    fn object(&mut self, tag: u8, depth: usize) -> Result<Rc<Object>, V8Error> {
        let outer = core::mem::replace(&mut self.deepest, depth);
        let object = self.contents(tag, depth)?;
        self.deepest = self.deepest.max(outer);
        Ok(object)
    }
    fn contents(&mut self, tag: u8, depth: usize) -> Result<Rc<Object>, V8Error> {
        let id = self.objects.len();
        self.objects.push(Slot::Open);
        let start = self.extent;
        let object = match tag {
            tag::BEGIN_OBJECT => Object::Plain(self.properties(tag::END_OBJECT, depth, true)?),
            tag::BEGIN_DENSE_ARRAY => self.dense(depth)?,
            tag::BEGIN_SPARSE_ARRAY => self.sparse(depth)?,
            tag::DATE => Object::Date(self.double()?),
            tag::BEGIN_MAP => {
                let mut entries = Vec::new();
                while self.peek_tag()? != tag::END_MAP {
                    if entries.len() == self.limits.elements {
                        return Err(V8Error::Limit("entry"));
                    }
                    let key = self.value(depth + 1)?;
                    entries.push((key, self.value(depth + 1)?));
                }
                self.tag()?;
                if self.varint()? as usize != entries.len() * 2 {
                    return Err(V8Error::Malformed("Map length"));
                }
                Object::Map(entries)
            }
            tag::BEGIN_SET => {
                let mut values = Vec::new();
                while self.peek_tag()? != tag::END_SET {
                    if values.len() == self.limits.elements {
                        return Err(V8Error::Limit("entry"));
                    }
                    values.push(self.value(depth + 1)?);
                }
                self.tag()?;
                if self.varint()? as usize != values.len() {
                    return Err(V8Error::Malformed("Set length"));
                }
                Object::Set(values)
            }
            _ => {
                self.objects[id] = Slot::Buffer;
                return self.bytes_view();
            }
        };
        let object = Rc::new(object);
        self.objects[id] = Slot::Done(
            Rc::clone(&object),
            self.deepest - depth,
            self.extent.since(start),
        );
        Ok(object)
    }
    fn dense(&mut self, depth: usize) -> Result<Object, V8Error> {
        let length = self.length("array", self.limits.elements)?;
        // Every element takes at least one byte.
        if length > self.bytes.len() - self.at {
            return Err(V8Error::Truncated);
        }
        let mut elements = Vec::with_capacity(length);
        for _ in 0..length {
            if self.peek_tag()? == tag::THE_HOLE {
                self.visit(depth + 1)?;
                self.tag()?;
                elements.push(None);
            } else {
                elements.push(Some(self.value(depth + 1)?));
            }
        }
        if !self
            .properties(tag::END_DENSE_ARRAY, depth, false)?
            .is_empty()
        {
            return Err(V8Error::Unsupported("array with named properties"));
        }
        if self.varint()? as usize != length {
            return Err(V8Error::Malformed("array length"));
        }
        Ok(Object::Dense(elements))
    }
    fn sparse(&mut self, depth: usize) -> Result<Object, V8Error> {
        let length = self.length("array", self.limits.elements)?;
        let mut elements = Vec::new();
        for (key, value) in self.properties(tag::END_SPARSE_ARRAY, depth, false)? {
            match key {
                Key::Index(index) if (index as usize) < length => elements.push((index, value)),
                Key::Index(_) => return Err(V8Error::Malformed("array index")),
                Key::Name(_) => return Err(V8Error::Unsupported("array with named properties")),
            }
        }
        if self.varint()? as usize != length {
            return Err(V8Error::Malformed("array length"));
        }
        elements.sort_by_key(|(index, _)| *index);
        Ok(Object::Sparse {
            length: length as u32,
            elements,
        })
    }
    /// An `ArrayBuffer` that a `Uint8Array` view over it must follow at
    /// once; the view takes the next id.
    fn bytes_view(&mut self) -> Result<Rc<Object>, V8Error> {
        let length = self.length("Uint8Array", self.limits.elements)?;
        let buffer = self.take(length)?.to_vec();
        if self.peek_tag()? != tag::ARRAY_BUFFER_VIEW {
            return Err(V8Error::Unsupported(
                "ArrayBuffer without a Uint8Array view",
            ));
        }
        self.tag()?;
        let view_id = self.objects.len();
        self.objects.push(Slot::Open);
        if self.byte()? != tag::UINT8_ARRAY {
            return Err(V8Error::Unsupported("typed array other than Uint8Array"));
        }
        let offset = self.varint()?;
        let view = self.varint()?;
        if self.version >= VIEW_FLAGS_VERSION && self.varint()? != 0 {
            return Err(V8Error::Unsupported("length-tracking or resizable view"));
        }
        if u64::from(offset) + u64::from(view) > buffer.len() as u64 {
            return Err(V8Error::Malformed("array buffer view range"));
        }
        let object = Rc::new(Object::Bytes {
            buffer,
            offset,
            length: view,
        });
        // A view holds no values: nothing below it.
        self.objects[view_id] = Slot::Done(Rc::clone(&object), 0, crate::Extent::default());
        Ok(object)
    }
}

fn zigzag(value: u32) -> i32 {
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

/// The array index a numeric key names (V8 writes only array indices as
/// numeric keys).
fn index_of(number: f64) -> Result<u32, V8Error> {
    if crate::integral(number)
        && (0.0..=f64::from(MAX_ARRAY_INDEX)).contains(&number)
        && !(number == 0.0 && number.is_sign_negative())
    {
        Ok(number as u32)
    } else {
        Err(V8Error::Malformed("numeric property key"))
    }
}

/// Encode `value` with the serializer header Deno 2.9.x writes. Objects
/// shared by `Rc` are written once and back-referenced after, as V8 writes
/// one object reached twice. Fails beyond `max_bytes` of output, below
/// [`crate::MAX_VALUE_DEPTH`] levels, a shared object at its deepest reach,
/// and then, once the whole value fits `max_bytes`, beyond the bounds of a
/// [`crate::Extent`], a shared object counted at every reach (the writer
/// never expands one, so it need not stop early).
pub fn encode(value: &Value, max_bytes: usize) -> Result<Vec<u8>, V8Error> {
    let mut writer = Writer {
        out: Vec::new(),
        ids: BTreeMap::new(),
        written: BTreeMap::new(),
        deepest: 0,
        extent: crate::Extent::default(),
        next: 0,
        max_bytes,
    };
    writer.out.push(tag::VERSION);
    writer.varint(WRITE_VERSION);
    writer.value(value, 0)?;
    writer.check()?;
    if !writer.extent.within() {
        return Err(V8Error::Expanded);
    }
    Ok(writer.out)
}

struct Writer {
    out: Vec<u8>,
    ids: BTreeMap<*const Object, u32>,
    /// The height (levels below it) and the extent below every object
    /// written, checked again at each back-reference to it.
    written: BTreeMap<*const Object, (usize, crate::Extent)>,
    /// The deepest level reached inside the object being written.
    deepest: usize,
    /// The extent of the value written so far, shared objects at every
    /// reach.
    extent: crate::Extent,
    next: u32,
    max_bytes: usize,
}

impl Writer {
    /// Whether a value may sit at level `depth`.
    fn reach(&mut self, depth: usize) -> Result<(), V8Error> {
        if depth > crate::MAX_VALUE_DEPTH {
            return Err(V8Error::Depth(crate::MAX_VALUE_DEPTH));
        }
        self.deepest = self.deepest.max(depth);
        Ok(())
    }
    /// Count `values` and `units` toward the value's extent.
    fn count(&mut self, values: usize, units: usize) {
        self.extent.add(values, units);
    }
    fn check(&self) -> Result<(), V8Error> {
        if self.out.len() > self.max_bytes {
            return Err(V8Error::Limit("value size"));
        }
        Ok(())
    }
    fn varint(&mut self, mut value: u32) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                self.out.push(byte);
                return;
            }
            self.out.push(byte | 0x80);
        }
    }
    fn number(&mut self, number: f64) {
        if crate::integral(number)
            && number >= f64::from(i32::MIN)
            && number <= f64::from(i32::MAX)
            && !(number == 0.0 && number.is_sign_negative())
        {
            let int = number as i32;
            self.out.push(tag::INT32);
            self.varint(((int << 1) ^ (int >> 31)) as u32);
        } else {
            self.out.push(tag::DOUBLE);
            let bits = if number.is_nan() {
                CANONICAL_NAN
            } else {
                number.to_bits()
            };
            self.out.extend_from_slice(&bits.to_le_bytes());
        }
    }
    fn string(&mut self, units: &[u16]) {
        if units.iter().all(|unit| *unit <= 0xFF) {
            self.out.push(tag::ONE_BYTE_STRING);
            self.varint(units.len() as u32);
            self.out.extend(units.iter().map(|unit| *unit as u8));
            return;
        }
        let bytes = units.len() * 2;
        // V8 aligns the code units: pad when the tag and length would leave
        // them at an odd offset.
        if !(self.out.len() + 1 + varint_len(bytes as u32)).is_multiple_of(2) {
            self.out.push(tag::PADDING);
        }
        self.out.push(tag::TWO_BYTE_STRING);
        self.varint(bytes as u32);
        for unit in units {
            self.out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    fn bigint(&mut self, value: &BigInt) {
        let (sign, mut digits) = value.to_bytes_le();
        if sign == Sign::NoSign {
            digits.clear();
        }
        digits.resize(digits.len().div_ceil(DIGIT_BYTES) * DIGIT_BYTES, 0);
        self.out.push(tag::BIGINT);
        self.varint(((digits.len() as u32) << 1) | u32::from(sign == Sign::Minus));
        self.out.extend_from_slice(&digits);
    }
    fn key(&mut self, key: &Key) {
        match key {
            Key::Index(index) => self.number(f64::from(*index)),
            Key::Name(units) => self.string(units),
        }
    }
    fn value(&mut self, value: &Value, depth: usize) -> Result<(), V8Error> {
        self.check()?;
        self.reach(depth)?;
        self.count(1, 0);
        if let Value::String(units) = value {
            self.count(0, units.len());
        }
        match value {
            Value::Undefined => self.out.push(tag::UNDEFINED),
            Value::Null => self.out.push(tag::NULL),
            Value::Boolean(true) => self.out.push(tag::TRUE),
            Value::Boolean(false) => self.out.push(tag::FALSE),
            Value::Number(number) => self.number(*number),
            Value::BigInt(value) => self.bigint(value),
            Value::String(units) => self.string(units),
            Value::Object(object) => self.object(object, depth)?,
        }
        Ok(())
    }
    fn assign(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
    /// Write `object` once; a later reach writes a back-reference and keeps
    /// the depth limit and the extent bounds for the whole object below it.
    fn object(&mut self, object: &Rc<Object>, depth: usize) -> Result<(), V8Error> {
        let identity = Rc::as_ptr(object);
        if let Some(id) = self.ids.get(&identity).copied() {
            // An `Rc` graph has no cycles: a written object is complete.
            let (height, below) = *self.written.get(&identity).ok_or(V8Error::Cycle)?;
            self.reach(depth + height)?;
            self.count(below.values, below.units);
            self.out.push(tag::OBJECT_REFERENCE);
            self.varint(id);
            return Ok(());
        }
        let outer = core::mem::replace(&mut self.deepest, depth);
        let start = self.extent;
        self.contents(object, identity, depth)?;
        self.written
            .insert(identity, (self.deepest - depth, self.extent.since(start)));
        self.deepest = self.deepest.max(outer);
        self.check()
    }
    fn contents(
        &mut self,
        object: &Rc<Object>,
        identity: *const Object,
        depth: usize,
    ) -> Result<(), V8Error> {
        // A Uint8Array writes its buffer first, which takes the first id.
        if matches!(**object, Object::Bytes { .. }) {
            self.assign();
        }
        let id = self.assign();
        self.ids.insert(identity, id);
        match &**object {
            Object::Plain(properties) => {
                self.out.push(tag::BEGIN_OBJECT);
                for (key, value) in properties {
                    self.key(key);
                    self.count(0, key.units());
                    self.value(value, depth + 1)?;
                }
                self.out.push(tag::END_OBJECT);
                self.varint(properties.len() as u32);
            }
            Object::Dense(elements) => {
                self.out.push(tag::BEGIN_DENSE_ARRAY);
                self.varint(elements.len() as u32);
                for element in elements {
                    match element {
                        Some(value) => self.value(value, depth + 1)?,
                        None => self.out.push(tag::THE_HOLE),
                    }
                }
                self.out.push(tag::END_DENSE_ARRAY);
                self.varint(0);
                self.varint(elements.len() as u32);
            }
            Object::Sparse { length, elements } => {
                self.out.push(tag::BEGIN_SPARSE_ARRAY);
                self.varint(*length);
                for (index, value) in elements {
                    self.number(f64::from(*index));
                    self.value(value, depth + 1)?;
                }
                self.out.push(tag::END_SPARSE_ARRAY);
                self.varint(elements.len() as u32);
                self.varint(*length);
            }
            Object::Date(time) => {
                self.out.push(tag::DATE);
                self.out.extend_from_slice(&time.to_le_bytes());
            }
            Object::Map(entries) => {
                self.out.push(tag::BEGIN_MAP);
                for (key, value) in entries {
                    self.value(key, depth + 1)?;
                    self.value(value, depth + 1)?;
                }
                self.out.push(tag::END_MAP);
                self.varint((entries.len() * 2) as u32);
            }
            Object::Set(values) => {
                self.out.push(tag::BEGIN_SET);
                for value in values {
                    self.value(value, depth + 1)?;
                }
                self.out.push(tag::END_SET);
                self.varint(values.len() as u32);
            }
            Object::Bytes {
                buffer: bytes,
                offset,
                length,
            } => {
                self.out.push(tag::ARRAY_BUFFER);
                self.varint(bytes.len() as u32);
                self.out.extend_from_slice(bytes);
                self.out.push(tag::ARRAY_BUFFER_VIEW);
                self.out.push(tag::UINT8_ARRAY);
                self.varint(*offset);
                self.varint(*length);
                // Flags: neither length-tracking nor backed by a resizable buffer.
                self.varint(0);
            }
        }
        Ok(())
    }
}

fn varint_len(value: u32) -> usize {
    let bits = u32::BITS - value.leading_zeros();
    (bits.max(1) as usize).div_ceil(7)
}

#[cfg(test)]
#[path = "v8_tests.rs"]
mod tests;
