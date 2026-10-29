//! Pure bounded serialization of the versioned guest value ABI. No OS access,
//! native pointers, eval, arbitrary object hooks or host authority are involved.
mod presence;
use crate::{RuntimeContext, host};
use std::{collections::BTreeSet, io::Write};
use varta_common::{abi, value::*};
use wasmtime::{Caller, Extern, Linker};

fn error(message: &str) -> wasmtime::Error {
    wasmtime::Error::msg(format!("JSON serialization: {message}"))
}

#[derive(Debug, thiserror::Error)]
#[error("JSON cannot serialize BigInt")]
struct BigIntSerialization;

pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    for (operation, version) in [
        (varta_common::HostOperation::JsonStringify, VALUE_VERSION),
        (
            varta_common::HostOperation::JsonStringifyV3,
            NUMBER_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV4,
            UNDEFINED_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV5,
            UTF16_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV6,
            UNION_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV7,
            MISSING_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV8,
            REFERENCE_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV9,
            VECTOR_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV10,
            ERROR_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV12,
            SPARSE_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV11,
            ERROR_FAMILY_VALUE_VERSION,
        ),
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>,
                  kind: i32,
                  lo: i32,
                  hi: i32|
                  -> Result<i32, wasmtime::Error> {
                caller.data_mut().tick()?;
                let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                    return Err(error("missing memory"));
                };
                let packed = ((hi as u32 as u64) << i32::BITS) | (lo as u32 as u64);
                let encoder = if version == VALUE_VERSION {
                    Encoder::new(memory.data(&caller))
                } else {
                    Encoder::with_version(memory.data(&caller), version)
                };
                let bytes = if version >= UNDEFINED_VALUE_VERSION {
                    encoder.encode_optional(kind as u32, packed)?
                } else {
                    Some(encoder.encode(kind as u32, packed)?)
                };
                match bytes {
                    Some(bytes) => (if version >= UTF16_VALUE_VERSION {
                        host::result16(&mut caller, &bytes)
                    } else {
                        host::result(&mut caller, &bytes)
                    })
                    .map_err(|_| error("result transfer failed")),
                    None => {
                        host::result(&mut caller, &[])
                            .map_err(|_| error("result transfer failed"))?;
                        Ok(0)
                    }
                }
            },
        )?;
    }
    for (operation, version) in [
        (
            varta_common::HostOperation::JsonStringifyV13,
            BIGINT_VALUE_VERSION,
        ),
        (
            varta_common::HostOperation::JsonStringifyV14,
            DYNAMIC_VALUE_VERSION,
        ),
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>,
                  kind: i32,
                  lo: i32,
                  hi: i32|
                  -> Result<(i32, i32), wasmtime::Error> {
                caller.data_mut().tick()?;
                let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                    return Err(error("missing memory"));
                };
                let packed = ((hi as u32 as u64) << 32) | (lo as u32 as u64);
                let encoded = Encoder::with_version(memory.data(&caller), version)
                    .encode_optional(kind as u32, packed);
                caller.data().check_budget()?;
                match encoded {
                    Ok(Some(bytes)) => Ok((
                        host::result16(&mut caller, &bytes)
                            .map_err(|_| error("result transfer failed"))?,
                        0,
                    )),
                    Ok(None) => {
                        host::result(&mut caller, &[])
                            .map_err(|_| error("result transfer failed"))?;
                        Ok((0, 0))
                    }
                    Err(error) if error.downcast_ref::<BigIntSerialization>().is_some() => {
                        Ok((0, ErrorClass::TypeError as i32))
                    }
                    Err(error) => Err(error),
                }
            },
        )?;
    }
    Ok(())
}

pub(super) fn render_reference(
    data: &[u8],
    kind: u32,
    payload: u64,
    version: u32,
) -> Result<Vec<u8>, wasmtime::Error> {
    Encoder::with_version(data, version).encode(kind, payload)
}

pub(super) fn render_with_functions(
    data: &[u8],
    kind: u32,
    payload: u64,
    catalog: &varta_common::function::FunctionCatalog,
    work: &mut u64,
) -> Result<Option<Vec<u8>>, wasmtime::Error> {
    let mut encoder = Encoder::with_version(data, catalog.value_version);
    encoder.function_catalog = Some(catalog);
    encoder.function_work = Some(work);
    encoder.encode_optional(kind, payload)
}

