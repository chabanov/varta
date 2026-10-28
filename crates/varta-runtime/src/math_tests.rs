use super::*;
use varta_common::language::LanguageProfile;

const METHODS: [&str; 6] = ["abs", "floor", "ceil", "trunc", "round", "sqrt"];
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V15)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
fn exported_math() -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let mut source = String::new();
    for method in METHODS {
        source.push_str(&format!(
            "function fixture_{method}(value:number):number{{return Math.{method}(value);}}"
        ));
    }
    source.push_str("export function main():void{}");
    let bytes = compile(&source);
    let first = export_index(&bytes, "_start") - METHODS.len() as u8;
    rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(usize::from(data[0]) + METHODS.len() < 128);
            data[0] += METHODS.len() as u8;
            for (index, name) in METHODS.iter().enumerate() {
                data.push(name.len() as u8);
                data.extend_from_slice(name.as_bytes());
                data.push(0);
                data.push(first + index as u8);
            }
        }
        vec![data]
    })
}
fn expected(method: &str, x: f64) -> f64 {
    match method {
        "abs" => x.abs(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "trunc" => x.trunc(),
        "sqrt" => x.sqrt(),
        "round" => {
            // Independent floor/distance oracle, without adding .5 to x.
            if !x.is_finite() || x == 0.0 || x.abs() >= 4503599627370496.0 {
                return x;
            }
            if (-0.5..0.0).contains(&x) || x == -0.5 {
                return -0.0;
            }
            let lower = x.floor();
            if x - lower < 0.5 { lower } else { lower + 1.0 }
        }
        _ => unreachable!(),
    }
}
fn check_values(values: &[f64]) {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_math()).unwrap();
    for batch in values.chunks(128) {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        for method in METHODS {
            let function = instance
                .get_typed_func::<f64, f64>(&mut execution.store, method)
                .unwrap();
            for &x in batch {
                let actual = function.call(&mut execution.store, x).unwrap();
                let expected = expected(method, x);
                if expected.is_nan() {
                    assert!(actual.is_nan(), "{method}({x:?}) = {actual:?}");
                } else {
                    assert_eq!(actual.to_bits(), expected.to_bits(), "{method}({x:?})");
                }
            }
        }
        assert_eq!(execution.store.data().host_calls, 0);
        crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    }
}

#[test]
fn math_special_values_signed_zero_and_halfway_neighbors_are_exact() {
    let mut values = vec![
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -0.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
    ];
    for x in [
        0.5_f64,
        -0.5,
        1.5,
        -1.5,
        2.5,
        -2.5,
        3.5,
        -3.5,
        4503599627370495.5,
        -4503599627370495.5,
        4503599627370496.0,
        9007199254740992.0,
        -9007199254740992.0,
    ] {
        values.extend([x.next_down(), x, x.next_up()]);
    }
    check_values(&values);
}

#[test]
fn math_all_binary64_exponents_and_representative_significands_match_oracle() {
    let mut values = Vec::new();
    for exponent in 0..=0x7ff_u64 {
        for mantissa in [
            0,
            1,
            0x7_ffff_ffff_ffff,
            0xf_ffff_ffff_fffe,
            0xf_ffff_ffff_ffff,
        ] {
            for sign in [0, 1_u64 << 63] {
                values.push(f64::from_bits(sign | (exponent << 52) | mantissa));
            }
        }
    }
    check_values(&values);
}

#[test]
fn math_nested_calls_narrowing_callbacks_and_storage_preserve_values() {
    let out = run(
        r"function read(value:number|undefined):number{if(value!==undefined){return Math.abs(value);}return 0;}export function main():void{console.log(Math.round(Math.sqrt(Math.abs(-6.25))));console.log(Math.floor(-1.2));console.log(Math.ceil(-1.2));console.log(Math.trunc(-1.2));console.log(read(-3));console.log(JSON.stringify([-2.5,-0.5,0.5,2.5].map(x=>Math.round(x))));const a=[0];a[0]=Math.sqrt(81);const r={n:0};r.n=Math.abs(-4);console.log(a[0]);console.log(r.n);console.log(1/Math.round(-0.5));console.log(1/Math.abs(-0));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "3",
            "-2",
            "-1",
            "-1",
            "3",
            "[-2,0,1,3]",
            "9",
            "4",
            "-Infinity",
            "Infinity"
        ])
    );
}

#[test]
fn math_arguments_evaluate_once_and_exceptions_skip_later_work() {
    let out = run(
        r"function fail():number{console.log('fail');throw RangeError('argument');}function later():number{console.log('BAD_LATER');return 1;}function take(a:number,b:number):void{console.log('BAD_BODY');}export function main():void{let count=0;const next=():number=>{count++;return -1.5;};console.log(Math.round(next()));console.log(count);try{take(Math.sqrt(fail()),later());}catch({message}){console.log(message);}finally{console.log('final');}console.log(count);}",
    );
    assert_eq!(
        out.lines,
        lines(&["-1", "1", "fail", "argument", "final", "1"])
    );
}

