//! Flow facts follow actual completion targets, including finalizer overrides.
use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V24)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn reject(body: &str) {
    let source = format!("export function main():void{{{body}}}");
    assert!(
        varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V24
        )
        .is_err(),
        "{source}"
    );
    compile(&source.replace("value.length", "0"));
}
#[test]
fn exit_flow_infinite_loop_breaks_prove_the_exit_type() {
    run(
        "export function main():void{let value:string|number=0;while(true){value='abc';break;}console.log(value.length);for(;;){value=2;break;}console.log(value.toFixed(1));}",
        &["3", "2.0"],
    );
}
#[test]
fn exit_flow_while_false_guard_describes_normal_exhaustion() {
    run(
        "function missing(value:undefined):number{return 7;}export function main():void{let value:{n:number}|undefined={n:1};while(value){console.log(value.n);value=undefined;}console.log(missing(value));}",
        &["1", "7"],
    );
}
#[test]
fn exit_flow_breaks_join_with_the_false_test_edge() {
    reject(
        "let value:{n:number}|undefined={n:1};const flag=true;while(value){if(flag)break;value=undefined;}console.log(value.length);",
    );
    // Use a receiver valid for the reference member, so rejection really tests
    // the possible Undefined exit rather than a missing record field.
    let source = "export function main():void{let value:{n:number}|undefined={n:1};const flag=true;while(value){if(flag)break;value=undefined;}console.log(value.n);}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V24
        )
        .is_err()
    );
    compile(&source.replace("console.log(value.n)", "console.log(Boolean(value))"));
}
#[test]
fn exit_flow_for_updates_join_fallthrough_and_continue_values() {
    run(
        "export function main():void{let value:string|number=0;for(let i=0;i<2;console.log(value.length)){i++;if(i===1){value='a';continue;}value='bb';}}",
        &["1", "2"],
    );
}
#[test]
fn exit_flow_do_while_test_joins_continues_and_preserves_exit_value() {
    run(
        "export function main():void{let value:string|number=0;let i=0;do{value='a';i++;if(i<2)continue;value='bb';}while(value.length<2);console.log(value.length);}",
        &["2"],
    );
}
#[test]
fn exit_flow_constant_false_tests_do_not_invent_backedges() {
    run(
        "export function main():void{let value:string|number=0;value='abc';do{console.log(value.length);value=2;}while(false);console.log(value.toFixed(0));value='x';while(false){value=3;}console.log(value.length);for(value='ab';false;value=4){value=5;}console.log(value.length);}",
        &["3", "2", "1", "2"],
    );
}
#[test]
fn exit_flow_for_initializer_facts_cannot_leak_across_repeated_mutations() {
    reject("let value:string|number=0;for(value='a';value.length>0;value=1){console.log('body');}");
    reject("let value:string|number=0;for(value='a';value.length>0;){value=1;}");
    run(
        "export function main():void{let value:string|number=0;for(value='a';typeof value==='string'&&value.length>0;value=1){console.log(value.length);}console.log(typeof value);}",
        &["1", "number"],
    );
}
#[test]
fn exit_flow_labels_join_named_breaks_with_fallthrough() {
    run(
        "function f(flag:boolean):void{let value:string|number=0;out:{if(flag){value='a';break out;}value='bb';}console.log(value.length);}export function main():void{f(true);f(false);}",
        &["1", "2"],
    );
}
#[test]
fn exit_flow_nested_loops_route_named_continue_to_the_outer_update() {
    run(
        "export function main():void{let value:string|number=0;outer:alias:for(let i=0;i<2;console.log(value.length)){i++;for(let j=0;j<2;j++){value='x';continue outer;}value='tail';}}",
        &["1", "1"],
    );
}
#[test]
fn exit_flow_switch_normal_exits_join_breaks_and_last_fallthrough() {
    run(
        "function f(key:number):void{let value:string|number=0;switch(key){case 1:value='a';break;case 2:value='bb';break;default:value='ccc';}console.log(value.length);}export function main():void{f(1);f(2);f(3);}",
        &["1", "2", "3"],
    );
    reject(
        "let value:string|number=0;switch(1){case 1:value='a';break;}console.log(value.length);",
    );
}
#[test]
fn exit_flow_switch_continue_targets_the_surrounding_loop() {
    run(
        "export function main():void{let value:string|number=0;for(let i=0;i<2;console.log(value.length)){i++;switch(i){case 1:value='a';continue;default:value='bb';break;}console.log(value.length);}}",
        &["1", "2", "2"],
    );
}
#[test]
fn exit_flow_finalizer_writes_transform_pending_break_facts() {
    reject(
        "let value:string|number=0;while(true){try{value='a';break;}finally{value=2;}}console.log(value.length);",
    );
    run(
        "export function main():void{let value:string|number=0;while(true){try{value=2;break;}finally{value='done';}}console.log(value.length);}",
        &["4"],
    );
}
#[test]
fn exit_flow_finalizer_writes_transform_pending_continue_facts() {
    reject(
        "let value:string|number=0;for(let i=0;i<2;console.log(value.length)){i++;try{value='a';continue;}finally{value=2;}}",
    );
    run(
        "export function main():void{let value:string|number=0;for(let i=0;i<2;console.log(value.toFixed(0))){i++;try{value='a';continue;}finally{value=2;}}}",
        &["2", "2"],
    );
}
#[test]
fn exit_flow_finalizer_break_overrides_break_and_continue_completions() {
    run(
        "export function main():void{let value:string|number=0;while(true){try{value=1;break;}finally{value='abc';break;}}console.log(value.length);while(true){try{value=1;continue;}finally{value='x';break;}}console.log(value.length);}",
        &["3", "1"],
    );
}
#[test]
fn exit_flow_finalizer_return_keeps_only_surviving_normal_exit_facts() {
    run(
        "function f(flag:boolean):number{let value:string|number=0;out:{try{value=1;break out;}finally{if(flag)return 0;value='abc';}}return value.length;}export function main():void{console.log(f(true));console.log(f(false));}",
        &["0", "3"],
    );
}
#[test]
fn exit_flow_nested_finalizers_transform_exits_in_unwind_order() {
    run(
        "export function main():void{let value:string|number=0;while(true){try{try{value='a';break;}finally{value='bb';}}finally{value=2;}}console.log(value.toFixed(1));}",
        &["2.0"],
    );
}
#[test]
fn exit_flow_finalizer_throw_cancels_a_pending_named_break() {
    run(
        "export function main():void{let value:string|number=0;out:{try{while(true){try{value=1;break out;}finally{throw 'stop';}}}catch(e){value='caught';}}console.log(value.length);}",
        &["6"],
    );
}
#[test]
fn exit_flow_unreachable_writes_and_breaks_do_not_poison_exit_facts() {
    run(
        "export function main():void{let value:string|number=0;while(true){value='abc';break;value=2;break;}console.log(value.length);out:{if(false){value=2;break out;}value='x';}console.log(value.length);}",
        &["3", "1"],
    );
}
#[test]
fn exit_flow_for_of_keeps_the_empty_input_exit() {
    reject(
        "let value:string|number=0;const items:number[]=[];for(const item of items){value='a';break;}console.log(value.length);",
    );
    run(
        "export function main():void{let value:string|number=0;const items=[1,2];for(const item of items){value='a';if(item===1)continue;console.log(value.length);}}",
        &["1"],
    );
}
#[test]
fn exit_flow_unknown_finalizer_calls_invalidate_captured_exit_facts() {
    reject(
        "let value:string|number=0;const change=():void=>{value=2;};while(true){try{value='a';break;}finally{change();}}console.log(value.length);",
    );
    run(
        "export function main():void{let value:string|number=0;const change=():void=>{value=2;};while(true){try{value='a';break;}finally{change();}}if(typeof value==='number'){console.log(value.toFixed(0));}}",
        &["2"],
    );
}
#[test]
fn exit_flow_native_finalizer_calls_preserve_pending_exit_facts() {
    run(
        "export function main():void{let value:string|number=0;while(true){try{value='abc';break;}finally{console.log(Math.abs(-1));}}console.log(value.length);}",
        &["1", "3"],
    );
}
#[test]
fn exit_flow_earlier_profiles_keep_readonly_guard_and_exit_rules() {
    let source = "export function main():void{let value:string|number=0;while(true){value='a';break;}console.log(value.length);}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V23
        )
        .is_err()
    );
    compile(source);
}
#[test]
fn exit_flow_finalizers_do_not_rewrite_exits_from_an_earlier_sibling_branch() {
    run(
        "function f(flag:boolean):void{let value:string|number=0;out:{if(flag){value='a';break out;}try{value=2;break out;}finally{value='bb';}}console.log(value.length);}export function main():void{f(true);f(false);}",
        &["1", "2"],
    );
}
#[test]
fn exit_flow_pending_reference_values_survive_collection_during_finally() {
    use crate::execution::Execution;
    let source = "function churn():void{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}}export function main():void{let value:{text:string}|undefined=undefined;while(true){try{value={text:'root'+'!'};break;}finally{churn();}}console.log(value.text);let text:string|number=0;out:{try{text='saved'+'!';break out;}finally{churn();}}console.log(text.length);}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
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
        vec!["varta log: root!", "varta log: 6"]
    );
}
#[test]
fn exit_flow_literal_analysis_obeys_existing_expression_depth_limit() {
    for statement in [
        format!("while({}true){{break;}}", "!".repeat(128)),
        format!("for(;{}true;){{break;}}", "!".repeat(128)),
        format!("do{{break;}}while({}true);", "!".repeat(128)),
    ] {
        let source = format!("export function main():void{{{statement}}}");
        let error = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V24,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("nesting exceeds 64"), "{error}");
    }
}
