//! The `varta-shell` and `varta-preview` binaries without opening a window:
//! arguments, and refusals that happen before any window or guest code.
mod support;

use std::path::Path;
use std::process::{Command, Output};
use support::{Temp, consent_probe, probe, view_probe};
use varta_desktop::shell::MAX_FONT_BYTES;

const SHELL_LABEL: &str = "varta-shell: ";
const PREVIEW_LABEL: &str = "varta-preview: ";

fn shell() -> Command {
    Command::new(env!("CARGO_BIN_EXE_varta-shell"))
}
fn preview() -> Command {
    Command::new(env!("CARGO_BIN_EXE_varta-preview"))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The probe packaged as `app.wasm` with its built manifest, which requests
/// KV: preparing it without a ceiling creates its storage directories.
fn packaged_probe(temp: &Temp) -> std::path::PathBuf {
    let artifact = temp.path("app.wasm");
    probe().write_wasm(&artifact).unwrap();
    artifact
}

#[test]
fn shell_arguments_are_checked_without_a_cli_framework() {
    let output = shell().arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("varta-shell ARTIFACT"), "{help}");
    assert!(help.contains("--policy PATH"), "{help}");
    assert!(help.contains("--revoke CAPABILITY"), "{help}");
    assert!(help.contains("asked once"), "{help}");
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
        assert!(stderr(&output).starts_with(SHELL_LABEL), "{arguments:?}");
    }
}

#[test]
fn shell_revoke_stores_a_denial_for_the_artifact_identity_without_a_window() {
    let temp = Temp::new("varta-shell-binary-test");
    let storage = temp.storage();
    let artifact = temp.path("consent.wasm");
    consent_probe().write_wasm(&artifact).unwrap();
    let identity = varta_runtime::artifact_identity(&consent_probe().wasm);
    let output = shell()
        .arg(&artifact)
        .arg("--revoke")
        .arg("kv.read")
        .arg("--data-dir")
        .arg(&storage)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("revoked kv.read for {identity}")),
        "{stdout}"
    );
    let store = varta_desktop::shell::consent::ConsentStore::new(&storage, &identity).unwrap();
    assert_eq!(
        store.load().unwrap().get("kv.read"),
        Some(&varta_common::permission::StoredDecision::Deny)
    );
    // Unknown capabilities and run options are refused; nothing changes.
    for arguments in [
        vec!["--revoke", "kv.everything"],
        vec!["--revoke", "kv.read", "--renderer", "cpu"],
    ] {
        let output = shell()
            .arg(&artifact)
            .args(&arguments)
            .arg("--data-dir")
            .arg(&storage)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{arguments:?}");
        assert!(stderr(&output).starts_with(SHELL_LABEL), "{arguments:?}");
    }
    assert_eq!(store.load().unwrap().len(), 1);
}

#[test]
fn shell_refuses_invalid_packages_and_policies_before_opening_a_window() {
    let temp = Temp::new("varta-shell-binary-test");
    let storage = temp.storage();
    let foreign = temp.path("foreign.bin");
    std::fs::write(&foreign, b"not a packaged artifact").unwrap();
    let output = shell()
        .arg(&foreign)
        .arg("--data-dir")
        .arg(&storage)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("packaged artifact"), "{output:?}");

    let artifact = packaged_probe(&temp);
    let policy = temp.path("policy.json");
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
    assert!(!storage.exists());
}

#[test]
fn a_bad_font_is_refused_before_any_storage_is_created() {
    let temp = Temp::new("varta-shell-binary-test");
    let storage = temp.storage();
    // No policy: the package request alone would create KV storage, so the
    // font is the only failure.
    let artifact = packaged_probe(&temp);
    let empty = temp.path("empty.ttf");
    std::fs::write(&empty, b"").unwrap();
    let oversized = temp.path("oversized.ttf");
    std::fs::write(&oversized, vec![0; MAX_FONT_BYTES + 1]).unwrap();
    for font in [temp.path("missing.ttf"), empty, oversized] {
        let output = shell()
            .arg(&artifact)
            .arg("--font")
            .arg(&font)
            .arg("Fixture")
            .arg("--data-dir")
            .arg(&storage)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{font:?}");
        let message = stderr(&output);
        assert!(
            message.starts_with(&format!("{SHELL_LABEL}font {}", font.display())),
            "{font:?}: {message}"
        );
        assert!(!storage.exists(), "{font:?}");
    }
}

#[test]
fn fonts_are_parsed_and_conflicts_refused_before_any_storage_is_created() {
    let temp = Temp::new("varta-shell-binary-test");
    let storage = temp.storage();
    // A file that is bounded and nonempty but not a font: the text shaper
    // refuses it before the probe's KV storage would be created.
    let artifact = packaged_probe(&temp);
    let malformed = temp.path("malformed.ttf");
    std::fs::write(&malformed, b"bounded, nonempty and not a font").unwrap();
    let run = |artifact: &Path, font: &Path| {
        shell()
            .arg(artifact)
            .arg("--font")
            .arg(font)
            .arg("Fixture")
            .arg("--data-dir")
            .arg(&storage)
            .output()
            .unwrap()
    };
    let output = run(&artifact, &malformed);
    assert!(!output.status.success());
    assert!(
        stderr(&output).starts_with(&format!("{SHELL_LABEL}fonts: ")),
        "{output:?}"
    );
    assert!(!storage.exists());
    // An artifact that ships its fonts refuses `--font`.
    let fonts = temp.path("fonts.wasm");
    let built = view_probe();
    built
        .artifact
        .write_wasm_with(&fonts, &built.resources)
        .unwrap();
    let face = temp.path("face.ttf");
    std::fs::write(&face, &built.resources.fonts()[0].data).unwrap();
    let output = run(&fonts, &face);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("ships its own fonts"),
        "{output:?}"
    );
    assert!(!storage.exists());
}

#[test]
fn an_explicit_gpu_renderer_is_refused_without_the_gpu_feature() {
    if cfg!(feature = "gpu") {
        // A GPU build opens a window here; its initialization failure is
        // covered by the window host's unit tests.
        return;
    }
    let temp = Temp::new("varta-shell-binary-test");
    let storage = temp.storage();
    let artifact = packaged_probe(&temp);
    let output = shell()
        .arg(&artifact)
        .arg("--renderer")
        .arg("gpu")
        .arg("--data-dir")
        .arg(&storage)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.starts_with(SHELL_LABEL) && message.contains("gpu feature"),
        "{message}"
    );
    assert!(!storage.exists());
}

#[test]
fn preview_keeps_its_renderer_probe_and_usage() {
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

#[test]
fn preview_refuses_empty_module_and_font_files_before_opening_a_window() {
    let temp = Temp::new("varta-preview-binary-test");
    let empty = temp.path("empty.bin");
    std::fs::write(&empty, b"").unwrap();
    let module = temp.path("app.wasm");
    probe().write_wasm(&module).unwrap();
    let refusal = |arguments: &[&Path], expected: &str| {
        let output = preview().args(arguments).arg("Fixture").output().unwrap();
        assert!(!output.status.success(), "{arguments:?}");
        let message = stderr(&output);
        assert!(
            message.starts_with(&format!("{PREVIEW_LABEL}{expected} {}", empty.display())),
            "{arguments:?}: {message}"
        );
        assert!(message.contains("empty file"), "{arguments:?}: {message}");
    };
    // The module is read first, then the font; both before any window.
    refusal(&[&empty, &empty], "module");
    refusal(&[&module, &empty], "font");
}
