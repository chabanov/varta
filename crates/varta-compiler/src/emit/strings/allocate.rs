//! First-fit reusable blocks; collect before growing committed memory, once per request.
//!
//! V38 (`codegen_v2`) bounds the first scan of the free list to
//! [`SCAN_LIMIT`] blocks before taking fresh heap space, and grows the heap
//! without a collection below the collection threshold. A request that would
//! exceed the memory budget or fail to grow still collects first and then
//! scans the whole free list, so it fails only when no block fits after a
//! collection, as in earlier profiles.
use super::*;

/// V38: free blocks examined before fresh heap space is used, unless the
/// request already collected.
const SCAN_LIMIT: i32 = 8;

pub(super) fn function(globals: Globals, collect: u32) -> Function {
    const LEN: u32 = 0;
    const NEED: u32 = 1;
    const CURRENT: u32 = 2;
    const PREVIOUS: u32 = 3;
    const SIZE: u32 = 4;
    const REST: u32 = 5;
    const REPLACEMENT: u32 = 6;
    const END: u32 = 7;
    const RETRIED: u32 = 8;
    const REPEAT: u32 = 0;
    const DONE: u32 = 1;
    const RETRY_FROM_IF: u32 = 1;
    /// From inside `if growth failed { if !retried { .. } }` to the retry loop.
    const RETRY_FROM_GROWTH: u32 = 3;
    /// V38: free blocks the current scan may still examine.
    const BUDGET: u32 = 9;
    let bounded = globals.collect_at.is_some();
    let mut function = Function::new([(if bounded { BUDGET } else { RETRIED }, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(LEN),
            I::I32Const(abi::MAX_STRING_BYTES as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(LEN),
            I::I32Const((HEADER + ALIGN - 1) as i32),
            I::I32Add,
            I::I32Const(-(ALIGN as i32)),
            I::I32And,
            I::LocalSet(NEED),
            I::Loop(BlockType::Empty),
            I::GlobalGet(globals.free),
            I::LocalSet(CURRENT),
            I::I32Const(0),
            I::LocalSet(PREVIOUS),
        ],
    );
    if bounded {
        // After a collection the scan is unbounded (u32::MAX blocks).
        instructions(
            &mut function,
            [
                I::I32Const(-1),
                I::I32Const(SCAN_LIMIT),
                I::LocalGet(RETRIED),
                I::Select,
                I::LocalSet(BUDGET),
            ],
        );
    }
    instructions(
        &mut function,
        [
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(CURRENT),
            I::I32Eqz,
            I::BrIf(DONE),
        ],
    );
    if bounded {
        instructions(
            &mut function,
            [
                I::LocalGet(BUDGET),
                I::I32Eqz,
                I::BrIf(DONE),
                I::LocalGet(BUDGET),
                I::I32Const(1),
                I::I32Sub,
                I::LocalSet(BUDGET),
            ],
        );
    }
    instructions(
        &mut function,
        [
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
        ],
    );
    let grow_without_collection = globals.collect_at.is_some();
    // Before increasing the memory high-water mark, trace all exact roots.
    // V38 first grows a heap that ended below its collection threshold, which
    // is twice the heap left by the previous collection: total collection work
    // stays proportional to allocation instead of one full pass per page.
    // An end above the memory budget, or a failed growth, still collects
    // before it traps, so only memory that is live after a collection fails.
    instructions(
        &mut function,
        [
            I::LocalGet(END),
            I::MemorySize(MEMORY),
            I::I32Const(abi::WASM_PAGE_BYTES as i32),
            I::I32Mul,
            I::I32GtU,
            I::LocalGet(RETRIED),
            I::I32Eqz,
            I::I32And,
        ],
    );
    if let Some(collect_at) = globals.collect_at {
        instructions(
            &mut function,
            [
                I::LocalGet(END),
                I::GlobalGet(collect_at),
                I::I32GtU,
                I::LocalGet(END),
                I::I32Const(abi::MAX_MEMORY_BYTES as i32),
                I::I32GtU,
                I::I32Or,
                I::I32And,
            ],
        );
    }
    instructions(
        &mut function,
        [
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::LocalSet(RETRIED),
            I::Call(collect),
        ],
    );
    if let Some(collect_at) = globals.collect_at {
        collection_threshold(&mut function, globals.heap, collect_at);
    }
    instructions(
        &mut function,
        [
            I::Br(RETRY_FROM_IF),
            I::End,
            I::LocalGet(END),
            I::I32Const(abi::MAX_MEMORY_BYTES as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
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
            I::MemoryGrow(MEMORY),
            I::I32Const(-1),
            I::I32Eq,
            I::If(BlockType::Empty),
        ],
    );
    if grow_without_collection {
        // A growth refused by the store limits collects once and retries.
        instructions(
            &mut function,
            [
                I::LocalGet(RETRIED),
                I::I32Eqz,
                I::If(BlockType::Empty),
                I::I32Const(1),
                I::LocalSet(RETRIED),
                I::Call(collect),
            ],
        );
        if let Some(collect_at) = globals.collect_at {
            collection_threshold(&mut function, globals.heap, collect_at);
        }
        instructions(&mut function, [I::Br(RETRY_FROM_GROWTH), I::End]);
    }
    instructions(
        &mut function,
        [
            I::Unreachable,
            I::End,
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
        ],
    );
    function
}

/// After a collection, grow without collecting until the heap end reaches
/// twice the heap the collection left (bounded by the memory budget).
fn collection_threshold(function: &mut Function, heap: u32, collect_at: u32) {
    instructions(
        function,
        [
            I::GlobalGet(heap),
            I::I32Const(abi::OWNED_STRING_BASE as i32),
            I::I32Sub,
            I::I32Const(1),
            I::I32Shl,
            I::I32Const(abi::OWNED_STRING_BASE as i32),
            I::I32Add,
            I::GlobalSet(collect_at),
        ],
    );
}
