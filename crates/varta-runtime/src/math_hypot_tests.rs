use super::*;
use varta_common::language::LanguageProfile;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V15)
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
fn exported_hypot() -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function norm(a:number,b:number):number{return Math.hypot(a,b);}export function main():void{}",
    );
    let index = export_index(&bytes, "_start") - 1;
    let name = "fixture.hypot";
    rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && index < 128);
            data[0] += 1;
            data.push(name.len() as u8);
            data.extend_from_slice(name.as_bytes());
            data.push(0);
            data.push(index);
        }
        vec![data]
    })
}
fn check_pairs(pairs: &[(f64, f64)]) {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_hypot()).unwrap();
    assert_eq!(module.imports().len(), 0);
    for batch in pairs.chunks(64) {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let f = instance
            .get_typed_func::<(f64, f64), f64>(&mut execution.store, "fixture.hypot")
            .unwrap();
        execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        for &(a, b) in batch {
            let expected = a.hypot(b);
            let actual = f.call(&mut execution.store, (a, b)).unwrap();
            if expected.is_nan() {
                assert!(actual.is_nan(), "hypot({a},{b}) = {actual}");
            } else if expected == 0.0 || expected.is_infinite() {
                assert_eq!(actual.to_bits(), expected.to_bits(), "hypot({a},{b})");
            } else {
                assert!(
                    actual.is_finite() && actual >= 0.0,
                    "hypot({a},{b}) = {actual}"
                );
                assert!(
                    actual.to_bits().abs_diff(expected.to_bits()) <= 3,
                    "hypot({a},{b}) = {actual}, native {expected}"
                );
            }
        }
        assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    }
}

#[test]
fn hypot_all_exponents_and_extreme_pairs_agree_with_independent_native_hypot() {
    let mut pairs = Vec::new();
    let special = [
        0.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        1.0,
        -1.0,
    ];
    for a in special {
        for b in special {
            pairs.push((a, b));
        }
    }
    for exponent in 0..=0x7ff_u64 {
        for fraction in [0, 1, 0x7_ffff_ffff_ffff, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | fraction);
            pairs.extend([(x, 0.0), (-x, x), (x, x * 0.75), (x, 1.0), (1.0, -x)]);
        }
    }
    check_pairs(&pairs);
}

#[test]
fn hypot_scaled_three_four_five_triples_are_exact_through_subnormals() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_hypot()).unwrap();
    for batch in (-1074..=1021).collect::<Vec<i32>>().chunks(64) {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let f = instance
            .get_typed_func::<(f64, f64), f64>(&mut execution.store, "fixture.hypot")
            .unwrap();
        for &exponent in batch {
            let scale = if exponent < -1022 {
                f64::from_bits(1_u64 << (exponent + 1074))
            } else {
                f64::from_bits(((exponent + 1023) as u64) << 52)
            };
            let result = f
                .call(&mut execution.store, (3.0 * scale, -4.0 * scale))
                .unwrap();
            assert_eq!(
                result.to_bits(),
                (5.0 * scale).to_bits(),
                "scale 2^{exponent}"
            );
        }
    }
}

#[test]
fn hypot_zero_infinity_nan_and_variadic_arguments_have_number_semantics() {
    let out = run(
        "export function main():void{console.log(Math.hypot());console.log(1/Math.hypot(-0));console.log(1/Math.hypot(-0,0,-0));console.log(Math.hypot(-3));console.log(Math.hypot(3,4,12));console.log(Math.hypot(NaN,Infinity));console.log(Math.hypot(-Infinity,NaN));console.log(Math.hypot(NaN,1));console.log(Math.hypot(Math.hypot(3,4),12));console.log(Math.hypot(Number.MAX_VALUE,0)===Number.MAX_VALUE);console.log(Math.hypot(Number.MIN_VALUE,0)===Number.MIN_VALUE);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "0", "Infinity", "Infinity", "3", "13", "Infinity", "Infinity", "NaN", "13", "1", "1"
        ])
    );
}

#[test]
fn hypot_compensation_keeps_many_subulp_squares() {
    let args = std::iter::once("1")
        .chain(std::iter::repeat_n("0.000000007450580596923828125", 63))
        .collect::<Vec<_>>()
        .join(",");
    // 63*(2^-27)^2 survives compensated addition and moves the root by 8 ULPs.
    let source = format!(
        "export function main():void{{console.log(Math.hypot({args})===1.0000000000000018);}}"
    );
    assert_eq!(run(&source).lines, lines(&["1"]));
}

