//! Deno KV compatible storage of profile v38 (`kv`): the contract shared by
//! the compiler's `varta.kv` prelude, the runtime's SQLite host and artifact
//! validation.
//!
//! A program opens a database file at a virtual path (`/data/app.kv`, under
//! the v38 [file roots](crate::files)) with `kv_open`, after the exact
//! `kv.read` grant check, before any file is opened; the database is writable
//! only when `kv.write` covers the same path. Keys and values are handed to
//! the host first (`kv_stage`, one guest value per call), then a read
//! (`kv_read`) or an atomic commit (`kv_commit`) names them by stage id in a
//! text descriptor (below). A read keeps its entries in the runtime until the
//! next read: the guest takes each entry's fields (`kv_entry`), its key
//! (`kv_key`) and its value (`kv_value`), the last two decoded strictly into
//! the static type the program names, exactly like checked JSON parsing. A
//! failed operation returns [`FAILED`] and leaves `<ErrorName>\n<message>`
//! in `kv_text`; a commit leaves its versionstamp there.
//!
//! Files are those of Deno KV's SQLite backend (`denokv_sqlite` 0.14.0):
//! the guest-visible limits below are Deno 2.9.4's `KvConfig` defaults.

/// Opening a database and reading from it.
pub const READ_CAPABILITY: &str = "kv.read";
/// Writing to an open database (and creating a missing file).
pub const WRITE_CAPABILITY: &str = "kv.write";

/// The first line of every read and commit descriptor.
pub const DESCRIPTOR_VERSION: &str = "varta-kv-1";
/// Separates descriptor fields. Every field is a decimal number, a
/// versionstamp, a cursor or empty, so none contains the separator.
pub const DESCRIPTOR_SEPARATOR: char = '\n';
/// UTF-16 bytes of one descriptor or one database path.
pub const MAX_DESCRIPTOR_BYTES: usize = 64 * 1024;
/// The result of a failed operation (an open, read, commit or stage).
pub const FAILED: f64 = -1.0;
/// The result of a commit whose checks failed (nothing was written).
pub const CHECK_FAILED: f64 = 0.0;
/// The result of a committed write.
pub const COMMITTED: f64 = 1.0;

/// Milliseconds before the turn deadline after which a guest commit may no
/// longer reach the file: the host's time to finish a commit SQLite has
/// begun (making it durable) and to return to the guest. A commit started
/// with less time left fails at once and applies nothing.
pub const COMMIT_RESERVE_MS: u64 = 50;
/// Milliseconds a commit's lock probe waits for the write lock, at most and
/// never past the commit cutoff: long enough for the database worker's own
/// brief maintenance transactions, short enough to fail fast on another
/// writer.
pub const LOCK_WAIT_MS: u64 = 100;
/// Databases one instance holds open at once.
pub const MAX_OPEN_DATABASES: usize = 8;
/// Values staged and not yet consumed by a read or a commit: every key and
/// value of the largest commit Deno admits, plus its checks.
pub const MAX_STAGED: usize = 2 * MAX_MUTATIONS + MAX_CHECKS;
/// Encoded bytes staged at once (the largest commit's keys and values).
pub const MAX_STAGED_BYTES: usize = MAX_TOTAL_MUTATION_BYTES + MAX_TOTAL_KEY_BYTES;
/// The fewest bytes Deno's V8 serializer writes for one value: a tag alone
/// (`null`, `undefined`, `true`, `false`); every other value takes a tag and
/// more.
pub const MIN_ENCODED_VALUE_BYTES: usize = 1;
/// Values one KV value expands to: its stored values (an element, a
/// property's value, a `Map` key or value, a `Set` value; not a hole), a
/// part shared within the value (written once, then back-referenced)
/// counted again at every reach, as a read builds it.
///
/// The bound only stops sharing-driven blow-up: a value without shared
/// parts has at most one value per [`MIN_ENCODED_VALUE_BYTES`] of its at
/// most [`MAX_VALUE_BYTES`] encoded bytes, so every value Deno stores
/// without sharing is within it (asserted below), and an expanded value is
/// never larger than an unshared value of the largest size. Deno KV, which
/// keeps shared parts shared, has no such bound.
pub const MAX_VALUE_NODES: usize = MAX_VALUE_BYTES / MIN_ENCODED_VALUE_BYTES;
const _: () = assert!(MAX_VALUE_NODES * MIN_ENCODED_VALUE_BYTES >= MAX_VALUE_BYTES);

