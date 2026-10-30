//! Construction of a checked [`Tape`] as guest values of a dynamic layout
//! (value15 to value18), top-down and iteratively (`no_std`).
//!
//! The host's v38 import and the linked v38 decoder build through this one
//! implementation; they differ only in their [`Heap`]. Every container is
//! allocated with zero payload slots, stamped and published into its already
//! reachable parent slot (or retained, at the root) before any of its
//! children is allocated, so the collector reaches every block built so far
//! through the retained root alone, whatever the size of the value, and no
//! recursion is needed whatever its depth. Zero slots are skipped by the
//! collector until they are filled. A record's presence and order words are
//! completed once its fields are known; nothing but the collector, which
//! reads neither, sees the record before the result is returned.
use super::{
    JsonSchema, JsonType,
    scan::{Node, Tape, Text, decode_text},
};
use crate::{abi::STRING_HEADER_BYTES, limits::DataLimits, value::*};
use alloc::vec::Vec;

/// Guest memory a builder writes through: the guest's own allocator and
/// retention helper, and plain stores into blocks it allocated.
pub trait Heap {
    type Error;
    /// A fresh block of `len` payload bytes from the guest allocator: its
    /// payload address. The contents are unspecified until written.
    fn allocate(&mut self, len: usize) -> Result<usize, Self::Error>;
    /// Write `bytes` at `at`, inside a block this builder allocated.
    fn write(&mut self, at: usize, bytes: &[u8]) -> Result<(), Self::Error>;
    fn word(&mut self, at: usize, value: u32) -> Result<(), Self::Error> {
        self.write(at, &value.to_le_bytes())
    }
    fn slot(&mut self, at: usize, value: u64) -> Result<(), Self::Error> {
        self.write(at, &value.to_le_bytes())
    }
    /// Zero `len` bytes at `at`, inside a block this builder allocated.
    fn zero(&mut self, at: usize, len: usize) -> Result<(), Self::Error>;
    /// Retain the result on the guest's root stack.
    fn retain(&mut self, handle: u64) -> Result<(), Self::Error>;
}

/// Where a built payload goes: the retained root, or a slot of a published
/// container.
#[derive(Clone, Copy)]
enum Destination {
    Root,
    Slot(usize),
}

/// An open container and where its next value goes.
enum Frame {
    /// A union cell's payload slot; its one member value is pending.
    Union { slot: usize },
    Array {
        slots: usize,
        index: usize,
        count: usize,
    },
    Record {
        ptr: usize,
        ty: u32,
        remaining: u32,
        present: u128,
        slot: usize,
    },
    Dictionary {
        keys: usize,
        values: usize,
        index: usize,
        count: usize,
        /// Own-key positions by input entry, when they differ from it.
        positions: Option<Vec<u32>>,
    },
}

/// Build the value of a successful scan of UTF-16 `text` against `schema`
/// (whose value layout is dynamic): the root payload, retained when it is a
/// block.
pub fn build<H: Heap>(
    tape: &Tape,
    text: &[u8],
    schema: &JsonSchema,
    heap: &mut H,
) -> Result<u64, H::Error> {
    let mut builder = Builder {
        heap,
        schema,
        text,
        version: schema.value_version(),
        frames: Vec::new(),
        orders: Vec::new(),
        decoded: Vec::new(),
        root: 0,
    };
    builder.run(&tape.nodes)?;
    Ok(builder.root)
}

struct Builder<'a, H> {
    heap: &'a mut H,
    schema: &'a JsonSchema,
    text: &'a [u8],
    version: u32,
    frames: Vec<Frame>,
    /// Record types' order words when every field is present (integer-named
    /// fields first, ascending, then the others in slot order), by type.
    orders: Vec<(u32, Vec<u32>)>,
    decoded: Vec<u16>,
    root: u64,
}

/// The address of header word `offset` of the block with payload `ptr`.
fn header(ptr: usize, offset: usize) -> usize {
    ptr - STRING_HEADER_BYTES + offset
}
/// A packed handle: byte length above the address.
fn handle(ptr: usize, len: usize) -> u64 {
    ((len as u64) << u32::BITS) | ptr as u64
}
/// The own-key position of input entry `index`.
fn position(positions: Option<&Vec<u32>>, index: usize) -> usize {
    positions.map_or(index, |positions| positions[index] as usize)
}

