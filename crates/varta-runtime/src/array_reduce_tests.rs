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
fn reduce_infers_accumulator_parameters_and_preserves_named_and_prefix_callbacks() {
    let output = run(r#"
function add(total:number,value:number):number{return total+value;}
export function main():void{
 const values=[1,2,3];console.log(values.reduce(add));console.log(values.reduce(add,10));
 console.log(values.reduce((total,value,index,array)=>{if(array!==values){throw Error("alias");}return total+value+index;},0));
 console.log(values.reduce(total=>total+1));console.log(values.reduce(()=>9));
 console.log(values.reduce((total,value)=>total+"!","start"));
 console.log(JSON.stringify(values.reduce((total,value)=>{total.push(value*2);return total;},values.slice(0,0))));
 console.log(JSON.stringify(values.reduce((total,value)=>({n:total.n+value}),{n:10})));
 console.log([true,false,true].reduce((total,value)=>total&&value));
 console.log(["a","b"].reduce((total,value)=>total+value));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "6",
            "16",
            "9",
            "3",
            "9",
            "start!!!",
            "[2,4,6]",
            "{\"n\":16}",
            "0",
            "ab"
        ])
    );
}

#[test]
fn reduce_distinguishes_no_seed_explicit_undefined_and_holes() {
    let output = run(r#"
export function main():void{
 const empty:number[]=[];console.log(empty.reduce((total,value)=>total+value,7));
 try{empty.reduce((total,value)=>total+value);}catch(e){if(e instanceof TypeError){console.log(e.name);console.log(e.message);}}finally{console.log("finally");}
 const holes:number[]=[,,,];try{holes.reduce((total,value)=>total+value);}catch(e){if(e instanceof TypeError){console.log(e.name);}}
 console.log(holes.reduce((total,value)=>{throw Error("BAD");},5));
 const sparse:number[]=[,,4,,6,];console.log(sparse.reduce((total,value,index)=>{console.log(index);return total+value;}));
 const lone:number[]=[,,8,,];console.log(lone.reduce(()=>{throw Error("BAD");}));
 const present:(number|undefined)[]=[,,undefined];console.log(present.reduce(()=>{throw Error("BAD");}));
 console.log(empty.reduce((total,value)=>undefined,undefined));
 console.log([1,2].reduce((total,value)=>{console.log(total);return undefined;},undefined));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "7",
            "TypeError",
            "Reduce of empty array with no initial value",
            "finally",
            "TypeError",
            "5",
            "4",
            "10",
            "8",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined"
        ])
    );
}

#[test]
fn some_and_every_short_circuit_and_skip_holes_with_live_mutations() {
    let output = run(r#"
export function main():void{
 console.log([1,2,3].some((value,index,array)=>{console.log(index);if(index===2){throw Error("BAD");}return value===2;}));
 console.log([1,2,3].every((value,index,array)=>{console.log(index);if(index===2){throw Error("BAD");}return value<2;}));
 const empty:number[]=[];console.log(empty.some(()=>{throw Error("BAD");}));console.log(empty.every(()=>{throw Error("BAD");}));
 const holes:number[]=[,,,];console.log(holes.some(()=>{throw Error("BAD");}));console.log(holes.every(()=>{throw Error("BAD");}));
 const source:number[]=[1,,3,4];console.log(source.some((value,index,array)=>{console.log(value);if(index===0){array[1]=2;delete array[2];array[3]=7;array.push(99);}return value===99;}));
 const other=[1,2,3];console.log(other.every((value,index,array)=>{console.log(value);if(index===0){array.length=0;array[2]=8;array[3]=10;}return true;}));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "0", "1", "1", "0", "1", "0", "0", "1", "0", "1", "1", "2", "7", "0", "1", "8", "1"
        ])
    );
}

