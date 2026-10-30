//! Module-level admission of the resumable-frame surface (K4.4 step 1).
//!
//! Runtime preflight and artifact validation each extract the same
//! [`FrameSurface`] from portable Wasm with their own parser and apply these
//! shared rules before compilation or any guest effect. Only a profile with
//! [`LanguageProfile::resumable_frames`] may carry a frame catalog, frame
//! layouts or the exported dispatcher: cell layouts (catalog version 2) for
//! v33/v34 and liveness layouts (version 3) for a
//! [`LanguageProfile::frame_liveness`] profile (v35). Every earlier profile
//! keeps the version 1 catalog reader and rejects the whole surface. Admission never
//! grants call authority: the runtime does not call the dispatcher until a
//! scheduler exists.
use super::{CaptureType, FrameLayout, FunctionCatalog, frame};
use crate::language::LanguageProfile;
use crate::value::ValueAbiError;
use std::collections::BTreeSet;

/// Scalar Wasm value types as far as admission distinguishes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmScalar {
    I32,
    I64,
    F32,
    F64,
    /// Any vector or reference type; never part of a frame signature.
    Other,
}
use WasmScalar::{I32, I64};

/// The dispatcher's exact signature `(token:i64, completion:i32, value:i64) -> outcome:i32`.
pub const RESUME_PARAMS: [WasmScalar; 3] = [I64, I32, I64];
pub const RESUME_RESULTS: [WasmScalar; 1] = [I32];
/// A resume body takes the dispatcher's arguments (with the frame handle in
/// place of the token) and reports through the frame, not a result.
pub const RESUME_BODY_RESULTS: [WasmScalar; 0] = [];
/// A creator returns exactly one resume token.
pub const CREATOR_RESULTS: [WasmScalar; 1] = [I64];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub params: Vec<WasmScalar>,
    pub results: Vec<WasmScalar>,
}
impl FunctionSignature {
    fn is(&self, params: &[WasmScalar], results: &[WasmScalar]) -> bool {
        self.params == params && self.results == results
    }
}

/// What the module exports under [`frame::RESUME_EXPORT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeExport {
    Function(u32),
    /// A memory, table, global or tag under the dispatcher's name.
    NotFunction,
}

/// Direct call targets and table use in the dispatcher's own body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatcherCode {
    /// Direct `call`/`return_call` targets in instruction order.
    pub calls: Vec<u32>,
    /// Any indirect call, `call_ref` or table instruction.
    pub indirect: bool,
}

/// Facts the shared rules need, extracted from one portable module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameSurface {
    /// Signatures in function index order, imported functions first.
    pub functions: Vec<FunctionSignature>,
    pub imported_functions: usize,
    /// Every exported function index.
    pub exported_functions: BTreeSet<u32>,
    pub resume_export: Option<ResumeExport>,
    /// The body of the exported dispatcher function, when it is guest-defined.
    pub dispatcher: Option<DispatcherCode>,
}

/// The piece of the frame surface an earlier profile carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePiece {
    CatalogVersion,
    ResumeExport,
    /// Catalog version 3 with liveness layouts (v35 `frame_liveness`).
    LivenessCatalog,
}
impl std::fmt::Display for FramePiece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::CatalogVersion => "function catalog version 2 with frame layouts",
            Self::ResumeExport => "the exported varta_resume dispatcher",
            Self::LivenessCatalog => "function catalog version 3 with liveness frame layouts",
        })
    }
}

/// The frame catalog version a profile's frame layouts must use: liveness
/// layouts for a [`LanguageProfile::frame_liveness`] profile, cell layouts
/// for the other [`LanguageProfile::resumable_frames`] profiles.
#[must_use]
pub fn frame_catalog_version(profile: LanguageProfile) -> Option<u32> {
    if profile.frame_liveness() {
        Some(frame::FRAME_LIVENESS_CATALOG_VERSION)
    } else if profile.resumable_frames() {
        Some(frame::FRAME_CATALOG_VERSION)
    } else {
        None
    }
}