/// The densest text an unshared encoded value carries: an array-index
/// property key of nine decimal digits (at least `10^8`), which Deno writes
/// as a number tag and a varint of four bytes, followed by its value of at
/// least [`MIN_ENCODED_VALUE_BYTES`].
pub const DENSEST_TEXT_UNITS: usize = 9;
/// The encoded bytes of [`DENSEST_TEXT_UNITS`]: tag, varint and value.
pub const DENSEST_TEXT_BYTES: usize = 6;
/// UTF-16 code units of text one KV value expands to: the code units of
/// its strings and of the property keys of its plain objects (an
/// array-index key counted as its decimal digits), a shared part counted
/// again at every reach.
///
/// Derivation: the most text a value without shared parts of
/// [`MAX_VALUE_BYTES`] encoded bytes can carry, at the densest rate of any
/// V8 form of text:
/// - a Latin-1 (one-byte) string takes a tag, a length and one byte per
///   code unit: at most one unit per byte;
/// - a two-byte string takes two bytes per code unit: at most half a unit
///   per byte (a UTF-8 string, which V8 still reads, at most one);
/// - an array-index key of `d` digits takes a number tag (`I` with a
///   zigzag varint, or `U` with a plain one, the shorter) and a varint of
///   `ceil(bits / 7)` bytes, then its value: `d` units in
///   `1 + varint + MIN_ENCODED_VALUE_BYTES` bytes, densest at nine digits
///   (nine units in six bytes, three units per two bytes; asserted below
///   for every digit count of a `u32` index).
///
/// So every value Deno stores without sharing is within the bound.
pub const MAX_VALUE_UNITS: usize = MAX_VALUE_BYTES * DENSEST_TEXT_UNITS / DENSEST_TEXT_BYTES;

/// The payload bits of one varint byte (V8's base-128 varints).
const VARINT_PAYLOAD_BITS: u32 = 7;
/// Decimal digits of the largest array index (`2^32 - 2`).
const MAX_INDEX_DIGITS: u32 = 10;
/// The decimal radix of an array-index key's text.
const DECIMAL: u32 = 10;
/// The tag byte of a number written as a property key.
const KEY_TAG_BYTES: usize = 1;

/// The fewest bytes of an array-index key of `digits` decimal digits and
/// its value: the plain varint of the smallest such index (never longer
/// than the zigzag varint of an `I` key).
const fn index_key_bytes(digits: u32) -> usize {
    let smallest = DECIMAL.pow(digits - 1);
    // A varint of zero still takes one byte.
    let bits = u32::BITS - smallest.leading_zeros();
    let varint = if bits > 0 { bits } else { 1 }.div_ceil(VARINT_PAYLOAD_BITS) as usize;
    KEY_TAG_BYTES + varint + MIN_ENCODED_VALUE_BYTES
}

/// Whether no form of text is denser than [`DENSEST_TEXT_UNITS`] in
/// [`DENSEST_TEXT_BYTES`], and the densest form is that one.
const fn densest_text_is_nine_digit_keys() -> bool {
    // One-byte strings: one unit per byte.
    if DENSEST_TEXT_BYTES > DENSEST_TEXT_UNITS {
        return false;
    }
    let mut digits = 1;
    while digits <= MAX_INDEX_DIGITS {
        if digits as usize * DENSEST_TEXT_BYTES > DENSEST_TEXT_UNITS * index_key_bytes(digits) {
            return false;
        }
        digits += 1;
    }
    index_key_bytes(DENSEST_TEXT_UNITS as u32) == DENSEST_TEXT_BYTES
}
const _: () = assert!(densest_text_is_nine_digit_keys());

