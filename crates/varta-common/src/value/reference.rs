//! Value v8 cells may retain a canonical managed record or array. Validation is
//! shallow and bounded; consumers such as JSON validate traversed child values.
use super::*;
use crate::abi;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct ReferenceBox {
    pub kind: ValueKind,
    pub payload: u64,
}
impl ReferenceBox {
    pub fn read(data: &[u8], handle: u64) -> Result<Self, ValueAbiError> {
        Self::read_with_version(data, handle, REFERENCE_VALUE_VERSION)
    }
    pub fn read_with_version(
        data: &[u8],
        handle: u64,
        version: u32,
    ) -> Result<Self, ValueAbiError> {
        if !matches!(
            version,
            REFERENCE_VALUE_VERSION
                | VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
        ) {
            return Err(invalid());
        }
        let cell = PrimitiveBox::raw(data, handle)?;
        if cell.kind == ValueKind::BigInt {
            if version != BIGINT_VALUE_VERSION {
                return Err(invalid());
            }
            crate::bigint::BigIntView::read(data, cell.payload)?;
        } else if cell.kind == ValueKind::Error {
            if !matches!(
                version,
                ERROR_VALUE_VERSION
                    | ERROR_FAMILY_VALUE_VERSION
                    | SPARSE_VALUE_VERSION
                    | BIGINT_VALUE_VERSION
            ) {
                return Err(invalid());
            }
            ErrorObject::read_with_version(data, cell.payload, version)?;
        } else if matches!(cell.kind, ValueKind::Record | ValueKind::Array) {
            aggregate(data, cell, version)?;
        } else {
            PrimitiveBox::read(data, handle)?;
        }
        Ok(Self {
            kind: cell.kind,
            payload: cell.payload,
        })
    }
}
fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical reference union".into())
}
fn word(data: &[u8], at: usize) -> Result<u32, ValueAbiError> {
    at.checked_add(4)
        .and_then(|end| data.get(at..end))
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(invalid)
}
/// Validate an aggregate handle without manufacturing a temporary union cell.
/// This is shallow: traversing consumers must still validate child payloads.
pub fn validate_aggregate(
    data: &[u8],
    kind: ValueKind,
    payload: u64,
    version: u32,
) -> Result<(), ValueAbiError> {
    if !matches!(kind, ValueKind::Record | ValueKind::Array) || !kind.supported_by(version) {
        return Err(invalid());
    }
    aggregate(data, PrimitiveBox { kind, payload }, version)
}
fn aggregate(data: &[u8], cell: PrimitiveBox, version: u32) -> Result<(), ValueAbiError> {
    if cell.kind == ValueKind::Array
        && matches!(
            version,
            VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
        )
    {
        Vector::read_with_version(data, cell.payload, version)?;
        return Ok(());
    }
    let ptr = cell.payload as u32 as usize;
    let length = (cell.payload >> i32::BITS) as usize;
    if ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
        || !ptr.is_multiple_of(HEAP_ALIGNMENT)
    {
        return Err(invalid());
    }
    let header = ptr - abi::STRING_HEADER_BYTES;
    let size = word(data, header)? as usize;
    let is_array = cell.kind == ValueKind::Array;
    let flag = if is_array {
        HEAP_ARRAY_FLAG
    } else {
        HEAP_RECORD_FLAG
    };
    if length < RECORD_META_BYTES
        || size < abi::STRING_HEADER_BYTES
        || length > size - abi::STRING_HEADER_BYTES
        || !size.is_multiple_of(HEAP_ALIGNMENT)
        || header.checked_add(size).is_none_or(|end| end > data.len())
        || word(data, header + HEAP_FLAGS_OFFSET)? != HEAP_ALLOCATED_FLAG | flag as u32
    {
        return Err(invalid());
    }
    let meta = word(data, ptr)?;
    let count = word(data, ptr + 4)? as usize;
    if is_array {
        if count > MAX_ARRAY_ELEMENTS || length != ARRAY_META_BYTES + count * ARRAY_SLOT_BYTES {
            return Err(invalid());
        }
        let kind = ValueKind::from_tag(meta)
            .filter(|k| k.supported_by(version))
            .ok_or_else(invalid)?;
        if word(data, header + HEAP_LINK_OFFSET)? != u32::from(kind.is_reference()) {
            return Err(invalid());
        }
        return Ok(());
    }
    if count > MAX_RECORD_FIELDS || length != record_bytes(version, count) {
        return Err(invalid());
    }
    let descriptor = meta as usize;
    if descriptor
        .checked_add(RECORD_DESCRIPTOR_HEADER + count * RECORD_DESCRIPTOR_ENTRY)
        .is_none_or(|end| end > abi::RESULT_BASE)
        || word(data, descriptor)? != version
        || word(data, descriptor + 4)? as usize != count
    {
        return Err(invalid());
    }
    let present = word(data, ptr + presence_offset(count))?;
    if present & !field_mask(count) != 0 {
        return Err(invalid());
    }
    let mut slots = 0u32;
    let mut reference_mask = 0u32;
    let mut names = BTreeSet::new();
    for index in 0..count {
        let entry = descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY;
        let key = word(data, entry)? as usize;
        let len = word(data, entry + 4)? as usize;
        let raw = word(data, entry + 8)?;
        let slot = (raw & !OPTIONAL_FIELD_FLAG) as usize;
        let optional = raw & OPTIONAL_FIELD_FLAG != 0;
        if slot >= count
            || slots & (1u32 << slot) != 0
            || len > MAX_RECORD_KEY_BYTES
            || key
                .checked_add(len)
                .is_none_or(|end| end > abi::RESULT_BASE || end > data.len())
        {
            return Err(invalid());
        }
        let name = std::str::from_utf8(&data[key..key + len]).map_err(|_| invalid())?;
        if !names.insert(name) {
            return Err(invalid());
        }
        let kind = word(data, entry + 12)
            .ok()
            .and_then(ValueKind::from_tag)
            .filter(|k| k.supported_by(version))
            .ok_or_else(invalid)?;
        if optional
            && !matches!(
                kind,
                ValueKind::Undefined | ValueKind::PrimitiveUnion | ValueKind::ReferenceUnion
            )
        {
            return Err(invalid());
        }
        let bit = 1u32 << slot;
        slots |= bit;
        if kind.is_reference() {
            reference_mask |= bit;
        }
        if present & bit == 0 {
            if !optional {
                return Err(invalid());
            }
            let lo = word(data, ptr + RECORD_META_BYTES + slot * RECORD_SLOT_BYTES)?;
            let hi = word(data, ptr + RECORD_META_BYTES + slot * RECORD_SLOT_BYTES + 4)?;
            let payload = ((hi as u64) << i32::BITS) | lo as u64;
            if kind == ValueKind::Undefined {
                if payload != 0 {
                    return Err(invalid());
                }
            } else {
                let absent = PrimitiveBox::read(data, payload)?;
                if absent.kind != ValueKind::Undefined {
                    return Err(invalid());
                }
            }
        }
    }
    if slots != field_mask(count) || word(data, header + HEAP_LINK_OFFSET)? != reference_mask {
        return Err(invalid());
    }
    let mut seen = 0u32;
    for index in 0..count {
        let slot = word(data, ptr + presence_offset(count) + 4 + index * 4)?;
        if index < present.count_ones() as usize {
            if slot as usize >= count || seen & (1u32 << slot) != 0 || present & (1u32 << slot) == 0
            {
                return Err(invalid());
            }
            seen |= 1u32 << slot;
        } else if slot != 0 {
            return Err(invalid());
        }
    }
    if seen != present {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod bigint_tests {
    use super::*;
    #[test]
    fn bigint_reference_cells_require_v13_canonical_leaf_and_exact_trace_mask() {
        let header = abi::OWNED_STRING_BASE;
        let leaf = header + abi::STRING_HEADER_BYTES;
        let cell_header = header + 32;
        let cell = cell_header + abi::STRING_HEADER_BYTES;
        let leaf_handle = (12u64 << 32) | leaf as u64;
        let handle = (UNION_BYTES as u64) << 32 | cell as u64;
        let mut data = vec![0; cell_header + 32];
        for (offset, words) in [
            (
                header,
                vec![
                    32,
                    HEAP_ALLOCATED_FLAG | crate::bigint::HEAP_FLAG,
                    0,
                    0,
                    1,
                    1,
                    7,
                    0,
                ],
            ),
            (
                cell_header,
                vec![
                    32,
                    HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32,
                    1,
                    0,
                    ValueKind::BigInt as u32,
                    1,
                    leaf_handle as u32,
                    (leaf_handle >> 32) as u32,
                ],
            ),
        ] {
            for (index, word) in words.into_iter().enumerate() {
                data[offset + index * 4..offset + index * 4 + 4]
                    .copy_from_slice(&word.to_le_bytes());
            }
        }
        let value = ReferenceBox::read_with_version(&data, handle, BIGINT_VALUE_VERSION).unwrap();
        assert_eq!(value.kind, ValueKind::BigInt);
        assert_eq!(value.payload, leaf_handle);
        assert!(PrimitiveBox::read(&data, handle).is_err());
        for version in REFERENCE_VALUE_VERSION..=SPARSE_VALUE_VERSION {
            assert!(ReferenceBox::read_with_version(&data, handle, version).is_err());
        }
        for (offset, word) in [
            (cell_header + HEAP_LINK_OFFSET, 0u32),
            (header + HEAP_LINK_OFFSET, 1),
            (leaf + 8, 0),
            (leaf + 4, 2),
            (leaf, 0),
        ] {
            let mut bad = data.clone();
            bad[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
            assert!(
                ReferenceBox::read_with_version(&bad, handle, BIGINT_VALUE_VERSION).is_err(),
                "{offset}"
            );
        }
    }
}
