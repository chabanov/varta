use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str, profile: LanguageProfile) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, profile).unwrap()
}
fn run(source: &str, profile: LanguageProfile) -> RunOutput {
    let bytes = compile(source, profile);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn array_enumeration_dense_profiles_preserve_order_empty_and_fresh_identity() {
    let source = "export function main():void{const a:number[]=[4,3,2];const keys:string[]=Object.keys(a);const values:number[]=Object.values(a);console.log(JSON.stringify(keys));console.log(JSON.stringify(values));console.log(Object.keys(a)===keys);console.log(Object.values(a)===values);keys[0]='changed';values[0]=9;console.log(JSON.stringify(a));const empty:number[]=[];console.log(JSON.stringify(Object.keys(empty)));console.log(JSON.stringify(Object.values(empty)));}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        assert_eq!(
            run(source, profile).lines,
            lines(&[
                "[\"0\",\"1\",\"2\"]",
                "[4,3,2]",
                "0",
                "0",
                "[4,3,2]",
                "[]",
                "[]"
            ])
        );
    }
}
#[test]
fn array_enumeration_skips_holes_keeps_explicit_undefined_and_returns_dense_results() {
    let source = "export function main():void{const a:(number|undefined)[]=[,undefined,2,,4];const keys=Object.keys(a);const values=Object.values(a);console.log(JSON.stringify(keys));console.log(JSON.stringify(values));console.log(Object.hasOwn(keys,0));console.log(Object.hasOwn(values,0));console.log(Object.hasOwn(values,2));console.log(Object.hasOwn(values,3));console.log(JSON.stringify(a));const holes:string[]=[,,,];console.log(JSON.stringify(Object.keys(holes)));console.log(JSON.stringify(Object.values(holes)));}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        assert_eq!(
            run(source, profile).lines,
            lines(&[
                "[\"1\",\"2\",\"4\"]",
                "[null,2,4]",
                "1",
                "1",
                "1",
                "0",
                "[null,null,2,null,4]",
                "[]",
                "[]"
            ])
        );
    }
}
#[test]
fn array_enumeration_respects_deleted_grown_and_shrunk_presence_without_widening() {
    let out = run(
        "export function main():void{const a:number[]=[1,2,3];delete a[1];a.length=5;a[4]=4;const values:number[]=Object.values(a);console.log(JSON.stringify(Object.keys(a)));console.log(JSON.stringify(values));values.length=0;values.length=3;console.log(JSON.stringify(values));console.log(Object.hasOwn(values,0));values[2]=9;console.log(JSON.stringify(values));console.log(JSON.stringify(a));const keys=Object.keys(a);keys.length=0;keys.length=3;console.log(Object.hasOwn(keys,2));}",
        LanguageProfile::CheckedF64Utf16V14,
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[\"0\",\"2\",\"4\"]",
            "[1,3,4]",
            "[null,null,null]",
            "0",
            "[null,null,9]",
            "[1,null,3,null,4]",
            "0"
        ])
    );
}
#[test]
fn array_enumeration_preserves_all_value_kinds_exact_bits_and_shallow_children() {
    let out = run(
        "export function main():void{console.log(JSON.stringify(Object.values([true,false])));console.log(JSON.stringify(Object.values([null,null])));console.log(JSON.stringify(Object.values([undefined,undefined])));const n=Object.values([-0,NaN,Infinity]);console.log(n[0]);console.log(JSON.stringify(n));console.log(JSON.stringify(Object.values(['\\ud800','a\\u0000b'])));const child={n:1};const a:{n:number}[]=[child,child];const values=Object.values(a);const selected=values[0];if(selected!==undefined){selected.n=7;console.log(selected===child);}a[0]={n:9};console.log(JSON.stringify(values));const inner:number[]=[1];const arrays:number[][]=[inner];const copy=Object.values(arrays);const first=copy[0];if(first!==undefined){first.push(2);}console.log(JSON.stringify(inner));const mixed:(number|string|{n:number}|number[])[]=[3,'x',child,inner];console.log(JSON.stringify(Object.values(mixed)));const errors:TypeError[]=[TypeError('message')];const error=Object.values(errors)[0];if(error instanceof TypeError){console.log(error.message);}}",
        LanguageProfile::CheckedF64Utf16V14,
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[true,false]",
            "[null,null]",
            "[null,null]",
            "-0",
            "[0,null,null]",
            "[\"\\ud800\",\"a\\u0000b\"]",
            "1",
            "[{\"n\":7},{\"n\":7}]",
            "[1,2]",
            "[3,\"x\",{\"n\":7},[1,2]]",
            "message"
        ])
    );
}
#[test]
fn array_enumeration_evaluates_arguments_once_before_snapshot_and_later_effects() {
    let out = run(
        "function consume(keys:string[],values:number[]):void{console.log(JSON.stringify(keys));console.log(JSON.stringify(values));}function absent():number[]|null|undefined{console.log('source');return null;}function later():number{console.log('BAD_LATER');return 1;}function bad(values:number[],n:number):void{console.log('BAD');}export function main():void{const a:number[]=[1];const source=():number[]=>{console.log('source');a.push(2);return a;};consume(Object.keys(source()),Object.values(source()));try{bad(Object.values(absent()),later());}catch(e){if(e instanceof TypeError){console.log(e.message);}}try{Object.keys(absent());}catch(e){if(e instanceof TypeError){console.log(e.name);}}}",
        LanguageProfile::CheckedF64Utf16V14,
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "source",
            "[\"0\",\"1\"]",
            "[1,2,2]",
            "source",
            "Cannot convert undefined or null to object",
            "source",
            "TypeError"
        ])
    );
}
#[test]
fn array_enumeration_infers_callbacks_returns_and_nullable_sources() {
    let out = run(
        "function values(a:number[]|null):number[]{return Object.values(a);}export function main():void{console.log(JSON.stringify([[1,2],[3]].map(a=>Object.values(a))));console.log(JSON.stringify([[1],[2,3]].map(a=>Object.keys(a))));console.log(JSON.stringify(values([4])));console.log(JSON.stringify(Object.values(varta.json.parse<number[]>('[2,3]'))));}",
        LanguageProfile::CheckedF64Utf16V14,
    );
    assert_eq!(
        out.lines,
        lines(&["[[1,2],[3]]", "[[\"0\"],[\"0\",\"1\"]]", "[4]", "[2,3]"])
    );
}
#[test]
fn array_enumeration_maximum_indices_fit_default_limits() {
    let source = "export function main():void{const a:number[]=[ELEMENTS];const keys=Object.keys(a);const values:number[]=Object.values(a);console.log(keys.length);console.log(keys[0]);console.log(keys[9]);console.log(keys[10]);console.log(keys[99]);console.log(keys[100]);console.log(keys[999]);console.log(keys[1000]);console.log(keys[1023]);console.log(values[1023]);}";
    let elements = (0..varta_common::value::MAX_ARRAY_ELEMENTS)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let source = source.replace("ELEMENTS", &elements);
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        assert_eq!(
            run(&source, profile).lines,
            lines(&[
                "1024", "0", "9", "10", "99", "100", "999", "1000", "1023", "1023"
            ])
        );
    }
}
#[test]
fn array_enumeration_rejects_bad_types_mixed_sources_and_earlier_profiles() {
    for source in [
        "export function main():void{const a:string[]=Object.values([1]);}",
        "export function main():void{const a:number[]=Object.keys([1]);}",
        "export function main():void{Object.values([1]).push('x');}",
        "function source(flag:boolean):number[]|{n:number}{if(flag){return [1];}return {n:1};}export function main():void{Object.keys(source(true));}",
        "export function main():void{Object.values<number>([1]);}",
        "export function main():void{Object.keys([1],[2]);}",
        "export function main():void{const Object={values:1};Object.values([1]);}",
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
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        for method in ["keys", "values"] {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{Object.{method}([1]);}}"),
                    profile
                )
                .is_err()
            );
        }
    }
}
fn run_with_gc_budget(source: &str) -> Vec<String> {
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
    execution.store.data().output.lines.clone()
}
#[test]
fn array_enumeration_gc_keeps_earlier_operands_pairs_child_graphs_and_finally_results() {
    let out = run_with_gc_budget(
        "function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}function values():(number|string|{n:number}|number[])[]{const a:(number|string|{n:number}|number[])[]=['child'+'!',{n:3},[4],2];delete a[3];const items=Object.values(a);a.length=0;churn();return items;}function keep(earlier:{text:string},pair:VartaResult<string>,items:(number|string|{n:number}|number[])[]):string{churn();console.log(pair.status);return earlier.text+JSON.stringify(items);}function delayed():(number|string|{n:number}|number[])[]{try{return values();}finally{churn();}}export function main():void{console.log(keep({text:'prefix'+'!'},varta.result.capture(varta.env.get('VARTA_ARRAY_ENUM_GC')),values()));console.log(JSON.stringify(delayed()));console.log(('earlier'+'!')+JSON.stringify(values()));}",
    );
    assert_eq!(
        out,
        lines(&[
            "<capability denied: env> (VARTA_ARRAY_ENUM_GC)",
            "1",
            "prefix![\"child!\",{\"n\":3},[4]]",
            "[\"child!\",{\"n\":3},[4]]",
            "earlier![\"child!\",{\"n\":3},[4]]"
        ])
    );
}
#[test]
fn array_enumeration_repeated_keys_keep_roots_bounded_and_partial_outputs_canonical() {
    let out = run_with_gc_budget(
        "function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}export function main():void{let keys:string[]=[];const a:number[]=[];a.length=1024;a[0]=1;a[9]=9;a[10]=10;a[99]=99;a[100]=100;a[999]=999;a[1000]=1000;a[1023]=1023;for(let i=0;i<3000;i=i+1){keys=Object.keys(a);Object.values(a);}churn();console.log(JSON.stringify(keys));keys.length=0;churn();keys.length=8;console.log(Object.hasOwn(keys,7));keys.push('fresh');console.log(keys[8]);}",
    );
    assert_eq!(
        out,
        lines(&[
            "[\"0\",\"9\",\"10\",\"99\",\"100\",\"999\",\"1000\",\"1023\"]",
            "0",
            "fresh"
        ])
    );
}
#[test]
fn array_enumeration_argument_effects_remain_visible_to_helper_purity_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function source():number[]{console.log('effect');return [1];}function fake(value:boolean):boolean{Object.keys(source());Object.values(source());return value;}export function main():void{varta.json.parse<number>('1');}",
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
