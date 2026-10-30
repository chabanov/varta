//! Versioned structured-value ABI. These tags describe physical values in the
//! i32/UTF-8 profile (v1/v2) and binary64/UTF-8 profiles (v3/v4).
//! v4 adds canonical undefined; v5 selects UTF-16LE strings; v6 adds primitive cells.
pub const VALUE_ALLOCATE: &str = "varta.value_abi.allocate";
pub const VALUE_RETAIN: &str = "varta.value_abi.retain";
pub const VALUE_ABI_PREFIX: &str = "varta.value_abi.";
pub const VALUE_SECTION: &str = "varta.values";
pub const VALUE_VERSION: u32 = 2;
/// Binary64 payload bits in eight-byte aggregate slots; legacy tags stay frozen.
pub const NUMBER_VALUE_VERSION: u32 = 3;
pub const UNDEFINED_VALUE_VERSION: u32 = 4;
pub const UTF16_VALUE_VERSION: u32 = 5;
pub const UNION_VALUE_VERSION: u32 = 6;
pub const MISSING_VALUE_VERSION: u32 = 7;
pub const REFERENCE_VALUE_VERSION: u32 = 8;
pub const VECTOR_VALUE_VERSION: u32 = 9;
pub const ERROR_VALUE_VERSION: u32 = 10;
pub const ERROR_FAMILY_VALUE_VERSION: u32 = 11;
pub const SPARSE_VALUE_VERSION: u32 = 12;
pub const BIGINT_VALUE_VERSION: u32 = 13;
/// Callable values with module-owned capture metadata; older tags stay frozen.
pub const FUNCTION_VALUE_VERSION: u32 = 14;
/// Value ABI and layout of pilot batch v34 (value15): every value13 tag and
/// layout with its value13 meaning, plus dictionaries ([`ValueKind::Dictionary`])
/// and records wider than [`MAX_RECORD_FIELDS`]. Only value15 readers accept
/// it, and they accept nothing older.
pub const DYNAMIC_VALUE_VERSION: u32 = 15;
/// Value ABI and layout of pilot batch v35 (value16): every value15 tag and
/// layout with its value15 meaning, plus promise values
/// ([`ValueKind::Promise`]). Only value16 readers accept it.
pub const PROMISE_VALUE_VERSION: u32 = 16;
/// Value ABI and layout of pilot batch v36 (value17): every value16 tag and
/// layout with its value16 meaning, plus function values as tagged values
/// ([`ValueKind::Function`]) in record and class fields, array elements,
/// dictionary values, reference unions and `unknown` cells. Only value17
/// readers accept it.
pub const FUNCTION_FIELD_VALUE_VERSION: u32 = 17;
/// Value ABI and layout of batch v38 (value18, `dictionary_hashing`): every
/// value17 tag and layout with its value17 meaning, except that a dictionary's
/// storage record gains a third canonical slot, its open-addressing hash
/// table ([`DICTIONARY_TABLE_FIELD`]), and its entry vectors may hold holes
/// (deleted entries) while the dictionary is indexed. Only value18 readers
/// accept it.
pub const HASHED_DICTIONARY_VALUE_VERSION: u32 = 18;
/// Whether `version` is a layout of the dynamic-value family (value15 and its
/// value16, value17 and value18 extensions), which share the dictionary tag
/// and the wide-record layout (value18 adds the dictionary hash table).
#[must_use]
pub fn dynamic_layout(version: u32) -> bool {
    matches!(
        version,
        DYNAMIC_VALUE_VERSION
            | PROMISE_VALUE_VERSION
            | FUNCTION_FIELD_VALUE_VERSION
            | HASHED_DICTIONARY_VALUE_VERSION
    )
}
/// Whether `version` lays dictionaries out with a hash table (value18).
#[must_use]
pub fn hashed_dictionaries(version: u32) -> bool {
    version == HASHED_DICTIONARY_VALUE_VERSION
}
/// Whether `version` tags promise values (value16 and its value17 and
/// value18 extensions).
#[must_use]
pub fn promise_layout(version: u32) -> bool {
    matches!(
        version,
        PROMISE_VALUE_VERSION | FUNCTION_FIELD_VALUE_VERSION | HASHED_DICTIONARY_VALUE_VERSION
    )
}
/// Whether `version` admits the function tag: value14 (closure transport with
/// an owner catalog) and value17 and value18 (function fields read through
/// their closure envelope).
#[must_use]
pub fn function_layout(version: u32) -> bool {
    matches!(
        version,
        FUNCTION_VALUE_VERSION | FUNCTION_FIELD_VALUE_VERSION | HASHED_DICTIONARY_VALUE_VERSION
    )
}
/// Whether `version` reads function values as tagged values through their
/// closure envelope (value17 and value18).
#[must_use]
pub fn function_field_layout(version: u32) -> bool {
    matches!(
        version,
        FUNCTION_FIELD_VALUE_VERSION | HASHED_DICTIONARY_VALUE_VERSION
    )
}
pub const SPARSE_BITMAP_BYTES: usize = MAX_ARRAY_ELEMENTS / u8::BITS as usize;
pub const HEAP_SPARSE_FLAG: i32 = 1024;
pub const ERROR_CLASS_SHIFT: u32 = 1;
pub const ERROR_OWN_MESSAGE: u32 = 1;
pub const HEAP_ERROR_FLAG: i32 = 64;
pub const ERROR_BYTES: usize = 16;
mod error;
pub use error::{ErrorClass, ErrorObject};
/// Value15 dictionaries (v34 `index_signatures`): a dynamic-key record is a
/// two-slot record block whose canonical slots hold its key vector (`keys`,
/// slot 0, Strings) and value vector (`values`, slot 1), tagged
/// [`ValueKind::Dictionary`]. Keys are distinct and in ECMAScript own-key
/// order: canonical array indices ascending, then insertion order.
pub const DICTIONARY_KEYS_FIELD: &str = "keys";
pub const DICTIONARY_VALUES_FIELD: &str = "values";
pub const DICTIONARY_FIELDS: usize = 2;
/// Entry ceiling of one dictionary: its vectors share the array element bound.
pub const MAX_DICTIONARY_ENTRIES: usize = MAX_ARRAY_ELEMENTS;
/// Largest canonical array index (2^32 - 2); such keys enumerate first.
pub const MAX_ARRAY_INDEX: u64 = u32::MAX as u64 - 1;
/// Value18 dictionaries (v38 `dictionary_hashing`): the storage record has
/// four required canonical slots, sorted by name: `holes` (a Number, the
/// count of deleted entries still in the vectors), `keys`, `table` (an array
/// slot) and `values`. With the zero `table` handle the dictionary is
/// unindexed: its vectors are dense exactly as in value15 and `holes` is 0.
/// Otherwise `table` is an open-addressing `number[]` whose capacity equals
/// its power-of-two length, between [`DICTIONARY_TABLE_MIN`] and the element
/// limit and at least twice the vectors' length. A hole of the table is an
/// empty slot, a present `+0` a tombstone and a present `p + 1` references
/// the live entry at position `p`; every live entry is referenced exactly
/// once and found by linear probing from [`dictionary_hash`] of its key, and
/// the tombstones and live references together number the vectors' length.
/// A deleted entry of an indexed dictionary is a hole of both vectors.
pub const DICTIONARY_HOLES_FIELD: &str = "holes";
pub const DICTIONARY_TABLE_FIELD: &str = "table";
pub const HASHED_DICTIONARY_FIELDS: usize = 4;
/// Canonical value18 storage slots (fields sorted by name).
pub const HASHED_DICTIONARY_HOLES_SLOT: usize = 0;
pub const HASHED_DICTIONARY_KEYS_SLOT: usize = 1;
pub const HASHED_DICTIONARY_TABLE_SLOT: usize = 2;
pub const HASHED_DICTIONARY_VALUES_SLOT: usize = 3;
/// The smallest table: a dictionary is indexed once it holds more entries
/// than [`DICTIONARY_LINEAR_ENTRIES`], which are searched linearly.
pub const DICTIONARY_TABLE_MIN: usize = 16;
pub const DICTIONARY_LINEAR_ENTRIES: usize = 8;
/// 32-bit FNV-1a over UTF-16 code units, the deterministic key hash of
/// value18 dictionaries (no seed: identical on every host).
pub const DICTIONARY_HASH_OFFSET: u32 = 0x811c_9dc5;
pub const DICTIONARY_HASH_PRIME: u32 = 0x0100_0193;
#[must_use]
pub fn dictionary_hash(units: &[u16]) -> u32 {
    units.iter().fold(DICTIONARY_HASH_OFFSET, |hash, unit| {
        (hash ^ u32::from(*unit)).wrapping_mul(DICTIONARY_HASH_PRIME)
    })
}
/// Canonical storage fields of a dictionary of value layout `version`.
#[must_use]
pub fn dictionary_fields(version: u32) -> usize {
    if hashed_dictionaries(version) {
        HASHED_DICTIONARY_FIELDS
    } else {
        DICTIONARY_FIELDS
    }
}
/// Entry ceiling of one dictionary of value layout `version` under `limits`:
/// a value18 table holds at most the element limit of slots at load one half,
/// so a value18 dictionary holds at most half the element limit of entries.
#[must_use]
pub fn dictionary_entry_limit(version: u32, limits: crate::limits::DataLimits) -> usize {
    if hashed_dictionaries(version) {
        limits.array_elements / 2
    } else {
        limits.array_elements
    }
}
mod dictionary;
pub use dictionary::{Dictionary, array_index, dictionary_storage};
pub const HEAP_VECTOR_FLAG: i32 = 32;
pub const VECTOR_BYTES: usize = 16;
pub const VECTOR_STORAGE_OFFSET: u64 = 8;
mod vector;
pub use vector::Vector;
#[must_use]
pub fn array_capacity(length: usize) -> usize {
    if length == 0 {
        0
    } else {
        length.next_power_of_two()
    }
}
pub const OPTIONAL_FIELD_FLAG: u32 = 1 << 31;
/// The descriptor key prefix of private state (profile v37): rewritten ES
/// private names, the fields of engine prelude classes and Error slots that
/// are not own enumerable properties. V37 source cannot spell a property key
/// with it; `json_stringify_v17` skips such fields, and the exception channel
/// reads a thrown instance's `#name`/`#message` when it has no own
/// `name`/`message`.
pub const PRIVATE_KEY_PREFIX: char = '#';
#[must_use]
pub fn field_mask(count: usize) -> u32 {
    if count >= MAX_RECORD_FIELDS {
        u32::MAX
    } else {
        (1u32 << count) - 1
    }
}
#[must_use]
pub fn presence_offset(count: usize) -> usize {
    RECORD_META_BYTES + count * RECORD_SLOT_BYTES
}
/// Value15 wide records (v34 `wide_records`): a record may declare up to this
/// many fields. It is a layout ceiling of value ABI 15, chosen to cover the
/// pilot's 60-field interfaces with a margin and to fit one `u128` presence
/// set; value13 and earlier keep [`MAX_RECORD_FIELDS`].
pub const MAX_WIDE_RECORD_FIELDS: usize = 128;
pub const PRESENCE_WORD_BITS: usize = u32::BITS as usize;
/// The largest record field count a value layout admits.
#[must_use]
pub fn max_record_fields(version: u32) -> usize {
    if dynamic_layout(version) {
        MAX_WIDE_RECORD_FIELDS
    } else {
        MAX_RECORD_FIELDS
    }
}
/// Whether a record of this field count uses the value15 wide layout: a
/// multiword presence bitmap and a slot-indexed reference bitmap trailing its
/// static descriptor instead of the heap link-word mask (which stays zero).
#[must_use]
pub fn is_wide_record(count: usize) -> bool {
    count > MAX_RECORD_FIELDS
}
/// Presence bitmap words of a record trailer: one for at most 32 fields.
#[must_use]
pub fn presence_words(count: usize) -> usize {
    count.div_ceil(PRESENCE_WORD_BITS).max(1)
}
/// Byte offset (from the payload) of the presence word holding `slot`, and
/// the slot's bit within that word.
#[must_use]
pub fn presence_bit(count: usize, slot: usize) -> (usize, u32) {
    (
        presence_offset(count) + (slot / PRESENCE_WORD_BITS) * size_of::<u32>(),
        1u32 << (slot % PRESENCE_WORD_BITS),
    )
}
/// Byte offset (from the payload) of the first insertion-order word.
#[must_use]
pub fn order_offset(count: usize) -> usize {
    presence_offset(count) + presence_words(count) * size_of::<u32>()
}
/// Every slot bit of a record with `count` fields.
#[must_use]
pub fn wide_field_mask(count: usize) -> u128 {
    if count >= MAX_WIDE_RECORD_FIELDS {
        u128::MAX
    } else {
        (1u128 << count) - 1
    }
}
/// Read `words` consecutive little-endian presence or mask words at `at` as one
/// slot-indexed set; `None` if the range is outside `data`.
#[must_use]
pub fn read_slot_set(data: &[u8], at: usize, words: usize) -> Option<u128> {
    if words > MAX_WIDE_RECORD_FIELDS / PRESENCE_WORD_BITS {
        return None;
    }
    let mut set = 0u128;
    for index in 0..words {
        let start = at.checked_add(index * size_of::<u32>())?;
        let bytes: [u8; 4] = data.get(start..start.checked_add(4)?)?.try_into().ok()?;
        set |= u128::from(u32::from_le_bytes(bytes)) << (index * PRESENCE_WORD_BITS);
    }
    Some(set)
}
/// Bytes of a static record descriptor: header, entries and, for a wide
/// record, its slot-indexed reference bitmap.
#[must_use]
pub fn descriptor_bytes(count: usize) -> usize {
    RECORD_DESCRIPTOR_HEADER
        + count * RECORD_DESCRIPTOR_ENTRY
        + if is_wide_record(count) {
            presence_words(count) * size_of::<u32>()
        } else {
            0
        }
}
#[must_use]
pub fn record_bytes(version: u32, count: usize) -> usize {
    presence_offset(count)
        + if matches!(
            version,
            MISSING_VALUE_VERSION
                | REFERENCE_VALUE_VERSION
                | VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
                | PROMISE_VALUE_VERSION
                | FUNCTION_FIELD_VALUE_VERSION
                | HASHED_DICTIONARY_VALUE_VERSION
        ) {
            size_of::<u32>() * (count + presence_words(count))
        } else {
            0
        }
}
pub const HEAP_UNION_FLAG: i32 = 16;
pub const UNION_META_BYTES: usize = 8;
pub const UNION_BYTES: usize = UNION_META_BYTES + RECORD_SLOT_BYTES;
pub const MAX_UNION_TYPES: usize = 128;
mod primitive;
pub use primitive::PrimitiveBox;
mod reference;
pub use reference::{ReferenceBox, validate_aggregate, validate_aggregate_in};

