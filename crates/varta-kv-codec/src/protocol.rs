//! What a KV import does to staged values and descriptors, before and after
//! the storage engine: the stored form of a staged value, the parsed read
//! and commit descriptors with Deno's limits (`op_kv_snapshot_read`,
//! `op_kv_atomic_write`), list cursors and versionstamp text.
//!
//! A failure the guest can cause is a recoverable [`KvError`]; a malformed
//! descriptor or an unknown stage id only a tampered module produces and is
//! [`Fatal`].
use crate::{
    v8,
    wire::{self, MutationCode, Selector, StageMode},
};
use alloc::{borrow::ToOwned, format, string::String, vec::Vec};
use core::num::NonZeroU32;

/// One failed KV operation: the name of the error the guest throws and its
/// message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvError {
    pub name: &'static str,
    pub message: String,
}

impl KvError {
    pub fn type_error(message: impl Into<String>) -> Self {
        Self {
            name: wire::error::TYPE_ERROR,
            message: message.into(),
        }
    }
    pub fn range_error(message: impl Into<String>) -> Self {
        Self {
            name: wire::error::RANGE_ERROR,
            message: message.into(),
        }
    }
    pub fn denied(id: &str, path: &str) -> Self {
        Self {
            name: wire::error::NOT_ALLOWED,
            message: format!("<capability denied: {id}> ({path})"),
        }
    }
    pub fn not_found(path: &str) -> Self {
        Self {
            name: wire::error::NOT_FOUND,
            message: format!("no KV database at {path}"),
        }
    }
    /// A storage failure opening `path`.
    pub fn unavailable(path: &str, error: impl core::fmt::Display) -> Self {
        Self::type_error(format!("cannot open the KV database {path}: {error}"))
    }
    /// A path whose last component is no database name.
    pub fn invalid_name(path: &str) -> Self {
        Self::type_error(format!("{path} is not a valid database file name"))
    }
    /// A path naming a root, not a database file.
    pub fn no_file(path: &str) -> Self {
        Self::type_error(format!("{path} names no database file"))
    }
    /// An open past `MAX_OPEN_DATABASES`.
    #[must_use]
    pub fn too_many_databases() -> Self {
        Self::type_error(format!(
            "at most {} KV databases are open at once",
            wire::MAX_OPEN_DATABASES
        ))
    }
    /// A database another writer holds.
    pub fn locked(path: &str) -> Self {
        Self::type_error(format!(
            "the KV database {path} is locked by another writer"
        ))
    }
    /// A commit to a database opened without `kv.write`.
    pub fn read_only(path: &str) -> Self {
        Self::type_error(format!("the KV database {path} is open read-only"))
    }
    /// A commit with too little of its turn left.
    pub fn too_late(path: &str) -> Self {
        Self::type_error(format!(
            "too little time is left in this turn to commit to the KV database {path}; nothing was applied"
        ))
    }
    /// The `kv_text` of this failure: `<ErrorName>\n<message>`.
    #[must_use]
    pub fn text(&self) -> String {
        format!(
            "{}{}{}",
            self.name,
            wire::DESCRIPTOR_SEPARATOR,
            self.message
        )
    }
}

/// A descriptor or id only a tampered module produces: fatal. The message
/// has no prefix; hosts trap with `kv: <message>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fatal(pub &'static str);

/// The stored form of a value (Deno KV's three value encodings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// V8 serializer bytes (`v_encoding` 1).
    V8(Vec<u8>),
    /// A `KvU64` (`v_encoding` 2).
    U64(u64),
    /// A top-level `Uint8Array` (`v_encoding` 3).
    Bytes(Vec<u8>),
}

impl Stored {
    /// The stored bytes of the value.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::V8(bytes) | Self::Bytes(bytes) => bytes.len(),
            Self::U64(_) => size_of::<u64>(),
        }
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// The value encoding (`v_encoding`).
    #[must_use]
    pub fn encoding(&self) -> i64 {
        match self {
            Self::V8(_) => wire::ENCODING_V8,
            Self::U64(_) => wire::ENCODING_U64,
            Self::Bytes(_) => wire::ENCODING_BYTES,
        }
    }
    /// The value of `encoding` stored as `bytes`, as Deno decodes a row;
    /// `None` for an unknown encoding or a `KvU64` that is not eight bytes.
    #[must_use]
    pub fn of(encoding: i64, bytes: Vec<u8>) -> Option<Self> {
        match encoding {
            wire::ENCODING_V8 => Some(Self::V8(bytes)),
            wire::ENCODING_BYTES => Some(Self::Bytes(bytes)),
            wire::ENCODING_U64 => Some(Self::U64(u64::from_le_bytes(bytes.try_into().ok()?))),
            _ => None,
        }
    }
    /// The bytes Deno stores for the value.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            Self::V8(bytes) | Self::Bytes(bytes) => bytes.clone(),
            Self::U64(number) => number.to_le_bytes().to_vec(),
        }
    }
}

