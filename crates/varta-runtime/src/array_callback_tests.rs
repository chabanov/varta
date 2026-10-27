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
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn array_callbacks_infer_parameters_and_results_and_use_real_closures() {
    let output = run(r#"
function double(value:number):number{return value*2;}
export function main():void{const values=[1,2,3];const offset=10;let sum=0;
 console.log(JSON.stringify(values.map(value=>value+offset)));console.log(JSON.stringify(values.map(double)));
 console.log(JSON.stringify(values.filter(value=>value>1)));console.log(values.forEach((value,index,array)=>{sum=sum+value+index;if(array!==values){throw Error("alias");}}));console.log(sum);
 console.log(values.find(value=>value===2));console.log(values.find(value=>value===99));
 console.log(JSON.stringify(values.map((value,index)=>({value:value,index:index}))));console.log(JSON.stringify(values.map(value=>"n")));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[11,12,13]",
            "[2,4,6]",
            "[2,3]",
            "undefined",
            "9",
            "2",
            "undefined",
            "[{\"value\":1,\"index\":0},{\"value\":2,\"index\":1},{\"value\":3,\"index\":2}]",
            "[\"n\",\"n\",\"n\"]"
        ])
    );
}
#[test]
fn callback_factories_and_receivers_evaluate_once_before_the_length_snapshot() {
    let output = run(r#"
function receiver():number[]{console.log("receiver");return [1,2];}
function callback():(value:number,index:number,array:number[])=>number{console.log("callback");return (value,index,array)=>{console.log(index);array.push(9);if(index===0){array[1]=5;}return value;};}
export function main():void{console.log(JSON.stringify(receiver().map(callback())));
 let source=[1];const old=source;const factory:()=>((value:number)=>number)=()=>{source=[99];old.push(2);return value=>value+1;};console.log(JSON.stringify(source.map(factory())));console.log(source===old);console.log(JSON.stringify(source));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "receiver", "callback", "0", "1", "[1,5]", "[2,3]", "0", "[99]"
        ])
    );
}
#[test]
fn filter_and_foreach_skip_removed_slots_but_read_live_regrown_values() {
    let output = run(r#"
export function main():void{
 const values=[1,2,3];console.log(JSON.stringify(values.filter((value,index,array)=>{if(index===0){array.pop();array.pop();array[0]=7;}console.log(value);return true;})));console.log(JSON.stringify(values));
 const each=[1,2,3];each.forEach((value,index,array)=>{console.log(value);if(index===0){array.pop();array.pop();}});
 const grown=[1,2,3];console.log(JSON.stringify(grown.filter((value,index,array)=>{if(index===0){array.pop();array.pop();array.push(8,9,10);}return true;})));
 const refs=[{n:1}];const kept=refs.filter((value,index,array)=>{array.pop();value.n=2;return true;});console.log(JSON.stringify(kept));console.log(refs.length);}
"#);
    assert_eq!(
        output.lines,
        lines(&["1", "[1]", "[7]", "1", "[1,8,9]", "[{\"n\":2}]", "0"])
    );
}
#[test]
fn find_visits_deleted_indices_as_undefined_and_returns_the_pre_callback_value() {
    let output = run(r#"
export function main():void{
 const values=[1,2,3];console.log(values.find((value,index,array)=>{console.log(value);if(index===0){array.pop();array.pop();}return index===2;}));
 const second=[1,2];console.log(second.find((value,index,array)=>{array[0]=9;return true;}));console.log(JSON.stringify(second));
 const refs=[{n:1}];const result=refs.find((value,index,array)=>{array.pop();return true;});if(result!==undefined){console.log(result.n);}
 const nums=[1,2];console.log(nums.find(value=>{if(value!==undefined){return value>1;}return false;}));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1",
            "undefined",
            "undefined",
            "undefined",
            "1",
            "[9,2]",
            "1",
            "2"
        ])
    );
}
#[test]
fn empty_arrays_void_results_truthy_predicates_and_prefix_arities_are_checked() {
    let output = run(r#"
function nothing():void{}
export function main():void{const empty:number[]=[];console.log(JSON.stringify(empty.map(()=>1)));console.log(JSON.stringify(empty.filter(()=>true)));console.log(empty.find(()=>true));console.log(empty.forEach(()=>{throw Error("BAD");}));
 const values=[0,1,2];console.log(JSON.stringify(values.map(()=>5)));console.log(JSON.stringify(values.map(nothing)));console.log(JSON.stringify(values.filter(value=>value)));console.log(JSON.stringify(values.filter(value=>value===0?"":"x")));console.log(JSON.stringify(values.filter(value=>({n:value}))));console.log(JSON.stringify(values.filter(()=>NaN)));console.log(JSON.stringify(values.filter(()=>{})));console.log(values.find(()=>true));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[]",
            "[]",
            "undefined",
            "undefined",
            "[5,5,5]",
            "[null,null,null]",
            "[1,2]",
            "[1,2]",
            "[0,1,2]",
            "[]",
            "[]",
            "0"
        ])
    );
}

