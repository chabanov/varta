use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn varta() -> Command {
    Command::new(env!("CARGO_BIN_EXE_varta"))
}

fn temp_project() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let thread = std::thread::current().id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("varta-smoke-{nanos}-{counter}-{thread:?}"))
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

#[test]
fn inspect_json_smoke_test() {
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
    let output = combined(&inspect);
    assert!(output.contains("\"precompiled\":false"));
    assert!(output.contains("\"capabilities\""));
    assert!(output.contains("time.now"));

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn invalid_capability_pattern_smoke_test() {
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
    "fs": {
      "read": ["*"]
    }
  }
}"#,
    )
    .unwrap();

    fs::write(
        project.join("src").join("main.ts"),
        "export function main(): void {\n  console.log(\"hello\");\n}\n",
    )
    .unwrap();

    let build = varta()
        .current_dir(&project)
        .arg("build")
        .output()
        .expect("failed to run varta build");
    assert!(
        !build.status.success(),
        "broad capability should fail validation"
    );
    assert!(
        combined(&build).contains("unsafe pattern for fs.read: *"),
        "expected capability pattern error, got: {}",
        combined(&build)
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn missing_capability_validation_smoke_test() {
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
        !validate.status.success(),
        "missing capability should fail validation"
    );
    assert!(
        combined(&validate).contains("requires capability namespace: time.now"),
        "expected capability mismatch, got: {}",
        combined(&validate)
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn net_get_validation_smoke_test() {
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
    "net": {
      "fetch": ["http://127.0.0.1/**"],
      "get": ["http://127.0.0.1/**"]
    }
  }
}"#,
    )
    .unwrap();

    fs::write(
        project.join("src").join("main.ts"),
        r#"export function main(): void {
  varta.net.get("http://127.0.0.1:9/unreachable");
}
"#,
    )
    .unwrap();

    fs::write(
        project.join("varta.d.ts"),
        r#"declare const varta: { net: { get(url: string): string } };
"#,
    )
    .unwrap();

    let build = varta()
        .current_dir(&project)
        .arg("build")
        .output()
        .expect("failed to run varta build");
    assert!(build.status.success(), "build failed: {}", combined(&build));

    // The net_get host import must be recognized and covered by the
    // net capability grants.
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

    let dev = varta()
        .current_dir(&project)
        .arg("dev")
        .output()
        .expect("failed to run varta dev");
    assert!(dev.status.success(), "dev failed: {}", combined(&dev));

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn typed_host_declarations_smoke_test() {
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
    },
    "env": {
      "has": ["PATH"],
      "print": ["PATH"],
      "get": ["PATH"]
    },
    "fs": {
      "read": ["/data/**"]
    },
    "process": {
      "exit": true
    },
    "net": {
      "fetch": ["http://api.example.com/**"]
    }
  }
}"#,
    )
    .unwrap();

    fs::write(
        project.join("src").join("main.ts"),
        r#"declare const varta: {
  time: { now(): number };
  env: { has(name: string): boolean; print(name: string): void; get(name: string): string };
  fs: { print(path: string): void; read(path: string): string };
  process: { exit(code: number): void };
  net: { fetch(url: string): number };
};

export function main(): void {
  console.log(varta.time.now());
  varta.env.print("PATH");
  console.log(varta.env.has("PATH"));
  console.log(varta.env.get("PATH"));
  varta.fs.print("/data/hello.txt");
  console.log(varta.fs.read("/data/hello.txt"));
  varta.process.exit(0);
  console.log(varta.net.fetch("https://api.example.com/health"));
}
"#,
    )
    .unwrap();

    fs::write(
        project.join("varta.d.ts"),
        r#"
declare const varta: {
  time: { now(): number };
  env: { has(name: string): boolean; print(name: string): void; get(name: string): string };
  fs: { print(path: string): void; read(path: string): string };
  process: { exit(code: number): void };
  net: { fetch(url: string): number };
};
"#,
    )
    .unwrap();

    let check = varta()
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("failed to run varta check");
    assert!(check.status.success(), "check failed: {}", combined(&check));

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn cross_operation_capability_denial_smoke_test() {
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
    "fs": {
      "read": ["/data/**"]
    }
  }
}"#,
    )
    .unwrap();

    fs::write(
        project.join("src").join("main.ts"),
        r#"export function main(): void {
  varta.fs.print("/data/hello.txt");
}
"#,
    )
    .unwrap();

    fs::write(
        project.join("varta.d.ts"),
        r#"declare const varta: { fs: { print(path: string): void; read(path: string): string } };
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
        combined(&dev).contains("varta log: <capability denied: fs.print>"),
        "expected cross-operation denial, got: {}",
        combined(&dev)
    );

    let _ = fs::remove_dir_all(&project);
}

