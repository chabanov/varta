//! Value layout 15 (v34): every value-reading host operation, the JSON schema
//! and the exception channel read value15 layouts with exactly the value13
//! behaviour, while v33 keeps value13 and its operations. Expected logs of the
//! Node-comparable cases were produced by Node 24 and are pinned here.
use super::*;
use varta_common::{HostOperation as H, language::LanguageProfile as P};

const V33: P = P::CheckedF64Utf16V33;
const V34: P = P::CheckedF64Utf16V34;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn logs(source: &str, profile: P) -> Vec<String> {
    VartaRuntime::new()
        .run_wasm_bytes(&compile(source, profile), &[])
        .unwrap_or_else(|e| panic!("{profile:?}: {e:?}\n{source}"))
        .lines
}
fn same_under_v33_and_v34(source: &str, expected: &[&str]) {
    let expected: Vec<_> = expected.iter().map(|s| format!("varta log: {s}")).collect();
    for profile in [V33, V34] {
        assert_eq!(logs(source, profile), expected, "{profile:?}");
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
fn value15_values_match_value13_and_node() {
    same_under_v33_and_v34(
        include_str!("value15_cases/values.ts"),
        &[
            r#"{"id":"a","count":2,"meta":{"tags":["x","y"],"owner":null}}"#,
            r#"{"tags":["x","y","z"],"owner":null,"score":0.5}"#,
            "bigint TypeError",
            "36 bigint",
            "number:1.5:1.5",
            "string:2:2",
            "boolean:true:1",
            "object:null:0",
            "undefined:undefined:NaN",
            r#"["s",[1,2],null]"#,
            "array",
            r#"{"label":"crate","size":3} true"#,
            "Failure failed 7 7",
            "error false",
        ],
    );
}
#[test]
fn value15_frames_and_closures_match_value13_and_node() {
    same_under_v33_and_v34(
        include_str!("value15_cases/frames.ts"),
        &[
            r#"{"name":"root","children":["c1","c2"]}"#,
            r#"{"name":"a38","children":["x"]}"#,
            r#"{"name":"b38","children":[]}"#,
            "6",
            r#"3 ["a","b"]"#,
        ],
    );
}
#[test]
fn value15_checked_json_and_recoverable_errors_match_value13() {
    same_under_v33_and_v34(
        include_str!("value15_cases/checked.ts"),
        &[
            "n 3 true",
            r#"{"list":[1,null,2.5],"name":"n"}"#,
            "TypeError: JSON value does not match the checked schema",
            "SyntaxError",
        ],
    );
}
#[test]
fn value15_diagnostics_and_uncaught_instances_match_value13() {
    let source = "class Failure extends Error{constructor(m:string){super(m);this.name='Failure';}}\
        export function main():void{\
        const r:{a:number;b:(string|null)[];c:bigint}={a:1,b:['x',null],c:5n};\
        const u:number|string|{n:number}={n:2};const w:number|bigint=r.c;\
        const a:number|(string|null)[]=r.b;console.log(u);console.log(w);console.log(a);\
        throw new Failure('boom');}";
    for profile in [V33, V34] {
        let error = VartaRuntime::new()
            .run_wasm_bytes(&compile(source, profile), &[])
            .unwrap_err();
        let text = error.to_string();
        assert!(text.contains("Failure: \"boom\""), "{profile:?}: {text}");
    }
    let run = |profile| {
        let bytes = compile(&source.replace("throw new Failure('boom');", ""), profile);
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines
    };
    assert_eq!(run(V33), run(V34));
    assert_eq!(
        run(V34),
        [
            r#"varta log: {"n":2}"#,
            "varta log: 5",
            r#"varta log: ["x",null]"#,
        ]
    );
}

#[test]
fn v34_modules_import_only_value15_readers_and_channel_v6() {
    let source = "class Failure extends Error{constructor(m:string){super(m);}}\
        function f(v:number|string|boolean|null|undefined|bigint):string{\
        console.log(v);console.log(typeof v);console.log(v===1);console.log(v==null);\
        return String(v)+String(Number(v===1n?1:v===undefined?2:3))+String(BigInt(v===null?1:2));}\
        export function main():void{\
        const xs:(number[]|string)[]=[[1],'a'];console.log(Array.isArray(xs[0]));\
        try{throw new Failure('x');}catch(e){console.log(e instanceof RangeError);}\
        const t=JSON.stringify({a:1});const p=varta.json.parse<{a:number}>(t);\
        console.log(f(p.a)+(xs[1]?'y':'n'));}";
    let new = imports(&compile(source, V34));
    let old = imports(&compile(source, V33));
    let value15: Vec<_> = new
        .iter()
        .filter(|op| op.value13_counterpart().is_some())
        .collect();
    // Every value13 reader the v33 module uses has its value15 replacement.
    for op in &old {
        if let Some(replacement) = new.iter().find(|n| n.value13_counterpart() == Some(*op)) {
            assert!(!new.contains(op), "{op:?}");
            assert!(value15.contains(&replacement));
        } else {
            assert!(new.contains(op), "{op:?}");
        }
    }
    assert!(value15.len() >= 8, "{new:?}");
    assert!(old.iter().all(|op| op.value13_counterpart().is_none()));
    for op in [
        H::JsonStringifyV14,
        H::JsonParseV16,
        H::ValueLogV9,
        H::PrimitiveStringV3,
    ] {
        assert!(new.contains(&op), "{op:?}");
    }
    let bytes = compile(source, V34);
    let module = VartaRuntime::new().validate_module(&bytes).unwrap();
    assert!(
        module
            .exports()
            .any(|e| e.name() == varta_common::exceptions::DYNAMIC_VERSION)
    );
}

#[test]
fn value13_and_value15_readers_reject_each_others_profiles() {
    let source = "export function main():void{const v:number|string='s';console.log(v);\
        console.log(JSON.stringify({a:[1]}));\
        console.log(varta.json.parse<{a:number}>('{\"a\":1}').a);}";
    let runtime = VartaRuntime::new();
    let new = compile(source, V34);
    let old = compile(source, V33);
    assert!(runtime.validate_module(&new).is_ok());
    assert!(runtime.validate_module(&old).is_ok());
    for (from, to) in [
        ("value_log_v9", "value_log_v8"),
        ("json_stringify_v14", "json_stringify_v13"),
        ("json_parse_v16", "json_parse_v15"),
    ] {
        // A v34 module may not read value15 graphs through value13 readers ...
        let downgraded = rename(&new, from, to);
        assert!(runtime.validate_module(&downgraded).is_err(), "{to}");
        // ... and a v33 module may not reach the value15 readers.
        let upgraded = rename(&old, to, from);
        assert!(runtime.validate_module(&upgraded).is_err(), "{from}");
    }
    // The exception channel names its value layout; a swapped channel fails
    // before any guest effect.
    let swapped = rename(
        &new,
        varta_common::exceptions::DYNAMIC_VERSION,
        varta_common::exceptions::CLASS_ERROR_VERSION,
    );
    let error = runtime.run_wasm_bytes(&swapped, &[]).unwrap_err();
    assert!(error.to_string().contains("exception"), "{error}");
}

#[test]
fn value15_readers_reject_value13_stamped_descriptors() {
    // A v34 record whose descriptor is stamped value13 is not a value15 graph.
    let source = "export function main():void{console.log(JSON.stringify({k:1}));}";
    let bytes = compile(source, V34);
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        [r#"varta log: {"k":1}"#]
    );
    // The descriptor header is [version:u32, count:u32] followed by the key
    // entry; stamp it with the value13 version in the data section.
    let mut header = varta_common::value::DYNAMIC_VALUE_VERSION
        .to_le_bytes()
        .to_vec();
    header.extend(1u32.to_le_bytes());
    let at = bytes
        .windows(header.len())
        .position(|w| w == header.as_slice())
        .expect("value15 descriptor in the data section");
    let mut stale = bytes.clone();
    stale[at..at + 4].copy_from_slice(&varta_common::value::BIGINT_VALUE_VERSION.to_le_bytes());
    let error = runtime.run_wasm_bytes(&stale, &[]).unwrap_err();
    assert!(error.to_string().contains("invalid value ABI"), "{error}");
}

#[test]
fn value15_generator_cases_match_value13() {
    for source in [
        include_str!("generator_cases/protocol.ts"),
        include_str!("generator_cases/finally.ts"),
        include_str!("generator_cases/for_of.ts"),
        include_str!("generator_cases/values.ts"),
        include_str!("generator_cases/lifetime.ts"),
        include_str!("generator_cases/guide.ts"),
    ] {
        let old = logs(source, V33);
        assert!(!old.is_empty());
        assert_eq!(logs(source, V34), old, "{source}");
    }
}
