//! V35 cross-stream integration: the records and Web streams' programs run on
//! the async stream's value layout 16 (value16 host readers, JSON schema 16)
//! and on the frames stream's liveness layouts, and promises live in
//! dictionaries and mixed records. Node-comparable logs were produced by
//! Node 24 for the same programs and are pinned here.
use super::*;
use varta_common::{HostOperation, language::LanguageProfile as P};

const V35: P = P::CheckedF64Utf16V35;

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V35)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn imports(bytes: &[u8]) -> Vec<HostOperation> {
    let module = wasmtime::Module::new(&VartaRuntime::new().engine, bytes).unwrap();
    module
        .imports()
        .map(|i| HostOperation::from_name(i.name()).unwrap())
        .collect()
}
fn custom_section<'a>(bytes: &'a [u8], name: &str) -> Option<&'a [u8]> {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::CustomSection(section) if section.name() == name => {
                Some(section.data())
            }
            _ => None,
        })
}

/// Programs of every v35 stream that use layout-bound values.
const CASES: [&str; 13] = [
    include_str!("web_cases/headers_basics.ts"),
    include_str!("web_cases/headers_values.ts"),
    include_str!("web_cases/abort_basics.ts"),
    include_str!("web_cases/abort_flow.ts"),
    include_str!("v35_cases/mixed.ts"),
    include_str!("v35_cases/dictionary_ops.ts"),
    include_str!("v35_cases/spread.ts"),
    include_str!("v35_cases/class_fields.ts"),
    include_str!("v35_cases/narrowing.ts"),
    include_str!("v35_cases/unknown_json.ts"),
    include_str!("v35_cases/promise_records.ts"),
    include_str!("async_method_cases/liveness.ts"),
    include_str!("promise_value_cases/containers.ts"),
];

/// No value15 reader (one `LanguageProfile::host_operation` would replace for
/// v35) is imported by a v35 module, and every module passes validation, so
/// its record descriptors, schemas and catalogs carry the value16 stamps the
/// value16 readers require.
#[test]
fn v35_stream_programs_import_only_value16_readers() {
    let runtime = VartaRuntime::new();
    let mut parses = 0;
    for source in CASES {
        let bytes = compile(source);
        for op in imports(&bytes) {
            assert_eq!(V35.host_operation(op), op, "{op:?}\n{source}");
            assert!(op.value13_counterpart().is_none(), "{op:?}");
            parses += usize::from(op == HostOperation::JsonParseV17);
        }
        runtime
            .validate_module(&bytes)
            .unwrap_or_else(|e| panic!("{e}\n{source}"));
        if let Some(catalog) = custom_section(&bytes, varta_common::function::CATALOG_SECTION)
            && let Ok(catalog) =
                varta_common::function::FunctionCatalog::from_liveness_bytes(catalog)
            && !catalog.frames.is_empty()
        {
            assert_eq!(
                catalog.version,
                varta_common::function::frame::FRAME_LIVENESS_CATALOG_VERSION
            );
        }
    }
    // `JSON.parse` to `unknown` reads schema 16 through the value16 parser.
    assert!(parses >= 2, "{parses}");
}

/// Promises held in a dictionary and in a mixed record are awaited, enumerated,
/// spread and passed to the combinators; the module also parses JSON to
/// `unknown`. The program carries liveness frames (catalog version 3).
#[test]
fn promises_inside_dictionaries_and_mixed_records_match_node() {
    let source = include_str!("v35_cases/promise_records.ts");
    let bytes = compile(source);
    let catalog = varta_common::function::FunctionCatalog::from_liveness_bytes(
        custom_section(&bytes, varta_common::function::CATALOG_SECTION).unwrap(),
    )
    .unwrap();
    assert_eq!(
        catalog.version,
        varta_common::function::frame::FRAME_LIVENESS_CATALOG_VERSION
    );
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}"))
            .lines,
        [
            "a=2",
            "b=4",
            "2,4",
            "jobs.x=10",
            "jobs.y=12",
            r#"[{"status":"fulfilled","value":2},{"status":"fulfilled","value":4},{"status":"fulfilled","value":6}]"#,
            r#"{"jobs":[1,2],"label":"p"}"#,
        ]
        .map(|line| format!("varta log: {line}"))
    );
}
