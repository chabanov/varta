//! V38 (`data_limits`) `json_stringify_owned`: `JSON.stringify` without the
//! 1 MiB host result buffer. The value is serialized under the module's data
//! limits into a host buffer capped at the memory budget (the result has to
//! fit in guest memory anyway), then copied in bounded chunks into an owned
//! UTF-16 string the module's host-free allocator provides; the string is
//! retained through the value ABI retention helper before the import returns
//! its packed handle. No host buffer outlives the call.
use super::*;
use crate::json_parse::heap::Writer;

/// Native stack of the thread that serializes or decodes a value nested
/// deeper than the legacy depth: every host stack sustains the legacy depth
/// of 64, only this dedicated stack sustains
/// [`varta_common::limits::V38_MAX_JSON_DEPTH`]. Measured at that depth, an
/// unoptimized build overflows 16 MiB and fits 64 MiB, so this reservation
/// (committed only as touched) keeps at least a fourfold margin.
pub(crate) const JSON_HOST_STACK_BYTES: usize = 256 << 20;
/// Bytes of encoded text the serializer keeps at most: the memory budget.
const OUTPUT_LIMIT: usize = abi::MAX_MEMORY_BYTES;

pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        varta_common::HostOperation::JsonStringifyOwned.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         kind: i32,
         lo: i32,
         hi: i32|
         -> Result<(i64, i32), wasmtime::Error> {
            caller.data_mut().tick()?;
            let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                return Err(error("missing memory"));
            };
            let packed = ((hi as u32 as u64) << i32::BITS) | (lo as u32 as u64);
            let limits = caller.data().data;
            let deadline = caller.data().deadline;
            let encoded = stringify(memory.data(&caller), kind as u32, packed, limits, deadline);
            caller.data().check_budget()?;
            match encoded {
                Ok(Some(bytes)) => {
                    let text = std::str::from_utf8(&bytes).map_err(|_| error("invalid UTF-8"))?;
                    let writer = Writer::new(&mut caller, memory)?;
                    Ok((writer.owned_text(&mut caller, text, limits)? as i64, 0))
                }
                Ok(None) => Ok((0, 0)),
                Err(error) if error.downcast_ref::<BigIntSerialization>().is_some() => {
                    Ok((0, ErrorClass::TypeError as i32))
                }
                Err(error) => Err(error),
            }
        },
    )?;
    Ok(())
}

/// Serialize with private state hidden, as `json_stringify_v17` does. The
/// first attempt runs on the calling thread within the legacy depth; a
/// deeper value is serialized again, from the start, on a dedicated thread
/// whose stack sustains the profile's depth limit. The encoder is pure and
/// deterministic, so the retry observes exactly the same graph.
pub(crate) fn stringify(
    data: &[u8],
    kind: u32,
    packed: u64,
    limits: DataLimits,
    deadline: std::time::Instant,
) -> Result<Option<Vec<u8>>, wasmtime::Error> {
    let encode = |limits: DataLimits| {
        let mut encoder = Encoder::with_version(data, FUNCTION_FIELD_VALUE_VERSION);
        encoder.hide_private = true;
        encoder.limits = limits;
        encoder.output.1 = OUTPUT_LIMIT;
        encoder.deadline = Some(deadline);
        encoder.encode_optional(kind, packed)
    };
    let shallow = DataLimits {
        json_depth: limits.json_depth.min(DataLimits::LEGACY.json_depth),
        ..limits
    };
    match encode(shallow) {
        Err(error)
            if limits.json_depth > shallow.json_depth
                && error
                    .downcast_ref::<TraversalLimit>()
                    .is_some_and(|limit| limit.depth) =>
        {
            deep(|| encode(limits)).and_then(|result| match result {
                Err(error)
                    if error
                        .downcast_ref::<TraversalLimit>()
                        .is_some_and(|limit| limit.depth) =>
                {
                    Err(wasmtime::Error::new(DepthCeiling(limits.json_depth)))
                }
                other => other,
            })
        }
        other => other,
    }
}

/// Run a JSON traversal on the dedicated JSON host stack.
pub(crate) fn deep<T: Send>(work: impl FnOnce() -> T + Send) -> Result<T, wasmtime::Error> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("varta-json".into())
            .stack_size(JSON_HOST_STACK_BYTES)
            .spawn_scoped(scope, work)
            .map_err(|_| error("JSON host stack unavailable"))?
            .join()
            .map_err(|_| error("JSON host stack traversal failed"))
    })
}
