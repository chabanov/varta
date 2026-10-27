use super::*;
use varta_common::{CapabilityGrant, CapabilityValue, language::LanguageProfile};

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf8V1)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[])
        .unwrap()
}

#[test]
fn binary64_values_and_live_strings_survive_heap_collection_without_nan_boxing() {
    let source = format!(
        r#"
function churn(n:number):number {{
 const piece={:?};for(let i=0;i<150;i=i+1){{const text=piece+"tail";}}
 return n;
}}
export function main():void{{const keep="live"+" alias";console.log(churn(-0));console.log(churn(NaN));console.log(keep);}}
"#,
        "X".repeat(1024)
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(&source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
    assert!(
        memory.data_size(&execution.store) <= abi::OWNED_STRING_BASE + 2 * abi::WASM_PAGE_BYTES
    );
    assert_eq!(
        execution.store.data().output.lines,
        ["varta log: -0", "varta log: NaN", "varta log: live alias"]
    );
}

#[test]
fn binary64_metadata_and_physical_import_signatures_cannot_be_downgraded_or_mixed() {
    use crate::contract_tests::rewrite;
    use varta_common::{
        abi, language,
        language::{LanguageContract, LanguageProfile},
        value,
    };
    let bytes = compile("export function main():void{console.log(1.5);}");
    let custom = |data: &[u8], name: &str| {
        data.first() == Some(&(name.len() as u8))
            && data.get(1..1 + name.len()) == Some(name.as_bytes())
    };
    let runtime = VartaRuntime::new();
    for action in 0..5 {
        let changed = rewrite(&bytes, |id, data| {
            if id == 0 && custom(data, language::SECTION) {
                if matches!(action, 0 | 2) {
                    return vec![];
                }
                if matches!(action, 1 | 3) {
                    let mut section = data[..1 + language::SECTION.len()].to_vec();
                    section.extend(
                        LanguageContract {
                            schema_version: language::SCHEMA_VERSION,
                            profile: LanguageProfile::CheckedI32Utf8V1,
                        }
                        .to_bytes()
                        .unwrap(),
                    );
                    return vec![section];
                }
            }
            if id == 0 && custom(data, abi::SECTION) {
                if action == 2 {
                    return vec![];
                }
                if action == 3 {
                    let mut section = data[..1 + abi::SECTION.len()].to_vec();
                    section.extend(abi::VERSION.to_le_bytes());
                    return vec![section];
                }
            }
            let mut data = data.to_vec();
            if action == 4 && id == 2 {
                let at = data
                    .windows(b"log_f64".len())
                    .position(|s| s == b"log_f64")
                    .unwrap();
                data[at..at + b"log_i32".len()].copy_from_slice(b"log_i32");
            }
            vec![data]
        });
        assert!(runtime.validate_module(&changed).is_err(), "{action}");
        assert!(runtime.run_wasm_bytes(&changed, &[]).is_err(), "{action}");
    }
    for (name, payload) in [
        (
            value::VALUE_SECTION,
            value::VALUE_VERSION.to_le_bytes().to_vec(),
        ),
        (varta_common::TOOL_SECTION, b"{}".to_vec()),
        (varta_common::http::SECTION, b"{}".to_vec()),
    ] {
        let changed = rewrite(&bytes, |id, data| {
            let mut sections = vec![data.to_vec()];
            if id == 0 && custom(data, language::SECTION) {
                let mut section = vec![name.len() as u8];
                section.extend(name.as_bytes());
                section.extend(&payload);
                sections.push(section);
            }
            sections
        });
        assert!(
            matches!(
                runtime.validate_module(&changed),
                Err(RuntimeError::Language(_))
            ),
            "{name}"
        );
    }
}

#[test]
fn binary64_arithmetic_special_values_comparisons_and_remainder_execute_as_numbers() {
    let source = r#"export function main():void {
 console.log(7/2);console.log(2147483647+1);console.log(0.1+0.2);console.log(-0);
 console.log(1/-0);console.log(1/0);console.log(0/0);console.log(-1e309);
 console.log(NaN===NaN);console.log(NaN!==NaN);console.log(-0===0);
 console.log(NaN<0);console.log(NaN>=0);console.log(Infinity>1e308);
 console.log(-5.5%2);console.log(5.5%-2);console.log(-0%2);console.log(2%Infinity);
 console.log(Infinity%2);console.log(2%0);console.log((5e-324%1)===5e-324);
 const r=1e308%1e-308;console.log(r===r);console.log(r>=0);console.log(r<1e-308);
}"#;
    let wasm = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &wasm).unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&wasm).unwrap();
    assert_eq!(
        inspection.language.profile,
        Some(LanguageProfile::CheckedF64Utf8V1)
    );
    assert!(inspection.imports.iter().any(|i| i.name == "log_f64"));
    let output = runtime.run_wasm_bytes(&wasm, &[]).unwrap();
    let expected = [
        "3.5",
        "2147483648",
        "0.30000000000000004",
        "-0",
        "-Infinity",
        "Infinity",
        "NaN",
        "-Infinity",
        "0",
        "1",
        "1",
        "0",
        "0",
        "1",
        "-1.5",
        "1.5",
        "-0",
        "2",
        "NaN",
        "NaN",
        "1",
        "1",
        "1",
        "1",
    ]
    .map(|s| format!("varta log: {s}"));
    assert_eq!(output.lines, expected);
}

