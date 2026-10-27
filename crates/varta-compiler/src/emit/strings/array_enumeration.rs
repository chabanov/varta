//! Own array indices only: reserve zeroed storage, publish one canonical payload
//! at a time, and keep the partial output rooted across decimal-key allocations.
use super::*;
use varta_common::value as values;
const INPUT: u32 = 0;
const SOURCE: u32 = 1;
const LENGTH: u32 = 2;
const CAPACITY: u32 = 3;
const BYTES: u32 = 4;
const DEST: u32 = 5;
const CELL: u32 = 6;
const INDEX: u32 = 7;
const COUNT: u32 = 8;
const SAVED_ROOTS: u32 = 9;
const SOURCE_BITS: u32 = 10;
const DEST_BITS: u32 = 11;
const DIGITS: u32 = 12;
const QUOTIENT: u32 = 13;
const POSITION: u32 = 14;
const TEXT: u32 = 15;
const BACKING: u32 = 16;
const OUTPUT: u32 = 17;
const AGAIN: u32 = 0;
const DONE: u32 = 1;
const RADIX: i32 = 10;
const UNIT_BYTES: i32 = size_of::<u16>() as i32;
const LENGTH_WORD: MemArg = MemArg {
    offset: values::ARRAY_LENGTH_OFFSET,
    ..WORD
};
const STORAGE: MemArg = MemArg {
    offset: values::VECTOR_STORAGE_OFFSET,
    ..ROOT
};
const UNIT: MemArg = MemArg {
    offset: 0,
    align: 1,
    memory_index: MEMORY,
};

