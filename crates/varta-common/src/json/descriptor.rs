//! The static record and dictionary descriptors a schema names, checked
//! against the schema before a decoder builds any value (`no_std`): the
//! host imports and the linked v38 decoder run this one check.
use super::*;
use crate::{abi::StaticRegion, memory::Memory, value::*};
use alloc::borrow::Cow;

/// Why a schema's descriptors are unusable (the host reports
/// `checked JSON: <message>`).
pub type DescriptorError = &'static str;

/// `len` bytes of static metadata at `ptr`.
fn metadata<M: Memory + ?Sized>(
    memory: &M,
    region: StaticRegion,
    ptr: usize,
    len: usize,
) -> Result<Cow<'_, [u8]>, DescriptorError> {
    if !region.contains(ptr, len) {
        return Err("metadata outside static memory");
    }
    memory.borrow(ptr, len).ok_or("memory range")
}
fn words<const N: usize>(data: &[u8], at: usize) -> Result<[u32; N], DescriptorError> {
    let bytes = at
        .checked_add(N * size_of::<u32>())
        .and_then(|end| data.get(at..end))
        .ok_or("memory range")?;
    let mut words = [0; N];
    for (word, chunk) in words
        .iter_mut()
        .zip(bytes.as_chunks::<{ size_of::<u32>() }>().0)
    {
        *word = u32::from_le_bytes(*chunk);
    }
    Ok(words)
}

/// A dictionary's storage descriptor: the schema's value layout (value15 to
/// value18) with exactly its canonical slots (`value::dictionary_storage`):
/// required `keys` and `values` vectors, and under value18 the `holes` count
/// and the `table` slot.
fn dictionary<M: Memory + ?Sized>(
    memory: &M,
    region: StaticRegion,
    descriptor: usize,
    version: u32,
) -> Result<(), DescriptorError> {
    let fields = dictionary_fields(version);
    let data = metadata(memory, region, descriptor, descriptor_bytes(fields))?;
    if !dynamic_layout(version) || words::<2>(&data, 0)? != [version, fields as u32] {
        return Err("dictionary descriptor version or count");
    }
    for (slot, (name, expected)) in dictionary_storage(version).iter().enumerate() {
        let [key, len, raw, kind] = words::<4>(
            &data,
            RECORD_DESCRIPTOR_HEADER + slot * RECORD_DESCRIPTOR_ENTRY,
        )?;
        if raw as usize != slot
            || kind != *expected as u32
            || *metadata(memory, region, key as usize, len as usize)? != *name.as_bytes()
        {
            return Err("dictionary descriptor disagrees with its layout");
        }
    }
    Ok(())
}

/// Whether descriptor entries carry the optional-field flag (schema 8+).
fn flags_optional(version: u32) -> bool {
    matches!(
        version,
        CHECKED_SCHEMA_VERSION
            | VECTOR_SCHEMA_VERSION
            | ERROR_SCHEMA_VERSION
            | ERROR_FAMILY_SCHEMA_VERSION
            | SPARSE_SCHEMA_VERSION
            | BIGINT_SCHEMA_VERSION
            | FUNCTION_SCHEMA_VERSION
            | DYNAMIC_SCHEMA_VERSION
            | PROMISE_SCHEMA_VERSION
            | FUNCTION_FIELD_SCHEMA_VERSION
            | HASHED_DICTIONARY_SCHEMA_VERSION
    )
}

impl JsonSchema {
    /// Check every record and dictionary descriptor the schema names in the
    /// static region of `memory`: layout version, field count, canonical
    /// slots, kinds, optional flags, names and wide-record edge bitmaps.
    pub fn check_descriptors<M: Memory + ?Sized>(
        &self,
        memory: &M,
        region: StaticRegion,
    ) -> Result<(), DescriptorError> {
        for ty in &self.types {
            if let JsonType::Dictionary { descriptor, .. } = ty {
                dictionary(memory, region, *descriptor as usize, self.value_version())?;
            }
            if let JsonType::Record { descriptor, fields } = ty {
                self.record(memory, region, *descriptor as usize, fields)?;
            }
        }
        Ok(())
    }
    fn record<M: Memory + ?Sized>(
        &self,
        memory: &M,
        region: StaticRegion,
        descriptor: usize,
        fields: &[JsonField],
    ) -> Result<(), DescriptorError> {
        let data = metadata(memory, region, descriptor, descriptor_bytes(fields.len()))?;
        if is_wide_record(fields.len()) {
            // The collector traces a wide record through this edge bitmap.
            let at = RECORD_DESCRIPTOR_HEADER + fields.len() * RECORD_DESCRIPTOR_ENTRY;
            let edges = read_slot_set(&data, at, presence_words(fields.len()))
                .ok_or("record edge bitmap")?;
            let expected = fields
                .iter()
                .enumerate()
                .fold(0u128, |mask, (slot, field)| {
                    if self.types[field.ty as usize].kind().is_reference() {
                        mask | 1 << slot
                    } else {
                        mask
                    }
                });
            if edges != expected {
                return Err("record edge bitmap disagrees with schema");
            }
        }
        let [version, count] = words::<2>(&data, 0)?;
        if !compatible_version(version, self.value_version()) || count as usize != fields.len() {
            return Err("record descriptor version or count");
        }
        let flagged = flags_optional(self.version);
        // Slots seen so far (at most `MAX_WIDE_RECORD_FIELDS`).
        let mut slots = 0u128;
        for index in 0..fields.len() {
            let [key, len, slot, kind] = words::<4>(
                &data,
                RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY,
            )?;
            let optional = flagged && slot & OPTIONAL_FIELD_FLAG != 0;
            let slot = if flagged {
                slot & !OPTIONAL_FIELD_FLAG
            } else {
                slot
            };
            let field = fields.get(slot as usize).ok_or("record slot")?;
            let bit = 1u128 << slot;
            if slots & bit != 0
                || optional != field.optional
                || kind != self.types[field.ty as usize].kind() as u32
                || *metadata(memory, region, key as usize, len as usize)? != *field.name.as_bytes()
            {
                return Err("record descriptor disagrees with schema");
            }
            slots |= bit;
        }
        Ok(())
    }
}
