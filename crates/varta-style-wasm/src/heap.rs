//! The module's heap: segregated free lists over its own memory, so the
//! engine's state (sheets, the document mirror) persists across calls and
//! freed blocks are reused. Small blocks are power-of-two size classes;
//! larger ones are whole runs of 64 KiB pages kept on one first-fit list.
//! Memory grows with `memory.grow` and never shrinks.
use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};
use varta_common::abi::WASM_PAGE_BYTES;

/// The module's own memory.
const MEMORY: u32 = 0;
/// The smallest block: room for a free-list link.
const MIN_CLASS: usize = 16;
/// Size classes from 16 bytes to 32 KiB; larger blocks take whole pages.
const CLASSES: usize = 12;
const LARGEST_CLASS: usize = MIN_CLASS << (CLASSES - 1);

unsafe extern "C" {
    /// The linker's first byte past static data and the stack.
    static __heap_base: u8;
}

/// The next never-used byte; 0 before the first allocation.
static TOP: AtomicUsize = AtomicUsize::new(0);
/// Head of each size class's free list (0 is empty).
static FREE: [AtomicUsize; CLASSES] = [const { AtomicUsize::new(0) }; CLASSES];
/// Head of the large-block free list: each block starts with its next link
/// and its byte size.
static LARGE: AtomicUsize = AtomicUsize::new(0);

/// Only the symbol's address is taken, never its contents.
fn base() -> usize {
    core::ptr::addr_of!(__heap_base) as usize
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

/// Fresh memory from the top, aligned to `align`.
fn bump(size: usize, align: usize) -> *mut u8 {
    let mut top = TOP.load(Relaxed);
    if top == 0 {
        top = base();
    }
    let Some(start) = top.checked_next_multiple_of(align) else {
        return core::ptr::null_mut();
    };
    let Some(end) = start.checked_add(size) else {
        return core::ptr::null_mut();
    };
    if !ensure(end) {
        return core::ptr::null_mut();
    }
    TOP.store(end, Relaxed);
    start as *mut u8
}

/// The size class of a layout, or `None` for a large block.
fn class(layout: Layout) -> Option<usize> {
    let size = layout.size().max(layout.align()).max(MIN_CLASS);
    if size > LARGEST_CLASS {
        return None;
    }
    Some(size.next_power_of_two().trailing_zeros() as usize - MIN_CLASS.trailing_zeros() as usize)
}

/// A large block's byte size: whole pages.
fn large_size(layout: Layout) -> usize {
    layout.size().div_ceil(WASM_PAGE_BYTES) * WASM_PAGE_BYTES
}

struct Heap;

// SAFETY: wasm32 code here is single-threaded. A block is either fresh
// memory below the top or a block this allocator handed out and got back;
// free blocks hold only this allocator's links, written at their start.
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Some(class) = class(layout) {
            let head = FREE[class].load(Relaxed);
            if head != 0 {
                // SAFETY: a free block starts with its successor's address.
                let next = unsafe { (head as *const usize).read() };
                FREE[class].store(next, Relaxed);
                return head as *mut u8;
            }
            let size = MIN_CLASS << class;
            return bump(size, size);
        }
        let size = large_size(layout);
        let mut previous = 0usize;
        let mut block = LARGE.load(Relaxed);
        while block != 0 {
            // SAFETY: a free large block starts with `[next, size]`.
            let (next, available) = unsafe {
                let words = block as *const usize;
                (words.read(), words.add(1).read())
            };
            if available >= size {
                if previous == 0 {
                    LARGE.store(next, Relaxed);
                } else {
                    // SAFETY: `previous` is a free large block.
                    unsafe { (previous as *mut usize).write(next) };
                }
                return block as *mut u8;
            }
            previous = block;
            block = next;
        }
        bump(size, WASM_PAGE_BYTES)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let block = ptr as usize;
        if let Some(class) = class(layout) {
            // SAFETY: the block is at least `MIN_CLASS` bytes and ours.
            unsafe { (block as *mut usize).write(FREE[class].load(Relaxed)) };
            FREE[class].store(block, Relaxed);
            return;
        }
        // SAFETY: a large block is at least a page and ours.
        unsafe {
            let words = block as *mut usize;
            words.write(LARGE.load(Relaxed));
            words.add(1).write(large_size(layout));
        }
        LARGE.store(block, Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Heap = Heap;