pub(super) fn function(
    globals: Globals,
    allocate: u32,
    retain: u32,
    keys: bool,
    sparse: bool,
) -> Function {
    let mut f = Function::new([(15, ValType::I32), (2, ValType::I64)]);
    instructions(
        &mut f,
        [
            I::GlobalGet(globals.roots),
            I::LocalSet(SAVED_ROOTS),
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::I32Load(LENGTH_WORD),
            I::LocalTee(LENGTH),
            I::I32Const(values::MAX_ARRAY_ELEMENTS as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::I64Load(STORAGE),
            I::I32WrapI64,
            I::LocalSet(SOURCE),
            I::LocalGet(LENGTH),
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
            I::LocalGet(LENGTH),
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
        ],
    );
    header(
        &mut f,
        DEST,
        ALLOCATED | values::HEAP_ARRAY_FLAG | if sparse { values::HEAP_SPARSE_FLAG } else { 0 },
        keys,
    );
    pack(&mut f, BYTES, DEST);
    instructions(
        &mut f,
        [
            I::Call(retain),
            I::LocalSet(BACKING),
            I::I32Const(values::VECTOR_BYTES as i32),
            I::Call(allocate),
            I::LocalSet(CELL),
            I::LocalGet(CELL),
            I::I32Const(0),
            I::I32Const(values::VECTOR_BYTES as i32),
            I::MemoryFill(MEMORY),
        ],
    );
    header(&mut f, CELL, ALLOCATED | values::HEAP_VECTOR_FLAG, keys);
    instructions(
        &mut f,
        [
            I::LocalGet(CELL),
            I::LocalGet(BACKING),
            I::I64Store(STORAGE),
            I::I32Const(values::VECTOR_BYTES as i32),
            I::LocalSet(BYTES),
        ],
    );
    pack(&mut f, BYTES, CELL);
    instructions(&mut f, [I::Call(retain), I::LocalSet(OUTPUT)]);
    if sparse {
        sparse::bitmap(&mut f, INPUT);
        f.instruction(&I::LocalSet(SOURCE_BITS));
        sparse::bitmap(&mut f, OUTPUT);
        f.instruction(&I::LocalSet(DEST_BITS));
    }
    instructions(
        &mut f,
        [
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(INDEX),
            I::LocalGet(LENGTH),
            I::I32GeU,
            I::BrIf(DONE),
        ],
    );
    if sparse {
        sparse::get(&mut f, SOURCE_BITS, INDEX);
        f.instruction(&I::If(BlockType::Empty));
    }
    // DEST/CELL are nonmoving and retained. Lengths describe only the completed
    // prefix at every allocation; no absent reference/union is ever traced.
    instructions(
        &mut f,
        [
            I::LocalGet(DEST),
            I::LocalGet(COUNT),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
        ],
    );
    if keys {
        index_string(&mut f, allocate);
    } else {
        instructions(
            &mut f,
            [
                I::LocalGet(SOURCE),
                I::LocalGet(INDEX),
                I::I32Const(values::ARRAY_SLOT_BYTES as i32),
                I::I32Mul,
                I::I32Add,
                I::I64Load(STORAGE),
            ],
        );
    }
    f.instruction(&I::I64Store(STORAGE));
    if sparse {
        sparse::set(&mut f, DEST_BITS, COUNT);
    }
    instructions(
        &mut f,
        [
            I::LocalGet(COUNT),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(COUNT),
        ],
    );
    for pointer in [DEST, CELL] {
        instructions(
            &mut f,
            [
                I::LocalGet(pointer),
                I::LocalGet(COUNT),
                I::I32Store(LENGTH_WORD),
            ],
        );
    }
    if sparse {
        f.instruction(&I::End);
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
            I::LocalGet(SAVED_ROOTS),
            I::GlobalSet(globals.roots),
            I::LocalGet(OUTPUT),
            I::End,
        ],
    );
    f
}
fn header(f: &mut Function, dest: u32, flags: i32, keys: bool) {
    instructions(
        f,
        [
            I::LocalGet(dest),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(flags),
            I::I32Store(FLAG),
            I::LocalGet(dest),
            I::I32Const(HEADER as i32),
            I::I32Sub,
        ],
    );
    if keys || flags & values::HEAP_VECTOR_FLAG != 0 {
        f.instruction(&I::I32Const(1));
    } else {
        instructions(
            f,
            [
                I::LocalGet(SOURCE),
                I::I32Const(HEADER as i32),
                I::I32Sub,
                I::I32Load(LINK),
            ],
        );
    }
    instructions(f, [I::I32Store(LINK), I::LocalGet(dest)]);
    if keys {
        f.instruction(&I::I32Const(values::ValueKind::String as i32));
    } else {
        instructions(f, [I::LocalGet(SOURCE), I::I32Load(WORD)]);
    }
    f.instruction(&I::I32Store(WORD));
}
/// Only bounded nonnegative array indices enter this path. Format directly as
/// UTF-16 decimal, with one allocation and no general coercion or host operation.
fn index_string(f: &mut Function, allocate: u32) {
    instructions(
        f,
        [
            I::I32Const(1),
            I::LocalSet(DIGITS),
            I::LocalGet(INDEX),
            I::LocalSet(QUOTIENT),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(QUOTIENT),
            I::I32Const(RADIX),
            I::I32LtU,
            I::BrIf(DONE),
            I::LocalGet(QUOTIENT),
            I::I32Const(RADIX),
            I::I32DivU,
            I::LocalSet(QUOTIENT),
            I::LocalGet(DIGITS),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(DIGITS),
            I::Br(AGAIN),
            I::End,
            I::End,
            I::LocalGet(DIGITS),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalTee(POSITION),
            I::Call(allocate),
            I::LocalSet(TEXT),
            I::LocalGet(INDEX),
            I::LocalSet(QUOTIENT),
            I::Loop(BlockType::Empty),
            I::LocalGet(POSITION),
            I::I32Const(UNIT_BYTES),
            I::I32Sub,
            I::LocalSet(POSITION),
            I::LocalGet(TEXT),
            I::LocalGet(POSITION),
            I::I32Add,
            I::LocalGet(QUOTIENT),
            I::I32Const(RADIX),
            I::I32RemU,
            I::I32Const(i32::from(b'0')),
            I::I32Add,
            I::I32Store16(UNIT),
            I::LocalGet(QUOTIENT),
            I::I32Const(RADIX),
            I::I32DivU,
            I::LocalSet(QUOTIENT),
            I::LocalGet(POSITION),
            I::BrIf(AGAIN),
            I::End,
            I::LocalGet(TEXT),
            I::I64ExtendI32U,
            I::LocalGet(DIGITS),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::I64ExtendI32U,
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64Shl,
            I::I64Or,
        ],
    );
}
