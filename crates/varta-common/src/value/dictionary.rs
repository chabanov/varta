//! Value15 dictionaries: a two-slot record block holding a dense String key
//! vector and a dense value vector of equal length. Validation is shallow for
//! values (consumers that traverse them validate each child) and complete for
//! keys: distinct, and in own-key order (array indices ascending, then the rest).
//!
//! Value18 (`dictionary_hashing`) adds the `holes` count and the `table`
//! slot (see [`HASHED_DICTIONARY_FIELDS`]): the zero handle while the
//! dictionary is unindexed (dense vectors, exactly as in value15), otherwise
//! an open-addressing `number[]` that is validated against the entries on
//! every read, so a corrupt table is rejected and never trusted. An indexed
//! dictionary's vectors may hold holes (deleted entries) at the same
//! positions.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct Dictionary {
    pub keys: Vector,
    pub values: Vector,
    /// The value18 hash table, when the dictionary is indexed.
    pub table: Option<Vector>,
}
fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical dictionary".into())
}
fn invalid_table() -> ValueAbiError {
    ValueAbiError("noncanonical dictionary table".into())
}
fn word(data: &[u8], at: usize) -> Result<u32, ValueAbiError> {
    at.checked_add(4)
        .and_then(|end| data.get(at..end))
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(invalid)
}
fn wide(data: &[u8], at: usize) -> Result<u64, ValueAbiError> {
    Ok(u64::from(word(data, at)?) | (u64::from(word(data, at + 4)?) << i32::BITS))
}
/// A canonical non-negative integral binary64 of at most `max`.
fn count(bits: u64, max: usize) -> Option<usize> {
    let value = f64::from_bits(bits);
    let count = value as u64;
    (bits >> (u64::BITS - 1) == 0 && count as f64 == value && count <= max as u64)
        .then_some(count as usize)
}
/// What a present table slot of `bits` holds: `None` for a tombstone (+0),
/// else the referenced entry position (`p + 1` with `p < length`).
fn table_reference(bits: u64, length: usize) -> Result<Option<usize>, ValueAbiError> {
    let reference = count(bits, length).ok_or_else(invalid_table)?;
    Ok(reference.checked_sub(1))
}
impl Dictionary {
    pub fn read(data: &[u8], handle: u64, version: u32) -> Result<Self, ValueAbiError> {
        Self::read_in(data, handle, version, ModuleBounds::LEGACY)
    }
    /// [`Self::read`] within a module's bounds.
    pub fn read_in(
        data: &[u8],
        handle: u64,
        version: u32,
        module: ModuleBounds,
    ) -> Result<Self, ValueAbiError> {
        if !ValueKind::Dictionary.supported_by(version) {
            return Err(invalid());
        }
        validate_aggregate_in(data, ValueKind::Record, handle, version, module)?;
        let ptr = handle as u32 as usize;
        let descriptor = word(data, ptr)? as usize;
        let fields = dictionary_storage(version);
        if word(data, ptr + 4)? as usize != fields.len() {
            return Err(invalid());
        }
        // The aggregate check validated entries, kinds and presence; require
        // exactly the canonical required slots.
        for (slot, (name, expected)) in fields.iter().enumerate() {
            let found = (0..fields.len()).any(|index| {
                let entry = descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY;
                let (Ok(key), Ok(len), Ok(raw), Ok(kind)) = (
                    word(data, entry),
                    word(data, entry + 4),
                    word(data, entry + 8),
                    word(data, entry + 12),
                ) else {
                    return false;
                };
                let key = key as usize;
                raw as usize == slot
                    && kind == *expected as u32
                    && key
                        .checked_add(len as usize)
                        .and_then(|end| data.get(key..end))
                        == Some(name.as_bytes())
            });
            if !found {
                return Err(invalid());
            }
        }
        let field = |slot: usize| wide(data, ptr + RECORD_META_BYTES + slot * RECORD_SLOT_BYTES);
        let hashed = hashed_dictionaries(version);
        let (keys_slot, values_slot) = if hashed {
            (HASHED_DICTIONARY_KEYS_SLOT, HASHED_DICTIONARY_VALUES_SLOT)
        } else {
            (0, 1)
        };
        let keys = Vector::read_in(data, field(keys_slot)?, version, module)?;
        let values = Vector::read_in(data, field(values_slot)?, version, module)?;
        // The value18 table slot holds the zero handle while unindexed.
        let table = if hashed {
            match field(HASHED_DICTIONARY_TABLE_SLOT)? {
                0 => None,
                handle => Some(
                    Vector::read_in(data, handle, version, module).map_err(|_| invalid_table())?,
                ),
            }
        } else {
            None
        };
        let limit = dictionary_entry_limit(version, module.limits.data);
        if keys.kind != ValueKind::String || keys.length != values.length || keys.length > limit {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        let mut previous = None;
        let mut named = false;
        // Key hashes by entry position (`None` for a hole), for the table.
        let mut hashes = Vec::new();
        let mut holes = 0usize;
        for index in 0..keys.length {
            let present = keys.is_present(data, index)?;
            if present != values.is_present(data, index)? {
                return Err(invalid());
            }
            if !present {
                holes += 1;
                if table.is_some() {
                    hashes.push(None);
                }
                continue;
            }
            let key = wide(
                data,
                keys.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES,
            )?;
            let start = key as u32 as usize;
            let len = (key >> i32::BITS) as usize;
            if len > module.limits.data.string_bytes || !len.is_multiple_of(2) {
                return Err(invalid());
            }
            let bytes = start
                .checked_add(len)
                .and_then(|end| data.get(start..end))
                .ok_or_else(invalid)?;
            let units = crate::text::units(bytes).map_err(|_| invalid())?;
            match array_index(&units) {
                Some(n) if !named && previous.is_none_or(|p| p < n) => previous = Some(n),
                Some(_) => return Err(invalid()),
                None => named = true,
            }
            if table.is_some() {
                hashes.push(Some(dictionary_hash(&units)));
            }
            if !seen.insert(units) {
                return Err(invalid());
            }
        }
        // Only an indexed value18 dictionary has holes, and it counts them.
        if holes != 0 && table.is_none()
            || hashed && count(field(HASHED_DICTIONARY_HOLES_SLOT)?, limit) != Some(holes)
        {
            return Err(invalid());
        }
        if let Some(table) = table {
            validate_table(data, table, &hashes, module)?;
        }
        Ok(Self {
            keys,
            values,
            table,
        })
    }
    /// The packed key and value of entry position `index` in own-key order,
    /// or `None` for a hole (a deleted entry of an indexed value18 dictionary).
    pub fn entry(&self, data: &[u8], index: usize) -> Result<Option<(u64, u64)>, ValueAbiError> {
        if index >= self.keys.length {
            return Err(invalid());
        }
        if !self.keys.is_present(data, index)? {
            return Ok(None);
        }
        Ok(Some((
            wide(
                data,
                self.keys.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES,
            )?,
            wide(
                data,
                self.values.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES,
            )?,
        )))
    }
}
/// Check a value18 table against the entries whose key hashes are `hashes`
/// by position (`None` for holes): a `number[]` whose capacity equals its
/// power-of-two length, between [`DICTIONARY_TABLE_MIN`] and the element
/// limit and at least twice the entry count; each slot empty (a hole), a
/// tombstone or a reference to a live entry; every live entry referenced
/// exactly once from a slot reached by linear probing from its home slot
/// without crossing an empty slot; and the tombstones and references
/// together as many as the entries. Extra host memory is one flag per entry.
fn validate_table(
    data: &[u8],
    table: Vector,
    hashes: &[Option<u32>],
    module: ModuleBounds,
) -> Result<(), ValueAbiError> {
    let capacity = table.length;
    if table.kind != ValueKind::Float64
        || table.capacity != capacity
        || !capacity.is_power_of_two()
        || capacity < DICTIONARY_TABLE_MIN
        || capacity > module.limits.data.array_elements
        || hashes
            .len()
            .checked_mul(2)
            .is_none_or(|need| need > capacity)
    {
        return Err(invalid_table());
    }
    let mask = capacity - 1;
    // `None` for an empty slot; `Some(None)` for a tombstone.
    let slot = |index: usize| -> Result<Option<Option<usize>>, ValueAbiError> {
        if !table.is_present(data, index)? {
            return Ok(None);
        }
        table_reference(
            wide(
                data,
                table.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES,
            )?,
            hashes.len(),
        )
        .map(Some)
    };
    // The load bound leaves at least half the slots empty; walking the
    // table once from just after an empty slot sees every probe run whole.
    let mut empty = None;
    for index in 0..capacity {
        if slot(index)?.is_none() {
            empty = Some(index);
            break;
        }
    }
    let empty = empty.ok_or_else(invalid_table)?;
    let mut referenced = vec![false; hashes.len()];
    let mut occupied = 0usize;
    let mut run = 0usize;
    for step in 1..=capacity {
        let index = (empty + step) & mask;
        let Some(content) = slot(index)? else {
            run = 0;
            continue;
        };
        run += 1;
        occupied += 1;
        let Some(position) = content else {
            continue;
        };
        // A reference names a live entry, once, reachable from its home.
        let hash = hashes[position].ok_or_else(invalid_table)?;
        if core::mem::replace(&mut referenced[position], true)
            || (index.wrapping_sub(hash as usize & mask) & mask) >= run
        {
            return Err(invalid_table());
        }
    }
    let live = hashes.iter().filter(|hash| hash.is_some()).count();
    if occupied != hashes.len() || referenced.iter().filter(|seen| **seen).count() != live {
        return Err(invalid_table());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn array_indices_follow_canonical_decimal_spelling() {
        let units = |s: &str| s.encode_utf16().collect::<Vec<_>>();
        assert_eq!(array_index(&units("0")), Some(0));
        assert_eq!(array_index(&units("42")), Some(42));
        assert_eq!(array_index(&units("4294967294")), Some(MAX_ARRAY_INDEX));
        for text in [
            "",
            "01",
            "4294967295",
            "99999999999",
            "-1",
            "1.5",
            "a",
            " 1",
            "1e3",
        ] {
            assert_eq!(array_index(&units(text)), None, "{text}");
        }
    }
    #[test]
    fn dictionary_hash_is_fnv1a_over_code_units() {
        assert_eq!(dictionary_hash(&[]), DICTIONARY_HASH_OFFSET);
        // FNV-1a of the single unit 0x61 ("a"), as the guest computes it.
        let a = (DICTIONARY_HASH_OFFSET ^ 0x61).wrapping_mul(DICTIONARY_HASH_PRIME);
        assert_eq!(dictionary_hash(&[0x61]), a);
        assert_eq!(a, 0xe40c_292c);
        // Units, not bytes: a supplementary pair hashes as two units.
        assert_ne!(
            dictionary_hash(&[0xd83d, 0xde00]),
            dictionary_hash(&[0xde00, 0xd83d])
        );
    }
    #[test]
    fn table_slots_and_counts_are_canonical_integers() {
        assert_eq!(table_reference(0, 3).unwrap(), None);
        assert_eq!(table_reference(1f64.to_bits(), 3).unwrap(), Some(0));
        assert_eq!(table_reference(3f64.to_bits(), 3).unwrap(), Some(2));
        for bits in [
            (-0f64).to_bits(),
            4f64.to_bits(),
            1.5f64.to_bits(),
            f64::NAN.to_bits(),
            f64::INFINITY.to_bits(),
            (-1f64).to_bits(),
        ] {
            assert!(table_reference(bits, 3).is_err(), "{bits:x}");
            assert_eq!(count(bits, 3), None, "{bits:x}");
        }
        assert_eq!(count(2f64.to_bits(), 3), Some(2));
    }
}