/// One staged guest value, encoded when it was staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    /// The stored form of the value.
    pub value: Stored,
    /// The key bytes, when the value is a key (an array of strings and
    /// numbers), or why it is none.
    pub key: Result<Vec<u8>, String>,
}

impl Staged {
    /// The encoded bytes this value keeps staged.
    #[must_use]
    pub fn size(&self) -> usize {
        self.value.len() + self.key.as_ref().map_or(0, Vec::len)
    }
}

/// The stored form of `value` staged in `mode`.
pub fn staged_value(mode: StageMode, value: &v8::Value) -> Result<Staged, KvError> {
    let text = || match value {
        v8::Value::String(units) => Ok(units),
        _ => Err(KvError::type_error("invalid staged KV value")),
    };
    let value_form =
        match mode {
            StageMode::Value => {
                Stored::V8(v8::encode(value, wire::MAX_VALUE_BYTES).map_err(|_| value_too_large())?)
            }
            StageMode::U64 => {
                let digits = String::from_utf16(text()?)
                    .map_err(|_| KvError::type_error("invalid KvU64"))?;
                Stored::U64(digits.parse().map_err(|_| {
                    KvError::range_error("Value must fit in a 64-bit unsigned integer")
                })?)
            }
            StageMode::Bytes => Stored::Bytes(byte_values(value)?),
        };
    Ok(Staged {
        value: value_form,
        key: key_bytes(value),
    })
}

/// The bytes of a staged `Uint8Array`: its storage, an array of byte
/// values.
fn byte_values(value: &v8::Value) -> Result<Vec<u8>, KvError> {
    let invalid = || KvError::type_error("invalid staged bytes");
    let v8::Value::Object(object) = value else {
        return Err(invalid());
    };
    let v8::Object::Dense(elements) = &**object else {
        return Err(invalid());
    };
    elements
        .iter()
        .map(|element| match element {
            Some(v8::Value::Number(byte))
                if crate::integral(*byte) && (0.0..=f64::from(u8::MAX)).contains(byte) =>
            {
                Ok(*byte as u8)
            }
            _ => Err(invalid()),
        })
        .collect()
}

fn value_too_large() -> KvError {
    KvError::type_error(format!(
        "Value too large (max {} bytes)",
        wire::MAX_VALUE_BYTES
    ))
}

/// The encoded key of an array of strings and numbers (Deno's key parts
/// `string` and `number`).
fn key_bytes(value: &v8::Value) -> Result<Vec<u8>, String> {
    let v8::Value::Object(object) = value else {
        return Err("a KV key must be an array".into());
    };
    let v8::Object::Dense(elements) = &**object else {
        return Err("a KV key must be an array".into());
    };
    let mut parts = Vec::with_capacity(elements.len());
    for element in elements {
        parts.push(match element {
            Some(v8::Value::String(units)) => Part::String(
                String::from_utf16(units)
                    .map_err(|_| "a KV key string must be valid UTF-16".to_owned())?,
            ),
            Some(v8::Value::Number(number)) => Part::Float(*number),
            _ => return Err("a KV key part must be a string or a number".into()),
        });
    }
    Ok(crate::key::encode(parts.iter().map(|part| match part {
        Part::String(text) => crate::key::Encode::String(text),
        Part::Float(number) => crate::key::Encode::Float(*number),
    })))
}

/// One part of a key being encoded.
enum Part {
    String(String),
    Float(f64),
}

/// A database id the guest passes back.
#[must_use]
pub fn id_of(value: f64) -> Option<u32> {
    (crate::integral(value) && value >= 1.0 && value <= f64::from(u32::MAX)).then_some(value as u32)
}

/// The descriptor fields after the version line.
fn fields(text: &str) -> Result<core::str::Split<'_, char>, Fatal> {
    let mut fields = text.split(wire::DESCRIPTOR_SEPARATOR);
    if fields.next() != Some(wire::DESCRIPTOR_VERSION) {
        return Err(Fatal("descriptor version"));
    }
    Ok(fields)
}

