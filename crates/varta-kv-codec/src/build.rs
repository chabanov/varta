//! V38 (`data_limits`) construction of a checked [`JsonValue`] tree in guest
//! memory: top-down and iterative, through the guest's own allocator
//! ([`Heap`]).
//!
//! Every container is allocated with zero payload slots, stamped and
//! published into its (already reachable) parent slot before any of its
//! children is allocated, so the collector reaches every block built so far
//! through the retained root alone. Construction therefore needs one root
//! stack slot whatever the size of the value, and no host recursion whatever
//! its depth. Zero slots are skipped by the collector until they are filled;
//! the result is published to the guest only after every slot is filled.
//!
//! The native runtime's checked JSON values (a v38 KV decode among them) and
//! the browser host's KV decode build through this one implementation, so a
//! value takes the same allocations in the same order on both.
use alloc::{string::ToString, vec::Vec};
use varta_common::{
    abi,
    json::{JsonSchema, JsonType, JsonValue, build::Heap},
    limits::DataLimits,
    memory::Memory,
    value::*,
};

/// The array meta words: the element kind, then the length.
const META_WORD_BYTES: usize = size_of::<u32>();

/// Why construction stopped.
#[derive(Debug)]
pub enum BuildError<E> {
    /// The heap (the guest allocator, retention or memory) failed.
    Heap(E),
    /// The value or schema is not one this layout builds; hosts trap with
    /// `checked JSON: <message>`.
    Invalid(&'static str),
}

impl<E> From<E> for BuildError<E> {
    fn from(error: E) -> Self {
        Self::Heap(error)
    }
}

/// Check the block of `len` payload bytes at `ptr` that the guest's
/// allocator returned: aligned past the root stack, allocated, large enough
/// and inside memory. The allocator is untrusted guest code.
pub fn check_block<M: Memory + ?Sized>(
    memory: &M,
    ptr: usize,
    len: usize,
) -> Result<(), &'static str> {
    if ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
        || !ptr.is_multiple_of(HEAP_ALIGNMENT)
    {
        return Err("allocator returned an invalid pointer");
    }
    let header = ptr - abi::STRING_HEADER_BYTES;
    // The whole header must be in memory; its first words are the block
    // size and the flags.
    let Some(words) = memory.bytes(header, abi::STRING_HEADER_BYTES) else {
        return Err("memory range");
    };
    let word = |at: usize| {
        let mut bytes = [0; size_of::<u32>()];
        bytes.copy_from_slice(&words[at..at + size_of::<u32>()]);
        u32::from_le_bytes(bytes)
    };
    let (size, flags) = (word(0), word(HEAP_FLAGS_OFFSET));
    if flags != HEAP_ALLOCATED_FLAG
        || (size as usize) < abi::STRING_HEADER_BYTES + len
        || !size.is_multiple_of(HEAP_ALIGNMENT as u32)
        || header
            .checked_add(size as usize)
            .is_none_or(|end| end > memory.size())
    {
        return Err("allocator returned an invalid block");
    }
    if ptr.checked_add(len).is_none_or(|end| end > memory.size()) {
        return Err("memory range");
    }
    Ok(())
}

