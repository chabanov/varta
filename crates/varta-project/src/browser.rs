//! Trusted browser preparation and local development delivery. Preparation
//! validates originals before instrumentation, without guest execution or native
//! root binding; the separate development host opens only a loopback listener.
use crate::{Artifact, HostPolicy, Result};
use varta_common::{
    Manifest, WasmInspection,
    budget::{Budgets, ResolvedBudgets},
};
mod bundle;
mod dev;
pub use dev::{BrowserDevOptions, BrowserDevServer};
mod font_data;
pub use bundle::BrowserBundle;
pub(crate) mod resources;
pub use resources::BrowserResource;

/// Compile through the shared checked source pipeline, then prepare the browser
/// image before creating output directories. Build-time loading never mounts
/// native execution roots. The portable manifest/bytes retain their meaning.
pub fn build_browser(config_path: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let (config, base, compilation) = crate::compile(config_path, false)?;
    let artifact = Artifact {
        wasm: compilation.wasm,
        manifest: config.manifest(),
    };
    let resources = resources::load(&config, &base)?;
    let bundle = BrowserArtifact::prepare(&artifact, None)?.bundle_with_resources(&resources)?;
    let output = base.join(&config.out);
    // Configured output is joined to the source base, like native output.
    let parent = output
        .parent()
        .ok_or_else(|| crate::ProjectError::Usage("browser output has no parent".into()))?;
    std::fs::create_dir_all(parent)?;
    bundle.write_new(&parent.join("web"))
}

/// An owned original, validated metadata, requested/ceiling policy and its
/// resource-bounded private execution image. No caller can replace one member
/// while retaining the validation of another. This is not a portable ABI or a
/// permission decision; browser effects still require concrete all-of checks.
pub struct BrowserArtifact {
    original: Artifact,
    image: Vec<u8>,
    inspection: WasmInspection,
    budgets: ResolvedBudgets,
    ceiling: Option<Manifest>,
}

impl BrowserArtifact {
    /// Validate the artifact's manifest/ABI and the runtime's full code,
    /// catalog, helper-effect and contract rules before deriving the image.
    /// Package/ceiling validation here performs no filesystem-root binding.
    /// Unsupported meter proposals fail explicitly; no unmetered fallback.
    pub fn prepare(artifact: &Artifact, host: Option<&HostPolicy>) -> Result<Self> {
        artifact.grants()?;
        let inspection = varta_runtime::VartaRuntime::new().inspect_module(&artifact.wasm)?;
        let profile = inspection.language.profile.unwrap_or_default();
        if !profile.protected_sessions()
            && host.is_some_and(|h| h.manifest().browser_session.is_some())
        {
            return Err(crate::ProjectError::Usage(
                "browserSession requires the protected-session language profile".into(),
            ));
        }
        let budgets = if profile.turn_budgets() {
            Budgets::resolve(
                &artifact.manifest.budget_request(),
                host.and_then(|h| h.manifest().budgets).as_ref(),
            )
        } else {
            ResolvedBudgets {
                budgets: Budgets::PRODUCTION,
                capped: Vec::new(),
            }
        };
        let image = varta_compiler::browser_meter::instrument_with_memory_limit(
            &artifact.wasm,
            budgets.budgets.memory_bytes,
        )?;
        Ok(Self {
            original: Artifact {
                wasm: artifact.wasm.clone(),
                manifest: artifact.manifest.clone(),
            },
            image,
            inspection,
            budgets,
            ceiling: host.map(|host| host.manifest().clone()),
        })
    }

    #[must_use]
    pub fn original(&self) -> &Artifact {
        &self.original
    }
    #[must_use]
    pub fn execution_image(&self) -> &[u8] {
        &self.image
    }
    #[must_use]
    pub fn inspection(&self) -> &WasmInspection {
        &self.inspection
    }
    #[must_use]
    pub fn budgets(&self) -> &ResolvedBudgets {
        &self.budgets
    }
    /// None retains native's package-only mode; Some(empty) keeps an explicit
    /// empty ceiling and must never fall back to package authority.
    #[must_use]
    pub fn ceiling(&self) -> Option<&Manifest> {
        self.ceiling.as_ref()
    }
}

