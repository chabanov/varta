//! Structural program limits by language profile: the one registry the
//! compiler, the runtime's module and catalog readers, the artifact and
//! launcher readers and the generated documentation read.
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
use crate::{abi, function, json, language::LanguageProfile, value};

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
}

/// The limits of every profile before v38, byte for byte the historical
/// constants.
pub const FROZEN: ProgramLimits = ProgramLimits {
    user_functions: abi::MAX_USER_FUNCTIONS,
    wasm_functions: abi::MAX_USER_FUNCTIONS + abi::MAX_INTERNAL_FUNCTIONS + abi::MAX_HTTP_HANDLERS,
    wasm_signatures: abi::MAX_WASM_SIGNATURES,
    wasm_bytes: abi::MAX_WASM_BYTES,
    function_body_bytes: abi::MAX_WASM_FUNCTION_BODY_BYTES,
    record_shapes: value::MAX_RECORD_SHAPES,
    union_types: value::MAX_UNION_TYPES,
    array_types: value::MAX_ARRAY_TYPES,
    promise_types: MAX_PROMISE_TYPES,
    generator_types: function::frame::MAX_GENERATOR_TYPES,
    function_types: function::MAX_TYPES,
    schema_bytes: json::MAX_SCHEMA_BYTES,
    static_extension_pages: 0,
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
    array_types: ENTITIES,
    promise_types: ENTITIES,
    generator_types: ENTITIES,
    function_types: ENTITIES,
    // A schema lives in static memory, bounded by the memory budget.
    schema_bytes: abi::MAX_MEMORY_BYTES,
    static_extension_pages: abi::MAX_STATIC_EXTENSION_PAGES,
};

/// The frozen `Promise<T>` type limit (v34 to v37), formerly compiler-local.
pub const MAX_PROMISE_TYPES: usize = 64;

/// The largest limits any profile admits: readers that must bound input
/// before they know its profile (the Wasm size before language inspection,
/// artifact payloads) use these and then the profile's own limits.
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

impl LanguageProfile {
    /// This profile's structural limits: [`WASM`] for a `program_limits()`
    /// profile, [`FROZEN`] for every earlier one.
    #[must_use]
    pub fn limits(self) -> ProgramLimits {
        if self.program_limits() { WASM } else { FROZEN }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_limits_are_the_historical_constants() {
        assert_eq!(FROZEN.schema_types(false), json::LEGACY_MAX_SCHEMA_TYPES);
        assert_eq!(FROZEN.schema_types(true), json::MAX_SCHEMA_TYPES);
        assert_eq!(FROZEN.capture_types(), function::MAX_CAPTURE_TYPES);
        assert_eq!(FROZEN.wasm_functions, 270);
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
}
