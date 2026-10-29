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
                saved: vec![],
            }],
            result: Some(0),
            return_input: Some(frame::FrameInput::Float64),
            exception: 1,
            generator: false,
            capacity: None,
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

#[test]
fn generator_layouts_admit_only_guest_inputs() {
    // A v33 generator row: every input is delivered by guest code.
    let generator = |_: &mut FrameSurface, c: &mut FunctionCatalog| {
        let layout = &mut c.frames[0];
        layout.generator = true;
        layout.states[0].input = frame::FrameInput::Guest;
        layout.return_input = Some(frame::FrameInput::Guest);
    };
    check(generator).unwrap();
    let error = check(|s, c| {
        generator(s, c);
        c.frames[0].return_input = Some(frame::FrameInput::Float64);
    })
    .unwrap_err();
    assert!(
        matches!(
            error,
            FrameAdmissionError::Layout {
                layout: 0,
                mismatch: LayoutMismatch::ResultBinding
            }
        ),
        "{error}"
    );
}

const V34: LanguageProfile = LanguageProfile::CheckedF64Utf16V34;
const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;
/// The fixture as a v35 liveness layout: state 1 saves binding 0 directly.
fn liveness_catalog() -> FunctionCatalog {
    let mut catalog = catalog();
    catalog.version = frame::FRAME_LIVENESS_CATALOG_VERSION;
    catalog.frames[0].capacity = Some(1);
    catalog.frames[0].states[0].saved = vec![FrameSlot {
        binding: 0,
        shared: false,
    }];
    catalog
}

#[test]
fn only_frame_liveness_profiles_read_and_admit_liveness_layouts() {
    use FrameAdmissionError as E;
    assert_eq!(
        frame_catalog_version(V35),
        Some(frame::FRAME_LIVENESS_CATALOG_VERSION)
    );
    assert_eq!(
        frame_catalog_version(V34),
        Some(frame::FRAME_CATALOG_VERSION)
    );
    assert_eq!(frame_catalog_version(V32), None);
    let liveness = liveness_catalog().to_bytes().unwrap();
    let cells = catalog().to_bytes().unwrap();
    let read = read_catalog(&liveness, V35).unwrap();
    assert_eq!(read.frames, liveness_catalog().frames);
    admit(&surface(), V35, Some(&read)).unwrap();
    // A v35 module relabelled as v33/v34, or a v34 module relabelled as v35,
    // fails on its frame layout version before compilation.
    for profile in [V33, V34] {
        assert!(matches!(
            read_catalog(&liveness, profile),
            Err(E::FrameLayoutVersion {
                expected: frame::FRAME_CATALOG_VERSION,
                found: frame::FRAME_LIVENESS_CATALOG_VERSION
            })
        ));
        assert!(matches!(
            admit(&surface(), profile, Some(&liveness_catalog())),
            Err(E::FrameLayoutVersion { .. })
        ));
    }
    assert!(matches!(
        read_catalog(&cells, V35),
        Err(E::FrameLayoutVersion {
            expected: frame::FRAME_LIVENESS_CATALOG_VERSION,
            found: frame::FRAME_CATALOG_VERSION
        })
    ));
    assert!(matches!(
        admit(&surface(), V35, Some(&catalog())),
        Err(E::FrameLayoutVersion { .. })
    ));
    for profile in crate::language::SUPPORTED_PROFILES
        .iter()
        .copied()
        .filter(|p| !p.resumable_frames())
    {
        assert!(matches!(
            read_catalog(&liveness, profile),
            Err(E::ProfileForbids(FramePiece::LivenessCatalog))
        ));
    }
    // Version 1 catalogs stay readable under v35.
    let mut plain = catalog();
    plain.version = super::CATALOG_VERSION;
    plain.frames.clear();
    read_catalog(&plain.to_bytes().unwrap(), V35).unwrap();
}

#[test]
fn tampered_liveness_layouts_fail_in_the_reader() {
    let cases: [fn(&mut FunctionCatalog); 7] = [
        // A version 3 row without its capacity, or one above the frame budget.
        |c| c.frames[0].capacity = None,
        |c| c.frames[0].capacity = Some(frame::MAX_FRAME_BINDINGS as u32 + 1),
        // Saved entries exceed the capacity: a second slot, or a two-slot binding.
        |c| {
            let slot = c.frames[0].states[0].saved[0].clone();
            c.frames[0].states[0].saved.push(slot);
        },
        |c| c.bindings[0].result = true,
        // The creator's state-0 slots must fit as well.
        |c| c.frames[0].slots = vec![c.frames[0].slots[0].clone(); 2],
        // Saved binding ids stay in range.
        |c| c.frames[0].states[0].saved[0].binding = 2,
        // Liveness fields never appear in a version 2 catalog.
        |c| c.version = frame::FRAME_CATALOG_VERSION,
    ];
    for edit in cases {
        let mut catalog = liveness_catalog();
        edit(&mut catalog);
        assert!(catalog.to_bytes().is_err(), "{catalog:?}");
        let bytes = serde_json::to_vec(&catalog).unwrap();
        assert!(
            matches!(
                read_catalog(&bytes, V35),
                Err(FrameAdmissionError::Catalog(_)
                    | FrameAdmissionError::FrameLayoutVersion { .. })
            ),
            "{catalog:?}"
        );
    }
    // A shared two-slot binding saves one entry: its closure cell.
    let mut shared = liveness_catalog();
    shared.bindings[0].result = true;
    shared.frames[0].states[0].saved[0].shared = true;
    shared.frames[0].slots[0].shared = true;
    shared.to_bytes().unwrap();
    // A cell row with a stray saved slot fails in the version 2 reader too.
    let mut stray = catalog();
    stray.frames[0].states[0].saved = liveness_catalog().frames[0].states[0].saved.clone();
    assert!(stray.to_bytes().is_err());
}
