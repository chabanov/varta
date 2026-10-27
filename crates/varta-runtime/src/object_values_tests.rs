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
fn object_values_preserves_js_order_numbers_and_fresh_dense_identity() {
    let out = run(
        "export function main():void{const value={z:1,'10':10,'2':2,a:3,'01':4};const values=Object.values(value);console.log(JSON.stringify(values));console.log(values===Object.values(value));console.log(Object.hasOwn(values,0));console.log(Object.hasOwn(values,4));console.log(Object.hasOwn(values,5));const edge=Object.values({a:-0,b:NaN,c:Infinity});console.log(edge[0]);console.log(JSON.stringify(edge));console.log(JSON.stringify(Object.values({})));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[2,10,1,3,4]",
            "0",
            "1",
            "1",
            "0",
            "-0",
            "[0,null,null]",
            "[]"
        ])
    );
}
#[test]
fn object_values_omits_absence_but_keeps_explicit_undefined_and_null() {
    let out = run(
        "export function main():void{const value:{a?:number;b:number;c?:number;'1'?:number}={b:1};value.c=undefined;value['1']=2;console.log(JSON.stringify(Object.keys(value)));console.log(JSON.stringify(Object.values(value)));value.a=3;console.log(JSON.stringify(Object.values(value)));value.b=4;console.log(JSON.stringify(Object.values(value)));const nulls=Object.values({a:null,b:null});console.log(JSON.stringify(nulls));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[\"1\",\"b\",\"c\"]",
            "[2,1,null]",
            "[2,1,null,3]",
            "[2,4,null,3]",
            "[null,null]"
        ])
    );
}
#[test]
fn object_values_infers_admitted_primitive_unions_without_coercion() {
    let out = run(
        "function values(): (number|string|boolean|null)[]{return Object.values({n:1,text:'Привіт',enabled:true,missing:null});}export function main():void{console.log(JSON.stringify(values()));const items=Object.values({n:2,text:'x'});items.push(3,'y');console.log(JSON.stringify(items));console.log(JSON.stringify(Object.values({text:'\\ud800',zero:'a\\u0000b'})));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[1,\"Привіт\",true,null]",
            "[2,\"x\",3,\"y\"]",
            "[\"\\ud800\",\"a\\u0000b\"]"
        ])
    );
}
#[test]
fn object_values_snapshots_scalars_but_preserves_nested_reference_identity() {
    let out = run(
        "export function main():void{const child={n:1};const source={a:child,b:child};const values=Object.values(source);const first=values[0];if(first!==undefined){first.n=7;console.log(first===child);}source.a={n:9};console.log(child.n);console.log(JSON.stringify(values));const numbers={a:1,b:2};const copy=Object.values(numbers);numbers.a=9;copy[1]=8;console.log(JSON.stringify(copy));console.log(numbers.b);const array:number[]=[1];const arrays=Object.values({a:array,b:array});const selected=arrays[0];if(selected!==undefined){selected.push(2);}console.log(JSON.stringify(array));}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "7", "[{\"n\":7},{\"n\":7}]", "[1,8]", "2", "[1,2]"])
    );
}
#[test]
fn object_values_reference_unions_retain_records_arrays_and_scalars() {
    let out = run(
        "export function main():void{const child={n:3};const array:number[]=[4];const items=Object.values({record:child,array:array,label:'x',count:2});console.log(JSON.stringify(items));for(const value of items){if(Array.isArray(value)){console.log(value.length);}else if(typeof value==='string'){console.log(value);}}const errors=Object.values({first:TypeError('one'),second:TypeError('two')});const selected=errors[1];if(selected instanceof TypeError){console.log(selected.message);}}",
    );
    assert_eq!(
        out.lines,
        lines(&["[{\"n\":3},[4],\"x\",2]", "1", "x", "two"])
    );
}
#[test]
fn object_values_nullable_sources_and_argument_order_preserve_exceptions() {
    let out = run(
        "function source(present:boolean):{a:number}|null|undefined{console.log('source');if(present){return {a:1};}return undefined;}function consume(values:number[],other:number):void{console.log('BAD');}function later():number{console.log('BAD_LATER');return 1;}export function main():void{console.log(JSON.stringify(Object.values(source(true))));try{consume(Object.values(source(false)),later());}catch(e){if(e instanceof TypeError){console.log(e.message);}}finally{console.log('finally');}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "[1]",
            "source",
            "Cannot convert undefined or null to object",
            "finally"
        ])
    );
}
#[test]
fn object_values_evaluates_source_once_and_keeps_checked_array_mutations() {
    let out = run(
        "export function main():void{const record:{a?:number;b:number;c?:number}={b:1};const source=():{a?:number;b:number;c?:number}=>{console.log('source');record.a=2;return record;};const values=Object.values(source());console.log(JSON.stringify(values));values.length=3;console.log(JSON.stringify(values));console.log(Object.hasOwn(values,2));values[2]=undefined;console.log(Object.hasOwn(values,2));values.length=0;values.push(9);console.log(JSON.stringify(values));console.log(JSON.stringify(record));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "[1,2]",
            "[1,2,null]",
            "0",
            "1",
            "[9]",
            "{\"b\":1,\"a\":2}"
        ])
    );
}
#[test]
fn object_values_typed_json_rest_and_callbacks_share_order_and_inferred_types() {
    let out = run(
        r#"function values():number[]{return Object.values({b:2,a:1});}export function main():void{const record=varta.json.parse<{a?:number;b:number;c?:number}>('{"b":1,"c":2}');record.a=undefined;console.log(JSON.stringify(Object.values(record)));const {b,...rest}=record;console.log(JSON.stringify(Object.values(rest)));console.log(JSON.stringify([1,2].map(n=>Object.values({n:n}))));console.log(JSON.stringify(values().map(n=>n+1)));}"#,
    );
    assert_eq!(
        out.lines,
        lines(&["[1,2,null]", "[2,null]", "[[1],[2]]", "[3,2]"])
    );
}
#[test]
fn object_values_rejects_unrepresentable_unions_shadowing_and_unsupported_sources() {
    for source in [
        "export function main():void{Object.values();}",
        "export function main():void{Object.values({},{n:1});}",
        "export function main():void{Object.values('text');}",
        "export function main():void{Object.values(Error('x'));}",
        "export function main():void{const Object={values:1};Object.values({n:1});}",
        "function Object():void{}export function main():void{Object.values({n:1});}",
        "export function main():void{Object.values<number>({n:1});}",
        "export function main():void{Object.values(...[{n:1}]);}",
        "export function main():void{Object.values({a:{x:1},b:{y:2}});}",
        "export function main():void{const a:number[]=[1];const b:string[]=['x'];Object.values({a:a,b:b});}",
        "export function main():void{const result:string[]=Object.values({n:1});}",
        "export function main():void{Object.values({n:1}).push('bad');}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{Object.values({n:1});}",
                profile
            )
            .is_err()
        );
    }
}
#[test]
fn object_values_maximum_shapes_and_high_presence_bits_fit_default_limits() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|n| format!("f{n:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let source = format!(
        "export function main():void{{const value:{{{fields}}}={{f31:31,f00:0}};const items=Object.values(value);console.log(JSON.stringify(items));items.push(9);console.log(JSON.stringify(items));value.f30=30;let count=0;for(let i=0;i<100;i=i+1){{const next=Object.values(value);count=count+next.length;}}console.log(count);}}"
    );
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["[31,0]", "[31,0,9]", "300"]),
            "{profile:?}"
        );
    }
}
#[test]
fn object_values_gc_keeps_child_graphs_earlier_operands_pairs_and_saved_results() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}return 1;}function values():(number|string|{n:number}|number[]|undefined)[]{const source:{a:number;b:string;c:{n:number};d:number[];gone?:string}={d:[4],c:{n:3},b:'owned'+'!',a:2};const output=Object.values(source);source.a=9;source.b='replacement';source.c={n:99};source.d=[];churn();return output;}function keep(first:{text:string},pair:VartaResult<string>,last:(number|string|{n:number}|number[]|undefined)[]):string{churn();console.log(pair.status);return first.text+JSON.stringify(last);}function delayed():(number|string|{n:number}|number[]|undefined)[]{try{return values();}finally{churn();}}export function main():void{console.log(keep({text:'prefix'+'!'},varta.result.capture(varta.env.get('VARTA_VALUES_GC')),values()));console.log(JSON.stringify(delayed()));console.log(('earlier'+'!')+JSON.stringify(values()));}";
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
    assert_eq!(
        execution.store.data().output.lines,
        lines(&[
            "<capability denied: env> (VARTA_VALUES_GC)",
            "1",
            "prefix![[4],{\"n\":3},\"owned!\",2]",
            "[[4],{\"n\":3},\"owned!\",2]",
            "earlier![[4],{\"n\":3},\"owned!\",2]"
        ])
    );
}
#[test]
fn object_values_cached_reference_union_cells_survive_repeated_collection() {
    let source = "function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}export function main():void{let saved:(number|string|{n:number})[]=[];for(let i=0;i<8;i=i+1){const text='child'+'!';const child={n:i};saved=Object.values({text:text,n:i,child:child});churn();}churn();console.log(JSON.stringify(saved));saved.length=1;churn();saved.length=3;console.log(JSON.stringify(saved));console.log(Object.hasOwn(saved,2));}";
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
    assert_eq!(
        execution.store.data().output.lines,
        lines(&["[\"child!\",7,{\"n\":7}]", "[\"child!\",null,null]", "0"])
    );
}
#[test]
fn object_values_argument_effects_remain_visible_to_host_free_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function source():{n:number;text:string}{console.log('effect');return {n:1,text:'x'};}function fake(value:boolean):boolean{Object.values(source());return value;}export function main():void{varta.json.parse<number>('1');}",
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
fn object_values_argument_resource_traps_stay_fatal() {
    let bytes = compile(
        "function source():{n:number}{while(true){}return {n:1};}export function main():void{try{Object.values(source());}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
