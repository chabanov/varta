use super::*;
use varta_common::language::LanguageProfile;
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
fn exported_power() -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function power(base:number,exponent:number):number{return base**exponent;}export function main():void{}",
    );
    let index = export_index(&bytes, "_start") - 1;
    let name = "fixture.power";
    rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127);
            data[0] += 1;
            data.push(name.len() as u8);
            data.extend_from_slice(name.as_bytes());
            data.push(0);
            data.push(index);
        }
        vec![data]
    })
}

#[test]
fn power_executes_plain_numbers_fractional_exponents_and_right_associativity() {
    let out = run(
        r"function square(value:number):number{return value**2;}export function main():void{console.log(2**3**2);console.log((-2)**3);console.log(-(2**2));console.log(2**-3);console.log(9**0.5);console.log(16**0.25);console.log(square(1.5));console.log(JSON.stringify([1,2,3].map(value=>value**2)));}",
    );
    assert_eq!(
        out.lines,
        lines(&["512", "-8", "-4", "0.125", "3", "2", "2.25", "[1,4,9]"])
    );
}

#[test]
fn power_special_cases_match_number_semantics_and_charge_one_host_operation() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_power()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let power = instance
        .get_typed_func::<(f64, f64), f64>(&mut execution.store, "fixture.power")
        .unwrap();
    let cases = [
        (f64::NAN, 0.0, 1.0),
        (f64::NAN, -0.0, 1.0),
        (f64::NAN, 1.0, f64::NAN),
        (1.0, f64::NAN, f64::NAN),
        (0.0, f64::NAN, f64::NAN),
        (1.0, f64::INFINITY, f64::NAN),
        (-1.0, f64::INFINITY, f64::NAN),
        (1.0, f64::NEG_INFINITY, f64::NAN),
        (-1.0, f64::NEG_INFINITY, f64::NAN),
        (f64::INFINITY, 0.0, 1.0),
        (f64::INFINITY, 2.0, f64::INFINITY),
        (f64::INFINITY, -2.0, 0.0),
        (f64::NEG_INFINITY, 3.0, f64::NEG_INFINITY),
        (f64::NEG_INFINITY, 2.0, f64::INFINITY),
        (f64::NEG_INFINITY, 0.5, f64::INFINITY),
        (f64::NEG_INFINITY, -3.0, -0.0),
        (f64::NEG_INFINITY, -2.0, 0.0),
        (0.0, 3.0, 0.0),
        (0.0, -3.0, f64::INFINITY),
        (-0.0, 3.0, -0.0),
        (-0.0, 2.0, 0.0),
        (-0.0, 0.5, 0.0),
        (-0.0, -3.0, f64::NEG_INFINITY),
        (-0.0, -2.0, f64::INFINITY),
        (-0.0, 0.0, 1.0),
        (-0.0, -0.0, 1.0),
        (2.0, f64::INFINITY, f64::INFINITY),
        (2.0, f64::NEG_INFINITY, 0.0),
        (0.5, f64::INFINITY, 0.0),
        (0.5, f64::NEG_INFINITY, f64::INFINITY),
        (-0.5, f64::INFINITY, 0.0),
        (-2.0, f64::NEG_INFINITY, 0.0),
        (-4.0, 0.5, f64::NAN),
        (-4.0, -0.5, f64::NAN),
        (-2.0, 3.0, -8.0),
        (-2.0, -3.0, -0.125),
        (-1.0, 9007199254740991.0, -1.0),
        (-1.0, 9007199254740992.0, 1.0),
        (-0.0, 9007199254740991.0, -0.0),
        (-0.0, 9007199254740992.0, 0.0),
        (2.0, -1074.0, f64::from_bits(1)),
        (2.0, -1075.0, 0.0),
        (-2.0, -1075.0, -0.0),
        (2.0, -1022.0, f64::MIN_POSITIVE),
        (2.0, 1024.0, f64::INFINITY),
        (-2.0, 1025.0, f64::NEG_INFINITY),
    ];
    for (index, (base, exponent, expected)) in cases.into_iter().enumerate() {
        let actual = power.call(&mut execution.store, (base, exponent)).unwrap();
        if expected.is_nan() {
            assert!(actual.is_nan(), "{base} ** {exponent}: {actual}");
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{base} ** {exponent}");
        }
        assert_eq!(execution.store.data().host_calls, index as u32 + 1);
    }
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
}

