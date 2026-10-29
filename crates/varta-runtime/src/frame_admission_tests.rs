//! K4.4 step 1 runtime admission: hook-produced v33 frame modules pass
//! preflight and run, every tampered or earlier-profile frame surface fails
//! with a typed error before output or exit, and frames keep exact roots
//! across a forced collection inside a runtime-owned store.
use super::*;
use crate::contract_tests::rewrite;
use varta_common::function::{
    FunctionCatalog,
    admission::{FrameAdmissionError, FramePiece, LayoutMismatch},
    frame::{self, Completion, FrameView, Outcome, ResumeToken},
};
use varta_common::language::{self, LanguageContract, LanguageProfile, SCHEMA_VERSION};
use varta_compiler::suspension_test_hook as hook;
use wasmparser::{ExternalKind, Parser, Payload};

const V33: LanguageProfile = LanguageProfile::CheckedF64Utf16V33;
const V32: LanguageProfile = LanguageProfile::CheckedF64Utf16V32;
const LOG: &str = "varta log: frames admitted";
const EXIT: i32 = 33;
/// Layout 0 retains a string, an array and an Error across its suspension
/// point; layout 1 churns the heap so a collection can reuse free blocks.
const SOURCE: &str = r#"
function task(): number {
  const kept = "ab".repeat(3);
  const list = [1, 2, 3];
  const error = new Error("kept " + kept);
  let junk = "";
  for (let i = 0; i < 32; i = i + 1) { junk = junk + "garbage!"; }
  const got = __varta_suspend(kept);
  let more = "";
  for (let i = 0; i < 32; i = i + 1) { more = more + "realloc!"; }
  if (got > 0) { throw error; }
  return (kept === "ababab" ? 1 : 0) + list[2] * 10 + (error.message === "kept ababab" ? 100 : 0);
}
function churn(): number {
  let junk = "";
  for (let i = 0; i < 64; i = i + 1) { junk = junk + "overwrite"; }
  return __varta_suspend(junk.length);
}
export function main(): void {
  console.log("frames admitted");
  varta.process.exit(33);
}
"#;

