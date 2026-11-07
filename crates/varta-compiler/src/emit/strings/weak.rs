//! V38 (`weak_references`, K9 D5): ephemeron tables in the managed heap and
//! the collector's weak phase.
//!
//! A table (`varta_common::weak`) is a block with `HEAP_WEAK_FLAG` and no
//! edge flag: ordinary tracing marks it without reading its pairs. Every
//! table is linked from the module's weak-list global through its header
//! link word when it is created. Its key slots hold identity addresses, its
//! value slots ordinary value slots (references only in a table of kind
//! `KIND_VALUES`, which the table helpers enforce).
//!
//! Once marking has traced every strong edge (an empty worklist in a
//! complete collection, nothing gray in an incremental cycle), the weak
//! phase runs without any guest code in between:
//!
//! - **Ephemerons.** For every marked table, every occupied pair whose key
//!   is marked (a static key always is) has its value marked and pushed,
//!   and every key of a `KIND_STRONG` table is; while a pass marks anything,
//!   marking resumes and the pass repeats, so the result is the least fixed
//!   point. Each pass costs fuel in proportion to the tables' pairs; a
//!   chain of `n` ephemerons whose keys are only reachable through each
//!   other's values takes up to `n` passes, all within the fuel budget of
//!   the turn (or of the idle period).
//! - **Vacating.** Every occupied pair of a marked table (other than the
//!   kept objects) whose key stayed unmarked is vacated and counted; in a
//!   `KIND_NOTIFY` table it raises the pending flag. Every unmarked table
//!   leaves the weak list. Only then does the sweep free any block, so no
//!   table ever holds the address of a freed or reused block.
use super::collect::BLOCK;
use super::incremental::{Bits, Slot, State};
use super::*;
use varta_common::weak as w;

/// The weak list head and the pending-finalization flag (i32 globals).
#[derive(Clone, Copy, Debug)]
pub(in crate::emit) struct WeakGlobals {
    pub head: u32,
    pub pending: u32,
}

/// A payload word of a table at the payload address on the stack.
pub(in crate::emit) const fn word(offset: u32) -> MemArg {
    MemArg {
        offset: offset as u64,
        ..WORD
    }
}
/// A payload word of a table at its block header address on the stack.
const fn header_word(offset: u32) -> MemArg {
    word(HEADER + offset)
}
/// A pair's key and value slot at the pair address on the stack.
pub(in crate::emit) const KEY: MemArg = ROOT;
pub(in crate::emit) const VALUE: MemArg = MemArg {
    offset: w::VALUE_OFFSET as u64,
    ..ROOT
};
/// Low word of a pair's value slot.
const VALUE_WORD: MemArg = MemArg {
    offset: w::VALUE_OFFSET as u64,
    ..WORD
};
const PAIR_SHIFT: i32 = w::PAIR_BYTES.trailing_zeros() as i32;
/// The multiplier of the identity hash (Fibonacci hashing, `2^32 / φ`).
const GOLDEN: u32 = 0x9E37_79B1;
/// Identity addresses are aligned; their low bits carry no information.
const ALIGN_SHIFT: i32 = varta_common::value::HEAP_ALIGNMENT.trailing_zeros() as i32;
/// The upper half of a hash word folded into its lower half.
const FOLD_SHIFT: i32 = (u32::BITS / 2) as i32;
const WEAK_BLOCK: i32 = ALLOCATED | w::HEAP_WEAK_FLAG;

/// `[i32 identity] -> [f64 hash]`: the identity hash of a key, with
/// `scratch` an i32 local.
pub(in crate::emit) fn hash(scratch: u32) -> Vec<I<'static>> {
    vec![
        I::I32Const(ALIGN_SHIFT),
        I::I32ShrU,
        I::I32Const(GOLDEN as i32),
        I::I32Mul,
        I::LocalTee(scratch),
        I::LocalGet(scratch),
        I::I32Const(FOLD_SHIFT),
        I::I32ShrU,
        I::I32Xor,
        I::F64ConvertI32U,
    ]
}

