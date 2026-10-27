//! One bounded allocation and logarithmically many bulk copies. Source is rooted
//! throughout count evaluation/allocation; no allocation occurs during copying.
use super::*;
impl Emitter<'_> {
    pub(super) fn string_repeat(
        &mut self,
        value: &Expr,
        count: &Expr,
        error: &Expr,
        length: u32,
        filled: u32,
    ) -> Result<()> {
        let length = self.local_slots[length as usize];
        let filled = self.local_slots[filled as usize];
        self.expr(value)?;
        self.array_bound(count, 0.0)?;
        self.code.extend([
            I::LocalSet(self.float_temporary),
            I::LocalSet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::LocalGet(self.float_temporary),
            I::F64Ne,
            I::If(BlockType::Empty),
            I::F64Const(0.0.into()),
            I::LocalSet(self.float_temporary),
            I::End,
            I::LocalGet(self.float_temporary),
            I::F64Trunc,
            I::LocalSet(self.float_temporary),
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::LocalGet(self.float_temporary),
            I::F64Const(f64::INFINITY.into()),
            I::F64Eq,
            I::I32Or,
            I::If(BlockType::Empty),
        ]);
        self.throw_value(error)?;
        self.code.extend([
            I::End,
            I::LocalGet(self.string_temporary),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalTee(filled),
            I::I32Eqz,
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Eq,
            I::I32Or,
            I::If(BlockType::Result(ValType::I64)),
            I::I64Const(0),
            I::Else,
            I::LocalGet(filled),
            I::F64ConvertI32U,
            I::LocalGet(self.float_temporary),
            I::F64Mul,
            I::LocalTee(self.float_temporary),
            I::F64Const((varta_common::abi::MAX_STRING_BYTES as f64).into()),
            I::F64Gt,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
            I::LocalTee(length),
        ]);
        self.string_helper(strings::Helper::Allocate)?;
        self.code.extend([
            I::LocalTee(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::LocalGet(filled),
            I::MemoryCopy {
                src_mem: strings::MEMORY,
                dst_mem: strings::MEMORY,
            },
        ]);
        let done = self.push_label(I::Block(BlockType::Empty), false, false);
        let again = self.push_label(I::Loop(BlockType::Empty), false, false);
        self.code.extend([
            I::LocalGet(filled),
            I::LocalGet(length),
            I::I32GeU,
            I::BrIf(self.depth(done)),
            I::LocalGet(self.temporary),
            I::LocalGet(filled),
            I::I32Add,
            I::LocalGet(self.temporary),
            I::LocalGet(filled),
            I::LocalGet(length),
            I::LocalGet(filled),
            I::I32Sub,
            I::LocalGet(filled),
            I::LocalGet(length),
            I::LocalGet(filled),
            I::I32Sub,
            I::I32LtU,
            I::Select,
            I::MemoryCopy {
                src_mem: strings::MEMORY,
                dst_mem: strings::MEMORY,
            },
            I::LocalGet(filled),
            I::I32Const(1),
            I::I32Shl,
            I::LocalGet(length),
            I::LocalGet(filled),
            I::I32Const(1),
            I::I32Shl,
            I::LocalGet(length),
            I::I32LtU,
            I::Select,
            I::LocalSet(filled),
            I::Br(self.depth(again)),
        ]);
        self.end();
        self.end();
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I64ExtendI32U,
            I::LocalGet(length),
            I::I64ExtendI32U,
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64Shl,
            I::I64Or,
            I::End,
        ]);
        Ok(())
    }
}
