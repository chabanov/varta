use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str, profile: LanguageProfile) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, profile).unwrap()
}
fn run(source: &str) -> RunOutput {
    VartaRuntime::new()
        .run_wasm_bytes(&compile(source, LanguageProfile::CheckedF64Utf16V14), &[])
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn spread_binds_fixed_default_optional_and_rest_parameters() {
    let output = run(r#"
function add(a:number,b:number):number{return a+b;}
function defaults(a:number=7,b?:number):number{return b===undefined?a:a+b;}
function collect(prefix:string,...values:number[]):string{return prefix+JSON.stringify(values);}
export function main():void{const values=[2,3];console.log(add(...values));console.log(add(1,...[4]));console.log(add(...[5],6));console.log(defaults(...[]));console.log(defaults(...[undefined],...[]));console.log(defaults(...values));console.log(collect("items",...[1,2],3,...[4]));
 const fn:(a:number,b:number)=>number=add;console.log(fn(...values));const factory=(base:number):((...items:number[])=>number)=>function self(...items:number[]):number{return base+items.reduce((sum,v)=>sum+v,0);};console.log(factory(10)(...values));}
"#);
    assert_eq!(
        output.lines,
        lines(&["5", "5", "11", "7", "7", "5", "items[1,2,3,4]", "5", "15"])
    );
}
#[test]
fn spread_snapshots_each_operand_before_later_effects_and_preserves_child_identity() {
    let output = run(r#"
function values(...items:number[]):string{return JSON.stringify(items);}
function refs(...items:{n:number}[]):number{const item=items[0];return item===undefined?0:item.n;}
export function main():void{const input=[1,2];const change=():number=>{input[0]=9;input.length=1;return 3;};console.log(values(...input,change(),...input));
 const child={n:1};const array=[child];const modify=():{n:number}=>{child.n=8;array[0]={n:4};return child;};console.log(refs(...array,modify()));console.log(JSON.stringify(array));
 const method=function(this:{base:number},...items:number[]):number{return this.base+items.reduce((s,v)=>s+v,0);};const getMethod=():((this:{base:number},...items:number[])=>number)=>{console.log("callee");return method;};const receiver=():{base:number}=>{console.log("receiver");return {base:10};};const operand=():number[]=>{console.log("operand");return [1,2];};console.log(getMethod().call(receiver(),...operand()));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[1,2,3,9]",
            "8",
            "[{\"n\":4}]",
            "callee",
            "receiver",
            "operand",
            "13"
        ])
    );
}
#[test]
fn spread_holes_become_present_undefined_values_and_can_trigger_defaults() {
    let output = run(r#"
function defaults(a:number=4,b:number=5):number{return a+b;}
function rest(...items:(number|undefined)[]):void{console.log(JSON.stringify(items));console.log(Object.hasOwn(items,0));console.log(Object.hasOwn(items,1));}
function required(value:number):void{console.log("BAD");}
export function main():void{const sparse:number[]=[,,];console.log(defaults(...sparse));rest(...sparse);try{const hole:number[]=[,];required(...hole);}catch(e){if(e instanceof TypeError){console.log(e.message);}}const mixed:(number|undefined)[]=[1,undefined];console.log(defaults(...mixed));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "9",
            "[null,null]",
            "1",
            "1",
            "Invalid spread argument type",
            "6"
        ])
    );
}
#[test]
fn spread_checks_all_dynamic_arguments_before_defaults_or_body() {
    let output = run(r#"
function effect():number{console.log("argument");return 1;}
function init():number{console.log("BAD_DEFAULT");return 0;}
function target(a:number,b:number=init()):void{console.log("BAD_BODY");}
export function main():void{try{target(...[]);}catch(e){if(e instanceof TypeError){console.log(e.message);}}try{target(...[1,2],effect());}catch(e){if(e instanceof TypeError){console.log(e.message);}}try{const bad:string[]=["bad"];target(...bad,effect());}catch(e){if(e instanceof TypeError){console.log(e.message);}}finally{console.log("finally");}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "Invalid spread argument count",
            "argument",
            "Invalid spread argument count",
            "argument",
            "Invalid spread argument type",
            "finally"
        ])
    );
}
#[test]
fn spread_operand_and_target_exceptions_preserve_order_and_finalizers() {
    let output = run(r#"
function fail():number[]{console.log("operand");throw RangeError("source");}
function later():number{console.log("BAD");return 1;}
function target(...items:number[]):void{throw TypeError("target");}
export function main():void{try{target(...fail(),later());}catch(e){if(e instanceof RangeError){console.log(e.message);}}finally{console.log("first");}try{target(...[1]);}catch(e){if(e instanceof TypeError){console.log(e.message);}}finally{console.log("second");}}
"#);
    assert_eq!(
        output.lines,
        lines(&["operand", "source", "first", "target", "second"])
    );
}
#[test]
fn spread_accepts_maximum_dense_rest_at_default_fuel_in_each_profile() {
    let values = vec!["1"; 512].join(",");
    let source = format!(
        "function size(...items:number[]):number{{return items.length;}}export function main():void{{const items=[{values}];console.log(size(...items,...items));}}"
    );
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let output = VartaRuntime::new()
            .run_wasm_bytes(&compile(&source, profile), &[])
            .unwrap();
        assert_eq!(output.lines, lines(&["1024"]), "{profile:?}");
    }
}
#[test]
fn spread_argument_limit_is_fatal_and_bypasses_guest_handlers() {
    let values = vec!["1"; 1024].join(",");
    let bytes = compile(
        &format!(
            "function target(...items:number[]):void{{console.log('BAD_BODY');}}export function main():void{{try{{target(...[{values}],2);}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
        ),
        LanguageProfile::CheckedF64Utf16V14,
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
#[test]
fn spread_rejects_unsupported_operands_and_preserves_source_diagnostics() {
    for source in [
        "function f(...items:number[]):void{}export function main():void{f(...'text');}",
        "export function main():void{console.log(...[1]);}",
        "export function main():void{const f=(n:number):number=>n;f.call(...[1]);}",
        "export function main():void{const a=[1];a.map(...[1]);}",
        "export function main():void{varta.json.parse<number>(...'1');}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    let source = "function f(...items:number[]):void{}export function main():void{f(...'text');}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert_eq!(
        error.diagnostic().unwrap().range.start.byte_offset,
        source.find("'text'").unwrap()
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "function f(n:number):void{}export function main():void{f(...[1]);}",
            LanguageProfile::CheckedF64Utf16V11
        )
        .is_err()
    );
}
#[test]
fn spread_keeps_explicit_function_and_result_pairs_outside_element_arrays() {
    let output = run(r#"
function apply(fn:(value:number)=>number,...items:number[]):number{return items.reduce((sum,v)=>sum+fn(v),0);}
function pair(value:VartaResult<string>,...items:number[]):VartaResult<string>{console.log(items.length);return value;}
export function main():void{console.log(apply((n)=>n*2,...[1,2]));const before=varta.result.capture(varta.env.get("VARTA_SPREAD_PAIR"));const after=pair(before,...[1,2]);console.log(after.status===before.status);console.log(after.value===before.value);}
"#);
    assert!(output.lines.contains(&"varta log: 6".into()));
    assert_eq!(
        &output.lines[output.lines.len() - 3..],
        lines(&["2", "1", "1"])
    );
}
#[test]
fn spread_roots_snapshots_callee_receiver_and_children_across_collection() {
    let source = r#"
function churn():void{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}}
export function main():void{const captured="closure"+"!";const fn=function self(this:{text:string},first:string="default",...tail:{text:string}[]):string{churn();const last=tail[0];return captured+this.text+first+(last===undefined?"":last.text);};const child={text:"child"+"!"};const source=[child];const change=():{text:string}=>{source.length=0;churn();child.text=child.text+"?";return child;};console.log(fn.call({text:"receiver"+"!"},...["first"+"!"],...source,change()));}
"#;
    let runtime = VartaRuntime::new();
    let module = runtime
        .validate_module(&compile(source, LanguageProfile::CheckedF64Utf16V14))
        .unwrap();
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
        lines(&["closure!receiver!first!child!?"])
    );
}
#[test]
fn default_parameter_tag_checks_do_not_consume_host_budget_per_array_element() {
    let values = vec!["1"; 1024].join(",");
    let output = run(&format!(
        "export function main():void{{const values=[{values}];console.log(values.map((n:number=0)=>n).length);}}"
    ));
    assert_eq!(output.lines, lines(&["1024"]));
}
#[test]
fn spread_operand_effects_remain_visible_to_helper_purity_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function source():number[]{console.log('effect');return [1];}function fake(value:boolean):boolean{const f=function(...items:number[]):number{return items.length;};f(...source());return value;}export function main():void{varta.json.parse<number>('1');}",
        LanguageProfile::CheckedF64Utf16V14,
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
fn spread_dispatch_rejects_forged_code_with_equal_physical_signature() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::function as layout;
    let bytes = compile(
        "function ordinary(items:number[]):number{console.log('BAD');return items.length;}function make():(...items:number[])=>number{return function(...items){return items.length;};}function use(fn:(...items:number[])=>number):number{return fn(...[1,2]);}export function main():void{}",
        LanguageProfile::CheckedF64Utf16V14,
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