fn next<'a>(fields: &mut impl Iterator<Item = &'a str>) -> Result<&'a str, Fatal> {
    fields.next().ok_or(Fatal("descriptor ends early"))
}

fn number<T: core::str::FromStr>(text: &str) -> Result<T, Fatal> {
    text.parse().map_err(|_| Fatal("descriptor number"))
}

/// The staged value `id` of the current descriptor.
fn staged<'a>(staged: &'a [Staged], id: &str) -> Result<&'a Staged, Fatal> {
    let index: usize = id.parse().map_err(|_| Fatal("stage id"))?;
    staged.get(index).ok_or(Fatal("unknown stage id"))
}

fn key(values: &[Staged], id: &str) -> Result<Result<Vec<u8>, KvError>, Fatal> {
    Ok(staged(values, id)?.key.clone().map_err(KvError::type_error))
}

/// One parsed read range: its bounds, limit and direction, and the common
/// prefix its cursors are relative to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub start: Vec<u8>,
    pub end: Vec<u8>,
    pub limit: NonZeroU32,
    pub reverse: bool,
    pub prefix: Vec<u8>,
}

/// A parsed read descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub id: u32,
    pub ranges: Vec<Range>,
}

/// Parse a read descriptor and check Deno's read limits.
pub fn parse_read(text: &str, values: &[Staged]) -> Result<Result<Read, KvError>, Fatal> {
    let mut fields = fields(text)?;
    let id = id_of(number(next(&mut fields)?)?).ok_or(Fatal("database id"))?;
    let count: usize = number(next(&mut fields)?)?;
    if count > wire::MAX_READ_RANGES {
        return Ok(Err(KvError::type_error(format!(
            "Too many ranges (max {})",
            wire::MAX_READ_RANGES
        ))));
    }
    let mut ranges = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        let range = match parse_range(values, &mut fields)? {
            Ok(range) => range,
            Err(error) => return Ok(Err(error)),
        };
        total += range.limit.get() as usize;
        ranges.push(range);
    }
    if fields.next().is_some() {
        return Err(Fatal("descriptor has trailing fields"));
    }
    if total > wire::MAX_READ_ENTRIES {
        return Ok(Err(KvError::type_error(format!(
            "Too many entries (max {})",
            wire::MAX_READ_ENTRIES
        ))));
    }
    Ok(Ok(Read { id, ranges }))
}

/// Parse one range of a read descriptor (Deno's `RawSelector` rules).
fn parse_range<'a>(
    values: &[Staged],
    fields: &mut impl Iterator<Item = &'a str>,
) -> Result<Result<Range, KvError>, Fatal> {
    let selector = Selector::from_code(number(next(fields)?)?).ok_or(Fatal("selector"))?;
    let first = next(fields)?;
    let second = next(fields)?;
    let limit: u32 = number(next(fields)?)?;
    let reverse = match next(fields)? {
        "0" => false,
        "1" => true,
        _ => return Err(Fatal("reverse flag")),
    };
    let cursor = next(fields)?;
    if selector.has_second() == second.is_empty() {
        return Err(Fatal("selector keys"));
    }
    let first = match key(values, first)? {
        Ok(key) => key,
        Err(error) => return Ok(Err(error)),
    };
    let second = if selector.has_second() {
        match key(values, second)? {
            Ok(key) => Some(key),
            Err(error) => return Ok(Err(error)),
        }
    } else {
        None
    };
    let Some(limit) = NonZeroU32::new(limit) else {
        return Ok(Err(KvError::type_error("limit must be greater than 0")));
    };
    let with = |key: &[u8], byte: u8| key.iter().copied().chain(Some(byte)).collect::<Vec<u8>>();
    // (start, end, prefix the cursor is relative to, selector start, selector end)
    let (start, end, prefix, low, high) = match (selector, second) {
        (Selector::Key, None) => {
            let end = with(&first, 0);
            (
                first.clone(),
                end.clone(),
                first.clone(),
                Some(first),
                Some(end),
            )
        }
        (Selector::Prefix, None) => (with(&first, 0), with(&first, 0xFF), first, None, None),
        (Selector::PrefixStart, Some(start)) => {
            if !start.starts_with(&first) || start.len() == first.len() {
                return Ok(Err(KvError::type_error(
                    "Start key is not in the keyspace defined by prefix",
                )));
            }
            (start.clone(), with(&first, 0xFF), first, Some(start), None)
        }
        (Selector::PrefixEnd, Some(end)) => {
            if !end.starts_with(&first) || end.len() == first.len() {
                return Ok(Err(KvError::type_error(
                    "End key is not in the keyspace defined by prefix",
                )));
            }
            (with(&first, 0), end.clone(), first, None, Some(end))
        }
        (Selector::StartEnd, Some(end)) => {
            if first > end {
                return Ok(Err(KvError::type_error(
                    "Start key is greater than end key",
                )));
            }
            let common = common_prefix(&first, &end).to_vec();
            (first.clone(), end.clone(), common, Some(first), Some(end))
        }
        _ => return Err(Fatal("selector keys")),
    };
    let (start, end) = if cursor.is_empty() {
        (start, end)
    } else {
        let Some(cursor) = base64url_decode(cursor) else {
            return Ok(Err(KvError::type_error("invalid cursor")));
        };
        let joined: Vec<u8> = prefix.iter().chain(&cursor).copied().collect();
        let (first_key, last_key) = if reverse {
            (start, joined)
        } else {
            (with(&joined, 0), end)
        };
        if low.as_ref().is_some_and(|low| first_key < *low)
            || high.as_ref().is_some_and(|high| last_key > *high)
        {
            return Ok(Err(KvError::type_error("cursor out of bounds")));
        }
        (first_key, last_key)
    };
    if start.len() > wire::MAX_READ_KEY_BYTES || end.len() > wire::MAX_READ_KEY_BYTES {
        return Ok(Err(KvError::type_error(format!(
            "Key too large for read (max {} bytes)",
            wire::MAX_READ_KEY_BYTES
        ))));
    }
    Ok(Ok(Range {
        start,
        end,
        limit,
        reverse,
        prefix,
    }))
}

