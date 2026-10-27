use super::*;
use varta_common::value as values;
impl Emitter<'_> {
    pub(super) fn is_union_member(&mut self, value: &Expr, member: Type) -> Result<()> {
        if !value.ty.is_union() {
            return Err(CompilerError::Internal(
                "member check requires a union".into(),
            ));
        }
        let tag = member
            .value_kind()
            .ok_or_else(|| CompilerError::Internal("member check requires a value type".into()))?;
        self.expr(value)?;
        self.code.extend([
            I::I32WrapI64,
            I::LocalTee(self.temporary),
            I::I32Load(strings::WORD),
            I::I32Const(tag as i32),
            I::I32Eq,
        ]);
        if let Some(class) = member.error_class() {
            // All Error families share a union tag. Only after that tag matches
            // may we dereference the Error payload and test its exact class.
            self.code.extend([
                I::If(BlockType::Result(ValType::I32)),
                I::LocalGet(self.temporary),
                I::I64Load(records::field(0)),
                I::I32WrapI64,
                I::I32Load(strings::WORD),
                I::I32Const(values::ERROR_CLASS_SHIFT as i32),
                I::I32ShrU,
                I::I32Const(class as i32),
                I::I32Eq,
                I::Else,
                I::I32Const(0),
                I::End,
            ]);
        }
        Ok(())
    }
    /// The value's references remain rooted while allocation can collect.
    pub(super) fn box_union(&mut self, ty: Type) -> Result<()> {
        if ty.needs_root() {
            self.string_helper(strings::Helper::RootPush)?;
        }
        self.pack_slot(ty);
        self.code.extend([
            I::LocalSet(self.string_temporary),
            I::I32Const(values::UNION_BYTES as i32),
        ]);
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        let tag = ty
            .value_kind()
            .ok_or_else(|| CompilerError::Internal("missing union member tag".into()))?;
        self.initialize_value(
            strings::ALLOCATED | values::HEAP_UNION_FLAG,
            i32::from(ty.needs_root()),
            tag as i32,
            1,
        );
        self.code.extend([
            I::LocalGet(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I64Store(records::field(0)),
        ]);
        self.pack_value(values::UNION_BYTES);
        Ok(())
    }
    pub(super) fn unbox_union(&mut self, value: &Expr, ty: Type) -> Result<()> {
        self.expr(value)?;
        self.code.extend([
            I::I32WrapI64,
            I::LocalTee(self.temporary),
            I::I32Load(strings::WORD),
            I::I32Const(
                ty.value_kind()
                    .ok_or_else(|| CompilerError::Internal("missing narrowed tag".into()))?
                    as i32,
            ),
            I::I32Ne,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(self.temporary),
            I::I64Load(records::field(0)),
        ]);
        self.unpack_slot(ty);
        Ok(())
    }
    pub(super) fn union_logical(&mut self, op: BinaryOp, a: &Expr, b: &Expr) -> Result<()> {
        self.expr(a)?;
        self.code.push(I::LocalTee(self.string_temporary));
        let nullish = op == BinaryOp::NullishCoalescing;
        self.code.push(I::Call(
            self.hosts[&self.profile.host_operation(if nullish {
                Host::UnionNullish
            } else {
                Host::UnionTruthy
            })],
        ));
        self.code.push(I::If(BlockType::Result(ValType::I64)));
        if nullish || op == BinaryOp::LogicalAnd {
            self.expr(b)?;
        } else {
            self.code.push(I::LocalGet(self.string_temporary));
        }
        self.code.push(I::Else);
        if op == BinaryOp::LogicalOr {
            self.expr(b)?;
        } else {
            self.code.push(I::LocalGet(self.string_temporary));
        }
        self.code.push(I::End);
        Ok(())
    }
}
