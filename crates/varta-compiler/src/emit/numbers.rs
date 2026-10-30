//! Binary64 stays in f64 slots; neither bit patterns nor NaNs become roots.
use super::*;
impl Emitter<'_> {
    pub(super) fn pack_slot(&mut self, ty: Type) {
        if ty == Type::Float64 {
            self.code.push(I::I64ReinterpretF64);
        } else if !ty.is_handle() {
            self.code.push(I::I64ExtendI32S);
        }
    }
    pub(super) fn unpack_slot(&mut self, ty: Type) {
        if ty == Type::Float64 {
            self.code.push(I::F64ReinterpretI64);
        } else if !ty.is_handle() {
            self.code.push(I::I32WrapI64);
        }
    }
    pub(super) fn index(&mut self, expr: &Expr) -> Result<()> {
        self.expr(expr)?;
        if expr.ty == Type::Float64 {
            // Checked dense arrays need an exact nonnegative bounded integer.
            self.code.extend([
                I::LocalTee(self.float_temporary),
                I::F64Trunc,
                I::LocalGet(self.float_temporary),
                I::F64Ne,
                I::LocalGet(self.float_temporary),
                I::F64Const(0.0.into()),
                I::F64Lt,
                I::I32Or,
                I::LocalGet(self.float_temporary),
                I::F64Const((varta_common::value::MAX_ARRAY_ELEMENTS as f64).into()),
                I::F64Ge,
                I::I32Or,
                I::If(BlockType::Empty),
                I::Unreachable,
                I::End,
                I::LocalGet(self.float_temporary),
                I::I32TruncF64U,
            ]);
        }
        Ok(())
    }
    pub(super) fn truthy(&mut self, ty: Type) {
        if matches!(ty, Type::Null | Type::Undefined) {
            self.code.extend([I::Drop, I::I32Const(0)]);
        }
        if ty == Type::Float64 {
            // abs(x)>0 is false for both zeroes and NaN, true for other numbers.
            self.code
                .extend([I::F64Abs, I::F64Const(0.0.into()), I::F64Gt]);
        }
    }
    pub(super) fn float_binary(&mut self, op: BinaryOp) -> Result<()> {
        if ir::is_number_bitwise(op) {
            return self.float_bitwise(op);
        }
        if op == BinaryOp::Mod && self.codegen_v2() {
            // V38: safe integers in the guest, every other operand on the host.
            return self.string_helper(strings::Helper::FloatRemainder);
        }
        self.code.push(match op {
            BinaryOp::Add => I::F64Add,
            BinaryOp::Sub => I::F64Sub,
            BinaryOp::Mul => I::F64Mul,
            BinaryOp::Div => I::F64Div,
            BinaryOp::Mod => I::Call(self.hosts[&Host::FloatRemainder]),
            BinaryOp::Exp => I::Call(self.hosts[&Host::FloatPower]),
            BinaryOp::Lt => I::F64Lt,
            BinaryOp::LtEq => I::F64Le,
            BinaryOp::Gt => I::F64Gt,
            BinaryOp::GtEq => I::F64Ge,
            BinaryOp::EqEq | BinaryOp::EqEqEq => I::F64Eq,
            BinaryOp::NotEq | BinaryOp::NotEqEq => I::F64Ne,
            _ => {
                return Err(CompilerError::Internal(
                    "unlowered binary64 operator".into(),
                ));
            }
        });
        Ok(())
    }
}
