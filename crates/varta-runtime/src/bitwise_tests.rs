use super::*;
use varta_common::language::LanguageProfile;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V14)
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

#[test]
fn bitwise_operators_execute_all_number_operations_in_admitted_profiles() {
    let source = r"function mask(value:number):number{return (value&15)^2;}export function main():void{console.log(5&3);console.log(5|2);console.log(5^3);console.log(~5);console.log(1<<31);console.log(-8>>2);console.log(-1>>>0);console.log(mask(29));console.log(1+2<<2);console.log(8>>1+1);console.log(1|2^3&1);console.log(~~5.9);}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&[
                "1",
                "7",
                "6",
                "-6",
                "-2147483648",
                "-2",
                "4294967295",
                "15",
                "12",
                "2",
                "3",
                "5"
            ]),
            "{profile:?}"
        );
    }
}

#[test]
fn bitwise_conversions_wrap_and_truncate_without_traps_or_negative_zero() {
    let out = run(
        r"function show(value:number):void{console.log(value|0);console.log(value>>>0);console.log(~value);}export function main():void{show(-1.9);show(4294967297.9);show(-4294967297.9);show(2147483648);show(9007199254740991);show(9007199254740992);show(19342813113834066795298816);show(1.7976931348623157e308);show(NaN);show(Infinity);show(-Infinity);show(5e-324);console.log(1/(-0|0));console.log(1/(-0>>>0));}",
    );
    let mut expected = vec![
        "-1",
        "4294967295",
        "0",
        "1",
        "1",
        "-2",
        "-1",
        "4294967295",
        "0",
        "-2147483648",
        "2147483648",
        "2147483647",
        "-1",
        "4294967295",
        "0",
    ];
    for _ in 0..7 {
        expected.extend(["0", "0", "-1"]);
    }
    expected.extend(["Infinity", "Infinity"]);
    assert_eq!(out.lines, lines(&expected));
}

