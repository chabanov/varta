//! A per-call arena: the intrinsic region's reserved arena first, then the
//! free memory above the guest heap.
//!
//! Every allocating export runs inside [`entry`], which starts the arena at
//! the region's reserved arena (`varta_link.arena_reserved`, committed with
//! the region, so small calls need no memory growth even when the guest heap
//! fills the memory budget) and refuses a nested entry, so no allocation,
//! pointer or state survives a call. An allocation that no longer fits the
//! reserved arena continues at the guest heap's end
//! (`varta_link.arena_base`). Such an entry never calls the guest allocator,
//! so nothing else writes above the heap end while it runs (an entry that
//! does, linked JSON, uses [`scratch`] and its rooted guest blocks instead);
//! afterwards the
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

/// Whether the running [`entry`] continued its arena above the guest heap's
/// end, where the guest allocator grows: its allocations may then lie where
/// a guest block is about to be placed (owned results stage through the
/// result buffer instead, [`crate::text::owned`]).
pub fn spilled() -> bool {
    LIMIT.load(Relaxed) == UNBOUNDED && !GUEST.load(Relaxed)
}

/// While set, the entry allocates from guest blocks ([`scratch`]).
static GUEST: AtomicBool = AtomicBool::new(false);
/// The size of the newest guest scratch block.
static CHUNK: AtomicUsize = AtomicUsize::new(0);
/// The smallest guest scratch block.
const MIN_CHUNK_BYTES: usize = 1024;
/// The largest guest scratch block shared by several allocations.
const SHARED_CHUNK_BYTES: usize = 64 * 1024;
/// Guest scratch blocks whose live allocations are counted, so a block is
/// unrooted as soon as nothing in it is live; later blocks of a call stay
/// rooted until it returns.
const TRACKED_CHUNKS: usize = 64;

/// One tracked guest scratch block: its payload range, the root slot that
/// keeps it and its live allocations.
#[derive(Clone, Copy)]
struct Chunk {
    start: usize,
    end: usize,
    root: usize,
    live: usize,
}
struct Chunks {
    list: [Chunk; TRACKED_CHUNKS],
    count: usize,
}
/// The tracked blocks of the running [`scratch`] entry.
struct Tracked(core::cell::UnsafeCell<Chunks>);
// SAFETY: wasm32 guest code is single-threaded, and only the allocator
// (which a nested entry cannot reach) touches the table.
unsafe impl Sync for Tracked {}
static TRACKED: Tracked = Tracked(core::cell::UnsafeCell::new(Chunks {
    list: [Chunk {
        start: 0,
        end: 0,
        root: 0,
        live: 0,
    }; TRACKED_CHUNKS],
    count: 0,
}));
/// The tracked block table.
///
/// SAFETY (callers): no other reference to the table is live.
unsafe fn chunks() -> &'static mut Chunks {
    // SAFETY: per the caller.
    unsafe { &mut *TRACKED.0.get() }
}

/// Run one export whose scratch lives in ordinary guest heap blocks: blocks
/// from the guest allocator, rooted on the guest root stack (`hint` bytes in
/// the first, then doubling), unrooted once nothing in them is live and in
/// any case when the body returns, so the guest's collector, memory budget
/// and root rules govern them like any other block. Such an export calls
/// the guest allocator, so nothing may live above the guest heap's end. A
/// nested entry traps.
pub fn scratch<T>(hint: usize, body: impl FnOnce() -> T) -> T {
    if ACTIVE.swap(true, Relaxed) {
        core::arch::wasm32::unreachable();
    }
    let roots = crate::guest::root_top();
    GUEST.store(true, Relaxed);
    TOP.store(0, Relaxed);
    LIMIT.store(0, Relaxed);
    CHUNK.store(hint.max(MIN_CHUNK_BYTES) / 2, Relaxed);
    // SAFETY: no reference to the table is live.
    unsafe { chunks() }.count = 0;
    let result = body();
    GUEST.store(false, Relaxed);
    TOP.store(0, Relaxed);
    ACTIVE.store(false, Relaxed);
    crate::guest::reset_roots(roots);
    result
}