struct RegressionProject(PathBuf);
impl RegressionProject {
    fn new(source: &str, capabilities: serde_json::Value) -> Self {
        let root = temp_project();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.ts"), source).unwrap();
        fs::write(root.join("varta.config.json"), serde_json::to_vec(&serde_json::json!({
            "entry":"src/main.ts", "target":"native", "policy":"strict", "out":"dist/app", "capabilities":capabilities
        })).unwrap()).unwrap();
        Self(root)
    }
    fn command(&self, args: &[&str]) -> std::process::Output {
        varta().current_dir(&self.0).args(args).output().unwrap()
    }
}
impl Drop for RegressionProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn strings_survive_host_buffer_reuse_nested_calls_and_module_returns_in_all_targets() {
    let project = RegressionProject::new(
        r#"
import {read, choose} from "./strings.ts";
export function main():void{
 const name:string="VARTA_STRING_ALPHA";
 let first=read(name); const saved=first;
 const second=read("VARTA_STRING_BETA"); first="reassigned";
 const selected=choose(read(name),read("VARTA_STRING_BETA"),true);
 console.log(first);console.log(saved);console.log(second);console.log(selected);
 const key=read("VARTA_STRING_KEY");console.log(read(key));
}
"#,
        serde_json::json!({"env":{"get":["VARTA_STRING_ALPHA","VARTA_STRING_BETA","VARTA_STRING_KEY"]}}),
    );
    fs::write(
        project.0.join("src/strings.ts"),
        r#"
export function read(name:string):string{return varta.env.get(name);}
export function choose(a:string,b:string,first:boolean):string{if(first){return a;}return b;}
"#,
    )
    .unwrap();
    let checked = project.command(&["check"]);
    assert!(checked.status.success(), "{}", combined(&checked));
    let built = project.command(&["build", "--single"]);
    assert!(built.status.success(), "{}", combined(&built));
    let mut native = Command::new(project.0.join("dist/app"));
    native.current_dir(&project.0);
    let mut dev = varta();
    dev.current_dir(&project.0).arg("dev");
    let mut run = varta();
    run.current_dir(&project.0).args(["run", "dist/app.wasm"]);
    for mut command in [dev, run, native] {
        let output = command
            .env("VARTA_STRING_ALPHA", "Привіт 🦀")
            .env("VARTA_STRING_BETA", "SECOND_VALUE")
            .env("VARTA_STRING_KEY", "VARTA_STRING_BETA")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert!(text.contains("varta log: reassigned\nvarta log: Привіт 🦀\nvarta log: SECOND_VALUE\nvarta log: Привіт 🦀\nvarta log: SECOND_VALUE"), "{text}");
    }
}

#[test]
fn dynamic_string_arguments_and_retained_results_preserve_permission_denials() {
    let project = RegressionProject::new(
        r#"
function read(name:string):string{return varta.env.get(name);}
export function main():void{
 const key="VARTA_STRING_ALLOWED"; const saved=read(key);
 const denied=read("VARTA_STRING_SECRET");const status=varta.result.status();
 console.log(saved);console.log(denied);console.log(status);
 console.log(varta.env.has(key));console.log(varta.result.status());
}
"#,
        serde_json::json!({"env":{"get":["VARTA_STRING_ALLOWED"]}}),
    );
    for grants in [
        serde_json::json!([{"id":"env.get","patterns":["VARTA_STRING_ALLOWED"]}]),
        serde_json::json!([]),
    ] {
        let allowed = !grants.as_array().unwrap().is_empty();
        project.host_policy(grants);
        let output = varta()
            .current_dir(&project.0)
            .args(["dev", "--policy", "host-policy.json"])
            .env("VARTA_STRING_ALLOWED", "SAVED_VALUE")
            .env("VARTA_STRING_SECRET", "HIDDEN_VALUE")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert_eq!(text.contains("varta log: SAVED_VALUE"), allowed, "{text}");
        assert!(!text.contains("HIDDEN_VALUE"), "{text}");
        assert!(text.contains("varta log: \nvarta log: 1"), "{text}");
        assert!(
            text.contains("<capability denied: env> (VARTA_STRING_SECRET)"),
            "{text}"
        );
        assert!(
            text.contains("<capability denied: env> (VARTA_STRING_ALLOWED)"),
            "{text}"
        );
    }
}

