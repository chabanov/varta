use super::contract::*;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write, sync::Arc};
use varta_common::{Manifest, http};
use varta_runtime::{RunOptions, RuntimeEvent, VartaRuntime};

pub(super) fn admit(input: &WorkerInput) -> Result<(crate::Artifact, Manifest)> {
    input
        .budgets
        .validate()
        .map_err(|_| ExecutionError::Invalid("budgets"))?;
    if varta_common::budget::BudgetKind::ALL
        .into_iter()
        .any(|k| input.budgets.value(k).is_some_and(|v| v > k.production()))
    {
        return Err(ExecutionError::Invalid("budgets above production"));
    }

    if !import_valid(&input.request) || input.files.len() != 2 {
        return Err(ExecutionError::Invalid("artifact files"));
    }
    let mut catalog = Sha256::new();
    catalog.update(b"varta-toolchain-artifact-v1\0");
    let mut files = BTreeMap::new();
    let mut total = 0;
    for f in &input.request.files {
        let encoded = input
            .files
            .get(&f.path)
            .ok_or(ExecutionError::Invalid("missing file"))?;
        let data = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| ExecutionError::Invalid("encoding"))?;
        total += data.len();
        if total > MAX_BYTES || data.len() != f.bytes || hash(&data) != f.sha256 {
            return Err(ExecutionError::Invalid("file integrity"));
        }
        catalog.update((f.path.len() as u64).to_le_bytes());
        catalog.update(f.path.as_bytes());
        catalog.update((data.len() as u64).to_le_bytes());
        catalog.update(&data);
        files.insert(f.path.clone(), data);
    }
    if format!("{:x}", catalog.finalize()) != input.request.artifact_sha256 {
        return Err(ExecutionError::Invalid("catalog integrity"));
    }
    let bytes = &files["app.manifest.json"];
    if bytes.len() > varta_common::MAX_MANIFEST_BYTES {
        return Err(ExecutionError::Invalid("manifest size"));
    }
    let manifest = Manifest::parse(bytes).map_err(|e| ExecutionError::Artifact(e.to_string()))?;
    let artifact = crate::Artifact {
        wasm: files
            .remove("app.wasm")
            .ok_or(ExecutionError::Invalid("wasm"))?,
        manifest,
    };
    artifact
        .grants()
        .map_err(|e| ExecutionError::Artifact(e.to_string()))?;
    let inspected = VartaRuntime::new()
        .inspect_module(&artifact.wasm)
        .map_err(|e| ExecutionError::Artifact(e.to_string()))?;
    if inspected.language.profile
        != Some(varta_common::language::LanguageProfile::CheckedF64Utf16V40)
    {
        return Err(ExecutionError::Invalid("profile"));
    }
    let service = inspected
        .http_service
        .ok_or(ExecutionError::Invalid("HTTP service required"))?;
    if service.version != http::UTF16_VERSION
        || !matches!(service.bind.as_str(), "127.0.0.1:0" | "[::1]:0")
        || service.required_effects.iter().any(|s| s != "net.listen")
        || artifact.manifest.capabilities.len() != 1
        || artifact.manifest.fs_roots.is_some()
        || artifact.manifest.http.is_some()
        || artifact.manifest.browser_session.is_some()
    {
        return Err(ExecutionError::Invalid("loopback service authority"));
    }
    let grant = &artifact.manifest.capabilities[0];
    if grant.id != "net.listen"
        || grant.patterns.as_deref() != Some(std::slice::from_ref(&service.bind))
        || grant.required == Some(false)
    {
        return Err(ExecutionError::Invalid("exact listen request"));
    }
    let mut ceiling: Manifest = serde_json::from_value(
        serde_json::json!({"abiVersion":artifact.manifest.abi_version,"capabilities":[{"id":"net.listen","patterns":[service.bind]}]}),
    )?;
    ceiling.budgets = Some(input.budgets);
    ceiling.http = Some(http::HttpPolicy {
        delegation: None,
        limits: Some(http::HostLimits {
            workers: Some(1),
            ..Default::default()
        }),
        tls: None,
        session: None,
    });
    RunOptions::from_manifests(&artifact.manifest, Some(&ceiling))
        .map_err(|e| ExecutionError::Artifact(e.to_string()))?;
    Ok((artifact, ceiling))
}
fn emit(event: &WorkerEvent) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, event)?;
    out.write_all(b"\n")?;
    out.flush()
}
/// Hidden fixed execution entry point. No source loader or shell is consulted.
pub fn run_execution_worker(
    address_space_bytes: u64,
    cpu_seconds: u64,
    validate_only: bool,
) -> Result<()> {
    if !(crate::toolchain::MIN_ADDRESS_SPACE_BYTES..=crate::toolchain::MAX_ADDRESS_SPACE_BYTES)
        .contains(&address_space_bytes)
        || !(1..=30).contains(&cpu_seconds)
    {
        return Err(ExecutionError::Invalid("worker limits"));
    }
    crate::toolchain::worker::limits(address_space_bytes, cpu_seconds)?;
    let data = crate::toolchain::worker::read_input(MAX_IPC)?;
    let input: WorkerInput = serde_json::from_slice(&data)?;
    let (artifact, ceiling) = admit(&input)?;
    if validate_only {
        emit(&WorkerEvent::Validated)?;
        return Ok(());
    }
    let mut options = RunOptions::from_manifests(&artifact.manifest, Some(&ceiling))
        .map_err(|e| ExecutionError::Artifact(e.to_string()))?;
    options.observer = Some(Arc::new(|event| {
        if let RuntimeEvent::Listening { address, .. } = event {
            let _ = emit(&WorkerEvent::Listening { address });
        }
    }));
    let report = VartaRuntime::new().run_with(&artifact.wasm, options);
    emit(&WorkerEvent::Exited {
        ok: report.failure.is_none(),
        exit_code: report.output.exit_code,
    })?;
    Ok(())
}
