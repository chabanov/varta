//! The guest side of a KV decode (`kv_key`, `kv_value`): the checked schema
//! in the module's static region, the classified entry, its fair fuel, and
//! the result built through the guest's own collector, or the recoverable
//! `TypeError` of a mismatch through the exception channel.
//!
//! A host implements [`Guest`] over its instance (the native runtime over
//! Wasmtime, the browser host over the Worker's guest exports); the
//! sequence, every check and every allocation are this module's, in one
//! order for both.
use crate::{
    Deadline,
    build::{self, BuildError},
    decode::{self, Classifier, DecodeError},
    protocol::Stored,
    v8,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use varta_common::{
    abi::{self, StaticRegion},
    json::{JsonSchema, JsonValue, build::Heap},
    limits::{DataLimits, ProgramLimits},
    memory::Memory,
    value::*,
};

/// One guest instance as a KV decode sees it: its heap ([`Heap`]: the
/// guest allocator, retention and plain stores) and memory, its exception
/// channel, its turn deadline and its fair fuel.
pub trait Guest: Heap {
    type Memory: Memory + ?Sized;
    /// Guest memory 0.
    fn memory(&self) -> &Self::Memory;
    /// The exception channel is canonical and clear (no pending exception,
    /// a zero value global and root slot).
    fn ready(&mut self) -> Result<(), Self::Error>;
    /// Fail fatally once the turn deadline has passed.
    fn deadline(&mut self) -> Result<(), Self::Error>;
    /// Charge v38 fair fuel.
    fn charge(&mut self, fuel: u64) -> Result<(), Self::Error>;
    /// Publish `cell` as the pending exception: the result length cleared,
    /// the root slot, the value global and the pending flag set.
    fn throw(&mut self, cell: u64) -> Result<(), Self::Error>;
    /// Set the result length global.
    fn result_length(&mut self, high: i32) -> Result<(), Self::Error>;
    /// Drop a decoded tree (a host may need a deeper stack for a deep one).
    fn dispose(&mut self, value: JsonValue) -> Result<(), Self::Error>;
    /// The fatal error of a malformed checked JSON request
    /// (`checked JSON: <message>`).
    fn json_fatal(&self, message: String) -> Self::Error;
    /// The fatal error of a malformed KV request (`kv: <message>`).
    fn kv_fatal(&self, message: &'static str) -> Self::Error;
    /// The fatal error of a layout reader.
    fn abi_fatal(&self, error: ValueAbiError) -> Self::Error;
    /// The fatal error of host work stopped by the turn deadline.
    fn deadline_error(&self) -> Self::Error;
}

/// What a decode reads: an entry's key, or its value.
#[derive(Debug, Clone, Copy)]
pub enum Entry<'a> {
    Key(&'a [u8]),
    Value(&'a Stored),
}

/// The v38 checked schema at `ptr` in the static region (schema 18, the one
/// `json_parse_v19` reads), its record and dictionary descriptors checked,
/// with a ready exception channel: the target of a KV decode. `read` reads
/// schema bytes within the module's limits (the host's schema reader).
pub fn schema<G: Guest>(
    guest: &mut G,
    module: ModuleBounds,
    ptr: i32,
    len: i32,
    read: impl FnOnce(&[u8], &ProgramLimits) -> Result<JsonSchema, String>,
) -> Result<JsonSchema, G::Error> {
    if ptr < 0 || len < 0 {
        return Err(guest.json_fatal("schema range".into()));
    }
    let region = module.static_region;
    let (ptr, len) = (ptr as usize, len as usize);
    if !region.contains(ptr, len) {
        return Err(guest.json_fatal("metadata outside static memory".into()));
    }
    let Some(bytes) = guest.memory().borrow(ptr, len) else {
        return Err(guest.json_fatal("memory range".into()));
    };
    let schema = match read(&bytes, &module.limits) {
        Ok(schema) => schema,
        Err(message) => return Err(guest.json_fatal(message)),
    };
    if schema.version != varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION {
        return Err(guest.json_fatal("schema version does not match the KV import".into()));
    }
    descriptors(guest, &schema, region)?;
    guest.ready()?;
    Ok(schema)
}

fn descriptors<G: Guest>(
    guest: &mut G,
    schema: &JsonSchema,
    region: StaticRegion,
) -> Result<(), G::Error> {
    schema
        .check_descriptors(guest.memory(), region)
        .map_err(|error| guest.json_fatal(error.to_string()))
}

/// The decoded value of `entry` and the values classified, before any
/// guest allocation.
pub fn classify<D: Deadline + ?Sized>(
    schema: &JsonSchema,
    limits: DataLimits,
    deadline: &D,
    entry: Entry<'_>,
) -> (Result<JsonValue, DecodeError>, usize) {
    let value = match entry {
        Entry::Key(bytes) => decode::key_value(bytes),
        Entry::Value(Stored::V8(bytes)) => v8::decode(
            bytes,
            v8::Limits {
                depth: limits.json_depth,
                nodes: limits.json_nodes,
                elements: limits.array_elements,
                string_units: limits.string_bytes / 2,
            },
        )
        .map_err(|error| match error {
            // The shared depth limit is named alike on every walk.
            v8::V8Error::Depth(_) => DecodeError::Mismatch(crate::too_deep()),
            error => DecodeError::Mismatch(format!("invalid stored value: {error}")),
        }),
        Entry::Value(Stored::U64(_)) => {
            Err(DecodeError::Mismatch("the stored value is a KvU64".into()))
        }
        Entry::Value(Stored::Bytes(_)) => Err(DecodeError::Mismatch(
            "the stored value is a Uint8Array".into(),
        )),
    };
    let mut classifier = Classifier::new(schema, limits, deadline);
    let classified = value.and_then(|value| classifier.root(&value));
    (classified, classifier.values())
}

/// `kv_key` (`key`) or `kv_value` of `entry` (`None`: no such entry, fatal
/// once the schema is checked) into the checked schema at `schema_ptr`: the
/// result handle, or 0 with a pending `TypeError` when the stored value does
/// not match.
pub fn decode<G: Guest, D: Deadline + ?Sized>(
    guest: &mut G,
    module: ModuleBounds,
    deadline: &D,
    (schema_ptr, schema_len): (i32, i32),
    entry: Option<Entry<'_>>,
    read: impl FnOnce(&[u8], &ProgramLimits) -> Result<JsonSchema, String>,
) -> Result<u64, G::Error> {
    let schema = schema(guest, module, schema_ptr, schema_len, read)?;
    let Some(entry) = entry else {
        return Err(guest.kv_fatal("entry index"));
    };
    let limits = module.limits.data;
    let (classified, values) = classify(&schema, limits, deadline, entry);
    // V38 fair fuel: the schema and the values decoded, before the first
    // guest allocation (as checked JSON charges).
    guest.charge(crate::charge::decoded(&schema, values))?;
    match classified {
        Ok(value) => materialize(guest, module, &schema, value),
        Err(DecodeError::Mismatch(message)) => {
            let message = format!(
                "KV {} does not match the checked type: {message}",
                if matches!(entry, Entry::Key(_)) {
                    "key"
                } else {
                    "value"
                }
            );
            type_error(guest, module, &schema, &message)
        }
        Err(DecodeError::Deadline) => Err(guest.deadline_error()),
    }
}

/// Allocate a completely checked tree through the guest's own collector,
/// then check the module's metadata again and publish the handle.
pub fn materialize<G: Guest>(
    guest: &mut G,
    module: ModuleBounds,
    schema: &JsonSchema,
    value: JsonValue,
) -> Result<u64, G::Error> {
    let built = build::value(guest, schema, &value, module.limits.data);
    guest.dispose(value)?;
    let value = built.map_err(|error| match error {
        BuildError::Heap(error) => error,
        BuildError::Invalid(message) => guest.json_fatal(message.into()),
    })?;
    guest.ready()?;
    guest.deadline()?;
    descriptors(guest, schema, module.static_region)?;
    let high = if schema.version == varta_common::json::SCHEMA_VERSION
        && schema.types[schema.root as usize].kind().is_reference()
    {
        (value >> i32::BITS) as u32 as i32
    } else {
        0
    };
    guest.result_length(high)?;
    // The value is already owned/rooted; this is not a borrowed result buffer.
    Ok(value)
}

/// The bounds `ErrorObject::read_with_version` reads an Error object under
/// (`ModuleBounds::LEGACY`, which needs `std`): of these the reader reads
/// only the string bound, the legacy `abi::MAX_STRING_BYTES`.
fn legacy_error_bounds() -> ModuleBounds {
    ModuleBounds {
        static_region: StaticRegion::SINGLE_PAGE,
        limits: ProgramLimits {
            data: DataLimits {
                string_bytes: abi::MAX_STRING_BYTES,
                ..DataLimits::V38
            },
            ..varta_common::limits::WASM
        },
    }
}

/// The error of a noncanonical exception channel.
fn channel() -> ValueAbiError {
    ValueAbiError("noncanonical JSON exception channel".into())
}

/// Publish a recoverable `class` error with `message` (`SyntaxError` or
/// `TypeError`, as checked JSON does) through the exception channel; the
/// result is the placeholder 0 the caller unpacks only without a pending
/// exception.
pub fn error<G: Guest>(
    guest: &mut G,
    module: ModuleBounds,
    schema: &JsonSchema,
    class: ErrorClass,
    message: &str,
) -> Result<u64, G::Error> {
    if !matches!(class, ErrorClass::SyntaxError | ErrorClass::TypeError) {
        return Err(guest.abi_fatal(channel()));
    }
    guest.ready()?;
    let message = build::publish(guest, &varta_common::text::encode(message), 0, 0, true)?;
    let mut object = Vec::with_capacity(ERROR_BYTES);
    object.extend((((class as u32) << ERROR_CLASS_SHIFT) | ERROR_OWN_MESSAGE).to_le_bytes());
    object.extend(1u32.to_le_bytes());
    object.extend(message.to_le_bytes());
    let object = build::publish(guest, &object, HEAP_ERROR_FLAG, 1, true)?;
    let mut cell = Vec::with_capacity(UNION_BYTES);
    cell.extend((ValueKind::Error as u32).to_le_bytes());
    cell.extend(1u32.to_le_bytes());
    cell.extend(object.to_le_bytes());
    let cell = build::publish(guest, &cell, HEAP_UNION_FLAG, 1, true)?;
    // Allocators/retainers are untrusted Wasm, even though their call graphs are
    // host-free. Validate their final product and all metadata before publication.
    guest.ready()?;
    guest.deadline()?;
    descriptors(guest, schema, module.static_region)?;
    let value = ReferenceBox::read_in(guest.memory(), cell, schema.value_version(), module)
        .map_err(|error| guest.abi_fatal(error))?;
    if value.kind != ValueKind::Error || value.payload != object {
        return Err(guest.abi_fatal(channel()));
    }
    let checked = ErrorObject::read_in(
        guest.memory(),
        object,
        schema.value_version(),
        legacy_error_bounds(),
    )
    .map_err(|error| guest.abi_fatal(error))?;
    if checked.class != class || !checked.has_message || checked.message != message {
        return Err(guest.abi_fatal(channel()));
    }
    guest.throw(cell)?;
    // Caller checks pending before unpacking this physical placeholder.
    Ok(0)
}

/// Publish a recoverable `TypeError` with `message` from a v38 KV decode.
pub fn type_error<G: Guest>(
    guest: &mut G,
    module: ModuleBounds,
    schema: &JsonSchema,
    message: &str,
) -> Result<u64, G::Error> {
    error(guest, module, schema, ErrorClass::TypeError, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_error_bounds_keep_the_legacy_string_limit() {
        assert_eq!(
            legacy_error_bounds().limits.data.string_bytes,
            abi::MAX_STRING_BYTES
        );
    }
}
