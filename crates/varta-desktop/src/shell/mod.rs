//! The native shell core shared by `varta-shell` and `varta-preview`.
//!
//! [`Launch::packaged`] reads a packaged artifact with `varta-artifact`
//! exactly as the `varta-runtime` launcher does (bounded container footer or
//! bare Wasm with its manifest, matching host ABI, package manifest rules,
//! the resource section), parses the artifact's fonts ([`FontSet`]), takes
//! its manifest as the package request, narrows it with an optional host
//! ceiling (the launcher's `--policy` file) and pins host-owned KV storage
//! per artifact hash, creating its directories (`storage.rs` says exactly
//! which and when).
//! It also opens the artifact's host-owned [`consent::Consent`] state and
//! reads its stored permission decisions.
//! [`Launch::start`] starts the application instance; the runtime validates
//! the code and its language profile before any guest effect, and
//! [`Launch::start_with`] adds the host's permission prompt. Granted guest
//! clipboard and link effects go to a [`varta_view::PlatformEffects`] sink
//! ([`NativeEffects`] in the window). No window is needed for any of this,
//! so the core is testable headlessly; [`window::run`] presents the
//! instance in a native window.
pub mod args;
pub mod consent;
mod effects;
mod fallback;
// Built on Linux; elsewhere only for its tests with the `fontconfig` feature.
#[cfg(any(target_os = "linux", all(test, feature = "fontconfig")))]
mod fontconfig;
mod fonts;
mod launch;
mod storage;
mod system_fonts;
pub mod window;

pub use effects::NativeEffects;
#[cfg(target_os = "macos")]
pub(crate) use fallback::PlatformFaces;
pub use fonts::FontSet;
pub use launch::{Launch, MAX_FONT_BYTES, Package, Started, read_font, read_module, revoke};
pub use storage::{STORAGE_DIRECTORY_PREFIX, default_storage_base, storage_directory};

use std::path::PathBuf;
use varta_common::{VartaError, files::FileRootError, permission::PermissionError};
use varta_runtime::RuntimeError;

/// Why the shell could not prepare, start or present an application.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    /// The packaged artifact is missing, malformed, tampered with or foreign.
    #[error("packaged artifact {path}: {source}")]
    Artifact {
        path: PathBuf,
        #[source]
        source: VartaError,
    },
    /// The host ceiling (policy) file is unreadable or invalid.
    #[error("host policy {path}: {source}")]
    Policy {
        path: PathBuf,
        #[source]
        source: VartaError,
    },
    /// A font file given on the command line is unreadable, empty or too
    /// large.
    #[error("font {path}: {source}")]
    Font {
        path: PathBuf,
        #[source]
        source: VartaError,
    },
    /// The text shaper refused a face of the artifact's resource section or
    /// of `--font`.
    #[error("fonts: {0}")]
    Fonts(#[source] varta_view::TextMeasureError),
    /// An image of the artifact's resource section did not decode within
    /// the view image limits.
    #[error("images: {0}")]
    Images(#[source] varta_view::ImageError),
    /// `--font` or `--system-fonts` was given for an artifact that ships its
    /// own fonts.
    #[error(
        "the artifact ships its own fonts; --font and --system-fonts apply only to artifacts without fonts"
    )]
    FontConflict,
    /// `--system-fonts` on a platform whose font lookup this build lacks.
    #[error("--system-fonts is supported on macOS and Linux only; use --font")]
    SystemFontsUnsupported,
    /// `--system-fonts` on Linux without a working fontconfig library.
    #[error("--system-fonts needs fontconfig (libfontconfig.so.1), which did not load")]
    SystemFontsUnavailable,
    /// The platform has no font for a generic family `--system-fonts` maps.
    #[error("the system has no font for the generic family {0}")]
    SystemFontMissing(&'static str),
    /// A system font file does not register the installed family that
    /// named it.
    #[error("system font {path} does not register the family {family}")]
    SystemFontFamily { path: PathBuf, family: String },
    /// A bare module given to the preview is unreadable, empty or too large.
    #[error("module {path}: {source}")]
    Module {
        path: PathBuf,
        #[source]
        source: VartaError,
    },
    /// `--renderer gpu` in a build without the `gpu` feature.
    #[error("the gpu renderer needs a build with the gpu feature; use --renderer cpu or auto")]
    GpuNotCompiled,
    /// `--renderer gpu` and the GPU backend did not initialize.
    #[error("the gpu renderer did not initialize: {0}")]
    GpuUnavailable(String),
    /// The runtime refused the module, its grants or its budgets.
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    /// A file root (a ceiling mapping or KV storage) could not be pinned.
    #[error(transparent)]
    FileRoot(#[from] FileRootError),
    /// The host-owned storage base must be an absolute directory path.
    #[error("storage directory {0} is not an absolute path")]
    StorageNotAbsolute(PathBuf),
    /// The host has no default location for application storage.
    #[error("no default storage directory; pass --data-dir")]
    NoStorageBase,
    /// A per-artifact storage directory could not be created.
    #[error("storage directory {path}: {source}")]
    Storage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The host-owned permission decision file is unreadable, malformed,
    /// oversized or belongs to another application identity.
    #[error("permission decisions {path}: {source}")]
    Decisions {
        path: PathBuf,
        #[source]
        source: PermissionError,
    },
    /// `--revoke` named no registered capability.
    #[error("unknown capability {0:?}")]
    UnknownCapability(String),
    /// The application instance failed after it started.
    #[error("application failed: {0}")]
    Application(String),
    /// The native window or its event loop failed.
    #[error("window: {0}")]
    Window(String),
}