#[test]
fn math_rejects_coercion_arity_aliases_computed_calls_and_old_profiles() {
    for statement in [
        "Math.abs('2');",
        "Math.floor(false);",
        "Math.ceil(null);",
        "Math.trunc(undefined);",
        "Math.round({n:2});",
        "Math.sqrt([2]);",
        "const n:number|undefined=2;Math.abs(n);",
        "Math.abs();",
        "Math.abs(1,2);",
        "Math.abs(...[1]);",
        "Math.abs<number>(1);",
        "Math['abs'](1);",
        "const abs=Math.abs;abs(1);",
        "const math=Math;math.abs(1);",
        "Math.pow(2,3);",
        "Math.min(1,2);",
        "console.log(Math.PI);",
    ] {
        let source = format!("export function main():void{{{statement}}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V15
            )
            .is_err(),
            "{source}"
        );
    }
    for profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .copied()
        .filter(|p| !p.math_intrinsics())
    {
        for method in METHODS {
            let source = format!("export function main():void{{console.log(Math.{method}(1));}}");
            assert!(
                varta_compiler::compile_typescript_with_profile(&source, profile).is_err(),
                "{profile:?}: {method}"
            );
        }
    }
}

#[test]
fn math_shadowing_parameters_captures_declarations_and_tdz_never_use_intrinsic() {
    for source in [
        "export function main():void{const Math={abs:1};Math.abs(1);}",
        "function f(Math:{abs:number}):number{return Math.abs(1);}export function main():void{}",
        "export function main():void{const Math={abs:1};const f=():number=>Math.abs(1);f();}",
        "export function main():void{Math.abs(1);let Math={abs:1};}",
        "function Math():number{return 1;}export function main():void{Math.abs(1);}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V15,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("shadowed Math"),
            "{source}: {error}"
        );
    }
}

#[test]
fn math_guest_intrinsics_do_not_consume_host_allowance_or_change_status() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_math()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
    for method in METHODS {
        let f = instance
            .get_typed_func::<f64, f64>(&mut execution.store, method)
            .unwrap();
        assert_eq!(
            f.call(&mut execution.store, 4.0).unwrap().to_bits(),
            expected(method, 4.0).to_bits()
        );
    }
    assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    let out = run(
        "export function main():void{varta.env.get('VARTA_MATH_DENIED');console.log(Math.sqrt(9));console.log(Math.round(-0.5));console.log(Math.abs(-0));console.log(Math.floor(NaN));console.log(varta.result.status());}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "<capability denied: env> (VARTA_MATH_DENIED)",
            "3",
            "-0",
            "0",
            "NaN",
            "1"
        ])
    );
}

#[test]
fn math_json_helper_validation_traverses_argument_effects() {
    use crate::contract_tests::{export_index, rewrite};
    for (argument, pure) in [("4", true), ("read()", false)] {
        let bytes = compile(&format!(
            "function read():number{{varta.env.get('MATH_EFFECT');return 4;}}function fake(value:boolean):boolean{{const n=Math.sqrt({argument});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
        ));
        let fake = export_index(&bytes, "_start") - 1;
        let name = varta_common::value::VALUE_ALLOCATE;
        let fixture = rewrite(&bytes, |id, data| {
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
        let result = VartaRuntime::new().validate_module(&fixture);
        if pure {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("cannot call host imports")
            );
        }
    }
}

#[test]
fn math_fuel_exhaustion_in_argument_remains_fatal() {
    let bytes = compile(
        "function forever():number{while(true){}return 1;}export function main():void{try{console.log(Math.sqrt(forever()));}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    assert!(
        instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .is_err()
    );
    assert!(execution.store.data().output.lines.is_empty());
}

#[test]
fn math_argument_allocations_preserve_earlier_roots_pairs_and_saved_return() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 6.25;}function keep(first:{text:string},result:VartaResult<string>,value:number):string{churn();console.log(result.status);console.log(value);return first.text;}function saved():number{try{return Math.round(-0.5);}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_MATH_GC')),Math.round(Math.sqrt(churn()))));console.log(1/saved());}";
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
            "<capability denied: env> (VARTA_MATH_GC)",
            "1",
            "3",
            "earlier!",
            "-Infinity"
        ])
    );
}

#[test]
fn math_preserves_existing_binary64_tool_contract_profile_boundary() {
    let source = "function read():number{varta.env.get('MATH_EFFECT');return 4;}export function check():boolean{return Math.sqrt(read())===2;}export function main():void{}varta.tool({name:'check',description:'Check'},check);";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V15,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("tool/HTTP contracts in binary64 profile")
    );
}