#[test]
fn callback_exceptions_stop_iteration_preserve_mutations_and_unwind_finalizers() {
    let output = run(r#"
export function main():void{const values=[1,2,3];let visits=0;
 try{values.map(value=>{visits=visits+1;if(value===2){throw RangeError("stop😀");}return value;});}catch(e){if(e instanceof RangeError){console.log(e.message);}}finally{console.log(visits);}
 try{values.filter(value=>{varta.json.parse<number>('{');return true;});}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}
 try{values.forEach(value=>{throw "each";});}catch(e){console.log(e);}
 try{values.find(value=>{throw "find";});}catch(e){console.log(e);}
 console.log(JSON.stringify(values.map(value=>value+1)));console.log(varta.result.status());}
"#);
    assert_eq!(
        output.lines,
        lines(&["stop😀", "2", "SyntaxError", "each", "find", "[2,3,4]", "0"])
    );
}
#[test]
fn map_holes_and_callback_resource_limits_remain_fatal() {
    for body in [
        "const values=[1,2];values.map((value,index,array)=>{array.pop();return value;});",
        "const values=[1];values.forEach(value=>{while(true){}});",
        "const values=[1];values.map((value):string=>{let text='x';for(let i=0;i<22;i=i+1){text=text+text;}return text;});",
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
fn callback_type_arity_profiles_and_wrapper_budgets_are_bounded() {
    for source in [
        "export function main():void{[1].map(1);}",
        "export function main():void{[1].map();}",
        "export function main():void{[1].map(value=>value,undefined,undefined);}",
        "export function main():void{[1].map((value,index,array,extra)=>value);}",
        "export function main():void{[1].map((value:string)=>value);}",
        "export function main():void{[1].find((value:number)=>true);}",
        "export function main():void{[1].map(value=>()=>value);}",
        "export function main():void{[1].map(value=>varta.result.capture(varta.env.get('DENIED')));}",
        "export function main():void{[1].filter(value=>varta.result.capture(varta.env.get('DENIED')));}",
        "export function main():void{[1].map(async value=>value);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V12
            )
            .is_err(),
            "{source}"
        );
    }
    let source = "export function main():void{[1].map((value:string)=>value);}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V12,
    )
    .unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.code, varta_common::DiagnosticCode::TypeError);
    assert_eq!(
        diagnostic.range.start.byte_offset,
        source.find("value:string").unwrap()
    );
    assert!(diagnostic.message.contains("expected number, found string"));
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{[1].map(value=>value);}",
            LanguageProfile::CheckedF64Utf16V11
        )
        .is_err()
    );
    let calls = "values.map(fn);".repeat(varta_common::abi::MAX_USER_FUNCTIONS);
    let source = format!(
        "function fn(value:number):number{{return value;}}export function main():void{{const values=[1];{calls}}}"
    );
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V12,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("too many functions including array callback wrappers"),
        "{error}"
    );
}
#[test]
fn callback_graphs_keep_detached_strings_and_aggregate_results_alive_through_collection() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<260;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
export function main():void{const suffix="😀";const source=["kept"+"!"];const result=source.filter((value,index,array)=>{array.pop();churn();return true;});console.log(JSON.stringify(result));
 const input=["find"+"!"];const found=input.find((value,index,array)=>{array.pop();churn();return true;});console.log(found);
 const mapped=[1,2].map((value):{text:string,n:number}=>{churn();return {text:"owned"+suffix,n:value};});churn();console.log(JSON.stringify(mapped));
 const nested=[1,2].map(value=>[3,4].map(other=>value+other));console.log(JSON.stringify(nested));}
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
        lines(&[
            "[\"kept!\"]",
            "find!",
            "[{\"text\":\"owned😀\",\"n\":1},{\"text\":\"owned😀\",\"n\":2}]",
            "[[4,5],[5,6]]"
        ])
    );
}
#[test]
fn array_callback_wrappers_do_not_hide_host_effects_from_json_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{[value].forEach(item=>console.log(item));return value;}export function main():void{varta.json.parse<number>('1');}",
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
fn empty_iteration_checks_callback_metadata_and_code_without_invoking_it() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::{function as layout, value};
    let bytes = compile(
        "function make():(value:number)=>number{return value=>{console.log('BAD_CALLBACK');return value;};}function empty(fn:(value:number)=>number):number[]{const values:number[]=[];return values.map(fn);}export function main():void{}",
    );
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 126);
            data[0] += 2;
            for (name, index) in [("fixture.make", main - 2), ("fixture.empty", main - 1)] {
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
    for mutation in 0..4 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let handle = instance
            .get_typed_func::<(), i64>(&mut execution.store, "fixture.make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let ptr = handle as u32 as usize;
        if mutation != 0 {
            let at = match mutation {
                1 => ptr + layout::SIGNATURE_OFFSET as usize,
                2 => ptr + layout::CODE_OFFSET as usize,
                _ => ptr - abi::STRING_HEADER_BYTES + value::HEAP_FLAGS_OFFSET,
            };
            memory
                .write(&mut execution.store, at, &u32::MAX.to_le_bytes())
                .unwrap();
        }
        let result = instance
            .get_typed_func::<i64, i64>(&mut execution.store, "fixture.empty")
            .unwrap()
            .call(&mut execution.store, handle);
        assert_eq!(result.is_ok(), mutation == 0);
        assert!(execution.store.data().output.lines.is_empty());
    }
}

#[test]
fn map_preserves_the_full_bounded_length_with_default_execution_fuel() {
    let source = (0..varta_common::value::MAX_ARRAY_ELEMENTS)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let output = run(&format!(
        "export function main():void{{const values=[{source}];const result=values.map(value=>value+1);console.log(result.length);console.log(result.at(-1));console.log(values.length);}}"
    ));
    let bound = varta_common::value::MAX_ARRAY_ELEMENTS.to_string();
    assert_eq!(output.lines, lines(&[&bound, &bound, &bound]));
}

#[test]
fn foreach_discards_multi_slot_results_and_returned_capture_bearing_functions() {
    let output = run(r#"
function pair(value:number):VartaResult<string>{return varta.result.capture(varta.env.get("VARTA_ARRAY_CALLBACK_PAIR"));}
export function main():void{console.log([1].forEach(pair));console.log([1].forEach(value=>()=>value));console.log(varta.result.status());console.log(JSON.stringify([1].map((value,index,array)=>{array.pop();return value;})));}
"#);
    assert_eq!(
        &output.lines[output.lines.len() - 4..],
        lines(&["undefined", "undefined", "1", "[1]"])
    );
}
