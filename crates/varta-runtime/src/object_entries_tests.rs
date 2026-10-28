use super::*;
use varta_common::language::LanguageProfile as P;
fn run(source: &str, expected: &[&str]) {
    let wasm = varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&wasm, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn object_entries_record_order_absence_and_explicit_undefined() {
    run(
        "export function main():void{const r:{'2'?:number;'1'?:number;a?:number;b?:number;empty?:undefined}={'2':2,'1':1,a:3,b:4,empty:undefined};delete r.a;r.a=5;console.log(JSON.stringify(Object.entries(r)));console.log(JSON.stringify(Object.entries({})));}",
        &[
            "[[\"1\",1],[\"2\",2],[\"b\",4],[\"empty\",null],[\"a\",5]]",
            "[]",
        ],
    );
}
#[test]
fn object_entries_sparse_arrays_skip_holes_and_keep_original_index_strings() {
    run(
        "export function main():void{const a:(number|undefined)[]=[1,,undefined,4];delete a[0];const empty:number[]=[];console.log(JSON.stringify(Object.entries(a)));console.log(JSON.stringify(Object.entries(empty)));}",
        &["[[\"2\",null],[\"3\",4]]", "[]"],
    );
}
#[test]
fn object_entries_preserves_value_identity_and_creates_independent_pairs() {
    run(
        "export function main():void{const child={n:1};const r={child:child};const entries=Object.entries(r);const pair=entries[0];if(pair!==undefined){const value=pair[1];if(typeof value==='object'&&value!==null){value.n=2;}pair[0]='changed';}console.log(child.n);console.log(JSON.stringify(Object.keys(r)));console.log(JSON.stringify(entries));}",
        &["2", "[\"child\"]", "[[\"changed\",{\"n\":2}]]"],
    );
}
#[test]
fn object_entries_runs_source_and_ignored_arguments_before_snapshot() {
    run(
        "function source(r:{n:number}):{n:number}{console.log('source');return r;}function fail():void{throw SyntaxError('ignored');}export function main():void{const r={n:1};const extra=():void=>{console.log('extra');r.n=2;};console.log(JSON.stringify(Object.entries(source(r),extra())));try{Object.entries(null,fail());}catch(e){console.log(e instanceof SyntaxError);}try{Object.entries();}catch(e){console.log(e instanceof TypeError);}console.log(JSON.stringify(Object.entries(1)));console.log(JSON.stringify(Object.entries(true)));console.log(JSON.stringify(Object.entries(1n)));}",
        &["source", "extra", "[[\"n\",2]]", "1", "1", "[]", "[]", "[]"],
    );
}
#[test]
fn object_entries_nullable_sources_and_v29_admission() {
    run(
        "function entries(r:{n:number}|undefined):void{console.log(JSON.stringify(Object.entries(r)));}export function main():void{entries({n:1});try{entries(undefined);}catch(e){console.log(e instanceof TypeError);}}",
        &["[[\"n\",1]]", "1"],
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{Object.entries({n:1});}",
            P::CheckedF64Utf16V29
        )
        .is_err()
    );
}

#[test]
fn object_entries_keeps_nested_pairs_alive_across_collection() {
    let source = "export function main():void{const r={text:'x'.repeat(20000),big:9007199254740993n};const entries=Object.entries(r);let i=0;while(i<24){const garbage='z'.repeat(300000);i++;}console.log(entries.length);const first=entries[0];if(first!==undefined){console.log(first[0]);const text=first[1];if(typeof text==='string'){console.log(text.length);}}const second=entries[1];if(second!==undefined){console.log(second[0]);console.log(second[1]);}}";
    run_with_stress_fuel(source, &["2", "text", "20000", "big", "9007199254740993"]);
}
fn run_with_stress_fuel(source: &str, expected: &[&str]) {
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30).unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Explicit stress fixture: production memory and fuel limits remain unchanged.
    execution
        .store
        .set_fuel(1000 * varta_common::abi::FUEL)
        .unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn object_entries_handles_maximum_array_and_is_a_snapshot() {
    run_with_stress_fuel(
        "export function main():void{const a:number[]=[];let i=0;while(i<1024){a.push(i);i++;}const entries=Object.entries(a);a[1023]=7;console.log(entries.length);console.log(JSON.stringify(entries[0]));console.log(JSON.stringify(entries[1023]));}",
        &["1024", "[\"0\",0]", "[\"1023\",1023]"],
    );
}

