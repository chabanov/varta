use super::*;

pub(super) fn function(allocate: u32) -> Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const LEFT_LEN: u32 = 2;
    const RIGHT_LEN: u32 = 3;
    const TOTAL: u32 = 4;
    const DEST: u32 = 5;
    let mut function = Function::new([(DEST - LEFT_LEN + 1, ValType::I32)]);
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
    instructions(
        &mut function,
        [I::LocalGet(TOTAL), I::Call(allocate), I::LocalSet(DEST)],
    );
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
    pack(&mut function, TOTAL, DEST);
    function.instruction(&I::End);
    function
}

/// V38 builder growth (`string_builder`): copy both operands into a fresh
/// block whose payload capacity is twice their total length, at least
/// [`super::super::string_builder::MIN_CAPACITY`] and at most the per-value
/// limit. A total above that limit traps in the allocator, as concatenation
/// does.
pub(super) fn grow(allocate: u32) -> Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const LEFT_LEN: u32 = 2;
    const RIGHT_LEN: u32 = 3;
    const TOTAL: u32 = 4;
    const CAPACITY: u32 = 5;
    const DEST: u32 = 6;
    let mut function = Function::new([(DEST - LEFT_LEN + 1, ValType::I32)]);
    length(&mut function, LEFT, LEFT_LEN);
    length(&mut function, RIGHT, RIGHT_LEN);
    let min = super::super::string_builder::MIN_CAPACITY;
    instructions(
        &mut function,
        [
            I::LocalGet(LEFT_LEN),
            I::LocalGet(RIGHT_LEN),
            I::I32Add,
            I::LocalTee(TOTAL),
            I::LocalGet(LEFT_LEN),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            // Both lengths are within the per-value limit, so doubling the
            // total cannot overflow.
            I::LocalGet(TOTAL),
            I::I32Const(1),
            I::I32Shl,
            I::LocalTee(CAPACITY),
            I::I32Const(min),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::I32Const(min),
            I::LocalSet(CAPACITY),
            I::End,
            I::LocalGet(CAPACITY),
            I::I32Const(abi::MAX_STRING_BYTES as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::I32Const(abi::MAX_STRING_BYTES as i32),
            I::LocalSet(CAPACITY),
            I::End,
            I::LocalGet(CAPACITY),
            I::LocalGet(TOTAL),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::LocalGet(TOTAL),
            I::LocalSet(CAPACITY),
            I::End,
            I::LocalGet(CAPACITY),
            I::Call(allocate),
            I::LocalSet(DEST),
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
    pack(&mut function, TOTAL, DEST);
    function.instruction(&I::End);
    function
}
