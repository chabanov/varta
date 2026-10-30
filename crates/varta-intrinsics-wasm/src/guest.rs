//! Guest memory 0 and the result globals, through linker-provided accessors.
//!
//! The library is linked into memory 0 itself (its stack and static data in
//! the module's intrinsic region, its arena above the guest heap), but it
//! never dereferences a guest address: guest values are copied through
//! `varta_link` functions the compiler's linker synthesizes as
//! `memory.copy` within memory 0. Copies are bounds-checked here before the
//! accessor runs, and `memory.copy` traps on any range a check missed.
use alloc::borrow::Cow;
use varta_common::{
    abi::{MAX_RESULT_BYTES, RESULT_BASE, StaticRegion, WASM_PAGE_BYTES},
    value::{ModuleBounds, ValueAbiError},
};
use varta_intrinsics::{IntrinsicError, Memory};

/// Guest memory 0, the only memory of a linked module.
const MEMORY: u32 = 0;

#[link(wasm_import_module = "varta_link")]
unsafe extern "C" {
    /// Copy guest `[src, src + len)` to the library buffer at `dst`.
    fn memory_read(dst: *mut u8, src: usize, len: usize);
    /// Copy the library buffer `[src, src + len)` to guest `dst`.
    fn memory_write(dst: usize, src: *const u8, len: usize);
    /// Set the guest's `env_result_len` global, when it is exported.
    fn result_len(len: i32);
    /// The guest allocator: a fresh managed-heap block of `len` payload
    /// bytes, stamped allocated; it collects, and reports exhausted memory
    /// and traps, itself.
    fn allocate(len: usize) -> usize;
    /// The guest retention helper: push `handle` on the root stack (a
    /// static or zero handle is not pushed).
    fn retain(handle: i64) -> i64;
    /// The root-stack top.
    fn roots() -> usize;
    /// Reset the root-stack top, dropping the roots above it.
    fn set_roots(top: usize);
    /// The v38 exception channel: its version, pending flag and value
    /// (-1 when the module has none), and throwing a cell.
    fn exception_version() -> i32;
    fn exception_pending() -> i32;
    fn exception_value() -> i64;
    fn raise(cell: i64);
    /// The static region's extension (low word) and intrinsic (high word)
    /// pages.
    fn static_pages() -> i64;
}

/// End the execution: a fatal failure of linked code.
pub fn trap() -> ! {
    core::arch::wasm32::unreachable()
}
/// Record a failure code and its request for the runtime's typed report,
/// then trap (`limits::Resource` codes and linked JSON input limits).
pub fn fail(code: i32, requested: u32) -> ! {
    crate::arena::report(code, requested)
}
/// Set the guest's `env_result_len` global.
pub fn set_result_len(len: i32) {
    // SAFETY: the accessor sets one exported i32 global.
    unsafe { result_len(len) }
}
/// The guest allocator (see [`allocate`]).
pub fn allocate_block(len: usize) -> usize {
    // SAFETY: the accessor calls the guest's own allocator, which returns a
    // fresh block of at least `len` payload bytes or traps.
    unsafe { allocate(len) }
}
/// Push `handle` on the guest root stack.
pub fn retain_root(handle: u64) {
    // SAFETY: the accessor calls the guest's retention helper.
    unsafe { retain(handle as i64) };
}
/// The root-stack top.
pub fn root_top() -> usize {
    // SAFETY: the accessor reads one guest global.
    unsafe { roots() }
}
/// Drop the roots above `top`.
pub fn reset_roots(top: usize) {
    // SAFETY: the accessor sets one guest global to a top read before.
    unsafe { set_roots(top) }
}
/// The exception channel: (version, pending, value).
pub fn exception_state() -> (i32, i32, i64) {
    // SAFETY: the accessors read guest globals.
    unsafe { (exception_version(), exception_pending(), exception_value()) }
}
/// Throw `cell` through the exception channel.
pub fn throw(cell: u64) {
    // SAFETY: the accessor sets the channel's two globals.
    unsafe { raise(cell as i64) }
}
/// The module's static region: its extension and intrinsic pages, linker
/// constants of the validated module.
pub fn static_region() -> Option<StaticRegion> {
    // SAFETY: the accessor returns a linker constant.
    let pages = unsafe { static_pages() } as u64;
    StaticRegion::with_extension_pages(pages as u32)?
        .with_intrinsic_pages((pages >> u32::BITS) as u32)
}
/// The bounds of this v38 module's value readers: its static region and the
/// v38 limits, as the host computes them from the validated module.
pub fn bounds() -> ModuleBounds {
    match static_region() {
        Some(region) => ModuleBounds::v38(region),
        None => trap(),
    }
}

/// Guest blocks this library allocated, written in place: the linked JSON
/// decoder's [`varta_common::json::build::Heap`]. Every address it writes
/// lies inside a block the guest allocator just returned for the build (the
/// builder writes only those), above the heap base and thus outside the
/// static data, the root stack and the intrinsic region.
pub struct GuestHeap;
impl varta_common::json::build::Heap for GuestHeap {
    type Error = core::convert::Infallible;
    fn allocate(&mut self, len: usize) -> Result<usize, Self::Error> {
        Ok(allocate_block(len))
    }
    fn write(&mut self, at: usize, bytes: &[u8]) -> Result<(), Self::Error> {
        // SAFETY: `[at, at + len)` lies in a freshly allocated block (see
        // the type), disjoint from `bytes` (the input text or library
        // scratch).
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), at as *mut u8, bytes.len()) };
        Ok(())
    }
    fn word(&mut self, at: usize, value: u32) -> Result<(), Self::Error> {
        // SAFETY: as for `write`.
        unsafe { (at as *mut u32).write_unaligned(value) };
        Ok(())
    }
    fn slot(&mut self, at: usize, value: u64) -> Result<(), Self::Error> {
        // SAFETY: as for `write`.
        unsafe { (at as *mut u64).write_unaligned(value) };
        Ok(())
    }
    fn zero(&mut self, at: usize, len: usize) -> Result<(), Self::Error> {
        // SAFETY: as for `write`.
        unsafe { core::ptr::write_bytes(at as *mut u8, 0, len) };
        Ok(())
    }
    fn retain(&mut self, handle: u64) -> Result<(), Self::Error> {
        retain_root(handle);
        Ok(())
    }
}

