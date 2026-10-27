//! Dense-array indexing is checked before calculating the element address.
use super::*;
use varta_common::value::{ARRAY_LENGTH_OFFSET, ARRAY_META_BYTES, ARRAY_SLOT_BYTES};

pub(super) fn function(resizable: bool) -> Function {
    const HANDLE: u32 = 0;
    const INDEX: u32 = 1;
    const PTR: u32 = 2;
    let mut function = Function::new([(1, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(HANDLE),
            I::I32WrapI64,
            I::LocalSet(PTR),
            I::LocalGet(INDEX),
            I::LocalGet(PTR),
            I::I32Load(MemArg {
                offset: ARRAY_LENGTH_OFFSET,
                ..WORD
            }),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
        ],
    );
    if resizable {
        instructions(
            &mut function,
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
        &mut function,
        [
            I::LocalGet(PTR),
            I::I32Const(ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::End,
        ],
    );
    function
}