/// Deno KV's limits (`deno_kv::KvConfig` defaults of Deno 2.9.4).
pub const MAX_WRITE_KEY_BYTES: usize = 2048;
/// Range selectors can carry one more byte (a `0x00` or `0xff` suffix).
pub const MAX_READ_KEY_BYTES: usize = MAX_WRITE_KEY_BYTES + 1;
pub const MAX_VALUE_BYTES: usize = 65_536;
pub const MAX_READ_RANGES: usize = 10;
pub const MAX_READ_ENTRIES: usize = 1000;
pub const MAX_CHECKS: usize = 100;
pub const MAX_MUTATIONS: usize = 1000;
pub const MAX_TOTAL_MUTATION_BYTES: usize = 800 * 1024;
pub const MAX_TOTAL_KEY_BYTES: usize = 80 * 1024;
/// `list()` batch size: Deno's default and its ceiling without an explicit
/// `batchSize`.
pub const DEFAULT_BATCH_SIZE: usize = 100;
pub const MAX_DEFAULT_BATCH_SIZE: usize = 500;

/// How a staged guest value is stored (Deno KV's three value encodings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum StageMode {
    /// Any other value: V8 serializer bytes (`v_encoding` 1).
    Value = 0,
    /// A `KvU64`, staged as its decimal text (`v_encoding` 2).
    U64 = 1,
    /// A top-level `Uint8Array`, staged as one code unit per byte
    /// (`v_encoding` 3).
    Bytes = 2,
}

impl StageMode {
    pub const ALL: [Self; 3] = [Self::Value, Self::U64, Self::Bytes];
    #[must_use]
    pub fn from_code(code: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| *mode as i32 == code)
    }
}

/// The selector of one read range: what its one or two staged keys mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Selector {
    /// One exact key (`get`, `getMany`).
    Key = 0,
    Prefix = 1,
    PrefixStart = 2,
    PrefixEnd = 3,
    StartEnd = 4,
}

impl Selector {
    pub const ALL: [Self; 5] = [
        Self::Key,
        Self::Prefix,
        Self::PrefixStart,
        Self::PrefixEnd,
        Self::StartEnd,
    ];
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|selector| *selector as u8 == code)
    }
    /// Whether the selector names a second key.
    #[must_use]
    pub fn has_second(self) -> bool {
        matches!(self, Self::PrefixStart | Self::PrefixEnd | Self::StartEnd)
    }
}

/// The kind of one mutation of a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MutationCode {
    Set = 0,
    Delete = 1,
    Sum = 2,
    Min = 3,
    Max = 4,
}

impl MutationCode {
    pub const ALL: [Self; 5] = [Self::Set, Self::Delete, Self::Sum, Self::Min, Self::Max];
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| *kind as u8 == code)
    }
    /// Whether the mutation carries a staged value.
    #[must_use]
    pub fn has_value(self) -> bool {
        self != Self::Delete
    }
}

/// The text fields of one read entry (`kv_entry`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EntryField {
    /// 20 lowercase hex digits, as Deno formats versionstamps.
    Versionstamp = 0,
    /// The value encoding (`v_encoding`): `1`, `2` or `3`.
    Encoding = 1,
    /// A `KvU64` value as decimal text.
    U64 = 2,
    /// A bytes value, one code unit per byte.
    Bytes = 3,
    /// The list cursor after this entry (base64url of the key bytes after
    /// the selector's common prefix).
    Cursor = 4,
    /// The index of the read range the entry belongs to.
    Range = 5,
}

impl EntryField {
    pub const ALL: [Self; 6] = [
        Self::Versionstamp,
        Self::Encoding,
        Self::U64,
        Self::Bytes,
        Self::Cursor,
        Self::Range,
    ];
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|field| *field as u8 == code)
    }
}

/// Deno KV's value encodings (`denokv_proto::VALUE_ENCODING_*`).
pub const ENCODING_V8: i64 = 1;
pub const ENCODING_U64: i64 = 2;
pub const ENCODING_BYTES: i64 = 3;

/// The error names `kv_text` reports; the prelude throws the matching class.
pub mod error {
    /// A denied capability (a recoverable `NotAllowedError` `DOMException`).
    pub const NOT_ALLOWED: &str = "NotAllowedError";
    /// A missing database file opened read-only (`NotFoundError`).
    pub const NOT_FOUND: &str = "NotFoundError";
    pub const TYPE_ERROR: &str = "TypeError";
    pub const RANGE_ERROR: &str = "RangeError";
}

