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
fn anonymous_functions_share_closures_inference_calls_and_fresh_identity() {
    let output = run(r#"
function create(offset:number):(value:number)=>number{return function(value){return value+offset;};}
export function main():void{const first=create(10);const second=create(10);console.log(first(2));console.log(first===second);console.log(first===first);
 let count=0;const increment=function(){count=count+1;return count;};console.log(increment());console.log(increment());
 console.log((function(value:number){return "text";})(1));console.log(JSON.stringify([1,2].map(function(value){return {n:value};})));
 console.log([1,2].reduce(function(total:number[],value){total.push(value);return total;},[]).length);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "12",
            "0",
            "1",
            "1",
            "2",
            "text",
            "[{\"n\":1},{\"n\":2}]",
            "2"
        ])
    );
}
#[test]
fn typed_this_arguments_retain_identity_and_mutations_for_every_callback_method() {
    let output = run(r#"
export function main():void{const state={offset:10,count:0};
 console.log(JSON.stringify([1,2].map(function(this:{offset:number;count:number},value,index,array){this.count=this.count+1;return value+this.offset;},state)));
 console.log(JSON.stringify([1,2].filter(function(this:{offset:number;count:number},value){return value+this.offset>11;},state)));
 [1,2].forEach(function(this:{offset:number;count:number},value){this.count=this.count+value;},state);console.log(state.count);
 console.log([1,2].find(function(this:{offset:number;count:number},value){return value===2;},state));
 console.log([1,2].some(function(this:{offset:number;count:number},value){return value===1;},state));
 console.log([1,2].every(function(this:{offset:number;count:number},value){return this.offset>value;},state));}
"#);
    assert_eq!(output.lines, lines(&["[11,12]", "[2]", "5", "2", "1", "1"]));
}
#[test]
fn function_call_checks_explicit_receivers_and_preserves_strict_primitive_values() {
    let output = run(r#"
export function main():void{const add=function(this:{n:number},value:number){return this.n+value;};console.log(add.call({n:3},4));
 const multiply:(this:number,value:number)=>number=function(value){return this*value;};console.log(multiply.call(3,4));
 const nullable=function(this:null){return this;};console.log(JSON.stringify(nullable.call(null)));
 const missing=function(this:undefined){return this;};console.log(missing());console.log(missing.call());
 const noThis=function(value:number){return value+1;};console.log(noThis.call({ignored:1},4));
 const union=function(this:number|undefined){return typeof this;};console.log(union());console.log(union.call(2));
 const text=function(this:string){return this+"!";};console.log(text.call("kept"));const invoke=function(this:()=>number){return this();};console.log(invoke.call(()=>6));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "7",
            "12",
            "null",
            "undefined",
            "undefined",
            "5",
            "undefined",
            "number",
            "kept!",
            "6"
        ])
    );
}
#[test]
fn arrows_capture_lexical_this_and_ignore_later_this_arguments() {
    let output = run(r#"
export function main():void{const factory=function(this:{n:number}):(value:number)=>number{const arrow=(value:number)=>this.n+value;return arrow;};
 const state={n:10};const arrow=factory.call(state);state.n=20;console.log(arrow(1));console.log(arrow.call({n:99},2));console.log(JSON.stringify([1,2].map(arrow,{n:99})));
 const nested=function(this:{n:number}):()=>()=>number{return ()=>()=>this.n;};const inner=nested.call(state)();state.n=30;console.log(inner());
 const isolated=function(this:{n:number}):(this:undefined)=>number{return function(this:undefined){return 7;};};console.log(isolated.call(state)());}
"#);
    assert_eq!(output.lines, lines(&["21", "22", "[21,22]", "30", "7"]));
}
#[test]
fn receiver_callback_and_this_argument_evaluate_once_before_length_snapshot() {
    let output = run(r#"
function receiver():number[]{console.log("receiver");return [1,2];}
function callback():(this:{n:number},value:number)=>number{console.log("callback");return function(value){return this.n+value;};}
function context():{n:number}{console.log("thisArg");return {n:10};}
export function main():void{console.log(JSON.stringify(receiver().map(callback(),context())));
 const source=[1];const make:()=>{n:number}=()=>{source.push(2);return {n:3};};console.log(JSON.stringify(source.map(function(this:{n:number},value){return this.n+value;},make())));
 const fn=function(value:number){console.log("body");return value;};console.log(fn.call(context(),varta.json.parse<number>('7')));
 try{receiver().map(callback(),varta.json.parse<{n:number}>('{'));}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "receiver",
            "callback",
            "thisArg",
            "[11,12]",
            "[4,5]",
            "thisArg",
            "body",
            "7",
            "receiver",
            "callback",
            "SyntaxError"
        ])
    );
}
#[test]
fn ignored_this_arguments_keep_effects_even_for_arrows_empty_arrays_and_zero_arity() {
    let output = run(r#"
function effect():number{console.log("this effect");return 9;}
export function main():void{const empty:number[]=[];console.log(JSON.stringify(empty.map(value=>value,effect())));console.log(empty.some(()=>true,effect()));console.log(empty.every(()=>false,effect()));
 console.log((()=>7).call(effect()));console.log([1].map(()=>8,effect()).length);
 const fn=function(this:undefined,total:number,value:number){return total+value;};console.log([1,2].reduce(fn,0));console.log((()=>8).call(varta.result.capture(varta.env.get("VARTA_THIS_PAIR"))));console.log(varta.result.status());}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "this effect",
            "[]",
            "this effect",
            "0",
            "this effect",
            "1",
            "this effect",
            "7",
            "this effect",
            "1",
            "3",
            "<capability denied: env> (VARTA_THIS_PAIR)",
            "8",
            "1"
        ])
    );
}
#[test]
fn this_and_function_syntax_reject_ambiguous_receivers_and_unsupported_features() {
    for source in [
        "export function main():void{const fn=function(this:{n:number}){return this.n;};fn();}",
        "export function main():void{const fn=function(this:{n:number}){return this.n;};fn.call(1);}",
        "export function main():void{[1].map(function(this:{n:number},value){return this.n+value;});}",
        "export function main():void{[1].some(function(this:{n:number},value){return true;},'bad');}",
        "export function main():void{[1].reduce(function(this:{n:number},total,value){return total+value;},0);}",
        "export function main():void{[1].map(function(value){return this.n;},{n:1});}",
        "export function main():void{const fn=function outer(){return outer();};}",
        "export function main():void{const fn=function(this:VartaResult<string>){return 1;};}",
        "export function main():void{const fn=function(value:any=1){return value;};}",
        "export function main():void{const fn=function(...args:number){return 1;};}",
        "export function main():void{const fn=async function(){return 1;};}",
        "export function main():void{const fn=function*(){return 1;};}",
        "export function main():void{const fn:(this:number)=>number=()=>1;}",
        "export function main():void{const fn:(this:number)=>number=function(this:string){return 1;};}",
        "export function main():void{const fn=function(this:{n:number}){return function(){return this.n;};};}",
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
    let source =
        "export function main():void{const fn=function(this:number){return this;};fn.call('bad');}";
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
    let source = "export function main():void{console.log([1].map(function(this:number,value){return this+value;},2).length);}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V11
        )
        .is_err()
    );
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
            lines(&["1"])
        );
    }
}
#[test]
fn this_callbacks_keep_sparse_live_traversal_exceptions_and_finalizer_overrides() {
    let output = run(r#"
export function main():void{const state={count:0};const values:number[]=[1,,3];console.log(JSON.stringify(values.map(function(this:{count:number},value,index,array){this.count=this.count+1;if(index===0){array[1]=2;delete array[2];}return value;},state)));console.log(state.count);
 try{[1,2,3].forEach(function(this:{count:number},value){this.count=this.count+1;try{if(value===2){throw RangeError("stop");}}finally{console.log(value);}},state);}catch(e){if(e instanceof RangeError){console.log(e.message);}}console.log(state.count);
 const fn=function(this:{n:number}){try{return this.n;}finally{return this.n+1;}};console.log(fn.call({n:8}));}
"#);
    assert_eq!(
        output.lines,
        lines(&["[1,2,null]", "2", "1", "2", "stop", "4", "9"])
    );
    for body in [
        "[1].forEach(function(this:number,value){while(true){}},1);",
        "const fn=function(this:number){const values:number[]=[];values.length=1025;};fn.call(1);",
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
}
#[test]
fn dynamic_and_escaped_lexical_this_remain_rooted_through_collection() {
    let source = r#"
function churn():void{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}}
function make():{text:string;n:number}{return {text:"owned"+"😀",n:0};}
export function main():void{
 console.log(JSON.stringify([1,2].map(function(this:{text:string;n:number},value){this.n=this.n+value;churn();return this.text;},make())));
 const factory=function(this:{text:string;n:number}):()=>string{return ()=>this.text;};const escaped=factory.call(make());churn();console.log(escaped());
 const nested=function(this:{text:string;n:number}):()=>()=>string{return ()=>()=>this.text;};const inner=nested.call(make())();churn();console.log(inner());
 const fn=function(this:{text:string;n:number}){try{return this.text+"!";}finally{churn();}};console.log(fn.call(make()));
 const ignored=():string=>{churn();return "kept";};console.log(ignored.call(make()));}
"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(500 * abi::FUEL).unwrap();
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
            "[\"owned😀\",\"owned😀\"]",
            "owned😀",
            "owned😀",
            "owned😀!",
            "kept"
        ])
    );
}
#[test]
fn receiver_discriminator_rejects_same_physical_signature_code_substitution() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::function as layout;
    let bytes = compile(
        "function ordinary(value:number):number{console.log('BAD');return value;}function make():(this:number)=>number{return function(){return this;};}function use(fn:(this:number)=>number):number{return fn.call(3);}export function main():void{}",
    );
    let main = export_index(&bytes, "_start");
    let wrong = main - 3;
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 126);
            data[0] += 2;
            for (name, index) in [("fixture.make", main - 2), ("fixture.use", main - 1)] {
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
    for corrupt in [false, true] {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let handle = instance
            .get_typed_func::<(), i64>(&mut execution.store, "fixture.make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        if corrupt {
            instance
                .get_memory(&mut execution.store, "memory")
                .unwrap()
                .write(
                    &mut execution.store,
                    handle as u32 as usize + layout::CODE_OFFSET as usize,
                    &(wrong as u32).to_le_bytes(),
                )
                .unwrap();
        }
        let result = instance
            .get_typed_func::<i64, f64>(&mut execution.store, "fixture.use")
            .unwrap()
            .call(&mut execution.store, handle);
        assert_eq!(result.is_ok(), !corrupt);
        if !corrupt {
            assert_eq!(result.unwrap(), 3.0);
        }
        assert!(execution.store.data().output.lines.is_empty());
    }
}
#[test]
fn this_and_ignored_receiver_effects_cannot_enter_host_free_json_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    for body in [
        "const fn=(value:boolean)=>value;fn.call(varta.env.get('DENIED'),value);",
        "[value].map(function(this:boolean,item){console.log(item);return item;},true);",
    ] {
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{{body}return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn receiver_slots_count_toward_parameter_limits_and_traversal_uses_default_fuel() {
    let parameters = (0..31)
        .map(|i| format!("p{i}:number"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "export function main():void{{const fn=function(this:number,{parameters}){{return this;}};}}"
    );
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert!(error.to_string().contains("plus environment"), "{error}");
    let source = (0..varta_common::value::MAX_ARRAY_ELEMENTS)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let output = run(&format!(
        "export function main():void{{const values=[{source}];const mapped=values.map(function(this:number,value){{return value+this;}},1);console.log(mapped.length);console.log(mapped.at(-1));}}"
    ));
    assert_eq!(output.lines, lines(&["1024", "1024"]));
}
