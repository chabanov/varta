use super::admission::*;
use super::{
    CaptureBinding, CaptureType, FrameLayout, FrameSlot, FrameState, FunctionCatalog, frame,
};
use crate::language::LanguageProfile;
use crate::value;
use WasmScalar::{F64, I32, I64};

const V33: LanguageProfile = LanguageProfile::CheckedF64Utf16V33;
const V32: LanguageProfile = LanguageProfile::CheckedF64Utf16V32;
const IMPORT: u32 = 0;
const CREATOR: u32 = 1;
const BODY: u32 = 2;
const ALLOCATE: u32 = 3;
const DISPATCHER: u32 = 4;

fn signature(params: &[WasmScalar], results: &[WasmScalar]) -> FunctionSignature {
    FunctionSignature {
        params: params.to_vec(),
        results: results.to_vec(),
    }
}
fn catalog() -> FunctionCatalog {
    FunctionCatalog {
        version: frame::FRAME_CATALOG_VERSION,
        value_version: value::FUNCTION_VALUE_VERSION,
        types: vec![
            CaptureType::Float64,
            CaptureType::Undefined,
            CaptureType::PrimitiveUnion {
                members: vec![0, 1],
            },
        ],
        bindings: vec![
            CaptureBinding {
                ty: 0,
                result: false,
            },
            CaptureBinding {
                ty: 2,
                result: false,
            },
        ],
        functions: vec![],
        frames: vec![FrameLayout {
            code: CREATOR - 1,
            slots: vec![FrameSlot {
                binding: 0,
                shared: false,
            }],
            states: vec![FrameState {
                input: frame::FrameInput::Float64,
                yielded: 0,
            }],
            result: Some(0),
            return_input: Some(frame::FrameInput::Float64),
            exception: 1,
        }],
    }
}
/// One import, a creator, a resume body, the allocator and the dispatcher.
fn surface() -> FrameSurface {
    FrameSurface {
        functions: vec![
            signature(&[F64], &[]),
            signature(&[F64], &CREATOR_RESULTS),
            signature(&RESUME_PARAMS, &RESUME_BODY_RESULTS),
            signature(&[I32], &[I32]),
            signature(&RESUME_PARAMS, &RESUME_RESULTS),
        ],
        imported_functions: 1,
        exported_functions: [DISPATCHER].into(),
        resume_export: Some(ResumeExport::Function(DISPATCHER)),
        dispatcher: Some(DispatcherCode {
            calls: vec![BODY, ALLOCATE],
            indirect: false,
        }),
    }
}
/// One fixture edit applied to the surface and its catalog.
type Edit = fn(&mut FrameSurface, &mut FunctionCatalog);
type Expect = fn(&FrameAdmissionError) -> bool;
fn check(
    edit: impl FnOnce(&mut FrameSurface, &mut FunctionCatalog),
) -> Result<(), FrameAdmissionError> {
    let (mut surface, mut catalog) = (surface(), catalog());
    edit(&mut surface, &mut catalog);
    admit(&surface, V33, Some(&catalog))
}

#[test]
fn resumable_frame_profiles_alone_read_and_admit_the_frame_surface() {
    let bytes = catalog().to_bytes().unwrap();
    let read = read_catalog(&bytes, V33).unwrap();
    assert_eq!(read.frames, catalog().frames);
    admit(&surface(), V33, Some(&read)).unwrap();
    for profile in crate::language::SUPPORTED_PROFILES
        .iter()
        .copied()
        .filter(|p| !p.resumable_frames())
    {
        assert!(matches!(
            read_catalog(&bytes, profile),
            Err(FrameAdmissionError::ProfileForbids(
                FramePiece::CatalogVersion
            ))
        ));
        assert!(matches!(
            admit(&surface(), profile, None),
            Err(FrameAdmissionError::ProfileForbids(
                FramePiece::ResumeExport
            ))
        ));
    }
    let mut plain = surface();
    plain.resume_export = None;
    admit(&plain, V32, None).unwrap();
    admit(&plain, V33, None).unwrap();
    assert!(matches!(
        admit(&plain, V32, Some(&catalog())),
        Err(FrameAdmissionError::ProfileForbids(
            FramePiece::CatalogVersion
        ))
    ));
    assert!(matches!(
        read_catalog(b"{}", V33),
        Err(FrameAdmissionError::Catalog(_))
    ));
    assert!(matches!(
        read_catalog(b"{}", V32),
        Err(FrameAdmissionError::Catalog(_))
    ));
}

