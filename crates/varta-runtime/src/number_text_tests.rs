use super::*;
use crate::contract_tests::{export_index, rewrite};
use crate::execution::Execution;
use varta_common::language::{LanguageContract, LanguageProfile, SCHEMA_VERSION};

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V18)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
fn fixtures() -> Vec<u8> {
    let bytes = compile(
        "function scalar(n:number):void{console.log(n);}function union(n:number):void{const v:number|undefined=n;console.log(v);}export function main():void{}",
    );
    let first = export_index(&bytes, "_start") - 2;
    rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 126 && first < 126);
            data[0] += 2;
            for (offset, name) in ["scalar", "union"].into_iter().enumerate() {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, first + offset as u8]);
            }
        }
        vec![data]
    })
}
fn canonical_properties(x: f64, text: &str) {
    // A negative fixed-point value just above 1e-6 can need 25 characters.
    assert!(text.len() <= 25, "{x}: {text}");
    let back = text.parse::<f64>().unwrap();
    if x.is_nan() {
        assert_eq!(text, "NaN");
        return;
    }
    if x == 0.0 {
        assert_eq!(text, "0");
        return;
    }
    assert_eq!(back.to_bits(), x.to_bits(), "{text}");
    if !x.is_finite() {
        return;
    }
    let text = text.strip_prefix('-').unwrap_or(text);
    assert_eq!(
        text.contains('e'),
        x.abs() < 1e-6 || x.abs() >= 1e21,
        "{x}: {text}"
    );
    let (mantissa, exponent) = if let Some((m, e)) = text.split_once('e') {
        assert!(e.starts_with('+') || e.starts_with('-'));
        assert!(!e[1..].starts_with('0'));
        (m, e.parse::<i32>().unwrap())
    } else {
        (text, 0)
    };
    let fractional = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = mantissa.replace('.', "");
    let digits = digits.trim_start_matches('0');
    let significant = digits.trim_end_matches('0');
    let power = exponent - fractional as i32 + (digits.len() - significant.len()) as i32;
    assert!(!significant.is_empty() && significant.len() <= 17);
    if significant.len() > 1 {
        let shorter = significant.parse::<u64>().unwrap() / 10;
        for candidate in [shorter, shorter + 1] {
            let value = format!("{candidate}e{}", power + 1).parse::<f64>().unwrap();
            assert_ne!(
                value.to_bits(),
                x.abs().to_bits(),
                "nonminimal {text}: shorter {candidate}e{}",
                power + 1
            );
        }
    }
}

#[test]
fn number_text_exact_decimal_scientific_boundaries_and_special_values() {
    let cases = [
        (0.0, "0"),
        (-0.0, "0"),
        (f64::NAN, "NaN"),
        (-f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (1e20, "100000000000000000000"),
        (1e21, "1e+21"),
        (f64::from_bits(1), "5e-324"),
        (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
        (f64::MAX, "1.7976931348623157e+308"),
        (9007199254740992.0, "9007199254740992"),
        (1000000000000000128.0, "1000000000000000100"),
        (
            f64::from_bits(1e-6_f64.to_bits() - 1),
            "9.999999999999997e-7",
        ),
        (
            f64::from_bits(1e21_f64.to_bits() - 1),
            "999999999999999900000",
        ),
    ];
    for (x, expected) in cases {
        assert_eq!(number::canonical_display(x), expected);
        canonical_properties(x, expected);
    }
}

#[test]
fn number_text_all_exponents_and_decimal_threshold_neighbors_round_trip_minimally() {
    for exponent in 0..=0x7ff_u64 {
        for fraction in [0, 1, 0x7_ffff_ffff_ffff, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | fraction);
            for x in [x, -x] {
                canonical_properties(x, &number::canonical_display(x));
            }
        }
    }
    for exponent in -323..=308 {
        let x = format!("1e{exponent}").parse::<f64>().unwrap();
        for offset in -2_i64..=2 {
            let value = f64::from_bits(x.to_bits().saturating_add_signed(offset));
            for value in [value, -value] {
                canonical_properties(value, &number::canonical_display(value));
            }
        }
    }
}

#[test]
fn number_text_scalar_and_union_imports_execute_all_exponents_identically() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&fixtures()).unwrap();
    let mut values = Vec::new();
    for exponent in 0..=0x7ff_u64 {
        for fraction in [0, 1, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | fraction);
            values.extend([x, -x]);
        }
    }
    for batch in values.chunks(32) {
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let a = instance
            .get_typed_func::<f64, ()>(&mut execution.store, "scalar")
            .unwrap();
        let b = instance
            .get_typed_func::<f64, ()>(&mut execution.store, "union")
            .unwrap();
        for &x in batch {
            a.call(&mut execution.store, x).unwrap();
            b.call(&mut execution.store, x).unwrap();
        }
        assert_eq!(execution.store.data().host_calls, batch.len() as u32 * 2);
        for (&x, pair) in batch
            .iter()
            .zip(execution.store.data().output.lines.as_chunks::<2>().0)
        {
            assert_eq!(pair[0], pair[1]);
            canonical_properties(x, pair[0].strip_prefix("varta log: ").unwrap());
        }
    }
}

