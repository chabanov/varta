//! Reading one guest value of the v38 value layout (value18) into the V8
//! value it stores as (`kv_stage`).
//!
//! The walk validates every handle like `JSON.stringify` does and is bounded
//! by the module's data limits and the turn deadline. It keeps what V8 keeps
//! and JSON drops: present fields and dictionary entries holding `undefined`,
//! `-0`, NaN and the infinities. A record reached twice is one object written
//! once (V8's back-reference); a cycle, a function, a promise, an Error, a
//! BigInt, an array hole and a record with private state (an engine class or
//! `#private` fields) fail with a `TypeError`, before anything is stored.
use super::v8::{Key, Object, Value};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    rc::Rc,
    time::Instant,
};
use varta_common::value::*;

/// Values read between two deadline checks.
const DEADLINE_STRIDE: usize = 1024;

/// Why a value cannot be staged.
#[derive(Debug)]
pub(crate) enum StageError {
    /// A value outside the storable subset: a recoverable `TypeError`.
    Unsupported(String),
    /// A malformed handle or an exhausted limit: fatal, as for JSON.
    Fatal(wasmtime::Error),
}

fn fatal(message: impl std::fmt::Display) -> StageError {
    StageError::Fatal(wasmtime::Error::msg(format!("kv value: {message}")))
}
fn abi(error: ValueAbiError) -> StageError {
    StageError::Fatal(wasmtime::Error::new(error))
}
fn unsupported(kind: &str) -> StageError {
    StageError::Unsupported(format!("{kind} cannot be stored in a KV value"))
}

/// The layout every v38 module uses.
const VERSION: u32 = HASHED_DICTIONARY_VALUE_VERSION;

pub(crate) struct Stager<'a> {
    data: &'a [u8],
    module: ModuleBounds,
    deadline: Instant,
    nodes: usize,
    /// Aggregates on the current path (a cycle check).
    path: HashSet<usize>,
    /// Aggregates already read, by handle address: written once by V8.
    seen: HashMap<usize, Rc<Object>>,
}

impl<'a> Stager<'a> {
    pub(crate) fn new(data: &'a [u8], module: ModuleBounds, deadline: Instant) -> Self {
        Self {
            data,
            module,
            deadline,
            nodes: 0,
            path: HashSet::new(),
            seen: HashMap::new(),
        }
    }

    fn visit(&mut self, depth: usize) -> Result<(), StageError> {
        self.nodes += 1;
        if self.nodes.is_multiple_of(DEADLINE_STRIDE) && Instant::now() >= self.deadline {
            return Err(fatal("execution deadline"));
        }
        let limits = self.module.limits.data;
        if depth > limits.json_depth || self.nodes > limits.json_nodes {
            return Err(fatal("depth or node limit"));
        }
        Ok(())
    }

