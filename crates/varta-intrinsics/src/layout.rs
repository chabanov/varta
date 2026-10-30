//! The guest memory layout constants the linked intrinsics read.
//!
//! `varta-common` owns these contracts (`abi`, `value`); this crate cannot
//! depend on it (common is `std` and the engine root), so the values the
//! intrinsics need are restated here and the runtime test
//! `intrinsic_layout_matches_common` fails when the two drift.

/// Host results are written here (`abi::RESULT_BASE`).
pub const RESULT_BASE: usize = 65_536;
/// One host result / one string value (`abi::MAX_RESULT_BYTES`).
pub const MAX_RESULT_BYTES: usize = 1024 * 1024;
/// One string value (`abi::MAX_STRING_BYTES`).
pub const MAX_STRING_BYTES: usize = MAX_RESULT_BYTES;
/// Start of the managed heap (`abi::OWNED_STRING_BASE`).
pub const OWNED_STRING_BASE: usize = RESULT_BASE + MAX_RESULT_BYTES + 65_536;
/// Heap block header (`abi::STRING_HEADER_BYTES`).
pub const STRING_HEADER_BYTES: usize = 16;
/// Wasm page (`abi::WASM_PAGE_BYTES`).
pub const WASM_PAGE_BYTES: usize = 65_536;
/// `value::HEAP_ALIGNMENT`.
pub const HEAP_ALIGNMENT: usize = 8;
/// `value::HEAP_FLAGS_OFFSET`.
pub const HEAP_FLAGS_OFFSET: usize = 4;
/// `value::HEAP_LINK_OFFSET`.
pub const HEAP_LINK_OFFSET: usize = 8;
/// `value::HEAP_ALLOCATED_FLAG`.
pub const HEAP_ALLOCATED_FLAG: u32 = 1;
/// `value::HEAP_UNION_FLAG`.
pub const HEAP_UNION_FLAG: u32 = 16;
/// `value::UNION_META_BYTES`.
pub const UNION_META_BYTES: usize = 8;
/// `value::UNION_BYTES` (`UNION_META_BYTES + RECORD_SLOT_BYTES`).
pub const UNION_BYTES: usize = UNION_META_BYTES + 8;

/// Value tags (`value::ValueKind`), in tag order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Kind {
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
    Dictionary = 13,
    Promise = 14,
}
impl Kind {
    /// Every tag, in tag order.
    pub const ALL: &[Self] = &[
        Self::Null,
        Self::I32,
        Self::Boolean,
        Self::String,
        Self::Record,
        Self::Array,
        Self::Float64,
        Self::Undefined,
        Self::PrimitiveUnion,
        Self::ReferenceUnion,
        Self::Error,
        Self::BigInt,
        Self::Function,
        Self::Dictionary,
        Self::Promise,
    ];
    #[must_use]
    pub fn from_tag(tag: u32) -> Option<Self> {
        Self::ALL.get(usize::try_from(tag).ok()?).copied()
    }
    /// `value::ValueKind::is_reference`.
    #[must_use]
    pub fn is_reference(self) -> bool {
        !matches!(
            self,
            Self::Null | Self::I32 | Self::Boolean | Self::Float64 | Self::Undefined
        )
    }
}
