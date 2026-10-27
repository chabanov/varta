//! In-place fill of a checked resizable array. Only slot roots/range presence
//! change; capacity and array identity are preserved.
use super::*;
use varta_common::value as values;

const INPUT: u32 = 0;
const VALUE: u32 = 1;
const START: u32 = 2;
const END: u32 = 3;
const CELL: u32 = 4;
const LENGTH: u32 = 5;
const STORAGE: u32 = 6;
const FIRST: u32 = 7;
const LAST: u32 = 8;
const INDEX: u32 = 9;
const BITMAP: u32 = 10;
const AGAIN: u32 = 0;
const DONE: u32 = 1;

const LENGTH_WORD: MemArg = MemArg {
    offset: values::ARRAY_LENGTH_OFFSET,
    ..WORD
};
const STORAGE_WORD: MemArg = MemArg {
    offset: values::VECTOR_STORAGE_OFFSET,
    ..ROOT
};

pub(super) fn function(sparse: bool) -> Function {
    let mut f = Function::new([(7, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalTee(CELL),
            I::I32Load(LENGTH_WORD),
            I::LocalSet(LENGTH),
            I::LocalGet(CELL),
            I::I64Load(STORAGE_WORD),
            I::I32WrapI64,
            I::LocalSet(STORAGE),
        ],
    );
    if sparse {
        sparse::bitmap(&mut f, INPUT);
        f.instruction(&I::LocalSet(BITMAP));
    }
    relative(&mut f, START, FIRST);
    relative(&mut f, END, LAST);
    instructions(
        &mut f,
        [
            I::LocalGet(FIRST),
            I::LocalSet(INDEX),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(INDEX),
            I::LocalGet(LAST),
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
        ],
    );
    f.instruction(&I::LocalGet(VALUE));
    f.instruction(&I::I64Store(ROOT));
    if sparse {
        sparse::set(&mut f, BITMAP, INDEX);
    }
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

fn relative(f: &mut Function, input: u32, output: u32) {
    instructions(
        f,
        [
            I::LocalGet(input),
            I::LocalGet(input),
            I::F64Ne,
            I::If(BlockType::Result(ValType::F64)),
            I::F64Const(0.0.into()),
            I::Else,
            I::LocalGet(input),
            I::F64Trunc,
            I::End,
            I::LocalSet(input),
            I::LocalGet(input),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(input),
            I::LocalGet(LENGTH),
            I::F64ConvertI32U,
            I::F64Add,
            I::F64Const(0.0.into()),
            I::F64Max,
            I::Else,
            I::LocalGet(input),
            I::LocalGet(LENGTH),
            I::F64ConvertI32U,
            I::F64Min,
            I::End,
            I::I32TruncF64U,
            I::LocalSet(output),
        ],
    );
}
