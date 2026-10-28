use super::*;
use crate::contract_tests::{export_index, rewrite};
use crate::execution::Execution;
use varta_common::language::{LanguageContract, LanguageProfile, SCHEMA_VERSION};

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V17)
        .unwrap()
}
fn script(execution: &mut Execution, words: &[Result<u64, &'static str>]) {
    execution.store.data_mut().random.scripted = Some(words.iter().copied().collect());
}
fn remaining(execution: &Execution) -> usize {
    execution
        .store
        .data()
        .random
        .scripted
        .as_ref()
        .unwrap()
        .len()
}
fn start(execution: &mut Execution, instance: &wasmtime::Instance) -> Result<(), RuntimeError> {
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")?
        .call(&mut execution.store, ())?;
    crate::exceptions::finish(instance, &mut execution.store)
}
fn exported_random() -> Vec<u8> {
    let bytes =
        compile("function fixture():number{return Math.random();}export function main():void{}");
    let index = export_index(&bytes, "_start") - 1;
    rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && index < 128);
            data[0] += 1;
            data.extend([6]);
            data.extend(b"sample");
            data.extend([0, index]);
        }
        vec![data]
    })
}

#[test]
fn random_exact_samples_charge_once_and_have_independent_execution_sources() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&exported_random()).unwrap();
    let mut first = Execution::new(&runtime.engine, &[], None).unwrap();
    let mut second = Execution::new(&runtime.engine, &[], None).unwrap();
    script(&mut first, &[Ok(0), Ok(u64::MAX), Ok(1 << 63)]);
    script(&mut second, &[Ok(1 << 62), Ok(2048)]);
    let one = first.instantiate(&module).unwrap();
    let two = second.instantiate(&module).unwrap();
    let a = one
        .get_typed_func::<(), f64>(&mut first.store, "sample")
        .unwrap();
    let b = two
        .get_typed_func::<(), f64>(&mut second.store, "sample")
        .unwrap();
    assert_eq!(a.call(&mut first.store, ()).unwrap().to_bits(), 0);
    assert_eq!(b.call(&mut second.store, ()).unwrap(), 0.25);
    assert_eq!(
        a.call(&mut first.store, ()).unwrap().to_bits(),
        1.0_f64.to_bits() - 1
    );
    assert_eq!(remaining(&first), 1);
    assert_eq!(remaining(&second), 1);
    assert_eq!(a.call(&mut first.store, ()).unwrap(), 0.5);
    assert_eq!(b.call(&mut second.store, ()).unwrap(), 2.0_f64.powi(-53));
    assert_eq!(first.store.data().host_calls, 3);
    assert_eq!(second.store.data().host_calls, 2);
    drop(first);
    let mut fresh = Execution::new(&runtime.engine, &[], None).unwrap();
    assert!(fresh.store.data().random.scripted.is_none());
    let instance = fresh.instantiate(&module).unwrap();
    let f = instance
        .get_typed_func::<(), f64>(&mut fresh.store, "sample")
        .unwrap();
    for _ in 0..128 {
        let x = f.call(&mut fresh.store, ()).unwrap();
        assert!((0.0..1.0).contains(&x) && !x.is_sign_negative());
        assert_eq!((x * 9007199254740992.0).fract(), 0.0);
    }
    assert_eq!(fresh.store.data().host_calls, 128);
}

