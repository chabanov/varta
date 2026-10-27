use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V2)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn primitive_unions_support_reassignment_guards_functions_and_closed_containers() {
    let output = run(r#"
function describe(value:number|string|undefined):string {
 if(typeof value==="number"){return JSON.stringify(value+0.5);}
 if(typeof value==="string"){return value+"!";}
 return "none";
}
function keep(v:string|number|null|undefined|boolean):string|number|null|undefined|boolean{return v;}
export function main():void {
 let v:number|string|undefined=1.5;console.log(describe(v));v="😀";console.log(describe(v));v=undefined;console.log(describe(v));
 const values:(number|string|undefined)[]=[1.5,"\ud800",undefined];values[0]="changed";
 const object:{value:number|string|undefined;ok:boolean}={value:v,ok:true};object.value=3.5;
 console.log(JSON.stringify(values));console.log(JSON.stringify(object));console.log(typeof values[0]);
 console.log(keep(null));console.log(keep(-0));console.log(keep(Infinity));console.log(keep(true));
 const chosen=true?"chosen":0.5;console.log(chosen);console.log(typeof chosen);
}"#);
    assert_eq!(
        output.lines,
        [
            "2",
            "😀!",
            "none",
            r#"["changed","\ud800",null]"#,
            r#"{"value":3.5,"ok":true}"#,
            "string",
            "null",
            "-0",
            "Infinity",
            "1",
            "chosen",
            "string"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn primitive_union_operators_preserve_types_nullishness_short_circuit_and_effect_order() {
    let output = run(r#"
function value(v:number|string|null|undefined|boolean):number|string|null|undefined|boolean{console.log("left");return v;}
function right():string{console.log("right");return "fallback";}
function nullValue():null{console.log("null");return null;}
export function main():void {
 const n:number|string|null|undefined|boolean=NaN;const z:number|undefined=-0;const s:string|number="";
 console.log(n===n);console.log(z===0);console.log(z!==undefined);console.log(n===undefined);console.log(s===false);
 console.log(!n);console.log(!s);if(s){console.log("BAD");}else{console.log("empty");}
 console.log(value(0)||right());console.log(value("kept")||right());
 console.log(value(0)&&right());console.log(value(1)&&right());
 console.log(value(0)??right());console.log(value(undefined)??right());
 console.log(value(undefined)==nullValue());console.log(nullValue()!=value(0));
}"#);
    assert_eq!(
        output.lines,
        [
            "0", "1", "1", "0", "0", "1", "1", "empty", "left", "right", "fallback", "left",
            "kept", "left", "0", "left", "right", "fallback", "left", "0", "left", "right",
            "fallback", "left", "null", "1", "null", "left", "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn primitive_union_json_preserves_undefined_at_root_and_omits_fields() {
    let output = run(r#"
function stringify(v:number|string|undefined):string|undefined{return JSON.stringify(v);}
export function main():void {
 let value:number|string|undefined=undefined;
 console.log(stringify(value));console.log(typeof stringify(value));
 console.log(JSON.stringify({first:value,n:1.5,last:value}));console.log(JSON.stringify([value,value]));
 value="\ud800";const encoded=stringify(value);console.log(encoded);
 if(typeof encoded==="string"){console.log(varta.json.parse<string>(encoded).charCodeAt());}
 value=NaN;console.log(stringify(value));console.log(JSON.stringify(varta.json.parse<{n:number}>('{"n":1.5}')));
}"#);
    assert_eq!(
        output.lines,
        [
            "undefined",
            "undefined",
            r#"{"n":1.5}"#,
            "[null,null]",
            r#""\ud800""#,
            "55296",
            "null",
            r#"{"n":1.5}"#
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn primitive_union_guards_are_scoped_and_preserve_shadowing_and_numeric_bits() {
    let output = run(r#"
function read(v:string|number|undefined):string {
 if("string"===typeof v){const n=v.length;{const v=4.5;console.log(v);}return v+JSON.stringify(n);}
 else {if(typeof v==="number"){return JSON.stringify(v*2);}else{return "missing";}}
}
function optional(v:string|undefined):string {if(v!==undefined){return v;}else{return "empty";}}
function maybe(v:number|string):number {return typeof v==="number"?v+1:v.length;}
function zero(v:number|undefined):number {if(typeof v==="number"){return 1/v;}return 0;}
export function main():void {
 console.log(read("😀"));console.log(read(1.25));console.log(read(undefined));console.log(optional("kept"));console.log(optional(undefined));
 console.log(maybe(0.5));console.log(maybe("😀"));console.log(zero(-0));
 const a:number|undefined=NaN;const b:number|undefined=NaN;console.log(a===b);
 const x:string|undefined="\ud83d"+"\ude00";console.log(x==="😀");
}"#);
    assert_eq!(
        output.lines,
        [
            "4.5",
            "😀2",
            "2.5",
            "missing",
            "kept",
            "empty",
            "1.5",
            "2",
            "-Infinity",
            "0",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn primitive_union_rejections_do_not_admit_unsafe_writes_casts_or_legacy_layouts() {
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf8V1,
        LanguageProfile::CheckedF64Utf8V2,
        LanguageProfile::CheckedF64Utf16V1,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{let v:number|string=1;}",
                profile
            )
            .is_err()
        );
    }
    for source in [
        "export function main():void{const v:number|string=true;}",
        "export function main():void{let v:number|string=1;v=undefined;}",
        "export function main():void{let v:number|string=1;if(typeof v==='number'){v='x';}}",
        "export function main():void{let v:number|string=1;if(typeof v==='number'){while(true){console.log(v+1);v='x';}}}",
        "export function main():void{const v:number|string=1;if(typeof v==='number'){console.log(v+1);}console.log(v+1);}",
        "export function main():void{const v:number|string=1;console.log(v==1);}",
        "export function main():void{const v:number|string=1;varta.env.get(v);}",
        "export function main():void{const v:{n:number}|string='x';}",
        "export function main():void{varta.json.parse<string|number>('1');}",
        "export function main():void{varta.json.parse<{v:string|number}>('{\"v\":1}');}",
        "export function main():void{let v:string|undefined;}",
        "export function main():void{console.log(v);const v:string|undefined=undefined;}",
        "export function main():void{const v:number|string=1;console.log(v as number);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V2
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn primitive_union_profile_validates_versions_and_imports_before_any_effect() {
    use crate::contract_tests::rewrite;
    use varta_common::{abi, value};
    let bytes = compile(
        "export function main():void{const v:string|undefined='kept';console.log(JSON.stringify(v));}",
    );
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.inspect_module(&bytes).unwrap().value_abi_version,
        Some(value::UNION_VALUE_VERSION)
    );
    for (name, version) in [
        (abi::SECTION, abi::UTF16_VERSION),
        (value::VALUE_SECTION, value::UTF16_VALUE_VERSION),
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0 && data.get(1..1 + name.len()) == Some(name.as_bytes()) {
                data.truncate(1 + name.len());
                data.extend(version.to_le_bytes());
            }
            vec![data]
        });
        assert!(runtime.run_wasm_bytes(&bad, &[]).is_err());
    }
    for name in [
        abi::SECTION,
        value::VALUE_SECTION,
        varta_common::language::SECTION,
    ] {
        let bad = rewrite(&bytes, |id, data| {
            if id == 0 && data.get(1..1 + name.len()) == Some(name.as_bytes()) {
                vec![]
            } else {
                vec![data.to_vec()]
            }
        });
        assert!(runtime.validate_module(&bad).is_err());
    }
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 2 {
            let from = b"json_stringify_v6";
            let at = data.windows(from.len()).position(|s| s == from).unwrap();
            data[at..at + from.len()].copy_from_slice(b"json_stringify_v5");
        }
        vec![data]
    });
    assert!(runtime.validate_module(&bad).is_err());
}

#[test]
fn primitive_union_cells_trace_string_children_and_never_treat_numeric_bits_as_roots() {
    let source=r#"
function churn():number|undefined {const piece="PIECE";for(let i=0;i<190;i=i+1){const dead:string|number=piece+piece+"\ud800";}return 1.5;}
function keep(a:string|number|undefined,b:number|undefined):string|number|undefined{return a;}
export function main():void {
 const strings:(string|number|undefined)[]=["saved"+"\ud800",undefined];
 const saved:{v:string|number|undefined;n:number|undefined}={v:"other"+"\udfff",n:FAKE};
 const first=keep("argument"+"!",churn());churn();
 console.log(JSON.stringify(strings));console.log(JSON.stringify(saved.v));console.log(first);console.log(saved.n===FAKE);
}"#.replace("PIECE",&"x".repeat(8192)).replace("FAKE",&format!("{:?}",f64::from_bits(varta_common::abi::OWNED_STRING_BASE as u64)));
    let bytes = compile(&source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution
        .store
        .set_fuel(200 * varta_common::abi::FUEL)
        .unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            r#"varta log: ["saved\ud800",null]"#,
            r#"varta log: "other\udfff""#,
            "varta log: argument!",
            "varta log: 1"
        ]
    );
}
