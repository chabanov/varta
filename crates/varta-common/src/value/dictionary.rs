//! Value15 dictionaries: a two-slot record block holding a dense String key
//! vector and a dense value vector of equal length. Validation is shallow for
//! values (consumers that traverse them validate each child) and complete for
//! keys: distinct, and in own-key order (array indices ascending, then the rest).
use super::*;
use crate::limits::DataLimits;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct Dictionary {
    pub keys: Vector,
    pub values: Vector,
}
fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical dictionary".into())
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
/// The canonical array index named by UTF-16 `units`, if any: `0` or a decimal
/// without leading zeros below `u32::MAX` (ECMAScript CanonicalNumericIndex
/// restricted to array indices).
#[must_use]
pub fn array_index(units: &[u16]) -> Option<u64> {
    const DIGITS: usize = 10;
    if units.is_empty() || units.len() > DIGITS || units.len() > 1 && units[0] == u16::from(b'0') {
        return None;
    }
    let mut value = 0u64;
    for unit in units {
        let digit = unit.checked_sub(u16::from(b'0')).filter(|d| *d <= 9)?;
        value = value * 10 + u64::from(digit);
    }
    (value <= MAX_ARRAY_INDEX).then_some(value)
}
impl Dictionary {
    pub fn read(data: &[u8], handle: u64, version: u32) -> Result<Self, ValueAbiError> {
        Self::read_limited(data, handle, version, DataLimits::LEGACY)
    }
    /// Read under a profile's data limits (v38 lifts the entry and key-size
    /// ceilings to its array and string limits).
    pub fn read_limited(
        data: &[u8],
        handle: u64,
        version: u32,
        limits: DataLimits,
    ) -> Result<Self, ValueAbiError> {
        if !ValueKind::Dictionary.supported_by(version) {
            return Err(invalid());
        }
        validate_aggregate_limited(data, ValueKind::Record, handle, version, limits)?;
        let ptr = handle as u32 as usize;
        let descriptor = word(data, ptr)? as usize;
        if word(data, ptr + 4)? as usize != DICTIONARY_FIELDS {
            return Err(invalid());
        }
        // The aggregate check validated entries, kinds and presence; require
        // exactly the canonical required `keys`/`values` vector slots.
        for (slot, name) in [DICTIONARY_KEYS_FIELD, DICTIONARY_VALUES_FIELD]
            .into_iter()
            .enumerate()
        {
            let found = (0..DICTIONARY_FIELDS).any(|index| {
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
                    && kind == ValueKind::Array as u32
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
        let keys = Vector::read_limited(data, field(0)?, version, limits)?;
        let values = Vector::read_limited(data, field(1)?, version, limits)?;
        if keys.kind != ValueKind::String
            || keys.length != values.length
            || keys.length > limits.array_elements
        {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        let mut previous = None;
        let mut named = false;
        for index in 0..keys.length {
            if !keys.is_present(data, index)? || !values.is_present(data, index)? {
                return Err(invalid());
            }
            let key = wide(
                data,
                keys.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES,
            )?;
            let start = key as u32 as usize;
            let len = (key >> i32::BITS) as usize;
            if len > limits.string_bytes || !len.is_multiple_of(2) {
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
            if !seen.insert(units) {
                return Err(invalid());
            }
        }
        Ok(Self { keys, values })
    }
    /// The packed key and value of entry `index` (in own-key order).
    pub fn entry(&self, data: &[u8], index: usize) -> Result<(u64, u64), ValueAbiError> {
        if index >= self.keys.length {
            return Err(invalid());
        }
        Ok((
            wide(
                data,
                self.keys.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES,
            )?,
            wide(
                data,
                self.values.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES,
            )?,
        ))
    }
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
}
