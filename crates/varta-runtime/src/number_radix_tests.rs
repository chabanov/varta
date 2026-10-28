use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V21)
        .unwrap()
}
fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn number_radix_method_handles_default_fractional_and_primitive_union_radices() {
    let source = "function show(radix:number|string|boolean|null|undefined):void{const value=255;try{console.log(value.toString(radix));}catch(error){console.log(error instanceof RangeError);}}export function main():void{console.log((255).toString());console.log((255).toString(undefined));show(16);show('0x10');show(16.9);show(undefined);show(true);show(false);show(null);show('bad');console.log((0.1).toString(16));console.log((35).toString(36));console.log((-0).toString(2));console.log((NaN).toString(2));console.log((Infinity).toString(36));console.log(RangeError('old').toString());}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "255",
            "255",
            "ff",
            "ff",
            "ff",
            "255",
            "1",
            "1",
            "1",
            "1",
            "0.1999999999999a",
            "z",
            "0",
            "NaN",
            "Infinity",
            "RangeError: old"
        ])
    );
}
#[test]
fn number_radix_invalid_values_throw_range_error_before_nonfinite_special_cases() {
    let source = "function check(n:number,r:number):void{try{console.log(n.toString(r));}catch(error){console.log(error instanceof RangeError);}finally{console.log('done');}}export function main():void{check(NaN,1);check(Infinity,37);check(0,NaN);check(-0,-Infinity);check(1,Infinity);check(1,1.99);check(1,-2);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "1", "done", "1", "done", "1", "done", "1", "done", "1", "done", "1", "done", "1",
            "done"
        ])
    );
}
#[test]
fn number_radix_receiver_and_all_arguments_evaluate_before_coercion_and_validation() {
    let source = "function receiver():number{console.log('receiver');return 255;}function first():string{console.log('radix');return '16';}function extra():string{console.log('extra');return 'ignored';}function fail():number{throw Error('receiver failed');}export function main():void{console.log(receiver().toString(first(),extra(),{n:1},[1]));let r:string|number='16';console.log((255).toString(r,r='2'));console.log(r);try{(NaN).toString(1,extra());}catch({message}){console.log(message);}try{fail().toString(first());}catch({message}){console.log(message);}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "receiver",
            "radix",
            "extra",
            "ff",
            "ff",
            "2",
            "extra",
            "radix must be between 2 and 36",
            "receiver failed"
        ])
    );
}
#[test]
fn number_radix_old_profiles_and_unsupported_receivers_or_arguments_stay_rejected() {
    for &profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .filter(|p| !p.number_radix())
    {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{(255).toString(16);}",
                profile
            )
            .is_err(),
            "{profile:?}"
        );
    }
    for statement in [
        "(1).toString({n:1});",
        "(1).toString([16]);",
        "(1).toString(Error('edge'));",
        "const n:number|undefined=1;n.toString();",
        "const r:number|{n:number}=16;(1).toString(r);",
        "(1).toString(...[16]);",
        "(1).toString<number>(16);",
        "const f=(1).toString;f(16);",
        "(1)['toString'](16);",
        "'1'.toString(16);",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V21
            )
            .is_err(),
            "{statement}"
        );
    }
}
#[test]
fn number_radix_host_calls_own_their_utf16_results_and_charge_once_per_format() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let bytes = compile(
        "function fixture(n:number,r:number):string{return n.toString(r);}export function main():void{}",
    );
    let index = export_index(&bytes, "_start") - 1;
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && index < 128);
            data[0] += 1;
            data.push(6);
            data.extend(b"format");
            data.extend([0, index]);
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let function = instance
        .get_typed_func::<(f64, f64), i64>(&mut execution.store, "format")
        .unwrap();
    let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
    let mut calls = 0;
    for radix in 2..=36 {
        for x in [0.1, -255.0, f64::from_bits(1), f64::MAX] {
            let handle = function
                .call(&mut execution.store, (x, f64::from(radix)))
                .unwrap() as u64;
            let start = handle as u32 as usize;
            let length = (handle >> 32) as usize;
            let text = String::from_utf16(
                &varta_common::text::units(&memory.data(&execution.store)[start..start + length])
                    .unwrap(),
            )
            .unwrap();
            assert!(text.len() <= 1077 && !text.is_empty());
            assert_eq!(text.starts_with('-'), x.is_sign_negative());
            calls += 1;
            assert_eq!(execution.store.data().host_calls, calls);
        }
    }
    assert!(execution.store.data().output.lines.is_empty());
}
#[test]
fn number_radix_budget_and_deadline_exhaustion_remain_fatal() {
    use crate::execution::Execution;
    let runtime = VartaRuntime::new();
    for expression in ["(255).toString(16)", "(255).toString('16')"] {
        let module=runtime.validate_module(&compile(&format!("export function main():void{{try{{console.log({expression});}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"))).unwrap();
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
    }
}
#[test]
fn number_radix_saved_receiver_arguments_results_and_pairs_survive_gc_finalizers_and_buffer_reuse()
{
    use crate::execution::Execution;
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 16;}function keep(first:{text:string},pair:VartaResult<string>,last:string):string{churn();console.log(pair.status);return first.text+last;}function saved():string{try{return (255).toString(16);}finally{(65535).toString(churn(),String('overwrite'));}}export function main():void{let radix:number|string='1'+'6';console.log((255).toString(radix,churn(),varta.result.capture(varta.env.get('RADIX_GC_DENIED')),radix='2'));console.log(keep({text:'root'+'!'},varta.result.capture(varta.env.get('RADIX_PAIR_DENIED')),(35).toString(36,churn())));console.log(saved());console.log(varta.result.status());try{(NaN).toString('bad',churn());}catch({message}){console.log(message);}}";
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
            "<capability denied: env> (RADIX_GC_DENIED)",
            "ff",
            "<capability denied: env> (RADIX_PAIR_DENIED)",
            "1",
            "root!z",
            "ff",
            "1",
            "radix must be between 2 and 36"
        ])
    );
}
#[test]
fn number_radix_abi_signature_and_helper_reachability_mismatches_fail_before_effects() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let runtime = VartaRuntime::new();
    let bytes = compile("export function main():void{(255).toString(16);}");
    for action in 0..4 {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                && action < 2
            {
                data[1 + abi::SECTION.len()..]
                    .copy_from_slice(&abi::PRIMITIVE_NUMBER_VERSION.to_le_bytes());
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
                        profile: LanguageProfile::CheckedF64Utf16V20,
                    }
                    .to_bytes()
                    .unwrap(),
                );
            }
            if id == 1 && action == 2 {
                let signature = [0x60, 2, 0x7c, 0x7c, 1, 0x7f];
                let at = data
                    .windows(signature.len())
                    .position(|s| s == signature)
                    .unwrap();
                data[at + 2] = 0x7f;
            }
            if id == 2 && action == 3 {
                let name = b"number_to_string";
                let at = data.windows(name.len()).position(|s| s == name).unwrap();
                data[at] = b'N';
            }
            vec![data]
        });
        assert!(runtime.validate_module(&bad).is_err(), "action {action}");
    }
    let bytes = compile(
        "function fake(value:boolean):boolean{(255).toString(16);return value;}export function main():void{varta.json.parse<number>('1');}",
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
}
