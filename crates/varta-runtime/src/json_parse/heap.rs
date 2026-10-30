use super::*;
use varta_common::json::JsonValue;
use wasmtime::{Memory, TypedFunc};

pub(super) struct Writer {
    memory: Memory,
    allocate: TypedFunc<i32, i32>,
    retain: TypedFunc<i64, i64>,
}
impl Writer {
    pub fn new(
        caller: &mut Caller<'_, RuntimeContext>,
        memory: Memory,
    ) -> Result<Self, wasmtime::Error> {
        let Some(Extern::Func(allocate)) = caller.get_export(VALUE_ALLOCATE) else {
            return Err(error("missing allocation helper"));
        };
        let Some(Extern::Func(retain)) = caller.get_export(VALUE_RETAIN) else {
            return Err(error("missing retention helper"));
        };
        Ok(Self {
            memory,
            allocate: allocate.typed(&*caller)?,
            retain: retain.typed(&*caller)?,
        })
    }
    pub(super) fn owned(
        &self,
        caller: &mut Caller<'_, RuntimeContext>,
        data: &[u8],
        flag: i32,
        link: u32,
    ) -> Result<u64, wasmtime::Error> {
        if std::time::Instant::now() >= caller.data().deadline {
            return Err(error("execution deadline"));
        }
        let ptr = self.allocate.call(&mut *caller, data.len() as i32)? as u32 as usize;
        if ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
            || !ptr.is_multiple_of(HEAP_ALIGNMENT)
        {
            return Err(error("allocator returned an invalid pointer"));
        }
        let header = ptr - abi::STRING_HEADER_BYTES;
        let memory = self.memory.data(&*caller);
        let [size, flags, _, _] = words::<4>(memory, header)?;
        if flags != HEAP_ALLOCATED_FLAG
            || (size as usize) < abi::STRING_HEADER_BYTES + data.len()
            || !size.is_multiple_of(HEAP_ALIGNMENT as u32)
            || header
                .checked_add(size as usize)
                .is_none_or(|end| end > memory.len())
        {
            return Err(error("allocator returned an invalid block"));
        }
        bytes(memory, ptr, data.len())?;
        self.memory.write(&mut *caller, ptr, data)?;
        self.memory.write(
            &mut *caller,
            header + HEAP_FLAGS_OFFSET,
            &(HEAP_ALLOCATED_FLAG | flag as u32).to_le_bytes(),
        )?;
        self.memory
            .write(&mut *caller, header + HEAP_LINK_OFFSET, &link.to_le_bytes())?;
        let value = ((data.len() as u64) << i32::BITS) | ptr as u64;
        let retained = self.retain.call(&mut *caller, value as i64)?;
        if retained as u64 != value {
            return Err(error("retention helper changed the value"));
        }
        Ok(value)
    }
    /// A dense vector (value9+) or legacy array of already-built payloads.
    fn vector(
        &self,
        caller: &mut Caller<'_, RuntimeContext>,
        schema: &JsonSchema,
        kind: ValueKind,
        payloads: &[u64],
    ) -> Result<u64, wasmtime::Error> {
        let mut data = Vec::with_capacity(ARRAY_META_BYTES + payloads.len() * ARRAY_SLOT_BYTES);
        data.extend((kind as u32).to_le_bytes());
        data.extend((payloads.len() as u32).to_le_bytes());
        for payload in payloads {
            data.extend(payload.to_le_bytes());
        }
        if matches!(
            schema.value_version(),
            VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
                | PROMISE_VALUE_VERSION
                | FUNCTION_FIELD_VALUE_VERSION
        ) {
            let bitmap = ARRAY_META_BYTES + array_capacity(payloads.len()) * ARRAY_SLOT_BYTES;
            let sparse = matches!(
                schema.value_version(),
                SPARSE_VALUE_VERSION
                    | BIGINT_VALUE_VERSION
                    | FUNCTION_VALUE_VERSION
                    | DYNAMIC_VALUE_VERSION
                    | PROMISE_VALUE_VERSION
                    | FUNCTION_FIELD_VALUE_VERSION
            );
            data.resize(bitmap + if sparse { SPARSE_BITMAP_BYTES } else { 0 }, 0);
            if sparse {
                for index in 0..payloads.len() {
                    data[bitmap + index / u8::BITS as usize] |= 1 << (index % u8::BITS as usize);
                }
            }
            let storage = self.owned(
                caller,
                &data,
                HEAP_ARRAY_FLAG
                    | if matches!(
                        schema.value_version(),
                        SPARSE_VALUE_VERSION
                            | BIGINT_VALUE_VERSION
                            | FUNCTION_VALUE_VERSION
                            | DYNAMIC_VALUE_VERSION
                            | PROMISE_VALUE_VERSION
                            | FUNCTION_FIELD_VALUE_VERSION
                    ) {
                        HEAP_SPARSE_FLAG
                    } else {
                        0
                    },
                u32::from(kind.is_reference()),
            )?;
            let mut cell = Vec::with_capacity(VECTOR_BYTES);
            cell.extend((kind as u32).to_le_bytes());
            cell.extend((payloads.len() as u32).to_le_bytes());
            cell.extend(storage.to_le_bytes());
            self.owned(caller, &cell, HEAP_VECTOR_FLAG, 1)
        } else {
            self.owned(
                caller,
                &data,
                HEAP_ARRAY_FLAG
                    | if matches!(
                        schema.value_version(),
                        SPARSE_VALUE_VERSION
                            | BIGINT_VALUE_VERSION
                            | FUNCTION_VALUE_VERSION
                            | DYNAMIC_VALUE_VERSION
                            | PROMISE_VALUE_VERSION
                            | FUNCTION_FIELD_VALUE_VERSION
                    ) {
                        HEAP_SPARSE_FLAG
                    } else {
                        0
                    },
                u32::from(kind.is_reference()),
            )
        }
    }
    pub fn build(
        &self,
        caller: &mut Caller<'_, RuntimeContext>,
        schema: &JsonSchema,
        value: &JsonValue,
    ) -> Result<u64, wasmtime::Error> {
        match value {
            JsonValue::Null | JsonValue::Undefined => Ok(0),
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
                let payload = self.build(caller, schema, value)?;
                let mut data = Vec::with_capacity(UNION_BYTES);
                data.extend((kind as u32).to_le_bytes());
                data.extend(1u32.to_le_bytes());
                data.extend(payload.to_le_bytes());
                self.owned(
                    caller,
                    &data,
                    HEAP_UNION_FLAG,
                    u32::from(kind.is_reference()),
                )
            }
            JsonValue::I32(n) => Ok(*n as i64 as u64),
            JsonValue::Float64(bits) => Ok(*bits),
            JsonValue::Boolean(b) => Ok(u64::from(*b)),
            JsonValue::String16(s) if s.is_empty() => Ok(0),
            JsonValue::String16(s) => self.owned(
                caller,
                &s.iter().flat_map(|u| u.to_le_bytes()).collect::<Vec<_>>(),
                0,
                0,
            ),
            JsonValue::String(s) if s.is_empty() => Ok(0),
            JsonValue::String(s) => self.owned(caller, s.as_bytes(), 0, 0),
            JsonValue::Array { ty, values } => {
                let JsonType::Array { element } = schema.types[*ty as usize] else {
                    return Err(error("array type"));
                };
                let kind = schema.types[element as usize].kind();
                let mut payloads = Vec::with_capacity(values.len());
                for value in values {
                    payloads.push(self.build(caller, schema, value)?);
                }
                self.vector(caller, schema, kind, &payloads)
            }
            JsonValue::Dictionary { ty, entries } => {
                let JsonType::Dictionary { descriptor, value } = schema.types[*ty as usize] else {
                    return Err(error("dictionary type"));
                };
                let mut keys = Vec::with_capacity(entries.len());
                for (key, _) in entries {
                    keys.push(self.build(caller, schema, &JsonValue::String16(key.clone()))?);
                }
                let keys = self.vector(caller, schema, ValueKind::String, &keys)?;
                let mut values = Vec::with_capacity(entries.len());
                for (_, entry) in entries {
                    values.push(self.build(caller, schema, entry)?);
                }
                let kind = schema.types[value as usize].kind();
                let values = self.vector(caller, schema, kind, &values)?;
                // The storage record: canonical `keys`/`values` slots, both present.
                let mut data =
                    Vec::with_capacity(record_bytes(schema.value_version(), DICTIONARY_FIELDS));
                data.extend(descriptor.to_le_bytes());
                data.extend((DICTIONARY_FIELDS as u32).to_le_bytes());
                data.extend(keys.to_le_bytes());
                data.extend(values.to_le_bytes());
                let both = wide_field_mask(DICTIONARY_FIELDS) as u32;
                data.extend(both.to_le_bytes());
                for slot in 0..DICTIONARY_FIELDS as u32 {
                    data.extend(slot.to_le_bytes());
                }
                self.owned(caller, &data, HEAP_RECORD_FLAG, both)
            }
            JsonValue::Record { ty, fields: values } => {
                let JsonType::Record { descriptor, fields } = &schema.types[*ty as usize] else {
                    return Err(error("record type"));
                };
                let mut data =
                    Vec::with_capacity(RECORD_META_BYTES + values.len() * RECORD_SLOT_BYTES);
                data.extend(descriptor.to_le_bytes());
                data.extend((values.len() as u32).to_le_bytes());
                let mut mask = 0u128;
                for (index, value) in values.iter().enumerate() {
                    data.extend(self.build(caller, schema, value)?.to_le_bytes());
                    if schema.types[fields[index].ty as usize]
                        .kind()
                        .is_reference()
                    {
                        mask |= 1 << index;
                    }
                }
                // Wide records trace through their descriptor's edge bitmap.
                let link = if is_wide_record(fields.len()) {
                    0
                } else {
                    mask as u32
                };
                if matches!(
                    schema.value_version(),
                    MISSING_VALUE_VERSION
                        | REFERENCE_VALUE_VERSION
                        | VECTOR_VALUE_VERSION
                        | ERROR_VALUE_VERSION
                        | ERROR_FAMILY_VALUE_VERSION
                        | SPARSE_VALUE_VERSION
                        | BIGINT_VALUE_VERSION
                        | FUNCTION_VALUE_VERSION
                        | DYNAMIC_VALUE_VERSION
                        | PROMISE_VALUE_VERSION
                        | FUNCTION_FIELD_VALUE_VERSION
                ) {
                    let absent = |slot: usize| {
                        fields[slot].optional
                            && match &values[slot] {
                                JsonValue::Undefined => true,
                                JsonValue::Union { value, .. } => {
                                    matches!(**value, JsonValue::Undefined)
                                }
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
                        data.extend(
                            ((present >> (word * PRESENCE_WORD_BITS)) as u32).to_le_bytes(),
                        );
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
                }
                self.owned(caller, &data, HEAP_RECORD_FLAG, link)
            }
        }
    }
}
