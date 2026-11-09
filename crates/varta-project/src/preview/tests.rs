use super::{
    bundle::{ValidationInput, validate},
    contract::*,
};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn ceiling() -> PreviewCeiling {
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"controlListen":"127.0.0.1:0","publicListen":"[::1]:0",
        "protectedOrigins":["http://127.0.0.1:8787"],"maxImports":4,"maxRuns":4,"maxRunSeconds":300,"importSeconds":60,
        "retentionSeconds":300,"validationDeadlineMs":15000,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"budgets":{}})).unwrap()
}
fn input(files: BTreeMap<String, Vec<u8>>) -> ValidationInput {
    let mut h = Sha256::new();
    h.update(b"varta-toolchain-artifact-v1\0");
    let descriptions = files
        .iter()
        .map(|(path, b)| {
            h.update((path.len() as u64).to_le_bytes());
            h.update(path.as_bytes());
            h.update((b.len() as u64).to_le_bytes());
            h.update(b);
            BundleFile {
                path: path.clone(),
                sha256: hash(b),
                bytes: b.len(),
            }
        })
        .collect();
    ValidationInput {
        application_origin: None,
        request: ImportRequest {
            schema_version: 1,
            host_sha256: "a".repeat(64),
            artifact_sha256: format!("{:x}", h.finalize()),
            files: descriptions,
        },
        files: files
            .into_iter()
            .map(|(k, b)| (k, base64::engine::general_purpose::STANDARD.encode(b)))
            .collect(),
        budgets: Default::default(),
    }
}
#[test]
fn preview_ceiling_keeps_distinct_hosts_and_production_budgets() {
    let c = ceiling();
    c.validate().unwrap();
    for bad in [
        serde_json::json!(["http://[::1]:9999"]),
        serde_json::json!([]),
        serde_json::json!(["http://127.0.0.1:80/"]),
    ] {
        let mut value=serde_json::to_value(serde_json::json!({"schemaVersion":1,"controlListen":"127.0.0.1:8801","publicListen":"[::1]:8800",
        "protectedOrigins":bad,"maxImports":4,"maxRuns":4,"maxRunSeconds":300,"importSeconds":60,"retentionSeconds":300,
        "validationDeadlineMs":15000,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"budgets":{}})).unwrap();
        assert!(
            serde_json::from_value::<PreviewCeiling>(value.clone())
                .unwrap()
                .validate()
                .is_err()
        );
        value["sourcePath"] = serde_json::json!("/tmp/source");
        assert!(serde_json::from_value::<PreviewCeiling>(value).is_err());
    }
    let mut c = c;
    c.budgets.deadline_per_turn_ms = Some(varta_common::budget::MAX_DEADLINE_PER_TURN_MS);
    assert!(c.validate().is_err());
}
#[test]
fn preview_canonical_bundle_is_validated_without_main_and_never_serves_uploaded_script() {
    let profile = varta_common::language::LanguageProfile::CheckedF64Utf16V40;
    let wasm = varta_compiler::compile_typescript_with_profile(
        "export function main():void{throw new Error('must not run');}",
        profile,
    )
    .unwrap();
    let mut manifest = varta_common::Manifest::new(&[]);
    manifest.abi_version = profile.host_abi();
    let artifact = crate::Artifact { wasm, manifest };
    let bundle = crate::browser::BrowserArtifact::prepare(&artifact, None)
        .unwrap()
        .bundle()
        .unwrap();
    let result = validate(input(bundle.files().clone())).unwrap();
    let loader = base64::engine::general_purpose::STANDARD
        .decode(&result.files["varta-host.js"])
        .unwrap();
    let loader = String::from_utf8(loader).unwrap();
    assert!(loader.contains("previewBootstrap"));
    assert!(loader.contains("permissionCeiling"));
    assert!(loader.contains("credentials: \"same-origin\""));
    let mut poisoned = bundle.files().clone();
    poisoned.insert(
        "varta-host.js".into(),
        b"fetch('https://untrusted.test')".to_vec(),
    );
    // Even completely recomputed catalog and per-file pins cannot bless custom JS.
    assert!(validate(input(poisoned)).is_err());
    let mut extra = bundle.files().clone();
    extra.insert("extra.js".into(), b"arbitrary".to_vec());
    assert!(validate(input(extra)).is_err());
    let mut corrupt = input(bundle.files().clone());
    corrupt.request.artifact_sha256 = "f".repeat(64);
    assert!(validate(corrupt).is_err());
}
#[test]
fn preview_descriptor_rejects_escaping_overflow_and_case_aliases() {
    let mut r = ImportRequest {
        schema_version: 1,
        host_sha256: "a".repeat(64),
        artifact_sha256: "b".repeat(64),
        files: vec![BundleFile {
            path: "app.wasm".into(),
            sha256: "c".repeat(64),
            bytes: 0,
        }],
    };
    r.validate().unwrap();
    for path in ["../file", "/file", ".hidden", "a//b", "a%2fb", "a\\b"] {
        r.files[0].path = path.into();
        assert!(r.validate().is_err(), "{path}");
    }
    r.files[0].path = "app.wasm".into();
    r.files[0].bytes = usize::MAX;
    assert!(r.validate().is_err());
    r.files[0].bytes = 0;
    r.files.push(BundleFile {
        path: "App.wasm".into(),
        sha256: "d".repeat(64),
        bytes: 0,
    });
    r.files.sort_by(|a, b| a.path.cmp(&b.path));
    assert!(r.validate().is_err());
}