#[test]
fn power_normal_finite_accuracy_agrees_with_independent_square_root_cases() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_power()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let power = instance
        .get_typed_func::<(f64, f64), f64>(&mut execution.store, "fixture.power")
        .unwrap();
    for base in [0.25_f64, 2.0, 3.0, 10.0, 1e-300, 1e300] {
        for exponent in [0.5, -0.5] {
            let expected = if exponent > 0.0 {
                base.sqrt()
            } else {
                base.sqrt().recip()
            };
            let actual = power.call(&mut execution.store, (base, exponent)).unwrap();
            assert!(
                (actual - expected).abs() <= expected.abs() * 8.0 * f64::EPSILON,
                "{base} ** {exponent}: {actual} versus {expected}"
            );
        }
    }
}

#[test]
fn power_operands_and_compound_references_keep_order_and_old_values() {
    let out = run(
        r"function left():number{console.log('left');return 2;}function right():number{console.log('right');return 3;}function source(value:{n:number}):{n:number}{console.log('source');return value;}export function main():void{console.log(left()**right());let n=2;const change=():number=>{n=99;return 3;};console.log(n**=change());console.log(n);let value={n:1};const original=value;let name='n';const key=():string=>{console.log('key');original.n=3;value={n:100};return name;};const rhs=():number=>{console.log('rhs');original.n=50;value={n:200};name='other';return 2;};console.log(source(value)[key()]**=rhs());console.log(original.n);console.log(value.n);let current=[2];const old=current;let index=0;const replace=():number=>{old[0]=99;old.push(4);current=[100];index=1;return 3;};console.log(current[index]**=replace());console.log(JSON.stringify(old));console.log(JSON.stringify(current));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "left", "right", "8", "8", "8", "source", "key", "rhs", "9", "9", "200", "8", "[8,4]",
            "[100]"
        ])
    );
}

#[test]
fn power_compounds_validate_number_tags_after_rhs_and_preserve_original_failures() {
    let out = run(
        r"function apply(value:{n:number,s:string},key:string,item:number|string):number{return value[key]**=item;}function effect():number{console.log('rhs');return 2;}function fail():number{console.log('fail');throw RangeError('original');}export function main():void{const value={n:2,s:'2'};console.log(apply(value,'n',3));try{apply(value,'s',effect());}catch(error){console.log(error instanceof TypeError);}try{apply(value,'n','3');}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(value));const absent:{n?:number}={};const fill=():number=>{absent.n=5;console.log('fill');return 2;};try{absent.n**=fill();}catch({message}){console.log(message);}console.log(absent.n);absent.n=undefined;try{absent['n'+'']**=fail();}catch({message}){console.log(message);}try{value['to'+'String']**=effect();}catch(error){console.log(error instanceof TypeError);}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "8",
            "rhs",
            "1",
            "1",
            "{\"n\":8,\"s\":\"2\"}",
            "fill",
            "Invalid compound exponentiation operands",
            "5",
            "fail",
            "original",
            "1"
        ])
    );
}

