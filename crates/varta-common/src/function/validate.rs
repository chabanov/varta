//! Typed capture validation. Metadata is supplied by the module owner, never by
//! the mutable guest heap. Recursive closures are memoized; resource work is bounded.
use super::{catalog::invalid_catalog, *};
use crate::{
    abi,
    value::{self, ValueAbiError, ValueKind},
};
use std::collections::BTreeSet;
#[derive(Debug, Clone, Copy)]
pub struct CaptureValidation {
    pub nodes: usize,
    pub work: u64,
}
fn invalid() -> ValueAbiError {
    ValueAbiError("invalid typed function capture".into())
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
struct Walker<'a> {
    catalog: &'a FunctionCatalog,
    data: &'a [u8],
    remaining: u64,
    initial: u64,
    nodes: usize,
    seen: BTreeSet<(u64, usize)>,
}
/// Validated, immutable metadata owned by the module loader. Guest memory can
/// never replace the schema. This proves capture layout, not call authority.
#[derive(Debug, Clone)]
pub struct VerifiedFunctionCatalog(FunctionCatalog);
impl VerifiedFunctionCatalog {
    /// Decode and verify metadata once, then retain it without mutable access.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ValueAbiError> {
        FunctionCatalog::from_bytes(bytes).map(Self)
    }

    pub fn as_catalog(&self) -> &FunctionCatalog {
        &self.0
    }
    pub fn validate_closure(
        &self,
        data: &[u8],
        handle: u64,
        work_budget: u64,
    ) -> Result<CaptureValidation, ValueAbiError> {
        self.0.walk_closure(data, handle, work_budget)
    }
}
impl FunctionCatalog {
    pub fn into_verified(self) -> Result<VerifiedFunctionCatalog, ValueAbiError> {
        self.validate()?;
        Ok(VerifiedFunctionCatalog(self))
    }

