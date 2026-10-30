//! V38 (`codegen_v2`): exact-size free lists of small heap blocks.
//!
//! One mutable i32 global heads the list of free blocks of each size from a
//! bare header up to [`MAX_CLASS_BYTES`], in steps of the heap alignment; an
//! i64 global after them has bit `c` set while list `c` is nonempty. Blocks
//! keep the ordinary free-block header (size, zero flags, link to the next
//! block of the list), so the heap walk, the collector and the value readers
//! see the same heap. The sweep rebuilds every list from the coalesced dead
//! runs; the allocator pops exact sizes in constant time and finds the
//! smallest nonempty larger class through the mask. The lists and the mask
//! live in globals, outside every static, host and root region; a class
//! selects its global through one `br_table`.
use super::*;

/// Exact-size classes: block sizes `HEADER + ALIGN * c` for `c < CLASSES`.
pub(in crate::emit) const CLASSES: u32 = 34;
/// Size units (heap alignments) of the smallest class: one bare header.
const FIRST_CLASS_UNITS: i32 = (HEADER / ALIGN) as i32;
const ALIGN_SHIFT: i32 = ALIGN.trailing_zeros() as i32;
/// The largest block (header included) kept in an exact-size list.
pub(in crate::emit) const MAX_CLASS_BYTES: i32 =
    HEADER as i32 + (CLASSES as i32 - 1) * ALIGN as i32;

/// The list globals and the nonempty mask global.
#[derive(Clone, Copy)]
pub(super) struct Lists {
    first: u32,
    mask: u32,
}
impl Lists {
    pub(super) fn new(first: u32) -> Self {
        Self {
            first,
            mask: first + CLASSES,
        }
    }
    /// Empty every list (the sweep's start).
    pub(super) fn reset(self) -> Vec<I<'static>> {
        let mut code: Vec<_> = (0..CLASSES)
            .flat_map(|class| [I::I32Const(0), I::GlobalSet(self.first + class)])
            .collect();
        code.extend([I::I64Const(0), I::GlobalSet(self.mask)]);
        code
    }
    /// Push the free block in `block`, whose size in `size` has a class,
    /// onto its list; `class` is a scratch local.
    pub(super) fn push(self, block: u32, size: u32, class: u32) -> Vec<I<'static>> {
        let mut code = class_of(size).to_vec();
        code.extend([I::LocalSet(class), I::LocalGet(block)]);
        code.extend(self.head(class));
        code.push(I::I32Store(LINK));
        code.extend(self.set_head(class, block));
        code.extend([
            I::GlobalGet(self.mask),
            I::I64Const(1),
            I::LocalGet(class),
            I::I64ExtendI32U,
            I::I64Shl,
            I::I64Or,
            I::GlobalSet(self.mask),
        ]);
        code
    }
    /// Pop the head of list `class` (in range) into `block`, or leave zero
    /// there when the list is empty; `next` is a scratch local.
    fn pop(self, class: u32, block: u32, next: u32) -> Vec<I<'static>> {
        let mut code = self.head(class);
        code.extend([
            I::LocalTee(block),
            I::If(BlockType::Empty),
            I::LocalGet(block),
            I::I32Load(LINK),
            I::LocalTee(next),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::GlobalGet(self.mask),
            I::I64Const(1),
            I::LocalGet(class),
            I::I64ExtendI32U,
            I::I64Shl,
            I::I64Const(-1),
            I::I64Xor,
            I::I64And,
            I::GlobalSet(self.mask),
            I::End,
        ]);
        code.extend(self.set_head(class, next));
        code.push(I::End);
        code
    }
    /// `[] -> [i32]`: the head of list `class`, or zero out of range.
    fn head(self, class: u32) -> Vec<I<'static>> {
        let mut code = vec![I::Block(BlockType::Result(ValType::I32))];
        code.extend(self.table(class, |list, depth| vec![I::GlobalGet(list), I::Br(depth)]));
        code.extend([I::I32Const(0), I::End]);
        code
    }
    /// Set list `class` (in range) to the block in `value`.
    fn set_head(self, class: u32, value: u32) -> Vec<I<'static>> {
        let mut code = vec![I::Block(BlockType::Empty)];
        code.extend(self.table(class, |list, depth| {
            vec![I::LocalGet(value), I::GlobalSet(list), I::Br(depth)]
        }));
        code.extend([I::Unreachable, I::End]);
        code
    }
    /// The `br_table` over the class in `class`: `arm(list, depth)` leaves the
    /// enclosing block at `depth`; the code after the table is the
    /// out-of-range default, still inside that block.
    fn table(self, class: u32, arm: impl Fn(u32, u32) -> Vec<I<'static>>) -> Vec<I<'static>> {
        let mut code = vec![I::Block(BlockType::Empty)];
        code.extend((0..CLASSES).map(|_| I::Block(BlockType::Empty)));
        code.extend([
            I::LocalGet(class),
            I::BrTable((0..CLASSES).collect::<Vec<_>>().into(), CLASSES),
        ]);
        for index in 0..CLASSES {
            code.push(I::End);
            // Past the remaining class blocks and the default block.
            code.extend(arm(self.first + index, CLASSES - index));
        }
        code.push(I::End);
        code
    }
}