fn common_prefix<'a>(a: &'a [u8], b: &[u8]) -> &'a [u8] {
    let length = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    &a[..length]
}

/// The cursor of an entry: base64url (padded, as Deno's `BASE64_URL_SAFE`)
/// of its key after the selector's common prefix.
#[must_use]
pub fn cursor(prefix: &[u8], key: &[u8]) -> String {
    base64url_encode(key.strip_prefix(prefix).unwrap_or_default())
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const PAD: u8 = b'=';

fn base64url_encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (index, byte)| {
            word | u32::from(*byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            if index <= chunk.len() {
                text.push(BASE64URL[(word >> (18 - 6 * index) & 0x3F) as usize] as char);
            } else {
                text.push(PAD as char);
            }
        }
    }
    text
}

/// Strict padded base64url: canonical padding and zero trailing bits.
fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index == bytes.len() / 4 - 1;
        let pads = chunk.iter().rev().take_while(|byte| **byte == PAD).count();
        if pads > 2 || pads > 0 && !last {
            return None;
        }
        let mut word = 0u32;
        for (position, byte) in chunk[..4 - pads].iter().enumerate() {
            let digit = BASE64URL.iter().position(|d| d == byte)? as u32;
            word |= digit << (18 - 6 * position);
        }
        let produced = 3 - pads;
        if word & ((1 << (8 * (3 - produced))) - 1) != 0 {
            return None;
        }
        out.extend((0..produced).map(|position| (word >> (16 - 8 * position)) as u8));
    }
    Some(out)
}

/// A Deno KV versionstamp: the database's data version (big-endian) and
/// two zero bytes.
pub type Versionstamp = [u8; 10];

/// Hexadecimal digits of one versionstamp byte.
const HEX_DIGITS_PER_BYTE: usize = 2;
const HEX_RADIX: u32 = 16;
/// The digits of a versionstamp text: 20 for Deno's ten-byte versionstamp.
const VERSIONSTAMP_DIGITS: usize = HEX_DIGITS_PER_BYTE * size_of::<Versionstamp>();

