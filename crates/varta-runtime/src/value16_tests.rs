//! Value layout 16 (v35 `promise_values`): every value-reading host operation,
//! JSON schema 16 and exception channel v7 read value16 layouts with exactly
//! the value15 behaviour, while v34 keeps value15, its operations and host ABI
//! 30. Every v34 case compiled under v35 must log exactly what it logs under
//! v34 (whose logs are pinned against Node 24 in their own tests).
use super::*;
use varta_common::{HostOperation as H, language::LanguageProfile as P, value};

const V34: P = P::CheckedF64Utf16V34;
const V35: P = P::CheckedF64Utf16V35;

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
fn value16_cases_match_value15() {
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
        include_str!("v34_cross_cases/wide_dictionary.ts"),
        include_str!("unknown_cases/assertions.ts"),
        include_str!("unknown_cases/errors.ts"),
        include_str!("unknown_cases/guards.ts"),
        include_str!("generator_cases/protocol.ts"),
        include_str!("generator_cases/finally.ts"),
        include_str!("generator_cases/values.ts"),
        include_str!("promise_cases/ordering.ts"),
        include_str!("promise_cases/values.ts"),
        include_str!("promise_cases/rejections.ts"),
        include_str!("promise_cases/async_order.ts"),
        include_str!("promise_cases/async_features.ts"),
        include_str!("promise_cases/lifetime.ts"),
    ] {
        let old = outcome(source, V34);
        assert!(!old.0.is_empty() || old.1.is_some(), "{source}");
        assert_eq!(outcome(source, V35), old, "{source}");
    }
}

#[test]
fn v35_modules_import_only_value16_readers_and_channel_v7() {
    let source = "class Failure extends Error{constructor(m:string){super(m);}}\
        function f(v:number|string|boolean|null|undefined|bigint):string{\
        console.log(v);console.log(typeof v);console.log(v===1);console.log(v==null);\
        return String(v)+String(Number(v===1n?1:v===undefined?2:3))+String(BigInt(v===null?1:2));}\
        export function main():void{\
        const xs:(number[]|string)[]=[[1],'a'];console.log(Array.isArray(xs[0]));\
        try{throw new Failure('x');}catch(e){console.log(e instanceof RangeError);}\
        const t=JSON.stringify({a:1});const p=varta.json.parse<{a:number}>(t);\
        console.log(f(p.a)+(xs[1]?'y':'n'));}";
    let new = imports(&compile(source, V35));
    let old = imports(&compile(source, V34));
    // Every value15 reader the v34 module uses has its value16 replacement,
    // and no other import changes.
    for op in &old {
        if let Some(replacement) = new.iter().find(|n| n.value15_counterpart() == Some(*op)) {
            assert!(!new.contains(op), "{op:?}");
            assert_eq!(replacement.minimum_abi(), abi::PROMISE_VALUES_VERSION);
        } else {
            assert!(new.contains(op), "{op:?}");
        }
    }
    assert!(
        new.iter()
            .filter(|op| op.value15_counterpart().is_some())
            .count()
            >= 8,
        "{new:?}"
    );
    assert!(old.iter().all(|op| op.value15_counterpart().is_none()));
    assert!(new.iter().all(|op| op.value13_counterpart().is_none()));
    let run = |profile| {
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source, profile), &[])
            .unwrap()
            .lines
    };
    assert_eq!(run(V35), run(V34));
    let module = VartaRuntime::new()
        .validate_module(&compile(source, V35))
        .unwrap();
    assert!(
        module
            .exports()
            .any(|e| e.name() == varta_common::exceptions::PROMISE_VERSION)
    );
}

#[test]
fn value15_and_value16_readers_reject_each_others_profiles() {
    let source = "export function main():void{const v:number|string='s';console.log(typeof v);\
        console.log(JSON.stringify({a:[1]}));\
        console.log(varta.json.parse<{a:number}>('{\"a\":1}').a);}";
    let runtime = VartaRuntime::new();
    let new = compile(source, V35);
    let old = compile(source, V34);
    assert!(runtime.validate_module(&new).is_ok());
    assert!(runtime.validate_module(&old).is_ok());
    for (from, to) in [
        ("value_typeof_v9", "value_typeof_v8"),
        ("json_stringify_v15", "json_stringify_v14"),
        ("json_parse_v17", "json_parse_v16"),
    ] {
        // A v35 module may not read value16 graphs through value15 readers ...
        let downgraded = rename(&new, from, to);
        assert!(runtime.validate_module(&downgraded).is_err(), "{to}");
        // ... and a v34 module may not reach the value16 readers.
        let upgraded = rename(&old, to, from);
        assert!(runtime.validate_module(&upgraded).is_err(), "{from}");
    }
    // The exception channel names its value layout; a swapped channel fails
    // before any guest effect.
    let swapped = rename(
        &new,
        varta_common::exceptions::PROMISE_VERSION,
        varta_common::exceptions::DYNAMIC_VERSION,
    );
    let error = runtime.run_wasm_bytes(&swapped, &[]).unwrap_err();
    assert!(error.to_string().contains("exception"), "{error}");
}

#[test]
fn value16_readers_reject_value15_stamped_descriptors() {
    let source = "export function main():void{console.log(JSON.stringify({k:1}));}";
    let bytes = compile(source, V35);
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        [r#"varta log: {"k":1}"#]
    );
    let mut header = value::PROMISE_VALUE_VERSION.to_le_bytes().to_vec();
    header.extend(1u32.to_le_bytes());
    let at = bytes
        .windows(header.len())
        .position(|w| w == header.as_slice())
        .expect("value16 descriptor in the data section");
    let mut stale = bytes.clone();
    stale[at..at + 4].copy_from_slice(&value::DYNAMIC_VALUE_VERSION.to_le_bytes());
    let error = runtime.run_wasm_bytes(&stale, &[]).unwrap_err();
    assert!(error.to_string().contains("invalid value ABI"), "{error}");
}