#[test]
fn some_and_every_use_checked_truthiness_and_prefix_arity() {
    let output = run(r#"
function nothing():void{}
export function main():void{
 console.log([0,0,2].some(value=>value));console.log([1,2].every(value=>value));console.log([1,0].every(value=>value));
 console.log([1].some(()=>""));console.log([1].some(()=>"😀"));console.log([1].every(()=>NaN));
 console.log([1].some(()=>({n:1})));const empty:number[]=[];console.log([1].every(()=>empty));
 console.log([1].some(nothing));console.log([1].every(nothing));
 console.log([1].some(()=>Error("truthy")));console.log([1].every(()=>()=>7));
 console.log([undefined].some(value=>value));console.log([null].every(value=>value));
 console.log([1,2].some(value=>{return value===2;}));console.log([1,2].every(()=>true));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1", "1", "0", "0", "1", "0", "1", "1", "0", "0", "1", "1", "0", "0", "1", "1"
        ])
    );
}

#[test]
fn reduce_reads_live_slots_and_retains_pre_callback_values_and_length_snapshot() {
    let output = run(r#"
export function main():void{
 const values:number[]=[,1,,3,4];console.log(values.reduce((total,value,index,array)=>{console.log(index);if(index===3){array[4]=9;array.push(100);}return total+value;}));
 const source=[1,2,3,4];console.log(source.reduce((total,value,index,array)=>{console.log(value);if(index===0){array.length=0;array[2]=7;array[4]=100;}return total+value;},0));
 const records=[{n:1},{n:2}];const original=records[0];const result=records.reduce((total,value,index,array)=>{array.length=0;value.n=8;return value;}, {n:0});console.log(result===original);console.log(result.n);
 const grown=[1,2,3];console.log(grown.reduce((total,value,index,array)=>{if(index===0){array.length=1;array[2]=6;}return total+value;},0));}
"#);
    assert_eq!(
        output.lines,
        lines(&["3", "4", "13", "1", "7", "8", "1", "8", "7"])
    );
}

#[test]
fn reducer_operands_evaluate_once_in_source_order_before_the_length_snapshot() {
    let output = run(r#"
function receiver():number[]{console.log("receiver");return [1];}
function callback():(total:number,value:number)=>number{console.log("callback");return (total,value)=>total+value;}
function seed():number{console.log("seed");return 10;}
export function main():void{
 console.log(receiver().reduce(callback(),seed()));
 let source=[1];const original=source;const make:()=>((total:number,value:number)=>number)=()=>{console.log("make");source=[99];original.push(2);return (total,value)=>total+value;};
 const initial:()=>number=()=>{console.log("initial");original.push(3);return 5;};console.log(source.reduce(make(),initial()));console.log(source===original);
 const values=[1];const predicate:()=>((value:number)=>boolean)=()=>{values.push(2);return value=>value===2;};console.log(values.some(predicate()));
 try{receiver().reduce(callback(),varta.json.parse<number>('{'));}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "receiver",
            "callback",
            "seed",
            "11",
            "make",
            "initial",
            "11",
            "0",
            "1",
            "receiver",
            "callback",
            "SyntaxError"
        ])
    );
}

#[test]
fn reductions_propagate_exceptions_and_finalizer_overrides() {
    let output = run(r#"
export function main():void{
 let visits=0;try{[1,2,3].reduce((total,value)=>{visits=visits+1;if(value===2){throw RangeError("stop");}return total+value;},0);}catch(e){if(e instanceof RangeError){console.log(e.message);}}finally{console.log(visits);}
 try{[1,2].some(value=>{throw "some";});}catch(e){console.log(e);}finally{console.log("some finally");}
 try{[1,2].every(value=>{varta.json.parse<number>('{');return true;});}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}
 console.log([1,2].reduce((total,value)=>{try{return total+value;}finally{if(value===2){return 90;}}},0));
 console.log([1,2].some(value=>{try{return false;}finally{return true;}}));console.log([1,2].every(value=>{try{return true;}finally{return false;}}));
 console.log([1,2].reduce((total,value)=>total+value));console.log(varta.result.status());}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "stop",
            "2",
            "some",
            "some finally",
            "SyntaxError",
            "90",
            "1",
            "0",
            "3",
            "0"
        ])
    );
}

