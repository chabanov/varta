//! Host boundary for the primitive guest exception channel. Resource traps and
//! authorized exits never enter this channel or become catchable guest values.
use crate::{RuntimeContext, RuntimeError};
use varta_common::{exceptions as abi, value::*};
use wasmtime::{ExternType, Instance, Module, Mutability, Store, ValType};
fn invalid(message: &str) -> RuntimeError {
    ValueAbiError(message.into()).into()
}
pub(crate) fn validate(
    module: &Module,
    profile: varta_common::language::LanguageProfile,
) -> Result<(), RuntimeError> {
    let enabled = profile.exceptions();
    let version_name = profile.exception_channel().0;
    for export in module
        .exports()
        .filter(|e| e.name().starts_with(abi::PREFIX))
    {
        if !enabled
            || !([abi::PENDING, abi::VALUE].contains(&export.name())
                || (profile.errors() && export.name() == version_name))
        {
            return Err(invalid("unexpected exception channel export"));
        }
    }
    if !enabled {
        return Ok(());
    }
    if profile.errors() {
        match module.get_export(version_name) {
            Some(ExternType::Global(g))
                if g.mutability() == Mutability::Const && matches!(g.content(), ValType::I32) => {}
            _ => return Err(invalid("Error channel requires immutable version global")),
        }
    }
    for (name, number) in [(abi::PENDING, true), (abi::VALUE, false)] {
        match module.get_export(name) {
            Some(ExternType::Global(g))
                if g.mutability() == Mutability::Var
                    && if number {
                        matches!(g.content(), ValType::I32)
                    } else {
                        matches!(g.content(), ValType::I64)
                    } => {}
            _ => {
                return Err(invalid(
                    "exception channel requires mutable i32/i64 globals",
                ));
            }
        }
    }
    match module.get_export("memory") {
        Some(ExternType::Memory(m))
            if !m.is_64()
                && !m.is_shared()
                && m.minimum()
                    >= (abi::ROOT + abi::RESERVED_ROOT_BYTES)
                        .div_ceil(varta_common::abi::WASM_PAGE_BYTES)
                        as u64 =>
        {
            Ok(())
        }
        _ => Err(invalid("exception channel requires exported 32-bit memory")),
    }
}
pub(crate) fn initial(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
) -> Result<(), wasmtime::Error> {
    for (name, expected) in [
        (abi::VERSION, abi::ERROR_CHANNEL_VERSION),
        (abi::FAMILY_VERSION, abi::FAMILY_CHANNEL_VERSION),
        (abi::BIGINT_VERSION, abi::BIGINT_CHANNEL_VERSION),
    ] {
        if let Some(version) = instance.get_global(&mut *store, name)
            && version.get(&mut *store).i32() != Some(expected)
        {
            return Err(wasmtime::Error::new(ValueAbiError(
                "invalid Error channel version".into(),
            )));
        }
    }
    let Some(pending) = instance.get_global(&mut *store, abi::PENDING) else {
        return Ok(());
    };
    let flag = pending.get(&mut *store).i32();
    let value = instance
        .get_global(&mut *store, abi::VALUE)
        .and_then(|g| g.get(&mut *store).i64());
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| wasmtime::Error::new(ValueAbiError("missing exception memory".into())))?;
    let root = memory
        .data(&*store)
        .get(abi::ROOT..abi::ROOT + abi::RESERVED_ROOT_BYTES);
    if flag != Some(0) || value != Some(0) || root.is_none_or(|bytes| bytes.iter().any(|b| *b != 0))
    {
        return Err(wasmtime::Error::new(ValueAbiError(
            "nonzero initial exception state".into(),
        )));
    }
    Ok(())
}
pub(crate) fn finish(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
) -> Result<(), RuntimeError> {
    let Some(pending) = instance.get_global(&mut *store, abi::PENDING) else {
        return Ok(());
    };
    let flag = pending
        .get(&mut *store)
        .i32()
        .ok_or_else(|| invalid("invalid exception flag"))?;
    let value = instance
        .get_global(&mut *store, abi::VALUE)
        .ok_or_else(|| invalid("missing exception value"))?
        .get(&mut *store)
        .i64()
        .ok_or_else(|| invalid("invalid exception value"))? as u64;
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| invalid("missing exception memory"))?;
    let bigints = instance
        .get_global(&mut *store, abi::BIGINT_VERSION)
        .is_some();
    let families = bigints
        || instance
            .get_global(&mut *store, abi::FAMILY_VERSION)
            .is_some();
    let errors = families || instance.get_global(&mut *store, abi::VERSION).is_some();
    let version = if bigints {
        BIGINT_VALUE_VERSION
    } else if families {
        ERROR_FAMILY_VALUE_VERSION
    } else {
        ERROR_VALUE_VERSION
    };
    let data = memory.data(&*store);
    let root = data
        .get(abi::ROOT..abi::ROOT + abi::RESERVED_ROOT_BYTES)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| invalid("invalid exception root"))?;
    if flag == 0 && value == 0 && root == 0 {
        return Ok(());
    }
    if flag != 1 || value != root {
        return Err(invalid("noncanonical exception channel state"));
    }
    let cell = if errors {
        ReferenceBox::read_with_version(data, value, version)?
    } else {
        let primitive = PrimitiveBox::read(data, value)?;
        ReferenceBox {
            kind: primitive.kind,
            payload: primitive.payload,
        }
    };
    use varta_common::exceptions::ThrownValue as V;
    let value = match cell.kind {
        ValueKind::BigInt => {
            crate::bigint::validate_handle(data, cell.payload as i64)?;
            let start = cell.payload as u32 as usize;
            let len = (cell.payload >> 32) as usize;
            V::BigInt(data[start..start + len].to_vec())
        }
        ValueKind::Float64 => V::Number(cell.payload),
        ValueKind::Boolean => V::Boolean(cell.payload != 0),
        ValueKind::Null => V::Null,
        ValueKind::Undefined => V::Undefined,
        ValueKind::String => {
            let start = cell.payload as u32 as usize;
            let len = (cell.payload >> i32::BITS) as usize;
            // PrimitiveBox checked the complete even byte range and size bound.
            V::String(
                varta_common::text::units(&data[start..start + len])
                    .map_err(|_| invalid("invalid thrown UTF-16 value"))?,
            )
        }
        ValueKind::Error if errors => {
            let object = ErrorObject::read_with_version(data, cell.payload, version)?;
            let start = object.message as u32 as usize;
            let len = (object.message >> i32::BITS) as usize;
            let message = varta_common::text::units(&data[start..start + len])
                .map_err(|_| invalid("invalid Error message"))?;
            if object.class == ErrorClass::Error {
                V::Error {
                    message,
                    has_message: object.has_message,
                }
            } else {
                V::NativeError {
                    class: object.class,
                    message,
                    has_message: object.has_message,
                }
            }
        }
        _ => return Err(invalid("unsupported thrown value kind")),
    };
    Err(varta_common::exceptions::UncaughtException { value }.into())
}
