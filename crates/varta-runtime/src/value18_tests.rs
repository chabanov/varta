//! Value layout 18 (v38 `dictionary_hashing`): every value-reading host
//! operation, JSON schema 18 and exception channel v9 read value18 layouts
//! with exactly the value17 behaviour, while v37 keeps value17, its readers
//! and host ABI 33. Every v37 case compiled under v38 logs exactly what it
//! logs under v37 (pinned against Node 24 in their own tests), and the two
//! layouts never read each other's modules.
use super::*;
use varta_common::{HostOperation as H, language::LanguageProfile as P, value};

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

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

/// The hashed-dictionary cases, which need test-only fuel, are compared in
/// `dictionary_hashing_tests`. V38's deliberate deviations (`frozen_profile_tests::V38_DEVIATIONS`,
/// `JSON.stringify` typed `string`) are left out of the corpus.
#[test]
fn value18_cases_match_value17() {
    for source in [
        include_str!("value15_cases/values.ts"),
        include_str!("value15_cases/checked.ts"),
        include_str!("value15_cases/dictionary.ts"),
        include_str!("value15_cases/dictionary_json.ts"),
        include_str!("value15_cases/recursive.ts"),
        include_str!("value15_cases/wide_json.ts"),
        include_str!("v34_cross_cases/caught_unknown.ts"),
        include_str!("v34_cross_cases/json_tree.ts"),
        include_str!("unknown_cases/assertions.ts"),
        include_str!("unknown_cases/guards.ts"),
        include_str!("promise_value_cases/containers.ts"),
        include_str!("v35_cases/mixed.ts"),
    ] {
        let old = outcome(source, V37);
        assert!(!old.0.is_empty() || old.1.is_some(), "{source}");
        assert_eq!(outcome(source, V38), old, "{source}");
    }
}

#[test]
fn v38_modules_import_only_value18_readers_and_channel_v9() {
    let source = "class Failure extends Error{constructor(m:string){super(m);}}\
        function f(v:number|string|boolean|null|undefined|bigint):string{\
        console.log(v);console.log(typeof v);console.log(v===1);console.log(v==null);\
        return String(v)+String(Number(v===1n?1:v===undefined?2:3))+String(BigInt(v===null?1:2));}\
        export function main():void{\
        const xs:(number[]|string)[]=[[1],'a'];console.log(Array.isArray(xs[0]));\
        try{throw new Failure('x');}catch(e){console.log(e instanceof RangeError);}\
        const d:Record<string,number>={a:1};const t=JSON.stringify(d);const p=varta.json.parse<{a:number}>(t);\
        console.log(f(p.a)+(xs[1]?'y':'n'));}";
    let new = imports(&compile(source, V38));
    let old = imports(&compile(source, V37));
    // Every value17 reader the v37 module uses has its value18 replacement,
    // and no other import changes except the owned serializer.
    for op in &old {
        if let Some(replacement) = new.iter().find(|n| n.value17_counterpart() == Some(*op)) {
            assert!(!new.contains(op), "{op:?}");
            assert_eq!(replacement.minimum_abi(), abi::PROGRAM_LIMITS_VERSION);
        } else if *op != H::JsonStringifyV17 {
            assert!(
                new.contains(op) || V38.links(*op),
                "{op:?} missing from {new:?}"
            );
        }
    }
    assert!(
        new.iter()
            .filter(|op| op.value17_counterpart().is_some())
            .count()
            >= 6,
        "{new:?}"
    );
    assert!(old.iter().all(|op| op.value17_counterpart().is_none()));
    assert!(new.iter().all(|op| op.value16_counterpart().is_none()));
    let run = |profile| {
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source, profile), &[])
            .unwrap()
            .lines
    };
    assert_eq!(run(V38), run(V37));
    let module = VartaRuntime::new()
        .validate_module(&compile(source, V38))
        .unwrap();
    assert!(
        module
            .exports()
            .any(|e| e.name() == varta_common::exceptions::HASHED_DICTIONARY_VERSION)
    );
    assert!(
        !module
            .exports()
            .any(|e| e.name() == varta_common::exceptions::FUNCTION_FIELD_VERSION)
    );
}

#[test]
fn value17_and_value18_readers_reject_each_others_profiles() {
    let source = "export function main():void{const v:number|string='s';console.log(v);\
        const d:Record<string,number>={a:1};console.log(JSON.stringify(d));\
        console.log(varta.json.parse<{a:number}>('{\"a\":1}').a);}";
    let runtime = VartaRuntime::new();
    let new = compile(source, V38);
    let old = compile(source, V37);
    assert!(runtime.validate_module(&new).is_ok());
    assert!(runtime.validate_module(&old).is_ok());
    for (from, to) in [
        ("value_log_v12", "value_log_v11"),
        ("json_parse_v19", "json_parse_v18"),
    ] {
        // A v38 module may not read value18 graphs through value17 readers ...
        let downgraded = rename(&new, from, to);
        assert!(runtime.validate_module(&downgraded).is_err(), "{to}");
        // ... and a v37 module may not reach the value18 readers.
        let upgraded = rename(&old, to, from);
        assert!(runtime.validate_module(&upgraded).is_err(), "{from}");
    }
    // The exception channel names its value layout; a swapped channel fails
    // before any guest effect.
    let swapped = rename(
        &new,
        varta_common::exceptions::HASHED_DICTIONARY_VERSION,
        varta_common::exceptions::FUNCTION_FIELD_VERSION,
    );
    let error = runtime.run_wasm_bytes(&swapped, &[]).unwrap_err();
    assert!(error.to_string().contains("exception"), "{error}");
}

#[test]
fn value18_readers_reject_value17_stamped_descriptors_and_storage() {
    let source = "export function main():void{const d:Record<string,number>={k:1};\
        console.log(JSON.stringify(d));}";
    let bytes = compile(source, V38);
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        [r#"varta log: {"k":1}"#]
    );
    // The value18 storage descriptor: four canonical slots.
    let mut header = value::HASHED_DICTIONARY_VALUE_VERSION
        .to_le_bytes()
        .to_vec();
    header.extend((value::HASHED_DICTIONARY_FIELDS as u32).to_le_bytes());
    let at = bytes
        .windows(header.len())
        .position(|w| w == header.as_slice())
        .expect("value18 dictionary storage descriptor in the data section");
    let mut stale = bytes.clone();
    stale[at..at + 4].copy_from_slice(&value::FUNCTION_FIELD_VALUE_VERSION.to_le_bytes());
    let error = runtime.run_wasm_bytes(&stale, &[]).unwrap_err();
    assert!(error.to_string().contains("invalid value ABI"), "{error}");
    // A storage descriptor naming another table field is not a dictionary.
    let key = value::DICTIONARY_TABLE_FIELD.as_bytes();
    let at = bytes
        .windows(key.len())
        .position(|w| w == key)
        .expect("table field name in the data section");
    let mut renamed = bytes.clone();
    renamed[at] = b'T';
    let error = runtime.run_wasm_bytes(&renamed, &[]).unwrap_err();
    assert!(error.to_string().contains("dictionary"), "{error}");
}
