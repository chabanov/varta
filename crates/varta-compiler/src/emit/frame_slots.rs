//! Liveness frame layouts (v35 `frame_liveness`). Each suspension point saves
//! only the locals live across it, directly in the frame's liveness segments
//! (`frame::FRAME_SLOT_SEGMENT_KIND`): one entry per one-slot value or shared
//! closure cell, two for a two-slot value, packed from entry zero in local
//! order, so states reuse the same entries. No frame cell is allocated for an
//! unshared value. A segment's edge mask is the exact GC map of the point the
//! frame is suspended at; resumption validates every touched segment against
//! that state's mask, restores the values and clears the masks, so a running
//! or completed frame retains nothing through its segments.
use super::frames::{FRAME_PARAM, Layout, Slot, binding_bytes, ceiling, segment_bytes};
use super::*;
use varta_common::abi;
use varta_common::function::{self as layout, frame};

impl Slot {
    /// Segment entries of this slot: its closure cell, or one per physical slot.
    pub(super) fn entries(self) -> usize {
        if self.shared {
            1
        } else {
            self.ty.width() as usize
        }
    }
    /// Whether entry `word` of this slot holds a managed reference.
    fn reference(self, word: usize) -> bool {
        word == 0 && (self.shared || self.ty.needs_root())
    }
}

/// `(slot, word, entry)` for every saved entry of `slots`, in entry order.
fn entries(slots: &[Slot]) -> Vec<(Slot, usize, usize)> {
    let mut entry = 0;
    let mut result = Vec::new();
    for slot in slots {
        for word in 0..slot.entries() {
            result.push((*slot, word, entry));
            entry += 1;
        }
    }
    result
}
/// Saved entries of one state.
pub(super) fn width(slots: &[Slot]) -> usize {
    slots.iter().map(|slot| slot.entries()).sum()
}
/// The exact edge mask of every segment while suspended with `slots` saved.
fn masks(slots: &[Slot], segments: usize) -> Vec<u32> {
    let mut masks = vec![0u32; segments];
    for (slot, word, entry) in entries(slots) {
        if slot.reference(word) {
            masks[entry / frame::SEGMENT_BINDINGS] |= 1 << (entry % frame::SEGMENT_BINDINGS);
        }
    }
    masks
}

impl Layout {
    /// Entries the frame's segments hold: the largest saved state.
    pub(super) fn capacity(&self) -> usize {
        self.states
            .iter()
            .flatten()
            .chain([&self.slots])
            .map(|slots| width(slots))
            .max()
            .unwrap_or(0)
    }
    /// The saved slots of `state`; state 0 holds the creator's parameters.
    pub(super) fn saved(&self, state: u32) -> Result<&[Slot]> {
        let states = self
            .states
            .as_ref()
            .ok_or_else(|| CompilerError::Internal("cell layout has no saved states".into()))?;
        match state {
            frame::INITIAL_STATE => Ok(&self.slots),
            state => states
                .get(state as usize - 1)
                .map(Vec::as_slice)
                .ok_or_else(|| CompilerError::Internal("unknown frame state".into())),
        }
    }
}

