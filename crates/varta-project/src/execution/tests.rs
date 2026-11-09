use super::{contract::*, worker::admit};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub(super) fn ceiling() -> ExecutionCeiling {
    serde_json::from_value(serde_json::json!({"schemaVersion":1,"listen":"127.0.0.1:0","maxRuns":2,"maxImports":4,"maxRunSeconds":300,"importSeconds":60,"retentionSeconds":300,"startupDeadlineMs":15000,"validationDeadlineMs":15000,"cpuSeconds":30,"addressSpaceBytes":8589934592u64,"budgets":{}})).unwrap()
}
fn input(files: BTreeMap<String, Vec<u8>>) -> WorkerInput {
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
    WorkerInput {
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
fn execution_ceiling_and_descriptor_never_expand_authority() {
    let c = ceiling();
    c.validate().unwrap();
    let mut bad = c.clone();
    bad.listen = "0.0.0.0:8803".parse().unwrap();
    assert!(bad.validate().is_err());
    bad = c.clone();
    bad.max_runs = 3;
    assert!(bad.validate().is_err());
    bad = c;
    bad.budgets.deadline_per_turn_ms = Some(varta_common::budget::MAX_DEADLINE_PER_TURN_MS);
    assert!(bad.validate().is_err());
    let mut request = input(BTreeMap::from([("app.wasm".into(), vec![0])])).request;
    assert!(!import_valid(&request));
    request.files[0].bytes = usize::MAX;
    assert!(!import_valid(&request));
}
#[test]
fn execution_admission_hashes_full_native_service_without_running_main() {
    let profile = varta_common::language::LanguageProfile::CheckedF64Utf16V40;
    let source = r#"function handle(r:VartaHttpRequest):VartaHttpResponse{return {status:200,body:"ok",headers:[]};}varta.http.serve({hostname:"127.0.0.1",port:0},handle);export function main():void{throw new Error("never run at admission");}"#;
    let wasm = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
    let manifest=serde_json::to_vec(&serde_json::json!({"abiVersion":profile.host_abi(),"capabilities":[{"id":"net.listen","patterns":["127.0.0.1:0"]}]})).unwrap();
    let files = BTreeMap::from([
        ("app.manifest.json".into(), manifest),
        ("app.wasm".into(), wasm),
    ]);
    admit(&input(files.clone())).unwrap();
    let mut corrupt = input(files.clone());
    corrupt.request.artifact_sha256 = "f".repeat(64);
    assert!(admit(&corrupt).is_err());
    let mut broader = files.clone();
    broader.insert("app.manifest.json".into(),serde_json::to_vec(&serde_json::json!({"abiVersion":profile.host_abi(),"capabilities":[{"id":"net.listen","patterns":["127.0.0.1:*"]}]})).unwrap());
    assert!(admit(&input(broader)).is_err());
    let mut extra = files;
    extra.insert("source.ts".into(), source.as_bytes().to_vec());
    assert!(admit(&input(extra)).is_err());
}