#[test]
fn power_profile_inherits_sparse_values_json_closures_and_prior_number_operators() {
    let out = run(
        r"export function main():void{const a:number[]=[1,,3];delete a[0];a[5]=2;console.log(JSON.stringify(a));a.length=3;console.log(JSON.stringify(a));let count=0;const next=():number=>count++;console.log(next());console.log(next());console.log(-1>>>0);let flags=1;console.log(flags|=4);const parsed=varta.json.parse<{n?:number}>('{}');console.log(parsed.n??=2);console.log(parsed.n**=3);console.log(JSON.stringify(parsed));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[null,null,3,null,null,2]",
            "[null,null,3]",
            "0",
            "1",
            "4294967295",
            "5",
            "2",
            "8",
            "{\"n\":8}"
        ])
    );
}

#[test]
fn power_compound_array_storage_and_length_errors_follow_computed_results() {
    let out = run(
        r"export function main():void{const a=[2];const shrink=():number=>{a.pop();return 3;};console.log(a[0]**=shrink());console.log(JSON.stringify(a));const rhs=():number=>{console.log('rhs');return 2;};try{a[-1]**=rhs();}catch(error){console.log(error instanceof TypeError);}console.log(a.length);const b:number[]=[];console.log(b.length**=0);console.log(JSON.stringify(b));let returned=99;try{returned=(b.length**=NaN);}catch(error){console.log(error instanceof RangeError);}console.log(returned);console.log(b.length);b.length=0;try{b.length**=-1;}catch(error){console.log(error instanceof RangeError);}b.length=2;try{b.length**=0.5;}catch(error){console.log(error instanceof RangeError);}console.log(b.length**=10);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "8", "[8]", "rhs", "1", "1", "1", "[null]", "1", "99", "1", "1", "1", "1024"
        ])
    );
}

#[test]
fn power_nested_compounds_and_logical_skips_preserve_partial_stores() {
    let out = run(
        r"function nested():void{let local=2;const value={n:3};const a=[2];console.log(local**=value['n'+'']**=a[0]**=1);console.log(value.n);console.log(JSON.stringify(a));let same=2;console.log(same**=same**=2);}function partial():void{const value={n:0,s:'2'};const inner={n:2};try{value['s'+'']**=inner.n**=3;}catch(error){console.log(error instanceof TypeError);}console.log(inner.n);console.log(value.s);let keep=1;console.log(keep||=inner.n**=2);console.log(inner.n);}export function main():void{nested();partial();}",
    );
    assert_eq!(
        out.lines,
        lines(&["512", "9", "[2]", "16", "1", "8", "2", "1", "8"])
    );
}

#[test]
fn power_operand_exceptions_skip_later_calls_and_preserve_finalizers() {
    let out = run(
        r"function left():number{console.log('left');throw TypeError('left failure');}function right():number{console.log('right');throw SyntaxError('right failure');}function source():{n:number}{console.log('source');throw RangeError('source failure');}function key():string{console.log('key');throw RangeError('key failure');}function later():number{console.log('BAD_LATER');return 1;}function take(value:number,next:number):void{console.log('BAD_BODY');}export function main():void{const value={n:2};try{take(left()**right(),later());}catch({message}){console.log(message);}try{take(2**right(),later());}catch({message}){console.log(message);}try{take(source()[key()]**=right(),later());}catch({message}){console.log(message);}try{take(value[key()]**=right(),later());}catch({message}){console.log(message);}finally{console.log('final');}console.log(value.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "left",
            "left failure",
            "right",
            "right failure",
            "source",
            "source failure",
            "key",
            "key failure",
            "final",
            "2"
        ])
    );
}

#[test]
fn power_gc_preserves_cached_graphs_earlier_pairs_and_saved_returns() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 3;}function keep(first:{text:string},result:VartaResult<string>,value:number,last:number):string{churn();console.log(result.status);console.log(value);return first.text;}function work(value:{n?:number}):number{try{return value.n**=3;}finally{churn();}}export function main():void{let value={n:2};let key='n'+''.repeat(1);const rhs=():number=>{value={n:0};key='missing';churn();return 3;};console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_POWER_GC')),value[key]**=rhs(),churn()));console.log(work({n:-0}));const graph:{v:number|{text:string}}={v:{text:'detached'+'!'}};const replace=():number=>{graph.v=1;churn();return 2;};try{graph.v**=replace();}catch(error){console.log(error instanceof TypeError);}console.log(graph.v);const count:{n:number|undefined}={n:2};for(let i=0;i<256;i++){count.n**=1;}churn();console.log(count.n);}";
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
            "<capability denied: env> (VARTA_POWER_GC)",
            "1",
            "8",
            "earlier!",
            "-0",
            "1",
            "1",
            "2"
        ])
    );
}

