//! Bounded views of existing managed closure envelopes; successful reads allocate no memory.
//! These do not establish module signature/code membership or decode captured
//! values. A typed consumer must separately check the module's binding schema.
use super::*;
use crate::{abi, value};
use value::ValueAbiError;
fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical managed function envelope".into())
}
fn word(data: &[u8], at: usize) -> Result<u32, ValueAbiError> {
    data.get(at..at.checked_add(4).ok_or_else(invalid)?)
        .and_then(|v| v.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(invalid)
}
fn wide(data: &[u8], at: usize) -> Result<u64, ValueAbiError> {
    data.get(at..at.checked_add(8).ok_or_else(invalid)?)
        .and_then(|v| v.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(invalid)
}
fn envelope(data: &[u8], handle: u64, bytes: usize, flag: i32) -> Result<usize, ValueAbiError> {
    let ptr = handle as u32 as usize;
    if (handle >> 32) as usize != bytes
        || ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
        || !ptr.is_multiple_of(value::HEAP_ALIGNMENT)
    {
        return Err(invalid());
    }
    let header = ptr - abi::STRING_HEADER_BYTES;
    let allocated = word(data, header)? as usize;
    if allocated < abi::STRING_HEADER_BYTES + bytes
        || !allocated.is_multiple_of(value::HEAP_ALIGNMENT)
        || header
            .checked_add(allocated)
            .is_none_or(|end| end > data.len())
        || word(data, header + value::HEAP_FLAGS_OFFSET)?
            != value::HEAP_ALLOCATED_FLAG | flag as u32
    {
        return Err(invalid());
    }
    Ok(ptr)
}
/// A binding's physical envelope. `type_id` is module-private, not a ValueKind.
/// `reference` describes only the first payload slot; payloads remain opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingView {
    pub type_id: u32,
    pub reference: bool,
    pub value: u64,
    pub status: Option<u64>,
}
impl BindingView {
    pub fn read_envelope(data: &[u8], handle: u64) -> Result<Self, ValueAbiError> {
        let bytes = (handle >> 32) as usize;
        if !matches!(bytes, 16 | 24) {
            return Err(invalid());
        }
        let ptr = envelope(data, handle, bytes, BINDING_FLAG)?;
        let slots = (bytes - value::RECORD_META_BYTES) / abi::STRING_ROOT_SLOT_BYTES;
        let mask = word(
            data,
            ptr - abi::STRING_HEADER_BYTES + value::HEAP_LINK_OFFSET,
        )?;
        if mask > 1 || word(data, ptr + 4)? as usize != slots {
            return Err(invalid());
        }
        let status = if slots == 2 {
            Some(wide(data, ptr + 16)?)
        } else {
            None
        };
        Ok(Self {
            type_id: word(data, ptr)?,
            reference: mask != 0,
            value: wide(data, ptr + 8)?,
            status,
        })
    }
    /// Check compiler-provided binding metadata without interpreting its payload.
    pub fn matches_shape(self, type_id: u32, slots: u32, reference: bool) -> bool {
        self.type_id == type_id
            && slots == if self.status.is_some() { 2 } else { 1 }
            && self.reference == reference
    }
}
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentView<'a> {
    bindings: &'a [u8],
}
impl<'a> EnvironmentView<'a> {
    pub fn read_envelope(data: &'a [u8], handle: u64) -> Result<Self, ValueAbiError> {
        let bytes = (handle >> 32) as usize;
        if !(value::RECORD_META_BYTES + 8..=value::RECORD_META_BYTES + MAX_CAPTURES * 8)
            .contains(&bytes)
            || !(bytes - value::RECORD_META_BYTES).is_multiple_of(8)
        {
            return Err(invalid());
        }
        let ptr = envelope(data, handle, bytes, ENVIRONMENT_FLAG)?;
        let count = (bytes - value::RECORD_META_BYTES) / 8;
        if word(data, ptr)? != INTERNAL_KIND as u32
            || word(data, ptr + 4)? as usize != count
            || word(
                data,
                ptr - abi::STRING_HEADER_BYTES + value::HEAP_LINK_OFFSET,
            )? != value::field_mask(count)
        {
            return Err(invalid());
        }
        let bindings = data.get(ptr + 8..ptr + bytes).ok_or_else(invalid)?;
        let result = Self { bindings };
        for binding in result.bindings() {
            BindingView::read_envelope(data, binding)?;
        }
        Ok(result)
    }
    pub fn bindings(self) -> impl ExactSizeIterator<Item = u64> + 'a {
        self.bindings
            .as_chunks::<8>()
            .0
            .iter()
            .map(|word| u64::from_le_bytes(*word))
    }
}
#[derive(Debug, Clone, Copy)]
pub struct ClosureView<'a> {
    pub signature: u32,
    pub code: u32,
    pub environment: Option<EnvironmentView<'a>>,
}
impl<'a> ClosureView<'a> {
    /// Read the v12+ physical closure form; legacy v11 cells stay distinct.
    pub fn read_envelope(data: &'a [u8], handle: u64) -> Result<Self, ValueAbiError> {
        Self::read_envelope_within(data, handle, &crate::limits::FROZEN)
    }
    /// [`Self::read_envelope`] with a profile's function and signature limits.
    pub fn read_envelope_within(
        data: &'a [u8],
        handle: u64,
        limits: &crate::limits::ProgramLimits,
    ) -> Result<Self, ValueAbiError> {
        let ptr = envelope(data, handle, CLOSURE_BYTES, HEAP_FLAG)?;
        let signature = word(data, ptr)?;
        let code = word(data, ptr + CODE_OFFSET as usize)?;
        let environment = wide(data, ptr + ENVIRONMENT_OFFSET as usize)?;
        let mask = word(
            data,
            ptr - abi::STRING_HEADER_BYTES + value::HEAP_LINK_OFFSET,
        )?;
        if signature as usize >= limits.function_types
            || code as usize >= limits.user_functions
            || word(data, ptr + 4)? != FUNCTION_FIELDS as u32
            || word(data, ptr + 12)? != 0
            || mask
                != if environment == 0 {
                    0
                } else {
                    ENVIRONMENT_MASK as u32
                }
        {
            return Err(invalid());
        }
        let environment = if environment == 0 {
            None
        } else {
            Some(EnvironmentView::read_envelope(data, environment)?)
        };
        Ok(Self {
            signature,
            code,
            environment,
        })
    }
}