/// `[] -> [i32 pair]`: the address of the pair at the position in f64 local
/// `position` of the table in i64 local `table`, leaving its payload in
/// `ptr` and the position in `at`; a position that is not an integer below
/// the capacity traps.
fn pair(table: u32, position: u32, ptr: u32, at: u32) -> Vec<I<'static>> {
    vec![
        I::LocalGet(table),
        I::I32WrapI64,
        I::LocalSet(ptr),
        I::LocalGet(position),
        I::I32TruncSatF64U,
        I::LocalTee(at),
        I::F64ConvertI32U,
        I::LocalGet(position),
        I::F64Ne,
        I::LocalGet(at),
        I::LocalGet(ptr),
        I::I32Load(word(w::CAPACITY_WORD)),
        I::I32GeU,
        I::I32Or,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(ptr),
        I::LocalGet(at),
        I::I32Const(PAIR_SHIFT),
        I::I32Shl,
        I::I32Add,
        I::I32Const(w::PAIRS_OFFSET as i32),
        I::I32Add,
    ]
}

/// `WeakTable (i32 kind, f64 capacity) -> i64 table`: a fresh table of
/// `capacity` empty pairs (a power of two up to the profile's element
/// bound), linked into the weak list.
pub(super) fn table(globals: Globals, allocate: u32, max_pairs: usize) -> Function {
    const KIND: u32 = 0;
    const CAPACITY: u32 = 1;
    const PAIRS: u32 = 2;
    const BYTES: u32 = 3;
    const PTR: u32 = 4;
    const BLOCK: u32 = 5;
    let mut function = Function::new([(BLOCK - CAPACITY, ValType::I32)]);
    let Some(weak) = globals.weak else {
        instructions(&mut function, [I::Unreachable, I::End]);
        return function;
    };
    instructions(
        &mut function,
        [
            I::LocalGet(CAPACITY),
            I::I32TruncSatF64U,
            I::LocalTee(PAIRS),
            I::F64ConvertI32U,
            I::LocalGet(CAPACITY),
            I::F64Ne,
            I::LocalGet(PAIRS),
            I::I32Eqz,
            I::I32Or,
            I::LocalGet(PAIRS),
            I::LocalGet(PAIRS),
            I::I32Const(1),
            I::I32Sub,
            I::I32And,
            I::I32Or,
            I::LocalGet(PAIRS),
            I::I32Const(max_pairs as i32),
            I::I32GtU,
            I::I32Or,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(PAIRS),
            I::I32Const(PAIR_SHIFT),
            I::I32Shl,
            I::I32Const(w::PAIRS_OFFSET as i32),
            I::I32Add,
            I::LocalTee(BYTES),
            I::Call(allocate),
            I::LocalTee(PTR),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::LocalTee(BLOCK),
            I::I32Const(WEAK_BLOCK),
            I::I32Store(FLAG),
            I::LocalGet(BLOCK),
            I::GlobalGet(weak.head),
            I::I32Store(LINK),
            I::LocalGet(BLOCK),
            I::GlobalSet(weak.head),
            I::LocalGet(PTR),
            I::I32Const(0),
            I::LocalGet(BYTES),
            I::MemoryFill(MEMORY),
            I::LocalGet(PTR),
            I::LocalGet(PAIRS),
            I::I32Store(word(w::CAPACITY_WORD)),
            I::LocalGet(PTR),
            I::LocalGet(KIND),
            I::I32Store(word(w::KIND_WORD)),
            I::LocalGet(PTR),
            I::I64ExtendI32U,
            I::End,
        ],
    );
    function
}

/// `WeakPair (i64 table, f64 position) -> i32 pair`.
pub(super) fn pair_helper() -> Function {
    const TABLE: u32 = 0;
    const POSITION: u32 = 1;
    const PTR: u32 = 2;
    const AT: u32 = 3;
    let mut function = Function::new([(2, ValType::I32)]);
    let mut code = pair(TABLE, POSITION, PTR, AT);
    code.push(I::End);
    instructions(&mut function, code);
    function
}

