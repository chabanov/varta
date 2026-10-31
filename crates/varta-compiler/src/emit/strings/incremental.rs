//! V38 (`incremental_collection`): incremental collection inside turns and
//! idle collection between them.
//!
//! A cycle marks from a snapshot of the exact roots (snapshot at the
//! beginning): `CollectBegin` shades every root slot, then every allocation
//! performs a bounded quantum of work (`CollectStep`, `collector::STEP_MIN_UNITS`
//! plus the request's bytes shifted right by `collector::STEP_BYTE_SHIFT`).
//! While the cycle marks, a deletion barrier before every reference store
//! that overwrites or removes a heap reference shades the old value
//! (`Shade`), and a bulk change of an object (a move, fill or truncation of
//! its slots) first traces the whole object (`ShadeObject`), so every block
//! reachable when the cycle began is marked; blocks allocated during the
//! marking are black. Marking state lives outside the block headers, in two
//! side bitmaps with one bit per header-sized span of the managed heap below
//! the cycle's limit (the heap end when it began): the block's mark bit and a
//! bit recording that a bulk barrier or black allocation covered the whole
//! object. Value readers therefore never see a mark. Blocks at or above the
//! limit were allocated during the cycle and are implicitly marked. Gray
//! blocks wait on a mark stack; a full stack records an overflow, after which
//! a rescan of the heap traces every marked block again, so no mark is lost.
//! An array is traced `collector::TRACE_CHUNK_SLOTS` slots per quantum. When
//! nothing is gray the sweep starts and runs lazily over the blocks below the
//! limit, rebuilding the exact-size lists as it goes and publishing the
//! larger free runs when it finishes. The bitmaps and the mark stack share one
//! ordinary heap block per cycle, taken from the heap end without a
//! collection (and only within the memory budget) and freed by the next
//! cycle's sweep. Cycles start only for a heap of at least
//! `collector::INCREMENTAL_MIN_HEAP_BYTES` below the memory maximum divided by
//! `collector::INCREMENTAL_HEAP_DIVISOR`, and not for a large request
//! (`collector::LARGE_REQUEST_SHIFT`); such requests collect completely.
//!
//! A request the memory budget or the store refuses still runs one
//! complete, atomic collection first (which abandons a cycle in progress
//! and forgets its metadata), so an allocation fails exactly where it did
//! before. The idle export (`varta_common::collector::IDLE_EXPORT`), which
//! the runtime calls only between turns of an application instance under
//! the maintenance budget, starts a cycle when the bytes allocated since the
//! last completed one reach `collector::IDLE_MIN_BYTES` and the live bytes
//! it left shifted right by `collector::IDLE_LIVE_SHIFT`, and continues a
//! cycle in progress. Every decision reads heap state only, so the same
//! allocation history always gives the same collections.
use super::collect::{BLOCK, EDGE_CURSOR, EDGE_MASK, FIELD_COUNT, IS_ARRAY};
use super::*;
use varta_common::collector;

/// One collector state global (all i32).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(in crate::emit) enum Slot {
    /// Bytes allocated since the last completed cycle or collection.
    Allocated,
    /// Live bytes the last completed cycle or collection left.
    Live,
    /// The cycle's [`Phase`].
    Phase,
    /// 1 while the cycle marks: the write barriers test only this.
    Marking,
    /// The heap end when the cycle began; blocks at or above it are black.
    Limit,
    /// The mark bitmap and the whole-object-shaded bitmap (one bit per
    /// header-sized span of the heap below the limit each), in the cycle's
    /// metadata block before the mark stack.
    Bitmap,
    Traced,
    /// The payload of the mark stack block, its top and its end.
    Stack,
    StackTop,
    StackEnd,
    /// A push found the mark stack full.
    Overflow,
    /// The rescan cursor after an overflow (a block header), or zero.
    Scan,
    /// An array whose tracing continues at `PartialNext`, or zero.
    Partial,
    PartialNext,
    /// The lazy sweep's next block, its open dead run and the pending list of
    /// larger free runs.
    Cursor,
    Run,
    LargeHead,
    LargeTail,
    /// Live bytes the sweep found so far.
    Swept,
}
impl Slot {
    pub(in crate::emit) const ALL: [Self; 19] = [
        Self::Allocated,
        Self::Live,
        Self::Phase,
        Self::Marking,
        Self::Limit,
        Self::Bitmap,
        Self::Traced,
        Self::Stack,
        Self::StackTop,
        Self::StackEnd,
        Self::Overflow,
        Self::Scan,
        Self::Partial,
        Self::PartialNext,
        Self::Cursor,
        Self::Run,
        Self::LargeHead,
        Self::LargeTail,
        Self::Swept,
    ];
}

/// The phase of the current cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
enum Phase {
    Idle,
    Marking,
    Sweeping,
}

/// Bytes of one mark-stack entry (a block header address).
const ENTRY: i32 = size_of::<u32>() as i32;
/// Bytes of one bitmap word.
const WORD_BYTES: i32 = size_of::<u32>() as i32;
const WORD_BYTES_SHIFT: i32 = WORD_BYTES.trailing_zeros() as i32;
const WORD_BITS_SHIFT: i32 = u32::BITS.trailing_zeros() as i32;
/// One bit per header-sized span: a block is at least one header long, so
/// no two blocks begin in the same span and its first span names it.
const SPAN_SHIFT: i32 = HEADER.trailing_zeros() as i32;
/// The byte a swept dead block's payload is filled with in a stress build,
/// so a block freed while still reachable is detected by its readers.
const POISON: i32 = 0xA5;

/// The collector's state globals, starting at `first`, and the work units
/// per allocation of a test-only stress build (`with_stress`).
#[derive(Clone, Copy, Debug)]
pub(in crate::emit) struct State {
    pub first: u32,
    pub stress: Option<u32>,
}
impl State {
    pub(in crate::emit) fn global(self, slot: Slot) -> u32 {
        self.first + slot as u32
    }
}

