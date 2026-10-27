//! Owned UTF-16 trimming. Removes leading/trailing ECMAScript WhiteSpace
//! and LineTerminator code units. Lone surrogates and NUL are
//! preserved because neither is in the whitespace set.
use super::*;
const INPUT: u32 = 0;
const START: u32 = 1;
const END: u32 = 2;
const LENGTH: u32 = 3;
const CHAR: u32 = 4;
const BYTES: u32 = 5;
const DEST: u32 = 6;
const WS: u32 = 7;
const UNIT_BYTES: i32 = size_of::<u16>() as i32;

pub(super) fn function(allocate: u32, trim_start: bool, trim_end: bool) -> Function {
    let mut f = Function::new([(WS - START + 1, ValType::I32)]);
    // byte length -> code-unit length.
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
    instructions(&mut f, [I::I32Const(0), I::LocalSet(START)]);
    instructions(&mut f, [I::LocalGet(LENGTH), I::LocalSet(END)]);

    if trim_start {
        instructions(
            &mut f,
            [
                I::Block(BlockType::Empty),
                I::Loop(BlockType::Empty),
                I::LocalGet(START),
                I::LocalGet(LENGTH),
                I::I32GeU,
                I::BrIf(1),
                I::LocalGet(INPUT),
                I::I32WrapI64,
                I::LocalGet(START),
                I::I32Const(UNIT_BYTES),
                I::I32Mul,
                I::I32Add,
                I::I32Load16U(MemArg {
                    offset: 0,
                    align: 1,
                    memory_index: MEMORY,
                }),
                I::LocalSet(CHAR),
            ],
        );
        whitespace_check(&mut f);
        instructions(
            &mut f,
            [
                I::LocalGet(WS),
                I::If(BlockType::Empty),
                I::LocalGet(START),
                I::I32Const(1),
                I::I32Add,
                I::LocalSet(START),
                I::Br(1),
                I::Else,
                I::Br(2),
                I::End,
                I::End,
                I::End,
            ],
        );
    }

    if trim_end {
        instructions(
            &mut f,
            [
                I::Block(BlockType::Empty),
                I::Loop(BlockType::Empty),
                I::LocalGet(END),
                I::I32Eqz,
                I::BrIf(1),
                I::LocalGet(INPUT),
                I::I32WrapI64,
                I::LocalGet(END),
                I::I32Const(1),
                I::I32Sub,
                I::I32Const(UNIT_BYTES),
                I::I32Mul,
                I::I32Add,
                I::I32Load16U(MemArg {
                    offset: 0,
                    align: 1,
                    memory_index: MEMORY,
                }),
                I::LocalSet(CHAR),
            ],
        );
        whitespace_check(&mut f);
        instructions(
            &mut f,
            [
                I::LocalGet(WS),
                I::If(BlockType::Empty),
                I::LocalGet(END),
                I::I32Const(1),
                I::I32Sub,
                I::LocalSet(END),
                I::Br(1),
                I::Else,
                I::Br(2),
                I::End,
                I::End,
                I::End,
            ],
        );
    }

    instructions(
        &mut f,
        [
            I::LocalGet(START),
            I::LocalGet(END),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::I64Const(0),
            I::Return,
            I::End,
            I::LocalGet(END),
            I::LocalGet(START),
            I::I32Sub,
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalTee(BYTES),
            I::Call(allocate),
            I::LocalTee(DEST),
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalGet(START),
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

fn whitespace_check(f: &mut Function) {
    // WS = whether CHAR is JavaScript TrimWhitespace.
    instructions(f, [I::I32Const(0), I::LocalSet(WS)]);
    // U+0009 .. U+000D
    instructions(
        f,
        [
            I::LocalGet(CHAR),
            I::I32Const(0x0009),
            I::I32GeU,
            I::LocalGet(CHAR),
            I::I32Const(0x000D),
            I::I32LeU,
            I::I32And,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::LocalSet(WS),
            I::End,
        ],
    );
    for value in [0x0020u32, 0x00A0, 0x1680, 0x202F, 0x205F, 0x3000, 0xFEFF] {
        instructions(
            f,
            [
                I::LocalGet(CHAR),
                I::I32Const(value as i32),
                I::I32Eq,
                I::If(BlockType::Empty),
                I::I32Const(1),
                I::LocalSet(WS),
                I::End,
            ],
        );
    }
    // U+2000 .. U+200A
    instructions(
        f,
        [
            I::LocalGet(CHAR),
            I::I32Const(0x2000),
            I::I32GeU,
            I::LocalGet(CHAR),
            I::I32Const(0x200A),
            I::I32LeU,
            I::I32And,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::LocalSet(WS),
            I::End,
        ],
    );
    // U+2028, U+2029
    for value in [0x2028u32, 0x2029] {
        instructions(
            f,
            [
                I::LocalGet(CHAR),
                I::I32Const(value as i32),
                I::I32Eq,
                I::If(BlockType::Empty),
                I::I32Const(1),
                I::LocalSet(WS),
                I::End,
            ],
        );
    }
}