#[test]
fn bitwise_shift_counts_use_uint32_then_the_low_five_bits() {
    let out = run(
        r"export function main():void{for(const count of [-1,-32,-33,0,1.9,31,32,33,4294967297,NaN,Infinity]){if(count!==undefined){console.log(1<<count);console.log(-2147483648>>count);console.log(-1>>>count);}}}",
    );
    let counts = [31, 0, 31, 0, 1, 31, 0, 1, 1, 0, 0];
    let mut expected = Vec::new();
    for count in counts {
        expected.extend([
            (1_i32.wrapping_shl(count)).to_string(),
            (i32::MIN.wrapping_shr(count)).to_string(),
            (u32::MAX.wrapping_shr(count)).to_string(),
        ]);
    }
    assert_eq!(
        out.lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn bitwise_every_binary64_exponent_and_seeded_payloads_match_an_independent_modulo_oracle() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function and(x:number,y:number):number{return x&y;}function or(x:number,y:number):number{return x|y;}function xor(x:number,y:number):number{return x^y;}function left(x:number,y:number):number{return x<<y;}function signed(x:number,y:number):number{return x>>y;}function unsigned(x:number,y:number):number{return x>>>y;}function not(x:number):number{return ~x;}export function main():void{}",
    );
    let names = [
        "fixture.and",
        "fixture.or",
        "fixture.xor",
        "fixture.left",
        "fixture.signed",
        "fixture.unsigned",
        "fixture.not",
    ];
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 120);
            data[0] += names.len() as u8;
            for (offset, name) in names.iter().enumerate() {
                data.push(name.len() as u8);
                data.extend_from_slice(name.as_bytes());
                data.push(0);
                data.push(main - names.len() as u8 + offset as u8);
            }
        }
        vec![data]
    });
    // The oracle uses floating truncation/remainder, independently of the
    // emitted exponent/significand extraction. Cast only the bounded result.
    fn uint32(value: f64) -> u32 {
        if !value.is_finite() || value == 0.0 {
            return 0;
        }
        let reduced = value.trunc() % 4294967296.0;
        (if reduced < 0.0 {
            reduced + 4294967296.0
        } else {
            reduced
        }) as u32
    }
    let mut inputs = Vec::new();
    for exponent in 0..=0x7ff_u64 {
        for fraction in [0, 1, (1_u64 << 51) + 1, (1_u64 << 52) - 1] {
            for sign in [0, 1_u64 << 63] {
                inputs.push(f64::from_bits(sign | (exponent << 52) | fraction));
            }
        }
    }
    let mut state = 0x5b86_8520_2026_1027_u64;
    for _ in 0..1024 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        inputs.push(f64::from_bits(state));
    }
    let counts = [
        -1.0,
        0.0,
        1.9,
        31.0,
        32.0,
        33.0,
        4294967297.0,
        f64::NAN,
        f64::INFINITY,
    ];
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    // Fresh bounded stores keep the normal execution fuel ceiling for every
    // batch rather than enlarging production limits for this exhaustive test.
    for chunk in inputs.chunks(128) {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let functions = names[..6]
            .iter()
            .map(|name| {
                instance
                    .get_typed_func::<(f64, f64), f64>(&mut execution.store, name)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let not = instance
            .get_typed_func::<f64, f64>(&mut execution.store, names[6])
            .unwrap();
        for (index, &x) in chunk.iter().enumerate() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let y = if index % 2 == 0 {
                f64::from_bits(state)
            } else {
                counts[index % counts.len()]
            };
            let a = uint32(x);
            let b = uint32(y);
            let shift = b & 31;
            let expected = [
                f64::from((a & b) as i32),
                f64::from((a | b) as i32),
                f64::from((a ^ b) as i32),
                f64::from(a.wrapping_shl(shift) as i32),
                f64::from((a as i32).wrapping_shr(shift)),
                f64::from(a.wrapping_shr(shift)),
            ];
            for (slot, function) in functions.iter().enumerate() {
                let actual = function.call(&mut execution.store, (x, y)).unwrap();
                assert_eq!(
                    actual.to_bits(),
                    expected[slot].to_bits(),
                    "{} x={:016x} y={:016x}",
                    names[slot],
                    x.to_bits(),
                    y.to_bits()
                );
            }
            let actual = not.call(&mut execution.store, x).unwrap();
            assert_eq!(
                actual.to_bits(),
                f64::from((!a) as i32).to_bits(),
                "~ x={:016x}",
                x.to_bits()
            );
        }
        crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    }
}

#[test]
fn bitwise_operands_evaluate_once_in_source_order_before_conversion() {
    let out = run(
        r"function left():number{console.log('left');return 4294967297;}function right():number{console.log('right');return 2;}export function main():void{console.log(left()|right());const value={n:5};const change=():number=>{value.n=99;console.log('change');return 2;};console.log(value.n|change());console.log(value.n);let n=1;console.log(n++|n++);console.log(n);console.log((left()^(right()<<1))>>>right());}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "left", "right", "3", "change", "7", "99", "3", "3", "left", "right", "right", "1"
        ])
    );
}

#[test]
fn bitwise_narrowed_unions_optional_reads_and_callback_results_keep_number_types() {
    let out = run(
        r"function mask(value:number|string|undefined):number{if(typeof value==='number'){return value&255;}return 0;}export function main():void{console.log(mask(511));console.log(mask('511'));const inputs:(number|undefined)[]=[1,undefined,3];console.log(JSON.stringify(inputs.map(value=>{if(value!==undefined){return value<<1;}return 0;})));const record:{n?:number}={n:9};const read=record['n'+''];if(read!==undefined){console.log(~read);}console.log((1<<31)===-2147483648);console.log((-1>>>0)>2147483647);}",
    );
    assert_eq!(out.lines, lines(&["255", "0", "[2,0,6]", "-10", "1", "1"]));
}

#[test]
fn bitwise_operand_throws_preserve_original_exceptions_and_skip_later_effects() {
    let out = run(
        r"function left():number{console.log('left');throw TypeError('left failure');}function right():number{console.log('right');throw RangeError('right failure');}function later():number{console.log('BAD_LATER');return 1;}function take(value:number,next:number):void{console.log('BAD_BODY');}export function main():void{try{take(left()|right(),later());}catch({message}){console.log(message);}finally{console.log('left final');}try{take(1<<right(),later());}catch({message}){console.log(message);}finally{console.log('right final');}try{console.log(~left());}catch({message}){console.log(message);}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "left",
            "left failure",
            "left final",
            "right",
            "right failure",
            "right final",
            "left",
            "left failure"
        ])
    );
}

