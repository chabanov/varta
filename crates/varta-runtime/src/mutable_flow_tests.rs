use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V24)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
fn reject(statement: &str) {
    assert!(
        varta_compiler::compile_typescript_with_profile(
            &format!("export function main():void{{{statement}}}"),
            LanguageProfile::CheckedF64Utf16V24
        )
        .is_err(),
        "{statement}"
    );
}
#[test]
fn mutable_flow_guarded_simple_destructuring_compound_and_logical_writes_execute() {
    let source = "export function main():void{let value:{n:number}|undefined={n:7};if(value){console.log(value.n);console.log(value=undefined);}console.log(Boolean(value));value={n:8};if(value){({n:value}={n:undefined});}console.log(Boolean(value));let number:number|string=1;if(typeof number==='number'){console.log(number++);console.log(number+=2);console.log(typeof number);}let text:string|undefined='';if(typeof text==='string'){console.log(text||='x');console.log(text&&='y');console.log(text??='z');}console.log(text);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "7",
            "undefined",
            "0",
            "0",
            "1",
            "4",
            "number",
            "x",
            "y",
            "y",
            "y"
        ])
    );
}
#[test]
fn mutable_flow_branch_alternatives_are_independent_and_joins_drop_written_facts() {
    let source = "function branch(flag:boolean):void{let value:{n:number}|undefined={n:7};if(value){if(flag){value=undefined;}else{console.log(value.n);}console.log(Boolean(value));}}export function main():void{branch(true);branch(false);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["0", "7", "1"])
    );
    for conditional in [
        "if(flag){value=undefined;}",
        "flag?(value=undefined):0;",
        "flag&&(value=undefined);",
        "flag||(value=undefined);",
    ] {
        reject(&format!(
            "let value:{{n:number}}|undefined={{n:1}};const flag=true;value=undefined;value={{n:1}};if(value){{{conditional}console.log(value.n);}}"
        ));
    }
}
#[test]
fn mutable_flow_test_writes_and_callbacks_do_not_create_stale_guard_facts() {
    for statement in [
        "let value:{n:number}|string|undefined={n:1};if(typeof value==='object'&&(value='text')){console.log(value.n);}",
        "let value:{n:number}|undefined={n:1};if((value=undefined)||true){console.log(value.n);}",
        "let value:{n:number}|undefined={n:1};const change=():boolean=>{value=undefined;return true;};if(value&&change()){console.log(value.n);}",
    ] {
        reject(statement);
    }
    let source = "export function main():void{let value:{n:number}|undefined={n:1};const change=():void=>{value=undefined;};if(value){change();if(value){console.log(value.n);}else{console.log('gone');}}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["gone"])
    );
}
#[test]
fn mutable_flow_loops_retest_mutated_values_and_for_of_heads() {
    let source = "function loop():void{let value:{n:number}|undefined={n:1};while(value){console.log(value.n);value=undefined;}value={n:2};for(;value;value=undefined){console.log(value.n);}const values:({n:number}|undefined)[]=[{n:3},undefined];for(value of values){if(value){console.log(value.n);}}console.log(Boolean(value));}export function main():void{loop();let value:{n:number}|undefined={n:4};const change=():void=>{value=undefined;};while(value){console.log(value.n);change();}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["1", "2", "3", "0", "4"])
    );
}
#[test]
fn mutable_flow_loop_backedges_never_reuse_outer_narrowing_after_writes() {
    for loop_body in [
        "while(flag){console.log(value.n);value=undefined;}",
        "do{console.log(value.n);value=undefined;}while(flag);",
        "for(;flag;){console.log(value.n);value=undefined;}",
        "for(const n of [1,2]){console.log(value.n);value=undefined;}",
        "while(flag){console.log(value.n);({n:value}={n:undefined});}",
    ] {
        reject(&format!(
            "let value:{{n:number}}|undefined={{n:1}};const flag=true;if(value){{{loop_body}}}"
        ));
    }
    reject(
        "let value:{n:number}|undefined={n:1};const change=():void=>{value=undefined;};if(value){while(true){console.log(value.n);change();}}",
    );
    reject(
        "let value:{n:number}|undefined={n:1};const change=():void=>{value=undefined;};if(value){while(true){const Boolean=change;console.log(value.n);Boolean();}}",
    );
    reject(
        "let value:{n:number}|undefined={n:1};for(;value;console.log(value.n)){value=undefined;}",
    );
}
#[test]
fn mutable_flow_loop_shadowing_and_unrelated_writes_preserve_valid_outer_facts() {
    let source = "export function main():void{let value:{n:number}|undefined={n:7};if(value){for(let value=0;value<1;value++){console.log(value);}console.log(value.n);for(let i=0;i<1;i++){const f=(value:number):number=>{value=2;return value;};console.log(f(1));}console.log(value.n);for(const value of [1]){console.log(value);}console.log(value.n);}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["0", "7", "2", "7", "1", "7"])
    );
}
#[test]
fn mutable_flow_mutable_captures_use_full_cell_types_and_rechecks_after_calls() {
    let source = "export function main():void{let value:{n:number}|undefined={n:7};const read=():number=>value?value.n:0;const change=():void=>{value=undefined;};if(value){console.log(value.n);change();console.log(read());if(value){console.log(value.n);}else{console.log('gone');}}value={n:8};if(value){[1].forEach((n:number):void=>{value=undefined;});console.log(Boolean(value));}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["7", "0", "gone", "0"])
    );
    reject(
        "let value:{n:number}|undefined={n:7};const change=():void=>{value=undefined;};if(value){change();console.log(value.n);}",
    );
    reject(
        "let value:{n:number}|undefined={n:7};if(value){[1].forEach((n:number):void=>{value=undefined;});console.log(value.n);}",
    );
}
#[test]
fn mutable_flow_const_captures_retain_proven_types_through_nested_closures() {
    let source = "function factory():()=>number{const value:{n:number}|undefined={n:9};if(value){const outer=():()=>number=>():number=>value.n;return outer();}return ():number=>0;}export function main():void{const read=factory();console.log(read());const value:{n:number}|undefined={n:7};if(value){const read=():number=>value.n;console.log(read());console.log(value.n);}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["9", "7", "7"])
    );
    reject(
        "let value:{n:number}|undefined={n:1};if(value){const read=():number=>value.n;console.log(read());}",
    );
}
#[test]
fn mutable_flow_known_native_calls_preserve_facts_and_shadowed_calls_invalidate() {
    let source = "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);if(value){console.log(value.n);String('overwrite');console.log(value.n);Math.abs(1);console.log(value.n);console.log(read());if(value){console.log(value.n);}}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["7", "7", "7", "1", "7"])
    );
    reject(
        "let value:{n:number}|undefined={n:7};const String=():void=>{value=undefined;};if(value){String();console.log(value.n);}",
    );
}
#[test]
fn mutable_flow_saved_reads_returns_pairs_and_escaped_environments_survive_collection() {
    use crate::execution::Execution;
    let source = "function churn():void{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}}function factory(input:{text:string}|undefined):()=>string{const value=input;if(value){return ():string=>{churn();return value.text;};}return ():string=>'';}function saved():{text:string}{let value:{text:string}|undefined={text:'saved'+'!'};try{if(value){return value;}throw Error('bad');}finally{value=undefined;churn();String('overwrite');}}function keep(first:{text:string},pair:VartaResult<string>,last:string):string{churn();console.log(pair.status);return first.text+last;}function consume(first:{text:string},ignored:number):string{churn();return first.text;}export function main():void{const read=factory({text:'root'+'!'});churn();console.log(read());console.log(saved().text);console.log(keep({text:'first'+'!'},varta.result.capture(varta.env.get('FLOW_PAIR_DENIED')),read()));console.log(varta.result.status());let value:{text:string}|undefined={text:'argument'+'!'};const change=():number=>{value=undefined;churn();return 1;};if(value){console.log(consume(value,change()));}}";
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
        lines(&[
            "root!",
            "saved!",
            "<capability denied: env> (FLOW_PAIR_DENIED)",
            "1",
            "first!root!",
            "1",
            "argument!"
        ])
    );
}
#[test]
fn mutable_flow_rhs_evaluation_precedes_store_and_old_reads_stay_owned() {
    let source = "function read(value:{n:number}):undefined{console.log(value.n);return undefined;}export function main():void{let value:{n:number}|undefined={n:7};if(value){value=read(value);console.log(Boolean(value));}value={n:8};const change=():number=>{value=undefined;return 2;};if(value){console.log(value.n+change());console.log(Boolean(value));}value={n:9};if(value){console.log(Boolean(value,change()));if(value){console.log(value.n);}else{console.log('gone');}}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["7", "0", "10", "0", "1", "gone"])
    );
}
#[test]
fn mutable_flow_exceptions_and_finalizers_do_not_restore_overwritten_guard_facts() {
    for statement in [
        "let value:{n:number}|undefined={n:1};if(value){try{value=undefined;throw Error('stop');}catch(error){console.log(value.n);}}",
        "let value:{n:number}|undefined={n:1};if(value){try{console.log('x');}finally{value=undefined;}console.log(value.n);}",
    ] {
        reject(statement);
    }
    let source = "export function main():void{let value:{n:number}|undefined={n:1};if(value){try{value=undefined;throw Error('stop');}catch(error){if(value){console.log(value.n);}else{console.log('gone');}}}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["gone"])
    );
}
#[test]
fn mutable_flow_legacy_profiles_keep_their_guarded_write_contract() {
    let source = "export function main():void{let value:number|string=1;if(typeof value==='number'){value='text';}console.log(value);}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V22,
        LanguageProfile::CheckedF64Utf16V23,
    ] {
        assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
    }
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["text"])
    );
}
