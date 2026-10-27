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
fn optional_and_default_parameters_distinguish_omission_undefined_and_null() {
    let output = run(r#"
function optional(value?:number):number|undefined{return value;}
function defaulted(value:number=7):number{return value;}
function nullable(value:number|null=5):number|null{return value;}
export function main():void{console.log(optional());console.log(optional(undefined));console.log(optional(3));console.log(defaulted());console.log(defaulted(undefined));console.log(defaulted(0));console.log(defaulted(NaN));console.log(JSON.stringify(nullable(null)));console.log(JSON.stringify(nullable()));
 const fn:(value?:number)=>number=(value=9)=>value;console.log(fn());console.log(fn(2));console.log(fn.call(undefined));
 const expression=function(value="inferred"){return value;};console.log(expression());}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "undefined",
            "undefined",
            "3",
            "7",
            "7",
            "0",
            "NaN",
            "null",
            "5",
            "9",
            "2",
            "9",
            "inferred"
        ])
    );
}
#[test]
fn defaults_run_left_to_right_after_all_arguments_and_see_previous_bindings() {
    let output = run(r#"
function effect(label:string,value:number):number{console.log(label);return value;}
function apply(first:number=effect("first default",3),second:number=effect("second default",first+1)):number{return first+second;}
function requiredLater(first:number=1,second:number):number{return first+second;}
export function main():void{console.log(apply());console.log(apply(undefined,effect("argument",10)));console.log(apply(effect("first argument",5),effect("second argument",6)));console.log(requiredLater(undefined,9));const alias:(first:number|undefined,second:number)=>number=requiredLater;console.log(alias(undefined,10));
 const fn=function(a:number=1,b:number=a+1){a=a+1;return a+b;};console.log(fn());}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "first default",
            "second default",
            "7",
            "argument",
            "first default",
            "13",
            "first argument",
            "second argument",
            "11",
            "10",
            "11",
            "4"
        ])
    );
}
#[test]
fn rest_parameters_create_fresh_dense_arrays_and_accept_contextual_values() {
    let output = run(r#"
function collect(prefix:string,...items:number[]):{prefix:string;items:number[]}{return {prefix:prefix,items:items};}
function sum(...items:number[]):number{return items.reduce((total,value)=>total+value,0);}
export function main():void{console.log(JSON.stringify(collect("empty")));console.log(JSON.stringify(collect("many",1,2,3)));console.log(sum());console.log(sum(1,2,3));
 const fn:(...values:number[])=>number=function(...values){return values.length;};console.log(fn(1,2));console.log(fn.call(undefined,1,2,3));
 const refs=[{n:1}];const mutate=function(...values:{n:number}[]){values[0]= {n:2};return values;};const original=refs[0];if(original!==undefined){console.log(JSON.stringify(mutate(original)));console.log(original.n);}
 console.log(JSON.stringify([1,2].reduce((total:number[],value)=>{total.push(value);return total;},[])));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "{\"prefix\":\"empty\",\"items\":[]}",
            "{\"prefix\":\"many\",\"items\":[1,2,3]}",
            "0",
            "6",
            "2",
            "3",
            "[{\"n\":2}]",
            "1",
            "[1,2]"
        ])
    );
}
#[test]
fn defaults_and_optional_callbacks_use_real_array_arguments_and_missing_tail_defaults() {
    let output = run(r#"
export function main():void{
 console.log(JSON.stringify([1,2].map((value:number=7,index:number=99)=>value+index)));
 console.log(JSON.stringify([1,2].map((value,index,array,extra:number=10)=>value+extra)));
 console.log(JSON.stringify([1,2].map((value,index,array,extra?:number)=>extra)));
 console.log([1,2].some((value?:number)=>value!==undefined));
 const values:(number|undefined)[]=[undefined,2];console.log(JSON.stringify(values.map((value:number=7)=>value)));
 const empty:number[]=[];console.log(empty.every((value:number=9)=>{console.log("BAD");return true;}));
 console.log(JSON.stringify([1].map(function(...args:(number|number[])[]){return args.length;})));}
"#);
    assert_eq!(
        output.lines,
        lines(&["[1,3]", "[11,12]", "[null,null]", "1", "[7,2]", "1", "[3]"])
    );
}
#[test]
fn defaults_use_parameter_scope_before_body_declarations_and_capture_prior_cells() {
    let output = run(r#"
export function main():void{const outer=9;const fn=function(value:number=outer){const outer=1;return value;};console.log(fn());
 let captured:()=>number=()=>0;const factory=function(value:number=3,ignored:number=(()=>{captured=()=>value;return 0;})()):()=>number{value=value+1;return captured;};const first=factory();const second=factory(10);console.log(first());console.log(second());
 const mutate=function(a:number=1,b:number=(()=>{a=a+1;return a;})()){return a+b;};console.log(mutate());
 const named=function self(value:number=3):number{if(value===0){return 7;}return self(value-1);};console.log(named());
}
"#);
    assert_eq!(output.lines, lines(&["9", "4", "11", "4", "7"]));
}
#[test]
fn typed_this_named_self_and_default_initializers_share_checked_call_paths() {
    let output = run(r#"
function method(this:{base:number},value:number=this.base,...others:number[]):number{return value+others.reduce((total,v)=>total+v,0);}
function callback(this:{base:number},value:number=this.base,index:number=9):number{return value+index;}
export function main():void{console.log(method.call({base:10}));console.log(method.call({base:10},undefined,2,3));
 const fn:(this:{base:number},value?:number)=>number=function self(value:number=this.base):number{if(value===0){return this.base;}return self.call(this,value-1);};console.log(fn.call({base:4}));
 console.log(JSON.stringify([1,2].map(callback,{base:8})));}
"#);
    assert_eq!(output.lines, lines(&["10", "15", "4", "[1,3]"]));
}
#[test]
fn default_and_rest_failures_stop_later_initializers_and_propagate_finalizers() {
    let output = run(r#"
function fail():number{console.log("fail");throw RangeError("default");}
function later():number{console.log("BAD_LATER");return 2;}
function args(first:number=fail(),second:number=later()):number{console.log("body");return first+second;}
function input():number{console.log("input");return 9;}
export function main():void{try{args(undefined,input());}catch(e){if(e instanceof RangeError){console.log(e.message);}}finally{console.log("outer");}
 console.log(args(1,2));
 const fn=function(value:{n:number}=varta.json.parse<{n:number}>('{')){console.log("BAD");};try{fn();}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "input",
            "fail",
            "default",
            "outer",
            "body",
            "3",
            "SyntaxError"
        ])
    );
}
#[test]
fn parameter_modes_are_invariant_and_invalid_scope_or_argument_types_are_rejected() {
    for source in [
        "function f(value:number=1):number{return value;}export function main():void{f('bad');}",
        "function f(value:number=1):number{return value;}export function main():void{f(null);}",
        "function f(first:number=1,second:number):number{return second;}export function main():void{f();}",
        "function f(...values:number[]):void{}export function main():void{f(1,'bad');}",
        "function f(value:number=value):number{return value;}export function main():void{}",
        "function f(first:number=second,second:number=2):number{return first;}export function main():void{}",
        "function f(value:number=body):number{const body=1;return value;}export function main():void{}",
        "export function main():void{const fn:(value:number)=>number=(value:number=1)=>value;}",
        "export function main():void{const fn:(...values:number[])=>number=function(values:number[]){return values.length;};}",
        "export function main():void{const fn=function(value:()=>number=()=>1){return value();};}",
        "export function main():void{const fn=function(value?:VartaResult<string>){return 1;};}",
        "export function main():void{const fn=function(...values:number){return 1;};}",
        "export function main():void{const fn=function(value?:number,other:number){return 1;};}",
        "export function main():void{const fn=function(value:number=1){return 1;};fn(...'bad');}",
        "export function main(value:number=1):void{}",
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
        "function f(value:number=1):number{return value;}export function main():void{f('bad');}";
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
    let source = "function f(first:number=second,second:number=2):number{return first;}export function main():void{}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert_eq!(
        error.diagnostic().unwrap().range.start.byte_offset,
        source.find("=second").unwrap() + 1
    );
    let source = "function f(value:number=1,...rest:number[]):number{return value+rest.length;}export function main():void{console.log(f(undefined,2,3));}";
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
            lines(&["3"])
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
fn prologue_roots_rest_values_self_and_result_pairs_survive_collection() {
    let source = r#"
function churn():void{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}}
function defaultText():string{churn();return "default"+"😀";}
function initialize(first:string=defaultText(),second:string=first+"!",...tail:string[]):string{churn();const last=tail[0];if(last===undefined){return second;}else{return second+last;}}
export function main():void{console.log(initialize(undefined,undefined,"tail"+"!"));
 const factory=function self(value:string=defaultText()):()=>string{return ()=>{churn();return value;};};const escaped=factory();churn();console.log(escaped());
 const pair=varta.result.capture(varta.env.get("VARTA_PARAMETER_PAIR"));const keep=function self(value:VartaResult<string>,depth:number=2):VartaResult<string>{try{churn();if(depth===0){return value;}return self(value,depth-1);}finally{churn();}};const retained=keep(pair);console.log(retained.status);console.log(retained.value===pair.value);}
"#;
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
    let output = &execution.store.data().output.lines;
    assert_eq!(&output[..2], lines(&["default😀!tail!", "default😀"]));
    assert_eq!(&output[output.len() - 2..], lines(&["1", "1"]));
}
#[test]
fn rest_mode_discriminator_rejects_code_substitution_with_equal_physical_parameters() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::function as layout;
    let bytes = compile(
        "function ordinary(items:number[]):number{console.log('BAD');return items.length;}function make():(...items:number[])=>number{return function(...items){return items.length;};}function use(fn:(...items:number[])=>number):number{return fn(1,2);}export function main():void{}",
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
            assert_eq!(result.unwrap(), 2.0);
        }
        assert!(execution.store.data().output.lines.is_empty());
    }
}
#[test]
fn initializer_resource_limits_and_rest_capacity_are_enforced() {
    for body in [
        "const fn=function(value:number=(()=>{while(true){}return 1;})()){console.log('BAD_BODY');};fn();",
        "const fn=function self(value:number=self()):number{return value;};fn();",
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
    let args = vec!["{n:1}"; varta_common::value::MAX_ARRAY_ELEMENTS + 1].join(",");
    let source = format!(
        "function collect(...values:{{n:number}}[]):void{{}}export function main():void{{collect({args});}}"
    );
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("rest array exceeds element bound"),
        "{error}"
    );
    let args = (0..31)
        .map(|i| format!("p{i}:number=1"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "export function main():void{{const fn=function self({args}):number{{return 1;}};}}"
    );
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert!(error.to_string().contains("plus environment"), "{error}");
}
#[test]
fn initializer_and_rest_operand_host_effects_remain_visible_to_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for body in [
        "const fn=function(n:number=effect()){return n;};fn();",
        "const fn=function(...names:string[]){return names.length;};fn(varta.env.get('DENIED'));",
    ] {
        let bytes = compile(&format!(
            "function effect():number{{console.log('effect');return 1;}}function fake(value:boolean):boolean{{{body}return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn reduce_callbacks_accept_default_parameters_without_widening_the_accumulator() {
    let output = run(r#"
function add(total:number=0,value:number=0):number{return total+value;}
export function main():void{console.log([1,2,3].reduce(add,0));console.log([1,2,3].reduce(add));console.log(JSON.stringify([1,2].reduce((total:number[]=[],value)=>{total.push(value);return total;},[])));}
"#);
    assert_eq!(output.lines, lines(&["6", "6", "[1,2]"]));
}
