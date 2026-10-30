//! Canonical, immutable primitive cells (value ABI v6 and later): the same
//! checks as `varta_common::value::PrimitiveBox::read`, over [`Memory`].
use crate::{
    IntrinsicError, Memory,
    layout::{self, Kind},
};

/// One checked primitive cell: its tag and 64-bit payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub kind: Kind,
    pub payload: u64,
}
impl Cell {
    /// The cell structure without payload canonicality.
    pub fn raw<M: Memory + ?Sized>(memory: &M, handle: u64) -> Result<Self, IntrinsicError> {
        let invalid = || IntrinsicError::PrimitiveCell;
        let ptr = handle as u32 as usize;
        if (handle >> u32::BITS) as usize != layout::UNION_BYTES
            || ptr < layout::OWNED_STRING_BASE + layout::STRING_HEADER_BYTES
            || !ptr.is_multiple_of(layout::HEAP_ALIGNMENT)
        {
            return Err(invalid());
        }
        let header = ptr - layout::STRING_HEADER_BYTES;
        let size = memory.u32_at(header).ok_or_else(invalid)? as usize;
        if size < layout::STRING_HEADER_BYTES + layout::UNION_BYTES
            || !size.is_multiple_of(layout::HEAP_ALIGNMENT)
            || header
                .checked_add(size)
                .is_none_or(|end| end > memory.size())
            || memory.u32_at(header + layout::HEAP_FLAGS_OFFSET)
                != Some(layout::HEAP_ALLOCATED_FLAG | layout::HEAP_UNION_FLAG)
            || memory.u32_at(ptr + 4) != Some(1)
        {
            return Err(invalid());
        }
        let kind = memory
            .u32_at(ptr)
            .and_then(Kind::from_tag)
            .ok_or_else(invalid)?;
        let payload = memory
            .u64_at(ptr + layout::UNION_META_BYTES)
            .ok_or_else(invalid)?;
        if memory.u32_at(header + layout::HEAP_LINK_OFFSET) != Some(u32::from(kind.is_reference()))
        {
            return Err(invalid());
        }
        Ok(Self { kind, payload })
    }
    /// A canonical primitive cell: null/undefined carry 0, booleans 0 or 1,
    /// strings an even, bounded, in-memory UTF-16 range.
    pub fn read<M: Memory + ?Sized>(memory: &M, handle: u64) -> Result<Self, IntrinsicError> {
        let cell = Self::raw(memory, handle)?;
        let canonical = match cell.kind {
            Kind::Null | Kind::Undefined => cell.payload == 0,
            Kind::Boolean => cell.payload <= 1,
            Kind::Float64 => true,
            Kind::String => {
                let (ptr, len) = string_range(cell.payload);
                len <= layout::MAX_STRING_BYTES
                    && len.is_multiple_of(2)
                    && ptr.checked_add(len).is_some_and(|end| end <= memory.size())
            }
            _ => false,
        };
        if !canonical {
            return Err(IntrinsicError::PrimitiveCell);
        }
        Ok(cell)
    }
}

/// The `(ptr, byte length)` of a packed string handle.
#[must_use]
pub fn string_range(handle: u64) -> (usize, usize) {
    (handle as u32 as usize, (handle >> u32::BITS) as usize)
}
