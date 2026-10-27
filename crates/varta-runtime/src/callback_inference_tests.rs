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
fn callback_blocks_infer_strings_booleans_records_arrays_and_local_return_types() {
    let output = run(r#"
export function main():void{
 console.log(JSON.stringify([1,2].map(value=>{const text="item";return text;})));
 console.log(JSON.stringify([1,2].map(value=>{return value===2;})));
 console.log(JSON.stringify([1,2].map(value=>{return {n:value};})));
 console.log(JSON.stringify([1,2].map(value=>{return [value,value+1];})));
 console.log(JSON.stringify([1,2].map(value=>{for(let i=0;i<1;i=i+1){if(value===1){return "loop";}}return "end";})));
 console.log(JSON.stringify([1,2].map(value=>{switch(value){case 1:return "one";default:return "other";}})));
 console.log(JSON.stringify([1,2].map(value=>{const fn:()=>number=()=>{return 99;};return "outer";})));
 console.log(JSON.stringify([1,2].map(value=>{return;})));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[\"item\",\"item\"]",
            "[false,true]",
            "[{\"n\":1},{\"n\":2}]",
            "[[1,2],[2,3]]",
            "[\"loop\",\"end\"]",
            "[\"one\",\"other\"]",
            "[\"outer\",\"outer\"]",
            "[null,null]"
        ])
    );
}

#[test]
fn inferred_returns_join_supported_unions_bare_returns_and_fallthrough() {
    let output = run(r#"
export function main():void{
 console.log(JSON.stringify([1,2].map(value=>{if(value===1){return "one";}return 2;})));
 console.log(JSON.stringify([1,2].map(value=>{if(value===1){return "one";}})));
 console.log(JSON.stringify([1,2].map(value=>{if(value===1){return;}return {n:value};})));
 console.log(JSON.stringify([1,2].map(value=>{if(value===1){return null;}return [value];})));
 console.log(JSON.stringify([1,2].map(value=>{try{if(value===1){throw "x";}return {n:value};}catch(e){return undefined;}})));
 console.log(JSON.stringify([1,2].map(value=>{return undefined;})));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[\"one\",2]",
            "[\"one\",null]",
            "[null,{\"n\":2}]",
            "[null,[2]]",
            "[null,{\"n\":2}]",
            "[null,null]"
        ])
    );
}

#[test]
fn predicate_blocks_infer_truthy_result_types_and_keep_short_circuit_behavior() {
    let output = run(r#"
export function main():void{
 console.log(JSON.stringify([0,1,2].filter(value=>{return value;})));
 console.log(JSON.stringify([0,1,2].filter(value=>{if(value===1){return "yes";}return "";})));
 console.log([1,2].some(value=>{console.log(value);return {n:value};}));
 console.log([1,2].every(value=>{console.log(value);return value===1?"ok":"";}));
 console.log([1,2].find(value=>{return value===2?1:0;}));
 console.log([1,2].some(value=>{if(value===2){return "found";}}));
 console.log([1,2].every(value=>{return ()=>value;}));}
"#);
    assert_eq!(
        output.lines,
        lines(&["[1,2]", "[1]", "1", "1", "1", "2", "0", "2", "1", "1"])
    );
}

#[test]
fn reduce_seeds_use_accumulator_and_return_annotations_for_empty_and_nested_literals() {
    let output = run(r#"
function collect(total:number[],value:number):number[]{total.push(value);return total;}
export function main():void{
 console.log(JSON.stringify([1,2].reduce((total:number[],value)=>{total.push(value);return total;},[])));
 console.log(JSON.stringify([1,2].reduce((total,value):number[]=>{total.push(value*2);return total;},[])));
 console.log(JSON.stringify([1,2].reduce(collect,[])));
 console.log(JSON.stringify([1,2].reduce((total:{items:number[]},value)=>{total.items.push(value);return total;},{items:[]})));
 console.log(JSON.stringify([1].reduce((total:{n?:number},value)=>{total.n=value;return total;},{})));
 console.log([1,2].reduce((total:number|string,value)=>"joined",0));
 console.log(JSON.stringify([1].reduce((total:{n:number}|undefined,value)=>({n:value}),undefined)));
 console.log(JSON.stringify([1].reduce(((total:number[],value)=>{total.push(value);return total;}),([]))));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[1,2]",
            "[2,4]",
            "[1,2]",
            "{\"items\":[1,2]}",
            "{\"n\":1}",
            "joined",
            "{\"n\":1}",
            "[1]"
        ])
    );
}

