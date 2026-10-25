use super::*;

impl Emitter<'_> {
    pub(super) fn expr(&mut self, expr: &Expr) -> Result<()> {
        match &expr.kind {
            E::Integer(n) => self.code.push(I::I32Const(*n)),
            E::Local(slot) => self.code.push(I::LocalGet(*slot)),
            E::String(s) => {
                let packed = ((s.len() as u64) << strings::LENGTH_SHIFT) | self.strings[s] as u64;
                self.code.push(I::I64Const(packed as i64));
            }
            E::Status => self.code.push(I::GlobalGet(self.result_status)),
            E::Negative(e) => {
                self.code.push(I::I32Const(0));
                self.expr(e)?;
                self.code.push(I::I32Sub);
            }
            E::Not(e) => {
                self.expr(e)?;
                self.code.push(I::I32Eqz);
            }
            E::Call(index, args) => {
                for arg in args {
                    self.expr(arg)?;
                }
                self.code
                    .push(I::Call(self.functions_begin + *index as u32));
            }
            E::Host(host, args) => {
                for arg in args {
                    self.expr(arg)?;
                    if arg.ty == Type::String {
                        // The internal packed value becomes the existing host ptr/len ABI.
                        self.code.push(I::LocalTee(self.string_temporary));
                        self.code.push(I::I32WrapI64);
                        self.code.push(I::LocalGet(self.string_temporary));
                        self.code.push(I::I64Const(strings::LENGTH_SHIFT as i64));
                        self.code.push(I::I64ShrU);
                        self.code.push(I::I32WrapI64);
                    }
                }
                self.code.push(I::Call(self.hosts[host]));
                if expr.ty == Type::String {
                    self.code.push(I::GlobalGet(self.result_len));
                    let snapshot = self.snapshot.ok_or_else(|| {
                        CompilerError::Internal("missing string snapshot helper".into())
                    })?;
                    self.code.push(I::Call(snapshot));
                }
            }
            E::Binary(op, left, right)
                if matches!(op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) =>
            {
                self.expr(left)?;
                self.code.push(I::LocalTee(self.temporary));
                self.code.push(I::If(BlockType::Result(ValType::I32)));
                if *op == BinaryOp::LogicalAnd {
                    self.expr(right)?;
                } else {
                    self.code.push(I::LocalGet(self.temporary));
                }
                self.code.push(I::Else);
                if *op == BinaryOp::LogicalOr {
                    self.expr(right)?;
                } else {
                    self.code.push(I::LocalGet(self.temporary));
                }
                self.code.push(I::End);
            }
            E::Binary(op, a, b) => {
                self.expr(a)?;
                self.expr(b)?;
                self.code.push(match op {
                    BinaryOp::Add => I::I32Add,
                    BinaryOp::Sub => I::I32Sub,
                    BinaryOp::Mul => I::I32Mul,
                    BinaryOp::Div => I::I32DivS,
                    BinaryOp::Mod => I::I32RemS,
                    BinaryOp::Lt => I::I32LtS,
                    BinaryOp::LtEq => I::I32LeS,
                    BinaryOp::Gt => I::I32GtS,
                    BinaryOp::GtEq => I::I32GeS,
                    BinaryOp::EqEq | BinaryOp::EqEqEq => I::I32Eq,
                    BinaryOp::NotEq | BinaryOp::NotEqEq => I::I32Ne,
                    _ => return Err(CompilerError::Internal("unlowered binary operator".into())),
                });
            }
        }
        Ok(())
    }
}