fn compile(profile: LanguageProfile) -> Vec<u8> {
    hook::compile_with_profile(SOURCE, profile).unwrap()
}
fn grants() -> [CapabilityGrant; 1] {
    [CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }]
}
fn custom(data: &[u8], name: &str) -> bool {
    // Fixture names are short, so their encoded name length occupies one byte.
    data.first() == Some(&(name.len() as u8))
        && data.get(1..1 + name.len()) == Some(name.as_bytes())
}
fn section<'a>(bytes: &'a [u8], name: &str) -> &'a [u8] {
    Parser::new(0)
        .parse_all(bytes)
        .find_map(|payload| match payload.unwrap() {
            Payload::CustomSection(section) if section.name() == name => Some(section.data()),
            _ => None,
        })
        .unwrap()
}
/// Replace one custom section's payload, or drop it with `None`.
fn custom_section(bytes: &[u8], name: &str, data: Option<&[u8]>) -> Vec<u8> {
    rewrite(bytes, |id, section| {
        if id == 0 && custom(section, name) {
            data.map_or_else(Vec::new, |data| {
                let mut section = section[..1 + name.len()].to_vec();
                section.extend(data);
                vec![section]
            })
        } else {
            vec![section.to_vec()]
        }
    })
}
fn frame_catalog(bytes: &[u8]) -> FunctionCatalog {
    FunctionCatalog::from_frame_bytes(section(bytes, varta_common::function::CATALOG_SECTION))
        .unwrap()
}
/// Rewrite the catalog without re-validating it, as a hostile producer could.
fn edit_catalog(bytes: &[u8], edit: impl FnOnce(&mut FunctionCatalog)) -> Vec<u8> {
    let mut catalog = frame_catalog(bytes);
    edit(&mut catalog);
    let data = serde_json::to_vec(&catalog).unwrap();
    custom_section(bytes, varta_common::function::CATALOG_SECTION, Some(&data))
}
/// Declare another profile together with its own host ABI section.
fn declare(bytes: &[u8], profile: LanguageProfile) -> Vec<u8> {
    let contract = LanguageContract {
        schema_version: SCHEMA_VERSION,
        profile,
    }
    .to_bytes()
    .unwrap();
    let bytes = custom_section(bytes, language::SECTION, Some(&contract));
    custom_section(
        bytes.as_slice(),
        abi::SECTION,
        Some(&profile.host_abi().to_le_bytes()),
    )
}
fn leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        out.push(byte | if value != 0 { 0x80 } else { 0 });
        if value == 0 {
            break;
        }
    }
}
/// Rebuild the export section; `edit` may rename, retarget or drop a function export.
fn edit_exports(bytes: &[u8], mut edit: impl FnMut(&str, u32) -> Option<(String, u32)>) -> Vec<u8> {
    const EXPORT_SECTION: u8 = 7;
    rewrite(bytes, |id, data| {
        if id != EXPORT_SECTION {
            return vec![data.to_vec()];
        }
        let reader =
            wasmparser::ExportSectionReader::new(wasmparser::BinaryReader::new(data, 0)).unwrap();
        let mut entries = Vec::new();
        for export in reader {
            let export = export.unwrap();
            let kind = export.kind;
            let entry = if kind == ExternalKind::Func {
                edit(export.name, export.index)
            } else {
                Some((export.name.to_owned(), export.index))
            };
            if let Some(entry) = entry {
                entries.push((entry, kind));
            }
        }
        let mut out = Vec::new();
        leb(&mut out, entries.len() as u32);
        for ((name, index), kind) in entries {
            leb(&mut out, name.len() as u32);
            out.extend(name.as_bytes());
            out.push(match kind {
                ExternalKind::Func => 0,
                ExternalKind::Table => 1,
                ExternalKind::Memory => 2,
                ExternalKind::Global => 3,
                ExternalKind::Tag => 4,
            });
            leb(&mut out, index);
        }
        vec![out]
    })
}
fn export_index(bytes: &[u8], name: &str) -> u32 {
    Parser::new(0)
        .parse_all(bytes)
        .find_map(|payload| match payload.unwrap() {
            Payload::ExportSection(reader) => reader
                .into_iter()
                .map(Result::unwrap)
                .find(|export| export.name == name)
                .map(|export| export.index),
            _ => None,
        })
        .unwrap()
}
fn imported_functions(bytes: &[u8]) -> u32 {
    Parser::new(0)
        .parse_all(bytes)
        .filter_map(|payload| match payload.unwrap() {
            Payload::ImportSection(reader) => Some(reader.into_iter().count() as u32),
            _ => None,
        })
        .sum()
}
/// Distinct direct call targets of the dispatcher with the resume-body signature.
fn resume_bodies(bytes: &[u8]) -> Vec<u32> {
    let dispatcher = export_index(bytes, frame::RESUME_EXPORT);
    let mut defined = imported_functions(bytes);
    let mut calls = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if defined == dispatcher {
                let mut reader = body.get_operators_reader().unwrap();
                while !reader.eof() {
                    if let wasmparser::Operator::Call { function_index } = reader.read().unwrap()
                        && !calls.contains(&function_index)
                    {
                        calls.push(function_index);
                    }
                }
            }
            defined += 1;
        }
    }
    // The allocator is the dispatcher's only other direct target; it is the
    // last one called, after every layout's resume body.
    calls.pop();
    calls
}
fn rejected(bytes: &[u8]) -> FrameAdmissionError {
    let runtime = VartaRuntime::new();
    assert!(runtime.inspect_module(bytes).is_err());
    // No log line or exit code is observable: the run fails before instantiation.
    match runtime.run_wasm_bytes(bytes, &grants()) {
        Err(RuntimeError::Frames(error)) => error,
        Err(other) => panic!("expected a frame admission error, got {other}"),
        Ok(output) => panic!("unexpected run: {:?}", output.lines),
    }
}

#[test]
fn hook_produced_v33_frame_modules_validate_and_run_without_a_scheduler() {
    let bytes = compile(V33);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let catalog = frame_catalog(&bytes);
    assert_eq!(catalog.version, frame::FRAME_CATALOG_VERSION);
    assert_eq!(catalog.frames.len(), 2);
    let runtime = VartaRuntime::new();
    let prepared = runtime.prepare_module(&bytes).unwrap();
    let retained = prepared.function_catalog().unwrap().as_catalog();
    assert_eq!(retained.frames, catalog.frames);
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert_eq!(inspection.language.profile, Some(V33));
    assert!(inspection.exports.iter().any(|e| e == frame::RESUME_EXPORT));
    // The runtime runs only the entry point; it never calls the dispatcher.
    let run = runtime.run_wasm_bytes(&bytes, &grants()).unwrap();
    assert_eq!(
        (run.lines.as_slice(), run.exit_code),
        (&[LOG.to_owned()][..], EXIT)
    );
    // The same source without suspension points keeps the plain v33 surface.
    let plain = varta_compiler::compile_typescript_with_profile(
        "export function main(): void { console.log(\"frames admitted\"); }",
        V33,
    )
    .unwrap();
    let plain_module = runtime.validate_module(&plain).unwrap();
    assert!(plain_module.get_export(frame::RESUME_EXPORT).is_none());
}

