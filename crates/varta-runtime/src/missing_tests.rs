use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V3)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn missing_primitive_indices_return_undefined_without_truncation_or_number_loss() {
    let output = run(r#"export function main():void {
 const values=[-0,1.5];const n=values[-0];if(typeof n==="number"){console.log(1/n);}
 console.log(values[1]);console.log(values[2]);console.log(values[-1]);console.log(values[0.5]);console.log(values[NaN]);console.log(values[Infinity]);
 console.log(values[1.9]??"fraction");console.log(values[4294967296]);
 const text="A😀";console.log(JSON.stringify(text[1]));console.log(JSON.stringify(text[2]));console.log(text[3]);console.log(text[-0]);
 console.log(text[NaN]);console.log(text[1.5]);console.log(text[Infinity]);console.log(""[0]);
 const empty:number[]=[];console.log(empty[0]);const u:undefined[]=[undefined];console.log(u[0]);console.log(u[1]);
 const mixed:(string|number)[]=["kept",1];console.log(mixed[0]);console.log(mixed[2]);mixed[1]="changed";console.log(mixed[1]);
}"#);
    assert_eq!(
        output.lines,
        [
            "-Infinity",
            "1.5",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "fraction",
            "undefined",
            r#""\ud83d""#,
            r#""\ude00""#,
            "undefined",
            "A",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "kept",
            "undefined",
            "changed"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn optional_records_preserve_presence_explicit_undefined_and_insertion_order() {
    let output = run(r#"
function inspect(value:{first?:number;last:number;other?:string}):void{
 console.log(Object.hasOwn(value,"first"));console.log(value.first);console.log(JSON.stringify(value));
 value.first=2.5;value.other="kept";console.log(JSON.stringify(value));
 value.first=undefined;console.log(Object.hasOwn(value,"first"));console.log(JSON.stringify(value));
 value.first=3.5;console.log(JSON.stringify(value));
}
export function main():void {
 inspect({last:1});inspect({first:undefined,last:1});
 const n:{"2"?:number;"1"?:number;a:number;z?:number;"4294967295"?:number}={a:0};
 n.z=3;n["2"]=2;n["1"]=1;n["4294967295"]=4;console.log(JSON.stringify(n));
 console.log(Object.hasOwn(n,"missing"));console.log(n.missing);console.log(Object.hasOwn(n,"toString"));
 const empty:{}={};console.log(JSON.stringify(empty));console.log(Object.hasOwn(empty,"x"));
}"#);
    assert_eq!(
        output.lines,
        [
            "0",
            "undefined",
            r#"{"last":1}"#,
            r#"{"last":1,"first":2.5,"other":"kept"}"#,
            "1",
            r#"{"last":1,"other":"kept"}"#,
            r#"{"last":1,"first":3.5,"other":"kept"}"#,
            "1",
            "undefined",
            r#"{"last":1}"#,
            r#"{"first":2.5,"last":1,"other":"kept"}"#,
            "1",
            r#"{"last":1,"other":"kept"}"#,
            r#"{"first":3.5,"last":1,"other":"kept"}"#,
            r#"{"1":1,"2":2,"a":0,"z":3,"4294967295":4}"#,
            "0",
            "undefined",
            "0",
            "{}",
            "0"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn missing_values_keep_receiver_and_rhs_effect_order_and_let_initialization() {
    let output = run(r#"
function make():{x:number}{console.log("receiver");return {x:1};}
function values():number[]{console.log("array");return [1.5];}
function index():number{console.log("index");return 4;}
function rhs(v:{field?:string}):string{console.log(Object.hasOwn(v,"field"));return "😀";}
function empty():number|undefined{}
function bare():string|undefined{return;}
export function main():void{
 let value:number|string|undefined;console.log(value);value="kept";console.log(value);
 let u:undefined;console.log(u);console.log(make().absent);console.log(Object.hasOwn(make(),"x"));
 console.log(values()[index()]);const object:{field?:string}={};object.field=rhs(object);console.log(Object.hasOwn(object,"field"));console.log(JSON.stringify(object));
 console.log(empty());console.log(bare());
}"#);
    assert_eq!(
        output.lines,
        [
            "undefined",
            "kept",
            "undefined",
            "receiver",
            "undefined",
            "receiver",
            "1",
            "array",
            "index",
            "undefined",
            "0",
            "1",
            r#"{"field":"😀"}"#,
            "undefined",
            "undefined"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn missing_profile_keeps_checked_parsing_for_required_records_and_system_status() {
    let output = run(r#"export function main():void{
 const denied=varta.result.capture(varta.env.get("VARTA_MISSING_DENIED"));
 const parsed=varta.json.parse<{"2":number;a:string;items:number[]}>('{"a":"😀","2":1.5,"items":[-0]}');
 console.log(Object.hasOwn(parsed,"a"));console.log(JSON.stringify(parsed));console.log(parsed.items[8]);
 const optional:{field?:string}={};console.log(Object.hasOwn(optional,"field"));console.log(JSON.stringify(optional));console.log(varta.result.status());
}"#);
    assert!(output.lines[0].contains("capability denied"));
    assert_eq!(
        &output.lines[1..],
        [
            "varta log: 1",
            r#"varta log: {"2":1.5,"a":"😀","items":[0]}"#,
            "varta log: undefined",
            "varta log: 0",
            "varta log: {}",
            "varta log: 1"
        ]
    );
}

#[test]
fn missing_profile_rejects_unimplemented_reference_unions_inherited_reads_and_unsafe_writes() {
    for source in [
        "export function main():void{const v:{x:number;y?:number}={};}",
        "export function main():void{const v:{x?:number}={x:'bad'};}",
        "export function main():void{const v:{x?:number}={other:1};}",
        "export function main():void{const v:{x?:{n:number}}={};}",
        "export function main():void{const v:{x?:number}={};v.unknown=1;}",
        "export function main():void{const v={};console.log(v.toString);}",
        "export function main():void{const v:{toString?:number}={};console.log(v.toString);}",
        "export function main():void{let v:number;}",
        "export function main():void{let v;}",
        "export function main():void{console.log(v);let v:number|undefined;}",
        "export function main():void{let v:number|undefined=v;}",
        "export function main():void{const a=[{n:1}];console.log(a[0]);}",
        "export function main():void{const a=[1];console.log(a['0']);}",
        "export function main():void{const v={};const name='x';Object.hasOwn(v,name);}",
        "export function main():void{const Object={};Object.hasOwn({},'x');}",
        "export function main():void{varta.json.parse<{x?:number}>('{}');}",
        "export function main():void{const v={};delete v.x;}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V3
            )
            .is_err(),
            "{source}"
        );
    }
    for source in [
        "export function main():void{const a=[1];a[1]=2;}",
        "export function main():void{const a=[1];a[-1]=2;}",
        "export function main():void{const a=[1];a[0.5]=2;}",
    ] {
        assert!(
            VartaRuntime::new()
                .run_wasm_bytes(&compile(source), &[])
                .is_err(),
            "{source}"
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf8V1,
        LanguageProfile::CheckedF64Utf8V2,
        LanguageProfile::CheckedF64Utf16V1,
        LanguageProfile::CheckedF64Utf16V2,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{const v:{x?:number}={};}",
                profile
            )
            .is_err()
        );
        let wasm = varta_compiler::compile_typescript_with_profile(
            "export function main():void{const a=[1];console.log(a[1]);}",
            profile,
        )
        .unwrap();
        assert!(VartaRuntime::new().run_wasm_bytes(&wasm, &[]).is_err());
    }
}

#[test]
fn missing_profile_requires_matching_versions_and_complete_metadata() {
    use crate::contract_tests::rewrite;
    use varta_common::{abi, value};
    let bytes = compile(
        "export function main():void{const v:{field?:number}={};console.log(JSON.stringify(v));}",
    );
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.inspect_module(&bytes).unwrap().value_abi_version,
        Some(value::MISSING_VALUE_VERSION)
    );
    for (name, version) in [
        (abi::SECTION, abi::UNION_VERSION),
        (value::VALUE_SECTION, value::UNION_VALUE_VERSION),
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
            let from = b"json_stringify_v7";
            let at = data.windows(from.len()).position(|s| s == from).unwrap();
            data[at..at + from.len()].copy_from_slice(b"json_stringify_v6");
        }
        vec![data]
    });
    assert!(runtime.validate_module(&bad).is_err());
}

#[test]
fn missing_records_use_all_32_presence_bits_without_reordering_existing_keys() {
    let shape = (0..32)
        .map(|i| format!("f{i:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let assignments = (0..32)
        .rev()
        .map(|i| format!("value.f{i:02}={i};"))
        .collect::<String>();
    let source = format!(
        "export function main():void{{const value:{{{shape}}}={{}};{assignments}value.f31=99;console.log(Object.hasOwn(value,'f31'));console.log(JSON.stringify(value));}}"
    );
    let output = run(&source);
    assert_eq!(output.lines[0], "varta log: 1");
    let expected = (0..32)
        .rev()
        .map(|i| format!("\"f{i:02}\":{}", if i == 31 { 99 } else { i }))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(output.lines[1], format!("varta log: {{{expected}}}"));
}

#[test]
fn missing_reads_and_optional_slots_keep_roots_during_nested_allocation_and_reclamation() {
    let source=r#"
function churn():number {const piece="PIECE";for(let i=0;i<190;i=i+1){const dead:{s?:string;v:number}={v:i};dead.s=piece+piece+"\ud800";}return 1;}
function text():string{return "😀"+"!";}
function array():string[]{return ["first"+"!","second"+"!"];}
function rhs(value:{s?:string}):string{console.log(Object.hasOwn(value,"s"));churn();return "saved"+"\ud800";}
export function main():void {
 const value:{s?:string}={};const alias=value;value.s=rhs(value);
 const char=text()[churn()];const item=array()[churn()];churn();
 console.log(JSON.stringify(alias));console.log(JSON.stringify(char));console.log(item);console.log(Object.hasOwn(alias,"s"));
}"#.replace("PIECE",&"x".repeat(8192));
    let bytes = compile(&source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution
        .store
        .set_fuel(300 * varta_common::abi::FUEL)
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
            "varta log: 0",
            r#"varta log: {"s":"saved\ud800"}"#,
            r#"varta log: "\ude00""#,
            "varta log: second!",
            "varta log: 1"
        ]
    );
}
