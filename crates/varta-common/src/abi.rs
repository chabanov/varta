//! ABI v1: strings use borrowed input ptr/len and host-owned result ptr/len/status.
pub const VERSION: u32 = 1;
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
pub const MAX_INTERNAL_FUNCTIONS: usize = 6;
pub const MAX_WASM_SIGNATURES: usize = 256;
pub const MAX_WASM_PARAMETERS: usize = 32;
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
