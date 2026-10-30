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

/// V38 (`codegen_v2`): the most operands one flattened concatenation takes.
pub(in crate::emit) const MAX_OPERANDS: usize = 8;

/// V38 (`codegen_v2`): concatenate `count` evaluated operands with one
/// allocation instead of one per `+`. It behaves as the left-nested
/// two-operand helper chain: an overflowing length traps, a running total
/// above the allocation limit fails with that total (where the nested chain's
/// allocation would fail), and with at most one nonempty operand that operand
/// (or the last one) is returned without allocating.
pub(super) fn many(
    count: u8,
    allocate: u32,
    globals: Globals,
    limits: varta_common::limits::DataLimits,
) -> Function {
    use varta_common::limits::Resource;
    let operands = u32::from(count);
    let total = operands;
    let nonempty = operands + 1;
    let len = operands + 2;
    let dest = operands + 3;
    let last = operands + 4;
    let mut f = Function::new([(4, ValType::I32), (1, ValType::I64)]);
    instructions(&mut f, [I::LocalGet(operands - 1), I::LocalSet(last)]);
    for operand in 0..operands {
        length(&mut f, operand, len);
        instructions(
            &mut f,
            [
                I::LocalGet(len),
                I::If(BlockType::Empty),
                I::LocalGet(total),
                I::LocalGet(len),
                I::I32Add,
                I::LocalTee(total),
                I::LocalGet(len),
                I::I32LtU,
                I::If(BlockType::Empty),
                I::Unreachable,
                I::End,
                // The nested chain allocates once both sides are nonempty.
                I::LocalGet(total),
                I::I32Const(limits.allocation_bytes as i32),
                I::I32GtU,
                I::LocalGet(nonempty),
                I::I32Const(0),
                I::I32Ne,
                I::I32And,
                I::If(BlockType::Empty),
            ],
        );
        instructions(&mut f, globals.fail(Resource::ValueSize, total));
        instructions(
            &mut f,
            [
                I::End,
                I::LocalGet(nonempty),
                I::I32Const(1),
                I::I32Add,
                I::LocalSet(nonempty),
                I::LocalGet(operand),
                I::LocalSet(last),
                I::End,
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalGet(nonempty),
            I::I32Const(2),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::LocalGet(last),
            I::Return,
            I::End,
            I::LocalGet(total),
            I::Call(allocate),
            I::LocalTee(dest),
            // `len` now holds the end of the copied bytes.
            I::LocalSet(len),
        ],
    );
    for operand in 0..operands {
        instructions(
            &mut f,
            [
                I::LocalGet(len),
                I::LocalGet(operand),
                I::I32WrapI64,
                I::LocalGet(operand),
                I::I64Const(LENGTH_SHIFT as i64),
                I::I64ShrU,
                I::I32WrapI64,
                I::LocalTee(nonempty),
                I::MemoryCopy {
                    src_mem: MEMORY,
                    dst_mem: MEMORY,
                },
                I::LocalGet(len),
                I::LocalGet(nonempty),
                I::I32Add,
                I::LocalSet(len),
            ],
        );
    }
    pack(&mut f, total, dest);
    f.instruction(&I::End);
    f
}
