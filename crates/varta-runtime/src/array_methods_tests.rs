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
fn array_at_normalizes_relative_numbers_and_optional_indices_without_changing_bracket_reads() {
    let output = run(r#"
function read(v:number[],index:number|undefined):number|undefined{return v.at(index);}
export function main():void {
 const v=[10,20,30];console.log(v.at());console.log(v.at(undefined));console.log(v.at(0/0));console.log(v.at(1.9));
 console.log(v.at(-1));console.log(v.at(-1.9));console.log(v.at(-0.9));console.log(v.at(-0));
 console.log(v.at(1/0));console.log(v.at(-1/0));console.log(v.at(-999));console.log(v.at(3));
 console.log(read(v,undefined));console.log(read(v,-2.9));console.log(v[-1]);console.log(v[1.9]);
 const empty:number[]=[];console.log(empty.at(-1));const zero=[-0].at(-1);if(typeof zero==="number"){console.log(1/zero);}
 const units=["😀","\uD800"];console.log(JSON.stringify(units.at(-1)));
 const missing:undefined[]=[];missing.push(undefined);console.log(missing.at());console.log(missing.at(9));
}"#);
    assert_eq!(
        output.lines,
        [
            "10",
            "10",
            "10",
            "20",
            "30",
            "30",
            "10",
            "10",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "10",
            "20",
            "undefined",
            "undefined",
            "undefined",
            "-Infinity",
            r#""\ud800""#,
            "undefined",
            "undefined"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_slice_normalizes_bounds_and_always_creates_independent_arrays() {
    let output = run(r#"
function copy(v:number[],start:number|undefined,end:number|undefined):number[]{return v.slice(start,end);}
export function main():void {
 const v=[0,1,2,3];console.log(JSON.stringify(v.slice()));console.log(JSON.stringify(v.slice(1,3)));
 console.log(JSON.stringify(v.slice(-2)));console.log(JSON.stringify(v.slice(-99,99)));console.log(JSON.stringify(v.slice(3,1)));
 console.log(JSON.stringify(v.slice(1.9,3.9)));console.log(JSON.stringify(v.slice(-1.9)));console.log(JSON.stringify(v.slice(-0.9)));
 console.log(JSON.stringify(v.slice(0/0)));console.log(JSON.stringify(v.slice(undefined,0/0)));console.log(JSON.stringify(v.slice(undefined,undefined)));
 console.log(JSON.stringify(v.slice(-1/0,1/0)));console.log(JSON.stringify(v.slice(1/0)));console.log(JSON.stringify(v.slice(0,-1/0)));
 console.log(JSON.stringify(copy(v,undefined,undefined)));console.log(JSON.stringify(copy(v,1,undefined)));console.log(JSON.stringify(copy(v,undefined,0)));
 const all=v.slice();console.log(all===v);all[0]=9;all.push(4);console.log(JSON.stringify(all));console.log(JSON.stringify(v));
 const a=v.slice(0,0);const b=v.slice(0,0);console.log(a===b);a.push(1);console.log(b.length);
 const empty:number[]=[];console.log(empty.slice()===empty);console.log(JSON.stringify(empty.slice(-1/0,1/0)));
}"#);
    assert_eq!(
        output.lines,
        [
            "[0,1,2,3]",
            "[1,2]",
            "[2,3]",
            "[0,1,2,3]",
            "[]",
            "[1,2]",
            "[3]",
            "[0,1,2,3]",
            "[0,1,2,3]",
            "[]",
            "[0,1,2,3]",
            "[0,1,2,3]",
            "[]",
            "[]",
            "[0,1,2,3]",
            "[1,2,3]",
            "[]",
            "0",
            "[9,1,2,3,4]",
            "[0,1,2,3]",
            "0",
            "0",
            "0",
            "[]"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_methods_preserve_shallow_reference_identity_and_union_values() {
    let output = run(r#"
export function main():void {
 const object={n:1.25};const child=[1];const values:(number|string|boolean|null|undefined|{n:number}|number[])[]=[object,child,-0,"😀",false,null,undefined];
 const copy=values.slice();console.log(copy===values);const row=copy.at(0);console.log(row===object);
 if(typeof row==="object"){if(row!==null){if(!Array.isArray(row)){row.n=2.5;}}}console.log(object.n);
 const nested=copy.at(1);if(Array.isArray(nested)){nested.push(3);}console.log(JSON.stringify(child));
 copy[0]=7;console.log(values.at(0)===object);copy.pop();console.log(copy.length);console.log(values.length);
 console.log(JSON.stringify(copy));console.log(JSON.stringify(values.slice(3)));
 const zero=copy.at(2);if(typeof zero==="number"){console.log(1/zero);}
 const owner=varta.json.parse<{rows?:{n:number}[]}>('{"rows":[{"n":4.5}]}');const rows=owner.rows;
 if(Array.isArray(rows)){const clones=rows.slice();const first=clones.at(0);if(first!==undefined){first.n=5.5;}console.log(JSON.stringify(rows));}
}"#);
    assert_eq!(
        output.lines,
        [
            "0",
            "1",
            "2.5",
            "[1,3]",
            "1",
            "6",
            "7",
            r#"[7,[1,3],0,"😀",false,null]"#,
            r#"["😀",false,null,null]"#,
            "-Infinity",
            r#"[{"n":5.5}]"#
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_methods_evaluate_receivers_and_bounds_once_before_selecting_current_storage() {
    let output = run(r#"
function receiver(v:number[]):number[]{console.log("receiver");return v;}
function begin(v:number[]):number|undefined{console.log("start");v.push(3);return -2;}
function end(v:number[]):number|undefined{console.log("end");v.push(4);return undefined;}
function index(v:number[]):number{console.log("index");v.pop();return -1;}
export function main():void {
 const v=[1,2];const copied=receiver(v).slice(begin(v),end(v));console.log(JSON.stringify(copied));console.log(JSON.stringify(v));
 console.log(receiver(v).at(index(v)));console.log(JSON.stringify(v));
 console.log(JSON.stringify(receiver(v).slice(void v.push(5),void v.push(6))));
 console.log(receiver(v).at(void v.pop()));
}"#);
    assert_eq!(
        output.lines,
        [
            "receiver",
            "start",
            "end",
            "[3,4]",
            "[1,2,3,4]",
            "receiver",
            "index",
            "3",
            "[1,2,3]",
            "receiver",
            "[1,2,3,5,6]",
            "receiver",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_methods_reject_coercion_spreads_wrong_arity_and_unnarrowed_receivers() {
    for body in [
        "const v=[1];v.at('0');",
        "const v=[1];v.at(null);",
        "const v=[1];v.slice(true);",
        "const v=[1];v.slice(0,'1');",
        "const v=[1];v.at(0,1);",
        "const v=[1];v.slice(0,1,2);",
        "const v=[1];v.slice(...v);",
        "const v:number[]|undefined=[];v.at(0);",
        "const v=[1];const method=v.slice;",
        "'text'.at(0);",
        "const n:number|string=1;[1].at(n);",
    ] {
        let source = format!("export function main():void{{{body}}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V6
            )
            .is_err(),
            "{body}"
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V4,
        LanguageProfile::CheckedF64Utf16V5,
    ] {
        for method in ["at(-1)", "slice()"] {
            let source = format!("export function main():void{{const v=[1];v.{method};}}");
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
    }
    let output = run(
        "export function main():void{const v:number[]=[];for(let i=0;i<1024;i=i+1){v.push(i);}const copy=v.slice(-1/0,1/0);console.log(copy.length);console.log(copy.at(-1));console.log(v.at(1e300));console.log(JSON.stringify(v.slice(-1e300,-1e300)));}",
    );
    assert_eq!(
        output.lines,
        ["1024", "1023", "undefined", "[]"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_methods_keep_sliced_children_and_temporaries_alive_through_collection() {
    let source = r#"
function churn():number {
 let text="x";for(let i=0;i<14;i=i+1){text=text+text;}
 for(let i=0;i<400;i=i+1){const values=[text+"!",text+"?"];const copied=values.slice(0);copied.pop();}return 0;
}
function keep(v:{label:string}[],unused:number):{label:string}[]{return v;}
function begin():number|undefined{churn();return undefined;}
export function main():void {
 const rows:{label:string}[]=[];for(let i=0;i<42;i=i+1){rows.push({label:"kept😀"});}
 const copy=keep(rows.slice(1,41),churn());for(let i=0;i<42;i=i+1){rows.pop();}
 const last=copy.at(-1);copy.pop();churn();if(last!==undefined){console.log(last.label);}
 console.log(copy.length);const first=copy.at(0);if(first!==undefined){console.log(first.label);}
 const next=copy.slice(begin(),begin());console.log(next.length);console.log(next.at(-1)===copy.at(-1));console.log(rows.length);
 const numeric:number[]=[];for(let i=0;i<42;i=i+1){numeric.push(2.775671592749e-309);}const numbers=numeric.slice();churn();console.log(numbers.length);
 const union:(number|string)[]=[1.25,"saved"];const values=union.slice();union.pop();churn();console.log(values.at(-1));
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
        ["kept😀", "39", "kept😀", "39", "1", "0", "42", "saved"]
            .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_methods_preserve_denied_status_across_allocating_operations() {
    let output = run(r#"
export function main():void {
 const denied=varta.result.capture(varta.env.get("VARTA_ARRAY_METHOD_DENIED"));
 const v:(number|string)[]=[];v.push(1.25,"😀");const copied=v.slice();console.log(copied.at(-1));console.log(varta.result.status());
 const a="a"+"b";console.log(a==="ab");v[0]=2.5;console.log(JSON.stringify(v));console.log(varta.result.status());console.log(denied.status);
}"#);
    assert!(output.lines.ends_with(
        &["😀", "1", "1", r#"[2.5,"😀"]"#, "1", "1"].map(|s| format!("varta log: {s}"))
    ));
}
