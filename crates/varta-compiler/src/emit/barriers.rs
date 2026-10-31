//! V38 (`incremental_collection`): the deletion write barrier of emitted
//! code. Every store that overwrites or removes a managed reference in a heap
//! object that may predate the current collection cycle shades the old value
//! first while the cycle marks (`Shade`); a bulk change of an object traces
//! the whole object first (`ShadeObject`). Stores into a block allocated by
//! the same statement before any later allocation (literal and creator
//! initialization) need none: the block is black or new. Modules of other
//! profiles emit nothing.
use super::*;

impl Emitter<'_> {
    /// The barrier of this module, if it has one.
    pub(super) fn barrier(&self) -> Option<strings::incremental::Barrier> {
        strings::Helper::barrier(self.memory, self.helpers)
    }
    /// Before overwriting the reference at `memarg` from the payload address
    /// in i32 local `address`.
    pub(super) fn slot_barrier(&mut self, address: u32, memarg: wasm_encoder::MemArg) {
        if let Some(barrier) = self.barrier() {
            self.code.extend(barrier.slot(address, memarg));
        }
    }
    /// [`Self::slot_barrier`] only for a slot of a managed reference type.
    pub(super) fn typed_slot_barrier(
        &mut self,
        ty: Type,
        address: u32,
        memarg: wasm_encoder::MemArg,
    ) {
        if ty.needs_root() {
            self.slot_barrier(address, memarg);
        }
    }
    /// The element type of an array type.
    pub(super) fn element(&self, ty: Type) -> Option<Type> {
        match ty {
            Type::Array(id) => self.arrays.get(id as usize).copied(),
            _ => None,
        }
    }
    /// Before a bulk change of the object whose payload address is in i32
    /// local `payload`.
    pub(super) fn object_barrier(&mut self, payload: u32) {
        if let Some(barrier) = self.barrier() {
            self.code.extend(barrier.object(payload));
        }
    }
}