#[test]
fn random_evaluates_all_ignored_argument_kinds_in_order_without_coercion() {
    let source = "function mark(n:number):number{console.log(n);return n;}function plus(n:number):number{return n+1;}export function main():void{const n:number|undefined=1;console.log(Math.random(mark(1),true,null,undefined,'text',n,{text:'own'},[1,2],RangeError('edge'),plus,varta.result.capture(varta.env.get('RANDOM_ARGUMENT_DENIED')),mark(2)));console.log(varta.result.status());console.log(Math.sin(0));console.log(Math.pow(2,3));console.log(JSON.stringify({finite:Number.isFinite(1),half:Math.f16round(1)}));}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    script(&mut execution, &[Ok(1 << 63)]);
    let instance = execution.instantiate(&module).unwrap();
    start(&mut execution, &instance).unwrap();
    assert_eq!(remaining(&execution), 0);
    assert_eq!(
        execution.store.data().output.lines,
        [
            "1",
            "<capability denied: env> (RANDOM_ARGUMENT_DENIED)",
            "2",
            "0.5",
            "1",
            "0",
            "8",
            r#"{"finite":true,"half":1}"#
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn random_throwing_arguments_skip_later_effects_and_do_not_consume_entropy() {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile("function fail():string{throw TypeError('argument');}function later():number{console.log('BAD_LATER');return 1;}export function main():void{try{Math.random('before',fail(),later());}catch({message}){console.log(message);}console.log(Math.random());}")).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    script(&mut execution, &[Ok(1 << 62), Ok(1 << 63)]);
    let instance = execution.instantiate(&module).unwrap();
    start(&mut execution, &instance).unwrap();
    assert_eq!(remaining(&execution), 1);
    assert_eq!(
        execution.store.data().output.lines,
        ["varta log: argument", "varta log: 0.25"]
    );
}

#[test]
fn random_entropy_failures_and_host_limits_are_fatal_without_fallback() {
    let bytes = compile(
        "export function main():void{try{console.log(Math.random());}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for kind in 0..3 {
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        script(
            &mut execution,
            &[Err("injected entropy failure"), Ok(1 << 63)],
        );
        let instance = execution.instantiate(&module).unwrap();
        if kind == 1 {
            execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        }
        if kind == 2 {
            execution.store.data_mut().deadline = Instant::now();
        }
        let error = start(&mut execution, &instance).unwrap_err();
        assert!(format!("{error:#}").contains(if kind == 0 {
            "injected entropy failure"
        } else {
            "host operation budget exhausted"
        }));
        assert!(execution.store.data().output.lines.is_empty());
        assert_eq!(remaining(&execution), if kind == 0 { 1 } else { 2 });
    }
    let module = runtime.validate_module(&exported_random()).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    script(
        &mut execution,
        &vec![Ok(0); abi::MAX_HOST_CALLS as usize + 1],
    );
    let instance = execution.instantiate(&module).unwrap();
    let f = instance
        .get_typed_func::<(), f64>(&mut execution.store, "sample")
        .unwrap();
    for _ in 0..abi::MAX_HOST_CALLS {
        assert_eq!(f.call(&mut execution.store, ()).unwrap().to_bits(), 0);
    }
    assert!(f.call(&mut execution.store, ()).is_err());
    assert_eq!(remaining(&execution), 1);
}

#[test]
fn random_profile_rejects_old_profiles_borrowing_spread_generics_and_shadowing() {
    for &profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .filter(|p| !p.random())
    {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{Math.random();}",
                profile
            )
            .is_err(),
            "{profile:?}"
        );
    }
    for source in [
        "Math['random']();",
        "const f=Math.random;f();",
        "Math.random(...[1]);",
        "Math.random<number>();",
        "const Math={random:1};Math.random();",
        "Math.random();let Math={random:1};",
        "Math.sin('bad');",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{source}}}"),
                LanguageProfile::CheckedF64Utf16V17
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn random_profile_and_import_mismatches_fail_before_guest_effects() {
    let runtime = VartaRuntime::new();
    let bytes = compile("export function main():void{console.log(Math.random());}");
    for action in 0..5 {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                && action < 3
            {
                if action == 2 {
                    return vec![];
                }
                data[1 + abi::SECTION.len()..]
                    .copy_from_slice(&abi::NATIVE_MATH_VERSION.to_le_bytes());
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
                        profile: LanguageProfile::CheckedF64Utf16V16,
                    }
                    .to_bytes()
                    .unwrap(),
                );
            }
            if id == 1 && action == 3 {
                let signature = [0x60, 0, 1, 0x7c];
                let at = data
                    .windows(signature.len())
                    .position(|s| s == signature)
                    .unwrap();
                data[at + 3] = 0x7f;
            }
            if id == 2 && action == 4 {
                let name = b"number_random";
                let at = data.windows(name.len()).position(|s| s == name).unwrap();
                data[at] = b'N';
            }
            vec![data]
        });
        assert!(runtime.validate_module(&bad).is_err(), "action {action}");
    }
    let inspection = runtime
        .inspect_module(&compile("export function main():void{}"))
        .unwrap();
    assert_eq!(
        inspection.language.profile.unwrap().host_abi(),
        abi::RANDOM_VERSION
    );
    assert_eq!(
        inspection.value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
}

#[test]
fn random_is_forbidden_in_host_free_json_allocation_helpers() {
    let bytes = compile(
        "function fake(value:boolean):boolean{Math.random();return value;}export function main():void{varta.json.parse<number>('1');}",
    );
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

#[test]
fn random_ignored_pairs_and_arguments_preserve_gc_roots_and_saved_returns() {
    let source = "function churn():string{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return text;}function keep(first:{text:string},pair:VartaResult<string>,n:number):string{churn();console.log(pair.status);console.log(n);return first.text;}function saved():number{try{return Math.random();}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('RANDOM_GC_DENIED')),Math.random(churn(),varta.result.capture(varta.env.get('RANDOM_PAIR_DENIED')))));console.log(saved());console.log(varta.result.status());}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(1000 * abi::FUEL).unwrap();
    script(&mut execution, &[Ok(1 << 63), Ok(1 << 62)]);
    let instance = execution.instantiate(&module).unwrap();
    start(&mut execution, &instance).unwrap();
    assert_eq!(remaining(&execution), 0);
    assert_eq!(
        execution.store.data().output.lines,
        [
            "<capability denied: env> (RANDOM_GC_DENIED)",
            "<capability denied: env> (RANDOM_PAIR_DENIED)",
            "1",
            "0.5",
            "earlier!",
            "0.25",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
