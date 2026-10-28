//! Immutable compiler metadata for typed inspection of private lexical captures.
use super::*;
use crate::{
    abi,
    value::{self, ValueAbiError, ValueKind},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const CATALOG_SECTION: &str = "varta.functions";
pub const CATALOG_VERSION: u32 = 1;
pub const MAX_CAPTURE_TYPES: usize =
    MAX_TYPES + value::MAX_RECORD_SHAPES + value::MAX_ARRAY_TYPES + 2 * value::MAX_UNION_TYPES + 32;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionCatalog {
    pub version: u32,
    pub value_version: u32,
    pub types: Vec<CaptureType>,
    pub bindings: Vec<CaptureBinding>,
    pub functions: Vec<CaptureFunction>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CaptureType {
    I32,
    Float64,
    Boolean,
    String,
    Null,
    Undefined,
    BigInt,
    Error { class: Option<u32> },
    Function { signature: u32 },
    Record { fields: Vec<CaptureField> },
    Array { element: u32 },
    PrimitiveUnion { members: Vec<u32> },
    ReferenceUnion { members: Vec<u32> },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureField {
    pub name: String,
    pub ty: u32,
    pub optional: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureBinding {
    pub ty: u32,
    pub result: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureFunction {
    pub code: u32,
    pub signature: u32,
    pub captures: Vec<u32>,
}
pub(super) fn invalid_catalog() -> ValueAbiError {
    ValueAbiError("invalid function capture catalog".into())
}
impl CaptureType {
    pub fn value_kind(&self) -> Option<ValueKind> {
        Some(match self {
            Self::I32 => ValueKind::I32,
            Self::Float64 => ValueKind::Float64,
            Self::Boolean => ValueKind::Boolean,
            Self::String => ValueKind::String,
            Self::Null => ValueKind::Null,
            Self::Undefined => ValueKind::Undefined,
            Self::BigInt => ValueKind::BigInt,
            Self::Error { .. } => ValueKind::Error,
            Self::Record { .. } => ValueKind::Record,
            Self::Array { .. } => ValueKind::Array,
            Self::PrimitiveUnion { .. } => ValueKind::PrimitiveUnion,
            Self::ReferenceUnion { .. } => ValueKind::ReferenceUnion,
            Self::Function { .. } => ValueKind::Function,
        })
    }
    pub fn reference(&self) -> bool {
        self.value_kind().is_none_or(ValueKind::is_reference)
    }
    fn children(&self) -> Vec<u32> {
        match self {
            Self::Record { fields } => fields.iter().map(|f| f.ty).collect(),
            Self::Array { element } => vec![*element],
            Self::PrimitiveUnion { members } | Self::ReferenceUnion { members } => members.clone(),
            _ => vec![],
        }
    }
}
impl FunctionCatalog {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ValueAbiError> {
        if bytes.len() > abi::MAX_WASM_BYTES {
            return Err(invalid_catalog());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| invalid_catalog())?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, ValueAbiError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| invalid_catalog())?;
        if bytes.len() > abi::MAX_WASM_BYTES {
            return Err(invalid_catalog());
        }
        Ok(bytes)
    }
    pub fn validate(&self) -> Result<(), ValueAbiError> {
        if self.version != CATALOG_VERSION
            || !matches!(
                self.value_version,
                value::BIGINT_VALUE_VERSION | value::FUNCTION_VALUE_VERSION
            )
            || self.types.len() > MAX_CAPTURE_TYPES
            || self.bindings.len() > MAX_CAPTURE_TYPES
            || self.functions.len() > abi::MAX_USER_FUNCTIONS
        {
            return Err(invalid_catalog());
        }
        for binding in &self.bindings {
            let ty = self
                .types
                .get(binding.ty as usize)
                .ok_or_else(invalid_catalog)?;
            if binding.result
                && !matches!(
                    ty,
                    CaptureType::I32
                        | CaptureType::Float64
                        | CaptureType::Boolean
                        | CaptureType::String
                        | CaptureType::Undefined
                )
            {
                return Err(invalid_catalog());
            }
        }
        let mut codes = BTreeSet::new();
        for function in &self.functions {
            if function.code as usize >= abi::MAX_USER_FUNCTIONS
                || function.signature as usize >= MAX_TYPES
                || !codes.insert(function.code)
                || function.captures.len() > MAX_CAPTURES
                || function
                    .captures
                    .iter()
                    .any(|id| *id as usize >= self.bindings.len())
            {
                return Err(invalid_catalog());
            }
        }
        for ty in &self.types {
            if ty
                .children()
                .iter()
                .any(|id| *id as usize >= self.types.len())
            {
                return Err(invalid_catalog());
            }
            match ty {
                CaptureType::Function { signature } if *signature as usize >= MAX_TYPES => {
                    return Err(invalid_catalog());
                }
                CaptureType::Error { class: Some(class) }
                    if value::ErrorClass::from_tag(*class).is_none() =>
                {
                    return Err(invalid_catalog());
                }
                CaptureType::Record { fields } => {
                    let mut names = BTreeSet::new();
                    if fields.windows(2).any(|pair| pair[0].name >= pair[1].name) {
                        return Err(invalid_catalog());
                    }
                    if fields.len() > value::MAX_RECORD_FIELDS {
                        return Err(invalid_catalog());
                    }
                    for field in fields {
                        if field.name.len() > value::MAX_RECORD_KEY_BYTES
                            || !names.insert(&field.name)
                            || self.value_version == value::BIGINT_VALUE_VERSION
                                && matches!(
                                    self.types[field.ty as usize],
                                    CaptureType::Function { .. }
                                )
                        {
                            return Err(invalid_catalog());
                        }
                        if field.optional && !self.contains_undefined(field.ty) {
                            return Err(invalid_catalog());
                        }
                    }
                }
                CaptureType::Array { element }
                    if self.value_version == value::BIGINT_VALUE_VERSION
                        && matches!(
                            self.types[*element as usize],
                            CaptureType::Function { .. }
                        ) =>
                {
                    return Err(invalid_catalog());
                }
                CaptureType::PrimitiveUnion { members }
                | CaptureType::ReferenceUnion { members } => {
                    let mut ids = BTreeSet::new();
                    let mut records = 0;
                    let mut arrays = 0;
                    if members.len() < 2 || members.len() > MAX_CAPTURE_TYPES {
                        return Err(invalid_catalog());
                    }
                    for id in members {
                        let member = &self.types[*id as usize];
                        if !ids.insert(id)
                            || matches!(
                                member,
                                CaptureType::PrimitiveUnion { .. }
                                    | CaptureType::ReferenceUnion { .. }
                                    | CaptureType::I32
                            )
                        {
                            return Err(invalid_catalog());
                        }
                        if matches!(member, CaptureType::Function { .. })
                            && (self.value_version != value::FUNCTION_VALUE_VERSION
                                || matches!(ty, CaptureType::PrimitiveUnion { .. }))
                        {
                            return Err(invalid_catalog());
                        }
                        records += usize::from(matches!(member, CaptureType::Record { .. }));
                        arrays += usize::from(matches!(member, CaptureType::Array { .. }));
                        if matches!(ty, CaptureType::PrimitiveUnion { .. })
                            && matches!(
                                member,
                                CaptureType::Record { .. }
                                    | CaptureType::Array { .. }
                                    | CaptureType::Error { .. }
                                    | CaptureType::BigInt
                            )
                        {
                            return Err(invalid_catalog());
                        }
                    }
                    if members
                        .iter()
                        .filter(|id| {
                            matches!(self.types[**id as usize], CaptureType::Function { .. })
                        })
                        .count()
                        > 1
                    {
                        return Err(invalid_catalog());
                    }
                    if records > 1 || arrays > 1 {
                        return Err(invalid_catalog());
                    }
                }
                _ => {}
            }
        }
        let mut state = vec![0u8; self.types.len()];
        let mut heights = vec![0usize; self.types.len()];
        for id in 0..self.types.len() {
            self.acyclic(id, 0, &mut state, &mut heights)?;
        }
        Ok(())
    }
    fn contains_undefined(&self, id: u32) -> bool {
        match &self.types[id as usize] {
            CaptureType::Undefined => true,
            CaptureType::PrimitiveUnion { members } | CaptureType::ReferenceUnion { members } => {
                members
                    .iter()
                    .any(|id| matches!(self.types.get(*id as usize), Some(CaptureType::Undefined)))
            }
            _ => false,
        }
    }
    fn acyclic(
        &self,
        id: usize,
        depth: usize,
        state: &mut [u8],
        heights: &mut [usize],
    ) -> Result<(), ValueAbiError> {
        if depth > value::MAX_JSON_DEPTH || state[id] == 1 {
            return Err(invalid_catalog());
        }
        if state[id] == 2 {
            return if depth + heights[id] <= value::MAX_JSON_DEPTH {
                Ok(())
            } else {
                Err(invalid_catalog())
            };
        }
        state[id] = 1;
        for child in self.types[id].children() {
            self.acyclic(child as usize, depth + 1, state, heights)?;
            heights[id] = heights[id].max(heights[child as usize] + 1);
        }
        state[id] = 2;
        Ok(())
    }
}
