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
pub(super) fn function(write: bool) -> Function {
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
            I::I32Const(values::MAX_ARRAY_ELEMENTS as i32),
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
    bitmap_address(&mut f, CELL, bitmap);
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
fn bitmap_address(f: &mut Function, cell: u32, output: u32) {
    bitmap(f, cell);
    f.instruction(&I::LocalSet(output));
}
