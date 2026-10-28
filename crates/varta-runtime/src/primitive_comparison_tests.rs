//! Six primitive kinds, independently checked against a pinned Node.js matrix.
use super::*;
use varta_common::language::LanguageProfile as P;
const SIX: &str = "number|string|boolean|null|undefined|bigint";
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
fn primitive_comparison_matrix_matches_node_for_all_eight_operators() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("comparison_matrix.json")).unwrap();
    for (op, rows) in expected.as_object().unwrap() {
        let source = format!(
            "function compare(a:{SIX},b:{SIX}):boolean{{return a{op}b;}}export function main():void{{const values:({SIX})[]=[0,-0,NaN,1,1.5,'','1','1.5','bad',true,false,null,undefined,0n,1n,9007199254740993n];for(let i=0;i<values.length;i++){{let row='';for(let j=0;j<values.length;j++){{row+=compare(values[i],values[j])?'1':'0';}}console.log(row);}}}}"
        );
        let lines: Vec<_> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        run(&source, &lines);
    }
}
#[test]
fn primitive_comparison_precision_infinities_bigint_strings_and_utf16() {
    let source = format!(
        "function lt(a:{SIX},b:{SIX}):boolean{{return a<b;}}function le(a:{SIX},b:{SIX}):boolean{{return a<=b;}}function eq(a:{SIX},b:{SIX}):boolean{{return a==b;}}function strict(a:{SIX},b:{SIX}):boolean{{return a===b;}}export function main():void{{console.log(lt(9007199254740992,9007199254740993n));console.log(eq(9007199254740992,9007199254740993n));console.log(eq('9007199254740993',9007199254740993n));console.log(lt(1n,'1.5'));console.log(le(1n,'bad'));console.log(lt(1n,1.5));console.log(lt(1n,Infinity));console.log(lt(-Infinity,1n));console.log(le(NaN,1n));console.log(lt('𐀀','\\ue000'));console.log(strict('\\ud800','\\ud800'));console.log(strict('a\\u0000b','a\\u0000b'));console.log(1==='1');console.log(1=='1');console.log(null==undefined);console.log(null===undefined);}}"
    );
    run(
        &source,
        &[
            "1", "0", "1", "0", "0", "1", "1", "1", "0", "1", "1", "1", "0", "1", "1", "0",
        ],
    );
}
#[test]
fn primitive_comparison_evaluates_sources_once_before_conversion() {
    run(
        "function left():number|string{console.log('left');return '2';}function right():number|bigint{console.log('right');return 2n;}export function main():void{console.log(left()==right());console.log(left()<right());}",
        &["left", "right", "1", "left", "right", "0"],
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{console.log(1=='1');}",
            P::CheckedF64Utf16V28
        )
        .is_err()
    );
}
#[test]
fn primitive_comparison_gc_keeps_left_payload_after_rhs_mutation() {
    let source = format!(
        "function churn():void{{let i=0;while(i<80){{const text='x'.repeat(20000);i++;}}}}function compare(a:{SIX},b:{SIX}):boolean{{return a==b;}}export function main():void{{let text='9007199254740993';const rhs=():bigint=>{{text='changed';churn();return 9007199254740993n;}};const equal=text==rhs();churn();console.log(equal);console.log(text);let big=1n;const right=():number=>{{big=100n;churn();return 1;}};console.log(big==right());console.log(big);}}"
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(&source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Finish deliberate collections without changing production limits.
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
            "varta log: 1",
            "varta log: changed",
            "varta log: 1",
            "varta log: 100"
        ]
    );
}
#[test]
fn primitive_comparison_numeric_loops_keep_guest_fast_paths() {
    let source = "export function main():void{let sum=0;for(let i=0;i<1500;i++){if(i!==3&&i!=null){sum+=i;}}console.log(sum);}";
    let bytes = compile(source);
    for name in ["primitive_compare", "primitive_equal"] {
        assert!(!bytes.windows(name.len()).any(|v| v == name.as_bytes()));
    }
    run(source, &["1124247"]);
}
#[test]
fn primitive_comparison_malformed_transport_and_native_work_remain_fatal() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    const LOW_FUEL: u64 = 10_000;
    let source = format!(
        "function make(big:boolean):{SIX}{{if(big){{return 1n;}}return '1'.repeat(20000);}}function compare(a:{SIX},b:{SIX},strict:boolean):boolean{{try{{if(strict){{return a===b;}}return a<b;}}catch(e){{console.log('caught');return true;}}finally{{console.log('finally');}}}}export function main():void{{}}"
    );
    let bytes = compile(&source);
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(main < 128 && data[0] < 125);
            data[0] += 2;
            for (name, index) in [("make", main - 2), ("compare", main - 1)] {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, index]);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mode in 0..4 {
        let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let make = instance
            .get_typed_func::<i32, i64>(&mut execution.store, "make")
            .unwrap();
        let a = make.call(&mut execution.store, 0).unwrap();
        execution.store.set_fuel(varta_common::abi::FUEL).unwrap();
        let b = make
            .call(&mut execution.store, i32::from(mode != 3))
            .unwrap();
        if mode == 1 {
            let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
            let cell = varta_common::value::ReferenceBox::read_with_version(
                memory.data(&execution.store),
                b as u64,
                varta_common::value::BIGINT_VALUE_VERSION,
            )
            .unwrap();
            let header = cell.payload as u32 as usize - varta_common::abi::STRING_HEADER_BYTES;
            memory
                .write(
                    &mut execution.store,
                    header + varta_common::value::HEAP_FLAGS_OFFSET,
                    &varta_common::value::HEAP_ALLOCATED_FLAG.to_le_bytes(),
                )
                .unwrap();
        }
        execution
            .store
            .set_fuel(if mode >= 2 {
                LOW_FUEL
            } else {
                varta_common::abi::FUEL
            })
            .unwrap();
        let result = instance
            .get_typed_func::<(i64, i64, i32), i32>(&mut execution.store, "compare")
            .unwrap()
            .call(&mut execution.store, (a, b, i32::from(mode != 2)));
        if mode == 0 {
            assert_eq!(result.unwrap(), 0);
            assert_eq!(execution.store.data().output.lines, ["varta log: finally"]);
        } else {
            let error = result.unwrap_err();
            assert!(
                matches!(error.root_cause().downcast_ref::<varta_bigint::Error>(),Some(varta_bigint::Error::InvalidPayload)if mode==1)
                    || matches!(error.root_cause().downcast_ref::<varta_bigint::Error>(),Some(varta_bigint::Error::WorkBudget)if mode>=2),
                "{mode}: {error:#}"
            );
            assert!(execution.store.data().output.lines.is_empty());
        }
    }
}
