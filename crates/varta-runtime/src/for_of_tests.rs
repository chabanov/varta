use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str, profile: LanguageProfile) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, profile).unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source, LanguageProfile::CheckedF64Utf16V14);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn for_of_evaluates_source_once_and_visits_dense_values_in_order() {
    let source = "function source():number[]{console.log('source');return [1,2,3];}export function main():void{let sum=0;for(const value of source()){sum=sum+value;}console.log(sum);const empty:number[]=[];for(const value of empty){console.log('BAD');}console.log('done');}";
    let bytes = compile(source, LanguageProfile::CheckedF64Utf16V12);
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&["source", "6", "done"])
    );
}
#[test]
fn for_of_visits_holes_as_undefined_in_sparse_profiles() {
    let out = run(
        "export function main():void{const values:(number|undefined)[]=[,1,undefined,,3];for(const value of values){console.log(value);}console.log(Object.hasOwn(values,0));console.log(Object.hasOwn(values,2));}",
    );
    assert_eq!(
        out.lines,
        lines(&["undefined", "1", "undefined", "undefined", "3", "0", "1"])
    );
}
#[test]
fn for_of_observes_live_length_backing_and_keeps_original_array_identity() {
    let out = run(
        "export function main():void{let values:number[]=[1,2];const original=values;let count=0;for(const value of values){console.log(value);count=count+1;if(count===1){values=[99];original.push(3,4);}if(count===3){original.length=3;}}console.log(count);console.log(values[0]);const shrinking:number[]=[1,2,3];for(const value of shrinking){console.log(value);shrinking.length=0;}const fill:number[]=[1,,3];let index=0;for(const value of fill){console.log(value);if(index===0){fill[1]=2;delete fill[2];}index=index+1;}}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "2", "3", "3", "99", "1", "1", "2", "undefined"])
    );
}
#[test]
fn for_of_let_and_const_create_distinct_escaping_capture_cells() {
    let out = run(
        "export function main():void{let first:()=>number|undefined=()=>99;let second:()=>number|undefined=()=>99;let count=0;for(let value of [1,2]){if(count===0){first=()=>value;}else{second=()=>value;}count=count+1;}console.log(first());console.log(second());let a:()=>number|undefined=()=>99;let b:()=>number|undefined=()=>99;count=0;for(const value of [3,4]){if(count===0){a=()=>value;}else{b=()=>value;}count=count+1;}console.log(a());console.log(b());let read:()=>number|undefined=()=>99;for(let value of [5]){read=()=>value;value=6;}console.log(read());}",
    );
    assert_eq!(out.lines, lines(&["1", "2", "3", "4", "6"]));
}
#[test]
fn for_of_binding_patterns_share_defaults_rest_and_nullish_errors() {
    let out = run(
        "export function main():void{const rows:{n?:number;extra:string}[]=[{extra:'a'},{n:2,extra:'b'}];for(const {n=7,...rest} of rows){console.log(n);console.log(JSON.stringify(rest));}const arrays:number[][]=[[1,2],[],[3]];for(let [first=9,...rest] of arrays){console.log(first);console.log(JSON.stringify(rest));}const missing:{n:number}[]=[,];try{for(const {n} of missing){console.log('BAD');}}catch(e){if(e instanceof TypeError){console.log(e.name);}}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "7",
            "{\"extra\":\"a\"}",
            "2",
            "{\"extra\":\"b\"}",
            "1",
            "[2]",
            "9",
            "[]",
            "3",
            "[]",
            "TypeError"
        ])
    );
}
#[test]
fn for_of_assignment_heads_read_step_before_evaluating_references() {
    let out = run(
        "export function main():void{let value=0;for(value of [1,2]){console.log(value);}const output={n:0};const source:number[]=[3,4];const receiver=():{n:number}=>{source[0]=99;console.log('receiver');return output;};for(receiver().n of source){console.log(output.n);}const slots:number[]=[];let index=0;for(slots[index] of [5,6]){index=index+1;}console.log(JSON.stringify(slots));const rows:{n?:number}[]=[{},{n:8}];for({n:output.n=7} of rows){console.log(output.n);}const arrays:number[][]=[[1,2],[3]];let rest:(number|undefined)[]=[];for([value=9,...rest] of arrays){console.log(value);console.log(JSON.stringify(rest));}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1", "2", "receiver", "3", "receiver", "4", "[5,6]", "7", "8", "1", "[2]", "3", "[]"
        ])
    );
}
#[test]
fn for_of_continue_break_nested_switch_and_finally_preserve_routing() {
    let out = run(
        "export function main():void{for(const value of [1,2,3,4]){try{if(typeof value==='number'){switch(value){case 1:continue;case 3:break;default:console.log(value);}}if(value===3){break;}}finally{console.log(value);}}for(const outer of [5,6]){for(const inner of [7,8]){try{if(inner===7){continue;}console.log(outer);break;}finally{console.log(inner);}}}try{for(const value of [9]){try{break;}finally{console.log('override');throw SyntaxError('stop');}}}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1", "2", "2", "3", "7", "5", "8", "7", "6", "8", "override", "stop"
        ])
    );
}
#[test]
fn for_of_returns_and_callback_inference_keep_finalizers_and_union_results() {
    let out = run(
        "function pick(values:number[]):number|undefined{for(const value of values){try{return value;}finally{console.log('finally');}}return undefined;}export function main():void{console.log(pick([4]));const empty:number[]=[];console.log(pick(empty));console.log(JSON.stringify([1,2].map(item=>{for(const value of [item]){return value;} })));}",
    );
    assert_eq!(out.lines, lines(&["finally", "4", "undefined", "[1,2]"]));
}
#[test]
fn for_of_nullable_source_throws_before_targets_and_body() {
    let out = run(
        "function missing():number[]|null{console.log('source');return null;}export function main():void{const target={n:9};const receiver=():{n:number}=>{console.log('BAD');return target;};try{for(receiver().n of missing()){console.log('BAD');}}catch(e){if(e instanceof TypeError){console.log(e.message);}}console.log(target.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&["source", "Cannot iterate null or undefined", "9"])
    );
}
#[test]
fn for_of_preserves_lexical_shadowing_and_rejects_unsupported_heads_with_spans() {
    let out = run(
        "export function main():void{const value=9;for(const value of [1]){console.log(value);}console.log(value);for(const {n} of [{n:2}]){console.log(n);}const n=8;console.log(n);}",
    );
    assert_eq!(out.lines, lines(&["1", "9", "2", "8"]));
    for source in [
        "export function main():void{for(const value of value){}}",
        "export function main():void{const value:number[]=[1];for(const value of (()=>value)()){}}",
        "export function main():void{for(const value of [1]){value=2;}}",
        "export function main():void{for(const value of [1]){}console.log(value);}",
        "export function main():void{for(var value of [1]){}}",
        "export function main():void{for(const value of 'text'){}}",
        "export function main():void{for(const value of {n:1}){}}",
        "export function main():void{const value=0;for(value of [1]){}}",
        "export function main():void{const target={n:0};for(target.n of ['bad']){}}",
        "export function main():void{for(const {n,n} of [{n:1}]){}}",
        "export function main():void{for(const value:number of [1]){}}",
        "export function main():void{for(const value in [1]){}}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{for(const value of [1]){}}",
                profile
            )
            .is_err()
        );
    }
}
#[test]
fn for_of_target_failures_preserve_prior_writes_and_skip_the_body() {
    let out = run(
        "export function main():void{let value=9;try{for(value of [1,,3]){console.log(value);}}catch(e){if(e instanceof TypeError){console.log(e.name);}}console.log(value);const output={a:0,b:9};const rows:{a:number;b?:number}[]=[{a:1,b:2},{a:3}];try{for({a:output.a,b:output.b} of rows){console.log(JSON.stringify(output));}}catch(e){if(e instanceof TypeError){console.log(e.name);}}finally{console.log(JSON.stringify(output));}} ",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "TypeError",
            "1",
            "{\"a\":1,\"b\":2}",
            "TypeError",
            "{\"a\":3,\"b\":2}"
        ])
    );
}
#[test]
fn for_of_keeps_source_current_and_escaping_iteration_cells_rooted_during_collection() {
    let source = r#"
function churn():void{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}}
function make():{text:string}[]{return [{text:"first"+"!"},{text:"second"+"!"}];}
function pick():{text:string}|undefined{for(const row of make()){try{return row;}finally{churn();}}return undefined;}
export function main():void{let first:()=>string=()=>"old";let second:()=>string=()=>"old";let index=0;for(const {text} of make()){churn();if(index===0){first=()=>text;}else{second=()=>text;}index=index+1;}churn();console.log(first());console.log(second());const result=pick();churn();console.log(JSON.stringify(result));let current=make();const original=current;for(const row of current){current=[];original.length=0;churn();console.log(JSON.stringify(row));}let pair=varta.result.capture(varta.env.get("VARTA_FOR_OF_GC"));for({missing:pair=varta.result.capture(varta.env.get("VARTA_FOR_OF_GC"))} of [{},{}]){churn();console.log(pair.status);}}
"#;
    let runtime = VartaRuntime::new();
    let module = runtime
        .validate_module(&compile(source, LanguageProfile::CheckedF64Utf16V14))
        .unwrap();
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
        lines(&[
            "first!",
            "second!",
            "{\"text\":\"first!\"}",
            "{\"text\":\"first!\"}",
            "<capability denied: env> (VARTA_FOR_OF_GC)",
            "<capability denied: env> (VARTA_FOR_OF_GC)",
            "1",
            "<capability denied: env> (VARTA_FOR_OF_GC)",
            "1"
        ])
    );
}
#[test]
fn for_of_iterates_maximum_arrays_at_default_fuel() {
    let values = vec!["1"; varta_common::value::MAX_ARRAY_ELEMENTS].join(",");
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let source = format!(
            "export function main():void{{let count=0;const values:number[]=[{values}];for(const value of values){{count=count+1;}}console.log(count);}}"
        );
        let bytes = compile(&source, profile);
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
fn for_of_receiver_source_and_body_effects_stay_visible_to_helper_purity_checks() {
    use crate::contract_tests::{export_index, rewrite};
    for statement in [
        "for(const value of source()){}",
        "for(receiver().n of [1]){}",
        "for(const {n=effect()} of [{}]){}",
        "for(const value of [1]){effect();}",
    ] {
        let bytes = compile(
            &format!(
                "function effect():number{{console.log('effect');return 1;}}function source():number[]{{return [effect()];}}function receiver():{{n:number}}{{effect();return {{n:0}};}}function fake(value:boolean):boolean{{{statement}return value;}}export function main():void{{varta.json.parse<number>('1');}}"
            ),
            LanguageProfile::CheckedF64Utf16V14,
        );
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
fn for_of_fatal_body_limits_bypass_guest_handlers() {
    let bytes = compile(
        "export function main():void{try{for(const value of [1]){console.log(value);while(true){}}}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
        LanguageProfile::CheckedF64Utf16V14,
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
    assert_eq!(execution.store.data().output.lines, lines(&["1"]));
}
#[test]
fn for_of_finalizers_capture_and_mutate_the_current_iteration_before_cleanup() {
    let out = run(
        "export function main():void{let first:()=>number|undefined=()=>99;let second:()=>number|undefined=()=>99;let count=0;for(let value of [1,2]){try{count=count+1;if(count===1){continue;}break;}finally{if(count===1){first=()=>value;value=3;}else{second=()=>value;value=4;}}}console.log(first());console.log(second());let read:()=>number=()=>0;const rows:{n?:number}[]=[{},{}];for(const {n=7,missing:local=()=>n} of rows){read=local;}console.log(read());}",
    );
    assert_eq!(out.lines, lines(&["3", "4", "7"]));
}
