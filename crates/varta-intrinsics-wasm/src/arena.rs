//! A per-call arena: the intrinsic region's reserved arena first, then the
//! free memory above the guest heap.
//!
//! Every allocating export runs inside [`entry`], which starts the arena at
//! the region's reserved arena (`varta_link.arena_reserved`, committed with
//! the region, so small calls need no memory growth even when the guest heap
//! fills the memory budget) and refuses a nested entry, so no allocation,
//! pointer or state survives a call. An allocation that no longer fits the
//! reserved arena continues at the guest heap's end
//! (`varta_link.arena_base`). Linked code never calls the guest allocator, so
//! nothing else writes above the heap end while an entry runs; afterwards the
//! guest allocator uses those bytes as fresh heap space (it initializes every
//! block it hands out). Growth uses `memory.grow` on memory 0 within the
//! memory budget; exhaustion fails the allocation, which traps.
use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
use varta_common::abi::WASM_PAGE_BYTES;

/// Guest memory 0, the only memory of a linked module.
const MEMORY: u32 = 0;
/// [`LIMIT`] once the arena continues above the guest heap: memory grows on
/// demand.
const UNBOUNDED: usize = usize::MAX;

#[link(wasm_import_module = "varta_link")]
unsafe extern "C" {
    /// The guest heap's end: its allocator's next fresh byte, or the heap
    /// base of a module without a managed heap.
    fn arena_base() -> usize;
    /// Start of the reserved arena in the intrinsic region.
    fn arena_reserved() -> usize;
    /// End of the reserved arena.
    fn arena_reserved_end() -> usize;
}

/// The next free byte; 0 outside an entry, where allocation fails.
static TOP: AtomicUsize = AtomicUsize::new(0);
/// The end of the current arena: the reserved arena's end, or
/// [`UNBOUNDED`] above the guest heap.
static LIMIT: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Run one export with a fresh arena; a nested entry traps.
pub fn entry<T>(body: impl FnOnce() -> T) -> T {
    if ACTIVE.swap(true, Relaxed) {
        core::arch::wasm32::unreachable();
    }
    // SAFETY: the accessors only return linker constants.
    unsafe {
        TOP.store(arena_reserved(), Relaxed);
        LIMIT.store(arena_reserved_end(), Relaxed);
    }
    let result = body();
    TOP.store(0, Relaxed);
    ACTIVE.store(false, Relaxed);
    result
}

/// Make `[0, end)` addressable in memory 0.
fn reserve(end: usize) -> bool {
    let available = core::arch::wasm32::memory_size(MEMORY) * WASM_PAGE_BYTES;
    if end <= available {
        return true;
    }
    let pages = (end - available).div_ceil(WASM_PAGE_BYTES);
    core::arch::wasm32::memory_grow(MEMORY, pages) != usize::MAX
}

/// The aligned range `[start, end)` of `size` bytes from `top`, if it fits
/// below `limit` (growing memory above the guest heap).
fn fit(top: usize, align: usize, size: usize, limit: usize) -> Option<(usize, usize)> {
    let start = top.checked_next_multiple_of(align)?;
    let end = start.checked_add(size)?;
    let fits = if limit == UNBOUNDED {
        reserve(end)
    } else {
        end <= limit
    };
    fits.then_some((start, end))
}

struct Arena;

// SAFETY: allocations are disjoint bump ranges of the reserved arena or
// above the guest heap's end, aligned as requested and valid until the
// entry that made them returns.
unsafe impl GlobalAlloc for Arena {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let top = TOP.load(Relaxed);
        if top == 0 {
            return core::ptr::null_mut();
        }
        let limit = LIMIT.load(Relaxed);
        let placed = fit(top, layout.align(), layout.size(), limit).or_else(|| {
            if limit == UNBOUNDED {
                return None;
            }
            // The reserved arena is full: continue above the guest heap.
            LIMIT.store(UNBOUNDED, Relaxed);
            // SAFETY: the accessor only reads the guest heap's end.
            fit(
                unsafe { arena_base() },
                layout.align(),
                layout.size(),
                UNBOUNDED,
            )
        });
        let Some((start, end)) = placed else {
            return core::ptr::null_mut();
        };
        TOP.store(end, Relaxed);
        start as *mut u8
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let start = ptr as usize;
        // The newest allocation grows in place, so a growing buffer costs
        // its final size rather than the sum of its doublings.
        if start.checked_add(layout.size()) == Some(TOP.load(Relaxed))
            && (new_size <= layout.size() || fit(start, 1, new_size, LIMIT.load(Relaxed)).is_some())
        {
            TOP.store(start + new_size, Relaxed);
            return ptr;
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
