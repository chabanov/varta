//! Fixed bounded presence bitmap follows capacity payload slots, outside GC edges.
use super::*;
use varta_common::value as values;
const SHIFT: i32 = u8::BITS.trailing_zeros() as i32;
const MASK: i32 = (u8::BITS - 1) as i32;
const BYTE: MemArg = MemArg {
    offset: 0,
    align: 0,
    memory_index: MEMORY,
};
/// Size arithmetic of sparse backings. Up to 1024 slots the presence bitmap is
/// the fixed 128 bytes of value layout 12; v38 (`data_limits`) backings above
/// 1024 slots carry one bit per slot (`DataLimits::sparse_bitmap_bytes`).
/// Every function emits exactly the earlier instructions when `large` is false.
#[derive(Clone, Copy)]
pub(in crate::emit) struct Sizes {
    /// Whether backings may exceed 1024 slots (v38).
    pub large: bool,
}
/// Packed backing bytes of the largest fixed-bitmap backing (1024 slots).
const SMALL_LIMIT: i32 = (values::ARRAY_META_BYTES
    + values::MAX_ARRAY_ELEMENTS * values::ARRAY_SLOT_BYTES
    + values::SPARSE_BITMAP_BYTES) as i32;
/// Bits a slot of a large backing takes: eight payload bytes and one bit.
const SLOT_BITS: i32 = (values::ARRAY_SLOT_BYTES * u8::BITS as usize + 1) as i32;
const BIT_SHIFT: i32 = u8::BITS.trailing_zeros() as i32;
impl Sizes {
    pub(in crate::emit) fn new(limits: varta_common::limits::DataLimits) -> Self {
        Self {
            large: limits.array_elements > values::MAX_ARRAY_ELEMENTS,
        }
    }
    /// `[] -> [capacity]` from the packed backing bytes that `bytes` pushes.
    pub(in crate::emit) fn capacity(self, bytes: &[I<'static>]) -> Vec<I<'static>> {
        let small = |code: &mut Vec<I<'static>>| {
            code.extend_from_slice(bytes);
            code.extend([
                I::I32Const((values::ARRAY_META_BYTES + values::SPARSE_BITMAP_BYTES) as i32),
                I::I32Sub,
                I::I32Const(values::ARRAY_SLOT_BYTES as i32),
                I::I32DivU,
            ]);
        };
        let mut code = Vec::new();
        if !self.large {
            small(&mut code);
            return code;
        }
        code.extend_from_slice(bytes);
        code.extend([
            I::I32Const(SMALL_LIMIT),
            I::I32GtU,
            I::If(BlockType::Result(ValType::I32)),
        ]);
        code.extend_from_slice(bytes);
        code.extend([
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Sub,
            I::I32Const(SLOT_BITS),
            I::I32DivU,
            I::I32Const(BIT_SHIFT),
            I::I32Shl,
            I::Else,
        ]);
        small(&mut code);
        code.push(I::End);
        code
    }
    /// `[] -> [bitmap bytes]` from the packed backing bytes that `bytes` pushes.
    pub(in crate::emit) fn bitmap_bytes(self, bytes: &[I<'static>]) -> Vec<I<'static>> {
        if !self.large {
            return vec![I::I32Const(values::SPARSE_BITMAP_BYTES as i32)];
        }
        let mut code = bytes.to_vec();
        code.extend([
            I::I32Const(SMALL_LIMIT),
            I::I32GtU,
            I::If(BlockType::Result(ValType::I32)),
        ]);
        code.extend_from_slice(bytes);
        code.extend([
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Sub,
            I::I32Const(SLOT_BITS),
            I::I32DivU,
            I::Else,
            I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
            I::End,
        ]);
        code
    }
    /// `[] -> [packed backing bytes]` for the slot capacity `capacity` pushes.
    pub(in crate::emit) fn backing_bytes(self, capacity: &[I<'static>]) -> Vec<I<'static>> {
        let mut code = capacity.to_vec();
        code.extend([
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Const((values::ARRAY_META_BYTES + values::SPARSE_BITMAP_BYTES) as i32),
            I::I32Add,
        ]);
        if !self.large {
            return code;
        }
        // Beyond 1024 slots: one more bit per slot than the fixed bitmap.
        code.extend_from_slice(capacity);
        code.extend([
            I::I32Const(values::MAX_ARRAY_ELEMENTS as i32),
            I::I32GtU,
            I::If(BlockType::Result(ValType::I32)),
        ]);
        code.extend_from_slice(capacity);
        code.extend([
            I::I32Const(BIT_SHIFT),
            I::I32ShrU,
            I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
            I::I32Sub,
            I::Else,
            I::I32Const(0),
            I::End,
            I::I32Add,
        ]);
        code
    }
}
/// The packed byte length of the backing of the vector cell in local `cell`.
pub(super) fn backing_length(cell: u32) -> [I<'static>; 6] {
    [
        I::LocalGet(cell),
        I::I32WrapI64,
        I::I64Load(MemArg {
            offset: values::VECTOR_STORAGE_OFFSET,
            ..ROOT
        }),
        I::I64Const(LENGTH_SHIFT as i64),
        I::I64ShrU,
        I::I32WrapI64,
    ]
}
/// `[] -> [bitmap address]` of the vector cell in local `cell` under `sizes`.
pub(super) fn bitmap_of(f: &mut Function, cell: u32, sizes: Sizes) {
    if !sizes.large {
        bitmap(f, cell);
        return;
    }
    instructions(
        f,
        [
            I::LocalGet(cell),
            I::I32WrapI64,
            I::I64Load(MemArg {
                offset: values::VECTOR_STORAGE_OFFSET,
                ..ROOT
            }),
            I::I32WrapI64,
        ],
    );
    instructions(f, backing_length(cell));
    f.instruction(&I::I32Add);
    instructions(f, sizes.bitmap_bytes(&backing_length(cell)));
    f.instruction(&I::I32Sub);
}
pub(super) fn bitmap(f: &mut Function, cell: u32) {
    instructions(
        f,
        [
            I::LocalGet(cell),
            I::I32WrapI64,
            I::I64Load(MemArg {
                offset: values::VECTOR_STORAGE_OFFSET,
                ..ROOT
            }),
            I::I32WrapI64,
            I::LocalGet(cell),
            I::I32WrapI64,
            I::I64Load(MemArg {
                offset: values::VECTOR_STORAGE_OFFSET,
                ..ROOT
            }),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Add,
            I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
            I::I32Sub,
        ],
    );
}
pub(super) fn get(f: &mut Function, bitmap: u32, index: u32) {
    instructions(
        f,
        [
            I::LocalGet(bitmap),
            I::LocalGet(index),
            I::I32Const(SHIFT),
            I::I32ShrU,
            I::I32Add,
            I::I32Load8U(BYTE),
            I::I32Const(1),
            I::LocalGet(index),
            I::I32Const(MASK),
            I::I32And,
            I::I32Shl,
            I::I32And,
            I::I32Eqz,
            I::I32Eqz,
        ],
    );
}
pub(super) fn set(f: &mut Function, bitmap: u32, index: u32) {
    instructions(
        f,
        [
            I::LocalGet(bitmap),
            I::LocalGet(index),
            I::I32Const(SHIFT),
            I::I32ShrU,
            I::I32Add,
            I::LocalGet(bitmap),
            I::LocalGet(index),
            I::I32Const(SHIFT),
            I::I32ShrU,
            I::I32Add,
            I::I32Load8U(BYTE),
            I::I32Const(1),
            I::LocalGet(index),
            I::I32Const(MASK),
            I::I32And,
            I::I32Shl,
            I::I32Or,
            I::I32Store8(BYTE),
        ],
    );
}
pub(super) fn clear(f: &mut Function, bitmap: u32, index: u32) {
    instructions(
        f,
        [
            I::LocalGet(bitmap),
            I::LocalGet(index),
            I::I32Const(SHIFT),
            I::I32ShrU,
            I::I32Add,
            I::LocalGet(bitmap),
            I::LocalGet(index),
            I::I32Const(SHIFT),
            I::I32ShrU,
            I::I32Add,
            I::I32Load8U(BYTE),
            I::I32Const(1),
            I::LocalGet(index),
            I::I32Const(MASK),
            I::I32And,
            I::I32Shl,
            I::I32Const(-1),
            I::I32Xor,
            I::I32And,
            I::I32Store8(BYTE),
        ],
    );
}
pub(super) fn function(write: bool, limits: varta_common::limits::DataLimits) -> Function {
    let sizes = Sizes::new(limits);
    const CELL: u32 = 0;
    const INDEX: u32 = 1;
    const PRESENT: u32 = 2;
    let bitmap = if write { 3 } else { 2 };
    let mut f = Function::new([(1, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::LocalGet(INDEX),
            I::LocalGet(CELL),
            I::I32WrapI64,
            I::I32Load(MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..WORD
            }),
            I::I32GeU,
            I::LocalGet(INDEX),
            I::I32Const(limits.array_elements as i32),
            I::I32GeU,
            I::I32Or,
            I::If(BlockType::Empty),
        ],
    );
    if write {
        f.instruction(&I::Unreachable);
    } else {
        instructions(&mut f, [I::I32Const(0), I::Return]);
    }
    f.instruction(&I::End);
    bitmap_of(&mut f, CELL, sizes);
    f.instruction(&I::LocalSet(bitmap));
    if write {
        instructions(&mut f, [I::LocalGet(PRESENT), I::If(BlockType::Empty)]);
        set(&mut f, bitmap, INDEX);
        f.instruction(&I::Else);
        clear(&mut f, bitmap, INDEX);
        f.instruction(&I::End);
    } else {
        get(&mut f, bitmap, INDEX);
    }
    f.instruction(&I::End);
    f
}