/// The versionstamp text Deno returns: 20 lowercase hex digits.
#[must_use]
pub fn versionstamp_text(versionstamp: &Versionstamp) -> String {
    versionstamp
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A versionstamp text Deno accepts in a check: exactly 20 hex digits.
#[must_use]
pub fn parse_versionstamp(text: &str) -> Option<Versionstamp> {
    if text.len() != VERSIONSTAMP_DIGITS || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut versionstamp = Versionstamp::default();
    for (index, byte) in versionstamp.iter_mut().enumerate() {
        let digits = HEX_DIGITS_PER_BYTE * index..HEX_DIGITS_PER_BYTE * (index + 1);
        *byte = u8::from_str_radix(&text[digits], HEX_RADIX).ok()?;
    }
    Some(versionstamp)
}

/// The latest expiration an `expireIn` may reach, in Unix milliseconds: the
/// last instant of `chrono`'s `DateTime<Utc>` (`MAX_UTC`), the native
/// runtime's instant type.
pub const MAX_EXPIRATION_MS: i64 = 8_210_266_876_799_999;

/// The expiration, in Unix milliseconds, of an entry set at `now_ms` with
/// `expireIn` = `milliseconds`; `None` when it is past
/// [`MAX_EXPIRATION_MS`] (the browser host's instant type).
#[must_use]
pub fn expire_at_ms(now_ms: i64, milliseconds: u64) -> Option<i64> {
    i64::try_from(milliseconds)
        .ok()
        .and_then(|milliseconds| now_ms.checked_add(milliseconds))
        .filter(|at| *at <= MAX_EXPIRATION_MS)
}

/// One check of a commit: the key's versionstamp must be this one (`None`:
/// the key must not exist).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub key: Vec<u8>,
    pub versionstamp: Option<Versionstamp>,
}

/// What one mutation does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Set(Stored),
    Delete,
    /// Wrapping `KvU64` addition.
    Sum(u64),
    Min(u64),
    Max(u64),
}

impl Kind {
    /// The value the mutation writes, if any.
    #[must_use]
    pub fn value(&self) -> Option<Stored> {
        match self {
            Self::Set(value) => Some(value.clone()),
            Self::Delete => None,
            Self::Sum(number) | Self::Min(number) | Self::Max(number) => Some(Stored::U64(*number)),
        }
    }
}

/// One mutation of a commit; `expire_at` is the host's own instant type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation<E> {
    pub key: Vec<u8>,
    pub kind: Kind,
    pub expire_at: Option<E>,
}

/// A parsed commit descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit<E> {
    pub id: u32,
    pub checks: Vec<Check>,
    pub mutations: Vec<Mutation<E>>,
}

impl<E> Commit<E> {
    /// The key and value bytes the commit writes: the keys of its checks
    /// and of its mutations, and its values (the commit's fuel).
    #[must_use]
    pub fn written(&self) -> usize {
        self.checks
            .iter()
            .map(|check| check.key.len())
            .chain(self.mutations.iter().map(|mutation| {
                mutation.key.len() + mutation.kind.value().map_or(0, |value| value.len())
            }))
            .sum()
    }
}

