use super::contract::*;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write};

#[derive(Deserialize, Serialize)]
pub(super) struct ValidationInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_origin: Option<String>,
    pub request: ImportRequest,
    pub files: BTreeMap<String, String>,
    pub budgets: varta_common::budget::BudgetRequest,
}
#[derive(Deserialize, Serialize)]
pub(super) struct ValidationOutput {
    pub files: BTreeMap<String, String>,
}

pub(super) fn validate(input: ValidationInput) -> Result<ValidationOutput> {
    validate_mode(input, false)
}
fn validate_mode(input: ValidationInput, application: bool) -> Result<ValidationOutput> {
    if input.application_origin.is_some() && !application {
        return Err(PreviewError::Invalid(
            "preview cannot admit application authority",
        ));
    }
    input.request.validate()?;
    if input.files.len() != input.request.files.len() {
        return Err(PreviewError::Invalid("file set"));
    }
    let mut files = BTreeMap::new();
    let mut catalog = Sha256::new();
    catalog.update(b"varta-toolchain-artifact-v1\0");
    let mut total = 0;
    for f in &input.request.files {
        let encoded = input
            .files
            .get(&f.path)
            .ok_or(PreviewError::Invalid("missing file"))?;
        if encoded.len() > MAX_WORKER_INPUT {
            return Err(PreviewError::Invalid("encoding size"));
        }
        let b = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| PreviewError::Invalid("encoding"))?;
        total += b.len();
        if total > MAX_BYTES || b.len() != f.bytes || hash(&b) != f.sha256 {
            return Err(PreviewError::Invalid("file integrity"));
        }
        catalog.update((f.path.len() as u64).to_le_bytes());
        catalog.update(f.path.as_bytes());
        catalog.update((b.len() as u64).to_le_bytes());
        catalog.update(&b);
        files.insert(f.path.clone(), b);
    }
    if format!("{:x}", catalog.finalize()) != input.request.artifact_sha256 {
        return Err(PreviewError::Invalid("catalog integrity"));
    }
    let manifest = files
        .get("app.manifest.json")
        .ok_or(PreviewError::Invalid("manifest"))?;
    if manifest.len() > varta_common::MAX_MANIFEST_BYTES {
        return Err(PreviewError::Invalid("manifest size"));
    }
    let manifest = varta_common::Manifest::parse(manifest)
        .map_err(|e| PreviewError::Artifact(e.to_string()))?;
    let application = match &input.application_origin {
        Some(origin) => Some(crate::application::artifact_request(&manifest, origin)?),
        None => {
            if !manifest.capabilities.is_empty()
                || manifest.fs_roots.is_some()
                || manifest.http.is_some()
                || manifest.browser_session.is_some()
            {
                return Err(PreviewError::Invalid("deny-all authority"));
            }
            None
        }
    };
    let artifact = crate::Artifact {
        wasm: files
            .get("app.wasm")
            .ok_or(PreviewError::Invalid("original"))?
            .clone(),
        manifest,
    };
    let prepared = crate::browser::BrowserArtifact::prepare(&artifact, None)
        .map_err(|e| PreviewError::Artifact(e.to_string()))?;
    if prepared.inspection().language.profile
        != Some(varta_common::language::LanguageProfile::CheckedF64Utf16V40)
    {
        return Err(PreviewError::Invalid("profile"));
    }
    let canonical = prepared
        .bundle()
        .map_err(|e| PreviewError::Artifact(e.to_string()))?;
    if canonical.files() != &files {
        return Err(PreviewError::Invalid("noncanonical browser runtime"));
    }
    let runtime = match application {
        Some(app) => crate::browser::application_files(&artifact, input.budgets, &app),
        None => crate::browser::preview_files(&artifact, input.budgets),
    }
    .map_err(|e| PreviewError::Artifact(e.to_string()))?;
    if runtime.values().map(Vec::len).sum::<usize>() > MAX_BYTES + MAX_JSON_BYTES {
        return Err(PreviewError::Invalid("runtime size"));
    }
    Ok(ValidationOutput {
        files: runtime
            .into_iter()
            .map(|(k, b)| (k, base64::engine::general_purpose::STANDARD.encode(b)))
            .collect(),
    })
}
/// Hidden trusted validation worker. It never executes guest initialization/main.
pub fn run_validation_worker(address_space_bytes: u64, cpu_seconds: u64) -> Result<()> {
    run_worker(address_space_bytes, cpu_seconds, false)
}
pub fn run_application_validation_worker(address_space_bytes: u64, cpu_seconds: u64) -> Result<()> {
    run_worker(address_space_bytes, cpu_seconds, true)
}
fn run_worker(address_space_bytes: u64, cpu_seconds: u64, application: bool) -> Result<()> {
    if !(crate::toolchain::MIN_ADDRESS_SPACE_BYTES..=crate::toolchain::MAX_ADDRESS_SPACE_BYTES)
        .contains(&address_space_bytes)
        || !(1..=crate::toolchain::MAX_CPU_SECONDS).contains(&cpu_seconds)
    {
        return Err(PreviewError::Invalid("worker limits"));
    }
    crate::toolchain::worker::limits(address_space_bytes, cpu_seconds)?;
    let b = crate::toolchain::worker::read_input(MAX_WORKER_INPUT)?;
    let input = serde_json::from_slice(&b)?;
    let result = if application {
        validate_mode(input, true)?
    } else {
        validate(input)?
    };
    let b = serde_json::to_vec(&result)?;
    if b.len() > MAX_WORKER_OUTPUT {
        return Err(PreviewError::Invalid("worker output"));
    }
    std::io::stdout().write_all(&b)?;
    Ok(())
}
