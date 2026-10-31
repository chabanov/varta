//! Compact typed entries into a rooted dense array in shared own-property order.
use super::*;
use varta_common::value as values;
const WORD_BYTES: usize = size_of::<u32>();
fn word(offset: usize) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg {
        offset: offset as u64,
        ..strings::WORD
    }
}
impl Emitter<'_> {
    pub(super) fn record_enumerate(
        &mut self,
        source: u32,
        target: u32,
        shape: u32,
        slots: [u32; 4],
        entries: &[Expr],
    ) -> Result<()> {
        let source = self.local_slots[source as usize];
        let target = self.local_slots[target as usize];
        let [index, cursor, selected, backing] = slots.map(|slot| self.local_slots[slot as usize]);
        let fields = self.records[shape as usize].fields.clone();
        let count = fields.len();
        let order = values::order_offset(count);
        self.code.extend([
            I::LocalGet(target),
            I::I32WrapI64,
            I::I64Load(arrays::slot(0)),
            I::I32WrapI64,
            I::LocalSet(backing),
            I::I32Const(0),
            I::LocalSet(cursor),
        ]);
        // V38 (`incremental_collection`): the entries overwrite and clear the
        // target's slots in place.
        self.object_barrier(backing);
        let mut numeric = Vec::new();
        let mut names = Vec::new();
        for (slot, field) in fields.iter().enumerate() {
            let integer = field
                .name
                .parse::<u32>()
                .ok()
                .filter(|n| *n != u32::MAX && n.to_string() == field.name);
            if let Some(number) = integer {
                numeric.push((number, slot));
            } else {
                names.push(slot);
            }
        }
        numeric.sort_by_key(|(number, _)| *number);
        for (_, slot) in numeric {
            let (presence, bit) = values::presence_bit(count, slot);
            self.code.extend([
                I::LocalGet(source),
                I::I32WrapI64,
                I::I32Load(word(presence)),
                I::I32Const(bit as i32),
                I::I32And,
                I::If(BlockType::Empty),
            ]);
            self.entry_slot(backing, cursor, &entries[slot])?;
            self.code.push(I::End);
        }
        if !names.is_empty() {
            self.code.extend([
                I::I32Const(0),
                I::LocalSet(index),
                I::Block(BlockType::Empty),
                I::Loop(BlockType::Empty),
                I::LocalGet(index),
            ]);
            self.present_count(&[I::LocalGet(source), I::I32WrapI64], count);
            self.code.extend([
                I::I32GeU,
                I::BrIf(1),
                I::LocalGet(source),
                I::I32WrapI64,
                I::LocalGet(index),
                I::I32Const(WORD_BYTES as i32),
                I::I32Mul,
                I::I32Add,
                I::I32Load(word(order)),
                I::LocalSet(selected),
            ]);
            for slot in names {
                self.code.extend([
                    I::LocalGet(selected),
                    I::I32Const(slot as i32),
                    I::I32Eq,
                    I::If(BlockType::Empty),
                ]);
                self.entry_slot(backing, cursor, &entries[slot])?;
                self.code.push(I::End);
            }
            self.code.extend([
                I::LocalGet(index),
                I::I32Const(1),
                I::I32Add,
                I::LocalSet(index),
                I::Br(0),
                I::End,
                I::End,
            ]);
        }
        // Keep the old length while clearing tail slots/presence. No allocation
        // or user code can observe the compacted prefix before length publication.
        self.code.extend([
            I::LocalGet(cursor),
            I::LocalSet(index),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(index),
            I::I32Const(count as i32),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(backing),
            I::LocalGet(index),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Const(0),
            I::I64Store(arrays::slot(0)),
        ]);
        if self.profile.sparse_arrays() {
            self.code
                .extend([I::LocalGet(target), I::LocalGet(index), I::I32Const(0)]);
            self.string_helper(strings::Helper::ArraySetPresent)?;
        }
        self.code.extend([
            I::LocalGet(index),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(index),
            I::Br(0),
            I::End,
            I::End,
        ]);
        for cell in [true, false] {
            self.code
                .push(I::LocalGet(if cell { target } else { backing }));
            if cell {
                self.code.push(I::I32WrapI64);
            }
            self.code.extend([
                I::LocalGet(cursor),
                I::I32Store(word(values::ARRAY_LENGTH_OFFSET as usize)),
            ]);
        }
        Ok(())
    }
    fn entry_slot(&mut self, backing: u32, cursor: u32, value: &Expr) -> Result<()> {
        self.code.extend([
            I::LocalGet(backing),
            I::LocalGet(cursor),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
        ]);
        // Entries are generated literals/rooted locals only: no user code,
        // allocation or output resizing can occur while the backing is selected.
        self.expr(value)?;
        self.pack_slot(value.ty);
        self.code.push(I::I64Store(arrays::slot(0)));
        self.end_temporaries();
        self.code.extend([
            I::LocalGet(cursor),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(cursor),
        ]);
        Ok(())
    }
}