/// The `migration_state` version of the supported file schema
/// (`denokv_sqlite` 0.14.0 runs three migrations).
pub const SCHEMA_VERSION: i64 = 3;
/// The tables of the supported schema, in name order.
pub const SCHEMA_TABLES: [&str; 5] = [
    "data_version",
    "kv",
    "migration_state",
    "queue",
    "queue_running",
];
/// The columns of the `kv` table, in order.
pub const KV_COLUMNS: [&str; 6] = ["k", "v", "v_encoding", "version", "seq", "expiration_ms"];

/// A database file name may not start with this character (SQLite's
/// special names such as `:memory:`).
pub const RESERVED_NAME_PREFIX: char = ':';
/// Nor end with the name of a sidecar SQLite keeps next to a database.
pub const SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];
/// The failure of a stage past [`MAX_STAGED_BYTES`].
pub const STAGED_TOO_LARGE: &str = "the staged keys and values of one KV operation are too large";

/// The interface of the browser host's KV codec module: the shared value
/// layer (`varta-kv-codec`) compiled to wasm32 (`varta-kv-codec-wasm`),
/// which the Worker instantiates beside the guest. The Worker passes inputs
/// through the module's arena and reads results from its output buffer; the
/// module reaches guest memory, the guest allocator, the exception channel,
/// the deadline and the fuel tank only through the imports below. All
/// multi-byte fields are little-endian; byte strings are a `u32` length and
/// the bytes.
pub mod codec {
    /// The module the codec imports its host functions from.
    pub const IMPORT_MODULE: &str = "varta_kv_host";
    /// `() -> i32`: the guest memory's byte length.
    pub const IMPORT_MEMORY_SIZE: &str = "memory_size";
    /// `(at, into, len) -> i32`: copy guest bytes into codec memory; 0 when
    /// the guest range is outside memory.
    pub const IMPORT_MEMORY_READ: &str = "memory_read";
    /// `(at, from, len) -> i32`: copy codec bytes into guest memory.
    pub const IMPORT_MEMORY_WRITE: &str = "memory_write";
    /// `(at, len) -> i32`: zero guest bytes.
    pub const IMPORT_MEMORY_ZERO: &str = "memory_zero";
    /// `(len) -> i32`: a guest block from the module's allocation helper.
    pub const IMPORT_ALLOCATE: &str = "allocate";
    /// `(i64) -> i64`: the module's retention helper.
    pub const IMPORT_RETAIN: &str = "retain";
    /// `() -> i32`: 1 once the turn deadline has passed.
    pub const IMPORT_DEADLINE: &str = "deadline_passed";
    /// `(i64) -> ()`: charge fair fuel (the host traps when it runs out).
    pub const IMPORT_CHARGE: &str = "charge";
    /// `() -> i32`: 1 when the exception channel is canonical and clear.
    pub const IMPORT_CHANNEL_READY: &str = "channel_ready";
    /// `(i64) -> i32`: throw an Error cell through the exception channel.
    pub const IMPORT_CHANNEL_THROW: &str = "channel_throw";
    /// `(i32) -> i32`: set the result length global.
    pub const IMPORT_RESULT_LENGTH: &str = "result_length";
    /// Every import, in this order.
    pub const IMPORTS: [&str; 11] = [
        IMPORT_MEMORY_SIZE,
        IMPORT_MEMORY_READ,
        IMPORT_MEMORY_WRITE,
        IMPORT_MEMORY_ZERO,
        IMPORT_ALLOCATE,
        IMPORT_RETAIN,
        IMPORT_DEADLINE,
        IMPORT_CHARGE,
        IMPORT_CHANNEL_READY,
        IMPORT_CHANNEL_THROW,
        IMPORT_RESULT_LENGTH,
    ];