/// The write barriers of emitted code and helpers: the marking flag and the
/// shading helpers. `None` outside v38 heap modules.
#[derive(Clone, Copy, Debug)]
pub(in crate::emit) struct Barrier {
    pub marking: u32,
    pub shade: u32,
    pub shade_object: u32,
}
impl Barrier {
    /// Before overwriting or removing the reference at `memarg` from the
    /// address in i32 local `address`: shade the old value while marking.
    pub(in crate::emit) fn slot(self, address: u32, memarg: MemArg) -> Vec<I<'static>> {
        vec![
            I::GlobalGet(self.marking),
            I::If(BlockType::Empty),
            I::LocalGet(address),
            I::I64Load(memarg),
            I::Call(self.shade),
            I::End,
        ]
    }
    /// Before a bulk change (move, fill, truncation) of the object whose
    /// payload is in i32 local `payload`: trace all of it while marking.
    pub(in crate::emit) fn object(self, payload: u32) -> Vec<I<'static>> {
        vec![
            I::GlobalGet(self.marking),
            I::If(BlockType::Empty),
            I::LocalGet(payload),
            I::Call(self.shade_object),
            I::End,
        ]
    }
    /// As [`Self::slot`] for the payload of the handle in i64 local `handle`.
    pub(in crate::emit) fn handle_slot(self, handle: u32, memarg: MemArg) -> Vec<I<'static>> {
        vec![
            I::GlobalGet(self.marking),
            I::If(BlockType::Empty),
            I::LocalGet(handle),
            I::I32WrapI64,
            I::I64Load(memarg),
            I::Call(self.shade),
            I::End,
        ]
    }
}

/// [`Barrier::slot`], or nothing without a barrier.
pub(in crate::emit) fn slot_code(
    barrier: Option<Barrier>,
    address: u32,
    memarg: MemArg,
) -> Vec<I<'static>> {
    barrier.map_or_else(Vec::new, |barrier| barrier.slot(address, memarg))
}
/// Before a bulk change of the backing of the array (or dictionary vector)
/// whose handle is in i64 local `handle`: trace the backing whole while
/// marking. Nothing without a barrier.
pub(in crate::emit) fn backing_code(barrier: Option<Barrier>, handle: u32) -> Vec<I<'static>> {
    let Some(barrier) = barrier else {
        return Vec::new();
    };
    vec![
        I::GlobalGet(barrier.marking),
        I::If(BlockType::Empty),
        I::LocalGet(handle),
        I::I32WrapI64,
        I::I64Load(MemArg {
            offset: varta_common::value::VECTOR_STORAGE_OFFSET,
            ..ROOT
        }),
        I::I32WrapI64,
        I::Call(barrier.shade_object),
        I::End,
    ]
}
/// [`Barrier::object`], or nothing without a barrier.
pub(in crate::emit) fn object_code(barrier: Option<Barrier>, payload: u32) -> Vec<I<'static>> {
    barrier.map_or_else(Vec::new, |barrier| barrier.object(payload))
}

/// Helper indices the collector's functions call.
#[derive(Clone, Copy, Debug)]
pub(in crate::emit) struct Calls {
    pub collect: u32,
    pub begin: u32,
    pub step: u32,
    pub trace: u32,
    pub shade: u32,
    pub blacken: u32,
}

/// Count `len` (a local holding the requested bytes) as allocated and, while
/// a cycle runs, perform the request's quantum of collection work. A stress
/// build starts the next cycle at once and performs its fixed quantum.
pub(super) fn allocation(globals: Globals, len: u32, calls: Option<Calls>) -> Vec<I<'static>> {
    let (Some(state), Some(calls)) = (globals.collector, calls) else {
        return Vec::new();
    };
    let mut code = vec![
        I::GlobalGet(state.global(Slot::Allocated)),
        I::LocalGet(len),
        I::I32Add,
        I::GlobalSet(state.global(Slot::Allocated)),
        I::GlobalGet(state.global(Slot::Phase)),
        I::If(BlockType::Empty),
    ];
    if let Some(units) = state.stress {
        code.push(I::I32Const(units as i32));
    } else {
        code.extend([
            I::LocalGet(len),
            I::I32Const(collector::STEP_BYTE_SHIFT as i32),
            I::I32ShrU,
            I::I32Const(collector::STEP_MIN_UNITS as i32),
            I::I32Add,
        ]);
    }
    code.extend([I::Call(calls.step), I::Drop, I::End]);
    if state.stress.is_some() {
        code.extend([
            I::GlobalGet(state.global(Slot::Phase)),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::Call(calls.begin),
            I::Drop,
            I::End,
        ]);
    }
    code
}

/// Before a block taken from a free list (below the cycle's limit) is handed
/// out while the cycle marks: make it black.
pub(super) fn blacken_taken(globals: Globals, block: u32, calls: Option<Calls>) -> Vec<I<'static>> {
    let (Some(state), Some(calls)) = (globals.collector, calls) else {
        return Vec::new();
    };
    vec![
        I::GlobalGet(state.global(Slot::Marking)),
        I::If(BlockType::Empty),
        I::LocalGet(block),
        I::Call(calls.blacken),
        I::End,
    ]
}

/// After a completed atomic collection: nothing allocated since, and the
/// heap it left is the live estimate.
pub(super) fn collected(globals: Globals) -> Vec<I<'static>> {
    let Some(state) = globals.collector else {
        return Vec::new();
    };
    vec![
        I::I32Const(0),
        I::GlobalSet(state.global(Slot::Allocated)),
        I::GlobalGet(globals.heap),
        I::I32Const(globals.base as i32),
        I::I32Sub,
        I::GlobalSet(state.global(Slot::Live)),
    ]
}

/// The start of an atomic collection: abandon a cycle in progress and forget
/// its metadata (the collection frees the bitmaps and the mark stack; the
/// sweep rebuilds every free list).
pub(super) fn abandon(globals: Globals) -> Vec<I<'static>> {
    let Some(state) = globals.collector else {
        return Vec::new();
    };
    [
        Slot::Phase,
        Slot::Marking,
        Slot::Bitmap,
        Slot::Traced,
        Slot::Stack,
        Slot::StackTop,
        Slot::StackEnd,
        Slot::Overflow,
        Slot::Scan,
        Slot::Partial,
        Slot::PartialNext,
        Slot::Run,
        Slot::LargeHead,
        Slot::LargeTail,
    ]
    .into_iter()
    .flat_map(|slot| [I::I32Const(0), I::GlobalSet(state.global(slot))])
    .collect()
}

