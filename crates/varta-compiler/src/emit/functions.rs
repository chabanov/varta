//! Function identities and checked closed dispatch using direct calls.
//! Consequently managed helper effect analysis needs no indirect-call exemption.
use super::*;
use varta_common::{abi, function as layout};
impl Emitter<'_> {
    pub(super) fn function_reference(
        &mut self,
        index: usize,
        fresh: bool,
        captures: &[u32],
        ty: Type,
    ) -> Result<()> {
        let Type::Function(signature) = ty else {
            return Err(CompilerError::Internal("missing function signature".into()));
        };
        let cache =
            if fresh {
                None
            } else {
                Some(*self.canonical_functions.get(&index).ok_or_else(|| {
                    CompilerError::Internal("missing canonical function root".into())
                })?)
            };
        if let Some(cache) = cache {
            self.code.extend([
                I::I32Const(cache as i32),
                I::I64Load(strings::ROOT),
                I::LocalTee(self.string_temporary),
                I::I64Eqz,
                I::If(BlockType::Result(ValType::I64)),
            ]);
        }
        if self.profile.closures() {
            self.environment(captures)?;
        }
        let bytes = if self.profile.closures() {
            layout::CLOSURE_BYTES
        } else {
            layout::CELL_BYTES
        };
        self.code.push(I::I32Const(bytes as i32));
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        self.initialize_value(
            strings::ALLOCATED | layout::HEAP_FLAG,
            if captures.is_empty() {
                0
            } else {
                layout::ENVIRONMENT_MASK
            },
            signature as i32,
            if self.profile.closures() {
                layout::FUNCTION_FIELDS
            } else {
                0
            },
        );
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I64Const(index as i64),
            I::I64Store(records::field(0)),
        ]);
        if self.profile.closures() {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::LocalGet(self.string_temporary),
                I::I64Store(wasm_encoder::MemArg {
                    offset: layout::ENVIRONMENT_OFFSET,
                    ..strings::ROOT
                }),
            ]);
        }
        self.pack_value(bytes);
        if let Some(cache) = cache {
            self.code.extend([
                I::LocalSet(self.string_temporary),
                I::I32Const(cache as i32),
                I::LocalGet(self.string_temporary),
                I::I64Store(strings::ROOT),
                I::LocalGet(self.string_temporary),
                I::Else,
                I::LocalGet(self.string_temporary),
                I::End,
            ]);
        }
        Ok(())
    }
    pub(super) fn scratch_set(&mut self, slot: u32, ty: Type) {
        for offset in (0..ty.width()).rev() {
            self.code
                .push(I::LocalSet(self.local_slots[slot as usize] + offset));
        }
        self.reset_builder(slot);
    }
    pub(super) fn scratch_get(&mut self, slot: u32, ty: Type) {
        for offset in 0..ty.width() {
            self.code
                .push(I::LocalGet(self.local_slots[slot as usize] + offset));
        }
    }
    pub(super) fn function_guard(&mut self, slot: u32, signature: u32) {
        let physical = self.local_slots[slot as usize];
        self.managed_guard(
            physical,
            if self.profile.closures() {
                layout::CLOSURE_BYTES
            } else {
                layout::CELL_BYTES
            },
            layout::HEAP_FLAG,
            if self.profile.closures() {
                None
            } else {
                Some(0)
            },
        );
        self.assert_word(layout::SIGNATURE_OFFSET, signature as i32);
        self.assert_word(
            layout::RESERVED_OFFSETS[0],
            if self.profile.closures() {
                layout::FUNCTION_FIELDS
            } else {
                0
            },
        );
        self.assert_word(layout::RESERVED_OFFSETS[1], 0);
        if self.profile.closures() {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I32Const(abi::STRING_HEADER_BYTES as i32),
                I::I32Sub,
                I::I32Load(strings::LINK),
                I::LocalGet(self.temporary),
                I::I64Load(wasm_encoder::MemArg {
                    offset: layout::ENVIRONMENT_OFFSET,
                    ..strings::ROOT
                }),
                I::I64Eqz,
                I::If(BlockType::Result(ValType::I32)),
                I::I32Const(0),
                I::Else,
                I::I32Const(layout::ENVIRONMENT_MASK),
                I::End,
                I::I32Ne,
            ]);
            self.trap_if();
        }
    }
    pub(super) fn function_call(
        &mut self,
        callee: &Expr,
        ignored_receiver: Option<&Expr>,
        args: &[Expr],
        slot: u32,
        arguments: &[u32],
        output: Option<u32>,
    ) -> Result<()> {
        let Type::Function(signature) = callee.ty else {
            return Err(CompilerError::Internal("missing call signature".into()));
        };
        let targets = self.function_targets[&signature].clone();
        let result = self.function_types[signature as usize].result;
        self.expr(callee)?;
        self.scratch_set(slot, callee.ty);
        self.function_guard(slot, signature);
        if let Some(value) = ignored_receiver {
            self.expr(value)?;
            for _ in 0..value.ty.width() {
                self.code.push(I::Drop);
            }
        }
        // Expression roots retain the callee/arguments. These hidden locals are
        // never installed as binding roots, so abandoned calls cannot leak them.
        for (value, slot) in args.iter().zip(arguments) {
            self.expr(value)?;
            self.scratch_set(*slot, value.ty);
        }
        let done = self.push_label(I::Block(BlockType::Empty), false, false);
        for target in targets {
            self.code.extend([
                I::LocalGet(self.local_slots[slot as usize]),
                I::I32WrapI64,
                I::I32Load(wasm_encoder::MemArg {
                    offset: layout::CODE_OFFSET,
                    ..strings::WORD
                }),
                I::I32Const(target as i32),
                I::I32Eq,
            ]);
            self.push_label(I::If(BlockType::Empty), false, false);
            for (value, slot) in args.iter().zip(arguments) {
                self.scratch_get(*slot, value.ty);
            }
            if self.self_functions.contains(&target) {
                self.code.push(I::LocalGet(self.local_slots[slot as usize]));
            }
            if self.profile.closures() {
                self.code.extend([
                    I::LocalGet(self.local_slots[slot as usize]),
                    I::I32WrapI64,
                    I::I64Load(wasm_encoder::MemArg {
                        offset: layout::ENVIRONMENT_OFFSET,
                        ..strings::ROOT
                    }),
                ]);
                if !self.environment_functions.contains(&target) {
                    self.code.extend([I::I64Eqz, I::I32Eqz]);
                    self.trap_if();
                }
            }
            self.code
                .push(I::Call(self.functions_begin + target as u32));
            if let Some(slot) = output {
                self.scratch_set(slot, result);
            }
            self.code.push(I::Br(self.depth(done)));
            self.end();
        }
        self.code.push(I::Unreachable);
        self.end();
        self.check_exception()?;
        if let Some(slot) = output {
            self.scratch_get(slot, result);
        }
        Ok(())
    }
}

impl Emitter<'_> {
    pub(super) fn check_function_value(&mut self, value: &Expr, slot: u32) -> Result<()> {
        let Type::Function(signature) = value.ty else {
            return Err(CompilerError::Internal(
                "missing checked callback signature".into(),
            ));
        };
        self.expr(value)?;
        self.scratch_set(slot, value.ty);
        self.function_guard(slot, signature);
        self.code.push(I::I32Const(0));
        for target in &self.function_targets[&signature] {
            self.code.extend([
                I::LocalGet(self.local_slots[slot as usize]),
                I::I32WrapI64,
                I::I32Load(wasm_encoder::MemArg {
                    offset: layout::CODE_OFFSET,
                    ..strings::WORD
                }),
                I::I32Const(*target as i32),
                I::I32Eq,
                I::I32Or,
            ]);
        }
        self.code.push(I::I32Eqz);
        self.trap_if();
        Ok(())
    }
}
