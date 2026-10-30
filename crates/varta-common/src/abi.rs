//! ABI v1: strings use borrowed input ptr/len and host-owned result ptr/len/status.
pub const VERSION: u32 = 1;
/// Opt-in binary64 scalar profile; legacy pointer/string operations retain their signatures.
pub const NUMBER_VERSION: u32 = 2;
pub const UNDEFINED_VERSION: u32 = 3;
pub const UTF16_VERSION: u32 = 4;
pub const UNION_VERSION: u32 = 5;
pub const MISSING_VERSION: u32 = 6;
pub const REFERENCE_VERSION: u32 = 7;
pub const CHECKED_JSON_VERSION: u32 = 8;
pub const VECTOR_VERSION: u32 = 9;
pub const EXCEPTION_VERSION: u32 = 10;
pub const ERROR_VERSION: u32 = 11;
pub const ERROR_FAMILY_VERSION: u32 = 12;
pub const RECOVERABLE_JSON_VERSION: u32 = 13;
pub const FUNCTION_VERSION: u32 = 14;
pub const CLOSURE_VERSION: u32 = 15;
pub const SPARSE_VERSION: u32 = 16;
pub const ARRAY_MUTATION_VERSION: u32 = 17;
pub const NUMBER_POWER_VERSION: u32 = 18;
pub const NATIVE_MATH_VERSION: u32 = 19;
pub const RANDOM_VERSION: u32 = 20;
pub const NUMBER_TEXT_VERSION: u32 = 21;
pub const PRIMITIVE_STRING_VERSION: u32 = 22;
pub const PRIMITIVE_NUMBER_VERSION: u32 = 23;
pub const NUMBER_RADIX_VERSION: u32 = 24;
pub const NUMBER_FORMAT_VERSION: u32 = 25;
pub const NUMBER_PARSE_VERSION: u32 = 26;
pub const BIGINT_VERSION: u32 = 27;
pub const BIGINT_VALUES_VERSION: u32 = 28;
/// K4.4 resumable frames (v33): declares the `varta_resume` export and frame
/// catalog version 2, so runtimes without frame admission reject the module.
pub const RESUMABLE_FRAMES_VERSION: u32 = 29;
/// Pilot batch v34: the runtime microtask scheduler drives `varta_resume` for
/// promise jobs and `await`, so runtimes without the scheduler reject the module.
pub const SCHEDULED_FRAMES_VERSION: u32 = 30;
/// Pilot batch v35: the value16 readers (`HostOperation::value15_counterpart`)
/// bound to value layout 16, which adds promise values.
pub const PROMISE_VALUES_VERSION: u32 = 31;
/// Pilot batch v36 (timers and function-valued fields): the minimum host ABI
/// of every import the batch adds, such as the timer operations behind
/// `setTimeout`/`clearTimeout`. No import requires it yet; it binds v36 alone,
/// so a v36 module relabelled as v35 fails at its host ABI section.
pub const TIMERS_VERSION: u32 = 32;
/// Pilot batch v37 (language core, class generics, collections, RegExp and
/// the async network request capability): the minimum host ABI of every
/// import the batch adds, such as the `net.request` operations behind
/// `fetch`. No import requires it yet; it binds v37 alone, so a v37 module
/// relabelled as v36 fails at its host ABI section.
pub const NET_REQUEST_VERSION: u32 = 33;
/// Batch v38 (program and data limits, turn budgets, linked intrinsics, the
/// view protocol and TSX, structural subtyping and code generation v2): the
/// minimum host ABI of every import the batch adds. No import requires it yet;
/// it binds v38 alone, so a v38 module relabelled as v37 fails at its host ABI
/// section. It also defines the multi-page static region ([`StaticRegion`]):
/// a [`STATIC_SECTION`] extension after the root stack, with the managed heap
/// after it, admitted for host ABI 34 profiles only.
pub const PROGRAM_LIMITS_VERSION: u32 = 34;
#[must_use]
pub fn supported(version: u32) -> bool {
    matches!(
        version,
        VERSION
            | NUMBER_VERSION
            | UNDEFINED_VERSION
            | UTF16_VERSION
            | UNION_VERSION
            | MISSING_VERSION
            | REFERENCE_VERSION
            | CHECKED_JSON_VERSION
            | VECTOR_VERSION
            | EXCEPTION_VERSION
            | ERROR_VERSION
            | ERROR_FAMILY_VERSION
            | RECOVERABLE_JSON_VERSION
            | FUNCTION_VERSION
            | CLOSURE_VERSION
            | SPARSE_VERSION
            | ARRAY_MUTATION_VERSION
            | NUMBER_POWER_VERSION
            | NATIVE_MATH_VERSION
            | RANDOM_VERSION
            | NUMBER_TEXT_VERSION
            | PRIMITIVE_STRING_VERSION
            | PRIMITIVE_NUMBER_VERSION
            | NUMBER_RADIX_VERSION
            | NUMBER_FORMAT_VERSION
            | NUMBER_PARSE_VERSION
            | BIGINT_VERSION
            | BIGINT_VALUES_VERSION
            | RESUMABLE_FRAMES_VERSION
            | SCHEDULED_FRAMES_VERSION
            | PROMISE_VALUES_VERSION
            | TIMERS_VERSION
            | NET_REQUEST_VERSION
            | PROGRAM_LIMITS_VERSION
    )
}
mod static_region;
pub use static_region::{
    MAX_STATIC_EXTENSION_PAGES, STATIC_SECTION, StaticRegion, StaticRegionError,
    WASM_MEMORY32_PAGES,
};
pub const SECTION: &str = "varta.abi";
pub const RESULT_LEN: &str = "env_result_len";
pub const RESULT_STATUS: &str = "result_status";
pub const RESULT_BASE: usize = 65_536;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_RESULT_BYTES: usize = 1024 * 1024;
pub const MAX_STRING_BYTES: usize = MAX_RESULT_BYTES;
pub const WASM_PAGE_BYTES: usize = 65_536;
pub const STRING_ROOT_BASE: usize = RESULT_BASE + MAX_RESULT_BYTES;
pub const STRING_ROOT_BYTES: usize = WASM_PAGE_BYTES;
pub const STRING_ROOT_END: usize = STRING_ROOT_BASE + STRING_ROOT_BYTES;
/// Nonmoving string heap follows the reusable host buffer and bounded root stack.
pub const OWNED_STRING_BASE: usize = STRING_ROOT_END;
pub const STRING_ROOT_SLOT_BYTES: usize = 8;
pub const STRING_HEADER_BYTES: usize = 16;
pub const MAX_USER_FUNCTIONS: usize = 256;
pub const MAX_HTTP_HANDLERS: usize = 1;
pub const LEGACY_INTERNAL_FUNCTIONS: usize = 10;
pub const SPARSE_INTERNAL_FUNCTIONS: usize = 12;
pub const MAX_INTERNAL_FUNCTIONS: usize = 13;
pub const MAX_WASM_SIGNATURES: usize = 256;
pub const MAX_WASM_PARAMETERS: usize = 32;
/// Bounded flattened argument count for checked spread calls.
pub const MAX_SPREAD_ARGUMENTS: usize = crate::value::MAX_ARRAY_ELEMENTS;
pub const MAX_MEMORY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_HOST_CALLS: u32 = 1024;
pub const FUEL: u64 = 1_000_000;
pub const DEADLINE_SECONDS: u64 = 5;
pub const MAX_WASM_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum HostStatus {
    Ok = 0,
    Denied = 1,
    InvalidInput = 2,
    Io = 3,
    TooLarge = 4,
}

pub const MAX_WASM_FUNCTION_BODY_BYTES: usize = 65_536;
