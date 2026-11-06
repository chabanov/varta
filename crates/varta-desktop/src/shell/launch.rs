//! Preparing an application instance: the packaged artifact, its fonts and
//! images, its grants within the host ceiling, its stored permission
//! decisions and its storage.
use super::consent::{Consent, ConsentStore, HostPrompt};
use super::{FontSet, ShellError, storage};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use varta_artifact::{Artifact, MAX_MANIFEST_BYTES, read_bounded};
use varta_common::{CapabilityGrant, Manifest, VartaError};
use varta_runtime::{Application, ApplicationOptions, RunOptions, VartaRuntime, ViewLayout};
use varta_view::{ImageSet, PlatformEffects};

/// The largest font file the shell reads from the command line: the
/// artifact resource section's per-face limit.
pub const MAX_FONT_BYTES: usize = varta_artifact::MAX_FONT_FACE_BYTES;
/// The largest bare module the preview reads (unchanged from the preview's
/// own reader); the runtime then applies the module's profile limit.
const MAX_WASM_BYTES: usize = varta_common::abi::MAX_WASM_BYTES;
/// Why an empty font or module file is refused.
const EMPTY_FILE: &str = "empty file";

/// Where a packaged application comes from and what bounds it.
#[derive(Debug, Clone, Copy)]
pub struct Package<'a> {
    /// The packaged artifact `varta build` wrote: `app.wasm` beside
    /// `app.manifest.json` (and `app.resources` when it ships fonts), or a
    /// fixed-footer container.
    pub artifact: &'a Path,
    /// The optional host ceiling: a host policy in the launcher's format.
    pub policy: Option<&'a Path>,
    /// The absolute host directory under which each artifact hash gets its
    /// own storage directory, which also holds its permission decisions and
    /// audit log.
    pub storage: &'a Path,
    /// An explicit font (`--font`) or the host's system fonts
    /// (`--system-fonts`) for an artifact that ships no fonts; an artifact
    /// that ships fonts refuses it.
    pub font: Option<&'a FontSet>,
}

/// A validated application ready to start: its portable Wasm, its fonts,
/// the resolved run options, its identity (the `sha256:` digest of the
/// Wasm) and, for a packaged application, its consent state.
pub struct Launch {
    wasm: Vec<u8>,
    run: RunOptions,
    identity: String,
    fonts: Option<FontSet>,
    /// The artifact's decoded images; `None` for an unpackaged module.
    images: Option<Arc<ImageSet>>,
    consent: Option<Consent>,
}

/// A started application instance and, for a packaged application, the
/// consent state to [`Consent::sync`] with it between turns.
pub struct Started {
    pub application: Application,
    pub consent: Option<Consent>,
}

impl Launch {
    /// A precompiled module with no manifest: no grants, no host ceiling, no
    /// file roots, no storage and no consent state, so it is never prompted
    /// (`varta-preview`), with an optional explicit font, parsed here.
    pub fn unprivileged(wasm: Vec<u8>, font: Option<FontSet>) -> Result<Self, ShellError> {
        if let Some(font) = &font {
            validate(font)?;
        }
        Ok(Self {
            identity: varta_runtime::artifact_identity(&wasm),
            wasm,
            run: RunOptions::default(),
            fonts: font,
            images: None,
            consent: None,
        })
    }