/// Runtime-only preview delivery. Distribution bytes remain unchanged.
pub(crate) fn preview_files(
    artifact: &Artifact,
    ceiling: varta_common::budget::BudgetRequest,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    delivery_files(artifact, ceiling, None)
}
pub(crate) fn application_files(
    artifact: &Artifact,
    ceiling: varta_common::budget::BudgetRequest,
    application: &crate::application::ApplicationRequest,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    delivery_files(artifact, ceiling, Some(application))
}
fn delivery_files(
    artifact: &Artifact,
    ceiling: varta_common::budget::BudgetRequest,
    application: Option<&crate::application::ApplicationRequest>,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    use base64::Engine;
    use sha2::Digest;
    let mut manifest = Manifest::new(&application.map_or_else(Vec::new, |a| a.grants()));
    if let Some(app) = application {
        manifest.browser_session = Some(varta_common::session::ClientBinding {
            api_origin: app.public_origin.clone(),
            client_origin: app.public_origin.clone(),
            allow_insecure_loopback: true,
        });
    }
    manifest.abi_version = artifact.manifest.abi_version;
    manifest.budgets = Some(ceiling);
    let policy = HostPolicy::from_manifest(manifest)?;
    let prepared = BrowserArtifact::prepare(artifact, Some(&policy))?;
    let mut files = prepared.development_bundle(&[])?.into_files();
    let mut loader = files
        .remove("varta-host.js")
        .ok_or_else(|| crate::ProjectError::Artifact("preview loader missing".into()))?;
    let sri = |b: &[u8]| {
        format!(
            "sha256-{}",
            base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(b))
        )
    };
    let before = sri(&loader);
    let source =
        String::from_utf8(loader).map_err(|e| crate::ProjectError::Artifact(e.to_string()))?;
    // Only the fixed verified-asset fetch uses the preview-only HttpOnly
    // cookie. Ordinary guest networking retains credentials:omit.
    const ASSET_FETCH: &str =
        "fetch(new URL(item.name, base), { signal, credentials: \"omit\", redirect: \"error\" })";
    if source.matches(ASSET_FETCH).count() != 1 {
        return Err(crate::ProjectError::Artifact(
            "preview asset boundary changed".into(),
        ));
    }
    loader = source.replace(ASSET_FETCH,
        "fetch(new URL(item.name, base), { signal, credentials: \"same-origin\", redirect: \"error\" })").into_bytes();
    if application.is_some() {
        let source =
            String::from_utf8(loader).map_err(|e| crate::ProjectError::Artifact(e.to_string()))?;
        const FRAME: &str =
            r#"lastBatch=data.id;worker.postMessage({type:"view_ack",id:data.id});"#;
        if source.matches(FRAME).count() != 1 {
            return Err(crate::ProjectError::Artifact(
                "application frame boundary changed".into(),
            ));
        }
        loader=source.replace(FRAME,r#"applicationFrame?.(data.id);lastBatch=data.id;worker.postMessage({type:"view_ack",id:data.id});"#).into_bytes();
        loader.extend_from_slice(b"\nlet applicationFrame;\n");
    }
    loader.extend_from_slice(include_bytes!("browser/preview-client.js"));
    let budgets: std::collections::BTreeMap<_, _> = varta_common::budget::BudgetKind::ALL
        .into_iter()
        .map(|k| (k.wire_name(), k.production()))
        .collect();
    let mut budgets = budgets;
    for k in varta_common::budget::BudgetKind::ALL {
        if let Some(v) = ceiling.value(k) {
            budgets.insert(k.wire_name(), v);
        }
    }
    let mut inputs = serde_json::json!({"permissionCeiling":{},"budgetCeiling":budgets,"application":true,"pollMs":crate::preview::POLL_MS});
    if application.is_some() {
        inputs["readyAfterFrame"] = serde_json::json!(
            prepared
                .inspection()
                .imports
                .iter()
                .any(|import| import.name == varta_common::HostOperation::ViewCommit.name())
        );
    }
    loader.extend_from_slice(format!("\nif(typeof document!==\"undefined\"&&[...document.querySelectorAll(\"script[data-varta-preview]\")].some(script=>script.src===import.meta.url))previewBootstrap({inputs});\n").as_bytes());
    let after = sri(&loader);
    let html = String::from_utf8(
        files
            .remove("index.html")
            .ok_or_else(|| crate::ProjectError::Artifact("preview index missing".into()))?,
    )
    .map_err(|e| crate::ProjectError::Artifact(e.to_string()))?;
    const OPTIN: &str = " data-varta-bootstrap src=";
    if html.matches(OPTIN).count() != 1 || html.matches(&before).count() != 2 {
        return Err(crate::ProjectError::Artifact(
            "preview bootstrap boundary changed".into(),
        ));
    }
    files.insert(
        "index.html".into(),
        html.replace(&before, &after)
            .replace(OPTIN, " data-varta-preview src=")
            .into_bytes(),
    );
    files.insert("varta-host.js".into(), loader);
    Ok(files)
}

#[cfg(test)]
mod kv_codec_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_engine_tests;