/// Locals holding one block's bit position: the byte offset of its bitmap
/// word (the same in the mark and the traced bitmap) and its mask.
#[derive(Clone, Copy)]
struct Bits {
    offset: u32,
    mask: u32,
}
impl Bits {
    /// Locate the bits of the block whose header is in `block`.
    fn locate(self, globals: Globals, block: u32) -> Vec<I<'static>> {
        vec![
            I::LocalGet(block),
            I::I32Const(globals.base as i32),
            I::I32Sub,
            I::I32Const(SPAN_SHIFT),
            I::I32ShrU,
            I::LocalTee(self.mask),
            I::I32Const(WORD_BITS_SHIFT),
            I::I32ShrU,
            I::I32Const(WORD_BYTES_SHIFT),
            I::I32Shl,
            I::LocalSet(self.offset),
            // `i32.shl` takes its count modulo 32: the bit within the word.
            I::I32Const(1),
            I::LocalGet(self.mask),
            I::I32Shl,
            I::LocalSet(self.mask),
        ]
    }
    /// `[] -> [i32]`: the address of the located word in the bitmap whose
    /// address is in global `bitmap`.
    fn word(self, bitmap: u32) -> [I<'static>; 3] {
        [I::GlobalGet(bitmap), I::LocalGet(self.offset), I::I32Add]
    }
    /// `[] -> [i32]`: the located bit, nonzero when set.
    fn test(self, bitmap: u32) -> Vec<I<'static>> {
        let mut code = self.word(bitmap).to_vec();
        code.extend([I::I32Load(WORD), I::LocalGet(self.mask), I::I32And]);
        code
    }
    fn set(self, bitmap: u32) -> Vec<I<'static>> {
        let mut code = self.word(bitmap).to_vec();
        code.extend(self.word(bitmap));
        code.extend([
            I::I32Load(WORD),
            I::LocalGet(self.mask),
            I::I32Or,
            I::I32Store(WORD),
        ]);
        code
    }
}

/// Push the header in `block` onto the mark stack, or record an overflow.
fn push(state: State, block: u32) -> Vec<I<'static>> {
    vec![
        I::GlobalGet(state.global(Slot::StackTop)),
        I::GlobalGet(state.global(Slot::StackEnd)),
        I::I32LtU,
        I::If(BlockType::Empty),
        I::GlobalGet(state.global(Slot::StackTop)),
        I::LocalGet(block),
        I::I32Store(WORD),
        I::GlobalGet(state.global(Slot::StackTop)),
        I::I32Const(ENTRY),
        I::I32Add,
        I::GlobalSet(state.global(Slot::StackTop)),
        I::Else,
        I::I32Const(1),
        I::GlobalSet(state.global(Slot::Overflow)),
        I::End,
    ]
}

/// The visit of one child (header in `BLOCK`) while tracing: below the limit,
/// an unmarked child is marked and, when it has edges, pushed.
fn visit(globals: Globals, state: State, bits: Bits, edges: i32) -> Vec<I<'static>> {
    let mark = state.global(Slot::Bitmap);
    let mut code = vec![
        I::LocalGet(BLOCK),
        I::GlobalGet(state.global(Slot::Limit)),
        I::I32LtU,
        I::If(BlockType::Empty),
    ];
    code.extend(bits.locate(globals, BLOCK));
    code.extend(bits.test(mark));
    code.extend([I::I32Eqz, I::If(BlockType::Empty)]);
    code.extend(bits.set(mark));
    code.extend([
        I::LocalGet(BLOCK),
        I::I32Load(FLAG),
        I::I32Const(edges),
        I::I32And,
        I::If(BlockType::Empty),
    ]);
    code.extend(push(state, BLOCK));
    code.extend([I::End, I::End, I::End]);
    code
}

/// `Shade (i64 handle) -> ()`: mark the block of a heap handle below the
/// limit and push it, once per cycle. Called only while marking.
pub(super) fn shade(globals: Globals) -> Function {
    const HANDLE: u32 = 0;
    const PTR: u32 = 1;
    const BLOCK: u32 = 2;
    const BITS: Bits = Bits { offset: 3, mask: 4 };
    let mut function = Function::new([(BITS.mask, ValType::I32)]);
    let Some(state) = globals.collector else {
        function.instruction(&I::End);
        return function;
    };
    let mark = state.global(Slot::Bitmap);
    // A block lies below the limit by its header, not its payload.
    let mut code = vec![
        I::LocalGet(HANDLE),
        I::I32WrapI64,
        I::LocalTee(PTR),
        I::I32Const((globals.base + HEADER) as i32),
        I::I32LtU,
        I::LocalGet(PTR),
        I::I32Const(HEADER as i32),
        I::I32Sub,
        I::LocalTee(BLOCK),
        I::GlobalGet(state.global(Slot::Limit)),
        I::I32GeU,
        I::I32Or,
        I::If(BlockType::Empty),
        I::Return,
        I::End,
        I::LocalGet(BLOCK),
        I::I32Load(FLAG),
        I::I32Const(ALLOCATED),
        I::I32And,
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
    ];
    code.extend(BITS.locate(globals, BLOCK));
    code.extend(BITS.test(mark));
    code.extend([I::If(BlockType::Empty), I::Return, I::End]);
    code.extend(BITS.set(mark));
    code.extend(push(state, BLOCK));
    code.push(I::End);
    instructions(&mut function, code);
    function
}

/// `Blacken (i32 block) -> ()`: a block below the limit handed out while
/// marking is marked and traced (its new contents need no tracing).
pub(super) fn blacken(globals: Globals) -> Function {
    const BLOCK: u32 = 0;
    const BITS: Bits = Bits { offset: 1, mask: 2 };
    let mut function = Function::new([(BITS.mask, ValType::I32)]);
    let Some(state) = globals.collector else {
        function.instruction(&I::End);
        return function;
    };
    let mut code = vec![
        I::LocalGet(BLOCK),
        I::GlobalGet(state.global(Slot::Limit)),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::Return,
        I::End,
    ];
    code.extend(BITS.locate(globals, BLOCK));
    code.extend(BITS.set(state.global(Slot::Bitmap)));
    code.extend(BITS.set(state.global(Slot::Traced)));
    code.push(I::End);
    instructions(&mut function, code);
    function
}

