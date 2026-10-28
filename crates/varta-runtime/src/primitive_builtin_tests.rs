use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V23)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn primitive_builtins_boolean_handles_primitives_unions_and_void_effects() {
    let source = r"function show(x:number|string|boolean|null|undefined):void{console.log(Boolean(x));}export function main():void{console.log(Boolean());show(0);show(-0);show(NaN);show(Infinity);show(-Infinity);show(0.0001);show('');show('0');show('false');show('\uD800');show('\u0000');show(false);show(true);show(null);show(undefined);console.log(Boolean(console.log('side')));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "0", "0", "0", "0", "1", "1", "1", "0", "1", "1", "1", "1", "0", "1", "0", "0", "side",
            "0"
        ])
    );
}
#[test]
fn primitive_builtins_boolean_treats_all_ordinary_references_as_true() {
    let source = "function show(x:number|string|null|undefined|{n:number}|number[]|Error):void{console.log(Boolean(x));}export function main():void{console.log(Boolean({n:0}));console.log(Boolean([0].slice(0,0)));console.log(Boolean(Error('')));console.log(Boolean(RangeError('')));console.log(Boolean(():number=>0));show({n:0});show([]);show(Error(''));show(null);show(undefined);show('');show('x');show(0);show(NaN);show(1);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "1", "1", "1", "1", "1", "1", "1", "1", "0", "0", "0", "1", "0", "0", "1"
        ])
    );
}
#[test]
fn primitive_builtins_global_numeric_predicates_coerce_and_number_predicates_do_not() {
    let source = "function show(x:number|string|boolean|null|undefined):void{console.log(JSON.stringify([isFinite(x),isNaN(x),Number.isFinite(x),Number.isNaN(x)]));}export function main():void{console.log(isFinite());console.log(isNaN());show(0);show(-0);show(NaN);show(Infinity);show('');show(' 12 ');show('12tail');show('0x10');show('Infinity');show(true);show(false);show(null);show(undefined);console.log(isNaN(console.log('side')));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "0",
            "1",
            "[true,false,true,false]",
            "[true,false,true,false]",
            "[false,true,false,true]",
            "[false,false,false,false]",
            "[true,false,false,false]",
            "[true,false,false,false]",
            "[false,true,false,false]",
            "[true,false,false,false]",
            "[false,false,false,false]",
            "[true,false,false,false]",
            "[true,false,false,false]",
            "[true,false,false,false]",
            "[false,true,false,false]",
            "side",
            "1"
        ])
    );
}
#[test]
fn primitive_builtins_cache_first_value_before_ignored_effects_and_exceptions() {
    let source = "function first():string{console.log('first');return '12';}function extra():string{console.log('extra');return 'ignored';}function fail():string{throw Error('stop');}export function main():void{let text:string|number='';console.log(Boolean(text,text='x',extra(),{n:1},[1]));console.log(text);console.log(isFinite(first(),extra()));console.log(isNaN('bad',extra()));let value:string|number='12';console.log(isFinite(value,value='bad'));try{Boolean(fail(),extra());}catch({message}){console.log(message);}try{isFinite(first(),fail(),extra());}catch({message}){console.log(message);}}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "extra", "0", "x", "first", "extra", "1", "extra", "1", "1", "stop", "first", "stop"
        ])
    );
}
#[test]
fn primitive_builtins_ordinary_bindings_closures_and_tdz_keep_precedence() {
    let source = "function Boolean(n:number):number{return n+1;}function isNaN(n:number):number{return n+2;}export function main():void{const isFinite=(n:number):number=>n+3;console.log(Boolean(3));console.log(isNaN(3));console.log(isFinite(3));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["4", "5", "6"])
    );
    for name in ["Boolean", "isFinite", "isNaN"] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!(
                    "export function main():void{{{name}(1);const {name}=(x:number):number=>x;}}"
                ),
                LanguageProfile::CheckedF64Utf16V23
            )
            .is_err()
        );
    }
    let source = "function factory(value:string):()=>boolean{return ():boolean=>isFinite(value);}export function main():void{const test=factory('12');console.log(test());const b=():boolean=>Boolean('');console.log(b());}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["1", "0"])
    );
}
#[test]
fn primitive_builtins_numeric_paths_cover_every_exponent_without_host_calls() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let runtime = VartaRuntime::new();
    for mode in ["Boolean", "isFinite", "isNaN"] {
        let bytes = compile(&format!(
            "function fixture(n:number):boolean{{return {mode}(n);}}export function main():void{{}}"
        ));
        let index = export_index(&bytes, "_start") - 1;
        let bytes = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                assert!(data[0] < 127 && index < 128);
                data[0] += 1;
                data.push(4);
                data.extend(b"test");
                data.extend([0, index]);
            }
            vec![data]
        });
        assert!(runtime.inspect_module(&bytes).unwrap().imports.is_empty());
        let module = runtime.validate_module(&bytes).unwrap();
        for batch in 0..16_u64 {
            let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
            let instance = execution.instantiate(&module).unwrap();
            let function = instance
                .get_typed_func::<f64, i32>(&mut execution.store, "test")
                .unwrap();
            for exponent in batch * 128..(batch + 1) * 128 {
                for fraction in [0, 1, 0xf_ffff_ffff_ffff] {
                    let x = f64::from_bits((exponent << 52) | fraction);
                    for x in [x, -x] {
                        let want = match mode {
                            "Boolean" => x != 0. && !x.is_nan(),
                            "isFinite" => x.is_finite(),
                            _ => x.is_nan(),
                        };
                        assert_eq!(
                            function.call(&mut execution.store, x).unwrap(),
                            i32::from(want),
                            "{mode}/{x:?}"
                        );
                    }
                }
            }
            assert_eq!(execution.store.data().host_calls, 0);
        }
    }
}
#[test]
fn primitive_builtins_boolean_strings_and_reference_unions_stay_host_free_at_the_host_ceiling() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let runtime = VartaRuntime::new();
    for expression in [
        "Boolean('')",
        "Boolean('x')",
        "Boolean({n:0})",
        "Boolean([0].slice(0,0))",
        "Boolean(value)",
    ] {
        let bytes = compile(&format!(
            "function fixture():boolean{{const value:string|{{n:number}}|undefined='x';return {expression};}}export function main():void{{}}"
        ));
        let index = export_index(&bytes, "_start") - 1;
        let bytes = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                assert!(data[0] < 127 && index < 128);
                data[0] += 1;
                data.push(4);
                data.extend(b"test");
                data.extend([0, index]);
            }
            vec![data]
        });
        assert!(runtime.inspect_module(&bytes).unwrap().imports.is_empty());
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        assert_eq!(
            instance
                .get_typed_func::<(), i32>(&mut execution.store, "test")
                .unwrap()
                .call(&mut execution.store, ())
                .unwrap(),
            i32::from(expression != "Boolean('')")
        );
        assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    }
}
#[test]
fn primitive_builtins_coercion_budgets_and_deadlines_stay_fatal() {
    use crate::execution::Execution;
    let runtime = VartaRuntime::new();
    for call in [
        "isFinite('1')",
        "isNaN(undefined)",
        "isFinite(null)",
        "isNaN(false)",
    ] {
        let module=runtime.validate_module(&compile(&format!("export function main():void{{try{{{call};}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"))).unwrap();
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
fn primitive_builtins_ignored_arguments_preserve_roots_pairs_status_and_saved_returns() {
    use crate::execution::Execution;
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function keep(first:{text:string},pair:VartaResult<string>,last:boolean):string{churn();console.log(pair.status);return first.text+String(last);}function saved():boolean{try{return Boolean('x');}finally{isFinite('12',churn(),String('overwrite'));}}export function main():void{let value:string|{n:number}|undefined='12'+'tail';console.log(Boolean(value,churn(),value=undefined,varta.result.capture(varta.env.get('BUILTIN_GC_DENIED'))));console.log(keep({text:'root'+'!'},varta.result.capture(varta.env.get('BUILTIN_PAIR_DENIED')),isNaN('bad',churn())));console.log(saved());console.log(varta.result.status());}";
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
            "<capability denied: env> (BUILTIN_GC_DENIED)",
            "1",
            "<capability denied: env> (BUILTIN_PAIR_DENIED)",
            "1",
            "root!true",
            "1",
            "1"
        ])
    );
}
#[test]
fn primitive_builtins_admission_preserves_profiles_and_explicit_opaque_result_fields() {
    for name in ["Boolean", "isFinite", "isNaN"] {
        for &profile in varta_common::language::SUPPORTED_PROFILES
            .iter()
            .filter(|p| !p.primitive_builtins())
        {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{{name}(1);}}"),
                    profile
                )
                .is_err()
            );
        }
        for statement in [
            format!("{name}(...[1]);"),
            format!("{name}<number>(1);"),
            format!("const f={name};f(1);"),
            format!("{name}(varta.result.capture(varta.env.get('X')));"),
        ] {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{{statement}}}"),
                    LanguageProfile::CheckedF64Utf16V23
                )
                .is_err(),
                "{statement}"
            );
        }
    }
    for statement in [
        "new Boolean(true);",
        "Boolean.prototype.valueOf();",
        "isFinite({n:1});",
        "isNaN([]);",
        "isFinite(Error(''));",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V23
            )
            .is_err(),
            "{statement}"
        );
    }
    assert_eq!(VartaRuntime::new().run_wasm_bytes(&compile("export function main():void{const pair=varta.result.capture(varta.env.get('BUILTIN_PAIR_FIELD'));console.log(Boolean(pair.value));console.log(pair.status);}"),&[]).unwrap().lines,lines(&["<capability denied: env> (BUILTIN_PAIR_FIELD)","0","1"]));
}
#[test]
fn primitive_builtins_helper_purity_distinguishes_guest_boolean_and_native_coercion() {
    use crate::contract_tests::{export_index, rewrite};
    let runtime = VartaRuntime::new();
    for expression in ["Boolean(value)", "isFinite('1')", "isNaN('bad')"] {
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{{expression};return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
        if expression.starts_with("Boolean") {
            runtime.validate_module(&bad).unwrap();
        } else {
            assert!(
                runtime
                    .validate_module(&bad)
                    .unwrap_err()
                    .to_string()
                    .contains("cannot call host imports")
            );
        }
    }
}
