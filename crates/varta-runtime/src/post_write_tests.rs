//! Committed stores refine declared unions; skipped and abrupt paths cannot.
use super::*;
use crate::execution::Execution;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V24)
        .unwrap_or_else(|error| panic!("{error}\n{source}"))
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
    if source.contains("value.length") {
        // Prove this negative exercises the unsafe read, not unrelated syntax.
        compile(&source.replace("value.length", "0"));
    }
}
#[test]
fn post_write_logical_target_uses_the_selected_normal_result_type() {
    run(
        "export function main():void{let value:string|undefined=undefined;value??='abc';console.log(value.length);value&&='x';console.log(value.length);value||='other';console.log(value.length);}",
        &["3", "1", "1"],
    );
}
#[test]
fn post_write_scalar_reference_and_cached_values_keep_declared_storage() {
    run(
        "export function main():void{let x:string|number|undefined=undefined;x='hello';console.log(x.length);const old=x;x=2;console.log(x.toFixed(1));console.log(old);x=undefined;console.log(Boolean(x));let record:{n:number}|undefined=undefined;record={n:7};console.log(record.n);let array:number[]|undefined=undefined;array=[8];console.log(array.length);}",
        &["5", "2.0", "hello", "0", "7", "1"],
    );
}
#[test]
fn post_write_destructuring_and_numeric_updates_refine_after_successful_store() {
    run(
        "export function main():void{let x:string|number|undefined=undefined;({n:x}={n:'text'});console.log(x.length);x=2;console.log(++x);console.log(x.toFixed(1));x*=3;console.log(x.toFixed(0));}",
        &["4", "3", "3.0", "9"],
    );
}
#[test]
fn post_write_if_joins_both_assignment_and_guard_paths() {
    run(
        "function f(flag:boolean):void{let value:{n:number}|undefined=undefined;if(flag){value={n:7};}else{value={n:8};}console.log(value.n);let second:{n:number}|undefined=undefined;if(!second){second={n:9};}console.log(second.n);}export function main():void{f(true);f(false);}",
        &["7", "9", "8", "9"],
    );
}
#[test]
fn post_write_join_retains_subunions_instead_of_declared_upper_bound() {
    run(
        "function f(flag:boolean):void{let value:string|number|undefined=undefined;if(flag){value='abc';}else{value=2;}if(typeof value==='string'){console.log(value.length);}else{console.log(value.toFixed(1));}}export function main():void{f(true);f(false);}",
        &["3", "2.0"],
    );
}
#[test]
fn post_write_conditional_expression_joins_side_effects() {
    run(
        "function f(flag:boolean):void{let value:string|number=0;console.log(flag?(value='a'):(value='bbb'));console.log(value.length);}export function main():void{f(true);f(false);}",
        &["a", "1", "bbb", "3"],
    );
}
#[test]
fn post_write_early_returns_and_throws_leave_only_normal_branch_facts() {
    run(
        "function read(value:{n:number}|undefined):number{if(!value){return 0;}return value.n;}function f(flag:boolean):number{let value:string|number=0;if(flag){throw 'stop';}else{value='abcd';}return value.length;}export function main():void{console.log(read(undefined));console.log(read({n:8}));console.log(f(false));try{f(true);}catch(e){console.log(e);}}",
        &["0", "8", "4", "stop"],
    );
}
#[test]
fn post_write_short_circuit_and_logical_assignment_rhs_facts_do_not_leak() {
    for expression in [
        "flag&&(value='s')",
        "flag||(value='s')",
        "other??(value='s')",
        "other||=(value='s')",
        "other&&=(value='s')",
        "other??=(value='s')",
    ] {
        reject(&format!(
            "let value:string|number=0;const flag=false;let other:string|undefined='old';{expression};console.log(value.length);"
        ));
    }
    run(
        "function f(flag:boolean):void{let value:string|number=0;flag&&(value='s');if(typeof value==='string'){console.log(value.length);}else{console.log(value.toFixed(1));}}export function main():void{f(false);f(true);}",
        &["0.0", "1"],
    );
}
#[test]
fn post_write_skipped_destructuring_defaults_cannot_install_fallback_facts() {
    reject(
        "let value:string|number=0;const {n=(value='s')}:{n?:string}={n:'present'};console.log(value.length);",
    );
    run(
        "export function main():void{let value:string|number=0;const {n=(value='s')}:{n?:string}={n:'present'};console.log(n);if(typeof value==='number'){console.log(value.toFixed(1));}}",
        &["present", "0.0"],
    );
}
#[test]
fn post_write_loop_body_facts_do_not_escape_zero_iterations_or_breaks() {
    for iteration in [
        "while(flag){value='s';}",
        "for(;flag;){value='s';}",
        "const items:number[]=[];for(const item of items){value='s';}",
        "do{if(flag)break;value='s';}while(false);",
    ] {
        reject(&format!(
            "const flag=true;let value:string|number=0;{iteration}console.log(value.length);"
        ));
    }
}
#[test]
fn post_write_continue_can_bypass_stores_before_update_and_do_while_test() {
    for iteration in [
        "for(;flag;console.log(value.length)){if(flag)continue;value='s';}",
        "do{if(flag)continue;value='s';}while(value.length>0);",
    ] {
        reject(&format!(
            "const flag=true;let value:string|number=0;{iteration}"
        ));
    }
}
#[test]
fn post_write_labels_and_switch_direct_entries_skip_prior_stores() {
    for statement in [
        "out:{if(flag)break out;value='s';}console.log(value.length);",
        "switch(2){case 1:value='s';break;case 2:console.log(value.length);}",
        "switch(2){case 1:value='s';case 2:console.log(value.length);}",
        "switch(2){case 1:value='s';break;case value.length:break;}",
        "switch(2){case 1:value='s';break;}console.log(value.length);",
    ] {
        reject(&format!(
            "const flag=true;let value:string|number=0;{statement}"
        ));
    }
}
#[test]
fn post_write_switch_preserves_unwritten_guards_and_checks_real_fallthrough() {
    run(
        "function f(key:number):void{let value:string|number=0;if(typeof value==='number'){switch(key){case 1:console.log(value.toFixed(0));value='a';case 2:if(typeof value==='string'){console.log(value.length);}else{console.log(value.toFixed(1));}break;default:console.log(value.toFixed(0));}}}export function main():void{f(1);f(2);f(3);}",
        &["0", "1", "0.0", "0"],
    );
}
#[test]
fn post_write_handlers_and_finalizers_cannot_assume_a_later_store_ran() {
    for statement in [
        "try{if(flag)throw 'stop';value='s';}catch(e){console.log(value.length);}",
        "try{if(flag)throw 'stop';value='s';}finally{console.log(value.length);}",
        "try{value='s';}catch(e){}console.log(value.length);",
        "try{throw 'stop';}catch(e){if(flag)throw e;value='s';}finally{console.log(value.length);}",
    ] {
        reject(&format!(
            "const flag=true;let value:string|number=0;{statement}"
        ));
    }
}
#[test]
fn post_write_normal_try_catch_joins_and_finalizer_stores_are_available() {
    run(
        "function f(flag:boolean):void{let value:string|number=0;try{if(flag)throw 'stop';value='try';}catch(e){value='catch';}console.log(value.length);try{if(flag)throw 'stop';}catch(e){console.log(e);}finally{value='done';}console.log(value.length);}export function main():void{f(false);f(true);}",
        &["3", "4", "5", "stop", "4"],
    );
}
#[test]
fn post_write_mutable_capture_calls_invalidate_assigned_facts() {
    reject(
        "let value:string|number=0;const change=():void=>{value=2;};value='s';change();console.log(value.length);",
    );
    run(
        "export function main():void{let value:string|number=0;const change=():void=>{value=2;};value='s';console.log(value.length);change();if(typeof value==='number'){console.log(value.toFixed(1));}}",
        &["1", "2.0"],
    );
}
#[test]
fn post_write_inference_is_opt_in_and_does_not_relax_storage_assignability() {
    let source = "export function main():void{let value:string|number=0;value='s';console.log(value.length);}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V23
        )
        .is_err()
    );
    compile(source);
    reject("let value:string|number=0;value='s';value=true;");
    reject("let value:{n:number}|undefined=undefined;value={n:1};value={other:2};");
}
#[test]
fn post_write_narrowed_arguments_and_pending_returns_survive_gc_and_finally() {
    let source = "function churn():void{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}}function saved():string{let value:string|undefined=undefined;value='saved'+'!';try{return value;}finally{value=undefined;churn();}}function consume(first:{text:string},ignored:number):string{churn();return first.text;}export function main():void{let value:{text:string}|undefined=undefined;const change=():number=>{value=undefined;churn();return 1;};value={text:'root'+'!'};console.log(consume(value,change()));console.log(saved());}";
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
        vec!["varta log: root!", "varta log: saved!"]
    );
}
