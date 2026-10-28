//! Checked Math uses guest instructions or resolved typed host operations.
use super::*;
const BINADE_EXPONENT_MASK: i64 = 0x7ff0_0000_0000_0000;
impl Emitter<'_> {
    pub(super) fn math_intrinsic(&mut self, mode: ir::MathIntrinsic, args: &[Expr]) -> Result<()> {
        if mode == ir::MathIntrinsic::Random {
            // Ignored arguments still evaluate, including physical pair values.
            for arg in args {
                self.expr(arg)?;
                for _ in 0..arg.ty.width() {
                    self.code.push(I::Drop);
                }
            }
            self.code.push(I::Call(self.hosts[&Host::MathRandom]));
            return Ok(());
        }
        if matches!(mode, ir::MathIntrinsic::Min | ir::MathIntrinsic::Max) {
            let maximum = mode == ir::MathIntrinsic::Max;
            self.code.push(I::F64Const(if maximum {
                f64::NEG_INFINITY.into()
            } else {
                f64::INFINITY.into()
            }));
            // Fold without early return on NaN: every argument still executes.
            // The accumulator stays on the Wasm stack across nested calls/GC.
            for arg in args {
                self.expr(arg)?;
                self.code.push(if maximum { I::F64Max } else { I::F64Min });
            }
            return Ok(());
        }
        for arg in args {
            self.expr(arg)?;
        }
        let instruction = match mode {
            ir::MathIntrinsic::Abs => I::F64Abs,
            ir::MathIntrinsic::Floor => I::F64Floor,
            ir::MathIntrinsic::Ceil => I::F64Ceil,
            ir::MathIntrinsic::Trunc => I::F64Trunc,
            ir::MathIntrinsic::Sqrt => I::F64Sqrt,
            ir::MathIntrinsic::Pow => I::Call(self.hosts[&Host::FloatPower]),
            ir::MathIntrinsic::Native(operation) => I::Call(self.hosts[&operation.host()]),
            ir::MathIntrinsic::BinadeScale => {
                self.code.extend([
                    I::I64ReinterpretF64,
                    I::I64Const(BINADE_EXPONENT_MASK),
                    I::I64And,
                    I::F64ReinterpretI64,
                    I::F64Const(f64::MIN_POSITIVE.into()),
                ]);
                I::F64Max
            }
            ir::MathIntrinsic::F16round => {
                self.float16_round();
                return Ok(());
            }
            ir::MathIntrinsic::Fround => {
                self.code.push(I::F32DemoteF64);
                I::F64PromoteF32
            }
            ir::MathIntrinsic::Clz32 => {
                self.number_to_uint32();
                self.code.push(I::I32Clz);
                I::F64ConvertI32U
            }
            ir::MathIntrinsic::Imul => {
                // Both arguments have finished, so scratch reuse cannot affect
                // nested argument computations or alter their source order.
                self.code.push(I::LocalSet(self.float_temporary));
                self.number_to_uint32();
                self.code.push(I::LocalGet(self.float_temporary));
                self.number_to_uint32();
                self.code.push(I::I32Mul);
                I::F64ConvertI32S
            }
            ir::MathIntrinsic::Sign => {
                self.code.extend([
                    I::LocalTee(self.float_temporary),
                    I::F64Abs,
                    I::F64Const(0.0.into()),
                    I::F64Gt,
                    I::If(BlockType::Result(ValType::F64)),
                    I::F64Const(1.0.into()),
                    I::LocalGet(self.float_temporary),
                    I::F64Copysign,
                    I::Else,
                    I::LocalGet(self.float_temporary),
                    I::End,
                ]);
                return Ok(());
            }
            ir::MathIntrinsic::Min | ir::MathIntrinsic::Max | ir::MathIntrinsic::Random => {
                return Err(CompilerError::Internal("unlowered Math extremum".into()));
            }
            ir::MathIntrinsic::Round => {
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
        };
        self.code.push(instruction);
        Ok(())
    }
}
