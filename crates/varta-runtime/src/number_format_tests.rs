use super::*;
use varta_common::{language::LanguageProfile, number_format::NumberFormat};
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V22)
        .unwrap()
}
fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn number_format_methods_apply_defaults_primitive_coercion_and_padding() {
    let source = "function show(p:number|string|boolean|null|undefined):void{try{console.log((1.25).toFixed(p));console.log((1.25).toExponential(p));console.log((1.25).toPrecision(p));}catch(error){console.log(error instanceof RangeError);}}export function main():void{console.log((1.25).toFixed());console.log((1.25).toExponential());console.log((1.25).toPrecision());show(undefined);show('2.9');show(true);show(null);console.log((-0).toFixed(2));console.log((-0.001).toFixed(2));console.log((999).toPrecision(2));console.log((0.000001).toPrecision(3));console.log((0.0000001).toPrecision(3));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "1",
            "1.25e+0",
            "1.25",
            "1",
            "1.25e+0",
            "1.25",
            "1.25",
            "1.25e+0",
            "1.3",
            "1.3",
            "1.3e+0",
            "1",
            "1",
            "1e+0",
            "1",
            "0.00",
            "-0.00",
            "1.0e+3",
            "0.00000100",
            "1.00e-7"
        ])
    );
}
#[test]
fn number_format_nonfinite_order_and_invalid_digits_match_each_method() {
    let source = "function check(x:number,p:number):void{try{console.log(x.toFixed(p));}catch(error){console.log('fixed range');}try{console.log(x.toExponential(p));}catch(error){console.log('exp range');}try{console.log(x.toPrecision(p));}catch(error){console.log('precision range');}}export function main():void{check(Infinity,101);check(NaN,-1);check(1,101);check(1,NaN);check(Infinity,Infinity);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "fixed range",
            "Infinity",
            "Infinity",
            "fixed range",
            "NaN",
            "NaN",
            "fixed range",
            "exp range",
            "precision range",
            "1",
            "1e+0",
            "precision range",
            "fixed range",
            "Infinity",
            "Infinity"
        ])
    );
}
#[test]
fn number_format_receiver_and_all_arguments_run_before_coercion_and_range_checks() {
    let source = "function receiver():number{console.log('receiver');return 1.25;}function first():string{console.log('precision');return '2';}function extra():string{console.log('extra');return 'ignored';}function fail():number{throw Error('receiver');}export function main():void{console.log(receiver().toFixed(first(),extra(),{n:1},[1]));let p:number|string='2';console.log((1.25).toPrecision(p,p='1'));console.log(p);try{(NaN).toFixed(101,extra());}catch(error){console.log(error instanceof RangeError);}console.log((Infinity).toPrecision('invalid',extra()));try{fail().toExponential(first());}catch({message}){console.log(message);}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "receiver",
            "precision",
            "extra",
            "1.25",
            "1.3",
            "1",
            "extra",
            "1",
            "extra",
            "Infinity",
            "receiver"
        ])
    );
}
#[test]
fn number_format_rejects_old_profiles_non_numbers_objects_and_unsupported_call_forms() {
    for &mode in NumberFormat::ALL {
        for &profile in varta_common::language::SUPPORTED_PROFILES
            .iter()
            .filter(|p| !p.number_format())
        {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{(1).{}(2);}}", mode.name()),
                    profile
                )
                .is_err(),
                "{mode:?}/{profile:?}"
            );
        }
        for statement in [
            format!("(1).{}({{n:1}});", mode.name()),
            format!("(1).{}([2]);", mode.name()),
            format!("const n:number|undefined=1;n.{}(2);", mode.name()),
            format!("(1).{}(...[2]);", mode.name()),
            format!("(1).{}<number>(2);", mode.name()),
            format!("const f=(1).{};f(2);", mode.name()),
            format!("(1)['{}'](2);", mode.name()),
            format!("'1'.{}(2);", mode.name()),
        ] {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{{statement}}}"),
                    LanguageProfile::CheckedF64Utf16V22
                )
                .is_err(),
                "{statement}"
            );
        }
    }
}
#[test]
fn number_format_native_calls_produce_owned_text_and_charge_once() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    for &mode in NumberFormat::ALL {
        let bytes = compile(&format!(
            "function fixture(n:number,p:number):string{{return n.{}(p);}}export function main():void{{}}",
            mode.name()
        ));
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
        for digits in [1.0, 17.0, 100.0] {
            for x in [
                0.0,
                -0.0,
                1.25,
                -0.001,
                f64::from_bits(1),
                f64::MAX,
                f64::NAN,
                f64::INFINITY,
            ] {
                let packed = function.call(&mut execution.store, (x, digits)).unwrap() as u64;
                let start = packed as u32 as usize;
                let len = (packed >> 32) as usize;
                let text = String::from_utf16(
                    &varta_common::text::units(&memory.data(&execution.store)[start..start + len])
                        .unwrap(),
                )
                .unwrap();
                assert!(!text.is_empty() && text.len() <= 123);
                calls += 1;
                assert_eq!(execution.store.data().host_calls, calls);
            }
        }
        assert!(execution.store.data().output.lines.is_empty());
    }
}
#[test]
fn number_format_budgets_and_deadlines_stay_fatal_for_numeric_and_string_precision() {
    use crate::execution::Execution;
    let runtime = VartaRuntime::new();
    for &mode in NumberFormat::ALL {
        for arg in ["2", "'2'"] {
            let module=runtime.validate_module(&compile(&format!("export function main():void{{try{{console.log((1.25).{}({arg}));}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}",mode.name()))).unwrap();
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
}
#[test]
fn number_format_roots_pairs_saved_precisions_results_and_finalizers_survive_gc() {
    use crate::execution::Execution;
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 2;}function keep(first:{text:string},pair:VartaResult<string>,last:string):string{churn();console.log(pair.status);return first.text+last;}function saved():string{try{return (1.25).toFixed(2);}finally{(9.99).toExponential(churn(),String('overwrite'));}}export function main():void{let precision:number|string='0'+'2';console.log((1.25).toPrecision(precision,churn(),varta.result.capture(varta.env.get('FORMAT_GC_DENIED')),precision='1'));console.log(keep({text:'root'+'!'},varta.result.capture(varta.env.get('FORMAT_PAIR_DENIED')),(1.25).toExponential(2,churn())));console.log(saved());console.log(varta.result.status());try{(NaN).toFixed(101,churn());}catch(error){console.log(error instanceof RangeError);}}";
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
            "<capability denied: env> (FORMAT_GC_DENIED)",
            "1.3",
            "<capability denied: env> (FORMAT_PAIR_DENIED)",
            "1",
            "root!1.25e+0",
            "1.25",
            "1",
            "1"
        ])
    );
}
#[test]
fn number_format_abi_signatures_and_helper_reachability_fail_closed() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let runtime = VartaRuntime::new();
    for &mode in NumberFormat::ALL {
        let bytes = compile(&format!(
            "export function main():void{{(1.25).{}(2);}}",
            mode.name()
        ));
        for action in 0..4 {
            let bad = rewrite(&bytes, |id, data| {
                let mut data = data.to_vec();
                if id == 0
                    && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                    && action < 2
                {
                    data[1 + abi::SECTION.len()..]
                        .copy_from_slice(&abi::NUMBER_RADIX_VERSION.to_le_bytes());
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
                            profile: LanguageProfile::CheckedF64Utf16V21,
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
                    let name = mode.host().name().as_bytes();
                    let at = data.windows(name.len()).position(|s| s == name).unwrap();
                    data[at] = b'N';
                }
                vec![data]
            });
            assert!(runtime.validate_module(&bad).is_err(), "{mode:?}/{action}");
        }
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{(1.25).{}(2);return value;}}export function main():void{{varta.json.parse<number>('1');}}",
            mode.name()
        ));
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
}
#[test]
fn number_format_value_of_is_guest_only_and_preserves_every_numeric_exponent() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let bytes = compile(
        "function fixture(n:number):number{return n.valueOf();}export function main():void{}",
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
    assert!(runtime.inspect_module(&bytes).unwrap().imports.is_empty());
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let function = instance
        .get_typed_func::<f64, f64>(&mut execution.store, "identity")
        .unwrap();
    for exponent in 0..=2047_u64 {
        for fraction in [0, 1, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | fraction);
            for x in [x, -x] {
                let actual = function.call(&mut execution.store, x).unwrap();
                if x.is_nan() {
                    assert!(actual.is_nan());
                } else {
                    assert_eq!(actual.to_bits(), x.to_bits());
                }
            }
        }
    }
    assert_eq!(execution.store.data().host_calls, 0);
}
#[test]
fn number_format_value_of_caches_receiver_and_evaluates_ignored_arguments_without_coercion() {
    let source = "function receiver():number{console.log('receiver');return -0;}function fail():string{throw Error('argument');}export function main():void{console.log(1/receiver().valueOf('ignored',{n:1},[1],varta.result.capture(varta.env.get('VALUE_OF_DENIED'))));console.log(varta.result.status());let n=1;console.log(n.valueOf(n=2));console.log(n);try{receiver().valueOf(fail(),console.log('BAD_LATER'));}catch({message}){console.log(message);}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "receiver",
            "<capability denied: env> (VALUE_OF_DENIED)",
            "-Infinity",
            "1",
            "1",
            "2",
            "receiver",
            "argument"
        ])
    );
    for statement in [
        "'1'.valueOf();",
        "const n:number|undefined=1;n.valueOf();",
        "(1).valueOf(...[]);",
        "const f=(1).valueOf;f();",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V22
            )
            .is_err()
        );
    }
    for &profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .filter(|p| !p.number_format())
    {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{(1).valueOf();}",
                profile
            )
            .is_err()
        );
    }
}
