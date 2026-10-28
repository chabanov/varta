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
pub const SPARSE_BITMAP_BYTES: usize = MAX_ARRAY_ELEMENTS / u8::BITS as usize;
pub const HEAP_SPARSE_FLAG: i32 = 1024;
pub const ERROR_CLASS_SHIFT: u32 = 1;
pub const ERROR_OWN_MESSAGE: u32 = 1;
pub const HEAP_ERROR_FLAG: i32 = 64;
pub const ERROR_BYTES: usize = 16;
mod error;
pub use error::{ErrorClass, ErrorObject};
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
        ) {
            size_of::<u32>() * (count + 1)
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
pub use reference::{ReferenceBox, validate_aggregate};
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
}
impl ValueKind {
    #[must_use]
    pub fn is_reference(self) -> bool {
        matches!(
            self,
            Self::Function
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
            _ => return None,
        })
    }
    #[must_use]
    pub fn supported_by(self, version: u32) -> bool {
        supported_version(version)
            && match self {
                Self::Function => version == FUNCTION_VALUE_VERSION,
                Self::BigInt => matches!(version, BIGINT_VALUE_VERSION | FUNCTION_VALUE_VERSION),
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
                    )
                }
                Self::Error => matches!(
                    version,
                    ERROR_VALUE_VERSION
                        | ERROR_FAMILY_VALUE_VERSION
                        | SPARSE_VALUE_VERSION
                        | BIGINT_VALUE_VERSION
                        | FUNCTION_VALUE_VERSION
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
