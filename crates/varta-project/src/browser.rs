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

#[cfg(test)]
mod kv_codec_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_engine_tests;
