//! Guest memory 0 and the result globals, through linker-provided accessors.
//!
//! The compiler's linker defines every `varta_link` import as a small
//! function over the guest module's own memory 0 and exported globals. Copies
//! are bounds-checked here before the accessor runs, and the accessor's
//! `memory.copy` traps on any range a check missed.
use varta_intrinsics::{IntrinsicError, Memory, layout};

#[link(wasm_import_module = "varta_link")]
unsafe extern "C" {
    /// Byte size of guest memory 0.
    fn memory_size() -> usize;
    /// `memory.grow` of guest memory 0 by `pages`; -1 when it cannot grow.
    fn memory_grow(pages: usize) -> isize;
    /// Copy guest `[src, src + len)` to private `dst`.
    fn memory_read(dst: *mut u8, src: usize, len: usize);
    /// Copy private `[src, src + len)` to guest `dst`.
    fn memory_write(dst: usize, src: *const u8, len: usize);
    /// Set the guest's `env_result_len` global, when it is exported.
    fn result_len(len: i32);
}

/// Guest memory 0, read-only.
pub struct Guest;
impl Memory for Guest {
    fn size(&self) -> usize {
        // SAFETY: the accessor only reads `memory.size` of guest memory 0.
        unsafe { memory_size() }
    }
    fn copy_to(&self, at: usize, out: &mut [u8]) -> bool {
        if at
            .checked_add(out.len())
            .is_none_or(|end| end > self.size())
        {
            return false;
        }
        // SAFETY: `out` is a live private buffer of exactly `out.len()`
        // bytes and the guest range was checked against memory 0's size.
        unsafe { memory_read(out.as_mut_ptr(), at, out.len()) };
        true
    }
}

/// End the execution: a fatal intrinsic failure is a trap in every host.
pub fn fatal(_error: IntrinsicError) -> ! {
    core::arch::wasm32::unreachable()
}

/// Unwrap a result or end the execution.
pub fn checked<T>(result: Result<T, IntrinsicError>) -> T {
    result.unwrap_or_else(|error| fatal(error))
}

/// Publish a string result at `RESULT_BASE`, growing memory 0 in whole pages
/// when needed, and set `env_result_len`, exactly as a host result transfer.
pub fn result(bytes: &[u8]) -> i32 {
    if bytes.len() > layout::MAX_RESULT_BYTES {
        fatal(IntrinsicError::ResultTooLarge);
    }
    let desired = layout::RESULT_BASE + bytes.len();
    let size = Guest.size();
    if desired > size {
        let pages = (desired - size).div_ceil(layout::WASM_PAGE_BYTES);
        // SAFETY: the accessor only grows guest memory 0 within its maximum.
        if unsafe { memory_grow(pages) } < 0 {
            core::arch::wasm32::unreachable();
        }
    }
    // SAFETY: `bytes` is a live private buffer and memory 0 now holds
    // `[RESULT_BASE, RESULT_BASE + len)`; the length fits i32 (at most 1 MiB).
    unsafe {
        memory_write(layout::RESULT_BASE, bytes.as_ptr(), bytes.len());
        result_len(bytes.len() as i32);
    }
    layout::RESULT_BASE as i32
}