    /// Read and validate a packaged artifact, its fonts and its grants.
    ///
    /// The artifact's resource section (bounded, digest-checked, bound to
    /// the Wasm, every face matching its declaration, every image admitted
    /// by its structure) is read with the artifact; its faces are then
    /// parsed by the text shaper and its images decoded once
    /// ([`ImageSet::decode`]). A malformed, tampered or oversized font or
    /// image fails here, before storage directories exist and before any
    /// guest code runs. The view is given the images (possibly none), so a
    /// guest `img` naming another image is refused as in the browser.
    ///
    /// The manifest is the package request and the host ceiling, when
    /// given, narrows it: the runtime admits a concrete resource only when
    /// both have a grant for its exact `namespace.operation` id that
    /// matches it. An empty ceiling denies everything; nothing falls back to
    /// the package request. The stored permission decisions of the
    /// artifact's identity are read from its host-owned
    /// [`ConsentStore`] under `storage` (none when the file does not
    /// exist; an invalid file is [`ShellError::Decisions`]) and apply at
    /// start: a stored allow never exceeds the request or the ceiling.
    /// Optional grants without a decision stay denied unless the instance
    /// is started with a prompt ([`Self::start_with`]). Nothing here runs
    /// guest code.
    ///
    /// This is where the per-artifact KV storage directories are created:
    /// after the artifact (payload bounds, host ABI, package manifest,
    /// resource section and fonts) and the ceiling are validated, but
    /// before the runtime validates the code and its language profile, which
    /// happens in [`Launch::start`] together with instantiation (the runtime
    /// compiles the module once, from the options that already carry the
    /// pinned roots). A module the runtime then refuses leaves those
    /// directories behind, empty. A caller should therefore read its cheaper
    /// inputs (fonts and other arguments) first.
    pub fn packaged(package: &Package<'_>) -> Result<Self, ShellError> {
        let artifact_error = |source| ShellError::Artifact {
            path: package.artifact.to_path_buf(),
            source,
        };
        let packaged = Artifact::read_packaged(package.artifact).map_err(artifact_error)?;
        let artifact = packaged.artifact;
        // Bounded portable payload, matching host ABI and package rules.
        artifact.grants().map_err(artifact_error)?;
        let fonts = match (FontSet::from_resources(&packaged.resources), package.font) {
            (Some(_), Some(_)) => return Err(ShellError::FontConflict),
            (Some(fonts), None) => Some(fonts),
            (None, font) => font.cloned(),
        };
        if let Some(fonts) = &fonts {
            validate(fonts)?;
        }
        let images = ImageSet::decode(
            packaged
                .resources
                .images()
                .iter()
                .map(|image| (image.name.as_str(), image.data.as_slice())),
        )
        .map_err(ShellError::Images)?;
        let ceiling = package.policy.map(read_policy).transpose()?;
        let mut run = RunOptions::from_manifests(&artifact.manifest, ceiling.as_ref())?;
        let identity = varta_runtime::artifact_identity(&artifact.wasm);
        let consent = Consent::open(ConsentStore::new(package.storage, &identity)?)?;
        run.decisions = Some(consent.decision_file());
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
            fonts,
            images: Some(Arc::new(images)),
            consent: Some(consent),
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

    /// The fonts the application is laid out and rendered with: the
    /// artifact's faces, the explicit font, or none.
    #[must_use]
    pub fn fonts(&self) -> Option<&FontSet> {
        self.fonts.as_ref()
    }

    /// The decoded images the view draws: the artifact's, or none for an
    /// unpackaged module.
    #[must_use]
    pub fn images(&self) -> Option<&ImageSet> {
        self.images.as_deref()
    }

    /// Validate the module and start its application instance with the
    /// given view layout, without a permission prompt (a headless host):
    /// required grants within the request and the ceiling are granted by
    /// policy unless a stored decision denies one, and optional grants
    /// follow their stored decisions or stay denied. Validation (code,
    /// profile, contracts, required grants and budgets) completes before
    /// any guest code runs. Granted `copy_text` and external link effects
    /// of trusted user gestures go to `effects` (the window passes the
    /// system clipboard and browser).
    pub fn start(
        self,
        layout: ViewLayout,
        effects: Arc<dyn PlatformEffects>,
    ) -> Result<Application, ShellError> {
        Ok(self.start_with(layout, effects, None)?.application)
    }

    /// [`Self::start`] with the host's permission prompt for a packaged
    /// application (an unprivileged module is never prompted). Before any
    /// guest code runs, every required grant without a stored decision is
    /// asked once (first-run approval); a refusal fails the start with
    /// `PermissionError::RequiredDenied`, and the Allow and Deny answers
    /// given so far are stored and audited even then. Optional grants
    /// without a decision are in `prompt` state: the runtime asks only for
    /// a `varta.permissions.request` of a user-gesture turn, between turns.
    /// A request that cannot be shown in full is denied without asking and
    /// reported to [`HostPrompt::unpresentable`]. The returned [`Consent`]
    /// has already stored the first-run answers and audited the start.
    pub fn start_with(
        self,
        layout: ViewLayout,
        effects: Arc<dyn PlatformEffects>,
        prompt: Option<HostPrompt>,
    ) -> Result<Started, ShellError> {
        let Self {
            wasm,
            run,
            images,
            mut consent,
            ..
        } = self;
        let layout = match images {
            Some(images) => layout.with_images(images),
            None => layout,
        };
        let prompter = match (&consent, prompt) {
            (Some(consent), Some(prompt)) => Some(consent.prompter(prompt)),
            _ => None,
        };
        let started = VartaRuntime::new().start_application(
            &wasm,
            ApplicationOptions {
                run,
                prompter,
                view_layout: Some(layout),
                platform_effects: Some(effects),
                ..ApplicationOptions::default()
            },
        );
        match started {
            Ok(mut application) => {
                if let Some(consent) = &mut consent {
                    consent.sync(&mut application)?;
                }
                Ok(Started {
                    application,
                    consent,
                })
            }
            Err(error) => {
                if let Some(consent) = &mut consent {
                    consent.refused(&error)?;
                }
                Err(error.into())
            }
        }
    }
}

/// Store a denial of `capability` for the packaged `artifact` (read and
/// validated as [`Launch::packaged`] reads it) in its host-owned consent
/// store under the absolute `storage` base: later instances start with it,
/// and a running instance of the same artifact revokes the grant for new
/// operations when its [`Consent::sync`] next rereads the file. Returns the
/// application identity.
pub fn revoke(artifact: &Path, storage: &Path, capability: &str) -> Result<String, ShellError> {
    let packaged = Artifact::read_packaged(artifact).map_err(|source| ShellError::Artifact {
        path: artifact.to_path_buf(),
        source,
    })?;
    let identity = varta_runtime::artifact_identity(&packaged.artifact.wasm);
    ConsentStore::new(storage, &identity)?.revoke(capability)?;
    Ok(identity)
}

/// Parse every face with the text shaper, as the renderer will.
fn validate(fonts: &FontSet) -> Result<(), ShellError> {
    fonts.measurer().map(drop).map_err(ShellError::Fonts)
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

/// Read a font file named on the command line: nonempty and at most
/// [`MAX_FONT_BYTES`].
pub fn read_font(path: &Path) -> Result<Vec<u8>, ShellError> {
    read_input(path, MAX_FONT_BYTES).map_err(|source| ShellError::Font {
        path: PathBuf::from(path),
        source,
    })
}

/// Read a system font file the platform named (`--system-fonts`): nonempty
/// and at most `varta_common::view::fonts::MAX_SYSTEM_FONT_FILE_BYTES`.
pub(super) fn read_system_font(path: &Path) -> Result<Vec<u8>, ShellError> {
    read_input(path, varta_common::view::fonts::MAX_SYSTEM_FONT_FILE_BYTES).map_err(|source| {
        ShellError::Font {
            path: PathBuf::from(path),
            source,
        }
    })
}

/// Read a bare module for `varta-preview`: nonempty and at most
/// `varta_common::abi::MAX_WASM_BYTES`.
pub fn read_module(path: &Path) -> Result<Vec<u8>, ShellError> {
    read_input(path, MAX_WASM_BYTES).map_err(|source| ShellError::Module {
        path: PathBuf::from(path),
        source,
    })
}

/// A bounded regular file that is not empty.
fn read_input(path: &Path, limit: usize) -> Result<Vec<u8>, VartaError> {
    let bytes = read_bounded(path, limit)?;
    if bytes.is_empty() {
        return Err(VartaError::InvalidArtifact(EMPTY_FILE.into()));
    }
    Ok(bytes)
}
