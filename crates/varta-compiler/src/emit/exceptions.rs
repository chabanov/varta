//! Explicit guest exception channel. Branch depths are resolved after emission so
//! calls inside expression-level conditionals unwind their entire operand stack.
use super::*;
use varta_common::exceptions as abi;
#[derive(Clone, Copy)]
pub(super) struct Globals {
    pub pending: u32,
    pub value: u32,
}
#[derive(Clone, Copy)]
pub(super) struct Handler {
    pub target: usize,
    pub marker: u32,
    pub scope_depth: usize,
}
impl Emitter<'_> {
    pub(super) fn throw_value(&mut self, value: &Expr) -> Result<()> {
        let globals = self
            .exceptions
            .ok_or_else(|| CompilerError::Internal("missing exception channel".into()))?;
        self.expr(value)?;
        self.code.extend([
            I::GlobalSet(globals.value),
            I::I32Const(abi::ROOT as i32),
            I::GlobalGet(globals.value),
            I::I64Store(strings::ROOT),
            I::I32Const(1),
            I::GlobalSet(globals.pending),
        ]);
        self.propagate_exception()
    }
    pub(super) fn check_exception(&mut self) -> Result<()> {
        if let Some(globals) = self.exceptions {
            self.code
                .extend([I::GlobalGet(globals.pending), I::If(BlockType::Empty)]);
            self.propagate_exception()?;
            self.code.push(I::End);
        }
        Ok(())
    }
    pub(super) fn propagate_exception(&mut self) -> Result<()> {
        if let Some(handler) = self.handlers.last().copied() {
            self.clear_scopes(handler.scope_depth);
            let memory = self
                .memory
                .ok_or_else(|| CompilerError::Internal("missing exception roots".into()))?;
            self.code.extend([
                I::LocalGet(self.local_slots[handler.marker as usize]),
                I::GlobalSet(memory.roots),
            ]);
            self.exception_jumps.insert(self.code.len(), handler.target);
            self.code.push(I::Br(0));
        } else if self.suspension.is_some() {
            // The dispatcher moves an uncaught exception into the frame.
            self.end_frame();
            self.code.push(I::Return);
        } else {
            self.end_frame();
            for ty in wasm_types(self.result) {
                self.code.push(match ty {
                    ValType::I64 => I::I64Const(0),
                    ValType::F64 => I::F64Const(0.0.into()),
                    _ => I::I32Const(0),
                });
            }
            self.code.push(I::Return);
        }
        Ok(())
    }
    pub(super) fn try_catch(
        &mut self,
        body: &[S],
        binding: Option<u32>,
        handler: &[S],
        marker: u32,
    ) -> Result<()> {
        let globals = self
            .exceptions
            .ok_or_else(|| CompilerError::Internal("missing catch channel".into()))?;
        let memory = self
            .memory
            .ok_or_else(|| CompilerError::Internal("missing catch roots".into()))?;
        self.code.extend([
            I::GlobalGet(memory.roots),
            I::LocalSet(self.local_slots[marker as usize]),
        ]);
        let done = self.push_label(I::Block(BlockType::Empty), false, false);
        let target = self.code.len();
        self.push_label(I::Block(BlockType::Empty), false, false);
        self.handlers.push(Handler {
            target,
            marker,
            scope_depth: self.scopes.len(),
        });
        self.statements(body)?;
        self.handlers.pop();
        self.code.push(I::Br(self.depth(done)));
        self.end();
        if let Some(slot) = binding {
            self.code.push(I::GlobalGet(globals.value));
            let ty = self.cell_types.get(&slot).copied();
            if let Some(ty) = ty {
                self.write_binding(slot, ty, true)?;
            } else {
                self.code.push(I::LocalSet(self.local_slots[slot as usize]));
                self.store_root(slot);
            }
        }
        self.clear_exception(globals);
        self.statements(handler)?;
        self.end();
        Ok(())
    }
    pub(super) fn clear_exception(&mut self, globals: Globals) {
        self.code.extend([
            I::I32Const(0),
            I::GlobalSet(globals.pending),
            I::I64Const(0),
            I::GlobalSet(globals.value),
            I::I32Const(abi::ROOT as i32),
            I::I64Const(0),
            I::I64Store(strings::ROOT),
        ]);
    }
    pub(super) fn resolve_exception_jumps(&mut self) -> Result<()> {
        if self.exception_jumps.is_empty() {
            return Ok(());
        }
        let mut blocks = Vec::new();
        for (index, instruction) in self.code.iter_mut().enumerate() {
            match instruction {
                I::Block(_) | I::Loop(_) | I::If(_) => blocks.push(index),
                I::End => {
                    blocks.pop();
                }
                _ => {}
            }
            if let Some(target) = self.exception_jumps.get(&index) {
                let position = blocks
                    .iter()
                    .rposition(|block| block == target)
                    .ok_or_else(|| {
                        CompilerError::Internal(
                            "exception target is outside active control stack".into(),
                        )
                    })?;
                *instruction = I::Br((blocks.len() - 1 - position) as u32);
            }
        }
        Ok(())
    }
}
