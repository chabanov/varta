use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn varta() -> Command {
    Command::new(env!("CARGO_BIN_EXE_varta"))
}

fn temp_project() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("varta-smoke-{nanos}"))
}

fn combined(output: &std::process::Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

#[test]
fn generated_project_smoke_test() {
    let project = temp_project();
    let _ = fs::remove_dir_all(&project);

    let init = varta()
        .arg("init")
        .arg(&project)
        .output()
        .expect("failed to run varta init");
    assert!(init.status.success(), "init failed: {}", combined(&init));

    let check = varta()
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("failed to run varta check");
    assert!(check.status.success(), "check failed: {}", combined(&check));

    let dev = varta()
        .current_dir(&project)
        .arg("dev")
        .output()
        .expect("failed to run varta dev");
    assert!(dev.status.success(), "dev failed: {}", combined(&dev));
    let dev_output = combined(&dev);
    assert!(dev_output.contains("varta log: hello from varta"));
    assert!(dev_output.contains("varta log: 42"));

    let _ = varta()
        .current_dir(&project)
        .arg("build")
        .status()
        .expect("failed to run varta build");
    let wasm = project.join("dist").join("app.wasm");
    assert!(wasm.exists(), "missing {}", wasm.display());

    let run = varta()
        .current_dir(&project)
        .arg("run")
        .arg("dist/app.wasm")
        .output()
        .expect("failed to run varta run");
    assert!(run.status.success(), "run failed: {}", combined(&run));
    let run_output = combined(&run);
    assert!(run_output.contains("varta log: hello from varta"));
    assert!(run_output.contains("varta log: 42"));

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn restricted_capability_smoke_test() {
    let project = temp_project();
    fs::create_dir_all(project.join("src")).unwrap();

    fs::write(
        project.join("varta.config.json"),
        r#"{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {}
}"#,
    )
    .unwrap();

    fs::write(
        project.join("src").join("main.ts"),
        r#"declare const varta: { env: { print(name: string): void } };

export function main(): void {
  varta.env.print("PATH");
}
"#,
    )
    .unwrap();

    let check = varta()
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("failed to run varta check");
    assert!(check.status.success(), "check failed: {}", combined(&check));

    let dev = varta()
        .current_dir(&project)
        .arg("dev")
        .output()
        .expect("failed to run varta dev");
    assert!(dev.status.success(), "dev failed: {}", combined(&dev));
    assert!(
        combined(&dev).contains("varta log: <capability denied: env>"),
        "expected capability denial, got: {}",
        combined(&dev)
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn single_binary_smoke_test() {
    let project = temp_project();
    fs::create_dir_all(project.join("src")).unwrap();

    fs::write(
        project.join("varta.config.json"),
        r#"{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {
    "time": {
      "now": true
    }
  }
}"#,
    )
    .unwrap();

    fs::write(
        project.join("src").join("main.ts"),
        r#"export function main(): void {
  console.log("hello from varta");
  console.log(varta.time.now());
}
"#,
    )
    .unwrap();

    fs::write(
        project.join("varta.d.ts"),
        r#"declare const varta: { time: { now(): number } };
"#,
    )
    .unwrap();

    let check = varta()
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("failed to run varta check");
    assert!(check.status.success(), "check failed: {}", combined(&check));

    let build = varta()
        .current_dir(&project)
        .arg("build")
        .arg("--single")
        .output()
        .expect("failed to run varta build");
    assert!(build.status.success(), "build failed: {}", combined(&build));

    let executable = project.join("dist").join("app");
    assert!(executable.exists(), "missing {}", executable.display());

    let run = Command::new(&executable)
        .current_dir(&project)
        .output()
        .expect("failed to run single binary");
    assert!(
        run.status.success(),
        "single binary failed: {}",
        combined(&run)
    );
    let output = combined(&run);
    assert!(output.contains("varta log: hello from varta"));
    assert!(
        output.lines().any(|line| {
            line.starts_with("varta log: ") && line != "varta log: hello from varta"
        })
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn manifest_validation_smoke_test() {
    let project = temp_project();
    fs::create_dir_all(project.join("src")).unwrap();

    fs::write(
        project.join("varta.config.json"),
        r#"{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {
    "time": {
      "now": true
    }
  }
}"#,
    )
    .unwrap();

    fs::write(
        project.join("src").join("main.ts"),
        r#"export function main(): void {
  console.log(varta.time.now());
}
"#,
    )
    .unwrap();

    fs::write(
        project.join("varta.d.ts"),
        r#"declare const varta: { time: { now(): number } };
"#,
    )
    .unwrap();

    let check = varta()
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("failed to run varta check");
    assert!(check.status.success(), "check failed: {}", combined(&check));

    let build = varta()
        .current_dir(&project)
        .arg("build")
        .output()
        .expect("failed to run varta build");
    assert!(build.status.success(), "build failed: {}", combined(&build));

    let validate = varta()
        .current_dir(&project)
        .arg("validate")
        .arg("dist/app.wasm")
        .output()
        .expect("failed to run varta validate");
    assert!(
        validate.status.success(),
        "validate failed: {}",
        combined(&validate)
    );
    assert!(
        combined(&validate).contains("varta validate: ok (dist/app.wasm)"),
        "expected validation success, got: {}",
        combined(&validate)
    );

    let inspect = varta()
        .current_dir(&project)
        .arg("inspect")
        .arg("dist/app.wasm")
        .arg("--json")
        .output()
        .expect("failed to run varta inspect");
    assert!(
        inspect.status.success(),
        "inspect failed: {}",
        combined(&inspect)
    );
    assert!(
        combined(&inspect).contains("\"precompiled\":false"),
        "expected raw wasm artifact metadata, got: {}",
        combined(&inspect)
    );

    let _ = fs::remove_dir_all(&project);
}
