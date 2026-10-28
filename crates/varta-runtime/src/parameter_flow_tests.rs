//! Callback parameters specialize captured-cell effects at proven call sites.
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
fn parameter_flow_pure_inputs_preserve_the_same_captured_cell() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():void=>{console.log(Boolean(value));};const invoke=(fn:()=>void):void=>{fn();};if(value){invoke(read);console.log(value.n);}}",
        &["1", "7"],
    );
}
#[test]
fn parameter_flow_writing_inputs_invalidate_only_matching_cells() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const clear=():void=>{other=undefined;};const invoke=(fn:()=>void):void=>fn();if(value){invoke(clear);console.log(value.n);console.log(Boolean(other));}}",
        &["7", "0"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:()=>void):void=>fn();if(value){invoke(clear);console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_combines_direct_capture_and_callback_writes() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const invoke=(fn:()=>void):void=>{other=undefined;fn();};if(value){invoke(():void=>{});console.log(value.n);console.log(Boolean(other));}}",
        &["7", "0"],
    );
}
#[test]
fn parameter_flow_forwarding_and_immutable_aliases_preserve_symbolic_inputs() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():void=>{};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>void):void=>fn();const relay=(fn:()=>void):void=>{const alias=fn;invoke(alias);};const outer=(fn:()=>void):void=>relay(fn);if(value){outer(read);console.log(value.n);}}",
        &["7"],
    );
}
#[test]
fn parameter_flow_unused_callback_inputs_are_not_assumed_invoked() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const ignore=(fn:()=>void):void=>{};if(value){ignore(clear);console.log(value.n);}}",
        &["7"],
    );
}
#[test]
fn parameter_flow_reassigned_inputs_use_the_unknown_fallback() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:()=>void):void=>{fn=clear;fn();};if(value){invoke(():void=>{});console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_nested_parameter_stores_are_not_mistaken_for_original_inputs() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:()=>void):void=>{const change=():void=>{fn=clear;};change();fn();};if(value){invoke(():void=>{});console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_readonly_captured_inputs_keep_their_original_function() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():void=>{};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>void):void=>{const nested=():void=>fn();nested();};if(value){invoke(read);console.log(value.n);}}",
        &["7"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:()=>void):void=>{const nested=():void=>fn();nested();};if(value){invoke(clear);console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_alias_captures_propagate_through_function_only_environments() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>void):void=>{const saved=fn;const nested=():void=>saved();const outer=():void=>nested();outer();};if(value){invoke(():void=>{});console.log(value.n);}}",
        &["7"],
    );
}
#[test]
fn parameter_flow_aliases_do_not_hide_mutation_of_the_source_parameter() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:()=>void):void=>{const saved=fn;fn=():void=>{};saved();};if(value){invoke(clear);console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_extended_parameter_prologues_map_callback_inputs_correctly() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>void,n:number=1):number=>{fn();return n;};if(value){console.log(invoke(():void=>{}));console.log(value.n);console.log(invoke(():void=>{},3));console.log(value.n);}}",
        &["1", "7", "3", "7"],
    );
}
#[test]
fn parameter_flow_default_initializers_can_invoke_an_earlier_callback() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():number=>{console.log('read');return 1;};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>number,n:number=fn()):number=>{fn();return n;};if(value){console.log(invoke(read));console.log(value.n);}}",
        &["read", "read", "1", "7"],
    );
}
#[test]
fn parameter_flow_rest_parameter_prologues_keep_fixed_callback_positions() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>void,...items:number[]):number=>{fn();return items.length;};if(value){console.log(invoke(():void=>{},1,2));console.log(value.n);}}",
        &["2", "7"],
    );
}
#[test]
fn parameter_flow_call_methods_adjust_ignored_receiver_positions() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const read=():void=>{};const invoke=(fn:()=>void):void=>fn();if(value){for(let i=0;i<2;i++){invoke.call(undefined,read);console.log(value.n);}}}",
        &["7", "7"],
    );
}
#[test]
fn parameter_flow_function_receivers_are_explicit_callback_inputs() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const read=():void=>{};const invoke=function(this:()=>void):void{this();};if(value){for(let i=0;i<2;i++){invoke.call(read);console.log(value.n);}}}",
        &["7", "7"],
    );
}
#[test]
fn parameter_flow_guards_specialize_known_and_literal_callback_inputs() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);const invoke=(fn:()=>boolean):boolean=>fn();if(value&&invoke(read)){console.log(value.n);}if(value&&invoke(():boolean=>true)){console.log(value.n);}}",
        &["7", "7"],
    );
    reject(
        "let value:{n:number}|undefined={n:7};const invoke=(fn:()=>boolean):boolean=>fn();if(value&&invoke(():boolean=>{value=undefined;return true;})){console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_loop_analysis_specializes_callback_writes() {
    run(
        "export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const observe=():boolean=>Boolean(value);const clear=():void=>{other=undefined;};const invoke=(fn:()=>void):void=>fn();if(value){for(let i=0;i<2;i++){invoke(clear);invoke(():void=>{other=undefined;});console.log(value.n);}}}",
        &["7", "7"],
    );
}
#[test]
fn parameter_flow_mutable_callback_arguments_keep_the_conservative_fallback() {
    reject(
        "let value:{n:number}|undefined={n:7};const invoke=(fn:()=>void):void=>fn();let callback:()=>void=():void=>{value=undefined;};if(value){invoke(callback);console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_argument_mutation_is_not_restored_after_specialization() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const read=():void=>{};const invoke=(ignored:undefined,fn:()=>void):void=>fn();if(value){invoke(clear(),read);console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_module_forwarding_retains_symbolic_callback_inputs() {
    run(
        "function apply(fn:()=>void):void{fn();}export function main():void{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>void):void=>apply(fn);if(value){invoke(():void=>{});console.log(value.n);}}",
        &["7"],
    );
}
#[test]
fn parameter_flow_higher_order_callback_arguments_remain_conservative() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const invoke=(fn:(cb:()=>void)=>void):void=>fn(clear);const run=(cb:()=>void):void=>cb();if(value){invoke(run);console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_nested_literal_proofs_obey_existing_depth_limits() {
    let mut callback = "():void=>{}".to_string();
    for _ in 0..100 {
        callback = format!("():void=>{{invoke({callback});}}");
    }
    let source = format!(
        "export function main():void{{const invoke=(fn:()=>void):void=>fn();while(true){{invoke({callback});break;}}}}"
    );
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V24,
    )
    .unwrap_err()
    .to_string();
    assert!(
        (error.contains("nesting exceeds 64")
            || error.contains("source complexity exceeds compiler limits")),
        "{error}"
    );
}
#[test]
fn parameter_flow_indirect_nested_parameter_mutation_blocks_specialization() {
    reject(
        "let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};const apply=(cb:()=>void):void=>cb();const invoke=(fn:()=>void):void=>{const change=():void=>{fn=clear;};apply(change);fn();};if(value){invoke(():void=>{});console.log(value.n);}",
    );
}
#[test]
fn parameter_flow_specialized_roots_survive_callback_allocation_and_collection() {
    use crate::execution::Execution;
    let source = "function churn():void{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}}export function main():void{let value:{text:string}|undefined={text:'root'+'!'};let other:{text:string}|undefined={text:'other'+'!'};const observe=():boolean=>Boolean(value);const clear=():void=>{other=undefined;churn();};const invoke=(fn:()=>void):void=>{const saved=fn;const nested=():void=>saved();nested();};if(value){invoke(clear);churn();console.log(value.text);console.log(Boolean(other));}}";
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