    /// The catalog must come from immutable module metadata. This method never
    /// grants call authority and never invokes a function from its code index.
    pub fn validate_closure(
        &self,
        data: &[u8],
        handle: u64,
        work_budget: u64,
    ) -> Result<CaptureValidation, ValueAbiError> {
        self.validate()?;
        self.walk_closure(data, handle, work_budget)
    }
    fn walk_closure(
        &self,
        data: &[u8],
        handle: u64,
        work_budget: u64,
    ) -> Result<CaptureValidation, ValueAbiError> {
        let mut walker = Walker {
            catalog: self,
            data,
            remaining: work_budget,
            initial: work_budget,
            nodes: 0,
            seen: BTreeSet::new(),
        };
        walker.function(handle, None, 0)?;
        Ok(CaptureValidation {
            nodes: walker.nodes,
            work: walker.initial - walker.remaining,
        })
    }
}
impl Walker<'_> {
    fn charge(&mut self, n: u64) -> Result<(), ValueAbiError> {
        self.remaining = self
            .remaining
            .checked_sub(n)
            .ok_or_else(|| ValueAbiError("function capture work budget exceeded".into()))?;
        Ok(())
    }
    fn node(&mut self, depth: usize) -> Result<(), ValueAbiError> {
        self.charge(1)?;
        self.nodes += 1;
        if self.nodes > value::MAX_JSON_NODES || depth > value::MAX_JSON_DEPTH {
            return Err(ValueAbiError(
                "function capture graph budget exceeded".into(),
            ));
        }
        Ok(())
    }
    fn function(
        &mut self,
        handle: u64,
        signature: Option<u32>,
        depth: usize,
    ) -> Result<(), ValueAbiError> {
        self.node(depth)?;
        let view = ClosureView::read_envelope(self.data, handle)?;
        let catalog = self.catalog;
        self.charge(catalog.functions.len() as u64 + MAX_CAPTURES as u64 * 8)?;
        let schema = catalog
            .functions
            .iter()
            .find(|row| row.code == view.code)
            .ok_or_else(invalid)?;
        if schema.signature != view.signature || signature.is_some_and(|id| id != view.signature) {
            return Err(invalid());
        }
        if !self
            .seen
            .insert((handle, MAX_CAPTURE_TYPES + view.signature as usize))
        {
            return Ok(());
        }
        let captures = view.environment.map(|e| e.bindings());
        if captures.as_ref().map_or(0, ExactSizeIterator::len) != schema.captures.len() {
            return Err(invalid());
        }
        if let Some(captures) = captures {
            for (handle, &id) in captures.zip(&schema.captures) {
                let binding = BindingView::read_envelope(self.data, handle)?;
                let expected = &catalog.bindings[id as usize];
                let ty = &catalog.types[expected.ty as usize];
                if !binding.matches_shape(id, if expected.result { 2 } else { 1 }, ty.reference())
                    || binding
                        .status
                        .is_some_and(|status| status > abi::HostStatus::TooLarge as u64)
                {
                    return Err(invalid());
                }
                self.value(expected.ty, binding.value, depth + 1)?;
            }
        }
        Ok(())
    }
    fn value(&mut self, id: u32, payload: u64, depth: usize) -> Result<(), ValueAbiError> {
        let catalog = self.catalog;
        let ty = catalog.types.get(id as usize).ok_or_else(invalid_catalog)?;
        if let CaptureType::Function { signature } = ty {
            return self.function(payload, Some(*signature), depth);
        }
        self.node(depth)?;
        if ty.reference() && !self.seen.insert((payload, id as usize)) {
            return Ok(());
        }
        match ty {
            CaptureType::I32 if payload as i64 != payload as i32 as i64 => Err(invalid()),
            CaptureType::Boolean if payload > 1 => Err(invalid()),
            CaptureType::Null | CaptureType::Undefined if payload != 0 => Err(invalid()),
            CaptureType::String => {
                let len = (payload >> 32) as usize;
                let ptr = payload as u32 as usize;
                if len > abi::MAX_STRING_BYTES
                    || !len.is_multiple_of(2)
                    || ptr.checked_add(len).is_none_or(|end| end > self.data.len())
                {
                    return Err(invalid());
                }
                Ok(())
            }
            CaptureType::BigInt => {
                crate::bigint::BigIntView::read(self.data, payload)?;
                Ok(())
            }
            CaptureType::Error { class } => {
                let error = value::ErrorObject::read_with_version(
                    self.data,
                    payload,
                    catalog.value_version,
                )?;
                if class.is_some_and(|class| class != error.class as u32) {
                    return Err(invalid());
                }
                Ok(())
            }
            CaptureType::Record { fields } => {
                self.charge(
                    fields.len() as u64 * 8
                        + fields
                            .iter()
                            .map(|field| field.name.len() as u64 * 8)
                            .sum::<u64>(),
                )?;
                value::validate_aggregate(
                    self.data,
                    ValueKind::Record,
                    payload,
                    catalog.value_version,
                )?;
                let ptr = payload as u32 as usize;
                let descriptor = word(self.data, ptr)? as usize;
                if word(self.data, ptr + 4)? as usize != fields.len() {
                    return Err(invalid());
                }
                let present = value::read_slot_set(
                    self.data,
                    ptr + value::presence_offset(fields.len()),
                    value::presence_words(fields.len()),
                )
                .ok_or_else(invalid)?;
                for at in 0..fields.len() {
                    let entry = descriptor
                        + value::RECORD_DESCRIPTOR_HEADER
                        + at * value::RECORD_DESCRIPTOR_ENTRY;
                    let name = word(self.data, entry)? as usize;
                    let len = word(self.data, entry + 4)? as usize;
                    let bytes = self
                        .data
                        .get(name..name.checked_add(len).ok_or_else(invalid)?)
                        .ok_or_else(invalid)?;
                    let raw = word(self.data, entry + 8)?;
                    let slot = (raw & !value::OPTIONAL_FIELD_FLAG) as usize;
                    let field = fields.get(slot).ok_or_else(invalid)?;
                    if field.name.as_bytes() != bytes {
                        return Err(invalid());
                    }
                    if (raw & value::OPTIONAL_FIELD_FLAG != 0) != field.optional
                        || Some(word(self.data, entry + 12)?)
                            != catalog.types[field.ty as usize]
                                .value_kind()
                                .map(|kind| kind as u32)
                    {
                        return Err(invalid());
                    }
                    if present & (1u128 << slot) != 0 {
                        self.value(field.ty, wide(self.data, ptr + 8 + slot * 8)?, depth + 1)?;
                    }
                }
                Ok(())
            }
            // Class field types are nominal and compile-time checked.
            CaptureType::Class => {
                value::validate_aggregate(
                    self.data,
                    ValueKind::Record,
                    payload,
                    catalog.value_version,
                )?;
                Ok(())
            }
            CaptureType::Array { element } => {
                let vector =
                    value::Vector::read_with_version(self.data, payload, catalog.value_version)?;
                self.charge(vector.capacity as u64 * 4)?;
                if Some(vector.kind) != catalog.types[*element as usize].value_kind() {
                    return Err(invalid());
                }
                for index in 0..vector.length {
                    if vector.is_present(self.data, index)? {
                        self.value(
                            *element,
                            wide(self.data, vector.storage + 8 + index * 8)?,
                            depth + 1,
                        )?;
                    }
                }
                Ok(())
            }
            CaptureType::PrimitiveUnion { members } | CaptureType::ReferenceUnion { members } => {
                let cell =
                    value::ReferenceBox::read_envelope(self.data, payload, catalog.value_version)?;
                let selected = members
                    .iter()
                    .copied()
                    .find(|id| {
                        let ty = &catalog.types[*id as usize];
                        if ty.value_kind() != Some(cell.kind) {
                            return false;
                        }
                        if let CaptureType::Error { class: Some(class) } = ty {
                            value::ErrorObject::read_with_version(
                                self.data,
                                cell.payload,
                                catalog.value_version,
                            )
                            .is_ok_and(|error| error.class as u32 == *class)
                        } else {
                            true
                        }
                    })
                    .ok_or_else(invalid)?;
                self.value(selected, cell.payload, depth + 1)
            }
            _ => Ok(()),
        }
    }
}