#[test]
fn dynamic_filesystem_paths_obey_scopes_and_descriptor_boundaries() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const path=varta.env.get("VARTA_STRING_PATH");
 const value=varta.fs.read(path);const status=varta.result.status();
 console.log(value);console.log(status);
}
"#,
        serde_json::json!({}),
    );
    let root = project.0.canonicalize().unwrap();
    fs::create_dir(root.join("data")).unwrap();
    fs::write(root.join("data/allowed.txt"), "VISIBLE_CONTENT").unwrap();
    fs::write(root.join("secret.txt"), "HIDDEN_CONTENT").unwrap();
    project.set_capabilities(serde_json::json!({"env":{"get":["VARTA_STRING_PATH"]},"fs":{"read":[format!("{}/data/**",root.display())]}}));
    let mut cases = vec![
        (root.join("data/allowed.txt"), true),
        (root.join("secret.txt"), false),
        (root.join("data/../secret.txt"), false),
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("secret.txt"), root.join("data/link.txt")).unwrap();
        cases.push((root.join("data/link.txt"), false));
    }
    for (path, allowed) in cases {
        let output = varta()
            .current_dir(&project.0)
            .arg("dev")
            .env("VARTA_STRING_PATH", path)
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert_eq!(
            text.contains("varta log: VISIBLE_CONTENT"),
            allowed,
            "{text}"
        );
        assert!(!text.contains("HIDDEN_CONTENT"), "{text}");
        if allowed {
            assert!(text.contains("varta log: 0"), "{text}");
        } else {
            assert!(!text.contains("varta log: 0"), "{text}");
        }
    }
}

#[test]
fn retained_string_copies_obey_execution_fuel_before_further_execution() {
    use varta_common::abi;
    let project = RegressionProject::new("export function main():void{}", serde_json::json!({}));
    let path = project.0.canonicalize().unwrap().join("chunk.txt");
    fs::write(&path, vec![b'X'; abi::WASM_PAGE_BYTES]).unwrap();
    project.set_capabilities(serde_json::json!({"fs":{"read":[path]}}));
    let capacity = (abi::MAX_MEMORY_BYTES - abi::OWNED_STRING_BASE) / abi::WASM_PAGE_BYTES;
    for (copies, success) in [(1, true), (capacity + 1, false)] {
        let source = format!(
            "export function main():void{{const path={};for(let i=0;i<{copies};i=i+1){{let s=varta.fs.read(path);}}console.log(\"AFTER_COPIES\");}}",
            serde_json::to_string(&path).unwrap()
        );
        fs::write(project.0.join("src/main.ts"), source).unwrap();
        let output = project.command(&["dev"]);
        let text = combined(&output);
        assert_eq!(output.status.success(), success, "{text}");
        assert_eq!(text.contains("varta log: AFTER_COPIES"), success, "{text}");
        if !success {
            assert!(text.contains("fuel"), "{text}");
        }
    }
}

