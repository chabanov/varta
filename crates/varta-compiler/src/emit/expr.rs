use super::*;

impl Emitter<'_> {
    pub(super) fn expr(&mut self, expr: &Expr) -> Result<()> {
        let Some(memory) = self.memory else {
            return self.expr_inner(expr);
        };
        let marker = self.root_mark_base + self.expr_depth;
        self.expr_depth += 1;
        self.max_expr_depth = self.max_expr_depth.max(self.expr_depth);
        self.code
            .extend([I::GlobalGet(memory.roots), I::LocalSet(marker)]);
        self.expr_inner(expr)?;
        self.code
            .extend([I::LocalGet(marker), I::GlobalSet(memory.roots)]);
        if expr.ty.has_string() {
            if expr.ty == Type::ResultString {
                self.code.push(I::LocalSet(self.temporary));
            }
            self.string_helper(strings::Helper::RootPush)?;
            if expr.ty == Type::ResultString {
                self.code.push(I::LocalGet(self.temporary));
            }
        }
        self.expr_depth -= 1;
        Ok(())
    }
    fn expr_inner(&mut self, expr: &Expr) -> Result<()> {
        match &expr.kind {
            E::Integer(n) => self.code.push(I::I32Const(*n)),
            E::Local(slot) => {
                for offset in 0..expr.ty.width() {
                    self.code
                        .push(I::LocalGet(self.local_slots[*slot as usize] + offset));
                }
            }
            E::String(s) => {
                let packed = ((s.len() as u64) << strings::LENGTH_SHIFT) | self.strings[s] as u64;
                self.code.push(I::I64Const(packed as i64));
            }
            E::Status => self.code.push(I::GlobalGet(self.result_status)),
            E::Capture(value) => {
                self.expr(value)?;
                if value.ty == Type::Void {
                    // Keep one payload slot for every result, including void.
                    self.code.push(I::I32Const(0));
                }
                self.code.push(I::GlobalGet(self.result_status));
            }
            E::ResultField(field, value) => {
                self.expr(value)?;
                match field {
                    ir::ResultField::Value => {
                        self.code.push(I::Drop); // status
                        if expr.ty == Type::Void {
                            self.code.push(I::Drop);
                        }
                    }
                    ir::ResultField::Status | ir::ResultField::Ok => {
                        self.code.push(I::LocalSet(self.temporary));
                        self.code.push(I::Drop); // payload
                        self.code.push(I::LocalGet(self.temporary));
                        if matches!(field, ir::ResultField::Ok) {
                            self.code.push(I::I32Eqz);
                        }
                    }
                }
            }
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
                    self.string_helper(strings::Helper::Snapshot)?;
                }
            }
            E::Binary(op, left, right) if left.ty == Type::String => {
                self.expr(left)?;
                self.expr(right)?;
                if *op == BinaryOp::Add {
                    self.string_helper(strings::Helper::Concat)?;
                } else {
                    self.string_helper(strings::Helper::Compare)?;
                    self.code.push(I::I32Const(0));
                    self.code.push(match op {
                        BinaryOp::EqEq | BinaryOp::EqEqEq => I::I32Eq,
                        BinaryOp::NotEq | BinaryOp::NotEqEq => I::I32Ne,
                        BinaryOp::Lt => I::I32LtS,
                        BinaryOp::LtEq => I::I32LeS,
                        BinaryOp::Gt => I::I32GtS,
                        BinaryOp::GtEq => I::I32GeS,
                        _ => {
                            return Err(CompilerError::Internal(
                                "unlowered string operator".into(),
                            ));
                        }
                    });
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

    fn string_helper(&mut self, helper: strings::Helper) -> Result<()> {
        let index = self
            .helpers
            .get(&helper)
            .ok_or_else(|| CompilerError::Internal("missing string helper".into()))?;
        self.code.push(I::Call(*index));
        Ok(())
    }
}
