//! Canonical array-index keys of value15 dictionaries: the value of a UTF-16
//! String spelling an array index (`0`, or digits without a leading zero, below
//! 2^32 - 1), or -1. Pure, allocation-free and bounded by the decimal width.
use super::*;
pub(super) fn function() -> Function {
    const KEY: u32 = 0;
    const PTR: u32 = 1;
    const LEN: u32 = 2;
    const OFFSET: u32 = 3;
    const DIGIT: u32 = 4;
    const VALUE: u32 = 5;
    const UNIT: u32 = 2;
    let digits = varta_common::value::MAX_ARRAY_INDEX.to_string().len() as i32;
    let mut f = Function::new([(4, ValType::I32), (1, ValType::I64)]);
    length(&mut f, KEY, LEN);
    instructions(
        &mut f,
        [
            I::LocalGet(KEY),
            I::I32WrapI64,
            I::LocalSet(PTR),
            // Empty or wider than any array index.
            I::LocalGet(LEN),
            I::I32Eqz,
            I::LocalGet(LEN),
            I::I32Const(digits * UNIT as i32),
            I::I32GtU,
            I::I32Or,
            I::If(BlockType::Empty),
            I::F64Const((-1.0).into()),
            I::Return,
            I::End,
            // A leading zero is canonical only as the whole key.
            I::LocalGet(LEN),
            I::I32Const(UNIT as i32),
            I::I32GtU,
            I::LocalGet(PTR),
            I::I32Load16U(MemArg { align: 1, ..WORD }),
            I::I32Const('0' as i32),
            I::I32Eq,
            I::I32And,
            I::If(BlockType::Empty),
            I::F64Const((-1.0).into()),
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
            I::F64Const((-1.0).into()),
            I::Return,
            I::End,
            I::LocalGet(VALUE),
            I::I64Const(10),
            I::I64Mul,
            I::LocalGet(DIGIT),
            I::I64ExtendI32U,
            I::I64Add,
            I::LocalSet(VALUE),
            I::LocalGet(OFFSET),
            I::I32Const(UNIT as i32),
            I::I32Add,
            I::LocalTee(OFFSET),
            I::LocalGet(LEN),
            I::I32LtU,
            I::BrIf(0),
            I::End,
            I::LocalGet(VALUE),
            I::I64Const(varta_common::value::MAX_ARRAY_INDEX as i64),
            I::I64GtU,
            I::If(BlockType::Result(ValType::F64)),
            I::F64Const((-1.0).into()),
            I::Else,
            I::LocalGet(VALUE),
            I::F64ConvertI64U,
            I::End,
            I::End,
        ],
    );
    f
}
