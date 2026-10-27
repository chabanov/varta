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
fn assignment_expressions_return_the_original_rhs_identity_type_fields_and_holes() {
    let output = run(r#"
export function main():void{let n=0;const source={n:2,extra:"kept"};const result=({n}=source);console.log(result===source);console.log(JSON.stringify(result));console.log(n);let first=0;let rest:(number|undefined)[]=[];const values:number[]=[,2,3];const original=([first=7,...rest]=values);console.log(original===values);console.log(Object.hasOwn(original,0));console.log(Object.hasOwn(rest,0));console.log(JSON.stringify(original));console.log(first);console.log(JSON.stringify(rest));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1",
            "{\"n\":2,\"extra\":\"kept\"}",
            "2",
            "1",
            "0",
            "1",
            "[null,2,3]",
            "7",
            "[2,3]"
        ])
    );
}
#[test]
fn assignment_values_chain_once_and_keep_expected_aggregate_context() {
    let output = run(r#"
function source():{n:number}{console.log("rhs");return {n:3};}
export function main():void{let a=0;let b=0;const result:{n:number}=({n:a}=({n:b}=source()));console.log(a);console.log(b);console.log(JSON.stringify(result));let first=0;const typed:number[]=([first]=[4,5]);console.log(JSON.stringify(typed));console.log(first);const empty:number[]=([first=9]=[]);console.log(empty.length);console.log(first);}
"#);
    assert_eq!(
        output.lines,
        lines(&["rhs", "3", "3", "{\"n\":3}", "[4,5]", "4", "0", "9"])
    );
}
#[test]
fn assignment_values_work_as_arguments_members_operators_and_short_circuit_branches() {
    let output = run(r#"
function take(value:{n:number},after:number):number{return value.n+after;}
function later():number{console.log("later");return 4;}
export function main():void{let a=0;console.log(take(({n:a}={n:2}),later()));console.log(a);console.log((({n:a}={n:5})).n+1);const chosen=false?({n:a}={n:8}):({n:a}={n:9});console.log(chosen.n);console.log(a);const skipped=false&&((({n:a}={n:20})).n===20);console.log(a);let b=0;const original=({a=([b]=[4]).length}={});console.log(a);console.log(b);console.log(JSON.stringify(original));}
"#);
    assert_eq!(
        output.lines,
        lines(&["later", "6", "2", "6", "9", "9", "9", "1", "4", "{}"])
    );
}
#[test]
fn assignment_value_throws_skip_enclosing_calls_and_keep_inner_partial_writes() {
    let output = run(r#"
function bad(value:number[]):void{console.log("BAD_CALL");}
function source():number[]{console.log("rhs");return [1];}
export function main():void{let a=0;let b=9;let outer=8;try{bad(([outer]=([a,b]=source())));}catch(e){if(e instanceof TypeError){console.log(e.name);}}finally{console.log(a);console.log(b);console.log(outer);}let n=0;const empty:{n?:number}={};const result=({n=2}=empty);console.log(result===empty);console.log(Object.hasOwn(result,'n'));console.log(n);}
"#);
    assert_eq!(
        output.lines,
        lines(&["rhs", "TypeError", "1", "9", "8", "1", "0", "2"])
    );
}
#[test]
fn assignment_expression_returns_survive_finalizers_and_callback_inference() {
    let output = run(r#"
function result(source:{n:number}):{n:number}{let n=0;try{return ({n}=source);}finally{source.n=9;}}
export function main():void{const source={n:2};console.log(result(source)===source);console.log(source.n);let assigned=0;console.log(JSON.stringify([1,2].map(value=>{return ({n:assigned}={n:value});})));console.log(assigned);console.log(JSON.stringify([1].map(value=>{try{return ([assigned]=[value]);}finally{console.log(assigned);}})));}
"#);
    assert_eq!(
        output.lines,
        lines(&["1", "9", "[{\"n\":1},{\"n\":2}]", "2", "1", "[[1]]"])
    );
}
#[test]
fn assignment_expression_floors_keep_earlier_operands_arguments_pairs_and_results_rooted() {
    let source = r#"
function churn():number{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}return 1;}
function keep(first:{text:string},pair:VartaResult<string>,last:{n?:number}):string{churn();console.log(pair.status);return first.text+JSON.stringify(last);}
function context():{n?:number}{churn();return {};}
export function main():void{let n=0;console.log(keep({text:"kept"+"!"},varta.result.capture(varta.env.get("VARTA_ASSIGN_VALUE")),({n=churn()}=context())));console.log(n);console.log(("prefix"+"!")+JSON.stringify(({n=churn()}={})));let other=0;const result=({n=other}=({n:other=churn()}=context()));churn();console.log(result.n);console.log(n);console.log(other);}
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
        &output[output.len() - 7..],
        lines(&["1", "kept!{}", "1", "prefix!{}", "undefined", "1", "1"])
    );
}
#[test]
fn assignment_expression_result_is_rooted_through_later_argument_collection_and_finally() {
    let source = r#"
function churn():number{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}return 1;}
function temporary():{n:number;text:string}{return {n:3,text:"original"+"!"};}
function keep(value:{n:number;text:string},unused:number):string{return value.text;}
function result():{n:number;text:string}{let n=0;try{return ({n}=temporary());}finally{churn();}}
export function main():void{let n=0;console.log(keep(({n}=temporary()),churn()));console.log(result().text);console.log(n);}
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
    assert_eq!(
        execution.store.data().output.lines,
        lines(&["original!", "original!", "3"])
    );
}
#[test]
fn assignment_expression_host_effects_in_rhs_and_traversal_stay_visible_to_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for expression in ["({n}=effect())", "({n=observe()}={})"] {
        let bytes = compile(&format!(
            "function observe():number{{console.log('effect');return 1;}}function effect():{{n:number}}{{return {{n:observe()}};}}function fake(value:boolean):boolean{{let n=0;const original={expression};return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn assignment_expression_type_mutability_and_member_boundaries_keep_real_diagnostics() {
    for source in [
        "export function main():void{const n=0;const result=({n}={n:1});}",
        "export function main():void{const result=({n}={n:1});let n=0;}",
        "export function main():void{let n=0;const target={n:0};const result=({n:target.n}={n:'bad'});}",
        "export function main():void{let n=0;const result:number[]=([n]=['bad']);}",
        "export function main():void{let n=0;const result=({n}={n:1});result.missing=1;}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    let source = "export function main():void{let n=0;const result=({n}={n:1});}";
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
    }
}
#[test]
fn assignment_value_keeps_maximum_rhs_array_and_dense_rest_distinct_at_default_limits() {
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
            "export function main():void{{const source:number[]=[{values}];let rest:({element})[]=[];const original=([...rest]=source);console.log(original===source);console.log(original.length);console.log(rest.length);}}"
        );
        let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["1", "1024", "1024"]),
            "{profile:?}"
        );
    }
}
#[test]
fn assignment_expression_fatal_defaults_bypass_enclosing_calls_and_handlers() {
    let bytes = compile(
        "function bad(value:{}):void{console.log('BAD_CALL');}export function main():void{let n=0;try{bad(({n=(()=>{while(true){}return 1;})()}={}));}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