#[test]
fn numeric_truthiness_short_circuit_functions_and_nested_switches_preserve_types_and_order() {
    let output = run(r#"
function side(n:number):number {console.log(n);return n;}
function negative(n:number):number {return -n;}
export function main():void {
 console.log(NaN||5.5);console.log(-0&&side(999));console.log(2.5||side(998));console.log(2.5&&side(3.5));
 console.log(!NaN);console.log(!-0);console.log(!Infinity);console.log(negative(2.5));
 if(NaN){console.log(997);}else{console.log(10.5);}
 let count=0;for(let i=0.5;i<2;i=i+0.5){count=count+1;}console.log(count);
 let once=0.5;while(once){console.log(once);once=0;}
 switch(2.5){case 2.5:switch(-0){case 0:console.log(11.5);break;default:console.log(996);}break;default:console.log(995);}
 switch(NaN){case NaN:console.log(994);break;default:console.log(12.5);}
 {const NaN=4.5;console.log(NaN);}
}"#);
    assert_eq!(
        output.lines,
        [
            "5.5", "-0", "2.5", "3.5", "3.5", "1", "1", "0", "-2.5", "10.5", "3", "0.5", "11.5",
            "12.5", "4.5"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn binary64_host_conversions_status_and_exit_obey_package_and_host_policy() {
    let wasm = compile(
        r#"
function keep(r:VartaResult<number>):VartaResult<number>{return r;}
export function main():void {
 const result=keep(varta.result.capture(varta.time.now()));console.log(result.ok);console.log(result.status);console.log(result.value);
 varta.process.exit(17.5);console.log(varta.result.status());
 const ignored=NaN%2;console.log(varta.result.status());
}"#,
    );
    let runtime = VartaRuntime::new();
    let grants = [
        CapabilityGrant {
            id: "time.now".into(),
            value: CapabilityValue::Boolean(true),
        },
        CapabilityGrant {
            id: "process.exit".into(),
            value: CapabilityValue::Boolean(true),
        },
    ];
    let allowed = runtime.run_wasm_bytes(&wasm, &grants).unwrap();
    assert_eq!(allowed.lines[0], "varta log: 1");
    assert_eq!(allowed.lines[1], "varta log: 0");
    assert!(
        allowed.lines[2]
            .strip_prefix("varta log: ")
            .unwrap()
            .parse::<f64>()
            .unwrap()
            > 0.0
    );
    assert_eq!(
        &allowed.lines[allowed.lines.len() - 2..],
        ["varta log: 2", "varta log: 2"]
    );
    let denied = runtime
        .run_wasm_bytes_with_policy(&wasm, &grants, &[])
        .unwrap();
    assert!(
        denied
            .lines
            .iter()
            .any(|s| s.contains("capability denied: time.now"))
    );
    assert!(
        denied
            .lines
            .iter()
            .any(|s| s.contains("capability denied: process.exit"))
    );
    assert_eq!(
        &denied.lines[denied.lines.len() - 2..],
        ["varta log: 1", "varta log: 1"]
    );
    for value in [
        "NaN",
        "Infinity",
        "-Infinity",
        "17.5",
        "2147483648",
        "-2147483649",
    ] {
        let wasm = compile(&format!(
            "export function main():void{{varta.process.exit({value});console.log(varta.result.status());}}"
        ));
        let output = runtime.run_wasm_bytes(&wasm, &grants).unwrap();
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.lines.last().unwrap(), "varta log: 2");
    }
    assert_eq!(
        runtime
            .run_wasm_bytes(
                &compile("export function main():void{varta.process.exit(17);}"),
                &grants
            )
            .unwrap()
            .exit_code,
        17
    );
}
