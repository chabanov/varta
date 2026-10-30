use super::*;

pub(super) fn function(globals: Globals) -> Function {
    const VALUE: u32 = 0;
    const TOP: u32 = 1;
    let Globals { roots, base, .. } = globals;
    let mut function = Function::new([(1, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(VALUE),
            I::I32WrapI64,
            I::I32Const(base as i32),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::LocalGet(VALUE),
            I::Return,
            I::End,
            I::GlobalGet(roots),
            I::LocalTee(TOP),
            I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
            I::I32Add,
            I::I32Const(abi::STRING_ROOT_END as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
        ],
    );
    instructions(
        &mut function,
        globals.fail(varta_common::limits::Resource::RootStack, TOP),
    );
    instructions(
        &mut function,
        [
            I::End,
            I::LocalGet(TOP),
            I::LocalGet(VALUE),
            I::I64Store(ROOT),
            I::LocalGet(TOP),
            I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
            I::I32Add,
            I::GlobalSet(roots),
            I::LocalGet(VALUE),
            I::End,
        ],
    );
    function
}