/// Count an allocation at `ptr` in its tracked block.
fn track(ptr: usize, delta: isize) {
    // SAFETY: no other reference to the table is live.
    let chunks = unsafe { chunks() };
    let count = chunks.count;
    let Some(index) = chunks.list[..count]
        .iter()
        .rposition(|chunk| (chunk.start..chunk.end).contains(&ptr))
    else {
        return;
    };
    let chunk = &mut chunks.list[index];
    chunk.live = chunk.live.saturating_add_signed(delta);
    // An empty block other than the current one is unrooted: a zero root
    // slot is skipped by the collector.
    if chunk.live == 0 && chunk.end != LIMIT.load(Relaxed) && chunk.root != 0 {
        // SAFETY: `root` is the root-stack slot this call pushed the block
        // into, below the current top.
        unsafe { (chunk.root as *mut u64).write_unaligned(0) };
        chunk.root = 0;
    }
}

/// A guest scratch range of `size` bytes at `align`.
fn guest_range(align: usize, size: usize) -> *mut u8 {
    let top = TOP.load(Relaxed);
    let start = top.checked_next_multiple_of(align);
    if top != 0
        && let Some(end) = start.and_then(|start| start.checked_add(size))
        && end <= LIMIT.load(Relaxed)
    {
        TOP.store(end, Relaxed);
        let start = top.next_multiple_of(align);
        track(start, 1);
        return start as *mut u8;
    }
    // A fresh block: twice the last one up to [`SHARED_CHUNK_BYTES`], at
    // least the request; a larger request gets a block of its own, which
    // is unrooted as soon as it is freed (a growing buffer then holds at
    // most its old and new capacity).
    let need = size.saturating_add(align);
    let chunk = if need > SHARED_CHUNK_BYTES {
        need
    } else {
        let chunk = CHUNK
            .load(Relaxed)
            .saturating_mul(2)
            .min(SHARED_CHUNK_BYTES)
            .max(need);
        CHUNK.store(chunk, Relaxed);
        chunk
    };
    let ptr = crate::guest::allocate_block(chunk);
    let header = ptr - varta_common::abi::STRING_HEADER_BYTES;
    // SAFETY: the allocator returned a block of at least `chunk` payload
    // bytes whose header precedes `ptr`. Stamped as an edge-free block, the
    // collector keeps it without reading its contents.
    unsafe {
        ((header + varta_common::value::HEAP_FLAGS_OFFSET) as *mut u32)
            .write_unaligned(varta_common::value::HEAP_ALLOCATED_FLAG);
        ((header + varta_common::value::HEAP_LINK_OFFSET) as *mut u32).write_unaligned(0);
    }
    let root = crate::guest::root_top();
    crate::guest::retain_root(((chunk as u64) << u32::BITS) | ptr as u64);
    let start = ptr.next_multiple_of(align);
    TOP.store(start + size, Relaxed);
    LIMIT.store(ptr + chunk, Relaxed);
    // SAFETY: no other reference to the table is live.
    let chunks = unsafe { chunks() };
    if chunks.count < TRACKED_CHUNKS {
        chunks.list[chunks.count] = Chunk {
            start: ptr,
            end: ptr + chunk,
            root,
            live: 1,
        };
        chunks.count += 1;
    }
    start as *mut u8
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
    report(
        Resource::Memory as i32,
        u32::try_from(requested).unwrap_or(u32::MAX),
    )
}
/// Store a failure code and its request in the guest's failure globals and
/// trap; the runtime reports the trap as typed.
pub fn report(code: i32, requested: u32) -> ! {
    // SAFETY: the accessor only sets the two exported i32 globals.
    unsafe { resource_failure(code, requested as i32) };
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
        if GUEST.load(Relaxed) {
            return guest_range(layout.align(), layout.size());
        }
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
        if GUEST.load(Relaxed) {
            // The newest range returns to the bump pointer; a block whose
            // ranges are all free is unrooted.
            if start.checked_add(layout.size()) == Some(TOP.load(Relaxed)) {
                TOP.store(start, Relaxed);
            }
            track(start, -1);
            return;
        }
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
        let guest = GUEST.load(Relaxed);
        let fits = |limit: usize| {
            if guest {
                start.checked_add(new_size).is_some_and(|end| end <= limit)
            } else {
                fit(start, 1, new_size, limit).is_some()
            }
        };
        if start.checked_add(layout.size()) == Some(TOP.load(Relaxed))
            && (new_size <= layout.size() || fits(LIMIT.load(Relaxed)))
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
