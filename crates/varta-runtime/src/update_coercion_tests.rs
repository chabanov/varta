//! Primitive ToNumeric updates retain checked storage invariants in v29.
use super::*;
use varta_common::language::LanguageProfile as P;

fn run(source: &str, expected: &[&str]) {
    let bytes = varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn update_coercion_all_primitives_return_converted_numeric_old_values() {
    run(
        "function post(n:bigint|number|string|boolean|null|undefined):bigint|number{return n++;}function pre(n:bigint|number|string|boolean|null|undefined):bigint|number{return --n;}export function main():void{console.log(post('2'));console.log(typeof post('2'));console.log(post(true));console.log(post(false));console.log(post(null));console.log(post(undefined));console.log(post(''));console.log(1/post('-0'));console.log(post('bad'));console.log(post('0x10'));console.log(pre('2'));console.log(pre(true));console.log(post(9007199254740993n));console.log(typeof post(1n));}",
        &[
            "2",
            "number",
            "1",
            "0",
            "0",
            "NaN",
            "0",
            "-Infinity",
            "NaN",
            "16",
            "1",
            "0",
            "9007199254740993",
            "bigint",
        ],
    );
}
#[test]
fn update_coercion_refined_local_unions_store_numbers_and_refine_subsequent_reads() {
    run(
        "export function main():void{let n:number|string='2';console.log(n++);console.log(n);console.log(typeof n);n='-0';console.log(1/n++);console.log(n);let b:number|boolean=true;console.log(++b);console.log(typeof b);let z:number|null=null;console.log(z++);console.log(z);let u:number|undefined=undefined;console.log(u++);console.log(u);}",
        &[
            "2",
            "3",
            "number",
            "-Infinity",
            "1",
            "2",
            "number",
            "0",
            "1",
            "NaN",
            "NaN",
        ],
    );
}
#[test]
fn update_coercion_sparse_number_arrays_create_nan_and_prepare_keys_once() {
    run(
        "export function main():void{const a:number[]=[5];delete a[0];console.log(a[0]++);console.log(Object.hasOwn(a,0));console.log(a[0]);console.log(a[3]++);console.log(a.length);console.log(Object.hasOwn(a,1));console.log(Object.hasOwn(a,3));const mixed:(number|string)[]=['2'];let calls=0;const index=():number=>{calls++;mixed.push('9');return 0;};console.log(mixed[index()]++);console.log(mixed[0]);console.log(mixed[1]);console.log(calls);}",
        &["NaN", "1", "NaN", "NaN", "4", "0", "1", "2", "3", "9", "1"],
    );
}
#[test]
fn update_coercion_optional_and_dynamic_records_keep_checked_field_types() {
    run(
        "function post(r:{n:bigint},key:string):bigint{return r[key]++;}function bump(r:{n:number|string;b:bigint|string},key:string):number|bigint{return r[key]++;}export function main():void{const optional:{first:number;n?:number|undefined}={first:1};console.log(optional.n++);console.log(Object.hasOwn(optional,'n'));console.log(JSON.stringify(Object.keys(optional)));const r:{n:number|string;b:bigint|string}={n:'2',b:3n};console.log(bump(r,'n'));console.log(r.n);console.log(bump(r,'b'));console.log(r.b);r.b='2';try{bump(r,'b');}catch(e){console.log(e instanceof TypeError);}console.log(r.b);console.log(typeof r.b);const big={n:7n};console.log(post(big,'n'));console.log(big.n);}",
        &[
            "NaN",
            "1",
            "[\"first\",\"n\"]",
            "2",
            "3",
            "3",
            "4",
            "1",
            "2",
            "string",
            "7",
            "8",
        ],
    );
}
#[test]
fn update_coercion_incompatible_bigint_storage_stays_unchanged_after_numeric_conversion() {
    run(
        "function post(r:{n?:bigint|undefined},key:string):bigint{return r[key]++;}export function main():void{const a:bigint[]=[1n];delete a[0];try{a[0]++;}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(a,0));const r:{n?:bigint|undefined}={};try{post(r,'n');}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(r,'n'));r.n=7n;console.log(post(r,'n'));console.log(r.n);}",
        &["1", "0", "1", "0", "7", "8"],
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{let n='1';n++;}",
            P::CheckedF64Utf16V29
        )
        .is_err()
    );
}
#[test]
fn update_coercion_retains_v28_noncoercive_admission_and_object_gap() {
    let source = "function post(n:number|string):number{return n++;}export function main():void{try{console.log(post('2'));}catch(e){console.log(e instanceof TypeError);}}";
    let old =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).unwrap();
    assert_eq!(
        VartaRuntime::new().run_wasm_bytes(&old, &[]).unwrap().lines,
        ["varta log: 1"]
    );
    run(source, &["2"]);
    run(
        "function post(n:number|{n:number}):number{return n++;}export function main():void{const r={n:2};try{post(r);}catch(e){console.log(e instanceof TypeError);}console.log(r.n);}",
        &["1", "2"],
    );
}
#[test]
fn update_coercion_string_parsing_consumes_fuel_and_exhaustion_bypasses_handlers() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let source = "function make():number|string{return '1'.repeat(20000);}function increment(n:number|string):number{try{return n++;}catch(e){console.log('caught');return 0;}finally{console.log('finally');}}export function main():void{}";
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29).unwrap();
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(main < 128 && data[0] < 125);
            data[0] += 2;
            for (name, index) in [("make", main - 2), ("increment", main - 1)] {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, index]);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for fuel in [10_000, varta_common::abi::FUEL] {
        let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let value = instance
            .get_typed_func::<(), i64>(&mut execution.store, "make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        execution.store.set_fuel(fuel).unwrap();
        let result = instance
            .get_typed_func::<i64, f64>(&mut execution.store, "increment")
            .unwrap()
            .call(&mut execution.store, value);
        if fuel == 10_000 {
            let error = result.unwrap_err();
            assert!(
                matches!(
                    error.root_cause().downcast_ref::<varta_bigint::Error>(),
                    Some(varta_bigint::Error::WorkBudget)
                ),
                "{error:#}"
            );
            assert!(execution.store.data().output.lines.is_empty());
        } else {
            assert_eq!(result.unwrap(), f64::INFINITY);
            assert_eq!(execution.store.data().output.lines, ["varta log: finally"]);
        }
    }
}
