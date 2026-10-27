use super::*;
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

pub(super) fn function(last: bool) -> Function {
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

    if last {
        instructions(
            &mut f,
            [
                I::LocalGet(POSITION),
                I::LocalGet(POSITION),
                I::F64Ne,
                I::If(BlockType::Result(ValType::F64)),
                I::F64Const(f64::INFINITY.into()),
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
            ],
        );
    } else {
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
            ],
        );
    }

    // empty needle => return FIRST
    instructions(
        &mut f,
        [
            I::LocalGet(NEEDLE_LENGTH),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::LocalGet(FIRST),
            I::Return,
            I::End,
        ],
    );

    // LAST = length - needleLen
    instructions(
        &mut f,
        [
            I::LocalGet(LENGTH),
            I::LocalGet(NEEDLE_LENGTH),
            I::I32Sub,
            I::LocalSet(LAST),
        ],
    );
    // if LAST < 0 return -1
    instructions(
        &mut f,
        [
            I::LocalGet(LAST),
            I::I32Const(0),
            I::I32LtS,
            I::If(BlockType::Empty),
            I::I32Const(-1),
            I::Return,
            I::End,
        ],
    );

    if last {
        // FIRST = min(FIRST, LAST)
        instructions(
            &mut f,
            [
                I::LocalGet(FIRST),
                I::LocalGet(LAST),
                I::I32GtU,
                I::If(BlockType::Result(ValType::I32)),
                I::LocalGet(LAST),
                I::Else,
                I::LocalGet(FIRST),
                I::End,
                I::LocalSet(FIRST),
            ],
        );
    } else {
        // if FIRST > LAST return -1
        instructions(
            &mut f,
            [
                I::LocalGet(FIRST),
                I::LocalGet(LAST),
                I::I32GtU,
                I::If(BlockType::Empty),
                I::I32Const(-1),
                I::Return,
                I::End,
            ],
        );
    }

    if last {
        instructions(
            &mut f,
            [I::Block(BlockType::Empty), I::Loop(BlockType::Empty)],
        );
        // if FIRST < 0 break
        instructions(
            &mut f,
            [I::LocalGet(FIRST), I::I32Const(0), I::I32LtS, I::BrIf(1)],
        );
    } else {
        instructions(
            &mut f,
            [I::Block(BlockType::Empty), I::Loop(BlockType::Empty)],
        );
        // if FIRST > LAST break
        instructions(
            &mut f,
            [I::LocalGet(FIRST), I::LocalGet(LAST), I::I32GtU, I::BrIf(1)],
        );
    }

    // compare needle at FIRST
    instructions(
        &mut f,
        [
            I::I32Const(0),
            I::LocalSet(OFFSET),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
        ],
    );
    instructions(
        &mut f,
        [
            I::LocalGet(OFFSET),
            I::LocalGet(NEEDLE_LENGTH),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::LocalGet(FIRST),
            I::Return,
            I::End,
        ],
    );
    instructions(
        &mut f,
        [
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
            I::BrIf(1), // mismatch block
            I::LocalGet(OFFSET),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(OFFSET),
            I::Br(0), // compare loop
            I::End,   // end compare loop
            I::End,   // end mismatch block
        ],
    );

    if last {
        instructions(
            &mut f,
            [
                I::LocalGet(FIRST),
                I::I32Const(1),
                I::I32Sub,
                I::LocalSet(FIRST),
            ],
        );
    } else {
        instructions(
            &mut f,
            [
                I::LocalGet(FIRST),
                I::I32Const(1),
                I::I32Add,
                I::LocalSet(FIRST),
            ],
        );
    }
    instructions(&mut f, [I::Br(0), I::End, I::End]);

    instructions(&mut f, [I::I32Const(-1), I::End]);
    f
}
