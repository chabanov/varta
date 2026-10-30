//! A per-call arena on the private intrinsic memory.
//!
//! Every allocating export runs inside [`entry`], which resets the arena to
//! `__heap_base` and refuses a nested entry, so no allocation, pointer or
//! state survives a call. Growth uses `memory.grow` on the private memory
//! (bounded by its declared maximum); exhaustion fails the allocation, which
//! traps.
use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};

const PAGE_BYTES: usize = 65_536;
/// The private memory, as `memory.grow`/`memory.size` name it before linking.
const MEMORY: u32 = 0;

unsafe extern "C" {
    /// First byte after the library's static data and stack (wasm-ld symbol).
    static __heap_base: u8;
}

/// The next free byte; 0 outside an entry, where allocation fails.
static TOP: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Run one export with a fresh arena; a nested entry traps.
pub fn entry<T>(body: impl FnOnce() -> T) -> T {
    if ACTIVE.swap(true, Relaxed) {
        core::arch::wasm32::unreachable();
    }
    // Only the address of the linker symbol is taken, never its value.
    TOP.store(&raw const __heap_base as usize, Relaxed);
    let result = body();
    TOP.store(0, Relaxed);
    ACTIVE.store(false, Relaxed);
    result
}

/// Make `[0, end)` addressable in the private memory.
fn reserve(end: usize) -> bool {
    let available = core::arch::wasm32::memory_size(MEMORY) * PAGE_BYTES;
    if end <= available {
        return true;
    }
    let pages = (end - available).div_ceil(PAGE_BYTES);
    core::arch::wasm32::memory_grow(MEMORY, pages) != usize::MAX
}

struct Arena;

// SAFETY: allocations are disjoint bump ranges inside the private memory,
// aligned as requested and valid until the entry that made them returns.
unsafe impl GlobalAlloc for Arena {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let top = TOP.load(Relaxed);
        if top == 0 {
            return core::ptr::null_mut();
        }
        let Some(start) = top.checked_next_multiple_of(layout.align()) else {
            return core::ptr::null_mut();
        };
        let Some(end) = start.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        if !reserve(end) {
            return core::ptr::null_mut();
        }
        TOP.store(end, Relaxed);
        start as *mut u8
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let start = ptr as usize;
        // The newest allocation grows in place, so a growing buffer costs
        // its final size rather than the sum of its doublings.
        if start.checked_add(layout.size()) == Some(TOP.load(Relaxed)) {
            let Some(end) = start.checked_add(new_size) else {
                return core::ptr::null_mut();
            };
            if new_size <= layout.size() || reserve(end) {
                TOP.store(end, Relaxed);
                return ptr;
            }
            return core::ptr::null_mut();
        }
        // SAFETY: the caller's layout is valid; the new block is fresh and
        // disjoint from `ptr`, and both hold at least the copied length.
        unsafe {
            let fresh = self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
            if !fresh.is_null() {
                core::ptr::copy_nonoverlapping(ptr, fresh, layout.size().min(new_size));
            }
            fresh
        }
    }
}

#[global_allocator]
static ALLOCATOR: Arena = Arena;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
