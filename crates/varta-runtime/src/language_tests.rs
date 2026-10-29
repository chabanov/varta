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
fn replace(bytes: &[u8], metadata: &[u8]) -> Vec<u8> {
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

fn host_abi(bytes: &[u8], version: Option<u32>) -> Vec<u8> {
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