#[test]
fn object_enumeration_string_utf16_units_and_keys() {
    run(
        "export function main():void{const s='A😀\\ud800';console.log(JSON.stringify(Object.keys(s)));console.log(JSON.stringify(Object.values(s)));console.log(JSON.stringify(Object.entries(s)));console.log(JSON.stringify(Object.entries('')));}",
        &[
            "[\"0\",\"1\",\"2\",\"3\"]",
            "[\"A\",\"\\ud83d\",\"\\ude00\",\"\\ud800\"]",
            "[[\"0\",\"A\"],[\"1\",\"\\ud83d\"],[\"2\",\"\\ude00\"],[\"3\",\"\\ud800\"]]",
            "[]",
        ],
    );
}
#[test]
fn object_enumeration_nullable_string_and_primitive_wrappers() {
    run(
        "function show(s:string|undefined):void{console.log(JSON.stringify(Object.values(s)));}export function main():void{show('ab');try{show(undefined);}catch(e){console.log(e instanceof TypeError);}console.log(JSON.stringify(Object.keys(1)));console.log(JSON.stringify(Object.values(true)));console.log(JSON.stringify(Object.keys(1n)));try{Object.values();}catch(e){console.log(e instanceof TypeError);}}",
        &["[\"a\",\"b\"]", "1", "[]", "[]", "[]", "1"],
    );
}
#[test]
fn object_enumeration_ignored_arguments_run_before_nullish_error_and_snapshot() {
    run(
        "function fail():void{throw SyntaxError('extra');}export function main():void{let s='ab';const input=():string=>{console.log('source');return s;};const extra=():void=>{console.log('extra');s='z';};console.log(JSON.stringify(Object.keys(input(),extra())));try{Object.values(null,fail());}catch(e){console.log(e instanceof SyntaxError);}const r={n:1};const change=():void=>{r.n=2;};console.log(JSON.stringify(Object.values(r,change())));}",
        &["source", "extra", "[\"0\",\"1\"]", "1", "[2]"],
    );
}
#[test]
fn object_enumeration_string_capacity_errors_are_recoverable() {
    run(
        "export function main():void{const s='a'.repeat(1025);try{Object.keys(s);}catch(e){console.log(e instanceof RangeError);}try{Object.values(s);}catch(e){console.log(e instanceof RangeError);}try{Object.entries(s);}catch(e){console.log(e instanceof RangeError);}console.log(JSON.stringify(Object.values('ok')));}",
        &["1", "1", "1", "[\"o\",\"k\"]"],
    );
    run_with_stress_fuel(
        "export function main():void{const s='x'.repeat(1024);const keys=Object.keys(s);console.log(keys.length);console.log(keys[1023]);const entries=Object.entries(s);console.log(entries.length);console.log(JSON.stringify(entries[1023]));}",
        &["1024", "1023", "1024", "[\"1023\",\"x\"]"],
    );
}
#[test]
fn object_enumeration_string_admission_and_call_rules_stay_frozen_in_v29() {
    for source in [
        "export function main():void{Object.keys('x');}",
        "export function main():void{Object.values('x');}",
        "export function main():void{Object.keys(1);}",
        "export function main():void{Object.keys({n:1},2);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29).is_err()
        );
    }
}
#[test]
fn object_enumeration_primitive_union_dispatch_preserves_nullish_errors() {
    run(
        "function show(v:string|number|boolean|bigint|null|undefined):void{console.log(JSON.stringify(Object.keys(v)));console.log(JSON.stringify(Object.values(v)));console.log(JSON.stringify(Object.entries(v)));}export function main():void{show('ab');show(3);show(true);show(4n);try{show(null);}catch(e){console.log(e instanceof TypeError);}try{show(undefined);}catch(e){console.log(e instanceof TypeError);}}",
        &[
            "[\"0\",\"1\"]",
            "[\"a\",\"b\"]",
            "[[\"0\",\"a\"],[\"1\",\"b\"]]",
            "[]",
            "[]",
            "[]",
            "[]",
            "[]",
            "[]",
            "[]",
            "[]",
            "[]",
            "1",
            "1",
        ],
    );
}
#[test]
fn object_enumeration_string_pairs_survive_repeated_collection() {
    run_with_stress_fuel(
        "export function main():void{let saved:string[][]=[];let i=0;while(i<40){saved=Object.entries('x'.repeat(256));const garbage='z'.repeat(300000);i++;}console.log(saved.length);console.log(JSON.stringify(saved[0]));console.log(JSON.stringify(saved[255]));}",
        &["256", "[\"0\",\"x\"]", "[\"255\",\"x\"]"],
    );
}