/// `WeakPut (i64 table, f64 position, i32 identity, i64 bits, i32
/// reference) -> ()`: occupy an empty or vacated pair with a key and value,
/// or replace the value of the pair holding the key. `reference` states
/// whether the value is a reference; it must match the table's kind. The
/// old value is shaded while a cycle marks.
pub(super) fn put(barrier: Option<incremental::Barrier>) -> Function {
    const TABLE: u32 = 0;
    const POSITION: u32 = 1;
    const IDENTITY: u32 = 2;
    const BITS: u32 = 3;
    const REFERENCE: u32 = 4;
    const PTR: u32 = 5;
    const AT: u32 = 6;
    const PAIR: u32 = 7;
    const OLD: u32 = 8;
    let mut function = Function::new([(3, ValType::I32), (1, ValType::I64)]);
    let mut code = pair(TABLE, POSITION, PTR, AT);
    code.extend([
        I::LocalSet(PAIR),
        I::LocalGet(PTR),
        I::I32Load(word(w::KIND_WORD)),
        I::I32Const(w::KIND_VALUES as i32),
        I::I32And,
        I::I32Eqz,
        I::I32Eqz,
        I::LocalGet(REFERENCE),
        I::I32Ne,
        I::LocalGet(IDENTITY),
        I::I32Const(w::KEY_VACATED as i32),
        I::I32LeU,
        I::I32Or,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(PAIR),
        I::I64Load(KEY),
        I::LocalTee(OLD),
        I::I64Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(PTR),
        I::LocalGet(PTR),
        I::I32Load(word(w::USED_WORD)),
        I::I32Const(1),
        I::I32Add,
        I::I32Store(word(w::USED_WORD)),
        I::Else,
        // A vacated pair, or the key's own pair.
        I::LocalGet(OLD),
        I::I64Const(w::KEY_VACATED as i64),
        I::I64Ne,
        I::LocalGet(OLD),
        I::LocalGet(IDENTITY),
        I::I64ExtendI32U,
        I::I64Ne,
        I::I32And,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::End,
    ]);
    if let Some(barrier) = barrier {
        code.extend([I::LocalGet(REFERENCE), I::If(BlockType::Empty)]);
        code.extend(barrier.slot(PAIR, VALUE));
        code.push(I::End);
    }
    code.extend([
        I::LocalGet(PAIR),
        I::LocalGet(IDENTITY),
        I::I64ExtendI32U,
        I::I64Store(KEY),
        I::LocalGet(PAIR),
        I::LocalGet(BITS),
        I::I64Store(VALUE),
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// `WeakRemove (i64 table, f64 position) -> ()`: vacate an occupied pair,
/// shading its value (and a kept key) while a cycle marks.
pub(super) fn remove(barrier: Option<incremental::Barrier>) -> Function {
    const TABLE: u32 = 0;
    const POSITION: u32 = 1;
    const PTR: u32 = 2;
    const AT: u32 = 3;
    const PAIR: u32 = 4;
    const KIND: u32 = 5;
    let mut function = Function::new([(4, ValType::I32)]);
    let mut code = pair(TABLE, POSITION, PTR, AT);
    code.extend([
        I::LocalTee(PAIR),
        I::I64Load(KEY),
        I::I64Const(w::KEY_VACATED as i64),
        I::I64LeU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(PTR),
        I::I32Load(word(w::KIND_WORD)),
        I::LocalSet(KIND),
    ]);
    code.extend(shade_pair(barrier, PAIR, KIND));
    code.extend([
        I::LocalGet(PAIR),
        I::I64Const(w::KEY_VACATED as i64),
        I::I64Store(KEY),
        I::LocalGet(PAIR),
        I::I64Const(0),
        I::I64Store(VALUE),
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// While a cycle marks, shade the references an occupied pair (address in
/// local `pair`) of a table of kind `kind` holds strongly: its value in a
/// `KIND_VALUES` table, its key in a `KIND_STRONG` one.
fn shade_pair(barrier: Option<incremental::Barrier>, pair: u32, kind: u32) -> Vec<I<'static>> {
    let Some(barrier) = barrier else {
        return Vec::new();
    };
    let mut code = vec![
        I::GlobalGet(barrier.marking),
        I::If(BlockType::Empty),
        I::LocalGet(kind),
        I::I32Const(w::KIND_VALUES as i32),
        I::I32And,
        I::If(BlockType::Empty),
    ];
    code.extend([I::LocalGet(pair), I::I64Load(VALUE), I::Call(barrier.shade)]);
    code.extend([
        I::End,
        I::LocalGet(kind),
        I::I32Const(w::KIND_STRONG as i32),
        I::I32And,
        I::If(BlockType::Empty),
        I::LocalGet(pair),
        I::I64Load(KEY),
        I::Call(barrier.shade),
        I::End,
        I::End,
    ]);
    code
}

/// `WeakMove (i64 from, f64 position, i64 to, f64 position) -> ()`: copy an
/// occupied pair into an empty pair of a table of the same kind. The
/// source pair stays, so nothing is deleted and no barrier is needed.
pub(super) fn moved() -> Function {
    const FROM: u32 = 0;
    const SOURCE_POSITION: u32 = 1;
    const TO: u32 = 2;
    const TARGET_POSITION: u32 = 3;
    const PTR: u32 = 4;
    const AT: u32 = 5;
    const SOURCE: u32 = 6;
    const TARGET_PTR: u32 = 7;
    const TARGET: u32 = 8;
    let mut function = Function::new([(5, ValType::I32)]);
    let mut code = pair(FROM, SOURCE_POSITION, PTR, AT);
    code.push(I::LocalSet(SOURCE));
    code.extend(pair(TO, TARGET_POSITION, TARGET_PTR, AT));
    code.extend([
        I::LocalSet(TARGET),
        I::LocalGet(PTR),
        I::I32Load(word(w::KIND_WORD)),
        I::LocalGet(TARGET_PTR),
        I::I32Load(word(w::KIND_WORD)),
        I::I32Ne,
        I::LocalGet(SOURCE),
        I::I64Load(KEY),
        I::I64Const(w::KEY_VACATED as i64),
        I::I64LeU,
        I::I32Or,
        I::LocalGet(TARGET),
        I::I64Load(KEY),
        I::I64Const(w::KEY_EMPTY as i64),
        I::I64Ne,
        I::I32Or,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(TARGET),
        I::LocalGet(SOURCE),
        I::I64Load(KEY),
        I::I64Store(KEY),
        I::LocalGet(TARGET),
        I::LocalGet(SOURCE),
        I::I64Load(VALUE),
        I::I64Store(VALUE),
        I::LocalGet(TARGET_PTR),
        I::LocalGet(TARGET_PTR),
        I::I32Load(word(w::USED_WORD)),
        I::I32Const(1),
        I::I32Add,
        I::I32Store(word(w::USED_WORD)),
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// `WeakClear (i64 table) -> ()`: empty every pair, shading what occupied
/// pairs held strongly while a cycle marks.
pub(super) fn clear(barrier: Option<incremental::Barrier>) -> Function {
    const TABLE: u32 = 0;
    const PTR: u32 = 1;
    const PAIR: u32 = 2;
    const BYTES: u32 = 3;
    const KIND: u32 = 4;
    let mut function = Function::new([(4, ValType::I32)]);
    let mut code = vec![
        I::LocalGet(TABLE),
        I::I32WrapI64,
        I::LocalTee(PTR),
        I::I32Load(word(w::KIND_WORD)),
        I::LocalSet(KIND),
        I::LocalGet(PTR),
        I::I32Load(word(w::CAPACITY_WORD)),
        I::I32Const(PAIR_SHIFT),
        I::I32Shl,
        I::LocalSet(BYTES),
    ];
    if let Some(barrier) = barrier {
        code.extend([
            I::GlobalGet(barrier.marking),
            I::If(BlockType::Empty),
            I::LocalGet(PTR),
            I::I32Const(w::PAIRS_OFFSET as i32),
            I::I32Add,
            I::LocalSet(PAIR),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(PAIR),
            I::LocalGet(PTR),
            I::I32Const(w::PAIRS_OFFSET as i32),
            I::I32Add,
            I::LocalGet(BYTES),
            I::I32Add,
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(PAIR),
            I::I64Load(KEY),
            I::I64Const(w::KEY_VACATED as i64),
            I::I64GtU,
            I::If(BlockType::Empty),
        ]);
        code.extend(shade_pair(Some(barrier), PAIR, KIND));
        code.extend([
            I::End,
            I::LocalGet(PAIR),
            I::I32Const(w::PAIR_BYTES as i32),
            I::I32Add,
            I::LocalSet(PAIR),
            I::Br(0),
            I::End,
            I::End,
            I::End,
        ]);
    }
    code.extend([
        I::LocalGet(PTR),
        I::I32Const(w::PAIRS_OFFSET as i32),
        I::I32Add,
        I::I32Const(0),
        I::LocalGet(BYTES),
        I::MemoryFill(MEMORY),
        I::LocalGet(PTR),
        I::I32Const(0),
        I::I32Store(word(w::USED_WORD)),
        I::End,
    ]);
    instructions(&mut function, code);
    function
}

/// `WeakGc () -> ()`: the test-only `gc()`, one complete collection.
pub(super) fn gc(globals: Globals, collect: u32) -> Function {
    let mut function = Function::new([]);
    let mut code = vec![I::Call(collect)];
    code.extend(super::allocate::collection_threshold(globals));
    code.push(I::End);
    instructions(&mut function, code);
    function
}

/// The locals of a weak phase (all i32).
#[derive(Clone, Copy)]
pub(super) struct Locals {
    /// The current table's header, the next one and the previous kept one.
    pub table: u32,
    pub next: u32,
    pub previous: u32,
    /// The current pair and the end of the table's pairs.
    pub pair: u32,
    pub end: u32,
    pub kind: u32,
    /// A key or value address, and the header of the block it names.
    pub key: u32,
    pub block: u32,
    /// Whether the ephemeron pass marked anything.
    pub new: u32,
}

/// How a collector's weak phase reads and sets marks.
trait Marks {
    /// `[] -> [i32]`: nonzero when the block whose header is in local
    /// `block` is marked.
    fn marked(&self, block: u32) -> Vec<I<'static>>;
    /// Mark the unmarked block whose header is in local `block` and push it
    /// for tracing.
    fn mark(&self, block: u32) -> Vec<I<'static>>;
}

/// The marks of a complete collection: the header's mark flag and the
/// worklist of `collect`.
struct Atomic;
impl Marks for Atomic {
    fn marked(&self, block: u32) -> Vec<I<'static>> {
        vec![
            I::LocalGet(block),
            I::I32Load(FLAG),
            I::I32Const(MARKED),
            I::I32And,
        ]
    }
    fn mark(&self, block: u32) -> Vec<I<'static>> {
        let mut code = vec![
            I::LocalGet(block),
            I::LocalGet(block),
            I::I32Load(FLAG),
            I::I32Const(MARKED),
            I::I32Or,
            I::I32Store(FLAG),
        ];
        if block != BLOCK {
            code.extend([I::LocalGet(block), I::LocalSet(BLOCK)]);
        }
        code.extend(super::collect::push());
        code
    }
}

/// The marks of an incremental cycle: blocks at or above the limit are
/// black, the others carry a bit of the mark bitmap; a newly marked block
/// with edges goes onto the mark stack.
struct Incremental {
    state: State,
    bits: Bits,
    globals: Globals,
    edges: i32,
}
impl Marks for Incremental {
    fn marked(&self, block: u32) -> Vec<I<'static>> {
        let mut code = vec![
            I::LocalGet(block),
            I::GlobalGet(self.state.global(Slot::Limit)),
            I::I32GeU,
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(1),
            I::Else,
        ];
        code.extend(self.bits.locate(self.globals, block));
        code.extend(self.bits.test(self.state.global(Slot::Bitmap)));
        code.push(I::End);
        code
    }
    fn mark(&self, block: u32) -> Vec<I<'static>> {
        // `marked` located the bits of `block` below the limit.
        let mut code = self.bits.set(self.state.global(Slot::Bitmap));
        code.extend([
            I::LocalGet(block),
            I::I32Load(FLAG),
            I::I32Const(self.edges),
            I::I32And,
            I::If(BlockType::Empty),
        ]);
        code.extend(super::incremental::push(self.state, block));
        code.push(I::End);
        code
    }
}

/// Mark and push the block of the address in local `l.key` unless it is
/// static or marked; sets `l.new` when it marks.
fn visit(globals: Globals, marks: &dyn Marks, l: Locals) -> Vec<I<'static>> {
    let mut code = vec![
        I::LocalGet(l.key),
        I::I32Const((globals.base + HEADER) as i32),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::LocalGet(l.key),
        I::GlobalGet(globals.heap),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(l.key),
        I::I32Const(HEADER as i32),
        I::I32Sub,
        I::LocalTee(l.block),
        I::I32Load(FLAG),
        I::I32Const(ALLOCATED),
        I::I32And,
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
    ];
    code.extend(marks.marked(l.block));
    code.extend([I::I32Eqz, I::If(BlockType::Empty)]);
    code.extend(marks.mark(l.block));
    code.extend([I::I32Const(1), I::LocalSet(l.new), I::End, I::End]);
    code
}

/// `[] -> [i32]`: nonzero when the key address in local `l.key` names a
/// static block or a marked one.
fn alive(globals: Globals, marks: &dyn Marks, l: Locals) -> Vec<I<'static>> {
    let mut code = vec![
        I::LocalGet(l.key),
        I::I32Const((globals.base + HEADER) as i32),
        I::I32LtU,
        I::If(BlockType::Result(ValType::I32)),
        I::I32Const(1),
        I::Else,
        I::LocalGet(l.key),
        I::I32Const(HEADER as i32),
        I::I32Sub,
        I::LocalSet(l.block),
    ];
    code.extend(marks.marked(l.block));
    code.push(I::End);
    code
}

/// Set `l.pair` and `l.end` to the pairs of the table in `l.table`.
fn pairs(l: Locals) -> Vec<I<'static>> {
    vec![
        I::LocalGet(l.table),
        I::I32Const((HEADER + w::PAIRS_OFFSET) as i32),
        I::I32Add,
        I::LocalTee(l.pair),
        I::LocalGet(l.table),
        I::I32Load(header_word(w::CAPACITY_WORD)),
        I::I32Const(PAIR_SHIFT),
        I::I32Shl,
        I::I32Add,
        I::LocalSet(l.end),
    ]
}

/// Run `body` for every pair of the table in `l.table` whose key slot is
/// occupied, with the key's address in `l.key`.
fn each_occupied(l: Locals, body: Vec<I<'static>>) -> Vec<I<'static>> {
    let mut code = pairs(l);
    code.extend([
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(l.pair),
        I::LocalGet(l.end),
        I::I32GeU,
        I::BrIf(1),
        I::LocalGet(l.pair),
        I::I32Load(KEY_WORD),
        I::LocalTee(l.key),
        I::I32Const(w::KEY_VACATED as i32),
        I::I32GtU,
        I::If(BlockType::Empty),
    ]);
    code.extend(body);
    code.extend([
        I::End,
        I::LocalGet(l.pair),
        I::I32Const(w::PAIR_BYTES as i32),
        I::I32Add,
        I::LocalSet(l.pair),
        I::Br(0),
        I::End,
        I::End,
    ]);
    code
}
/// Low word of a pair's key slot.
const KEY_WORD: MemArg = WORD;

/// The ephemeron pass: mark the value of every occupied pair of a marked
/// table whose key is alive, and every key of a kept-objects table.
fn ephemerons(
    globals: Globals,
    weak: WeakGlobals,
    marks: &dyn Marks,
    l: Locals,
) -> Vec<I<'static>> {
    let mut pair_body = vec![
        I::LocalGet(l.kind),
        I::I32Const(w::KIND_STRONG as i32),
        I::I32And,
        I::If(BlockType::Empty),
    ];
    pair_body.extend(visit(globals, marks, l));
    pair_body.push(I::Else);
    pair_body.extend(alive(globals, marks, l));
    pair_body.extend([
        I::If(BlockType::Empty),
        I::LocalGet(l.pair),
        I::I32Load(VALUE_WORD),
        I::LocalSet(l.key),
    ]);
    pair_body.extend(visit(globals, marks, l));
    pair_body.extend([I::End, I::End]);
    let mut code = vec![
        I::I32Const(0),
        I::LocalSet(l.new),
        I::GlobalGet(weak.head),
        I::LocalSet(l.table),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(l.table),
        I::I32Eqz,
        I::BrIf(1),
    ];
    code.extend(marks.marked(l.table));
    code.extend([
        I::If(BlockType::Empty),
        I::LocalGet(l.table),
        I::I32Load(header_word(w::KIND_WORD)),
        I::LocalTee(l.kind),
        I::I32Const((w::KIND_VALUES | w::KIND_STRONG) as i32),
        I::I32And,
        I::If(BlockType::Empty),
    ]);
    code.extend(each_occupied(l, pair_body));
    code.extend([
        I::End,
        I::End,
        I::LocalGet(l.table),
        I::I32Load(LINK),
        I::LocalSet(l.table),
        I::Br(0),
        I::End,
        I::End,
    ]);
    code
}

/// The vacating pass: vacate every occupied pair of a marked table (other
/// than the kept objects) whose key died, raising the pending flag for a
/// notifying table, and unlink every unmarked table.
fn vacate(globals: Globals, weak: WeakGlobals, marks: &dyn Marks, l: Locals) -> Vec<I<'static>> {
    let mut pair_body = alive(globals, marks, l);
    pair_body.extend([
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(l.pair),
        I::I64Const(w::KEY_VACATED as i64),
        I::I64Store(KEY),
        I::LocalGet(l.pair),
        I::I64Const(0),
        I::I64Store(VALUE),
        I::LocalGet(l.table),
        I::LocalGet(l.table),
        I::I32Load(header_word(w::CLEARED_WORD)),
        I::I32Const(1),
        I::I32Add,
        I::I32Store(header_word(w::CLEARED_WORD)),
        I::LocalGet(l.kind),
        I::I32Const(w::KIND_NOTIFY as i32),
        I::I32And,
        I::If(BlockType::Empty),
        I::I32Const(1),
        I::GlobalSet(weak.pending),
        I::End,
        I::End,
    ]);
    let mut code = vec![
        I::I32Const(0),
        I::LocalSet(l.previous),
        I::GlobalGet(weak.head),
        I::LocalSet(l.table),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(l.table),
        I::I32Eqz,
        I::BrIf(1),
        I::LocalGet(l.table),
        I::I32Load(LINK),
        I::LocalSet(l.next),
        // Only live tables are on the list.
        I::LocalGet(l.table),
        I::I32Load(FLAG),
        I::I32Const(WEAK_BLOCK),
        I::I32And,
        I::I32Const(WEAK_BLOCK),
        I::I32Ne,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
    ];
    code.extend(marks.marked(l.table));
    code.extend([
        I::If(BlockType::Empty),
        I::LocalGet(l.table),
        I::I32Load(header_word(w::KIND_WORD)),
        I::LocalTee(l.kind),
        I::I32Const(w::KIND_STRONG as i32),
        I::I32And,
        I::I32Eqz,
        I::If(BlockType::Empty),
    ]);
    code.extend(each_occupied(l, pair_body));
    code.extend([
        I::End,
        I::LocalGet(l.table),
        I::LocalSet(l.previous),
        I::Else,
        I::LocalGet(l.previous),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(l.next),
        I::GlobalSet(weak.head),
        I::Else,
        I::LocalGet(l.previous),
        I::LocalGet(l.next),
        I::I32Store(LINK),
        I::End,
        I::End,
        I::LocalGet(l.next),
        I::LocalSet(l.table),
        I::Br(0),
        I::End,
        I::End,
    ]);
    code
}