#[test]
fn filesystem_scopes_deny_siblings_traversal_and_other_operations() {
    let project = RegressionProject::new("export function main():void {}", serde_json::json!({}));
    let root = project.0.join("data");
    let sibling = project.0.join("data-secret");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    fs::write(root.join("allowed.txt"), "ALLOWED_FILE_CONTENT").unwrap();
    fs::write(sibling.join("secret.txt"), "DENIED_FILE_CONTENT").unwrap();
    let source = format!(
        r#"export function main():void {{
        console.log(varta.fs.read({allowed}));
        console.log(varta.fs.read({outside}));
        console.log(varta.result.status());
        varta.fs.print({allowed});
        console.log(varta.result.status());
        console.log(varta.fs.read({traversal}));
        console.log(varta.result.status());
    }}"#,
        allowed = serde_json::to_string(&root.join("allowed.txt")).unwrap(),
        outside = serde_json::to_string(&sibling.join("secret.txt")).unwrap(),
        traversal = serde_json::to_string(&root.join("../data-secret/secret.txt")).unwrap()
    );
    fs::write(project.0.join("src/main.ts"), source).unwrap();
    let config = serde_json::json!({"entry":"src/main.ts", "target":"native", "policy":"strict", "out":"dist/app",
        "capabilities":{"fs":{"read":[format!("{}/**",root.display())]}}});
    fs::write(
        project.0.join("varta.config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let result = project.command(&["dev"]);
    let text = combined(&result);
    assert!(result.status.success(), "{text}");
    assert_eq!(text.matches("ALLOWED_FILE_CONTENT").count(), 1);
    assert!(!text.contains("DENIED_FILE_CONTENT"));
    assert_eq!(
        text.lines().filter(|line| *line == "varta log: 1").count(),
        3
    );
}

#[test]
#[cfg(unix)]
fn filesystem_rejects_symlinks_at_every_component() {
    use std::os::unix::fs::symlink;
    let project = RegressionProject::new("export function main():void {}", serde_json::json!({}));
    let granted = project.0.join("granted");
    let outside = project.0.join("outside");
    fs::create_dir_all(&granted).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "SYMLINK_SECRET").unwrap();
    fs::write(granted.join("secret.txt"), "INTERNAL_SYMLINK_SECRET").unwrap();
    symlink(outside.join("secret.txt"), granted.join("escape.txt")).unwrap();
    symlink(&outside, granted.join("directory")).unwrap();
    symlink("secret.txt", granted.join("inside.txt")).unwrap();
    let mut source = String::from("export function main():void {");
    for path in [
        granted.join("escape.txt"),
        granted.join("directory/secret.txt"),
        granted.join("inside.txt"),
    ] {
        source.push_str(&format!(
            "console.log(varta.fs.read({})); console.log(varta.result.status());",
            serde_json::to_string(&path).unwrap()
        ));
    }
    source.push('}');
    fs::write(project.0.join("src/main.ts"), source).unwrap();
    fs::write(
        project.0.join("varta.config.json"),
        serde_json::to_vec(
            &serde_json::json!({"entry":"src/main.ts","target":"native","out":"dist/app",
        "capabilities":{"fs":{"read":[format!("{}/**",granted.display())]}}}),
        )
        .unwrap(),
    )
    .unwrap();
    let output = project.command(&["dev"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("SYMLINK_SECRET"));
    assert_eq!(
        text.lines().filter(|line| *line == "varta log: 3").count(),
        3
    );
}

#[test]
fn malformed_manifests_and_native_payloads_fail_validation_and_execution() {
    let project = RegressionProject::new(
        "export function main():void { console.log(varta.env.get(\"TEST_SECRET\")); }",
        serde_json::json!({}),
    );
    assert!(project.command(&["build"]).status.success());
    for grant in [
        serde_json::json!({"id":"env.get","patterns":"INVALID"}),
        serde_json::json!({"id":"env.get","enabled":true}),
        serde_json::json!({"id":"env.get"}),
        serde_json::json!({"id":"env.unknown","patterns":["TEST_SECRET"]}),
    ] {
        fs::write(
            project.0.join("dist/app.manifest.json"),
            serde_json::to_vec(&serde_json::json!({"capabilities":[grant]})).unwrap(),
        )
        .unwrap();
        for command in ["validate", "run", "inspect"] {
            let output = project.command(&[command, "dist/app.wasm"]);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{command}: {}",
                combined(&output)
            );
        }
    }
    fs::write(
        project.0.join("native.bin"),
        b"VARTA2\0\x08\0\0\0\0\0\0\0NOT_AOT!\x13\0\0\0\0\0\0\0{\"capabilities\":[]}",
    )
    .unwrap();
    for command in ["validate", "run", "inspect"] {
        assert_eq!(
            project.command(&[command, "native.bin"]).status.code(),
            Some(1)
        );
    }
}

#[test]
fn invalid_wasm_has_a_failure_exit_code_in_every_artifact_command() {
    let project = RegressionProject::new("export function main():void {}", serde_json::json!({}));
    fs::write(project.0.join("bad.wasm"),b"\0asm\x01\0\0\0\x01\x04\x01\x60\0\0\x03\x02\x01\0\x07\x0a\x01\x06_start\0\0\x0a\x05\x01\x03\0\x1a\x0b").unwrap();
    for command in ["validate", "run", "inspect"] {
        let output = project.command(&[command, "bad.wasm"]);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{command}: {}",
            combined(&output)
        );
    }
}

#[test]
fn all_project_commands_reject_the_same_type_and_subset_errors() {
    let project = RegressionProject::new("export function main():void {}", serde_json::json!({}));
    for source in [
        "export function main():void { let x:string=7; console.log(x); }",
        "throw new Error('must not disappear'); export function main():void {}",
        "export function main():void { console.log(1.5); }",
        "export function main( {",
    ] {
        fs::write(project.0.join("src/main.ts"), source).unwrap();
        for command in ["check", "build", "dev"] {
            let output = project.command(&[command]);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{command}: {}",
                combined(&output)
            );
        }
    }
}

