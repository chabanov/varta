use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V4)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}

#[test]
fn reference_unions_preserve_record_identity_aliases_and_nullable_indexing() {
    let output = run(r#"
function keep(v:{n:number;label?:string}|undefined):{n:number;label?:string}|undefined{return v;}
export function main():void {
 const object:{n:number;label?:string}={n:1.5};const alias=keep(object);const same=keep(object);
 console.log(alias===same);console.log(alias===object);const other:{n:number;label?:string}={n:1.5};console.log(alias===other);
 if(alias!==undefined){alias.n=2.5;alias.label="😀";}
 console.log(JSON.stringify(object));console.log(JSON.stringify(alias));
 const values=[object];const found=values[0];const absent=values[1];console.log(typeof found);console.log(absent);
 if(typeof found==="object"){console.log(found.n);console.log(Object.hasOwn(found,"label"));}
 console.log(absent??object);console.log(!!found);console.log(!absent);
}"#);
    assert_eq!(
        output.lines,
        [
            "1",
            "1",
            "0",
            r#"{"n":2.5,"label":"😀"}"#,
            r#"{"n":2.5,"label":"😀"}"#,
            "object",
            "undefined",
            "2.5",
            "1",
            r#"{"n":2.5,"label":"😀"}"#,
            "1",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn reference_unions_distinguish_null_records_and_arrays_with_runtime_guards() {
    let output = run(r#"
function label(v:{n:number}|number[]|null|undefined):string {
 if(Array.isArray(v)){return JSON.stringify(v);}
 else {if(typeof v==="object"){if(v!==null){return JSON.stringify(v.n);}else{return "null";}}else{return "missing";}}
}
export function main():void {
 console.log(label({n:1.5}));console.log(label([1.5,2.5]));console.log(label(null));console.log(label(undefined));
 let v:{n:number}|number[]|null|undefined=[1.5];console.log(Array.isArray(v));
 if(Array.isArray(v)){v[0]=2.5;const first=v[0];if(typeof first==="number"){console.log(first+0.5);}}
 v={n:3.5};if(!Array.isArray(v)){if(v!==null){if(v!==undefined){console.log(v.n);}}}
 console.log(Array.isArray([1]));console.log(Array.isArray({n:1}));console.log(Array.isArray(null));
 const primitive:number|string=1;console.log(Array.isArray(primitive));
 const nested=[[1.5]];const inner=nested[0];if(Array.isArray(inner)){console.log(inner[0]);}console.log(nested[9]);
}"#);
    assert_eq!(
        output.lines,
        [
            "1.5",
            "[1.5,2.5]",
            "null",
            "missing",
            "1",
            "3",
            "3.5",
            "1",
            "0",
            "0",
            "0",
            "1.5",
            "undefined"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn reference_union_contexts_build_optional_members_and_serialize_nested_values() {
    let output = run(r#"
function create(flag:boolean):{n?:number}|undefined {return flag?{}:undefined;}
export function main():void {
 const value:{child?:{n?:number};items?:number[]}={};console.log(JSON.stringify(value));
 value.child={};value.items=[1.5];console.log(JSON.stringify(value));
 const child=value.child;if(typeof child==="object"){child.n=2.5;}
 console.log(JSON.stringify(value));value.child=undefined;console.log(Object.hasOwn(value,"child"));console.log(JSON.stringify(value));
 const created=create(true);console.log(JSON.stringify(created));console.log(create(false));
 const list:({n:number}|number[]|undefined)[]=[{n:1.5},[2.5],undefined];console.log(JSON.stringify(list));
 const item=list[0];if(typeof item==="object"){if(!Array.isArray(item)){console.log(item.n);}}
 const absent=list[9];console.log(JSON.stringify(absent));
}

"#);
    assert_eq!(
        output.lines,
        [
            "{}",
            r#"{"child":{},"items":[1.5]}"#,
            r#"{"child":{"n":2.5},"items":[1.5]}"#,
            "1",
            r#"{"items":[1.5]}"#,
            "{}",
            "undefined",
            r#"[{"n":1.5},[2.5],null]"#,
            "1.5",
            "undefined"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn reference_union_operators_keep_operand_order_short_circuit_and_identity() {
    let output = run(r#"
function value(v:{n:number}|number[]|undefined):{n:number}|number[]|undefined {console.log("left");return v;}
function fallback():{n:number}|number[]|undefined {console.log("right");return {n:2.5};}
function nullValue():null {console.log("null");return null;}
export function main():void {
 console.log(JSON.stringify(value([])||fallback()));console.log(JSON.stringify(value([])&&fallback()));
 console.log(JSON.stringify(value(undefined)??fallback()));console.log(JSON.stringify(value([])??fallback()));
 console.log(value(undefined)==nullValue());console.log(nullValue()!=value({n:1.5}));
 console.log(Array.isArray(value([])));console.log(Array.isArray(value({n:1.5})));
 const a:number[]=[];const b:number[]=[];const x:number[]|undefined=a;const y:number[]|undefined=a;const z:number[]|undefined=b;
 console.log(x===y);console.log(x===z);console.log(!x);console.log(x===a);
}"#);
    assert_eq!(
        output.lines,
        [
            "left",
            "[]",
            "left",
            "right",
            r#"{"n":2.5}"#,
            "left",
            "right",
            r#"{"n":2.5}"#,
            "left",
            "[]",
            "left",
            "null",
            "1",
            "null",
            "left",
            "1",
            "left",
            "1",
            "left",
            "0",
            "1",
            "0",
            "0",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn reference_union_rejections_keep_shapes_and_narrowing_sound_and_legacy_frozen() {
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf8V1,
        LanguageProfile::CheckedF64Utf8V2,
        LanguageProfile::CheckedF64Utf16V1,
        LanguageProfile::CheckedF64Utf16V2,
        LanguageProfile::CheckedF64Utf16V3,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{const x:{n:number}|undefined=undefined;}",
                profile
            )
            .is_err()
        );
    }
    for source in [
        "export function main():void{const x:{n:number}|{s:string}|undefined=undefined;}",
        "export function main():void{const x:number[]|string[]|undefined=undefined;}",
        "export function main():void{const x:{n:number}|undefined={s:1};}",
        "export function main():void{const x:{n:number}|undefined={n:1};console.log(x.n);}",
        "export function main():void{const x:{n:number}|null={n:1};if(typeof x==='object'){console.log(x.n);}}",
        "export function main():void{const x:{n:number}|number[]|undefined=undefined;if(typeof x==='object'){console.log(x.n);}}",
        "export function main():void{let x:{n:number}|undefined={n:1};if(x!==undefined){x=undefined;}}",
        "export function main():void{const Array={};Array.isArray([]);}",
        "export function main():void{varta.json.parse<{n:number}|undefined>('{}');}",
        "export function main():void{varta.json.parse<{child?:{n:number}}>('{\"child\":{\"n\":1}}');}",
        "export function main():void{const x:{n:number}|undefined={n:1};console.log(x==1);}",
        "export function main():void{const x:{n:number}|undefined={n:1};varta.env.get(x);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V4
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn reference_union_versions_and_legacy_intrinsics_are_rejected_before_effects() {
    use crate::contract_tests::rewrite;
    use varta_common::{abi, value};
    let bytes = compile(
        "export function main():void{const v:{n:number}|undefined={n:1};console.log(typeof v);console.log(JSON.stringify(v));}",
    );
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.inspect_module(&bytes).unwrap().value_abi_version,
        Some(value::REFERENCE_VALUE_VERSION)
    );
    for (name, version) in [
        (abi::SECTION, abi::MISSING_VERSION),
        (value::VALUE_SECTION, value::MISSING_VALUE_VERSION),
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
            let from = b"json_stringify_v8";
            let at = data.windows(from.len()).position(|s| s == from).unwrap();
            data[at..at + from.len()].copy_from_slice(b"json_stringify_v7");
        }
        vec![data]
    });
    assert!(runtime.validate_module(&bad).is_err());
    // The old pure union imports cannot be substituted even when their physical signatures match.
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 2 {
            let from = b"value_typeof_v2";
            let to = b"union_typeof";
            let at = data.windows(from.len()).position(|s| s == from).unwrap();
            data[at - 1] = to.len() as u8;
            data.splice(at..at + from.len(), to.iter().copied());
        }
        vec![data]
    });
    assert!(runtime.validate_module(&bad).is_err());
}

#[test]
fn reference_union_gc_retains_nested_graphs_and_only_traces_actual_reference_payloads() {
    let rows = (0..42)
        .map(|n| format!("{{n:{n},label:\"saved\"+\"{n}\"}}"))
        .collect::<Vec<_>>()
        .join(",");
    let source=r#"
function create():{label:string;items:string[]}|undefined{return {label:"root"+"!",items:["child"+"!"]};}
function churn():number {const piece="PIECE";for(let i=0;i<190;i=i+1){const dead:{text:string}|undefined={text:piece+piece+"\ud800"};}return 0;}
function keep(v:{label:string;items:string[]}|undefined,n:number):{label:string;items:string[]}|undefined{return v;}
export function main():void {
 const held=keep(create(),churn());const owner:{child?:{label:string;items:string[]}}={};owner.child=create();
 const detached=owner.child;owner.child=undefined;
 const rows:({n:number;label:string}|undefined)[]=[ROWS];const fake:number|{n:number;label:string}|undefined=FAKE;
 churn();churn();const last=rows[41];if(typeof last==="object"){console.log(last.label);}
 if(held!==undefined){console.log(held.label);console.log(JSON.stringify(held.items));}
 if(typeof detached==="object"){console.log(detached.label);}
 console.log(fake===FAKE);console.log(JSON.stringify(owner));console.log(Object.hasOwn(owner,"child"));
}"#.replace("PIECE",&"x".repeat(8192)).replace("ROWS",&rows).replace("FAKE",&format!("{:?}",f64::from_bits(varta_common::abi::OWNED_STRING_BASE as u64)));
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
            "varta log: saved41",
            "varta log: root!",
            r#"varta log: ["child!"]"#,
            "varta log: root!",
            "varta log: 1",
            "varta log: {}",
            "varta log: 1"
        ]
    );
}
