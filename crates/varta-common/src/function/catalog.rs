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
    /// Continuation-frame layouts; present only in [`frame::FRAME_CATALOG_VERSION`]
    /// (cell layouts) and [`frame::FRAME_LIVENESS_CATALOG_VERSION`] (liveness layouts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<FrameLayout>,
}
/// One suspendable function's frame. Binding ids index `bindings`; layout ids
/// are the row positions and resume codes are unique user-function indices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameLayout {
    pub code: u32,
    /// Cell layouts (version 2): every saved binding, one frame-owned or
    /// shared cell each. Liveness layouts (version 3): the bindings the
    /// creator saves for state 0 (the parameters), in entry order.
    pub slots: Vec<FrameSlot>,
    /// Liveness layouts only: saved entries the frame's segments hold, at least
    /// the entries of every state and at most [`frame::MAX_FRAME_BINDINGS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
    /// Suspension points 1..=n in state order; state 0 is the initial state.
    pub states: Vec<FrameState>,
    /// Binding id of a returned completion value, absent for void results.
    pub result: Option<u32>,
    /// Admissible `Return` completion input, absent when none is admitted.
    pub return_input: Option<frame::FrameInput>,
    /// Binding id of an uncaught exception completion.
    pub exception: u32,
    /// A `function*` frame (v33): owned by its generator object, never in the
    /// live-frame root table and never resumed by `varta_resume`. Every input
    /// of a generator row is [`frame::FrameInput::Guest`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generator: bool,
}
/// A saved binding: frame-owned cells are allocated with the frame; shared
/// cells are the closure cells of captured locals and may still be absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameSlot {
    pub binding: u32,
    pub shared: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameState {
    pub input: frame::FrameInput,
    /// Binding id of the yielded value stored in the completion slot.
    pub yielded: u32,
    /// Liveness layouts only: the bindings live across this suspension point,
    /// saved in entry order. A shared slot takes one entry (its closure cell),
    /// an unshared two-slot (`result`) binding two and every other binding one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved: Vec<FrameSlot>,
}
impl FrameSlot {
    /// Saved entries of this slot in a liveness layout.
    pub fn entries(&self, catalog: &FunctionCatalog) -> Option<usize> {
        let binding = catalog.bindings.get(self.binding as usize)?;
        Some(if binding.result && !self.shared { 2 } else { 1 })
    }
}
impl FrameLayout {
    /// Whether this row describes a liveness layout (catalog version 3).
    pub fn liveness(&self) -> bool {
        self.capacity.is_some()
    }
}
/// `Class` is a nominal class instance (v32): a record carrying its own class
/// descriptor. Its fields are fixed by the class declaration and may form
/// cycles through other classes, so the catalog keeps it as a leaf.
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
    Error {
        class: Option<u32>,
    },
    Function {
        signature: u32,
    },
    Record {
        fields: Vec<CaptureField>,
    },
    Array {
        element: u32,
    },
    /// A value15 dictionary (v34 `index_signatures`) of `value`.
    Dictionary {
        value: u32,
    },
    PrimitiveUnion {
        members: Vec<u32>,
    },
    ReferenceUnion {
        members: Vec<u32>,
    },
    Class,
    /// A generator object (v33): an opaque frame handle with no value tag; it
    /// may be saved in frames but never crosses a typed value boundary.
    Generator,
    /// An `unknown` binding (v34): a reference-union cell whose member may be
    /// any kind the catalog's value version admits in a cell.
    Unknown,
    /// A promise object (v34): a guest-heap record handle with private fields
    /// and no value tag; it may be saved in frames and closure cells but never
    /// crosses a typed value boundary.
    Promise,
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
            Self::Record { .. } | Self::Class => ValueKind::Record,
            Self::Array { .. } => ValueKind::Array,
            Self::Dictionary { .. } => ValueKind::Dictionary,
            Self::PrimitiveUnion { .. } => ValueKind::PrimitiveUnion,
            Self::ReferenceUnion { .. } | Self::Unknown => ValueKind::ReferenceUnion,
            Self::Function { .. } => ValueKind::Function,
            Self::Generator | Self::Promise => return None,
        })
    }
    pub fn reference(&self) -> bool {
        self.value_kind().is_none_or(ValueKind::is_reference)
    }
    fn children(&self) -> Vec<u32> {
        match self {
            Self::Record { fields } => fields.iter().map(|f| f.ty).collect(),
            Self::Array { element } | Self::Dictionary { value: element } => vec![*element],
            Self::PrimitiveUnion { members } | Self::ReferenceUnion { members } => members.clone(),
            _ => vec![],
        }
    }
}
impl FunctionCatalog {
    /// Read a version 1 catalog. Frame catalogs (versions 2 and 3) are rejected
    /// here, so existing artifacts and runtimes keep exactly their earlier contract.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ValueAbiError> {
        Self::read(bytes, &[CATALOG_VERSION])
    }
    /// Read a version 1 or cell-layout version 2 catalog; version 3 is rejected.
    pub fn from_frame_bytes(bytes: &[u8]) -> Result<Self, ValueAbiError> {
        Self::read(bytes, &[CATALOG_VERSION, frame::FRAME_CATALOG_VERSION])
    }
    /// Read a version 1 or liveness-layout version 3 catalog (v35
    /// `frame_liveness`); cell layouts (version 2) are rejected.
    pub fn from_liveness_bytes(bytes: &[u8]) -> Result<Self, ValueAbiError> {
        Self::read(
            bytes,
            &[CATALOG_VERSION, frame::FRAME_LIVENESS_CATALOG_VERSION],
        )
    }
    fn read(bytes: &[u8], accepted: &[u32]) -> Result<Self, ValueAbiError> {
        if bytes.len() > abi::MAX_WASM_BYTES {
            return Err(invalid_catalog());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| invalid_catalog())?;
        if !accepted.contains(&value.version) {
            return Err(invalid_catalog());
        }
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
        let frames = matches!(
            self.version,
            frame::FRAME_CATALOG_VERSION | frame::FRAME_LIVENESS_CATALOG_VERSION
        );
        if !(self.version == CATALOG_VERSION || frames)
            || frames == self.frames.is_empty()
            || !matches!(
                self.value_version,
                value::BIGINT_VALUE_VERSION
                    | value::FUNCTION_VALUE_VERSION
                    | value::DYNAMIC_VALUE_VERSION
                    | value::PROMISE_VALUE_VERSION
                    | value::FUNCTION_FIELD_VALUE_VERSION
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
                CaptureType::Dictionary { .. }
                    if !ValueKind::Dictionary.supported_by(self.value_version) =>
                {
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
                    if fields.len() > value::max_record_fields(self.value_version) {
                        return Err(invalid_catalog());
                    }
                    for field in fields {
                        if field.name.len() > value::MAX_RECORD_KEY_BYTES
                            || !names.insert(&field.name)
                            || !value::function_layout(self.value_version)
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
                    if !value::function_layout(self.value_version)
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
                            && (!value::function_layout(self.value_version)
                                || matches!(ty, CaptureType::PrimitiveUnion { .. }))
                        {
                            return Err(invalid_catalog());
                        }
                        records += usize::from(matches!(
                            member,
                            CaptureType::Record { .. } | CaptureType::Class
                        ));
                        arrays += usize::from(matches!(member, CaptureType::Array { .. }));
                        if matches!(ty, CaptureType::PrimitiveUnion { .. })
                            && matches!(
                                member,
                                CaptureType::Record { .. }
                                    | CaptureType::Class
                                    | CaptureType::Array { .. }
                                    | CaptureType::Dictionary { .. }
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
        self.validate_frames()?;
        let mut state = vec![0u8; self.types.len()];
        let mut heights = vec![0usize; self.types.len()];
        for id in 0..self.types.len() {
            self.acyclic(id, 0, &mut state, &mut heights)?;
        }
        Ok(())
    }
    fn validate_frames(&self) -> Result<(), ValueAbiError> {
        let binding = |id: u32| self.bindings.get(id as usize).ok_or_else(invalid_catalog);
        let mut codes = BTreeSet::new();
        if self.frames.len() > abi::MAX_USER_FUNCTIONS {
            return Err(invalid_catalog());
        }
        for layout in &self.frames {
            if layout.code as usize >= abi::MAX_USER_FUNCTIONS
                || !codes.insert(layout.code)
                || layout.slots.len() > frame::MAX_FRAME_BINDINGS
                || layout.states.len() > frame::MAX_SUSPENSION_STATES
            {
                return Err(invalid_catalog());
            }
            let guest = layout
                .states
                .iter()
                .map(|state| state.input)
                .chain(layout.return_input)
                .map(|input| input == frame::FrameInput::Guest);
            if guest.into_iter().any(|guest| guest != layout.generator)
                || layout.generator && layout.return_input.is_none()
            {
                return Err(invalid_catalog());
            }
            for id in layout
                .slots
                .iter()
                .chain(layout.states.iter().flat_map(|state| &state.saved))
                .map(|slot| slot.binding)
                .chain(layout.states.iter().map(|state| state.yielded))
                .chain(layout.result)
                .chain([layout.exception])
            {
                binding(id)?;
            }
            self.validate_liveness(layout)?;
        }
        Ok(())
    }
    /// Version 2 rows carry no liveness fields; version 3 rows declare a
    /// bounded capacity that holds the saved entries of every state.
    fn validate_liveness(&self, layout: &FrameLayout) -> Result<(), ValueAbiError> {
        if self.version != frame::FRAME_LIVENESS_CATALOG_VERSION {
            return if layout.liveness() || layout.states.iter().any(|s| !s.saved.is_empty()) {
                Err(invalid_catalog())
            } else {
                Ok(())
            };
        }
        let capacity = layout
            .capacity
            .map(|capacity| capacity as usize)
            .filter(|capacity| *capacity <= frame::MAX_FRAME_BINDINGS)
            .ok_or_else(invalid_catalog)?;
        let entries = |slots: &[FrameSlot]| -> Result<usize, ValueAbiError> {
            slots.iter().try_fold(0usize, |total, slot| {
                slot.entries(self)
                    .map(|entries| total + entries)
                    .ok_or_else(invalid_catalog)
            })
        };
        for slots in std::iter::once(&layout.slots).chain(layout.states.iter().map(|s| &s.saved)) {
            if entries(slots)? > capacity {
                return Err(invalid_catalog());
            }
        }
        Ok(())
    }
    fn contains_undefined(&self, id: u32) -> bool {
        match &self.types[id as usize] {
            CaptureType::Undefined | CaptureType::Unknown => true,
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
