//! The shell probe, built at test time.
//!
//! `varta-desktop` never depends on the compiler, SWC, project tooling or
//! the CLI, not even as a dev-dependency
//! (`crates/varta-project/tests/boundaries.rs`). So, as an outside user
//! would, the tests run the workspace's `varta` binary as a separate
//! process: Cargo builds it (offline and locked, into the same target
//! directory) and `varta build` compiles `fixtures/shell-probe`,
//! `fixtures/consent-probe`, `fixtures/view-probe` with its declared
//! fonts and `fixtures/image-probe` with its declared image, into portable
//! v38 artifacts in temporary directories. Each test then writes
//! its own packaged artifact with `varta_artifact::Artifact`, as the
//! `varta-launcher` tests do, with the manifest (the package request) it
//! needs.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use varta_artifact::{Artifact, PackagedArtifact};

/// The workspace package and binary that compile TypeScript.
const CLI_PACKAGE: &str = "varta";
const CLI_BINARY: &str = "varta";
/// The probe's sources and project configuration, beside this module.
const PROBE_DIRECTORY: &str = "tests/fixtures/shell-probe";
const PROBE_SOURCES: [&str; 2] = ["main.ts", "varta.config.json"];
const PROBE_CONFIG: &str = "varta.config.json";
/// The artifact the probe's configuration writes (`"out": "probe"`).
const PROBE_ARTIFACT: &str = "probe.wasm";
/// Cargo's JSON message for a built target.
const ARTIFACT_MESSAGE: &str = "compiler-artifact";
/// The storage base's name inside a test's temporary directory.
const STORAGE_DIRECTORY: &str = "storage";

/// A temporary directory removed on drop.
pub struct Temp(PathBuf);
impl Temp {
    pub fn new(prefix: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    /// The host-owned storage base of this test (not created up front).
    pub fn storage(&self) -> PathBuf {
        self.path(STORAGE_DIRECTORY)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The view probe's sources and configuration: text in the artifact's
/// faces, a copy button and an external link.
const VIEW_PROBE_DIRECTORY: &str = "tests/fixtures/view-probe";
/// The faces the view probe declares, copied from the view crate's
/// generated fixtures into the probe's `fonts` directory.
const VIEW_PROBE_FONTS: [(&str, &str); 4] = [
    ("regular.ttf", "advance-400.ttf"),
    ("bold.ttf", "bold-900.ttf"),
    ("italic.ttf", "italic-900.ttf"),
    ("mono.ttf", "advance-900.ttf"),
];
const FONT_FIXTURES: &str = "../varta-view/tests/fixtures";
const VIEW_PROBE_FONT_DIRECTORY: &str = "fonts";

/// The image probe's sources and configuration: one packaged PNG image.
const IMAGE_PROBE_DIRECTORY: &str = "tests/fixtures/image-probe";
const IMAGE_PROBE_IMAGE_DIRECTORY: &str = "images";
const IMAGE_PROBE_IMAGE: &str = "red.png";
/// A valid 1x1 opaque red RGBA PNG, the image probe's `icons/red.png`.
pub const RED_PNG: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The consent probe's sources and configuration: a required `kv.write`,
/// an optional `kv.read` and buttons that request it, and an optional
/// any-HTTPS `ui.open_url` with a link longer than a native prompt shows.
const CONSENT_PROBE_DIRECTORY: &str = "tests/fixtures/consent-probe";

/// The probe artifact as `varta build` wrote it (Wasm and manifest), built
/// once per test binary.
pub fn probe() -> &'static Artifact {
    static PROBE: OnceLock<Artifact> = OnceLock::new();
    PROBE.get_or_init(|| build(PROBE_DIRECTORY, (VIEW_PROBE_FONT_DIRECTORY, &[])).artifact)
}

/// The view probe as `varta build` wrote it: Wasm, manifest and the
/// resource section of its four declared faces, built once per test binary.
pub fn view_probe() -> &'static PackagedArtifact {
    static PROBE: OnceLock<PackagedArtifact> = OnceLock::new();
    PROBE.get_or_init(|| {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join(FONT_FIXTURES);
        let fonts: Vec<_> = VIEW_PROBE_FONTS
            .iter()
            .map(|(name, fixture)| (*name, std::fs::read(fixtures.join(fixture)).unwrap()))
            .collect();
        build(VIEW_PROBE_DIRECTORY, (VIEW_PROBE_FONT_DIRECTORY, &fonts))
    })
}

/// The image probe as `varta build` wrote it: Wasm, manifest and the
/// resource section of its one image, built once per test binary.
pub fn image_probe() -> &'static PackagedArtifact {
    static PROBE: OnceLock<PackagedArtifact> = OnceLock::new();
    PROBE.get_or_init(|| {
        build(
            IMAGE_PROBE_DIRECTORY,
            (
                IMAGE_PROBE_IMAGE_DIRECTORY,
                &[(IMAGE_PROBE_IMAGE, RED_PNG.to_vec())],
            ),
        )
    })
}

/// The consent probe as `varta build` wrote it (Wasm and a manifest with
/// `required` and `purpose`), built once per test binary.
pub fn consent_probe() -> &'static Artifact {
    static PROBE: OnceLock<Artifact> = OnceLock::new();
    PROBE.get_or_init(|| build(CONSENT_PROBE_DIRECTORY, (VIEW_PROBE_FONT_DIRECTORY, &[])).artifact)
}

/// Copy a fixture project (and `files` into its `subdirectory`), build it
/// with `varta build` and read the artifact it wrote.
fn build(directory: &str, (subdirectory, files): (&str, &[(&str, Vec<u8>)])) -> PackagedArtifact {
    let temp = Temp::new("varta-shell-probe-build");
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join(directory);
    for source in PROBE_SOURCES {
        std::fs::copy(sources.join(source), temp.path(source)).unwrap();
    }
    if !files.is_empty() {
        let directory = temp.path(subdirectory);
        std::fs::create_dir(&directory).unwrap();
        for (name, bytes) in files {
            std::fs::write(directory.join(name), bytes).unwrap();
        }
    }
    let output = Command::new(varta_binary())
        .arg("build")
        .arg("--config")
        .arg(temp.path(PROBE_CONFIG))
        .output()
        .unwrap();
    assert!(output.status.success(), "varta build failed: {output:?}");
    Artifact::read_packaged(&temp.path(PROBE_ARTIFACT)).unwrap()
}

/// Build the workspace's `varta` binary and return its path.
fn varta_binary() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "build",
            "--package",
            CLI_PACKAGE,
            "--bin",
            CLI_BINARY,
            "--locked",
            "--offline",
            "--message-format",
            "json-render-diagnostics",
            "--manifest-path",
        ])
        .arg(workspace)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "building {CLI_BINARY} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| {
            message["reason"] == ARTIFACT_MESSAGE && message["target"]["name"] == CLI_BINARY
        })
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .unwrap_or_else(|| panic!("cargo reported no {CLI_BINARY} executable"))
}