/// Why a layout does not agree with the module or its own catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMismatch {
    /// The resume code names no guest-defined function.
    CodeOutOfRange,
    /// The resume code names the dispatcher or a resume body, not a creator.
    CodeNotCreator,
    /// The creator does not return exactly one resume token.
    CreatorSignature,
    /// A resume body is exported; the scheduler enters only through the dispatcher.
    ExportedBody,
    /// The result binding holds a two-slot value or disagrees with the Return input class.
    ResultBinding,
    /// A yielded binding holds a two-slot value.
    YieldedBinding,
    /// The exception binding is not a union cell shared by every layout.
    ExceptionBinding,
}
impl std::fmt::Display for LayoutMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::CodeOutOfRange => "resume code names no guest-defined function",
            Self::CodeNotCreator => "resume code names the dispatcher or a resume body",
            Self::CreatorSignature => "creator must return exactly one i64 resume token",
            Self::ExportedBody => "resume body must not be exported",
            Self::ResultBinding => "result binding disagrees with the Return input class",
            Self::YieldedBinding => "yielded binding must be a single-slot value",
            Self::ExceptionBinding => "exception binding must be one union shared by every layout",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrameAdmissionError {
    #[error("the declared language profile does not admit resumable frames: {0}")]
    ProfileForbids(FramePiece),
    /// A frame profile carried the other frame layout: cell layouts under a
    /// liveness profile (v35) or liveness layouts under v33/v34.
    #[error(
        "the declared language profile requires frame catalog version {expected}, found version {found}"
    )]
    FrameLayoutVersion { expected: u32, found: u32 },
    /// A malformed catalog keeps the reader's own diagnostic.
    #[error(transparent)]
    Catalog(ValueAbiError),
    #[error("frame layouts require the exported varta_resume dispatcher")]
    MissingDispatcher,
    #[error("varta_resume is exported without frame layouts")]
    UnusedDispatcher,
    #[error("varta_resume must be a guest function (i64, i32, i64) -> i32")]
    DispatcherSignature,
    #[error("varta_resume must not use tables or indirect calls")]
    IndirectDispatch,
    #[error("varta_resume may call only guest-defined functions other than itself")]
    DispatcherTarget,
    #[error("varta_resume dispatches {dispatched} resume bodies for {declared} frame layouts")]
    LayoutCount { declared: usize, dispatched: usize },
    #[error("frame layout {layout} is inconsistent with the module: {mismatch}")]
    Layout {
        layout: usize,
        mismatch: LayoutMismatch,
    },
}

// The envelope the dispatcher and the collector check is self-consistent: the
// edge mask names only reference fields of a five-field frame, never the
// scalar control and status words, and the segment table fits one environment.
const _: () = {
    assert!((frame::FRAME_MASK as u32) >> frame::FRAME_FIELDS == 0);
    assert!(frame::FRAME_MASK & (1 << frame::CONTROL_FIELD | 1 << frame::STATUS_FIELD) == 0);
    assert!(frame::MAX_FRAME_SEGMENTS <= frame::SEGMENT_BINDINGS);
    assert!(frame::MAX_FRAME_BINDINGS == frame::SEGMENT_BINDINGS * frame::MAX_FRAME_SEGMENTS);
};

/// Read the function catalog version the profile admits. A
/// [`LanguageProfile::frame_liveness`] profile (v35) reads versions 1 and 3,
/// the other [`LanguageProfile::resumable_frames`] profiles read versions 1
/// and 2, and every other profile uses the version 1 reader. A well-formed
/// catalog of another frame version gets a typed refusal, so a module
/// relabelled to another profile fails before compilation.
pub fn read_catalog(
    bytes: &[u8],
    profile: LanguageProfile,
) -> Result<FunctionCatalog, FrameAdmissionError> {
    let accepted: &[u32] = match frame_catalog_version(profile) {
        Some(frame::FRAME_LIVENESS_CATALOG_VERSION) => &[
            super::CATALOG_VERSION,
            frame::FRAME_LIVENESS_CATALOG_VERSION,
        ],
        Some(_) => &[super::CATALOG_VERSION, frame::FRAME_CATALOG_VERSION],
        None => &[super::CATALOG_VERSION],
    };
    // Validated within the profile's structural limits (v38: the Wasm limits).
    FunctionCatalog::read_within(bytes, accepted, &profile.limits()).map_err(|error| {
        let other = FunctionCatalog::from_frame_bytes(bytes)
            .or_else(|_| FunctionCatalog::from_liveness_bytes(bytes))
            .map(|catalog| catalog.version);
        match (frame_catalog_version(profile), other) {
            (None, Ok(frame::FRAME_CATALOG_VERSION)) => {
                FrameAdmissionError::ProfileForbids(FramePiece::CatalogVersion)
            }
            (None, Ok(frame::FRAME_LIVENESS_CATALOG_VERSION)) => {
                FrameAdmissionError::ProfileForbids(FramePiece::LivenessCatalog)
            }
            (Some(expected), Ok(found)) if found != expected && found != super::CATALOG_VERSION => {
                FrameAdmissionError::FrameLayoutVersion { expected, found }
            }
            _ => FrameAdmissionError::Catalog(error),
        }
    })
}

