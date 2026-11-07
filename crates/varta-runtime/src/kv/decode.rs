//! Native adapters of the shared synchronous/asynchronous value codec.
use super::*;
type Entry = (Vec<u8>, Option<varta_kv_codec::protocol::Stored>);
fn prepare(
    caller: &mut Caller<'_, RuntimeContext>,
    index: f64,
    key: bool,
) -> Result<(wasmtime::Memory, Option<Entry>, Deadline), wasmtime::Error> {
    caller.data_mut().tick()?;
    let Some(Extern::Memory(memory)) = caller.get_export(varta_common::abi::MEMORY_EXPORT) else {
        return Err(fatal("missing memory"));
    };
    let context = caller.data();
    let entry = entry_at(&context.kv, index).ok().map(|entry| {
        if key {
            (entry.key.clone(), None)
        } else {
            (Vec::new(), Some(stored(&entry.value)))
        }
    });
    Ok((memory, entry, Deadline(context.deadline)))
}
fn entry_view(entry: &Option<Entry>) -> Option<guest::Entry<'_>> {
    entry.as_ref().map(|(key, value)| match value {
        Some(value) => guest::Entry::Value(value),
        None => guest::Entry::Key(key),
    })
}
fn schema(
    bytes: &[u8],
    limits: &varta_common::limits::ProgramLimits,
) -> Result<varta_common::json::JsonSchema, String> {
    varta_common::json::JsonSchema::from_bytes_within(bytes, limits).map_err(|e| e.to_string())
}
pub(super) fn decoded(
    caller: &mut Caller<'_, RuntimeContext>,
    index: f64,
    ptr: i32,
    len: i32,
    key: bool,
) -> Result<i64, wasmtime::Error> {
    let (memory, entry, deadline) = prepare(caller, index, key)?;
    let mut host = crate::json_parse::guest::Host::new(caller, memory);
    let module = host.module();
    guest::decode(
        &mut host,
        module,
        &deadline,
        (ptr, len),
        entry_view(&entry),
        schema,
    )
    .map(|v| v as i64)
}
pub(super) async fn decoded_async(
    caller: &mut Caller<'_, RuntimeContext>,
    index: f64,
    ptr: i32,
    len: i32,
    key: bool,
) -> Result<i64, wasmtime::Error> {
    let (memory, entry, deadline) = prepare(caller, index, key)?;
    let mut host = crate::json_parse::guest::Host::new(caller, memory);
    let module = host.module();
    guest::decode_async(
        &mut host,
        module,
        &deadline,
        (ptr, len),
        entry_view(&entry),
        schema,
    )
    .await
    .map(|v| v as i64)
}
