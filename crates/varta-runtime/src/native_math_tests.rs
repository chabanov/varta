use super::*;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use varta_common::{language::LanguageProfile, native_math::NativeMath};

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V16)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn exported_math() -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let mut source = String::new();
    for &operation in NativeMath::ALL {
        let name = operation.name();
        let (parameters, args) = if operation.arity() == 2 {
            ("a:number,b:number", "a,b")
        } else {
            ("a:number", "a")
        };
        source.push_str(&format!(
            "function fixture_{name}({parameters}):number{{return Math.{name}({args});}}"
        ));
    }
    source.push_str("export function main():void{}");
    let bytes = compile(&source);
    let first = export_index(&bytes, "_start") - NativeMath::ALL.len() as u8;
    rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(usize::from(data[0]) + NativeMath::ALL.len() < 128);
            data[0] += NativeMath::ALL.len() as u8;
            for (index, operation) in NativeMath::ALL.iter().enumerate() {
                let name = operation.name();
                data.push(name.len() as u8);
                data.extend_from_slice(name.as_bytes());
                data.push(0);
                data.push(first + index as u8);
            }
        }
        vec![data]
    })
}
fn assert_number(actual: f64, expected: f64, label: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}: {actual}");
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{label}");
    }
}

#[test]
fn native_math_unary_special_values_are_exact_and_charge_one_call() {
    use NativeMath::*;
    // Explicit expectations for +0, -0, +Infinity, -Infinity, in that order.
    let cases = [
        (Acos, [FRAC_PI_2, FRAC_PI_2, f64::NAN, f64::NAN]),
        (Acosh, [f64::NAN, f64::NAN, f64::INFINITY, f64::NAN]),
        (Asin, [0.0, -0.0, f64::NAN, f64::NAN]),
        (Asinh, [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY]),
        (Atan, [0.0, -0.0, FRAC_PI_2, -FRAC_PI_2]),
        (Atanh, [0.0, -0.0, f64::NAN, f64::NAN]),
        (Cbrt, [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY]),
        (Cos, [1.0, 1.0, f64::NAN, f64::NAN]),
        (Cosh, [1.0, 1.0, f64::INFINITY, f64::INFINITY]),
        (Exp, [1.0, 1.0, f64::INFINITY, 0.0]),
        (Expm1, [0.0, -0.0, f64::INFINITY, -1.0]),
        (
            Log,
            [
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NAN,
            ],
        ),
        (Log1p, [0.0, -0.0, f64::INFINITY, f64::NAN]),
        (
            Log10,
            [
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NAN,
            ],
        ),
        (
            Log2,
            [
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NAN,
            ],
        ),
        (Sin, [0.0, -0.0, f64::NAN, f64::NAN]),
        (Sinh, [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY]),
        (Tan, [0.0, -0.0, f64::NAN, f64::NAN]),
        (Tanh, [0.0, -0.0, 1.0, -1.0]),
    ];
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_math()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let mut calls = 0;
    for (operation, expected) in cases {
        let f = instance
            .get_typed_func::<f64, f64>(&mut execution.store, operation.name())
            .unwrap();
        for (x, expected) in [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY]
            .into_iter()
            .zip(expected)
            .chain([(f64::NAN, f64::NAN)])
        {
            let actual = f.call(&mut execution.store, x).unwrap();
            assert_number(actual, expected, operation.name());
            calls += 1;
            assert_eq!(execution.store.data().host_calls, calls);
        }
    }
}

