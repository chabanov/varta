use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V6)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn resizable_arrays_preserve_identity_and_evaluate_arguments_before_appending() {
    let output = run(r#"
function add(v:number[],n:number):number{console.log(n);v.push(n);return n+0.5;}
function receiver(v:number[]):number[]{console.log("receiver");return v;}
export function main():void{
 const v:number[]=[];const alias=v;console.log(v.push());console.log(v.pop());
 console.log(receiver(v).push(add(v,1),add(v,2),3.5));console.log(JSON.stringify(alias));console.log(alias===v);
 console.log(alias.pop());console.log(v.length);v[0]=7.5;console.log(v[0]);console.log(v[9]);
 console.log(JSON.stringify(v));console.log(Array.isArray(v));console.log(typeof v);
}"#);
    assert_eq!(
        output.lines,
        [
            "0",
            "undefined",
            "receiver",
            "1",
            "2",
            "5",
            "[1,2,1.5,2.5,3.5]",
            "1",
            "3.5",
            "4",
            "7.5",
            "undefined",
            "[7.5,2,1.5,2.5]",
            "1",
            "object"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn resizable_arrays_pop_real_union_members_and_keep_reference_aliases() {
    let output = run(r#"
export function main():void {
 const v:(number|string|boolean|null|undefined|{n:number}|number[])[]=[];
 const object={n:1.5};const nested:number[]=[];
 console.log(v.push(-0,"😀",false,null,undefined,object,nested));
 const a=v.pop();if(Array.isArray(a)){a.push(2.5,3.5);console.log(JSON.stringify(nested));}
 const b=v.pop();if(typeof b==="object"){if(b!==null){if(!Array.isArray(b)){b.n=4.5;}}}console.log(object.n);
 console.log(v.pop());console.log(v.pop());console.log(v.pop());console.log(v.pop());
 const zero=v.pop();if(typeof zero==="number"){console.log(1/zero);}console.log(v.pop());
 const strings:string[]=[];strings.push("kept");const text=strings.pop();if(typeof text==="string"){console.log(text);}
 const refs:{n:number}[]=[];refs.push(object);console.log(refs.pop()===object);console.log(refs.pop());
 const missing:undefined[]=[];missing.push(undefined);console.log(missing.pop());console.log(missing.length);
}"#);
    assert_eq!(
        output.lines,
        [
            "7",
            "[2.5,3.5]",
            "4.5",
            "undefined",
            "null",
            "0",
            "😀",
            "-Infinity",
            "undefined",
            "kept",
            "1",
            "undefined",
            "undefined",
            "0"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn resizable_arrays_parse_nested_optional_unions_and_grow_after_json_decoding() {
    let output = run(r#"
export function main():void {
 const value=varta.json.parse<{items?:(number|string)[];rows:{n:number}[]}>('{"rows":[{"n":1.25}]}');
 console.log(Object.hasOwn(value,"items"));value.items=[];const items=value.items;
 if(Array.isArray(items)){items.push(1.5,"😀");console.log(items.pop());}
 value.rows.push({n:2.5},{n:3.5});const row=value.rows.pop();if(row!==undefined){row.n=4.5;console.log(JSON.stringify(row));}
 console.log(JSON.stringify(value));console.log(JSON.stringify(varta.json.parse<number[]>('[]').push(7.5)));
}"#);
    assert_eq!(
        output.lines,
        [
            "0",
            "😀",
            r#"{"n":4.5}"#,
            r#"{"rows":[{"n":1.25},{"n":2.5}],"items":[1.5]}"#,
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn resizable_arrays_follow_the_current_backing_after_nested_mutations() {
    let output = run(r#"
function grow(v:number[]):number{v.push(3,4,5);return 7.5;}
function index(v:number[]):number{v.push(9);return 0;}
function last(v:number[]):number{const n=v.pop();return typeof n==="number"?n:0;}
export function main():void {
 const v=[1,2];v[index(v)]=grow(v);console.log(JSON.stringify(v));
 const saved=v[0];console.log(saved);console.log(v.length);const before=v;v.push(last(v));console.log(before===v);console.log(JSON.stringify(before));
}"#);
    assert_eq!(
        output.lines,
        ["[7.5,2,9,3,4,5]", "7.5", "6", "1", "[7.5,2,9,3,4,5]"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn resizable_array_limits_and_unsupported_receivers_fail_explicitly() {
    for source in [
        "export function main():void{const v:number[]=[];v.push('wrong');}",
        "export function main():void{const v=[1];v.pop(1);}",
        "export function main():void{const v:number[]|undefined=[];v.push(1);}",
        "export function main():void{const v=[1];v.length=0;}",
        "export function main():void{const v=[1];v.push(...v);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V6
            )
            .is_err(),
            "{source}"
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V4,
        LanguageProfile::CheckedF64Utf16V5,
    ] {
        for method in ["push(1)", "pop()"] {
            let source = format!("export function main():void{{const v=[1];v.{method};}}");
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
    }
    for body in [
        "const v:number[]=[];for(let i=0;i<1025;i=i+1){v.push(i);}",
        "const v=[1];v[1]=2;",
        "const v=[1];v[-1]=2;",
    ] {
        let source = format!("export function main():void{{{body}}}");
        assert!(
            VartaRuntime::new()
                .run_wasm_bytes(&compile(&source), &[])
                .is_err()
        );
    }
    let output = run(
        "export function main():void{const v:number[]=[];for(let i=0;i<1024;i=i+1){v.push(i);}console.log(v.length);console.log(v.push());console.log(v.pop());console.log(v.push(7));console.log(v[1023]);}",
    );
    assert_eq!(
        output.lines,
        ["1024", "1024", "1023", "1024", "7"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn resizable_arrays_trace_grown_backings_and_detached_pop_results_under_collection() {
    let source = r#"
function churn():number {
 let text="x";for(let i=0;i<14;i=i+1){text=text+text;}
 for(let i=0;i<400;i=i+1){const a:string[]=[];a.push(text+"!");const b=a.pop();a.push(text+"?");}
 return 0;
}
function keep(value:{label:string},unused:number):{label:string}{return value;}
function detached(v:{label:string}[]):{label:string}{const child=v.pop();return child!==undefined?child:{label:"bad"};}
export function main():void {
 const rows:{label:string}[]=[];for(let i=0;i<42;i=i+1){rows.push({label:"kept😀"});}
 const alias=rows;const box:{data?:{label:string}[]}={data:rows};const first=rows[41];
 const saved=keep(detached(rows),churn());box.data=undefined;churn();console.log(saved.label);console.log(first===saved);
 const last=rows[40];if(last!==undefined){console.log(last.label);}console.log(alias.length);console.log(JSON.stringify(rows).length);
 const numeric:number[]=[];for(let i=0;i<42;i=i+1){numeric.push(2.775671592749e-309);}churn();console.log(numeric.length);
 const union:(string|number)[]=[];union.push("saved",1.25);console.log(union.pop());churn();console.log(union.pop());console.log(union.pop());
}"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(300 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            "kept😀",
            "1",
            "kept😀",
            "41",
            "780",
            "42",
            "1.25",
            "saved",
            "undefined"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn resizable_array_operations_and_json_preserve_system_status() {
    let output = run(r#"
export function main():void {
 const denied=varta.result.capture(varta.env.get("VARTA_VECTOR_DENIED"));
 const v:(number|string)[]=[];v.push(1.25,"😀");console.log(v.pop());console.log(varta.result.status());
 const value=varta.json.parse<{items?:number[]}>('{"items":[]}');const items=value.items;
 if(Array.isArray(items)){items.push(2.5);console.log(JSON.stringify(items));console.log(typeof items);}
 console.log(varta.result.status());console.log(denied.status);
}"#);
    assert!(
        output.lines.ends_with(
            &["😀", "1", "[2.5]", "object", "1", "1"].map(|s| format!("varta log: {s}"))
        )
    );
}

#[test]
fn resizable_array_version_substitution_and_missing_metadata_fail_before_execution() {
    use crate::contract_tests::rewrite;
    let bytes = compile(
        r#"export function main():void{const v:number[]=[];v.push(1.25);console.log(v[0]);console.log(JSON.stringify(varta.json.parse<number[]>('[2.5]')));}"#,
    );
    for name in [
        abi::SECTION,
        varta_common::value::VALUE_SECTION,
        varta_common::language::SECTION,
        varta_common::value::VALUE_ALLOCATE,
        varta_common::value::VALUE_RETAIN,
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if matches!(id, 0 | 7)
                && let Some(at) = data.windows(name.len()).position(|s| s == name.as_bytes())
            {
                data[at] = b'x';
            }
            vec![data]
        });
        assert!(VartaRuntime::new().validate_module(&bad).is_err(), "{name}");
    }
    for (from, to) in [
        (b"value_log_v3".as_slice(), b"value_log_v2".as_slice()),
        (b"json_stringify_v9", b"json_stringify_v8"),
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 2
                && let Some(at) = data.windows(from.len()).position(|s| s == from)
            {
                data[at..at + to.len()].copy_from_slice(to);
            }
            vec![data]
        });
        assert_ne!(bad, bytes);
        assert!(VartaRuntime::new().validate_module(&bad).is_err());
    }
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 0
            && let Some(at) = data
                .windows(b"checked-f64-utf16-v6".len())
                .position(|s| s == b"checked-f64-utf16-v6")
        {
            data[at + b"checked-f64-utf16-v".len()] = b'5';
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
}
