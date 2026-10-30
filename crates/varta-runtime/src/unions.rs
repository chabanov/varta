//! Pure operations over canonical primitive cells. No casts or OS authority.
use crate::{RuntimeContext, host, number};
use varta_common::{
    HostOperation as Host,
    value::{PrimitiveBox, ValueKind as Kind},
};
use wasmtime::{Caller, Extern, Linker};

fn data<'a>(caller: &'a mut Caller<'_, RuntimeContext>) -> Result<&'a [u8], wasmtime::Error> {
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(wasmtime::Error::msg("missing union memory"));
    };
    Ok(memory.data(&*caller))
}
fn read(
    caller: &mut Caller<'_, RuntimeContext>,
    value: i64,
) -> Result<PrimitiveBox, wasmtime::Error> {
    let module = caller.data().module;
    PrimitiveBox::read_in(data(caller)?, value as u64, module).map_err(wasmtime::Error::new)
}
fn nullish(cell: PrimitiveBox) -> bool {
    matches!(cell.kind, Kind::Null | Kind::Undefined)
}
fn text(bytes: &[u8], cell: PrimitiveBox) -> &[u8] {
    // PrimitiveBox::read already checks the complete string range and even length.
    &bytes[cell.payload as u32 as usize
        ..cell.payload as u32 as usize + (cell.payload >> i32::BITS) as usize]
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        Host::UnionLog.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i64| {
            caller.data_mut().tick()?;
            let cell = read(&mut caller, value)?;
            let display = match cell.kind {
                Kind::Null => "null".into(),
                Kind::Undefined => "undefined".into(),
                Kind::Boolean => cell.payload.to_string(),
                Kind::Float64 => number::display(f64::from_bits(cell.payload)),
                Kind::String => String::from_utf16_lossy(
                    &varta_common::text::units(text(data(&mut caller)?, cell))
                        .map_err(wasmtime::Error::new)?,
                ),
                _ => return Err(wasmtime::Error::msg("invalid union kind")),
            };
            caller.data_mut().emit(format!("varta log: {display}"))
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::UnionTypeOf.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i64| {
            caller.data_mut().tick()?;
            let cell = read(&mut caller, value)?;
            let name = match cell.kind {
                Kind::Null => "object",
                Kind::Undefined => "undefined",
                Kind::Float64 => "number",
                Kind::String => "string",
                Kind::Boolean => "boolean",
                _ => return Err(wasmtime::Error::msg("invalid union kind")),
            };
            host::result16(&mut caller, name.as_bytes())
                .map_err(|_| wasmtime::Error::msg("union typeof result transfer"))
        },
    )?;
    for (operation, is_nullish) in [(Host::UnionTruthy, false), (Host::UnionNullish, true)] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>, value: i64| {
                caller.data_mut().tick()?;
                let cell = read(&mut caller, value)?;
                let result = if is_nullish {
                    nullish(cell)
                } else {
                    match cell.kind {
                        Kind::Null | Kind::Undefined => false,
                        Kind::Boolean => cell.payload != 0,
                        Kind::Float64 => {
                            let n = f64::from_bits(cell.payload);
                            n != 0.0 && !n.is_nan()
                        }
                        Kind::String => cell.payload >> i32::BITS != 0,
                        _ => false,
                    }
                };
                Ok(i32::from(result))
            },
        )?;
    }
    for (operation, is_nullish) in [
        (Host::UnionStrictEqual, false),
        (Host::UnionNullishEqual, true),
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>, a: i64, b: i64| {
                caller.data_mut().tick()?;
                let a = read(&mut caller, a)?;
                let b = read(&mut caller, b)?;
                let equal = if is_nullish {
                    nullish(a) && nullish(b)
                } else if a.kind != b.kind {
                    false
                } else {
                    match a.kind {
                        Kind::Float64 => f64::from_bits(a.payload) == f64::from_bits(b.payload),
                        Kind::String => {
                            let bytes = data(&mut caller)?;
                            text(bytes, a) == text(bytes, b)
                        }
                        _ => a.payload == b.payload,
                    }
                };
                Ok(i32::from(equal))
            },
        )?;
    }
    Ok(())
}
