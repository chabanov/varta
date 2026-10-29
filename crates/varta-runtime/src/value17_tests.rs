//! Value layout 17 (v36 `function_fields`): every value-reading host operation,
//! JSON schema 17 and exception channel v8 read value17 layouts with exactly
//! the value16 behaviour for every value16 tag, while v35 keeps value16, its
//! operations and host ABI 31. Every v35 case compiled under v36 must log
//! exactly what it logs under v35 (whose logs are pinned against Node 24 in
//! their own tests).
use super::*;
use varta_common::{HostOperation as H, language::LanguageProfile as P, value};

const V35: P = P::CheckedF64Utf16V35;
const V36: P = P::CheckedF64Utf16V36;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn outcome(source: &str, profile: P) -> (Vec<String>, Option<String>) {
    match VartaRuntime::new().run_wasm_bytes(&compile(source, profile), &[]) {
        Ok(run) => (run.lines, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}
fn imports(bytes: &[u8]) -> Vec<H> {
    let module = wasmtime::Module::new(&VartaRuntime::new().engine, bytes).unwrap();
    module
        .imports()
        .map(|i| H::from_name(i.name()).unwrap())
        .collect()
}
/// Rewrite every occurrence of an equal-length name (imports, globals).
fn rename(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let mut bytes = bytes.to_vec();
    let mut found = false;
    let mut at = 0;
    while let Some(offset) = bytes[at..]
        .windows(from.len())
        .position(|w| w == from.as_bytes())
    {
        let start = at + offset;
        bytes[start..start + to.len()].copy_from_slice(to.as_bytes());
        at = start + to.len();
        found = true;
    }
    assert!(found, "{from}");
    bytes
}

#[test]
fn value17_cases_match_value16() {
    for source in [
        include_str!("value15_cases/values.ts"),
        include_str!("value15_cases/frames.ts"),
        include_str!("value15_cases/checked.ts"),
        include_str!("value15_cases/dictionary.ts"),
        include_str!("value15_cases/dictionary_json.ts"),
        include_str!("value15_cases/recursive.ts"),
        include_str!("value15_cases/wide.ts"),
        include_str!("value15_cases/wide_json.ts"),
        include_str!("v34_cross_cases/caught_unknown.ts"),
        include_str!("v34_cross_cases/json_tree.ts"),
        include_str!("unknown_cases/assertions.ts"),
        include_str!("unknown_cases/guards.ts"),
        include_str!("generator_cases/values.ts"),
        include_str!("promise_cases/values.ts"),
        include_str!("promise_cases/rejections.ts"),
        include_str!("promise_value_cases/combinators.ts"),
        include_str!("promise_value_cases/containers.ts"),
        include_str!("promise_value_cases/logging.ts"),
        include_str!("async_method_cases/closures.ts"),
        include_str!("v35_cases/class_fields.ts"),
        include_str!("v35_cases/mixed.ts"),
        include_str!("v35_cases/narrowing.ts"),
        include_str!("v35_cases/promise_records.ts"),
        include_str!("v35_cases/unknown_json.ts"),
        include_str!("web_cases/abort_basics.ts"),
        include_str!("web_cases/headers_values.ts"),
    ] {
        let old = outcome(source, V35);
        assert!(!old.0.is_empty() || old.1.is_some(), "{source}");
        assert_eq!(outcome(source, V36), old, "{source}");
    }
}

#[test]
fn v36_modules_import_only_value17_readers_and_channel_v8() {
    let source = "class Failure extends Error{constructor(m:string){super(m);}}\
        function f(v:number|string|boolean|null|undefined|bigint):string{\
        console.log(v);console.log(typeof v);console.log(v===1);console.log(v==null);\
        return String(v)+String(Number(v===1n?1:v===undefined?2:3))+String(BigInt(v===null?1:2));}\
        export function main():void{\
        const xs:(number[]|string)[]=[[1],'a'];console.log(Array.isArray(xs[0]));\
        try{throw new Failure('x');}catch(e){console.log(e instanceof RangeError);}\
        const t=JSON.stringify({a:1});const p=varta.json.parse<{a:number}>(t);\
        console.log(f(p.a)+(xs[1]?'y':'n'));}";
    let new = imports(&compile(source, V36));
    let old = imports(&compile(source, V35));
    // Every value16 reader the v35 module uses has its value17 replacement,
    // and no other import changes.
    for op in &old {
        if let Some(replacement) = new.iter().find(|n| n.value16_counterpart() == Some(*op)) {
            assert!(!new.contains(op), "{op:?}");
            assert_eq!(replacement.minimum_abi(), abi::TIMERS_VERSION);
        } else {
            assert!(new.contains(op), "{op:?}");
        }
    }
    assert!(
        new.iter()
            .filter(|op| op.value16_counterpart().is_some())
            .count()
            >= 8,
        "{new:?}"
    );
    assert!(old.iter().all(|op| op.value16_counterpart().is_none()));
    assert!(new.iter().all(|op| op.value15_counterpart().is_none()));
    let run = |profile| {
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source, profile), &[])
            .unwrap()
            .lines
    };
    assert_eq!(run(V36), run(V35));
    let module = VartaRuntime::new()
        .validate_module(&compile(source, V36))
        .unwrap();
    assert!(
        module
            .exports()
            .any(|e| e.name() == varta_common::exceptions::FUNCTION_FIELD_VERSION)
    );
    assert!(
        !module
            .exports()
            .any(|e| e.name() == varta_common::exceptions::PROMISE_VERSION)
    );
}