#[test]
fn dispatcher_presence_signature_and_closed_calls_are_exact() {
    use FrameAdmissionError as E;
    let cases: [(Edit, Expect); 9] = [
        (
            |s, _| s.resume_export = None,
            |e| matches!(e, E::MissingDispatcher),
        ),
        (
            |_, c| c.frames.clear(),
            |e| matches!(e, E::UnusedDispatcher),
        ),
        (
            |s, _| s.resume_export = Some(ResumeExport::NotFunction),
            |e| matches!(e, E::DispatcherSignature),
        ),
        (
            |s, _| s.resume_export = Some(ResumeExport::Function(IMPORT)),
            |e| matches!(e, E::DispatcherSignature),
        ),
        (
            |s, _| s.functions[DISPATCHER as usize].results = vec![I64],
            |e| matches!(e, E::DispatcherSignature),
        ),
        (
            |s, _| s.dispatcher = None,
            |e| matches!(e, E::DispatcherSignature),
        ),
        (
            |s, _| s.dispatcher.as_mut().unwrap().indirect = true,
            |e| matches!(e, E::IndirectDispatch),
        ),
        (
            |s, _| s.dispatcher.as_mut().unwrap().calls.push(IMPORT),
            |e| matches!(e, E::DispatcherTarget),
        ),
        (
            |s, _| s.dispatcher.as_mut().unwrap().calls.push(DISPATCHER),
            |e| matches!(e, E::DispatcherTarget),
        ),
    ];
    for (edit, expected) in cases {
        let error = check(edit).unwrap_err();
        assert!(expected(&error), "{error}");
    }
}

#[test]
fn every_layout_agrees_with_its_creator_body_and_bindings() {
    use LayoutMismatch as M;
    let cases: [(Edit, Option<M>); 9] = [
        (
            |s, _| s.dispatcher.as_mut().unwrap().calls = vec![ALLOCATE],
            None,
        ),
        (|_, c| c.frames[0].code = 64, Some(M::CodeOutOfRange)),
        (|_, c| c.frames[0].code = BODY - 1, Some(M::CodeNotCreator)),
        (
            |_, c| c.frames[0].code = DISPATCHER - 1,
            Some(M::CodeNotCreator),
        ),
        (
            |_, c| c.frames[0].code = ALLOCATE - 1,
            Some(M::CreatorSignature),
        ),
        (
            |s, _| _ = s.exported_functions.insert(BODY),
            Some(M::ExportedBody),
        ),
        (
            |_, c| c.frames[0].return_input = None,
            Some(M::ResultBinding),
        ),
        (|_, c| c.bindings[0].result = true, Some(M::YieldedBinding)),
        (|_, c| c.frames[0].exception = 0, Some(M::ExceptionBinding)),
    ];
    for (edit, expected) in cases {
        let error = check(edit).unwrap_err();
        match expected {
            None => assert!(matches!(
                error,
                FrameAdmissionError::LayoutCount {
                    declared: 1,
                    dispatched: 0
                }
            )),
            Some(mismatch) => assert!(
                matches!(error, FrameAdmissionError::Layout { layout: 0, mismatch: m } if m == mismatch),
                "{error}"
            ),
        }
    }
    // A void result admits exactly an undefined Return input.
    check(|_, c| {
        c.frames[0].result = None;
        c.frames[0].return_input = Some(frame::FrameInput::Undefined);
    })
    .unwrap();
}