#[test]
fn reducer_accumulators_support_unions_function_values_errors_and_multi_slot_results() {
    let output = run(r#"
function keep(total:VartaResult<string>):VartaResult<string>{return total;}
export function main():void{
 const seed:number|string=0;const joined=[1,2].reduce((total,value)=>"text",seed);console.log(joined);
 const refs:{n:number}|undefined=undefined;const result=[1,2].reduce((total,value)=>({n:value}),refs);console.log(JSON.stringify(result));
 const fn:()=>number=()=>10;const final=[1,2].reduce((total,value)=>()=>total()+value,fn);console.log(final());
 const error=Error("kept");console.log([1,2].reduce((total,value)=>total,error)===error);
 const pair=varta.result.capture(varta.env.get("VARTA_REDUCE_DENIED"));const kept=[1,2].reduce(keep,pair);console.log(kept.status);console.log(kept.value===pair.value);console.log(varta.result.status());
 const empty:number[]=[];console.log(empty.reduce((total,value)=>total,fn)===fn);
 console.log([undefined].reduce((total,value)=>undefined));}
"#);
    assert_eq!(&output.lines[..4], lines(&["text", "{\"n\":2}", "13", "1"]));
    assert_eq!(
        &output.lines[output.lines.len() - 5..],
        lines(&["1", "1", "1", "1", "undefined"]).as_slice()
    );
}

#[test]
fn collection_methods_reject_unsupported_types_arities_and_profiles_with_source_diagnostics() {
    for source in [
        "export function main():void{[1].reduce();}",
        "export function main():void{[1].reduce((total,value)=>total+value,0,1);}",
        "export function main():void{[1].reduce(0);}",
        "export function main():void{[1].reduce((total:string,value)=>total);}",
        "export function main():void{[1].reduce((total,value)=>'bad');}",
        "function bad(total:number):string{return 'bad';}export function main():void{[1].reduce(bad,0);}",
        "export function main():void{[1].reduce((total,value,index,array,extra)=>total,0);}",
        "export function main():void{[1].some();}",
        "export function main():void{[1].some(value=>true,undefined,undefined);}",
        "export function main():void{[1].every(1);}",
        "export function main():void{[1].every((value,index,array,extra)=>true);}",
        "export function main():void{[1].some((value:string)=>true);}",
        "export function main():void{[1].every(value=>varta.result.capture(varta.env.get('X')));}",
        "export function main():void{[1].reduce(async (total,value)=>total+value,0);}",
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
    let source = "export function main():void{[1].reduce((total:string,value)=>total);}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.code, varta_common::DiagnosticCode::TypeError);
    assert_eq!(
        diagnostic.range.start.byte_offset,
        source.find("total:string").unwrap()
    );
    for method in [
        "some(value=>true)",
        "every(value=>true)",
        "reduce((total,value)=>total+value,0)",
    ] {
        let source = format!("export function main():void{{console.log([1,2].{method});}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V11
            )
            .is_err()
        );
        for profile in [
            LanguageProfile::CheckedF64Utf16V12,
            LanguageProfile::CheckedF64Utf16V13,
            LanguageProfile::CheckedF64Utf16V14,
        ] {
            let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
            VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
        }
    }
}

#[test]
fn collection_callbacks_preserve_fatal_resource_boundaries() {
    for body in [
        "[1].reduce((total,value)=>{while(true){}return total;},0);",
        "[1].some(value=>{while(true){}return true;});",
        "[1].every(value=>{while(true){}return true;});",
        "[1].reduce((total,value)=>{const array:number[]=[];array.length=1025;return total;},0);",
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
fn collection_methods_handle_1024_elements_with_default_fuel() {
    let source = (0..varta_common::value::MAX_ARRAY_ELEMENTS)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    // Run separately: each complete traversal must fit the ordinary execution
    // budget without granting the program extra fuel for the implementation.
    for (method, expected) in [
        ("reduce((total,value)=>total+value,0)", "523776"),
        ("reduce((total,value)=>total+value)", "523776"),
        ("some(value=>value===1023)", "1"),
        ("every(value=>value>=0)", "1"),
    ] {
        let output = run(&format!(
            "export function main():void{{const values=[{source}];console.log(values.{method});}}"
        ));
        assert_eq!(output.lines, lines(&[expected]), "{method}");
    }
}

#[test]
fn reducer_and_predicate_roots_survive_collection_and_detached_arrays() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<260;i=i+1){const garbage=[payload+"!",payload+"?"];garbage.slice();}}
export function main():void{
 const values=["first"+"!","second"+"!"];const reduced=values.reduce((total,value,index,array)=>{array.length=0;churn();console.log(value);return total+value;});churn();console.log(reduced);
 const record=[1,2].reduce((total,value)=>{churn();return {text:total.text+"!",n:total.n+value};},{text:"seed"+"😀",n:0});churn();console.log(JSON.stringify(record));
 const strings=["kept"+"!"];console.log(strings.some((value,index,array)=>{array.length=0;churn();console.log(value);return true;}));
 const objects=[{text:"retained"+"!"}];console.log(objects.every((value,index,array)=>{array.length=0;churn();console.log(value.text);return true;}));
 const output=[1,2].reduce((fn,value)=>()=>fn()+value,()=>10);churn();console.log(output());
 console.log([1,2].reduce((total,value)=>[3,4].reduce((inner,other)=>inner+value+other,total),0));}
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
            "second!",
            "first!second!",
            "{\"text\":\"seed😀!!\",\"n\":3}",
            "kept!",
            "1",
            "retained!",
            "1",
            "13",
            "20"
        ])
    );
}

