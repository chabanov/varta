//! Program and data limits by language profile: the one registry the
//! compiler, the runtime's module, catalog and value readers, the artifact
//! and launcher readers and the generated documentation read. A module's
//! readers take it through one parameter, [`crate::value::ModuleBounds`].
//!
//! Every profile before v38 keeps [`FROZEN`], the historical constants in
//! `abi`, `value`, `function` and `json`. A `program_limits()` profile (v38)
//! uses [`WASM`]: structural limits are the WebAssembly implementation limits
//! that Wasmtime enforces through `wasmparser` and that the WebAssembly
//! JavaScript API fixes for every engine (see
//! <https://webassembly.github.io/spec/js-api/#limits>). Entities without a
//! Wasm counterpart (record shapes, unions, array, promise, generator and
//! function types, JSON schema and capture types) take the same uniform
//! 1,000,000 entity bound as the Wasm type and function index spaces they
//! lower to; in practice module size, static memory and compile time bound
//! them much earlier. Production runtime budgets (fuel, memory, deadline,
//! host calls) are not structural limits and are not part of this registry.
//!
//! Per-value data limits ([`DataLimits`], batch v38 `data_limits`) are one
//! field of each profile's entry. Profiles before v38 keep their frozen
//! per-value ceilings: 1024 elements per array, dictionary, collection and
//! spread call, 1 MiB per string value, JSON depth 64 and 4096 visited
//! values. Under v38 these ceilings are lifted to the limits of the 32-bit
//! Wasm address space and of the guest's own size arithmetic, so the memory
//! budget (and fuel and the deadline) is what bounds data in practice, as in
//! a JavaScript engine.
//!
//! A v38 guest that exhausts a real resource records which one in its
//! exported [`RESOURCE_FAILURE`] global (and the requested amount in
//! [`RESOURCE_REQUEST`]) immediately before it traps, and the runtime reports
//! the trap as a typed [`ResourceExhausted`] error instead of an anonymous
//! `unreachable` trap. The globals carry no authority: they only select the
//! diagnostic of an execution that already failed.
//!
//! Without the `std` feature (the value-layout core) this module keeps the
//! limit constants, [`DataLimits`], [`ProgramLimits`] and the v38 limits
//! ([`WASM`], [`DataLimits::V38`]) the linked readers of a v38 module use;
//! the profile tables and frozen ceilings that name engine modules are `std`
//! only.
use crate::{abi, value};
#[cfg(feature = "std")]
use crate::{function, json, language::LanguageProfile};

/// The JS API limit on functions (imported plus defined) per module, which
/// `wasmparser` (`MAX_WASM_FUNCTIONS`) enforces for Wasmtime.
pub const WASM_FUNCTIONS: usize = 1_000_000;
/// The JS API limit on types per module (`MAX_WASM_TYPES`).
pub const WASM_TYPES: usize = 1_000_000;
/// The JS API limit on one function body (`MAX_WASM_FUNCTION_SIZE`).
pub const WASM_FUNCTION_BODY_BYTES: usize = 7_654_321;
/// The JS API limit on a module's encoded size (1 GiB, as in V8 and
/// SpiderMonkey); `wasmparser` itself has no module-size limit.
pub const WASM_MODULE_BYTES: usize = 1 << 30;
/// The JS API limit on data segments per module (`MAX_WASM_DATA_SEGMENTS`).
pub const WASM_DATA_SEGMENTS: usize = 100_000;
/// The uniform bound on Varta entity tables that have no Wasm counterpart.
pub const ENTITIES: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramLimits {
    /// User functions in the emitted module (the frozen limit excludes the
    /// compiler helpers and the HTTP adapter; the Wasm limit bounds the
    /// whole function index space, imports included).
    pub user_functions: usize,
    /// Defined plus imported Wasm functions, checked by the runtime.
    pub wasm_functions: usize,
    /// Distinct Wasm function signatures.
    pub wasm_signatures: usize,
    /// Encoded module bytes accepted by the compiler, runtime and artifact
    /// readers.
    pub wasm_bytes: usize,
    /// Encoded bytes of one function body.
    pub function_body_bytes: usize,
    /// Record shapes per program; classes have their own table of the same
    /// size.
    pub record_shapes: usize,
    pub union_types: usize,
    /// Structural (non-literal) members of one union type; literal members
    /// merge into constrained primitives and have their own compiler bound.
    pub union_members: usize,
    /// Members of one checked JSON schema union (schema 5+). Members also
    /// need distinct JSON kinds, which bounds a valid union further.
    pub json_union_members: usize,
    pub array_types: usize,
    /// `Promise<T>` types (v34+).
    pub promise_types: usize,
    /// `Generator`/`AsyncGenerator` types (v33+).
    pub generator_types: usize,
    /// Function signature types of typed function values (v11+).
    pub function_types: usize,
    /// Encoded bytes of one checked JSON schema.
    pub schema_bytes: usize,
    /// Static data in the first page and the extension.
    pub static_extension_pages: u32,
    /// Per-value data limits (v38 `data_limits`).
    pub data: DataLimits,
}

