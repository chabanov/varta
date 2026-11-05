//! The per-call arena: a bump allocator over this module's own memory,
//! reset when a call starts (`kv_input`). Freed blocks are reclaimed at the
//! next reset; the newest block returns to the bump pointer at once. Growth
//! uses `memory.grow` on the module's memory 0, which is never the guest's.
use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};
use varta_common::abi::WASM_PAGE_BYTES;

/// The module's own memory.
const MEMORY: u32 = 0;

unsafe extern "C" {
    /// The linker's first byte past static data and the stack.
    static __heap_base: u8;
}

/// The next free byte; 0 before the first reset.
static TOP: AtomicUsize = AtomicUsize::new(0);

/// Only the symbol's address is taken, never its contents.
fn base() -> usize {
    core::ptr::addr_of!(__heap_base) as usize
}

/// Start a call: everything allocated before is free again.
pub fn reset() {
    TOP.store(base(), Relaxed);
}

/// Grow memory until `end` bytes are addressable; false when it cannot.
fn ensure(end: usize) -> bool {
    let size = core::arch::wasm32::memory_size(MEMORY) * WASM_PAGE_BYTES;
    if end <= size {
        return true;
    }
    let pages = (end - size).div_ceil(WASM_PAGE_BYTES);
    core::arch::wasm32::memory_grow(MEMORY, pages) != usize::MAX
}

struct Arena;

// SAFETY: wasm32 code here is single-threaded; every block is a fresh,
// aligned range of this module's memory below the bump pointer.
unsafe impl GlobalAlloc for Arena {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut top = TOP.load(Relaxed);
        if top == 0 {
            top = base();
        }
        let Some(start) = top.checked_next_multiple_of(layout.align()) else {
            return core::ptr::null_mut();
        };
        let Some(end) = start.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        if !ensure(end) {
            return core::ptr::null_mut();
        }
        TOP.store(end, Relaxed);
        start as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // The newest block returns to the bump pointer.
        let start = ptr as usize;
        if start + layout.size() == TOP.load(Relaxed) {
            TOP.store(start, Relaxed);
        }
    }
}

#[global_allocator]
static ALLOCATOR: Arena = Arena;