// Locals of the tracing functions (`trace`, `mark`): 1 is the block, 3 to
// 18 follow the atomic collector's numbering (`collect`).
const CURRENT: u32 = 1;
const CHILD_BITS: Bits = Bits {
    offset: 19,
    mask: 20,
};
const VISITED: u32 = 21;
const FINISHED: u32 = 22;
const TRACE_LOCALS: u32 = FINISHED;

/// Shade the children of the marked block in `CURRENT` from slot `start`
/// (a local; arrays only) for at most `max` (a local) slots of an array; an
/// unfinished array becomes the partial block, a finished one stops being
/// it. Leaves the slots visited in `VISITED`. The traced bit is not set: it
/// records only the whole-object shading of a bulk barrier and black
/// allocation, so a block is traced at most once by `ShadeObject`.
fn trace_code(
    globals: Globals,
    state: State,
    (closures, frames, wide): (bool, bool, bool),
    limits: varta_common::limits::DataLimits,
    (start, max): (u32, u32),
) -> Vec<I<'static>> {
    let (setup, edge_loop) = super::collect::edge_parts(
        globals,
        wide,
        limits.array_elements,
        &visit(
            globals,
            state,
            CHILD_BITS,
            super::collect::edge_flags(closures, frames),
        ),
    );
    let mut code = vec![
        I::I32Const(1),
        I::LocalSet(FINISHED),
        I::I32Const(0),
        I::LocalSet(VISITED),
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(super::collect::edge_flags(closures, frames)),
        I::I32And,
        I::If(BlockType::Empty),
    ];
    code.extend(setup);
    code.extend([
        I::LocalGet(IS_ARRAY),
        I::If(BlockType::Empty),
        // Scalar elements have no edges.
        I::LocalGet(EDGE_MASK),
        I::I32Eqz,
        I::LocalGet(start),
        I::LocalGet(FIELD_COUNT),
        I::I32GeU,
        I::I32Or,
        I::If(BlockType::Empty),
        I::I32Const(0),
        I::LocalSet(FIELD_COUNT),
        I::Else,
        I::LocalGet(EDGE_CURSOR),
        I::LocalGet(start),
        I::I32Const(varta_common::value::ARRAY_SLOT_BYTES.trailing_zeros() as i32),
        I::I32Shl,
        I::I32Add,
        I::LocalSet(EDGE_CURSOR),
        I::LocalGet(FIELD_COUNT),
        I::LocalGet(start),
        I::I32Sub,
        I::LocalSet(FIELD_COUNT),
        I::End,
        I::LocalGet(FIELD_COUNT),
        I::LocalGet(max),
        I::I32GtU,
        I::If(BlockType::Empty),
        I::LocalGet(CURRENT),
        I::GlobalSet(state.global(Slot::Partial)),
        I::LocalGet(start),
        I::LocalGet(max),
        I::I32Add,
        I::GlobalSet(state.global(Slot::PartialNext)),
        I::LocalGet(max),
        I::LocalSet(FIELD_COUNT),
        I::I32Const(0),
        I::LocalSet(FINISHED),
        I::End,
        I::End,
        I::LocalGet(FIELD_COUNT),
        I::LocalSet(VISITED),
    ]);
    code.extend(edge_loop);
    code.push(I::End);
    code.extend([
        I::LocalGet(FINISHED),
        I::LocalGet(CURRENT),
        I::GlobalGet(state.global(Slot::Partial)),
        I::I32Eq,
        I::I32And,
        I::If(BlockType::Empty),
        I::I32Const(0),
        I::GlobalSet(state.global(Slot::Partial)),
        I::End,
    ]);
    code
}

/// What the tracing functions need to know about the module.
#[derive(Clone, Copy)]
pub(in crate::emit) struct Tracing {
    pub closures: bool,
    pub frames: bool,
    pub wide: bool,
    pub limits: varta_common::limits::DataLimits,
}

/// `CollectTrace (i32 start, i32 block, i32 max) -> i32 slots`: trace one
/// marked block ([`trace_code`]): the partial array, a rescanned block, and a
/// whole object before a bulk change.
pub(super) fn trace(globals: Globals, tracing: Tracing) -> Function {
    const START: u32 = 0;
    const MAX: u32 = 2;
    let mut function = Function::new([(TRACE_LOCALS - MAX, ValType::I32)]);
    let Some(state) = globals.collector else {
        instructions(&mut function, [I::I32Const(0), I::End]);
        return function;
    };
    let mut code = trace_code(
        globals,
        state,
        (tracing.closures, tracing.frames, tracing.wide),
        tracing.limits,
        (START, MAX),
    );
    code.extend([I::LocalGet(VISITED), I::End]);
    instructions(&mut function, code);
    function
}

