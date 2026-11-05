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
    let selector = if caller
        .get_export(channel::HASHED_DICTIONARY_VERSION)
        .is_some()
    {
        (
            channel::HASHED_DICTIONARY_VERSION,
            channel::HASHED_DICTIONARY_CHANNEL_VERSION,
        )
    } else if caller.get_export(channel::FUNCTION_FIELD_VERSION).is_some() {
        (
            channel::FUNCTION_FIELD_VERSION,
            channel::FUNCTION_FIELD_CHANNEL_VERSION,
        )
    } else if caller.get_export(channel::PROMISE_VERSION).is_some() {
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
    let Some(Extern::Memory(memory)) = caller.get_export(varta_common::abi::MEMORY_EXPORT) else {
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
    let message = match class {
        ErrorClass::SyntaxError => "Invalid JSON syntax",
        ErrorClass::TypeError => "JSON value does not match the checked schema",
        _ => return Err(invalid()),
    };
    publish_message(caller, memory, schema, class, message)
}
/// Publish a recoverable `class` error with `message` (the checked JSON
/// messages above) through the shared value layer, as a v38 KV decode does.
pub(super) fn publish_message(
    caller: &mut Caller<'_, RuntimeContext>,
    memory: wasmtime::Memory,
    schema: &JsonSchema,
    class: ErrorClass,
    message: &str,
) -> Result<u64, wasmtime::Error> {
    // The shared value layer builds and checks the Error cell and throws it.
    let mut host = super::guest::Host::new(caller, memory);
    let module = host.module();
    varta_kv_codec::guest::error(&mut host, module, schema, class, message)
}
