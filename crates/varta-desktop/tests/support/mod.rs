//! The shell probe, built at test time.
//!
//! `varta-desktop` never depends on the compiler, SWC, project tooling or
//! the CLI, not even as a dev-dependency
//! (`crates/varta-project/tests/boundaries.rs`). So, as an outside user
//! would, the tests run the workspace's `varta` binary as a separate
//! process: Cargo builds it (offline and locked, into the same target
//! directory) and `varta build` compiles `fixtures/shell-probe` into a
//! portable v38 artifact in a temporary directory. Each test then writes
//! its own packaged artifact with `varta_artifact::Artifact`, as the
//! `varta-launcher` tests do, with the manifest (the package request) it
//! needs.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use varta_artifact::Artifact;

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

/// The probe artifact as `varta build` wrote it (Wasm and manifest), built
/// once per test binary.
pub fn probe() -> &'static Artifact {
    static PROBE: OnceLock<Artifact> = OnceLock::new();
    PROBE.get_or_init(build_probe)
}

fn build_probe() -> Artifact {
    let temp = Temp::new("varta-shell-probe-build");
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join(PROBE_DIRECTORY);
    for source in PROBE_SOURCES {
        std::fs::copy(sources.join(source), temp.path(source)).unwrap();
    }
    let output = Command::new(varta_binary())
        .arg("build")
        .arg("--config")
        .arg(temp.path(PROBE_CONFIG))
        .output()
        .unwrap();
    assert!(output.status.success(), "varta build failed: {output:?}");
    Artifact::read(&temp.path(PROBE_ARTIFACT)).unwrap()
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
