//! Exact roots for bindings and expression temporaries, across calls and control flow.
use super::*;
use varta_common::abi;

pub(super) fn slots(types: &[Type]) -> Vec<Option<u32>> {
    let mut count = 0;
    types
        .iter()
        .map(|ty| {
            if ty.has_string() {
                let offset = count;
                count += abi::STRING_ROOT_SLOT_BYTES as u32;
                Some(offset)
            } else {
                None
            }
        })
        .collect()
}
impl Emitter<'_> {
    pub(super) fn begin_frame(&mut self, function: &ir::Function) -> Result<()> {
        let Some(memory) = self.memory else {
            return Ok(());
        };
        let bytes = self.root_slots.iter().flatten().count() * abi::STRING_ROOT_SLOT_BYTES;
        if bytes > abi::STRING_ROOT_BYTES {
            return Err(function.source.locate(
                CompilerError::Invalid("function string roots exceed root stack capacity".into()),
                function.span,
            ));
        }
        self.code.extend([
            I::GlobalGet(memory.roots),
            I::LocalTee(self.frame_base),
            I::I32Const(bytes as i32),
            I::I32Add,
            I::LocalTee(self.frame_end),
            I::I32Const(abi::STRING_ROOT_END as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(self.frame_base),
            I::I32Const(0),
            I::I32Const(bytes as i32),
            I::MemoryFill(strings::MEMORY),
            I::LocalGet(self.frame_end),
            I::GlobalSet(memory.roots),
        ]);
        for slot in 0..function.params.len() {
            self.store_root(slot as u32);
        }
        Ok(())
    }
    pub(super) fn end_frame(&mut self) {
        if let Some(memory) = self.memory {
            self.code
                .extend([I::LocalGet(self.frame_base), I::GlobalSet(memory.roots)]);
        }
    }
    pub(super) fn end_temporaries(&mut self) {
        if let Some(memory) = self.memory {
            self.code
                .extend([I::LocalGet(self.frame_end), I::GlobalSet(memory.roots)]);
        }
    }
    pub(super) fn store_root(&mut self, slot: u32) {
        if self.memory.is_none() {
            return;
        }
        if let Some(offset) = self.root_slots[slot as usize] {
            self.code.extend([
                I::LocalGet(self.frame_base),
                I::I32Const(offset as i32),
                I::I32Add,
                I::LocalGet(self.local_slots[slot as usize]),
                I::I64Store(strings::ROOT),
            ]);
        }
    }
    pub(super) fn clear_scopes(&mut self, depth: usize) {
        if self.memory.is_none() {
            return;
        }
        let slots: Vec<_> = self.scopes[depth..].iter().flatten().copied().collect();
        for slot in slots {
            if let Some(offset) = self.root_slots[slot as usize] {
                self.code.extend([
                    I::LocalGet(self.frame_base),
                    I::I32Const(offset as i32),
                    I::I32Add,
                    I::I64Const(0),
                    I::I64Store(strings::ROOT),
                    I::I64Const(0),
                    I::LocalSet(self.local_slots[slot as usize]),
                ]);
            }
        }
    }
    pub(super) fn branch(&mut self, continuing: bool) -> Result<()> {
        let index = self
            .labels
            .iter()
            .rposition(|label| {
                if continuing {
                    label.continuable
                } else {
                    label.breakable
                }
            })
            .ok_or_else(|| CompilerError::Internal("missing branch target".into()))?;
        self.clear_scopes(self.labels[index].scope_depth);
        self.code.push(I::Br(self.depth(index)));
        Ok(())
    }
}
