//! Parameter-only pure tag tests and closed-record rest metadata transfer.
use super::*;
use varta_common::value as values;
const WORD_BYTES: usize = size_of::<u32>();
impl Emitter<'_> {
    pub(super) fn is_nullish(&mut self, value: &Expr) -> Result<()> {
        self.expr(value)?;
        if value.ty.is_union() {
            self.code.extend([
                I::I32WrapI64,
                I::LocalTee(self.temporary),
                I::I32Load(strings::WORD),
                I::I32Const(values::ValueKind::Null as i32),
                I::I32Eq,
                I::LocalGet(self.temporary),
                I::I32Load(strings::WORD),
                I::I32Const(values::ValueKind::Undefined as i32),
                I::I32Eq,
                I::I32Or,
            ]);
        } else {
            for _ in 0..value.ty.width() {
                self.code.push(I::Drop);
            }
            self.code.push(I::I32Const(i32::from(matches!(
                value.ty,
                Type::Null | Type::Undefined | Type::Void
            ))));
        }
        Ok(())
    }
    /// Payloads are already copied and rooted. No allocation or user code runs
    /// while mapping presence and filtering the source's live enumeration order.
    pub(super) fn record_rest(
        &mut self,
        slots: (u32, u32),
        shapes: (u32, u32),
        mapping: &[(u32, u32)],
        iteration: (u32, u32),
    ) {
        let (source, target) = (
            self.local_slots[slots.0 as usize],
            self.local_slots[slots.1 as usize],
        );
        let (index, cursor) = (
            self.local_slots[iteration.0 as usize],
            self.local_slots[iteration.1 as usize],
        );
        let from = values::presence_offset(self.records[shapes.0 as usize].fields.len());
        let count = self.records[shapes.1 as usize].fields.len();
        let to = values::presence_offset(count);
        self.code.extend([
            I::LocalGet(target),
            I::I32WrapI64,
            I::I32Const(to as i32),
            I::I32Add,
            I::I32Const(0),
            I::I32Const(((count + 1) * WORD_BYTES) as i32),
            I::MemoryFill(strings::MEMORY),
        ]);
        for (old, new) in mapping {
            self.code.extend([
                I::LocalGet(source),
                I::I32WrapI64,
                I::I32Load(word(from)),
                I::I32Const((1u32 << old) as i32),
                I::I32And,
                I::If(BlockType::Empty),
                I::LocalGet(target),
                I::I32WrapI64,
                I::LocalGet(target),
                I::I32WrapI64,
                I::I32Load(word(to)),
                I::I32Const((1u32 << new) as i32),
                I::I32Or,
                I::I32Store(word(to)),
                I::End,
            ]);
        }
        self.code.extend([
            I::I32Const(0),
            I::LocalSet(index),
            I::I32Const(0),
            I::LocalSet(cursor),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(index),
            I::LocalGet(source),
            I::I32WrapI64,
            I::I32Load(word(from)),
            I::I32Popcnt,
            I::I32GeU,
            I::BrIf(1),
        ]);
        for (old, new) in mapping {
            self.code.extend([
                I::LocalGet(source),
                I::I32WrapI64,
                I::LocalGet(index),
                I::I32Const(WORD_BYTES as i32),
                I::I32Mul,
                I::I32Add,
                I::I32Load(word(from + WORD_BYTES)),
                I::I32Const(*old as i32),
                I::I32Eq,
                I::If(BlockType::Empty),
                I::LocalGet(target),
                I::I32WrapI64,
                I::LocalGet(cursor),
                I::I32Const(WORD_BYTES as i32),
                I::I32Mul,
                I::I32Add,
                I::I32Const(*new as i32),
                I::I32Store(word(to + WORD_BYTES)),
                I::LocalGet(cursor),
                I::I32Const(1),
                I::I32Add,
                I::LocalSet(cursor),
                I::End,
            ]);
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
}
fn word(offset: usize) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg {
        offset: offset as u64,
        ..strings::WORD
    }
}
