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
    )
}
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