#[test]
fn native_math_atan2_signed_axes_infinities_and_nan_match_quadrants() {
    let cases = [
        (0.0, 0.0, 0.0),
        (-0.0, 0.0, -0.0),
        (0.0, -0.0, PI),
        (-0.0, -0.0, -PI),
        (0.0, -1.0, PI),
        (-0.0, -1.0, -PI),
        (0.0, 1.0, 0.0),
        (-0.0, 1.0, -0.0),
        (1.0, 0.0, FRAC_PI_2),
        (-1.0, 0.0, -FRAC_PI_2),
        (1.0, -0.0, FRAC_PI_2),
        (-1.0, -0.0, -FRAC_PI_2),
        (f64::INFINITY, f64::INFINITY, FRAC_PI_4),
        (f64::INFINITY, f64::NEG_INFINITY, 3.0 * FRAC_PI_4),
        (f64::NEG_INFINITY, f64::INFINITY, -FRAC_PI_4),
        (f64::NEG_INFINITY, f64::NEG_INFINITY, -3.0 * FRAC_PI_4),
        (1.0, f64::INFINITY, 0.0),
        (-1.0, f64::INFINITY, -0.0),
        (1.0, f64::NEG_INFINITY, PI),
        (-1.0, f64::NEG_INFINITY, -PI),
        (f64::INFINITY, 1.0, FRAC_PI_2),
        (f64::NEG_INFINITY, -1.0, -FRAC_PI_2),
        (f64::NAN, 1.0, f64::NAN),
        (1.0, f64::NAN, f64::NAN),
    ];
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_math()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let f = instance
        .get_typed_func::<(f64, f64), f64>(&mut execution.store, "atan2")
        .unwrap();
    for (index, (y, x, expected)) in cases.into_iter().enumerate() {
        assert_number(
            f.call(&mut execution.store, (y, x)).unwrap(),
            expected,
            "atan2",
        );
        assert_eq!(execution.store.data().host_calls, index as u32 + 1);
    }
}

#[test]
fn native_math_domain_boundaries_and_small_argument_accuracy() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_math()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    for (name, x, expected) in [
        ("acos", 1.0, 0.0),
        ("acos", -1.0, PI),
        ("acos", 1.0_f64.next_up(), f64::NAN),
        ("asin", 1.0, FRAC_PI_2),
        ("asin", -1.0, -FRAC_PI_2),
        ("asin", -1.0_f64.next_up(), f64::NAN),
        ("acosh", 1.0, 0.0),
        ("acosh", 1.0_f64.next_down(), f64::NAN),
        ("atanh", 1.0, f64::INFINITY),
        ("atanh", -1.0, f64::NEG_INFINITY),
        ("atanh", 1.0_f64.next_up(), f64::NAN),
        ("log", 1.0, 0.0),
        ("log2", 1.0, 0.0),
        ("log10", 1.0, 0.0),
        ("log", -1.0, f64::NAN),
        ("log1p", -1.0, f64::NEG_INFINITY),
        ("log1p", (-1.0_f64).next_down(), f64::NAN),
        ("expm1", f64::from_bits(1), f64::from_bits(1)),
        ("log1p", f64::from_bits(1), f64::from_bits(1)),
        ("asinh", -f64::from_bits(1), -f64::from_bits(1)),
        ("atanh", f64::from_bits(1), f64::from_bits(1)),
    ] {
        let f = instance
            .get_typed_func::<f64, f64>(&mut execution.store, name)
            .unwrap();
        assert_number(f.call(&mut execution.store, x).unwrap(), expected, name);
    }
}

#[test]
fn native_math_finite_results_agree_with_independent_known_values() {
    let cases = [
        ("acos", 0.5, std::f64::consts::FRAC_PI_3),
        ("acosh", 2.0, 1.3169578969248168),
        ("asin", 0.5, std::f64::consts::FRAC_PI_6),
        ("asinh", 1.0, 0.881373587019543),
        ("atan", 1.0, FRAC_PI_4),
        ("atanh", 0.5, 0.5493061443340549),
        ("cbrt", 8.0, 2.0),
        ("cos", PI, -1.0),
        ("cosh", 1.0, 1.5430806348152437),
        ("exp", 1.0, std::f64::consts::E),
        ("expm1", 1e-10, 1.00000000005e-10),
        ("log", 2.0, std::f64::consts::LN_2),
        ("log1p", 1e-10, 9.999999999500001e-11),
        ("log10", 1000.0, 3.0),
        ("log2", 8.0, 3.0),
        ("sin", PI / 6.0, 0.5),
        ("sinh", 1.0, 1.1752011936438014),
        ("tan", FRAC_PI_4, 1.0),
        ("tanh", 1.0, 0.7615941559557649),
    ];
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_math()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    for (name, x, expected) in cases {
        let actual = instance
            .get_typed_func::<f64, f64>(&mut execution.store, name)
            .unwrap()
            .call(&mut execution.store, x)
            .unwrap();
        assert!(
            (actual - expected).abs() <= expected.abs() * 8.0 * f64::EPSILON,
            "{name}({x}): {actual}, expected {expected}"
        );
    }
}

