use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V11)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn function_values_execute_the_apply_example_with_contextual_arrow_parameters() {
    let output = run(r#"
function apply(fn:(value:number)=>number,value:number):number{return fn(value);}
export function main():void{console.log(apply(value=>value+1,41));}
"#);
    assert_eq!(output.lines, ["varta log: 42"]);
}
#[test]
fn named_function_references_and_fresh_arrow_evaluations_preserve_identity() {
    let output = run(r#"
function inc(value:number):number{return value+1;}
function make():(value:number)=>number{return value=>value+1;}
export function main():void{
 const named=inc;const alias=named;console.log(named===inc);console.log(named===alias);console.log(named(2));
 const first=make();const second=make();console.log(first===second);console.log(first===first);console.log(second(3));
 console.log(typeof first);console.log(!first);if(first){console.log("truthy");}
}"#);
    assert_eq!(
        output.lines,
        ["1", "1", "3", "0", "1", "4", "function", "0", "truthy"]
            .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn function_values_support_locals_reassignment_block_arrows_and_returned_calls() {
    let output = run(r#"
function inc(value:number):number{return value+1;}
function pick(flag:boolean):(value:number)=>number{if(flag){return inc;}return (value:number):number=>{return value*2;};}
function invoke(fn:(value:number)=>number,value:number):number{return fn(value);}
export function main():void{
 let fn:(value:number)=>number=inc;console.log(fn(1));fn=value=>value*3;console.log(fn(2));
 console.log(pick(true)(3));console.log(pick(false)(4));console.log(invoke((value:number)=>value+2,5));
 const say:(text:string)=>void=text=>console.log(text);say("void");
}"#);
    assert_eq!(
        output.lines,
        ["2", "6", "4", "8", "7", "void"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn callback_calls_keep_strings_records_and_captured_result_pairs_typed() {
    let output = run(r#"
function text(fn:(value:string)=>string,value:string):string{return fn(value);}
function object(fn:(value:{n:number})=>{n:number},value:{n:number}):{n:number}{return fn(value);}
function pair(fn:()=>VartaResult<string>):VartaResult<string>{return fn();}
export function main():void{
 console.log(text(value=>value+"😀","owned"));const value={n:1.25};console.log(object(v=>v,value)===value);
 const result=pair(():VartaResult<string>=>varta.result.capture(varta.env.get("VARTA_FUNCTION_DENIED")));console.log(result.status);
}"#);
    assert!(
        output
            .lines
            .starts_with(&["owned😀", "1"].map(|s| format!("varta log: {s}")))
    );
    assert!(output.lines.ends_with(&["varta log: 1".into()]));
}
#[test]
fn callback_errors_cross_dispatch_and_preserve_finalizer_overrides() {
    let output = run(r#"
function apply(fn:(value:number)=>number):number{return fn(1);}
export function main():void{
 try{apply((value:number):number=>{throw TypeError("callback");});}catch(e){if(e instanceof TypeError){console.log(e.message);}}
 try{try{const fn=():number=>varta.json.parse<number>('{');console.log(fn());}finally{console.log("finally");}}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}
 const good=():number=>{try{throw Error("old");}finally{return 7;}};console.log(good());
}"#);
    assert_eq!(
        output.lines,
        ["callback", "finally", "SyntaxError", "7"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn function_values_reject_captures_bad_signatures_and_unsupported_aggregate_exposure() {
    for source in [
        "function apply(fn:(n:number)=>number):void{}export function main():void{apply((n:string)=>n);}",
        "export function main():void{const f=(n:number)=>n;f('wrong');}",
        "export function main():void{const f=(n:number)=>n;f();}",
        "export function main():void{const f=n=>n+1;}",
        "export function main():void{const outer=1;const f=(n:number)=>n+outer;}",
        "export function main():void{const f=(n:number)=>{return n;};const g=()=>f(1);}",
        "export function main():void{const f=async(n:number)=>n;}",
        "export function main():void{const f=(n?:number)=>n;}",
        "export function main():void{const f=main;}",
        "export function main():void{const values=[(n:number)=>n];}",
        "export function main():void{const record={f:(n:number)=>n};}",
        "export function main():void{JSON.stringify((n:number)=>n);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V11
            )
            .is_err(),
            "{source}"
        );
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const f=(n:number)=>n;}",
            LanguageProfile::CheckedF64Utf16V10
        )
        .is_err()
    );
}

#[test]
fn callback_callees_and_arguments_evaluate_once_in_order_and_keep_semantic_signatures_distinct() {
    let output = run(r#"
function add(a:number,b:number):number{console.log("body");return a+b;}
function choose():(a:number,b:number)=>number{console.log("callee");return add;}
function arg(n:number):number{console.log(n);return n;}
function use(fn:(v:string)=>string,text:string):string{return fn(text);}
function identity(value:{n:number}):{n:number}{console.log("BAD_RECORD_TARGET");return value;}
export function main():void{
 console.log(choose()(arg(1),arg(2)));console.log(use(text=>text+"!","string"));
 const alias=add;console.log((true?alias:add)(3,4));
 try{choose()(varta.json.parse<number>('{'),arg(99));}catch(e){if(e instanceof SyntaxError){console.log("stopped");}}
}"#);
    assert_eq!(
        output.lines,
        [
            "callee", "1", "2", "body", "3", "string!", "body", "7", "callee", "stopped"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn capture_free_nested_arrows_can_return_functions_and_pass_function_values_as_arguments() {
    let output = run(r#"
function relay(fn:(n:number)=>number):(n:number)=>number{return fn;}
function factory():()=>((n:number)=>number){return ()=>(n:number)=>n+2;}
export function main():void{
 const outer=factory();const first=outer();const second=outer();console.log(first===second);console.log(relay(first)(40));
 const apply:(fn:(n:number)=>number,n:number)=>number=(fn,n)=>fn(n);console.log(apply(first,2));
 const undefinedValue:()=>undefined=()=>undefined;console.log(undefinedValue());
}"#);
    assert_eq!(
        output.lines,
        ["0", "42", "4", "undefined"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn function_identities_and_argument_results_survive_collection_without_retaining_dead_arrows() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<400;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
function inc(value:number):number{return value+1;}
function make():(value:string)=>string{return value=>value+"😀";}
function choose():(value:string)=>string{return make();}
function argument():string{churn();return "owned";}
export function main():void{
 const named=inc;const saved=make();churn();console.log(named===inc);console.log(named(1));console.log(saved("kept"));console.log(choose()(argument()));
 for(let i=0;i<7000;i=i+1){const temporary=make();temporary("garbage");}
 churn();console.log(saved("after"));console.log(named===inc);
}"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(300 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        ["1", "2", "kept😀", "owned😀", "after😀", "1"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn callback_budget_failures_and_authorized_exit_bypass_handlers_and_finalizers() {
    for body in ["while(true){}", "varta.process.exit(7);"] {
        let source = format!(
            "export function main():void{{const fn=():void=>{{{body}}};try{{fn();}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');return;}}}}"
        );
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&compile(&source)).unwrap();
        let grants = [varta_common::CapabilityGrant {
            id: "process.exit".into(),
            value: varta_common::CapabilityValue::Boolean(true),
        }];
        let mut execution =
            crate::execution::Execution::new(&runtime.engine, &grants, None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        if body.contains("process.exit") {
            assert_eq!(error.downcast_ref::<GuestExit>().unwrap().0, 7);
        }
        assert!(execution.store.data().output.lines.is_empty());
    }
}

#[test]
fn malformed_function_cells_trap_before_dispatch_and_cannot_enter_source_handlers() {
    use varta_common::{exceptions, function as layout};
    let bytes = compile(
        "function target(n:boolean):boolean{console.log('TARGET');return n;}export function main():void{const fn=target;try{fn(true);}catch(e){console.log('BAD_CATCH');}finally{console.log('FINAL');}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mutation in 0..10 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let start = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap();
        start.call(&mut execution.store, ()).unwrap();
        execution.store.data_mut().output.lines.clear();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let cache = abi::STRING_ROOT_BASE + exceptions::RESERVED_ROOT_BYTES;
        let handle = u64::from_le_bytes(
            memory.data(&execution.store)[cache..cache + 8]
                .try_into()
                .unwrap(),
        );
        let ptr = handle as u32 as usize;
        let (at, value) = match mutation {
            0 => (cache + 4, 0),
            1 => (cache, 1),
            2 => (ptr - abi::STRING_HEADER_BYTES, 0),
            3 => (ptr - abi::STRING_HEADER_BYTES, u32::MAX),
            4 => (ptr - abi::STRING_HEADER_BYTES + 4, 1),
            5 => (ptr - abi::STRING_HEADER_BYTES + 8, 1),
            6 => (ptr + layout::SIGNATURE_OFFSET as usize, u32::MAX),
            7 => (ptr + layout::RESERVED_OFFSETS[0] as usize, 1),
            8 => (ptr + layout::RESERVED_OFFSETS[1] as usize, 1),
            _ => (ptr + layout::CODE_OFFSET as usize, u32::MAX),
        };
        memory
            .write(&mut execution.store, at, &value.to_le_bytes())
            .unwrap();
        assert!(
            start.call(&mut execution.store, ()).is_err(),
            "mutation {mutation}"
        );
        assert!(
            execution.store.data().output.lines.is_empty(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn callback_dispatch_does_not_hide_host_effects_from_json_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function effect(n:boolean):boolean{console.log(n);return n;}function fake(n:boolean):boolean{const fn=effect;return fn(n);}export function main():void{varta.json.parse<number>('1');}",
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
fn function_values_bound_lifted_functions_semantic_types_parameters_and_dispatch_expansion() {
    let mut cases = Vec::new();
    let arrows = (0..varta_common::abi::MAX_USER_FUNCTIONS)
        .map(|i| format!("const f{i}=()=>0;"))
        .collect::<String>();
    cases.push((
        format!("export function main():void{{{arrows}}}"),
        "too many functions including arrows",
    ));
    let signatures = (0..=varta_common::function::MAX_TYPES)
        .map(|i| {
            let params = (0..8)
                .map(|bit| {
                    format!(
                        "p{bit}:{}",
                        if i & (1 << bit) == 0 {
                            "number"
                        } else {
                            "string"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("const f{i}=({params})=>0;")
        })
        .collect::<String>();
    cases.push((
        format!("export function main():void{{{signatures}}}"),
        "function type count exceeds limit",
    ));
    let params = (0..=varta_common::abi::MAX_WASM_PARAMETERS)
        .map(|i| format!("p{i}:number"))
        .collect::<Vec<_>>()
        .join(",");
    cases.push((
        format!("export function main():void{{const f=({params})=>0;}}"),
        "callback parameters exceed Wasm slot limit",
    ));
    let targets = (0..200)
        .map(|i| format!("function f{i}(n:number):number{{return n;}}"))
        .collect::<String>();
    let calls = "fn(1);".repeat(30);
    cases.push((
        format!("{targets}export function main():void{{const fn=f0;{calls}}}"),
        "function body exceeds compilation budget",
    ));
    for (source, expected) in cases {
        let error = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V11,
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}
