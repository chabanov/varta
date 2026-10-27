//! Bounded UTF-16 code-unit ranges. Caller roots the receiver while bounds and
//! allocation run; return an owned string, never an interior GC pointer.
use super::*;
const INPUT: u32 = 0;
const START: u32 = 1;
const END: u32 = 2;
const LENGTH: u32 = 3;
const FIRST: u32 = 4;
const LAST: u32 = 5;
const BYTES: u32 = 6;
const DEST: u32 = 7;
const SWAP: u32 = 8;
const UNIT_BYTES: i32 = size_of::<u16>() as i32;

pub(super) fn function(allocate: u32, substring: bool) -> Function {
    let mut f = Function::new([(SWAP - END, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::LocalGet(INPUT),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Const(UNIT_BYTES),
            I::I32DivU,
            I::LocalSet(LENGTH),
        ],
    );
    bound(&mut f, START, FIRST, !substring);
    bound(&mut f, END, LAST, !substring);
    if substring {
        instructions(
            &mut f,
            [
                I::LocalGet(FIRST),
                I::LocalGet(LAST),
                I::I32GtU,
                I::If(BlockType::Empty),
                I::LocalGet(FIRST),
                I::LocalSet(SWAP),
                I::LocalGet(LAST),
                I::LocalSet(FIRST),
                I::LocalGet(SWAP),
                I::LocalSet(LAST),
                I::End,
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalGet(LAST),
            I::LocalGet(FIRST),
            I::I32LeU,
            I::If(BlockType::Empty),
            I::I64Const(0),
            I::Return,
            I::End,
            I::LocalGet(LAST),
            I::LocalGet(FIRST),
            I::I32Sub,
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalTee(BYTES),
            I::Call(allocate),
            I::LocalTee(DEST),
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalGet(FIRST),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::I32Add,
            I::LocalGet(BYTES),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    pack(&mut f, BYTES, DEST);
    f.instruction(&I::End);
    f
}
/// ToIntegerOrInfinity, then clamp in floating point before integer conversion.
/// Undefined defaults have already been supplied at the typed call site.
fn bound(f: &mut Function, input: u32, output: u32, relative: bool) {
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
        ],
    );
    if relative {
        instructions(
            f,
            [
                I::LocalGet(input),
                I::F64Const(0.0.into()),
                I::F64Lt,
                I::If(BlockType::Empty),
                I::LocalGet(input),
                I::LocalGet(LENGTH),
                I::F64ConvertI32U,
                I::F64Add,
                I::LocalSet(input),
                I::End,
            ],
        );
    }
    instructions(
        f,
        [
            I::LocalGet(input),
            I::F64Const(0.0.into()),
            I::F64Max,
            I::LocalGet(LENGTH),
            I::F64ConvertI32U,
            I::F64Min,
            I::I32TruncF64U,
            I::LocalSet(output),
        ],
    );
}
