//! Closed function provenance bounds writes without changing physical captures.
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
    compile(&source.replace("console.log(value.n)", "console.log(Boolean(value))"));
}
#[test]
fn call_flow_pure_module_and_known_reader_preserve_mutable_capture_facts() {
    run(
        "function add(n:number):number{return n+1;}export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);if(value){console.log(add(1));console.log(value.n);console.log(read());console.log(value.n);}}",
        &["2", "7", "1", "7"],
    );
}
#[test]
fn call_flow_known_writer_invalidates_only_its_captured_cell() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const clear=():void=>{other=undefined;};if(value){clear();console.log(value.n);console.log(Boolean(other));}}",
        &["7", "0"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};if(value){clear();console.log(value.n);}",
    );
}
#[test]
fn call_flow_transitive_writes_cross_a_function_only_capture() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const relay=():void=>{clear();};const outer=():void=>{relay();};if(value){outer();console.log(value.n);}",
    );
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const clear=():void=>{other=undefined;};const relay=():void=>clear();if(value){relay();console.log(value.n);console.log(Boolean(other));}}",
        &["7", "0"],
    );
}
#[test]
fn call_flow_pure_transitive_aliases_keep_facts_through_nested_environments() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);const alias=read;const relay=():boolean=>alias();const outer=():boolean=>relay();if(value){console.log(outer());console.log(value.n);}}",
        &["1", "7"],
    );
}
#[test]
fn call_flow_conditional_function_proofs_join_possible_targets() {
    run(
        "function f(flag:boolean):void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const keep=():void=>{};const clear=():void=>{other=undefined;};const selected=flag?keep:clear;if(value){selected();console.log(value.n);console.log(Boolean(other));}}export function main():void{f(true);f(false);}",
        &["7", "1", "7", "0"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const keep=():void=>{};const clear=():void=>{value=undefined;};const flag=true;const selected=flag?keep:clear;if(value){selected();console.log(value.n);}",
    );
}
#[test]
fn call_flow_mutable_function_values_do_not_keep_stale_identity_proofs() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};let fn:()=>void=()=>{};fn=clear;if(value){fn();console.log(value.n);}",
    );
}
#[test]
fn call_flow_direct_module_callbacks_preserve_unrelated_cells() {
    run(
        "function invoke(fn:()=>void):void{fn();}export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const clear=():void=>{other=undefined;};if(value){invoke(clear);console.log(value.n);console.log(Boolean(other));}}",
        &["7", "0"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:()=>void):void=>{fn();};if(value){invoke(clear);console.log(value.n);}",
    );
}
#[test]
fn call_flow_module_pure_callbacks_can_read_the_same_captured_cell() {
    run(
        "function invoke(fn:()=>void):void{fn();}export function main():void{let value:{n:number}|undefined={n:7};const observe=():void=>{console.log(Boolean(value));};if(value){invoke(observe);console.log(value.n);}}",
        &["1", "7"],
    );
}
#[test]
fn call_flow_inline_callback_literals_bound_reachable_effects() {
    run(
        "function invoke(fn:()=>void):void{fn();}export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);if(value){invoke(():void=>{other=undefined;});console.log(value.n);console.log(Boolean(other));}}",
        &["7", "0"],
    );
}

#[test]
fn call_flow_function_valued_aggregates_remain_an_explicit_boundary() {
    let source = "function invoke(holder:{fn:()=>void}):void{holder.fn.call(undefined);}export function main():void{invoke({fn:():void=>{}});}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V24,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("function-valued record fields"), "{error}");
}

