use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V14)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn assignment_patterns_write_nested_aliases_defaults_and_rest_to_existing_locals() {
    let output = run(r#"
export function main():void{let label="";let n=0;let missing=0;let copy:{enabled?:boolean}={};const source:{name:string;child:{n:number};enabled?:boolean}={name:"item",child:{n:3},enabled:false};({name:label,child:{n},missing=7,...copy}=source);console.log(label);console.log(n);console.log(missing);console.log(JSON.stringify(copy));let first=0;let third=0;let tail:(number|undefined)[]=[];[first=1,,third=3,...tail]=[2,,4,,6];console.log(first);console.log(third);console.log(JSON.stringify(tail));[first=8,third=9,...tail]=[];console.log(first);console.log(third);console.log(JSON.stringify(tail));let array:number[]=[];({absent:array=[]}={});}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "item",
            "3",
            "7",
            "{\"enabled\":false}",
            "2",
            "4",
            "[null,6]",
            "8",
            "9",
            "[]"
        ])
    );
}
#[test]
fn assignment_patterns_evaluate_rhs_once_and_allow_duplicate_self_and_prior_targets() {
    let output = run(r#"
function source():{a?:number;b?:number}{console.log("source");return {};}
export function main():void{let a=1;let b=2;({a=a+1,b=a+1}=source());console.log(a);console.log(b);[a,a]=[4,5];console.log(a);[a,b]=[b,a];console.log(a);console.log(b);let saved=0;const before=()=>a;({a=(()=>{saved=a;return a+1;})()}={});console.log(saved);console.log(before());}
"#);
    assert_eq!(
        output.lines,
        lines(&["source", "2", "3", "5", "3", "5", "3", "4"])
    );
}
#[test]
fn assignment_patterns_keep_captured_cells_and_lexical_shadowing() {
    let output = run(r#"
export function main():void{let n=1;const read=()=>n;const write=(value:number):void=>{({n}={n:value});};write(3);console.log(read());{let n=8;({n}={n:9});console.log(n);}console.log(read());let reader:()=>number=()=>0;let setter:(value:number)=>void=(value)=>{};{let {value}={value:1};reader=()=>value;setter=(n)=>{[value]=[n];};}setter(7);console.log(reader());}
"#);
    assert_eq!(output.lines, lines(&["3", "9", "3", "7"]));
}
#[test]
fn assignment_patterns_observe_live_source_mutation_and_sticky_exhaustion() {
    let output = run(r#"
export function main():void{let a=0;let b=0;let tail:(number|undefined)[]=[];const source:number[]=[];const grow=():number=>{source.push(7,8);return 1;};[a=grow(),b=2,...tail]=source;console.log(a);console.log(b);console.log(JSON.stringify(tail));const live:number[]=[,2,3];const change=():number=>{live[1]=9;live.push(4);return 1;};[a=change(),b,...tail]=live;console.log(a);console.log(b);console.log(JSON.stringify(tail));let values:(number|undefined)[]=[1,2,3];[a,...values]=values;console.log(a);console.log(JSON.stringify(values));}
"#);
    assert_eq!(
        output.lines,
        lines(&["1", "2", "[]", "1", "9", "[3,4]", "1", "[2,3]"])
    );
}
#[test]
fn assignment_errors_preserve_prior_writes_skip_later_targets_and_run_finalizers() {
    let output = run(r#"
function fail():number{console.log("default");throw SyntaxError("failed");}
export function main():void{let a=0;let b=9;let c=8;try{[a,b,c]=[1];}catch(e){if(e instanceof TypeError){console.log(e.message);}}console.log(a);console.log(b);console.log(c);try{({a,b=fail(),c}={a:2,c:3});}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}finally{console.log(a);}console.log(b);console.log(c);try{const source:{a:number;child:{n:number}|null}={a:3,child:null};({a,child:{n:b}}=source);}catch(e){if(e instanceof TypeError){console.log(e.name);}}console.log(a);console.log(b);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "Invalid destructuring assignment value",
            "1",
            "9",
            "8",
            "default",
            "failed",
            "2",
            "9",
            "8",
            "TypeError",
            "3",
            "9"
        ])
    );
}
#[test]
fn assignment_object_rest_preserves_presence_order_and_shallow_identity() {
    let output = run(r#"
export function main():void{let removed=0;let rest:{a?:number;z:number;child:{n:number}}={z:0,child:{n:0}};const child={n:1};const source:{remove:number;a?:number;z:number;child:{n:number}}={z:2,child:child,remove:3};({remove:removed,...rest}=source);console.log(removed);console.log(JSON.stringify(rest));console.log(Object.hasOwn(rest,'a'));rest.child.n=4;rest.z=9;console.log(source.child.n);console.log(source.z);rest.a=undefined;const before=rest;({...rest}=rest);console.log(Object.hasOwn(rest,'a'));console.log(rest===before);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "3",
            "{\"z\":2,\"child\":{\"n\":1}}",
            "0",
            "4",
            "2",
            "1",
            "0"
        ])
    );
}
#[test]
fn assignment_patterns_work_in_parenthesized_statements_and_for_initializers_updates() {
    let output = run(r#"
export function main():void{let i=9;let total=0;for(([i]=[0]);i<3;([i]=[i+1])){try{if(i===1){continue;}total=total+i;}finally{console.log(i);}}console.log(total);((( {i}={i:7} )));console.log(i);}
"#);
    assert_eq!(output.lines, lines(&["0", "1", "2", "2", "7"]));
}
#[test]
fn assignment_patterns_reject_const_tdz_unknown_types_refinement_and_invalid_members() {
    for source in [
        "export function main():void{const a=1;({a}={a:2});}",
        "export function main():void{({a}={a:2});let a=1;}",
        "export function main():void{({a}={a:2});}",
        "export function main():void{let a=1;({a}={a:'bad'});}",
        "export function main():void{const target={a:1};({a:target.a}={a:'bad'});}",
        "export function main():void{let a=1;const key='a';({[key]:a}={a:2});}",
        "export function main():void{let a=1;[a]='text';}",
        "export function main():void{let a=1;const value=([a]=[2]);value.push('bad');}",
        "export function main():void{let a=1;[a]+= [2];}",
        "export function main():void{let a:number|string=1;if(typeof a==='number'){({a}={a:2});}}",
        "export function main():void{let a:number|string=1;const change=()=>{({a}={a:'bad'});};if(typeof a==='number'){console.log(a+1);}}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    let source = "export function main():void{const a=1;({a}={a:2});}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert_eq!(
        error.diagnostic().unwrap().range.start.byte_offset,
        source.find("({a}").unwrap() + 2
    );
}
#[test]
fn assignment_defaults_admit_undefined_and_contextual_empty_aggregates() {
    let output = run(r#"
export function main():void{let a=0;let b=0;[a=1,b=2]=[undefined,3];console.log(a);console.log(b);let list:number[]=[];({missing:list=[]}={});console.log(list.length);let object:{n:number}={n:0};({missing:object={n:4}}={});console.log(object.n);let union:number|string|undefined=0;({missing:union='fallback'}={});console.log(JSON.stringify(union));}
"#);
    assert_eq!(output.lines, lines(&["1", "3", "0", "4", "\"fallback\""]));
}
#[test]
fn assignment_nested_array_and_record_defaults_keep_reference_identity() {
    let output = run(r#"
export function main():void{let a=0;let b=0;let source:{n:number}|undefined=undefined;const child={n:7};const object:{child?:{n:number};values?:number[]}={child:child};({child:source,values:[a=1,b=2]=[]}=object);console.log(a);console.log(b);console.log(source===child);const empty:number[][]=[];[[a=3,b=4]=[]]=empty;console.log(a);console.log(b);}
"#);
    assert_eq!(output.lines, lines(&["1", "2", "1", "3", "4"]));
}
#[test]
fn assignment_roots_old_rhs_new_payloads_and_existing_cells_through_collection() {
    let source = r#"
function churn():number{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}return 1;}
function data():{text:string;tail:string;child:{n:number};optional?:number}{return {tail:"tail"+"!",text:"text"+"!",child:{n:7}};}
export function main():void{let text="old";let tail="old";let optional=0;let rest:{child:{n:number}}={child:{n:0}};let pair=varta.result.capture(varta.env.get("VARTA_ASSIGNMENT_OLD"));const read=()=>text+tail;const status=()=>pair.status;({text,optional=churn(),tail,...rest}=data());({pair=varta.result.capture(varta.env.get("VARTA_ASSIGNMENT_NEW"))}={});churn();console.log(read());console.log(JSON.stringify(rest));console.log(status());let first:{n:number}={n:0};let others:({n:number}|undefined)[]=[];[first=(():{n:number}=>{churn();return {n:9};})(),...others]=[, {n:2}];churn();console.log(first.n);console.log(JSON.stringify(others));}
"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(1000 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    let output = &execution.store.data().output.lines;
    assert_eq!(
        &output[output.len() - 5..],
        lines(&[
            "text!tail!",
            "{\"child\":{\"n\":7}}",
            "1",
            "9",
            "[{\"n\":2}]"
        ])
    );
}
#[test]
fn assignment_defaults_can_capture_existing_function_and_typed_this_self_bindings() {
    let output = run(r#"
export function main():void{let value=1;let read:()=>number=()=>0;({missing:read=()=>value}={});value=3;console.log(read());const empty:undefined[]=[];[read=()=>value+1]=empty;console.log(read());const fn=function self(this:{base:number},depth:number):number{let n=0;({n=this.base}={});if(depth===0){return n;}({n=self.call(this,depth-1)+n}={});return n;};console.log(fn.call({base:2},2));}
"#);
    assert_eq!(output.lines, lines(&["3", "4", "6"]));
}
#[test]
fn assignment_default_and_rhs_host_effects_cannot_hide_in_guest_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    for body in ["let n=0;({n=effect()}={});", "let n=0;({n}={n:effect()});"] {
        let bytes = compile(&format!(
            "function effect():number{{console.log('effect');return 1;}}function fake(value:boolean):boolean{{{body}return value;}}export function main():void{{varta.json.parse<number>('1');}}"
        ));
        let fake = export_index(&bytes, "_start") - 1;
        let name = varta_common::value::VALUE_ALLOCATE;
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                let at = data
                    .windows(name.len())
                    .position(|s| s == name.as_bytes())
                    .unwrap()
                    + name.len()
                    + 1;
                assert!(fake < 128 && data[at] < 128);
                data[at] = fake;
            }
            vec![data]
        });
        let error = VartaRuntime::new().validate_module(&bad).unwrap_err();
        assert!(
            error.to_string().contains("cannot call host imports"),
            "{error}"
        );
    }
}
#[test]
fn assignment_rest_copies_maximum_arrays_with_default_budgets() {
    let values = vec!["1"; varta_common::value::MAX_ARRAY_ELEMENTS].join(",");
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let element = if profile == LanguageProfile::CheckedF64Utf16V12 {
            "number"
        } else {
            "number|undefined"
        };
        let source = format!(
            "export function main():void{{const source:number[]=[{values}];let rest:({element})[]=[];[...rest]=source;console.log(rest.length);}}"
        );
        let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["1024"]),
            "{profile:?}"
        );
    }
}
#[test]
fn assignment_pattern_limits_and_legacy_profiles_are_explicit() {
    let holes = ",".repeat(varta_common::function::MAX_BINDING_PATTERN_NODES + 1);
    let source = format!("export function main():void{{[{holes}]=[];}}");
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("binding pattern budget exceeded"),
        "{error}"
    );
    let source = "export function main():void{let a=0;({a}={a:1});}";
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        let error = varta_compiler::compile_typescript_with_profile(source, profile).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("destructuring assignment requires"),
            "{error}"
        );
    }
}
#[test]
fn assignment_default_fatal_limits_bypass_guest_handlers() {
    let bytes = compile(
        "export function main():void{let a=0;try{({a=(()=>{while(true){}return 1;})()}={});console.log('BAD_BODY');}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    assert!(
        instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .is_err()
    );
    assert!(execution.store.data().output.lines.is_empty());
}
