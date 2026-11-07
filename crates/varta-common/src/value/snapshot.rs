//! Profile-gated own-key order for checked snapshots using canonical value18
//! records, a hidden index dictionary and a string vector. No new heap layout.
use super::*;
use crate::memory::Memory;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

pub struct SnapshotMembers {
    pub extras: Dictionary,
    /// A declared slot, or `fields.len() + dictionary position` for an extra.
    pub order: Vec<usize>,
}
fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical snapshot metadata".into())
}

fn string<M: Memory + ?Sized>(
    data: &M,
    handle: u64,
    module: ModuleBounds,
) -> Result<Vec<u16>, ValueAbiError> {
    let (start, len) = string_range(handle);
    if !len.is_multiple_of(2) || len > module.limits.data.string_bytes {
        return Err(invalid());
    }
    let bytes = data.borrow(start, len).ok_or_else(invalid)?;
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
        .collect())
}

/// The complete validated public-member order of a snapshot. Other records
/// and every earlier profile return `None` and retain their old interpretation.
pub fn snapshot_members<M: Memory + ?Sized>(
    data: &M,
    fields: &[RecordEntry],
    version: u32,
    module: ModuleBounds,
) -> Result<Option<SnapshotMembers>, ValueAbiError> {
    if !module.snapshots {
        return Ok(None);
    }
    let named = |field: &RecordEntry, name: &str| {
        field.len == name.len()
            && data.borrow(field.key, field.len).as_deref() == Some(name.as_bytes())
    };
    let Some(order_field) = fields.iter().find(|field| named(field, SNAPSHOT_ORDER_KEY)) else {
        return Ok(None);
    };
    if !hashed_dictionaries(version)
        || fields
            .iter()
            .enumerate()
            .any(|(slot, field)| field.slot != slot)
    {
        return Err(invalid());
    }
    let extra_field = fields
        .iter()
        .find(|field| named(field, INDEX_SIGNATURE_KEY))
        .ok_or_else(invalid)?;
    if !order_field.present
        || order_field.kind != ValueKind::Array
        || !extra_field.present
        || extra_field.kind != ValueKind::Dictionary
    {
        return Err(invalid());
    }
    let keys = Vector::read_in(data, order_field.payload, version, module)?;
    if keys.kind != ValueKind::String {
        return Err(invalid());
    }
    let extras = Dictionary::read_in(data, extra_field.payload, version, module)?;
    if extras.values.kind != ValueKind::ReferenceUnion {
        return Err(invalid());
    }
    let mut known = BTreeMap::new();
    let mut visible = BTreeSet::new();
    for field in fields {
        let bytes = data.borrow(field.key, field.len).ok_or_else(invalid)?;
        if bytes.first() == Some(&(PRIVATE_KEY_PREFIX as u8)) {
            continue;
        }
        let text = core::str::from_utf8(&bytes).map_err(|_| invalid())?;
        let name: Vec<u16> = text.encode_utf16().collect();
        if known.insert(name, (field.slot, field.present)).is_some() {
            return Err(invalid());
        }
        if field.present {
            visible.insert(field.slot);
        }
    }
    for position in 0..extras.keys.length {
        let Some((key, _)) = extras.entry(data, position)? else {
            continue;
        };
        if known
            .insert(string(data, key, module)?, (fields.len() + position, true))
            .is_some()
        {
            return Err(invalid());
        }
        visible.insert(fields.len() + position);
    }
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    let mut numeric = BTreeMap::new();
    for position in 0..keys.length {
        if !keys.is_present(data, position)? {
            return Err(invalid());
        }
        let at = keys.storage + ARRAY_META_BYTES + position * ARRAY_SLOT_BYTES;
        let handle = data.u64_at(at).ok_or_else(invalid)?;
        let name = string(data, handle, module)?;
        let &(member, present) = known.get(&name).ok_or_else(invalid)?;
        if !seen.insert(member) {
            return Err(invalid());
        }
        numeric.insert(member, array_index(&name));
        if present {
            order.push(member);
        }
    }
    if !visible.is_subset(&seen) {
        return Err(invalid());
    }
    // As for a normal record's order trailer, numeric keys enumerate first
    // even if a typed update created one after a string key.
    order.sort_by_key(|member| match numeric[member] {
        Some(index) => (false, index),
        None => (true, 0),
    });
    Ok(Some(SnapshotMembers { extras, order }))
}
