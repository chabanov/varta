//! Canonical, immutable primitive-union cells in value ABI v6.
use super::*;
use crate::abi;

#[derive(Debug, Clone, Copy)]
pub struct PrimitiveBox {
    pub kind: ValueKind,
    pub payload: u64,
}
impl PrimitiveBox {
    pub(super) fn raw(data: &[u8], handle: u64) -> Result<Self, ValueAbiError> {
        let invalid = || ValueAbiError("noncanonical primitive union".into());
        let ptr = handle as u32 as usize;
        if (handle >> i32::BITS) as usize != UNION_BYTES
            || ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
            || !ptr.is_multiple_of(HEAP_ALIGNMENT)
        {
            return Err(invalid());
        }
        let header = ptr - abi::STRING_HEADER_BYTES;
        let read = |at: usize| {
            data.get(at..at.checked_add(4)?)
                .and_then(|s| s.try_into().ok())
                .map(u32::from_le_bytes)
        };
        let size = read(header).ok_or_else(invalid)? as usize;
        if size < abi::STRING_HEADER_BYTES + UNION_BYTES
            || !size.is_multiple_of(HEAP_ALIGNMENT)
            || header.checked_add(size).is_none_or(|end| end > data.len())
            || read(header + HEAP_FLAGS_OFFSET)
                != Some(HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32)
            || read(ptr + 4) != Some(1)
        {
            return Err(invalid());
        }
        let kind = read(ptr)
            .and_then(ValueKind::from_tag)
            .ok_or_else(invalid)?;
        let payload = data
            .get(ptr + UNION_META_BYTES..ptr + UNION_BYTES)
            .and_then(|s| s.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or_else(invalid)?;
        if read(header + HEAP_LINK_OFFSET) != Some(u32::from(kind.is_reference())) {
            return Err(invalid());
        }
        Ok(Self { kind, payload })
    }
    pub fn read(data: &[u8], handle: u64) -> Result<Self, ValueAbiError> {
        let invalid = || ValueAbiError("noncanonical primitive union".into());
        let cell = Self::raw(data, handle)?;
        let Self { kind, payload } = cell;
        let canonical = match kind {
            ValueKind::Null | ValueKind::Undefined => payload == 0,
            ValueKind::Boolean => payload <= 1,
            ValueKind::Float64 => true,
            ValueKind::String => {
                let len = (payload >> i32::BITS) as usize;
                let ptr = payload as u32 as usize;
                len <= abi::MAX_STRING_BYTES
                    && len.is_multiple_of(2)
                    && ptr.checked_add(len).is_some_and(|end| end <= data.len())
            }
            _ => false,
        };
        if !canonical {
            return Err(invalid());
        }
        Ok(cell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cell(kind: ValueKind, payload: u64) -> (Vec<u8>, u64) {
        let mut bytes = vec![0; abi::OWNED_STRING_BASE + 256];
        let header = abi::OWNED_STRING_BASE;
        let ptr = header + abi::STRING_HEADER_BYTES;
        for (index, word) in [
            32,
            HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32,
            u32::from(kind.is_reference()),
            0,
            kind as u32,
            1,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[header + index * 4..header + index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        bytes[ptr + UNION_META_BYTES..ptr + UNION_BYTES].copy_from_slice(&payload.to_le_bytes());
        (bytes, ((UNION_BYTES as u64) << i32::BITS) | ptr as u64)
    }
    #[test]
    fn primitive_union_cells_validate_every_tag_payload_range_and_trace_mask() {
        for (kind, payload) in [
            (ValueKind::Null, 0),
            (ValueKind::Undefined, 0),
            (ValueKind::Boolean, 1),
            (ValueKind::Float64, f64::NAN.to_bits()),
            (ValueKind::Float64, (-0.0f64).to_bits()),
            (ValueKind::Float64, abi::OWNED_STRING_BASE as u64),
            (ValueKind::String, 2u64 << i32::BITS),
        ] {
            let (bytes, handle) = cell(kind, payload);
            let value = PrimitiveBox::read(&bytes, handle).unwrap();
            assert_eq!(value.kind, kind);
            assert_eq!(value.payload, payload);
        }
        for (kind, payload) in [
            (ValueKind::Null, 1),
            (ValueKind::Undefined, 1),
            (ValueKind::Boolean, 2),
            (ValueKind::String, 1u64 << i32::BITS),
            (ValueKind::String, u64::MAX),
            (ValueKind::Record, 0),
            (ValueKind::Array, 0),
            (ValueKind::PrimitiveUnion, 0),
            (ValueKind::I32, 0),
        ] {
            let (bytes, handle) = cell(kind, payload);
            assert!(PrimitiveBox::read(&bytes, handle).is_err(), "{kind:?}");
        }
        let (bytes, handle) = cell(ValueKind::String, 0);
        for (at, word) in [
            (abi::OWNED_STRING_BASE, 0u32),
            (abi::OWNED_STRING_BASE + HEAP_FLAGS_OFFSET, 1),
            (abi::OWNED_STRING_BASE + HEAP_LINK_OFFSET, 0),
            (handle as u32 as usize, 999),
            (handle as u32 as usize + 4, 2),
        ] {
            let mut bad = bytes.clone();
            bad[at..at + 4].copy_from_slice(&word.to_le_bytes());
            assert!(PrimitiveBox::read(&bad, handle).is_err());
        }
        for bad in [0, handle + 1, handle ^ (1u64 << i32::BITS), u64::MAX] {
            assert!(PrimitiveBox::read(&bytes, bad).is_err());
        }
    }
}
