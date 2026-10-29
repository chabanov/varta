//! Nominal class tests (v32). Every instance is a record whose static descriptor
//! is the canonical layout of its most-derived class; the descriptor offset is
//! therefore the class identity. A class that is never constructed has no
//! descriptor and can never match.
use super::*;

impl Emitter<'_> {
    pub(super) fn is_class(&mut self, value: &Expr, shapes: &[u32], slot: u32) -> Result<()> {
        self.expr(value)?;
        let descriptor = self.local_slots[slot as usize];
        self.code.extend([
            I::I32WrapI64,
            I::I32Load(strings::WORD),
            I::LocalSet(descriptor),
        ]);
        let offsets: Vec<u32> = shapes
            .iter()
            .filter_map(|shape| {
                self.layouts
                    .get(&records::canonical_layout(*shape, self.records))
                    .copied()
            })
            .collect();
        self.code.push(I::I32Const(0));
        for offset in offsets {
            self.code.extend([
                I::LocalGet(descriptor),
                I::I32Const(offset as i32),
                I::I32Eq,
                I::I32Or,
            ]);
        }
        Ok(())
    }
}
