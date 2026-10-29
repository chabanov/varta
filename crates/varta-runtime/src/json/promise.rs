//! Value16 promise values (v35 `promise_values`). `JSON.stringify` writes a
//! promise as `{}` (it has no own enumerable properties). A logged promise is
//! rendered like Node's inspection of its state: `Promise { <pending> }`,
//! `Promise { value }` or `Promise { <rejected> reason }`, with the settled
//! value or reason in the diagnostic notation of logged aggregates.
use super::*;
use varta_common::scheduler::{
    PROMISE_REASON_FIELD, PROMISE_STATE_FIELD, PROMISE_VALUE_FIELD, PromiseState,
};

/// A field of the promise record: its value kind and canonical payload.
#[derive(Clone, Copy)]
struct Field {
    kind: u32,
    payload: u64,
}

impl Encoder<'_, '_> {
    pub(super) fn promise(&mut self, packed: u64, depth: usize) -> Result<(), wasmtime::Error> {
        if self.version != PROMISE_VALUE_VERSION {
            return Err(error("incompatible promise value"));
        }
        validate_aggregate(self.data, ValueKind::Record, packed, self.version)
            .map_err(wasmtime::Error::new)?;
        self.visit(depth + 1)?;
        if self.diagnostic_budget.is_none() {
            return self.write(b"{}");
        }
        let ptr = packed as u32 as usize;
        if self.path.contains(&ptr) {
            return Err(error("cyclic object"));
        }
        let state = self.promise_field(packed, PROMISE_STATE_FIELD)?;
        if state.kind != ValueKind::Float64 as u32 {
            return Err(error("promise state is not a number"));
        }
        let state = f64::from_bits(state.payload);
        self.path.push(ptr);
        let result = if state == f64::from(PromiseState::Pending as i32) {
            self.write(b"Promise { <pending> }")
        } else if state == f64::from(PromiseState::Fulfilled as i32) {
            let value = self.promise_field(packed, PROMISE_VALUE_FIELD)?;
            self.write(b"Promise { ")
                .and_then(|()| self.settled(value, depth))
                .and_then(|()| self.write(b" }"))
        } else if state == f64::from(PromiseState::Rejected as i32) {
            let reason = self.promise_field(packed, PROMISE_REASON_FIELD)?;
            self.write(b"Promise { <rejected> ")
                .and_then(|()| self.settled(reason, depth))
                .and_then(|()| self.write(b" }"))
        } else {
            Err(error("invalid promise state"))
        };
        self.path.pop();
        result
    }
    /// A settled value or reason; `undefined` prints as in Node.
    fn settled(&mut self, field: Field, depth: usize) -> Result<(), wasmtime::Error> {
        let (kind, payload) = self.resolve(field.kind, field.payload)?;
        if kind == ValueKind::Undefined as u32 {
            if payload != 0 {
                return Err(error("noncanonical undefined"));
            }
            self.visit(depth + 2)?;
            return self.write(b"undefined");
        }
        self.value(kind, payload, depth + 2)
    }
    /// The named field of a promise record, located through its static
    /// descriptor (version, count, key bytes and slot all checked).
    fn promise_field(&self, packed: u64, name: &str) -> Result<Field, wasmtime::Error> {
        let ptr = packed as u32 as usize;
        let [descriptor, count] = self.words::<2>(ptr)?;
        let (descriptor, count) = (descriptor as usize, count as usize);
        if count > MAX_RECORD_FIELDS
            || descriptor
                .checked_add(descriptor_bytes(count))
                .is_none_or(|end| end > abi::RESULT_BASE)
            || self.words::<2>(descriptor)? != [self.version, count as u32]
        {
            return Err(error("invalid promise descriptor"));
        }
        for index in 0..count {
            let [key, len, raw_slot, kind] = self.words::<4>(
                descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY,
            )?;
            let slot = (raw_slot & !OPTIONAL_FIELD_FLAG) as usize;
            if slot >= count || len as usize > MAX_RECORD_KEY_BYTES {
                return Err(error("invalid promise field metadata"));
            }
            if self.bytes(key as usize, len as usize)? != name.as_bytes() {
                continue;
            }
            let [lo, hi] = self.words::<2>(ptr + RECORD_META_BYTES + slot * RECORD_SLOT_BYTES)?;
            return Ok(Field {
                kind,
                payload: (u64::from(hi) << i32::BITS) | u64::from(lo),
            });
        }
        Err(error("promise record lacks a field"))
    }
}
