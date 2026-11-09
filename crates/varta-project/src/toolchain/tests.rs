use super::{compile, contract::*};
use base64::Engine;

fn fixture() -> CompileRequest {
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"compilerSha256":"a".repeat(64),"operation":"build","target":"browser","languageProfile":"checked-f64-utf16-v40","dependencyPolicy":"local-only-v1","entry":"src/main.ts","files":[{"path":"src/main.ts","content":"import {value} from '../value.ts'; export function main():void{console.log(value);throw new Error('must never execute');}"},{"path":"value.ts","content":"export const value:number=42;"}]})).unwrap()
}
#[test]
fn toolchain_bundle_bounds_canonical_identity_and_no_source_configuration() {
    let request = fixture();
    request.validate().unwrap();
    let mut reordered = request.clone();
    reordered.files.reverse();
    assert_eq!(request.identity().unwrap(), reordered.identity().unwrap());
    for path in [
        "../escape.ts",
        "/absolute.ts",
        "a//b.ts",
        "a/./b.ts",
        "node_modules/pkg/index.ts",
        "a\\b.ts",
        "a%2fb.ts",
        "x:\\a.ts",
        "a\0.ts",
    ] {
        let mut invalid = request.clone();
        invalid.files[0].path = path.into();
        assert!(invalid.validate().is_err(), "{path}");
    }
    let mut duplicate = request.clone();
    duplicate.files.push(duplicate.files[0].clone());
    assert!(duplicate.validate().is_err());
    let mut large = request.clone();
    large.files[0].content = "x".repeat(MAX_FILE_BYTES + 1);
    assert!(large.validate().is_err());
    let mut json = serde_json::to_value(&request).unwrap();
    json["capabilities"] = serde_json::json!({"env":{"get":["*"]}});
    assert!(serde_json::from_value::<CompileRequest>(json).is_err());
}
#[test]
fn toolchain_compiles_exact_immutable_files_without_executing_them() {
    let request = fixture();
    let built = compile::compile(&request).unwrap();
    assert!(built.result.ok, "{:?}", built.result.diagnostics);
    assert!(built.files.contains_key("index.html"));
    assert!(built.files.keys().any(|p| p.ends_with(".wasm")));
    assert!(built.files.keys().all(|p| !p.ends_with(".ts")));
    let mut checked = request.clone();
    checked.operation = Operation::Check;
    let checked = compile::compile(&checked).unwrap();
    assert!(checked.result.ok);
    assert!(checked.files.is_empty());
    // Inputs stay immutable and no relative filesystem fallback supplies a file.
    let mut missing = request.clone();
    missing.files.pop();
    let missing = compile::compile(&missing).unwrap();
    assert!(!missing.result.ok);
    assert!(missing.files.is_empty());
    assert!(
        missing.result.diagnostics[0]
            .message
            .contains("bundle module not found")
    );
    assert_eq!(request.identity().unwrap(), fixture().identity().unwrap());
}
#[test]
fn toolchain_preserves_unicode_source_diagnostics_and_refuses_ambient_imports() {
    let mut request = fixture();
    request.files = vec![SourceFile {
        path: request.entry.clone(),
        content: "export function main():void{const назва: number = 'помилка';}".into(),
    }];
    let reply = compile::compile(&request).unwrap();
    assert!(!reply.result.ok);
    let diagnostic = &reply.result.diagnostics[0];
    assert_eq!(diagnostic.path, "src/main.ts");
    assert_eq!(diagnostic.line, 1);
    assert!(diagnostic.column > 1);
    assert!(!diagnostic.message.is_empty());
    for specifier in [
        "../../outside.ts",
        "pkg",
        "https://example.test/index.ts",
        "/etc/passwd",
        "./legacy.js",
    ] {
        request.files[0].content = format!(
            "import {{x}} from '{specifier}';export function main():void{{console.log(x);}}"
        );
        let reply = compile::compile(&request).unwrap();
        assert!(!reply.result.ok, "{specifier}");
        assert!(reply.files.is_empty());
    }
}
#[test]
fn toolchain_host_ceiling_is_explicit_strict_and_resource_bounded() {
    let value = serde_json::json!({"schemaVersion":1,"listen":"127.0.0.1:0","operations":["check","build"],"profiles":["checked-f64-utf16-v40"],"maxJobs":4,"maxWorkers":1,"deadlineMs":15000,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"retentionSeconds":300});
    serde_json::from_value::<ToolchainCeiling>(value.clone())
        .unwrap()
        .validate()
        .unwrap();
    for (key, bad) in [
        ("listen", serde_json::json!("0.0.0.0:8799")),
        ("maxJobs", serde_json::json!(17)),
        ("maxWorkers", serde_json::json!(0)),
        ("deadlineMs", serde_json::json!(30001)),
        ("retentionSeconds", serde_json::json!(301)),
        ("addressSpaceBytes", serde_json::json!(u64::MAX)),
    ] {
        let mut invalid = value.clone();
        invalid[key] = bad;
        assert!(
            serde_json::from_value::<ToolchainCeiling>(invalid)
                .unwrap()
                .validate()
                .is_err(),
            "{key}"
        );
    }
}

#[test]
fn application_compile_policy_is_explicit_and_local_only_stays_empty() {
    let original = fixture();
    let mut request = original.clone();
    request.application = Some(crate::application::ApplicationRequest {
        public_origin: "http://localhost:8802".into(),
        access: crate::application::Access::ReadWrite,
    });
    assert!(request.validate().is_err());
    request.dependency_policy = crate::application::DEPENDENCY_POLICY.into();
    let result = compile::compile(&request).unwrap();
    assert!(result.result.ok, "{:?}", result.result.diagnostics);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&result.files["app.manifest.json"])
        .unwrap();
    let m = varta_common::Manifest::parse(&bytes).unwrap();
    assert_eq!(
        m.grants().unwrap(),
        request.application.as_ref().unwrap().grants()
    );
    assert!(m.browser_session.is_none());
    let old = compile::compile(&original).unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&old.files["app.manifest.json"])
        .unwrap();
    assert!(
        varta_common::Manifest::parse(&bytes)
            .unwrap()
            .capabilities
            .is_empty()
    );
}
