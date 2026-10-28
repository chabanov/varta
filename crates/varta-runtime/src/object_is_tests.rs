use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn object_is_primitive_matrix_matches_pinned_node_same_value() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("same_value_matrix.json")).unwrap();
    assert_eq!(fixture["node"], "v24.20.0");
    let rows: Vec<_> = fixture["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    run(
        "function same(a:number|string|boolean|null|undefined|bigint,b:number|string|boolean|null|undefined|bigint):boolean{return Object.is(a,b);}export function main():void{const values:(number|string|boolean|null|undefined|bigint)[]=[0,-0,NaN,-NaN,Infinity,-Infinity,1,1.5,'','1','bad',true,false,null,undefined,0n,1n,9007199254740993n];let i=0;while(i<values.length){let row='';let j=0;while(j<values.length){row+=same(values[i],values[j])?'1':'0';j++;}console.log(row);i++;}}",
        &rows,
    );
}
#[test]
fn object_is_reference_identity_across_shapes_arrays_functions_and_error_classes() {
    run(
        "export function main():void{const a={n:1};const b={n:1};const alias=a;console.log(Object.is(a,alias));console.log(Object.is(a,b));console.log(Object.is(a,{other:1}));const xs:number[]=[1];const ys:number[]=[1];console.log(Object.is(xs,xs));console.log(Object.is(xs,ys));console.log(Object.is(xs,a));const f=():number=>1;const g=():number=>1;console.log(Object.is(f,f));console.log(Object.is(f,g));const e=TypeError('x');const base:Error=e;console.log(Object.is(e,base));console.log(Object.is(e,TypeError('x')));console.log(Object.is(e,a));}",
        &["1", "0", "0", "1", "0", "0", "1", "0", "1", "0", "0"],
    );
}
#[test]
fn object_is_reference_union_members_and_bigint_value_equality() {
    run(
        "function same(a:number|string|boolean|null|undefined|bigint|{n:number}|number[]|Error,b:number|string|boolean|null|undefined|bigint|{n:number}|number[]|Error):boolean{return Object.is(a,b);}export function main():void{const r={n:1};const a:number[]=[2];const e=RangeError('x');console.log(same(r,r));console.log(same(r,{n:1}));console.log(same(a,a));console.log(same(r,a));console.log(same(e,e));console.log(same(e,r));console.log(same(9007199254740993n,BigInt('9007199254740993')));console.log(same(1n,1));console.log(same(NaN,NaN));console.log(same(-0,0));console.log(same('x','x'));}",
        &["1", "0", "1", "0", "1", "0", "1", "0", "1", "0", "1"],
    );
}
#[test]
fn object_is_argument_order_missing_ignored_and_void_values() {
    run(
        "function value(s:string,n:number):number{console.log(s);return n;}function fail():void{throw SyntaxError('extra');}export function main():void{console.log(Object.is(value('a',-0),value('b',0),value('extra',1)));console.log(Object.is());console.log(Object.is(undefined));console.log(Object.is(null));console.log(Object.is(console.log('void'),undefined));try{Object.is(value('left',1),value('right',1),fail());}catch(e){console.log(e instanceof SyntaxError);}}",
        &[
            "a", "b", "extra", "0", "1", "1", "0", "void", "1", "left", "right", "1",
        ],
    );
}
#[test]
fn object_is_scalar_loop_keeps_guest_number_comparison_without_host_pressure() {
    run(
        "export function main():void{let i=0;let count=0;while(i<1500){if(Object.is(i,i)){count++;}i++;}console.log(count);}",
        &["1500"],
    );
}
#[test]
fn object_is_admission_and_shadowing_stay_explicit() {
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{console.log(Object.is(1,1));}",
            P::CheckedF64Utf16V29
        )
        .is_err()
    );
    for source in [
        "export function main():void{const Object={is:1};Object.is(1,1);}",
        "export function main():void{Object.is<number>(1,1);}",
        "export function main():void{const xs:number[]=[1];Object.is(...xs);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30).is_err()
        );
    }
}
#[test]
fn object_is_roots_operands_across_later_arguments_and_ignored_argument_gc() {
    let source = "function collect():string{let i=0;while(i<24){const garbage='z'.repeat(300000);i++;}return 'x'.repeat(20000);}export function main():void{console.log(Object.is('x'.repeat(20000),collect()));console.log(Object.is(BigInt('9007199254740993'),BigInt('9007199254740993'),collect()));}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Collection stress only; production limits remain unchanged.
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
        ["varta log: 1", "varta log: 1"]
    );
}