#[test]
fn network_control_characters_and_wrong_operations_are_denied_before_io() {
    let project = RegressionProject::new(
        r#"export function main():void {
        console.log(varta.net.fetch("http://127.0.0.1:9/public/x HTTP/1.1\r\nHost: localhost\r\n\r\nGET /private HTTP/1.1"));
        console.log(varta.result.status());
        console.log(varta.net.get("http://127.0.0.1:9/public/ok"));
        console.log(varta.result.status());
    }"#,
        serde_json::json!({"net":{"fetch":["http://127.0.0.1:9/public/**"]}}),
    );
    let output = project.command(&["dev"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("capability denied: net.fetch"));
    assert!(text.contains("capability denied: net.get"));
    assert!(!text.contains("host I/O error"));
    assert_eq!(
        text.lines().filter(|line| *line == "varta log: 1").count(),
        2
    );
}

#[test]
fn env_results_reset_after_denial_and_preserve_operation_scopes() {
    let project = RegressionProject::new(
        r#"export function main():void {
        console.log(varta.env.get("VARTA_TEST_ALLOWED"));
        console.log(varta.env.get("VARTA_TEST_DENIED"));
        console.log(varta.result.status());
        console.log(varta.env.has("VARTA_TEST_ALLOWED"));
        console.log(varta.result.status());
    }"#,
        serde_json::json!({"env":{"get":["VARTA_TEST_ALLOWED"]}}),
    );
    let output = varta()
        .current_dir(&project.0)
        .arg("dev")
        .env("VARTA_TEST_ALLOWED", "ABCDE")
        .env("VARTA_TEST_DENIED", "ENV_SECRET")
        .output()
        .unwrap();
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("varta log: ABCDE"));
    assert!(!text.contains("ENV_SECRET"));
    assert!(
        text.contains("<capability denied: env> (VARTA_TEST_DENIED)\nvarta log: \nvarta log: 1")
    );
    assert!(text.ends_with("varta log: 0\nvarta log: 1\nvarta dev: completed\n"));
}

#[test]
fn single_binary_markers_and_manifest_authority_are_unambiguous() {
    let project = RegressionProject::new(
        r#"export function main():void {
        console.log("VARTA1\0VARTA2\0VARTA3\0\0"); console.log(varta.time.now());
    }"#,
        serde_json::json!({}),
    );
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::write(
        project.0.join("dist/app.manifest.json"),
        r#"{"capabilities":[{"id":"time.now","enabled":true}]}"#,
    )
    .unwrap();
    let native = Command::new(project.0.join("dist/app"))
        .current_dir(&project.0)
        .output()
        .unwrap();
    assert!(native.status.success(), "{}", combined(&native));
    assert!(combined(&native).contains("capability denied: time.now"));
    let inspect = project.command(&["inspect", "dist/app", "--json"]);
    let metadata: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(metadata["capabilities"], serde_json::json!([]));
    assert_eq!(metadata["precompiled"], false);
    assert_eq!(metadata["abiVersion"], 1);
    let mut bytes = fs::read(project.0.join("dist/app")).unwrap();
    let footer = bytes.len() - 32;
    bytes[footer..footer + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    fs::write(project.0.join("overflow.bin"), bytes).unwrap();
    assert_eq!(
        project.command(&["validate", "overflow.bin"]).status.code(),
        Some(1)
    );
}

