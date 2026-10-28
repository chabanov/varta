use super::*;
use varta_common::language::LanguageProfile;

const METHODS: [&str; 4] = ["isFinite", "isNaN", "isInteger", "isSafeInteger"];
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
fn exported_predicates() -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let mut source = String::new();
    for method in METHODS {
        source.push_str(&format!(
            "function fixture_{method}(value:number):boolean{{return Number.{method}(value);}}"
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

#[test]
fn number_predicates_match_all_exponents_and_exact_safe_integer_boundaries() {
    let mut values = vec![
        0.0,
        -0.0,
        9007199254740990.0,
        9007199254740991.0,
        9007199254740992.0,
        -9007199254740991.0,
        -9007199254740992.0,
        1.5,
        -1.5,
    ];
    for exponent in 0..=0x7ff_u64 {
        for mantissa in [0, 1, 0x7_ffff_ffff_ffff, 0xf_ffff_ffff_ffff] {
            let x = f64::from_bits((exponent << 52) | mantissa);
            values.extend([x, -x]);
        }
    }
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_predicates()).unwrap();
    for batch in values.chunks(128) {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        for method in METHODS {
            let f = instance
                .get_typed_func::<f64, i32>(&mut execution.store, method)
                .unwrap();
            for &x in batch {
                let expected = match method {
                    "isFinite" => x.is_finite(),
                    "isNaN" => x.is_nan(),
                    "isInteger" => x.is_finite() && x.fract() == 0.0,
                    "isSafeInteger" => {
                        x.is_finite() && x.fract() == 0.0 && x.abs() <= 9007199254740991.0
                    }
                    _ => unreachable!(),
                };
                assert_eq!(
                    f.call(&mut execution.store, x).unwrap(),
                    i32::from(expected),
                    "{method}({x:?})"
                );
            }
        }
        assert_eq!(execution.store.data().host_calls, 0);
    }
}

#[test]
fn number_predicates_test_union_tags_without_narrowing_or_coercion() {
    let out = run(
        "function show(value:number|string|undefined|{n:number}|number[]):void{console.log(Number.isFinite(value));console.log(Number.isNaN(value));console.log(Number.isInteger(value));console.log(Number.isSafeInteger(value));}export function main():void{show(2);show(NaN);show(Infinity);show('2');show(undefined);show({n:2});show([2]);console.log(Number.isFinite(null));console.log(Number.isInteger(true));console.log(Number.isNaN(TypeError('x')));console.log(Number.isFinite(()=>2));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1", "0", "1", "1", "0", "1", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0",
            "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0"
        ])
    );
}

#[test]
fn number_predicates_missing_and_ignored_arguments_preserve_effects_and_first_value() {
    let out = run(
        "function mark(value:number):number{console.log(value);return value;}function fail():number{console.log('fail');throw RangeError('ignored');}export function main():void{console.log(Number.isFinite());console.log(Number.isNaN());console.log(Number.isInteger());console.log(Number.isSafeInteger());let n=2;const change=():number=>{n=NaN;return 3;};console.log(Number.isInteger(n,change(),mark(4)));console.log(Number.isNaN(n));console.log(Number.isFinite('2',mark(5)));try{console.log(Number.isFinite(2,fail(),mark(99)));}catch({message}){console.log(message);}finally{console.log('final');}console.log(Number.isFinite(2,varta.result.capture(varta.env.get('NUMBER_PREDICATE_DENIED'))));console.log(varta.result.status());console.log(Number.isFinite(varta.result.capture(varta.env.get('NUMBER_PREDICATE_DENIED'))));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "0",
            "0",
            "0",
            "0",
            "4",
            "1",
            "1",
            "5",
            "0",
            "fail",
            "ignored",
            "final",
            "<capability denied: env> (NUMBER_PREDICATE_DENIED)",
            "1",
            "1",
            "<capability denied: env> (NUMBER_PREDICATE_DENIED)",
            "0"
        ])
    );
}

#[test]
fn number_predicates_reject_shadowing_borrowing_spread_and_old_profiles() {
    for source in [
        "export function main():void{const Number={isFinite:1};Number.isFinite(1);}",
        "export function main():void{Number.isFinite(1);let Number={isFinite:1};}",
        "function f(Number:{isFinite:number}):boolean{return Number.isFinite(1);}export function main():void{}",
        "export function main():void{const Number={isFinite:1};const f=():boolean=>Number.isFinite(1);f();}",
        "function Number():number{return 1;}export function main():void{Number.isFinite(1);}",
        "export function main():void{Number['isFinite'](1);}",
        "export function main():void{Number.isFinite(...[1]);}",
        "export function main():void{Number.isFinite<number>(1);}",
        "export function main():void{const f=Number.isFinite;f(1);}",
        "export function main():void{Number.parseFloat('2');}",
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
        .filter(|p| !p.number_predicates())
    {
        for method in METHODS {
            let source = format!("export function main():void{{Number.{method}(1);}}");
            assert!(
                varta_compiler::compile_typescript_with_profile(&source, profile).is_err(),
                "{profile:?}: {method}"
            );
        }
    }
}

#[test]
fn number_predicates_argument_effects_remain_visible_to_json_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for (expression, pure) in [
        ("Number.isFinite(1)", true),
        ("Number.isNaN(1,read())", false),
        ("Number.isInteger(read())", false),
        ("Number.isSafeInteger(1,read())", false),
    ] {
        let bytes = compile(&format!(
            "function read():number{{varta.env.get('NUMBER_EFFECT');return 1;}}function fake(value:boolean):boolean{{return Number.isFinite({expression});}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn number_predicates_preserve_pairs_roots_and_results_through_ignored_argument_gc() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function keep(first:{text:string},pair:VartaResult<string>,value:boolean):string{churn();console.log(pair.status);console.log(value);return first.text;}function work(value:number|string):boolean{try{return Number.isFinite(value,Math.min(NaN,churn()),varta.result.capture(varta.env.get('NUMBER_GC')));}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('NUMBER_GC')),work(2)));console.log(work('2'));}";
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
            "<capability denied: env> (NUMBER_GC)",
            "<capability denied: env> (NUMBER_GC)",
            "1",
            "1",
            "earlier!",
            "<capability denied: env> (NUMBER_GC)",
            "0"
        ])
    );
}

#[test]
fn number_predicates_do_not_consume_host_allowance() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_predicates()).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
    for method in METHODS {
        let f = instance
            .get_typed_func::<f64, i32>(&mut execution.store, method)
            .unwrap();
        assert_eq!(
            f.call(&mut execution.store, 2.0).unwrap(),
            i32::from(method != "isNaN")
        );
    }
    assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
}

#[test]
fn number_predicates_fatal_ignored_argument_fuel_bypasses_catch_and_finally() {
    let bytes = compile(
        "function forever():number{while(true){}return 1;}export function main():void{try{console.log(Number.isFinite('2',forever()));}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