#[test]
fn native_math_profile_inherits_values_json_functions_sparse_arrays_and_guest_math() {
    let out = run(
        "export function main():void{const a:number[]=[1,,3];a[5]=2;delete a[0];console.log(JSON.stringify(a));const r=varta.json.parse<{n?:number}>('{}');r.n??=2;console.log(r.n**=3);let n=1;const next=():number=>n++;console.log(next());console.log(next());console.log(Math.hypot(3,4));console.log(Math.f16round(1.0004882812500002));console.log(Number.isSafeInteger(Number.MAX_SAFE_INTEGER));console.log(Math.sin(-0));console.log(Math.log(0));console.log(Math.exp(0));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[null,null,3,null,null,2]",
            "8",
            "1",
            "2",
            "5",
            "1.0009765625",
            "1",
            "-0",
            "-Infinity",
            "1"
        ])
    );
}

#[test]
fn native_math_rejects_old_profiles_types_arity_shadowing_and_borrowed_calls() {
    for &operation in NativeMath::ALL {
        let args = if operation.arity() == 2 { "1,2" } else { "1" };
        let source = format!(
            "export function main():void{{Math.{}({args});}}",
            operation.name()
        );
        for profile in varta_common::language::SUPPORTED_PROFILES
            .iter()
            .copied()
            .filter(|p| !p.native_math())
        {
            assert!(
                varta_compiler::compile_typescript_with_profile(&source, profile).is_err(),
                "{profile:?}: {source}"
            );
        }
    }
    for statement in [
        "Math.sin('1');",
        "Math.log(false);",
        "Math.acos();",
        "Math.exp(1,2);",
        "Math.atan2(1);",
        "Math.atan2(1,2,3);",
        "Math.atan2(1,'2');",
        "Math.sin(...[1]);",
        "Math.sin<number>(1);",
        "Math['sin'](1);",
        "const f=Math.sin;f(1);",
        "const n:number|undefined=1;Math.sin(n);",
        "const Math={sin:1};Math.sin(1);",
        "Math.sin(1);let Math={sin:1};",
        "Math.random();",
    ] {
        let source = format!("export function main():void{{{statement}}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V16
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn native_math_json_helpers_reject_every_reachable_native_operation() {
    use crate::contract_tests::{export_index, rewrite};
    for &operation in NativeMath::ALL {
        let args = if operation.arity() == 2 { "0,1" } else { "1" };
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{const n=Math.{}({args});return value;}}export function main():void{{varta.json.parse<number>('1');}}",
            operation.name()
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
        let error = VartaRuntime::new().validate_module(&bad).unwrap_err();
        assert!(
            error.to_string().contains("cannot call host imports"),
            "{}: {error}",
            operation.name()
        );
    }
}

#[test]
fn native_math_host_budget_and_deadline_traps_remain_fatal() {
    for source in ["Math.sin(0)", "Math.atan2(0,1)"] {
        for exhausted in [false, true] {
            let bytes = compile(&format!(
                "export function main():void{{try{{console.log({source});}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
            ));
            let runtime = VartaRuntime::new();
            let module = runtime.validate_module(&bytes).unwrap();
            let mut execution =
                crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
            let instance = execution.instantiate(&module).unwrap();
            if exhausted {
                execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
            } else {
                execution.store.data_mut().deadline = Instant::now();
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
fn native_math_all_exponents_preserve_domains_and_bounded_function_ranges() {
    let mut values = Vec::new();
    for exponent in 0..=0x7ff_u64 {
        for fraction in [0, 1, 0x7_ffff_ffff_ffff, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | fraction);
            values.extend([x, -x]);
        }
    }
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_math()).unwrap();
    for batch in values.chunks(32) {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        for &operation in NativeMath::ALL.iter().filter(|op| op.arity() == 1) {
            let f = instance
                .get_typed_func::<f64, f64>(&mut execution.store, operation.name())
                .unwrap();
            for &x in batch {
                use NativeMath::*;
                let expected_nan = x.is_nan()
                    || match operation {
                        Acos | Asin | Atanh => x.abs() > 1.0,
                        Acosh => x < 1.0,
                        Log | Log2 | Log10 => x < 0.0,
                        Log1p => x < -1.0,
                        Sin | Cos | Tan => x.is_infinite(),
                        _ => false,
                    };
                let actual = f.call(&mut execution.store, x).unwrap();
                assert_eq!(
                    actual.is_nan(),
                    expected_nan,
                    "{}({x}) = {actual}",
                    operation.name()
                );
                if !expected_nan {
                    match operation {
                        Sin | Cos | Tanh => assert!(actual.abs() <= 1.0),
                        Atan | Asin => assert!(actual.abs() <= FRAC_PI_2),
                        Acos => assert!((0.0..=PI).contains(&actual)),
                        Exp => assert!(actual >= 0.0),
                        Cosh => assert!(actual >= 1.0),
                        Expm1 => assert!(actual >= -1.0),
                        Asinh | Cbrt => assert_eq!(actual.is_sign_negative(), x.is_sign_negative()),
                        _ => {}
                    }
                }
            }
        }
        assert_eq!(execution.store.data().host_calls, batch.len() as u32 * 19);
    }
}

#[test]
fn native_math_abi_profile_signature_and_unknown_import_mismatches_fail_closed() {
    use crate::contract_tests::rewrite;
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let runtime = VartaRuntime::new();
    for &operation in NativeMath::ALL {
        let args = if operation.arity() == 2 { "0,1" } else { "0" };
        let bytes = compile(&format!(
            "export function main():void{{console.log(Math.{}({args}));}}",
            operation.name()
        ));
        for action in 0..4 {
            let bad = rewrite(&bytes, |id, data| {
                let mut data = data.to_vec();
                if id == 0
                    && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                    && action < 2
                {
                    let at = 1 + abi::SECTION.len();
                    data[at..].copy_from_slice(&abi::NUMBER_POWER_VERSION.to_le_bytes());
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
                            profile: LanguageProfile::CheckedF64Utf16V15,
                        }
                        .to_bytes()
                        .unwrap(),
                    );
                }
                if id == 1 && action == 2 {
                    let signature = if operation.arity() == 2 {
                        vec![0x60, 2, 0x7c, 0x7c, 1, 0x7c]
                    } else {
                        vec![0x60, 1, 0x7c, 1, 0x7c]
                    };
                    let at = data
                        .windows(signature.len())
                        .position(|s| s == signature)
                        .unwrap();
                    data[at + 2] = 0x7f;
                }
                if id == 2 && action == 3 {
                    let name = operation.host().name().as_bytes();
                    let at = data.windows(name.len()).position(|s| s == name).unwrap();
                    data[at] = b'N';
                }
                vec![data]
            });
            assert!(
                runtime.validate_module(&bad).is_err(),
                "{} action {action}",
                operation.name()
            );
        }
    }
    let old = varta_compiler::compile_typescript_with_profile(
        "export function main():void{console.log(Math.pow(2,3));}",
        LanguageProfile::CheckedF64Utf16V15,
    )
    .unwrap();
    assert_eq!(
        runtime.run_wasm_bytes(&old, &[]).unwrap().lines,
        lines(&["8"])
    );
}

#[test]
fn native_math_argument_order_exceptions_status_pairs_and_gc_are_preserved() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function keep(first:{text:string},pair:VartaResult<string>,n:number):string{churn();console.log(pair.status);console.log(1/n);return first.text;}function saved():number{try{return Math.asin(-0);}finally{churn();}}function left():number{console.log('left');return -0;}function right():number{console.log('right');return -1;}function fail():number{console.log('fail');throw RangeError('argument');}export function main():void{console.log(Math.atan2(left(),right()));try{Math.atan2(fail(),right());}catch({message}){console.log(message);}console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('NATIVE_MATH_GC')),Math.atan2(-0,churn())));console.log(1/saved());console.log(varta.result.status());}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
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
            "left",
            "right",
            "-3.141592653589793",
            "fail",
            "argument",
            "<capability denied: env> (NATIVE_MATH_GC)",
            "1",
            "-Infinity",
            "earlier!",
            "-Infinity",
            "1"
        ])
    );
}
