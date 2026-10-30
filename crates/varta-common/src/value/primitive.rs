//! Canonical, immutable primitive-union cells in value ABI v6.
//!
//! The one reader of the cell layout: host readers pass the instance's
//! memory as a byte slice, the linked intrinsic library its guest-memory
//! accessors ([`Memory`]).
use super::*;
use crate::{abi, memory::Memory};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimitiveBox {
    pub kind: ValueKind,
    pub payload: u64,
}
impl PrimitiveBox {
    /// The cell structure (heap header, tag, trace mask) without payload
    /// canonicality.
    pub fn raw<M: Memory + ?Sized>(memory: &M, handle: u64) -> Result<Self, ValueAbiError> {
        let invalid = || ValueAbiError("noncanonical primitive union".into());
        let ptr = handle as u32 as usize;
        if (handle >> i32::BITS) as usize != UNION_BYTES
            || ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
            || !ptr.is_multiple_of(HEAP_ALIGNMENT)
        {
            return Err(invalid());
        }
        let header = ptr - abi::STRING_HEADER_BYTES;
        let size = memory.u32_at(header).ok_or_else(invalid)? as usize;
        if size < abi::STRING_HEADER_BYTES + UNION_BYTES
            || !size.is_multiple_of(HEAP_ALIGNMENT)
            || header
                .checked_add(size)
                .is_none_or(|end| end > memory.size())
            || memory.u32_at(header + HEAP_FLAGS_OFFSET)
                != Some(HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32)
            || memory.u32_at(ptr + 4) != Some(1)
        {
            return Err(invalid());
        }
        let kind = memory
            .u32_at(ptr)
            .and_then(ValueKind::from_tag)
            .ok_or_else(invalid)?;
        let payload = memory.u64_at(ptr + UNION_META_BYTES).ok_or_else(invalid)?;
        if memory.u32_at(header + HEAP_LINK_OFFSET) != Some(u32::from(kind.is_reference())) {
            return Err(invalid());
        }
        Ok(Self { kind, payload })
    }
    /// A canonical cell under the frozen string limit of every profile
    /// before v38.
    pub fn read<M: Memory + ?Sized>(memory: &M, handle: u64) -> Result<Self, ValueAbiError> {
        Self::read_limited(memory, handle, abi::MAX_STRING_BYTES)
    }
    /// Read within a module's bounds, under its profile's data limits (v38 admits longer strings).
    #[cfg(feature = "std")]
    pub fn read_in(data: &[u8], handle: u64, module: ModuleBounds) -> Result<Self, ValueAbiError> {
        Self::read_limited(data, handle, module.limits.data.string_bytes)
    }
    /// A canonical cell: null/undefined carry 0, booleans 0 or 1, strings an
    /// even, in-memory UTF-16 range of at most `string_bytes` bytes.
    pub fn read_limited<M: Memory + ?Sized>(
        memory: &M,
        handle: u64,
        string_bytes: usize,
    ) -> Result<Self, ValueAbiError> {
        let invalid = || ValueAbiError("noncanonical primitive union".into());
        let cell = Self::raw(memory, handle)?;
        let Self { kind, payload } = cell;
        let canonical = match kind {
            ValueKind::Null | ValueKind::Undefined => payload == 0,
            ValueKind::Boolean => payload <= 1,
            ValueKind::Float64 => true,
            ValueKind::String => {
                let (ptr, len) = string_range(payload);
                len <= string_bytes
                    && len.is_multiple_of(2)
                    && ptr.checked_add(len).is_some_and(|end| end <= memory.size())
            }
            _ => false,
        };
        if !canonical {
            return Err(invalid());
        }
        Ok(cell)
    }
}

/// The `(ptr, byte length)` of a packed string handle.
#[must_use]
pub fn string_range(handle: u64) -> (usize, usize) {
    (handle as u32 as usize, (handle >> i32::BITS) as usize)
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

    /// Guest memory reached only through copies, as linked code reads it.
    struct Copied(Vec<u8>);
    impl Memory for Copied {
        fn size(&self) -> usize {
            self.0.len()
        }
        fn copy_to(&self, at: usize, out: &mut [u8]) -> bool {
            self.0.as_slice().copy_to(at, out)
        }
    }

    #[test]
    fn copying_and_slice_memories_read_every_corruption_alike() {
        let (bytes, handle) = cell(ValueKind::String, 2u64 << i32::BITS);
        let header = abi::OWNED_STRING_BASE;
        for at in header..header + abi::STRING_HEADER_BYTES + UNION_BYTES {
            for flip in [1u8, 0x80] {
                let mut bad = bytes.clone();
                bad[at] ^= flip;
                let slice = PrimitiveBox::read(&bad, handle).ok();
                let copied = PrimitiveBox::read(&Copied(bad), handle).ok();
                assert_eq!(slice, copied, "byte {at}");
            }
        }
        // The caller's limit bounds string cells: v38 linked code admits
        // strings beyond the frozen 1 MiB.
        let long = ((abi::MAX_STRING_BYTES as u64 + 2) << i32::BITS) | 8;
        let (bytes, handle) = cell(ValueKind::String, long);
        assert!(bytes.len() >= abi::MAX_STRING_BYTES + 10);
        assert!(PrimitiveBox::read(&bytes, handle).is_err());
        let limit = crate::limits::V38_MAX_STRING_BYTES;
        assert_eq!(
            PrimitiveBox::read_limited(&Copied(bytes), handle, limit)
                .unwrap()
                .payload,
            long
        );
    }
}
