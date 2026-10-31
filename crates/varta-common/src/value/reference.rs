//! Value v8 cells may retain a canonical managed record or array. Validation is
//! shallow and bounded; consumers such as JSON validate traversed child values.
//!
//! The readers are generic over [`Memory`] (`no_std` core): host readers pass
//! the instance's memory as a byte slice, the linked value readers of a v38
//! module guest memory itself, through the same code.
use super::*;
use crate::{abi, memory::Memory};
use alloc::{borrow::Cow, vec::Vec};

/// Whether `items` are pairwise distinct; sorts them.
pub(super) fn distinct<T: Ord>(items: &mut [T]) -> bool {
    items.sort_unstable();
    items.windows(2).all(|pair| pair[0] != pair[1])
}

#[derive(Debug, Clone, Copy)]
pub struct ReferenceBox {
    pub kind: ValueKind,
    pub payload: u64,
}
impl ReferenceBox {
    /// Header-only inspection for a typed walker that validates the selected payload.
    pub fn read_envelope<M: Memory + ?Sized>(
        data: &M,
        handle: u64,
        version: u32,
    ) -> Result<Self, ValueAbiError> {
        let cell = PrimitiveBox::raw(data, handle)?;
        if !cell.kind.supported_by(version)
            || matches!(
                cell.kind,
                ValueKind::I32 | ValueKind::PrimitiveUnion | ValueKind::ReferenceUnion
            )
        {
            return Err(invalid());
        }
        Ok(Self {
            kind: cell.kind,
            payload: cell.payload,
        })
    }
    /// Function cells require an immutable owner catalog, not metadata from the heap.
    #[cfg(feature = "std")]
    pub fn read_with_catalog(
        data: &[u8],
        handle: u64,
        catalog: &crate::function::FunctionCatalog,
        work_budget: u64,
    ) -> Result<Self, ValueAbiError> {
        Self::read_with_catalog_in(data, handle, catalog, work_budget, ModuleBounds::LEGACY)
    }
    /// [`Self::read_with_catalog`] within a module's bounds.
    #[cfg(feature = "std")]
    pub fn read_with_catalog_in(
        data: &[u8],
        handle: u64,
        catalog: &crate::function::FunctionCatalog,
        work_budget: u64,
        module: ModuleBounds,
    ) -> Result<Self, ValueAbiError> {
        let cell = Self::read_envelope(data, handle, catalog.value_version)?;
        if cell.kind == ValueKind::Function {
            catalog.validate_closure(data, cell.payload, work_budget)?;
            return Ok(cell);
        }
        Self::read_in(data, handle, catalog.value_version, module)
    }
    #[cfg(feature = "std")]
    pub fn read(data: &[u8], handle: u64) -> Result<Self, ValueAbiError> {
        Self::read_with_version(data, handle, REFERENCE_VALUE_VERSION)
    }
    #[cfg(feature = "std")]
    pub fn read_with_version(
        data: &[u8],
        handle: u64,
        version: u32,
    ) -> Result<Self, ValueAbiError> {
        Self::read_in(data, handle, version, ModuleBounds::LEGACY)
    }
    /// [`Self::read_with_version`] for a module whose static metadata lies in
    /// `module`'s bounds (v38 `program_limits`: record descriptors and keys may
    /// live in the static extension, closures follow its function limits;
    /// v38 `data_limits`: strings and arrays follow its data limits).
    pub fn read_in<M: Memory + ?Sized>(
        data: &M,
        handle: u64,
        version: u32,
        module: ModuleBounds,
    ) -> Result<Self, ValueAbiError> {
        if !matches!(
            version,
            REFERENCE_VALUE_VERSION
                | VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
                | PROMISE_VALUE_VERSION
                | FUNCTION_FIELD_VALUE_VERSION
                | HASHED_DICTIONARY_VALUE_VERSION
        ) {
            return Err(invalid());
        }
        let cell = PrimitiveBox::raw(data, handle)?;
        if cell.kind == ValueKind::BigInt {
            if !matches!(
                version,
                BIGINT_VALUE_VERSION
                    | FUNCTION_VALUE_VERSION
                    | DYNAMIC_VALUE_VERSION
                    | PROMISE_VALUE_VERSION
                    | FUNCTION_FIELD_VALUE_VERSION
                    | HASHED_DICTIONARY_VALUE_VERSION
            ) {
                return Err(invalid());
            }
            crate::bigint::BigIntView::check(data, cell.payload)?;
        } else if cell.kind == ValueKind::Error {
            if !matches!(
                version,
                ERROR_VALUE_VERSION
                    | ERROR_FAMILY_VALUE_VERSION
                    | SPARSE_VALUE_VERSION
                    | BIGINT_VALUE_VERSION
                    | FUNCTION_VALUE_VERSION
                    | DYNAMIC_VALUE_VERSION
                    | PROMISE_VALUE_VERSION
                    | FUNCTION_FIELD_VALUE_VERSION
                    | HASHED_DICTIONARY_VALUE_VERSION
            ) {
                return Err(invalid());
            }
            ErrorObject::read_in(data, cell.payload, version, module)?;
        } else if cell.kind == ValueKind::Dictionary {
            Dictionary::read_in(data, cell.payload, version, module)?;
        } else if cell.kind == ValueKind::Promise {
            // A promise is its private record block; consumers that read its
            // state check the record's descriptor (see `promise_state`).
            if !cell.kind.supported_by(version) {
                return Err(invalid());
            }
            validate_aggregate_in(data, ValueKind::Record, cell.payload, version, module)?;
        } else if cell.kind == ValueKind::Function && function_field_layout(version) {
            // Value17 function fields: the payload is a v12+ closure whose
            // envelope (block, signature/reserved words, environment edge and
            // binding cells) is checked; captured payloads stay opaque, and
            // no value reader invokes or decodes the function. Value14 cells
            // still need their owner catalog (`read_with_catalog`).
            crate::function::check_closure_envelope(data, cell.payload, &module.limits)?;
        } else if matches!(cell.kind, ValueKind::Record | ValueKind::Array) {
            aggregate(data, cell, version, module)?;
        } else {
            PrimitiveBox::read_limited(data, handle, module.limits.data.string_bytes)?;
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
#[inline(always)]
fn word<M: Memory + ?Sized>(data: &M, at: usize) -> Result<u32, ValueAbiError> {
    data.u32_at(at).ok_or_else(invalid)
}
/// Validate an aggregate handle without manufacturing a temporary union cell.
/// This is shallow: traversing consumers must still validate child payloads.
#[cfg(feature = "std")]
pub fn validate_aggregate(
    data: &[u8],
    kind: ValueKind,
    payload: u64,
    version: u32,
) -> Result<(), ValueAbiError> {
    validate_aggregate_in(data, kind, payload, version, ModuleBounds::LEGACY)
}
/// [`validate_aggregate`] within a module's bounds.
pub fn validate_aggregate_in<M: Memory + ?Sized>(
    data: &M,
    kind: ValueKind,
    payload: u64,
    version: u32,
    module: ModuleBounds,
) -> Result<(), ValueAbiError> {
    if !matches!(kind, ValueKind::Record | ValueKind::Array) || !kind.supported_by(version) {
        return Err(invalid());
    }
    aggregate(data, PrimitiveBox { kind, payload }, version, module)
}
fn aggregate<M: Memory + ?Sized>(
    data: &M,
    cell: PrimitiveBox,
    version: u32,
    module: ModuleBounds,
) -> Result<(), ValueAbiError> {
    if cell.kind == ValueKind::Array
        && matches!(
            version,
            VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
                | DYNAMIC_VALUE_VERSION
                | PROMISE_VALUE_VERSION
                | FUNCTION_FIELD_VALUE_VERSION
                | HASHED_DICTIONARY_VALUE_VERSION
        )
    {
        Vector::read_in(data, cell.payload, version, module)?;
        return Ok(());
    }
    if cell.kind == ValueKind::Record {
        return read_record_in(data, cell.payload, version, &module, |_| {}, |_| {});
    }
    let (ptr, length, header) = block(data, cell.payload, HEAP_ARRAY_FLAG)?;
    let meta = word(data, ptr)?;
    let count = word(data, ptr + 4)? as usize;
    if count > module.limits.data.array_elements
        || length != ARRAY_META_BYTES + count * ARRAY_SLOT_BYTES
    {
        return Err(invalid());
    }
    let kind = ValueKind::from_tag(meta)
        .filter(|k| k.supported_by(version))
        .ok_or_else(invalid)?;
    if word(data, header + HEAP_LINK_OFFSET)? != u32::from(kind.is_reference()) {
        return Err(invalid());
    }
    Ok(())
}
/// The `(pointer, length, header)` of a canonical managed record or legacy
/// array block `payload` stamped `flag`.
fn block<M: Memory + ?Sized>(
    data: &M,
    payload: u64,
    flag: i32,
) -> Result<(usize, usize, usize), ValueAbiError> {
    let ptr = payload as u32 as usize;
    let length = (payload >> i32::BITS) as usize;
    if ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
        || !ptr.is_multiple_of(HEAP_ALIGNMENT)
    {
        return Err(invalid());
    }
    let header = ptr - abi::STRING_HEADER_BYTES;
    let size = word(data, header)? as usize;
    if length < RECORD_META_BYTES
        || size < abi::STRING_HEADER_BYTES
        || length > size - abi::STRING_HEADER_BYTES
        || !size.is_multiple_of(HEAP_ALIGNMENT)
        || header.checked_add(size).is_none_or(|end| end > data.size())
        || word(data, header + HEAP_FLAGS_OFFSET)? != HEAP_ALLOCATED_FLAG | flag as u32
    {
        return Err(invalid());
    }
    Ok((ptr, length, header))
}

/// One field of a record as [`read_record_in`] validates it: its
/// descriptor entry and its slot.
#[derive(Debug, Clone, Copy)]
pub struct RecordEntry {
    /// The UTF-8 key's range in static data.
    pub key: usize,
    pub len: usize,
    pub slot: usize,
    /// The declared kind (a union or `undefined` for an optional field).
    pub kind: ValueKind,
    pub optional: bool,
    /// Whether the field is an own property.
    pub present: bool,
    /// The slot's payload.
    pub payload: u64,
}
/// Records with at most this many fields keep their borrowed names in
/// place for the distinctness check; larger ones collect and sort them.
const INLINE_NAMES: usize = 8;

/// A validated record descriptor in static data: everything
/// [`read_record_in`] checks that depends on the descriptor alone (its
/// version and count, each entry's key, slot, kind and optional flag,
/// distinct names, the slot and edge masks). Static data does not change
/// while a reader runs, so a reader of many records of one shape (the
/// linked serializer) validates the shape once and each record with
/// [`read_record_with`].
#[derive(Debug, Clone, Default)]
pub struct RecordShape {
    descriptor: usize,
    count: usize,
    /// The entries in descriptor order (`present` and `payload` unset).
    entries: Vec<RecordEntry>,
    /// Slots holding references (the record's link word, or the wide
    /// record's trailing edge mask).
    reference_mask: u128,
}
impl RecordShape {
    /// Validate the descriptor at `descriptor` of a `count`-field record of
    /// layout `version` within `module`.
    pub fn read<M: Memory + ?Sized>(
        data: &M,
        descriptor: usize,
        count: usize,
        version: u32,
        module: &ModuleBounds,
    ) -> Result<Self, ValueAbiError> {
        if count > max_record_fields(version)
            || !module
                .static_region
                .contains(descriptor, descriptor_bytes(count))
            || word(data, descriptor)? != version
            || word(data, descriptor + 4)? as usize != count
        {
            return Err(invalid());
        }
        let mut slots = 0u128;
        let mut reference_mask = 0u128;
        let mut entries = Vec::with_capacity(count);
        let mut inline = count <= INLINE_NAMES;
        let mut names: Vec<Cow<'_, [u8]>> = Vec::new();
        let mut inline_names: [&[u8]; INLINE_NAMES] = [&[]; INLINE_NAMES];
        for index in 0..count {
            let entry = descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY;
            let key = word(data, entry)? as usize;
            let len = word(data, entry + 4)? as usize;
            let raw = word(data, entry + 8)?;
            let slot = (raw & !OPTIONAL_FIELD_FLAG) as usize;
            let optional = raw & OPTIONAL_FIELD_FLAG != 0;
            if slot >= count
                || slots & slot_bit(slot) != 0
                || len > MAX_RECORD_KEY_BYTES
                || !module.static_region.contains(key, len)
                || key.checked_add(len).is_none_or(|end| end > data.size())
            {
                return Err(invalid());
            }
            let name = data.borrow(key, len).ok_or_else(invalid)?;
            if !name.is_ascii() {
                core::str::from_utf8(&name).map_err(|_| invalid())?;
            }
            match (name, inline_names.get_mut(index).filter(|_| inline)) {
                (Cow::Borrowed(bytes), Some(inline_name)) => *inline_name = bytes,
                (name, _) => {
                    // A name the memory could not lend: collect them all.
                    if inline {
                        names.extend(
                            inline_names[..index]
                                .iter()
                                .map(|name| Cow::Borrowed(*name)),
                        );
                        inline = false;
                    }
                    names.push(name);
                }
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
            let bit = slot_bit(slot);
            slots |= bit;
            if kind.is_reference() {
                reference_mask |= bit;
            }
            entries.push(RecordEntry {
                key,
                len,
                slot,
                kind,
                optional,
                present: false,
                payload: 0,
            });
        }
        // Field names are distinct.
        let distinct_names = if inline {
            let names = &inline_names[..count];
            (1..names.len()).all(|index| !names[..index].contains(&names[index]))
        } else {
            distinct(&mut names)
        };
        if !distinct_names || slots != wide_field_mask(count) {
            return Err(invalid());
        }
        // A wide record's edge mask trails its descriptor.
        if is_wide_record(count) {
            let at = descriptor + RECORD_DESCRIPTOR_HEADER + count * RECORD_DESCRIPTOR_ENTRY;
            if read_slot_set(data, at, presence_words(count)).ok_or_else(invalid)? != reference_mask
            {
                return Err(invalid());
            }
        }
        Ok(Self {
            descriptor,
            count,
            entries,
            reference_mask,
        })
    }
    /// The descriptor address this shape validated.
    #[must_use]
    pub fn descriptor(&self) -> usize {
        self.descriptor
    }
    /// The field count this shape validated.
    #[must_use]
    pub fn count(&self) -> usize {
        self.count
    }
}

/// The descriptor address and field count of the record block `payload`
/// (`block` checked), for [`RecordShape::read`].
pub fn record_descriptor<M: Memory + ?Sized>(
    data: &M,
    payload: u64,
) -> Result<(usize, usize), ValueAbiError> {
    let (ptr, _, _) = block(data, payload, HEAP_RECORD_FLAG)?;
    Ok((word(data, ptr)? as usize, word(data, ptr + 4)? as usize))
}

/// Validate the record `payload` of layout `version` within `module`, as
/// [`validate_aggregate_in`] does, reporting each field to `field` (in
/// descriptor order) and each present slot to `order` (in insertion order)
/// while it is checked: one pass for readers that need the fields, such as
/// the linked serializer. What the callbacks received describes a canonical
/// record only when the result is `Ok`.
pub fn read_record_in<M: Memory + ?Sized>(
    data: &M,
    payload: u64,
    version: u32,
    module: &ModuleBounds,
    field: impl FnMut(RecordEntry),
    order: impl FnMut(usize),
) -> Result<(), ValueAbiError> {
    let (descriptor, count) = record_descriptor(data, payload)?;
    let shape = RecordShape::read(data, descriptor, count, version, module)?;
    read_record_with(data, payload, version, &shape, field, order)
}

/// [`read_record_in`] of a record whose descriptor `shape` validated: the
/// record's own block, its presence and insertion order, its absent slots
/// and its link word.
pub fn read_record_with<M: Memory + ?Sized>(
    data: &M,
    payload: u64,
    version: u32,
    shape: &RecordShape,
    mut field: impl FnMut(RecordEntry),
    mut order: impl FnMut(usize),
) -> Result<(), ValueAbiError> {
    let (ptr, length, header) = block(data, payload, HEAP_RECORD_FLAG)?;
    let count = shape.count;
    if word(data, ptr)? as usize != shape.descriptor
        || word(data, ptr + 4)? as usize != count
        || length != record_bytes(version, count)
    {
        return Err(invalid());
    }
    // Value15 wide records keep their presence in several words.
    let present = read_slot_set(data, ptr + presence_offset(count), presence_words(count))
        .ok_or_else(invalid)?;
    if present & !wide_field_mask(count) != 0 {
        return Err(invalid());
    }
    for entry in &shape.entries {
        let slot = ptr + RECORD_META_BYTES + entry.slot * RECORD_SLOT_BYTES;
        let payload =
            u64::from(word(data, slot)?) | (u64::from(word(data, slot + 4)?) << i32::BITS);
        let present = present & slot_bit(entry.slot) != 0;
        if !present {
            if !entry.optional {
                return Err(invalid());
            }
            if entry.kind == ValueKind::Undefined {
                if payload != 0 {
                    return Err(invalid());
                }
            } else if PrimitiveBox::read(data, payload)?.kind != ValueKind::Undefined {
                return Err(invalid());
            }
        }
        field(RecordEntry {
            present,
            payload,
            ..*entry
        });
    }
    // A wide record's link word is zero (its edge mask trails the
    // descriptor); a narrow record's link word is its edge mask.
    let link = if is_wide_record(count) {
        0
    } else {
        shape.reference_mask as u32
    };
    if word(data, header + HEAP_LINK_OFFSET)? != link {
        return Err(invalid());
    }
    let mut seen = 0u128;
    for index in 0..count {
        let slot = word(data, ptr + order_offset(count) + index * 4)? as usize;
        if index < present.count_ones() as usize {
            if slot >= count || seen & slot_bit(slot) != 0 || present & slot_bit(slot) == 0 {
                return Err(invalid());
            }
            seen |= slot_bit(slot);
            order(slot);
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
