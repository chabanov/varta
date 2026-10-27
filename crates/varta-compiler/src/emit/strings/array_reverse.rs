//! In-place reversal of a checked resizable array. Identity, capacity and rooted
//! slot handles are preserved; only slot positions and sparse presence move.
use super::*;
use varta_common::value as values;

const INPUT: u32 = 0;
const CELL: u32 = 1;
const LENGTH: u32 = 2;
const STORAGE: u32 = 3;
const INDEX: u32 = 4;
const OTHER: u32 = 5;
const BITMAP: u32 = 6;
const BIT_LEFT: u32 = 7;
const BIT_RIGHT: u32 = 8;
const _SPARE: u32 = 9;
const TEMP: u32 = 10;
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

pub(super) fn function(sparse: bool) -> Function {
    let mut f = Function::new([(9, ValType::I32), (1, ValType::I64)]);
    instructions(
        &mut f,
        [
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalTee(CELL),
            I::I32Load(COUNT),
            I::LocalSet(LENGTH),
            I::LocalGet(CELL),
            I::I64Load(BACKING),
            I::I32WrapI64,
            I::LocalSet(STORAGE),
        ],
    );
    if sparse {
        sparse::bitmap(&mut f, INPUT);
        f.instruction(&I::LocalSet(BITMAP));
    }
    instructions(
        &mut f,
        [
            I::I32Const(0),
            I::LocalSet(INDEX),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(INDEX),
            I::LocalGet(LENGTH),
            I::I32Const(2),
            I::I32DivU,
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(LENGTH),
            I::I32Const(1),
            I::I32Sub,
            I::LocalGet(INDEX),
            I::I32Sub,
            I::LocalSet(OTHER),
        ],
    );
    if sparse {
        sparse::get(&mut f, BITMAP, INDEX);
        f.instruction(&I::LocalSet(BIT_LEFT));
        sparse::get(&mut f, BITMAP, OTHER);
        f.instruction(&I::LocalSet(BIT_RIGHT));
        instructions(
            &mut f,
            [
                I::LocalGet(BIT_LEFT),
                I::LocalGet(BIT_RIGHT),
                I::I32Ne,
                I::If(BlockType::Empty),
                I::LocalGet(BIT_LEFT),
                I::If(BlockType::Empty),
            ],
        );
        sparse::clear(&mut f, BITMAP, INDEX);
        sparse::set(&mut f, BITMAP, OTHER);
        instructions(&mut f, [I::Else]);
        sparse::set(&mut f, BITMAP, INDEX);
        sparse::clear(&mut f, BITMAP, OTHER);
        instructions(&mut f, [I::End, I::End]);
    }
    // swap slot values
    // TEMP = slot INDEX
    instructions(
        &mut f,
        [
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
        ],
    );
    f.instruction(&I::I64Load(ROOT));
    f.instruction(&I::LocalSet(TEMP));
    // slot INDEX = slot OTHER
    instructions(
        &mut f,
        [
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
        ],
    );
    instructions(
        &mut f,
        [
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(OTHER),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
        ],
    );
    f.instruction(&I::I64Load(ROOT));
    f.instruction(&I::I64Store(ROOT));
    // slot OTHER = TEMP
    instructions(
        &mut f,
        [
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(OTHER),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
        ],
    );
    f.instruction(&I::LocalGet(TEMP));
    f.instruction(&I::I64Store(ROOT));
    instructions(
        &mut f,
        [
            I::LocalGet(INDEX),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(INDEX),
            I::Br(AGAIN),
            I::End,
            I::End,
            I::LocalGet(INPUT),
            I::End,
        ],
    );
    f
}
