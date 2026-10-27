//! Copy a checked dense range into fresh backing and identity blocks. The caller
//! roots the receiver; retain the copied backing before allocating its cell.
use super::*;
use varta_common::value as values;
const INPUT: u32 = 0;
const START: u32 = 1;
const END: u32 = 2;
const SOURCE: u32 = 3;
const LENGTH: u32 = 4;
const FIRST: u32 = 5;
const LAST: u32 = 6;
const COUNT: u32 = 7;
const CAPACITY: u32 = 8;
const DEST: u32 = 9;
const BYTES: u32 = 10;
const BACKING: u32 = 11;
const CELL: u32 = 12;
const OWNED: u32 = 13;
const BIT_INDEX: u32 = 14;
const BIT_SOURCE: u32 = 15;
const BIT_DEST: u32 = 16;
const BIT_INPUT: u32 = 17;
const AGAIN: u32 = 0;
const DONE: u32 = 1;
const LENGTH_WORD: MemArg = MemArg {
    offset: values::ARRAY_LENGTH_OFFSET,
    ..WORD
};
const STORAGE: MemArg = MemArg {
    offset: values::VECTOR_STORAGE_OFFSET,
    ..ROOT
};

pub(super) fn function(allocate: u32, retain: u32, sparse: bool) -> Function {
    let mut f = Function::new([
        (10, ValType::I32),
        (1, ValType::I64),
        (if sparse { 4 } else { 0 }, ValType::I32),
    ]);
    instructions(
        &mut f,
        [
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalTee(SOURCE),
            I::I32Load(LENGTH_WORD),
            I::LocalSet(LENGTH),
            I::LocalGet(SOURCE),
            I::I64Load(STORAGE),
            I::I32WrapI64,
            I::LocalSet(BACKING),
        ],
    );
    relative(&mut f, START, FIRST);
    relative(&mut f, END, LAST);
    instructions(
        &mut f,
        [
            I::LocalGet(LAST),
            I::LocalGet(FIRST),
            I::I32GtU,
            I::If(BlockType::Result(ValType::I32)),
            I::LocalGet(LAST),
            I::LocalGet(FIRST),
            I::I32Sub,
            I::Else,
            I::I32Const(0),
            I::End,
            I::LocalSet(COUNT),
            I::LocalGet(COUNT),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::LocalSet(CAPACITY),
            I::Else,
            I::I32Const(1),
            I::LocalSet(CAPACITY),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(CAPACITY),
            I::LocalGet(COUNT),
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(CAPACITY),
            I::I32Const(1),
            I::I32Shl,
            I::LocalSet(CAPACITY),
            I::Br(AGAIN),
            I::End,
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
            I::LocalGet(SOURCE),
            I::I32Load(WORD),
            I::I32Store(WORD),
            I::LocalGet(DEST),
            I::LocalGet(COUNT),
            I::I32Store(LENGTH_WORD),
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
            I::LocalGet(BACKING),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Load(LINK),
            I::I32Store(LINK),
            I::LocalGet(DEST),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(BACKING),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(FIRST),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::LocalGet(COUNT),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    if sparse {
        sparse::bitmap(&mut f, INPUT);
        f.instruction(&I::LocalSet(BIT_SOURCE));
        instructions(
            &mut f,
            [
                I::LocalGet(DEST),
                I::LocalGet(BYTES),
                I::I32Add,
                I::I32Const(values::SPARSE_BITMAP_BYTES as i32),
                I::I32Sub,
                I::LocalSet(BIT_DEST),
                I::I32Const(0),
                I::LocalSet(BIT_INDEX),
                I::Block(BlockType::Empty),
                I::Loop(BlockType::Empty),
                I::LocalGet(BIT_INDEX),
                I::LocalGet(COUNT),
                I::I32GeU,
                I::BrIf(DONE),
                I::LocalGet(FIRST),
                I::LocalGet(BIT_INDEX),
                I::I32Add,
                I::LocalSet(BIT_INPUT),
            ],
        );
        sparse::get(&mut f, BIT_SOURCE, BIT_INPUT);
        f.instruction(&I::If(BlockType::Empty));
        sparse::set(&mut f, BIT_DEST, BIT_INDEX);
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
    pack(&mut f, BYTES, DEST);
    instructions(
        &mut f,
        [
            I::Call(retain),
            I::LocalSet(OWNED),
            I::I32Const(values::VECTOR_BYTES as i32),
            I::Call(allocate),
            I::LocalSet(CELL),
            I::LocalGet(CELL),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(ALLOCATED | values::HEAP_VECTOR_FLAG),
            I::I32Store(FLAG),
            I::LocalGet(CELL),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(1),
            I::I32Store(LINK),
            I::LocalGet(CELL),
            I::LocalGet(SOURCE),
            I::I32Load(WORD),
            I::I32Store(WORD),
            I::LocalGet(CELL),
            I::LocalGet(COUNT),
            I::I32Store(LENGTH_WORD),
            I::LocalGet(CELL),
            I::LocalGet(OWNED),
            I::I64Store(STORAGE),
            I::I32Const(values::VECTOR_BYTES as i32),
            I::LocalSet(BYTES),
        ],
    );
    pack(&mut f, BYTES, CELL);
    f.instruction(&I::End);
    f
}

/// Number|Undefined admission is handled at the call site. Normalize finite
/// fractions, NaN and infinities before the bounded conversion to a slot index.
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
