//! Append a fully evaluated, rooted argument array. Identity stays in the cell;
//! backing growth is geometric and all allocation precedes publication/mutation.
use super::*;
use varta_common::value as values;
const TARGET: u32 = 0;
const ITEMS: u32 = 1;
const CELL: u32 = 2;
const SOURCE: u32 = 3;
const LENGTH: u32 = 4;
const ADDED: u32 = 5;
const STORAGE: u32 = 6;
const INPUT: u32 = 7;
const CAPACITY: u32 = 8;
const TOTAL: u32 = 9;
const DEST: u32 = 10;
const BYTES: u32 = 11;
const BIT_INDEX: u32 = 12;
const BIT_SOURCE: u32 = 13;
const BIT_DEST: u32 = 14;
const BIT_TARGET: u32 = 15;
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
pub(super) fn function(allocate: u32, sparse: bool) -> Function {
    let mut f = Function::new([(if sparse { 14 } else { 10 }, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::LocalGet(TARGET),
            I::I32WrapI64,
            I::LocalTee(CELL),
            I::I32Load(COUNT),
            I::LocalSet(LENGTH),
            I::LocalGet(ITEMS),
            I::I32WrapI64,
            I::LocalTee(SOURCE),
            I::I32Load(COUNT),
            I::LocalTee(ADDED),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::LocalGet(LENGTH),
            I::Return,
            I::End,
            I::LocalGet(LENGTH),
            I::LocalGet(ADDED),
            I::I32Add,
            I::LocalTee(TOTAL),
            I::I32Const(values::MAX_ARRAY_ELEMENTS as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(SOURCE),
            I::I64Load(BACKING),
            I::I32WrapI64,
            I::LocalSet(INPUT),
            I::LocalGet(CELL),
            I::I64Load(BACKING),
            I::I32WrapI64,
            I::LocalSet(STORAGE),
            I::LocalGet(CELL),
            I::I64Load(BACKING),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Const(
                (values::ARRAY_META_BYTES
                    + if sparse {
                        values::SPARSE_BITMAP_BYTES
                    } else {
                        0
                    }) as i32,
            ),
            I::I32Sub,
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32DivU,
            I::LocalSet(CAPACITY),
            I::LocalGet(TOTAL),
            I::LocalGet(CAPACITY),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::LocalGet(CAPACITY),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::LocalSet(CAPACITY),
            I::End,
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(CAPACITY),
            I::LocalGet(TOTAL),
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(CAPACITY),
            I::I32Const(1),
            I::I32Shl,
            I::LocalSet(CAPACITY),
            I::Br(AGAIN),
            I::End,
            I::End,
            I::LocalGet(CAPACITY),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Const(
                (values::ARRAY_META_BYTES
                    + if sparse {
                        values::SPARSE_BITMAP_BYTES
                    } else {
                        0
                    }) as i32,
            ),
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
            I::I32Const(
                ALLOCATED
                    | values::HEAP_ARRAY_FLAG
                    | if sparse { values::HEAP_SPARSE_FLAG } else { 0 },
            ),
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
            I::LocalGet(LENGTH),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    if sparse {
        instructions(
            &mut f,
            [
                I::LocalGet(DEST),
                I::LocalGet(BYTES),
                I::I32Add,
                I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
                I::I32Sub,
            ],
        );
        sparse::bitmap(&mut f, TARGET);
        instructions(
            &mut f,
            [
                I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
                I::MemoryCopy {
                    src_mem: MEMORY,
                    dst_mem: MEMORY,
                },
            ],
        );
    }
    f.instruction(&I::LocalGet(CELL));
    pack(&mut f, BYTES, DEST);
    instructions(
        &mut f,
        [
            I::I64Store(BACKING),
            I::LocalGet(DEST),
            I::LocalSet(STORAGE),
            I::End,
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(LENGTH),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::LocalGet(INPUT),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(ADDED),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    if sparse {
        sparse::bitmap(&mut f, ITEMS);
        f.instruction(&I::LocalSet(BIT_SOURCE));
        sparse::bitmap(&mut f, TARGET);
        f.instruction(&I::LocalSet(BIT_DEST));
        instructions(
            &mut f,
            [
                I::I32Const(0),
                I::LocalSet(BIT_INDEX),
                I::Block(BlockType::Empty),
                I::Loop(BlockType::Empty),
                I::LocalGet(BIT_INDEX),
                I::LocalGet(ADDED),
                I::I32GeU,
                I::BrIf(DONE),
            ],
        );
        sparse::get(&mut f, BIT_SOURCE, BIT_INDEX);
        instructions(
            &mut f,
            [
                I::If(BlockType::Empty),
                I::LocalGet(LENGTH),
                I::LocalGet(BIT_INDEX),
                I::I32Add,
                I::LocalSet(BIT_TARGET),
            ],
        );
        sparse::set(&mut f, BIT_DEST, BIT_TARGET);
        instructions(
            &mut f,
            [
                I::End,
                I::LocalGet(BIT_INDEX),
                I::I32Const(1),
                I::I32Add,
                I::LocalSet(BIT_INDEX),
                I::Br(AGAIN),
                I::End,
                I::End,
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalGet(STORAGE),
            I::LocalGet(TOTAL),
            I::I32Store(COUNT),
            I::LocalGet(CELL),
            I::LocalGet(TOTAL),
            I::I32Store(COUNT),
            I::LocalGet(TOTAL),
            I::End,
        ],
    );
    f
}
