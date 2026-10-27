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
fn labels_continue_outer_for_runs_update_and_break_skips_it() {
    let out = run(
        "export function main():void{let visits=0;outer:for(let n=0;n<3;n=n+1){for(let inner=0;inner<2;inner=inner+1){console.log(n);visits=visits+1;if(n<2){continue outer;}break outer;}console.log('BAD');}console.log(visits);}",
    );
    assert_eq!(out.lines, lines(&["0", "1", "2", "3"]));
}
#[test]
fn labels_route_while_and_do_while_continues_to_their_own_conditions() {
    let out = run(
        "export function main():void{let n=0;outer:while(n<2){n=n+1;do{continue outer;}while(true);}console.log(n);n=0;const condition=():boolean=>{console.log('condition');return n<2;};outer:do{n=n+1;for(const value of [1]){continue outer;}}while(condition());console.log(n);outer:do{while(true){break outer;}}while(condition());}",
    );
    assert_eq!(out.lines, lines(&["2", "condition", "condition", "2"]));
}
#[test]
fn labels_chain_aliases_share_iteration_targets_and_sibling_names_can_be_reused() {
    let out = run(
        "export function main():void{let n=0;a:b:for(;n<2;n=n+1){if(n===0){continue a;}continue b;}console.log(n);a:b:do{break a;}while(true);a:while(true){break a;}console.log('done');}",
    );
    assert_eq!(out.lines, lines(&["2", "done"]));
}
#[test]
fn labels_blocks_and_switches_accept_break_without_hijacking_unlabeled_break() {
    let out = run(
        "export function main():void{done:{console.log('block');for(const value of [1]){break done;}console.log('BAD');}let n=0;outer:while(n<2){n=n+1;inside:{break;}console.log('BAD');}console.log(n);outer:switch(1){case 1:console.log('switch');break outer;default:console.log('BAD');}outer:if(true){break outer;}else{console.log('BAD');}const same=7;same:{console.log(same);break same;}console.log('done');}",
    );
    assert_eq!(out.lines, lines(&["block", "1", "switch", "7", "done"]));
}
#[test]
fn labels_cross_nested_finalizers_and_allow_completion_overrides() {
    let out = run(
        "export function main():void{let n=0;outer:do{n=n+1;try{try{if(n===1){continue outer;}break outer;}finally{console.log('inner');}}finally{console.log(n);}}while(true);console.log(n);n=0;outer:do{n=n+1;try{break outer;}finally{if(n===1){continue outer;}console.log('finish');}}while(n<2);console.log(n);outer:do{try{continue outer;}finally{break outer;}}while(true);try{outer:while(true){try{break outer;}finally{throw SyntaxError('override');}}}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}}",
    );
    assert_eq!(
        out.lines,
        lines(&["inner", "1", "inner", "2", "2", "finish", "2", "override"])
    );
}
#[test]
fn labels_outer_continue_preserves_for_of_identity_and_escaping_iteration_cells() {
    let out = run(
        "export function main():void{let first:()=>number|undefined=()=>99;let second:()=>number|undefined=()=>99;let n=0;const values:number[]=[1,2];outer:for(let value of values){try{for(const inner of [3,4]){n=n+1;continue outer;}}finally{if(n===1){first=()=>value;value=7;values.push(5);}else{second=()=>value;break outer;}}}console.log(first());console.log(second());console.log(n);console.log(values.length);}",
    );
    assert_eq!(out.lines, lines(&["7", "2", "2", "3"]));
}
#[test]
fn labels_for_initializers_and_for_of_sources_do_not_bind_to_generated_rest_loops() {
    let out = run(
        "export function main():void{outer:for(let [head=0,...tail]:number[]=[0,1];head<2;head=head+1){for(const value of [9]){console.log(head);continue outer;}}const values:number[]=[1,2];let rest:(number|undefined)[]=[];outer:for(const n of ([...rest]=values)){do{console.log(n);continue outer;}while(false);}console.log(JSON.stringify(rest));}",
    );
    assert_eq!(out.lines, lines(&["0", "1", "1", "2", "[1,2]"]));
}
#[test]
fn labels_completion_analysis_preserves_nonlocal_abrupt_paths_and_callback_inference() {
    let out = run(
        "function result():number{out:{return 7;}}function nested():number{outer:do{inner:do{return 8;}while(false);}while(false);}export function main():void{console.log(result());console.log(nested());console.log(JSON.stringify([1,2].map(n=>{outer:do{while(n===1){break outer;}return n;}while(false);})));console.log(JSON.stringify([1].map(n=>{outer:do{try{return n;}finally{console.log('finally');}}while(false);})));}",
    );
    assert_eq!(out.lines, lines(&["7", "8", "[null,2]", "finally", "[1]"]));
    for source in [
        "function bad():number{outer:do{while(true){break outer;}return 1;}while(false);}export function main():void{}",
        "function bad():number{outer:do{do{continue outer;}while(false);return 1;}while(false);}export function main():void{}",
        "function bad():number{out:{break out;return 1;}}export function main():void{}",
        "function bad():number{outer:do{try{return 1;}finally{break outer;}}while(false);}export function main():void{}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
}
#[test]
fn labels_reject_unknown_duplicate_noniteration_and_cross_function_targets() {
    for source in [
        "export function main():void{while(true){break unknown;}}",
        "export function main():void{out:{continue out;}}",
        "export function main():void{out:if(true){while(true){continue out;}}}",
        "export function main():void{out:out:while(true){break out;}}",
        "export function main():void{out:while(true){const fn=()=>{break out;};break;}}",
        "export function main():void{out:while(true){const fn=()=>{continue out;};break;}}",
        "export function main():void{out:{let n=0;}console.log(n);}",
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
            "export function main():void{out:while(true){console.log(1);break out;}}",
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["1"])
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{out:while(true){break out;}}",
                profile
            )
            .is_err()
        );
    }
    let source = format!(
        "export function main():void{{{}while(false){{}}}}",
        (0..65).map(|i| format!("l{i}:")).collect::<String>()
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V14
        )
        .is_err()
    );
}
#[test]
fn labels_preserve_saved_returns_result_pairs_and_captures_across_finalizer_collection() {
    let source = "function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}function result():VartaResult<string>{out:do{try{return varta.result.capture(varta.env.get('VARTA_LABEL_GC'));}finally{churn();}}while(false);}export function main():void{let n=0;let read:()=>string=()=>'';outer:do{const text='kept'+'!';read=()=>text;try{for(const value of [1]){n=n+1;if(n<2){continue outer;}break outer;}}finally{churn();console.log(read());}}while(true);churn();console.log(read());const pair=result();churn();console.log(pair.status);console.log(pair.value);}";
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
        lines(&[
            "kept!",
            "kept!",
            "kept!",
            "<capability denied: env> (VARTA_LABEL_GC)",
            "1",
            ""
        ])
    );
}
#[test]
fn labels_body_effects_remain_visible_to_host_free_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{out:{console.log('effect');break out;}return value;}export function main():void{varta.json.parse<number>('1');}",
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
#[test]
fn labels_do_not_convert_resource_traps_into_recoverable_branches() {
    let bytes = compile(
        "export function main():void{try{out:while(true){continue out;}}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