/// `[] -> [i32]`: the class of the block size in `local`; a size outside the
/// classes gives an out-of-range class.
pub(super) fn class_of(local: u32) -> [I<'static>; 5] {
    [
        I::LocalGet(local),
        I::I32Const(ALIGN_SHIFT),
        I::I32ShrU,
        I::I32Const(FIRST_CLASS_UNITS),
        I::I32Sub,
    ]
}

/// The allocator locals the class paths use.
pub(super) struct AllocateLocals {
    pub need: u32,
    pub current: u32,
    pub end: u32,
    pub rest: u32,
    pub replacement: u32,
    pub class: u32,
    pub next: u32,
    /// An i64 local.
    pub mask: u32,
}

/// The constant-time paths at the start of every allocator retry: pop the
/// request's exact class, else bump within the committed memory (which never
/// collects; growth keeps the threshold and budget checks of the allocator),
/// else take the smallest nonempty larger class, splitting off a remainder of
/// at least one header and alignment unit into its own class.
pub(super) fn allocate(heap: u32, lists: Lists, l: AllocateLocals) -> Vec<I<'static>> {
    // While every list is empty (a growing heap) only the bump runs.
    let mut code = vec![
        I::GlobalGet(lists.mask),
        I::I64Eqz,
        I::LocalGet(l.need),
        I::I32Const(MAX_CLASS_BYTES),
        I::I32GtU,
        I::I32Or,
        I::I32Eqz,
        I::If(BlockType::Empty),
    ];
    code.extend(class_of(l.need));
    code.push(I::LocalSet(l.class));
    code.extend(lists.pop(l.class, l.current, l.next));
    code.extend([I::LocalGet(l.current), I::If(BlockType::Empty)]);
    code.extend(take(l.current));
    code.extend([
        I::End,
        I::End,
        I::GlobalGet(heap),
        I::LocalGet(l.need),
        I::I32Add,
        I::LocalTee(l.end),
        I::MemorySize(MEMORY),
        I::I32Const(abi::WASM_PAGE_BYTES.trailing_zeros() as i32),
        I::I32Shl,
        I::I32LeU,
        I::If(BlockType::Empty),
        I::GlobalGet(heap),
        I::LocalTee(l.current),
        I::LocalGet(l.need),
        I::I32Store(WORD),
        I::LocalGet(l.end),
        I::GlobalSet(heap),
    ]);
    code.extend(take(l.current));
    code.extend([
        I::End,
        // Nonempty classes above the request's (none for a larger request).
        I::GlobalGet(lists.mask),
        I::I64Eqz,
        I::LocalGet(l.need),
        I::I32Const(MAX_CLASS_BYTES),
        I::I32GeU,
        I::I32Or,
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::GlobalGet(lists.mask),
        I::I64Const(-1),
    ]);
    code.extend(class_of(l.need));
    code.extend([
        I::I32Const(1),
        I::I32Add,
        I::I64ExtendI32U,
        I::I64Shl,
        I::I64And,
        I::LocalTee(l.mask),
        I::I64Eqz,
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(l.mask),
        I::I64Ctz,
        I::I32WrapI64,
        I::LocalSet(l.class),
    ]);
    code.extend(lists.pop(l.class, l.current, l.next));
    code.extend([
        I::LocalGet(l.current),
        I::If(BlockType::Empty),
        I::LocalGet(l.current),
        I::I32Load(WORD),
        I::LocalGet(l.need),
        I::I32Sub,
        I::LocalTee(l.rest),
        I::I32Const((HEADER + ALIGN) as i32),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::LocalGet(l.current),
        I::LocalGet(l.need),
        I::I32Add,
        I::LocalTee(l.replacement),
        I::LocalGet(l.rest),
        I::I32Store(WORD),
        I::LocalGet(l.replacement),
        I::I32Const(0),
        I::I32Store(FLAG),
        I::LocalGet(l.current),
        I::LocalGet(l.need),
        I::I32Store(WORD),
    ]);
    code.extend(lists.push(l.replacement, l.rest, l.class));
    code.push(I::End);
    code.extend(take(l.current));
    code.extend([I::End, I::End, I::End]);
    code
}

/// Mark the block in `block` allocated and return its payload.
fn take(block: u32) -> [I<'static>; 10] {
    [
        I::LocalGet(block),
        I::I32Const(ALLOCATED),
        I::I32Store(FLAG),
        I::LocalGet(block),
        I::I32Const(0),
        I::I32Store(LINK),
        I::LocalGet(block),
        I::I32Const(HEADER as i32),
        I::I32Add,
        I::Return,
    ]
}