#[test]
fn call_flow_function_arguments_through_parameters_remain_conservative() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:()=>void):void=>{fn();};const relay=():void=>{invoke(clear);};if(value){relay();console.log(value.n);}",
    );
}
#[test]
fn call_flow_returned_function_identities_are_not_invented() {
    let source = "function forward(fn:()=>void):()=>void{return fn;}export function main():void{let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const later=forward(clear);value={n:8};if(value){later();console.log(value.n);}}";
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
fn call_flow_record_mutation_does_not_change_its_local_binding_class() {
    run(
        "function change(record:{n:number}):void{record.n=8;}export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);if(value){change(value);console.log(value.n);}}",
        &["8"],
    );
}
#[test]
fn call_flow_checked_member_intrinsics_do_not_reenter_captured_cells() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);const format=():string=>(2).toFixed(1);if(value){console.log(format());console.log(value.n);console.log('abc'.slice(1));console.log(value.n);const list=[1,2];list.push(3);list.reverse();list.fill(4);list.pop();console.log(value.n);Error('safe');console.log(value.n);}}",
        &["2.0", "7", "bc", "7", "7", "7"],
    );
}
#[test]
fn call_flow_argument_mutation_is_not_undone_by_a_pure_outer_call() {
    let source = "function id(n:number):number{return n;}export function main():void{let value:{n:number}|undefined={n:7};const clear=():number=>{value=undefined;return 1;};if(value){id(clear());console.log(value.n);}}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V24
        )
        .is_err()
    );
    compile(&source.replace("console.log(value.n)", "console.log(Boolean(value))"));
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():number=>{value=undefined;return 1;};const id=(n:number):number=>n;if(value){id(clear());console.log(value.n);}",
    );
}
#[test]
fn call_flow_guard_effects_block_only_the_cells_the_known_call_writes() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const clear=():boolean=>{other=undefined;return true;};if(value&&read()){console.log(value.n);}if(value&&clear()){console.log(value.n);console.log(Boolean(other));}}",
        &["7", "7", "0"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():boolean=>{value=undefined;return true;};if(value&&clear()){console.log(value.n);}",
    );
}
#[test]
fn call_flow_loop_effects_preserve_cells_unrelated_to_known_calls() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const clear=():void=>{other=undefined;};if(value){for(let i=0;i<2;i++){console.log(value.n);read();clear();}console.log(value.n);}}",
        &["7", "7", "7"],
    );
}
#[test]
fn call_flow_shadowed_function_bindings_have_distinct_effects() {
    reject(
        "let value:{n:number}|undefined={n:7};const fn=():void=>{};if(value){{const fn=():void=>{value=undefined;};fn();console.log(value.n);}}",
    );
    reject(
        "let value:{n:number}|undefined={n:7};const fn=():void=>{};if(value){for(let i=0;i<1;i++){const fn=():void=>{value=undefined;};fn();console.log(value.n);}}",
    );
}
#[test]
fn call_flow_recursive_self_calls_retain_unknown_reentry() {
    reject(
        "let value:{n:number}|undefined={n:7};const fn=function self(n:number):void{if(n>0){self(n-1);}else{value=undefined;}};if(value){fn(1);console.log(value.n);}",
    );
}
#[test]
fn call_flow_local_cell_writes_do_not_escape_into_the_callers_summary() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);const work=():void=>{let local:number|string=0;const change=():void=>{local=2;};local='x';change();};if(value){work();console.log(value.n);}}",
        &["7"],
    );
}
#[test]
fn call_flow_host_denial_status_and_cell_facts_are_independent() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);const denied=():void=>{varta.env.get('CALL_FLOW_DENIED');};if(value){denied();console.log(varta.result.status());console.log(value.n);}}",
        &["<capability denied: env> (CALL_FLOW_DENIED)", "1", "7"],
    );
}
#[test]
fn call_flow_identity_analysis_is_opt_in() {
    let source = "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);if(value){read();console.log(value.n);}}";
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
fn call_flow_spread_and_call_methods_keep_stable_callee_effects() {
    run(
        "function id(n:number):number{return n;}export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);const local=(n:number):number=>n+1;const args=[1];if(value){console.log(id(...args));console.log(value.n);console.log(local(...args));console.log(value.n);for(let i=0;i<1;i++){read.call(undefined);console.log(value.n);}}}",
        &["1", "7", "2", "7", "7"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const clear=(n:number):void=>{value=undefined;};const args=[1];if(value){clear(...args);console.log(value.n);}",
    );
}
#[test]
fn call_flow_narrowed_roots_and_transitive_environments_survive_collection() {
    use crate::execution::Execution;
    let source = "function churn():void{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}}export function main():void{let value:{text:string}|undefined={text:'root'+'!'};let other:{text:string}|undefined={text:'other'+'!'};const read=():boolean=>Boolean(value);const clear=():void=>{other=undefined;churn();};const relay=():void=>clear();if(value){relay();churn();console.log(value.text);console.log(Boolean(other));}}";
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
        vec!["varta log: root!", "varta log: 0"]
    );
}