impl<H: Heap> Builder<'_, H> {
    fn run(&mut self, nodes: &[Node]) -> Result<(), H::Error> {
        let mut index = 0;
        while index < nodes.len() {
            let node = nodes[index];
            index += 1;
            match node {
                // A field selects the slot of the value that follows.
                Node::Field { slot } => {
                    if let Some(Frame::Record {
                        slot: next,
                        present,
                        ..
                    }) = self.frames.last_mut()
                    {
                        *next = slot as usize;
                        *present |= 1 << slot;
                    }
                    continue;
                }
                Node::Key(text) => {
                    if let Some(Frame::Dictionary {
                        keys,
                        index,
                        positions,
                        ..
                    }) = self.frames.last()
                    {
                        let at = *keys + position(positions.as_ref(), *index) * ARRAY_SLOT_BYTES;
                        self.string(text, Destination::Slot(at))?;
                    }
                    continue;
                }
                _ => {}
            }
            let destination = self.destination();
            let complete = match node {
                Node::Null | Node::Field { .. } | Node::Key(_) => {
                    self.place(0, destination)?;
                    true
                }
                Node::Boolean(value) => {
                    self.place(u64::from(value), destination)?;
                    true
                }
                Node::I32(value) => {
                    self.place(value as i64 as u64, destination)?;
                    true
                }
                Node::Float64(bits) => {
                    self.place(bits, destination)?;
                    true
                }
                Node::String(text) => {
                    self.string(text, destination)?;
                    true
                }
                Node::Union { kind, .. } => {
                    let link = u32::from(kind.is_reference());
                    let ptr = self.block(UNION_BYTES, HEAP_UNION_FLAG, link)?;
                    self.heap.word(ptr, kind as u32)?;
                    self.heap.word(ptr + size_of::<u32>(), 1)?;
                    self.heap.slot(ptr + UNION_META_BYTES, 0)?;
                    self.publish(handle(ptr, UNION_BYTES), destination)?;
                    self.frames.push(Frame::Union {
                        slot: ptr + UNION_META_BYTES,
                    });
                    false
                }
                Node::Array { ty, count } => {
                    let kind = match self.schema.types[ty as usize] {
                        JsonType::Array { element } => self.schema.types[element as usize].kind(),
                        _ => ValueKind::Undefined,
                    };
                    let slots = self.vector(kind, count as usize, destination)?;
                    if count > 0 {
                        self.frames.push(Frame::Array {
                            slots,
                            index: 0,
                            count: count as usize,
                        });
                    }
                    count == 0
                }
                Node::Record { ty, count } => {
                    let ptr = self.record(ty, destination)?;
                    if count == 0 {
                        self.complete_record(ptr, ty, 0)?;
                    } else {
                        self.frames.push(Frame::Record {
                            ptr,
                            ty,
                            remaining: count,
                            present: 0,
                            slot: 0,
                        });
                    }
                    count == 0
                }
                Node::Dictionary { ty, count, indexed } => {
                    let positions = indexed.then(|| own_key_positions(nodes, index, count, self.text));
                    self.dictionary(ty, count as usize, positions, destination)?;
                    count == 0
                }
            };
            if complete {
                self.advance()?;
            }
        }
        Ok(())
    }

    /// Where the next value goes.
    fn destination(&self) -> Destination {
        match self.frames.last() {
            None => Destination::Root,
            Some(Frame::Union { slot }) => Destination::Slot(*slot),
            Some(Frame::Array { slots, index, .. }) => {
                Destination::Slot(*slots + *index * ARRAY_SLOT_BYTES)
            }
            Some(Frame::Record { ptr, slot, .. }) => {
                Destination::Slot(*ptr + RECORD_META_BYTES + *slot * RECORD_SLOT_BYTES)
            }
            Some(Frame::Dictionary {
                values,
                index,
                positions,
                ..
            }) => Destination::Slot(*values + position(positions.as_ref(), *index) * ARRAY_SLOT_BYTES),
        }
    }
    /// A value of the innermost container is complete: move on, completing
    /// every container whose last value it was.
    fn advance(&mut self) -> Result<(), H::Error> {
        loop {
            match self.frames.last_mut() {
                None => return Ok(()),
                Some(Frame::Union { .. }) => {}
                Some(Frame::Array { index, count, .. })
                | Some(Frame::Dictionary { index, count, .. }) => {
                    *index += 1;
                    if *index < *count {
                        return Ok(());
                    }
                }
                Some(Frame::Record {
                    ptr,
                    ty,
                    remaining,
                    present,
                    ..
                }) => {
                    *remaining -= 1;
                    if *remaining > 0 {
                        return Ok(());
                    }
                    let (ptr, ty, present) = (*ptr, *ty, *present);
                    self.complete_record(ptr, ty, present)?;
                }
            }
            self.frames.pop();
        }
    }

    /// A scalar payload (or the empty string) at `destination`.
    fn place(&mut self, payload: u64, destination: Destination) -> Result<(), H::Error> {
        match destination {
            Destination::Root => self.root = payload,
            Destination::Slot(at) => self.heap.slot(at, payload)?,
        }
        Ok(())
    }
    /// A block's handle at `destination`: retained at the root.
    fn publish(&mut self, handle: u64, destination: Destination) -> Result<(), H::Error> {
        match destination {
            Destination::Root => {
                self.heap.retain(handle)?;
                self.root = handle;
            }
            Destination::Slot(at) => self.heap.slot(at, handle)?,
        }
        Ok(())
    }
    /// A stamped block of `len` payload bytes; it must be published before
    /// anything else allocates.
    fn block(&mut self, len: usize, flag: i32, link: u32) -> Result<usize, H::Error> {
        let ptr = self.heap.allocate(len)?;
        self.heap
            .word(header(ptr, HEAP_FLAGS_OFFSET), HEAP_ALLOCATED_FLAG | flag as u32)?;
        self.heap.word(header(ptr, HEAP_LINK_OFFSET), link)?;
        Ok(ptr)
    }
    /// An owned UTF-16 string of `text` (the zero handle when empty).
    fn string(&mut self, text: Text, destination: Destination) -> Result<(), H::Error> {
        if text.units == 0 {
            return self.place(0, destination);
        }
        let len = text.units as usize * size_of::<u16>();
        let ptr = self.block(len, 0, 0)?;
        if text.escaped {
            self.decoded.clear();
            decode_text(self.text, text, &mut self.decoded);
            for (offset, unit) in self.decoded.iter().enumerate() {
                self.heap
                    .write(ptr + offset * size_of::<u16>(), &unit.to_le_bytes())?;
            }
        } else {
            let start = text.start as usize * size_of::<u16>();
            self.heap.write(ptr, &self.text[start..start + len])?;
        }
        self.publish(handle(ptr, len), destination)
    }
    /// A vector of `count` zero slots, all present: the identity cell at
    /// `destination`, then its backing published into the cell. Returns the
    /// address of the backing's first slot.
    fn vector(
        &mut self,
        kind: ValueKind,
        count: usize,
        destination: Destination,
    ) -> Result<usize, H::Error> {
        let cell = self.block(VECTOR_BYTES, HEAP_VECTOR_FLAG, 1)?;
        self.heap.word(cell, kind as u32)?;
        self.heap.word(cell + size_of::<u32>(), count as u32)?;
        self.heap.slot(cell + VECTOR_STORAGE_OFFSET as usize, 0)?;
        self.publish(handle(cell, VECTOR_BYTES), destination)?;
        let capacity = array_capacity(count);
        let bitmap = ARRAY_META_BYTES + capacity * ARRAY_SLOT_BYTES;
        let len = bitmap + DataLimits::sparse_bitmap_bytes(capacity);
        let storage = self.block(
            len,
            HEAP_ARRAY_FLAG | HEAP_SPARSE_FLAG,
            u32::from(kind.is_reference()),
        )?;
        self.heap.word(storage, kind as u32)?;
        self.heap.word(storage + size_of::<u32>(), count as u32)?;
        self.heap.zero(storage + ARRAY_META_BYTES, len - ARRAY_META_BYTES)?;
        // Presence bits of slots `0..count`, a byte at a time at the edge.
        let full = count / u8::BITS as usize;
        let at = storage + bitmap;
        for byte in 0..full {
            self.heap.write(at + byte, &[u8::MAX])?;
        }
        let rest = count % u8::BITS as usize;
        if rest != 0 {
            self.heap.write(at + full, &[(1u8 << rest) - 1])?;
        }
        self.publish(
            handle(storage, len),
            Destination::Slot(cell + VECTOR_STORAGE_OFFSET as usize),
        )?;
        Ok(storage + ARRAY_META_BYTES)
    }
    /// A record of type `ty` with zero slots at `destination`: its address.
    fn record(&mut self, ty: u32, destination: Destination) -> Result<usize, H::Error> {
        let JsonType::Record { descriptor, fields } = &self.schema.types[ty as usize] else {
            return self.block(0, HEAP_RECORD_FLAG, 0);
        };
        let count = fields.len();
        // Wide records trace through their descriptor's edge bitmap.
        let link = if is_wide_record(count) {
            0
        } else {
            fields
                .iter()
                .enumerate()
                .filter(|(_, field)| self.schema.types[field.ty as usize].kind().is_reference())
                .fold(0u32, |mask, (slot, _)| mask | 1 << slot)
        };
        let len = record_bytes(self.version, count);
        let descriptor = *descriptor;
        let ptr = self.block(len, HEAP_RECORD_FLAG, link)?;
        self.heap.word(ptr, descriptor)?;
        self.heap.word(ptr + size_of::<u32>(), count as u32)?;
        self.heap.zero(ptr + RECORD_META_BYTES, len - RECORD_META_BYTES)?;
        self.publish(handle(ptr, len), destination)?;
        Ok(ptr)
    }
    /// Complete a record whose fields `present` were set: `undefined` cells
    /// for its missing union fields, then its presence and order words.
    fn complete_record(&mut self, ptr: usize, ty: u32, present: u128) -> Result<(), H::Error> {
        let schema = self.schema;
        let JsonType::Record { fields, .. } = &schema.types[ty as usize] else {
            return Ok(());
        };
        let count = fields.len();
        let all = wide_field_mask(count);
        if present != all {
            for (slot, field) in fields.iter().enumerate() {
                // A missing field of a union type holds an `undefined` cell;
                // one of type `undefined` keeps its zero payload.
                if present & 1 << slot != 0
                    || matches!(schema.types[field.ty as usize], JsonType::Undefined)
                {
                    continue;
                }
                let cell = self.block(UNION_BYTES, HEAP_UNION_FLAG, 0)?;
                self.heap.word(cell, ValueKind::Undefined as u32)?;
                self.heap.word(cell + size_of::<u32>(), 1)?;
                self.heap.slot(cell + UNION_META_BYTES, 0)?;
                self.heap.slot(
                    ptr + RECORD_META_BYTES + slot * RECORD_SLOT_BYTES,
                    handle(cell, UNION_BYTES),
                )?;
            }
        }
        for word in 0..presence_words(count) {
            self.heap.word(
                ptr + presence_offset(count) + word * size_of::<u32>(),
                (present >> (word * PRESENCE_WORD_BITS)) as u32,
            )?;
        }
        let order = self.order(ty, fields);
        let at = ptr + order_offset(count);
        let mut written = 0;
        for slot in order {
            if present & 1 << slot != 0 {
                self.heap.word(at + written * size_of::<u32>(), slot)?;
                written += 1;
            }
        }
        // The remaining order words stay zero.
        Ok(())
    }
    /// The full order words of record type `ty`.
    fn order(&mut self, ty: u32, fields: &[super::JsonField]) -> Vec<u32> {
        if let Some((_, order)) = self.orders.iter().find(|(known, _)| *known == ty) {
            return order.clone();
        }
        let mut order: Vec<u32> = (0..fields.len() as u32).collect();
        order.sort_by_key(|slot| {
            let name = &fields[*slot as usize].name;
            let integer = name
                .parse::<u32>()
                .ok()
                .filter(|n| *n < u32::MAX && alloc::format!("{n}") == *name);
            (integer.is_none(), integer.unwrap_or(*slot))
        });
        self.orders.push((ty, order.clone()));
        order
    }
    /// A dictionary of `count` entries at `destination`: its storage record,
    /// then its key and value vectors published into it.
    fn dictionary(
        &mut self,
        ty: u32,
        count: usize,
        positions: Option<Vec<u32>>,
        destination: Destination,
    ) -> Result<(), H::Error> {
        let (descriptor, value) = match self.schema.types[ty as usize] {
            JsonType::Dictionary { descriptor, value } => (descriptor, value),
            _ => (0, 0),
        };
        let kind = self.schema.types[value as usize].kind();
        // The storage record: canonical slots (value18 adds zero holes and
        // the zero table handle: unindexed), all present; every array slot
        // is an edge.
        let storage = dictionary_storage(self.version);
        let len = record_bytes(self.version, storage.len());
        let edges = storage
            .iter()
            .enumerate()
            .filter(|(_, (_, kind))| kind.is_reference())
            .fold(0u32, |edges, (slot, _)| edges | 1 << slot);
        let ptr = self.block(len, HEAP_RECORD_FLAG, edges)?;
        self.heap.word(ptr, descriptor)?;
        self.heap.word(ptr + size_of::<u32>(), storage.len() as u32)?;
        self.heap.zero(ptr + RECORD_META_BYTES, len - RECORD_META_BYTES)?;
        let count_slots = storage.len();
        self.heap.word(
            ptr + presence_offset(count_slots),
            wide_field_mask(count_slots) as u32,
        )?;
        for slot in 0..count_slots {
            self.heap.word(
                ptr + order_offset(count_slots) + slot * size_of::<u32>(),
                slot as u32,
            )?;
        }
        self.publish(handle(ptr, len), destination)?;
        let slot = |name: &str| {
            storage
                .iter()
                .position(|(field, _)| *field == name)
                .map_or(ptr, |index| ptr + RECORD_META_BYTES + index * RECORD_SLOT_BYTES)
        };
        let keys = self.vector(
            ValueKind::String,
            count,
            Destination::Slot(slot(DICTIONARY_KEYS_FIELD)),
        )?;
        let values = self.vector(kind, count, Destination::Slot(slot(DICTIONARY_VALUES_FIELD)))?;
        if count > 0 {
            self.frames.push(Frame::Dictionary {
                keys,
                values,
                index: 0,
                count,
                positions,
            });
        }
        Ok(())
    }
}