/// `CollectMark (i32 units) -> i32 units`: trace gray blocks from the mark
/// stack until it is empty or `units` (a slot or a block each) are spent;
/// returns the units left (positive: the stack is empty). An array is traced
/// in chunks.
pub(super) fn mark(globals: Globals, tracing: Tracing) -> Function {
    const UNITS: u32 = 0;
    const START: u32 = 2;
    const MAX: u32 = 3;
    let mut function = Function::new([(TRACE_LOCALS - UNITS, ValType::I32)]);
    let Some(state) = globals.collector else {
        instructions(&mut function, [I::LocalGet(UNITS), I::End]);
        return function;
    };
    let g = |slot| state.global(slot);
    let mut code = vec![
        I::I32Const(0),
        I::LocalSet(START),
        I::I32Const(collector::TRACE_CHUNK_SLOTS as i32),
        I::LocalSet(MAX),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(UNITS),
        I::I32Const(0),
        I::I32LeS,
        I::GlobalGet(g(Slot::StackTop)),
        I::GlobalGet(g(Slot::Stack)),
        I::I32LeU,
        I::I32Or,
        // The partial array is resumed by the caller between pops.
        I::GlobalGet(g(Slot::Partial)),
        I::I32Or,
        I::BrIf(1),
        I::GlobalGet(g(Slot::StackTop)),
        I::I32Const(ENTRY),
        I::I32Sub,
        I::GlobalSet(g(Slot::StackTop)),
        I::GlobalGet(g(Slot::StackTop)),
        I::I32Load(WORD),
        I::LocalSet(CURRENT),
        I::LocalGet(UNITS),
        I::I32Const(1),
        I::I32Sub,
        I::LocalSet(UNITS),
    ];
    code.extend(trace_code(
        globals,
        state,
        (tracing.closures, tracing.frames, tracing.wide),
        tracing.limits,
        (START, MAX),
    ));
    code.extend([
        I::LocalGet(UNITS),
        I::LocalGet(VISITED),
        I::I32Sub,
        I::LocalSet(UNITS),
        I::Br(0),
        I::End,
        I::End,
        I::LocalGet(UNITS),
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// `ShadeObject (i32 payload) -> ()`: before a bulk change of an object below
/// the limit while marking, mark it and trace all of it at once.
pub(super) fn shade_object(globals: Globals, trace: u32) -> Function {
    const PAYLOAD: u32 = 0;
    const BLOCK: u32 = 1;
    const BITS: Bits = Bits { offset: 2, mask: 3 };
    let mut function = Function::new([(BITS.mask, ValType::I32)]);
    let Some(state) = globals.collector else {
        function.instruction(&I::End);
        return function;
    };
    let mut code = vec![
        I::LocalGet(PAYLOAD),
        I::I32Const((globals.base + HEADER) as i32),
        I::I32LtU,
        I::LocalGet(PAYLOAD),
        I::I32Const(HEADER as i32),
        I::I32Sub,
        I::LocalTee(BLOCK),
        I::GlobalGet(state.global(Slot::Limit)),
        I::I32GeU,
        I::I32Or,
        I::If(BlockType::Empty),
        I::Return,
        I::End,
    ];
    code.extend(BITS.locate(globals, BLOCK));
    code.extend(BITS.test(state.global(Slot::Traced)));
    code.extend([I::If(BlockType::Empty), I::Return, I::End]);
    code.extend(BITS.set(state.global(Slot::Bitmap)));
    code.extend(BITS.set(state.global(Slot::Traced)));
    code.extend([
        I::I32Const(0),
        I::LocalGet(BLOCK),
        I::I32Const(-1),
        I::Call(trace),
        I::Drop,
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// The idle policy, `[] -> [i32]`: the bytes allocated since the last
/// completed cycle reach both the minimum and the shifted live bytes.
fn due(state: State, scratch: u32) -> Vec<I<'static>> {
    vec![
        I::GlobalGet(state.global(Slot::Live)),
        I::I32Const(collector::IDLE_LIVE_SHIFT as i32),
        I::I32ShrU,
        I::LocalTee(scratch),
        I::I32Const(collector::IDLE_MIN_BYTES as i32),
        I::LocalGet(scratch),
        I::I32Const(collector::IDLE_MIN_BYTES as i32),
        I::I32GtU,
        I::Select,
        I::GlobalGet(state.global(Slot::Allocated)),
        I::I32LeU,
    ]
}

/// `[] -> [i32]`: the bytes of one bitmap covering the managed heap below the
/// limit.
fn bitmap_bytes(globals: Globals, state: State) -> Vec<I<'static>> {
    vec![
        I::GlobalGet(state.global(Slot::Limit)),
        I::I32Const(globals.base as i32),
        I::I32Sub,
        I::I32Const(SPAN_SHIFT),
        I::I32ShrU,
        I::I32Const((1 << WORD_BITS_SHIFT) - 1),
        I::I32Add,
        I::I32Const(WORD_BITS_SHIFT),
        I::I32ShrU,
        I::I32Const(WORD_BYTES_SHIFT),
        I::I32Shl,
    ]
}

/// A metadata block of at least the bytes in local `bytes` from the heap end
/// into `ptr` (zero when it does not fit), growing memory only within the
/// memory budget and never collecting.
fn bump(globals: Globals, bytes: u32, size: u32, end: u32, ptr: u32) -> Vec<I<'static>> {
    vec![
        I::I32Const(0),
        I::LocalSet(ptr),
        I::GlobalGet(globals.heap),
        I::LocalGet(bytes),
        I::I32Const((HEADER + ALIGN - 1) as i32),
        I::I32Add,
        I::I32Const(-(ALIGN as i32)),
        I::I32And,
        I::LocalTee(size),
        I::I32Add,
        I::LocalTee(end),
        I::I32Const(abi::MAX_MEMORY_BYTES as i32),
        I::I32LeU,
        I::LocalGet(end),
        I::GlobalGet(globals.heap),
        I::I32GtU,
        I::I32And,
        I::If(BlockType::Empty),
        I::LocalGet(end),
        I::MemorySize(MEMORY),
        I::I32Const(abi::WASM_PAGE_BYTES as i32),
        I::I32Mul,
        I::I32GtU,
        I::If(BlockType::Result(ValType::I32)),
        I::LocalGet(end),
        I::I32Const((abi::WASM_PAGE_BYTES - 1) as i32),
        I::I32Add,
        I::I32Const(abi::WASM_PAGE_BYTES as i32),
        I::I32DivU,
        I::MemorySize(MEMORY),
        I::I32Sub,
        I::MemoryGrow(MEMORY),
        I::I32Const(-1),
        I::I32Ne,
        I::Else,
        I::I32Const(1),
        I::End,
        I::If(BlockType::Empty),
        I::GlobalGet(globals.heap),
        I::LocalGet(size),
        I::I32Store(WORD),
        I::GlobalGet(globals.heap),
        I::I32Const(ALLOCATED),
        I::I32Store(FLAG),
        I::GlobalGet(globals.heap),
        I::I32Const(0),
        I::I32Store(LINK),
        I::GlobalGet(globals.heap),
        I::I32Const(HEADER as i32),
        I::I32Add,
        I::LocalSet(ptr),
        I::LocalGet(end),
        I::GlobalSet(globals.heap),
        I::End,
        I::End,
    ]
}

/// `CollectBegin () -> i32 started`: start a cycle at the current heap end:
/// size (or reuse) the bitmaps and the mark stack, keep metadata of earlier
/// cycles below the limit, and shade every root. Returns 0, starting
/// nothing, when the metadata does not fit the memory budget.
pub(super) fn begin(globals: Globals, calls: Calls) -> Function {
    const BITMAP_BYTES: u32 = 0;
    const STACK_BYTES: u32 = 1;
    const PTR: u32 = 2;
    const SIZE: u32 = 3;
    const END: u32 = 4;
    const CURSOR: u32 = 5;
    let mut function = Function::new([(CURSOR + 1, ValType::I32)]);
    let Some(state) = globals.collector else {
        instructions(&mut function, [I::I32Const(0), I::End]);
        return function;
    };
    let g = |slot| state.global(slot);
    let mut code = vec![
        I::GlobalGet(g(Slot::Phase)),
        I::If(BlockType::Empty),
        I::I32Const(1),
        I::Return,
        I::End,
        I::GlobalGet(globals.heap),
        I::GlobalSet(g(Slot::Limit)),
    ];
    // One fresh metadata block per cycle: both bitmaps (zeroed), then the
    // mark stack, one entry per heap bytes per entry and at least the
    // minimum. The block of the previous cycle is garbage now, freed by this
    // cycle's sweep, so no metadata outlives its cycle to split the heap.
    code.extend(bitmap_bytes(globals, state));
    code.extend([
        I::LocalSet(BITMAP_BYTES),
        I::GlobalGet(g(Slot::Limit)),
        I::I32Const(globals.base as i32),
        I::I32Sub,
        I::I32Const(collector::MARK_STACK_BYTES_PER_ENTRY as i32),
        I::I32DivU,
        I::LocalTee(STACK_BYTES),
        I::I32Const(collector::MIN_MARK_STACK_ENTRIES as i32),
        I::LocalGet(STACK_BYTES),
        I::I32Const(collector::MIN_MARK_STACK_ENTRIES as i32),
        I::I32GtU,
        I::Select,
        I::I32Const(ENTRY),
        I::I32Mul,
        I::LocalTee(STACK_BYTES),
        I::LocalGet(BITMAP_BYTES),
        I::I32Const(1),
        I::I32Shl,
        I::I32Add,
        I::LocalSet(SIZE),
    ]);
    code.extend(bump(globals, SIZE, END, CURSOR, PTR));
    code.extend([
        I::LocalGet(PTR),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::I32Const(0),
        I::Return,
        I::End,
        I::LocalGet(PTR),
        I::I32Const(0),
        I::LocalGet(BITMAP_BYTES),
        I::I32Const(1),
        I::I32Shl,
        I::MemoryFill(MEMORY),
        I::LocalGet(PTR),
        I::GlobalSet(g(Slot::Bitmap)),
        I::LocalGet(PTR),
        I::LocalGet(BITMAP_BYTES),
        I::I32Add,
        I::GlobalSet(g(Slot::Traced)),
        I::LocalGet(PTR),
        I::LocalGet(BITMAP_BYTES),
        I::I32Const(1),
        I::I32Shl,
        I::I32Add,
        I::LocalTee(PTR),
        I::GlobalSet(g(Slot::Stack)),
        I::LocalGet(PTR),
        I::GlobalSet(g(Slot::StackTop)),
        I::LocalGet(PTR),
        I::LocalGet(STACK_BYTES),
        I::I32Add,
        I::GlobalSet(g(Slot::StackEnd)),
        I::I32Const(Phase::Marking as i32),
        I::GlobalSet(g(Slot::Phase)),
        I::I32Const(1),
        I::GlobalSet(g(Slot::Marking)),
    ]);
    for slot in [Slot::Overflow, Slot::Scan, Slot::Partial] {
        code.extend([I::I32Const(0), I::GlobalSet(g(slot))]);
    }
    // The snapshot: every root slot is shaded.
    code.extend([
        I::I32Const(abi::STRING_ROOT_BASE as i32),
        I::LocalSet(CURSOR),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(CURSOR),
        I::GlobalGet(globals.roots),
        I::I32GeU,
        I::BrIf(1),
        I::LocalGet(CURSOR),
        I::I64Load(ROOT),
        I::Call(calls.shade),
        I::LocalGet(CURSOR),
        I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
        I::I32Add,
        I::LocalSet(CURSOR),
        I::Br(0),
        I::End,
        I::End,
        I::I32Const(1),
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// `CollectStep (i32 units) -> i32 status`: perform up to `units` units of
/// the cycle in progress (a traced slot, a popped, rescanned or swept block
/// each cost one) and return `collector::IDLE_NONE` without a cycle,
/// `IDLE_MORE` while it continues and `IDLE_DONE` when this step completed
/// it.
pub(super) fn step(globals: Globals, calls: Calls, mark: u32) -> Function {
    const UNITS: u32 = 0;
    const CURRENT: u32 = 1;
    const BITS: Bits = Bits { offset: 6, mask: 7 };
    /// The last local, the sweep's limit (`sweep`).
    const LIMIT: u32 = 9;
    const AGAIN: u32 = 0;
    let mut function = Function::new([(LIMIT, ValType::I32)]);
    let (Some(state), Some(classes)) = (globals.collector, globals.classes) else {
        instructions(&mut function, [I::I32Const(collector::IDLE_NONE), I::End]);
        return function;
    };
    let lists = super::free_classes::Lists::new(classes);
    let g = |slot| state.global(slot);
    let spend = |units: Vec<I<'static>>| {
        let mut code = vec![I::LocalGet(UNITS)];
        code.extend(units);
        code.extend([I::I32Sub, I::LocalSet(UNITS)]);
        code
    };
    let trace = |start: Vec<I<'static>>| {
        let mut code = start;
        code.extend([
            I::LocalGet(CURRENT),
            I::I32Const(collector::TRACE_CHUNK_SLOTS as i32),
            I::Call(calls.trace),
        ]);
        code
    };
    let mut code = vec![
        I::GlobalGet(g(Slot::Phase)),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::I32Const(collector::IDLE_NONE),
        I::Return,
        I::End,
        I::Loop(BlockType::Empty),
        I::LocalGet(UNITS),
        I::I32Const(0),
        I::I32LeS,
        I::If(BlockType::Empty),
        I::I32Const(collector::IDLE_MORE),
        I::Return,
        I::End,
        I::GlobalGet(g(Slot::Phase)),
        I::I32Const(Phase::Marking as i32),
        I::I32Eq,
        I::If(BlockType::Empty),
        // An array traced in chunks continues first.
        I::GlobalGet(g(Slot::Partial)),
        I::LocalTee(CURRENT),
        I::If(BlockType::Empty),
    ];
    code.extend(spend(trace(vec![I::GlobalGet(g(Slot::PartialNext))])));
    code.extend(spend(vec![I::I32Const(1)]));
    code.extend([
        I::Br(AGAIN + 2),
        I::End,
        // Then the gray blocks on the mark stack.
        I::GlobalGet(g(Slot::StackTop)),
        I::GlobalGet(g(Slot::Stack)),
        I::I32GtU,
        I::If(BlockType::Empty),
        I::LocalGet(UNITS),
        I::Call(mark),
        I::LocalSet(UNITS),
        I::Br(AGAIN + 2),
        I::End,
        // After an overflow, rescan the heap below the limit for marked
        // blocks not yet traced, until a whole pass overflows no more.
        I::GlobalGet(g(Slot::Overflow)),
        I::GlobalGet(g(Slot::Scan)),
        I::I32Or,
        I::If(BlockType::Empty),
        I::GlobalGet(g(Slot::Scan)),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::I32Const(globals.base as i32),
        I::GlobalSet(g(Slot::Scan)),
        I::I32Const(0),
        I::GlobalSet(g(Slot::Overflow)),
        I::End,
        I::GlobalGet(g(Slot::Scan)),
        I::GlobalGet(g(Slot::Limit)),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::I32Const(0),
        I::GlobalSet(g(Slot::Scan)),
        I::Br(AGAIN + 3),
        I::End,
        I::GlobalGet(g(Slot::Scan)),
        I::LocalTee(CURRENT),
        I::LocalGet(CURRENT),
        I::I32Load(WORD),
        I::I32Add,
        I::GlobalSet(g(Slot::Scan)),
    ]);
    code.extend(spend(vec![I::I32Const(1)]));
    code.extend([
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(ALLOCATED),
        I::I32And,
        I::If(BlockType::Empty),
    ]);
    code.extend(BITS.locate(globals, CURRENT));
    code.extend(BITS.test(g(Slot::Bitmap)));
    code.push(I::If(BlockType::Empty));
    code.extend(BITS.test(g(Slot::Traced)));
    code.extend([I::I32Eqz, I::If(BlockType::Empty)]);
    code.extend(spend(trace(vec![I::I32Const(0)])));
    code.extend([I::End, I::End, I::End, I::Br(AGAIN + 2), I::End]);
    // Nothing is gray: the marking is complete and the sweep starts. Every
    // free list is rebuilt from the blocks it passes.
    code.extend(lists.reset());
    code.extend([I::I32Const(0), I::GlobalSet(globals.free)]);
    for slot in [Slot::Run, Slot::LargeHead, Slot::LargeTail, Slot::Swept] {
        code.extend([I::I32Const(0), I::GlobalSet(g(slot))]);
    }
    code.extend([
        I::I32Const(globals.base as i32),
        I::GlobalSet(g(Slot::Cursor)),
        I::I32Const(Phase::Sweeping as i32),
        I::GlobalSet(g(Slot::Phase)),
        I::I32Const(0),
        I::GlobalSet(g(Slot::Marking)),
        I::Br(AGAIN + 1),
        I::End,
    ]);
    code.extend(sweep(globals, state, lists, calls));
    code.extend([I::End, I::Unreachable, I::End]);
    instructions(&mut function, code);
    function
}

/// The lazy sweep inside `step` (its locals): one block per unit, with the
/// cursor and the limit in locals; the bitmaps are cleared when it finishes.
fn sweep(
    globals: Globals,
    state: State,
    lists: super::free_classes::Lists,
    calls: Calls,
) -> Vec<I<'static>> {
    const UNITS: u32 = 0;
    const CURRENT: u32 = 1;
    const SIZE: u32 = 2;
    const RUN: u32 = 3;
    const RUN_SIZE: u32 = 4;
    const CLASS: u32 = 5;
    const BITS: Bits = Bits { offset: 6, mask: 7 };
    const CURSOR: u32 = 8;
    const LIMIT: u32 = 9;
    let g = |slot| state.global(slot);
    let mut code = vec![
        I::GlobalGet(g(Slot::Cursor)),
        I::LocalSet(CURSOR),
        I::GlobalGet(g(Slot::Limit)),
        I::LocalSet(LIMIT),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(CURSOR),
        I::LocalGet(LIMIT),
        I::I32GeU,
        I::BrIf(1),
        I::LocalGet(UNITS),
        I::I32Const(0),
        I::I32LeS,
        I::If(BlockType::Empty),
        I::LocalGet(CURSOR),
        I::GlobalSet(g(Slot::Cursor)),
        I::I32Const(collector::IDLE_MORE),
        I::Return,
        I::End,
        I::LocalGet(CURSOR),
        I::LocalTee(CURRENT),
        I::I32Load(WORD),
        I::LocalTee(SIZE),
        I::LocalGet(CURRENT),
        I::I32Add,
        I::LocalSet(CURSOR),
        I::LocalGet(UNITS),
        I::I32Const(1),
        I::I32Sub,
        I::LocalSet(UNITS),
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(ALLOCATED),
        I::I32And,
        I::If(BlockType::Result(ValType::I32)),
    ];
    code.extend(BITS.locate(globals, CURRENT));
    code.extend(BITS.test(g(Slot::Bitmap)));
    code.extend([I::Else, I::I32Const(0), I::End, I::If(BlockType::Empty)]);
    // Live: count it and close the dead run before it.
    code.extend([
        I::GlobalGet(g(Slot::Swept)),
        I::LocalGet(SIZE),
        I::I32Add,
        I::GlobalSet(g(Slot::Swept)),
    ]);
    code.extend(close_run(state, lists, RUN, RUN_SIZE, CLASS));
    code.push(I::Else);
    // Dead or free: it joins the open run.
    if state.stress.is_some() {
        code.extend([
            I::LocalGet(CURRENT),
            I::I32Load(FLAG),
            I::If(BlockType::Empty),
            I::LocalGet(CURRENT),
            I::I32Const(HEADER as i32),
            I::I32Add,
            I::I32Const(POISON),
            I::LocalGet(SIZE),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::MemoryFill(MEMORY),
            I::End,
        ]);
    }
    code.extend([
        I::LocalGet(CURRENT),
        I::I32Const(0),
        I::I32Store(FLAG),
        I::GlobalGet(g(Slot::Run)),
        I::LocalTee(RUN),
        I::If(BlockType::Empty),
        I::LocalGet(RUN),
        I::LocalGet(RUN),
        I::I32Load(WORD),
        I::LocalGet(SIZE),
        I::I32Add,
        I::I32Store(WORD),
        I::Else,
        I::LocalGet(CURRENT),
        I::GlobalSet(g(Slot::Run)),
        I::End,
        I::End,
        I::Br(0),
        I::End,
        I::End,
        I::LocalGet(CURSOR),
        I::GlobalSet(g(Slot::Cursor)),
    ]);
    code.extend(finish(globals, state, lists));
    if state.stress.is_some() {
        // A stress build verifies every cycle with a complete collection,
        // whose exact marking traps on any reachable block the cycle freed.
        code.push(I::Call(calls.collect));
        code.extend(super::allocate::collection_threshold(globals));
    }
    code.extend([I::I32Const(collector::IDLE_DONE), I::Return]);
    code
}

/// Close the sweep's open dead run: its exact-size list, or the pending list
/// of larger runs.
fn close_run(
    state: State,
    lists: super::free_classes::Lists,
    run: u32,
    run_size: u32,
    class: u32,
) -> Vec<I<'static>> {
    let g = |slot| state.global(slot);
    let mut code = vec![
        I::GlobalGet(g(Slot::Run)),
        I::LocalTee(run),
        I::If(BlockType::Empty),
        I::LocalGet(run),
        I::I32Load(WORD),
        I::LocalTee(run_size),
        I::I32Const(super::free_classes::MAX_CLASS_BYTES),
        I::I32LeU,
        I::If(BlockType::Empty),
    ];
    code.extend(lists.push(run, run_size, class));
    code.extend([
        I::Else,
        I::LocalGet(run),
        I::I32Const(0),
        I::I32Store(LINK),
        I::GlobalGet(g(Slot::LargeTail)),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(run),
        I::GlobalSet(g(Slot::LargeHead)),
        I::Else,
        I::GlobalGet(g(Slot::LargeTail)),
        I::LocalGet(run),
        I::I32Store(LINK),
        I::End,
        I::LocalGet(run),
        I::GlobalSet(g(Slot::LargeTail)),
        I::End,
        I::I32Const(0),
        I::GlobalSet(g(Slot::Run)),
        I::End,
    ]);
    code
}

/// The end of the sweep: a dead run ending at the heap end returns to it,
/// the larger free runs are published in address order, the live bytes are
/// the new estimate and the next in-turn cycle starts at twice the heap.
fn finish(globals: Globals, state: State, lists: super::free_classes::Lists) -> Vec<I<'static>> {
    const RUN: u32 = 3;
    const RUN_SIZE: u32 = 4;
    const CLASS: u32 = 5;
    let g = |slot| state.global(slot);
    let mut code = vec![
        I::GlobalGet(g(Slot::Run)),
        I::LocalTee(RUN),
        I::If(BlockType::Empty),
        I::LocalGet(RUN),
        I::LocalGet(RUN),
        I::I32Load(WORD),
        I::I32Add,
        I::GlobalGet(globals.heap),
        I::I32Eq,
        I::If(BlockType::Empty),
        I::LocalGet(RUN),
        I::GlobalSet(globals.heap),
        I::I32Const(0),
        I::GlobalSet(g(Slot::Run)),
        I::End,
        I::End,
    ];
    code.extend(close_run(state, lists, RUN, RUN_SIZE, CLASS));
    code.extend([
        I::GlobalGet(g(Slot::LargeHead)),
        I::GlobalSet(globals.free),
        I::I32Const(Phase::Idle as i32),
        I::GlobalSet(g(Slot::Phase)),
        // Live: what the sweep kept plus what the cycle allocated.
        I::GlobalGet(g(Slot::Swept)),
        I::GlobalGet(globals.heap),
        I::GlobalGet(g(Slot::Limit)),
        I::I32Sub,
        I::I32Add,
        I::GlobalSet(g(Slot::Live)),
        I::I32Const(0),
        I::GlobalSet(g(Slot::Allocated)),
    ]);
    code.extend(super::allocate::threshold(globals));
    code
}

/// The idle export `(i32 units) -> i32 status`: start a cycle when the
/// policy finds one due, then perform up to `units` units of it.
pub(super) fn idle(globals: Globals, calls: Calls) -> Function {
    const UNITS: u32 = 0;
    const SCRATCH: u32 = 1;
    let mut function = Function::new([(1, ValType::I32)]);
    let Some(state) = globals.collector else {
        instructions(&mut function, [I::I32Const(collector::IDLE_NONE), I::End]);
        return function;
    };
    let mut code = vec![
        I::GlobalGet(state.global(Slot::Phase)),
        I::I32Eqz,
        I::If(BlockType::Empty),
    ];
    code.extend(due(state, SCRATCH));
    code.extend([
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::I32Const(collector::IDLE_NONE),
        I::Return,
        I::End,
        I::Call(calls.begin),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::I32Const(collector::IDLE_NONE),
        I::Return,
        I::End,
        I::End,
        I::LocalGet(UNITS),
        I::Call(calls.step),
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

thread_local! {
    static STRESS: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}
/// The work units per allocation when modules compiled on this thread are
/// collector stress builds.
pub(in crate::emit) fn stress() -> Option<u32> {
    STRESS.with(std::cell::Cell::get)
}
/// Test-only: compile inside `body` as collector stress builds: a cycle
/// starts at every allocation without one, every allocation performs
/// `units` units, the sweep poisons the payload of every block it frees, and
/// a complete collection verifies every finished cycle.
pub(crate) fn with_stress<T>(units: u32, body: impl FnOnce() -> T) -> T {
    let previous = STRESS.with(|cell| cell.replace(Some(units.max(1))));
    let result = body();
    STRESS.with(|cell| cell.set(previous));
    result
}