/// The weak phase of a complete collection, after the worklist and the
/// fallback scans traced every strong edge: the ephemeron pass marks and
/// pushes; when it marked anything, the worklist drains (`drain`) and the
/// caller's loop (depth `again` from the end of this code) repeats the
/// fallback scans if the worklist overflowed and the pass; then the
/// vacating pass.
pub(super) fn atomic(
    globals: Globals,
    weak: WeakGlobals,
    l: Locals,
    drain: Vec<I<'static>>,
) -> (Vec<I<'static>>, Vec<I<'static>>) {
    let mut ephemeron = ephemerons(globals, weak, &Atomic, l);
    ephemeron.extend([I::LocalGet(l.new), I::If(BlockType::Empty)]);
    ephemeron.extend(drain);
    // The `if` and then the caller's loop.
    ephemeron.extend([I::Br(1), I::End]);
    (ephemeron, vacate(globals, weak, &Atomic, l))
}

/// `CollectWeak () -> i32`: the weak phase of an incremental cycle whose
/// marking found nothing gray. Returns 1 when the ephemeron pass marked
/// anything (marking resumes, and the phase runs again once nothing is
/// gray), else vacates and returns 0.
pub(super) fn incremental(globals: Globals, edges: i32) -> Function {
    const L: Locals = Locals {
        table: 0,
        next: 1,
        previous: 2,
        pair: 3,
        end: 4,
        kind: 5,
        key: 6,
        block: 7,
        new: 8,
    };
    const BITS: Bits = Bits {
        offset: 9,
        mask: 10,
    };
    let mut function = Function::new([(BITS.mask + 1, ValType::I32)]);
    let (Some(state), Some(weak)) = (globals.collector, globals.weak) else {
        instructions(&mut function, [I::I32Const(0), I::End]);
        return function;
    };
    let marks = Incremental {
        state,
        bits: BITS,
        globals,
        edges,
    };
    let mut code = ephemerons(globals, weak, &marks, L);
    code.extend([
        I::LocalGet(L.new),
        I::If(BlockType::Empty),
        I::I32Const(1),
        I::Return,
        I::End,
    ]);
    code.extend(vacate(globals, weak, &marks, L));
    code.extend([I::I32Const(0), I::End]);
    instructions(&mut function, code);
    function
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Key slots hold identity addresses: the empty and vacated markers lie
    /// below every heap address, and a pair is two value slots.
    #[test]
    fn the_table_layout_fits_the_heap() {
        assert!(w::KEY_VACATED < HEADER as u64);
        assert_eq!(
            w::PAIR_BYTES as usize,
            2 * varta_common::value::RECORD_SLOT_BYTES
        );
        assert_eq!(
            w::PAIRS_OFFSET % varta_common::value::HEAP_ALIGNMENT as u32,
            0
        );
        assert!(w::MIN_PAIRS.is_power_of_two());
        assert_eq!(
            WEAK_BLOCK & super::super::collect::edge_flags(true, true),
            0
        );
    }
}