/// The index after the value starting at `index`.
fn skip(nodes: &[Node], mut index: usize) -> usize {
    // Values still to pass.
    let mut pending = 1usize;
    while pending > 0 {
        match nodes.get(index) {
            Some(Node::Field { .. } | Node::Key(_) | Node::Union { .. }) => {}
            Some(Node::Record { count, .. })
            | Some(Node::Array { count, .. })
            | Some(Node::Dictionary { count, .. }) => pending += *count as usize,
            Some(_) => {}
            None => return index,
        }
        if !matches!(
            nodes.get(index),
            Some(Node::Field { .. } | Node::Key(_) | Node::Union { .. })
        ) {
            pending -= 1;
        }
        index += 1;
    }
    index
}
/// The own-key position of each input entry of the dictionary whose first
/// entry starts at `index`: canonical array-index keys first, ascending,
/// then the other keys in input order.
fn own_key_positions(nodes: &[Node], mut index: usize, count: u32, text: &[u8]) -> Vec<u32> {
    let mut keys: Vec<(bool, u64, u32)> = Vec::with_capacity(count as usize);
    let mut units = Vec::new();
    for entry in 0..count {
        if let Some(Node::Key(key)) = nodes.get(index) {
            units.clear();
            decode_text(text, *key, &mut units);
            let order = array_index(&units);
            keys.push((order.is_none(), order.unwrap_or(0), entry));
        }
        index = skip(nodes, index + 1);
    }
    keys.sort_by_key(|(named, order, _)| (*named, *order));
    let mut positions = alloc::vec![0; count as usize];
    for (position, (_, _, entry)) in keys.into_iter().enumerate() {
        positions[entry as usize] = position as u32;
    }
    positions
}
