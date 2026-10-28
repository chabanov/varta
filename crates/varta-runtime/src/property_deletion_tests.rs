use super::*;
use varta_common::language::LanguageProfile as P;
fn run(source: &str, expected: &[&str]) {
    let bytes = varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30)
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
fn property_deletion_optional_fields_compact_order_and_restore_presence() {
    run(
        "export function main():void{const r:{a?:number;b:number;c?:string;big?:bigint|undefined}={a:1,b:2,c:'x',big:3n};console.log(delete r.a);console.log(r.a);console.log(JSON.stringify(Object.keys(r)));console.log(delete r.big);console.log(JSON.stringify(r));console.log(delete r.c);console.log(delete r.c);r.c='y';r.a=4;console.log(JSON.stringify(Object.keys(r)));console.log(JSON.stringify(r));}",
        &[
            "1",
            "undefined",
            "[\"b\",\"c\",\"big\"]",
            "1",
            "{\"b\":2,\"c\":\"x\"}",
            "1",
            "1",
            "[\"b\",\"c\",\"a\"]",
            "{\"b\":2,\"c\":\"y\",\"a\":4}",
        ],
    );
}
#[test]
fn property_deletion_base_and_key_run_once_and_throwing_keys_do_not_mutate() {
    run(
        "export function main():void{let r:{x?:number;z?:number}={x:1,z:2};const original=r;const base=():{x?:number;z?:number}=>{console.log('base');return r;};const key=():string=>{console.log('key');r={x:9};return 'x';};console.log(delete base()[key()]);console.log(original.x);console.log(r.x);const fail=():string=>{throw TypeError('key');};try{delete r[fail()];}catch(e){console.log(e instanceof TypeError);}console.log(r.x);}",
        &["base", "key", "1", "undefined", "9", "1", "9"],
    );
}
#[test]
fn property_deletion_primitive_keys_unknown_names_and_required_guards() {
    run(
        "export function main():void{const r:{'0'?:number;'true'?:number;'undefined'?:number;required:number}={'0':1,'true':2,'undefined':3,required:4};console.log(delete r[-0]);console.log(delete r[true]);console.log(delete r[undefined]);console.log(delete r['__proto__']);console.log(JSON.stringify(r));try{delete r.required;}catch(e){console.log(e instanceof TypeError);}console.log(r.required);}",
        &["1", "1", "1", "1", "{\"required\":4}", "1", "4"],
    );
}
#[test]
fn property_deletion_array_string_keys_preserve_canonical_indices_and_length() {
    run(
        "export function main():void{const a:bigint[]=[1n,2n];let key='0';console.log(delete a[key]);console.log(Object.hasOwn(a,0));console.log(delete a['01']);console.log(Object.hasOwn(a,1));console.log(delete a[1n]);console.log(Object.hasOwn(a,1));console.log(a.length);key='length';try{delete a[key];}catch(e){console.log(e instanceof TypeError);}console.log(a.length);console.log(delete a[true]);}",
        &["1", "0", "1", "1", "1", "0", "2", "1", "2", "1"],
    );
}
#[test]
fn property_deletion_v29_stays_frozen() {
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const r:{x?:number}={x:1};delete r.x;}",
            P::CheckedF64Utf16V29
        )
        .is_err()
    );
}
#[test]
fn property_deletion_high_field_bit_and_numeric_enumeration_survive_reinsertion() {
    let fields = (0..32)
        .map(|n| format!("f{n:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let values = (0..32)
        .map(|n| format!("f{n:02}:{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "export function main():void{{const r:{{{fields}}}={{{values}}};const key='f31';console.log(delete r[key]);console.log(Object.keys(r).length);r.f31=99;console.log(Object.values(r)[31]);console.log(JSON.stringify(Object.keys(r).slice(30)));const n:{{'2'?:number;'1'?:number;a?:number;b?:number}}={{'2':2,'1':1,a:3,b:4}};delete n['1'];delete n.a;n.a=5;n['1']=9;console.log(JSON.stringify(Object.keys(n)));console.log(JSON.stringify(n));}}"
    );
    run(
        &source,
        &[
            "1",
            "31",
            "99",
            "[\"f30\",\"f31\"]",
            "[\"1\",\"2\",\"b\",\"a\"]",
            "{\"1\":9,\"2\":2,\"b\":4,\"a\":5}",
        ],
    );
}
#[test]
fn property_deletion_gc_drops_old_edges_but_keeps_escaped_values() {
    let source = "export function main():void{const r:{text?:string;big?:bigint|undefined;values?:bigint[]|undefined}={text:'x'.repeat(20000),big:9007199254740993n,values:[7n]};const saved=r.text;const big=r.big;const values=r.values;delete r.text;delete r.big;delete r.values;let i=0;while(i<24){r.text='x'.repeat(300000);delete r.text;i++;}console.log(JSON.stringify(r));if(saved!==undefined){console.log(saved.length);}console.log(big);if(values!==undefined){console.log(values[0]);}i=0;while(i<1500){delete r.text;i++;}console.log(JSON.stringify(Object.keys(r)));}";
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30).unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Deliberate allocation/collection fixture; production memory/fuel limits stay unchanged.
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
        [
            "varta log: {}",
            "varta log: 20000",
            "varta log: 9007199254740993",
            "varta log: 7",
            "varta log: []"
        ]
    );
}
