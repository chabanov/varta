//! V29 compound assignment converts cached primitives before checked stores.
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
fn primitive_compound_arithmetic_and_string_addition_preserve_conversion_rules() {
    run(
        "export function main():void{let text='x';console.log(text+=1);console.log(text+=true);console.log(text+=null);console.log(text+=undefined);console.log(text+=2n);let n:number|string=2;console.log(n+='x');console.log(typeof n);n=2;console.log(n*='3');console.log(n+=true);console.log(n-=null);console.log(n/=false);console.log(n%='3');n=2;console.log(n**='3');let b:number|boolean=true;console.log(b+=2);console.log(typeof b);}",
        &[
            "x1",
            "x1true",
            "x1truenull",
            "x1truenullundefined",
            "x1truenullundefined2",
            "2x",
            "string",
            "6",
            "7",
            "7",
            "Infinity",
            "NaN",
            "8",
            "3",
            "number",
        ],
    );
}
#[test]
fn primitive_compound_bitwise_operations_convert_strings_booleans_and_undefined() {
    run(
        "export function main():void{let n:number|string='13';console.log(n&='7');console.log(n|=true);console.log(n^='3');console.log(n<<=true);console.log(n>>='2');console.log(n>>>=undefined);console.log(n|=null);}",
        &["5", "5", "6", "12", "3", "3", "3"],
    );
}
#[test]
fn primitive_compound_six_member_operand_unions_use_bounded_numeric_dispatch() {
    run(
        "function add(n:number|string|boolean|null|undefined|bigint,r:number|string|boolean|null|undefined|bigint):number|string|bigint{return n+=r;}function multiply(n:number|string|boolean|null|undefined|bigint,r:number|string|boolean|null|undefined|bigint):number|bigint{return n*=r;}export function main():void{console.log(add('a',true));console.log(add(2,'x'));console.log(add(null,false));console.log(add(undefined,1));console.log(add(2n,3n));console.log(multiply('3',true));console.log(multiply(null,4));console.log(multiply(2n,3n));try{add(2n,true);}catch(e){console.log(e instanceof TypeError);}try{multiply(2n,'3');}catch(e){console.log(e instanceof TypeError);}}",
        &["atrue", "2x", "0", "NaN", "5", "3", "0", "6", "1", "1"],
    );
}
#[test]
fn primitive_compound_prepares_old_values_and_keys_before_rhs_mutation_once() {
    run(
        "export function main():void{const a:(number|string)[]=['2'];let calls=0;const index=():number=>{calls++;return 0;};const rhs=():string=>{calls+=10;a[0]='100';a.push('x');return '3';};console.log(a[index()]*=rhs());console.log(a[0]);console.log(a[1]);console.log(calls);const numbers:number[]=[1];const text=():number|string=>{console.log('rhs');numbers[0]=9;return 'x';};try{numbers[0]+=text();}catch(e){console.log(e instanceof TypeError);}console.log(numbers[0]);}",
        &["6", "6", "x", "11", "rhs", "1", "9"],
    );
}
#[test]
fn primitive_compound_sparse_and_optional_stores_publish_only_compatible_results() {
    run(
        "function text(r:{s?:string},key:string):string{return r[key]+=1;}export function main():void{const numbers:number[]=[1];delete numbers[0];console.log(numbers[0]+=true);console.log(Object.hasOwn(numbers,0));console.log(numbers[3]*=null);console.log(numbers.length);const strings:string[]=[];console.log(strings[0]+='x');const big:bigint[]=[];try{big[0]+=1n;}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(big,0));const r:{s?:string}={s:'x'};console.log(text(r,'s'));const absent:{s?:string}={};try{text(absent,'s');}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(absent,'s'));}",
        &[
            "NaN",
            "1",
            "NaN",
            "4",
            "undefinedx",
            "1",
            "0",
            "x1",
            "1",
            "0",
        ],
    );
}
#[test]
fn primitive_compound_mixed_numeric_failures_do_not_invent_bigint_result_types() {
    run(
        "export function main():void{let n=2;try{n+=1n;}catch(e){console.log(e instanceof TypeError);}console.log(n);const a:number[]=[3];try{a[0]*=2n;}catch(e){console.log(e instanceof TypeError);}console.log(a[0]);let big=5n;try{big*=true;}catch(e){console.log(e instanceof TypeError);}console.log(big);try{big**=-1n;}catch(e){console.log(e instanceof RangeError);}console.log(big);}",
        &["1", "2", "1", "3", "1", "5", "1", "5"],
    );
}
#[test]
fn primitive_compound_keeps_old_profiles_and_object_coercion_gap_explicit() {
    for source in [
        "export function main():void{let text='x';text+=1;}",
        "export function main():void{let n:number|string='2';n*=true;}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).is_err()
        );
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{let n=1;n+={v:2};}",
            P::CheckedF64Utf16V29
        )
        .is_err()
    );
}
#[test]
fn primitive_compound_gc_retains_cached_strings_after_rhs_replaces_original_bindings() {
    let source = "function churn():void{let i=0;while(i<80){const text='x'.repeat(20000);i++;}}export function main():void{let n:number|string='2';const rhs=():string=>{n='100';churn();return '3';};const first=n*=rhs();churn();console.log(first);console.log(n);const a:string[]=['x'];const text=():bigint=>{a[0]='y';churn();return 2n;};const saved=a[0]+=text();churn();console.log(saved);console.log(a[0]);}";
    let runtime = VartaRuntime::new();
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29).unwrap();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Complete the deliberate collection fixture without changing production limits.
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
            "varta log: 6",
            "varta log: 6",
            "varta log: x2",
            "varta log: x2"
        ]
    );
}
#[test]
fn primitive_compound_native_conversion_work_is_fatal_on_exhaustion() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    const LOW_FUEL: u64 = 10_000;
    let source = "function make():number|string{return '1'.repeat(20000);}function multiply(n:number|string):number{try{return n*=1;}catch(e){console.log('caught');return 0;}finally{console.log('finally');}}export function main():void{}";
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29).unwrap();
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(main < 128 && data[0] < 125);
            data[0] += 2;
            for (name, index) in [("make", main - 2), ("multiply", main - 1)] {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, index]);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for fuel in [LOW_FUEL, varta_common::abi::FUEL] {
        let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let value = instance
            .get_typed_func::<(), i64>(&mut execution.store, "make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        execution.store.set_fuel(fuel).unwrap();
        let result = instance
            .get_typed_func::<i64, f64>(&mut execution.store, "multiply")
            .unwrap()
            .call(&mut execution.store, value);
        if fuel == LOW_FUEL {
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
