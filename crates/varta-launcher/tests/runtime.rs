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
