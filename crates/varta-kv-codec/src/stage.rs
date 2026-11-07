//! Reading one guest value of the v38 value layout (value18) into the V8
//! value it stores as (`kv_stage`).
//!
//! The walk validates every handle like `JSON.stringify` does and is bounded
//! by the module's data limits and the turn deadline. It keeps what V8 keeps
//! and JSON drops: present fields and dictionary entries holding `undefined`,
//! `-0`, NaN and the infinities. A record reached twice is one object written
//! once (V8's back-reference), and counts toward the shared depth limit at
//! its deepest reach, as a reader that expands it sees it (the V8 encoder
//! then counts the value's [`crate::Extent`]); a cycle, a function, a
//! promise, an Error, a BigInt, an array hole, a `Uint8Array` held by
//! `unknown` or nested in another value and a record with private state (an
//! engine class or `#private` fields) fail with a `TypeError`, before
//! anything is stored.
//!
//! Guest memory is read through [`Memory`]: the native runtime passes the
//! instance's memory as a byte slice, the browser host its bounded copy
//! accessors.
use super::v8::{Key, Object, Value};
use crate::Deadline;
use alloc::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    format,
    rc::Rc,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use varta_common::{memory::Memory, value::*};

/// Why a value cannot be staged.
#[derive(Debug)]
pub enum StageError {
    /// A value outside the storable subset: a recoverable `TypeError`.
    Unsupported(String),
    /// A malformed handle or an exhausted limit: fatal, as for JSON. The
    /// message carries its `kv value:` prefix.
    Fatal(String),
    /// A layout reader rejected a handle: fatal.
    Abi(ValueAbiError),
}

fn fatal(message: impl core::fmt::Display) -> StageError {
    StageError::Fatal(format!("kv value: {message}"))
}
fn abi(error: ValueAbiError) -> StageError {
    StageError::Abi(error)
}
fn unsupported(kind: &str) -> StageError {
    StageError::Unsupported(format!("{kind} cannot be stored in a KV value"))
}

/// The layout every v38 module uses.
const VERSION: u32 = HASHED_DICTIONARY_VALUE_VERSION;
/// What a tagged `Uint8Array` (v38 `byte_values`) is to the stager.
const BYTES_INSIDE: &str = "a Uint8Array held by unknown or nested in another value";

pub struct Stager<'a, M: Memory + ?Sized, D: Deadline + ?Sized> {
    data: &'a M,
    module: ModuleBounds,
    deadline: &'a D,
    nodes: usize,
    /// Aggregates on the current path (a cycle check).
    path: BTreeSet<usize>,
    /// Aggregates already read, by handle address: written once by V8, with
    /// their height (the levels below them), checked again at every reach.
    seen: BTreeMap<usize, (Rc<Object>, usize)>,
    /// The deepest level reached inside the aggregate being read.
    deepest: usize,
}

impl<'a, M: Memory + ?Sized, D: Deadline + ?Sized> Stager<'a, M, D> {
    pub fn new(data: &'a M, module: ModuleBounds, deadline: &'a D) -> Self {
        Self {
            data,
            module,
            deadline,
            nodes: 0,
            path: BTreeSet::new(),
            seen: BTreeMap::new(),
            deepest: 0,
        }
    }

    fn visit(&mut self, depth: usize) -> Result<(), StageError> {
        self.nodes += 1;
        if self.nodes.is_multiple_of(crate::DEADLINE_STRIDE) && self.deadline.passed() {
            return Err(fatal("execution deadline"));
        }
        self.reach(depth)?;
        if self.nodes > self.module.limits.data.json_nodes {
            return Err(fatal("depth or node limit"));
        }
        Ok(())
    }