#[test]
fn number_text_keeps_old_scalar_and_union_formats_and_current_json() {
    let runtime = VartaRuntime::new();
    for &profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .filter(|p| p.binary64() && !p.number_text())
    {
        let mut source =
            "export function main():void{console.log(-0);console.log(1e-7);console.log(1e21);"
                .to_owned();
        if profile.unions() {
            source.push_str("const a:number|undefined=-0;const b:number|undefined=1e21;console.log(a);console.log(b);");
        }
        source.push('}');
        let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
        let mut expected = lines(&["-0", "0.0000001", "1000000000000000000000"]);
        if profile.unions() {
            expected.extend(lines(&["-0", "1000000000000000000000"]));
        }
        assert_eq!(
            runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
            expected,
            "{profile:?}"
        );
    }
    let bytes = compile(
        "export function main():void{const n:number|string=1e21;console.log(n);console.log(-0);console.log(JSON.stringify([1e21,-0,NaN,Infinity]));console.log(Math.sin(-0));const x=Math.random();console.log(Number.isFinite(x));}",
    );
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        lines(&["1e+21", "0", "[1e+21,0,null,null]", "0", "1"])
    );
}

#[test]
fn number_text_union_non_numbers_errors_and_owned_values_retain_their_contracts() {
    let bytes = compile(
        "function show(v:number|string|boolean|null|undefined|{n:number}|number[]):void{console.log(v);}function showError(v:number|Error):void{console.log(v);}export function main():void{show('text');show(true);show(null);show(undefined);show({n:1e21});show([1e-7,-0]);showError(RangeError('edge'));show(NaN);show(-Infinity);show(-0);}",
    );
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&[
            "text",
            "1",
            "null",
            "undefined",
            "{\"n\":1e+21}",
            "[1e-7,0]",
            "RangeError: edge",
            "NaN",
            "-Infinity",
            "0"
        ])
    );
}

#[test]
fn number_text_budgets_and_argument_exceptions_preserve_fatal_and_pending_rules() {
    let runtime = VartaRuntime::new();
    for value in ["1e21", "({n:1e21}).n", "(true?1e21:undefined)"] {
        let module=runtime.validate_module(&compile(&format!("export function main():void{{try{{console.log({value});}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"))).unwrap();
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
    let bytes = compile(
        "function fail():number{throw RangeError('argument');}export function main():void{try{console.log(fail());}catch({message}){console.log(message);}finally{console.log(1e21);}}",
    );
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        lines(&["argument", "1e+21"])
    );
}

#[test]
fn number_text_mismatched_profiles_signatures_old_names_and_helper_imports_fail_closed() {
    for (statement, host, signature) in [
        (
            "console.log(1e21);",
            varta_common::HostOperation::LogFloatV2,
            vec![0x60, 1, 0x7c, 0],
        ),
        (
            "const n:number|undefined=1e21;console.log(n);",
            varta_common::HostOperation::ValueLogV7,
            vec![0x60, 1, 0x7e, 0],
        ),
    ] {
        let runtime = VartaRuntime::new();
        let bytes = compile(&format!("export function main():void{{{statement}}}"));
        for action in 0..6 {
            let bad = rewrite(&bytes, |id, data| {
                let mut data = data.to_vec();
                if id == 0
                    && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                    && action < 2
                {
                    data[1 + abi::SECTION.len()..]
                        .copy_from_slice(&abi::RANDOM_VERSION.to_le_bytes());
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
                            profile: LanguageProfile::CheckedF64Utf16V17,
                        }
                        .to_bytes()
                        .unwrap(),
                    );
                }
                if id == 1 && action == 2 {
                    let at = data
                        .windows(signature.len())
                        .position(|s| s == signature)
                        .unwrap();
                    data[at + 2] = 0x7f;
                }
                if id == 2 && action == 3 {
                    let name = host.name().as_bytes();
                    let at = data.windows(name.len()).position(|s| s == name).unwrap();
                    data[at] = b'L';
                }
                if id == 2 && action == 4 {
                    let name = host.name().as_bytes();
                    let at = data.windows(name.len()).position(|s| s == name).unwrap();
                    data[at + name.len() - 1] = b'1';
                }
                if id == 2 && action == 5 {
                    let name = host.name().as_bytes();
                    let at = data.windows(name.len()).position(|s| s == name).unwrap();
                    let old_name = if host == varta_common::HostOperation::LogFloatV2 {
                        b"log_f64".as_slice()
                    } else {
                        b"value_log_v6".as_slice()
                    };
                    assert_eq!(usize::from(data[at - 1]), name.len());
                    data[at - 1] = old_name.len() as u8;
                    data.splice(at..at + name.len(), old_name.iter().copied());
                }
                vec![data]
            });
            assert!(
                runtime.validate_module(&bad).is_err(),
                "{host:?} action {action}"
            );
        }
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{{statement}return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn number_text_roots_pairs_buffer_status_and_finalizers_survive_collection() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1e21;}function keep(first:{text:string},pair:VartaResult<string>,n:number):string{console.log(n);churn();console.log(pair.status);return first.text;}function saved():number{try{return 1e21;}finally{console.log(churn());}}export function main():void{console.log(keep({text:'root'+'!'},varta.result.capture(varta.env.get('NUMBER_TEXT_DENIED')),churn()));console.log(saved());console.log(varta.result.status());}";
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
            "<capability denied: env> (NUMBER_TEXT_DENIED)",
            "1e+21",
            "1",
            "root!",
            "1e+21",
            "1e+21",
            "1"
        ])
    );
}
