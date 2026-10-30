//! Exact roots for bindings and expression temporaries, across calls and control flow.
use super::*;
use varta_common::abi;

pub(super) fn slots(types: &[Type], cells: &BTreeSet<u32>) -> Vec<Option<u32>> {
    let mut count = 0;
    types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            if ty.needs_root() || cells.contains(&(index as u32)) {
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
    pub(super) fn constant_root(&self, index: u32) -> i32 {
        (self.constant_roots + index * abi::STRING_ROOT_SLOT_BYTES as u32) as i32
    }
    pub(super) fn begin_frame(&mut self, function: &ir::Function) -> Result<()> {
        let Some(memory) = self.memory else {
            return Ok(());
        };
        let bytes = self.root_slots.iter().flatten().count() * abi::STRING_ROOT_SLOT_BYTES;
        if bytes > abi::STRING_ROOT_BYTES - self.reserved_roots {
            return Err(function.source.locate(
                CompilerError::Invalid("function string roots exceed root stack capacity".into()),
                function.span,
            ));
        }
        if self.codegen_v2() && bytes == 0 {
            // V38: an empty reference frame needs neither a bound check nor
            // zeroing; its base and end are the incoming root pointer.
            self.code.extend([
                I::GlobalGet(memory.roots),
                I::LocalTee(self.frame_base),
                I::LocalSet(self.frame_end),
            ]);
        } else {
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
            ]);
            self.zero_frame(bytes);
            self.code
                .extend([I::LocalGet(self.frame_end), I::GlobalSet(memory.roots)]);
        }
        for slot in 0..function.params.len() {
            // Boxed parameters still hold their incoming payload here. Root it
            // until every captured parameter has been copied into its own cell.
            if function.params[slot].needs_root() {
                self.write_root(slot as u32, self.local_slots[slot]);
            } else {
                self.store_root(slot as u32);
            }
        }
        Ok(())
    }
    /// Zero the reference frame. V38 stores small frames slot by slot, since
    /// `memory.fill` is charged per byte.
    fn zero_frame(&mut self, bytes: usize) {
        const STORED_SLOTS: usize = 8;
        let slots = bytes / abi::STRING_ROOT_SLOT_BYTES;
        if self.codegen_v2() && slots <= STORED_SLOTS {
            for slot in 0..slots {
                self.code.extend([
                    I::LocalGet(self.frame_base),
                    I::I64Const(0),
                    I::I64Store(wasm_encoder::MemArg {
                        offset: (slot * abi::STRING_ROOT_SLOT_BYTES) as u64,
                        ..strings::ROOT
                    }),
                ]);
            }
        } else {
            self.code.extend([
                I::LocalGet(self.frame_base),
                I::I32Const(0),
                I::I32Const(bytes as i32),
                I::MemoryFill(strings::MEMORY),
            ]);
        }
    }
    pub(super) fn end_frame(&mut self) {
        if let Some(memory) = self.memory {
            self.code
                .extend([I::LocalGet(self.frame_base), I::GlobalSet(memory.roots)]);
        }
    }
    pub(super) fn end_temporaries(&mut self) {
        if let Some(memory) = self.memory {
            self.code.extend([
                I::LocalGet(self.temporary_floor.unwrap_or(self.frame_end)),
                I::GlobalSet(memory.roots),
            ]);
        }
    }
    pub(super) fn store_root(&mut self, slot: u32) {
        if self.memory.is_none() {
            return;
        }
        let physical = self
            .cell_slots
            .get(&slot)
            .copied()
            .unwrap_or(self.local_slots[slot as usize]);
        self.write_root(slot, physical);
    }
    fn write_root(&mut self, slot: u32, physical: u32) {
        if let Some(offset) = self.root_slots[slot as usize] {
            self.code.extend([
                I::LocalGet(self.frame_base),
                I::I32Const(offset as i32),
                I::I32Add,
                I::LocalGet(physical),
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
                    I::LocalSet(
                        self.cell_slots
                            .get(&slot)
                            .copied()
                            .unwrap_or(self.local_slots[slot as usize]),
                    ),
                ]);
                self.reset_builder(slot);
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
        self.complete_branch(index)
    }
}