/// Check the frame surface of one module against its profile and the catalog
/// returned by [`read_catalog`]. Nothing here instantiates or calls guest code.
pub fn admit(
    surface: &FrameSurface,
    profile: LanguageProfile,
    catalog: Option<&FunctionCatalog>,
) -> Result<(), FrameAdmissionError> {
    let layouts = catalog.map_or(&[][..], |catalog| &catalog.frames);
    if let (Some(expected), Some(catalog)) = (frame_catalog_version(profile), catalog)
        && !layouts.is_empty()
        && catalog.version != expected
    {
        return Err(FrameAdmissionError::FrameLayoutVersion {
            expected,
            found: catalog.version,
        });
    }
    if !profile.resumable_frames() {
        return if surface.resume_export.is_some() {
            Err(FrameAdmissionError::ProfileForbids(
                FramePiece::ResumeExport,
            ))
        } else if !layouts.is_empty() {
            Err(FrameAdmissionError::ProfileForbids(
                FramePiece::CatalogVersion,
            ))
        } else {
            Ok(())
        };
    }
    let (catalog, export) = match (catalog, surface.resume_export) {
        (_, None) if layouts.is_empty() => return Ok(()),
        (_, Some(_)) if layouts.is_empty() => return Err(FrameAdmissionError::UnusedDispatcher),
        (Some(catalog), Some(export)) => (catalog, export),
        _ => return Err(FrameAdmissionError::MissingDispatcher),
    };
    let ResumeExport::Function(dispatcher) = export else {
        return Err(FrameAdmissionError::DispatcherSignature);
    };
    let signature = |index: u32| surface.functions.get(index as usize);
    let defined = |index: u32| {
        (index as usize) >= surface.imported_functions && (index as usize) < surface.functions.len()
    };
    if !defined(dispatcher)
        || !signature(dispatcher).is_some_and(|s| s.is(&RESUME_PARAMS, &RESUME_RESULTS))
    {
        return Err(FrameAdmissionError::DispatcherSignature);
    }
    let code = surface
        .dispatcher
        .as_ref()
        .ok_or(FrameAdmissionError::DispatcherSignature)?;
    if code.indirect {
        return Err(FrameAdmissionError::IndirectDispatch);
    }
    let mut bodies = Vec::new();
    for &target in &code.calls {
        if !defined(target) || target == dispatcher {
            return Err(FrameAdmissionError::DispatcherTarget);
        }
        if signature(target).is_some_and(|s| s.is(&RESUME_PARAMS, &RESUME_BODY_RESULTS))
            && !bodies.contains(&target)
        {
            bodies.push(target);
        }
    }
    if bodies.len() != layouts.len() {
        return Err(FrameAdmissionError::LayoutCount {
            declared: layouts.len(),
            dispatched: bodies.len(),
        });
    }
    let exception = layouts[0].exception;
    for (id, (layout, &body)) in layouts.iter().zip(&bodies).enumerate() {
        let fail = |mismatch| FrameAdmissionError::Layout {
            layout: id,
            mismatch,
        };
        let creator = u32::try_from(surface.imported_functions)
            .ok()
            .and_then(|imported| imported.checked_add(layout.code))
            .filter(|creator| defined(*creator))
            .ok_or(fail(LayoutMismatch::CodeOutOfRange))?;
        if creator == dispatcher || bodies.contains(&creator) {
            return Err(fail(LayoutMismatch::CodeNotCreator));
        }
        if !signature(creator).is_some_and(|s| s.results == CREATOR_RESULTS) {
            return Err(fail(LayoutMismatch::CreatorSignature));
        }
        if surface.exported_functions.contains(&body) {
            return Err(fail(LayoutMismatch::ExportedBody));
        }
        layout_bindings(catalog, layout, exception).map_err(fail)?;
    }
    Ok(())
}

/// Binding classes the dispatcher and resume bodies rely on. Binding ids are
/// already range-checked by [`FunctionCatalog::validate`].
fn layout_bindings(
    catalog: &FunctionCatalog,
    layout: &FrameLayout,
    exception: u32,
) -> Result<(), LayoutMismatch> {
    let binding = |id: u32| &catalog.bindings[id as usize];
    let ty = |id: u32| &catalog.types[binding(id).ty as usize];
    if layout
        .states
        .iter()
        .any(|state| binding(state.yielded).result)
    {
        return Err(LayoutMismatch::YieldedBinding);
    }
    // A generator frame (v33) takes every input from guest code only.
    let expected = match layout.result {
        _ if layout.generator => Some(frame::FrameInput::Guest),
        None => Some(frame::FrameInput::Undefined),
        Some(id) if binding(id).result => return Err(LayoutMismatch::ResultBinding),
        Some(id) => match ty(id) {
            CaptureType::Float64 => Some(frame::FrameInput::Float64),
            CaptureType::Boolean => Some(frame::FrameInput::Boolean),
            CaptureType::Undefined => Some(frame::FrameInput::Undefined),
            _ => None,
        },
    };
    if layout.return_input != expected {
        return Err(LayoutMismatch::ResultBinding);
    }
    if layout.exception != exception
        || binding(exception).result
        || !matches!(
            ty(exception),
            CaptureType::PrimitiveUnion { .. } | CaptureType::ReferenceUnion { .. }
        )
    {
        return Err(LayoutMismatch::ExceptionBinding);
    }
    Ok(())
}
