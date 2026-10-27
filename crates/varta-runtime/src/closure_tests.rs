use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V12)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn escaped_closures_share_mutable_bindings_and_keep_activations_independent() {
    let out = run(r#"
function counter(start:number):()=>number{let value=start;return ()=>{value=value+1;return value;};}
export function main():void{
 const first=counter(0);const second=counter(10);console.log(first());console.log(first());console.log(second());console.log(first());
 let value=1;const read=()=>value;const write=(n:number)=>{value=n;};console.log(read());value=3;console.log(read());write(7);console.log(value);
 const suffix="😀";const text=(s:string)=>s+suffix;console.log(text("owned"));
}"#);
    assert_eq!(
        out.lines,
        ["1", "2", "11", "3", "1", "3", "7", "owned😀"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn nested_closures_forward_outer_cells_and_preserve_shadowing() {
    let out = run(r#"
function outer(value:string):()=>((suffix:string)=>string){return ()=>suffix=>value+suffix;}
function parameter(value:number):()=>number{const f=()=>value;value=value+1;return f;}
export function main():void{const make=outer("kept");const first=make();const second=make();console.log(first("😀"));console.log(first===second);console.log(parameter(4)());
 const x=1;const f:()=>()=>number=()=>{const x=2;return ()=>x;};console.log(f()());console.log(x);}
"#);
    assert_eq!(
        out.lines,
        ["kept😀", "0", "5", "2", "1"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn loop_let_bindings_have_fresh_cells_on_continue_and_body_reentry() {
    let out = run(r#"
export function main():void{
 let first:()=>number=()=>99;let second:()=>number=()=>99;
 for(let i=0;i<2;i=i+1){if(i===0){first=()=>i;continue;}second=()=>i;}
 console.log(first());console.log(second());
 let body:()=>number=()=>99;for(let i=0;i<2;i=i+1){let current=i;body=()=>current;current=current+10;}console.log(body());
 let final:()=>number=()=>99;for(let i=0;i<2;i=i+1){try{final=()=>i;continue;}finally{i=i+1;}}console.log(final());
 for(let i=0;i<2;i=i+1){try{final=()=>i;continue;}finally{i=7;break;}}console.log(final());
}"#);
    assert_eq!(
        out.lines,
        ["0", "1", "11", "1", "7"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn captured_references_results_and_function_values_keep_types_and_aliases() {
    let out = run(r#"
function pair():()=>VartaResult<string>{let result=varta.result.capture(varta.env.get("VARTA_CLOSURE_DENIED"));const fn=()=>result;return fn;}
function cycle():()=>number{let fn:()=>number=()=>0;const read=()=>fn();fn=()=>42;return read;}
function object(value:{n:number},array:string[]):()=>string{return ()=>{value.n=value.n+1;array.push("😀");return JSON.stringify(value)+JSON.stringify(array);};}
export function main():void{console.log(pair()().status);console.log(pair()().value);console.log(cycle()());
 const value={n:1};const array=["kept"];const fn=object(value,array);console.log(fn());console.log(value.n);console.log(array.length);
 let text="before";const read=()=>text;text="after😀";console.log(read());const bit=true;const number=1.25;const truth=()=>bit;const numeric=()=>number;console.log(truth());console.log(numeric());}
"#);
    assert_eq!(
        out.lines,
        [
            "<capability denied: env> (VARTA_CLOSURE_DENIED)",
            "1",
            "<capability denied: env> (VARTA_CLOSURE_DENIED)",
            "",
            "42",
            "{\"n\":2}[\"kept\",\"😀\"]",
            "2",
            "2",
            "after😀",
            "1",
            "1.25"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn closures_preserve_mutations_across_exceptions_and_finalizers() {
    let out = run(r#"
function make(input:string):()=>number{let count=0;return ()=>{try{count=count+1;return varta.json.parse<number>(input);}finally{console.log(count);}};}
export function main():void{
 let x=0;const read=()=>x;const fail=()=>{x=3;throw TypeError("owned😀");};
 try{try{fail();}finally{x=x+1;console.log(read());}}catch(e){if(e instanceof TypeError){console.log(e.message);}}
 const parse=make("{");for(let i=0;i<2;i=i+1){try{parse();}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}}
 let saved:()=>string|undefined=()=>"empty";try{throw "caught😀";}catch(e){saved=()=>JSON.stringify(e);}console.log(saved());
}"#);
    assert_eq!(
        out.lines,
        [
            "4",
            "owned😀",
            "1",
            "SyntaxError",
            "2",
            "SyntaxError",
            "\"caught😀\""
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn closure_cells_handle_maximum_environment_and_reject_unsafe_or_unsupported_sources() {
    let locals = (0..varta_common::function::MAX_CAPTURES)
        .map(|i| format!("let x{i}={i};"))
        .collect::<String>();
    let sum = (0..varta_common::function::MAX_CAPTURES)
        .map(|i| format!("x{i}"))
        .collect::<Vec<_>>()
        .join("+");
    let out = run(&format!(
        "export function main():void{{{locals}const fn=()=>{sum};x0=1;console.log(fn());}}"
    ));
    assert_eq!(out.lines, ["varta log: 497"]);
    let mut sources=vec![
 "export function main():void{const fn=()=>later;const later=1;}".to_owned(),
 "export function main():void{const n=1;const fn=()=>{n=2;};}".to_owned(),
 "export function main():void{let n=1;const fn=()=>{n='bad';};}".to_owned(),
 "export function main():void{let value:number|string='x';const change=()=>{value=1;};if(typeof value==='string'){change();console.log(value.length);}}".to_owned(),
 "export function main():void{let value:number|string='x';if(typeof value==='string'){const change=()=>{value=1;};change();console.log(value.length);}}".to_owned(),
 ];
    sources.push("export function main():void{let value:number|string='x';let change:()=>void=()=>{};for(let i=0;i<2;i=i+1){if(typeof value==='string'){change();console.log(value.length);}change=()=>{value=1;};}}".into());
    sources.push(format!(
        "export function main():void{{{locals}const extra=32;const fn=()=>{sum}+extra;}}"
    ));
    for source in sources {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V12
            )
            .is_err(),
            "{source}"
        );
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const x=1;const fn=()=>x;}",
            LanguageProfile::CheckedF64Utf16V11
        )
        .is_err()
    );
}
#[test]
fn escaped_closure_graphs_survive_collection_and_unreachable_cycles_are_reclaimed() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<350;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
function make(text:string):()=>string{let value=text;return ()=>{value=value+"😀";return value;};}
function garbage(text:string):void{let fn:()=>string=()=>text;fn=()=>{return typeof fn;};}
function take(fn:()=>string):()=>string{return fn;}
export function main():void{const saved=make("kept");churn();console.log(saved());for(let i=0;i<8000;i=i+1){garbage("garbage"+"!");}churn();console.log(saved());console.log(take(make("new"))());}
"#;
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
        ["kept😀", "kept😀😀", "new😀"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn malformed_closure_environments_and_shared_cells_trap_before_user_effects() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::{function as layout, value};
    let bytes = compile(
        "function make(value:number):()=>number{return ()=>{console.log('TARGET');return value;};}function invoke(fn:()=>number):number{try{return fn();}catch(e){console.log('BAD_CATCH');return 0;}finally{console.log('BAD_FINALLY');}}export function main():void{}",
    );
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 126);
            data[0] += 2;
            for (name, index) in [("fixture.make", main - 2), ("fixture.invoke", main - 1)] {
                data.push(name.len() as u8);
                data.extend_from_slice(name.as_bytes());
                data.push(0);
                data.push(index);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mutation in 0..12 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let handle = instance
            .get_typed_func::<f64, i64>(&mut execution.store, "fixture.make")
            .unwrap()
            .call(&mut execution.store, 42.0)
            .unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let data = memory.data(&execution.store);
        let function = handle as u32 as usize;
        let environment = u64::from_le_bytes(
            data[function + layout::ENVIRONMENT_OFFSET as usize
                ..function + layout::ENVIRONMENT_OFFSET as usize + 8]
                .try_into()
                .unwrap(),
        ) as u32 as usize;
        let binding = u64::from_le_bytes(
            data[environment + value::RECORD_META_BYTES
                ..environment + value::RECORD_META_BYTES + 8]
                .try_into()
                .unwrap(),
        ) as u32 as usize;
        let (at, word) = match mutation {
            0 => (function + layout::RESERVED_OFFSETS[0] as usize, 0),
            1 => (
                function - abi::STRING_HEADER_BYTES + value::HEAP_LINK_OFFSET,
                0,
            ),
            2 => (function + layout::ENVIRONMENT_OFFSET as usize + 4, 0),
            3 => (
                environment - abi::STRING_HEADER_BYTES + value::HEAP_FLAGS_OFFSET,
                1,
            ),
            4 => (
                environment - abi::STRING_HEADER_BYTES + value::HEAP_LINK_OFFSET,
                0,
            ),
            5 => (environment, 1),
            6 => (environment + 4, 2),
            7 => (
                binding - abi::STRING_HEADER_BYTES + value::HEAP_FLAGS_OFFSET,
                1,
            ),
            8 => (
                binding - abi::STRING_HEADER_BYTES + value::HEAP_LINK_OFFSET,
                1,
            ),
            9 => (binding, u32::MAX),
            10 => (binding + 4, 2),
            _ => (binding - abi::STRING_HEADER_BYTES, 0),
        };
        memory
            .write(&mut execution.store, at, &word.to_le_bytes())
            .unwrap();
        assert!(
            instance
                .get_typed_func::<i64, f64>(&mut execution.store, "fixture.invoke")
                .unwrap()
                .call(&mut execution.store, handle)
                .is_err(),
            "mutation {mutation}"
        );
        assert!(
            execution.store.data().output.lines.is_empty(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn closure_callbacks_keep_json_helper_effect_analysis_transitive() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{const fn:()=>boolean=()=>{console.log(value);return value;};return fn();}export function main():void{varta.json.parse<number>('1');}",
    );
    let index = export_index(&bytes, "_start") - 1;
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
            assert!(index < 128 && data[at] < 128);
            data[at] = index;
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
fn closure_budget_traps_and_authorized_exit_bypass_catch_and_finally() {
    for (body, grants) in [
        (
            "const value=1;const fn=()=>{while(value===1){}};try{fn();}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}",
            vec![],
        ),
        (
            "const code=7;const fn=()=>{varta.process.exit(code);};try{fn();}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}",
            vec![CapabilityGrant {
                id: "process.exit".into(),
                value: varta_common::CapabilityValue::Boolean(true),
            }],
        ),
    ] {
        let bytes = compile(&format!("export function main():void{{{body}}}"));
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution =
            crate::execution::Execution::new(&runtime.engine, &grants, None).unwrap();
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
}