#[test]
fn power_rejects_non_number_sources_unsupported_targets_and_all_old_profiles() {
    for source in [
        "export function main():void{console.log('2'**3);}",
        "export function main():void{console.log(2**false);}",
        "export function main():void{const n:number|undefined=2;console.log(n**3);}",
        "export function main():void{let n='2';n**=3;}",
        "export function main():void{let n=2;n**='3';}",
        "export function main():void{const n=2;n**=3;}",
        "export function main():void{let n:number|undefined=2;if(n!==undefined){n**=3;}}",
        "export function main():void{const a=[2];a['0']**=3;}",
        "export function main():void{const r={n:2};const key=0;r[key]**=3;}",
        "export function main():void{console.log(-2**2);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V15
            )
            .is_err(),
            "{source}"
        );
    }
    for profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .copied()
        .filter(|profile| !profile.number_power())
    {
        for source in [
            "export function main():void{console.log(2**3);}",
            "export function main():void{let n=2;n**=3;}",
        ] {
            assert!(
                varta_compiler::compile_typescript_with_profile(source, profile).is_err(),
                "{profile:?}"
            );
        }
    }
}

#[test]
fn power_profile_abi_downgrades_and_wrong_import_signatures_fail_before_execution() {
    use crate::contract_tests::rewrite;
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let bytes = compile("export function main():void{console.log(2**3);}");
    let runtime = VartaRuntime::new();
    for action in 0..4 {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                && action < 2
            {
                let at = 1 + abi::SECTION.len();
                data[at..].copy_from_slice(&abi::ARRAY_MUTATION_VERSION.to_le_bytes());
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
                        profile: LanguageProfile::CheckedF64Utf16V14,
                    }
                    .to_bytes()
                    .unwrap(),
                );
            }
            if id == 1 && action == 2 {
                let signature = [0x60, 2, 0x7c, 0x7c, 1, 0x7c];
                let at = data
                    .windows(signature.len())
                    .position(|s| s == signature)
                    .unwrap();
                data[at + 2] = 0x7f;
            }
            if id == 2 && action == 3 {
                let name = b"number_power";
                let at = data.windows(name.len()).position(|s| s == name).unwrap();
                data[at + 7] = b'P';
            }
            vec![data]
        });
        assert!(runtime.validate_module(&bad).is_err(), "action {action}");
    }
    let old = varta_compiler::compile_typescript_with_profile(
        "export function main():void{console.log(2+3);}",
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap();
    assert_eq!(
        runtime.run_wasm_bytes(&old, &[]).unwrap().lines,
        lines(&["5"])
    );
}

#[test]
fn power_math_import_is_not_allowed_in_host_free_json_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    for expression in ["2**3", "local**=3"] {
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{let local=2;const result=({expression});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
            "{error}"
        );
    }
}

#[test]
fn power_host_fuel_deadline_and_store_limits_remain_fatal_without_catch_finally() {
    for exhausted in [false, true] {
        let runtime = VartaRuntime::new();
        let module=runtime.validate_module(&compile("export function main():void{try{console.log(2**3);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}")).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
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
    for source in [
        "function rhs():number{while(true){}return 3;}export function main():void{const value:{n?:number}={};try{console.log(value.n**=rhs());}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
        "export function main():void{const a:number[]=[];a.length=1024;try{console.log(a.length**=2);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    ] {
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&compile(source)).unwrap();
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
