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
        assert_eq!(MAX_READ_KEY_BYTES, 2049);
        assert_eq!(MAX_TOTAL_MUTATION_BYTES, 819_200);
        const { assert!(MAX_STAGED_BYTES <= crate::abi::MAX_MEMORY_BYTES) };
    }
}
