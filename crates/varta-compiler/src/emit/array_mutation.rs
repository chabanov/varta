//! Array writes retain receiver/key/RHS until checks and allocation complete.
use super::*;
use varta_common::value as values;
impl Emitter<'_> {
    pub(super) fn array_set_index(
        &mut self,
        array: &Expr,
        index: &Expr,
        value: &Expr,
        slots: Option<(u32, u32)>,
    ) -> Result<()> {
        let (base, key) =
            slots.ok_or_else(|| CompilerError::Internal("missing extending array slots".into()))?;
        self.expr(array)?;
        self.expr(index)?;
        self.expr(value)?;
        self.pack_slot(value.ty);
        self.code.push(I::LocalSet(self.string_temporary));
        self.scratch_set(key, Type::Float64);
        self.scratch_set(base, array.ty);
        // Numeric property validation happens after the RHS. All three operands
        // already have their expression roots, including a detached RHS child.
        self.index(&Expr {
            span: index.span,
            ty: Type::Float64,
            kind: E::Local(key),
        })?;
        self.code.push(I::LocalSet(self.temporary));
        self.code.push(I::LocalGet(self.temporary));
        self.scratch_get(base, array.ty);
        self.code.extend([
            I::I32WrapI64,
            I::I32Load(wasm_encoder::MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..strings::WORD
            }),
            I::I32GeU,
            I::If(BlockType::Empty),
        ]);
        self.scratch_get(base, array.ty);
        self.code
            .extend([I::LocalGet(self.temporary), I::I32Const(1), I::I32Add]);
        self.string_helper(strings::Helper::ArrayResize)?;
        self.code.push(I::End);
        self.scratch_get(base, array.ty);
        self.code.push(I::LocalGet(self.temporary));
        self.string_helper(strings::Helper::ArrayIndex)?;
        self.code.extend([
            I::LocalGet(self.string_temporary),
            I::I64Store(strings::ROOT),
        ]);
        self.scratch_get(base, array.ty);
        self.code
            .extend([I::LocalGet(self.temporary), I::I32Const(1)]);
        self.string_helper(strings::Helper::ArraySetPresent)?;
        Ok(())
    }
    pub(super) fn array_length(&mut self, array: &Expr, length: &Expr, error: &Expr) -> Result<()> {
        self.expr(array)?;
        self.expr(length)?;
        self.code.extend([
            I::LocalSet(self.float_temporary),
            I::LocalSet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::F64Trunc,
            I::LocalGet(self.float_temporary),
            I::F64Ne,
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::I32Or,
            I::LocalGet(self.float_temporary),
            I::F64Const((u32::MAX as f64).into()),
            I::F64Gt,
            I::I32Or,
            I::If(BlockType::Empty),
        ]);
        self.throw_value(error)?;
        self.code.extend([
            I::End,
            I::LocalGet(self.float_temporary),
            I::F64Const((values::MAX_ARRAY_ELEMENTS as f64).into()),
            I::F64Gt,
        ]);
        self.trap_if();
        self.code.extend([
            I::LocalGet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
        ]);
        self.string_helper(strings::Helper::ArrayResize)?;
        Ok(())
    }
    pub(super) fn array_delete(
        &mut self,
        array: &Expr,
        index: Option<&Expr>,
        error: Option<&Expr>,
    ) -> Result<()> {
        self.expr(array)?;
        if let Some(error) = error {
            self.code.push(I::Drop);
            self.throw_value(error)?;
            self.code.push(I::I32Const(0));
            return Ok(());
        }
        let Some(index) = index else {
            self.code.extend([I::Drop, I::I32Const(1)]);
            return Ok(());
        };
        self.expr(index)?;
        self.code.extend([
            I::LocalSet(self.float_temporary),
            I::LocalSet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::F64Trunc,
            I::LocalGet(self.float_temporary),
            I::F64Ne,
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::I32Or,
            I::LocalGet(self.float_temporary),
            I::F64Const((values::MAX_ARRAY_ELEMENTS as f64).into()),
            I::F64Ge,
            I::I32Or,
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::LocalGet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
        ]);
        self.string_helper(strings::Helper::ArrayPresent)?;
        self.code.extend([
            I::If(BlockType::Empty),
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::I64Load(wasm_encoder::MemArg {
                offset: values::VECTOR_STORAGE_OFFSET,
                ..strings::ROOT
            }),
            I::I32WrapI64,
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Const(0),
            I::I64Store(wasm_encoder::MemArg {
                offset: values::ARRAY_META_BYTES as u64,
                ..strings::ROOT
            }),
            I::LocalGet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
            I::I32Const(0),
        ]);
        self.string_helper(strings::Helper::ArraySetPresent)?;
        self.code.extend([I::End, I::End, I::I32Const(1)]);
        Ok(())
    }
}