#[test]
fn earlier_profiles_reject_every_piece_of_the_frame_surface() {
    // A real v32 frame module from the hook: catalog version 2 is refused by
    // the version 1 reader with a typed profile error.
    let older = compile(V32);
    assert!(matches!(
        rejected(&older),
        FrameAdmissionError::ProfileForbids(FramePiece::CatalogVersion)
    ));
    // V33 bytes relabelled as v32 (profile and host ABI rewritten together).
    let bytes = compile(V33);
    assert!(matches!(
        rejected(&declare(&bytes, V32)),
        FrameAdmissionError::ProfileForbids(FramePiece::CatalogVersion)
    ));
    // A version 1 catalog without layouts still cannot carry the dispatcher.
    let v1 = edit_catalog(&older, |catalog| {
        catalog.version = varta_common::function::CATALOG_VERSION;
        catalog.frames.clear();
    });
    assert!(matches!(
        rejected(&v1),
        FrameAdmissionError::ProfileForbids(FramePiece::ResumeExport)
    ));
    // The version 1 reader itself never accepts version 2.
    let data = section(&bytes, varta_common::function::CATALOG_SECTION);
    assert!(FunctionCatalog::from_bytes(data).is_err());
    assert!(varta_common::function::VerifiedFunctionCatalog::from_bytes(data).is_err());
}

#[test]
fn v33_dispatcher_export_must_be_present_exact_and_closed() {
    let bytes = compile(V33);
    let missing = edit_exports(&bytes, |name, index| {
        (name != frame::RESUME_EXPORT).then(|| (name.to_owned(), index))
    });
    assert!(matches!(
        rejected(&missing),
        FrameAdmissionError::MissingDispatcher
    ));
    // Retarget the dispatcher name at a creator: `(…) -> i64`, not the exact signature.
    let creator = export_index(&bytes, &format!("{}0", hook::CREATE_PREFIX));
    let wrong = edit_exports(&bytes, |name, index| {
        Some((
            name.to_owned(),
            if name == frame::RESUME_EXPORT {
                creator
            } else {
                index
            },
        ))
    });
    assert!(matches!(
        rejected(&wrong),
        FrameAdmissionError::DispatcherSignature
    ));
    // A version 1 catalog under v33 has no layouts, so the dispatcher is unused.
    let unused = edit_catalog(&bytes, |catalog| {
        catalog.version = varta_common::function::CATALOG_VERSION;
        catalog.frames.clear();
    });
    assert!(matches!(
        rejected(&unused),
        FrameAdmissionError::UnusedDispatcher
    ));
    // Resume bodies are entered only through the dispatcher.
    let body = resume_bodies(&bytes)[0];
    let leaked = edit_exports(&bytes, |name, index| {
        Some(if name == hook::COLLECT {
            ("leaked_body".to_owned(), body)
        } else {
            (name.to_owned(), index)
        })
    });
    assert!(matches!(
        rejected(&leaked),
        FrameAdmissionError::Layout {
            layout: 0,
            mismatch: LayoutMismatch::ExportedBody
        }
    ));
}