#[test]
fn value16_and_value17_readers_reject_each_others_profiles() {
    let source = "export function main():void{const v:number|string='s';console.log(v);\
        console.log(JSON.stringify({a:[1]}));\
        console.log(varta.json.parse<{a:number}>('{\"a\":1}').a);}";
    let runtime = VartaRuntime::new();
    let new = compile(source, V36);
    let old = compile(source, V35);
    assert!(runtime.validate_module(&new).is_ok());
    assert!(runtime.validate_module(&old).is_ok());
    for (from, to) in [
        ("value_log_v11", "value_log_v10"),
        ("json_stringify_v16", "json_stringify_v15"),
        ("json_parse_v18", "json_parse_v17"),
    ] {
        // A v36 module may not read value17 graphs through value16 readers ...
        let downgraded = rename(&new, from, to);
        assert!(runtime.validate_module(&downgraded).is_err(), "{to}");
        // ... and a v35 module may not reach the value17 readers.
        let upgraded = rename(&old, to, from);
        assert!(runtime.validate_module(&upgraded).is_err(), "{from}");
    }
    // The exception channel names its value layout; a swapped channel fails
    // before any guest effect.
    let swapped = rename(
        &new,
        varta_common::exceptions::FUNCTION_FIELD_VERSION,
        varta_common::exceptions::PROMISE_VERSION,
    );
    let error = runtime.run_wasm_bytes(&swapped, &[]).unwrap_err();
    assert!(error.to_string().contains("exception"), "{error}");
}

#[test]
fn value17_readers_reject_value16_stamped_descriptors() {
    let source = "export function main():void{console.log(JSON.stringify({k:1}));}";
    let bytes = compile(source, V36);
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        [r#"varta log: {"k":1}"#]
    );
    let mut header = value::FUNCTION_FIELD_VALUE_VERSION.to_le_bytes().to_vec();
    header.extend(1u32.to_le_bytes());
    let at = bytes
        .windows(header.len())
        .position(|w| w == header.as_slice())
        .expect("value17 descriptor in the data section");
    let mut stale = bytes.clone();
    stale[at..at + 4].copy_from_slice(&value::PROMISE_VALUE_VERSION.to_le_bytes());
    let error = runtime.run_wasm_bytes(&stale, &[]).unwrap_err();
    assert!(error.to_string().contains("invalid value ABI"), "{error}");
}

#[test]
fn value17_cells_read_function_envelopes_and_value16_rejects_the_tag() {
    use varta_common::{abi as host_abi, function as layout};
    // A synthetic heap: a closure block without environment and a reference
    // union cell tagged Function pointing at it.
    let mut data = vec![0u8; host_abi::OWNED_STRING_BASE + 256];
    let closure = host_abi::OWNED_STRING_BASE + host_abi::STRING_HEADER_BYTES;
    let header = closure - host_abi::STRING_HEADER_BYTES;
    let put = |data: &mut Vec<u8>, at: usize, word: u32| {
        data[at..at + 4].copy_from_slice(&word.to_le_bytes());
    };
    put(
        &mut data,
        header,
        (host_abi::STRING_HEADER_BYTES + layout::CLOSURE_BYTES) as u32,
    );
    put(
        &mut data,
        header + value::HEAP_FLAGS_OFFSET,
        value::HEAP_ALLOCATED_FLAG | layout::HEAP_FLAG as u32,
    );
    put(&mut data, closure + 4, layout::FUNCTION_FIELDS as u32);
    put(&mut data, closure + layout::CODE_OFFSET as usize, 3);
    let handle = closure as u64 | ((layout::CLOSURE_BYTES as u64) << 32);
    let cell = closure + 32 + host_abi::STRING_HEADER_BYTES;
    let cell_header = cell - host_abi::STRING_HEADER_BYTES;
    put(
        &mut data,
        cell_header,
        (host_abi::STRING_HEADER_BYTES + value::UNION_BYTES) as u32,
    );
    put(
        &mut data,
        cell_header + value::HEAP_FLAGS_OFFSET,
        value::HEAP_ALLOCATED_FLAG | value::HEAP_UNION_FLAG as u32,
    );
    put(&mut data, cell_header + value::HEAP_LINK_OFFSET, 1);
    put(&mut data, cell, value::ValueKind::Function as u32);
    put(&mut data, cell + 4, 1);
    data[cell + value::UNION_META_BYTES..cell + value::UNION_BYTES]
        .copy_from_slice(&handle.to_le_bytes());
    let packed = cell as u64 | ((value::UNION_BYTES as u64) << 32);
    let read =
        value::ReferenceBox::read_with_version(&data, packed, value::FUNCTION_FIELD_VALUE_VERSION)
            .unwrap();
    assert_eq!(read.kind, value::ValueKind::Function);
    assert_eq!(read.payload, handle);
    // Value16 readers never admit the function tag.
    assert!(
        value::ReferenceBox::read_with_version(&data, packed, value::PROMISE_VALUE_VERSION)
            .is_err()
    );
    // A tampered closure (wrong flag, reserved word or environment edge) is
    // rejected before any reader observes it.
    for (at, word) in [
        (
            header + value::HEAP_FLAGS_OFFSET,
            value::HEAP_ALLOCATED_FLAG,
        ),
        (closure + 4, 0),
        (
            header + value::HEAP_LINK_OFFSET,
            layout::ENVIRONMENT_MASK as u32,
        ),
    ] {
        let mut bad = data.clone();
        put(&mut bad, at, word);
        assert!(
            value::ReferenceBox::read_with_version(
                &bad,
                packed,
                value::FUNCTION_FIELD_VALUE_VERSION
            )
            .is_err()
        );
    }
}
