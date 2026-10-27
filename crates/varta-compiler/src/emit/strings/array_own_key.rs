//! Exact UTF-16 own keys over bounded sparse arrays, without coercion or allocation.
use super::*;
pub(super) fn function(present: u32) -> Function {
    const ARRAY: u32 = 0;
    const KEY: u32 = 1;
    const PTR: u32 = 2;
    const LEN: u32 = 3;
    const OFFSET: u32 = 4;
    const DIGIT: u32 = 5;
    const INDEX: u32 = 6;
    let max_key_bytes = ((varta_common::value::MAX_ARRAY_ELEMENTS - 1)
        .to_string()
        .len()
        * 2) as i32;
    let mut f = Function::new([(5, ValType::I32)]);
    length(&mut f, KEY, LEN);
    instructions(
        &mut f,
        [
            I::LocalGet(KEY),
            I::I32WrapI64,
            I::LocalSet(PTR),
            I::LocalGet(LEN),
            I::I32Const(12),
            I::I32Eq,
            I::If(BlockType::Empty),
        ],
    );
    for (offset, unit) in "length".encode_utf16().enumerate() {
        instructions(
            &mut f,
            [
                I::LocalGet(PTR),
                I::I32Load16U(MemArg {
                    offset: (offset * 2) as u64,
                    align: 1,
                    memory_index: MEMORY,
                }),
                I::I32Const(unit as i32),
                I::I32Eq,
            ],
        );
        if offset != 0 {
            f.instruction(&I::I32And);
        }
    }
    instructions(
        &mut f,
        [
            I::Return,
            I::End,
            I::LocalGet(LEN),
            I::I32Eqz,
            // Reject names longer than any index of this bounded vector.
            I::LocalGet(LEN),
            I::I32Const(max_key_bytes),
            I::I32GtU,
            I::I32Or,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
            I::LocalGet(LEN),
            I::I32Const(2),
            I::I32GtU,
            I::LocalGet(PTR),
            I::I32Load16U(MemArg { align: 1, ..WORD }),
            I::I32Const('0' as i32),
            I::I32Eq,
            I::I32And,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
            I::Loop(BlockType::Empty),
            I::LocalGet(PTR),
            I::LocalGet(OFFSET),
            I::I32Add,
            I::I32Load16U(MemArg { align: 1, ..WORD }),
            I::I32Const('0' as i32),
            I::I32Sub,
            I::LocalTee(DIGIT),
            I::I32Const(9),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
            I::LocalGet(INDEX),
            I::I32Const(10),
            I::I32Mul,
            I::LocalGet(DIGIT),
            I::I32Add,
            I::LocalSet(INDEX),
            I::LocalGet(OFFSET),
            I::I32Const(2),
            I::I32Add,
            I::LocalTee(OFFSET),
            I::LocalGet(LEN),
            I::I32LtU,
            I::BrIf(0),
            I::End,
            I::LocalGet(ARRAY),
            I::LocalGet(INDEX),
            I::Call(present),
            I::End,
        ],
    );
    f
}
