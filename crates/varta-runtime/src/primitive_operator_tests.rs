//! Ordinary v29 arithmetic shares primitive conversion with compound operators.
use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29)
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
fn primitive_operator_scalar_arithmetic_and_concat_convert_all_primitive_kinds() {
    run(
        "export function main():void{console.log(true+2);console.log(null+false);console.log(undefined+1);console.log(2+'x');console.log('x'+1n);console.log('6'-true);console.log('6'*'2');console.log(null/true);console.log(true/false);console.log('7'%'4');console.log('2'**'3');console.log('bad'*2);}",
        &[
            "3", "0", "NaN", "2x", "x1", "5", "12", "0", "Infinity", "3", "8", "NaN",
        ],
    );
}
#[test]
fn primitive_operator_shifts_bitwise_and_unary_preserve_number_edges_and_bigint() {
    run(
        "export function main():void{console.log('13'&'7');console.log('8'|true);console.log('7'^null);console.log('3'<<true);console.log('16'>>'2');console.log('-1'>>>undefined);console.log(true<<-1);console.log(undefined|true);console.log(~'2');console.log(1/+'-0');console.log(1/-'0');console.log(+true);console.log(-true);console.log(+null);console.log(-undefined);console.log(~undefined);console.log(+'0x10');console.log(+'Infinity');console.log(+'bad');console.log(~0n);console.log(-9007199254740993n);}",
        &[
            "5",
            "9",
            "7",
            "6",
            "4",
            "4294967295",
            "-2147483648",
            "1",
            "-3",
            "-Infinity",
            "-Infinity",
            "1",
            "-1",
            "0",
            "NaN",
            "-1",
            "16",
            "Infinity",
            "NaN",
            "-1",
            "-9007199254740993",
        ],
    );
}
#[test]
fn primitive_operator_wide_unions_normalize_once_and_unary_plus_returns_number_domain() {
    run(
        "function add(a:number|string|boolean|null|undefined|bigint,b:number|string|boolean|null|undefined|bigint):number|string|bigint{return a+b;}function plus(n:number|string|boolean|null|undefined|bigint):number{return +n;}function negate(n:number|string|boolean|null|undefined|bigint):number|bigint{return -n;}function invert(n:number|string|boolean|null|undefined|bigint):number|bigint{return ~n;}export function main():void{console.log(add(true,2));console.log(add('x',2n));console.log(add(undefined,1));console.log(add(2n,3n));console.log(plus('2'));console.log(negate('2'));console.log(negate(2n));console.log(typeof negate(2n));console.log(invert(0n));console.log(invert(undefined));try{console.log(plus(1n));}catch(e){console.log(e instanceof TypeError);}try{const value:number=+1n;console.log(value);}catch(e){console.log(e instanceof TypeError);}}",
        &[
            "3", "x2", "NaN", "5", "2", "-2", "-2", "bigint", "-1", "-1", "1", "1",
        ],
    );
}
#[test]
fn primitive_operator_source_order_and_unary_calls_are_not_replayed() {
    run(
        "function left():number|string|bigint{console.log('left');return 3n;}function right():number|string|bigint{console.log('right');return 2;}export function main():void{try{console.log(left()*right());console.log('after');}catch(e){console.log(e instanceof TypeError);}let calls=0;const value=():string=>{calls++;return '2';};console.log(+value());console.log(-value());console.log(~value());console.log(calls);}",
        &["left", "right", "1", "2", "-2", "-3", "3"],
    );
}
#[test]
fn primitive_operator_sparse_reads_do_not_store_and_mixed_numbers_throw() {
    run(
        "export function main():void{const a:number[]=[];console.log(a[0]+1);console.log(a[0]*false);console.log(Object.hasOwn(a,0));const text:string[]=[];console.log(text[0]+'x');const big:bigint[]=[1n];console.log(-big[0]);try{console.log(+big[0]);}catch(e){console.log(e instanceof TypeError);}try{console.log(1n*true);}catch(e){console.log(e instanceof TypeError);}try{console.log(1n>>>1n);}catch(e){console.log(e instanceof TypeError);}console.log(big[0]);}",
        &["NaN", "NaN", "0", "undefinedx", "-1", "1", "1", "1", "1"],
    );
}
#[test]
fn primitive_operator_old_profiles_and_object_toprimitive_gap_remain_explicit() {
    for source in [
        "export function main():void{console.log(true+2);}",
        "export function main():void{console.log('3'*2);}",
        "export function main():void{console.log(+true);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).is_err()
        );
    }
    for source in [
        "export function main():void{console.log(1+{n:2});}",
        "export function main():void{console.log(-{n:2});}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29).is_err()
        );
    }
}
#[test]
fn primitive_operator_gc_retains_cached_left_operands_after_rhs_replacement() {
    let source = "function churn():void{let i=0;while(i<80){const text='x'.repeat(20000);i++;}}export function main():void{const a:string[]=['x'];const text=():bigint=>{a[0]='y';churn();return 2n;};const saved=a[0]+text();churn();console.log(saved);console.log(a[0]);const big:bigint[]=[1n];const rhs=():bigint=>{big[0]=100n;churn();return 2n;};const sum=big[0]+rhs();churn();console.log(sum);console.log(big[0]);}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Finish deliberate collections without changing production budgets.
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
            "varta log: x2",
            "varta log: y",
            "varta log: 3",
            "varta log: 100"
        ]
    );
}
#[test]
fn primitive_operator_unary_plus_validates_bigint_leaves_before_type_errors() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let bytes = compile(
        "function make():bigint{return 123n;}function convert(n:bigint):number{try{return +n;}catch(e){console.log('caught');return 0;}finally{console.log('finally');}}export function main():void{}",
    );
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(main < 128 && data[0] < 125);
            data[0] += 2;
            for (name, index) in [("make", main - 2), ("convert", main - 1)] {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, index]);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for corrupt in [false, true] {
        let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let value = instance
            .get_typed_func::<(), i64>(&mut execution.store, "make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        if corrupt {
            let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
            let header = value as u32 as usize - varta_common::abi::STRING_HEADER_BYTES;
            memory
                .write(
                    &mut execution.store,
                    header + varta_common::value::HEAP_FLAGS_OFFSET,
                    &varta_common::value::HEAP_ALLOCATED_FLAG.to_le_bytes(),
                )
                .unwrap();
        }
        let result = instance
            .get_typed_func::<i64, f64>(&mut execution.store, "convert")
            .unwrap()
            .call(&mut execution.store, value);
        if corrupt {
            let error = result.unwrap_err();
            assert!(matches!(
                error.root_cause().downcast_ref::<varta_bigint::Error>(),
                Some(varta_bigint::Error::InvalidPayload)
            ));
            assert!(execution.store.data().output.lines.is_empty());
        } else {
            assert_eq!(result.unwrap(), 0.0);
            assert_eq!(
                execution.store.data().output.lines,
                ["varta log: caught", "varta log: finally"]
            );
        }
    }
}
