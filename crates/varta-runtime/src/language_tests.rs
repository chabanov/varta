use super::*;
use crate::contract_tests::rewrite;
use varta_common::{
    ToolInvocationCode, ToolValue,
    language::{self, LanguageOrigin, LanguageProfile},
};

fn custom(data: &[u8], name: &str) -> bool {
    // Fixture names are short, so their encoded name length occupies one byte.
    data.first() == Some(&(name.len() as u8))
        && data.get(1..1 + name.len()) == Some(name.as_bytes())
}
pub(super) fn replace(bytes: &[u8], metadata: &[u8]) -> Vec<u8> {
    rewrite(bytes, |id, data| {
        if id == 0 && custom(data, language::SECTION) {
            let mut section = data[..1 + language::SECTION.len()].to_vec();
            section.extend(metadata);
            vec![section]
        } else {
            vec![data.to_vec()]
        }
    })
}
fn legacy(bytes: &[u8]) -> Vec<u8> {
    rewrite(bytes, |id, data| {
        if id == 0 && custom(data, language::SECTION) {
            vec![]
        } else {
            vec![data.to_vec()]
        }
    })
}

#[test]
fn declared_and_legacy_profiles_preserve_existing_numeric_string_and_identity_semantics() {
    let bytes = varta_compiler::compile_typescript(
        r#"
export function probe():string {
 const value={n:1};const alias=value;const other={n:1};
 return JSON.stringify({wrap:2147483647+1,divide:7/2,remainder:-5%2,zero:-0,
  content:("same"+"!")=="same!",strict:("same"+"!")==="same!",
  order:"\u{10000}"<"\uE000",same:value===alias,different:value===other,empty:null});
}
export function main():void {console.log(probe());}
varta.tool({name:"probe",description:"Observe the checked language profile"},probe);
"#,
    )
    .unwrap();
    // A compiler emission change must also pass actual Wasmtime module validation.
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let runtime = VartaRuntime::new();
    for (bytes, origin) in [
        (&bytes, LanguageOrigin::Declared),
        (&legacy(&bytes), LanguageOrigin::LegacyHostAbi),
    ] {
        let inspection = runtime.inspect_module(bytes).unwrap();
        assert_eq!(
            inspection.language.profile,
            Some(LanguageProfile::CheckedI32Utf8V1)
        );
        assert_eq!(inspection.language.origin, origin);
        let output = runtime.invoke_tool(bytes, "probe", b"{}", &[]).unwrap();
        let ToolValue::String(json) = output.result.value else {
            panic!("expected string");
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({
                "wrap":i32::MIN,"divide":3,"remainder":-1,"zero":0,"content":true,"strict":true,
                "order":true,"same":true,"different":false,"empty":null
            })
        );
        let run = runtime.run_wasm_bytes(bytes, &[]).unwrap();
        assert_eq!(run.lines, [format!("varta log: {json}")]);
    }
    let unspecified = runtime.inspect_module(b"\0asm\x01\0\0\0").unwrap();
    assert_eq!(unspecified.language.profile, None);
    assert_eq!(unspecified.language.origin, LanguageOrigin::Unspecified);
}