impl Emitter<'_> {
    /// Write `mask` into the edge word of the segment in `self.temporary`.
    fn segment_mask(&mut self, mask: u32) {
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Const(abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Const(mask as i32),
            I::I32Store(strings::LINK),
        ]);
    }
    /// Suspension of a liveness frame: store the state's live locals and
    /// publish its exact GC map; nothing allocates in between.
    pub(super) fn save_slots(&mut self, layout: &Layout, state: u32) -> Result<()> {
        let saved = layout.saved(state)?.to_vec();
        let masks = masks(&saved, layout.segments().len());
        let mut current = None;
        for (slot, word, entry) in entries(&saved) {
            let segment = entry / frame::SEGMENT_BINDINGS;
            if current != Some(segment) {
                if let Some(previous) = current.filter(|s| masks[*s] != 0) {
                    self.segment_mask(masks[previous]);
                }
                self.segment(FRAME_PARAM, entry);
                self.code.push(I::LocalSet(self.temporary));
                current = Some(segment);
            }
            self.code.push(I::LocalGet(self.temporary));
            if slot.shared {
                self.code.push(I::LocalGet(self.cell_slots[&slot.local]));
            } else if word == 0 {
                self.code
                    .push(I::LocalGet(self.local_slots[slot.local as usize]));
                self.pack_slot(slot.ty.result_value().unwrap_or(slot.ty));
            } else {
                self.code.extend([
                    I::LocalGet(self.local_slots[slot.local as usize] + 1),
                    I::I64ExtendI32U,
                ]);
            }
            self.code.push(I::I64Store(Self::entry(entry)));
        }
        if let Some(previous) = current.filter(|s| masks[*s] != 0) {
            self.segment_mask(masks[previous]);
        }
        Ok(())
    }
    /// Load and check the segment holding `entry`: its handle length, flags,
    /// kind, entry count and the exact edge mask of the state being resumed.
    fn checked_segment(&mut self, layout: &Layout, entry: usize, mask: u32) {
        let index = entry / frame::SEGMENT_BINDINGS;
        let count = layout.segments()[index];
        self.code.extend([
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::I64Load(records::field(frame::ENVIRONMENT_FIELD)),
            I::I32WrapI64,
            I::I64Load(records::field(index as u32)),
            I::LocalSet(self.string_temporary),
        ]);
        self.managed_guard(
            self.string_temporary,
            segment_bytes(count),
            layout::ENVIRONMENT_FLAG,
            Some(mask as i32),
        );
        self.assert_word(layout::SIGNATURE_OFFSET, frame::FRAME_SLOT_SEGMENT_KIND);
        self.assert_word(layout::RESERVED_OFFSETS[0], count as i32);
    }
    /// Restore one state's saved slots, then clear the masks it published.
    fn restore_slots(&mut self, layout: &Layout, saved: &[Slot]) {
        let masks = masks(saved, layout.segments().len());
        let mut current = None;
        for (slot, word, entry) in entries(saved) {
            let segment = entry / frame::SEGMENT_BINDINGS;
            if current != Some(segment) {
                if current.is_some_and(|s| masks[s] != 0) {
                    self.segment_mask(0);
                }
                self.checked_segment(layout, entry, masks[segment]);
                current = Some(segment);
            }
            let field = Self::entry(entry);
            if slot.shared {
                let cell = self.cell_slots[&slot.local];
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I64Load(field),
                    I::LocalTee(cell),
                    I::I64Eqz,
                    I::I32Eqz,
                    I::If(BlockType::Empty),
                ]);
                self.checked_cell(cell, slot.ty);
                self.code.push(I::End);
                // The cell check reused the scratch pointer.
                self.segment(FRAME_PARAM, entry);
                self.code.push(I::LocalSet(self.temporary));
            } else {
                let physical = self.local_slots[slot.local as usize];
                let limit = if word == 0 {
                    ceiling(slot.ty)
                } else {
                    // The second word of a two-slot value is a u32 status.
                    None
                };
                if let Some(limit) = limit {
                    self.code.extend([
                        I::LocalGet(self.temporary),
                        I::I64Load(field),
                        I::I64Const(limit),
                        I::I64GtU,
                    ]);
                    self.trap_if();
                }
                if word == 1 {
                    self.code.extend([
                        I::LocalGet(self.temporary),
                        I::I64Load(field),
                        I::I64Const(strings::LENGTH_SHIFT as i64),
                        I::I64ShrU,
                        I::I64Eqz,
                        I::I32Eqz,
                    ]);
                    self.trap_if();
                }
                self.code
                    .extend([I::LocalGet(self.temporary), I::I64Load(field)]);
                if word == 0 {
                    self.unpack_slot(slot.ty.result_value().unwrap_or(slot.ty));
                    self.code.push(I::LocalSet(physical));
                } else {
                    self.code.extend([I::I32WrapI64, I::LocalSet(physical + 1)]);
                }
            }
            if word + 1 == slot.entries() {
                self.store_root(slot.local);
            }
        }
        if current.is_some_and(|s| masks[s] != 0) {
            self.segment_mask(0);
        }
    }
    /// Resume prologue of a liveness frame: select the target state's saved
    /// set with one `br_table` and restore exactly that set.
    pub(super) fn restore_states(
        &mut self,
        layout: &Layout,
        machine: &ir::Suspendable,
    ) -> Result<()> {
        let state = self.local_slots[machine.state as usize];
        let states = machine.points.len() as u32;
        self.code.extend([
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::I32Load(super::frames::word(frame::CONTROL_FIELD, true)),
            I::LocalSet(state),
            // The resumption target, then the trap for any other state.
            I::Block(BlockType::Empty),
            I::Block(BlockType::Empty),
        ]);
        for _ in 0..=states {
            self.code.push(I::Block(BlockType::Empty));
        }
        self.code.extend([
            I::LocalGet(state),
            I::BrTable((0..=states).collect::<Vec<_>>().into(), states + 1),
        ]);
        for target in 0..=states {
            self.code.push(I::End);
            let saved = layout.saved(target)?.to_vec();
            self.restore_slots(layout, &saved);
            self.code.push(I::Br(states - target + 1));
        }
        self.code.extend([I::End, I::Unreachable, I::End]);
        Ok(())
    }
    /// Creator of a liveness frame: zeroed liveness segments, the unshared
    /// parameters and captured environment cells stored directly, the state-0
    /// GC map published before any further allocation, then the closure cells
    /// of shared parameters.
    pub(super) fn create_slots(&mut self, function: &ir::Function, layout: &Layout) -> Result<()> {
        let segments = layout.segments();
        for (index, count) in segments.iter().copied().enumerate() {
            let bytes = segment_bytes(count);
            self.frame_block(
                bytes,
                layout::ENVIRONMENT_FLAG,
                0,
                frame::FRAME_SLOT_SEGMENT_KIND,
                count as i32,
            )?;
            self.zero_fields(count);
            self.code.extend([
                I::LocalGet(self.string_temporary),
                I::I32WrapI64,
                I::I64Load(records::field(frame::ENVIRONMENT_FIELD)),
                I::I32WrapI64,
            ]);
            self.pack_value(bytes);
            self.code.push(I::I64Store(records::field(index as u32)));
        }
        let saved = layout.slots.clone();
        let parameters = function.params.len() as u32;
        for (slot, word, entry) in entries(&saved) {
            if slot.shared {
                if slot.local >= parameters {
                    // A captured environment cell (async closure body) is the
                    // binding itself: store the same cell, never a copy.
                    self.segment(self.string_temporary, entry);
                    self.code.extend([
                        I::LocalGet(self.cell_slots[&slot.local]),
                        I::I64Store(Self::entry(entry)),
                    ]);
                }
                continue;
            }
            let physical = self.local_slots[slot.local as usize];
            self.segment(self.string_temporary, entry);
            if word == 0 {
                self.code.push(I::LocalGet(physical));
                self.pack_slot(slot.ty.result_value().unwrap_or(slot.ty));
            } else {
                self.code
                    .extend([I::LocalGet(physical + 1), I::I64ExtendI32U]);
            }
            self.code.push(I::I64Store(Self::entry(entry)));
        }
        for (index, mask) in masks(&saved, segments.len()).into_iter().enumerate() {
            if mask != 0 {
                self.segment(self.string_temporary, index * frame::SEGMENT_BINDINGS);
                self.code.extend([
                    I::I32Const(abi::STRING_HEADER_BYTES as i32),
                    I::I32Sub,
                    I::I32Const(mask as i32),
                    I::I32Store(strings::LINK),
                ]);
            }
        }
        for (slot, _, entry) in entries(&saved) {
            if !slot.shared || slot.local >= parameters {
                continue;
            }
            let bytes = binding_bytes(slot.ty);
            self.frame_block(
                bytes,
                layout::BINDING_FLAG,
                i32::from(slot.ty.needs_root()),
                self.binding_types[&slot.ty] as i32,
                slot.ty.width() as i32,
            )?;
            let physical = self.local_slots[slot.local as usize];
            self.code
                .extend([I::LocalGet(self.temporary), I::LocalGet(physical)]);
            self.pack_slot(slot.ty.result_value().unwrap_or(slot.ty));
            self.code.push(I::I64Store(records::field(0)));
            if slot.ty.width() == 2 {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::LocalGet(physical + 1),
                    I::I64ExtendI32U,
                    I::I64Store(records::field(1)),
                ]);
            }
            self.segment(self.string_temporary, entry);
            self.pack_value(bytes);
            self.code.push(I::I64Store(Self::entry(entry)));
        }
        Ok(())
    }
}