#[test]
fn v33_frame_layouts_must_agree_with_the_module() {
    let bytes = compile(V33);
    let imported = imported_functions(&bytes);
    let dispatcher = export_index(&bytes, frame::RESUME_EXPORT);
    let main = export_index(&bytes, "_start");
    let cases: [(
        Box<dyn Fn(&mut FunctionCatalog)>,
        fn(&FrameAdmissionError) -> bool,
    ); 7] = [
        (
            // One more layout than the dispatcher resumes.
            Box::new(|c| {
                let mut extra = c.frames[0].clone();
                extra.code = c.frames.iter().map(|f| f.code).max().unwrap() + 1;
                c.frames.push(extra);
            }),
            |e| {
                matches!(
                    e,
                    FrameAdmissionError::LayoutCount {
                        declared: 3,
                        dispatched: 2
                    }
                )
            },
        ),
        (
            // Out of range: inside the catalog bound, beyond the module.
            Box::new(|c| c.frames[1].code = abi::MAX_USER_FUNCTIONS as u32 - 1),
            |e| {
                matches!(
                    e,
                    FrameAdmissionError::Layout {
                        layout: 1,
                        mismatch: LayoutMismatch::CodeOutOfRange
                    }
                )
            },
        ),
        (
            // A resume code naming the dispatcher instead of a creator.
            Box::new(move |c| c.frames[0].code = dispatcher - imported),
            |e| {
                matches!(
                    e,
                    FrameAdmissionError::Layout {
                        layout: 0,
                        mismatch: LayoutMismatch::CodeNotCreator
                    }
                )
            },
        ),
        (
            // A resume code naming `main`, whose result is not a resume token.
            Box::new(move |c| c.frames[0].code = main - imported),
            |e| {
                matches!(
                    e,
                    FrameAdmissionError::Layout {
                        layout: 0,
                        mismatch: LayoutMismatch::CreatorSignature
                    }
                )
            },
        ),
        (
            // Return input class that disagrees with the number result.
            Box::new(|c| c.frames[0].return_input = Some(frame::FrameInput::Boolean)),
            |e| {
                matches!(
                    e,
                    FrameAdmissionError::Layout {
                        layout: 0,
                        mismatch: LayoutMismatch::ResultBinding
                    }
                )
            },
        ),
        (
            // Exception completions are one union cell shared by every layout.
            Box::new(|c| c.frames[1].exception = c.frames[1].slots[0].binding),
            |e| {
                matches!(
                    e,
                    FrameAdmissionError::Layout {
                        layout: 1,
                        mismatch: LayoutMismatch::ExceptionBinding
                    }
                )
            },
        ),
        (
            // Unknown binding ids fail the catalog reader itself.
            Box::new(|c| c.frames[0].slots[0].binding = c.bindings.len() as u32),
            |e| matches!(e, FrameAdmissionError::Catalog(_)),
        ),
    ];
    for (edit, expected) in cases {
        let error = rejected(&edit_catalog(&bytes, edit));
        assert!(expected(&error), "{error}");
    }
    // A duplicate catalog section is refused before either copy is used.
    let duplicate = rewrite(&bytes, |id, data| {
        if id == 0 && custom(data, varta_common::function::CATALOG_SECTION) {
            vec![data.to_vec(), data.to_vec()]
        } else {
            vec![data.to_vec()]
        }
    });
    assert!(VartaRuntime::new().validate_module(&duplicate).is_err());
}

#[test]
fn suspended_frames_keep_exact_roots_across_a_forced_collection_in_a_runtime_store() {
    let bytes = compile(V33);
    let runtime = VartaRuntime::new();
    let prepared = runtime.prepare_module(&bytes).unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &grants(), None).unwrap();
    let instance = execution.instantiate_prepared(&prepared).unwrap();
    let store = &mut execution.store;
    let create = |store: &mut wasmtime::Store<RuntimeContext>, layout: u32| {
        instance
            .get_typed_func::<(), i64>(&mut *store, &format!("{}{layout}", hook::CREATE_PREFIX))
            .unwrap()
            .call(&mut *store, ())
            .unwrap()
    };
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(&mut *store, frame::RESUME_EXPORT)
        .unwrap();
    let collect = instance
        .get_typed_func::<(), ()>(&mut *store, hook::COLLECT)
        .unwrap();
    let memory = instance.get_memory(&mut *store, "memory").unwrap();
    let view = |store: &wasmtime::Store<RuntimeContext>, token: i64| {
        FrameView::read_envelope(memory.data(store), ResumeToken::from_bits(token).frame).unwrap()
    };
    let next = |store: &mut wasmtime::Store<RuntimeContext>, token: i64| {
        Outcome::from_code(
            resume
                .call(
                    &mut *store,
                    (token, Completion::Normal as i32, 0f64.to_bits() as i64),
                )
                .unwrap(),
        )
        .unwrap()
    };
    let text = |store: &wasmtime::Store<RuntimeContext>, handle: u64| {
        let (ptr, len) = (handle as u32 as usize, (handle >> u32::BITS) as usize);
        let units: Vec<u16> = memory.data(store)[ptr..ptr + len]
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).unwrap()
    };
    let token = create(store, 0);
    assert_eq!(next(store, token), Outcome::Suspended);
    let frame = ResumeToken::from_bits(token).frame;
    let token = view(store, token).token(frame).to_bits();
    let yielded = view(store, token).completion.unwrap().value;
    // Only the suspended frame retains these values: reclaim, reuse, reclaim.
    collect.call(&mut *store, ()).unwrap();
    let other = create(store, 1);
    assert_eq!(next(store, other), Outcome::Suspended);
    collect.call(&mut *store, ()).unwrap();
    assert_eq!(text(store, yielded), "ababab");
    assert_eq!(next(store, token), Outcome::Returned);
    let result = f64::from_bits(view(store, token).completion.unwrap().value);
    assert_eq!(result, 131.0);
    assert_eq!(view(store, token).status, frame::STATUS_COMPLETED);
    // The scheduler-facing channel is empty again; no exception was left pending.
    exceptions::finish(&instance, store).unwrap();
}
