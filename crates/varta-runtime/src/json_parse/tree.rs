//! V38 (`data_limits`) construction of a checked JSON tree: top-down and
//! iterative. Every container is allocated with zero payload slots, stamped
//! and published into its (already reachable) parent slot before any of its
//! children is allocated, so the collector reaches every block built so far
//! through the retained root alone. Construction therefore needs one root
//! stack slot whatever the size of the value, and no host recursion whatever
//! its depth. Zero slots are skipped by the collector until they are filled;
//! the result is published to the guest only after every slot is filled.
//! Earlier profiles keep the child-first builder of [`super::heap`].
use super::*;
use heap::Writer;
use varta_common::{json::JsonValue, limits::DataLimits};

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

pub(super) fn build(
    writer: &Writer,
    caller: &mut Caller<'_, RuntimeContext>,
    schema: &JsonSchema,
    value: &JsonValue,
    limits: DataLimits,
) -> Result<u64, wasmtime::Error> {
    if !dynamic_layout(schema.value_version()) {
        return Err(error(
            "top-down construction requires a dynamic value layout",
        ));
    }
    let mut root = None;
    // A stack, children pushed in reverse: entries are built in order.
    let mut tasks = vec![Task::Value(value, Destination::Root)];
    while let Some(task) = tasks.pop() {
        let (payload, destination) = match task {
            Task::Value(value, destination) => (
                node(
                    writer,
                    caller,
                    schema,
                    value,
                    destination,
                    &mut tasks,
                    limits,
                )?,
                destination,
            ),
            Task::Key(units, destination) => {
                (string(writer, caller, units, destination)?, destination)
            }
            Task::Entries(entries, keys, kind, destination) => {
                let (handle, slots) =
                    vector(writer, caller, kind, entries.len(), destination, limits)?;
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
            Destination::Slot(at) => writer.store(caller, at, payload)?,
        }
    }
    root.ok_or_else(|| error("missing root"))
}

/// Allocate, stamp and publish a block: retained when it is the root, stored
/// into its parent's slot otherwise. Nothing allocates in between.
fn publish(
    writer: &Writer,
    caller: &mut Caller<'_, RuntimeContext>,
    data: &[u8],
    flag: i32,
    link: u32,
    destination: Destination,
) -> Result<u64, wasmtime::Error> {
    match destination {
        Destination::Root => writer.owned(caller, data, flag, link),
        Destination::Slot(at) => {
            let handle = writer.unretained(caller, data, flag, link)?;
            writer.store(caller, at, handle)?;
            Ok(handle)
        }
    }
}

fn string(
    writer: &Writer,
    caller: &mut Caller<'_, RuntimeContext>,
    units: &[u16],
    destination: Destination,
) -> Result<u64, wasmtime::Error> {
    if units.is_empty() {
        return Ok(0);
    }
    let bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    publish(writer, caller, &bytes, 0, 0, destination)
}

/// A vector of `count` zero slots, all present: the identity cell first, then
/// its backing, published into the cell at once. Returns the cell handle and
/// the address of the backing's first slot.
fn vector(
    writer: &Writer,
    caller: &mut Caller<'_, RuntimeContext>,
    kind: ValueKind,
    count: usize,
    destination: Destination,
    limits: DataLimits,
) -> Result<(u64, usize), wasmtime::Error> {
    if count > limits.array_elements {
        return Err(error("array element limit"));
    }
    let length = u32::try_from(count).map_err(|_| error("array element limit"))?;
    let mut cell = Vec::with_capacity(VECTOR_BYTES);
    cell.extend((kind as u32).to_le_bytes());
    cell.extend(length.to_le_bytes());
    cell.extend(0u64.to_le_bytes());
    let handle = publish(writer, caller, &cell, HEAP_VECTOR_FLAG, 1, destination)?;
    let capacity = array_capacity(count);
    let bitmap = ARRAY_META_BYTES + capacity * ARRAY_SLOT_BYTES;
    let mut data = vec![0; bitmap + DataLimits::sparse_bitmap_bytes(capacity)];
    data[..4].copy_from_slice(&(kind as u32).to_le_bytes());
    data[4..8].copy_from_slice(&length.to_le_bytes());
    for index in 0..count {
        data[bitmap + index / u8::BITS as usize] |= 1 << (index % u8::BITS as usize);
    }
    let storage = publish(
        writer,
        caller,
        &data,
        HEAP_ARRAY_FLAG | HEAP_SPARSE_FLAG,
        u32::from(kind.is_reference()),
        Destination::Slot(handle as u32 as usize + VECTOR_STORAGE_OFFSET as usize),
    )?;
    Ok((handle, storage as u32 as usize + ARRAY_META_BYTES))
}

fn node<'a>(
    writer: &Writer,
    caller: &mut Caller<'_, RuntimeContext>,
    schema: &JsonSchema,
    value: &'a JsonValue,
    destination: Destination,
    tasks: &mut Vec<Task<'a>>,
    limits: DataLimits,
) -> Result<u64, wasmtime::Error> {
    Ok(match value {
        JsonValue::Null | JsonValue::Undefined => 0,
        JsonValue::I32(n) => *n as i64 as u64,
        JsonValue::Float64(bits) => *bits,
        JsonValue::Boolean(b) => u64::from(*b),
        JsonValue::String16(units) => string(writer, caller, units, destination)?,
        JsonValue::String(text) => {
            let units: Vec<u16> = text.encode_utf16().collect();
            string(writer, caller, &units, destination)?
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
                _ => return Err(error("invalid union member")),
            };
            let mut data = Vec::with_capacity(UNION_BYTES);
            data.extend((kind as u32).to_le_bytes());
            data.extend(1u32.to_le_bytes());
            data.extend(0u64.to_le_bytes());
            let handle = publish(
                writer,
                caller,
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
                return Err(error("array type"));
            };
            let kind = schema.types[element as usize].kind();
            let (handle, slots) = vector(writer, caller, kind, values.len(), destination, limits)?;
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
                return Err(error("dictionary type"));
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
            let handle = publish(writer, caller, &data, HEAP_RECORD_FLAG, edges, destination)?;
            let slot = |name: &str| {
                let index = storage
                    .iter()
                    .position(|(field, _)| *field == name)
                    .ok_or_else(|| error("dictionary storage slot"))?;
                Ok::<_, wasmtime::Error>(Destination::Slot(
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
                return Err(error("record type"));
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
            let handle = publish(writer, caller, &data, HEAP_RECORD_FLAG, link, destination)?;
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