/// Guest memory 0, read-only.
pub struct Guest;
impl Memory for Guest {
    fn size(&self) -> usize {
        core::arch::wasm32::memory_size(MEMORY) * WASM_PAGE_BYTES
    }
    fn copy_to(&self, at: usize, out: &mut [u8]) -> bool {
        if at
            .checked_add(out.len())
            .is_none_or(|end| end > self.size())
        {
            return false;
        }
        // SAFETY: `out` is a live library buffer of exactly `out.len()`
        // bytes and the guest range was checked against memory 0's size.
        unsafe { memory_read(out.as_mut_ptr(), at, out.len()) };
        true
    }
    /// Words are loaded in place, as [`Memory::borrow`] lends ranges: the
    /// value readers read many header and descriptor words per call.
    #[inline]
    fn u32_at(&self, at: usize) -> Option<u32> {
        if !in_place(at, size_of::<u32>()) {
            let mut word = [0; 4];
            return self
                .copy_to(at, &mut word)
                .then(|| u32::from_le_bytes(word));
        }
        // SAFETY: as in `borrow`: a nonnull, in-bounds range of memory 0
        // outside the result buffer, unchanged during the call; wasm32
        // loads are little-endian and may be unaligned.
        Some(unsafe { (at as *const u32).read_unaligned() })
    }
    #[inline]
    fn u64_at(&self, at: usize) -> Option<u64> {
        if !in_place(at, size_of::<u64>()) {
            let mut word = [0; 8];
            return self
                .copy_to(at, &mut word)
                .then(|| u64::from_le_bytes(word));
        }
        // SAFETY: as for `u32_at`.
        Some(unsafe { (at as *const u64).read_unaligned() })
    }
    /// A guest argument range in place: no per-call copy of a string
    /// argument. A range at address 0 (a Rust slice may not start at null)
    /// or overlapping the result buffer (which linked code writes during
    /// the call) is copied instead.
    fn borrow(&self, at: usize, len: usize) -> Option<Cow<'_, [u8]>> {
        let end = at.checked_add(len).filter(|end| *end <= self.size())?;
        if len == 0 {
            return Some(Cow::Borrowed(&[]));
        }
        let result_buffer = RESULT_BASE..RESULT_BASE + MAX_RESULT_BYTES;
        if at == 0 || (at < result_buffer.end && end > result_buffer.start) {
            return self.bytes(at, len).map(Cow::Owned);
        }
        // SAFETY: `[at, end)` is a nonnull, in-bounds range of memory 0,
        // which never shrinks or moves. Linked code writes guest memory only
        // in the result buffer (excluded above), its own region, the arena
        // above the guest heap's end and (linked JSON) blocks the guest
        // allocator just returned, the root stack and the exception root;
        // none of them holds a live argument, and linked JSON roots its
        // input, so the bytes stay unchanged while the borrow lives (within
        // one call).
        Some(Cow::Borrowed(unsafe {
            core::slice::from_raw_parts(at as *const u8, len)
        }))
    }
}

/// Whether guest `[at, at + len)` can be read in place: nonnull, inside
/// memory 0 and outside the result buffer, which linked code writes.
#[inline]
fn in_place(at: usize, len: usize) -> bool {
    let size = core::arch::wasm32::memory_size(MEMORY) * WASM_PAGE_BYTES;
    at.checked_add(len).is_some_and(|end| {
        at != 0 && end <= size && (at >= RESULT_BASE + MAX_RESULT_BYTES || end <= RESULT_BASE)
    })
}

/// End the execution: a fatal intrinsic failure is a trap in every host.
pub fn fatal(_error: IntrinsicError) -> ! {
    core::arch::wasm32::unreachable()
}

/// End the execution on a fatal BigInt failure (an error without a
/// recoverable class), as the host does.
pub fn fatal_bigint(_error: varta_bigint::Error) -> ! {
    core::arch::wasm32::unreachable()
}

/// End the execution on a noncanonical value cell, as the host does.
pub fn fatal_value(_error: ValueAbiError) -> ! {
    core::arch::wasm32::unreachable()
}

/// Unwrap a result or end the execution.
pub fn checked<T>(result: Result<T, IntrinsicError>) -> T {
    result.unwrap_or_else(|error| fatal(error))
}

/// Publish a string result at `RESULT_BASE` and set `env_result_len`, exactly
/// as a host result transfer. Memory 0 of a linked module already holds the
/// whole result buffer: its minimum covers the heap base, which follows it.
pub fn result(bytes: &[u8]) -> i32 {
    if bytes.len() > MAX_RESULT_BYTES {
        fatal(IntrinsicError::ResultTooLarge);
    }
    // SAFETY: `bytes` is a live library buffer and memory 0 holds
    // `[RESULT_BASE, RESULT_BASE + len)`, below the intrinsic region; the
    // length fits i32 (at most 1 MiB).
    unsafe {
        memory_write(RESULT_BASE, bytes.as_ptr(), bytes.len());
        result_len(bytes.len() as i32);
    }
    RESULT_BASE as i32
}