/// What a module's layout-bound readers may accept (v38 `program_limits`
/// foundation): the static region holding its descriptors, keys and schemas,
/// and its profile's structural limits (closure codes and signatures). Every
/// module before v38 reads with [`ModuleBounds::LEGACY`]. Comes from validated
/// module metadata, never from guest memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleBounds {
    pub static_region: crate::abi::StaticRegion,
    pub limits: crate::limits::ProgramLimits,
}
impl ModuleBounds {
    pub const LEGACY: Self = Self {
        static_region: crate::abi::StaticRegion::SINGLE_PAGE,
        limits: crate::limits::FROZEN,
    };
    /// The bounds of a validated module of `profile` with `static_region`.
    #[must_use]
    pub fn new(
        profile: crate::language::LanguageProfile,
        static_region: crate::abi::StaticRegion,
    ) -> Self {
        Self {
            static_region,
            limits: profile.limits(),
        }
    }
}
impl Default for ModuleBounds {
    fn default() -> Self {
        Self::LEGACY
    }
}
pub const LEGACY_VALUE_VERSION: u32 = 1;
/// Version 2 adds dense arrays; existing scalar/record layouts retain their meaning.
#[must_use]
pub fn supported_version(version: u32) -> bool {
    matches!(
        version,
        LEGACY_VALUE_VERSION
            | VALUE_VERSION
            | NUMBER_VALUE_VERSION
            | UNDEFINED_VALUE_VERSION
            | UTF16_VALUE_VERSION
            | UNION_VALUE_VERSION
            | MISSING_VALUE_VERSION
            | REFERENCE_VALUE_VERSION
            | VECTOR_VALUE_VERSION
            | ERROR_VALUE_VERSION
            | ERROR_FAMILY_VALUE_VERSION
            | SPARSE_VALUE_VERSION
            | BIGINT_VALUE_VERSION
            | FUNCTION_VALUE_VERSION
            | DYNAMIC_VALUE_VERSION
            | PROMISE_VALUE_VERSION
            | FUNCTION_FIELD_VALUE_VERSION
            | HASHED_DICTIONARY_VALUE_VERSION
    )
}
#[must_use]
pub fn compatible_version(actual: u32, selected: u32) -> bool {
    if !supported_version(selected) {
        return false;
    }
    if selected >= NUMBER_VALUE_VERSION {
        actual == selected
    } else {
        matches!(actual, LEGACY_VALUE_VERSION | VALUE_VERSION)
    }
}
pub const MAX_ARRAY_ELEMENTS: usize = 1024;
pub const MAX_ARRAY_TYPES: usize = 128;
pub const ARRAY_META_BYTES: usize = 8;
pub const ARRAY_SLOT_BYTES: usize = 8;
pub const ARRAY_LENGTH_OFFSET: u64 = 4;
pub const HEAP_ARRAY_FLAG: i32 = 8;
pub const MAX_RECORD_FIELDS: usize = 32;
pub const MAX_RECORD_SHAPES: usize = 128;
pub const MAX_RECORD_KEY_BYTES: usize = 256;
pub const MAX_JSON_DEPTH: usize = 64;
pub const MAX_JSON_NODES: usize = 4096;
pub const RECORD_META_BYTES: usize = 8;
pub const RECORD_SLOT_BYTES: usize = 8;
pub const RECORD_DESCRIPTOR_HEADER: usize = 8;
pub const RECORD_DESCRIPTOR_ENTRY: usize = 16;
pub const HEAP_ALIGNMENT: usize = 8;
pub const HEAP_ALLOCATED_FLAG: u32 = 1;
pub const HEAP_FLAGS_OFFSET: usize = 4;
pub const HEAP_LINK_OFFSET: usize = 8;
pub const HEAP_RECORD_FLAG: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ValueKind {
    Null = 0,
    I32 = 1,
    Boolean = 2,
    String = 3,
    Record = 4,
    Array = 5,
    Float64 = 6,
    Undefined = 7,
    PrimitiveUnion = 8,
    ReferenceUnion = 9,
    Error = 10,
    BigInt = 11,
    Function = 12,
    /// Value15 dynamic-key record (see [`Dictionary`]).
    Dictionary = 13,
    /// Value16 promise (v35 `promise_values`): the payload is the handle of
    /// the promise's private record block.
    Promise = 14,
}
impl ValueKind {
    #[must_use]
    pub fn is_reference(self) -> bool {
        matches!(
            self,
            Self::Dictionary
                | Self::Promise
                | Self::Function
                | Self::BigInt
                | Self::Error
                | Self::String
                | Self::Record
                | Self::Array
                | Self::PrimitiveUnion
                | Self::ReferenceUnion
        )
    }
    #[must_use]
    pub fn from_tag(tag: u32) -> Option<Self> {
        Some(match tag {
            0 => Self::Null,
            1 => Self::I32,
            2 => Self::Boolean,
            3 => Self::String,
            4 => Self::Record,
            5 => Self::Array,
            6 => Self::Float64,
            7 => Self::Undefined,
            8 => Self::PrimitiveUnion,
            9 => Self::ReferenceUnion,
            10 => Self::Error,
            11 => Self::BigInt,
            12 => Self::Function,
            13 => Self::Dictionary,
            14 => Self::Promise,
            _ => return None,
        })
    }
    #[must_use]
    pub fn supported_by(self, version: u32) -> bool {
        supported_version(version)
            && match self {
                Self::Function => function_layout(version),
                Self::Dictionary => dynamic_layout(version),
                Self::Promise => promise_layout(version),
                Self::BigInt => matches!(
                    version,
                    BIGINT_VALUE_VERSION
                        | FUNCTION_VALUE_VERSION
                        | DYNAMIC_VALUE_VERSION
                        | PROMISE_VALUE_VERSION
                        | FUNCTION_FIELD_VALUE_VERSION
                        | HASHED_DICTIONARY_VALUE_VERSION
                ),
                Self::Float64 => matches!(
                    version,
                    NUMBER_VALUE_VERSION
                        | UNDEFINED_VALUE_VERSION
                        | UTF16_VALUE_VERSION
                        | UNION_VALUE_VERSION
                        | MISSING_VALUE_VERSION
                        | REFERENCE_VALUE_VERSION
                        | VECTOR_VALUE_VERSION
                        | ERROR_VALUE_VERSION
                        | ERROR_FAMILY_VALUE_VERSION
                        | SPARSE_VALUE_VERSION
                        | BIGINT_VALUE_VERSION
                        | FUNCTION_VALUE_VERSION
                        | DYNAMIC_VALUE_VERSION
                        | PROMISE_VALUE_VERSION
                        | FUNCTION_FIELD_VALUE_VERSION
                        | HASHED_DICTIONARY_VALUE_VERSION
                ),
                Self::Undefined => matches!(
                    version,
                    UNDEFINED_VALUE_VERSION
                        | UTF16_VALUE_VERSION
                        | UNION_VALUE_VERSION
                        | MISSING_VALUE_VERSION
                        | REFERENCE_VALUE_VERSION
                        | VECTOR_VALUE_VERSION
                        | ERROR_VALUE_VERSION
                        | ERROR_FAMILY_VALUE_VERSION
                        | SPARSE_VALUE_VERSION
                        | BIGINT_VALUE_VERSION
                        | FUNCTION_VALUE_VERSION
                        | DYNAMIC_VALUE_VERSION
                        | PROMISE_VALUE_VERSION
                        | FUNCTION_FIELD_VALUE_VERSION
                        | HASHED_DICTIONARY_VALUE_VERSION
                ),
                Self::PrimitiveUnion => {
                    matches!(
                        version,
                        UNION_VALUE_VERSION
                            | MISSING_VALUE_VERSION
                            | REFERENCE_VALUE_VERSION
                            | VECTOR_VALUE_VERSION
                            | ERROR_VALUE_VERSION
                            | ERROR_FAMILY_VALUE_VERSION
                            | SPARSE_VALUE_VERSION
                            | BIGINT_VALUE_VERSION
                            | FUNCTION_VALUE_VERSION
                            | DYNAMIC_VALUE_VERSION
                            | PROMISE_VALUE_VERSION
                            | FUNCTION_FIELD_VALUE_VERSION
                            | HASHED_DICTIONARY_VALUE_VERSION
                    )
                }
                Self::ReferenceUnion => {
                    matches!(
                        version,
                        REFERENCE_VALUE_VERSION
                            | VECTOR_VALUE_VERSION
                            | ERROR_VALUE_VERSION
                            | ERROR_FAMILY_VALUE_VERSION
                            | SPARSE_VALUE_VERSION
                            | BIGINT_VALUE_VERSION
                            | FUNCTION_VALUE_VERSION
                            | DYNAMIC_VALUE_VERSION
                            | PROMISE_VALUE_VERSION
                            | FUNCTION_FIELD_VALUE_VERSION
                            | HASHED_DICTIONARY_VALUE_VERSION
                    )
                }
                Self::Error => matches!(
                    version,
                    ERROR_VALUE_VERSION
                        | ERROR_FAMILY_VALUE_VERSION
                        | SPARSE_VALUE_VERSION
                        | BIGINT_VALUE_VERSION
                        | FUNCTION_VALUE_VERSION
                        | DYNAMIC_VALUE_VERSION
                        | PROMISE_VALUE_VERSION
                        | FUNCTION_FIELD_VALUE_VERSION
                        | HASHED_DICTIONARY_VALUE_VERSION
                ),
                Self::I32 => matches!(version, LEGACY_VALUE_VERSION | VALUE_VERSION),
                _ => true,
            }
    }
}

/// Malformed guest exports/call graphs for managed value transfer.
#[derive(Debug, thiserror::Error)]
#[error("invalid value ABI: {0}")]
pub struct ValueAbiError(pub String);
