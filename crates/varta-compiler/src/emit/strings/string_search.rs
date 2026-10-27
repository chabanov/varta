//! Allocation-free, fuel-metered search over exact UTF-16 code units. Positions
//! are clamped before conversion; all candidate ranges fit the source payload.
use super::*;
use crate::ir::StringSearch;
const INPUT: u32 = 0;
const NEEDLE: u32 = 1;
const POSITION: u32 = 2;
const LENGTH: u32 = 3;
const NEEDLE_LENGTH: u32 = 4;
const FIRST: u32 = 5;
const LAST: u32 = 6;
const OFFSET: u32 = 7;
const UNIT_BYTES: i32 = size_of::<u16>() as i32;
const UNIT: MemArg = MemArg {
    offset: 0,
    align: 1,
    memory_index: MEMORY,
};
const AGAIN: u32 = 0;
const DONE: u32 = 1;
const MISMATCH: u32 = 1;

pub(super) fn function(mode: StringSearch) -> Function {
    let mut f = Function::new([(OFFSET - POSITION, ValType::I32)]);
    for (input, output) in [(INPUT, LENGTH), (NEEDLE, NEEDLE_LENGTH)] {
        length(&mut f, input, output);
        instructions(
            &mut f,
            [
                I::LocalGet(output),
                I::I32Const(UNIT_BYTES),
                I::I32DivU,
                I::LocalSet(output),
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalGet(POSITION),
            I::LocalGet(POSITION),
            I::F64Ne,
            I::If(BlockType::Result(ValType::F64)),
            I::F64Const(0.0.into()),
            I::Else,
            I::LocalGet(POSITION),
            I::F64Trunc,
            I::End,
            I::F64Const(0.0.into()),
            I::F64Max,
            I::LocalGet(LENGTH),
            I::F64ConvertI32U,
            I::F64Min,
            I::I32TruncF64U,
            I::LocalSet(FIRST),
            I::LocalGet(NEEDLE_LENGTH),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::Return,
            I::End,
            I::LocalGet(NEEDLE_LENGTH),
        ],
    );
    if mode == StringSearch::EndsWith {
        f.instruction(&I::LocalGet(FIRST));
    } else {
        instructions(&mut f, [I::LocalGet(LENGTH), I::LocalGet(FIRST), I::I32Sub]);
    }
    instructions(
        &mut f,
        [
            I::I32GtU,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
        ],
    );
    match mode {
        StringSearch::Includes => instructions(
            &mut f,
            [
                I::LocalGet(LENGTH),
                I::LocalGet(NEEDLE_LENGTH),
                I::I32Sub,
                I::LocalSet(LAST),
            ],
        ),
        StringSearch::EndsWith => instructions(
            &mut f,
            [
                I::LocalGet(FIRST),
                I::LocalGet(NEEDLE_LENGTH),
                I::I32Sub,
                I::LocalSet(FIRST),
            ],
        ),
        StringSearch::StartsWith => {}
    }
    if mode == StringSearch::Includes {
        instructions(
            &mut f,
            [
                I::Block(BlockType::Empty),
                I::Loop(BlockType::Empty),
                I::LocalGet(FIRST),
                I::LocalGet(LAST),
                I::I32GtU,
                I::BrIf(DONE),
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::I32Const(0),
            I::LocalSet(OFFSET),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(OFFSET),
            I::LocalGet(NEEDLE_LENGTH),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::Return,
            I::End,
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalGet(FIRST),
            I::LocalGet(OFFSET),
            I::I32Add,
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Load16U(UNIT),
            I::LocalGet(NEEDLE),
            I::I32WrapI64,
            I::LocalGet(OFFSET),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Load16U(UNIT),
            I::I32Ne,
            I::BrIf(MISMATCH),
            I::LocalGet(OFFSET),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(OFFSET),
            I::Br(AGAIN),
            I::End,
            I::End,
        ],
    );
    if mode == StringSearch::Includes {
        instructions(
            &mut f,
            [
                I::LocalGet(FIRST),
                I::I32Const(1),
                I::I32Add,
                I::LocalSet(FIRST),
                I::Br(AGAIN),
                I::End,
                I::End,
            ],
        );
    }
    instructions(&mut f, [I::I32Const(0), I::End]);
    f
}
