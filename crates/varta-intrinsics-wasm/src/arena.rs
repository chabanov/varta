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
//! block it hands out). The newest block returns to the bump pointer and
//! other freed blocks to size-class free lists, reused within the call.
//! Growth uses `memory.grow` on memory 0 within the memory budget; an
//! allocation the budget cannot hold reports the typed memory resource
//! (`exhausted`) and traps.
use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
use varta_common::{abi::WASM_PAGE_BYTES, limits::Resource};

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
    /// Store a `limits::Resource` failure and its request before a trap.
    fn resource_failure(code: i32, requested: i32);
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
    NONEMPTY.store(0, Relaxed);
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
/// below `limit`. Above the guest heap (`UNBOUNDED`) memory grows instead,
/// and a range the memory budget cannot hold ends the execution with a
/// typed resource failure.
fn fit(top: usize, align: usize, size: usize, limit: usize) -> Option<(usize, usize)> {
    let range = top
        .checked_next_multiple_of(align)
        .and_then(|start| Some((start, start.checked_add(size)?)));
    match range {
        Some((start, end)) if limit == UNBOUNDED && reserve(end) => Some((start, end)),
        _ if limit == UNBOUNDED => exhausted(size),
        Some((start, end)) if end <= limit => Some((start, end)),
        _ => None,
    }
}

/// Report the memory budget as the exhausted resource (`limits::Resource`,
/// through the guest's `resource_failure`/`resource_request` globals) and
/// trap, as the guest allocator does; the runtime reports a typed error.
fn exhausted(requested: usize) -> ! {
    let requested = u32::try_from(requested).unwrap_or(u32::MAX);
    // SAFETY: the accessor only sets the two exported i32 globals.
    unsafe { resource_failure(Resource::Memory as i32, requested as i32) };
    core::arch::wasm32::unreachable()
}

/// Freed blocks by size class `k` (sizes in `[2^k, 2^(k+1))`), as intrusive
/// lists: a block's first word links the next block of its class, its
/// second word holds its size. Cleared on every entry.
static FREE: [AtomicUsize; usize::BITS as usize] =
    [const { AtomicUsize::new(0) }; usize::BITS as usize];
/// Bit `k` set while class `k` lists a block; cleared on every entry, which
/// makes every head stale.
static NONEMPTY: AtomicUsize = AtomicUsize::new(0);
/// The smallest block a free list keeps: its link and size words.
const MIN_FREE_BYTES: usize = 2 * size_of::<usize>();
/// Free-list blocks are carved at this alignment, so any request aligned at
/// most this much may reuse one.
const FREE_ALIGNMENT: usize = size_of::<u64>();

/// The size class of a block of `size` bytes (floor log2).
fn class_of(size: usize) -> usize {
    (usize::BITS - 1 - size.max(1).leading_zeros()) as usize
}
/// The link and size words of a listed block.
///
/// SAFETY (callers): `block` is a freed, word-aligned arena block of at
/// least [`MIN_FREE_BYTES`].
unsafe fn words(block: usize) -> (usize, usize) {
    let words = block as *const usize;
    // SAFETY: per the caller.
    unsafe { (words.read(), words.add(1).read()) }
}
/// Take a freed block of at least `size` bytes: first fit within its own
/// class, else the first block of the smallest larger nonempty class.
fn reuse(size: usize) -> Option<usize> {
    let size = size.max(MIN_FREE_BYTES);
    let class = class_of(size);
    let nonempty = NONEMPTY.load(Relaxed);
    if nonempty == 0 {
        return None;
    }
    if nonempty & (1 << class) != 0 {
        let mut previous: Option<usize> = None;
        let mut block = FREE[class].load(Relaxed);
        while block != 0 {
            // SAFETY: a listed block is a freed, word-aligned arena block.
            let (next, length) = unsafe { words(block) };
            if length >= size {
                match previous {
                    // SAFETY: `previous` is a listed block; its link word is ours.
                    Some(previous) => unsafe { (previous as *mut usize).write(next) },
                    None => pop(class, next),
                }
                return Some(block);
            }
            previous = Some(block);
            block = next;
        }
    }
    // Every class above `class` (none above the last).
    let above = 1_usize
        .checked_shl(class as u32 + 1)
        .map_or(0, |bit| !(bit - 1));
    let larger = nonempty & above;
    if larger == 0 {
        return None;
    }
    let class = larger.trailing_zeros() as usize;
    let block = FREE[class].load(Relaxed);
    // SAFETY: a listed block is a freed, word-aligned arena block.
    pop(class, unsafe { words(block) }.0);
    Some(block)
}
/// Make `next` the head of `class`, marking the class empty when it is 0.
fn pop(class: usize, next: usize) {
    FREE[class].store(next, Relaxed);
    if next == 0 {
        NONEMPTY.fetch_and(!(1 << class), Relaxed);
    }
}

struct Arena;

// SAFETY: allocations are disjoint ranges of the reserved arena or above the
// guest heap's end, aligned as requested and valid until they are freed or
// the entry that made them returns; a freed block is reused only for a
// request it holds.
unsafe impl GlobalAlloc for Arena {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let top = TOP.load(Relaxed);
        if top == 0 {
            return core::ptr::null_mut();
        }
        // Reuse a freed block first, so the temporaries of big-integer
        // arithmetic (resized and freed out of order) do not add up over the
        // call.
        if layout.align() <= FREE_ALIGNMENT
            && let Some(block) = reuse(layout.size())
        {
            return block as *mut u8;
        }
        let align = layout.align().max(FREE_ALIGNMENT);
        let limit = LIMIT.load(Relaxed);
        let placed = fit(top, align, layout.size(), limit).or_else(|| {
            if limit == UNBOUNDED {
                return None;
            }
            // The reserved arena is full: continue above the guest heap.
            LIMIT.store(UNBOUNDED, Relaxed);
            // SAFETY: the accessor only reads the guest heap's end.
            fit(unsafe { arena_base() }, align, layout.size(), UNBOUNDED)
        });
        let Some((start, end)) = placed else {
            return core::ptr::null_mut();
        };
        TOP.store(end, Relaxed);
        start as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let start = ptr as usize;
        // The newest block returns to the bump pointer; any other joins the
        // free list of its requested size (a reused block may be larger).
        if start.checked_add(layout.size()) == Some(TOP.load(Relaxed)) {
            TOP.store(start, Relaxed);
            return;
        }
        if layout.size() < MIN_FREE_BYTES || !start.is_multiple_of(FREE_ALIGNMENT) {
            return;
        }
        let class = class_of(layout.size());
        // A class not marked nonempty holds a stale head from an earlier call.
        let next = if NONEMPTY.load(Relaxed) & (1 << class) != 0 {
            FREE[class].load(Relaxed)
        } else {
            0
        };
        let words = start as *mut usize;
        // SAFETY: the block is freed, word-aligned and holds two words.
        unsafe {
            words.write(next);
            words.add(1).write(layout.size());
        }
        FREE[class].store(start, Relaxed);
        NONEMPTY.fetch_or(1 << class, Relaxed);
    }
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
        // disjoint from `ptr`, and both hold at least the copied length;
        // the old block is freed after the copy.
        unsafe {
            let fresh = self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
            if !fresh.is_null() {
                core::ptr::copy_nonoverlapping(ptr, fresh, layout.size().min(new_size));
                self.dealloc(ptr, layout);
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
