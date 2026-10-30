//! Checked JSON decoding followed by allocation through the guest's own bounded
//! collector. Complete input validation precedes the first guest helper call.
mod exceptions;
pub(crate) mod heap;
mod tree;
use crate::RuntimeContext;
use varta_common::{
    HostOperation, abi,
    json::{JsonSchema, JsonType, JsonValue},
    limits::DataLimits,
    value::*,
};
use wasmtime::{Caller, Extern, Linker, Val};

fn error(message: impl std::fmt::Display) -> wasmtime::Error {
    wasmtime::Error::msg(format!("checked JSON: {message}"))
}
fn bytes(data: &[u8], ptr: usize, len: usize) -> Result<&[u8], wasmtime::Error> {
    ptr.checked_add(len)
        .and_then(|end| data.get(ptr..end))
        .ok_or_else(|| error("memory range"))
}
/// Schema and descriptor bytes in the module's static region (v38: the first
/// page or the static extension).
fn static_bytes(
    data: &[u8],
    region: abi::StaticRegion,
    ptr: usize,
    len: usize,
) -> Result<&[u8], wasmtime::Error> {
    if !region.contains(ptr, len) {
        return Err(error("metadata outside static memory"));
    }
    bytes(data, ptr, len)
}
fn words<const N: usize>(data: &[u8], ptr: usize) -> Result<[u32; N], wasmtime::Error> {
    let data = bytes(data, ptr, N * size_of::<u32>())?;
    let mut words = [0; N];
    for (word, chunk) in words
        .iter_mut()
        .zip(data.as_chunks::<{ size_of::<u32>() }>().0)
    {
        *word = u32::from_le_bytes(*chunk);
    }
    Ok(words)
}
/// A dictionary's storage descriptor: the schema's value layout (value15 to
/// value18) with exactly its canonical slots (`value::dictionary_storage`):
/// required `keys` and `values` vectors, and under value18 the `holes` count
/// and the `table` slot.
fn dictionary_descriptor(
    data: &[u8],
    region: abi::StaticRegion,
    descriptor: usize,
    version: u32,
) -> Result<(), wasmtime::Error> {
    let fields = dictionary_fields(version);
    static_bytes(data, region, descriptor, descriptor_bytes(fields))?;
    if !dynamic_layout(version) || words::<2>(data, descriptor)? != [version, fields as u32] {
        return Err(error("dictionary descriptor version or count"));
    }
    for (slot, (name, expected)) in dictionary_storage(version).iter().enumerate() {
        let [key, len, raw, kind] = words::<4>(
            data,
            descriptor + RECORD_DESCRIPTOR_HEADER + slot * RECORD_DESCRIPTOR_ENTRY,
        )?;
        if raw as usize != slot
            || kind != *expected as u32
            || static_bytes(data, region, key as usize, len as usize)? != name.as_bytes()
        {
            return Err(error("dictionary descriptor disagrees with its layout"));
        }
    }
    Ok(())
}
fn descriptors(
    schema: &JsonSchema,
    data: &[u8],
    region: abi::StaticRegion,
) -> Result<(), wasmtime::Error> {
    for ty in &schema.types {
        if let JsonType::Dictionary { descriptor, .. } = ty {
            dictionary_descriptor(data, region, *descriptor as usize, schema.value_version())?;
        }
        if let JsonType::Record { descriptor, fields } = ty {
            let descriptor = *descriptor as usize;
            static_bytes(data, region, descriptor, descriptor_bytes(fields.len()))?;
            if is_wide_record(fields.len()) {
                // The collector traces a wide record through this edge bitmap.
                let at =
                    descriptor + RECORD_DESCRIPTOR_HEADER + fields.len() * RECORD_DESCRIPTOR_ENTRY;
                let edges = read_slot_set(data, at, presence_words(fields.len()))
                    .ok_or_else(|| error("record edge bitmap"))?;
                let expected = fields
                    .iter()
                    .enumerate()
                    .fold(0u128, |mask, (slot, field)| {
                        if schema.types[field.ty as usize].kind().is_reference() {
                            mask | 1 << slot
                        } else {
                            mask
                        }
                    });
                if edges != expected {
                    return Err(error("record edge bitmap disagrees with schema"));
                }
            }
            let [version, count] = words::<2>(data, descriptor)?;
            if !compatible_version(version, schema.value_version())
                || count as usize != fields.len()
            {
                return Err(error("record descriptor version or count"));
            }
            let mut slots = std::collections::BTreeSet::new();
            for index in 0..fields.len() {
                let [key, len, slot, kind] = words::<4>(
                    data,
                    descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY,
                )?;
                let optional = matches!(
                    schema.version,
                    varta_common::json::CHECKED_SCHEMA_VERSION
                        | varta_common::json::VECTOR_SCHEMA_VERSION
                        | varta_common::json::ERROR_SCHEMA_VERSION
                        | varta_common::json::ERROR_FAMILY_SCHEMA_VERSION
                        | varta_common::json::SPARSE_SCHEMA_VERSION
                        | varta_common::json::BIGINT_SCHEMA_VERSION
                        | varta_common::json::FUNCTION_SCHEMA_VERSION
                        | varta_common::json::DYNAMIC_SCHEMA_VERSION
                        | varta_common::json::PROMISE_SCHEMA_VERSION
                        | varta_common::json::FUNCTION_FIELD_SCHEMA_VERSION
                        | varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION
                ) && slot & OPTIONAL_FIELD_FLAG != 0;
                let slot = if matches!(
                    schema.version,
                    varta_common::json::CHECKED_SCHEMA_VERSION
                        | varta_common::json::VECTOR_SCHEMA_VERSION
                        | varta_common::json::ERROR_SCHEMA_VERSION
                        | varta_common::json::ERROR_FAMILY_SCHEMA_VERSION
                        | varta_common::json::SPARSE_SCHEMA_VERSION
                        | varta_common::json::BIGINT_SCHEMA_VERSION
                        | varta_common::json::FUNCTION_SCHEMA_VERSION
                        | varta_common::json::DYNAMIC_SCHEMA_VERSION
                        | varta_common::json::PROMISE_SCHEMA_VERSION
                        | varta_common::json::FUNCTION_FIELD_SCHEMA_VERSION
                        | varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION
                ) {
                    slot & !OPTIONAL_FIELD_FLAG
                } else {
                    slot
                };
                let field = fields
                    .get(slot as usize)
                    .ok_or_else(|| error("record slot"))?;
                if !slots.insert(slot)
                    || optional != field.optional
                    || kind != schema.types[field.ty as usize].kind() as u32
                    || static_bytes(data, region, key as usize, len as usize)?
                        != field.name.as_bytes()
                {
                    return Err(error("record descriptor disagrees with schema"));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::JsonParse.name(),
        |caller: Caller<'_, RuntimeContext>,
         ptr: i32,
         len: i32,
         schema_ptr: i32,
         schema_len: i32|
         -> Result<i32, wasmtime::Error> {
            decode(
                caller,
                ptr,
                len,
                schema_ptr,
                schema_len,
                varta_common::json::SCHEMA_VERSION,
                false,
            )
            .map(|value| value as u32 as i32)
        },
    )?;
    for (operation, version) in [
        (
            HostOperation::JsonParseV3,
            varta_common::json::NUMBER_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV4,
            varta_common::json::UNDEFINED_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV5,
            varta_common::json::UTF16_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV6,
            varta_common::json::UNION_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV7,
            varta_common::json::MISSING_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV8,
            varta_common::json::REFERENCE_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV9,
            varta_common::json::CHECKED_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV10,
            varta_common::json::VECTOR_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV11,
            varta_common::json::ERROR_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV12,
            varta_common::json::ERROR_FAMILY_SCHEMA_VERSION,
        ),
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |caller: Caller<'_, RuntimeContext>,
                  ptr: i32,
                  len: i32,
                  schema_ptr: i32,
                  schema_len: i32|
                  -> Result<i64, wasmtime::Error> {
                decode(caller, ptr, len, schema_ptr, schema_len, version, false)
                    .map(|value| value as i64)
            },
        )?;
    }
    for (operation, version) in [
        (
            HostOperation::JsonParseV13,
            varta_common::json::ERROR_FAMILY_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV15,
            varta_common::json::BIGINT_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV16,
            varta_common::json::DYNAMIC_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV17,
            varta_common::json::PROMISE_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV18,
            varta_common::json::FUNCTION_FIELD_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV19,
            varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION,
        ),
        (
            HostOperation::JsonParseV14,
            varta_common::json::SPARSE_SCHEMA_VERSION,
        ),
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |caller: Caller<'_, RuntimeContext>,
                  ptr: i32,
                  len: i32,
                  schema_ptr: i32,
                  schema_len: i32| {
                decode(caller, ptr, len, schema_ptr, schema_len, version, true)
                    .map(|value| value as i64)
            },
        )?;
    }
    Ok(())
}
/// Nesting depth of JSON source text, scanned iteratively (strings skipped).
fn source_depth(input: &[u8]) -> usize {
    let (mut depth, mut deepest, mut quoted, mut escaped) = (0usize, 0, false, false);
    for byte in input {
        if quoted {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => quoted = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'[' | b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}
/// Nesting depth of a checked tree, measured iteratively.
fn nesting(value: &JsonValue) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(value, 0usize)];
    while let Some((value, depth)) = pending.pop() {
        deepest = deepest.max(depth);
        match value {
            JsonValue::Union { value, .. } => pending.push((value, depth)),
            JsonValue::Record { fields: values, .. } | JsonValue::Array { values, .. } => {
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            JsonValue::Dictionary { entries, .. } => {
                pending.extend(entries.iter().map(|(_, value)| (value, depth + 1)));
            }
            _ => {}
        }
    }
    deepest
}
/// Decode on the calling thread when the source nests no deeper than the
/// legacy depth, which every host stack sustains; a deeper v38 source is
/// decoded on the JSON host stack (its recursive validation and the drop of
/// a failed partial tree included).
fn deep_decode<T: Send>(
    input: &[u8],
    limits: DataLimits,
    decode: impl FnOnce(&[u8]) -> T + Send,
) -> Result<T, wasmtime::Error> {
    if limits.json_depth > DataLimits::LEGACY.json_depth
        && source_depth(input) > DataLimits::LEGACY.json_depth
    {
        crate::json::deep(|| decode(input))
    } else {
        Ok(decode(input))
    }
}
fn decode(
    mut caller: Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
    schema_ptr: i32,
    schema_len: i32,
    expected_schema: u32,
    recoverable: bool,
) -> Result<u64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let limits = caller.data().module.limits.data;
    // V38 (`data_limits`) keeps its trees out of host recursion and the root
    // stack (`tree`); earlier profiles keep the child-first builder.
    let top_down = limits.json_input_bytes > abi::MAX_INPUT_BYTES;
    // V38 host decoding stops at the execution deadline; earlier profiles
    // keep their bounded decoder unchanged.
    let deadline = top_down.then(|| caller.data().deadline);
    if recoverable && len >= 0 && len as usize > limits.json_input_bytes {
        return Err(wasmtime::Error::new(
            varta_common::json::JsonDecodeError::Limit(varta_common::json::JsonInputLimit::Bytes),
        ));
    }
    if ptr < 0
        || len < 0
        || len as usize > limits.json_input_bytes
        || schema_ptr < 0
        || schema_len < 0
    {
        return Err(error("input range or byte limit"));
    }
    if recoverable {
        exceptions::ready(&mut caller)?;
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(error("missing memory"));
    };
    let module = caller.data().module;
    let region = module.static_region;
    let data = memory.data(&caller);
    let schema = JsonSchema::from_bytes_within(
        static_bytes(data, region, schema_ptr as usize, schema_len as usize)?,
        &module.limits,
    )
    .map_err(error)?;
    if schema.version != expected_schema {
        return Err(error("schema version does not match JSON import"));
    }
    descriptors(&schema, data, region)?;
    let input = bytes(data, ptr as usize, len as usize)?;
    let value = if recoverable {
        use varta_common::{
            json::{JsonDecodeError, JsonInputLimit},
            text::JsonTextError,
        };
        let parsed = match varta_common::text::json_input_limited(input, limits.json_input_bytes) {
            Ok(encoded) => deep_decode(&encoded, limits, |encoded| match deadline {
                Some(deadline) => schema.parse_json_classified_until(encoded, limits, deadline),
                None => schema.parse_json_classified(encoded),
            })?,
            Err(JsonTextError::Syntax) => Err(JsonDecodeError::Syntax { line: 1, column: 1 }),
            Err(JsonTextError::Limit) => Err(JsonDecodeError::Limit(JsonInputLimit::Bytes)),
            Err(JsonTextError::Encoding) => {
                return Err(wasmtime::Error::new(ValueAbiError(
                    "invalid JSON UTF-16 byte length".into(),
                )));
            }
        };
        exceptions::deadline(&caller)?;
        match parsed {
            Ok(value) => value,
            Err(JsonDecodeError::Syntax { .. }) => {
                return exceptions::publish(&mut caller, memory, &schema, ErrorClass::SyntaxError);
            }
            Err(JsonDecodeError::Type) => {
                return exceptions::publish(&mut caller, memory, &schema, ErrorClass::TypeError);
            }
            Err(error) => return Err(wasmtime::Error::new(error)),
        }
    } else {
        let encoded;
        let input = if expected_schema >= varta_common::json::UTF16_SCHEMA_VERSION {
            encoded = varta_common::text::json_input_limited(input, limits.json_input_bytes)
                .map_err(error)?;
            &encoded
        } else {
            input
        };
        deep_decode(input, limits, |input| match deadline {
            Some(deadline) => schema.parse_json_until(input, limits, deadline),
            None => schema.parse_json(input),
        })?
        .map_err(error)?
    };
    let writer = heap::Writer::new(&mut caller, memory)?;
    let built = if top_down {
        tree::build(&writer, &mut caller, &schema, &value, limits)
    } else {
        writer.build(&mut caller, &schema, &value)
    };
    // A deep tree is dropped on the JSON host stack, never by host recursion here.
    if top_down && nesting(&value) > DataLimits::LEGACY.json_depth {
        crate::json::deep(move || drop(value))?;
    }
    let value = built?;
    if recoverable {
        exceptions::ready(&mut caller)?;
        exceptions::deadline(&caller)?;
    }
    descriptors(
        &schema,
        memory.data(&caller),
        caller.data().module.static_region,
    )?;
    let Some(Extern::Global(length)) = caller.get_export(abi::RESULT_LEN) else {
        return Err(error("missing result length"));
    };
    let high = if schema.version == varta_common::json::SCHEMA_VERSION
        && schema.types[schema.root as usize].kind().is_reference()
    {
        (value >> i32::BITS) as u32 as i32
    } else {
        0
    };
    length.set(&mut caller, Val::I32(high))?;
    // The value is already owned/rooted; this is not a borrowed result buffer.
    Ok(value)
}