/// The limits of every profile before v38, byte for byte the historical
/// constants.
#[cfg(feature = "std")]
pub const FROZEN: ProgramLimits = ProgramLimits {
    user_functions: abi::MAX_USER_FUNCTIONS,
    wasm_functions: abi::MAX_USER_FUNCTIONS + abi::MAX_INTERNAL_FUNCTIONS + abi::MAX_HTTP_HANDLERS,
    wasm_signatures: abi::MAX_WASM_SIGNATURES,
    wasm_bytes: abi::MAX_WASM_BYTES,
    function_body_bytes: abi::MAX_WASM_FUNCTION_BODY_BYTES,
    record_shapes: value::MAX_RECORD_SHAPES,
    union_types: value::MAX_UNION_TYPES,
    union_members: MAX_UNION_MEMBERS,
    json_union_members: json::MAX_UNION_MEMBERS,
    array_types: value::MAX_ARRAY_TYPES,
    promise_types: MAX_PROMISE_TYPES,
    generator_types: function::frame::MAX_GENERATOR_TYPES,
    function_types: function::MAX_TYPES,
    schema_bytes: json::MAX_SCHEMA_BYTES,
    static_extension_pages: 0,
    data: DataLimits::LEGACY,
};

/// The `program_limits()` (v38) limits: the Wasm implementation limits.
pub const WASM: ProgramLimits = ProgramLimits {
    user_functions: WASM_FUNCTIONS,
    wasm_functions: WASM_FUNCTIONS,
    wasm_signatures: WASM_TYPES,
    wasm_bytes: WASM_MODULE_BYTES,
    function_body_bytes: WASM_FUNCTION_BODY_BYTES,
    record_shapes: ENTITIES,
    union_types: ENTITIES,
    // A union's members are tested one by one (descriptor tests, a compare
    // chain on a discriminant), so module size and compile time bound them
    // long before the entity bound.
    union_members: ENTITIES,
    json_union_members: ENTITIES,
    array_types: ENTITIES,
    promise_types: ENTITIES,
    generator_types: ENTITIES,
    function_types: ENTITIES,
    // A schema lives in static memory, bounded by the memory budget.
    schema_bytes: abi::MAX_MEMORY_BYTES,
    static_extension_pages: abi::MAX_STATIC_EXTENSION_PAGES,
    data: DataLimits::V38,
};

/// The frozen `Promise<T>` type limit (v34 to v37), formerly compiler-local.
pub const MAX_PROMISE_TYPES: usize = 64;
/// The frozen structural member limit of one union type (v2 to v37),
/// formerly compiler-local.
pub const MAX_UNION_MEMBERS: usize = 16;

/// The largest limits any profile admits: readers that must bound input
/// before they know its profile (the Wasm size before language inspection,
/// artifact payloads) use these and then the profile's own limits.
#[cfg(feature = "std")]
pub const MAXIMUM: ProgramLimits = WASM;

