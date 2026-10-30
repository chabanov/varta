//! Versioned closed JSON schemas, including checked optional fields and unions.
//!
//! Without `std` this module is the schema model, its validation, the
//! hand-written schema reader ([`JsonSchema::read`]) and the shared UTF-16
//! decoder ([`scan`]) that both the host imports of the recoverable profiles
//! and the linked v38 decoder run. The `serde_json` decoders of the earlier
//! profiles (and the reference classifier the shared decoder is tested
//! against) need `std`.
use alloc::{boxed::Box, string::String, vec::Vec};
#[cfg(feature = "std")]
mod classified;
#[cfg(feature = "std")]
mod decode;
pub mod build;
mod descriptor;
pub use descriptor::DescriptorError;
mod error;
pub use error::{JsonDecodeError, JsonInputLimit};
mod read;
pub mod scan;
mod schema;
pub use schema::{JsonField, JsonSchema, JsonType, SchemaBounds};

pub const SCHEMA_VERSION: u32 = 1;
pub const NUMBER_SCHEMA_VERSION: u32 = 2;
pub const UNDEFINED_SCHEMA_VERSION: u32 = 3;
pub const UTF16_SCHEMA_VERSION: u32 = 4;
pub const UNION_SCHEMA_VERSION: u32 = 5;
pub const MISSING_SCHEMA_VERSION: u32 = 6;
pub const REFERENCE_SCHEMA_VERSION: u32 = 7;
pub const CHECKED_SCHEMA_VERSION: u32 = 8;
pub const VECTOR_SCHEMA_VERSION: u32 = 9;
pub const ERROR_SCHEMA_VERSION: u32 = 10;
pub const ERROR_FAMILY_SCHEMA_VERSION: u32 = 11;
pub const SPARSE_SCHEMA_VERSION: u32 = 12;
pub const BIGINT_SCHEMA_VERSION: u32 = 13;
/// Existing JSON types materialize value14 layouts; JSON cannot fabricate callables.
pub const FUNCTION_SCHEMA_VERSION: u32 = 14;
/// Value15 layouts (profile v34): JSON types materialize value15 records,
/// arrays and cells. Later v34 types extend this schema version.
pub const DYNAMIC_SCHEMA_VERSION: u32 = 15;
/// Value16 layouts (profile v35): the schema-15 grammar materializing value16
/// records, arrays and cells. JSON cannot fabricate promises.
pub const PROMISE_SCHEMA_VERSION: u32 = 16;
/// Value17 layouts (profile v36): the schema-15 grammar materializing value17
/// records, arrays and cells. JSON cannot fabricate functions or promises.
pub const FUNCTION_FIELD_SCHEMA_VERSION: u32 = 17;
/// Schema 18 (v38 `dictionary_hashing`): schema 17 types decoded into value18
/// layouts, whose dictionaries carry the hash-table slot (built unindexed).
pub const HASHED_DICTIONARY_SCHEMA_VERSION: u32 = 18;
/// Whether a schema version uses the schema-15 grammar (forward references,
/// dictionaries and wide records).
#[must_use]
pub fn dynamic_schema(version: u32) -> bool {
    matches!(
        version,
        DYNAMIC_SCHEMA_VERSION
            | PROMISE_SCHEMA_VERSION
            | FUNCTION_FIELD_SCHEMA_VERSION
            | HASHED_DICTIONARY_SCHEMA_VERSION
    )
}
pub const MAX_SCHEMA_BYTES: usize = crate::abi::RESULT_BASE;
pub const LEGACY_MAX_SCHEMA_TYPES: usize =
    4 + crate::value::MAX_RECORD_SHAPES + crate::value::MAX_ARRAY_TYPES;
pub const MAX_SCHEMA_TYPES: usize = LEGACY_MAX_SCHEMA_TYPES + 1 + crate::value::MAX_UNION_TYPES;
/// The frozen member limit of one schema union (schemas of v5 to v37);
/// readers take it from `ProgramLimits::json_union_members`.
pub const MAX_UNION_MEMBERS: usize = 7;

#[derive(Debug, thiserror::Error)]
pub enum CheckedJsonError {
    #[error("invalid JSON schema: {0}")]
    Schema(String),
    #[error("invalid JSON input: {0}")]
    Input(String),
}

/// A completely checked tree, before any guest allocation. Record fields are in
/// canonical schema-slot order, independently of the input object's key order.
#[derive(Debug, PartialEq, Eq)]
pub enum JsonValue {
    Null,
    Undefined,
    Union {
        ty: u32,
        value: Box<JsonValue>,
    },
    I32(i32),
    Float64(u64),
    Boolean(bool),
    String(String),
    String16(Vec<u16>),
    Record {
        ty: u32,
        fields: Vec<JsonValue>,
    },
    Array {
        ty: u32,
        values: Vec<JsonValue>,
    },
    /// Schema 15 dictionary entries in own-key order: array-index keys
    /// ascending, then the remaining keys in input order.
    Dictionary {
        ty: u32,
        entries: Vec<(Vec<u16>, JsonValue)>,
    },
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod classified_tests;
