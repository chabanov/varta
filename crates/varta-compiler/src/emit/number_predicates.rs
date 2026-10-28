//! Pure type-aware predicates return canonical booleans without allocation.
use super::*;

const MAX_SAFE_INTEGER: f64 = 9007199254740991.0;

impl Emitter<'_> {
    pub(super) fn number_predicate(
        &mut self,
        mode: ir::NumberPredicate,
        args: &[Expr],
    ) -> Result<()> {
        let Some((first, rest)) = args.split_first() else {
            self.code.push(I::I32Const(0));
            return Ok(());
        };
        self.expr(first)?;
        if first.ty == Type::Float64 {
            self.float_predicate(mode);
        } else if first.ty.is_union() {
            self.code.extend([
                I::I32WrapI64,
                I::LocalTee(self.temporary),
                I::I32Load(strings::WORD),
                I::I32Const(varta_common::value::ValueKind::Float64 as i32),
                I::I32Eq,
                I::If(BlockType::Result(ValType::I32)),
                I::LocalGet(self.temporary),
                I::I64Load(records::field(0)),
                I::F64ReinterpretI64,
            ]);
            self.float_predicate(mode);
            self.code.extend([I::Else, I::I32Const(0), I::End]);
        } else {
            for _ in 0..first.ty.width() {
                self.code.push(I::Drop);
            }
            self.code.push(I::I32Const(0));
        }
        // The predicate depends only on the immutable first Number/type. Keep
        // its boolean on the Wasm stack while all ignored arguments execute;
        // they may allocate, overwrite scratch, throw or return physical pairs.
        for arg in rest {
            self.expr(arg)?;
            for _ in 0..arg.ty.width() {
                self.code.push(I::Drop);
            }
        }
        Ok(())
    }
    fn float_predicate(&mut self, mode: ir::NumberPredicate) {
        self.code.push(I::LocalTee(self.float_temporary));
        if mode == ir::NumberPredicate::Nan {
            self.code
                .extend([I::LocalGet(self.float_temporary), I::F64Ne]);
            return;
        }
        self.code.push(I::F64Abs);
        if mode == ir::NumberPredicate::SafeInteger {
            self.code
                .extend([I::F64Const(MAX_SAFE_INTEGER.into()), I::F64Le]);
        } else {
            self.code
                .extend([I::F64Const(f64::INFINITY.into()), I::F64Lt]);
        }
        if mode != ir::NumberPredicate::Finite {
            self.code.extend([
                I::LocalGet(self.float_temporary),
                I::F64Trunc,
                I::LocalGet(self.float_temporary),
                I::F64Eq,
                I::I32And,
            ]);
        }
    }
}
