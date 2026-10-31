//! First-fit reusable blocks; collect before growing committed memory, once per request.
//!
//! V38 (`codegen_v2`) bounds the first scan of the free list to
//! [`SCAN_LIMIT`] blocks before taking fresh heap space, and grows the heap
//! without a collection below the collection threshold. A request that would
//! exceed the memory budget or fail to grow still collects first and then
//! scans the whole free list, so it fails only when no block fits after a
//! collection, as in earlier profiles. Before it collects, a v38 request
//! first scans the whole free list once ([`FULL`]): a heap that has reached
//! the memory budget then reuses free blocks at the cost of a v37 first-fit
//! scan instead of collecting for every request the bounded scan misses.
//! V38 (`data_limits`) grows memory geometrically and reports an exhausted
//! resource before trapping.
//!
//! V38 (`codegen_v2`) also keeps small blocks in exact-size free lists
//! ([`free_classes`], filled by the sweep): a
//! request first pops its own class, then takes fresh heap space within the
//! committed memory, both in constant time.
//! Only then does it scan the address-ordered list of larger free blocks as
//! before. Every unbounded retry (before a collection and after it) also
//! takes the first block of a larger class, splitting off a remainder of at
//! least one header and alignment unit, so a request still fails only when no
//! free block of any list fits after a collection.
use super::*;

// Locals of the allocator (parameter 0 is the requested byte count).
const LEN: u32 = 0;
const NEED: u32 = 1;
const CURRENT: u32 = 2;
const PREVIOUS: u32 = 3;
const SIZE: u32 = 4;
const REST: u32 = 5;
const REPLACEMENT: u32 = 6;
const END: u32 = 7;
const RETRIED: u32 = 8;
/// V38 (`codegen_v2`): free blocks the current scan may still examine.
const BUDGET: u32 = 9;
/// V38 (`codegen_v2`): the request already scanned the whole free list.
const FULL: u32 = 10;
const REPEAT: u32 = 0;
const DONE: u32 = 1;
const RETRY_FROM_IF: u32 = 1;
/// V38: free blocks examined before fresh heap space is used, unless the
/// request already collected.
const SCAN_LIMIT: i32 = 8;
/// V38 (`codegen_v2`): the class of the request or remainder, the next block
/// of a popped list, and the i64 mask of nonempty classes above the request's
/// (`free_classes`).
const CLASS: u32 = 11;
const NEXT: u32 = 12;
const CLASS_MASK: u32 = 13;