#[test]
fn reduce_uses_surrounding_aggregate_context_without_widening_numeric_seeds() {
    let output = run(r#"
function collect():number[]{return [1,2].reduce((total,value)=>{total.push(value);return total;},[]);}
function accept(values:number[]):void{console.log(JSON.stringify(values));}
export function main():void{
 const values:number[]=[1,2].reduce((total,value)=>{total.push(value);return total;},[]);console.log(JSON.stringify(values));
 const nested:{items:number[]}=[1,2].reduce((total,value)=>{total.items.push(value);return total;},{items:[]});console.log(JSON.stringify(nested));
 const optional:{n?:number}=[1].reduce((total,value)=>{total.n=value;return total;},{});console.log(JSON.stringify(optional));
 const wider:number[]|undefined=[1].reduce((total,value)=>{total.push(value);return total;},[]);console.log(JSON.stringify(wider));
 const numeric:number|string=[1,2].reduce((total,value)=>total+value,0);console.log(numeric);
 accept([3].reduce((total,value)=>{total.push(value);return total;},[]));console.log(JSON.stringify(collect()));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[1,2]",
            "{\"items\":[1,2]}",
            "{\"n\":1}",
            "[1]",
            "3",
            "[3]",
            "[1,2]"
        ])
    );
}

#[test]
fn reducer_factories_evaluate_once_before_contextual_seeds_and_mutation_snapshot() {
    let output = run(r#"
function receiver():number[]{console.log("receiver");return [1,2];}
function factory():(total:{items:number[]},value:number)=>{items:number[]}{console.log("callback");return (total,value)=>{total.items.push(value);return total;};}
function seedItem():number{console.log("seed");return 7;}
export function main():void{
 console.log(JSON.stringify(receiver().reduce(factory(),{items:[seedItem()]})));
 let calls=0;const fn:(total:number[],value:number)=>number[]=(total,value)=>{calls=calls+1;total.push(value);return total;};
 console.log(JSON.stringify([1,2].reduce(fn,[])));console.log(calls);
 const source=[1];const make:()=>((total:number[],value:number)=>number[])=()=>{source.push(2);return (total,value)=>{total.push(value);return total;};};console.log(JSON.stringify(source.reduce(make(),[])));
 try{receiver().reduce(factory(),{items:[varta.json.parse<number>('{')]});}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "receiver",
            "callback",
            "seed",
            "{\"items\":[7,1,2]}",
            "[1,2]",
            "2",
            "[1,2]",
            "receiver",
            "callback",
            "SyntaxError"
        ])
    );
}

#[test]
fn inferred_finalizer_return_slots_preserve_strings_unions_pairs_and_completion_overrides() {
    let output = run(r#"
export function main():void{
 console.log(JSON.stringify([1,2].map(value=>{try{return "saved";}finally{console.log(value);}})));
 console.log(JSON.stringify([1,2].map(value=>{try{if(value===1){return "old";}return 3;}finally{if(value===1){return "new";}}})));
 console.log(JSON.stringify([1,2].map(value=>{try{return {n:value};}finally{if(value===1){return undefined;}}})));
 console.log([1].forEach(value=>{try{return varta.result.capture(varta.env.get("VARTA_INFERRED_PAIR"));}finally{console.log("pair finally");}}));
 console.log([1,2].some(value=>{try{return "yes";}finally{return "";}}));
 try{[1].map(value=>{try{return "saved";}finally{throw "override";}});}catch(e){console.log(e);}
 console.log([1,2].every(value=>{try{try{return "kept";}finally{console.log("inner");}}finally{console.log("outer");}}));}
"#);
    assert_eq!(
        &output.lines[..5],
        lines(&[
            "1",
            "2",
            "[\"saved\",\"saved\"]",
            "[\"new\",3]",
            "[null,{\"n\":2}]"
        ])
    );
    assert_eq!(
        &output.lines[output.lines.len() - 9..],
        lines(&[
            "pair finally",
            "undefined",
            "0",
            "override",
            "inner",
            "outer",
            "inner",
            "outer",
            "1"
        ])
    );
}

#[test]
fn inferred_saved_returns_survive_collection_and_detached_source_values() {
    let source = r#"
function churn():void{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}}
export function main():void{
 const source=["first"+"!","second"+"!"];console.log(JSON.stringify(source.map((value,index,array)=>{try{return value+"😀";}finally{if(index===0){delete array[0];}churn();}})));
 console.log(JSON.stringify([1,2].map(value=>{try{return {text:"owned"+"😀",n:value};}finally{churn();}})));
 console.log(JSON.stringify([1,2].map(value=>{try{if(value===1){return "string"+"!";}return {text:"object"+"!"};}finally{churn();}})));
 console.log(JSON.stringify([1,2].reduce((total:{items:string[]},value)=>{try{total.items.push("kept"+"!");return total;}finally{churn();}},{items:[]})));
 console.log([1,2].some(value=>{try{return "present"+"!";}finally{churn();}}));}
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
        lines(&[
            "[\"first!😀\",\"second!😀\"]",
            "[{\"text\":\"owned😀\",\"n\":1},{\"text\":\"owned😀\",\"n\":2}]",
            "[\"string!\",{\"text\":\"object!\"}]",
            "{\"items\":[\"kept!\",\"kept!\"]}",
            "1"
        ])
    );
}

