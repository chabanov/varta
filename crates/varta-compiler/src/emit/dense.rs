//! V38 (`codegen_v2`): checked element reads (`xs[i]` and `for...of` items of
//! the element type) without a union cell.
//!
//! `lower::dense_read` reads `T | undefined` into a hidden slot, throws the
//! missing-element `TypeError` when it is `undefined` and unboxes it. For an
//! array whose element type is not itself a cell, v38 emits the same checks on
//! the index and presence bitmap and then loads the element slot directly, so
//! the read allocates nothing. The throw path builds its error with ordinary
//! roots and never falls through, so the read is quiet when its operands are:
//! an abandoned evaluation needs no operand it held.
use super::*;
use varta_common::value as values;

/// The operands of a checked element read in the shape `dense_read` lowers.
pub(super) struct DenseRead<'e> {
    pub base: &'e Expr,
    pub index: &'e Expr,
    pub error: &'e Expr,
}

/// The checked element read `expr` is, or `None` for any other shape
/// (including string reads and element types that are cells).
pub(super) fn dense_read(expr: &Expr) -> Option<DenseRead<'_>> {
    let E::ValueBlock {
        output,
        roots,
        body,
        ..
    } = &expr.kind
    else {
        return None;
    };
    let [
        S::Init(saved, maybe),
        S::If(test, then, otherwise),
        S::Init(result, converted),
    ] = body.as_slice()
    else {
        return None;
    };
    let (
        E::MaybeIndex(base, index),
        E::IsUndefined(tested),
        [S::Throw(error)],
        E::UnboxUnion(read),
    ) = (&maybe.kind, &test.kind, then.as_slice(), &converted.kind)
    else {
        return None;
    };
    let reads_saved = |value: &Expr| matches!(value.kind, E::Local(slot) if slot == *saved);
    (matches!(base.ty, Type::Array(_))
        && !expr.ty.is_cell()
        && otherwise.is_empty()
        && roots.as_slice() == [*saved]
        && result == output
        && reads_saved(tested)
        && reads_saved(read))
    .then_some(DenseRead { base, index, error })
}

impl Emitter<'_> {
    /// `[] -> [element]`: the checked read without boxing; the missing path
    /// throws the lowered error.
    pub(super) fn dense_index(&mut self, read: &DenseRead, ty: Type) -> Result<()> {
        self.expr(read.base)?;
        self.expr(read.index)?;
        self.code.extend([
            I::LocalSet(self.float_temporary),
            I::LocalTee(self.string_temporary),
            I::I32WrapI64,
            I::I32Load(wasm_encoder::MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..strings::WORD
            }),
            I::LocalSet(self.temporary),
            I::LocalGet(self.float_temporary),
            I::F64Trunc,
            I::LocalGet(self.float_temporary),
            I::F64Ne,
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::I32Or,
            I::LocalGet(self.float_temporary),
            I::LocalGet(self.temporary),
            I::F64ConvertI32U,
            I::F64Ge,
            I::I32Or,
        ]);
        if self.profile.sparse_arrays() {
            self.code.extend([
                I::If(BlockType::Result(ValType::I32)),
                I::I32Const(1),
                I::Else,
                I::LocalGet(self.string_temporary),
                I::LocalGet(self.float_temporary),
                I::I32TruncF64U,
            ]);
            self.string_helper(strings::Helper::ArrayPresent)?;
            self.code.extend([I::I32Eqz, I::End]);
        }
        self.code.push(I::If(BlockType::Empty));
        // The error allocates with ordinary roots even inside a quiet
        // expression; the throw leaves this evaluation.
        let quiet = std::mem::replace(&mut self.quiet_mode, false);
        let thrown = self.throw_value(read.error);
        self.quiet_mode = quiet;
        thrown?;
        self.code
            .extend([I::End, I::LocalGet(self.string_temporary), I::I32WrapI64]);
        if self.profile.resizable_arrays() {
            self.code
                .extend([I::I64Load(arrays::slot(0)), I::I32WrapI64]);
        }
        self.code.extend([
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Load(wasm_encoder::MemArg {
                offset: values::ARRAY_META_BYTES as u64,
                ..strings::ROOT
            }),
        ]);
        self.unpack_slot(ty);
        Ok(())
    }
}