#[test]
fn unsupported_profiles_fail_in_all_runtime_entry_points_before_guest_execution() {
    let bytes = varta_compiler::compile_typescript(
        r#"
export function probe():void{varta.process.exit(77);}
export function main():void{varta.process.exit(78);}
varta.tool({name:"probe",description:"Must not run"},probe);
"#,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let grants = [varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let mut cases = vec![
        replace(&bytes, b"not JSON"),
        replace(&bytes, br#"{"schemaVersion":1,"profile":"ecmascript-2025-v1"}"#),
        replace(&bytes, br#"{"schemaVersion":2,"profile":"checked-i32-utf8-v1"}"#),
        replace(&bytes, br#"{"schemaVersion":1,"profile":"checked-i32-utf8-v1","unknown":true}"#),
        replace(&bytes, br#"{"schemaVersion":1,"profile":"checked-i32-utf8-v1","profile":"checked-i32-utf8-v1"}"#),
        replace(&bytes, &vec![b' '; language::MAX_METADATA_BYTES+1]),
    ];
    cases.push(rewrite(&bytes, |id, data| {
        if id == 0 && custom(data, language::SECTION) {
            vec![data.to_vec(), data.to_vec()]
        } else {
            vec![data.to_vec()]
        }
    }));
    for bytes in cases {
        assert!(matches!(
            runtime.validate_module(&bytes),
            Err(RuntimeError::Language(_))
        ));
        assert!(runtime.inspect_module(&bytes).is_err());
        assert!(matches!(
            runtime.run_wasm_bytes(&bytes, &grants),
            Err(RuntimeError::Language(_))
        ));
        assert_eq!(
            runtime
                .invoke_tool(&bytes, "probe", b"{}", &grants)
                .unwrap_err()
                .code(),
            ToolInvocationCode::InvalidArtifact
        );
    }
}

#[test]
fn explicit_profiles_require_one_matching_host_abi_and_legacy_cannot_bypass_its_version() {
    let bytes = varta_compiler::compile_typescript("export function main():void{}").unwrap();
    for (source, declared) in [(&bytes, true), (&legacy(&bytes), false)] {
        for action in 0..3 {
            let changed = rewrite(source, |id, data| {
                if id == 0 && custom(data, abi::SECTION) {
                    match action {
                        0 => vec![],
                        1 => vec![data.to_vec(), data.to_vec()],
                        _ => {
                            let mut section = data[..1 + abi::SECTION.len()].to_vec();
                            section.extend((abi::VERSION + 1).to_le_bytes());
                            vec![section]
                        }
                    }
                } else {
                    vec![data.to_vec()]
                }
            });
            let runtime = VartaRuntime::new();
            if action == 0 && !declared {
                assert_eq!(
                    runtime.inspect_module(&changed).unwrap().language.origin,
                    LanguageOrigin::Unspecified
                );
            } else {
                assert!(matches!(
                    runtime.validate_module(&changed),
                    Err(RuntimeError::Language(_))
                ));
            }
        }
    }
}

#[test]
fn legacy_invalid_integer_division_keeps_trapping_instead_of_producing_js_numbers() {
    for expression in ["7/0", "(0-2147483647-1)/-1"] {
        let bytes = varta_compiler::compile_typescript(&format!(
            "export function main():void{{console.log({expression});}}"
        ))
        .unwrap();
        for bytes in [&bytes, &legacy(&bytes)] {
            assert!(matches!(
                VartaRuntime::new().run_wasm_bytes(bytes, &[]),
                Err(RuntimeError::Execution(_))
            ));
        }
    }
}

pub(super) fn host_abi(bytes: &[u8], version: Option<u32>) -> Vec<u8> {
    rewrite(bytes, |id, data| {
        if id == 0 && custom(data, abi::SECTION) {
            version.map_or_else(Vec::new, |version| {
                let mut section = data[..1 + abi::SECTION.len()].to_vec();
                section.extend(version.to_le_bytes());
                vec![section]
            })
        } else {
            vec![data.to_vec()]
        }
    })
}

#[test]
fn suspension_profile_v33_requires_its_own_host_abi_before_guest_effects() {
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let source = r#"
class Probe{n:number;constructor(n:number){this.n=n;}}
export function main():void{console.log(`${new Probe(33).n}`);varta.process.exit(77);}
"#;
    let v33 = LanguageProfile::CheckedF64Utf16V33;
    let v32 = LanguageProfile::CheckedF64Utf16V32;
    let bytes = varta_compiler::compile_typescript_with_profile(source, v33).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert_eq!(inspection.language.profile, Some(v33));
    assert_eq!(inspection.language.origin, LanguageOrigin::Declared);
    assert_eq!(v33.host_abi(), abi::RESUMABLE_FRAMES_VERSION);
    let grants = [varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let run = runtime.run_wasm_bytes(&bytes, &grants).unwrap();
    assert_eq!(
        (run.lines.as_slice(), run.exit_code),
        (&["varta log: 33".to_string()][..], 77)
    );
    let older = varta_compiler::compile_typescript_with_profile(source, v32).unwrap();
    runtime.validate_module(&older).unwrap();
    assert_eq!(
        runtime.inspect_module(&older).unwrap().language.profile,
        Some(v32)
    );
    let declared = |profile: LanguageProfile| {
        LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile,
        }
        .to_bytes()
        .unwrap()
    };
    let cases = [
        // V33 metadata downgraded to the v32 host ABI.
        host_abi(&bytes, Some(abi::BIGINT_VALUES_VERSION)),
        // V32 metadata paired with the v33 host ABI, rewritten from either side.
        host_abi(&older, Some(abi::RESUMABLE_FRAMES_VERSION)),
        replace(&bytes, &declared(v32)),
        // V33 metadata without a host ABI section.
        host_abi(&bytes, None),
        // The v33 host ABI without a declared profile is not legacy input.
        legacy(&bytes),
    ];
    for changed in cases {
        assert!(matches!(
            runtime.validate_module(&changed),
            Err(RuntimeError::Language(_))
        ));
        assert!(runtime.inspect_module(&changed).is_err());
        // No log line or exit code is observable: the run fails before instantiation.
        assert!(matches!(
            runtime.run_wasm_bytes(&changed, &grants),
            Err(RuntimeError::Language(_))
        ));
    }
}

fn value_abi(bytes: &[u8], version: Option<u32>) -> Vec<u8> {
    use varta_common::value::VALUE_SECTION;
    rewrite(bytes, |id, data| {
        if id == 0 && custom(data, VALUE_SECTION) {
            version.map_or_else(Vec::new, |version| {
                let mut section = data[..1 + VALUE_SECTION.len()].to_vec();
                section.extend(version.to_le_bytes());
                vec![section]
            })
        } else {
            vec![data.to_vec()]
        }
    })
}

#[test]
fn pilot_profile_v34_requires_its_own_host_and_value_abis_before_guest_effects() {
    use varta_common::{
        language::{LanguageContract, SCHEMA_VERSION},
        value,
    };
    // Classes, generators (frame catalog) and checked JSON: every inherited
    // layout stays value13 while the module declares the reserved value ABI.
    let source = r#"
class Probe{n:number;constructor(n:number){this.n=n;}}
function* digits():Generator<number,void,undefined>{yield 3;yield 4;}
export function main():void{
 let text="";
 for(const digit of digits()){text=text+String(digit);}
 const record={label:"v",n:new Probe(34).n};
 console.log(`${text}:${JSON.stringify(record)}`);
 varta.process.exit(77);
}
"#;
    let v34 = LanguageProfile::CheckedF64Utf16V34;
    let v33 = LanguageProfile::CheckedF64Utf16V33;
    let bytes = varta_compiler::compile_typescript_with_profile(source, v34).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert_eq!(inspection.language.profile, Some(v34));
    assert_eq!(inspection.language.origin, LanguageOrigin::Declared);
    assert_eq!(
        inspection.value_abi_version,
        Some(value::DYNAMIC_VALUE_VERSION)
    );
    assert_eq!(v34.host_abi(), abi::SCHEDULED_FRAMES_VERSION);
    let grants = [varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let logs = [r#"varta log: 34:{"label":"v","n":34}"#.to_string()];
    let run = runtime.run_wasm_bytes(&bytes, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    let older = varta_compiler::compile_typescript_with_profile(source, v33).unwrap();
    let run = runtime.run_wasm_bytes(&older, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    assert_eq!(
        runtime.inspect_module(&older).unwrap().value_abi_version,
        Some(value::BIGINT_VALUE_VERSION)
    );
    let declared = |profile: LanguageProfile| {
        LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile,
        }
        .to_bytes()
        .unwrap()
    };
    let cases = [
        // V34 metadata downgraded to the v33 host ABI.
        host_abi(&bytes, Some(abi::RESUMABLE_FRAMES_VERSION)),
        // V33 metadata paired with the v34 host ABI, rewritten from either side.
        host_abi(&older, Some(abi::SCHEDULED_FRAMES_VERSION)),
        replace(&bytes, &declared(v33)),
        // V34 metadata without a host ABI section.
        host_abi(&bytes, None),
        // The v34 host ABI without a declared profile is not legacy input.
        legacy(&bytes),
        // V34 declaring the v33 value ABI, and v33 declaring the reserved one.
        value_abi(&bytes, Some(value::BIGINT_VALUE_VERSION)),
        value_abi(&older, Some(value::DYNAMIC_VALUE_VERSION)),
        // The function value codec version is not the v34 value ABI either.
        value_abi(&bytes, Some(value::FUNCTION_VALUE_VERSION)),
    ];
    for changed in cases {
        assert!(matches!(
            runtime.validate_module(&changed),
            Err(RuntimeError::Language(_))
        ));
        assert!(runtime.inspect_module(&changed).is_err());
        // No log line or exit code is observable: the run fails before instantiation.
        assert!(matches!(
            runtime.run_wasm_bytes(&changed, &grants),
            Err(RuntimeError::Language(_))
        ));
    }
    // Unknown or missing value ABI metadata also fails before any guest effect.
    for changed in [
        value_abi(&bytes, Some(value::DYNAMIC_VALUE_VERSION + 1)),
        value_abi(&bytes, None),
    ] {
        assert!(runtime.validate_module(&changed).is_err());
        assert!(runtime.inspect_module(&changed).is_err());
        assert!(runtime.run_wasm_bytes(&changed, &grants).is_err());
    }
}

#[test]
fn pilot_profile_v35_declares_value16_and_host_abi_31_and_fails_where_unknown() {
    use varta_common::value;
    // Classes, generators, dictionaries, checked JSON and the scheduler: v35
    // runs every inherited v34 feature unchanged under value layout 16.
    let source = r#"
class Probe{n:number;constructor(n:number){this.n=n;}}
function* digits():Generator<number,void,undefined>{yield 3;yield 5;}
async function later(n:number):Promise<number>{return n;}
export function main():void{
 let text="";
 for(const digit of digits()){text=text+String(digit);}
 const scores:Record<string,number>={v:new Probe(35).n};
 console.log(`${text}:${JSON.stringify(scores)}`);
 later(35).then((n:number):void=>{console.log(`async ${n}`);varta.process.exit(77);});
}
"#;
    let v35 = LanguageProfile::CheckedF64Utf16V35;
    let v34 = LanguageProfile::CheckedF64Utf16V34;
    let bytes = varta_compiler::compile_typescript_with_profile(source, v35).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert_eq!(inspection.language.profile, Some(v35));
    assert_eq!(inspection.language.origin, LanguageOrigin::Declared);
    assert_eq!(
        inspection.value_abi_version,
        Some(value::PROMISE_VALUE_VERSION)
    );
    assert_eq!(v35.host_abi(), abi::PROMISE_VALUES_VERSION);
    assert_eq!(
        (v34.host_abi(), v34.value_abi()),
        (abi::SCHEDULED_FRAMES_VERSION, value::DYNAMIC_VALUE_VERSION)
    );
    let grants = [varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let logs = [
        r#"varta log: 35:{"v":35}"#.to_string(),
        "varta log: async 35".to_string(),
    ];
    let run = runtime.run_wasm_bytes(&bytes, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    let older = varta_compiler::compile_typescript_with_profile(source, v34).unwrap();
    let run = runtime.run_wasm_bytes(&older, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    let cases = [
        // A runtime that does not list a profile rejects its name: this runtime
        // sees an unregistered successor exactly as a v34 runtime sees v35.
        replace(
            &bytes,
            br#"{"schemaVersion":1,"profile":"checked-f64-utf16-v38"}"#,
        ),
        // V35 metadata with the v37, v36, v34 or v33 host ABI or without a
        // host ABI section.
        host_abi(&bytes, Some(abi::NET_REQUEST_VERSION)),
        host_abi(&bytes, Some(abi::TIMERS_VERSION)),
        host_abi(&bytes, Some(abi::SCHEDULED_FRAMES_VERSION)),
        host_abi(&bytes, Some(abi::RESUMABLE_FRAMES_VERSION)),
        host_abi(&bytes, None),
        // The v35 host ABI without a declared profile is not legacy input.
        legacy(&bytes),
        // V35 declaring the value15, value13 or function value codec version.
        value_abi(&bytes, Some(value::DYNAMIC_VALUE_VERSION)),
        value_abi(&bytes, Some(value::BIGINT_VALUE_VERSION)),
        value_abi(&bytes, Some(value::FUNCTION_VALUE_VERSION)),
    ];
    for changed in cases {
        assert!(matches!(
            runtime.validate_module(&changed),
            Err(RuntimeError::Language(_))
        ));
        assert!(runtime.inspect_module(&changed).is_err());
        // No log line or exit code is observable: the run fails before instantiation.
        assert!(matches!(
            runtime.run_wasm_bytes(&changed, &grants),
            Err(RuntimeError::Language(_))
        ));
    }
    for changed in [
        value_abi(&bytes, Some(value::PROMISE_VALUE_VERSION + 1)),
        value_abi(&bytes, None),
    ] {
        assert!(runtime.validate_module(&changed).is_err());
        assert!(runtime.inspect_module(&changed).is_err());
        assert!(runtime.run_wasm_bytes(&changed, &grants).is_err());
    }
}

#[test]
fn pilot_profile_v36_declares_host_abi_32_with_value17_and_fails_where_unknown() {
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    use varta_common::value;
    // Classes, generators, dictionaries, checked JSON, promise values and the
    // scheduler: v36 runs every inherited v35 feature unchanged under its own
    // host ABI and value layout 17.
    let source = r#"
class Probe{n:number;constructor(n:number){this.n=n;}}
function* digits():Generator<number,void,undefined>{yield 3;yield 6;}
async function later(n:number):Promise<number>{return n;}
export function main():void{
 let text="";
 for(const digit of digits()){text=text+String(digit);}
 const scores:Record<string,number>={v:new Probe(36).n};
 console.log(`${text}:${JSON.stringify(scores)}`);
 const pending:Promise<number>[]=[later(36)];
 Promise.all(pending).then((ns:number[]):void=>{console.log(`async ${ns[0]}`);varta.process.exit(77);});
}
"#;
    let v36 = LanguageProfile::CheckedF64Utf16V36;
    let v35 = LanguageProfile::CheckedF64Utf16V35;
    let bytes = varta_compiler::compile_typescript_with_profile(source, v36).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert_eq!(inspection.language.profile, Some(v36));
    assert_eq!(inspection.language.origin, LanguageOrigin::Declared);
    assert_eq!(
        inspection.value_abi_version,
        Some(value::FUNCTION_FIELD_VALUE_VERSION)
    );
    assert_eq!(v36.host_abi(), abi::TIMERS_VERSION);
    assert_eq!(
        (v35.host_abi(), v35.value_abi()),
        (abi::PROMISE_VALUES_VERSION, value::PROMISE_VALUE_VERSION)
    );
    let grants = [varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let logs = [
        r#"varta log: 36:{"v":36}"#.to_string(),
        "varta log: async 36".to_string(),
    ];
    let run = runtime.run_wasm_bytes(&bytes, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    let older = varta_compiler::compile_typescript_with_profile(source, v35).unwrap();
    let run = runtime.run_wasm_bytes(&older, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    let declared = |profile: LanguageProfile| {
        LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile,
        }
        .to_bytes()
        .unwrap()
    };
    let cases = [
        // A runtime that does not list a profile rejects its name: this runtime
        // sees an unregistered successor exactly as a v35 runtime sees v36.
        replace(
            &bytes,
            br#"{"schemaVersion":1,"profile":"checked-f64-utf16-v38"}"#,
        ),
        // A v36 module relabelled as v35 keeps host ABI 32 and fails at its
        // host ABI section; v35 metadata paired with host ABI 32 from the
        // other side fails the same way.
        replace(&bytes, &declared(v35)),
        host_abi(&older, Some(abi::TIMERS_VERSION)),
        // V36 metadata with the v37, v35 or v34 host ABI or without a host
        // ABI section.
        host_abi(&bytes, Some(abi::NET_REQUEST_VERSION)),
        host_abi(&bytes, Some(abi::PROMISE_VALUES_VERSION)),
        host_abi(&bytes, Some(abi::SCHEDULED_FRAMES_VERSION)),
        host_abi(&bytes, None),
        // The v36 host ABI without a declared profile is not legacy input.
        legacy(&bytes),
        // V36 declaring the value16, value15, value13 or function value codec
        // version.
        value_abi(&bytes, Some(value::PROMISE_VALUE_VERSION)),
        value_abi(&bytes, Some(value::DYNAMIC_VALUE_VERSION)),
        value_abi(&bytes, Some(value::BIGINT_VALUE_VERSION)),
        value_abi(&bytes, Some(value::FUNCTION_VALUE_VERSION)),
    ];
    for changed in cases {
        assert!(matches!(
            runtime.validate_module(&changed),
            Err(RuntimeError::Language(_))
        ));
        assert!(runtime.inspect_module(&changed).is_err());
        // No log line or exit code is observable: the run fails before instantiation.
        assert!(matches!(
            runtime.run_wasm_bytes(&changed, &grants),
            Err(RuntimeError::Language(_))
        ));
    }
    for changed in [
        value_abi(&bytes, Some(value::FUNCTION_FIELD_VALUE_VERSION + 1)),
        value_abi(&bytes, None),
    ] {
        assert!(runtime.validate_module(&changed).is_err());
        assert!(runtime.inspect_module(&changed).is_err());
        assert!(runtime.run_wasm_bytes(&changed, &grants).is_err());
    }
}

#[test]
fn pilot_profile_v37_declares_host_abi_33_with_value17_and_fails_where_unknown() {
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    use varta_common::value;
    // Classes, generators, dictionaries, checked JSON, promise values and the
    // scheduler: v37 runs every inherited v36 feature unchanged under its own
    // host ABI and value layout 17.
    let source = r#"
class Probe{n:number;constructor(n:number){this.n=n;}}
function* digits():Generator<number,void,undefined>{yield 3;yield 7;}
async function later(n:number):Promise<number>{return n;}
export function main():void{
 let text="";
 for(const digit of digits()){text=text+String(digit);}
 const scores:Record<string,number>={v:new Probe(37).n};
 console.log(`${text}:${JSON.stringify(scores)}`);
 const pending:Promise<number>[]=[later(37)];
 Promise.all(pending).then((ns:number[]):void=>{console.log(`async ${ns[0]}`);varta.process.exit(77);});
}
"#;
    let v37 = LanguageProfile::CheckedF64Utf16V37;
    let v36 = LanguageProfile::CheckedF64Utf16V36;
    let bytes = varta_compiler::compile_typescript_with_profile(source, v37).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert_eq!(inspection.language.profile, Some(v37));
    assert_eq!(inspection.language.origin, LanguageOrigin::Declared);
    assert_eq!(
        inspection.value_abi_version,
        Some(value::FUNCTION_FIELD_VALUE_VERSION)
    );
    assert_eq!(v37.host_abi(), abi::NET_REQUEST_VERSION);
    assert_eq!(
        (v36.host_abi(), v36.value_abi()),
        (abi::TIMERS_VERSION, value::FUNCTION_FIELD_VALUE_VERSION)
    );
    let grants = [varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let logs = [
        r#"varta log: 37:{"v":37}"#.to_string(),
        "varta log: async 37".to_string(),
    ];
    let run = runtime.run_wasm_bytes(&bytes, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    let older = varta_compiler::compile_typescript_with_profile(source, v36).unwrap();
    let run = runtime.run_wasm_bytes(&older, &grants).unwrap();
    assert_eq!((run.lines.as_slice(), run.exit_code), (&logs[..], 77));
    let declared = |profile: LanguageProfile| {
        LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile,
        }
        .to_bytes()
        .unwrap()
    };
    let cases = [
        // A runtime that does not list a profile rejects its name: this runtime
        // sees an unregistered successor exactly as a v36 runtime sees v37.
        replace(
            &bytes,
            br#"{"schemaVersion":1,"profile":"checked-f64-utf16-v38"}"#,
        ),
        // A v37 module relabelled as v36 keeps host ABI 33 and fails at its
        // host ABI section; v36 metadata paired with host ABI 33 from the
        // other side fails the same way.
        replace(&bytes, &declared(v36)),
        host_abi(&older, Some(abi::NET_REQUEST_VERSION)),
        // V37 metadata with the v36 or v35 host ABI or without a host ABI
        // section.
        host_abi(&bytes, Some(abi::TIMERS_VERSION)),
        host_abi(&bytes, Some(abi::PROMISE_VALUES_VERSION)),
        host_abi(&bytes, None),
        // The v37 host ABI without a declared profile is not legacy input.
        legacy(&bytes),
        // V37 declaring the value16, value15, value13 or function value codec
        // version.
        value_abi(&bytes, Some(value::PROMISE_VALUE_VERSION)),
        value_abi(&bytes, Some(value::DYNAMIC_VALUE_VERSION)),
        value_abi(&bytes, Some(value::BIGINT_VALUE_VERSION)),
        value_abi(&bytes, Some(value::FUNCTION_VALUE_VERSION)),
    ];
    for changed in cases {
        assert!(matches!(
            runtime.validate_module(&changed),
            Err(RuntimeError::Language(_))
        ));
        assert!(runtime.inspect_module(&changed).is_err());
        // No log line or exit code is observable: the run fails before instantiation.
        assert!(matches!(
            runtime.run_wasm_bytes(&changed, &grants),
            Err(RuntimeError::Language(_))
        ));
    }
    for changed in [
        value_abi(&bytes, Some(value::FUNCTION_FIELD_VALUE_VERSION + 1)),
        value_abi(&bytes, None),
    ] {
        assert!(runtime.validate_module(&changed).is_err());
        assert!(runtime.inspect_module(&changed).is_err());
        assert!(runtime.run_wasm_bytes(&changed, &grants).is_err());
    }
}
