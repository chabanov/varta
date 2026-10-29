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
        (abi::CLASS_ERROR_VERSION, abi::CLASS_ERROR_CHANNEL_VERSION),
        (abi::DYNAMIC_VERSION, abi::DYNAMIC_CHANNEL_VERSION),
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
    // Channel v6 (v34) carries the v5 contents in value15 layouts.
    let dynamic = instance
        .get_global(&mut *store, abi::DYNAMIC_VERSION)
        .is_some();
    let instances = dynamic
        || instance
            .get_global(&mut *store, abi::CLASS_ERROR_VERSION)
            .is_some();
    let bigints = instances
        || instance
            .get_global(&mut *store, abi::BIGINT_VERSION)
            .is_some();
    let families = bigints
        || instance
            .get_global(&mut *store, abi::FAMILY_VERSION)
            .is_some();
    let errors = families || instance.get_global(&mut *store, abi::VERSION).is_some();
    let version = if dynamic {
        DYNAMIC_VALUE_VERSION
    } else if bigints {
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
        ValueKind::Record if instances => error_instance(data, cell.payload)?,
        _ => return Err(invalid("unsupported thrown value kind")),
    };
    Err(varta_common::exceptions::UncaughtException { value }.into())
}

/// Field keys a thrown class-instance record must declare as String slots.
const INSTANCE_NAME: &str = "name";
const INSTANCE_MESSAGE: &str = "message";

/// Copy the `name`/`message` String fields of a thrown class instance (channel
/// v5). `ReferenceBox` has already validated the record header, descriptor
/// range, keys, kinds and presence; this reads only those checked words.
fn error_instance(
    data: &[u8],
    record: u64,
) -> Result<varta_common::exceptions::ThrownValue, RuntimeError> {
    let word = |at: usize| {
        at.checked_add(size_of::<u32>())
            .and_then(|end| data.get(at..end))
            .and_then(|s| s.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| invalid("invalid thrown Error instance"))
    };
    let ptr = record as u32 as usize;
    let descriptor = word(ptr)? as usize;
    let count = word(ptr + size_of::<u32>())? as usize;
    let mut fields = [None, None];
    for index in 0..count {
        let entry = descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY;
        let key = word(entry)? as usize;
        let len = word(entry + 4)? as usize;
        let raw = word(entry + 8)?;
        let kind = word(entry + 12)?;
        let position = match key.checked_add(len).and_then(|end| data.get(key..end)) {
            Some(bytes) if bytes == INSTANCE_NAME.as_bytes() => 0,
            Some(bytes) if bytes == INSTANCE_MESSAGE.as_bytes() => 1,
            _ => continue,
        };
        if raw & OPTIONAL_FIELD_FLAG != 0 || kind != ValueKind::String as u32 {
            return Err(invalid("thrown Error instance field is not a string"));
        }
        let slot = ptr + RECORD_META_BYTES + raw as usize * RECORD_SLOT_BYTES;
        let handle = (u64::from(word(slot + 4)?) << i32::BITS) | u64::from(word(slot)?);
        let start = handle as u32 as usize;
        let len = (handle >> i32::BITS) as usize;
        if len > varta_common::abi::MAX_STRING_BYTES || !len.is_multiple_of(2) {
            return Err(invalid("invalid thrown Error instance field"));
        }
        let bytes = start
            .checked_add(len)
            .and_then(|end| data.get(start..end))
            .ok_or_else(|| invalid("invalid thrown Error instance field"))?;
        fields[position] = Some(
            varta_common::text::units(bytes)
                .map_err(|_| invalid("invalid thrown Error instance field"))?,
        );
    }
    let [Some(name), Some(message)] = fields else {
        return Err(invalid("thrown record is not an Error instance"));
    };
    Ok(varta_common::exceptions::ThrownValue::ErrorInstance { name, message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::exceptions::ThrownValue;

    const RECORD: usize = 64;
    const DESCRIPTOR: usize = 256;
    const KEYS: usize = 400;
    const TEXT: usize = 440;

    fn put(data: &mut [u8], at: usize, word: u32) {
        data[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }
    /// A two-field record `{name, message}` whose name is "Ab" and message empty.
    fn instance() -> Vec<u8> {
        let mut data = vec![0; 512];
        put(&mut data, RECORD, DESCRIPTOR as u32);
        put(&mut data, RECORD + 4, 2);
        let message_key = KEYS + INSTANCE_NAME.len();
        data[KEYS..message_key].copy_from_slice(INSTANCE_NAME.as_bytes());
        data[message_key..message_key + INSTANCE_MESSAGE.len()]
            .copy_from_slice(INSTANCE_MESSAGE.as_bytes());
        for (index, (key, len)) in [
            (KEYS, INSTANCE_NAME.len()),
            (message_key, INSTANCE_MESSAGE.len()),
        ]
        .into_iter()
        .enumerate()
        {
            let entry = DESCRIPTOR + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY;
            put(&mut data, entry, key as u32);
            put(&mut data, entry + 4, len as u32);
            put(&mut data, entry + 8, index as u32);
            put(&mut data, entry + 12, ValueKind::String as u32);
        }
        let units: Vec<u8> = "Ab".encode_utf16().flat_map(u16::to_le_bytes).collect();
        data[TEXT..TEXT + units.len()].copy_from_slice(&units);
        put(&mut data, RECORD + RECORD_META_BYTES, TEXT as u32);
        put(
            &mut data,
            RECORD + RECORD_META_BYTES + 4,
            units.len() as u32,
        );
        data
    }
    fn handle() -> u64 {
        RECORD as u64
    }

    #[test]
    fn thrown_instances_copy_name_and_message_fields() {
        assert_eq!(
            error_instance(&instance(), handle()).unwrap(),
            ThrownValue::ErrorInstance {
                name: "Ab".encode_utf16().collect(),
                message: Vec::new(),
            }
        );
    }

    #[test]
    fn thrown_instances_reject_missing_or_noncanonical_fields() {
        let entry =
            |index: usize| DESCRIPTOR + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY;
        let corruptions: [(usize, u32); 5] = [
            // The message key no longer names `message`.
            (entry(1) + 4, INSTANCE_MESSAGE.len() as u32 - 1),
            // The name field is optional or not a string.
            (entry(0) + 8, OPTIONAL_FIELD_FLAG),
            (entry(0) + 12, ValueKind::Float64 as u32),
            // The name string has an odd byte length or lies outside memory.
            (RECORD + RECORD_META_BYTES + 4, 3),
            (RECORD + RECORD_META_BYTES, u32::MAX - 1),
        ];
        for (at, word) in corruptions {
            let mut data = instance();
            put(&mut data, at, word);
            assert!(error_instance(&data, handle()).is_err(), "{at}");
        }
        // Descriptor entries past the end of memory fail instead of panicking.
        let mut data = instance();
        put(&mut data, RECORD, u32::MAX - 8);
        assert!(error_instance(&data, handle()).is_err());
    }
}
