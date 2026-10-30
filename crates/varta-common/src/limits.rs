//! Profile-dependent data limits (batch v38 `data_limits`).
//!
//! Profiles before v38 keep their frozen per-value ceilings: 1024 elements per
//! array, dictionary, collection and spread call, 1 MiB per string value, JSON
//! depth 64 and 4096 visited values. Under `data_limits()` these ceilings are
//! lifted to the limits of the 32-bit Wasm address space and of the guest's
//! own size arithmetic, so the memory budget (and fuel and the deadline) is
//! what bounds data in practice, as in a JavaScript engine.
//!
//! A v38 guest that exhausts a real resource records which one in its
//! exported [`RESOURCE_FAILURE`] global (and the requested amount in
//! [`RESOURCE_REQUEST`]) immediately before it traps, and the runtime reports
//! the trap as a typed [`ResourceExhausted`] error instead of an anonymous
//! `unreachable` trap. The globals carry no authority: they only select the
//! diagnostic of an execution that already failed.
use crate::{abi, value};

/// Largest element count of one v38 array (dense, resizable or sparse). The
/// sparse backing of this capacity (eight metadata bytes, eight bytes per
/// slot and one presence bit per slot) and its heap header stay below 2^31
/// bytes, so every size the guest computes fits a signed i32.
pub const V38_MAX_ARRAY_ELEMENTS: usize = 1 << 27;
/// Largest byte length of one v38 string value: 2^29 UTF-16 code units, the
/// order of V8's maximum string length. Beyond it ECMAScript engines raise
/// "Invalid string length"; Varta reports [`Resource::ValueSize`].
pub const V38_MAX_STRING_BYTES: usize = 1 << 30;
/// Largest heap block a v38 allocator request may name: every v38 value
/// (the largest sparse backing included) fits, and the rounded block size
/// with its header still fits a signed i32.
pub const V38_MAX_ALLOCATION_BYTES: usize = (1 << 31) - (1 << 16);
/// Exported mutable i32 global of v38 heap modules: zero, or the
/// [`Resource`] code a failing guest stored before trapping.
pub const RESOURCE_FAILURE: &str = "resource_failure";
/// Exported mutable i32 global of v38 heap modules: the requested amount
/// (bytes or entries, unsigned) that accompanies [`RESOURCE_FAILURE`].
pub const RESOURCE_REQUEST: &str = "resource_request";

/// A resource a v38 guest can exhaust. The code is what the guest stores in
/// [`RESOURCE_FAILURE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Resource {
    /// The memory budget: growing linear memory failed or would exceed it.
    Memory = 1,
    /// One value (a string, an array backing) beyond the profile's
    /// Wasm-derived size limit.
    ValueSize = 2,
    /// The exact shadow root stack.
    RootStack = 3,
    /// The live continuation-frame table.
    LiveFrames = 4,
}
impl Resource {
    #[must_use]
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            1 => Self::Memory,
            2 => Self::ValueSize,
            3 => Self::RootStack,
            4 => Self::LiveFrames,
            _ => return None,
        })
    }
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Memory => "memory budget exhausted",
            Self::ValueSize => "value exceeds the Wasm size limit",
            Self::RootStack => "root stack exhausted",
            Self::LiveFrames => "live continuation frames exhausted",
        }
    }
}

/// A fatal resource failure that a v38 guest reported before trapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("resource limit: {} (requested {requested})", resource.describe())]
pub struct ResourceExhausted {
    pub resource: Resource,
    /// Bytes or entries requested, as the guest reported them.
    pub requested: u32,
}

/// Per-value data limits of a language profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataLimits {
    /// Elements of one array, entries of one dictionary, `Map`, `Set` or
    /// `Uint8Array`, and flattened spread-call arguments.
    pub array_elements: usize,
    /// Bytes of one string value (UTF-16 in UTF-16 profiles).
    pub string_bytes: usize,
    /// Largest guest heap block request.
    pub allocation_bytes: usize,
    /// Nesting depth of a JSON value the host serializes or parses.
    pub json_depth: usize,
    /// Values visited by one JSON operation (aliases counted again).
    pub json_nodes: usize,
    /// Whether the guest reports exhausted resources through
    /// [`RESOURCE_FAILURE`] before trapping.
    pub reports_resources: bool,
}
impl DataLimits {
    /// The frozen ceilings of every profile before v38.
    pub const LEGACY: Self = Self {
        array_elements: value::MAX_ARRAY_ELEMENTS,
        string_bytes: abi::MAX_STRING_BYTES,
        allocation_bytes: abi::MAX_STRING_BYTES,
        json_depth: value::MAX_JSON_DEPTH,
        json_nodes: value::MAX_JSON_NODES,
        reports_resources: false,
    };
    /// Batch v38 (`data_limits`): Wasm-derived ceilings; the memory budget
    /// bounds data in practice. JSON depth and visited values are bounded by
    /// the encoded size, which the memory budget bounds.
    pub const V38: Self = Self {
        array_elements: V38_MAX_ARRAY_ELEMENTS,
        string_bytes: V38_MAX_STRING_BYTES,
        allocation_bytes: V38_MAX_ALLOCATION_BYTES,
        json_depth: usize::MAX,
        json_nodes: usize::MAX,
        reports_resources: true,
    };
    /// Presence-bitmap bytes of a sparse backing of `capacity` slots: the
    /// fixed 128-byte bitmap of value layout 12 up to 1024 slots (so every
    /// backing an earlier profile can build keeps its layout), one bit per
    /// slot beyond.
    #[must_use]
    pub const fn sparse_bitmap_bytes(capacity: usize) -> usize {
        let bits = capacity / u8::BITS as usize;
        if bits > value::SPARSE_BITMAP_BYTES {
            bits
        } else {
            value::SPARSE_BITMAP_BYTES
        }
    }
}