    /// `(len) -> ptr`: start a call: reset the arena and reserve `len`
    /// input bytes.
    pub const EXPORT_INPUT: &str = "kv_input";
    /// `() -> ptr` and `() -> len` of the last call's output.
    pub const EXPORT_OUTPUT: &str = "kv_output";
    pub const EXPORT_OUTPUT_LENGTH: &str = "kv_output_len";
    /// `() -> status`: input the module's `varta.static` and
    /// `varta.intrinsics` section payloads (each a byte string, empty when
    /// absent).
    pub const EXPORT_CONFIGURE: &str = "kv_configure";
    /// `(mode, kind, packed: i64) -> status`: stage a guest value. Output:
    /// the fuel (`u64`), the staged bytes (`u32`) and the staged record.
    pub const EXPORT_STAGE: &str = "kv_stage";
    /// `() -> status`: input the staged table and the descriptor; output
    /// the database id, then each range's start, end, limit (`u32`),
    /// reverse flag (`u8`) and cursor prefix.
    pub const EXPORT_PARSE_READ: &str = "kv_parse_read";
    /// `() -> status`: input the range prefixes, then each entry's range
    /// (`u32`), key, stored value length (`u32`) and versionstamp (ten
    /// bytes); output the fuel (`u64`), then each entry's cursor and
    /// versionstamp text.
    pub const EXPORT_READ_ENTRIES: &str = "kv_read_entries";
    /// `(now_ms: f64) -> status`: input the staged table and the
    /// descriptor; output the database id, the fuel (`u64`), the checks
    /// (key, presence flag, versionstamp) and the mutations (kind, key, then
    /// a staged value for a set, a `u64` for sum, min and max, then the
    /// presence flag and expiration milliseconds as `f64`).
    pub const EXPORT_PARSE_COMMIT: &str = "kv_parse_commit";
    /// `(key, schema_ptr, schema_len) -> status`: input the entry kind and,
    /// for a value, its encoding byte and bytes; output the result handle
    /// (`u64`).
    pub const EXPORT_DECODE: &str = "kv_decode";
    /// The codec's own memory.
    pub const EXPORT_MEMORY: &str = "memory";

    /// Call statuses.
    pub const OK: u32 = 0;
    /// A recoverable failure: the `kv_text` in the output.
    pub const FAILED: u32 = 1;
    /// A fatal failure: the trap message in the output.
    pub const FATAL: u32 = 2;
    /// Host work stopped at the turn deadline.
    pub const DEADLINE: u32 = 3;

    /// The value of a staged record: its encoding byte (one of Deno's
    /// value encodings, `super::ENCODING_*`) and stored bytes, or this byte
    /// and the text of why the value cannot be stored.
    pub const VALUE_FAILURE: u8 = 0;
    /// The key of a staged record: encoded key bytes, or the text of why
    /// the value is no key.
    pub const KEY_BYTES: u8 = 0;
    pub const KEY_FAILURE: u8 = 1;
    /// The entry a decode reads: none (the index named none), the key, or
    /// the value.
    pub const ENTRY_NONE: u8 = 0;
    pub const ENTRY_KEY: u8 = 1;
    pub const ENTRY_VALUE: u8 = 2;
    /// An optional field's presence byte.
    pub const ABSENT: u8 = 0;
    pub const PRESENT: u8 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_limits_match_deno() {
        for mode in StageMode::ALL {
            assert_eq!(StageMode::from_code(mode as i32), Some(mode));
        }
        for selector in Selector::ALL {
            assert_eq!(Selector::from_code(selector as u8), Some(selector));
        }
        for kind in MutationCode::ALL {
            assert_eq!(MutationCode::from_code(kind as u8), Some(kind));
        }
        for field in EntryField::ALL {
            assert_eq!(EntryField::from_code(field as u8), Some(field));
        }
        assert_eq!(StageMode::from_code(3), None);
        for encoding in [ENCODING_V8, ENCODING_U64, ENCODING_BYTES] {
            assert_ne!(encoding, i64::from(codec::VALUE_FAILURE));
        }
        assert_eq!(MAX_READ_KEY_BYTES, 2049);
        assert_eq!(MAX_TOTAL_MUTATION_BYTES, 819_200);
        // One value per encoded byte; three code units per two bytes.
        assert_eq!(MAX_VALUE_NODES, 65_536);
        assert_eq!(MAX_VALUE_UNITS, 98_304);
        assert_eq!(index_key_bytes(1), 3);
        assert_eq!(index_key_bytes(9), DENSEST_TEXT_BYTES);
        assert_eq!(index_key_bytes(MAX_INDEX_DIGITS), 7);
        const { assert!(MAX_STAGED_BYTES <= crate::abi::MAX_MEMORY_BYTES) };
    }
}
