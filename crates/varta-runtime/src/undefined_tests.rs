use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf8V2)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}

#[test]
fn undefined_is_distinct_from_null_and_preserves_evaluation_and_void_returns() {
    let output = run(r#"
function effect():void{console.log("effect");}
function empty():undefined{}
function bare():undefined{return;}
function explicit():void{return undefined;}
function identity(u:undefined):undefined{return u;}
export function main():void{
 const u=identity(undefined);console.log(typeof u);console.log(typeof null);
 console.log(u===null);console.log(u==null);console.log(u!==null);console.log(u!=null);
 console.log(u===0);console.log(u==false);console.log(!u);console.log(!!u);
 if(u){console.log("BAD");}else{console.log("false");}
 console.log(u||"fallback");console.log(u&&effect());console.log(void effect());
 console.log(effect());console.log(empty());console.log(bare());console.log(explicit());
 console.log((void effect())==null);console.log(null===(void effect()));
 {const undefined=4.5;console.log(undefined);}
}"#);
    assert_eq!(
        output.lines,
        [
            "undefined",
            "object",
            "0",
            "1",
            "1",
            "0",
            "0",
            "0",
            "1",
            "0",
            "false",
            "fallback",
            "undefined",
            "effect",
            "undefined",
            "effect",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "effect",
            "1",
            "effect",
            "0",
            "4.5"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn undefined_json_omits_fields_nulls_array_elements_and_returns_undefined_at_root() {
    let output = run(r#"
function make():{a:undefined;b:number;c:undefined;nested:{empty:undefined};values:undefined[]}{return {a:undefined,b:1.5,c:void 0,nested:{empty:undefined},values:[undefined,void 1]};}
export function main():void {
 const value=make();const alias=value;alias.a=void 2;
 console.log(JSON.stringify(value));console.log(JSON.stringify([undefined,void 0]));
 const text=JSON.stringify(undefined);console.log(text);console.log(typeof text);console.log(text===undefined);
 console.log(JSON.stringify(null));console.log(JSON.stringify({first:undefined,last:undefined}));
 console.log(varta.json.parse<number>('1.25'));console.log(varta.json.parse<string>('"kept"'));
 const empty=varta.json.parse<string>('""');console.log(typeof empty);console.log(JSON.stringify(empty));console.log(empty===undefined);
}"#);
    assert_eq!(
        output.lines,
        [
            r#"varta log: {"b":1.5,"nested":{},"values":[null,null]}"#,
            "varta log: [null,null]",
            "varta log: undefined",
            "varta log: undefined",
            "varta log: 1",
            "varta log: null",
            "varta log: {}",
            "varta log: 1.25",
            "varta log: kept",
            "varta log: string",
            "varta log: \"\"",
            "varta log: 0"
        ]
    );
}

#[test]
fn undefined_void_capture_and_json_preserve_system_status_without_authorizing_effects() {
    let output = run(r#"export function main():void {
 const result=varta.result.capture(varta.env.print("VARTA_UNDEFINED_DENIED"));
 console.log(result.value);console.log(result.status);
 console.log(JSON.stringify({value:result.value,n:1.5}));
 console.log(JSON.stringify(result.value));console.log(varta.result.status());
 console.log(void varta.process.exit(17));console.log(varta.result.status());
}"#);
    assert_eq!(output.exit_code, 0);
    assert!(output.lines[0].contains("capability denied: env"));
    assert_eq!(
        &output.lines[1..6],
        [
            "varta log: undefined",
            "varta log: 1",
            r#"varta log: {"n":1.5}"#,
            "varta log: undefined",
            "varta log: 1"
        ]
    );
    assert!(output.lines[6].contains("capability denied: process.exit"));
    assert_eq!(&output.lines[7..], ["varta log: undefined", "varta log: 1"]);
}

#[test]
fn undefined_keeps_legacy_profiles_and_unimplemented_dynamic_shapes_explicit() {
    let source = "export function main():void{console.log(undefined);}";
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf8V1,
    ] {
        assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
    }
    for source in [
        "export function main():void{varta.json.parse<undefined>('null');}",
        "export function main():void{varta.json.parse<{u:undefined}>('{}');}",
        "export function main():void{varta.json.parse<undefined[]>('[]');}",
        "export function main():void{const value=[undefined,1];}",
        "export function main():void{console.log(undefined);const undefined=1;}",
        "export function main():void{const n:number=undefined;}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf8V2
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn undefined_profile_requires_matching_abi_and_json_metadata_before_effects() {
    use crate::contract_tests::rewrite;
    use varta_common::{abi, value};
    let bytes =
        compile("export function main():void{console.log(JSON.stringify({u:undefined,n:1.5}));}");
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.inspect_module(&bytes).unwrap().value_abi_version,
        Some(value::UNDEFINED_VALUE_VERSION)
    );
    for (name, version) in [
        (abi::SECTION, abi::NUMBER_VERSION),
        (value::VALUE_SECTION, value::NUMBER_VALUE_VERSION),
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0 && data.get(1..1 + name.len()) == Some(name.as_bytes()) {
                data.truncate(1 + name.len());
                data.extend(version.to_le_bytes());
            }
            vec![data]
        });
        assert!(runtime.validate_module(&bad).is_err());
        assert!(runtime.run_wasm_bytes(&bad, &[]).is_err());
    }
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 2 {
            let from = b"json_stringify_v4";
            let to = b"json_stringify_v3";
            let at = data.windows(from.len()).position(|s| s == from).unwrap();
            data[at..at + from.len()].copy_from_slice(to);
        }
        vec![data]
    });
    assert!(runtime.validate_module(&bad).is_err());
}

#[test]
fn undefined_fields_do_not_become_roots_or_hide_live_aliases_during_collection() {
    let output = run(r#"
function churn():void{const piece="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
 for(let i=0;i<180;i=i+1){const dead={u:undefined,text:piece+piece+piece+piece,n:i};}}
function keep(o:{u:undefined;text:string},unused:undefined):{u:undefined;text:string}{return o;}
export function main():void {
 const saved={u:undefined,text:"saved"+"!"};const other=keep({u:void 0,text:"other"+"!"},churn());
 console.log(JSON.stringify(saved));console.log(JSON.stringify(other));console.log(saved.u===undefined);
 const parsed=varta.json.parse<{n:number;labels:string[]}>('{"n":-0,"labels":["kept"]}');
 churn();console.log(1/parsed.n);console.log(parsed.labels[0]);console.log(JSON.stringify(void churn()));
}"#);
    assert_eq!(
        output.lines,
        [
            r#"varta log: {"text":"saved!"}"#,
            r#"varta log: {"text":"other!"}"#,
            "varta log: 1",
            "varta log: -Infinity",
            "varta log: kept",
            "varta log: undefined"
        ]
    );
}