    /// Whether a value may sit at level `depth`: the shared KV depth first,
    /// recoverable on every host, before the module's (deeper) fatal JSON
    /// depth.
    fn reach(&mut self, depth: usize) -> Result<(), StageError> {
        if depth > crate::MAX_VALUE_DEPTH {
            return Err(StageError::Unsupported(crate::too_deep()));
        }
        if depth > self.module.limits.data.json_depth {
            return Err(fatal("depth or node limit"));
        }
        self.deepest = self.deepest.max(depth);
        Ok(())
    }

    fn bytes(&self, ptr: usize, len: usize) -> Result<Cow<'a, [u8]>, StageError> {
        self.data
            .borrow(ptr, len)
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
                // `PrimitiveBox::read_in` under the module's string limit.
                let cell = PrimitiveBox::read_limited(
                    self.data,
                    value,
                    self.module.limits.data.string_bytes,
                )
                .map_err(abi)?;
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
        varta_common::text::units(&self.bytes(packed as u32 as usize, length)?)
            .map_err(|_| fatal("invalid UTF-16 byte length"))
    }

    /// Read the value of kind word `kind` and packed slot `packed`.
    pub fn value(&mut self, kind: u32, packed: u64, depth: usize) -> Result<Value, StageError> {
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
            // A symbol (v38 `symbol_values`) cannot be cloned into storage.
            ValueKind::Symbol => {
                symbol_record(self.data, packed, VERSION, self.module).map_err(abi)?;
                return Err(unsupported("a Symbol"));
            }
            // Only a top-level `Uint8Array` stores raw bytes (by its static
            // type, `StageMode::Bytes`); a tagged one is checked and refused.
            ValueKind::Bytes => {
                byte_vector(self.data, packed, VERSION, self.module).map_err(abi)?;
                return Err(unsupported(BYTES_INSIDE));
            }
            _ => return Err(fatal("noncanonical scalar")),
        })
    }

    /// Read an aggregate once: a second reach shares the first object, a
    /// reach from inside itself is a cycle. Every reach keeps the depth
    /// limit for the whole aggregate below it, so the depth limit never
    /// refuses on a `get` (which expands shared parts) what a `set` stored.
    fn aggregate(
        &mut self,
        packed: u64,
        depth: usize,
        read: fn(&mut Self, u64, usize) -> Result<Object, StageError>,
    ) -> Result<Rc<Object>, StageError> {
        let ptr = packed as u32 as usize;
        if let Some((object, height)) = self.seen.get(&ptr) {
            let (object, height) = (Rc::clone(object), *height);
            self.reach(depth + height)?;
            return Ok(object);
        }
        if !self.path.insert(ptr) {
            return Err(unsupported("a cyclic value"));
        }
        let outer = core::mem::replace(&mut self.deepest, depth);
        let object = Rc::new(read(self, packed, depth)?);
        let height = self.deepest - depth;
        self.deepest = self.deepest.max(outer);
        self.path.remove(&ptr);
        self.seen.insert(ptr, (Rc::clone(&object), height));
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
        let mut fields: Vec<Option<(String, u32)>> = vec![None; count];
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
            let name = core::str::from_utf8(&self.bytes(key as usize, len as usize)?)
                .map_err(|_| fatal("invalid field UTF-8"))?
                .to_string();
            if !names.insert(name.clone()) {
                return Err(fatal("duplicate record field"));
            }
            fields[slot] = Some((name, kind));
        }
        let mut ranked: Vec<(Option<u32>, usize, u32)> = Vec::with_capacity(order.len());
        for (ordinal, slot) in order.iter().enumerate() {
            let (name, _) = fields[*slot as usize]
                .as_ref()
                .ok_or_else(|| fatal("missing field slot"))?;
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
            let (name, kind) = fields[slot as usize]
                .as_ref()
                .ok_or_else(|| fatal("missing field slot"))?;
            let key = Key::of(name.encode_utf16().collect());
            let kind = *kind;
            let value = self.slot(ptr + RECORD_META_BYTES + slot as usize * RECORD_SLOT_BYTES)?;
            properties.push((key, self.value(kind, value, depth + 1)?));
        }
        Ok(Object::Plain(properties))
    }
}
