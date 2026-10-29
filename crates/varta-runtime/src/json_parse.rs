//! Checked JSON decoding followed by allocation through the guest's own bounded
//! collector. Complete input validation precedes the first guest helper call.
mod exceptions;
mod heap;
use crate::RuntimeContext;
use varta_common::{
    HostOperation, abi,
    json::{JsonSchema, JsonType},
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
fn static_bytes(data: &[u8], ptr: usize, len: usize) -> Result<&[u8], wasmtime::Error> {
    if ptr
        .checked_add(len)
        .is_none_or(|end| end > abi::RESULT_BASE)
    {
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
fn descriptors(schema: &JsonSchema, data: &[u8]) -> Result<(), wasmtime::Error> {
    for ty in &schema.types {
        if let JsonType::Record { descriptor, fields } = ty {
            let descriptor = *descriptor as usize;
            static_bytes(data, descriptor, descriptor_bytes(fields.len()))?;
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
                    || static_bytes(data, key as usize, len as usize)? != field.name.as_bytes()
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
    if recoverable && len >= 0 && len as usize > abi::MAX_INPUT_BYTES {
        return Err(wasmtime::Error::new(
            varta_common::json::JsonDecodeError::Limit(varta_common::json::JsonInputLimit::Bytes),
        ));
    }
    if ptr < 0 || len < 0 || len as usize > abi::MAX_INPUT_BYTES || schema_ptr < 0 || schema_len < 0
    {
        return Err(error("input range or byte limit"));
    }
    if recoverable {
        exceptions::ready(&mut caller)?;
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(error("missing memory"));
    };
    let data = memory.data(&caller);
    let schema = JsonSchema::from_bytes(static_bytes(
        data,
        schema_ptr as usize,
        schema_len as usize,
    )?)
    .map_err(error)?;
    if schema.version != expected_schema {
        return Err(error("schema version does not match JSON import"));
    }
    descriptors(&schema, data)?;
    let input = bytes(data, ptr as usize, len as usize)?;
    let value = if recoverable {
        use varta_common::{
            json::{JsonDecodeError, JsonInputLimit},
            text::JsonTextError,
        };
        let parsed = match varta_common::text::json_input_classified(input) {
            Ok(encoded) => schema.parse_json_classified(&encoded),
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
            encoded = varta_common::text::json_input(input).map_err(error)?;
            &encoded
        } else {
            input
        };
        schema.parse_json(input).map_err(error)?
    };
    let writer = heap::Writer::new(&mut caller, memory)?;
    let value = writer.build(&mut caller, &schema, &value)?;
    if recoverable {
        exceptions::ready(&mut caller)?;
        exceptions::deadline(&caller)?;
    }
    descriptors(&schema, memory.data(&caller))?;
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
