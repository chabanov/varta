//! Preparing an application instance: the packaged artifact, its grants
//! within the host ceiling and its storage.
use super::{ShellError, storage};
use std::path::{Path, PathBuf};
use varta_artifact::{Artifact, MAX_MANIFEST_BYTES, read_bounded};
use varta_common::{CapabilityGrant, Manifest};
use varta_runtime::{Application, ApplicationOptions, RunOptions, VartaRuntime, ViewLayout};

/// The largest font file the shell reads from the command line.
pub const MAX_FONT_BYTES: usize = 4 * 1024 * 1024;

/// Where a packaged application comes from and what bounds it.
#[derive(Debug, Clone, Copy)]
pub struct Package<'a> {
    /// The packaged artifact `varta build` wrote: `app.wasm` with its
    /// `app.manifest.json`, or a fixed-footer container.
    pub artifact: &'a Path,
    /// The optional host ceiling: a host policy in the launcher's format.
    pub policy: Option<&'a Path>,
    /// The absolute host directory under which each artifact hash gets its
    /// own storage directory.
    pub storage: &'a Path,
}

/// A validated application ready to start: its portable Wasm, the resolved
/// run options and its identity (the `sha256:` digest of the Wasm).
pub struct Launch {
    wasm: Vec<u8>,
    run: RunOptions,
    identity: String,
}

impl Launch {
    /// A precompiled module with no manifest: no grants, no host ceiling, no
    /// file roots and no storage (`varta-preview`).
    #[must_use]
    pub fn unprivileged(wasm: Vec<u8>) -> Self {
        Self {
            identity: varta_runtime::artifact_identity(&wasm),
            wasm,
            run: RunOptions::default(),
        }
    }

    /// Read and validate a packaged artifact and resolve its grants.
    ///
    /// The manifest is the package request and the host ceiling, when
    /// given, narrows it: the runtime admits a concrete resource only when
    /// both have a grant for its exact `namespace.operation` id that
    /// matches it. An empty ceiling denies everything; nothing falls back to
    /// the package request. Optional grants stay denied, because this host
    /// has no prompt and reads no stored decisions. Nothing here runs guest
    /// code.
    pub fn packaged(package: &Package<'_>) -> Result<Self, ShellError> {
        let artifact_error = |source| ShellError::Artifact {
            path: package.artifact.to_path_buf(),
            source,
        };
        let artifact = Artifact::read(package.artifact).map_err(artifact_error)?;
        // Bounded portable payload, matching host ABI and package rules.
        artifact.grants().map_err(artifact_error)?;
        let ceiling = package.policy.map(read_policy).transpose()?;
        let mut run = RunOptions::from_manifests(&artifact.manifest, ceiling.as_ref())?;
        let identity = varta_runtime::artifact_identity(&artifact.wasm);
        storage::pin(
            &mut run.file_roots,
            &artifact.manifest,
            ceiling.as_ref(),
            package.storage,
            &identity,
        )?;
        Ok(Self {
            wasm: artifact.wasm,
            run,
            identity,
        })
    }

    /// The application identity stored decisions and storage are keyed by.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The package request.
    #[must_use]
    pub fn requested(&self) -> &[CapabilityGrant] {
        &self.run.requested
    }

    /// The host ceiling, when one was given (possibly empty).
    #[must_use]
    pub fn ceiling(&self) -> Option<&[CapabilityGrant]> {
        self.run.ceiling.as_deref()
    }

    /// The pinned file root names (ceiling mappings and KV storage).
    pub fn file_roots(&self) -> impl Iterator<Item = &str> {
        self.run.file_roots.names()
    }

    /// Validate the module and start its application instance with the
    /// given view layout. Validation (code, profile, contracts, required
    /// grants and budgets) completes before any guest code runs.
    pub fn start(self, layout: ViewLayout) -> Result<Application, ShellError> {
        Ok(VartaRuntime::new().start_application(
            &self.wasm,
            ApplicationOptions {
                run: self.run,
                view_layout: Some(layout),
                ..ApplicationOptions::default()
            },
        )?)
    }
}

/// A validated host ceiling file.
fn read_policy(path: &Path) -> Result<Manifest, ShellError> {
    let policy_error = |source| ShellError::Policy {
        path: path.to_path_buf(),
        source,
    };
    let manifest = Manifest::parse(&read_bounded(path, MAX_MANIFEST_BYTES).map_err(policy_error)?)
        .map_err(policy_error)?;
    manifest.validate_ceiling().map_err(policy_error)?;
    Ok(manifest)
}

/// Read a font file named on the command line.
pub fn read_font(path: &Path) -> Result<Vec<u8>, ShellError> {
    read_bounded(path, MAX_FONT_BYTES).map_err(|source| ShellError::Font {
        path: PathBuf::from(path),
        source,
    })
}