impl DataLimits {
    /// Slot capacity of a sparse backing whose packed byte length is `bytes`
    /// (metadata, slots and the bitmap of [`Self::sparse_bitmap_bytes`]),
    /// when it is a valid power of two (or zero) within these limits.
    #[must_use]
    pub fn sparse_capacity(self, bytes: usize) -> Option<usize> {
        // Beyond 1024 slots each slot takes eight payload bytes and one bit.
        const SLOT_BITS: usize = value::ARRAY_SLOT_BYTES * u8::BITS as usize + 1;
        let fixed = value::ARRAY_META_BYTES + value::SPARSE_BITMAP_BYTES;
        let small = bytes
            .checked_sub(fixed)
            .filter(|slots| slots.is_multiple_of(value::ARRAY_SLOT_BYTES))
            .map(|slots| slots / value::ARRAY_SLOT_BYTES)
            .filter(|capacity| *capacity <= value::MAX_ARRAY_ELEMENTS);
        let large = bytes
            .checked_sub(value::ARRAY_META_BYTES)
            .filter(|rest| rest.is_multiple_of(SLOT_BITS))
            .map(|rest| rest / SLOT_BITS * u8::BITS as usize)
            .filter(|capacity| *capacity > value::MAX_ARRAY_ELEMENTS);
        small
            .or(large)
            .filter(|capacity| *capacity == 0 || capacity.is_power_of_two())
            .filter(|capacity| *capacity <= self.array_elements)
    }
}

// The largest v38 sparse backing and its header fit the allocation limit.
const _: () = assert!(
    abi::STRING_HEADER_BYTES
        + value::ARRAY_META_BYTES
        + V38_MAX_ARRAY_ELEMENTS * value::ARRAY_SLOT_BYTES
        + DataLimits::sparse_bitmap_bytes(V38_MAX_ARRAY_ELEMENTS)
        <= V38_MAX_ALLOCATION_BYTES
        && V38_MAX_STRING_BYTES + abi::STRING_HEADER_BYTES <= V38_MAX_ALLOCATION_BYTES
        && V38_MAX_ALLOCATION_BYTES < i32::MAX as usize
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::{LanguageProfile, SUPPORTED_PROFILES};

    #[test]
    fn only_v38_lifts_the_data_ceilings() {
        for &profile in SUPPORTED_PROFILES {
            let limits = profile.data();
            if profile == LanguageProfile::CheckedF64Utf16V38 {
                assert_eq!(limits, DataLimits::V38);
            } else {
                assert_eq!(limits, DataLimits::LEGACY, "{profile:?}");
            }
        }
        assert_eq!(DataLimits::LEGACY.array_elements, 1024);
        assert_eq!(DataLimits::LEGACY.string_bytes, 1 << 20);
    }

    #[test]
    fn sparse_bitmaps_keep_the_value12_layout_up_to_1024_slots() {
        for capacity in [0, 1, 2, 512, 1024] {
            assert_eq!(DataLimits::sparse_bitmap_bytes(capacity), 128);
        }
        assert_eq!(DataLimits::sparse_bitmap_bytes(2048), 256);
        assert_eq!(
            DataLimits::sparse_bitmap_bytes(V38_MAX_ARRAY_ELEMENTS),
            V38_MAX_ARRAY_ELEMENTS / 8
        );
    }

    #[test]
    fn sparse_capacities_invert_the_backing_size_within_each_profile() {
        let bytes = |capacity: usize| {
            value::ARRAY_META_BYTES
                + capacity * value::ARRAY_SLOT_BYTES
                + DataLimits::sparse_bitmap_bytes(capacity)
        };
        for capacity in [0, 1, 2, 64, 1024] {
            assert_eq!(
                DataLimits::LEGACY.sparse_capacity(bytes(capacity)),
                Some(capacity)
            );
            assert_eq!(
                DataLimits::V38.sparse_capacity(bytes(capacity)),
                Some(capacity)
            );
        }
        for capacity in [2048, 1 << 20, V38_MAX_ARRAY_ELEMENTS] {
            assert_eq!(DataLimits::LEGACY.sparse_capacity(bytes(capacity)), None);
            assert_eq!(
                DataLimits::V38.sparse_capacity(bytes(capacity)),
                Some(capacity)
            );
        }
        for bad in [0, 7, bytes(3), bytes(2048) + 65, bytes(1024) + 8] {
            assert_eq!(DataLimits::V38.sparse_capacity(bad), None, "{bad}");
        }
    }

    #[test]
    fn resource_codes_round_trip() {
        for resource in [
            Resource::Memory,
            Resource::ValueSize,
            Resource::RootStack,
            Resource::LiveFrames,
        ] {
            assert_eq!(Resource::from_code(resource as i32), Some(resource));
        }
        assert_eq!(Resource::from_code(0), None);
        assert_eq!(Resource::from_code(5), None);
        assert_eq!(
            ResourceExhausted {
                resource: Resource::Memory,
                requested: 64
            }
            .to_string(),
            "resource limit: memory budget exhausted (requested 64)"
        );
    }
}
