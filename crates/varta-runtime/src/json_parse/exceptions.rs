//! Only declared syntax/schema-mismatch failures enter the guest channel. Helper,
//! ABI, metadata and resource failures return through the fatal execution path.
use super::*;
use varta_common::exceptions as channel;
fn invalid() -> wasmtime::Error {
    wasmtime::Error::new(ValueAbiError("noncanonical JSON exception channel".into()))
}
pub(super) fn deadline(caller: &Caller<'_, RuntimeContext>) -> Result<(), wasmtime::Error> {
    if std::time::Instant::now() >= caller.data().deadline {
        Err(error("execution deadline"))
    } else {
        Ok(())
    }
}
pub(super) fn ready(caller: &mut Caller<'_, RuntimeContext>) -> Result<(), wasmtime::Error> {
    let selector = if caller.get_export(channel::PROMISE_VERSION).is_some() {
        (channel::PROMISE_VERSION, channel::PROMISE_CHANNEL_VERSION)
    } else if caller.get_export(channel::DYNAMIC_VERSION).is_some() {
        (channel::DYNAMIC_VERSION, channel::DYNAMIC_CHANNEL_VERSION)
    } else if caller.get_export(channel::CLASS_ERROR_VERSION).is_some() {
        (
            channel::CLASS_ERROR_VERSION,
            channel::CLASS_ERROR_CHANNEL_VERSION,
        )
    } else if caller.get_export(channel::BIGINT_VERSION).is_some() {
        (channel::BIGINT_VERSION, channel::BIGINT_CHANNEL_VERSION)
    } else {
        (channel::FAMILY_VERSION, channel::FAMILY_CHANNEL_VERSION)
    };
    for (name, expected) in [selector, (channel::PENDING, 0)] {
        let Some(Extern::Global(global)) = caller.get_export(name) else {
            return Err(invalid());
        };
        if global.get(&mut *caller).i32() != Some(expected) {
            return Err(invalid());
        }
    }
    let Some(Extern::Global(value)) = caller.get_export(channel::VALUE) else {
        return Err(invalid());
    };
    if value.get(&mut *caller).i64() != Some(0) {
        return Err(invalid());
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(invalid());
    };
    if bytes(
        memory.data(&*caller),
        channel::ROOT,
        channel::RESERVED_ROOT_BYTES,
    )?
    .iter()
    .any(|byte| *byte != 0)
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn publish(
    caller: &mut Caller<'_, RuntimeContext>,
    memory: wasmtime::Memory,
    schema: &JsonSchema,
    class: ErrorClass,
) -> Result<u64, wasmtime::Error> {
    ready(caller)?;
    let message = match class {
        ErrorClass::SyntaxError => "Invalid JSON syntax",
        ErrorClass::TypeError => "JSON value does not match the checked schema",
        _ => return Err(invalid()),
    };
    let writer = heap::Writer::new(caller, memory)?;
    let message = writer.owned(caller, &varta_common::text::encode(message), 0, 0)?;
    let mut object = Vec::with_capacity(ERROR_BYTES);
    object.extend((((class as u32) << ERROR_CLASS_SHIFT) | ERROR_OWN_MESSAGE).to_le_bytes());
    object.extend(1u32.to_le_bytes());
    object.extend(message.to_le_bytes());
    let object = writer.owned(caller, &object, HEAP_ERROR_FLAG, 1)?;
    let mut cell = Vec::with_capacity(UNION_BYTES);
    cell.extend((ValueKind::Error as u32).to_le_bytes());
    cell.extend(1u32.to_le_bytes());
    cell.extend(object.to_le_bytes());
    let cell = writer.owned(caller, &cell, HEAP_UNION_FLAG, 1)?;
    // Allocators/retainers are untrusted Wasm, even though their call graphs are
    // host-free. Validate their final product and all metadata before publication.
    ready(caller)?;
    deadline(caller)?;
    descriptors(schema, memory.data(&*caller))?;
    let value =
        ReferenceBox::read_with_version(memory.data(&*caller), cell, schema.value_version())
            .map_err(wasmtime::Error::new)?;
    if value.kind != ValueKind::Error || value.payload != object {
        return Err(invalid());
    }
    let checked =
        ErrorObject::read_with_version(memory.data(&*caller), object, schema.value_version())
            .map_err(wasmtime::Error::new)?;
    if checked.class != class || !checked.has_message || checked.message != message {
        return Err(invalid());
    }
    let Some(Extern::Global(value)) = caller.get_export(channel::VALUE) else {
        return Err(invalid());
    };
    let Some(Extern::Global(pending)) = caller.get_export(channel::PENDING) else {
        return Err(invalid());
    };
    let Some(Extern::Global(length)) = caller.get_export(abi::RESULT_LEN) else {
        return Err(invalid());
    };
    length.set(&mut *caller, Val::I32(0))?;
    memory.write(&mut *caller, channel::ROOT, &cell.to_le_bytes())?;
    value.set(&mut *caller, Val::I64(cell as i64))?;
    pending.set(&mut *caller, Val::I32(1))?;
    // Caller checks pending before unpacking this physical placeholder.
    Ok(0)
}