#[test]
fn bitwise_gc_and_nested_value_blocks_preserve_earlier_graphs_pairs_and_numeric_operands() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 2;}function keep(first:{text:string},result:VartaResult<string>,value:number,last:number):string{churn();console.log(result.status);console.log(value);return first.text;}function work():number{try{return 4294967297|churn();}finally{churn();}}export function main():void{const record={n:0};console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_BITWISE_GC')),4294967297|(record.n||=churn()),churn()));console.log(work());console.log((4294967297|churn())^(churn()<<31));}";
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
            "<capability denied: env> (VARTA_BITWISE_GC)",
            "1",
            "3",
            "earlier!",
            "3",
            "3"
        ])
    );
}

#[test]
fn bitwise_helper_purity_controls_pass_and_both_operand_effects_remain_visible() {
    use crate::contract_tests::{export_index, rewrite};
    for (left, right, expression, has_host) in [
        ("return 4294967297;", "return 1;", "left()>>>right()", false),
        ("return 1;", "return 1;", "~left()", false),
        (
            "console.log('left');return 1;",
            "return 1;",
            "left()&right()",
            true,
        ),
        (
            "return 1;",
            "console.log('right');return 1;",
            "left()<<right()",
            true,
        ),
        (
            "console.log('left');return 1;",
            "return 1;",
            "~left()",
            true,
        ),
    ] {
        let bytes = compile(&format!(
            "function left():number{{{left}}}function right():number{{{right}}}function fake(value:boolean):boolean{{const result=({expression});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
        ));
        let fake = export_index(&bytes, "_start") - 1;
        let name = varta_common::value::VALUE_ALLOCATE;
        let rewritten = rewrite(&bytes, |id, data| {
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
        let result = VartaRuntime::new().validate_module(&rewritten);
        if has_host {
            let error = result.unwrap_err();
            assert!(
                error.to_string().contains("cannot call host imports"),
                "{error}"
            );
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn bitwise_rejects_non_number_unnarrowed_union_and_legacy_sources() {
    for source in [
        "export function main():void{console.log('1'|0);}",
        "export function main():void{console.log(1&false);}",
        "export function main():void{console.log(~undefined);}",
        "export function main():void{console.log(null>>>0);}",
        "export function main():void{const value:number|undefined=1;console.log(value<<1);}",
        "export function main():void{const value={n:1};console.log(value|0);}",
        "export function main():void{const value=[1];console.log(~value[0]);}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{console.log(1|2);}",
                profile
            )
            .is_err()
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{console.log(~1);}",
                profile
            )
            .is_err()
        );
    }
}

#[test]
fn bitwise_operand_exhaustion_stays_fatal_without_catch_or_finally() {
    for expression in ["forever()|rhs()", "1<<forever()", "~forever()"] {
        let bytes = compile(&format!(
            "function forever():number{{while(true){{}}return 1;}}function rhs():number{{console.log('BAD_RHS');return 1;}}export function main():void{{try{{console.log({expression});}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
        ));
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
}

#[test]
fn bitwise_adds_no_import_or_helper_export_and_keeps_profile_contracts() {
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let runtime = VartaRuntime::new();
        let baseline = runtime
            .validate_module(
                &varta_compiler::compile_typescript_with_profile(
                    "export function main():void{}",
                    profile,
                )
                .unwrap(),
            )
            .unwrap();
        let module=runtime.validate_module(&varta_compiler::compile_typescript_with_profile("function mix(a:number,b:number):number{return ~(a&b)|(a<<b)^(a>>>b);}export function main():void{mix(1,2);}",profile).unwrap()).unwrap();
        assert_eq!(module.imports().count(), 0);
        assert_eq!(
            module
                .exports()
                .map(|item| item.name().to_owned())
                .collect::<Vec<_>>(),
            baseline
                .exports()
                .map(|item| item.name().to_owned())
                .collect::<Vec<_>>()
        );
    }
}
