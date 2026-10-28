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
fn number_parse_global_and_number_forms_use_prefixes_and_primitive_coercion() {
    let source = "function show(x:number|string|boolean|null|undefined):void{console.log(JSON.stringify([parseInt(x,36),Number.parseInt(x,36),parseFloat(x),Number.parseFloat(x)]));}export function main():void{console.log(Number.isNaN(parseInt()));console.log(Number.isNaN(Number.parseFloat()));console.log(parseInt('  -0xFtail'));console.log(parseInt('0x10',10));console.log(Number.parseInt('0x10','0x10'));console.log(parseInt('10',4294967312));console.log(parseInt('10',-4294967280));console.log(parseInt('10',Infinity));console.log(parseInt('10',true));console.log(parseFloat('1.e-2tail'));console.log(parseFloat('1e+tail'));console.log(parseFloat('-Infinity!'));console.log(1/parseInt('-0tail'));console.log(1/parseFloat('-0e999tail'));show('12.5junk');show(true);show(null);show(undefined);show(1e21);console.log(parseInt(-0));console.log(1/parseFloat(-0));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "1",
            "1",
            "-15",
            "0",
            "16",
            "16",
            "16",
            "10",
            "NaN",
            "0.01",
            "1",
            "-Infinity",
            "-Infinity",
            "-Infinity",
            "[38,38,12.5,12.5]",
            "[1389110,1389110,null,null]",
            "[1112745,1112745,null,null]",
            "[86464843759093,86464843759093,null,null]",
            "[50,50,1e+21,1e+21]",
            "0",
            "Infinity"
        ])
    );
}
#[test]
fn number_parse_caches_arguments_before_conversion_and_preserves_exception_order() {
    let source = "function first():string{console.log('first');return 'ff!';}function radix():string{console.log('radix');return '16';}function extra():string{console.log('extra');return 'ignored';}function fail():string{throw Error('stop');}export function main():void{console.log(parseInt(first(),radix(),extra(),{n:1},[1]));let x:string|number='12tail';let r:number|string='10';console.log(Number.parseInt(x,r,x='99',r='16'));console.log(x);console.log(r);console.log(parseFloat('1.5tail',extra(),{n:1}));try{parseInt(fail(),radix(),extra());}catch({message}){console.log(message);}try{parseInt(first(),fail(),extra());}catch({message}){console.log(message);}console.log(Number.parseInt('10',1,extra()));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "first", "radix", "extra", "255", "12", "99", "16", "extra", "1.5", "stop", "first",
            "stop", "extra", "NaN"
        ])
    );
}
#[test]
fn number_parse_global_shadowing_retains_functions_closures_and_tdz() {
    let source = "function parseInt(n:number):number{return n+1;}export function main():void{const parseFloat=(n:number):number=>n+2;console.log(parseInt(3));console.log(parseFloat(3));console.log(Number.parseInt('12tail'));console.log(Number.parseFloat('1.5tail'));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["4", "5", "12", "1.5"])
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{parseInt('12');const parseInt=(x:string):number=>1;}",
            LanguageProfile::CheckedF64Utf16V23
        )
        .is_err()
    );
}
#[test]
fn number_parse_rejects_old_profiles_reference_inputs_and_unsupported_call_forms() {
    for name in [
        "parseInt",
        "parseFloat",
        "Number.parseInt",
        "Number.parseFloat",
    ] {
        for &profile in varta_common::language::SUPPORTED_PROFILES
            .iter()
            .filter(|p| !p.number_parse())
        {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{{name}('12');}}"),
                    profile
                )
                .is_err(),
                "{name}/{profile:?}"
            );
        }
        for statement in [
            format!("{name}({{n:1}});"),
            format!("{name}([1]);"),
            format!("{name}(...['12']);"),
            format!("{name}<string>('12');"),
            format!("const f={name};f('12');"),
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
        "parseInt('12',{n:1});",
        "Number['parseInt']('12');",
        "const Number={n:1};Number.parseInt('12');",
        "const x:string|{n:number}='12';parseFloat(x);",
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
#[test]
fn number_parse_budgets_and_deadlines_bypass_catch_and_finally() {
    use crate::execution::Execution;
    let runtime = VartaRuntime::new();
    for call in [
        "parseInt('12')",
        "Number.parseInt('12','10')",
        "parseFloat('12')",
        "Number.parseFloat(12)",
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
fn number_parse_owns_cached_inputs_pairs_and_saved_returns_across_collection() {
    use crate::execution::Execution;
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 2;}function keep(first:{text:string},pair:VartaResult<string>,last:number):string{churn();console.log(pair.status);return first.text+String(last);}function saved():number{try{return parseFloat('-0tail');}finally{Number.parseInt('ff',16,churn(),String('overwrite'));}}export function main():void{let text:string|number='12'+'tail';let radix:number|string='1'+'0';console.log(parseInt(text,radix,churn(),text='99',radix=16,varta.result.capture(varta.env.get('PARSE_GC_DENIED'))));console.log(keep({text:'root'+'!'},varta.result.capture(varta.env.get('PARSE_PAIR_DENIED')),parseFloat('1.5tail',churn())));console.log(1/saved());console.log(varta.result.status());}";
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
            "<capability denied: env> (PARSE_GC_DENIED)",
            "12",
            "<capability denied: env> (PARSE_PAIR_DENIED)",
            "1",
            "root!1.5",
            "-Infinity",
            "1"
        ])
    );
}
#[test]
fn number_parse_metadata_signatures_and_host_free_helpers_fail_closed() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let runtime = VartaRuntime::new();
    for (name, host, sig) in [
        (
            "parseInt",
            varta_common::HostOperation::NumberParseInt,
            &[0x60, 2, 0x7e, 0x7e, 1, 0x7c][..],
        ),
        (
            "parseFloat",
            varta_common::HostOperation::NumberParseFloat,
            &[0x60, 1, 0x7e, 1, 0x7c][..],
        ),
    ] {
        let bytes = compile(&format!("export function main():void{{{name}('12');}}"));
        for action in 0..4 {
            let bad = rewrite(&bytes, |id, data| {
                let mut data = data.to_vec();
                if id == 0
                    && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                    && action <= 1
                {
                    data[1 + abi::SECTION.len()..]
                        .copy_from_slice(&abi::NUMBER_FORMAT_VERSION.to_le_bytes());
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
                            profile: LanguageProfile::CheckedF64Utf16V22,
                        }
                        .to_bytes()
                        .unwrap(),
                    );
                }
                if id == 1 && action == 2 {
                    let at = data.windows(sig.len()).position(|s| s == sig).unwrap();
                    data[at + 2] = 0x7f;
                }
                if id == 2 && action == 3 {
                    let name = host.name().as_bytes();
                    let at = data.windows(name.len()).position(|s| s == name).unwrap();
                    data[at] = b'N';
                }
                vec![data]
            });
            assert!(runtime.validate_module(&bad).is_err(), "{name}/{action}");
        }
        let bytes = compile(&format!(
            "function fake(value:boolean):boolean{{{name}('12');return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn number_parse_numeric_inputs_charge_once_and_need_no_result_globals() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let runtime = VartaRuntime::new();
    for call in ["parseInt(n,16)", "Number.parseFloat(n)"] {
        let bytes = compile(&format!(
            "function fixture(n:number):number{{return {call};}}export function main():void{{}}"
        ));
        let index = export_index(&bytes, "_start") - 1;
        let bytes = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                assert!(data[0] < 127 && index < 128);
                data[0] += 1;
                data.push(5);
                data.extend(b"parse");
                data.extend([0, index]);
            }
            vec![data]
        });
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let function = instance
            .get_typed_func::<f64, f64>(&mut execution.store, "parse")
            .unwrap();
        for (index, x) in [
            0.,
            -0.,
            12.,
            1e21,
            1e-7,
            f64::MAX,
            f64::from_bits(1),
            f64::NAN,
            f64::INFINITY,
        ]
        .into_iter()
        .enumerate()
        {
            let value = function.call(&mut execution.store, x).unwrap();
            if call.contains("Float") {
                if x.is_nan() {
                    assert!(value.is_nan());
                } else if x == 0. {
                    assert_eq!(value.to_bits(), 0);
                } else {
                    assert_eq!(value.to_bits(), x.to_bits());
                }
            }
            assert_eq!(execution.store.data().host_calls, index as u32 + 1);
        }
        assert!(execution.store.data().output.lines.is_empty());
    }
}
