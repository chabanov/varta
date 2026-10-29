use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use varta_artifact::Artifact;
use varta_common::{Manifest, launcher::LauncherInfo};
fn runtime() -> Command {
    Command::new(env!("CARGO_BIN_EXE_varta-runtime"))
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "varta-launcher-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn runtime_has_a_checked_identity_and_execution_only_arguments() {
    let output = runtime().arg("--runtime-info").output().unwrap();
    assert!(output.status.success());
    let info: LauncherInfo = serde_json::from_slice(&output.stdout).unwrap();
    info.validate_current().unwrap();
    let output = runtime().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("check --config"));
    for argument in ["init", "build", "check", "dev", "--unknown"] {
        assert!(
            !runtime().arg(argument).output().unwrap().status.success(),
            "{argument}"
        );
    }
    for arguments in [
        vec!["run"],
        vec!["--invoke"],
        vec!["inspect"],
        vec!["--policy"],
        vec!["--inspect", "--json", "--json"],
        vec!["--inspect", "--params", "{}"],
        vec!["--validate", "missing.wasm", "--policy", "policy.json"],
    ] {
        assert!(
            !runtime()
                .args(&arguments)
                .output()
                .unwrap()
                .status
                .success(),
            "{arguments:?}"
        );
    }
    assert!(!runtime().output().unwrap().status.success());
}
#[test]
fn runtime_executes_and_inspects_portable_artifacts_and_fixed_footer_packages() {
    let temp = Temp::new();
    let artifact = Artifact {
        wasm: b"\0asm\x01\0\0\0".to_vec(),
        manifest: Manifest::new(&[]),
    };
    let wasm = temp.0.join("empty.wasm");
    artifact.write_wasm(&wasm).unwrap();
    assert!(
        runtime()
            .arg("--validate")
            .arg(&wasm)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        runtime()
            .arg("run")
            .arg(&wasm)
            .output()
            .unwrap()
            .status
            .success()
    );
    let native = temp.0.join(format!("app{}", std::env::consts::EXE_SUFFIX));
    artifact
        .write_executable(
            &native,
            std::path::Path::new(env!("CARGO_BIN_EXE_varta-runtime")),
        )
        .unwrap();
    fs::remove_file(&wasm).unwrap();
    fs::remove_file(wasm.with_extension("manifest.json")).unwrap();
    assert!(Command::new(&native).output().unwrap().status.success());
    let output = Command::new(&native).arg("--inspect").output().unwrap();
    assert!(output.status.success());
    let inspected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(inspected["manifest"]["capabilities"], serde_json::json!([]));
    assert_eq!(inspected["module"]["memories"], 0);
    assert_eq!(
        inspected["module"]["language"],
        serde_json::json!({"profile":null,"origin":"unspecified"})
    );
    let output = runtime().arg("--validate").arg(&native).output().unwrap();
    assert!(output.status.success());
    let policy = temp.0.join("policy.json");
    fs::write(&policy, r#"{"capabilities":[]}"#).unwrap();
    assert!(
        Command::new(&native)
            .arg("--policy")
            .arg(&policy)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(&policy, "{bad").unwrap();
    assert!(
        !Command::new(&native)
            .arg("--policy")
            .arg(&policy)
            .output()
            .unwrap()
            .status
            .success()
    );
    let mut data = fs::read(&native).unwrap();
    let footer = data.len() - 32;
    data[footer..footer + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    fs::write(&native, data).unwrap();
    assert!(!Command::new(&native).output().unwrap().status.success());
}

/// K4.4 step 1: a hook-produced v33 frame module is a valid portable artifact
/// that the source-free runtime runs (without calling the dispatcher); the same
/// surface under v32 is refused by artifact validation and by the runtime
/// before any guest output.
#[test]
fn resumable_frame_artifacts_run_source_free_only_under_their_profile() {
    use varta_common::{function::frame, language::LanguageProfile};
    use varta_compiler::suspension_test_hook as hook;
    const SOURCE: &str = "function task(n: number): number {
       const kept = \"ab\".repeat(2);
       const got = __varta_suspend(n);
       return got + kept.length;
     }
     export function main(): void { console.log(\"v33 frames\"); }";
    let temp = Temp::new();
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let profile = LanguageProfile::CheckedF64Utf16V33;
    let wasm = hook::compile_with_profile(SOURCE, profile).unwrap();
    assert!(
        wasm.windows(frame::RESUME_EXPORT.len())
            .any(|window| window == frame::RESUME_EXPORT.as_bytes())
    );
    let mut manifest = Manifest::new(&[]);
    manifest.abi_version = profile.host_abi();
    let artifact = Artifact { wasm, manifest };
    let path = temp.0.join("frames.wasm");
    artifact.write_wasm(&path).unwrap();
    let read = Artifact::read(&path).unwrap();
    assert_eq!(read.wasm, artifact.wasm);
    let native = temp
        .0
        .join(format!("frames{}", std::env::consts::EXE_SUFFIX));
    artifact
        .write_executable(
            &native,
            std::path::Path::new(env!("CARGO_BIN_EXE_varta-runtime")),
        )
        .unwrap();
    for mut command in [
        {
            let mut command = runtime();
            command.arg("run").arg(&path);
            command
        },
        Command::new(&native),
    ] {
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(logs(&output), ["varta log: v33 frames"]);
    }
    let output = runtime().arg("--validate").arg(&path).output().unwrap();
    assert!(output.status.success(), "{output:?}");

    let older = LanguageProfile::CheckedF64Utf16V32;
    let wasm = hook::compile_with_profile(SOURCE, older).unwrap();
    let mut manifest = Manifest::new(&[]);
    manifest.abi_version = older.host_abi();
    let rejected = Artifact { wasm, manifest };
    let refused = temp.0.join("refused.wasm");
    assert!(rejected.write_wasm(&refused).is_err());
    assert!(!refused.exists());
    // Written around the artifact API, the payload still fails to load and
    // to run before any guest output.
    fs::write(&refused, &rejected.wasm).unwrap();
    fs::write(
        refused.with_extension("manifest.json"),
        rejected.manifest.to_bytes().unwrap(),
    )
    .unwrap();
    assert!(Artifact::read(&refused).is_err());
    let output = runtime().arg("run").arg(&refused).output().unwrap();
    assert!(!output.status.success());
    assert!(logs(&output).is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not admit resumable frames"),
        "{output:?}"
    );
}

/// V35 liveness frames (catalog version 3) are valid portable artifacts that
/// run source-free; the same bytes relabelled as v34 are refused by artifact
/// validation and by the runtime before any guest output.
#[test]
fn liveness_frame_artifacts_run_source_free_only_under_v35() {
    use varta_common::language::LanguageProfile;
    use varta_compiler::suspension_test_hook as hook;
    const SOURCE: &str = "function task(n: number): number {
       const kept = \"ab\".repeat(2);
       const got = __varta_suspend(n);
       return got + kept.length;
     }
     export function main(): void { console.log(\"v35 frames\"); }";
    let temp = Temp::new();
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let profile = LanguageProfile::CheckedF64Utf16V35;
    let wasm = hook::compile_with_profile(SOURCE, profile).unwrap();
    let mut manifest = Manifest::new(&[]);
    manifest.abi_version = profile.host_abi();
    let artifact = Artifact {
        wasm: wasm.clone(),
        manifest: manifest.clone(),
    };
    let path = temp.0.join("liveness.wasm");
    artifact.write_wasm(&path).unwrap();
    assert_eq!(Artifact::read(&path).unwrap().wasm, artifact.wasm);
    let output = runtime().arg("run").arg(&path).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(logs(&output), ["varta log: v35 frames"]);

    // V35 declares host ABI 31 and value layout 16, so a module relabelled as
    // v34 fails at its host ABI section before the frame catalog is read
    // (runtime `frame_admission_tests` rewrites both and pins the catalog
    // version refusal).
    let (from, to) = (b"checked-f64-utf16-v35", b"checked-f64-utf16-v34");
    let at = wasm
        .windows(from.len())
        .position(|window| window == from)
        .unwrap();
    let mut relabelled = wasm;
    relabelled[at..at + to.len()].copy_from_slice(to);
    let rejected = Artifact {
        wasm: relabelled,
        manifest,
    };
    let refused = temp.0.join("relabelled.wasm");
    assert!(rejected.write_wasm(&refused).is_err());
    fs::write(&refused, &rejected.wasm).unwrap();
    fs::write(
        refused.with_extension("manifest.json"),
        rejected.manifest.to_bytes().unwrap(),
    )
    .unwrap();
    assert!(Artifact::read(&refused).is_err());
    let output = runtime().arg("run").arg(&refused).output().unwrap();
    assert!(!output.status.success());
    assert!(logs(&output).is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("declared profile requires its matching host ABI section"),
        "{output:?}"
    );
}