impl ProgramLimits {
    /// JSON schema types: the four primitive slots, records and arrays, plus
    /// (schema 15+) the dictionary slot and unions.
    #[must_use]
    pub fn schema_types(&self, unions: bool) -> usize {
        4 + self.record_shapes + self.array_types + if unions { 1 + self.union_types } else { 0 }
    }
    /// Distinct capture types of one function catalog.
    #[must_use]
    pub fn capture_types(&self) -> usize {
        self.function_types + self.record_shapes + self.array_types + 2 * self.union_types + 32
    }
}

#[cfg(feature = "std")]
impl LanguageProfile {
    /// This profile's structural limits: [`WASM`] for a `program_limits()`
    /// profile, [`FROZEN`] for every earlier one.
    #[must_use]
    pub fn limits(self) -> ProgramLimits {
        if self.program_limits() { WASM } else { FROZEN }
    }
    /// This profile's per-value data limits: the `data` field of
    /// [`Self::limits`].
    #[must_use]
    pub fn data(self) -> DataLimits {
        self.limits().data
    }
}

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
/// Smallest guest block of one live continuation frame: its heap header and
/// the frame envelope. Every pending resume token and every pending timer
/// names a distinct live frame, so the host tracks at most one entry per such
/// block of the memory budget.
pub const FRAME_BLOCK_BYTES: usize = abi::STRING_HEADER_BYTES + crate::function::FRAME_BYTES;
/// V38 host bound on pending resume tokens (queued or awaiting a promise):
/// the memory budget divided by [`FRAME_BLOCK_BYTES`] (131072 under the
/// production 8 MiB budget). A correct module exhausts its memory budget,
/// which it reports as [`Resource::Memory`], before it reaches this bound.
pub const V38_MAX_PENDING_CONTINUATIONS: usize = abi::MAX_MEMORY_BYTES / FRAME_BLOCK_BYTES;
/// V38 host bound on pending timers: half the continuation bound, as before
/// v38 (each timer also holds one resume token).
pub const V38_MAX_PENDING_TIMERS: usize = V38_MAX_PENDING_CONTINUATIONS / 2;
/// Nesting depth of one v38 JSON value the host serializes or parses: a host
/// ceiling, as a JavaScript engine's native stack bounds its recursive
/// `JSON.stringify`. The host traverses such values on a dedicated JSON
/// stack sized for this depth; exceeding it is a reported JSON limit error.
pub const V38_MAX_JSON_DEPTH: usize = 10_000;
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
    /// Bytes of UTF-8 JSON source one checked parse accepts.
    pub json_input_bytes: usize,
    /// Whether the guest reports exhausted resources through
    /// [`RESOURCE_FAILURE`] before trapping.
    pub reports_resources: bool,
    /// Whether the guest heap grows geometrically (by at least half its
    /// committed pages, within the memory budget) and marks through a
    /// bounded worklist, so collection work stays amortized linear in the
    /// allocated bytes while live data grows.
    pub amortized_growth: bool,
    /// Resume tokens the host scheduler holds at once.
    pub pending_continuations: usize,
    /// Timers armed or awaiting release at once.
    pub pending_timers: usize,
}
impl DataLimits {
    /// The frozen ceilings of every profile before v38.
    #[cfg(feature = "std")]
    pub const LEGACY: Self = Self {
        array_elements: value::MAX_ARRAY_ELEMENTS,
        string_bytes: abi::MAX_STRING_BYTES,
        allocation_bytes: abi::MAX_STRING_BYTES,
        json_depth: value::MAX_JSON_DEPTH,
        json_nodes: value::MAX_JSON_NODES,
        json_input_bytes: abi::MAX_INPUT_BYTES,
        reports_resources: false,
        amortized_growth: false,
        pending_continuations: crate::scheduler::MAX_PENDING_CONTINUATIONS,
        pending_timers: crate::scheduler::MAX_PENDING_TIMERS,
    };
    /// Batch v38 (`data_limits`): Wasm-derived ceilings; the memory budget
    /// bounds data in practice. Visited JSON values are bounded by the
    /// encoded size, which the memory budget bounds; JSON nesting by the
    /// host's JSON depth ceiling [`V38_MAX_JSON_DEPTH`].
    pub const V38: Self = Self {
        array_elements: V38_MAX_ARRAY_ELEMENTS,
        string_bytes: V38_MAX_STRING_BYTES,
        allocation_bytes: V38_MAX_ALLOCATION_BYTES,
        json_depth: V38_MAX_JSON_DEPTH,
        json_nodes: usize::MAX,
        // A guest string is bounded by the memory budget; its UTF-8 form by
        // one and a half times its UTF-16 bytes.
        json_input_bytes: V38_MAX_STRING_BYTES,
        reports_resources: true,
        amortized_growth: true,
        pending_continuations: V38_MAX_PENDING_CONTINUATIONS,
        pending_timers: V38_MAX_PENDING_TIMERS,
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
    use crate::language::SUPPORTED_PROFILES;

    #[test]
    fn frozen_limits_are_the_historical_constants() {
        assert_eq!(FROZEN.schema_types(false), json::LEGACY_MAX_SCHEMA_TYPES);
        assert_eq!(FROZEN.schema_types(true), json::MAX_SCHEMA_TYPES);
        assert_eq!(FROZEN.capture_types(), function::MAX_CAPTURE_TYPES);
        assert_eq!(FROZEN.wasm_functions, 270);
        assert_eq!((FROZEN.union_members, FROZEN.json_union_members), (16, 7));
        for profile in crate::language::SUPPORTED_PROFILES {
            let expected = if *profile == LanguageProfile::CheckedF64Utf16V38 {
                WASM
            } else {
                FROZEN
            };
            assert_eq!(profile.limits(), expected, "{profile:?}");
        }
    }

    // Every v38 limit stays within the implementation limits and none
    // shrinks against the frozen one; entity ids stay u32, below the i32
    // range used by Wasm constants.
    const _: () = {
        assert!(WASM.function_body_bytes <= WASM_FUNCTION_BODY_BYTES);
        assert!(WASM.wasm_functions <= WASM_FUNCTIONS);
        assert!(WASM.wasm_signatures <= WASM_TYPES);
        assert!(FROZEN.user_functions <= WASM.user_functions);
        assert!(FROZEN.wasm_functions <= WASM.wasm_functions);
        assert!(FROZEN.wasm_signatures <= WASM.wasm_signatures);
        assert!(FROZEN.wasm_bytes <= WASM.wasm_bytes);
        assert!(FROZEN.function_body_bytes <= WASM.function_body_bytes);
        assert!(FROZEN.record_shapes <= WASM.record_shapes);
        assert!(FROZEN.union_types <= WASM.union_types);
        assert!(FROZEN.union_members <= WASM.union_members);
        assert!(FROZEN.json_union_members <= WASM.json_union_members);
        assert!(FROZEN.array_types <= WASM.array_types);
        assert!(FROZEN.promise_types <= WASM.promise_types);
        assert!(FROZEN.generator_types <= WASM.generator_types);
        assert!(FROZEN.function_types <= WASM.function_types);
        assert!(FROZEN.schema_bytes <= WASM.schema_bytes);
        assert!(ENTITIES < i32::MAX as usize);
    };

    #[test]
    fn the_maximum_is_the_wasm_limits() {
        assert_eq!(MAXIMUM, WASM);
        assert!(WASM.capture_types() < i32::MAX as usize);
    }

    #[test]
    fn only_v38_lifts_the_data_ceilings() {
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.data_limits(),
                profile.program_limits(),
                "{profile:?}"
            );
            let limits = profile.data();
            if profile == LanguageProfile::CheckedF64Utf16V38 {
                assert_eq!(limits, DataLimits::V38);
            } else {
                assert_eq!(limits, DataLimits::LEGACY, "{profile:?}");
            }
        }
        assert_eq!(DataLimits::LEGACY.array_elements, 1024);
        assert_eq!(DataLimits::LEGACY.string_bytes, 1 << 20);
        assert_eq!(DataLimits::LEGACY.pending_continuations, 64);
        assert_eq!(DataLimits::LEGACY.pending_timers, 32);
        assert_eq!(DataLimits::V38.pending_continuations, 131_072);
        assert_eq!(DataLimits::V38.pending_timers, 65_536);
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
