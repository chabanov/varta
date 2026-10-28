use super::*;
use varta_common::language::LanguageProfile;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V15)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
// Independent oracle: round discarded binary64 significand bits as integers.
pub(super) fn reference(x: f64) -> f64 {
    let bits = x.to_bits();
    let encoded = (bits >> 52) & 0x7ff;
    if encoded == 0x7ff {
        return x;
    }
    let exponent = encoded as i32 - 1023;
    if exponent > 15 {
        return f64::INFINITY.copysign(x);
    }
    if exponent < -25 {
        return 0.0_f64.copysign(x);
    }
    let significand = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
    let discarded = if exponent < -14 {
        (28 - exponent) as u32
    } else {
        42
    };
    let mut integer = significand >> discarded;
    let remainder = significand & ((1_u64 << discarded) - 1);
    let halfway = 1_u64 << (discarded - 1);
    if remainder > halfway || (remainder == halfway && integer & 1 != 0) {
        integer += 1;
    }
    let value = if exponent < -14 {
        integer as f64 * 2.0_f64.powi(-24)
    } else {
        integer as f64 * 2.0_f64.powi(exponent - 10)
    };
    if value >= 65536.0 {
        f64::INFINITY.copysign(x)
    } else {
        value.copysign(x)
    }
}
fn decode_half(bits: u16) -> f64 {
    let exponent = (bits >> 10) & 0x1f;
    let fraction = bits & 0x3ff;
    let magnitude = match exponent {
        0 => f64::from(fraction) * 2.0_f64.powi(-24),
        31 => {
            if fraction == 0 {
                f64::INFINITY
            } else {
                f64::NAN
            }
        }
        _ => f64::from(1024 + fraction) * 2.0_f64.powi(i32::from(exponent) - 25),
    };
    if bits & 0x8000 != 0 {
        -magnitude
    } else {
        magnitude
    }
}
fn exported_f16() -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function half(value:number):number{return Math.f16round(value);}export function main():void{}",
    );
    let index = export_index(&bytes, "_start") - 1;
    let name = "fixture.f16";
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
fn check_values(values: &[f64]) {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_f16()).unwrap();
    assert_eq!(module.imports().len(), 0);
    for batch in values.chunks(128) {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let f = instance
            .get_typed_func::<f64, f64>(&mut execution.store, "fixture.f16")
            .unwrap();
        execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        for &x in batch {
            let expected = reference(x);
            let actual = f.call(&mut execution.store, x).unwrap();
            if expected.is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits(), "f16round({x:?})");
            }
        }
        assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    }
}

#[test]
fn f16_every_binary16_value_and_every_finite_midpoint_neighbor_matches_oracle() {
    let mut values = (0..=u16::MAX).map(decode_half).collect::<Vec<_>>();
    for bits in 0..0x7bff {
        let low = decode_half(bits);
        let high = decode_half(bits + 1);
        let midpoint = (low + high) / 2.0;
        for x in [midpoint.next_down(), midpoint, midpoint.next_up()] {
            values.extend([x, -x]);
        }
    }
    for x in [65520.0_f64.next_down(), 65520.0, 65520.0_f64.next_up()] {
        values.extend([x, -x]);
    }
    check_values(&values);
}

#[test]
fn f16_all_binary64_exponents_and_significands_match_integer_rounding() {
    let mut values = Vec::new();
    for exponent in 0..=0x7ff_u64 {
        for fraction in [0, 1, 0x7_ffff_ffff_ffff, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | fraction);
            values.extend([x, -x]);
        }
    }
    check_values(&values);
}

#[test]
fn f16_double_rounding_zero_overflow_and_nested_calls_follow_source_semantics() {
    let bytes = compile(
        "function next():number{console.log('once');return 1.0004882812500002;}function fail():number{throw RangeError('argument');}export function main():void{console.log(Math.f16round(next()));console.log(Math.f16round(Math.fround(1.0004882812500002)));console.log(1/Math.f16round(-0.00000001));console.log(Math.f16round(65519));console.log(Math.f16round(65520));console.log(Math.f16round(-65520));console.log(Math.f16round(Math.f16round(2.1)));console.log(Number.isNaN(Math.f16round(NaN)));try{console.log(Math.f16round(fail()));}catch({message}){console.log(message);}finally{console.log('final');}}",
    );
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&[
            "once",
            "1.0009765625",
            "1",
            "-Infinity",
            "65504",
            "Infinity",
            "-Infinity",
            "2.099609375",
            "1",
            "argument",
            "final"
        ])
    );
}

#[test]
fn f16_arguments_remain_visible_to_host_free_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for (argument, pure) in [("1.5", true), ("read()", false)] {
        let bytes = compile(&format!(
            "function read():number{{varta.env.get('F16_EFFECT');return 1;}}function fake(value:boolean):boolean{{const n=Math.f16round({argument});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
