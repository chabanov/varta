//! Pure binary64 intrinsics use guest instructions and no host allowance.
use super::*;
impl Emitter<'_> {
    pub(super) fn math_unary(&mut self, mode: ir::MathUnary, value: &Expr) -> Result<()> {
        self.expr(value)?;
        self.code.push(match mode {
            ir::MathUnary::Abs => I::F64Abs,
            ir::MathUnary::Floor => I::F64Floor,
            ir::MathUnary::Ceil => I::F64Ceil,
            ir::MathUnary::Trunc => I::F64Trunc,
            ir::MathUnary::Sqrt => I::F64Sqrt,
            ir::MathUnary::Round => {
                // Nearest already preserves -0, nonfinite values and integral
                // numbers. Adjust only halfway cases rounded down to even;
                // floor(x + .5) loses -0 and rounds some non-ties incorrectly.
                self.code.extend([
                    I::LocalTee(self.float_temporary),
                    I::LocalGet(self.float_temporary),
                    I::F64Nearest,
                    I::F64Sub,
                    I::F64Const(0.5.into()),
                    I::F64Eq,
                    I::If(BlockType::Result(ValType::F64)),
                    I::LocalGet(self.float_temporary),
                    I::F64Nearest,
                    I::F64Const(1.0.into()),
                    I::F64Add,
                    I::Else,
                    I::LocalGet(self.float_temporary),
                    I::F64Nearest,
                    I::End,
                ]);
                return Ok(());
            }
        });
        Ok(())
    }
}
