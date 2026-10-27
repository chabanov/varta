//! Bounded sparse resize. Allocate/copy before growth publication; shrink clears
//! every removed edge and presence bit before committing either live count.
use super::*;
use varta_common::value as values;
const TARGET: u32 = 0;
const LENGTH: u32 = 1;
const CELL: u32 = 2;
const OLD_LENGTH: u32 = 3;
const STORAGE: u32 = 4;
const CAPACITY: u32 = 5;
const BYTES: u32 = 6;
const DEST: u32 = 7;
const BITMAP: u32 = 8;
const CURSOR: u32 = 9;
const OLD_BYTES: u32 = 10;
const AGAIN: u32 = 0;
const DONE: u32 = 1;
const COUNT: MemArg = MemArg {
    offset: values::ARRAY_LENGTH_OFFSET,
    ..WORD
};
const BACKING: MemArg = MemArg {
    offset: values::VECTOR_STORAGE_OFFSET,
    ..ROOT
};
pub(super) fn function(allocate: u32) -> Function {
    let mut f = Function::new([(9, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::LocalGet(LENGTH),
            I::I32Const(values::MAX_ARRAY_ELEMENTS as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(TARGET),
            I::I32WrapI64,
            I::LocalTee(CELL),
            I::I32Load(COUNT),
            I::LocalSet(OLD_LENGTH),
            I::LocalGet(CELL),
            I::I64Load(BACKING),
            I::I32WrapI64,
            I::LocalSet(STORAGE),
            I::LocalGet(CELL),
            I::I64Load(BACKING),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalTee(OLD_BYTES),
            I::I32Const((values::ARRAY_META_BYTES + values::SPARSE_BITMAP_BYTES) as i32),
            I::I32Sub,
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32DivU,
            I::LocalSet(CAPACITY),
            I::LocalGet(LENGTH),
            I::LocalGet(CAPACITY),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::I32Const(i32::BITS as i32),
            I::LocalGet(LENGTH),
            I::I32Const(1),
            I::I32Sub,
            I::I32Clz,
            I::I32Sub,
            I::I32Shl,
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Const((values::ARRAY_META_BYTES + values::SPARSE_BITMAP_BYTES) as i32),
            I::I32Add,
            I::LocalTee(BYTES),
            I::Call(allocate),
            I::LocalSet(DEST),
            I::LocalGet(DEST),
            I::I32Const(0),
            I::LocalGet(BYTES),
            I::MemoryFill(MEMORY),
            I::LocalGet(DEST),
            I::LocalGet(CELL),
            I::I32Load(WORD),
            I::I32Store(WORD),
            I::LocalGet(DEST),
            I::LocalGet(LENGTH),
            I::I32Store(COUNT),
            I::LocalGet(DEST),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(ALLOCATED | values::HEAP_ARRAY_FLAG | values::HEAP_SPARSE_FLAG),
            I::I32Store(FLAG),
            I::LocalGet(DEST),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::LocalGet(STORAGE),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Load(LINK),
            I::I32Store(LINK),
            I::LocalGet(DEST),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(OLD_LENGTH),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
            I::LocalGet(DEST),
            I::LocalGet(BYTES),
            I::I32Add,
            I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
            I::I32Sub,
            I::LocalGet(STORAGE),
            I::LocalGet(OLD_BYTES),
            I::I32Add,
            I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
            I::I32Sub,
            I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
            I::LocalGet(CELL),
        ],
    );
    pack(&mut f, BYTES, DEST);
    instructions(
        &mut f,
        [
            I::I64Store(BACKING),
            I::LocalGet(CELL),
            I::LocalGet(LENGTH),
            I::I32Store(COUNT),
            I::Return,
            I::End,
            I::LocalGet(LENGTH),
            I::LocalGet(OLD_LENGTH),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::LocalGet(STORAGE),
            I::LocalGet(OLD_BYTES),
            I::I32Add,
            I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
            I::I32Sub,
            I::LocalSet(BITMAP),
            I::LocalGet(LENGTH),
            I::LocalSet(CURSOR),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(CURSOR),
            I::LocalGet(OLD_LENGTH),
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(STORAGE),
            I::LocalGet(CURSOR),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Const(0),
            I::I64Store(MemArg {
                offset: values::ARRAY_META_BYTES as u64,
                ..ROOT
            }),
        ],
    );
    sparse::clear(&mut f, BITMAP, CURSOR);
    instructions(
        &mut f,
        [
            I::LocalGet(CURSOR),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(CURSOR),
            I::Br(AGAIN),
            I::End,
            I::End,
            I::End,
            I::LocalGet(STORAGE),
            I::LocalGet(LENGTH),
            I::I32Store(COUNT),
            I::LocalGet(CELL),
            I::LocalGet(LENGTH),
            I::I32Store(COUNT),
            I::End,
        ],
    );
    f
}