#[test]
fn hypot_finishes_all_argument_effects_and_propagates_abrupt_completion() {
    let out = run(
        "function mark(n:number):number{console.log(n);return n;}function fail():number{console.log('fail');throw TypeError('argument');}export function main():void{console.log(Math.hypot(mark(NaN),mark(Infinity),mark(3)));try{console.log(Math.hypot(mark(Infinity),fail(),mark(99)));}catch({message}){console.log(message);}finally{console.log('final');}console.log(Math.hypot(3,Math.hypot(0,4)));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "NaN", "Infinity", "3", "Infinity", "Infinity", "fail", "argument", "final", "5"
        ])
    );
}

#[test]
fn hypot_argument_host_effects_remain_visible_to_json_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for (expression, pure) in [
        ("Math.hypot(3,4)", true),
        ("Math.hypot(Infinity,read())", false),
        ("Math.hypot(read(),4)", false),
    ] {
        let bytes = compile(&format!(
            "function read():number{{varta.env.get('HYPOT_EFFECT');return 1;}}function fake(value:boolean):boolean{{const n={expression};return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn hypot_cached_values_and_earlier_pairs_survive_argument_gc_and_finalizers() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 4;}function keep(first:{text:string},pair:VartaResult<string>,n:number):string{churn();console.log(pair.status);console.log(n);return first.text;}function work():number{try{return Math.hypot(3,churn());}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('HYPOT_GC')),Math.hypot(3,churn())));console.log(work());}";
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
            "<capability denied: env> (HYPOT_GC)",
            "1",
            "5",
            "earlier!",
            "5"
        ])
    );
}

#[test]
fn hypot_near_overflow_matches_high_precision_decimal_goldens() {
    // Generated independently using 200- and 400-digit Decimal sqrt; both
    // precisions agree. Native libm can spuriously overflow for these pairs.
    let cases = [
        (
            0x7fe6a09e667f3bca_u64,
            0x7fe6a09e667f3bca_u64,
            0x7feffffffffffffc_u64,
        ),
        (
            0x7fe6a09e667f3bca_u64,
            0x7fe0f876ccdf6cd8_u64,
            0x7fec48c6001f0abd_u64,
        ),
        (
            0x7fe6a09e667f3bcb_u64,
            0x7fe6a09e667f3bcb_u64,
            0x7feffffffffffffe_u64,
        ),
        (
            0x7fe6a09e667f3bcb_u64,
            0x7fe0f876ccdf6cd8_u64,
            0x7fec48c6001f0abe_u64,
        ),
        (
            0x7fe6a09e667f3bcc_u64,
            0x7fe6a09e667f3bcc_u64,
            0x7fefffffffffffff_u64,
        ),
        (
            0x7fe6a09e667f3bcc_u64,
            0x7fe0f876ccdf6cd9_u64,
            0x7fec48c6001f0abf_u64,
        ),
        (
            0x7fe9999999999998_u64,
            0x7fe9999999999998_u64,
            0x7ff0000000000000_u64,
        ),
        (
            0x7fe9999999999998_u64,
            0x7fe3333333333332_u64,
            0x7feffffffffffffe_u64,
        ),
        (
            0x7fe9999999999999_u64,
            0x7fe9999999999999_u64,
            0x7ff0000000000000_u64,
        ),
        (
            0x7fe9999999999999_u64,
            0x7fe3333333333333_u64,
            0x7fefffffffffffff_u64,
        ),
        (
            0x7fe999999999999a_u64,
            0x7fe999999999999a_u64,
            0x7ff0000000000000_u64,
        ),
        (
            0x7fe999999999999a_u64,
            0x7fe3333333333334_u64,
            0x7ff0000000000000_u64,
        ),
    ];
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_hypot()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let f = instance
        .get_typed_func::<(f64, f64), f64>(&mut execution.store, "fixture.hypot")
        .unwrap();
    for (a, b, expected) in cases {
        let result = f
            .call(&mut execution.store, (f64::from_bits(a), f64::from_bits(b)))
            .unwrap();
        if f64::from_bits(expected).is_infinite() {
            assert!(result.is_infinite(), "{a:x}, {b:x}: {result}");
        } else {
            assert!(result.is_finite(), "{a:x}, {b:x}: {result}");
            assert!(
                result.to_bits().abs_diff(expected) <= 3,
                "{a:x}, {b:x}: {result}"
            );
        }
    }
}

#[test]
fn hypot_infinity_does_not_skip_fatal_fuel_in_later_argument() {
    let bytes = compile(
        "function forever():number{while(true){}return 1;}export function main():void{try{console.log(Math.hypot(Infinity,forever()));}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
