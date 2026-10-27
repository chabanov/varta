//! Pure UTF-16 intrinsics. Resource conversion lives in the privileged host path.
use crate::{RuntimeContext, host};
use varta_common::{HostOperation as Host, abi, text};
use wasmtime::{Caller, Extern, Linker};

fn read(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u16>, wasmtime::Error> {
    if ptr < 0 || len < 0 || len as usize > abi::MAX_STRING_BYTES {
        return Err(wasmtime::Error::msg("invalid UTF-16 range"));
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(wasmtime::Error::msg("missing memory"));
    };
    let data = memory
        .data(&*caller)
        .get(ptr as usize..(ptr as usize).saturating_add(len as usize))
        .ok_or_else(|| wasmtime::Error::msg("UTF-16 range outside memory"))?;
    text::units(data).map_err(wasmtime::Error::new)
}
fn index(value: f64, length: usize) -> Option<usize> {
    let value = if value.is_nan() { 0.0 } else { value.trunc() };
    if value < 0.0 || value >= length as f64 {
        None
    } else {
        Some(value as usize)
    }
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        Host::LogString16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            // Diagnostic output is Unicode text, never a round-trippable resource identifier.
            caller
                .data_mut()
                .emit(format!("varta log: {}", String::from_utf16_lossy(&units)))
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::StringCharAt16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32, position: f64| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            let bytes = index(position, units.len())
                .map(|i| units[i].to_le_bytes().to_vec())
                .unwrap_or_default();
            host::result_bytes(&mut caller, &bytes)
                .map_err(|_| wasmtime::Error::msg("UTF-16 result transfer failed"))
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::StringCharCodeAt16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32, position: f64| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            Ok(index(position, units.len()).map_or(f64::NAN, |i| f64::from(units[i])))
        },
    )?;
    Ok(())
}
