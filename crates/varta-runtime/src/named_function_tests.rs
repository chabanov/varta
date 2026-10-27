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
fn named_expressions_recurse_through_their_own_immutable_identity() {
    let output = run(r#"
function identity(n:number):number{return n;}
export function main():void{const factorial=function fact(n:number):number{if(n<=1){return 1;}return n*fact(n-1);};console.log(factorial(6));
 const fib:(n:number)=>number=function fibonacci(n){if(n<2){return n;}return fibonacci(n-1)+fibonacci(n-2);};console.log(fib(8));
 let seen:(n:number)=>number=identity;const original=function stable(n:number):number{seen=stable;if(n===0){return 7;}return stable(n-1);};let alias=original;alias=function(n){return 99;};console.log(original(3));console.log(seen===original);console.log(alias(3));
 console.log(JSON.stringify([1,2].map(function double(value):number{return value*2;})));}
"#);
    assert_eq!(output.lines, lines(&["720", "21", "7", "1", "99", "[2,4]"]));
}
#[test]
fn self_names_obey_private_scope_shadowing_and_capture_precedence() {
    let output = run(r#"
function shared(n:number):number{return 99;}
export function main():void{const fn=function shared(n:number):number{if(n===0){return 3;}return shared(n-1);};console.log(fn(2));console.log(shared(2));
 const parameter=function inner(inner:number):number{return inner+1;};console.log(parameter(4));
 const local=function inner():string{const inner="local";return inner;};console.log(local());
 const outer=10;const make=function recursive(n:number):number{if(n===0){return outer;}return recursive(n-1);};console.log(make(2));
 console.log(typeof fn);console.log(fn===fn);}
"#);
    assert_eq!(
        output.lines,
        lines(&["3", "99", "5", "local", "10", "function", "1"])
    );
}
#[test]
fn declaration_receivers_support_bare_checked_calls_and_lexical_this_capture() {
    let output = run(r#"
function add(this:{n:number},value:number):number{return this.n+value;}
function optional(this:number|undefined,value:number):number{const current=this;if(current===undefined){return value;}else{return current+value;}}
function reader(this:{n:number}):()=>number{return ()=>this.n;}
function recurse(this:{n:number},depth:number):number{if(depth===0){return this.n;}return recurse.call(this,depth-1);}
export function main():void{const state={n:10};console.log(add.call(state,2));console.log(JSON.stringify([1,2].map(add,state)));console.log(optional(4));console.log(optional.call(3,4));
 const escaped=reader.call(state);state.n=20;console.log(escaped());console.log(recurse.call(state,3));console.log(add===add);}
"#);
    assert_eq!(
        output.lines,
        lines(&["12", "[11,12]", "4", "7", "20", "20", "1"])
    );
}
#[test]
fn named_receiver_functions_keep_this_and_do_not_inherit_it_in_bare_recursion() {
    let output = run(r#"
export function main():void{const fn=function recursive(this:{n:number},depth:number):number{if(depth===0){return this.n;}return recursive.call(this,depth-1);};console.log(fn.call({n:5},4));
 const state={n:8};console.log(JSON.stringify([1,2].map(function named(this:{n:number},value):number{if(value===0){return this.n;}return named.call(this,value-1)+1;},state)));
 const lexical=function named(this:{n:number},depth:number):()=>number{if(depth===0){return ()=>this.n;}return named.call(this,depth-1);};const read=lexical.call(state,2);state.n=9;console.log(read.call({n:99}));}
"#);
    assert_eq!(output.lines, lines(&["5", "[9,10]", "9"]));
}
#[test]
fn recursive_calls_propagate_source_errors_and_finalizer_returns() {
    let output = run(r#"
export function main():void{const fn=function named(depth:number):number{try{if(depth===0){throw RangeError("stop");}return named(depth-1);}finally{console.log(depth);}};try{fn(2);}catch(e){if(e instanceof RangeError){console.log(e.message);}}
 const override=function named(depth:number):number{try{if(depth===0){throw "caught";}return named(depth-1);}finally{if(depth===1){return 9;}}};console.log(override(2));
 const voidFn=function named(depth:number):void{if(depth>0){named(depth-1);}else{console.log("void");}};console.log(voidFn(3));console.log(varta.result.status());}
"#);
    assert_eq!(
        output.lines,
        lines(&["0", "1", "2", "stop", "9", "void", "undefined", "0"])
    );
}
#[test]
fn self_and_receiver_references_survive_collection_escaping_and_alias_reassignment() {
    let source = r#"
function churn():void{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}}
function create(offset:number):(n:number)=>()=>number{return function self(n:number):()=>number{return ()=>{churn();if(n===0){return offset;}return self(n-1)();};};}
function reader(this:{text:string}):()=>string{return ()=>this.text;}
export function main():void{let fn=create(7);const original=fn;const escaped=fn(3);fn=create(99);churn();console.log(escaped());console.log(original(1)());
 const self=function recursive(this:{text:string},n:number):string{if(n===0){return this.text;}churn();return recursive.call(this,n-1);};console.log(self.call({text:"owned"+"😀"},3));
 const read=reader.call({text:"retained"+"!"});churn();console.log(read());}
"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(700 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        lines(&["7", "7", "owned😀", "retained!"])
    );
}
#[test]
fn private_self_names_reject_mutation_ambiguity_and_unsupported_initialization() {
    for source in [
        "export function main():void{const fn=function self(n:number):number{self=(n:number)=>n;return n;};}",
        "export function main():void{const fn=function privateName():number{return 1;};privateName();}",
        "export function main():void{const fn=function self(n:number){return self(n-1);};}",
        "export function main():void{const fn=function self(n:number):number{return 'bad';};}",
        "export function main():void{const fn=function self(this:number,n:number):number{return self(n-1);};}",
        "export function main():void{const fn=function JSON():number{return 1;};}",
        "export function main():void{const fn=function self():number{return fn();};}",
        "function need(this:number):number{return this;}export function main():void{need();}",
        "function need(this:number):number{return this;}export function main():void{need.call('bad');}",
        "export function main(this:undefined):void{}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V14
            )
            .is_err(),
            "{source}"
        );
    }
    let source = "function need(this:number):number{return this;}export function main():void{need.call('bad');}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.code, varta_common::DiagnosticCode::TypeError);
    assert_eq!(
        diagnostic.range.start.byte_offset,
        source.find("'bad'").unwrap()
    );
    let source = "function value(this:number):number{return this;}export function main():void{const fn=function self(n:number):number{if(n===0){return 2;}return self(n-1);};console.log(fn(3));console.log(value.call(4));}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["2", "4"])
        );
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V11
        )
        .is_err()
    );
}
#[test]
fn recursive_exhaustion_remains_fatal_and_private_self_counts_as_a_parameter() {
    for body in [
        "const fn=function self():number{return self();};fn();",
        "const fn=function self(n:number):number{const values:number[]=[];values.length=1025;return self(n+1);};fn(0);",
    ] {
        let bytes = compile(&format!(
            "export function main():void{{try{{{body}}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
        ));
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
    let parameters = (0..31)
        .map(|i| format!("p{i}:number"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "export function main():void{{const fn=function self({parameters}):number{{return 1;}};}}"
    );
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert!(error.to_string().contains("plus environment"), "{error}");
    let parameters = (0..30)
        .map(|i| format!("p{i}:number"))
        .collect::<Vec<_>>()
        .join(",");
    let arguments = vec!["1"; 30].join(",");
    let output = run(&format!(
        "export function main():void{{const fn=function self({parameters}):number{{return p0;}};console.log(fn({arguments}));}}"
    ));
    assert_eq!(output.lines, lines(&["1"]));
}
#[test]
fn recursive_and_declaration_receiver_calls_cannot_hide_host_effects() {
    use crate::contract_tests::{export_index, rewrite};
    for body in [
        "const fn=function self(n:boolean):boolean{console.log(n);if(n){return self(false);}return n;};fn(value);",
        "receiver.call(value);",
    ] {
        let bytes = compile(&format!(
            "function receiver(this:boolean):boolean{{console.log(this);return this;}}function fake(value:boolean):boolean{{{body}return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn self_entry_guard_rejects_forged_identity_signature_code_and_environment_before_effects() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::function as layout;
    let bytes = compile(
        "function make(offset:number):(n:number)=>number{return function self(n:number):number{console.log('BODY');if(n===0){return offset;}return self(n-1);};}export function main():void{}",
    );
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 126);
            data[0] += 2;
            for (name, index) in [("fixture.make", main - 1), ("fixture.body", main + 1)] {
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
    for mutation in 0..6 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let make = instance
            .get_typed_func::<f64, i64>(&mut execution.store, "fixture.make")
            .unwrap();
        let handle = make.call(&mut execution.store, 7.0).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let ptr = handle as u32 as usize;
        let mut environment = [0; 8];
        memory
            .read(
                &execution.store,
                ptr + layout::ENVIRONMENT_OFFSET as usize,
                &mut environment,
            )
            .unwrap();
        let environment = i64::from_le_bytes(environment);
        let self_handle = match mutation {
            1 => 0,
            2 | 3 => {
                let at = ptr
                    + if mutation == 2 {
                        layout::SIGNATURE_OFFSET
                    } else {
                        layout::CODE_OFFSET
                    } as usize;
                memory
                    .write(&mut execution.store, at, &u32::MAX.to_le_bytes())
                    .unwrap();
                handle
            }
            5 => make.call(&mut execution.store, 9.0).unwrap(),
            _ => handle,
        };
        let result = instance
            .get_typed_func::<(f64, i64, i64), f64>(&mut execution.store, "fixture.body")
            .unwrap()
            .call(
                &mut execution.store,
                (
                    0.0,
                    self_handle,
                    if mutation == 4 { 0 } else { environment },
                ),
            );
        assert_eq!(result.is_ok(), mutation == 0, "mutation {mutation}");
        if mutation == 0 {
            assert_eq!(result.unwrap(), 7.0);
            assert_eq!(execution.store.data().output.lines, lines(&["BODY"]));
        } else {
            assert!(
                execution.store.data().output.lines.is_empty(),
                "mutation {mutation}"
            );
        }
    }
}
#[test]
fn private_self_with_multi_slot_parameters_returns_and_captured_names_is_exact() {
    let output = run(r#"
export function main():void{const pair=varta.result.capture(varta.env.get("VARTA_NAMED_PAIR"));const fn=function self(value:VartaResult<string>,depth:number):VartaResult<string>{try{if(depth===0){return value;}const invoke=()=>self(value,depth-1);return invoke();}finally{console.log(depth);}};const result=fn(pair,2);console.log(result.status);console.log(result.value===pair.value);
 const sameName=function sameName(n:number):number{if(n===0){return 11;}return sameName(n-1);};console.log(sameName(2));}
"#);
    assert_eq!(
        &output.lines[output.lines.len() - 6..],
        lines(&["0", "1", "2", "1", "1", "11"])
    );
}