/// Where a built payload goes: the retained root, or a slot of a published
/// container.
#[derive(Clone, Copy)]
enum Destination {
    Root,
    Slot(usize),
}
/// One pending value, in construction order.
enum Task<'a> {
    Value(&'a JsonValue, Destination),
    Key(&'a [u16], Destination),
    /// A vector whose items are the keys (`true`) or values of dictionary entries.
    Entries(&'a [(Vec<u16>, JsonValue)], bool, ValueKind, Destination),
}

/// Build `value` of `schema` and return its retained payload.
pub fn value<H: Heap>(
    heap: &mut H,
    schema: &JsonSchema,
    value: &JsonValue,
    limits: DataLimits,
) -> Result<u64, BuildError<H::Error>> {
    if !dynamic_layout(schema.value_version()) {
        return Err(BuildError::Invalid(
            "top-down construction requires a dynamic value layout",
        ));
    }
    let mut root = None;
    // A stack, children pushed in reverse: entries are built in order.
    let mut tasks = alloc::vec![Task::Value(value, Destination::Root)];
    while let Some(task) = tasks.pop() {
        let (payload, destination) = match task {
            Task::Value(value, destination) => (
                node(heap, schema, value, destination, &mut tasks, limits)?,
                destination,
            ),
            Task::Key(units, destination) => (string(heap, units, destination)?, destination),
            Task::Entries(entries, keys, kind, destination) => {
                let (handle, slots) = vector(heap, kind, entries.len(), destination, limits)?;
                for (index, (key, value)) in entries.iter().enumerate().rev() {
                    let slot = Destination::Slot(slots + index * ARRAY_SLOT_BYTES);
                    tasks.push(if keys {
                        Task::Key(key, slot)
                    } else {
                        Task::Value(value, slot)
                    });
                }
                (handle, destination)
            }
        };
        match destination {
            Destination::Root => root = Some(payload),
            // Containers already published themselves; scalars and strings land here.
            Destination::Slot(at) => heap.slot(at, payload)?,
        }
    }
    root.ok_or(BuildError::Invalid("missing root"))
}

/// Allocate, stamp and publish a block of `data`: retained when it is the
/// root, stored into its parent's slot otherwise. Nothing allocates in
/// between.
pub fn publish<H: Heap>(
    heap: &mut H,
    data: &[u8],
    flag: i32,
    link: u32,
    root: bool,
) -> Result<u64, H::Error> {
    let ptr = heap.allocate(data.len())?;
    heap.write(ptr, data)?;
    let header = ptr - abi::STRING_HEADER_BYTES;
    heap.word(
        header + HEAP_FLAGS_OFFSET,
        HEAP_ALLOCATED_FLAG | flag as u32,
    )?;
    heap.word(header + HEAP_LINK_OFFSET, link)?;
    let handle = ((data.len() as u64) << i32::BITS) | ptr as u64;
    if root {
        heap.retain(handle)?;
    }
    Ok(handle)
}

fn placed<H: Heap>(
    heap: &mut H,
    data: &[u8],
    flag: i32,
    link: u32,
    destination: Destination,
) -> Result<u64, H::Error> {
    match destination {
        Destination::Root => publish(heap, data, flag, link, true),
        Destination::Slot(at) => {
            let handle = publish(heap, data, flag, link, false)?;
            heap.slot(at, handle)?;
            Ok(handle)
        }
    }
}

fn string<H: Heap>(heap: &mut H, units: &[u16], destination: Destination) -> Result<u64, H::Error> {
    if units.is_empty() {
        return Ok(0);
    }
    let bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    placed(heap, &bytes, 0, 0, destination)
}

/// A vector of `count` zero slots, all present: the identity cell first, then
/// its backing, published into the cell at once. Returns the cell handle and
/// the address of the backing's first slot.
fn vector<H: Heap>(
    heap: &mut H,
    kind: ValueKind,
    count: usize,
    destination: Destination,
    limits: DataLimits,
) -> Result<(u64, usize), BuildError<H::Error>> {
    if count > limits.array_elements {
        return Err(BuildError::Invalid("array element limit"));
    }
    let length = u32::try_from(count).map_err(|_| BuildError::Invalid("array element limit"))?;
    let mut cell = Vec::with_capacity(VECTOR_BYTES);
    cell.extend((kind as u32).to_le_bytes());
    cell.extend(length.to_le_bytes());
    cell.extend(0u64.to_le_bytes());
    let handle = placed(heap, &cell, HEAP_VECTOR_FLAG, 1, destination)?;
    let capacity = array_capacity(count);
    let bitmap = ARRAY_META_BYTES + capacity * ARRAY_SLOT_BYTES;
    let mut data = alloc::vec![0; bitmap + DataLimits::sparse_bitmap_bytes(capacity)];
    data[..META_WORD_BYTES].copy_from_slice(&(kind as u32).to_le_bytes());
    data[META_WORD_BYTES..ARRAY_META_BYTES].copy_from_slice(&length.to_le_bytes());
    for index in 0..count {
        data[bitmap + index / u8::BITS as usize] |= 1 << (index % u8::BITS as usize);
    }
    let storage = placed(
        heap,
        &data,
        HEAP_ARRAY_FLAG | HEAP_SPARSE_FLAG,
        u32::from(kind.is_reference()),
        Destination::Slot(handle as u32 as usize + VECTOR_STORAGE_OFFSET as usize),
    )?;
    Ok((handle, storage as u32 as usize + ARRAY_META_BYTES))
}

fn node<'a, H: Heap>(
    heap: &mut H,
    schema: &JsonSchema,
    value: &'a JsonValue,
    destination: Destination,
    tasks: &mut Vec<Task<'a>>,
    limits: DataLimits,
) -> Result<u64, BuildError<H::Error>> {
    Ok(match value {
        JsonValue::Null | JsonValue::Undefined => 0,
        JsonValue::I32(n) => *n as i64 as u64,
        JsonValue::Float64(bits) => *bits,
        JsonValue::Boolean(b) => u64::from(*b),
        JsonValue::String16(units) => string(heap, units, destination)?,
        JsonValue::String(text) => {
            let units: Vec<u16> = text.encode_utf16().collect();
            string(heap, &units, destination)?
        }
        JsonValue::Union { value, .. } => {
            let kind = match &**value {
                JsonValue::Null => ValueKind::Null,
                JsonValue::Undefined => ValueKind::Undefined,
                JsonValue::Float64(_) => ValueKind::Float64,
                JsonValue::Boolean(_) => ValueKind::Boolean,
                JsonValue::String(_) | JsonValue::String16(_) => ValueKind::String,
                JsonValue::Record { .. } => ValueKind::Record,
                JsonValue::Dictionary { .. } => ValueKind::Dictionary,
                JsonValue::Array { .. } => ValueKind::Array,
                _ => return Err(BuildError::Invalid("invalid union member")),
            };
            let mut data = Vec::with_capacity(UNION_BYTES);
            data.extend((kind as u32).to_le_bytes());
            data.extend(1u32.to_le_bytes());
            data.extend(0u64.to_le_bytes());
            let handle = placed(
                heap,
                &data,
                HEAP_UNION_FLAG,
                u32::from(kind.is_reference()),
                destination,
            )?;
            let slot = handle as u32 as usize + UNION_META_BYTES;
            tasks.push(Task::Value(value, Destination::Slot(slot)));
            handle
        }
        JsonValue::Array { ty, values } => {
            let JsonType::Array { element } = schema.types[*ty as usize] else {
                return Err(BuildError::Invalid("array type"));
            };
            let kind = schema.types[element as usize].kind();
            let (handle, slots) = vector(heap, kind, values.len(), destination, limits)?;
            for (index, value) in values.iter().enumerate().rev() {
                tasks.push(Task::Value(
                    value,
                    Destination::Slot(slots + index * ARRAY_SLOT_BYTES),
                ));
            }
            handle
        }
        JsonValue::Dictionary { ty, entries } => {
            let JsonType::Dictionary { descriptor, value } = schema.types[*ty as usize] else {
                return Err(BuildError::Invalid("dictionary type"));
            };
            let kind = schema.types[value as usize].kind();
            // The storage record: canonical `keys`/`values` vector slots
            // (and under value18 zero holes and the zero table handle:
            // unindexed), all present; every array slot is an edge. The
            // vectors are published into their slots by the tasks below.
            let version = schema.value_version();
            let storage = dictionary_storage(version);
            let mut data = Vec::with_capacity(record_bytes(version, storage.len()));
            data.extend(descriptor.to_le_bytes());
            data.extend((storage.len() as u32).to_le_bytes());
            data.resize(RECORD_META_BYTES + storage.len() * RECORD_SLOT_BYTES, 0);
            let all = wide_field_mask(storage.len()) as u32;
            data.extend(all.to_le_bytes());
            for slot in 0..storage.len() as u32 {
                data.extend(slot.to_le_bytes());
            }
            let edges = storage
                .iter()
                .enumerate()
                .filter(|(_, (_, kind))| kind.is_reference())
                .fold(0u32, |edges, (slot, _)| edges | 1 << slot);
            let handle = placed(heap, &data, HEAP_RECORD_FLAG, edges, destination)?;
            let slot = |name: &str| {
                let index = storage
                    .iter()
                    .position(|(field, _)| *field == name)
                    .ok_or(BuildError::Invalid("dictionary storage slot"))?;
                Ok::<_, BuildError<H::Error>>(Destination::Slot(
                    handle as u32 as usize + RECORD_META_BYTES + index * RECORD_SLOT_BYTES,
                ))
            };
            tasks.push(Task::Entries(
                entries,
                false,
                kind,
                slot(DICTIONARY_VALUES_FIELD)?,
            ));
            tasks.push(Task::Entries(
                entries,
                true,
                ValueKind::String,
                slot(DICTIONARY_KEYS_FIELD)?,
            ));
            handle
        }
        JsonValue::Record { ty, fields: values } => {
            let JsonType::Record { descriptor, fields } = &schema.types[*ty as usize] else {
                return Err(BuildError::Invalid("record type"));
            };
            let mut data = Vec::with_capacity(RECORD_META_BYTES + values.len() * RECORD_SLOT_BYTES);
            data.extend(descriptor.to_le_bytes());
            data.extend((values.len() as u32).to_le_bytes());
            data.resize(RECORD_META_BYTES + values.len() * RECORD_SLOT_BYTES, 0);
            let mut mask = 0u128;
            for (index, field) in fields.iter().enumerate() {
                if schema.types[field.ty as usize].kind().is_reference() {
                    mask |= 1 << index;
                }
            }
            // Wide records trace through their descriptor's edge bitmap.
            let link = if is_wide_record(fields.len()) {
                0
            } else {
                mask as u32
            };
            let absent = |slot: usize| {
                fields[slot].optional
                    && match &values[slot] {
                        JsonValue::Undefined => true,
                        JsonValue::Union { value, .. } => matches!(**value, JsonValue::Undefined),
                        _ => false,
                    }
            };
            let mut present = wide_field_mask(fields.len());
            let mut order: Vec<_> = (0..fields.len())
                .filter(|slot| {
                    if absent(*slot) {
                        present &= !(1u128 << slot);
                        false
                    } else {
                        true
                    }
                })
                .collect();
            for word in 0..presence_words(fields.len()) {
                data.extend(((present >> (word * PRESENCE_WORD_BITS)) as u32).to_le_bytes());
            }
            order.sort_by_key(|slot| {
                let name = &fields[*slot].name;
                let integer = name
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n < u32::MAX && n.to_string() == *name);
                (integer.is_none(), integer.unwrap_or(*slot as u32))
            });
            order.resize(fields.len(), 0);
            for slot in order {
                data.extend((slot as u32).to_le_bytes());
            }
            let handle = placed(heap, &data, HEAP_RECORD_FLAG, link, destination)?;
            for (index, value) in values.iter().enumerate().rev() {
                tasks.push(Task::Value(
                    value,
                    Destination::Slot(
                        handle as u32 as usize + RECORD_META_BYTES + index * RECORD_SLOT_BYTES,
                    ),
                ));
            }
            handle
        }
    })
}
