//! Code-unit access for the v37 engine preludes (`ir::Intrinsic`): read one
//! UTF-16 code unit of an immutable string in place, and build a fresh string
//! from an array of code units. Neither makes a host call.
use super::*;
use varta_common::value::{ARRAY_LENGTH_OFFSET, ARRAY_META_BYTES, ARRAY_SLOT_BYTES};
const UNIT_BYTES: i32 = size_of::<u16>() as i32;
const UNIT_MEMORY: MemArg = MemArg {
    offset: 0,
    align: 1,
    memory_index: MEMORY,
};
const ELEMENT: MemArg = MemArg {
    offset: ARRAY_META_BYTES as u64,
    align: 3,
    memory_index: MEMORY,
};

/// `(input: string, index: f64) -> f64`: the code unit at `index`, or NaN
/// for a negative index or one at or past the end. Callers pass integers.
pub(super) fn code_unit() -> Function {
    const INPUT: u32 = 0;
    const INDEX: u32 = 1;
    const BYTES: u32 = 2;
    const AT: u32 = 3;
    let mut f = Function::new([(AT - BYTES + 1, ValType::I32)]);
    length(&mut f, INPUT, BYTES);
    instructions(
        &mut f,
        [
            I::LocalGet(INDEX),
            I::F64Const(0.0.into()),
            I::F64Ge,
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::F64Const(f64::NAN.into()),
            I::Return,
            I::End,
            I::LocalGet(INDEX),
            I::I32TruncSatF64U,
            I::LocalTee(AT),
            I::LocalGet(BYTES),
            I::I32Const(1),
            I::I32ShrU,
            I::I32GeU,
            I::If(BlockType::Empty),
            I::F64Const(f64::NAN.into()),
            I::Return,
            I::End,
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalGet(AT),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Load16U(UNIT_MEMORY),
            I::F64ConvertI32U,
            I::End,
        ],
    );
    f
}

/// `(u0: f64, …, uN: f64) -> string` (v38 view prelude): a fresh string of
/// `count` code units, each argument truncated to an integer and stored as
/// its low 16 bits (callers pass integers in `0..=0xFFFF`).
pub(super) fn units(allocate: u32, count: u32) -> Function {
    let bytes = count;
    let dest = count + 1;
    let mut f = Function::new([(2, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::I32Const(count as i32 * UNIT_BYTES),
            I::LocalTee(bytes),
            I::Call(allocate),
            I::LocalSet(dest),
        ],
    );
    for index in 0..count {
        instructions(
            &mut f,
            [
                I::LocalGet(dest),
                I::LocalGet(index),
                I::I32TruncSatF64U,
                I::I32Store16(MemArg {
                    offset: u64::from(index) * UNIT_BYTES as u64,
                    ..UNIT_MEMORY
                }),
            ],
        );
    }
    pack(&mut f, bytes, dest);
    f.instruction(&I::End);
    f
}

/// `(units: number[]) -> string`: a fresh string whose code units are the
/// array's elements, each truncated to its low 16 bits (callers pass
/// integers in `0..=0xFFFF`). The array stays rooted by its caller.
pub(super) fn from_units(allocate: u32, resizable: bool) -> Function {
    const ARRAY: u32 = 0;
    const PTR: u32 = 1;
    const LENGTH: u32 = 2;
    const BYTES: u32 = 3;
    const DEST: u32 = 4;
    const AT: u32 = 5;
    let mut f = Function::new([(AT - PTR + 1, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::LocalGet(ARRAY),
            I::I32WrapI64,
            I::LocalTee(PTR),
            I::I32Load(MemArg {
                offset: ARRAY_LENGTH_OFFSET,
                ..WORD
            }),
            I::LocalTee(LENGTH),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalTee(BYTES),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I64Const(0),
            I::Return,
            I::End,
        ],
    );
    if resizable {
        // Resizable arrays keep their elements in a separate storage block.
        instructions(
            &mut f,
            [
                I::LocalGet(PTR),
                I::I64Load(MemArg {
                    offset: varta_common::value::VECTOR_STORAGE_OFFSET,
                    ..ROOT
                }),
                I::I32WrapI64,
                I::LocalSet(PTR),
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalGet(BYTES),
            I::Call(allocate),
            I::LocalSet(DEST),
            I::I32Const(0),
            I::LocalSet(AT),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(AT),
            I::LocalGet(LENGTH),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(DEST),
            I::LocalGet(AT),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::I32Add,
            I::LocalGet(PTR),
            I::LocalGet(AT),
            I::I32Const(ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::F64Load(ELEMENT),
            I::I32TruncSatF64S,
            I::I32Store16(UNIT_MEMORY),
            I::LocalGet(AT),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(AT),
            I::Br(0),
            I::End,
            I::End,
        ],
    );
    pack(&mut f, BYTES, DEST);
    f.instruction(&I::End);
    f
}