pub(super) fn function(
    globals: Globals,
    collect: u32,
    limits: varta_common::limits::DataLimits,
    calls: Option<super::incremental::Calls>,
) -> Function {
    use varta_common::limits::Resource;
    let bounded = globals.collect_at.is_some();
    let locals = if bounded { FULL } else { RETRIED };
    let mut groups = vec![(locals, ValType::I32)];
    if globals.classes.is_some() {
        groups = vec![(NEXT, ValType::I32), (1, ValType::I64)];
    }
    let mut function = Function::new(groups);
    let mut code = vec![
        I::LocalGet(LEN),
        I::I32Const(limits.allocation_bytes as i32),
        I::I32GtU,
        I::If(BlockType::Empty),
    ];
    code.extend(globals.fail(Resource::ValueSize, LEN));
    code.push(I::End);
    code.extend(super::incremental::allocation(globals, LEN, calls));
    code.extend([
        I::LocalGet(LEN),
        I::I32Const((HEADER + ALIGN - 1) as i32),
        I::I32Add,
        I::I32Const(-(ALIGN as i32)),
        I::I32And,
        I::LocalSet(NEED),
        I::Loop(BlockType::Empty),
    ]);
    if let Some(classes) = globals.classes {
        code.extend(free_classes::allocate(
            globals.heap,
            free_classes::Lists::new(classes),
            free_classes::AllocateLocals {
                need: NEED,
                current: CURRENT,
                end: END,
                rest: REST,
                replacement: REPLACEMENT,
                class: CLASS,
                next: NEXT,
                mask: CLASS_MASK,
            },
            super::incremental::blacken_taken(globals, CURRENT, calls),
        ));
    }
    code.extend([
        I::GlobalGet(globals.free),
        I::LocalSet(CURRENT),
        I::I32Const(0),
        I::LocalSet(PREVIOUS),
    ]);
    if bounded {
        // After a collection, or before one, the scan is unbounded
        // (u32::MAX blocks).
        code.extend([
            I::I32Const(-1),
            I::I32Const(SCAN_LIMIT),
            I::LocalGet(RETRIED),
            I::LocalGet(FULL),
            I::I32Or,
            I::Select,
            I::LocalSet(BUDGET),
        ]);
    }
    code.extend([
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(CURRENT),
        I::I32Eqz,
        I::BrIf(DONE),
    ]);
    if bounded {
        code.extend([
            I::LocalGet(BUDGET),
            I::I32Eqz,
            I::BrIf(DONE),
            I::LocalGet(BUDGET),
            I::I32Const(1),
            I::I32Sub,
            I::LocalSet(BUDGET),
        ]);
    }
    code.extend([
        I::LocalGet(CURRENT),
        I::I32Load(WORD),
        I::LocalTee(SIZE),
        I::LocalGet(NEED),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::LocalGet(SIZE),
        I::LocalGet(NEED),
        I::I32Sub,
        I::LocalTee(REST),
        I::I32Const((HEADER + ALIGN) as i32),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::LocalGet(CURRENT),
        I::LocalGet(NEED),
        I::I32Add,
        I::LocalSet(REPLACEMENT),
        I::LocalGet(REPLACEMENT),
        I::LocalGet(REST),
        I::I32Store(WORD),
        I::LocalGet(REPLACEMENT),
        I::I32Const(0),
        I::I32Store(FLAG),
        I::LocalGet(REPLACEMENT),
        I::LocalGet(CURRENT),
        I::I32Load(LINK),
        I::I32Store(LINK),
        I::LocalGet(CURRENT),
        I::LocalGet(NEED),
        I::I32Store(WORD),
        I::Else,
        I::LocalGet(CURRENT),
        I::I32Load(LINK),
        I::LocalSet(REPLACEMENT),
        I::End,
        I::LocalGet(PREVIOUS),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(REPLACEMENT),
        I::GlobalSet(globals.free),
        I::Else,
        I::LocalGet(PREVIOUS),
        I::LocalGet(REPLACEMENT),
        I::I32Store(LINK),
        I::End,
        I::LocalGet(CURRENT),
        I::I32Const(ALLOCATED),
        I::I32Store(FLAG),
        I::LocalGet(CURRENT),
        I::I32Const(0),
        I::I32Store(LINK),
    ]);
    code.extend(super::incremental::blacken_taken(globals, CURRENT, calls));
    code.extend([
        I::LocalGet(CURRENT),
        I::I32Const(HEADER as i32),
        I::I32Add,
        I::Return,
        I::End,
        I::LocalGet(CURRENT),
        I::LocalSet(PREVIOUS),
        I::LocalGet(CURRENT),
        I::I32Load(LINK),
        I::LocalSet(CURRENT),
        I::Br(REPEAT),
        I::End,
        I::End,
        I::GlobalGet(globals.heap),
        I::LocalGet(NEED),
        I::I32Add,
        I::LocalSet(END),
        // Before increasing the memory high-water mark, trace all exact roots.
        // V38 (`codegen_v2`) first grows a heap that ended below its
        // collection threshold, which is twice the heap left by the previous
        // collection: total collection work stays proportional to allocation
        // instead of one full pass per page. An end above the memory budget,
        // or a failed growth, still collects before it traps, so only memory
        // that is live after a collection fails.
        I::LocalGet(END),
        I::MemorySize(MEMORY),
        I::I32Const(abi::WASM_PAGE_BYTES as i32),
        I::I32Mul,
        I::I32GtU,
        I::LocalGet(RETRIED),
        I::I32Eqz,
        I::I32And,
    ]);
    if let Some(collect_at) = globals.collect_at {
        code.extend([
            I::LocalGet(END),
            I::GlobalGet(collect_at),
            I::I32GtU,
            I::LocalGet(END),
            I::I32Const(abi::MAX_MEMORY_BYTES as i32),
            I::I32GtU,
            I::I32Or,
            I::I32And,
        ]);
    }
    code.push(I::If(BlockType::Empty));
    if let (Some(state), Some(calls)) = (globals.collector, calls) {
        // V38 (`incremental_collection`): while the heap end stays in the
        // lower part of the memory maximum, the heap grows while a cycle
        // runs, and past the threshold a cycle starts once the heap is large
        // enough (after one full scan of the free list) and the request
        // retries, since the cycle's metadata moved the heap end. Any other
        // request past the threshold (a small heap, a large request, an end
        // above that part, a cycle that could not start) collects at once,
        // abandoning a cycle in progress.
        code.extend([
            I::GlobalGet(state.global(super::incremental::Slot::Phase)),
            I::GlobalGet(globals.heap),
            I::I32Const(globals.base as i32),
            I::I32Sub,
            I::I32Const(varta_common::collector::INCREMENTAL_MIN_HEAP_BYTES as i32),
            I::I32GeU,
            I::I32Or,
            I::LocalGet(END),
            I::I32Const(
                (abi::MAX_MEMORY_BYTES / varta_common::collector::INCREMENTAL_HEAP_DIVISOR as usize)
                    as i32,
            ),
            I::I32LeU,
            I::I32And,
            // A large request collects at once and goes where the garbage was.
            I::LocalGet(NEED),
            I::GlobalGet(globals.heap),
            I::I32Const(globals.base as i32),
            I::I32Sub,
            I::I32Const(varta_common::collector::LARGE_REQUEST_SHIFT as i32),
            I::I32ShrU,
            I::I32LtU,
            I::I32And,
            I::If(BlockType::Empty),
            I::GlobalGet(state.global(super::incremental::Slot::Phase)),
            I::BrIf(1),
        ]);
        code.extend(scan_everything_first(globals, RETRY_FROM_IF + 1));
        code.extend([I::Call(calls.begin), I::BrIf(RETRY_FROM_IF + 1), I::End]);
    }
    code.extend(scan_everything_first(globals, RETRY_FROM_IF));
    code.extend([I::I32Const(1), I::LocalSet(RETRIED), I::Call(collect)]);
    code.extend(collection_threshold(globals));
    code.extend([
        I::Br(RETRY_FROM_IF),
        I::End,
        I::LocalGet(END),
        I::I32Const(abi::MAX_MEMORY_BYTES as i32),
        I::I32GtU,
        I::If(BlockType::Empty),
    ]);
    code.extend(globals.fail(Resource::Memory, NEED));
    code.extend([
        I::End,
        I::LocalGet(END),
        I::MemorySize(MEMORY),
        I::I32Const(abi::WASM_PAGE_BYTES as i32),
        I::I32Mul,
        I::I32GtU,
        I::If(BlockType::Empty),
        I::LocalGet(END),
        I::I32Const((abi::WASM_PAGE_BYTES - 1) as i32),
        I::I32Add,
        I::I32Const(abi::WASM_PAGE_BYTES as i32),
        I::I32DivU,
        I::MemorySize(MEMORY),
        I::I32Sub,
    ]);
    code.extend(grow(globals, collect, limits));
    code.extend([
        I::End,
        I::GlobalGet(globals.heap),
        I::LocalTee(CURRENT),
        I::LocalGet(NEED),
        I::I32Store(WORD),
        I::LocalGet(CURRENT),
        I::I32Const(ALLOCATED),
        I::I32Store(FLAG),
        I::LocalGet(CURRENT),
        I::I32Const(0),
        I::I32Store(LINK),
        I::LocalGet(END),
        I::GlobalSet(globals.heap),
        I::LocalGet(CURRENT),
        I::I32Const(HEADER as i32),
        I::I32Add,
        I::Return,
        I::End,
        I::Unreachable,
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// `[needed pages] -> []`: grow memory or report the memory budget. V38
/// (`data_limits`) grows by at least half the committed pages when the budget
/// allows, so collections run once per geometric growth step rather than on
/// every page (amortized linear collection work while live data grows);
/// it falls back to the exact need. Earlier profiles grow by the exact need.
/// V38 (`codegen_v2`): a growth refused by the store limits collects once
/// and retries the request.
fn grow(
    globals: Globals,
    collect: u32,
    limits: varta_common::limits::DataLimits,
) -> Vec<I<'static>> {
    use varta_common::limits::Resource;
    let mut code = Vec::new();
    // Blocks between the growth-failure branch and the allocator's retry
    // loop: the failure `if`, the enclosing `if END > committed` and, with
    // geometric growth, the fallback `if`.
    let mut retry_from_growth = 2;
    if limits.amortized_growth {
        retry_from_growth += 1;
        code.extend([
            I::LocalSet(SIZE),
            I::MemorySize(MEMORY),
            I::I32Const(1),
            I::I32ShrU,
            I::LocalSet(REST),
            I::LocalGet(REST),
            I::LocalGet(SIZE),
            I::LocalGet(REST),
            I::LocalGet(SIZE),
            I::I32GtU,
            I::Select,
            I::MemoryGrow(MEMORY),
            I::I32Const(-1),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(SIZE),
        ]);
    }
    code.extend([
        I::MemoryGrow(MEMORY),
        I::I32Const(-1),
        I::I32Eq,
        I::If(BlockType::Empty),
    ]);
    if globals.collect_at.is_some() {
        code.extend([I::LocalGet(RETRIED), I::I32Eqz, I::If(BlockType::Empty)]);
        code.extend(scan_everything_first(globals, retry_from_growth + 1));
        code.extend([I::I32Const(1), I::LocalSet(RETRIED), I::Call(collect)]);
        code.extend(collection_threshold(globals));
        code.extend([I::Br(retry_from_growth + 1), I::End]);
    }
    code.extend(globals.fail(Resource::Memory, NEED));
    code.push(I::End);
    if limits.amortized_growth {
        code.push(I::End);
    }
    code
}

/// V38 (`codegen_v2`): before the first collection of a request, retry it
/// once with an unbounded free-list scan; `retry` is the depth of the retry
/// loop from the enclosing block. Nothing for other modules.
fn scan_everything_first(globals: Globals, retry: u32) -> Vec<I<'static>> {
    if globals.collect_at.is_none() {
        return Vec::new();
    }
    vec![
        I::LocalGet(FULL),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::I32Const(1),
        I::LocalSet(FULL),
        I::Br(retry + 1),
        I::End,
    ]
}

/// V38 (`codegen_v2`): after a collection, grow without collecting until the
/// heap end reaches twice the managed heap the collection left (bounded by
/// the memory budget). Nothing for other modules.
pub(super) fn collection_threshold(globals: Globals) -> Vec<I<'static>> {
    let mut code = super::incremental::collected(globals);
    code.extend(threshold(globals));
    code
}

/// V38 (`codegen_v2`): the next collection threshold, twice the managed heap
/// left by a collection. Nothing for other modules.
pub(super) fn threshold(globals: Globals) -> Vec<I<'static>> {
    let Some(collect_at) = globals.collect_at else {
        return Vec::new();
    };
    let mut code = Vec::new();
    code.extend([
        I::GlobalGet(globals.heap),
        I::I32Const(globals.base as i32),
        I::I32Sub,
        I::I32Const(1),
        I::I32Shl,
        I::I32Const(globals.base as i32),
        I::I32Add,
        I::GlobalSet(collect_at),
    ]);
    code
}