#[test]
fn inference_rejects_ambiguous_seed_and_return_types_without_weakening_annotations() {
    for source in [
        "export function main():void{[1].reduce((total,value)=>total,[]);}",
        "export function main():void{[1].reduce((total:number[],value)=>total,['bad']);}",
        "export function main():void{[1].reduce((total:{n:number},value)=>total,{});}",
        "export function main():void{[1].reduce((total:number,value)=>total,'bad');}",
        "export function main():void{[1].reduce((total:number,value):string=>'bad',0);}",
        "function bad(total:string[],value:number):string[]{return total;}export function main():void{[1].reduce(bad,[1]);}",
        "export function main():void{[1].map(value=>{return [];});}",
        "export function main():void{[1].map(value=>{if(value===1){return {n:1};}return {text:'bad'};});}",
        "export function main():void{[1].forEach(value=>{if(value===1){return ()=>value;}return 1;});}",
        "export function main():void{[1].map((value):number=>{return 'bad';});}",
        "export function main():void{[1].filter(value=>{return varta.result.capture(varta.env.get('DENIED'));});}",
        "export function main():void{[1].map(value=>{return ()=>value;});}",
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
    let source = "export function main():void{[1].reduce((total:number[],value)=>total,['bad']);}";
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
    // Ordinary arrows keep their preexisting default return behavior. This
    // increment is confined to the contextual array callback entry point.
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const fn=()=>{return 'text';};fn();}",
            LanguageProfile::CheckedF64Utf16V14
        )
        .is_err()
    );
    let source = "export function main():void{console.log(JSON.stringify([1].map(value=>{return 'inferred';})));console.log(JSON.stringify([1].reduce((total:number[],value)=>{total.push(value);return total;},[])));}";
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
            lines(&["[\"inferred\"]", "[1]"])
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
fn inferred_callbacks_preserve_sparse_results_mutations_errors_and_fatal_limits() {
    let output = run(r#"
export function main():void{
 const values:number[]=[1,,3];console.log(JSON.stringify(values.map((value,index,array)=>{if(index===0){array[1]=2;delete array[2];}return {n:value};})));
 try{[1,2].map(value=>{if(value===2){throw RangeError("stop");}return "ok";});}catch(e){if(e instanceof RangeError){console.log(e.message);}}finally{console.log("done");}
 console.log([1].some(value=>{try{throw "initial";}finally{return "recovered";}}));
 console.log([1].forEach(value=>{return ()=>value;}));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[{\"n\":1},{\"n\":2},null]",
            "stop",
            "done",
            "1",
            "undefined"
        ])
    );
    for body in [
        "[1].map(value=>{while(true){}return 'unreachable';});",
        "[1].reduce((total:number[],value)=>{total.length=1025;return total;},[]);",
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
fn inference_keeps_host_effects_visible_to_json_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{[value].map(current=>{console.log(current);return 'owned';});[1].reduce((total:number[],current)=>{console.log(current);return total;},[]);return value;}export function main():void{varta.json.parse<number>('1');}",
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
