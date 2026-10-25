use super::*;

pub(super) fn function(arena: u32) -> Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const LEFT_LEN: u32 = 2;
    const RIGHT_LEN: u32 = 3;
    const TOTAL: u32 = 4;
    const DEST: u32 = 5;
    const END: u32 = 6;
    let mut function = Function::new([(END - LEFT_LEN + 1, ValType::I32)]);
    length(&mut function, LEFT, LEFT_LEN);
    length(&mut function, RIGHT, RIGHT_LEN);
    // Both operands have already been evaluated. Immutable empty concatenation
    // can share the other operand without allocating or modifying its bytes.
    instructions(
        &mut function,
        [
            I::LocalGet(LEFT_LEN),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::LocalGet(RIGHT),
            I::Return,
            I::End,
            I::LocalGet(RIGHT_LEN),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::LocalGet(LEFT),
            I::Return,
            I::End,
            I::LocalGet(LEFT_LEN),
            I::LocalGet(RIGHT_LEN),
            I::I32Add,
            I::LocalTee(TOTAL),
            I::LocalGet(LEFT_LEN),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
        ],
    );
    reserve(&mut function, arena, TOTAL, DEST, END);
    instructions(
        &mut function,
        [
            I::LocalGet(DEST),
            I::LocalGet(LEFT),
            I::I32WrapI64,
            I::LocalGet(LEFT_LEN),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
            I::LocalGet(DEST),
            I::LocalGet(LEFT_LEN),
            I::I32Add,
            I::LocalGet(RIGHT),
            I::I32WrapI64,
            I::LocalGet(RIGHT_LEN),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    commit(&mut function, arena, TOTAL, DEST, END);
    function.instruction(&I::End);
    function
}