    fn bytes(&self, ptr: usize, len: usize) -> Result<&'a [u8], StageError> {
        ptr.checked_add(len)
            .and_then(|end| self.data.get(ptr..end))
            .ok_or_else(|| fatal("range outside memory"))
    }

    fn words<const N: usize>(&self, ptr: usize) -> Result<[u32; N], StageError> {
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

    fn slot(&self, at: usize) -> Result<u64, StageError> {
        let [lo, hi] = self.words::<2>(at)?;
        Ok((u64::from(hi) << i32::BITS) | u64::from(lo))
    }

    /// Unbox union cells to the value they hold.
    fn resolve(&self, kind: u32, value: u64) -> Result<(ValueKind, u64), StageError> {
        let kind = ValueKind::from_tag(kind)
            .filter(|kind| kind.supported_by(VERSION))
            .ok_or_else(|| fatal("unknown value kind"))?;
        match kind {
            ValueKind::ReferenceUnion => {
                let cell =
                    ReferenceBox::read_in(self.data, value, VERSION, self.module).map_err(abi)?;
                self.resolve(cell.kind as u32, cell.payload)
            }
            ValueKind::PrimitiveUnion => {
                let cell = PrimitiveBox::read_in(self.data, value, self.module).map_err(abi)?;
                self.resolve(cell.kind as u32, cell.payload)
            }
            kind => Ok((kind, value)),
        }
    }

    fn units(&self, packed: u64) -> Result<Vec<u16>, StageError> {
        let length = (packed >> i32::BITS) as usize;
        if length > self.module.limits.data.string_bytes {
            return Err(fatal("string byte limit"));
        }
        varta_common::text::units(self.bytes(packed as u32 as usize, length)?)
            .map_err(|_| fatal("invalid UTF-16 byte length"))
    }

    /// Read the value of kind word `kind` and packed slot `packed`.
    pub(crate) fn value(
        &mut self,
        kind: u32,
        packed: u64,
        depth: usize,
    ) -> Result<Value, StageError> {
        self.visit(depth)?;
        let (kind, packed) = self.resolve(kind, packed)?;
        Ok(match kind {
            ValueKind::Undefined if packed == 0 => Value::Undefined,
            ValueKind::Null if packed == 0 => Value::Null,
            ValueKind::Boolean if packed <= 1 => Value::Boolean(packed == 1),
            ValueKind::Float64 => Value::Number(f64::from_bits(packed)),
            ValueKind::I32 => Value::Number(f64::from(
                i32::try_from(packed as i64).map_err(|_| fatal("noncanonical integer"))?,
            )),
            ValueKind::String => Value::String(self.units(packed)?),
            ValueKind::Record => Value::Object(self.aggregate(packed, depth, Self::record)?),
            ValueKind::Array => Value::Object(self.aggregate(packed, depth, Self::array)?),
            ValueKind::Dictionary => {
                Value::Object(self.aggregate(packed, depth, Self::dictionary)?)
            }
            ValueKind::BigInt => return Err(unsupported("a BigInt")),
            ValueKind::Error => return Err(unsupported("an Error")),
            ValueKind::Function => return Err(unsupported("a function")),
            ValueKind::Promise => return Err(unsupported("a Promise")),
            _ => return Err(fatal("noncanonical scalar")),
        })
    }

    /// Read an aggregate once: a second reach shares the first object, a
    /// reach from inside itself is a cycle.
    fn aggregate(
        &mut self,
        packed: u64,
        depth: usize,
        read: fn(&mut Self, u64, usize) -> Result<Object, StageError>,
    ) -> Result<Rc<Object>, StageError> {
        let ptr = packed as u32 as usize;
        if let Some(object) = self.seen.get(&ptr) {
            return Ok(Rc::clone(object));
        }
        if !self.path.insert(ptr) {
            return Err(unsupported("a cyclic value"));
        }
        let object = Rc::new(read(self, packed, depth)?);
        self.path.remove(&ptr);
        self.seen.insert(ptr, Rc::clone(&object));
        Ok(object)
    }

    fn array(&mut self, packed: u64, depth: usize) -> Result<Object, StageError> {
        let vector = Vector::read_in(self.data, packed, VERSION, self.module).map_err(abi)?;
        let mut elements = Vec::with_capacity(vector.length);
        for index in 0..vector.length {
            if !vector.is_present(self.data, index).map_err(abi)? {
                return Err(unsupported("an array with holes"));
            }
            let slot = self.slot(vector.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES)?;
            elements.push(Some(self.value(vector.kind as u32, slot, depth + 1)?));
        }
        Ok(Object::Dense(elements))
    }

    /// A dictionary: its entries in stored own-key order (array indices
    /// first, ascending), `undefined` values kept.
    fn dictionary(&mut self, packed: u64, depth: usize) -> Result<Object, StageError> {
        let dictionary =
            Dictionary::read_in(self.data, packed, VERSION, self.module).map_err(abi)?;
        let mut properties = Vec::with_capacity(dictionary.keys.length);
        for index in 0..dictionary.keys.length {
            let Some((key, value)) = dictionary.entry(self.data, index).map_err(abi)? else {
                continue;
            };
            let key = Key::of(self.units(key)?);
            properties.push((
                key,
                self.value(dictionary.values.kind as u32, value, depth + 1)?,
            ));
        }
        Ok(Object::Plain(properties))
    }

    /// A record: its present fields in property order (integer-index keys
    /// ascending, then insertion order), as `record_present` orders JSON.
    fn record(&mut self, packed: u64, depth: usize) -> Result<Object, StageError> {
        validate_aggregate_in(self.data, ValueKind::Record, packed, VERSION, self.module)
            .map_err(abi)?;
        let ptr = packed as u32 as usize;
        let [descriptor, count] = self.words::<2>(ptr)?;
        let (descriptor, count) = (descriptor as usize, count as usize);
        if count > max_record_fields(VERSION)
            || (packed >> i32::BITS) as usize != record_bytes(VERSION, count)
            || !self.module.static_region.contains(
                descriptor,
                RECORD_DESCRIPTOR_HEADER + count * RECORD_DESCRIPTOR_ENTRY,
            )
            || self.words::<2>(descriptor)? != [VERSION, count as u32]
        {
            return Err(fatal("invalid record shape"));
        }
        let present = read_slot_set(
            self.data,
            ptr + presence_offset(count),
            presence_words(count),
        )
        .ok_or_else(|| fatal("presence outside memory"))?;
        if present & !wide_field_mask(count) != 0 {
            return Err(fatal("presence outside record shape"));
        }
        let mut order = Vec::new();
        for index in 0..present.count_ones() as usize {
            let [slot] = self.words::<1>(ptr + order_offset(count) + index * 4)?;
            if slot as usize >= count || present & (1u128 << slot) == 0 || order.contains(&slot) {
                return Err(fatal("invalid property insertion order"));
            }
            order.push(slot);
        }
        let mut fields = vec![None; count];
        let mut names = BTreeSet::new();
        for index in 0..count {
            let [key, len, raw_slot, kind] = self.words::<4>(
                descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY,
            )?;
            let slot = (raw_slot & !OPTIONAL_FIELD_FLAG) as usize;
            if slot >= count
                || fields[slot].is_some()
                || len as usize > MAX_RECORD_KEY_BYTES
                || !self
                    .module
                    .static_region
                    .contains(key as usize, len as usize)
            {
                return Err(fatal("invalid record field metadata"));
            }
            let name = std::str::from_utf8(self.bytes(key as usize, len as usize)?)
                .map_err(|_| fatal("invalid field UTF-8"))?;
            if !names.insert(name) {
                return Err(fatal("duplicate record field"));
            }
            fields[slot] = Some((name, kind));
        }
        let mut ranked: Vec<(Option<u32>, usize, u32)> = Vec::with_capacity(order.len());
        for (ordinal, slot) in order.iter().enumerate() {
            let (name, _) = fields[*slot as usize].ok_or_else(|| fatal("missing field slot"))?;
            if name.starts_with(PRIVATE_KEY_PREFIX) {
                return Err(unsupported("an object with private state"));
            }
            let units: Vec<u16> = name.encode_utf16().collect();
            let index = array_index(&units).map(|index| index as u32);
            ranked.push((index, ordinal, *slot));
        }
        // Integer-index keys first, ascending; other keys by insertion.
        ranked.sort_by_key(|(index, ordinal, _)| (index.is_none(), index.unwrap_or(0), *ordinal));
        let mut properties = Vec::with_capacity(ranked.len());
        for (_, _, slot) in ranked {
            let (name, kind) = fields[slot as usize].ok_or_else(|| fatal("missing field slot"))?;
            let value = self.slot(ptr + RECORD_META_BYTES + slot as usize * RECORD_SLOT_BYTES)?;
            let key = Key::of(name.encode_utf16().collect());
            properties.push((key, self.value(kind, value, depth + 1)?));
        }
        Ok(Object::Plain(properties))
    }
}