/// Parse the checks and mutations of a commit descriptor and check Deno's
/// limits (`op_kv_atomic_write`). `expire_at` turns an `expireIn` of
/// milliseconds into the host's instant on one reading of its KV clock,
/// `None` when it is outside the host's range.
pub fn parse_commit<E>(
    text: &str,
    values: &[Staged],
    mut expire_at: impl FnMut(u64) -> Option<E>,
) -> Result<Result<Commit<E>, KvError>, Fatal> {
    let mut fields = fields(text)?;
    let id = id_of(number(next(&mut fields)?)?).ok_or(Fatal("database id"))?;
    let checks: usize = number(next(&mut fields)?)?;
    if checks > wire::MAX_CHECKS {
        return Ok(Err(KvError::type_error(format!(
            "Too many checks (max {})",
            wire::MAX_CHECKS
        ))));
    }
    let mut parsed_checks = Vec::with_capacity(checks);
    for _ in 0..checks {
        let key = key(values, next(&mut fields)?)?;
        let versionstamp = next(&mut fields)?;
        let key = match key {
            Ok(key) => key,
            Err(error) => return Ok(Err(error)),
        };
        let versionstamp = if versionstamp.is_empty() {
            None
        } else {
            match parse_versionstamp(versionstamp) {
                Some(versionstamp) => Some(versionstamp),
                None => return Ok(Err(KvError::type_error("invalid versionstamp"))),
            }
        };
        parsed_checks.push(Check { key, versionstamp });
    }
    let mutations: usize = number(next(&mut fields)?)?;
    if mutations > wire::MAX_MUTATIONS {
        return Ok(Err(KvError::type_error(format!(
            "Too many mutations (max {})",
            wire::MAX_MUTATIONS
        ))));
    }
    let mut parsed = Vec::with_capacity(mutations);
    for _ in 0..mutations {
        let code =
            MutationCode::from_code(number(next(&mut fields)?)?).ok_or(Fatal("mutation kind"))?;
        let key = key(values, next(&mut fields)?)?;
        let value = next(&mut fields)?;
        let expire_in = next(&mut fields)?;
        if code.has_value() == value.is_empty()
            || code != MutationCode::Set && !expire_in.is_empty()
        {
            return Err(Fatal("mutation fields"));
        }
        let key = match key {
            Ok(key) => key,
            Err(error) => return Ok(Err(error)),
        };
        let value = if code.has_value() {
            Some(staged(values, value)?.value.clone())
        } else {
            None
        };
        let kind = match (code, value) {
            (MutationCode::Set, Some(value)) => Kind::Set(value),
            (MutationCode::Delete, None) => Kind::Delete,
            (MutationCode::Sum, Some(Stored::U64(number))) => Kind::Sum(number),
            (MutationCode::Min, Some(Stored::U64(number))) => Kind::Min(number),
            (MutationCode::Max, Some(Stored::U64(number))) => Kind::Max(number),
            _ => return Err(Fatal("mutation value")),
        };
        let expire_at = if expire_in.is_empty() {
            None
        } else {
            let milliseconds: u64 = number(expire_in)?;
            match expire_at(milliseconds) {
                Some(at) => Some(at),
                None => {
                    return Ok(Err(KvError::type_error(format!(
                        "expireIn is too large: {milliseconds}"
                    ))));
                }
            }
        };
        parsed.push(Mutation {
            key,
            kind,
            expire_at,
        });
    }
    let (mut payload, mut keys) = (0usize, 0usize);
    for key in parsed_checks
        .iter()
        .map(|check| &check.key)
        .chain(parsed.iter().map(|mutation| &mutation.key))
    {
        if key.is_empty() {
            return Ok(Err(KvError::type_error("key cannot be empty")));
        }
        if key.len() > wire::MAX_WRITE_KEY_BYTES {
            return Ok(Err(KvError::type_error(format!(
                "Key too large for write (max {} bytes)",
                wire::MAX_WRITE_KEY_BYTES
            ))));
        }
        payload += key.len();
    }
    for mutation in &parsed {
        let Some(value) = mutation.kind.value() else {
            continue;
        };
        let size = value.len();
        if size > wire::MAX_VALUE_BYTES {
            return Ok(Err(value_too_large()));
        }
        payload += size + mutation.key.len();
        keys += mutation.key.len();
    }
    if payload > wire::MAX_TOTAL_MUTATION_BYTES {
        return Ok(Err(KvError::type_error(format!(
            "Total mutation size too large (max {} bytes)",
            wire::MAX_TOTAL_MUTATION_BYTES
        ))));
    }
    if keys > wire::MAX_TOTAL_KEY_BYTES {
        return Ok(Err(KvError::type_error(format!(
            "Total key size too large (max {} bytes)",
            wire::MAX_TOTAL_KEY_BYTES
        ))));
    }
    // Trailing fields are fatal only once the commit is otherwise valid.
    if fields.next().is_some() {
        return Err(Fatal("descriptor has trailing fields"));
    }
    Ok(Ok(Commit {
        id,
        checks: parsed_checks,
        mutations: parsed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_are_padded_base64url() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", &[0xFF, 0xFE, 0x00, 0x01]] {
            let text = base64url_encode(bytes);
            assert_eq!(base64url_decode(&text).as_deref(), Some(bytes), "{text}");
        }
        assert_eq!(base64url_encode(&[0xFB, 0xFF]), "-_8=");
        for bad in ["a", "ab=", "a===", "Zg=a", "Zh==", "Z!=="] {
            assert_eq!(base64url_decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn versionstamps_are_twenty_hex_digits() {
        let stamp = [0, 0, 0, 0, 0, 0, 0x01, 0xab, 0, 0];
        let text = versionstamp_text(&stamp);
        assert_eq!(text, "00000000000001ab0000");
        assert_eq!(parse_versionstamp(&text), Some(stamp));
        assert_eq!(
            parse_versionstamp("000000000000000000AB"),
            Some([0, 0, 0, 0, 0, 0, 0, 0, 0, 0xab])
        );
        for bad in ["", "00", "000000000000000000000", "00000000000000000g00"] {
            assert_eq!(parse_versionstamp(bad), None, "{bad}");
        }
    }

    #[test]
    fn expirations_stop_at_the_last_instant() {
        assert_eq!(expire_at_ms(1_000, 500), Some(1_500));
        let last = MAX_EXPIRATION_MS as u64;
        assert_eq!(expire_at_ms(0, last), Some(MAX_EXPIRATION_MS));
        assert_eq!(expire_at_ms(1, last), None);
        assert_eq!(expire_at_ms(0, u64::MAX), None);
    }
}