#[test]
fn empty_collections_validate_callback_metadata_even_when_no_call_is_needed() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::{function as layout, value};
    for method in ["some(fn)", "every(fn)", "reduce(fn,true)", "reduce(fn)"] {
        let bytes = compile(&format!(
            "function make():(value:boolean)=>boolean{{return value=>{{console.log('BAD_CALLBACK');return value;}};}}function empty(fn:(value:boolean)=>boolean):boolean{{const values:boolean[]=[];return values.{method};}}export function main():void{{}}"
        ));
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
            let mut execution =
                crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
            let instance = execution.instantiate(&module).unwrap();
            let handle = instance
                .get_typed_func::<(), i64>(&mut execution.store, "fixture.make")
                .unwrap()
                .call(&mut execution.store, ())
                .unwrap();
            if mutation != 0 {
                let ptr = handle as u32 as usize;
                let at = match mutation {
                    1 => ptr + layout::SIGNATURE_OFFSET as usize,
                    2 => ptr + layout::CODE_OFFSET as usize,
                    _ => ptr - abi::STRING_HEADER_BYTES + value::HEAP_FLAGS_OFFSET,
                };
                instance
                    .get_memory(&mut execution.store, "memory")
                    .unwrap()
                    .write(&mut execution.store, at, &u32::MAX.to_le_bytes())
                    .unwrap();
            }
            let result = instance
                .get_typed_func::<i64, i32>(&mut execution.store, "fixture.empty")
                .unwrap()
                .call(&mut execution.store, handle);
            assert_eq!(
                result.is_ok(),
                mutation == 0,
                "{method}, mutation {mutation}"
            );
            if mutation == 0 {
                assert_eq!(
                    crate::exceptions::finish(&instance, &mut execution.store).is_err(),
                    method == "reduce(fn)"
                );
            }
            assert!(execution.store.data().output.lines.is_empty());
        }
    }
}

#[test]
fn reduction_and_quantifier_wrappers_do_not_hide_host_effects_from_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for method in [
        "reduce((total,current)=>{console.log(total);return current;},false)",
        "some(current=>{console.log(current);return false;})",
        "every(current=>{console.log(current);return true;})",
    ] {
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{[value].{method};return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
