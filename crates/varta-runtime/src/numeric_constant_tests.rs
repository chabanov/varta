use super::*;
use varta_common::language::LanguageProfile;

const CONSTANTS: [(&str, u64); 16] = [
    ("Math.E", 0x4005_bf0a_8b14_5769),
    ("Math.LN10", 0x4002_6bb1_bbb5_5516),
    ("Math.LN2", 0x3fe6_2e42_fefa_39ef),
    ("Math.LOG10E", 0x3fdb_cb7b_1526_e50e),
    ("Math.LOG2E", 0x3ff7_1547_652b_82fe),
    ("Math.PI", 0x4009_21fb_5444_2d18),
    ("Math.SQRT1_2", 0x3fe6_a09e_667f_3bcd),
    ("Math.SQRT2", 0x3ff6_a09e_667f_3bcd),
    ("Number.EPSILON", 0x3cb0_0000_0000_0000),
    ("Number.MAX_SAFE_INTEGER", 0x433f_ffff_ffff_ffff),
    ("Number.MIN_SAFE_INTEGER", 0xc33f_ffff_ffff_ffff),
    ("Number.MAX_VALUE", 0x7fef_ffff_ffff_ffff),
    ("Number.MIN_VALUE", 1),
    ("Number.NaN", 0x7ff8_0000_0000_0000),
    ("Number.NEGATIVE_INFINITY", 0xfff0_0000_0000_0000),
    ("Number.POSITIVE_INFINITY", 0x7ff0_0000_0000_0000),
];
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

#[test]
fn constants_emit_exact_binary64_bits_without_imports_or_host_allowance() {
    use crate::contract_tests::{export_index, rewrite};
    for computed in [false, true] {
        let mut source = String::new();
        for (index, (name, _)) in CONSTANTS.iter().enumerate() {
            let (object, key) = name.split_once('.').unwrap();
            let expression = if computed {
                format!("({object})[('{key}')]")
            } else {
                (*name).into()
            };
            source.push_str(&format!(
                "function value_{index}():number{{return {expression};}}"
            ));
        }
        source.push_str("export function main():void{}");
        let bytes = compile(&source);
        let first = export_index(&bytes, "_start") - CONSTANTS.len() as u8;
        let bytes = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                assert!(usize::from(data[0]) + CONSTANTS.len() < 128);
                data[0] += CONSTANTS.len() as u8;
                for (index, (name, _)) in CONSTANTS.iter().enumerate() {
                    data.push(name.len() as u8);
                    data.extend_from_slice(name.as_bytes());
                    data.push(0);
                    data.push(first + index as u8);
                }
            }
            vec![data]
        });
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        assert_eq!(module.imports().len(), 0);
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        for (name, bits) in CONSTANTS {
            let actual = instance
                .get_typed_func::<(), f64>(&mut execution.store, name)
                .unwrap()
                .call(&mut execution.store, ())
                .unwrap();
            if f64::from_bits(bits).is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), bits, "{name}, computed={computed}");
            }
        }
        assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    }
}

#[test]
fn constants_work_in_numeric_operations_json_unions_and_closures() {
    let out = run(
        "function limit(value:number|undefined):number{if(value!==undefined){return Math.min(value,Number.MAX_VALUE);}return Number.MIN_VALUE;}export function main():void{const pi=Math.PI;const read=():number=>pi;console.log(read());console.log(1+Number.EPSILON>1);console.log(Number.MIN_VALUE>0);console.log(Number.MIN_VALUE/2);console.log(Number.isFinite(Number.MAX_VALUE));console.log(Number.isSafeInteger(Number.MAX_SAFE_INTEGER));console.log(Number.isSafeInteger(Number.MIN_SAFE_INTEGER));console.log(Number.isSafeInteger(Number.MAX_SAFE_INTEGER+1));console.log(Number.isNaN(Number.NaN));console.log(JSON.stringify(limit(undefined)));console.log(JSON.stringify([Number.NaN,Number.NEGATIVE_INFINITY,Number.POSITIVE_INFINITY]));console.log(Math['PI']===Math.PI);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "3.141592653589793",
            "1",
            "1",
            "0",
            "1",
            "1",
            "1",
            "0",
            "1",
            "5e-324",
            "[null,null,null]",
            "1"
        ])
    );
}

#[test]
fn constants_preserve_shadowed_records_parameter_captures_and_tdz() {
    let out = run(
        "function f(Math:{PI:number},Number:{EPSILON:number}):number{Math.PI++;Number.EPSILON+=2;return Math.PI+Number.EPSILON;}export function main():void{const Math={PI:7};const Number={EPSILON:3};const read=():number=>Math.PI+Number.EPSILON;console.log(read());console.log(f(Math,Number));console.log(read());{const Math={PI:1};console.log(Math.PI);}console.log(Math.PI);}",
    );
    assert_eq!(out.lines, lines(&["10", "13", "13", "1", "8"]));
    for source in [
        "export function main():void{console.log(Math.PI);let Math={PI:1};}",
        "export function main():void{console.log(Number.EPSILON);let Number={EPSILON:1};}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V15,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("before initialization"),
            "{error}"
        );
    }
}

#[test]
fn constants_reject_every_write_path_with_a_readonly_type_diagnostic() {
    for statement in [
        "Math.PI=1;",
        "console.log(Math.PI=1);",
        "Number.MAX_VALUE+=1;",
        "Math.PI**=2;",
        "Math.PI|=1;",
        "Math.PI&&=1;",
        "Math.PI||=1;",
        "Math.PI??=1;",
        "Math.PI++;",
        "++Number.EPSILON;",
        "Math['PI']=1;",
        "delete Math.PI;",
        "[Math.PI]=[1];",
        "({n:Number.MIN_VALUE}={n:1});",
    ] {
        let source = format!("export function main():void{{{statement}}}");
        let error = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V15,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("read-only Math/Number constant"),
            "{source}: {error}"
        );
    }
}

#[test]
fn constants_keep_old_profiles_and_ordinary_shadowed_reads_unchanged() {
    for profile in varta_common::language::SUPPORTED_PROFILES.iter().copied() {
        let shadow = "export function main():void{const Math={PI:1};const Number={MAX_VALUE:2};console.log(Math.PI);console.log(Number.MAX_VALUE);}";
        let bytes = varta_compiler::compile_typescript_with_profile(shadow, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["1", "2"])
        );
        if !profile.numeric_constants() {
            for (name, _) in CONSTANTS {
                let source = format!("export function main():void{{console.log({name});}}");
                assert!(
                    varta_compiler::compile_typescript_with_profile(&source, profile).is_err(),
                    "{profile:?}: {name}"
                );
            }
        }
    }
    for expression in [
        "Math.UNKNOWN",
        "Number.UNKNOWN",
        "Math['P'+'I']",
        "Math[(Math.PI)]",
        "Math.abs",
        "Number.isFinite",
    ] {
        let source = format!("export function main():void{{console.log({expression});}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V15
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn constants_are_host_free_in_json_helpers_and_preserve_prior_status() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{return Number.isFinite(Math.PI)&&Number.isFinite(Number.MIN_VALUE);}export function main():void{varta.json.parse<number>('1');}",
    );
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
    VartaRuntime::new().validate_module(&fixture).unwrap();
    let out = run(
        "export function main():void{varta.env.get('NUMERIC_CONSTANT_DENIED');console.log(JSON.stringify(Number.MIN_VALUE));console.log(Math.PI);console.log(varta.result.status());}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "<capability denied: env> (NUMERIC_CONSTANT_DENIED)",
            "5e-324",
            "3.141592653589793",
            "1"
        ])
    );
}
