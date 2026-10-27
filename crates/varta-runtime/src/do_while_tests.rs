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
fn do_while_executes_body_before_condition_and_tests_once_per_completed_iteration() {
    let out = run(
        "export function main():void{let n=0;const condition=():boolean=>{console.log('test');return n<3;};do{n=n+1;console.log(n);}while(condition());console.log('once');do{console.log('body');}while(false);console.log(n);}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "test", "2", "test", "3", "test", "once", "body", "3"])
    );
}
#[test]
fn do_while_continue_reaches_condition_after_finally_and_break_skips_it() {
    let out = run(
        "export function main():void{let n=0;const condition=():boolean=>{console.log('test');return n<3;};do{n=n+1;try{if(n===1){continue;}if(n===2){break;}}finally{console.log(n);}}while(condition());do{try{break;}finally{console.log('break');}}while(condition());console.log(n);}",
    );
    assert_eq!(out.lines, lines(&["1", "test", "2", "break", "2"]));
}
#[test]
fn do_while_nested_loops_and_switches_route_their_own_branches() {
    let out = run(
        "export function main():void{let outer=0;do{outer=outer+1;let inner=0;do{inner=inner+1;switch(inner){case 1:continue;default:break;}console.log(outer);break;}while(inner<3);console.log(inner);}while(outer<2);for(const item of [3,4]){do{console.log(item);break;}while(true);}}",
    );
    assert_eq!(out.lines, lines(&["1", "2", "2", "2", "3", "4"]));
}
#[test]
fn do_while_abrupt_body_return_and_throw_skip_condition() {
    let out = run(
        "function condition():boolean{console.log('BAD');return true;}function result():number{do{try{return 7;}finally{console.log('finally');}}while(condition());}export function main():void{console.log(result());try{do{throw SyntaxError('body');}while(condition());}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}}",
    );
    assert_eq!(out.lines, lines(&["finally", "7", "body"]));
}
#[test]
fn do_while_finalizer_overrides_return_with_continue_or_break() {
    let out = run(
        "function choose():number{let n=0;do{try{return 9;}finally{n=n+1;if(n<2){continue;}break;}}while(true);return n;}export function main():void{console.log(choose());let n=0;const condition=():boolean=>{console.log('test');return false;};do{try{break;}finally{n=n+1;continue;}}while(condition());console.log(n);}",
    );
    assert_eq!(out.lines, lines(&["2", "test", "1"]));
}
#[test]
fn do_while_condition_throws_after_body_finalizers_and_skips_further_iterations() {
    let out = run(
        "function condition():boolean{console.log('test');throw RangeError('condition');}export function main():void{let n=0;try{do{try{n=n+1;}finally{console.log('body finally');}}while(condition());}catch(e){if(e instanceof RangeError){console.log(e.message);}}finally{console.log(n);}try{do{throw TypeError('original');}while(condition());}catch(e){if(e instanceof TypeError){console.log(e.message);}}}",
    );
    assert_eq!(
        out.lines,
        lines(&["body finally", "test", "condition", "1", "original"])
    );
}
#[test]
fn do_while_accepts_existing_truthiness_without_implicitly_boxing_aggregates() {
    let out = run(
        "export function main():void{let n=0;do{n=n+1;}while(0);do{n=n+1;}while(-0);do{n=n+1;}while(NaN);do{n=n+1;}while('');do{n=n+1;}while(null);do{n=n+1;}while(undefined);console.log(n);let condition:string|undefined='yes';do{n=n+1;condition=undefined;}while(condition);console.log(n);}",
    );
    assert_eq!(out.lines, lines(&["6", "7"]));
}
#[test]
fn do_while_completion_and_callback_inference_preserve_guaranteed_first_return() {
    let out = run(
        "function value():number{do{return 4;}while(false);}function nested():string{do{do{return 'nested';}while(false);}while(false);}export function main():void{console.log(value());console.log(nested());console.log(JSON.stringify([1,2].map(item=>{do{return item+1;}while(false);})));console.log(JSON.stringify([1,2].map(item=>{do{if(item===1){break;}return item;}while(false);})));console.log(JSON.stringify([1].map(item=>{do{try{return item;}finally{console.log('finally');}}while(false);})));}",
    );
    assert_eq!(
        out.lines,
        lines(&["4", "nested", "[2,3]", "[null,2]", "finally", "[1]"])
    );
}
#[test]
fn do_while_body_scopes_captures_and_condition_scope_stay_separate() {
    let out = run(
        "export function main():void{let n=0;let first:()=>number=()=>99;let second:()=>number=()=>99;do{const value=n;if(n===0){first=()=>value;}else{second=()=>value;}n=n+1;}while(n<2);console.log(first());console.log(second());const value=9;do{const value=2;console.log(value);}while(value===0);console.log(value);}",
    );
    assert_eq!(out.lines, lines(&["0", "1", "2", "9"]));
    for source in [
        "export function main():void{do{const value=1;}while(value===1);}",
        "export function main():void{do{const value=1;value=2;}while(false);}",
        "function bad():number{do{break;}while(false);}export function main():void{}",
        "function bad():number{do{continue;}while(false);}export function main():void{}",
        "export function main():void{do{}while({n:1});}",
        "export function main():void{do{break missing;}while(false);}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(
            "export function main():void{let n=0;do{n=n+1;}while(n<2);console.log(n);}",
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["2"])
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{do{}while(false);}",
                profile
            )
            .is_err()
        );
    }
}
fn run_with_collection_fuel(source: &str) -> Vec<String> {
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
    execution.store.data().output.lines.clone()
}
#[test]
fn do_while_empty_body_conditions_keep_managed_argument_roots_bounded() {
    let out = run_with_collection_fuel(
        "export function main():void{let n=0;const condition=(value:{n:number}):boolean=>{n=n+1;return n<20000&&value.n===1;};do{}while(condition({n:1}));console.log(n);}",
    );
    assert_eq!(out, lines(&["20000"]));
}
#[test]
fn do_while_captures_and_saved_return_values_survive_condition_and_finalizer_collection() {
    let out = run_with_collection_fuel(
        "function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}function result():{text:string}{do{try{return {text:'returned'+'!'};}finally{churn();}}while(false);}function pair():VartaResult<string>{do{try{return varta.result.capture(varta.env.get('VARTA_DO_PAIR'));}finally{churn();}}while(false);}export function main():void{let n=0;let read:()=>string=()=>'';const condition=():boolean=>{churn();return n<2;};do{const text='captured'+'!';read=()=>text;n=n+1;}while(condition());churn();console.log(read());console.log(result().text);const saved=pair();churn();console.log(saved.status);console.log(saved.value);}",
    );
    assert_eq!(
        out,
        lines(&[
            "captured!",
            "returned!",
            "<capability denied: env> (VARTA_DO_PAIR)",
            "1",
            ""
        ])
    );
}
#[test]
fn do_while_body_and_condition_effects_remain_visible_to_helper_purity_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for statement in ["do{effect();}while(false);", "do{}while(effect());"] {
        let bytes = compile(&format!(
            "function effect():boolean{{console.log('effect');return false;}}function fake(value:boolean):boolean{{{statement}return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn do_while_fatal_condition_limits_bypass_guest_handlers() {
    let bytes = compile(
        "export function main():void{try{do{}while(true);}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
