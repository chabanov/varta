//! The `varta-shell` and `varta-preview` binaries without opening a window:
//! arguments, and refusals that happen before any window or guest code.
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use varta_artifact::Artifact;
use varta_common::Manifest;

const PROBE_WASM: &[u8] = include_bytes!("fixtures/shell-probe/probe.wasm");
const PROBE_MANIFEST: &str = include_str!("fixtures/shell-probe/probe.manifest.json");

fn shell() -> Command {
    Command::new(env!("CARGO_BIN_EXE_varta-shell"))
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "varta-shell-binary-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn shell_arguments_are_checked_without_a_cli_framework() {
    let output = shell().arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("varta-shell ARTIFACT"), "{help}");
    assert!(help.contains("--policy PATH"), "{help}");
    assert!(help.contains("Optional permissions are denied"), "{help}");
    let output = shell().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("varta-shell "));
    for arguments in [
        vec![],
        vec!["--policy", "policy.json"],
        vec!["app.wasm", "--policy"],
        vec!["app.wasm", "--font", "font.ttf"],
        vec!["app.wasm", "--decisions", "decisions.json"],
        vec!["app.wasm", "second.wasm"],
        vec!["app.wasm", "--renderer", "metal"],
        vec!["build", "--config", "varta.config.json"],
    ] {
        let output = shell().args(&arguments).output().unwrap();
        assert!(!output.status.success(), "{arguments:?}");
        assert!(
            stderr(&output).starts_with("varta-shell: "),
            "{arguments:?}"
        );
    }
}

#[test]
fn shell_refuses_invalid_packages_and_policies_before_opening_a_window() {
    let temp = Temp::new();
    let storage = temp.0.join("storage");
    let foreign = temp.0.join("foreign.bin");
    std::fs::write(&foreign, b"not a packaged artifact").unwrap();
    let output = shell()
        .arg(&foreign)
        .arg("--data-dir")
        .arg(&storage)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("packaged artifact"), "{output:?}");

    let artifact = temp.0.join("app.wasm");
    Artifact {
        wasm: PROBE_WASM.to_vec(),
        manifest: Manifest::parse(PROBE_MANIFEST.as_bytes()).unwrap(),
    }
    .write_wasm(&artifact)
    .unwrap();
    let policy = temp.0.join("policy.json");
    std::fs::write(&policy, "{bad").unwrap();
    let output = shell()
        .arg(&artifact)
        .arg("--policy")
        .arg(&policy)
        .arg("--data-dir")
        .arg(&storage)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("host policy"), "{output:?}");
    let output = shell()
        .arg(&artifact)
        .arg("--font")
        .arg(temp.0.join("missing.ttf"))
        .arg("Missing")
        .arg("--policy")
        .arg(&policy)
        .arg("--data-dir")
        .arg(&storage)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!storage.exists());
}

#[test]
fn preview_keeps_its_renderer_probe_and_usage() {
    let preview = || Command::new(env!("CARGO_BIN_EXE_varta-preview"));
    let output = preview().arg("--renderer-info").output().unwrap();
    assert!(output.status.success());
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["gpuCompiled"], cfg!(feature = "gpu"));
    let output = preview().output().unwrap();
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("usage: varta-preview"),
        "{output:?}"
    );
}
