use super::*;
use crate::execution::Execution;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V20)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn primitive_number_numeric_inputs_preserve_all_exponents_and_signed_zero_bits() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fixture(n:number):number{return Number(n);}export function main():void{}",
    );
    let index = export_index(&bytes, "_start") - 1;
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && index < 128);
            data[0] += 1;
            data.push(8);
            data.extend(b"identity");
            data.extend([0, index]);
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut values = Vec::new();
    for exponent in 0..=0x7ff_u64 {
        for fraction in [0, 1, 0x7_ffff_ffff_ffff, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | fraction);
            values.extend([x, -x]);
        }
    }
    for batch in values.chunks(32) {
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let function = instance
            .get_typed_func::<f64, f64>(&mut execution.store, "identity")
            .unwrap();
        for &x in batch {
            let actual = function.call(&mut execution.store, x).unwrap();
            if x.is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), x.to_bits());
            }
        }
        assert_eq!(execution.store.data().host_calls, batch.len() as u32);
    }
}
#[test]
fn primitive_number_converts_all_primitives_unions_and_numeric_string_forms() {
    let bytes = compile(
        "function show(v:number|string|boolean|null|undefined):void{console.log(Number(v));}export function main():void{console.log(1/Number());show(undefined);show(null);show(true);show(false);show('');show('  +1.5e2  ');show('0x20000000000001');show('0b101');show('0o77');show('invalid');show(-0);console.log(1/Number('-0'));console.log(Number('\\uFEFF\\u2028 42\\u3000'));console.log(Number('\\u008542'));console.log(Number('1\\uDFFF'));console.log(Number(console.log('void')));console.log(String(Number('1e21')));}",
    );
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&[
            "Infinity",
            "NaN",
            "0",
            "1",
            "0",
            "0",
            "150",
            "9007199254740992",
            "5",
            "63",
            "NaN",
            "0",
            "-Infinity",
            "42",
            "NaN",
            "NaN",
            "void",
            "NaN",
            "1e+21"
        ])
    );
}
#[test]
fn primitive_number_caches_first_value_before_all_ignored_effects_and_throws() {
    let bytes = compile(
        "function later():string{console.log('later');return 'ignored';}function fail():number{throw RangeError('argument');}export function main():void{let value:number|string='0x10';console.log(Number(value,value='2',later(),{n:1},[1],varta.result.capture(varta.env.get('NUMBER_ORDER_DENIED'))));console.log(value);console.log(varta.result.status());try{Number(3,fail(),later());}catch({message}){console.log(message);}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
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
            "later",
            "<capability denied: env> (NUMBER_ORDER_DENIED)",
            "16",
            "2",
            "1",
            "argument"
        ])
    );
    assert_eq!(execution.store.data().host_calls, 7);
}
#[test]
fn primitive_number_preserves_lexical_functions_and_rejects_objects_wrappers_and_old_profiles() {
    for source in [
        "export function main():void{const Number=(n:string):number=>7;console.log(Number('local'));}",
        "function Number(n:string):number{return 7;}export function main():void{console.log(Number('local'));}",
    ] {
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compile(source), &[])
                .unwrap()
                .lines,
            lines(&["7"])
        );
    }
    for statement in [
        "Number({n:1});",
        "Number([1]);",
        "Number(Error('edge'));",
        "const n:number|{n:number}=1;Number(n);",
        "Number(...[1]);",
        "Number<number>(1);",
        "new Number(1);",
        "Number(1);const Number=(n:number):number=>n;",
        "const f=Number;f(1);",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V20
            )
            .is_err(),
            "{statement}"
        );
    }
    for &profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .filter(|p| !p.primitive_number())
    {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{Number(1);}",
                profile
            )
            .is_err(),
            "{profile:?}"
        );
    }
}
#[test]
fn primitive_number_host_limits_are_fatal_and_argument_throws_skip_conversion() {
    let runtime = VartaRuntime::new();
    let module=runtime.validate_module(&compile("export function main():void{try{Number('1');}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}")).unwrap();
    for expired in [false, true] {
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        if expired {
            execution.store.data_mut().deadline = Instant::now();
        } else {
            execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        }
        assert!(
            instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .is_err()
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
    let module=runtime.validate_module(&compile("function fail():string{throw Error('first');}export function main():void{try{Number(fail(),console.log('BAD_LATER'));}catch({message}){console.log(message);}}")).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(execution.store.data().host_calls, 1);
    assert_eq!(execution.store.data().output.lines, lines(&["first"]));
}
#[test]
fn primitive_number_retains_earlier_roots_pairs_first_input_and_saved_returns_through_gc() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function keep(first:{text:string},pair:VartaResult<string>,last:number):string{churn();console.log(pair.status);console.log(last);return first.text;}function saved():number{try{return Number('-0');}finally{Number(String(churn()));}}export function main():void{const first:string|number='0x'+'10';console.log(Number(first,churn(),varta.result.capture(varta.env.get('NUMBER_GC_DENIED'))));console.log(keep({text:'root'+'!'},varta.result.capture(varta.env.get('NUMBER_PAIR_DENIED')),Number('5e-324',churn())));console.log(1/saved());console.log(varta.result.status());}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
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
            "<capability denied: env> (NUMBER_GC_DENIED)",
            "16",
            "<capability denied: env> (NUMBER_PAIR_DENIED)",
            "1",
            "5e-324",
            "root!",
            "-Infinity",
            "1"
        ])
    );
}
#[test]
fn primitive_number_reads_maximum_strings_without_mutating_result_buffers() {
    let source = "export function main():void{const input=' '.repeat(524288);console.log(Number(input));const owned=String('preserved');console.log(Number('invalid'));console.log(owned);console.log(Number('0x'+ '0'.repeat(524285)+'1'));console.log(varta.result.status());}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(1000 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        lines(&["0", "NaN", "preserved", "1", "0"])
    );
}
#[test]
fn primitive_number_rejects_downgraded_metadata_signatures_and_host_free_helper_calls() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let bytes = compile("export function main():void{Number(1);}");
    let runtime = VartaRuntime::new();
    for action in 0..4 {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                && action < 2
            {
                data[1 + abi::SECTION.len()..]
                    .copy_from_slice(&abi::PRIMITIVE_STRING_VERSION.to_le_bytes());
            }
            if id == 0
                && data.get(1..1 + varta_common::language::SECTION.len())
                    == Some(varta_common::language::SECTION.as_bytes())
                && action == 0
            {
                data.truncate(1 + varta_common::language::SECTION.len());
                data.extend(
                    LanguageContract {
                        schema_version: SCHEMA_VERSION,
                        profile: LanguageProfile::CheckedF64Utf16V19,
                    }
                    .to_bytes()
                    .unwrap(),
                );
            }
            if id == 1 && action == 2 {
                let signature = [0x60, 1, 0x7e, 1, 0x7c];
                let at = data
                    .windows(signature.len())
                    .position(|s| s == signature)
                    .unwrap();
                data[at + 2] = 0x7f;
            }
            if id == 2 && action == 3 {
                let name = b"primitive_to_number";
                let at = data.windows(name.len()).position(|s| s == name).unwrap();
                data[at] = b'P';
            }
            vec![data]
        });
        assert!(runtime.validate_module(&bad).is_err(), "action {action}");
    }
    let bytes = compile(
        "function fake(value:boolean):boolean{Number(1);return value;}export function main():void{varta.json.parse<number>('1');}",
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
    assert!(
        runtime
            .validate_module(&bad)
            .unwrap_err()
            .to_string()
            .contains("cannot call host imports")
    );
    // The compiler may export shared globals, but this host does not require them.
    let bytes = compile("export function main():void{console.log(Number('1'));}");
    let without_results = rewrite(&bytes, |id, data| {
        if id != 7 {
            return vec![data.to_vec()];
        }
        let mut encoded = vec![0];
        let mut at = 1;
        assert!(data[0] < 128);
        for _ in 0..data[0] {
            let begin = at;
            let length = usize::from(data[at]);
            assert!(length < 128);
            at += 1;
            let name = std::str::from_utf8(&data[at..at + length]).unwrap();
            at += length;
            assert!(data[at + 1] < 128);
            at += 2; // kind and the fixture's single-byte index
            if name != abi::RESULT_LEN && name != abi::RESULT_STATUS {
                encoded[0] += 1;
                encoded.extend_from_slice(&data[begin..at]);
            }
        }
        assert_eq!(at, data.len());
        vec![encoded]
    });
    assert_eq!(
        runtime.run_wasm_bytes(&without_results, &[]).unwrap().lines,
        lines(&["1"])
    );
}
