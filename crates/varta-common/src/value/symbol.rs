//! Value18 `Symbol` values (v38 `symbol_values`, K9 D2).
//!
//! A symbol is the guest's private record block of the symbol prelude
//! class: exactly the two present fields [`SYMBOL_DESCRIPTION_FIELD`] (a
//! String) and [`SYMBOL_DESCRIBED_FIELD`] (a Boolean, `false` for a symbol
//! without a description, whose description field is then the empty
//! string). Like every prelude instance field, their keys carry the
//! private-name prefix, so no serializer reads them as properties. The block's address is the symbol's identity; no reader
//! compares symbols by their fields.
use super::{ModuleBounds, ValueAbiError, ValueKind, read_record_in, string_range, symbol_layout};
use crate::memory::Memory;

/// The String field of a symbol record: its description (empty when the
/// symbol has none).
pub const SYMBOL_DESCRIPTION_FIELD: &str = "#description";
/// The Boolean field of a symbol record: whether it has a description.
pub const SYMBOL_DESCRIBED_FIELD: &str = "#described";
/// The field count of a symbol record.
pub const SYMBOL_FIELDS: usize = 2;

/// A validated symbol record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolRecord {
    /// The description string handle, or `None` for `Symbol()`.
    pub description: Option<u64>,
}

fn noncanonical() -> ValueAbiError {
    ValueAbiError("noncanonical symbol".into())
}

/// Read the symbol record `handle` of layout `version` within `module`:
/// a canonical record with exactly the two present symbol fields, a String
/// description of at most the profile's string bytes and a Boolean flag.
pub fn symbol_record<M: Memory + ?Sized>(
    data: &M,
    handle: u64,
    version: u32,
    module: ModuleBounds,
) -> Result<SymbolRecord, ValueAbiError> {
    if !symbol_layout(version) {
        return Err(noncanonical());
    }
    let mut fields = [None::<(ValueKind, bool, u64)>; SYMBOL_FIELDS];
    let mut count = 0usize;
    let mut misnamed = false;
    read_record_in(
        data,
        handle,
        version,
        &module,
        |entry| {
            count += 1;
            let name = data.borrow(entry.key, entry.len);
            let slot = match name.as_deref() {
                Some(name) if name == SYMBOL_DESCRIPTION_FIELD.as_bytes() => 0,
                Some(name) if name == SYMBOL_DESCRIBED_FIELD.as_bytes() => 1,
                _ => {
                    misnamed = true;
                    return;
                }
            };
            if let Some(field) = fields.get_mut(slot) {
                *field = Some((entry.kind, entry.present, entry.payload));
            }
        },
        |_| {},
    )?;
    if misnamed || count != SYMBOL_FIELDS {
        return Err(noncanonical());
    }
    let (Some((ValueKind::String, true, text)), Some((ValueKind::Boolean, true, described))) =
        (fields[0], fields[1])
    else {
        return Err(noncanonical());
    };
    let (ptr, len) = string_range(text);
    if len > module.limits.data.string_bytes
        || !len.is_multiple_of(2)
        || ptr.checked_add(len).is_none_or(|end| end > data.size())
    {
        return Err(noncanonical());
    }
    match described {
        0 if len == 0 => Ok(SymbolRecord { description: None }),
        1 => Ok(SymbolRecord {
            description: Some(text),
        }),
        _ => Err(noncanonical()),
    }
}