pub(super) fn render_diagnostic(
    data: &[u8],
    kind: u32,
    payload: u64,
    version: u32,
    budget: &mut varta_bigint::Budget,
) -> Result<Vec<u8>, wasmtime::Error> {
    let mut encoder = Encoder::with_version(data, version);
    encoder.diagnostic_budget = Some(budget);
    encoder.encode(kind, payload)
}

struct Output(Vec<u8>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > abi::MAX_RESULT_BYTES - self.0.len() {
            return Err(std::io::Error::other("JSON byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Encoder<'a, 'b> {
    bigint_seen: bool,
    function_catalog: Option<&'a varta_common::function::FunctionCatalog>,
    function_work: Option<&'b mut u64>,
    functions: BTreeSet<u64>,
    diagnostic_budget: Option<&'b mut varta_bigint::Budget>,
    version: u32,
    data: &'a [u8],
    output: Output,
    path: Vec<usize>,
    nodes: usize,
}
impl<'a, 'b> Encoder<'a, 'b> {
    fn new(data: &'a [u8]) -> Self {
        Self::with_version(data, VALUE_VERSION)
    }
    fn with_version(data: &'a [u8], version: u32) -> Self {
        Self {
            bigint_seen: false,
            function_catalog: None,
            function_work: None,
            functions: BTreeSet::new(),
            diagnostic_budget: None,
            version,
            data,
            output: Output(Vec::new()),
            path: Vec::new(),
            nodes: 0,
        }
    }
    fn encode(mut self, kind: u32, value: u64) -> Result<Vec<u8>, wasmtime::Error> {
        let (kind, value) = self.resolve(kind, value)?;
        if kind == ValueKind::Undefined as u32 {
            return Err(error("undefined root requires optional encoding"));
        }
        self.value(kind, value, 0)?;
        if self.bigint_seen {
            return Err(wasmtime::Error::new(BigIntSerialization));
        }
        Ok(self.output.0)
    }
    fn encode_optional(
        mut self,
        kind: u32,
        value: u64,
    ) -> Result<Option<Vec<u8>>, wasmtime::Error> {
        let (kind, value) = self.resolve(kind, value)?;
        if kind == ValueKind::Undefined as u32 {
            if !ValueKind::Undefined.supported_by(self.version) || value != 0 {
                return Err(error("noncanonical or incompatible undefined"));
            }
            self.visit(0)?;
            return Ok(None);
        }
        self.encode(kind, value).map(Some)
    }
    fn resolve(&mut self, kind: u32, value: u64) -> Result<(u32, u64), wasmtime::Error> {
        if kind == ValueKind::Function as u32 {
            self.function(value)?;
            return Ok((ValueKind::Undefined as u32, 0));
        }
        if kind == ValueKind::ReferenceUnion as u32 {
            if !matches!(
                self.version,
                REFERENCE_VALUE_VERSION
                    | VECTOR_VALUE_VERSION
                    | ERROR_VALUE_VERSION
                    | ERROR_FAMILY_VALUE_VERSION
                    | SPARSE_VALUE_VERSION
                    | BIGINT_VALUE_VERSION
                    | FUNCTION_VALUE_VERSION
                    | DYNAMIC_VALUE_VERSION
            ) {
                return Err(error("incompatible reference union"));
            }
            let cell = if self.version == FUNCTION_VALUE_VERSION {
                ReferenceBox::read_envelope(self.data, value, self.version)
            } else {
                ReferenceBox::read_with_version(self.data, value, self.version)
            }
            .map_err(wasmtime::Error::new)?;
            return self.resolve(cell.kind as u32, cell.payload);
        }
        if kind != ValueKind::PrimitiveUnion as u32 {
            return Ok((kind, value));
        }
        if !matches!(
            self.version,
            UNION_VALUE_VERSION
                | MISSING_VALUE_VERSION
                | REFERENCE_VALUE_VERSION
                | VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
        ) {
            return Err(error("incompatible primitive union"));
        }
        let cell = PrimitiveBox::read(self.data, value).map_err(wasmtime::Error::new)?;
        Ok((cell.kind as u32, cell.payload))
    }
    fn function(&mut self, handle: u64) -> Result<(), wasmtime::Error> {
        if self.version != FUNCTION_VALUE_VERSION {
            return Err(error("incompatible function value"));
        }
        if !self.functions.insert(handle) {
            return Ok(());
        }
        let catalog = self
            .function_catalog
            .ok_or_else(|| error("function value requires owner catalog"))?;
        if catalog.value_version != FUNCTION_VALUE_VERSION {
            return Err(error("function catalog version mismatch"));
        }
        let remaining = self
            .function_work
            .as_deref_mut()
            .ok_or_else(|| error("function work budget missing"))?;
        let proof = catalog
            .validate_closure(self.data, handle, *remaining)
            .map_err(wasmtime::Error::new)?;
        *remaining = remaining
            .checked_sub(proof.work)
            .ok_or_else(|| error("function work budget exceeded"))?;
        Ok(())
    }
    fn visit(&mut self, depth: usize) -> Result<(), wasmtime::Error> {
        self.nodes += 1;
        if depth > MAX_JSON_DEPTH || self.nodes > MAX_JSON_NODES {
            return Err(error("depth or node limit"));
        }
        Ok(())
    }
    fn bytes(&self, ptr: usize, len: usize) -> Result<&'a [u8], wasmtime::Error> {
        let end = ptr
            .checked_add(len)
            .ok_or_else(|| error("range overflow"))?;
        self.data
            .get(ptr..end)
            .ok_or_else(|| error("range outside memory"))
    }
    fn words<const N: usize>(&self, ptr: usize) -> Result<[u32; N], wasmtime::Error> {
        let bytes = self.bytes(ptr, N * size_of::<u32>())?;
        let mut words = [0; N];
        for (word, chunk) in words
            .iter_mut()
            .zip(bytes.as_chunks::<{ size_of::<u32>() }>().0)
        {
            *word = u32::from_le_bytes(*chunk);
        }
        Ok(words)
    }
    fn text(&self, packed: u64) -> Result<&'a str, wasmtime::Error> {
        let length = (packed >> i32::BITS) as usize;
        if length > abi::MAX_STRING_BYTES {
            return Err(error("string byte limit"));
        }
        std::str::from_utf8(self.bytes(packed as u32 as usize, length)?)
            .map_err(|_| error("invalid UTF-8"))
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), wasmtime::Error> {
        self.output
            .write_all(bytes)
            .map_err(|_| error("output byte limit"))
    }
    fn quoted(&mut self, text: &str) -> Result<(), wasmtime::Error> {
        serde_json::to_writer(&mut self.output, text).map_err(|_| error("output byte limit"))
    }
    fn value(&mut self, kind: u32, packed: u64, depth: usize) -> Result<(), wasmtime::Error> {
        self.visit(depth)?;
        let (kind, packed) = self.resolve(kind, packed)?;
        let kind = ValueKind::from_tag(kind)
            .filter(|k| k.supported_by(self.version))
            .ok_or_else(|| error("unknown or incompatible value kind"))?;
        match kind {
            ValueKind::BigInt => {
                varta_common::bigint::BigIntView::read(self.data, packed)
                    .map_err(wasmtime::Error::new)?;
                if let Some(budget) = self.diagnostic_budget.as_deref_mut() {
                    let text = crate::bigint::integer(self.data, packed as i64, budget)
                        .and_then(|n| n.to_string_radix(10, budget))
                        .map_err(wasmtime::Error::new)?;
                    budget
                        .consume_work(text.len() as u64)
                        .map_err(wasmtime::Error::new)?;
                    self.write(text.as_bytes())?;
                    self.write(b"n")
                } else {
                    // Continue bounded validation of the complete graph before
                    // publishing a recoverable semantic failure. Corrupt slots,
                    // cycles and resource limits must not be hidden by BigInt.
                    self.bigint_seen = true;
                    self.write(b"null")
                }
            }
            ValueKind::Undefined if packed == 0 => self.write(b"null"),
            ValueKind::Float64 => {
                let number = f64::from_bits(packed);
                if !number.is_finite() {
                    self.write(b"null")
                } else if number == 0.0 {
                    self.write(b"0")
                } else {
                    self.write(ryu_js::Buffer::new().format_finite(number).as_bytes())
                }
            }
            ValueKind::Null if packed == 0 => self.write(b"null"),
            ValueKind::I32 => {
                let value =
                    i32::try_from(packed as i64).map_err(|_| error("noncanonical integer"))?;
                self.write(value.to_string().as_bytes())
            }
            ValueKind::Boolean if packed <= 1 => {
                self.write(if packed == 0 { b"false" } else { b"true" })
            }
            ValueKind::String if self.version >= UTF16_VALUE_VERSION => {
                let length = (packed >> i32::BITS) as usize;
                if length > abi::MAX_STRING_BYTES {
                    return Err(error("string byte limit"));
                }
                let units = varta_common::text::units(self.bytes(packed as u32 as usize, length)?)
                    .map_err(|_| error("invalid UTF-16 byte length"))?;
                varta_common::text::write_json(&mut self.output, &units)
                    .map_err(|_| error("output byte limit"))
            }
            ValueKind::String => self.quoted(self.text(packed)?),
            ValueKind::Error => {
                ErrorObject::read_with_version(self.data, packed, self.version)
                    .map_err(wasmtime::Error::new)?;
                self.visit(depth + 1)?;
                self.write(b"{}")
            }
            ValueKind::Record => {
                if matches!(
                    self.version,
                    BIGINT_VALUE_VERSION | FUNCTION_VALUE_VERSION | DYNAMIC_VALUE_VERSION
                ) {
                    validate_aggregate(self.data, ValueKind::Record, packed, self.version)
                        .map_err(wasmtime::Error::new)?;
                }
                self.record(packed, depth)
            }
            ValueKind::Array => self.array(packed, depth),
            _ => Err(error("noncanonical scalar")),
        }
    }
    fn managed(
        &self,
        packed: u64,
        kind_flag: i32,
        max_length: usize,
    ) -> Result<(usize, usize), wasmtime::Error> {
        let ptr = packed as u32 as usize;
        let length = (packed >> i32::BITS) as usize;
        if !(RECORD_META_BYTES..=max_length).contains(&length) {
            return Err(error("invalid object length"));
        }
        if ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES {
            return Err(error("object outside managed heap"));
        }
        let [block_size, flags, _, _] = self.words::<4>(ptr - abi::STRING_HEADER_BYTES)?;
        const ALLOCATED: u32 = HEAP_ALLOCATED_FLAG;
        let kinds = if matches!(
            self.version,
            ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
        ) {
            HEAP_ERROR_FLAG as u32
        } else {
            0
        } | HEAP_RECORD_FLAG as u32
            | HEAP_ARRAY_FLAG as u32
            | if matches!(
                self.version,
                VECTOR_VALUE_VERSION
                    | ERROR_VALUE_VERSION
                    | ERROR_FAMILY_VALUE_VERSION
                    | SPARSE_VALUE_VERSION
                    | BIGINT_VALUE_VERSION
                    | FUNCTION_VALUE_VERSION
                    | DYNAMIC_VALUE_VERSION
            ) {
                HEAP_VECTOR_FLAG as u32
            } else {
                0
            }
            | if matches!(
                self.version,
                MISSING_VALUE_VERSION
                    | REFERENCE_VALUE_VERSION
                    | VECTOR_VALUE_VERSION
                    | ERROR_VALUE_VERSION
                    | ERROR_FAMILY_VALUE_VERSION
                    | SPARSE_VALUE_VERSION
                    | BIGINT_VALUE_VERSION
                    | FUNCTION_VALUE_VERSION
                    | DYNAMIC_VALUE_VERSION
            ) {
                HEAP_UNION_FLAG as u32
            } else {
                0
            };
        if flags & (ALLOCATED | kinds) != (ALLOCATED | kind_flag as u32)
            || (block_size as usize) < abi::STRING_HEADER_BYTES + length
            || block_size as usize
                > self
                    .data
                    .len()
                    .saturating_sub(ptr - abi::STRING_HEADER_BYTES)
        {
            return Err(error("invalid object block"));
        }
        self.bytes(ptr, length)?;
        Ok((ptr, length))
    }
    fn array(&mut self, packed: u64, depth: usize) -> Result<(), wasmtime::Error> {
        if matches!(
            self.version,
            VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
        ) {
            return self.vector(packed, depth);
        }
        let (ptr, length) = self.managed(
            packed,
            HEAP_ARRAY_FLAG,
            ARRAY_META_BYTES + MAX_ARRAY_ELEMENTS * ARRAY_SLOT_BYTES,
        )?;
        let [kind, count] = self.words::<2>(ptr)?;
        let count = count as usize;
        if !ValueKind::from_tag(kind).is_some_and(|k| k.supported_by(self.version))
            || count > MAX_ARRAY_ELEMENTS
            || length != ARRAY_META_BYTES + count * ARRAY_SLOT_BYTES
        {
            return Err(error("invalid array length or element kind"));
        }
        if self.path.contains(&ptr) {
            return Err(error("cyclic object"));
        }
        self.path.push(ptr);
        self.write(b"[")?;
        for index in 0..count {
            if index != 0 {
                self.write(b",")?;
            }
            let [lo, hi] = self.words::<2>(ptr + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES)?;
            self.value(kind, ((hi as u64) << i32::BITS) | lo as u64, depth + 1)?;
        }
        self.write(b"]")?;
        self.path.pop();
        Ok(())
    }
    fn vector(&mut self, packed: u64, depth: usize) -> Result<(), wasmtime::Error> {
        let vector = Vector::read_with_version(self.data, packed, self.version)
            .map_err(wasmtime::Error::new)?;
        if self.path.contains(&vector.ptr) {
            return Err(error("cyclic object"));
        }
        self.path.push(vector.ptr);
        self.write(b"[")?;
        for index in 0..vector.length {
            if index != 0 {
                self.write(b",")?;
            }
            if !vector
                .is_present(self.data, index)
                .map_err(wasmtime::Error::new)?
            {
                self.value(ValueKind::Undefined as u32, 0, depth + 1)?;
                continue;
            }
            let [lo, hi] =
                self.words::<2>(vector.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES)?;
            self.value(
                vector.kind as u32,
                ((hi as u64) << i32::BITS) | lo as u64,
                depth + 1,
            )?;
        }
        self.write(b"]")?;
        self.path.pop();
        Ok(())
    }
    fn record(&mut self, packed: u64, depth: usize) -> Result<(), wasmtime::Error> {
        if matches!(
            self.version,
            MISSING_VALUE_VERSION
                | REFERENCE_VALUE_VERSION
                | VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
        ) {
            return self.record_present(packed, depth);
        }
        let (ptr, length) = self.managed(
            packed,
            HEAP_RECORD_FLAG,
            RECORD_META_BYTES + MAX_RECORD_FIELDS * RECORD_SLOT_BYTES,
        )?;
        let [descriptor, count] = self.words::<2>(ptr)?;
        let count = count as usize;
        if count > MAX_RECORD_FIELDS || length != RECORD_META_BYTES + count * RECORD_SLOT_BYTES {
            return Err(error("invalid object length"));
        }
        if self.path.contains(&ptr) {
            return Err(error("cyclic object"));
        }
        let descriptor = descriptor as usize;
        let descriptor_size = RECORD_DESCRIPTOR_HEADER + count * RECORD_DESCRIPTOR_ENTRY;
        if descriptor
            .checked_add(descriptor_size)
            .is_none_or(|end| end > abi::RESULT_BASE)
        {
            return Err(error("descriptor outside static memory"));
        }
        let [version, fields] = self.words::<2>(descriptor)?;
        if !compatible_version(version, self.version) || fields as usize != count {
            return Err(error("invalid descriptor version or shape"));
        }
        self.path.push(ptr);
        self.write(b"{")?;
        let mut names = BTreeSet::new();
        let mut slots = BTreeSet::new();
        let mut written = false;
        for index in 0..count {
            let [key_ptr, key_len, slot, kind] = self.words::<4>(
                descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY,
            )?;
            let (key_ptr, key_len, slot) = (key_ptr as usize, key_len as usize, slot as usize);
            if key_len > MAX_RECORD_KEY_BYTES
                || key_ptr
                    .checked_add(key_len)
                    .is_none_or(|end| end > abi::RESULT_BASE)
                || slot >= count
                || !slots.insert(slot)
            {
                return Err(error("invalid object field metadata"));
            }
            let key = std::str::from_utf8(self.bytes(key_ptr, key_len)?)
                .map_err(|_| error("invalid field UTF-8"))?;
            if !names.insert(key) {
                return Err(error("duplicate object field"));
            }
            let [lo, hi] = self.words::<2>(ptr + RECORD_META_BYTES + slot * RECORD_SLOT_BYTES)?;
            let packed = ((hi as u64) << i32::BITS) | lo as u64;
            let (kind, packed) = self.resolve(kind, packed)?;
            let tag = ValueKind::from_tag(kind)
                .filter(|k| k.supported_by(self.version))
                .ok_or_else(|| error("incompatible field kind"))?;
            if tag == ValueKind::Undefined {
                self.visit(depth + 1)?;
                if packed != 0 {
                    return Err(error("noncanonical undefined"));
                }
                continue;
            }
            if written {
                self.write(b",")?;
            }
            written = true;
            self.quoted(key)?;
            self.write(b":")?;
            self.value(kind, packed, depth + 1)?;
        }
        self.write(b"}")?;
        self.path.pop();
        Ok(())
    }
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