#[test]
fn oversized_files_are_bounded_before_returning_and_do_not_exhaust_guest_memory() {
    let project = RegressionProject::new("export function main():void {}", serde_json::json!({}));
    let path = project.0.join("large.txt");
    let file = fs::File::create(&path).unwrap();
    file.set_len(2 * 1024 * 1024).unwrap();
    fs::write(project.0.join("src/main.ts"),format!("export function main():void {{ console.log(varta.fs.read({})); console.log(varta.result.status()); }}",serde_json::to_string(&path).unwrap())).unwrap();
    fs::write(project.0.join("varta.config.json"),serde_json::to_vec(&serde_json::json!({"entry":"src/main.ts","target":"native","out":"dist/app","capabilities":{"fs":{"read":[path]}}})).unwrap()).unwrap();
    let output = project.command(&["dev"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("<result too large>"));
    assert!(text.contains("varta log: 4"));
    assert!(text.len() < 1024);
}

impl RegressionProject {
    fn host_policy(&self, grants: serde_json::Value) {
        fs::write(
            self.0.join("host-policy.json"),
            serde_json::to_vec(
                &serde_json::json!({"schemaVersion":1,"abiVersion":1,"capabilities":grants}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    fn set_capabilities(&self, grants: serde_json::Value) {
        let path = self.0.join("varta.config.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["capabilities"] = grants;
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    }
}

#[test]
fn imported_functions_obey_package_permissions_and_the_host_ceiling() {
    let project = RegressionProject::new(
        "import {show} from './library.ts'; export function main():void{show();}",
        serde_json::json!({"env":{"get":["VARTA_MODULE_SECRET"]}}),
    );
    fs::write(project.0.join("src/library.ts"), "export function show():void{console.log(varta.env.get(\"VARTA_MODULE_SECRET\"));console.log(varta.result.status());}").unwrap();
    project.host_policy(serde_json::json!([]));
    assert!(project.command(&["build"]).status.success());
    for command in ["dev", "run"] {
        for restricted in [false, true] {
            let mut process = varta();
            process
                .current_dir(&project.0)
                .arg(command)
                .env("VARTA_MODULE_SECRET", "MODULE_SECRET_VALUE");
            if command == "run" {
                process.arg("dist/app.wasm");
            }
            if restricted {
                process.args(["--policy", "host-policy.json"]);
            }
            let output = process.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            if restricted {
                assert!(!text.contains("MODULE_SECRET_VALUE"), "{text}");
                assert!(
                    text.contains("<capability denied: env> (VARTA_MODULE_SECRET)"),
                    "{text}"
                );
                assert!(text.contains("varta log: 1"), "{text}");
            } else {
                assert!(
                    text.contains("varta log: MODULE_SECRET_VALUE\nvarta log: 0"),
                    "{text}"
                );
            }
        }
    }
    project.set_capabilities(serde_json::json!({}));
    project.host_policy(serde_json::json!([{"id":"env.get","patterns":["VARTA_MODULE_SECRET"]}]));
    let output = varta()
        .current_dir(&project.0)
        .args(["dev", "--policy", "host-policy.json"])
        .env("VARTA_MODULE_SECRET", "MODULE_SECRET_VALUE")
        .output()
        .unwrap();
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("MODULE_SECRET_VALUE"), "{text}");
    assert!(text.contains("varta log: 1"), "{text}");
}

#[test]
fn host_policy_narrows_environment_access_for_dev_run_and_review() {
    let project = RegressionProject::new(
        r#"export function main():void {
        console.log(varta.env.get("APP_NAME")); console.log(varta.env.get("APP_SECRET"));
        console.log(varta.result.status()); console.log(varta.env.has("APP_NAME"));
        console.log(varta.result.status());
    }"#,
        serde_json::json!({"env":{"get":["APP_*"],"has":["APP_*"]}}),
    );
    project.host_policy(serde_json::json!([{"id":"env.get","patterns":["APP_NAME"]}]));
    assert!(project.command(&["build"]).status.success());
    for args in [
        vec!["dev", "--policy", "host-policy.json"],
        vec!["--policy", "host-policy.json", "run", "dist/app.wasm"],
    ] {
        let result = varta()
            .current_dir(&project.0)
            .args(args)
            .env("APP_NAME", "VISIBLE_NAME")
            .env("APP_SECRET", "HIDDEN_SECRET")
            .output()
            .unwrap();
        let text = combined(&result);
        assert!(result.status.success(), "{text}");
        assert!(text.contains("varta log: VISIBLE_NAME"));
        assert!(!text.contains("HIDDEN_SECRET"));
        assert!(text.contains("<capability denied: env> (APP_SECRET)"));
        assert!(text.contains("<capability denied: env> (APP_NAME)"));
    }
    let review = project.command(&[
        "inspect",
        "dist/app.wasm",
        "--policy",
        "host-policy.json",
        "--json",
    ]);
    assert!(review.status.success(), "{}", combined(&review));
    let json: serde_json::Value = serde_json::from_slice(&review.stdout).unwrap();
    assert_eq!(json["authorization"]["mode"], "host_policy");
    assert_eq!(json["authorization"]["combination"], "allOf");
    assert_eq!(
        json["authorization"]["hostCeiling"][0]["patterns"],
        serde_json::json!(["APP_NAME"])
    );
    let operations = json["authorization"]["operations"].as_array().unwrap();
    assert_eq!(
        operations.iter().find(|op| op["id"] == "env.has").unwrap()["decision"],
        "denied"
    );
    assert_eq!(
        operations.iter().find(|op| op["id"] == "env.get").unwrap()["decision"],
        "runtime_check"
    );
}

#[test]
fn empty_host_policy_denies_all_privileges_including_a_packaged_app() {
    let project = RegressionProject::new(
        r#"export function main():void {
        console.log(varta.env.get("VARTA_HOST_TEST")); console.log(varta.result.status());
        console.log(varta.time.now()); console.log(varta.result.status());
        console.log(varta.net.fetch("http://127.0.0.1:9/public/ok")); console.log(varta.result.status());
        varta.process.exit(23); console.log(varta.result.status()); console.log("still running");
    }"#,
        serde_json::json!({"env":{"get":["VARTA_HOST_TEST"]},"time":{"now":true},"net":{"fetch":["http://127.0.0.1:9/public/**"]},"process":{"exit":true}}),
    );
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let manifest_before = fs::read(project.0.join("dist/app.manifest.json")).unwrap();
    for result in [
        project.command(&["dev", "--policy", "host-policy.json"]),
        project.command(&["run", "dist/app.wasm", "--policy", "host-policy.json"]),
        Command::new(project.0.join("dist/app"))
            .current_dir(&project.0)
            .args(["--policy", "host-policy.json"])
            .env("VARTA_HOST_TEST", "SHOULD_NOT_ESCAPE")
            .output()
            .unwrap(),
    ] {
        let text = combined(&result);
        assert!(result.status.success(), "{text}");
        assert!(!text.contains("SHOULD_NOT_ESCAPE"));
        assert!(!text.contains("host I/O error"));
        for operation in ["env", "time.now", "net.fetch", "process.exit"] {
            assert!(
                text.contains(&format!("capability denied: {operation}")),
                "{text}"
            );
        }
        assert_eq!(
            text.lines().filter(|line| *line == "varta log: 1").count(),
            4
        );
        assert!(text.contains("varta log: still running"));
    }
    assert_eq!(
        fs::read(project.0.join("dist/app.manifest.json")).unwrap(),
        manifest_before
    );
    let validate = project.command(&[
        "validate",
        "dist/app.wasm",
        "--policy",
        "host-policy.json",
        "--json",
    ]);
    assert!(validate.status.success(), "{}", combined(&validate));
    let json: serde_json::Value = serde_json::from_slice(&validate.stdout).unwrap();
    assert!(
        json["authorization"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|op| op["decision"] == "denied")
    );
}

#[test]
fn neither_host_nor_package_can_expand_the_other_filesystem_scope() {
    let project = RegressionProject::new("export function main():void {}", serde_json::json!({}));
    let root = project.0.join("data");
    fs::create_dir(&root).unwrap();
    let allowed = root.join("allowed.txt");
    let secret = root.join("secret.txt");
    fs::write(&allowed, "ALLOWED_CONTENT").unwrap();
    fs::write(&secret, "DENIED_CONTENT").unwrap();
    fs::write(project.0.join("src/main.ts"),format!("export function main():void {{console.log(varta.fs.read({}));console.log(varta.fs.read({}));console.log(varta.result.status());varta.fs.print({});console.log(varta.result.status());}}",serde_json::to_string(&allowed).unwrap(),serde_json::to_string(&secret).unwrap(),serde_json::to_string(&allowed).unwrap())).unwrap();
    for (requested, ceiling) in [
        (
            format!("{}/**", root.display()),
            allowed.display().to_string(),
        ),
        (
            allowed.display().to_string(),
            format!("{}/**", root.display()),
        ),
    ] {
        project.set_capabilities(serde_json::json!({"fs":{"read":[requested],"print":[allowed]}}));
        project.host_policy(serde_json::json!([{"id":"fs.read","patterns":[ceiling]}]));
        let result = project.command(&["dev", "--policy", "host-policy.json"]);
        let text = combined(&result);
        assert!(result.status.success(), "{text}");
        assert_eq!(text.matches("ALLOWED_CONTENT").count(), 1);
        assert!(!text.contains("DENIED_CONTENT"));
        assert_eq!(
            text.lines().filter(|line| *line == "varta log: 1").count(),
            2
        );
    }
}

#[test]
#[cfg(unix)]
fn package_root_symlink_cannot_escape_a_host_directory_ceiling() {
    use std::os::unix::fs::symlink;
    let project = RegressionProject::new("export function main():void {}", serde_json::json!({}));
    let safe = project.0.join("safe");
    let outside = project.0.join("outside");
    fs::create_dir(&safe).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "OUTSIDE_HOST_AUTHORITY").unwrap();
    symlink(&outside, safe.join("alias")).unwrap();
    let path = safe.join("alias/secret.txt");
    project.set_capabilities(serde_json::json!({"fs":{"read":[path]}}));
    project.host_policy(
        serde_json::json!([{"id":"fs.read","patterns":[format!("{}/**",safe.display())]}]),
    );
    fs::write(project.0.join("src/main.ts"),format!("export function main():void {{console.log(varta.fs.read({}));console.log(varta.result.status());}}",serde_json::to_string(&path).unwrap())).unwrap();
    let result = project.command(&["dev", "--policy", "host-policy.json"]);
    let text = combined(&result);
    assert!(result.status.success(), "{text}");
    assert!(!text.contains("OUTSIDE_HOST_AUTHORITY"));
    assert!(text.contains("varta log: 3"));
}

#[test]
fn bad_host_policies_never_fall_back_to_package_grants() {
    let project = RegressionProject::new(
        "export function main():void {console.log(\"MUST_NOT_EXECUTE\");}",
        serde_json::json!({"time":{"now":true}}),
    );
    assert!(project.command(&["build"]).status.success());
    let missing = project.command(&["run", "dist/app.wasm", "--policy", "host-policy.json"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(!combined(&missing).contains("MUST_NOT_EXECUTE"));
    for invalid in [
        r#"{"capabilities":[{"id":"env.get","patterns":"wrong"}]}"#,
        r#"{"capabilities":[{"id":"time.now"}]}"#,
        "not json",
    ] {
        fs::write(project.0.join("host-policy.json"), invalid).unwrap();
        for args in [
            vec!["run", "dist/app.wasm"],
            vec!["dev"],
            vec!["inspect", "dist/app.wasm"],
            vec!["validate", "dist/app.wasm"],
        ] {
            let mut args = args;
            args.extend(["--policy", "host-policy.json"]);
            let result = project.command(&args);
            assert_eq!(result.status.code(), Some(1), "{}", combined(&result));
            assert!(!combined(&result).contains("MUST_NOT_EXECUTE"));
        }
    }
}

#[test]
fn host_policy_cannot_add_unrequested_rights_and_is_never_embedded_by_build() {
    let project = RegressionProject::new(
        "export function main():void {console.log(varta.time.now());console.log(varta.result.status());}",
        serde_json::json!({}),
    );
    project.host_policy(serde_json::json!([{"id":"time.now","enabled":true}]));
    let rejected = project.command(&["build", "--policy", "host-policy.json"]);
    assert_eq!(rejected.status.code(), Some(1));
    assert!(!project.0.join("dist").exists());
    let result = project.command(&["dev", "--policy", "host-policy.json"]);
    let text = combined(&result);
    assert!(result.status.success(), "{text}");
    assert!(text.contains("capability denied: time.now"));
    assert!(text.contains("varta log: 0\nvarta log: 1"));
    assert!(project.command(&["run", "--help"]).status.success());
    assert!(combined(&project.command(&["run", "--help"])).contains("--policy"));
}

#[test]
fn source_diagnostics_use_the_real_entry_path_and_consistent_ranges_in_every_command() {
    let source = "export function main(): void {\n  let x: string = 7;\n}\n";
    let project = RegressionProject::new(source, serde_json::json!({}));
    fs::create_dir(project.0.join("configs")).unwrap();
    let entry = project.0.join("src/actual entry.ts");
    fs::write(&entry, source).unwrap();
    fs::write(
        project.0.join("configs/project.json"),
        r#"{"entry":"../src/actual entry.ts","target":"wasm","out":"../dist/app"}"#,
    )
    .unwrap();
    let expected = entry.canonicalize().unwrap();
    let check = project.command(&["check", "--config", "configs/project.json", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(json["status"], "failed");
    assert_eq!(json["diagnostics"].as_array().unwrap().len(), 1);
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["file"], serde_json::json!(expected));
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(diagnostic["range"]["start"]["line"], 2);
    assert_eq!(diagnostic["range"]["start"]["column"], 19);
    assert_eq!(diagnostic["range"]["end"]["column"], 20);
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find('7').unwrap()
    );
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("expected string, found number")
    );
    assert_eq!(diagnostic["snippet"]["text"], "  let x: string = 7;");
    for command in ["check", "build", "dev"] {
        let result = project.command(&[command, "--config", "configs/project.json"]);
        assert_eq!(result.status.code(), Some(1));
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains(&format!("{}:2:19:", expected.display())),
            "{error}"
        );
        assert!(error.contains("let x: string = 7;"));
        assert!(!error.contains("input.ts"));
    }
    assert!(!project.0.join("dist").exists());
}

#[test]
fn syntax_diagnostics_are_structured_and_non_source_errors_do_not_invent_locations() {
    let project = RegressionProject::new("export function main( {", serde_json::json!({}));
    let output = project.command(&["check", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json["diagnostics"][0]["code"], "syntax_error");
    assert_eq!(
        json["diagnostics"][0]["file"],
        serde_json::json!(project.0.join("src/main.ts").canonicalize().unwrap())
    );
    let output = project.command(&["check", "--config", "missing.json", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json["diagnostics"], serde_json::json!([]));
    fs::write(
        project.0.join("src/main.ts"),
        "export function main():void {console.log(42);}",
    )
    .unwrap();
    let output = project.command(&["check", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(json["diagnostics"], serde_json::json!([]));
}
