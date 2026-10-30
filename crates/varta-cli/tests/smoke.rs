use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn varta() -> Command {
    Command::new(env!("CARGO_BIN_EXE_varta"))
}

fn runtime_path() -> PathBuf {
    varta_project::runtime_launcher(std::path::Path::new(env!("CARGO_BIN_EXE_varta")))
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

#[test]
fn records_and_json_work_across_modules_dev_wasm_native_and_tool_invocation() {
    let project = RegressionProject::new(
        r#"
import {wrap} from "./values.ts";
export function encode(text:string):string{
 const o=wrap(text);const alias=o;alias.count=2;return JSON.stringify(o);
}
export function main():void{console.log(encode("Привіт🦀"));}
varta.tool({name:"encode",description:"Encode a value"},encode);
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/values.ts"), r#"
function identity(o:{text:string;count:number;meta:{ok:boolean;empty:null}}):{count:number;text:string;meta:{empty:null;ok:boolean}}{return o;}
export function wrap(text:string):{meta:{ok:boolean;empty:null};text:string;count:number}{
 return identity({text,count:1,meta:{ok:true,empty:null}});
}
"#).unwrap();
    for args in [&["check"][..], &["build", "--single"][..]] {
        let output = project.command(args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let expected = serde_json::json!({"text":"Привіт🦀","count":2,"meta":{"ok":true,"empty":null}});
    for output in [
        project.command(&["dev"]),
        project.command(&["run", "dist/app.wasm"]),
        Command::new(project.0.join("dist/app"))
            .current_dir(&project.0)
            .output()
            .unwrap(),
    ] {
        assert!(output.status.success(), "{}", combined(&output));
        let text = String::from_utf8(output.stdout).unwrap();
        let logged = text
            .lines()
            .find_map(|s| s.strip_prefix("varta log: "))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(logged).unwrap(),
            expected
        );
    }
    fs::remove_dir_all(project.0.join("src")).unwrap();
    for artifact in ["dist/app.wasm", "dist/app"] {
        let output = project.command(&[
            "invoke",
            artifact,
            "encode",
            "--params",
            r#"{"text":"Привіт🦀"}"#,
            "--json",
        ]);
        assert!(output.status.success(), "{}", combined(&output));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let json = result["result"]["value"].as_str().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(json).unwrap(),
            expected
        );
    }
}

#[test]
fn json_and_record_resources_still_require_matching_grants_and_host_ceiling() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const names={allowed:"VARTA_RECORD_ALLOWED",secret:"VARTA_RECORD_SECRET"};
 const a=varta.result.capture(varta.env.get(names.allowed));
 const b=varta.result.capture(varta.env.get(names.secret));
 const c=varta.result.capture(varta.env.get(JSON.stringify("VARTA_RECORD_ALLOWED")));
 console.log(JSON.stringify({a:a.value,astatus:a.status,b:b.value,bstatus:b.status,cstatus:c.status}));
}
"#,
        serde_json::json!({"env":{"get":["VARTA_RECORD_ALLOWED"]}}),
    );
    project.host_policy(serde_json::json!([]));
    for restricted in [false, true] {
        let mut command = varta();
        command.current_dir(&project.0).arg("dev");
        if restricted {
            command.args(["--policy", "host-policy.json"]);
        }
        let output = command
            .env("VARTA_RECORD_ALLOWED", "VISIBLE")
            .env("VARTA_RECORD_SECRET", "HIDDEN_RECORD_VALUE")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        let text = combined(&output);
        assert!(!text.contains("HIDDEN_RECORD_VALUE"), "{text}");
        let json = text
            .lines()
            .filter_map(|s| s.strip_prefix("varta log: "))
            .find(|s| s.starts_with('{'))
            .unwrap();
        let result: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"a":if restricted {""} else {"VISIBLE"},"astatus":if restricted {1} else {0},"b":"","bstatus":1,"cstatus":1})
        );
    }
}

#[test]
fn typed_arrays_run_across_modules_and_packaged_targets_without_sources() {
    let project = RegressionProject::new(
        r#"
import {make} from "./arrays.ts";
export function encode(text:string):string{
 const o=make(text);const alias=o.values;alias[1]=alias[0]+"!";
 o.rows[0]=[7];return JSON.stringify(o);
}
export function main():void{console.log(encode("Привіт🦀"));}
varta.tool({name:"encode",description:"Encode"},encode);
"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/arrays.ts"),
        r#"
export function make(text:string):{values:Array<string>;rows:number[][]}{
 return {values:[text,"fallback"],rows:[[],[1,2]]};
}
"#,
    )
    .unwrap();
    for args in [&["check"][..], &["build", "--single"][..]] {
        let output = project.command(args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let expected = serde_json::json!({"values":["Привіт🦀","Привіт🦀!"],"rows":[[7],[1,2]]});
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    for output in [
        dev,
        project.command(&["run", "dist/app.wasm"]),
        Command::new(project.0.join("dist/app"))
            .current_dir(&project.0)
            .output()
            .unwrap(),
    ] {
        assert!(output.status.success(), "{}", combined(&output));
        let text = String::from_utf8(output.stdout).unwrap();
        let json = text
            .lines()
            .find_map(|s| s.strip_prefix("varta log: "))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(json).unwrap(),
            expected
        );
    }
    for artifact in ["dist/app.wasm", "dist/app"] {
        let output = project.command(&[
            "invoke",
            artifact,
            "encode",
            "--params",
            r#"{"text":"Привіт🦀"}"#,
            "--json",
        ]);
        assert!(output.status.success(), "{}", combined(&output));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(result["result"]["value"].as_str().unwrap())
                .unwrap(),
            expected
        );
    }
}

#[test]
fn array_derived_resources_preserve_grants_host_ceiling_and_system_status() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const names=["VARTA_ARRAY_ALLOWED","VARTA_ARRAY_SECRET"];
 const a=varta.result.capture(varta.env.get(names[0]));
 const b=varta.result.capture(varta.env.get(names[1]));
 const c=varta.result.capture(varta.env.get(JSON.stringify(names[0])));
 const n=names.length;const snapshot=JSON.stringify([a.value,b.value]);
 console.log(varta.result.status());console.log(snapshot);
 console.log(JSON.stringify([a.status,b.status,c.status]));
}
"#,
        serde_json::json!({"env":{"get":["VARTA_ARRAY_ALLOWED"]}}),
    );
    project.host_policy(serde_json::json!([]));
    for restricted in [false, true] {
        let mut command = varta();
        command.current_dir(&project.0).arg("dev");
        if restricted {
            command.args(["--policy", "host-policy.json"]);
        }
        let output = command
            .env("VARTA_ARRAY_ALLOWED", "VISIBLE")
            .env("VARTA_ARRAY_SECRET", "HIDDEN_ARRAY_VALUE")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert!(!text.contains("HIDDEN_ARRAY_VALUE"), "{text}");
        let json = text
            .lines()
            .filter_map(|s| s.strip_prefix("varta log: "))
            .filter(|s| s.starts_with('['))
            .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
            .collect::<Vec<_>>();
        assert!(text.contains("varta log: 1"), "{text}");
        assert_eq!(
            json,
            [
                serde_json::json!([if restricted { "" } else { "VISIBLE" }, ""]),
                serde_json::json!([if restricted { 1 } else { 0 }, 1, 1])
            ]
        );
    }
}

#[test]
fn checked_json_parsing_runs_through_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {parse} from "./decode.ts";
export function decode(text:string):string{
 const value=parse(text);value.count=value.count+1;value.values[0]=value.values[0]+"!";
 return JSON.stringify(value);
}
export function main():void{console.log(decode("{\"values\":[\"Привіт🦀\"],\"count\":1}"));}
varta.tool({name:"decode",description:"Decode"},decode);
"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/decode.ts"),
        r#"
export function parse(text:string):{values:string[];count:number}{
 return varta.json.parse<{count:number;values:Array<string>}>(text);
}
"#,
    )
    .unwrap();
    for args in [&["check"][..], &["build", "--single"][..]] {
        let output = project.command(args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let expected = serde_json::json!({"count":2,"values":["Привіт🦀!"]});
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for output in [
        dev,
        project.command(&["run", "dist/app.wasm"]),
        Command::new(project.0.join("dist/app"))
            .current_dir(&project.0)
            .output()
            .unwrap(),
    ] {
        assert!(output.status.success(), "{}", combined(&output));
        let text = String::from_utf8(output.stdout).unwrap();
        let json = text
            .lines()
            .find_map(|line| line.strip_prefix("varta log: "))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(json).unwrap(),
            expected
        );
    }
    let parameters = serde_json::json!({"text":r#"{"values":["Привіт🦀"],"count":1}"#}).to_string();
    for artifact in ["dist/app.wasm", "dist/app"] {
        let output = project.command(&[
            "invoke",
            artifact,
            "decode",
            "--params",
            &parameters,
            "--json",
        ]);
        assert!(output.status.success(), "{}", combined(&output));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(result["result"]["value"].as_str().unwrap())
                .unwrap(),
            expected
        );
        for text in [
            r#"{"values":[],"count":-0}"#,
            r#"{"values":[],"count":1,"extra":true}"#,
        ] {
            let parameters = serde_json::json!({"text":text}).to_string();
            let output = project.command(&[
                "invoke",
                artifact,
                "decode",
                "--params",
                &parameters,
                "--json",
            ]);
            assert!(!output.status.success());
            let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(error["code"], "execution_failed");
            assert!(error.get("result").is_none());
            assert!(combined(&output).contains("checked JSON"));
        }
    }
}

#[test]
fn parsed_resources_require_matching_grants_and_host_ceiling() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const names=varta.json.parse<{allowed:string;secret:string}>("{\"secret\":\"VARTA_PARSE_SECRET\",\"allowed\":\"VARTA_PARSE_ALLOWED\"}");
 const a=varta.result.capture(varta.env.get(names.allowed));
 const b=varta.result.capture(varta.env.get(names.secret));
 const snapshot=varta.json.parse<string>("\"still pure\"");
 console.log(varta.result.status());console.log(JSON.stringify({a:a.value,astatus:a.status,b:b.value,bstatus:b.status}));
}
"#,
        serde_json::json!({"env":{"get":["VARTA_PARSE_ALLOWED"]}}),
    );
    project.host_policy(serde_json::json!([]));
    for restricted in [false, true] {
        let mut process = varta();
        process.current_dir(&project.0).arg("dev");
        if restricted {
            process.args(["--policy", "host-policy.json"]);
        }
        let output = process
            .env("VARTA_PARSE_ALLOWED", "VISIBLE")
            .env("VARTA_PARSE_SECRET", "HIDDEN_PARSE_VALUE")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert!(!text.contains("HIDDEN_PARSE_VALUE"), "{text}");
        assert!(text.contains("varta log: 1"), "{text}");
        let json = text
            .lines()
            .filter_map(|s| s.strip_prefix("varta log: "))
            .find(|s| s.starts_with('{'))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(json).unwrap(),
            serde_json::json!({"a":if restricted {""}else{"VISIBLE"},"astatus":if restricted {1}else{0},"b":"","bstatus":1})
        );
    }
}

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
fn named_json_invocation_works_without_sources_in_wasm_and_native_artifacts() {
    let project = RegressionProject::new(
        r#"
export function preview(title:string,suffix:string,revision:number,confirmed:boolean):string{
 if(revision!=42){return "WRONG_ARGUMENT_ORDER";}if(!confirmed){return title;}
 console.log("preview log");return title+suffix;
}
export function main():void{varta.process.exit(99);}
varta.tool({name:"document.previewTitle",description:"Preview a title"},preview);
"#,
        serde_json::json!({"process":{"exit":true}}),
    );
    assert!(project.command(&["build", "--single"]).status.success());
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let params =
        serde_json::json!({"suffix":"🦀\0!","confirmed":true,"title":"Привіт","revision":42})
            .to_string();
    for artifact in ["dist/app.wasm", "dist/app"] {
        let output = project.command(&[
            "invoke",
            artifact,
            "document.previewTitle",
            "--params",
            &params,
            "--json",
        ]);
        assert!(output.status.success(), "{}", combined(&output));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["result"]["value"], "Привіт🦀\0!");
        assert_eq!(
            result["logs"],
            serde_json::json!(["varta log: preview log"])
        );
    }
    let plain = project.command(&[
        "invoke",
        "dist/app.wasm",
        "document.previewTitle",
        "--params",
        &params,
    ]);
    assert!(plain.status.success(), "{}", combined(&plain));
    let result: serde_json::Value = serde_json::from_slice(&plain.stdout).unwrap();
    assert_eq!(result["value"], "Привіт🦀\0!");
    assert!(String::from_utf8_lossy(&plain.stderr).contains("preview log"));
}

#[test]
fn invocation_errors_are_structured_and_do_not_execute_main_or_private_exports() {
    let project = RegressionProject::new(
        r#"
export function exit(title:string,n:number):void{varta.process.exit(17);}
export function privateFn():void{varta.process.exit(18);}
export function main():void{varta.process.exit(19);}
varta.tool({name:"exit",description:"Exit"},exit);
"#,
        serde_json::json!({"process":{"exit":true}}),
    );
    assert!(project.command(&["build"]).status.success());
    for (name, params, code) in [
        ("exit", "{}", "invalid_arguments"),
        ("exit", r#"{"title":"x","n":1.0}"#, "invalid_arguments"),
        (
            "exit",
            r#"{"title":"x","n":2147483648}"#,
            "invalid_arguments",
        ),
        ("exit", r#"{"title":"x","n":1,"n":2}"#, "invalid_arguments"),
        (
            "exit",
            r#"{"title":"x","n":1,"other":0}"#,
            "invalid_arguments",
        ),
        ("exit", "null", "invalid_arguments"),
        ("exit", "[]", "invalid_arguments"),
        ("main", "{}", "unknown_tool"),
        ("privateFn", "{}", "unknown_tool"),
        ("exit", r#"{"title":"x","n":1}"#, "guest_exit"),
    ] {
        let output = project.command(&[
            "invoke",
            "dist/app.wasm",
            name,
            "--params",
            params,
            "--json",
        ]);
        assert_eq!(output.status.code(), Some(1), "{}", combined(&output));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["status"], "failed");
        assert_eq!(result["code"], code, "{result}");
        assert!(result.get("result").is_none());
    }
    let params = " ".repeat(varta_common::abi::MAX_INPUT_BYTES + 1);
    let output = project.command(&[
        "invoke",
        "dist/app.wasm",
        "exit",
        "--params",
        &params,
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["code"], "invalid_arguments");
    fs::write(
        project.0.join("dist/invalid.wasm"),
        b"\0asm\x01\0\0\0\x01\x01\xff",
    )
    .unwrap();
    let bad = project.command(&["invoke", "dist/invalid.wasm", "exit", "--json"]);
    assert_eq!(bad.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&bad.stdout).unwrap();
    assert_eq!(result["code"], "invalid_artifact");
}

#[test]
fn json_invocation_checks_concrete_resources_against_package_and_host_scopes() {
    let project = RegressionProject::new(
        r#"
export function read(name:string):string{
 const r=varta.result.capture(varta.env.get(name));if(!r.ok){return "DENIED";}return r.value;
}
export function has(name:string):number{const r=varta.result.capture(varta.env.has(name));return r.status;}
export function main():void{varta.process.exit(99);}
varta.tool({name:"read",description:"Read"},read);
varta.tool({name:"has",description:"Test"},has);
"#,
        serde_json::json!({"env":{"get":["VARTA_INVOKE_*"]}}),
    );
    assert!(project.command(&["build", "--single"]).status.success());
    let host = project.0.join("host.json");
    fs::write(&host,serde_json::json!({"capabilities":[{"id":"env.get","patterns":["VARTA_INVOKE_ALLOWED","OTHER_ALLOWED"]},{"id":"env.has","patterns":["VARTA_INVOKE_ALLOWED"]}]}).to_string()).unwrap();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for (name, expected) in [
            ("VARTA_INVOKE_ALLOWED", "VISIBLE"),
            ("VARTA_INVOKE_SECRET", "DENIED"),
            ("OTHER_ALLOWED", "DENIED"),
        ] {
            let params = serde_json::json!({"name":name}).to_string();
            let output = varta()
                .current_dir(&project.0)
                .args([
                    "invoke",
                    artifact,
                    "read",
                    "--params",
                    &params,
                    "--json",
                    "--policy",
                    host.to_str().unwrap(),
                ])
                .env("VARTA_INVOKE_ALLOWED", "VISIBLE")
                .env("VARTA_INVOKE_SECRET", "SECRET_NEVER_RETURNED")
                .env("OTHER_ALLOWED", "SECRET_NEVER_RETURNED")
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["result"]["value"], expected);
            assert!(!combined(&output).contains("SECRET_NEVER_RETURNED"));
        }
        let has = project.command(&[
            "invoke",
            artifact,
            "has",
            "--params",
            r#"{"name":"VARTA_INVOKE_ALLOWED"}"#,
            "--policy",
            host.to_str().unwrap(),
            "--json",
        ]);
        assert!(has.status.success(), "{}", combined(&has));
        let result: serde_json::Value = serde_json::from_slice(&has.stdout).unwrap();
        assert_eq!(result["result"]["value"], 1);
    }
    fs::write(&host, r#"{"capabilities":[]}"#).unwrap();
    let denied = project.command(&[
        "invoke",
        "dist/app.wasm",
        "read",
        "--params",
        r#"{"name":"VARTA_INVOKE_ALLOWED"}"#,
        "--policy",
        host.to_str().unwrap(),
        "--json",
    ]);
    let result: serde_json::Value = serde_json::from_slice(&denied.stdout).unwrap();
    assert_eq!(result["result"]["value"], "DENIED");
    fs::write(&host, "{").unwrap();
    assert_eq!(
        project
            .command(&[
                "invoke",
                "dist/app.wasm",
                "read",
                "--params",
                r#"{"name":"VARTA_INVOKE_ALLOWED"}"#,
                "--policy",
                host.to_str().unwrap(),
                "--json"
            ])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn invoked_filesystem_tools_preserve_descriptor_scopes_and_retained_arguments() {
    let project = RegressionProject::new(
        r#"
export function read(path:string,suffix:string):string{
 const result=varta.result.capture(varta.fs.read(path));if(!result.ok){return "DENIED";}return result.value+suffix;
}
export function main():void{}
varta.tool({name:"read",description:"Read"},read);
"#,
        serde_json::json!({}),
    );
    let root = project.0.canonicalize().unwrap();
    fs::create_dir(root.join("data")).unwrap();
    let allowed = root.join("data/allowed.txt");
    let outside = root.join("secret.txt");
    fs::write(&allowed, "FILE_VALUE").unwrap();
    fs::write(&outside, "SECRET_FILE_VALUE").unwrap();
    project.set_capabilities(serde_json::json!({"fs":{"read":[root.join("data/*")]}}));
    assert!(project.command(&["build"]).status.success());
    for (path, expected) in [(&allowed, "FILE_VALUE🦀"), (&outside, "DENIED")] {
        let params = serde_json::json!({"path":path,"suffix":"🦀"}).to_string();
        let output = project.command(&[
            "invoke",
            "dist/app.wasm",
            "read",
            "--params",
            &params,
            "--json",
        ]);
        assert!(output.status.success(), "{}", combined(&output));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["result"]["value"], expected);
        assert!(!combined(&output).contains("SECRET_FILE_VALUE"));
    }
}

#[test]
fn tool_catalogs_are_inspected_without_sources_or_execution_in_wasm_and_native_packages() {
    let project = RegressionProject::new(
        r#"
function read(name:string):string{return varta.env.get(name);}
export function title(name:string):string{return read(name);}
export function ordinary():number{return 42;}
export function main():void{varta.env.print("VARTA_DISCOVERY_SECRET");varta.process.exit(17);}
varta.tool({name:"document.title",description:"Read a named title"},title);
"#,
        serde_json::json!({"env":{"get":["VARTA_DISCOVERY_SECRET"],"print":["VARTA_DISCOVERY_SECRET"]},"process":{"exit":true}}),
    );
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            project.0.join("dist/app"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
    let mut catalogs = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        let output = varta()
            .current_dir(&project.0)
            .args(["inspect", artifact, "--json"])
            .env("VARTA_DISCOVERY_SECRET", "NEVER_EXECUTE_GUEST_CODE")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert!(!combined(&output).contains("NEVER_EXECUTE_GUEST_CODE"));
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let catalog = &metadata["toolCatalog"];
        assert_eq!(catalog["contractVersion"], 1);
        assert_eq!(catalog["invocationAbiVersion"], 1);
        let tools = catalog["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "document.title");
        assert_eq!(tools[0]["requiredEffects"], serde_json::json!(["env.get"]));
        assert_eq!(
            tools[0]["inputSchema"]["properties"]["name"]["type"],
            "string"
        );
        assert_eq!(
            tools[0]["outputSchema"]["properties"]["value"]["type"],
            "string"
        );
        assert_eq!(tools[0]["inputSchema"]["additionalProperties"], false);
        catalogs.push(catalog.clone());
        assert!(project.command(&["validate", artifact]).status.success());
    }
    assert_eq!(catalogs[0], catalogs[1]);
}

#[test]
fn tool_handlers_can_be_explicitly_reexported_from_local_modules() {
    let project = RegressionProject::new(
        r#"
import {preview as title} from "./document.ts";
export {title as previewTitle};
export function main():void{console.log(title("Document"));}
varta.tool({name:"document.previewTitle",description:"Preview a title"},title);
"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/document.ts"),
        "export function preview(nextTitle:string):string{return nextTitle+\"!\";}",
    )
    .unwrap();
    let output = project.command(&["dev"]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(combined(&output).contains("varta log: Document!"));
    assert!(project.command(&["build"]).status.success());
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let metadata: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(
        metadata["toolCatalog"]["tools"][0]["parameters"][0]["name"],
        "nextTitle"
    );
    fs::write(project.0.join("src/document.ts"), "export function preview(nextTitle:string):string{return nextTitle;}varta.tool({name:'unwanted',description:'Dependency publication'},preview);").unwrap();
    let check = project.command(&["check"]);
    assert_eq!(check.status.code(), Some(1));
    assert!(combined(&check).contains("entry module"));
}

#[test]
fn invalid_tool_contracts_fail_consistently_before_build_or_execution() {
    let project = RegressionProject::new(
        r#"
export function preview(title:string):string{return title;}
export function main():void{console.log("MUST_NOT_RUN");}
varta.tool({name:"document.preview",description:"Preview"},preview);
varta.tool({name:"document.preview",description:"Duplicate"},preview);
"#,
        serde_json::json!({}),
    );
    for command in ["check", "build", "dev"] {
        let args = if command == "check" {
            vec![command, "--json"]
        } else {
            vec![command]
        };
        let output = project.command(&args);
        assert_eq!(output.status.code(), Some(1), "{}", combined(&output));
        assert!(combined(&output).contains("duplicate tool name"));
        if command == "check" {
            let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert!(metadata.to_string().contains("duplicate tool name"));
        }
        assert!(!combined(&output).contains("varta log: MUST_NOT_RUN"));
    }
    assert!(!project.0.join("dist").exists());
}

#[test]
fn publishing_a_tool_cannot_grant_system_access() {
    let project = RegressionProject::new(
        r#"
export function read(name:string):string{return varta.env.get(name);}
export function main():void{const r=varta.result.capture(varta.env.get("VARTA_TOOL_SECRET"));console.log(r.status);console.log(read("VARTA_TOOL_SECRET"));}
varta.tool({name:"read",description:"Read a value"},read);
"#,
        serde_json::json!({}),
    );
    assert!(project.command(&["build", "--single"]).status.success());
    for command in [
        vec!["dev"],
        vec!["run", "dist/app.wasm"],
        vec!["run", "dist/app"],
    ] {
        let output = varta()
            .current_dir(&project.0)
            .args(command)
            .env("VARTA_TOOL_SECRET", "MUST_STAY_SECRET")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert!(combined(&output).contains("varta log: 1"));
        assert!(!combined(&output).contains("MUST_STAY_SECRET"));
    }
    let native = Command::new(project.0.join("dist/app"))
        .env("VARTA_TOOL_SECRET", "MUST_STAY_SECRET")
        .output()
        .unwrap();
    assert!(native.status.success(), "{}", combined(&native));
    assert!(!combined(&native).contains("MUST_STAY_SECRET"));
    assert_eq!(
        project
            .command(&["validate", "dist/app.wasm"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn reclaimed_strings_preserve_module_arguments_results_and_permissions_in_every_target() {
    let project = RegressionProject::new(
        r#"
import {churn,first,roundtrip} from "./memory.ts";
export function main():void{
 const saved=varta.result.capture(varta.env.get("VARTA_GC_KEEP"));const alias=saved;
 const denied=varta.result.capture(varta.env.get("VARTA_GC_SECRET"));
 const argument=first("left"+"!",churn());const compound=("left"+"!")+churn();
 const returned=roundtrip(saved);
 console.log(argument);console.log(compound);console.log(returned.value);console.log(alias.value);
 console.log(returned.ok);console.log(returned.status);console.log(denied.status);console.log(varta.result.status());
}
"#,
        serde_json::json!({"env":{"get":["VARTA_GC_KEEP"]}}),
    );
    let library = format!(
        r#"
export function churn():string{{const piece={:?};
 for(let i=0,guard="guard"+"!";i<100;i=i+1){{
  if(guard!="guard!"){{return "BROKEN_GUARD";}}
  {{const temporary=piece+"x";if(i%3==0){{continue;}}
   switch(i%3){{case 1:{{const nested=piece+"y";break;}}default:{{const other=piece+"z";break;}}}}
  }}
 }}return "right"+"!";
}}
export function first(a:string,b:string):string{{return a;}}
export function roundtrip(r:VartaResult<string>):VartaResult<string>{{const temporary=churn();return r;}}
"#,
        "X".repeat(256)
    );
    fs::write(project.0.join("src/memory.ts"), library).unwrap();
    let checked = project.command(&["check"]);
    assert!(checked.status.success(), "{}", combined(&checked));
    let built = project.command(&["build", "--single"]);
    assert!(built.status.success(), "{}", combined(&built));
    let mut dev = varta();
    dev.current_dir(&project.0).arg("dev");
    let mut run = varta();
    run.current_dir(&project.0).args(["run", "dist/app.wasm"]);
    let mut native = Command::new(project.0.join("dist/app"));
    native.current_dir(&project.0);
    for mut process in [dev, run, native] {
        let output = process
            .env("VARTA_GC_KEEP", "Збережено 🦀")
            .env("VARTA_GC_SECRET", "GC_HIDDEN_VALUE")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert!(!text.contains("GC_HIDDEN_VALUE"), "{text}");
        assert!(text.contains("varta log: left!\nvarta log: left!right!\nvarta log: Збережено 🦀\nvarta log: Збережено 🦀\nvarta log: 1\nvarta log: 0\nvarta log: 1\nvarta log: 1"),"{text}");
    }
}

#[test]
fn string_operators_preserve_module_calls_operand_order_and_results_in_every_target() {
    let project = RegressionProject::new(
        r#"
import {join,less} from "./strings.ts";
function part(name:string):string{console.log(name);return varta.env.get(name);}
export function main():void{
 const saved=varta.result.capture(varta.env.get("VARTA_OP_NAME"));
 const pair=part("VARTA_OP_LEFT")+part("VARTA_OP_RIGHT");
 const combined=join(join(saved.value," "),pair);
 const emptyLeft=part("VARTA_OP_EMPTY")+part("VARTA_OP_RIGHT");
 const same=part("VARTA_OP_LEFT")==part("VARTA_OP_LEFT");
 const denied=varta.result.capture(varta.env.get("VARTA_OP_SECRET"));
 const fallback=denied.value+"fallback";const matched=fallback=="fallback";
 console.log(varta.result.status());console.log(combined);console.log(saved.value);
 console.log(saved.ok);console.log(saved.status);console.log(pair=="AB");
 console.log(emptyLeft);console.log(same);console.log(less("🙂","\uE000"));
 console.log(""+combined==combined+"");console.log(matched);console.log(denied.status);
}
"#,
        serde_json::json!({"env":{"get":["VARTA_OP_NAME","VARTA_OP_LEFT","VARTA_OP_RIGHT","VARTA_OP_EMPTY"]}}),
    );
    fs::write(project.0.join("src/strings.ts"),"export function join(a:string,b:string):string{return a+b;} export function less(a:string,b:string):boolean{return a<b;}").unwrap();
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.current_dir(&project.0).arg("dev");
    let mut run = varta();
    run.current_dir(&project.0).args(["run", "dist/app.wasm"]);
    let mut native = Command::new(project.0.join("dist/app"));
    native.current_dir(&project.0);
    for mut process in [dev, run, native] {
        let output = process
            .env("VARTA_OP_NAME", "Привіт 🦀")
            .env("VARTA_OP_LEFT", "A")
            .env("VARTA_OP_RIGHT", "B")
            .env("VARTA_OP_EMPTY", "")
            .env("VARTA_OP_SECRET", "HIDDEN_OPERATOR_VALUE")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert_eq!(
            text.matches("varta log: VARTA_OP_LEFT\n").count(),
            3,
            "{text}"
        );
        assert_eq!(
            text.matches("varta log: VARTA_OP_RIGHT\n").count(),
            2,
            "{text}"
        );
        assert_eq!(
            text.matches("varta log: VARTA_OP_EMPTY\n").count(),
            1,
            "{text}"
        );
        assert!(
            text.contains("varta log: VARTA_OP_LEFT\nvarta log: VARTA_OP_RIGHT"),
            "{text}"
        );
        assert!(!text.contains("HIDDEN_OPERATOR_VALUE"), "{text}");
        let expected = [
            "1",
            "Привіт 🦀 AB",
            "Привіт 🦀",
            "1",
            "0",
            "1",
            "B",
            "1",
            "1",
            "1",
            "1",
            "1",
        ]
        .map(|value| format!("varta log: {value}"))
        .join("\n");
        assert!(text.contains(&expected), "{text}");
    }
}

#[test]
fn concatenated_resources_keep_filesystem_network_and_host_policy_boundaries() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const base=varta.env.get("VARTA_OP_ROOT");
 const allowed=varta.result.capture(varta.fs.read(base+"/allowed.txt"));
 const outside=varta.result.capture(varta.fs.read(base+"/../secret.txt"));
 const net=varta.result.capture(varta.net.fetch("http://127.0.0.1:"+"1/private"));
 console.log(allowed.ok);console.log(allowed.value);console.log(allowed.status);
 console.log(outside.ok);console.log(outside.value);console.log(net.ok);console.log(net.status);
}
"#,
        serde_json::json!({}),
    );
    let root = project.0.canonicalize().unwrap();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("allowed.txt"), "VISIBLE_OPERATOR_FILE").unwrap();
    fs::write(root.join("secret.txt"), "HIDDEN_OPERATOR_FILE").unwrap();
    project.set_capabilities(serde_json::json!({"env":{"get":["VARTA_OP_ROOT"]},"fs":{"read":[format!("{}/**",data.display())]},"net":{"fetch":[]}}));
    project.host_policy(serde_json::json!([{"id":"env.get","patterns":["VARTA_OP_ROOT"]}]));
    for restricted in [false, true] {
        let mut process = varta();
        process
            .current_dir(&project.0)
            .arg("dev")
            .env("VARTA_OP_ROOT", &data);
        if restricted {
            process.args(["--policy", "host-policy.json"]);
        }
        let output = process.output().unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert!(!text.contains("HIDDEN_OPERATOR_FILE"), "{text}");
        if restricted {
            assert!(text.contains("varta log: 0\nvarta log: \nvarta log: 1\nvarta log: 0\nvarta log: \nvarta log: 0\nvarta log: 1"),"{text}");
        } else {
            assert!(text.contains("varta log: 1\nvarta log: VISIBLE_OPERATOR_FILE\nvarta log: 0\nvarta log: 0\nvarta log: \nvarta log: 0\nvarta log: 1"),"{text}");
        }
    }
}

#[test]
fn captured_results_keep_values_statuses_and_types_across_modules_in_every_target() {
    let project = RegressionProject::new(
        r#"
import {read,first,printedOk} from "./results.ts";
export function main():void{
 const failed=read("VARTA_CAPTURE_SECRET");
 let good:VartaResult<string>=read("VARTA_CAPTURE_ALLOWED");const alias=good;
 const empty=read("VARTA_CAPTURE_EMPTY");
 const missing=varta.result.capture(varta.env.has("VARTA_CAPTURE_ABSENT"));
 const picked=first(good,read("VARTA_CAPTURE_SECRET"));
 const printed=varta.result.capture(varta.env.print("VARTA_CAPTURE_ALLOWED"));
 good=read("VARTA_CAPTURE_OTHER");
 const nested=varta.result.capture(varta.env.get(varta.env.get("VARTA_CAPTURE_KEY")));
 const ignored=read("VARTA_CAPTURE_SECRET");
 if(alias.ok){console.log(alias.value);}else{console.log("WRONG_BRANCH");}
 console.log(alias.status);console.log(good.value);console.log(picked.value);console.log(picked.status);
 console.log(failed.ok);console.log(failed.status);console.log(failed.value);
 console.log(empty.ok);console.log(empty.status);console.log(empty.value);
 console.log(missing.ok);console.log(missing.value);console.log(missing.status);
 console.log(printedOk(printed));console.log(printed.status);printed.value;
 console.log(nested.value);console.log(nested.status);
}
"#,
        serde_json::json!({"env":{"get":["VARTA_CAPTURE_ALLOWED","VARTA_CAPTURE_OTHER","VARTA_CAPTURE_EMPTY","VARTA_CAPTURE_KEY"],"has":["VARTA_CAPTURE_ABSENT"],"print":["VARTA_CAPTURE_ALLOWED"]}}),
    );
    fs::write(project.0.join("src/results.ts"),r#"
export function read(name:string):VartaResult<string>{return varta.result.capture(varta.env.get(name));}
export function first(a:VartaResult<string>,b:VartaResult<string>):VartaResult<string>{return a;}
export function printedOk(r:VartaResult<void>):boolean{return r.ok;}
"#).unwrap();
    let checked = project.command(&["check"]);
    assert!(checked.status.success(), "{}", combined(&checked));
    let built = project.command(&["build", "--single"]);
    assert!(built.status.success(), "{}", combined(&built));
    let mut dev = varta();
    dev.current_dir(&project.0).arg("dev");
    let mut run = varta();
    run.current_dir(&project.0).args(["run", "dist/app.wasm"]);
    let mut native = Command::new(project.0.join("dist/app"));
    native.current_dir(&project.0);
    for mut command in [dev, run, native] {
        let output = command
            .env("VARTA_CAPTURE_ALLOWED", "Збережено 🦀")
            .env("VARTA_CAPTURE_OTHER", "OTHER_VALUE")
            .env("VARTA_CAPTURE_EMPTY", "")
            .env("VARTA_CAPTURE_KEY", "VARTA_CAPTURE_ALLOWED")
            .env("VARTA_CAPTURE_SECRET", "HIDDEN_CAPTURE_VALUE")
            .env_remove("VARTA_CAPTURE_ABSENT")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert!(!text.contains("HIDDEN_CAPTURE_VALUE"), "{text}");
        let expected = [
            "Збережено 🦀",
            "0",
            "OTHER_VALUE",
            "Збережено 🦀",
            "0",
            "0",
            "1",
            "",
            "1",
            "0",
            "",
            "1",
            "0",
            "0",
            "1",
            "0",
            "Збережено 🦀",
            "0",
        ]
        .map(|value| format!("varta log: {value}"))
        .join("\n");
        assert!(text.contains(&expected), "{text}");
    }
}

#[test]
fn captured_filesystem_failures_preserve_all_status_codes_and_empty_success() {
    use varta_common::abi;
    let project = RegressionProject::new("export function main():void{}", serde_json::json!({}));
    let root = project.0.canonicalize().unwrap();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("empty.txt"), []).unwrap();
    fs::write(data.join("invalid.txt"), [0xff]).unwrap();
    fs::write(
        data.join("large.txt"),
        vec![b'X'; abi::MAX_RESULT_BYTES + 1],
    )
    .unwrap();
    fs::write(root.join("secret.txt"), "UNREAD_CAPTURE_SECRET").unwrap();
    project.set_capabilities(serde_json::json!({"fs":{"read":[format!("{}/*",data.display())]}}));
    let source = format!(
        r#"
function read(path:string):VartaResult<string>{{return varta.result.capture(varta.fs.read(path));}}
export function main():void{{
 const empty=read({});const denied=read({});const invalid=read({});const missing=read({});const large=read({});
 console.log(empty.ok);console.log(empty.status);console.log(empty.value);
 console.log(denied.ok);console.log(denied.status);console.log(denied.value);
 console.log(invalid.status);console.log(missing.status);console.log(large.status);
 if(!denied.ok){{switch(denied.status){{case 1:console.log(101);break;default:console.log(999);}}}}
}}
"#,
        serde_json::to_string(&data.join("empty.txt")).unwrap(),
        serde_json::to_string(&root.join("secret.txt")).unwrap(),
        serde_json::to_string(&data.join("invalid.txt")).unwrap(),
        serde_json::to_string(&data.join("missing.txt")).unwrap(),
        serde_json::to_string(&data.join("large.txt")).unwrap()
    );
    fs::write(project.0.join("src/main.ts"), source).unwrap();
    let output = project.command(&["dev"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("UNREAD_CAPTURE_SECRET"), "{text}");
    assert!(text.contains("varta log: 1\nvarta log: 0\nvarta log: \nvarta log: 0\nvarta log: 1\nvarta log: \nvarta log: 2\nvarta log: 3\nvarta log: 4\nvarta log: 101"),"{text}");
}

#[test]
fn captured_calls_obey_host_ceilings_and_keep_network_and_process_denials_programmatic() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const env=varta.result.capture(varta.env.get("VARTA_CAPTURE_SECRET"));
 const time=varta.result.capture(varta.time.now());
 const fetched=varta.result.capture(varta.net.fetch("http://127.0.0.1:1/secret"));
 const body=varta.result.capture(varta.net.get("http://127.0.0.1:1/secret"));
 const exited=varta.result.capture(varta.process.exit(9));
 console.log(env.status);console.log(time.status);console.log(fetched.status);console.log(body.status);console.log(exited.status);
 console.log(env.value);console.log(time.value);console.log(fetched.value);console.log(body.value);
 console.log(exited.ok);exited.value;
}
"#,
        serde_json::json!({"env":{"get":["VARTA_CAPTURE_SECRET"]},"time":{"now":true},"net":{"fetch":["http://127.0.0.1:1/**"],"get":["http://127.0.0.1:1/**"]},"process":{"exit":true}}),
    );
    project.host_policy(serde_json::json!([]));
    let built = project.command(&["build"]);
    assert!(built.status.success(), "{}", combined(&built));
    for args in [
        vec!["dev", "--policy", "host-policy.json"],
        vec!["run", "dist/app.wasm", "--policy", "host-policy.json"],
    ] {
        let output = varta()
            .current_dir(&project.0)
            .args(args)
            .env("VARTA_CAPTURE_SECRET", "HOST_DENIED_VALUE")
            .output()
            .unwrap();
        let text = combined(&output);
        assert!(output.status.success(), "{text}");
        assert!(!text.contains("HOST_DENIED_VALUE"), "{text}");
        assert!(text.contains("varta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: \nvarta log: 0\nvarta log: -1\nvarta log: \nvarta log: 0"),"{text}");
    }
}

#[test]
fn captured_process_exit_still_ends_execution_with_the_requested_code() {
    let project = RegressionProject::new(
        "export function main():void{const result=varta.result.capture(varta.process.exit(7));console.log(\"AFTER_EXIT\");}",
        serde_json::json!({"process":{"exit":true}}),
    );
    let output = project.command(&["dev"]);
    let text = combined(&output);
    assert_eq!(output.status.code(), Some(7), "{text}");
    assert!(!text.contains("AFTER_EXIT"), "{text}");
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
        "export function main():void { varta.result.capture(42); }",
        "export function main():void { console.log(\"x\"+42); }",
        "export function main():void { console.log(\"x\"<false); }",
        "export function main():void { console.log(\"x\"*\"y\"); }",
        "export function main():void { const r=varta.result.capture(varta.time.now()); r.status=0; }",
        "export function main():void { const r:VartaResult<string>=varta.result.capture(varta.time.now()); }",
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
    assert!(!project.0.join("dist").exists());
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
fn packaging_uses_verified_runtime_and_never_executes_guest_main() {
    let project = RegressionProject::new(
        r#"export function main():void{console.log("PACKAGE_MAIN_EXECUTED");varta.process.exit(99);}"#,
        serde_json::json!({"process":{"exit":true}}),
    );
    let staging = project.0.join("staging");
    fs::create_dir(&staging).unwrap();
    let build = |candidate: Option<&std::path::Path>| {
        let mut command = varta();
        command
            .current_dir(&project.0)
            .args(["build", "--single"])
            .env("TMPDIR", &staging)
            .env("TMP", &staging)
            .env("TEMP", &staging);
        if let Some(candidate) = candidate {
            command.arg("--launcher").arg(candidate);
        }
        let output = command.output().unwrap();
        assert_eq!(
            fs::read_dir(&staging).unwrap().count(),
            0,
            "launcher staging leaked"
        );
        output
    };
    let output = build(None);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(!combined(&output).contains("PACKAGE_MAIN_EXECUTED"));
    let native_path = project.0.join("dist/app");
    let original = fs::read(&native_path).unwrap();
    let original_wasm = fs::read(project.0.join("dist/app.wasm")).unwrap();
    let original_manifest = fs::read(project.0.join("dist/app.manifest.json")).unwrap();
    let launcher = fs::read(runtime_path()).unwrap();
    assert!(original.starts_with(&launcher));
    assert_eq!(
        original.len(),
        launcher.len()
            + fs::metadata(project.0.join("dist/app.wasm")).unwrap().len() as usize
            + fs::metadata(project.0.join("dist/app.manifest.json"))
                .unwrap()
                .len() as usize
            + 32
    );
    let run = Command::new(&native_path).output().unwrap();
    assert_eq!(run.status.code(), Some(99));
    assert!(combined(&run).contains("PACKAGE_MAIN_EXECUTED"));
    let source_path = project.0.join("src/main.ts");
    let source = fs::read(&source_path).unwrap();
    fs::write(
        &source_path,
        "export function main():void{console.log(123);}",
    )
    .unwrap();
    let candidates = [
        PathBuf::from(env!("CARGO_BIN_EXE_varta")),
        native_path.clone(),
        project.0.join("missing-runtime"),
    ];
    for candidate in candidates {
        let output = build(Some(&candidate));
        assert!(!output.status.success(), "{}", combined(&output));
        assert!(!combined(&output).contains("PACKAGE_MAIN_EXECUTED"));
        assert_eq!(fs::read(&native_path).unwrap(), original);
        assert_eq!(
            fs::read(project.0.join("dist/app.wasm")).unwrap(),
            original_wasm
        );
        assert_eq!(
            fs::read(project.0.join("dist/app.manifest.json")).unwrap(),
            original_manifest
        );
    }
    let output = varta()
        .current_dir(&project.0)
        .args(["build", "--launcher"])
        .arg(runtime_path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    fs::write(source_path, source).unwrap();
    let output = build(Some(&runtime_path()));
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(fs::read(native_path).unwrap(), original);
}

#[test]
fn packaging_requires_a_sibling_companion_or_explicit_launcher() {
    let project = RegressionProject::new("export function main():void{}", serde_json::json!({}));
    let tools = project.0.join("tools");
    fs::create_dir(&tools).unwrap();
    let cli = tools.join(format!("varta{}", std::env::consts::EXE_SUFFIX));
    fs::copy(env!("CARGO_BIN_EXE_varta"), &cli).unwrap();
    // A runtime in cwd must not silently satisfy the installed companion lookup.
    fs::copy(
        runtime_path(),
        project.0.join(runtime_path().file_name().unwrap()),
    )
    .unwrap();
    let output = Command::new(&cli)
        .current_dir(&project.0)
        .args(["build", "--single"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(combined(&output).contains("pass --launcher PATH"));
    assert!(!project.0.join("dist").exists());
    let output = Command::new(cli)
        .current_dir(&project.0)
        .args(["build", "--single", "--launcher"])
        .arg(runtime_path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
}

#[test]
fn independent_runtime_invokes_tools_offline_and_preserves_the_host_ceiling() {
    let project = RegressionProject::new(
        r#"export function read(name:string):string{return varta.env.get(name);}
export function main():void{varta.process.exit(99);}
varta.tool({name:"read",description:"Read an explicitly permitted variable"},read);"#,
        serde_json::json!({"env":{"get":["VARTA_LAUNCHER_VALUE"]},"process":{"exit":true}}),
    );
    project.host_policy(serde_json::json!([]));
    let output = project.command(&["build", "--single"]);
    assert!(output.status.success(), "{}", combined(&output));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for (binary, prefix) in [
        (runtime_path(), vec!["invoke", "dist/app.wasm", "read"]),
        (runtime_path(), vec!["invoke", "dist/app", "read"]),
        (project.0.join("dist/app"), vec!["--invoke", "read"]),
    ] {
        for restricted in [false, true] {
            let mut command = Command::new(&binary);
            command
                .current_dir(&project.0)
                .args(&prefix)
                .args(["--params", r#"{"name":"VARTA_LAUNCHER_VALUE"}"#, "--json"])
                .env("VARTA_LAUNCHER_VALUE", "RUNTIME_VALUE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                value["result"]["value"],
                if restricted { "" } else { "RUNTIME_VALUE" }
            );
        }
        let output = Command::new(binary)
            .current_dir(&project.0)
            .args(prefix)
            .args(["--params", "{}", "--json"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["code"], "invalid_arguments");
    }
    let output = Command::new(project.0.join("dist/app"))
        .arg("--inspect")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["module"]["toolCatalog"]["tools"][0]["name"], "read");
    assert_eq!(
        value["toolCatalog"]["tools"][0]["inputSchema"]["properties"]["name"]["type"],
        "string"
    );
}

#[test]
fn language_profiles_are_discovered_and_rejected_before_execution_in_every_artifact_entry_point() {
    let project = RegressionProject::new(
        r#"export function read():string{return varta.fs.read("input.txt");}
export function main():void{console.log(read());}
varta.tool({name:"read",description:"Read a granted test file"},read);"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("input.txt"), "PROFILE_EXECUTION_MARKER").unwrap();
    let source_path = project.0.join("src/main.ts");
    let source = fs::read_to_string(&source_path).unwrap().replace(
        "\"input.txt\"",
        &serde_json::to_string(&project.0.join("input.txt")).unwrap(),
    );
    fs::write(source_path, source).unwrap();
    project.set_capabilities(serde_json::json!({"fs":{"read":[project.0.join("input.txt")]}}));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let run = project.command(&["run", "dist/app.wasm"]);
    assert!(run.status.success(), "{}", combined(&run));
    assert!(
        combined(&run).contains("PROFILE_EXECUTION_MARKER"),
        "{}",
        combined(&run)
    );
    let expected = serde_json::json!({"profile":"checked-i32-utf8-v1","origin":"declared"});
    for artifact in ["dist/app.wasm", "dist/app"] {
        let output = project.command(&["inspect", artifact, "--json"]);
        assert!(output.status.success(), "{}", combined(&output));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["language"], expected);
        let output = Command::new(runtime_path())
            .current_dir(&project.0)
            .args(["inspect", artifact, "--json"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["module"]["language"], expected);
        let path = project.0.join(artifact);
        let mut bytes = fs::read(&path).unwrap();
        let profile = b"checked-i32-utf8-v1";
        let payload = if artifact.ends_with(".wasm") {
            0
        } else {
            let footer = bytes.len() - 32;
            let wasm = u64::from_le_bytes(bytes[footer..footer + 8].try_into().unwrap()) as usize;
            let manifest =
                u64::from_le_bytes(bytes[footer + 8..footer + 16].try_into().unwrap()) as usize;
            footer - wasm - manifest
        };
        let index = payload
            + bytes[payload..]
                .windows(profile.len())
                .position(|s| s == profile)
                .unwrap();
        bytes[index] = b'X'; // unknown profile with unchanged section/footer lengths
        fs::write(path, bytes).unwrap();
        for command in ["run", "inspect", "validate"] {
            let output = project.command(&[command, artifact]);
            assert!(!output.status.success(), "{command}: {}", combined(&output));
            assert!(combined(&output).contains("invalid language profile"));
            assert!(!combined(&output).contains("PROFILE_EXECUTION_MARKER"));
        }
        let output = project.command(&["invoke", artifact, "read", "--json"]);
        assert!(!output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["code"], "invalid_artifact");
        let output = Command::new(runtime_path())
            .current_dir(&project.0)
            .args(["run", artifact])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(combined(&output).contains("invalid language profile"));
    }
    for args in [
        vec![],
        vec!["--inspect"],
        vec!["--invoke", "read", "--json"],
    ] {
        let output = Command::new(project.0.join("dist/app"))
            .current_dir(&project.0)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(combined(&output).contains("invalid language profile"));
    }
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
    fn language_profile(&self, name: &str) {
        let path = self.0.join("varta.config.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["languageProfile"] = serde_json::json!(name);
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    }
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

struct HttpChild {
    child: std::process::Child,
    address: std::net::SocketAddr,
    reader: Option<std::thread::JoinHandle<String>>,
}

#[test]
fn binary64_profile_runs_across_modules_and_source_free_portable_and_native_targets() {
    let project = RegressionProject::new(
        r#"
import {divide} from "./numbers.ts";
export function main():void{console.log(divide(7,2));console.log(2147483647+1);console.log(1/-0);console.log(NaN||4.5);console.log(-5.5%2);}
"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/numbers.ts"),
        "export function divide(a:number,b:number):number{return a/b;}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf8-v1");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let output = project.command(&["inspect", "dist/app.wasm", "--json"]);
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(metadata["abiVersion"], 2);
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf8-v1");
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected =
        ["3.5", "2147483648", "-Infinity", "4.5", "-1.5"].map(|s| format!("varta log: {s}"));
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .current_dir(&project.0)
                .args(["run", artifact])
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            assert_eq!(
                &String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(expected.len())
                    .collect::<Vec<_>>(),
                &expected.iter().map(String::as_str).collect::<Vec<_>>()
            );
        }
    }
    let output = Command::new(project.0.join("dist/app"))
        .current_dir(&project.0)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
    // An absent sidecar still means no grants and uses the declared code ABI.
    fs::remove_file(project.0.join("dist/app.manifest.json")).unwrap();
    let output = project.command(&["run", "dist/app.wasm"]);
    assert!(output.status.success(), "{}", combined(&output));
    fs::write(
        project.0.join("dist/app.manifest.json"),
        r#"{"abiVersion":1,"capabilities":[]}"#,
    )
    .unwrap();
    for command in ["run", "inspect", "validate"] {
        let output = project.command(&[command, "dist/app.wasm"]);
        assert!(!output.status.success());
        assert!(combined(&output).contains("manifest and Wasm host ABI versions differ"));
    }
    let output = project.command(&["invoke", "dist/app.wasm", "anything", "--json"]);
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["code"], "invalid_artifact");
}

#[test]
fn binary64_process_and_environment_calls_preserve_exact_checks_and_host_ceilings() {
    let project = RegressionProject::new(
        r#"export function main():void{
 console.log(varta.env.get("VARTA_F64_SECRET"));
 const r=varta.result.capture(varta.process.exit(7.5));console.log(r.status);
 console.log(9.5);
}"#,
        serde_json::json!({"process":{"exit":true},"env":{"get":["VARTA_F64_SECRET"]}}),
    );
    project.language_profile("checked-f64-utf8-v1");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        let native = Command::new(project.0.join("dist/app"));
        for mut command in [dev, wasm, runtime, native] {
            command
                .current_dir(&project.0)
                .env("VARTA_F64_SECRET", "F64_SECRET_VALUE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("F64_SECRET_VALUE"), !restricted, "{text}");
            assert!(
                text.contains(if restricted {
                    "capability denied: process.exit"
                } else {
                    "invalid host input"
                }),
                "{text}"
            );
            assert!(
                text.contains(if restricted {
                    "varta log: 1\nvarta log: 9.5"
                } else {
                    "varta log: 2\nvarta log: 9.5"
                }),
                "{text}"
            );
        }
    }
}

#[test]
fn binary64_unimplemented_boundaries_are_rejected_consistently_before_outputs() {
    for source in [
        "export function main():void{console.log(undefined);}",
        "export function main():void{console.log(1.5=='1.5');}",
        "export function f(n:number):number{return n;}export function main():void{}varta.tool({name:'f',description:'f'},f);",
        "export function f(r:VartaHttpRequest):VartaHttpResponse{return {status:200,body:r.body,headers:[]};}export function main():void{}varta.http.serve({hostname:'127.0.0.1',port:0},f);",
    ] {
        let project = RegressionProject::new(source, serde_json::json!({}));
        project.language_profile("checked-f64-utf8-v1");
        for command in ["check", "dev", "build"] {
            let output = project.command(&[command]);
            assert!(!output.status.success(), "{command}: {source}");
        }
        assert!(!project.0.join("dist").exists());
    }
    let project = RegressionProject::new("export function main():void{}", serde_json::json!({}));
    project.language_profile("ecmascript-2025-v1");
    assert!(!project.command(&["check"]).status.success());
}

#[test]
fn reference_unions_run_in_source_free_native_and_wasm_artifacts() {
    let project = RegressionProject::new(
        r#"
import {first,keep} from './values.ts';
export function main():void{
 const object={n:1.5};const alias=first([object]);if(alias!==undefined){alias.n=2.5;}
 console.log(keep(object)===object);console.log(JSON.stringify(alias));const array=keep([3.5]);
 if(Array.isArray(array)){console.log(array[0]);}console.log(first([]));
 console.log(JSON.stringify(varta.json.parse<{n:number}>('{"n":1.25}')));
}"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/values.ts"),
        r#"
export function keep(v:{n:number}|number[]|undefined):{n:number}|number[]|undefined{return v;}
export function first(v:{n:number}[]):{n:number}|undefined{return v[0];}
"#,
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v4");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let metadata: serde_json::Value = serde_json::from_slice(
        &project
            .command(&["inspect", "dist/app.wasm", "--json"])
            .stdout,
    )
    .unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v4");
    assert_eq!(metadata["abiVersion"], 7);
    assert_eq!(metadata["valueAbiVersion"], 8);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: 1",
        r#"varta log: {"n":2.5}"#,
        "varta log: 3.5",
        "varta log: undefined",
        r#"varta log: {"n":1.25}"#,
    ];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .current_dir(&project.0)
                .args(["run", artifact])
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(expected.len())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    let output = Command::new(project.0.join("dist/app")).output().unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn reference_guards_preserve_system_authority_and_status_under_host_ceilings() {
    let project = RegressionProject::new(
        r#"export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_REFERENCE_SECRET"));const owner:{child?:{name:string}}={};
 if(result.ok){owner.child={name:result.value};}const child=owner.child;
 console.log(typeof child);console.log(Array.isArray(child));console.log(JSON.stringify(owner));console.log(varta.result.status());
 if(typeof child==="object"){console.log(child.name);}
 const denied=varta.result.capture(varta.env.has("VARTA_REFERENCE_SECRET"));console.log(denied.status);
}"#,
        serde_json::json!({"env":{"get":["VARTA_REFERENCE_SECRET"]}}),
    );
    project.language_profile("checked-f64-utf16-v4");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_REFERENCE_SECRET", "Привіт 😀");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("Привіт 😀"), !restricted, "{text}");
            assert!(text.contains(if restricted{"varta log: undefined\nvarta log: 0\nvarta log: {}\nvarta log: 1"}else{"varta log: object\nvarta log: 0\nvarta log: {\"child\":{\"name\":\"Привіт 😀\"}}\nvarta log: 0"}),"{text}");
            assert!(
                text.contains("<capability denied: env> (VARTA_REFERENCE_SECRET)"),
                "{text}"
            );
        }
    }
}

#[test]
fn missing_profile_runs_optional_fields_and_nullable_reads_without_source_files() {
    let project = RegressionProject::new(
        r#"
import {create,read} from './values.ts';
export function main():void {const value=create();console.log(Object.hasOwn(value,"label"));value.label="😀";
 console.log(Object.hasOwn(value,"label"));console.log(JSON.stringify(value));console.log(read(["kept"],1));
 console.log(JSON.stringify("😀"[1]));console.log(value.unknown);let empty:number|undefined;console.log(empty);
}
"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/values.ts"),
        r#"
export function create():{label?:string;n:number}{return {n:1.5};}
export function read(values:string[],index:number):string|undefined{return values[index];}
"#,
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v3");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let metadata: serde_json::Value = serde_json::from_slice(
        &project
            .command(&["inspect", "dist/app.wasm", "--json"])
            .stdout,
    )
    .unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v3");
    assert_eq!(metadata["abiVersion"], 6);
    assert_eq!(metadata["valueAbiVersion"], 7);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: 0",
        "varta log: 1",
        r#"varta log: {"n":1.5,"label":"😀"}"#,
        "varta log: undefined",
        r#"varta log: "\ude00""#,
        "varta log: undefined",
        "varta log: undefined",
    ];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .current_dir(&project.0)
                .args(["run", artifact])
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(expected.len())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    let output = Command::new(project.0.join("dist/app")).output().unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn missing_profile_preserves_host_ceilings_and_status_while_building_optional_fields() {
    let project = RegressionProject::new(
        r#"export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_MISSING_SECRET"));const value:{text?:string}={};
 if(result.ok){value.text=result.value;}
 console.log(Object.hasOwn(value,"text"));console.log(JSON.stringify(value));console.log(varta.result.status());
 const missing=""[0];console.log(missing);console.log(varta.result.status());
 const name:string|undefined="VARTA_MISSING_SECRET";
 if(typeof name==="string"){const denied=varta.result.capture(varta.env.has(name));console.log(denied.status);}
}"#,
        serde_json::json!({"env":{"get":["VARTA_MISSING_SECRET"]}}),
    );
    project.language_profile("checked-f64-utf16-v3");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_MISSING_SECRET", "Привіт 😀");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("Привіт 😀"), !restricted, "{text}");
            assert!(text.contains(if restricted {"varta log: 0\nvarta log: {}\nvarta log: 1\nvarta log: undefined\nvarta log: 1"}else{"varta log: 1\nvarta log: {\"text\":\"Привіт 😀\"}\nvarta log: 0\nvarta log: undefined\nvarta log: 0"}),"{text}");
            assert!(
                text.contains("<capability denied: env> (VARTA_MISSING_SECRET)"),
                "{text}"
            );
        }
    }
}

#[test]
fn primitive_unions_run_across_modules_in_source_free_native_and_wasm_artifacts() {
    let project = RegressionProject::new(
        r#"
import {keep,label} from './values.ts';
export function main():void {
 console.log(label(keep(1.5)));console.log(label(keep("😀")));console.log(label(keep(undefined)));
 const values:(number|string|undefined)[]=[keep("\ud800"),keep(undefined)];console.log(JSON.stringify(values));
 console.log(JSON.stringify(keep(undefined)));console.log(keep(-0)===0);
}"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/values.ts"),
        r#"
export function keep(v:number|string|undefined):number|string|undefined{return v;}
export function label(v:number|string|undefined):string {
 if(typeof v==="number"){return JSON.stringify(v+0.5);}
 return typeof v==="string"?v+"!":"missing";
}"#,
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v2");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let metadata: serde_json::Value = serde_json::from_slice(
        &project
            .command(&["inspect", "dist/app.wasm", "--json"])
            .stdout,
    )
    .unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v2");
    assert_eq!(metadata["abiVersion"], 5);
    assert_eq!(metadata["valueAbiVersion"], 6);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: 2",
        "varta log: 😀!",
        "varta log: missing",
        r#"varta log: ["\ud800",null]"#,
        "varta log: undefined",
        "varta log: 1",
    ];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .current_dir(&project.0)
                .args(["run", artifact])
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(expected.len())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    let output = Command::new(project.0.join("dist/app")).output().unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn primitive_union_narrowing_preserves_host_permissions_and_system_status() {
    let project = RegressionProject::new(
        r#"export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_UNION_SECRET"));
 const value:string|undefined=result.ok?result.value:undefined;
 console.log(typeof value);console.log(JSON.stringify(value));console.log(!value);console.log(varta.result.status());
 const name:string|number="VARTA_UNION_SECRET";
 if(typeof name==="string"){const denied=varta.result.capture(varta.env.has(name));console.log(denied.status);}
}"#,
        serde_json::json!({"env":{"get":["VARTA_UNION_SECRET"]}}),
    );
    project.language_profile("checked-f64-utf16-v2");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_UNION_SECRET", "Привіт 😀");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("Привіт 😀"), !restricted, "{text}");
            assert!(
                text.contains(if restricted {
                    "varta log: undefined\nvarta log: undefined\nvarta log: 1\nvarta log: 1"
                } else {
                    "varta log: string\nvarta log: \"Привіт 😀\"\nvarta log: 0\nvarta log: 0"
                }),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_UNION_SECRET)"),
                "{text}"
            );
        }
    }
}

#[test]
fn utf16_profile_runs_modules_and_json_in_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {unit} from './text.ts';
export function main():void{
 const text="😀";console.log(text.length);console.log(JSON.stringify(unit(text)));
 console.log("\ud83d"+"\ude00"===text);
 console.log(JSON.stringify(varta.json.parse<{text:string}>('{"text":"\\ud800"}')));
 console.log(JSON.stringify(undefined));
}"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/text.ts"),
        "export function unit(s:string):string{return s.charAt(0);}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v1");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let metadata: serde_json::Value = serde_json::from_slice(
        &project
            .command(&["inspect", "dist/app.wasm", "--json"])
            .stdout,
    )
    .unwrap();
    assert_eq!(metadata["abiVersion"], 4);
    assert_eq!(metadata["valueAbiVersion"], 5);
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v1");
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: 2",
        r#"varta log: "\ud83d""#,
        "varta log: 1",
        r#"varta log: {"text":"\ud800"}"#,
        "varta log: undefined",
    ];
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn utf16_outbound_network_converts_text_and_denies_before_connecting() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/text", listener.local_addr().unwrap());
    let project = RegressionProject::new(
        &format!(
            r#"export function main():void{{
 console.log(JSON.stringify(varta.net.get({url:?})));console.log(varta.net.fetch({url:?}));
 const invalid=varta.result.capture(varta.net.get("http://127.0.0.1/\ud800"));console.log(invalid.status);
}}"#
        ),
        serde_json::json!({"net":{"get":[url],"fetch":[url]}}),
    );
    project.language_profile("checked-f64-utf16-v1");
    project.host_policy(serde_json::json!([]));
    let denied = project.command(&["dev", "--policy", "host-policy.json"]);
    assert!(denied.status.success(), "{}", combined(&denied));
    assert!(combined(&denied).contains("capability denied: net.get"));
    assert!(combined(&denied).contains("capability denied: net.fetch"));
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut count = 0;
        while count < 2 && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 512];
                    while !request.windows(4).any(|s| s == b"\r\n\r\n") {
                        let read = socket.read(&mut buffer).unwrap();
                        assert_ne!(read, 0);
                        request.extend_from_slice(&buffer[..read]);
                        assert!(request.len() < 4096);
                    }
                    let body = "Мережа 😀";
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                    count += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(e) => panic!("{e}"),
            }
        }
        count
    });
    let output = project.command(&["dev"]);
    let count = worker.join().unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(count, 2);
    let text = combined(&output);
    assert!(
        text.contains("varta log: \"Мережа 😀\"\nvarta log: 200"),
        "{text}"
    );
    assert!(text.contains("varta log: 2"), "{text}");
}

#[test]
fn utf16_system_strings_round_trip_and_obey_operation_and_host_ceiling_checks() {
    let project = RegressionProject::new(
        r#"export function main():void{
 const name="VARTA_UTF16_VALUE";console.log(JSON.stringify(varta.env.get(name)));console.log(varta.env.has(name));
 console.log(varta.env.has("VARTA_UTF16_PRESENT"));varta.env.print(name);
 const path=varta.env.get("VARTA_UTF16_PATH");const value=varta.result.capture(varta.fs.read(path));
 console.log(JSON.stringify(value.value));console.log(value.status);varta.fs.print(path);
 const invalid=varta.result.capture(varta.env.get("VARTA\ud800"));console.log(invalid.status);
 const nul=varta.result.capture(varta.env.has("VARTA\u0000NAME"));console.log(nul.status);
 console.log(JSON.stringify("\ud800"));console.log(varta.result.status());
}"#,
        serde_json::json!({}),
    );
    let file = project.0.canonicalize().unwrap().join("дані😀.txt");
    fs::write(&file, "Текст 😀").unwrap();
    project.set_capabilities(serde_json::json!({"env":{"get":["VARTA_UTF16_VALUE","VARTA_UTF16_PATH"],"has":["VARTA_UTF16_PRESENT"],"print":["VARTA_UTF16_VALUE"]},"fs":{"read":[file],"print":[file]}}));
    project.language_profile("checked-f64-utf16-v1");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_UTF16_VALUE", "Привіт 😀")
                .env("VARTA_UTF16_PRESENT", "present")
                .env("VARTA_UTF16_PATH", &file);
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("Привіт 😀"), !restricted, "{text}");
            assert_eq!(text.contains("Текст 😀"), !restricted, "{text}");
            // env.get grants must not authorize env.has.
            assert!(
                text.contains("<capability denied: env> (VARTA_UTF16_VALUE)"),
                "{text}"
            );
            assert_eq!(
                text.lines().filter(|s| *s == "varta log: 2").count(),
                3,
                "{text}"
            );
            assert!(text.contains(r#"varta log: "\ud800""#), "{text}");
        }
    }
}

#[test]
fn undefined_profile_runs_modules_and_json_in_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {empty} from './values.ts';
export function main():void {
 const u=empty();console.log(typeof u);console.log(JSON.stringify({u,n:1.5}));
 console.log(JSON.stringify([u,undefined]));console.log(JSON.stringify(u));
 console.log(JSON.stringify(varta.json.parse<{n:number}>('{"n":2.5}')));
}"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/values.ts"),
        "export function empty():void{}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf8-v2");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let inspected = project.command(&["inspect", "dist/app.wasm", "--json"]);
    let metadata: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(metadata["abiVersion"], 3);
    assert_eq!(metadata["valueAbiVersion"], 4);
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf8-v2");
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: undefined",
        r#"varta log: {"n":1.5}"#,
        "varta log: [null,null]",
        "varta log: undefined",
        r#"varta log: {"n":2.5}"#,
    ];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .current_dir(&project.0)
                .args(["run", artifact])
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(expected.len())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    let output = Command::new(project.0.join("dist/app"))
        .current_dir(&project.0)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
    let path = project.0.join("dist/app.wasm");
    let mut bytes = fs::read(&path).unwrap();
    let name = varta_common::value::VALUE_SECTION.as_bytes();
    let at = bytes.windows(name.len()).position(|s| s == name).unwrap() + name.len();
    bytes[at..at + 4].copy_from_slice(&varta_common::value::NUMBER_VALUE_VERSION.to_le_bytes());
    fs::write(path, bytes).unwrap();
    for command in ["run", "inspect", "validate"] {
        let output = project.command(&[command, "dist/app.wasm"]);
        assert!(!output.status.success());
        assert!(combined(&output).contains("undefined profile requires value ABI v4"));
    }
}

#[test]
fn undefined_from_void_host_calls_preserves_capability_and_host_policy_checks() {
    let project = RegressionProject::new(
        r#"export function main():void {
 const r=varta.result.capture(varta.env.print("VARTA_UNDEFINED_VALUE"));
 console.log(JSON.stringify({value:r.value,status:r.status}));console.log(JSON.stringify(r.value));console.log(varta.result.status());
}"#,
        serde_json::json!({"env":{"print":["VARTA_UNDEFINED_VALUE"]}}),
    );
    project.language_profile("checked-f64-utf8-v2");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        let native = Command::new(project.0.join("dist/app"));
        for mut command in [dev, wasm, runtime, native] {
            command
                .current_dir(&project.0)
                .env("VARTA_UNDEFINED_VALUE", "UNDEFINED_SECRET_VALUE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(
                text.contains("UNDEFINED_SECRET_VALUE"),
                !restricted,
                "{text}"
            );
            assert!(
                text.contains(if restricted {
                    "varta log: {\"status\":1}\nvarta log: undefined\nvarta log: 1"
                } else {
                    "varta log: {\"status\":0}\nvarta log: undefined\nvarta log: 0"
                }),
                "{text}"
            );
        }
    }
}

#[test]
fn binary64_aggregates_and_checked_json_run_in_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {parse} from "./values.ts";
export function main():void{
 const object=parse('{"n":-0,"items":[1.25,2.5]}');const alias=object;alias.items[0]=alias.items[0]+0.5;
 console.log(1/object.n);console.log(JSON.stringify(object));
}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/values.ts"),r#"export function parse(text:string):{n:number;items:number[]}{return varta.json.parse<{n:number;items:number[]}>(text);}"#).unwrap();
    project.language_profile("checked-f64-utf8-v1");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let inspection = project.command(&["inspect", "dist/app.wasm", "--json"]);
    let metadata: serde_json::Value = serde_json::from_slice(&inspection.stdout).unwrap();
    assert_eq!(metadata["abiVersion"], 2);
    assert_eq!(metadata["valueAbiVersion"], 3);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .current_dir(&project.0)
                .args(["run", artifact])
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            let text = String::from_utf8(output.stdout).unwrap();
            let mut lines = text.lines();
            assert_eq!(lines.next().unwrap(), "varta log: -Infinity");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(
                    lines.next().unwrap().strip_prefix("varta log: ").unwrap()
                )
                .unwrap(),
                serde_json::json!({"n":0,"items":[1.75,2.5]})
            );
        }
    }
    let native = Command::new(project.0.join("dist/app"))
        .current_dir(&project.0)
        .output()
        .unwrap();
    assert!(native.status.success(), "{}", combined(&native));
    assert!(combined(&native).starts_with("varta log: -Infinity"));
    let path = project.0.join("dist/app.wasm");
    let mut bytes = fs::read(&path).unwrap();
    let name = varta_common::value::VALUE_SECTION.as_bytes();
    let at = bytes.windows(name.len()).position(|s| s == name).unwrap() + name.len();
    bytes[at..at + 4].copy_from_slice(&varta_common::value::VALUE_VERSION.to_le_bytes());
    fs::write(path, bytes).unwrap();
    for command in ["run", "inspect", "validate"] {
        let output = project.command(&[command, "dist/app.wasm"]);
        assert!(!output.status.success());
        assert!(combined(&output).contains("binary64 requires value ABI v3"));
    }
    let output = project.command(&["invoke", "dist/app.wasm", "any", "--json"]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["code"],
        "invalid_artifact"
    );
}

#[test]
fn binary64_json_preserves_status_and_cannot_turn_parsed_names_into_permission() {
    let project = RegressionProject::new(
        r#"export function main():void {
 const allowed=varta.env.get("VARTA_JSON_ALLOWED");
 const data=varta.json.parse<{key:string;n:number}>('{"key":"VARTA_JSON_HIDDEN","n":1.25}');
 const hidden=varta.result.capture(varta.env.get(data.key));
 console.log(JSON.stringify({status:hidden.status,n:data.n}));console.log(varta.result.status());console.log(allowed);
}"#,
        serde_json::json!({"env":{"get":["VARTA_JSON_ALLOWED"]}}),
    );
    project.language_profile("checked-f64-utf8-v1");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        let native = Command::new(project.0.join("dist/app"));
        for mut command in [dev, wasm, runtime, native] {
            command
                .current_dir(&project.0)
                .env("VARTA_JSON_ALLOWED", "ALLOWED_JSON_VALUE")
                .env("VARTA_JSON_HIDDEN", "HIDDEN_JSON_VALUE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert!(!text.contains("HIDDEN_JSON_VALUE"));
            assert_eq!(text.contains("ALLOWED_JSON_VALUE"), !restricted, "{text}");
            assert!(
                text.contains("varta log: {\"status\":1,\"n\":1.25}\nvarta log: 1"),
                "{text}"
            );
        }
    }
}
impl HttpChild {
    fn start(mut command: Command) -> Self {
        use std::{io::BufRead, process::Stdio};
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut output = String::new();
            for line in std::io::BufReader::new(stderr).lines() {
                let line = line.unwrap();
                if let Some(address) = line.strip_prefix("varta listening: http://") {
                    let _ = sender.send(address.parse::<std::net::SocketAddr>().unwrap());
                }
                output.push_str(&line);
                output.push('\n');
            }
            output
        });
        let ready = receiver.recv_timeout(std::time::Duration::from_secs(15));
        if ready.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("HTTP readiness failed: {}", reader.join().unwrap());
        }
        Self {
            child,
            address: ready.unwrap(),
            reader: Some(reader),
        }
    }
    fn request(&self, path: &str) -> (u16, String) {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(15)))
            .unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: test\r\n\r\n").unwrap();
        let mut output = String::new();
        stream.read_to_string(&mut output).unwrap();
        let (headers, body) = output.split_once("\r\n\r\n").unwrap();
        (
            headers.split_whitespace().nth(1).unwrap().parse().unwrap(),
            body.to_owned(),
        )
    }
    fn finish(mut self) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let stderr = self.reader.take().unwrap().join().unwrap();
                assert!(status.success(), "{stderr}");
                return;
            }
            assert!(std::time::Instant::now() < until, "HTTP did not shut down");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
impl Drop for HttpChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn http_servers_work_in_dev_wasm_and_source_free_native_artifacts() {
    let project = RegressionProject::new(
        r#"
function handle(r:VartaHttpRequest):VartaHttpResponse{
 if(r.target=="/stop"){varta.http.stop();return {status:200,body:"bye",headers:[]};}
 return {status:200,body:JSON.stringify({ok:true,target:r.target}),headers:[{name:"content-type",value:"application/json"}]};
}
export function main():void{}
varta.http.serve({hostname:"127.0.0.1",port:0},handle);
"#,
        serde_json::json!({"net":{"listen":["127.0.0.1:0"]}}),
    );
    for args in [&["check"][..], &["build", "--single"][..]] {
        let output = project.command(args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    for artifact in ["dist/app.wasm", "dist/app"] {
        let output = project.command(&["inspect", artifact, "--json"]);
        assert!(output.status.success(), "{}", combined(&output));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value["toolCatalog"].is_null());
        assert_eq!(value["httpService"]["bind"], "127.0.0.1:0");
    }
    let mut dev = varta();
    dev.current_dir(&project.0).arg("dev");
    let dev = HttpChild::start(dev);
    assert_eq!(dev.request("/first").0, 200);
    assert_eq!(dev.request("/stop").1, "bye");
    dev.finish();
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let deployment = project.0.join("deployment");
    fs::create_dir(&deployment).unwrap();
    let deployed = deployment.join("server");
    fs::rename(project.0.join("dist/app"), &deployed).unwrap();
    // Only the self-contained executable is distributed: no CLI, companion,
    // sidecar, source or project configuration in the deployment directory.
    assert_eq!(fs::read_dir(&deployment).unwrap().count(), 1);
    let inspected = Command::new(&deployed)
        .current_dir(&deployment)
        .arg("--inspect")
        .output()
        .unwrap();
    assert!(inspected.status.success(), "{}", combined(&inspected));
    let mut wasm = varta();
    wasm.current_dir(&project.0).args(["run", "dist/app.wasm"]);
    let mut independent = Command::new(runtime_path());
    independent
        .current_dir(&project.0)
        .args(["run", "dist/app.wasm"]);
    let mut native = Command::new(&deployed);
    native.current_dir(&deployment);
    for command in [wasm, independent, native] {
        let server = HttpChild::start(command);
        let (status, body) = server.request("/json");
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"ok":true,"target":"/json"})
        );
        assert_eq!(server.request("/stop").0, 200);
        server.finish();
    }
}
#[test]
fn http_cli_listener_denials_precede_bind_and_host_policy_cannot_expand_requests() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = held.local_addr().unwrap();
    let source = format!(
        "function handle(r:VartaHttpRequest):VartaHttpResponse{{return {{status:200,body:r.body,headers:[]}};}}export function main():void{{}}varta.http.serve({{hostname:'127.0.0.1',port:{}}},handle);",
        address.port()
    );
    let project = RegressionProject::new(
        &source,
        serde_json::json!({"net":{"get":[format!("http://{address}/**")]}}),
    );
    let output = project.command(&["dev"]);
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("HTTP listener denied"),
        "{}",
        combined(&output)
    );
    project.set_capabilities(serde_json::json!({"net":{"listen":[address.to_string()]}}));
    project.host_policy(serde_json::json!([]));
    let output = project.command(&["dev", "--policy", "host-policy.json"]);
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("HTTP listener denied"),
        "{}",
        combined(&output)
    );
    project.set_capabilities(serde_json::json!({}));
    project.host_policy(serde_json::json!([{"id":"net.listen","patterns":[address.to_string()]}]));
    let output = project.command(&["dev", "--policy", "host-policy.json"]);
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("HTTP listener denied"),
        "{}",
        combined(&output)
    );
}

#[test]
fn checked_json_unions_and_optional_fields_run_without_sources_in_all_artifacts() {
    let project = RegressionProject::new(
        r#"
import {decode} from './values.ts';
export function main():void {
 const value=decode('{"items":[1.25,"😀"],"child":{"n":2.5}}');
 console.log(Object.hasOwn(value,"label"));console.log(JSON.stringify(value));value.label=undefined;
 console.log(Object.hasOwn(value,"label"));const child=value.child;if(child!==undefined){child.n=3.5;}
 const item=value.items[1];if(typeof item==="string"){console.log(item.length);}console.log(JSON.stringify(value));
}"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/values.ts"),
        r#"
export function decode(text:string):{label?:string;items:(number|string)[];child?:{n:number}} {
 return varta.json.parse<{label?:string;items:(number|string)[];child?:{n:number}}>(text);
}"#,
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v5");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let inspection = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspection.status.success(), "{}", combined(&inspection));
    let metadata: serde_json::Value = serde_json::from_slice(&inspection.stdout).unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v5");
    assert_eq!(metadata["abiVersion"], 8);
    assert_eq!(metadata["valueAbiVersion"], 8);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: 0",
        r#"varta log: {"child":{"n":2.5},"items":[1.25,"😀"]}"#,
        "varta log: 1",
        "varta log: 2",
        r#"varta log: {"child":{"n":3.5},"items":[1.25,"😀"]}"#,
    ];
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn checked_json_parsing_preserves_authority_and_rejects_bad_external_values() {
    let project = RegressionProject::new(
        r#"
export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_TYPED_JSON_INPUT"));let text="{}";if(result.ok){text=result.value;}
 const value=varta.json.parse<{child?:{name:string};items?:(number|string)[]}>(text);
 console.log(JSON.stringify(value));console.log(varta.result.status());console.log(result.status);
 const denied=varta.result.capture(varta.env.has("VARTA_TYPED_JSON_INPUT"));console.log(denied.status);
}"#,
        serde_json::json!({"env":{"get":["VARTA_TYPED_JSON_INPUT"]}}),
    );
    project.language_profile("checked-f64-utf16-v5");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command.current_dir(&project.0).env(
                "VARTA_TYPED_JSON_INPUT",
                r#"{"items":[1.25,"😀"],"child":{"name":"VISIBLE"}}"#,
            );
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("VISIBLE"), !restricted, "{text}");
            assert!(text.contains(if restricted {"varta log: {}\nvarta log: 1\nvarta log: 1"} else {"varta log: {\"child\":{\"name\":\"VISIBLE\"},\"items\":[1.25,\"😀\"]}\nvarta log: 0\nvarta log: 0"}),"{text}");
            assert!(
                text.contains("<capability denied: env> (VARTA_TYPED_JSON_INPUT)"),
                "{text}"
            );
        }
    }
    for input in [
        r#"{"items":[true]}"#,
        r#"{"child":{"name":"a","name":"b"}}"#,
        r#"{"child":null}"#,
    ] {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .current_dir(&project.0)
                .args(["run", "dist/app.wasm"])
                .env("VARTA_TYPED_JSON_INPUT", input)
                .output()
                .unwrap();
            assert!(!output.status.success(), "{}", combined(&output));
            assert!(combined(&output).contains("checked JSON"));
        }
        let output = Command::new(project.0.join("dist/app"))
            .env("VARTA_TYPED_JSON_INPUT", input)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{}", combined(&output));
        assert!(combined(&output).contains("checked JSON"));
    }
}

#[test]
fn resizable_arrays_run_across_modules_and_source_free_native_artifacts() {
    let project = RegressionProject::new(
        r#"
import {decode,append} from './values.ts';
export function main():void {
 const values=decode('[{"n":1.25}]');const alias=append(values);console.log(alias===values);
 const removed=alias.pop();if(removed!==undefined){removed.n=4.5;console.log(JSON.stringify(removed));}
 console.log(JSON.stringify(values));console.log(values.length);console.log(values[9]);
}"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/values.ts"),
        r#"
export function decode(text:string):{n:number}[]{return varta.json.parse<{n:number}[]>(text);}
export function append(v:{n:number}[]):{n:number}[]{v.push({n:2.5},{n:3.5});return v;}
"#,
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v6");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let inspection = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspection.status.success(), "{}", combined(&inspection));
    let metadata: serde_json::Value = serde_json::from_slice(&inspection.stdout).unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v6");
    assert_eq!(metadata["abiVersion"], 9);
    assert_eq!(metadata["valueAbiVersion"], 9);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: 1",
        r#"varta log: {"n":4.5}"#,
        r#"varta log: [{"n":1.25},{"n":2.5}]"#,
        "varta log: 2",
        "varta log: undefined",
    ];
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn resizable_array_push_pop_and_json_preserve_permissions_under_host_ceilings() {
    let project = RegressionProject::new(
        r#"
export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_VECTOR_SECRET"));const values:string[]=[];
 if(result.ok){values.push(result.value,"😀");}console.log(JSON.stringify(values));console.log(values.pop());console.log(values.pop());
 const parsed=varta.json.parse<{values?:number[]}>('{"values":[]}');const items=parsed.values;
 if(Array.isArray(items)){items.push(1.25,2.5);console.log(items.pop());}
 console.log(varta.result.status());console.log(result.status);
 const denied=varta.result.capture(varta.env.has("VARTA_VECTOR_SECRET"));console.log(denied.status);
}"#,
        serde_json::json!({"env":{"get":["VARTA_VECTOR_SECRET"]}}),
    );
    project.language_profile("checked-f64-utf16-v6");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_VECTOR_SECRET", "VISIBLE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("VISIBLE"), !restricted, "{text}");
            assert!(text.contains(if restricted{"varta log: []\nvarta log: undefined\nvarta log: undefined\nvarta log: 2.5\nvarta log: 1\nvarta log: 1"}else{"varta log: [\"VISIBLE\",\"😀\"]\nvarta log: 😀\nvarta log: VISIBLE\nvarta log: 2.5\nvarta log: 0\nvarta log: 0"}),"{text}");
            assert!(
                text.contains("<capability denied: env> (VARTA_VECTOR_SECRET)"),
                "{text}"
            );
        }
    }
}

#[test]
fn array_at_and_slice_run_across_modules_and_source_free_native_artifacts() {
    let project = RegressionProject::new(
        r#"
import {copy,last} from './values.ts';
export function main():void {
 const values=varta.json.parse<{n:number}[]>('[{"n":1.25},{"n":2.5}]');const copied=copy(values,undefined,undefined);
 console.log(copied===values);const tail=last(copied);if(tail!==undefined){tail.n=3.5;}
 copied.pop();console.log(JSON.stringify(values));console.log(copied.length);console.log(copy(values,9,undefined).at(-1));
}"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/values.ts"),r#"
export function copy(v:{n:number}[],start:number|undefined,end:number|undefined):{n:number}[]{return v.slice(start,end);}
export function last(v:{n:number}[]):{n:number}|undefined{return v.at(-1);}
"#).unwrap();
    project.language_profile("checked-f64-utf16-v6");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let metadata: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v6");
    assert_eq!(metadata["abiVersion"], 9);
    assert_eq!(metadata["valueAbiVersion"], 9);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = [
        "varta log: 0",
        r#"varta log: [{"n":1.25},{"n":3.5}]"#,
        "varta log: 1",
        "varta log: undefined",
    ];
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn array_at_and_slice_evaluate_privileged_bounds_once_and_obey_host_ceilings() {
    let project = RegressionProject::new(
        r#"
export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_ARRAY_METHOD_SECRET"));const values:string[]=[];
 if(result.ok){values.push(result.value,"😀");}
 const copied=values.slice(void varta.env.print("VARTA_ARRAY_METHOD_SECRET"),void varta.env.print("VARTA_ARRAY_METHOD_SECRET"));
 console.log(JSON.stringify(copied));console.log(copied.at(-1));console.log(copied.at(void varta.env.has("VARTA_ARRAY_METHOD_SECRET")));
 console.log(varta.result.status());console.log(result.status);
}"#,
        serde_json::json!({"env":{"get":["VARTA_ARRAY_METHOD_SECRET"]}}),
    );
    project.language_profile("checked-f64-utf16-v6");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_ARRAY_METHOD_SECRET", "VISIBLE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(text.contains("VISIBLE"), !restricted, "{text}");
            assert_eq!(
                text.matches("<capability denied: env> (VARTA_ARRAY_METHOD_SECRET)")
                    .count(),
                if restricted { 4 } else { 3 },
                "{text}"
            );
            assert!(
                text.contains(if restricted {
                    "varta log: []\nvarta log: undefined"
                } else {
                    "varta log: [\"VISIBLE\",\"😀\"]\nvarta log: 😀"
                }),
                "{text}"
            );
            assert!(
                text.contains(if restricted {
                    "varta log: 1\nvarta log: 1"
                } else {
                    "varta log: 1\nvarta log: 0"
                }),
                "{text}"
            );
        }
    }
}

#[test]
fn array_search_runs_across_modules_and_source_free_native_artifacts() {
    let project = RegressionProject::new(
        r#"
import {find,hasNaN} from './search.ts';
export function main():void {
 const v=varta.json.parse<{n:number}[]>('[{"n":1.25},{"n":2.5}]');const alias=v.at(-1);
 console.log(find(v,alias,undefined));console.log(find(v.slice(),alias,-1));console.log(find(v,{n:2.5},undefined));
 console.log(hasNaN([0/0,-0]));console.log([0/0].indexOf(0/0));console.log(["😀"].includes(""+"😀"));
}"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/search.ts"),r#"
export function find(v:{n:number}[],needle:{n:number}|undefined,from:number|undefined):number{return v.indexOf(needle,from);}
export function hasNaN(v:number[]):boolean{return v.includes(0/0);}
"#).unwrap();
    project.language_profile("checked-f64-utf16-v6");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let metadata: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v6");
    assert_eq!(metadata["abiVersion"], 9);
    assert_eq!(metadata["valueAbiVersion"], 9);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = ["1", "1", "-1", "1", "-1", "1"].map(|s| format!("varta log: {s}"));
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn array_search_obeys_host_ceilings_and_preserves_argument_order_and_system_status() {
    let project = RegressionProject::new(
        r#"
export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_ARRAY_SEARCH_SECRET"));const values:string[]=[];
 if(result.ok){values.push(result.value,"😀");}
 console.log(values.includes(varta.env.get("VARTA_ARRAY_SEARCH_SECRET"),void varta.env.print("VARTA_ARRAY_SEARCH_SECRET")));
 console.log(varta.result.status());console.log(values.indexOf("😀",void varta.env.has("VARTA_ARRAY_SEARCH_SECRET")));
 console.log(varta.result.status());console.log(result.status);
 const empty:string[]=[];console.log(empty.includes(varta.env.get("VARTA_ARRAY_SEARCH_SECRET"),void varta.env.print("VARTA_ARRAY_SEARCH_SECRET")));
}"#,
        serde_json::json!({"env":{"get":["VARTA_ARRAY_SEARCH_SECRET"]}}),
    );
    project.language_profile("checked-f64-utf16-v6");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_ARRAY_SEARCH_SECRET", "VISIBLE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(output.status.success(), "{text}");
            assert_eq!(
                text.matches("<capability denied: env> (VARTA_ARRAY_SEARCH_SECRET)")
                    .count(),
                if restricted { 6 } else { 3 },
                "{text}"
            );
            assert!(
                text.contains(if restricted {
                    "varta log: 0\nvarta log: 1"
                } else {
                    "varta log: 1\nvarta log: 1"
                }),
                "{text}"
            );
            assert!(
                text.contains(if restricted {
                    "varta log: -1\nvarta log: 1\nvarta log: 1"
                } else {
                    "varta log: 1\nvarta log: 1\nvarta log: 0"
                }),
                "{text}"
            );
            assert!(
                !text.contains("VISIBLE"),
                "search must not log its arguments: {text}"
            );
        }
    }
}

#[test]
fn primitive_exceptions_cross_modules_and_run_in_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {fail} from './fail.ts';
export function main():void{const keep={text:"kept"};try{const v=[1+fail("owned")];console.log(JSON.stringify(v));}catch(e){if(typeof e==="string"){console.log(e);}}console.log(keep.text);}
"#,
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/fail.ts"),
        "export function fail(text:string):number{throw text+'😀';}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v7");
    for args in [vec!["check"], vec!["dev"], vec!["build", "--single"]] {
        let output = project.command(&args);
        assert!(output.status.success(), "{}", combined(&output));
    }
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let metadata: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(metadata["language"]["profile"], "checked-f64-utf16-v7");
    assert_eq!(metadata["abiVersion"], 10);
    assert_eq!(metadata["valueAbiVersion"], 9);
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let expected = ["varta log: owned😀", "varta log: kept"];
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn primitive_exceptions_uncaught_values_fail_in_all_runtime_entry_points() {
    let project = RegressionProject::new(
        "export function main():void{throw 'uncaught'+'😀';console.log('BAD_AFTER');}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v7");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut wasm = varta();
    wasm.args(["run", "dist/app.wasm"]);
    let mut runtime = Command::new(runtime_path());
    runtime.args(["run", "dist/app.wasm"]);
    for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
        let output = command.current_dir(&project.0).output().unwrap();
        let text = combined(&output);
        assert!(!output.status.success(), "{text}");
        assert!(
            text.contains("uncaught guest exception: \"uncaught😀\""),
            "{text}"
        );
        assert!(!text.contains("BAD_AFTER"), "{text}");
    }
}

#[test]
fn primitive_exceptions_preserve_host_ceilings_and_fatal_exit_boundaries() {
    let project = RegressionProject::new(
        r#"
export function main():void {
 const result=varta.result.capture(varta.env.get("VARTA_EXCEPTION_SECRET"));
 try{if(!result.ok){throw "denied";}console.log(result.value);}catch(e){console.log(e);}
 console.log(varta.result.status());console.log(result.status);
 try{varta.env.print("VARTA_EXCEPTION_SECRET");console.log(varta.result.status());throw "handled";}catch(e){console.log(e);}
 try{varta.process.exit(7);console.log("after exit");}catch(e){console.log("BAD_CAUGHT_EXIT");}
}

"#,
        serde_json::json!({"env":{"get":["VARTA_EXCEPTION_SECRET"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v7");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_EXCEPTION_SECRET", "VISIBLE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(text.contains("VISIBLE"), !restricted, "{text}");
            assert!(!text.contains("BAD_CAUGHT_EXIT"), "{text}");
            assert!(
                text.contains(if restricted {
                    "varta log: denied\nvarta log: 1\nvarta log: 1"
                } else {
                    "varta log: VISIBLE\nvarta log: 0\nvarta log: 0"
                }),
                "{text}"
            );
            assert!(text.contains("varta log: handled"), "{text}");
            assert_eq!(text.contains("varta log: after exit"), restricted, "{text}");
        }
    }
}

#[test]
fn finally_runs_across_modules_and_preserves_captured_results_in_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {read,fail} from './values.ts';
export function main():void {
 const result=read();console.log(result.value);console.log(result.status);console.log(varta.result.status());
 try{fail();}catch(e){console.log(e);}finally{console.log("entry-final");}
 for(let i=0;i<2;i=i+1){try{continue;}finally{console.log(i);}}
}"#,
        serde_json::json!({"env":{"get":["VARTA_FINALLY_VALUE"]}}),
    );
    fs::write(project.0.join("src/values.ts"), r#"
export function read():VartaResult<string>{try{return varta.result.capture(varta.env.get("VARTA_FINALLY_VALUE"));}finally{varta.env.print("VARTA_FINALLY_VALUE");console.log("return-final");}}
export function fail():void{try{throw "owned"+"😀";}finally{try{throw 1;}catch{}console.log("throw-final");}}
"#).unwrap();
    project.language_profile("checked-f64-utf16-v7");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let expected = [
        "<capability denied: env> (VARTA_FINALLY_VALUE)",
        "return-final",
        "saved😀",
        "0",
        "1",
        "throw-final",
        "owned😀",
        "entry-final",
        "0",
        "1",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut commands = Vec::new();
    let mut dev = varta();
    dev.arg("dev");
    commands.push(dev);
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command
            .current_dir(&project.0)
            .env("VARTA_FINALLY_VALUE", "saved😀")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn finally_does_not_replace_fatal_exit_or_checked_json_failure_in_packaged_execution() {
    for (body, exit) in [
        ("varta.process.exit(7);", Some(7)),
        ("varta.json.parse<{n:number}>('{');", None),
    ] {
        let project = RegressionProject::new(
            &format!(
                "export function main():void{{try{{{body}}}finally{{console.log('BAD_FINALIZER');return;}}}}"
            ),
            serde_json::json!({"process":{"exit":true}}),
        );
        project.language_profile("checked-f64-utf16-v7");
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            let output = command.current_dir(&project.0).output().unwrap();
            let text = combined(&output);
            assert!(!output.status.success(), "{text}");
            if let Some(exit) = exit {
                assert_eq!(output.status.code(), Some(exit), "{text}");
            } else {
                assert!(text.contains("checked JSON"), "{text}");
            }
            assert!(!text.contains("BAD_FINALIZER"), "{text}");
        }
    }
}

#[test]
fn error_objects_cross_modules_and_run_without_sources_in_portable_and_native_artifacts() {
    let project = RegressionProject::new(
        r#"
import {make,fail} from './errors.ts';
export function main():void {
 const error=make();const values:Error[]=[error];values.push(Error("second"));console.log(values.includes(error));
 try{fail(error);}catch(e){if(e instanceof Error){console.log(e===error);console.log(e.name);console.log(e.message);}}finally{console.log("final");}
 console.log(JSON.stringify({errors:values}));console.log(error.toString());
}"#,
        serde_json::json!({"env":{"get":["VARTA_ERROR_MESSAGE"]}}),
    );
    fs::write(
        project.0.join("src/errors.ts"),
        r#"
export function make():Error{return new Error(varta.env.get("VARTA_ERROR_MESSAGE"));}
export function fail(error:Error):void{throw error;}
"#,
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v8");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 11);
    assert_eq!(info["valueAbiVersion"], 10);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v8");
    let expected = [
        "1",
        "1",
        "Error",
        "owned😀",
        "final",
        "{\"errors\":[{},{}]}",
        "Error: owned😀",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut commands = Vec::new();
    let mut dev = varta();
    dev.arg("dev");
    commands.push(dev);
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command
            .current_dir(&project.0)
            .env("VARTA_ERROR_MESSAGE", "owned😀")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn error_objects_preserve_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const result=varta.result.capture(varta.env.get("VARTA_ERROR_SECRET"));
 try{if(!result.ok){throw Error("denied");}console.log(result.value);}catch(e){if(e instanceof Error){console.log(e.toString());}}
 console.log(result.status);console.log(varta.result.status());
 try{varta.process.exit(7);}catch{console.log("BAD_CATCH");}finally{console.log("continued");}
 return;
}"#,
        serde_json::json!({"env":{"get":["VARTA_ERROR_SECRET"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v8");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_ERROR_SECRET", "VISIBLE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(!text.contains("BAD_CATCH"), "{text}");
            if restricted {
                assert!(output.status.success(), "{text}");
                assert!(
                    text.contains("varta log: Error: denied\nvarta log: 1\nvarta log: 1"),
                    "{text}"
                );
                assert!(text.contains("varta log: continued"), "{text}");
                assert!(!text.contains("VISIBLE"), "{text}");
            } else {
                assert_eq!(output.status.code(), Some(7), "{text}");
                assert!(
                    text.contains("varta log: VISIBLE\nvarta log: 0\nvarta log: 0"),
                    "{text}"
                );
                assert!(!text.contains("continued"), "{text}");
            }
        }
    }
}

#[test]
fn uncaught_error_messages_are_escaped_in_all_execution_entry_points() {
    let project = RegressionProject::new(
        "export function main():void{try{throw 'old';}finally{throw Error('uncaught\\n😀');}}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v8");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut wasm = varta();
    wasm.args(["run", "dist/app.wasm"]);
    let mut runtime = Command::new(runtime_path());
    runtime.args(["run", "dist/app.wasm"]);
    for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
        let output = command.current_dir(&project.0).output().unwrap();
        let text = combined(&output);
        assert!(!output.status.success(), "{text}");
        assert!(
            text.contains("uncaught guest exception: Error: \"uncaught\\n😀\""),
            "{text}"
        );
    }
}

#[test]
fn native_error_families_run_across_modules_and_source_free_execution_targets() {
    let project = RegressionProject::new(
        r#"
import {create,fail} from './errors.ts';
export function main():void {
 const error:Error=create();
 try{if(error instanceof TypeError){fail(error);}}catch(e){if(e instanceof Error){console.log(e.name);console.log(e.message);console.log(e instanceof TypeError);}}finally{console.log("final");}
 const values:Error[]=[error,RangeError("range"),SyntaxError("syntax")];console.log(JSON.stringify(values));console.log(values.includes(error));const second=values.at(1);if(second!==undefined){console.log(second.toString());}
}"#,
        serde_json::json!({"env":{"get":["VARTA_NATIVE_ERROR_MESSAGE"]}}),
    );
    fs::write(project.0.join("src/errors.ts"),"export function create():TypeError{return new TypeError(varta.env.get('VARTA_NATIVE_ERROR_MESSAGE'));}export function fail(error:TypeError):void{throw error;}").unwrap();
    project.language_profile("checked-f64-utf16-v9");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 12);
    assert_eq!(info["valueAbiVersion"], 11);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v9");
    let expected = [
        "TypeError",
        "owned😀",
        "1",
        "final",
        "[{},{},{}]",
        "1",
        "RangeError: range",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut commands = Vec::new();
    let mut dev = varta();
    dev.arg("dev");
    commands.push(dev);
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command
            .current_dir(&project.0)
            .env("VARTA_NATIVE_ERROR_MESSAGE", "owned😀")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn native_error_guards_preserve_host_ceilings_status_and_fatal_exit_in_every_target() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const result=varta.result.capture(varta.env.get("VARTA_NATIVE_ERROR_SECRET"));
 try{if(!result.ok){throw TypeError("denied");}console.log(result.value);}catch(e){if(e instanceof TypeError){console.log(e.name);console.log(e.message);}}
 console.log(result.status);console.log(varta.result.status());
 try{varta.process.exit(7);}catch(e){if(e instanceof Error){console.log("BAD_CATCH");}}finally{console.log("continued");}
}"#,
        serde_json::json!({"env":{"get":["VARTA_NATIVE_ERROR_SECRET"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v9");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_NATIVE_ERROR_SECRET", "VISIBLE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert!(!text.contains("BAD_CATCH"), "{text}");
            if restricted {
                assert!(output.status.success(), "{text}");
                assert!(
                    text.contains(
                        "varta log: TypeError\nvarta log: denied\nvarta log: 1\nvarta log: 1"
                    ),
                    "{text}"
                );
                assert!(text.contains("varta log: continued"), "{text}");
                assert!(!text.contains("VISIBLE"), "{text}");
            } else {
                assert_eq!(output.status.code(), Some(7), "{text}");
                assert!(
                    text.contains("varta log: VISIBLE\nvarta log: 0\nvarta log: 0"),
                    "{text}"
                );
                assert!(!text.contains("continued"), "{text}");
            }
        }
    }
}

#[test]
fn uncaught_native_error_family_diagnostics_are_owned_escaped_and_preserved_by_finally() {
    let project = RegressionProject::new(
        "export function main():void{try{throw TypeError('old');}finally{throw SyntaxError('uncaught\\n😀');}}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v9");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut wasm = varta();
    wasm.args(["run", "dist/app.wasm"]);
    let mut runtime = Command::new(runtime_path());
    runtime.args(["run", "dist/app.wasm"]);
    for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
        let output = command.current_dir(&project.0).output().unwrap();
        let text = combined(&output);
        assert!(!output.status.success(), "{text}");
        assert!(
            text.contains("uncaught guest exception: SyntaxError: \"uncaught\\n😀\""),
            "{text}"
        );
    }
}

#[test]
fn error_subclasses_link_across_modules_and_report_uncaught_instances_in_every_target() {
    const ERRORS: &str = "export class UarpError extends Error{constructor(message:string){super(message);this.name=new.target.name;}}\
         export class APITimeoutError extends UarpError{constructor(message='slow\\n😀'){super(message);}}\
         export class APIError extends UarpError{readonly status:number;constructor(status:number){super(`HTTP ${status}`);this.status=status;}}\
         export function request(status:number):void{if(status===0){throw new APITimeoutError();}throw new APIError(status);}";
    let caught = r#"
import {APIError, APITimeoutError, UarpError, request} from './errors.ts';
export function main():void {
 try{request(404);}catch(e){if(e instanceof APIError){console.log(e.toString()+' '+String(e.status));}}
 try{request(0);}catch(e){if(e instanceof UarpError){console.log(e.name);console.log(e instanceof APITimeoutError);}}
 try{try{request(0);}finally{console.log("finally");}}catch({message}){console.log(message===undefined);}
}"#;
    let uncaught = "import {request} from './errors.ts';\
                    export function main():void{try{request(0);}finally{console.log('cleanup');}}";
    for (main, success) in [(caught, true), (uncaught, false)] {
        let project = RegressionProject::new(main, serde_json::json!({}));
        fs::write(project.0.join("src/errors.ts"), ERRORS).unwrap();
        project.language_profile("checked-f64-utf16-v32");
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for (index, mut command) in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))]
            .into_iter()
            .enumerate()
        {
            if index == 1 {
                // Packaged artifacts run without the sources.
                fs::remove_dir_all(project.0.join("src")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            let text = combined(&output);
            assert_eq!(output.status.success(), success, "{text}");
            if success {
                for line in [
                    "varta log: APIError: HTTP 404 404",
                    "varta log: APITimeoutError",
                    "varta log: 1",
                    "varta log: finally",
                    "varta log: 0",
                ] {
                    assert!(text.contains(line), "{text}");
                }
            } else {
                assert!(
                    text.contains("uncaught guest exception: APITimeoutError: \"slow\\n😀\""),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn recoverable_json_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {read} from './decode.ts';
export function main():void {
 try{read(varta.env.get("VARTA_JSON_INPUT"));}catch(e){if(e instanceof SyntaxError){console.log(e.name);console.log(e.message);}}
 try{read('{"n":"PRIVATE_INPUT"}');}catch(e){if(e instanceof TypeError){console.log(e.name);}}
 try{try{read('{');}finally{console.log("finally");}}catch(e){if(e instanceof Error){console.log("recovered");}}
 console.log(read('{"n":1.25}').n);
}"#,
        serde_json::json!({"env":{"get":["VARTA_JSON_INPUT"]}}),
    );
    fs::write(
        project.0.join("src/decode.ts"),
        "export function read(text:string):{n:number}{return varta.json.parse<{n:number}>(text);}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v10");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 13);
    assert_eq!(info["valueAbiVersion"], 11);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v10");
    let expected = [
        "SyntaxError",
        "Invalid JSON syntax",
        "TypeError",
        "finally",
        "recovered",
        "1.25",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut commands = Vec::new();
    let mut dev = varta();
    dev.arg("dev");
    commands.push(dev);
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command
            .current_dir(&project.0)
            .env("VARTA_JSON_INPUT", "{PRIVATE_INPUT")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            text.lines().take(expected.len()).collect::<Vec<_>>(),
            expected
        );
        assert!(!text.contains("PRIVATE_INPUT"));
    }
}

#[test]
fn recoverable_json_preserves_host_ceilings_system_status_and_authorized_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const result=varta.result.capture(varta.env.get("VARTA_JSON_RESOURCE"));
 try{varta.json.parse<{n:number}>(result.value);}catch(e){if(e instanceof SyntaxError){console.log(e.name);console.log(e.message);}}
 console.log(result.status);console.log(varta.result.status());
 const parsed=varta.json.parse<{name:string}>('{"name":"VARTA_JSON_RESOURCE"}');varta.env.print(parsed.name);console.log(varta.result.status());
 try{varta.process.exit(7);}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}
}"#,
        serde_json::json!({"env":{"get":["VARTA_JSON_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v10");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_JSON_RESOURCE", "{\"secret\":\"SECRET_PAYLOAD\",");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(!text.contains("SECRET_PAYLOAD"), "{text}");
            assert!(text.contains(if restricted{"varta log: SyntaxError\nvarta log: Invalid JSON syntax\nvarta log: 1\nvarta log: 1"}else{"varta log: SyntaxError\nvarta log: Invalid JSON syntax\nvarta log: 0\nvarta log: 0"}),"{text}");
            assert!(
                text.contains("<capability denied: env> (VARTA_JSON_RESOURCE)\nvarta log: 1"),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn recoverable_json_uncaught_errors_and_fatal_limits_are_distinct_in_every_target() {
    for (input, expected) in [
        (
            "{".to_owned(),
            "uncaught guest exception: SyntaxError: \"Invalid JSON syntax\"",
        ),
        (
            format!(
                "[{}]",
                vec!["0"; varta_common::value::MAX_ARRAY_ELEMENTS + 1].join(",")
            ),
            "checked JSON array element limit",
        ),
    ] {
        let literal = serde_json::to_string(&input).unwrap();
        let source = if input == "{" {
            format!("export function main():void{{varta.json.parse<number>({literal});}}")
        } else {
            format!(
                "export function main():void{{try{{varta.json.parse<number>({literal});}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');return;}}}}"
            )
        };
        let project = RegressionProject::new(&source, serde_json::json!({}));
        project.language_profile("checked-f64-utf16-v10");
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            let output = command.current_dir(&project.0).output().unwrap();
            let text = combined(&output);
            assert!(!output.status.success(), "{text}");
            assert!(text.contains(expected), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(!text.contains("BAD_FINALLY"), "{text}");
        }
    }
}

#[test]
fn string_repeat_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {repeat} from './repeat.ts';
export function main():void{
 console.log(repeat(varta.env.get("VARTA_REPEAT_VALUE"),2));console.log(JSON.stringify(repeat("\uD800",3)));
 try{repeat("private",-1);}catch(e){if(e instanceof RangeError){console.log(e.name);console.log(e.message);}}
 console.log(repeat("done",1));
}"#,
        serde_json::json!({"env":{"get":["VARTA_REPEAT_VALUE"]}}),
    );
    fs::write(project.0.join("src/repeat.ts"),"export function repeat(value:string,count:number|undefined):string{return value.repeat(count);}").unwrap();
    project.language_profile("checked-f64-utf16-v10");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let expected = [
        "owned😀owned😀",
        "\"\\ud800\\ud800\\ud800\"",
        "RangeError",
        "Invalid string repeat count",
        "done",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut commands = Vec::new();
    let mut dev = varta();
    dev.arg("dev");
    commands.push(dev);
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command
            .current_dir(&project.0)
            .env("VARTA_REPEAT_VALUE", "owned😀")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn string_repeat_range_errors_preserve_host_ceilings_status_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const result=varta.result.capture(varta.env.get("VARTA_REPEAT_RESOURCE"));
 try{result.value.repeat(-1);}catch(e){if(e instanceof RangeError){console.log(e.name);}}
 console.log(result.status);console.log(varta.result.status());varta.env.print("VARTA_REPEAT_RESOURCE".repeat(1));console.log(varta.result.status());
 try{varta.process.exit(7);}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}
}"#,
        serde_json::json!({"env":{"get":["VARTA_REPEAT_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v10");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_REPEAT_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(
                text.contains(if restricted {
                    "varta log: RangeError\nvarta log: 1\nvarta log: 1"
                } else {
                    "varta log: RangeError\nvarta log: 0\nvarta log: 0"
                }),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_REPEAT_RESOURCE)\nvarta log: 1"),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn string_repeat_uncaught_range_errors_and_fatal_string_limits_remain_distinct() {
    for (count, expected) in [
        (
            "-1".into(),
            "uncaught guest exception: RangeError: \"Invalid string repeat count\"",
        ),
        (
            (varta_common::abi::MAX_STRING_BYTES / 2 + 1).to_string(),
            "unreachable",
        ),
    ] {
        let source = if count == "-1" {
            format!("export function main():void{{'x'.repeat({count});}}")
        } else {
            format!(
                "export function main():void{{try{{'x'.repeat({count});}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');return;}}}}"
            )
        };
        let project = RegressionProject::new(&source, serde_json::json!({}));
        project.language_profile("checked-f64-utf16-v10");
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            let output = command.current_dir(&project.0).output().unwrap();
            let text = combined(&output);
            assert!(!output.status.success(), "{text}");
            assert!(text.contains(expected), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(!text.contains("BAD_FINALLY"), "{text}");
        }
    }
}

#[test]
fn typed_callbacks_run_across_reexports_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {apply,inc as first} from './callbacks.ts';import {inc as second} from './bridge.ts';
export function main():void{
 console.log(apply(value=>value+1,41));const fn=first;console.log(fn===second);console.log(apply(fn,2));
 const echo:(text:string)=>string=text=>text+"😀";console.log(echo("owned"));
 try{apply(value=>varta.json.parse<number>('{'),0);}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}
}"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/callbacks.ts"),"export function apply(fn:(value:number)=>number,value:number):number{return fn(value);}export function inc(value:number):number{return value+1;}").unwrap();
    fs::write(
        project.0.join("src/bridge.ts"),
        "import {inc} from './callbacks.ts';export {inc};",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v11");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v11");
    let mut commands = Vec::new();
    let mut dev = varta();
    dev.arg("dev");
    commands.push(dev);
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["42", "1", "3", "owned😀", "SyntaxError"].map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn callbacks_preserve_operation_grants_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
function invoke(fn:()=>void):void{fn();}
export function main():void{
 invoke(()=>{varta.env.print("VARTA_CALLBACK_RESOURCE");console.log(varta.result.status());});
 try{invoke(()=>{varta.process.exit(7);});}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}
}"#,
        serde_json::json!({"env":{"get":["VARTA_CALLBACK_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v11");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_CALLBACK_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_CALLBACK_RESOURCE)\nvarta log: 1"),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn escaped_closures_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {counter,decorate} from './closures.ts';
export function main():void{const first=counter(0);const second=counter(10);console.log(first());console.log(second());console.log(first());
 const text=decorate(varta.env.get("VARTA_CLOSURE_VALUE"));console.log(text("😀"));console.log(JSON.stringify(decorate("\uD800")("!")));}
"#,
        serde_json::json!({"env":{"get":["VARTA_CLOSURE_VALUE"]}}),
    );
    fs::write(project.0.join("src/closures.ts"),"export function counter(start:number):()=>number{let value=start;return ()=>{value=value+1;return value;};}export function decorate(prefix:string):(suffix:string)=>string{return suffix=>prefix+suffix;}").unwrap();
    project.language_profile("checked-f64-utf16-v12");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspected = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspected.status.success(), "{}", combined(&inspected));
    let info: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v12");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command
            .current_dir(&project.0)
            .env("VARTA_CLOSURE_VALUE", "owned")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["1", "11", "2", "owned😀", "\"\\ud800!\""].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn closures_preserve_operation_grants_host_ceilings_and_authorized_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{const resource="VARTA_CLOSURE_RESOURCE";const read=()=>{varta.env.print(resource);console.log(varta.result.status());};read();
 const code=7;const stop=()=>{varta.process.exit(code);};try{stop();}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_CLOSURE_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v12");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_CLOSURE_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_CLOSURE_RESOURCE)\nvarta log: 1"),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn array_callbacks_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {transform,first} from './arrays.ts';
export function main():void{const values=transform([1,2,3],10);console.log(JSON.stringify(values));console.log(first(values));let sum=0;console.log(values.forEach(value=>{sum=sum+value;}));console.log(sum);console.log(JSON.stringify(["\uD800","😀"].map(value=>value+"!")));}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/arrays.ts"),"export function transform(values:number[],offset:number):number[]{return values.map(value=>value+offset).filter(value=>value>11);}export function first(values:number[]):number|undefined{return values.find(value=>value===13);}").unwrap();
    project.language_profile("checked-f64-utf16-v12");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["[12,13]", "13", "undefined", "25", "[\"\\ud800!\",\"😀!\"]"]
                .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn array_callbacks_preserve_operation_grants_host_ceilings_status_and_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{["VARTA_ARRAY_CALLBACK_RESOURCE"].forEach(value=>{varta.env.print(value);});console.log(varta.result.status());console.log(JSON.stringify([1].map(()=>varta.result.status())));
 try{[7].forEach(code=>{varta.process.exit(code);});}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_ARRAY_CALLBACK_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v12");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_ARRAY_CALLBACK_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_ARRAY_CALLBACK_RESOURCE)\nvarta log: 1\nvarta log: [1]"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn sparse_arrays_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {transform} from './sparse.ts';
export function main():void{const result=transform([1,,3],10);console.log(JSON.stringify(result));console.log(Object.hasOwn(result,1));const copy=result.slice();copy[1]=12;console.log(JSON.stringify(copy));console.log(Object.hasOwn(copy,1));
 const input=[1,2,3];console.log(JSON.stringify(input.map((value,index,array)=>{if(index===0){array.pop();array.pop();}return value;})));const parsed=varta.json.parse<(number|null)[]>('[1,null,3]');console.log(Object.hasOwn(parsed,1));console.log(JSON.stringify(parsed));}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/sparse.ts"),"export function transform(values:number[],offset:number):number[]{return values.map(value=>value+offset);}").unwrap();
    project.language_profile("checked-f64-utf16-v13");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v13");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(7)
                .collect::<Vec<_>>(),
            [
                "[11,null,13]",
                "0",
                "[11,12,13]",
                "1",
                "[1,null,null]",
                "1",
                "[1,null,3]"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn sparse_callbacks_preserve_permission_scopes_status_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{const resources=["VARTA_SPARSE_RESOURCE",,];const copy=resources.slice();copy.forEach(value=>{varta.env.print(value);});console.log(varta.result.status());console.log(Object.hasOwn(copy,1));console.log(copy.includes(undefined));
 try{[7,,].forEach(code=>{varta.process.exit(code);});}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_SPARSE_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v13");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_SPARSE_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_SPARSE_RESOURCE)\nvarta log: 1\nvarta log: 0\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn array_mutations_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {change} from './mutate.ts';export function main():void{const values=[1,2,3];change(values);console.log(JSON.stringify(values));console.log(values.length);console.log(Object.hasOwn(values,1));console.log(Object.hasOwn(values,5));try{values.length=1.5;}catch(e){if(e instanceof RangeError){console.log(e.name);console.log(e.message);}}try{delete values.length;}catch(e){if(e instanceof TypeError){console.log(e.name);}}console.log(JSON.stringify(values.map(value=>value*2)));}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/mutate.ts"),"export function change(values:number[]):void{delete values[1];values.length=2;values[5]=6;}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(8)
                .collect::<Vec<_>>(),
            [
                "[1,null,null,null,null,6]",
                "6",
                "0",
                "1",
                "RangeError",
                "Invalid array length",
                "TypeError",
                "[2,null,null,null,null,12]"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn array_mutation_errors_keep_system_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{const names=["VARTA_MUTATION_RESOURCE"];names.length=3;delete names[0];names[2]="VARTA_MUTATION_RESOURCE";names.forEach(value=>{varta.env.print(value);});try{names.length=-1;}catch(e){if(e instanceof RangeError){console.log(varta.result.status());}}console.log(JSON.stringify(names));
 try{const codes:number[]=[];codes[2]=7;codes.forEach(code=>{varta.process.exit(code);});}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_MUTATION_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_MUTATION_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_MUTATION_RESOURCE)\nvarta log: 1\nvarta log: [null,null,\"VARTA_MUTATION_RESOURCE\"]"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn array_reductions_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {summarize} from './collections.ts';export function main():void{
 const values:number[]=[1,,3,4];console.log(JSON.stringify(summarize(values)));
 console.log(values.reduce((total,value)=>total+"!","seed"));
 const empty:number[]=[];console.log(empty.some(()=>true));console.log(empty.every(()=>false));
 try{empty.reduce((total,value)=>total+value);}catch(e){if(e instanceof TypeError){console.log(e.name);console.log(e.message);}}finally{console.log("finally");}}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/collections.ts"),"export function summarize(values:number[]):{sum:number;any:boolean;all:boolean}{return {sum:values.reduce((total,value)=>total+value,0),any:values.some(value=>value===3),all:values.every(value=>value>0)};}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(7)
                .collect::<Vec<_>>(),
            [
                "{\"sum\":8,\"any\":true,\"all\":true}",
                "seed!!!",
                "0",
                "1",
                "TypeError",
                "Reduce of empty array with no initial value",
                "finally"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn array_reductions_preserve_operation_grants_host_ceilings_status_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{
 const names:string[]=[,"VARTA_REDUCTION_RESOURCE",];console.log(names.reduce((count,name)=>{varta.env.print(name);return count+1;},0));console.log(varta.result.status());
 console.log(names.some(name=>name.length>0));console.log(names.every(name=>name.length>0));console.log(varta.result.status());
 const empty:number[]=[];try{empty.reduce((total,value)=>total+value);}catch(e){if(e instanceof TypeError){console.log(varta.result.status());}}
 try{[7].some(code=>{varta.process.exit(code);return false;});}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_REDUCTION_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_REDUCTION_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_REDUCTION_RESOURCE)\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn callback_inference_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {collect} from './collect.ts';export function main():void{
 const values:number[]=[1,,3];console.log(JSON.stringify(collect(values)));
 console.log(JSON.stringify([1,2].map(value=>{if(value===1){return "one";}})));
 console.log(JSON.stringify([1,2].map(value=>{try{return {n:value};}finally{const local=1;}})));
 console.log([1,2].some(value=>{return value===2?"yes":"";}));
 console.log([1,2].every(value=>{return value===1?"yes":"";}));
 const optional:{n?:number}=[1].reduce((total,value)=>{total.n=value;return total;},{});console.log(JSON.stringify(optional));}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/collect.ts"),"export function collect(values:number[]):number[]{return values.reduce((total,value)=>{total.push(value);return total;},[]);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(6)
                .collect::<Vec<_>>(),
            [
                "[1,3]",
                "[\"one\",null]",
                "[{\"n\":1},{\"n\":2}]",
                "1",
                "0",
                "{\"n\":1}"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn callback_inference_preserves_host_grants_status_and_fatal_finalizer_boundaries() {
    let project = RegressionProject::new(
        r#"
function callback():(total:string[],name:string)=>string[]{console.log("callback");return (total,name)=>{varta.env.print(name);total.push(name);return total;};}
export function main():void{const names:string[]=[,"VARTA_INFERENCE_RESOURCE",];const result=names.reduce(callback(),[]);console.log(JSON.stringify(result));
 console.log(JSON.stringify(names.filter(name=>{return name;})));console.log(varta.result.status());
 try{names.some(name=>{try{varta.process.exit(7);return "yes";}finally{console.log("callback finally");}});}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_INFERENCE_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_INFERENCE_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("varta log: callback\nvarta log: <capability denied: env> (VARTA_INFERENCE_RESOURCE)\nvarta log: [\"VARTA_INFERENCE_RESOURCE\"]\nvarta log: [\"VARTA_INFERENCE_RESOURCE\"]\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(
                text.contains("varta log: callback finally"),
                restricted,
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn typed_this_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {make} from './functions.ts';export function main():void{const state={n:10};const callback=make(2);const values:number[]=[1,,3];console.log(JSON.stringify(values.map(callback,state)));console.log(callback.call(state,4));
 const factory=function(this:{n:number}):()=>number{return ()=>this.n;};const arrow=factory.call(state);state.n=20;console.log(arrow.call({n:99}));console.log(JSON.stringify([1,2].map(function(value){return {n:value};})));}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/functions.ts"),"export function make(offset:number):(this:{n:number},value:number)=>number{return function(value){return this.n+value+offset;};}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            ["[13,null,15]", "16", "20", "[{\"n\":1},{\"n\":2}]"]
                .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn typed_this_preserves_operation_permissions_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
export function main():void{const context={name:"VARTA_THIS_RESOURCE"};const fn=function(this:{name:string},value:number){varta.env.print(this.name);return this.name;};console.log(JSON.stringify([1].map(fn,context)));console.log(varta.result.status());console.log(fn.call(context,2));console.log(varta.result.status());
 const exit=function(this:{code:number}){try{varta.process.exit(this.code);}finally{console.log("callback finally");}};try{exit.call({code:7});}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_THIS_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_THIS_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_THIS_RESOURCE)\nvarta log: [\"VARTA_THIS_RESOURCE\"]\nvarta log: 1"),"{text}");
            assert!(text.contains("<capability denied: env> (VARTA_THIS_RESOURCE)\nvarta log: VARTA_THIS_RESOURCE\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(
                text.contains("varta log: callback finally"),
                restricted,
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn named_functions_and_declaration_receivers_run_in_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {factorial,alias,make} from './functions.ts';export function main():void{const context={offset:0};console.log(factorial.call(context,5));console.log(factorial===alias);const values:number[]=[3,,4];console.log(JSON.stringify(values.map(factorial,context)));let fn=make(7);const original=fn;fn=make(99);console.log(original(3));}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/functions.ts"),"export function factorial(this:{offset:number},n:number):number{if(n<=1){return 1+this.offset;}return n*factorial.call(this,n-1);}export {factorial as alias};export function make(offset:number):(n:number)=>number{return function self(n:number):number{if(n===0){return offset;}return self(n-1)+1;};}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            ["120", "1", "[6,null,24]", "10"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn named_functions_preserve_permissions_host_ceilings_and_fatal_recursive_exit() {
    let project = RegressionProject::new(
        r#"
function read(this:{name:string},value:number):string{varta.env.print(this.name);return this.name;}
export function main():void{console.log(JSON.stringify([1].map(read,{name:"VARTA_NAMED_RESOURCE"})));console.log(varta.result.status());
 const exit=function self(this:{code:number},depth:number):void{try{if(depth===0){varta.process.exit(this.code);}else{self.call(this,depth-1);}}finally{console.log(depth);}};try{exit.call({code:7},1);}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_NAMED_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_NAMED_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_NAMED_RESOURCE)\nvarta log: [\"VARTA_NAMED_RESOURCE\"]\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 0\nvarta log: 1\nvarta log: continued"),
                    "{text}"
                );
            } else {
                assert!(!text.contains("varta log: 0"), "{text}");
            }
        }
    }
}

#[test]
fn function_parameters_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        r#"
import {summary,recur} from './parameters.ts';export function main():void{console.log(JSON.stringify(summary()));console.log(JSON.stringify(summary(undefined,1,2)));console.log(JSON.stringify(summary("set",3,4)));console.log(recur()());console.log(JSON.stringify([1,2].map((value,index,array,extra=10)=>value+extra)));const values:(number|undefined)[]=[undefined,2];console.log(JSON.stringify(values.map((value:number=7)=>value)));}
"#,
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/parameters.ts"),"export function summary(prefix:string='default',...values:number[]):{prefix:string;sum:number}{return {prefix:prefix,sum:values.reduce((total,value)=>total+value,0)};}export function recur():(depth?:number)=>number{return function self(depth:number=3):number{if(depth===0){return 7;}return self(depth-1);};}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(6)
                .collect::<Vec<_>>(),
            [
                "{\"prefix\":\"default\",\"sum\":0}",
                "{\"prefix\":\"default\",\"sum\":3}",
                "{\"prefix\":\"set\",\"sum\":7}",
                "7",
                "[11,12]",
                "[7,2]"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn function_parameters_preserve_default_effect_grants_status_and_fatal_exit() {
    let project = RegressionProject::new(
        r#"
function initialize():string{varta.env.print("VARTA_PARAMETER_RESOURCE");return "ready";}
function count(value:string=initialize(),...tail:string[]):number{return tail.length;}
function stop():number{varta.process.exit(7);return 9;}
function execute(value:number=stop()):number{console.log("body");return value;}
export function main():void{console.log(count(undefined,"one"));console.log(varta.result.status());console.log(count("given","one","two"));console.log(varta.result.status());try{console.log(execute());}catch(e){console.log("BAD_CATCH");}finally{console.log("continued");}}
"#,
        serde_json::json!({"env":{"get":["VARTA_PARAMETER_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_PARAMETER_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_PARAMETER_RESOURCE)\nvarta log: 1\nvarta log: 1\nvarta log: 2\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: body"), restricted, "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: body\nvarta log: 9\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn spread_calls_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {summary,total} from './spread.ts';export function main():void{console.log(summary(...[]));console.log(summary('items',...[1,2],3));console.log(total.call({base:10},...[1,2]));try{total.call({base:10},...['bad']);}catch(e){if(e instanceof TypeError){console.log(e.message);}}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/spread.ts"),"export function summary(prefix:string='default',...values:number[]):string{return prefix+JSON.stringify(values);}export function total(this:{base:number},...items:number[]):number{return items.reduce((sum,n)=>sum+n,this.base);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            [
                "default[]",
                "items[1,2,3]",
                "13",
                "Invalid spread argument type"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn spread_calls_preserve_operand_permissions_status_and_fatal_exit() {
    let project = RegressionProject::new(
        "function operand():string[]{varta.env.print('VARTA_SPREAD_RESOURCE');return ['one'];}function count(...items:string[]):number{return items.length;}function stop():number[]{varta.process.exit(7);return [9];}function execute(...items:number[]):number{console.log('body');return items.length;}export function main():void{console.log(count(...operand()));console.log(varta.result.status());try{console.log(execute(...stop()));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"get":["VARTA_SPREAD_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_SPREAD_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_SPREAD_RESOURCE)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: body"), restricted, "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn parameter_patterns_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {summary,factory,nested} from './patterns.ts';export function main():void{console.log(JSON.stringify(summary({})));const options:{prefix?:string;flag?:boolean}={prefix:'set',flag:false};console.log(JSON.stringify(summary(...[options],[2,,3])));console.log(factory({text:'closure'+'!'})());try{nested([]);}catch(e){if(e instanceof TypeError){console.log(e.message);}}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/patterns.ts"),"export function summary({prefix='default',...copy}:{prefix?:string;flag?:boolean},[first=1,...tail]:number[]=[]):{prefix:string;first:number;tail:(number|undefined)[];copy:{flag?:boolean}}{return {prefix:prefix,first:first,tail:tail,copy:copy};}export function factory({text}:{text:string}):()=>string{return ()=>text;}export function nested([{n}]:{n:number}[]):number{return n;}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            [
                "{\"prefix\":\"default\",\"first\":1,\"tail\":[],\"copy\":{}}",
                "{\"prefix\":\"set\",\"first\":2,\"tail\":[null,3],\"copy\":{\"flag\":false}}",
                "closure!",
                "Cannot destructure null or undefined"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn parameter_patterns_preserve_default_effect_permissions_status_and_fatal_exit() {
    let project = RegressionProject::new(
        "function initialize():string{varta.env.print('VARTA_PATTERN_RESOURCE');return 'ready';}function count({name=initialize(),...copy}:{name?:string;flag?:boolean},[first=1,...tail]:number[]):number{return tail.length;}function stop():number{varta.process.exit(7);return 9;}function execute({value=stop()}:{value?:number}):number{console.log('body');return value;}export function main():void{console.log(count({},[1,2]));console.log(varta.result.status());console.log(count({name:'given'},[1,2,3]));console.log(varta.result.status());try{console.log(execute({}));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"get":["VARTA_PATTERN_RESOURCE"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_PATTERN_RESOURCE", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_PATTERN_RESOURCE)\nvarta log: 1\nvarta log: 1\nvarta log: 2\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: body"), restricted, "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: body\nvarta log: 9\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn local_binding_patterns_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {snapshot,reader} from './locals.ts';export function main():void{console.log(JSON.stringify(snapshot()));console.log(reader()());}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/locals.ts"),"export function snapshot():{label:string;first:number;tail:(number|undefined)[];copy:{flag?:boolean}}{const {name:label='local',...copy}:{name?:string;flag?:boolean}={flag:false};let [first=1,...tail]:number[]=[2,,3];first=first+1;return {label:label,first:first,tail:tail,copy:copy};}export function reader():()=>number{let saved:()=>number=()=>99;for(let {i=0,read=()=>i}:{}={};i<2;i=i+1){saved=read;if(i===0){i=1;}}return saved;}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(2)
                .collect::<Vec<_>>(),
            [
                "{\"label\":\"local\",\"first\":3,\"tail\":[null,3],\"copy\":{\"flag\":false}}",
                "0"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn local_binding_defaults_preserve_real_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):number{varta.env.print(name);return 1;}function stop():number{varta.process.exit(7);return 9;}export function main():void{const {a=read('VARTA_LOCAL_ALLOWED'),...copy}:{a?:number;flag?:boolean}={};const [first=0,...tail]:number[]=[1,2];console.log(a);console.log(varta.result.status());const {b=read('VARTA_LOCAL_DENIED')}:{}={};const [other,...remaining]:number[]=[3,4];console.log(b);console.log(varta.result.status());try{const {code=stop()}:{}={};console.log('body');console.log(code);}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_LOCAL_ALLOWED"],"get":["VARTA_LOCAL_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_LOCAL_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_LOCAL_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_LOCAL_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: body"), restricted, "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: body\nvarta log: 9\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn local_binding_tdz_diagnostics_point_to_the_dependency_default_expression() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{console.log(bad());}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():number{const {a=b,b=1}:{}={};return a;}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "invalid_program");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("=b").unwrap() + 1
    );
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("before initialization")
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn destructuring_assignment_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {snapshot,reader} from './assign.ts';export function main():void{console.log(JSON.stringify(snapshot()));console.log(reader()());let first=0;let second=9;try{[first,second]=[1];}catch(e){if(e instanceof TypeError){console.log(e.name);}}console.log(first);console.log(second);}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/assign.ts"),"export function snapshot():{first:number;tail:(number|undefined)[]}{let first=0;let tail:(number|undefined)[]=[];[first,...tail]=[2,,3];return {first:first,tail:tail};}export function reader():()=>number{let value=1;const read=()=>value;({value}={value:3});return read;}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            [
                "{\"first\":2,\"tail\":[null,3]}",
                "3",
                "TypeError",
                "1",
                "9"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn destructuring_assignment_preserves_default_grants_status_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function read(name:string):string{varta.env.print(name);return name;}function stop():number{varta.process.exit(7);return 9;}export function main():void{let name='old';let code=0;const observe=()=>name;({name=read('VARTA_ASSIGN_ALLOWED')}={});const [...pure]:number[]=[];console.log(observe());console.log(varta.result.status());({name=read('VARTA_ASSIGN_DENIED')}={});console.log(observe());console.log(varta.result.status());try{({code=stop()}={});console.log('body');console.log(code);}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_ASSIGN_ALLOWED"],"get":["VARTA_ASSIGN_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_ASSIGN_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_ASSIGN_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: VARTA_ASSIGN_ALLOWED\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_ASSIGN_DENIED)\nvarta log: VARTA_ASSIGN_DENIED\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: body"), restricted, "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: body\nvarta log: 9\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn destructuring_assignment_const_diagnostics_identify_the_dependency_target() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{const value=1;({value}={value:2});}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "invalid_program");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("({value}").unwrap() + 2
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn assignment_values_return_original_rhs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {assign,assignArray} from './values.ts';export function main():void{const source={extra:'kept',n:2};const original=assign(source);console.log(original===source);console.log(JSON.stringify(original));const input:number[]=[,2];const output=assignArray(input);console.log(output===input);console.log(Object.hasOwn(output,0));console.log(JSON.stringify(output));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/values.ts"),"export function assign(source:{n:number;extra:string}):{n:number;extra:string}{let a=0;let b=0;return ({n:a}=({n:b}=source));}export function assignArray(source:number[]):number[]{let n=0;return ([n=7]=source);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v14");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["1", "{\"extra\":\"kept\",\"n\":2}", "1", "0", "[null,2]"]
                .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn assignment_values_preserve_nested_effect_grants_status_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(resource:string):number{varta.env.print(resource);return 1;}function stop():number{varta.process.exit(7);return 9;}export function main():void{let n=0;let code=0;console.log(JSON.stringify(({n=read('VARTA_VALUE_ALLOWED')}={})));console.log(n);console.log(varta.result.status());console.log(JSON.stringify(({n=read('VARTA_VALUE_DENIED')}={})));console.log(n);console.log(varta.result.status());try{console.log(JSON.stringify(({code=stop()}={})));console.log(code);}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_VALUE_ALLOWED"],"get":["VARTA_VALUE_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_VALUE_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_VALUE_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: {{}}\nvarta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_VALUE_DENIED)\nvarta log: {}\nvarta log: 1\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: {}\nvarta log: 9\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn assignment_expression_result_type_errors_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{let n=0;const wrong:number=({n}={n:1});}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("({n}").unwrap() + 1
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn member_targets_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {assign} from './members.ts';export function main():void{const source={extra:'kept',n:2};const output={n:0};const values:number[]=[];console.log(assign(source,output,values)===source);console.log(output.n);console.log(JSON.stringify(values));console.log(Object.hasOwn(values,0));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/members.ts"),"export function assign(source:{n:number;extra:string},output:{n:number},values:number[]):{n:number;extra:string}{({n:output.n}=source);[values[2]]=[output.n];return ({n:output.n}=source);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            ["1", "2", "[null,null,2]", "0"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn member_targets_preserve_receiver_key_grants_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function stop():number{varta.process.exit(7);return 9;}export function main():void{const output={n:0};const values:number[]=[0];const receiver=():{n:number}=>{varta.env.print('VARTA_MEMBER_ALLOWED');return output;};const key=():number=>{varta.env.print('VARTA_MEMBER_DENIED');return 0;};({n:receiver().n}={n:2});console.log(output.n);console.log(varta.result.status());[values[key()]]=[3];console.log(values[0]);console.log(varta.result.status());try{[values[stop()]]=[4];console.log(values.length);}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_MEMBER_ALLOWED"],"get":["VARTA_MEMBER_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_MEMBER_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_MEMBER_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 2\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_MEMBER_DENIED)\nvarta log: 3\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 10\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn member_target_type_errors_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{const output={n:0};({n:output.n}={n:'bad'});}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("output.n").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn for_of_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {count,first} from './iteration.ts';export function main():void{const values:number[]=[1,,3];console.log(count(values));console.log(first([{}]));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/iteration.ts"),"export function count(values:number[]):number{let visited=0;for(const value of values){console.log(value);visited=visited+1;}return visited;}export function first(rows:{n?:number}[]):number{for(const {n=7} of rows){return n;}return 0;}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["1", "undefined", "3", "3", "7"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn for_of_preserves_source_head_and_body_grants_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function source():number[]{varta.env.print('VARTA_FOR_OF_ALLOWED');return [1,2];}export function main():void{for(const value of source()){console.log(value);console.log(varta.result.status());}const output={n:0};const receiver=():{n:number}=>{varta.env.print('VARTA_FOR_OF_DENIED');return output;};for(receiver().n of [3]){console.log(output.n);console.log(varta.result.status());}try{for(const value of [4]){varta.process.exit(7);console.log('after exit');}}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_FOR_OF_ALLOWED"],"get":["VARTA_FOR_OF_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_FOR_OF_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_FOR_OF_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            let status = u8::from(restricted);
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {status}\nvarta log: 2\nvarta log: {status}"
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_FOR_OF_DENIED)\nvarta log: 3\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert_eq!(text.contains("varta log: after exit"), restricted, "{text}");
        }
    }
}
#[test]
fn for_of_source_type_errors_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{for(const value of {n:1}){}}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("{n:1}").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn do_while_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {spin,always} from './loops.ts';export function main():void{console.log(spin(2));console.log(always());}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/loops.ts"),"export function spin(limit:number):number{let n=0;do{n=n+1;try{if(n<limit){continue;}}finally{console.log(n);}}while(n<limit);return n;}export function always():string{do{return 'once';}while(false);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            ["1", "2", "2", "once"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn do_while_preserves_body_condition_grants_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function condition():boolean{varta.env.print('VARTA_DO_DENIED');return false;}export function main():void{do{varta.env.print('VARTA_DO_ALLOWED');console.log(varta.result.status());}while(condition());console.log(varta.result.status());let n=0;try{do{n=n+1;varta.process.exit(7);console.log(n);}while(false);}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_DO_ALLOWED"],"get":["VARTA_DO_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_DO_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_DO_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(text.contains(&format!("varta log: {}\nvarta log: <capability denied: env> (VARTA_DO_DENIED)\nvarta log: 1",u8::from(restricted))),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 1\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn do_while_condition_errors_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{do{}while({n:1});}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "unsupported_syntax");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("{n:1}").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn labels_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {visit} from './labels.ts';export function main():void{console.log(visit());}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/labels.ts"),"export function visit():number{let n=0;outer:do{n=n+1;for(const value of [1,2]){try{if(n<2){continue outer;}break outer;}finally{console.log(n);}}}while(n<3);return n;}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(3)
                .collect::<Vec<_>>(),
            ["1", "2", "2"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn labels_preserve_grants_host_ceilings_and_exit_across_named_finalizer_routes() {
    let project = RegressionProject::new(
        "export function main():void{let n=0;outer:do{n=n+1;try{varta.env.print('VARTA_LABEL_ALLOWED');if(n===1){continue outer;}break outer;}finally{varta.env.print('VARTA_LABEL_DENIED');console.log(n);console.log(varta.result.status());}}while(true);try{exit:while(true){varta.process.exit(7);break exit;}}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_LABEL_ALLOWED"],"get":["VARTA_LABEL_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_LABEL_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_LABEL_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                if restricted { 0 } else { 2 },
                "{text}"
            );
            for n in [1, 2] {
                assert!(text.contains(&format!("<capability denied: env> (VARTA_LABEL_DENIED)\nvarta log: {n}\nvarta log: 1")),"{text}");
            }
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}
#[test]
fn labels_invalid_targets_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{out:{continue out;}}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(diagnostic["code"], "syntax_error");
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("continue").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn object_keys_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {names} from './keys.ts';export function main():void{const value:{a?:number;b:number;'2'?:number}={b:1};value.a=undefined;value['2']=2;const keys=names(value);console.log(JSON.stringify(keys));for(const key of keys){console.log(key);}console.log(keys===names(value));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/keys.ts"),"export function names(value:{a?:number;b:number;'2'?:number}):string[]{return Object.keys(value);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["[\"2\",\"b\",\"a\"]", "2", "b", "a", "0"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn object_keys_preserves_argument_grants_status_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):{a?:number;b:number}{varta.env.print(resource);return {b:1};}function stop():{n:number}{varta.process.exit(7);return {n:9};}export function main():void{console.log(JSON.stringify(Object.keys(source('VARTA_KEYS_ALLOWED'))));console.log(varta.result.status());console.log(JSON.stringify(Object.keys(source('VARTA_KEYS_DENIED'))));console.log(varta.result.status());try{console.log(JSON.stringify(Object.keys(stop())));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_KEYS_ALLOWED"],"get":["VARTA_KEYS_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_KEYS_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_KEYS_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: [\"b\"]\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_KEYS_DENIED)\nvarta log: [\"b\"]\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: [\"n\"]\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn object_keys_type_errors_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{Object.keys(true);}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("true").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn object_values_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {enumerate} from './values.ts';export function main():void{const source:{a?:number;b:string;'2'?:number}={b:'x'};source.a=undefined;source['2']=2;const items=enumerate(source);console.log(JSON.stringify(items));console.log(Object.hasOwn(items,2));for(const value of items){console.log(value);}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/values.ts"),"export function enumerate(source:{a?:number;b:string;'2'?:number}):(number|string|undefined)[]{return Object.values(source);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["[2,\"x\",null]", "1", "2", "x", "undefined"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn object_values_preserves_argument_grants_status_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):{a?:number;b:string}{varta.env.print(resource);return {b:'x'};}function stop():{n:number}{varta.process.exit(7);return {n:9};}export function main():void{console.log(JSON.stringify(Object.values(source('VARTA_VALUES_ALLOWED'))));console.log(varta.result.status());console.log(JSON.stringify(Object.values(source('VARTA_VALUES_DENIED'))));console.log(varta.result.status());try{console.log(JSON.stringify(Object.values(stop())));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_VALUES_ALLOWED"],"get":["VARTA_VALUES_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_VALUES_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_VALUES_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: [\"x\"]\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_VALUES_DENIED)\nvarta log: [\"x\"]\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: [9]\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn object_values_type_errors_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{Object.values(true);}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("true").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn array_enumeration_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {keys,values} from './enumerate.ts';export function main():void{const source:number[]=[,2,,4];console.log(JSON.stringify(keys(source)));const items=values(source);console.log(JSON.stringify(items));console.log(Object.hasOwn(items,0));source[1]=9;items.push(6);console.log(JSON.stringify(items));console.log(JSON.stringify(source));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/enumerate.ts"),"export function keys(source:number[]|null):string[]{return Object.keys(source);}export function values(source:number[]):number[]{return Object.values(source);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["[\"1\",\"3\"]", "[2,4]", "1", "[2,4,6]", "[null,9,null,4]"]
                .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn array_enumeration_preserves_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):number[]{varta.env.print(resource);return [1,,3];}function stop():number[]{varta.process.exit(7);return [9];}export function main():void{console.log(JSON.stringify(Object.keys(source('VARTA_ARRAY_ENUM_ALLOWED'))));console.log(varta.result.status());console.log(JSON.stringify(Object.values(source('VARTA_ARRAY_ENUM_DENIED'))));console.log(varta.result.status());try{console.log(JSON.stringify(Object.values(stop())));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_ARRAY_ENUM_ALLOWED"],"get":["VARTA_ARRAY_ENUM_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_ARRAY_ENUM_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_ARRAY_ENUM_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: [\"0\",\"2\"]\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_ARRAY_ENUM_DENIED)\nvarta log: [1,3]\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: [9]\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn catch_patterns_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {read} from './catch.ts';export function main():void{console.log(read(true));console.log(read(false));try{try{throw null;}catch({message='BAD'}){console.log(message);}}catch({name}){console.log(name);}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/catch.ts"),"export function read(flag:boolean):string{try{if(flag){throw TypeError('owned'+'😀');}throw 1;}catch({name='primitive',message='fallback'}){return name+':'+message;}finally{console.log('finally');}}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            [
                "finally",
                "TypeError:owned😀",
                "finally",
                "primitive:fallback",
                "TypeError"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn catch_patterns_preserve_default_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function fallback(resource:string):string{varta.env.print(resource);return 'fallback';}function stop():string{varta.process.exit(7);return 'denied';}export function main():void{try{throw 1;}catch({message=fallback('VARTA_CATCH_ALLOWED')}){console.log(message);console.log(varta.result.status());}try{throw 1;}catch({message=fallback('VARTA_CATCH_DENIED')}){console.log(message);console.log(varta.result.status());}try{throw TypeError('original');}catch({message=fallback('VARTA_CATCH_ALLOWED')}){console.log(message);console.log(varta.result.status());}try{throw 1;}catch({message=stop()}){console.log(message);}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_CATCH_ALLOWED"],"get":["VARTA_CATCH_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_CATCH_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_CATCH_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: fallback\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_CATCH_DENIED)\nvarta log: fallback\nvarta log: 1"),"{text}");
            assert!(text.contains("varta log: original\nvarta log: 1"), "{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: denied\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn catch_patterns_unsupported_properties_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{try{throw 1;}catch({length}){}}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "unsupported_syntax");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("length").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn string_ranges_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {cut,between} from './ranges.ts';export function main():void{const text='A😀B';console.log(JSON.stringify(cut(text,1,2)));console.log(between(text,3,1));console.log(cut(text,-1,undefined));console.log(JSON.stringify([1,2,3].slice(-2)));console.log(JSON.stringify([1,2].map(n=>cut('abc',n,undefined))));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/ranges.ts"),"export function cut(source:string,start:number|undefined,end:number|undefined):string{return source.slice(start,end);}export function between(source:string,start:number,end:number):string{return source.substring(start,end);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["\"\\ud83d\"", "😀", "B", "[2,3]", "[\"bc\",\"c\"]"]
                .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn string_ranges_preserve_operand_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):string{varta.env.print(resource);return 'abcdef';}function stop():number{varta.process.exit(7);return 0;}export function main():void{console.log(source('VARTA_RANGE_ALLOWED').slice(1,3));console.log(varta.result.status());console.log(source('VARTA_RANGE_DENIED').substring(3,1));console.log(varta.result.status());try{console.log('abcdef'.substring(stop()));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_RANGE_ALLOWED"],"get":["VARTA_RANGE_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_RANGE_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_RANGE_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: bc\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_RANGE_DENIED)\nvarta log: bc\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: abcdef\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn string_ranges_invalid_bounds_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{'abc'.slice(0,true);}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("true").unwrap()
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn string_searches_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {contains,begins,finishes} from './search.ts';export function main():void{const text='A😀B';console.log(contains(text,'\\ud83d',1));console.log(begins(text,'😀',1));console.log(finishes(text,'B',4));console.log([NaN].includes(NaN));console.log(JSON.stringify(['abc','xyz'].map(s=>contains(s,'b',undefined))));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/search.ts"),"export function contains(text:string,needle:string,position:number|undefined):boolean{return text.includes(needle,position);}export function begins(text:string,needle:string,position:number):boolean{return text.startsWith(needle,position);}export function finishes(text:string,needle:string,position:number):boolean{return text.endsWith(needle,position);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            ["1", "1", "1", "1", "[true,false]"].map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn string_searches_preserve_operand_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):string{varta.env.print(resource);return 'abc';}function needle(resource:string):string{varta.env.print(resource);return 'bc';}function stop():number{varta.process.exit(7);return 0;}export function main():void{console.log(source('VARTA_SEARCH_ALLOWED').includes('b'));console.log(varta.result.status());console.log('abc'.endsWith(needle('VARTA_SEARCH_DENIED')));console.log(varta.result.status());console.log('abc'.startsWith('a'));console.log(varta.result.status());try{console.log('abc'.startsWith('a',stop()));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_SEARCH_ALLOWED"],"get":["VARTA_SEARCH_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_SEARCH_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_SEARCH_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_SEARCH_DENIED)\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 1\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn string_searches_invalid_arguments_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let dependency = project.0.join("src/bad.ts");
    for source in [
        "export function bad():void{'abc'.includes(true);}",
        "export function bad():void{'abc'.endsWith('c',true);}",
    ] {
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        assert_eq!(
            diagnostic["range"]["start"]["byte_offset"],
            source.find("true").unwrap()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn reviewed_array_and_trim_regressions_run_in_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v13", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {verify} from './checks.ts';export function main():void{verify();}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/checks.ts"),r"export function verify():void{const a=[1,2,1,3];console.log(a.lastIndexOf(1));console.log(a.lastIndexOf(1,undefined));console.log(a.lastIndexOf(1,1));console.log(a.lastIndexOf(1,Infinity));console.log(a.lastIndexOf(1,-100));const sparse:number[]=[1,,3];console.log(JSON.stringify(sparse.reverse()));console.log(Object.hasOwn(sparse,1));console.log(sparse.reverse()===sparse);const text=' \u0085x\u0085 '.trim();console.log(text.charCodeAt(0));console.log(text.length);}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{profile}: {}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{profile}: {}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(10)
                    .collect::<Vec<_>>(),
                ["2", "0", "0", "2", "-1", "[3,null,1]", "0", "1", "133", "3"]
                    .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn dynamic_own_keys_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {own} from './own.ts';export function main():void{const source:{a?:number;b:number}={b:1};console.log(JSON.stringify(['a','b','missing'].map(key=>own(source,key))));source.a=undefined;console.log(JSON.stringify(['a','b','missing'].map(key=>own(source,key))));console.log(JSON.stringify(Object.keys(source)));console.log(own(source,'__proto__'));console.log(JSON.stringify(source));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/own.ts"),"export function own(source:{a?:number;b:number},key:string):boolean{return Object.hasOwn(source,key);}").unwrap();
    project.language_profile("checked-f64-utf16-v14");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            [
                "[false,true,false]",
                "[true,true,false]",
                "[\"b\",\"a\"]",
                "0",
                "{\"b\":1}"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}
#[test]
fn dynamic_own_keys_preserve_argument_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):{b:number}{varta.env.print(resource);return {b:1};}function sourceArray(resource:string):number[]{varta.env.print(resource);return [1];}function sourceError(resource:string):TypeError{varta.env.print(resource);return TypeError('x');}function key(resource:string,name:string):string{varta.env.print(resource);return name;}function stop():string{varta.process.exit(7);return 'b';}export function main():void{console.log(Object.hasOwn(source('VARTA_OWN_ALLOWED'),'b'+''));console.log(varta.result.status());console.log(Object.hasOwn({b:1},key('VARTA_OWN_DENIED','b')));console.log(varta.result.status());console.log(Object.hasOwn(sourceArray('VARTA_OWN_ALLOWED'),key('VARTA_OWN_DENIED','0')));console.log(varta.result.status());console.log(Object.hasOwn(sourceError('VARTA_OWN_ALLOWED'),key('VARTA_OWN_DENIED','message')));console.log(varta.result.status());try{console.log(Object.hasOwn({b:1},stop()));}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_OWN_ALLOWED"],"get":["VARTA_OWN_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_OWN_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_OWN_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                3 * usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_OWN_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 1\nvarta log: continued"),
                    "{text}"
                );
            }
        }
    }
}
#[test]
fn dynamic_own_keys_invalid_types_keep_dependency_source_ranges() {
    for receiver in ["{n:1}", "[1]", "Error('x')"] {
        let project = RegressionProject::new(
            "import {bad} from './bad.ts';export function main():void{bad();}",
            serde_json::json!({}),
        );
        project.language_profile("checked-f64-utf16-v14");
        let source = format!("export function bad():void{{Object.hasOwn({receiver},true);}}");
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        assert_eq!(
            diagnostic["range"]["start"]["byte_offset"],
            source.find("true").unwrap()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn array_and_error_own_keys_run_in_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v13", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {arrayOwn,errorOwn} from './own.ts';export function main():void{const a:(number|undefined)[]=[1,,undefined];console.log(JSON.stringify(['0','1','2','3','length'].map(key=>arrayOwn(a,key))));a[1]=undefined;console.log(JSON.stringify(['0','1','2','3','length'].map(key=>arrayOwn(a,key))));console.log(JSON.stringify(['message','name','__proto__','stack',''].map(key=>errorOwn(TypeError(''),key))));console.log(errorOwn(Error(),'mess'+'age'));console.log(JSON.stringify(Object.keys(a)));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/own.ts"),"export function arrayOwn(a:(number|undefined)[],key:string):boolean{return Object.hasOwn(a,key);}export function errorOwn(e:Error,key:string):boolean{return Object.hasOwn(e,key);}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(5)
                    .collect::<Vec<_>>(),
                [
                    "[true,false,true,false,true]",
                    "[true,true,true,false,true]",
                    "[true,false,false,false,false]",
                    "0",
                    "[\"0\",\"1\",\"2\"]"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn computed_record_reads_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {read} from './read.ts';export function main():void{const source:{a?:number;b:string}={b:'owned'};console.log(JSON.stringify(['a','b','missing'].map(key=>read(source,key))));source.a=2.5;console.log(JSON.stringify(['a','b','missing'].map(key=>read(source,key))));try{read(source,'constructor');}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(Object.keys(source)));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/read.ts"),"export function read(source:{a?:number;b:string},key:string):number|string|undefined{return source[key];}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(4)
                    .collect::<Vec<_>>(),
                [
                    "[null,\"owned\",null]",
                    "[2.5,\"owned\",null]",
                    "1",
                    "[\"b\",\"a\"]"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn computed_record_reads_preserve_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):{b:number}{varta.env.print(resource);return {b:1};}function key(resource:string,name:string):string{varta.env.print(resource);return name;}function stop():string{varta.process.exit(7);return 'b';}export function main():void{console.log(source('VARTA_READ_ALLOWED')['b'+'']);console.log(varta.result.status());console.log(({b:1})[key('VARTA_READ_DENIED','b')]);console.log(varta.result.status());try{console.log(({b:1})[stop()]);}catch(e){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_READ_ALLOWED"],"get":["VARTA_READ_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_READ_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_READ_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_READ_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
        }
    }
}

#[test]
fn computed_record_reads_invalid_keys_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    let source = "export function bad():void{const source={n:1};source[true];}";
    let dependency = project.0.join("src/bad.ts");
    fs::write(&dependency, source).unwrap();
    let check = project.command(&["check", "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(diagnostic["code"], "type_error");
    assert_eq!(
        diagnostic["file"],
        serde_json::json!(dependency.canonicalize().unwrap())
    );
    assert_eq!(
        diagnostic["range"]["start"]["byte_offset"],
        source.find("true").unwrap()
    );
    assert_eq!(
        diagnostic["range"]["end"]["byte_offset"],
        source.find("true").unwrap() + 4
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn computed_record_writes_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {put,putError} from './write.ts';export function main():void{const source:{n:number;label:string;extra?:number}={n:1,label:'old'};put(source,'n',2.5);put(source,'label','new');put(source,'extra',3);console.log(JSON.stringify(source));try{put(source,'label',3);}catch(error){console.log(error instanceof TypeError);}try{put(source,'missing',4);}catch(error){console.log(error instanceof TypeError);}const errors={type:TypeError('old'),range:RangeError('old')};putError(errors,'type',TypeError('new'));try{putError(errors,'type',RangeError('wrong'));}catch(error){console.log(error instanceof TypeError);}console.log(errors.type.message);console.log(JSON.stringify(Object.keys(source)));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/write.ts"),"export function put(source:{n:number;label:string;extra?:number},key:string,item:number|string|undefined):void{source[key]=item;}export function putError(source:{type:TypeError;range:RangeError},key:string,item:TypeError|RangeError|number):void{source[key]=item;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(6)
                    .collect::<Vec<_>>(),
                [
                    "{\"n\":2.5,\"label\":\"new\",\"extra\":3}",
                    "1",
                    "1",
                    "1",
                    "new",
                    "[\"n\",\"label\",\"extra\"]"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn computed_record_writes_preserve_operand_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):{n:number}{varta.env.print(resource);return {n:1};}function key(resource:string):string{varta.env.print(resource);return 'n';}function item(resource:string):number|string{varta.env.print(resource);return 2;}function stop():number|string{varta.process.exit(7);return 4;}export function main():void{source('VARTA_WRITE_ALLOWED')['n'+'']=2;console.log(varta.result.status());const value={n:1,s:'old'};value[key('VARTA_WRITE_DENIED')]=3;console.log(varta.result.status());console.log(value.n);value['n'+'']=item('VARTA_WRITE_ALLOWED');console.log(varta.result.status());console.log(value.n);try{value['n'+'']=stop();}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(value.n);}",
        serde_json::json!({"env":{"print":["VARTA_WRITE_ALLOWED"],"get":["VARTA_WRITE_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_WRITE_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_WRITE_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert!(text.contains(&format!("varta log: {}\nvarta log: <capability denied: env> (VARTA_WRITE_DENIED)\nvarta log: 1\nvarta log: 3",u8::from(restricted))),"{text}");
            assert!(
                text.contains(&format!(
                    "varta log: {}\nvarta log: 2",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: continued\nvarta log: 4"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn computed_record_writes_invalid_keys_and_values_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        (
            "export function bad():void{const value={n:1};value[true]=2;}",
            "true",
        ),
        (
            "export function bad():void{const value={n:1};const key='n';value[key]='wrong';}",
            "'wrong'",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.find(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn record_assignment_values_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {assign,chain,setChild} from './assign.ts';export function main():void{const a:{n?:number}={};const b:{n?:number}={};const result:number=chain(a,b,'n',3);console.log(result+1);console.log(JSON.stringify(a));console.log(JSON.stringify(b));const holder:{child?:{text:string}}={};const child={text:'owned'+'!'};console.log(setChild(holder,'child',child)===child);const exact:number=assign(a,'n',4.5);console.log(exact);console.log(JSON.stringify(holder));try{chain(a,b,'missing',8);}catch(error){console.log(error instanceof TypeError);}console.log(a.n);console.log(b.n);}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/assign.ts"),"export function assign(target:{n?:number},key:string,item:number):number{return target[key]=item;}export function chain(a:{n?:number},b:{n?:number},key:string,item:number):number{return a[key]=b['n']=item;}export function setChild(target:{child?:{text:string}},key:string,item:{text:string}):{text:string}{return target[key]=item;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(9)
                    .collect::<Vec<_>>(),
                [
                    "4",
                    "{\"n\":3}",
                    "{\"n\":3}",
                    "1",
                    "4.5",
                    "{\"child\":{\"text\":\"owned!\"}}",
                    "1",
                    "4.5",
                    "8"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn record_assignment_values_preserve_operand_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):{n:number}{varta.env.print(resource);return {n:1};}function key(resource:string):string{varta.env.print(resource);return 'n';}function item(resource:string):number|string{varta.env.print(resource);return 2;}function stop():number|string{varta.process.exit(7);return 4;}export function main():void{console.log(source('VARTA_ASSIGN_ALLOWED')['n'+'']=2);console.log(varta.result.status());const value={n:1,s:'old'};console.log(value[key('VARTA_ASSIGN_DENIED')]=3);console.log(varta.result.status());console.log(value['n'+'']=item('VARTA_ASSIGN_ALLOWED'));console.log(varta.result.status());try{console.log(value['n'+'']=stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(value.n);}",
        serde_json::json!({"env":{"print":["VARTA_ASSIGN_ALLOWED"],"get":["VARTA_ASSIGN_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_ASSIGN_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_ASSIGN_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert_eq!(
                text.matches(&format!(
                    "varta log: 2\nvarta log: {}",
                    u8::from(restricted)
                ))
                .count(),
                2,
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_ASSIGN_DENIED)\nvarta log: 3\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 4\nvarta log: continued\nvarta log: 4"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn record_assignment_values_invalid_keys_and_rhs_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        (
            "export function bad():number{const value={n:1};return value[true]=2;}",
            "true",
        ),
        (
            "export function bad():number{const value={n:1};return value['n']='wrong';}",
            "'wrong'",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.find(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn simple_assignment_values_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {assign,setFunction} from './simple.ts';export function main():void{const target:{n?:number}={};const values:(number|undefined)[]=[undefined];console.log(assign(target,values,3)+1);console.log(JSON.stringify(target));console.log(JSON.stringify(values));const selected=setFunction();console.log(selected());let number=0;console.log(number=target.n=values[0]=4.5);try{console.log(target['missing'+ '']=number=values[0]=8);}catch(error){console.log(error instanceof TypeError);}console.log(number);console.log(target.n);console.log(JSON.stringify(values));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/simple.ts"),"export function assign(target:{n?:number},values:(number|undefined)[],item:number):number{let local:number|string=0;return local=target.n=values[0]=item;}export function setFunction():()=>string{let fn:()=>string=()=>'';const text='owned'+'!';return fn=()=>text;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(9)
                    .collect::<Vec<_>>(),
                [
                    "4",
                    "{\"n\":3}",
                    "[3]",
                    "owned!",
                    "4.5",
                    "1",
                    "8",
                    "4.5",
                    "[8]"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn simple_assignment_values_preserve_grants_status_owned_pairs_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function rhs(resource:string):number{varta.env.print(resource);return 2;}function key(resource:string):number{varta.env.print(resource);return 0;}function stop():number{varta.process.exit(7);return 4;}export function main():void{let captured=varta.result.capture(varta.env.get('VARTA_SIMPLE_GET'));const chosen:VartaResult<string>=(captured=varta.result.capture(varta.env.get('VARTA_SIMPLE_GET')));let local=0;console.log(local=rhs('VARTA_SIMPLE_ALLOWED'));console.log(varta.result.status());const record={n:1};console.log(record.n=rhs('VARTA_SIMPLE_DENIED'));console.log(varta.result.status());const array=[0];let index=1;console.log(array[index=key('VARTA_SIMPLE_ALLOWED')]=rhs('VARTA_SIMPLE_DENIED'));console.log(varta.result.status());console.log(chosen.value);console.log(chosen.status);console.log(captured.value===chosen.value);try{console.log(local=array[0]=record.n=stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(local);console.log(record.n);console.log(array[0]);}",
        serde_json::json!({"env":{"print":["VARTA_SIMPLE_ALLOWED"],"get":["VARTA_SIMPLE_GET","VARTA_SIMPLE_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_SIMPLE_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_SIMPLE_DENIED", "NEVER_DISCLOSE")
                .env("VARTA_SIMPLE_GET", "OWNED_RESULT");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert_eq!(
                text.matches("OWNED_RESULT").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 2\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_SIMPLE_DENIED)\nvarta log: 2\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: {}\nvarta log: {}\nvarta log: 1",
                    if restricted { "" } else { "OWNED_RESULT" },
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(text.contains("varta log: 4\nvarta log: continued\nvarta log: 4\nvarta log: 4\nvarta log: 4"),"{text}");
            }
        }
    }
}

#[test]
fn simple_assignment_values_invalid_targets_and_rhs_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        (
            "export function bad():number{let n=0;return n='wrong';}",
            "'wrong'",
        ),
        (
            "export function bad():number{const value={n:0};return value.n='wrong';}",
            "'wrong'",
        ),
        (
            "export function bad():number{const a=[0];return a[true]=2;}",
            "true",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.find(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn compound_assignments_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {numeric,append} from './compound.ts';export function main():void{const record:{n?:number}={n:2};const values=[4];console.log(numeric(record,values,3));console.log(JSON.stringify(record));console.log(JSON.stringify(values));const text={s:'old'};console.log(append(text,'new'));console.log(record.n%=5);const missing:{n?:number}={};try{numeric(missing,values,1);}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(missing,'n'));console.log(JSON.stringify(values));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/compound.ts"),"export function numeric(record:{n?:number},values:number[],amount:number):number{let local=1;return local+=record.n*=values[0]+=amount;}export function append(record:{s:string},suffix:string):string{return record.s+=suffix;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(8)
                    .collect::<Vec<_>>(),
                ["15", "{\"n\":14}", "[7]", "oldnew", "4", "1", "0", "[8]"]
                    .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn compound_assignments_preserve_operand_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source(resource:string):number[]{varta.env.print(resource);return [1];}function key(resource:string):number{varta.env.print(resource);return 0;}function rhs(resource:string):number{varta.env.print(resource);return 2;}function stop():number{varta.process.exit(7);return 3;}export function main():void{let n=1;console.log(n+=rhs('VARTA_COMPOUND_ALLOWED'));console.log(varta.result.status());const record={n:2};console.log(record.n*=rhs('VARTA_COMPOUND_DENIED'));console.log(varta.result.status());console.log(source('VARTA_COMPOUND_ALLOWED')[key('VARTA_COMPOUND_DENIED')]+=rhs('VARTA_COMPOUND_ALLOWED'));console.log(varta.result.status());console.log(n%=2);console.log(varta.result.status());const missing:{n?:number}={};try{console.log(missing.n+=rhs('VARTA_COMPOUND_DENIED'));}catch(error){console.log(error instanceof TypeError);}console.log(varta.result.status());try{console.log(n+=record.n*=stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(n);console.log(record.n);}",
        serde_json::json!({"env":{"print":["VARTA_COMPOUND_ALLOWED"],"get":["VARTA_COMPOUND_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_COMPOUND_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_COMPOUND_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                3 * usize::from(!restricted),
                "{text}"
            );
            assert_eq!(
                text.matches(&format!(
                    "varta log: 3\nvarta log: {}",
                    u8::from(restricted)
                ))
                .count(),
                2,
                "{text}"
            );
            assert!(text.contains("varta log: 4\nvarta log: 1"), "{text}");
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_COMPOUND_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains(
                        "varta log: 13\nvarta log: continued\nvarta log: 13\nvarta log: 12"
                    ),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn compound_assignments_invalid_operands_and_targets_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid, last) in [
        (
            "export function bad():void{let n=1;n+='wrong';}",
            "'wrong'",
            false,
        ),
        (
            "export function bad():void{let text='x';text-=2;}",
            "text",
            true,
        ),
        (
            "export function bad():void{const a=[1];a[true]+=2;}",
            "true",
            false,
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = if last {
            source.rfind(invalid).unwrap()
        } else {
            source.find(invalid).unwrap()
        };
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn record_compounds_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {numeric,append} from './computed.ts';export function main():void{let local=1;const record:{n?:number}={n:2};const values=[4];console.log(local+=numeric(record,'n',values,3));console.log(JSON.stringify(record));console.log(JSON.stringify(values));const text={s:'old'};console.log(append(text,'s','new'));console.log(record['n'+'']%=5);const missing:{n?:number}={};try{numeric(missing,'n',values,1);}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(missing,'n'));console.log(JSON.stringify(values));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/computed.ts"), "export function numeric(record:{n?:number},key:string,values:number[],amount:number):number{return record[key]*=values[0]+=amount;}export function append(record:{s:string},key:string,suffix:string):string{return record[key]+=suffix;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(8)
                    .collect::<Vec<_>>(),
                ["15", "{\"n\":14}", "[7]", "oldnew", "4", "1", "0", "[8]"]
                    .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn record_compounds_preserve_operation_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source():{n:number}{varta.env.print('VARTA_RECORD_COMPOUND_ALLOWED');return {n:1};}function key():string{varta.env.print('VARTA_RECORD_COMPOUND_DENIED');return 'n';}function rhs(resource:string):number{varta.env.print(resource);return 2;}function stop():number{varta.process.exit(7);return 3;}export function main():void{console.log(source()[key()]+=rhs('VARTA_RECORD_COMPOUND_ALLOWED'));console.log(varta.result.status());const record={n:3};const name='n';console.log(record[name]*=rhs('VARTA_RECORD_COMPOUND_DENIED'));console.log(varta.result.status());console.log(record[name]%=5);console.log(varta.result.status());const missing:{n?:number}={};try{missing[name]+=rhs('VARTA_RECORD_COMPOUND_DENIED');}catch(error){console.log(error instanceof TypeError);}console.log(varta.result.status());try{record['to'+'String']+=rhs('VARTA_RECORD_COMPOUND_ALLOWED');}catch(error){console.log(error instanceof TypeError);}console.log(varta.result.status());try{console.log(record[name]*=stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(record.n);}",
        serde_json::json!({"env":{"print":["VARTA_RECORD_COMPOUND_ALLOWED"],"get":["VARTA_RECORD_COMPOUND_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_RECORD_COMPOUND_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_RECORD_COMPOUND_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 3\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains("varta log: 6\nvarta log: 1\nvarta log: 1\nvarta log: 1"),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_RECORD_COMPOUND_DENIED)\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 3\nvarta log: continued\nvarta log: 3"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn record_compounds_invalid_keys_and_operands_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        (
            "export function bad():void{const value={n:1};const key=1;value[key]+=2;}",
            "key",
        ),
        (
            "export function bad():void{const value={n:1};const key:string|undefined='n';value[key]+=2;}",
            "key",
        ),
        (
            "export function bad():void{const value={n:1};const key='n';value[key]+='wrong';}",
            "'wrong'",
        ),
        (
            "export function bad():void{const value={s:'x'};const key='s';value[key]-=2;}",
            "value[key]",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn logical_assignments_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {fallback,keep} from './logical.ts';export function main():void{const record:{n?:number}={};const values=[0];console.log(fallback(record,'n',values,5));console.log(JSON.stringify(record));console.log(JSON.stringify(values));console.log(fallback(record,'n',values,7));console.log(JSON.stringify(values));const text:{s?:string}={s:''};console.log(keep(text,'s','owned'));console.log(record['n'+'']&&=3);console.log(record['missing'+'']&&=9);console.log(Object.hasOwn(record,'missing'));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/logical.ts"),"export function fallback(record:{n?:number},key:string,values:number[],amount:number):number{return record[key]??=values[0]||=amount;}export function keep(record:{s?:string},key:string,text:string):string{return record[key]||=text;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(9)
                    .collect::<Vec<_>>(),
                [
                    "5",
                    "{\"n\":5}",
                    "[5]",
                    "5",
                    "[5]",
                    "owned",
                    "3",
                    "undefined",
                    "0"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn logical_assignments_preserve_branch_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source():{n:number}{varta.env.print('VARTA_LOGICAL_ALLOWED');return {n:0};}function key():string{varta.env.print('VARTA_LOGICAL_DENIED');return 'n';}function rhs(resource:string):number{varta.env.print(resource);return 2;}function stop():number{varta.process.exit(7);return 3;}export function main():void{let first=1;console.log(first||=rhs('VARTA_LOGICAL_ALLOWED'));console.log(varta.result.status());console.log(source()[key()]||=rhs('VARTA_LOGICAL_ALLOWED'));console.log(varta.result.status());const record={n:2};const name='n';console.log(record[name]||=rhs('VARTA_LOGICAL_DENIED'));console.log(varta.result.status());console.log(record[name]&&=rhs('VARTA_LOGICAL_DENIED'));console.log(varta.result.status());let text='kept';console.log(text||=varta.env.get('VARTA_LOGICAL_DENIED'));console.log(varta.result.status());const missing:{n?:number}={};console.log(missing[name]&&=rhs('VARTA_LOGICAL_ALLOWED'));console.log(varta.result.status());console.log(missing[name]??=rhs('VARTA_LOGICAL_DENIED'));console.log(varta.result.status());let exitValue:number|undefined=undefined;try{console.log(exitValue??=stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(exitValue);}",
        serde_json::json!({"env":{"print":["VARTA_LOGICAL_ALLOWED"],"get":["VARTA_LOGICAL_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_LOGICAL_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_LOGICAL_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert!(text.starts_with("varta log: 1\nvarta log: 0\n"), "{text}");
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert_eq!(
                text.matches(&format!(
                    "varta log: 2\nvarta log: {}",
                    u8::from(restricted)
                ))
                .count(),
                if restricted { 4 } else { 2 },
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_LOGICAL_DENIED)\nvarta log: 2\nvarta log: 1\nvarta log: kept\nvarta log: 1\nvarta log: undefined\nvarta log: 1"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 3\nvarta log: continued\nvarta log: 3"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn logical_assignments_invalid_keys_and_rhs_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        (
            "export function bad():void{let n=1;n||='wrong';}",
            "'wrong'",
        ),
        (
            "export function bad():void{const value={n:0};const key=1;value[key]??=2;}",
            "key",
        ),
        (
            "export function bad():void{const a=[1];a[true]&&=2;}",
            "true",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn updates_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {post,pre} from './updates.ts';export function main():void{const record:{n?:number}={n:2};const values=[4];console.log(post(record,'n',values));console.log(JSON.stringify(record));console.log(JSON.stringify(values));console.log(pre(record,'n'));console.log(record['n'+'']--);console.log(JSON.stringify(record));const missing:{n?:number}={};try{post(missing,'n',values);}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(missing,'n'));console.log(JSON.stringify(values));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/updates.ts"),"export function post(record:{n?:number},key:string,values:number[]):number{return record[key]++ + ++values[0];}export function pre(record:{n?:number},key:string):number{return ++record[key];}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(9)
                    .collect::<Vec<_>>(),
                [
                    "7",
                    "{\"n\":3}",
                    "[5]",
                    "4",
                    "4",
                    "{\"n\":3}",
                    "1",
                    "0",
                    "[5]"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn updates_preserve_operand_operation_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source():{n:number}{varta.env.print('VARTA_UPDATE_ALLOWED');return {n:1};}function key():string{varta.env.print('VARTA_UPDATE_DENIED');return 'n';}function exitKey():string{varta.process.exit(7);return 'n';}export function main():void{console.log(source()[key()]++);console.log(varta.result.status());const record={n:2};const name='n';varta.env.print('VARTA_UPDATE_ALLOWED');console.log(++record[name]);console.log(varta.result.status());varta.env.print('VARTA_UPDATE_DENIED');console.log(record[name]--);console.log(varta.result.status());const missing:{n?:number}={};try{console.log(missing[name]++);}catch(error){console.log(error instanceof TypeError);}console.log(varta.result.status());console.log(Object.hasOwn(missing,name));try{console.log(++record[exitKey()]);}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(record.n);}",
        serde_json::json!({"env":{"print":["VARTA_UPDATE_ALLOWED"],"get":["VARTA_UPDATE_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_UPDATE_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_UPDATE_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_UPDATE_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 3\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_UPDATE_DENIED)\nvarta log: 3\nvarta log: 1\nvarta log: 1\nvarta log: 1\nvarta log: 0"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 3\nvarta log: continued\nvarta log: 3"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn updates_invalid_keys_and_numeric_targets_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        ("export function bad():void{let text='1';text++;}", "text"),
        (
            "export function bad():void{const value={n:1};const key=0;++value[key];}",
            "key",
        ),
        (
            "export function bad():void{const a=[1];a[false]--;}",
            "false",
        ),
        (
            "export function bad():void{const value={s:'1'};const key='s';value[key]++;}",
            "value[key]",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn bitwise_operators_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {flags,unsigned,complement,shift} from './bits.ts';export function main():void{console.log(flags(259,5));console.log(unsigned(-1));console.log(complement(1.5));console.log(shift(-8,33));console.log(unsigned(flags(4294967297,0)));console.log(unsigned(1.7976931348623157e308));const record={n:unsigned(-1)};console.log(JSON.stringify(record));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/bits.ts"),"export function flags(value:number,count:number):number{return ((value&255)|16)^(1<<count);}export function unsigned(value:number):number{return value>>>0;}export function complement(value:number):number{return ~value;}export function shift(value:number,count:number):number{return value>>count;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(7)
                    .collect::<Vec<_>>(),
                [
                    "51",
                    "4294967295",
                    "-2",
                    "-4",
                    "16",
                    "0",
                    "{\"n\":4294967295}"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn bitwise_operators_preserve_operand_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function left(resource:string):number{varta.env.print(resource);return 4294967297;}function right(resource:string):number{varta.env.print(resource);return 1;}function exitValue():number{varta.process.exit(7);return 32;}export function main():void{console.log(left('VARTA_BITWISE_ALLOWED')|right('VARTA_BITWISE_DENIED'));console.log(varta.result.status());console.log(left('VARTA_BITWISE_DENIED')<<right('VARTA_BITWISE_ALLOWED'));console.log(varta.result.status());console.log(~left('VARTA_BITWISE_ALLOWED'));console.log(varta.result.status());console.log(NaN|Infinity);console.log(varta.result.status());console.log(-1>>>0);console.log(varta.result.status());try{console.log(left('VARTA_BITWISE_ALLOWED')^exitValue());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(varta.result.status());}",
        serde_json::json!({"env":{"print":["VARTA_BITWISE_ALLOWED"],"get":["VARTA_BITWISE_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_BITWISE_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_BITWISE_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                4 * usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_BITWISE_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            let status = u8::from(restricted);
            assert!(
                text.contains(&format!("varta log: 2\nvarta log: {status}")),
                "{text}"
            );
            assert!(text.contains(&format!("varta log: -2\nvarta log: {status}\nvarta log: 0\nvarta log: {status}\nvarta log: 4294967295\nvarta log: {status}")),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 33\nvarta log: continued\nvarta log: 1"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn bitwise_operators_invalid_operand_types_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        ("export function bad():void{console.log('1'|0);}", "'1'"),
        (
            "export function bad():void{console.log(1<<false);}",
            "false",
        ),
        (
            "export function bad():void{const value:number|undefined=1;console.log(~value);}",
            "value",
        ),
        ("export function bad():void{console.log(null>>>0);}", "null"),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn bitwise_compounds_run_across_modules_and_source_free_artifacts() {
    for profile in ["checked-f64-utf16-v12", "checked-f64-utf16-v14"] {
        let project = RegressionProject::new(
            "import {mix,unsigned} from './compound-bits.ts';export function main():void{const record:{n?:number}={n:2};const values=[4];console.log(mix(record,'n',values,1));console.log(JSON.stringify(record));console.log(JSON.stringify(values));record.n=-1;console.log(unsigned(record,'n'));console.log(JSON.stringify(record));const missing:{n?:number}={};try{mix(missing,'n',values,1);}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(missing,'n'));console.log(JSON.stringify(values));}",
            serde_json::json!({}),
        );
        fs::write(project.0.join("src/compound-bits.ts"),"export function mix(record:{n?:number},key:string,values:number[],mask:number):number{let local=1;return local|=record[key]<<=values[0]^=mask;}export function unsigned(record:{n?:number},key:string):number{return record[key]>>>=0;}").unwrap();
        project.language_profile(profile);
        let check = project.command(&["check"]);
        assert!(check.status.success(), "{}", combined(&check));
        let build = project.command(&["build", "--single"]);
        assert!(build.status.success(), "{}", combined(&build));
        let mut dev = varta();
        dev.arg("dev");
        let mut commands = vec![dev];
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for (index, mut command) in commands.into_iter().enumerate() {
            if index == 1 {
                fs::remove_dir_all(project.0.join("src")).unwrap();
                fs::remove_file(project.0.join("varta.config.json")).unwrap();
            }
            let output = command.current_dir(&project.0).output().unwrap();
            assert!(output.status.success(), "{profile}: {}", combined(&output));
            assert_eq!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .take(8)
                    .collect::<Vec<_>>(),
                [
                    "65",
                    "{\"n\":64}",
                    "[5]",
                    "4294967295",
                    "{\"n\":4294967295}",
                    "1",
                    "0",
                    "[4]"
                ]
                .map(|s| format!("varta log: {s}")),
                "{profile}"
            );
        }
    }
}

#[test]
fn bitwise_compounds_preserve_operand_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function source():{n:number}{varta.env.print('VARTA_BITWISE_COMPOUND_ALLOWED');return {n:1};}function key():string{varta.env.print('VARTA_BITWISE_COMPOUND_DENIED');return 'n';}function rhs(resource:string):number{varta.env.print(resource);return 2;}function stop():number{varta.process.exit(7);return 1;}export function main():void{console.log(source()[key()]|=rhs('VARTA_BITWISE_COMPOUND_ALLOWED'));console.log(varta.result.status());const record={n:-1};const name='n';console.log(record[name]>>>=rhs('VARTA_BITWISE_COMPOUND_DENIED'));console.log(varta.result.status());console.log(record[name]&=255);console.log(varta.result.status());const missing:{n?:number}={};try{console.log(missing[name]|=rhs('VARTA_BITWISE_COMPOUND_DENIED'));}catch(error){console.log(error instanceof TypeError);}console.log(varta.result.status());console.log(Object.hasOwn(missing,name));try{console.log(record[name]^=stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(record.n);}",
        serde_json::json!({"env":{"print":["VARTA_BITWISE_COMPOUND_ALLOWED"],"get":["VARTA_BITWISE_COMPOUND_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v14");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_BITWISE_COMPOUND_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_BITWISE_COMPOUND_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 3\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains("varta log: 1073741823\nvarta log: 1\nvarta log: 255\nvarta log: 1"),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_BITWISE_COMPOUND_DENIED)\nvarta log: 1\nvarta log: 1\nvarta log: 0"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 254\nvarta log: continued\nvarta log: 254"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn bitwise_compounds_invalid_targets_and_rhs_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v14");
    for (source, invalid) in [
        ("export function bad():void{let text='1';text|=2;}", "text"),
        (
            "export function bad():void{let n=1;n<<='wrong';}",
            "'wrong'",
        ),
        (
            "export function bad():void{const value={s:'x'};const key='s';value[key]^=1;}",
            "value[key]",
        ),
        (
            "export function bad():void{const a=[1];a[false]&=2;}",
            "false",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn power_profile_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {evaluate,update} from './power.ts';export function main():void{const record:{n?:number}={n:2};const values=[3];console.log(update(record,'n',values));console.log(JSON.stringify(record));console.log(JSON.stringify(values));console.log(evaluate(9,0.5));console.log(evaluate(1,NaN));console.log(1/evaluate(-0,3));const missing:{n?:number}={};try{update(missing,'n',values);}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(missing,'n'));console.log(JSON.stringify(values));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/power.ts"),"export function evaluate(base:number,exponent:number):number{return base**exponent;}export function update(record:{n?:number},key:string,values:number[]):number{return record[key]**=values[0]**=2;}").unwrap();
    project.language_profile("checked-f64-utf16-v15");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 18);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v15");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(9)
                .collect::<Vec<_>>(),
            [
                "512",
                "{\"n\":512}",
                "[9]",
                "3",
                "NaN",
                "-Infinity",
                "1",
                "0",
                "[81]"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn power_profile_preserves_operand_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function base(resource:string):number{varta.env.print(resource);return 2;}function exponent(resource:string):number{varta.env.print(resource);return 3;}function stop():number{varta.process.exit(7);return 2;}export function main():void{console.log(Math.pow(base('VARTA_POWER_ALLOWED'),exponent('VARTA_POWER_DENIED')));console.log(varta.result.status());console.log(base('VARTA_POWER_DENIED')**exponent('VARTA_POWER_ALLOWED'));console.log(varta.result.status());const record={n:2};const name='n';console.log(record[name]**=exponent('VARTA_POWER_DENIED'));console.log(varta.result.status());console.log(1**NaN);console.log(varta.result.status());const missing:{n?:number}={};try{console.log(missing[name]**=exponent('VARTA_POWER_DENIED'));}catch(error){console.log(error instanceof TypeError);}console.log(varta.result.status());console.log(Object.hasOwn(missing,name));try{console.log(record[name]**=stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}console.log(record.n);}",
        serde_json::json!({"env":{"print":["VARTA_POWER_ALLOWED"],"get":["VARTA_POWER_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v15");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_POWER_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_POWER_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                2 * usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_POWER_DENIED)\nvarta log: 8\nvarta log: 1"
                ),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 8\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains("varta log: 8\nvarta log: 1\nvarta log: NaN\nvarta log: 1"),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_POWER_DENIED)\nvarta log: 1\nvarta log: 1\nvarta log: 0"),"{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            if restricted {
                assert!(
                    text.contains("varta log: 64\nvarta log: continued\nvarta log: 64"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn power_profile_invalid_types_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v15");
    for (source, invalid) in [
        ("export function bad():void{console.log('2'**3);}", "'2'"),
        (
            "export function bad():void{console.log(2**false);}",
            "false",
        ),
        ("export function bad():void{let text='2';text**=3;}", "text"),
        (
            "export function bad():void{let n=2;n**='wrong';}",
            "'wrong'",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn math_profile_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {evaluate} from './math.ts';export function main():void{console.log(JSON.stringify(evaluate(-6.25)));console.log(Math.round(0.49999999999999994));console.log(1/Math.round(-0.5));console.log(1/Math.abs(-0));console.log(Math.min());console.log(Math.max());console.log(Math.imul(4294967295,5));console.log(Math.clz32(1));console.log(1/Math.sign(-0));console.log(Math.fround(16777217));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/math.ts"),"export function evaluate(x:number):number[]{return [Math.abs(x),Math.floor(x),Math.ceil(x),Math.trunc(x),Math.round(x),Math.sqrt(Math.abs(x))];}").unwrap();
    project.language_profile("checked-f64-utf16-v15");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 18);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v15");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(10)
                .collect::<Vec<_>>(),
            [
                "[6.25,-7,-6,-6,-6,2.5]",
                "0",
                "-Infinity",
                "Infinity",
                "Infinity",
                "-Infinity",
                "-5",
                "31",
                "-Infinity",
                "16777216"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn math_arguments_preserve_exact_grants_host_ceiling_status_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(resource:string):number{varta.env.print(resource);return -2.5;}function stop():number{varta.process.exit(7);return -0.5;}export function main():void{console.log(Math.max(Math.f16round(Math.abs(read('VARTA_MATH_ALLOWED'))),Math.imul(1,2)));console.log(varta.result.status());console.log(Math.min(Math.round(Math.f16round(read('VARTA_MATH_DENIED'))),Math.fround(1),Math.clz32(1)));console.log(varta.result.status());console.log(Math.max(NaN,1));console.log(varta.result.status());try{console.log(Math.min(NaN,Math.sign(stop())));}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_MATH_ALLOWED"],"get":["VARTA_MATH_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v15");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_MATH_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_MATH_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 2.5\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_MATH_DENIED)\nvarta log: -2\nvarta log: 1\nvarta log: NaN\nvarta log: 1"),"{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
        }
    }
}

#[test]
fn math_invalid_arguments_keep_dependency_source_ranges_and_shadowing() {
    let project = RegressionProject::new(
        "import {bad} from './bad.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v15");
    for (source, invalid) in [
        ("export function bad():void{Math.abs('2');}", "'2'"),
        ("export function bad():void{Math.floor(false);}", "false"),
        (
            "export function bad():void{const n:number|undefined=2;Math.sqrt(n);}",
            "n",
        ),
        (
            "export function bad():void{Math.imul(1,'wrong');}",
            "'wrong'",
        ),
        ("export function bad():void{Math.max(1,false);}", "false"),
        (
            "export function bad():void{Math.hypot(1,'wrong');}",
            "'wrong'",
        ),
    ] {
        let dependency = project.0.join("src/bad.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
    }
    fs::write(
        project.0.join("src/bad.ts"),
        "import {Math} from './shadow.ts';export function bad():void{Math.abs(2);}",
    )
    .unwrap();
    fs::write(
        project.0.join("src/shadow.ts"),
        "export function Math():number{return 1;}",
    )
    .unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("shadowed Math"),
        "{}",
        combined(&check)
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn number_predicates_run_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {inspect} from './numbers.ts';export function main():void{inspect(2);inspect(NaN);inspect('2');console.log(Number.isSafeInteger(9007199254740991));console.log(Number.isSafeInteger(9007199254740992));console.log(Number.isFinite());console.log(Number.isInteger(-0));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/numbers.ts"),"export function inspect(value:number|string):void{console.log(Number.isFinite(value));console.log(Number.isNaN(value));console.log(Number.isInteger(value));}").unwrap();
    project.language_profile("checked-f64-utf16-v15");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(13)
                .collect::<Vec<_>>(),
            [
                "1", "0", "1", "0", "1", "0", "0", "0", "0", "1", "0", "0", "1"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn number_predicates_ignored_arguments_preserve_grants_status_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):number{varta.env.print(name);return 3;}function stop():number{varta.process.exit(7);return 1;}export function main():void{console.log(Number.isFinite(2,read('VARTA_NUMBER_ALLOWED')));console.log(varta.result.status());console.log(Number.isNaN('NaN',read('VARTA_NUMBER_DENIED')));console.log(varta.result.status());console.log(Number.isInteger(Infinity));console.log(Number.isSafeInteger(-0));console.log(varta.result.status());try{console.log(Number.isFinite('2',stop()));}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_NUMBER_ALLOWED"],"get":["VARTA_NUMBER_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v15");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_NUMBER_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_NUMBER_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_NUMBER_DENIED)\nvarta log: 0\nvarta log: 1\nvarta log: 0\nvarta log: 1\nvarta log: 1"),"{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
        }
    }
}

#[test]
fn numeric_constants_run_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {bounds} from './constants.ts';export function main():void{console.log(JSON.stringify(bounds()));console.log(Math.PI);console.log(1+Number.EPSILON>1);console.log(Number.isSafeInteger(Number.MAX_SAFE_INTEGER));console.log(Math['PI']===Math.PI);console.log(Math.pow(2,3));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/constants.ts"),"export function bounds():number[]{return [Number.MIN_VALUE,Number.MAX_SAFE_INTEGER,Number.NEGATIVE_INFINITY];}").unwrap();
    project.language_profile("checked-f64-utf16-v15");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(6)
                .collect::<Vec<_>>(),
            [
                "[5e-324,9007199254740991,null]",
                "3.141592653589793",
                "1",
                "1",
                "1",
                "8"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn numeric_constant_write_diagnostics_keep_dependency_target_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './constants.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v15");
    for (statement, target) in [
        ("Math.PI=1;", "Math.PI"),
        ("Number.MIN_VALUE++;", "Number.MIN_VALUE"),
        ("Math['PI']||=2;", "Math['PI']"),
        ("[Number.EPSILON]=[1];", "Number.EPSILON"),
    ] {
        let source = format!("export function bad():void{{{statement}}}");
        let dependency = project.0.join("src/constants.ts");
        fs::write(&dependency, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert!(
            diagnostic["message"]
                .as_str()
                .unwrap()
                .contains("read-only Math/Number constant")
        );
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.find(target).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + target.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn f16_rounding_runs_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {half} from './half.ts';export function main():void{console.log(half(1.0004882812500002));console.log(half(Math.fround(1.0004882812500002)));console.log(1/half(-0.00000001));console.log(half(65519));console.log(half(65520));console.log(half(-65520));console.log(Number.isNaN(half(NaN)));console.log(half(5.960464477539063e-8)>0);}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/half.ts"),
        "export function half(value:number):number{return Math.f16round(value);}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v15");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(8)
                .collect::<Vec<_>>(),
            [
                "1.0009765625",
                "1",
                "-Infinity",
                "65504",
                "Infinity",
                "-Infinity",
                "1",
                "1"
            ]
            .map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn f16_invalid_arguments_keep_dependency_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './half.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v15");
    for (source, invalid) in [
        ("export function bad():void{Math.f16round('2');}", "'2'"),
        ("export function bad():void{Math.f16round(false);}", "false"),
        (
            "export function bad():void{const n:number|undefined=1;Math.f16round(n);}",
            "n",
        ),
    ] {
        let dependency = project.0.join("src/half.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn hypot_runs_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {norm} from './norm.ts';export function main():void{console.log(norm(3,4,12));console.log(Math.hypot());console.log(1/Math.hypot(-0,0));console.log(Math.hypot(NaN,Infinity));console.log(Number.isNaN(Math.hypot(NaN,1)));console.log(Number.isFinite(norm(3e200,4e200,0)));console.log(norm(3e-200,4e-200,0)>0);}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/norm.ts"),
        "export function norm(a:number,b:number,c:number):number{return Math.hypot(a,b,c);}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v15");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(7)
                .collect::<Vec<_>>(),
            ["13", "0", "Infinity", "Infinity", "1", "1", "1"].map(|s| format!("varta log: {s}"))
        );
    }
}

#[test]
fn hypot_finishes_argument_grants_before_infinity_and_preserves_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):number{varta.env.print(name);return 3;}function stop():number{varta.process.exit(7);return 4;}export function main():void{console.log(Math.hypot(read('VARTA_HYPOT_ALLOWED'),4));console.log(varta.result.status());console.log(Math.hypot(NaN,read('VARTA_HYPOT_DENIED'),Infinity));console.log(varta.result.status());try{console.log(Math.hypot(Infinity,stop()));}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_HYPOT_ALLOWED"],"get":["VARTA_HYPOT_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v15");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_HYPOT_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_HYPOT_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 5\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_HYPOT_DENIED)\nvarta log: Infinity\nvarta log: 1"),"{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
        }
    }
}

#[test]
fn native_math_profile_runs_every_operation_in_source_free_artifacts() {
    use varta_common::native_math::NativeMath;
    let project = RegressionProject::new(
        "import {evaluate} from './math.ts';export function main():void{console.log(JSON.stringify(evaluate()));console.log(1/Math.sin(-0));console.log(Math.log(0));console.log(Math.atan2(-0,-1));console.log(Number.isNaN(Math.acosh(0.5)));}",
        serde_json::json!({}),
    );
    let expressions = NativeMath::ALL
        .iter()
        .map(|operation| {
            let args = match operation {
                NativeMath::Atan2 => "0,1",
                NativeMath::Acos
                | NativeMath::Acosh
                | NativeMath::Log
                | NativeMath::Log10
                | NativeMath::Log2 => "1",
                _ => "0",
            };
            format!("Math.{}({args})", operation.name())
        })
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        project.0.join("src/math.ts"),
        format!("export function evaluate():number[]{{return [{expressions}];}}"),
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v16");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 19);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v16");
    let values = NativeMath::ALL
        .iter()
        .map(|operation| {
            if matches!(
                operation,
                NativeMath::Cos | NativeMath::Cosh | NativeMath::Exp
            ) {
                "1"
            } else {
                "0"
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let expected = [
        format!("varta log: [{values}]"),
        "varta log: -Infinity".into(),
        "varta log: -Infinity".into(),
        "varta log: -3.141592653589793".into(),
        "varta log: 1".into(),
    ];
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn native_math_preserves_argument_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(resource:string):number{varta.env.print(resource);return 0;}function stop():number{varta.process.exit(7);return 0;}export function main():void{console.log(Math.sin(read('VARTA_NATIVE_MATH_ALLOWED')));console.log(varta.result.status());console.log(Math.atan2(read('VARTA_NATIVE_MATH_DENIED'),-1));console.log(varta.result.status());console.log(Math.acos(2));console.log(varta.result.status());try{console.log(Math.exp(stop()));}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_NATIVE_MATH_ALLOWED"],"get":["VARTA_NATIVE_MATH_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v16");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_NATIVE_MATH_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_NATIVE_MATH_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 0\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_NATIVE_MATH_DENIED)\nvarta log: 3.141592653589793\nvarta log: 1\nvarta log: NaN\nvarta log: 1"),"{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(!text.contains("BAD_CATCH"), "{text}");
            assert!(!text.contains("NEVER_DISCLOSE"), "{text}");
        }
    }
}

#[test]
fn native_math_argument_diagnostics_preserve_dependency_ranges_and_old_profile_gates() {
    let project = RegressionProject::new(
        "import {bad} from './math.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v16");
    for (source, invalid) in [
        ("export function bad():void{Math.sin('1');}", "'1'"),
        ("export function bad():void{Math.atan2(1,false);}", "false"),
        (
            "export function bad():void{const n:number|undefined=1;Math.log(n);}",
            "n",
        ),
    ] {
        let dependency = project.0.join("src/math.ts");
        fs::write(&dependency, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "type_error");
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
    }
    project.language_profile("checked-f64-utf16-v15");
    fs::write(
        project.0.join("src/math.ts"),
        "export function bad():void{Math.sin(1);}",
    )
    .unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("require checked-f64-utf16-v16"),
        "{}",
        combined(&check)
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn random_profile_runs_in_source_free_portable_and_native_artifacts() {
    let project = RegressionProject::new(
        "import {draw} from './random.ts';function mark():string{console.log('argument');return 'ignored';}export function main():void{const values:number[]=[];for(let i=0;i<32;i++){values.push(draw());}let valid=true;for(const x of values){if(x===undefined){valid=false;}else if(!Number.isFinite(x)||x<0||x>=1){valid=false;}}console.log(valid);console.log(values.length);const extra=Math.random(mark(),{text:'value'},[1,2],undefined);console.log(extra>=0&&extra<1);console.log(Math.sin(0));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/random.ts"),
        "export function draw():number{return Math.random();}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v17");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 20);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v17");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            [
                "varta log: 1",
                "varta log: 32",
                "varta log: argument",
                "varta log: 1",
                "varta log: 0"
            ]
        );
    }
}

#[test]
fn random_ignored_arguments_keep_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(resource:string):string{varta.env.print(resource);return 'ignored';}function stop():string{varta.process.exit(7);return 'ignored';}export function main():void{const a=Math.random(read('VARTA_RANDOM_ALLOWED'));console.log(a>=0&&a<1);console.log(varta.result.status());const b=Math.random(read('VARTA_RANDOM_DENIED'));console.log(b>=0&&b<1);console.log(varta.result.status());try{Math.random(stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_RANDOM_ALLOWED"],"get":["VARTA_RANDOM_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v17");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_RANDOM_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_RANDOM_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_RANDOM_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}

#[test]
fn random_profile_rejections_keep_dependency_diagnostics_and_leave_no_artifacts() {
    let project = RegressionProject::new(
        "import {bad} from './random.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let dependency = project.0.join("src/random.ts");
    for (profile, source, expected) in [
        (
            "checked-f64-utf16-v16",
            "export function bad():void{Math.random();}",
            "checked-f64-utf16-v17",
        ),
        (
            "checked-f64-utf16-v17",
            "export function bad():void{Math.random(...[1]);}",
            "spread",
        ),
        (
            "checked-f64-utf16-v17",
            "export function bad():void{const f=Math.random;f();}",
            "unsupported",
        ),
    ] {
        project.language_profile(profile);
        fs::write(&dependency, source).unwrap();
        let output = project.command(&["check", "--json"]);
        assert_eq!(output.status.code(), Some(1), "{}", combined(&output));
        let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(dependency.canonicalize().unwrap())
        );
        assert!(
            diagnostic["message"].as_str().unwrap().contains(expected),
            "{info}"
        );
        assert!(!project.command(&["build", "--single"]).status.success());
        assert!(!project.0.join("dist/app.wasm").exists());
        assert!(!project.0.join("dist/app").exists());
    }
}

#[test]
fn number_text_profile_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {values} from './numbers.ts';export function main():void{values();console.log(JSON.stringify([1e21,-0,NaN]));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/numbers.ts"),"export function values():void{console.log(-0);console.log(1e-7);console.log(1e-6);console.log(1e20);console.log(1e21);const n:number|string=1e21;console.log(n);console.log(Number.MIN_VALUE);console.log(Number.MAX_VALUE);console.log(NaN);console.log(Infinity);}").unwrap();
    project.language_profile("checked-f64-utf16-v18");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspection = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspection.status.success(), "{}", combined(&inspection));
    let info: serde_json::Value = serde_json::from_slice(&inspection.stdout).unwrap();
    assert_eq!(info["abiVersion"], 21);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v18");
    let expected = [
        "0",
        "1e-7",
        "0.000001",
        "100000000000000000000",
        "1e+21",
        "1e+21",
        "5e-324",
        "1.7976931348623157e+308",
        "NaN",
        "Infinity",
        "[1e+21,0,null]",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn number_text_arguments_preserve_grants_status_host_ceiling_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):number{varta.env.print(name);return 1e21;}function stop():number{varta.process.exit(7);return 1e21;}function show(value:number|undefined):void{console.log(value);}export function main():void{console.log(read('VARTA_NUMBER_TEXT_ALLOWED'));console.log(varta.result.status());show(read('VARTA_NUMBER_TEXT_DENIED'));console.log(varta.result.status());try{console.log(stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_NUMBER_TEXT_ALLOWED"],"get":["VARTA_NUMBER_TEXT_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v18");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_NUMBER_TEXT_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_NUMBER_TEXT_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1e+21\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_NUMBER_TEXT_DENIED)\nvarta log: 1e+21\nvarta log: 1"),"{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}

#[test]
fn number_text_profile_keeps_dependency_type_diagnostics_and_no_failed_outputs() {
    let project = RegressionProject::new(
        "import {bad} from './numbers.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v18");
    let source = "export function bad():void{console.log(1e21+'text');}";
    let file = project.0.join("src/numbers.ts");
    fs::write(&file, source).unwrap();
    let output = project.command(&["check", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        info["diagnostics"][0]["file"],
        serde_json::json!(file.canonicalize().unwrap())
    );
    assert_eq!(info["diagnostics"][0]["code"], "type_error");
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn template_literals_v32_run_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {activate,agent} from './api.ts';export function main():void{console.log(activate(42));console.log(agent({userAgent:`varta/${0.5}`},'pilot'));console.log(`${10n**20n}|${null}|${-0}|\\uD800${'x'}`.length);}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/api.ts"),
        "type Options={userAgent:string};export function activate(id:number):string{return `/api/v1/agents/${String(id)}/activate`;}export function agent(defaults:Options,extra:string|undefined):string{return `${defaults.userAgent} ${extra}`;}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v31");
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("unsupported expression"),
        "{}",
        combined(&check)
    );
    project.language_profile("checked-f64-utf16-v32");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v32");
    let expected =
        ["/api/v1/agents/42/activate", "varta/0.5 pilot", "31"].map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .filter(|s| s.starts_with("varta log:"))
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn suspension_profile_v33_declares_its_host_abi_through_both_binaries() {
    use varta_common::abi;
    let project = RegressionProject::new(
        "import {label} from './api.ts';export function main():void{console.log(label(33));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/api.ts"),
        "class Probe{n:number;constructor(n:number){this.n=n;}}export function label(n:number):string{return `v${new Probe(n).n}`;}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v33");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for artifact in ["dist/app.wasm", "dist/app"] {
        let inspect = project.command(&["inspect", artifact, "--json"]);
        assert!(inspect.status.success(), "{}", combined(&inspect));
        let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
        assert_eq!(info["language"]["profile"], "checked-f64-utf16-v33");
        assert_eq!(info["abiVersion"], abi::RESUMABLE_FRAMES_VERSION);
    }
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), ["varta log: v33"]);
    }
    // Downgrading the Wasm and manifest host ABI to v32 leaves a v33 declaration
    // without its ABI; both binaries reject it before any guest output.
    let wasm = project.0.join("dist/app.wasm");
    let mut bytes = fs::read(&wasm).unwrap();
    let mut section = abi::SECTION.as_bytes().to_vec();
    section.extend(abi::RESUMABLE_FRAMES_VERSION.to_le_bytes());
    let at = bytes
        .windows(section.len())
        .position(|window| window == section)
        .unwrap()
        + abi::SECTION.len();
    bytes[at..at + section.len() - abi::SECTION.len()]
        .copy_from_slice(&abi::BIGINT_VALUES_VERSION.to_le_bytes());
    fs::write(&wasm, bytes).unwrap();
    let manifest_path = project.0.join("dist/app.manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["abiVersion"] = abi::BIGINT_VALUES_VERSION.into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    for mut command in [varta(), Command::new(runtime_path())] {
        let output = command
            .args(["run", "dist/app.wasm"])
            .current_dir(&project.0)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{}", combined(&output));
        assert!(logs(&output).is_empty(), "{}", combined(&output));
        assert!(
            combined(&output).contains("matching host ABI"),
            "{}",
            combined(&output)
        );
    }
}

#[test]
fn pilot_profile_v34_runs_dynamic_wide_and_recursive_records_in_dev_and_artifacts() {
    let fields: String = (0..36).map(|i| format!("f{i}?:number;")).collect();
    let project = RegressionProject::new(
        &format!(
            "type JsonValue=null|boolean|number|string|JsonValue[]|{{[key:string]:JsonValue}};\
             interface Wide{{{fields}tags:Record<string,string>}}\
             export function main():void{{\
             const w:Wide={{tags:{{b:'x'}}}};w.f35=35;w.tags['2']='two';delete w.tags.b;\
             const doc=varta.json.parse<JsonValue>('{{\"a\":[1,{{\"b\":true}}]}}');\
             console.log(JSON.stringify(w)+' '+JSON.stringify(doc));}}"
        ),
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v34");
    let expected = [r#"varta log: {"tags":{"2":"two"},"f35":35} {"a":[1,{"b":true}]}"#];
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for mut command in [varta(), Command::new(runtime_path())] {
        let output = command
            .args(["run", "dist/app.wasm"])
            .current_dir(&project.0)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
    // The same source stays rejected under v33 before any guest effect.
    project.language_profile("checked-f64-utf16-v33");
    let old = project.command(&["dev"]);
    assert!(!old.status.success());
    let text = combined(&old);
    assert!(
        text.contains("object field count exceeds limit")
            || text.contains("recursive type declaration"),
        "{text}"
    );
}

#[test]
fn pilot_profile_v34_declares_its_host_and_value_abis_through_both_binaries() {
    use varta_common::{abi, value};
    let project = RegressionProject::new(
        "import {label} from './api.ts';export function main():void{console.log(label(34));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/api.ts"),
        "class Probe{n:number;constructor(n:number){this.n=n;}}export function label(n:number):string{return JSON.stringify({v:new Probe(n).n});}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v34");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for artifact in ["dist/app.wasm", "dist/app"] {
        let inspect = project.command(&["inspect", artifact, "--json"]);
        assert!(inspect.status.success(), "{}", combined(&inspect));
        let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
        assert_eq!(info["language"]["profile"], "checked-f64-utf16-v34");
        assert_eq!(info["abiVersion"], abi::SCHEDULED_FRAMES_VERSION);
        assert_eq!(info["valueAbiVersion"], value::DYNAMIC_VALUE_VERSION);
    }
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), [r#"varta log: {"v":34}"#]);
    }
    let wasm = project.0.join("dist/app.wasm");
    let original = fs::read(&wasm).unwrap();
    // Rewrites one versioned custom section payload in place.
    let downgrade = |name: &str, from: u32, to: u32| {
        let mut bytes = original.clone();
        let mut section = name.as_bytes().to_vec();
        section.extend(from.to_le_bytes());
        let at = bytes
            .windows(section.len())
            .position(|window| window == section)
            .unwrap()
            + name.len();
        bytes[at..at + size_of::<u32>()].copy_from_slice(&to.to_le_bytes());
        bytes
    };
    let rejects = |expected: &str| {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .args(["run", "dist/app.wasm"])
                .current_dir(&project.0)
                .output()
                .unwrap();
            assert!(!output.status.success(), "{}", combined(&output));
            assert!(logs(&output).is_empty(), "{}", combined(&output));
            assert!(
                combined(&output).contains(expected),
                "{}",
                combined(&output)
            );
        }
    };
    // A v34 declaration with the v33 value ABI is rejected before guest output.
    fs::write(
        &wasm,
        downgrade(
            value::VALUE_SECTION,
            value::DYNAMIC_VALUE_VERSION,
            value::BIGINT_VALUE_VERSION,
        ),
    )
    .unwrap();
    rejects("structured value ABI");
    // Downgrading the Wasm and manifest host ABI to v33 leaves a v34 declaration
    // without its ABI; both binaries reject it before any guest output.
    fs::write(
        &wasm,
        downgrade(
            abi::SECTION,
            abi::SCHEDULED_FRAMES_VERSION,
            abi::RESUMABLE_FRAMES_VERSION,
        ),
    )
    .unwrap();
    let manifest_path = project.0.join("dist/app.manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["abiVersion"] = abi::RESUMABLE_FRAMES_VERSION.into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    rejects("matching host ABI");
}

#[test]
fn pilot_profile_v35_declares_value16_and_host_abi_31_through_both_binaries() {
    use varta_common::{abi, value};
    const PROFILE: &str = "checked-f64-utf16-v35";
    // An unregistered successor of the same length: every binary sees it as a
    // v34 binary sees v35.
    const UNKNOWN: &str = "checked-f64-utf16-v38";
    let project = RegressionProject::new(
        "import {label} from './api.ts';export function main():void{console.log(label(35));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/api.ts"),
        "class Probe{n:number;constructor(n:number){this.n=n;}}export function label(n:number):string{const scores:Record<string,number>={v:new Probe(n).n};return JSON.stringify(scores);}",
    )
    .unwrap();
    project.language_profile(PROFILE);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for artifact in ["dist/app.wasm", "dist/app"] {
        let inspect = project.command(&["inspect", artifact, "--json"]);
        assert!(inspect.status.success(), "{}", combined(&inspect));
        let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
        assert_eq!(info["language"]["profile"], PROFILE);
        assert_eq!(info["abiVersion"], abi::PROMISE_VALUES_VERSION);
        assert_eq!(info["valueAbiVersion"], value::PROMISE_VALUE_VERSION);
    }
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), [r#"varta log: {"v":35}"#]);
    }
    let wasm = project.0.join("dist/app.wasm");
    let original = fs::read(&wasm).unwrap();
    let rejects = |expected: &str| {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .args(["run", "dist/app.wasm"])
                .current_dir(&project.0)
                .output()
                .unwrap();
            assert!(!output.status.success(), "{}", combined(&output));
            assert!(logs(&output).is_empty(), "{}", combined(&output));
            assert!(
                combined(&output).contains(expected),
                "{}",
                combined(&output)
            );
        }
    };
    // Renaming the declared profile in the Wasm language section to one the
    // binaries do not list fails before any guest output.
    let mut renamed = original.clone();
    let at = renamed
        .windows(PROFILE.len())
        .position(|window| window == PROFILE.as_bytes())
        .unwrap();
    renamed[at..at + UNKNOWN.len()].copy_from_slice(UNKNOWN.as_bytes());
    assert!(
        !renamed
            .windows(PROFILE.len())
            .any(|window| window == PROFILE.as_bytes())
    );
    fs::write(&wasm, renamed).unwrap();
    rejects("invalid language profile");
}

#[test]
fn pilot_profile_v36_declares_host_abi_32_with_value17_through_both_binaries() {
    use varta_common::{abi, value};
    const PROFILE: &str = "checked-f64-utf16-v36";
    // An unregistered successor of the same length: every binary sees it as a
    // v35 binary sees v36.
    const UNKNOWN: &str = "checked-f64-utf16-v38";
    // The predecessor of the same length, whose host ABI is 31.
    const PREDECESSOR: &str = "checked-f64-utf16-v35";
    let project = RegressionProject::new(
        "import {label} from './api.ts';export function main():void{console.log(label(36));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/api.ts"),
        "class Probe{n:number;constructor(n:number){this.n=n;}}export function label(n:number):string{const scores:Record<string,number>={v:new Probe(n).n};return JSON.stringify(scores);}",
    )
    .unwrap();
    project.language_profile(PROFILE);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for artifact in ["dist/app.wasm", "dist/app"] {
        let inspect = project.command(&["inspect", artifact, "--json"]);
        assert!(inspect.status.success(), "{}", combined(&inspect));
        let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
        assert_eq!(info["language"]["profile"], PROFILE);
        assert_eq!(info["abiVersion"], abi::TIMERS_VERSION);
        assert_eq!(info["valueAbiVersion"], value::FUNCTION_FIELD_VALUE_VERSION);
    }
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), [r#"varta log: {"v":36}"#]);
    }
    let wasm = project.0.join("dist/app.wasm");
    let original = fs::read(&wasm).unwrap();
    let rejects = |expected: &str| {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .args(["run", "dist/app.wasm"])
                .current_dir(&project.0)
                .output()
                .unwrap();
            assert!(!output.status.success(), "{}", combined(&output));
            assert!(logs(&output).is_empty(), "{}", combined(&output));
            assert!(
                combined(&output).contains(expected),
                "{}",
                combined(&output)
            );
        }
    };
    // Renames the declared profile in the Wasm language section in place.
    let rename = |to: &str| {
        let mut renamed = original.clone();
        let at = renamed
            .windows(PROFILE.len())
            .position(|window| window == PROFILE.as_bytes())
            .unwrap();
        renamed[at..at + to.len()].copy_from_slice(to.as_bytes());
        assert!(
            !renamed
                .windows(PROFILE.len())
                .any(|window| window == PROFILE.as_bytes())
        );
        renamed
    };
    // A profile the binaries do not list fails before any guest output.
    fs::write(&wasm, rename(UNKNOWN)).unwrap();
    rejects("invalid language profile");
    // Relabelled as v35, the module keeps host ABI 32 and fails at its host
    // ABI section before any guest output.
    fs::write(&wasm, rename(PREDECESSOR)).unwrap();
    rejects("matching host ABI");
}

#[test]
fn pilot_profile_v37_declares_host_abi_33_with_value17_through_both_binaries() {
    use varta_common::{abi, value};
    const PROFILE: &str = "checked-f64-utf16-v37";
    // An unregistered successor of the same length: every binary sees it as a
    // v36 binary sees v37.
    const UNKNOWN: &str = "checked-f64-utf16-v38";
    // The predecessor of the same length, whose host ABI is 32.
    const PREDECESSOR: &str = "checked-f64-utf16-v36";
    let project = RegressionProject::new(
        "import {label} from './api.ts';export function main():void{console.log(label(37));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/api.ts"),
        "class Probe{n:number;constructor(n:number){this.n=n;}}export function label(n:number):string{const scores:Record<string,number>={v:new Probe(n).n};return JSON.stringify(scores);}",
    )
    .unwrap();
    project.language_profile(PROFILE);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for artifact in ["dist/app.wasm", "dist/app"] {
        let inspect = project.command(&["inspect", artifact, "--json"]);
        assert!(inspect.status.success(), "{}", combined(&inspect));
        let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
        assert_eq!(info["language"]["profile"], PROFILE);
        assert_eq!(info["abiVersion"], abi::NET_REQUEST_VERSION);
        assert_eq!(info["valueAbiVersion"], value::FUNCTION_FIELD_VALUE_VERSION);
    }
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), [r#"varta log: {"v":37}"#]);
    }
    let wasm = project.0.join("dist/app.wasm");
    let original = fs::read(&wasm).unwrap();
    let rejects = |expected: &str| {
        for mut command in [varta(), Command::new(runtime_path())] {
            let output = command
                .args(["run", "dist/app.wasm"])
                .current_dir(&project.0)
                .output()
                .unwrap();
            assert!(!output.status.success(), "{}", combined(&output));
            assert!(logs(&output).is_empty(), "{}", combined(&output));
            assert!(
                combined(&output).contains(expected),
                "{}",
                combined(&output)
            );
        }
    };
    // Renames the declared profile in the Wasm language section in place.
    let rename = |to: &str| {
        let mut renamed = original.clone();
        let at = renamed
            .windows(PROFILE.len())
            .position(|window| window == PROFILE.as_bytes())
            .unwrap();
        renamed[at..at + to.len()].copy_from_slice(to.as_bytes());
        assert!(
            !renamed
                .windows(PROFILE.len())
                .any(|window| window == PROFILE.as_bytes())
        );
        renamed
    };
    // A profile the binaries do not list fails before any guest output.
    fs::write(&wasm, rename(UNKNOWN)).unwrap();
    rejects("invalid language profile");
    // Relabelled as v36, the module keeps host ABI 33 and fails at its host
    // ABI section before any guest output.
    fs::write(&wasm, rename(PREDECESSOR)).unwrap();
    rejects("matching host ABI");
}

/// V36 timers through `varta dev`, both artifacts, both binaries and the
/// packaged executable: the sleep/backoff pattern, an interval, timer
/// arguments and `AbortSignal.timeout` keep Node's order. Arming a timer
/// needs the exact `time.timer` grant: without it, or under a host ceiling
/// that withholds it, the execution fails at the call before any callback.
#[test]
fn pilot_profile_v36_timers_run_everywhere_and_are_denied_without_time_timer() {
    const PROFILE: &str = "checked-f64-utf16-v36";
    let project = RegressionProject::new(
        "function sleep(ms:number):Promise<void>{return new Promise<void>((resolve)=>setTimeout(resolve,ms));}\
         export async function main():Promise<void>{\
         console.log('start');\
         setTimeout((label:string)=>console.log(label),2,'timer arg');\
         const signal=AbortSignal.timeout(1);\
         let ticks=0;let id=0;\
         id=setInterval(()=>{ticks=ticks+1;console.log('tick '+String(ticks));if(ticks===2){clearInterval(id);}},3);\
         for(let attempt=1;attempt<=2;attempt++){await sleep(5*2**attempt);console.log('retry '+String(attempt));}\
         console.log('aborted '+String(signal.aborted));}",
        serde_json::json!({"time":{"timer":true}}),
    );
    project.language_profile(PROFILE);
    let expected = [
        "varta log: start",
        "varta log: timer arg",
        "varta log: tick 1",
        "varta log: tick 2",
        "varta log: retry 1",
        "varta log: retry 2",
        "varta log: aborted true",
    ];
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected, "{}", combined(&dev));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info = String::from_utf8_lossy(&inspect.stdout).into_owned();
    assert!(
        info.contains("timer_start") && info.contains("time.timer"),
        "{info}"
    );
    let validate = project.command(&["validate", "dist/app.wasm"]);
    assert!(validate.status.success(), "{}", combined(&validate));
    // The host ceiling withholds the grant the package requested.
    project.host_policy(serde_json::json!([]));
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let mut restricted = Command::new(command.get_program());
        restricted.args(command.get_args());
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected, "{}", combined(&output));
        let output = restricted
            .args(["--policy", "host-policy.json"])
            .current_dir(&project.0)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{}", combined(&output));
        // A failed execution reports its error, not its captured log.
        assert!(logs(&output).is_empty(), "{}", combined(&output));
        assert!(
            combined(&output).contains("<capability denied: time.timer>"),
            "{}",
            combined(&output)
        );
    }
    // Without the grant, the timer call fails before any callback runs.
    project.set_capabilities(serde_json::json!({"time":{"now":true}}));
    let dev = project.command(&["dev"]);
    assert!(!dev.status.success(), "{}", combined(&dev));
    assert!(logs(&dev).is_empty(), "{}", combined(&dev));
    assert!(
        combined(&dev).contains("<capability denied: time.timer>"),
        "{}",
        combined(&dev)
    );
    // Policy review requires a declaration for the privileged timer import.
    let build = project.command(&["build"]);
    assert!(build.status.success(), "{}", combined(&build));
    let validate = project.command(&["validate", "dist/app.wasm"]);
    assert!(!validate.status.success(), "{}", combined(&validate));
    assert!(
        combined(&validate).contains("requires capability namespace: time.timer"),
        "{}",
        combined(&validate)
    );
}

#[test]
fn pilot_profile_v35_runs_mixed_records_and_json_to_unknown_in_dev_and_artifacts() {
    let project = RegressionProject::new(
        "interface Problem{title?:string;status?:number;[key:string]:unknown}\
         export function main():void{\
         const value:unknown=JSON.parse('{\"title\":\"Gone\",\"status\":410,\"trace\":[1]}');\
         let keys='';\
         if(typeof value==='object'&&value!==null&&'status' in value){for(const k in value){keys=keys+k+';';}}\
         const problem=value as Problem;const counts:Record<string,number>={};\
         for(const k in problem){counts[k]??=0;counts[k]+=k.length;}\
         const all:Record<string,number>={...counts,extra:1};\
         console.log(keys+' '+String(problem.title)+' '+JSON.stringify(all));}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v35");
    let expected =
        [r#"varta log: title;status;trace; Gone {"title":5,"status":6,"trace":5,"extra":1}"#];
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for mut command in [varta(), Command::new(runtime_path())] {
        let output = command
            .args(["run", "dist/app.wasm"])
            .current_dir(&project.0)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
    // The same source stays rejected under v34 before any guest effect.
    project.language_profile("checked-f64-utf16-v34");
    let old = project.command(&["dev"]);
    assert!(!old.status.success());
    let text = combined(&old);
    // V34 has no global `JSON.parse` (and no mixed object types).
    assert!(text.contains("unsupported call"), "{text}");
    assert!(logs(&old).is_empty(), "{text}");
}

#[test]
fn pilot_profile_v36_runs_checked_declared_fields_in_dev_and_artifacts() {
    // A declared array of optional records next to `[key: string]: unknown`:
    // checked parsing converts it, a plain JSON object converts on its first
    // declared read, and both runs agree in dev and in the packaged artifact.
    let project = RegressionProject::new(
        "interface Problem{title?:string;errors?:Array<{field?:string;message?:string}>;[key:string]:unknown}\
         function fields(p:Problem):string{let out='';for(const e of p.errors??[]){\
         if(e!==undefined){out+=String(e.field)+'='+String(e.message)+';';}}return out;}\
         export function main():void{\
         const text='{\"title\":\"Bad\",\"errors\":[{\"field\":\"a\",\"message\":\"m\"},{\"field\":\"b\"}],\"trace\":1}';\
         const parsed=JSON.parse(text) as Problem;\
         const plain=JSON.parse(text) as Record<string,unknown> as Problem;\
         const list=plain.errors;if(list!==undefined){list.push({message:'added'});}\
         console.log(fields(parsed)+' '+fields(plain)+' '+String(plain.errors===list));}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v36");
    let expected = ["varta log: a=m;b=undefined; a=m;b=undefined;undefined=added; true"];
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for mut command in [varta(), Command::new(runtime_path())] {
        let output = command
            .args(["run", "dist/app.wasm"])
            .current_dir(&project.0)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
    // V35 keeps rejecting the declared array before any guest effect.
    project.language_profile("checked-f64-utf16-v35");
    let old = project.command(&["dev"]);
    assert!(!old.status.success());
    let text = combined(&old);
    assert!(text.contains("not checkable from it"), "{text}");
    assert!(logs(&old).is_empty(), "{text}");
}

#[test]
fn pilot_profile_v35_runs_web_types_in_dev_and_source_free_artifacts() {
    // Two modules name Headers, AbortController and DOMException without
    // importing them; no capability is granted or needed.
    let project = RegressionProject::new(
        "import {numericHeader} from './errors.ts';export function main():void{\
         const headers=new Headers({'Retry-After':'30'});headers.append('Set-Cookie','a=1');\
         headers.append('set-cookie','b=2');let names='';for(const [name] of headers){names=names+'['+name!+']';}\
         const controller=new AbortController();controller.abort();\
         try{controller.signal.throwIfAborted();}catch(error){\
         if(error instanceof DOMException)console.log(`${error.name} ${error.code}`);}\
         console.log(`${numericHeader(headers,'retry-after')} ${names}`);}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/errors.ts"),
        "export function numericHeader(headers:Headers,name:string):number|undefined{\
         const value=headers.get(name);if(value===null)return undefined;return Number(value);}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v35");
    let expected = [
        "varta log: AbortError 20",
        "varta log: 30 [retry-after][set-cookie][set-cookie]",
    ];
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    // `check --json` lists the program's modules, not the engine preludes.
    let check = project.command(&["check", "--json"]);
    assert!(check.status.success(), "{}", combined(&check));
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let modules = report["modules"].as_array().unwrap();
    assert_eq!(modules.len(), 2, "{report}");
    assert!(!String::from_utf8_lossy(&check.stdout).contains("<varta>"));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
}

#[test]
fn pilot_profile_v36_runs_function_fields_and_abort_listeners_in_dev_and_artifacts() {
    // A hooks interface with optional function fields, a defaulted
    // `random: () => number = Math.random` parameter, optional chains, abort
    // listeners and `Promise.withResolvers`; no capability is granted.
    let project = RegressionProject::new(
        "import {backoff,retry} from './util.ts';import type {Hooks} from './util.ts';\
         export function main():void{let seen='';\
         const hooks:Hooks={onRetry:(n:number):void=>{seen=seen+String(n);},delay:(n:number):number=>backoff(n,():number=>0.5)};\
         const controller=new AbortController();const order:string[]=[];\
         controller.signal.onabort=():void=>{order.push('onabort');};\
         controller.signal.addEventListener('abort',():void=>{order.push('listener');},{once:true});\
         const r=Promise.withResolvers<string>();r.promise.then((s:string):void=>{console.log(s);});\
         controller.signal.addEventListener('abort',():void=>{r.resolve(`aborted ${String(controller.signal?.aborted)}`);});\
         controller.abort();controller.abort();\
         console.log(`${retry(hooks,3)} ${seen} ${JSON.stringify(order)} ${typeof hooks.delay}`);\
         const jitter=backoff(1);console.log(String(jitter>=50&&jitter<=100));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/util.ts"),
        "export interface Hooks{onRetry?:(n:number)=>void;delay:(n:number)=>number}\
         export function backoff(n:number,random:()=>number=Math.random):number{return Math.round(50*n*(1+random()));}\
         export function retry(hooks:Hooks,attempts:number):number{let total=0;\
         for(let n=1;n<=attempts;n++){hooks.onRetry?.(n);total+=hooks.delay(n);}return total;}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v36");
    let expected = [
        r#"varta log: 450 123 ["onabort","listener"] function"#,
        "varta log: true",
        "varta log: aborted true",
    ];
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
}

#[test]
fn function_fields_stay_rejected_by_v35_projects() {
    let project = RegressionProject::new(
        "interface H{run:(x:number)=>number}\
         export function main():void{const h:H={run:(x:number):number=>x};console.log(h.run(1));}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v35");
    let dev = project.command(&["dev"]);
    assert!(!dev.status.success());
    let text = combined(&dev);
    assert!(text.contains("function-valued record fields"), "{text}");
    assert!(!text.contains("varta log:"), "{text}");
}

#[test]
fn web_types_stay_rejected_by_v34_projects() {
    let project = RegressionProject::new(
        "export function main():void{const h=new Headers();console.log(String(h.has('a')));}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v34");
    let dev = project.command(&["dev"]);
    assert!(!dev.status.success());
    let text = combined(&dev);
    assert!(
        text.contains("unsupported constructor other than an unshadowed supported error class"),
        "{text}"
    );
    assert!(!text.contains("varta log:"), "{text}");
}

#[test]
fn generators_run_through_dev_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {pages} from './pages.ts';export function main():void{let total=0;for(const page of pages(3)){if(page===2){break;}total+=page;}const g=pages(1);const first=g.next();const done=g.return();console.log(`total ${total} ${String(first.value)} ${String(done.done)}`);}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/pages.ts"),
        "export function* pages(n:number):Generator<number>{try{for(let i=0;i<n;i++){yield i;}}finally{console.log(`closed ${n}`);}}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v32");
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("unsupported async, generator, generic or decorated function"),
        "{}",
        combined(&check)
    );
    project.language_profile("checked-f64-utf16-v33");
    let expected = ["closed 3", "closed 1", "total 1 0 true"].map(|s| format!("varta log: {s}"));
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
}

#[test]
fn async_methods_and_promise_values_run_through_dev_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {Api,Pages} from './api.ts';export async function main():Promise<void>{const api:Api=new Pages('svc');let calls=0;const fetchPage=async(n:number):Promise<string>=>{calls++;return api.get('/p',n);};const pages:Promise<string>[]=[fetchPage(1),fetchPage(2)];console.log(pages[0]);const all=await Promise.all(pages);console.log(`${all[0]} ${all[1]} ${calls}`);const first=await Promise.race([api.get('/r'),fetchPage(3)]);console.log(first);}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/api.ts"),
        "export class Api{base:string;constructor(base:string){this.base=base;}async get(path:string,page?:number):Promise<string>{await null;return `${this.base}${path}#${page??1}`;}}export class Pages extends Api{async get(path:string,page?:number):Promise<string>{const raw=await super.get(path,page);return raw+'!';}}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v34");
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("unsupported async class method"),
        "{}",
        combined(&check)
    );
    project.language_profile("checked-f64-utf16-v35");
    // Node 24 prints the same three lines for this program.
    let expected = ["Promise { <pending> }", "svc/p#1! svc/p#2! 2", "svc/r#1!"]
        .map(|s| format!("varta log: {s}"));
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
}

#[test]
fn async_generators_and_for_await_run_through_dev_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {Feed} from './feed.ts';export async function main():Promise<void>{const feed=new Feed('f');const seen:string[]=[];for await(const e of feed.events(5)){seen.push(e);if(seen.length===2)break;}const it=feed.events(1);const first=await it.next();const last=await it.next();console.log(`${JSON.stringify(seen)} ${JSON.stringify(first)} ${JSON.stringify(last)}`);for await(const n of [Promise.resolve(1),2])console.log(String(n));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/feed.ts"),
        "export class Feed{name:string;constructor(name:string){this.name=name;}async *events(count:number):AsyncGenerator<string,number|undefined,undefined>{let sent=0;try{for(let i=0;i<count;i++){await null;sent++;yield `${this.name}:${i}`;}}finally{console.log(`closed ${this.name} after ${sent}`);}return sent;}}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v35");
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("unsupported async generator function"),
        "{}",
        combined(&check)
    );
    project.language_profile("checked-f64-utf16-v36");
    // Node 24 prints the same five lines for this program.
    let expected = [
        "closed f after 2",
        "closed f after 1",
        r#"["f:0","f:1"] {"value":"f:0","done":false} {"value":1,"done":true}"#,
        "1",
        "2",
    ]
    .map(|s| format!("varta log: {s}"));
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
}

#[test]
fn unknown_and_checked_assertions_run_through_dev_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {AppError,read} from './values.ts';export function main():void{try{throw new AppError('m',{cause:{code:3}});}catch(e){if(e instanceof AppError){console.log(`${e.name} ${(e.cause as {code:number}).code}`);}}const values:unknown[]=['a',2];console.log(read(values,0));try{console.log(read(values,1));}catch(e){if(e instanceof TypeError){console.log(e.message);}}}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/values.ts"),
        "export class AppError extends Error{constructor(m:string,o?:{cause?:unknown}){super(m,o);this.name='AppError';}}export function read(values:unknown[],i:number):string{return values[i] as string;}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v33");
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("unsupported type annotation"),
        "{}",
        combined(&check)
    );
    project.language_profile("checked-f64-utf16-v34");
    let expected =
        ["AppError 3", "a", "type assertion to string failed"].map(|s| format!("varta log: {s}"));
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
}

#[test]
fn promises_and_async_functions_run_through_dev_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {fetchWithRetry} from './transport.ts';export async function main():Promise<void>{Promise.resolve(1).then((v)=>console.log(`then ${v}`));const text=await fetchWithRetry(3);console.log(text);try{await fetchWithRetry(1);}catch(e){if(e instanceof Error){console.log(`caught ${e.message}`);}}}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/transport.ts"),
        "async function attempt(n:number):Promise<string>{await null;if(n<3){throw new Error(`attempt ${n}`);}return `ok ${n}`;}export async function fetchWithRetry(limit:number):Promise<string>{let last='none';for(let n=1;n<=limit;n++){try{return await attempt(n);}catch(e){if(e instanceof Error){last=e.message;}}}throw new Error(`gave up after ${last}`);}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v33");
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(
        combined(&check).contains("unsupported async, generator, generic or decorated function"),
        "{}",
        combined(&check)
    );
    project.language_profile("checked-f64-utf16-v34");
    // Node 24 prints the same lines for the same program.
    let expected =
        ["then 1", "ok 3", "caught gave up after attempt 1"].map(|s| format!("varta log: {s}"));
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected);
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected);
    }
}

#[test]
fn unhandled_rejections_fail_dev_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "export async function main():Promise<void>{await null;throw new TypeError('lost');}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v34");
    let dev = project.command(&["dev"]);
    assert!(!dev.status.success());
    assert!(
        combined(&dev).contains("unhandled promise rejection: TypeError: \"lost\""),
        "{}",
        combined(&dev)
    );
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for mut command in [varta(), Command::new(runtime_path())] {
        command.args(["run", "dist/app"]);
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(!output.status.success(), "{}", combined(&output));
        assert!(
            combined(&output).contains("unhandled promise rejection"),
            "{}",
            combined(&output)
        );
    }
}

#[test]
fn primitive_string_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {text} from './string.ts';export function main():void{console.log(text(1e21));console.log(text(true));console.log(text(undefined));console.log(String().length);console.log(String('').length);const s=String('A\\uDFFF\\u0000Z');console.log(s.length);console.log(s.charCodeAt(1));console.log(String(Number.MIN_VALUE));console.log(String(-0));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/string.ts"),
        "export function text(v:number|string|boolean|null|undefined):string{return String(v);}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v19");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 22);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v19");
    let expected = [
        "1e+21",
        "true",
        "undefined",
        "0",
        "0",
        "4",
        "57343",
        "5e-324",
        "0",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
#[test]
fn primitive_string_ignored_arguments_preserve_grants_status_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function read(name:string):number{varta.env.print(name);return 1e21;}function stop():number{varta.process.exit(7);return 1e21;}export function main():void{console.log(String(read('VARTA_STRING_ALLOWED')));console.log(varta.result.status());console.log(String(true,read('VARTA_STRING_DENIED')));console.log(varta.result.status());try{String(1,stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_STRING_ALLOWED"],"get":["VARTA_STRING_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v19");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_STRING_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_STRING_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1e+21\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_STRING_DENIED)\nvarta log: true\nvarta log: 1"
                ),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn primitive_string_rejection_diagnostics_keep_the_dependency_argument_range() {
    let project = RegressionProject::new(
        "import {bad} from './string.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/string.ts");
    project.language_profile("checked-f64-utf16-v19");
    for (source, argument) in [
        ("export function bad():void{String({n:1});}", "{n:1}"),
        ("export function bad():void{String([1]);}", "[1]"),
        (
            "export function bad():void{const n:number|{n:number}=1;String(n);}",
            "n",
        ),
    ] {
        fs::write(&file, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "type_error");
        let start = source.rfind(argument).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + argument.len()
        );
    }
    project.language_profile("checked-f64-utf16-v18");
    fs::write(&file, "export function bad():void{String(1);}").unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(combined(&check).contains("checked-f64-utf16-v19"));
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn primitive_number_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {numeric} from './number.ts';export function main():void{console.log(numeric('0x20000000000001'));console.log(numeric('0b101'));console.log(numeric('0o77'));console.log(numeric(true));console.log(numeric(null));console.log(numeric(undefined));console.log(numeric('invalid'));console.log(1/Number('-0'));console.log(Number('\\uFEFF 42\\u3000'));console.log(String(Number('1e21')));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/number.ts"),"export function numeric(value:number|string|boolean|null|undefined):number{return Number(value);}").unwrap();
    project.language_profile("checked-f64-utf16-v20");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 23);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v20");
    let expected = [
        "9007199254740992",
        "5",
        "63",
        "1",
        "0",
        "NaN",
        "NaN",
        "-Infinity",
        "42",
        "1e+21",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
#[test]
fn primitive_number_ignored_arguments_preserve_grants_status_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function read(name:string):string{varta.env.print(name);return '0x10';}function stop():number{varta.process.exit(7);return 1;}export function main():void{console.log(Number(read('VARTA_NUMBER_ALLOWED')));console.log(varta.result.status());console.log(Number('invalid',read('VARTA_NUMBER_DENIED')));console.log(varta.result.status());try{Number(1,stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_NUMBER_ALLOWED"],"get":["VARTA_NUMBER_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v20");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_NUMBER_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_NUMBER_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 16\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_NUMBER_DENIED)\nvarta log: NaN\nvarta log: 1"
                ),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn primitive_number_rejection_diagnostics_keep_the_dependency_argument_range() {
    let project = RegressionProject::new(
        "import {bad} from './number.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/number.ts");
    project.language_profile("checked-f64-utf16-v20");
    for (source, argument) in [
        ("export function bad():void{Number({n:1});}", "{n:1}"),
        ("export function bad():void{Number([1]);}", "[1]"),
        (
            "export function bad():void{const n:number|{n:number}=1;Number(n);}",
            "n",
        ),
    ] {
        fs::write(&file, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "type_error");
        let start = source.rfind(argument).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + argument.len()
        );
    }
    project.language_profile("checked-f64-utf16-v19");
    fs::write(&file, "export function bad():void{Number(1);}").unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(combined(&check).contains("checked-f64-utf16-v20"));
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn number_radix_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {text} from './radix.ts';export function main():void{console.log(text(255,'16'));console.log(text(35,36));console.log(text(255,undefined));const small=Number.MIN_VALUE.toString(2);console.log(small.length);console.log(small.charAt(small.length-1));const large=Number.MAX_VALUE.toString(16);console.log(Number('0x'+large)===Number.MAX_VALUE);try{NaN.toString(1);}catch(error){console.log(error instanceof RangeError);}console.log((0.1).toString(16));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/radix.ts"),"export function text(value:number,radix:number|string|undefined):string{return value.toString(radix);}").unwrap();
    project.language_profile("checked-f64-utf16-v21");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 24);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v21");
    let expected = ["ff", "z", "255", "1076", "1", "1", "1", "0.1999999999999a"]
        .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
#[test]
fn number_radix_arguments_preserve_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):string{varta.env.print(name);return '16';}function stop():number{varta.process.exit(7);return 16;}export function main():void{console.log((255).toString(read('VARTA_RADIX_ALLOWED')));console.log(varta.result.status());console.log((35).toString(36,read('VARTA_RADIX_DENIED')));console.log(varta.result.status());try{(255).toString(16,stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_RADIX_ALLOWED"],"get":["VARTA_RADIX_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v21");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_RADIX_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_RADIX_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: ff\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_RADIX_DENIED)\nvarta log: z\nvarta log: 1"
                ),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn number_radix_rejection_diagnostics_keep_receiver_and_argument_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './radix.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/radix.ts");
    project.language_profile("checked-f64-utf16-v21");
    for (source, invalid) in [
        (
            "export function bad():void{(255).toString({n:1});}",
            "{n:1}",
        ),
        ("export function bad():void{(255).toString([16]);}", "[16]"),
        (
            "export function bad():void{const value:number|undefined=255;value.toString(16);}",
            "value",
        ),
    ] {
        fs::write(&file, source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "type_error");
        let start = source.rfind(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
    }
    project.language_profile("checked-f64-utf16-v20");
    fs::write(&file, "export function bad():void{(255).toString(16);}").unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(combined(&check).contains("checked-f64-utf16-v21"));
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn number_format_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {texts} from './format.ts';export function main():void{console.log(JSON.stringify(texts(1.25,'2')));console.log(JSON.stringify(texts(1.25,undefined)));console.log((-0.001).toFixed(2));console.log((999).toPrecision(2));console.log(Number.MIN_VALUE.toExponential());try{Infinity.toFixed(101);}catch(error){console.log(error instanceof RangeError);}console.log(Infinity.toExponential(101));console.log(Infinity.toPrecision(0));console.log(1/(-0).valueOf('ignored'));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/format.ts"),"export function texts(n:number,p:number|string|undefined):string[]{return [n.toFixed(p),n.toExponential(p),n.toPrecision(p)];}").unwrap();
    project.language_profile("checked-f64-utf16-v22");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 25);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v22");
    let expected = [
        "[\"1.25\",\"1.25e+0\",\"1.3\"]",
        "[\"1\",\"1.25e+0\",\"1.25\"]",
        "-0.00",
        "1.0e+3",
        "5e-324",
        "1",
        "Infinity",
        "Infinity",
        "-Infinity",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
#[test]
fn number_format_arguments_preserve_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):string{varta.env.print(name);return '2';}function stop():number{varta.process.exit(7);return 2;}export function main():void{console.log((1.25).toFixed(read('VARTA_FORMAT_ALLOWED')));console.log(varta.result.status());console.log((1.25).toExponential(2,read('VARTA_FORMAT_DENIED')));console.log(varta.result.status());try{(1.25).toPrecision(2,stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_FORMAT_ALLOWED"],"get":["VARTA_FORMAT_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v22");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_FORMAT_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_FORMAT_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1.25\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_FORMAT_DENIED)\nvarta log: 1.25e+0\nvarta log: 1"),"{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn number_format_rejection_diagnostics_keep_receiver_and_argument_ranges() {
    use varta_common::number_format::NumberFormat;
    let project = RegressionProject::new(
        "import {bad} from './format.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/format.ts");
    project.language_profile("checked-f64-utf16-v22");
    for &mode in NumberFormat::ALL {
        for (source, invalid) in [
            (
                format!(
                    "export function bad():void{{(1).{}({{n:1}});}}",
                    mode.name()
                ),
                "{n:1}",
            ),
            (
                format!(
                    "export function bad():void{{const value:number|undefined=1;value.{}(2);}}",
                    mode.name()
                ),
                "value",
            ),
        ] {
            fs::write(&file, &source).unwrap();
            let check = project.command(&["check", "--json"]);
            assert_eq!(check.status.code(), Some(1));
            let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
            let diagnostic = &info["diagnostics"][0];
            assert_eq!(
                diagnostic["file"],
                serde_json::json!(file.canonicalize().unwrap())
            );
            assert_eq!(diagnostic["code"], "type_error");
            let start = source.rfind(invalid).unwrap();
            assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
            assert_eq!(
                diagnostic["range"]["end"]["byte_offset"],
                start + invalid.len()
            );
        }
    }
    project.language_profile("checked-f64-utf16-v21");
    fs::write(&file, "export function bad():void{(1).toFixed(2);}").unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(combined(&check).contains("checked-f64-utf16-v22"));
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn number_parse_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {parse} from './parse.ts';export function main():void{console.log(JSON.stringify(parse('0x10tail','16')));console.log(JSON.stringify(parse('1.25e2tail',undefined)));console.log(1/parseInt('-0tail'));console.log(1/Number.parseFloat('-0e999tail'));console.log(parseInt('10',4294967312));console.log(Number.isNaN(parseFloat()));console.log(Number.parseFloat('-Infinity!'));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/parse.ts"),"export function parse(value:string,radix:number|string|undefined):number[]{return [parseInt(value,radix),Number.parseInt(value,radix),parseFloat(value),Number.parseFloat(value)];}").unwrap();
    project.language_profile("checked-f64-utf16-v23");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v23");
    let expected = [
        "[16,16,0,0]",
        "[1,1,125,125]",
        "-Infinity",
        "-Infinity",
        "16",
        "1",
        "-Infinity",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
#[test]
fn number_parse_arguments_preserve_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):string{varta.env.print(name);return '12tail';}function stop():number{varta.process.exit(7);return 10;}export function main():void{console.log(parseInt(read('VARTA_PARSE_ALLOWED'),10));console.log(varta.result.status());console.log(Number.parseFloat('1.5tail',read('VARTA_PARSE_DENIED')));console.log(varta.result.status());try{Number.parseInt('12',10,stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_PARSE_ALLOWED"],"get":["VARTA_PARSE_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v23");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_PARSE_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_PARSE_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 12\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_PARSE_DENIED)\nvarta log: 1.5\nvarta log: 1"
                ),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn number_parse_rejection_diagnostics_preserve_dependency_argument_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './parse.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/parse.ts");
    project.language_profile("checked-f64-utf16-v23");
    for statement in [
        "parseInt({n:1});",
        "Number.parseFloat({n:1});",
        "Number.parseInt('12',{n:1});",
    ] {
        let source = format!("export function bad():void{{{statement}}}");
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "type_error");
        let start = source.find("{n:1}").unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(diagnostic["range"]["end"]["byte_offset"], start + 5);
    }
    project.language_profile("checked-f64-utf16-v22");
    fs::write(&file, "export function bad():void{Number.parseInt('12');}").unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(combined(&check).contains("checked-f64-utf16-v23"));
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn primitive_builtins_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {facts} from './facts.ts';export function main():void{console.log(JSON.stringify(facts('')));console.log(JSON.stringify(facts('12')));console.log(JSON.stringify(facts('bad')));console.log(JSON.stringify(facts(null)));console.log(Boolean({n:0}));console.log(Boolean(():number=>0));console.log(Number.isFinite('12'));console.log(isNaN());}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/facts.ts"),"export function facts(value:number|string|null|undefined):boolean[]{return [Boolean(value),isFinite(value),isNaN(value)];}").unwrap();
    project.language_profile("checked-f64-utf16-v23");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v23");
    let expected = [
        "[false,true,false]",
        "[true,true,false]",
        "[true,false,true]",
        "[false,true,false]",
        "1",
        "1",
        "0",
        "1",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn primitive_builtins_arguments_preserve_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function read(name:string):string{varta.env.print(name);return '12';}function stop():number{varta.process.exit(7);return 10;}export function main():void{console.log(isFinite(read('VARTA_BUILTIN_ALLOWED')));console.log(varta.result.status());console.log(Boolean('x',read('VARTA_BUILTIN_DENIED')));console.log(varta.result.status());try{isNaN(0,stop());}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_BUILTIN_ALLOWED"],"get":["VARTA_BUILTIN_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v23");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_BUILTIN_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_BUILTIN_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: 1\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains(
                    "<capability denied: env> (VARTA_BUILTIN_DENIED)\nvarta log: 1\nvarta log: 1"
                ),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn primitive_builtins_rejection_diagnostics_preserve_dependency_argument_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './facts.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/facts.ts");
    project.language_profile("checked-f64-utf16-v23");
    for (statement, invalid) in [
        ("isFinite({n:1});", "{n:1}"),
        ("isNaN({n:1});", "{n:1}"),
        (
            "Boolean(varta.result.capture(varta.env.get('X')));",
            "varta.result.capture(varta.env.get('X'))",
        ),
    ] {
        let source = format!("export function bad():void{{{statement}}}");
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "type_error");
        let start = source.find(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
    }
    project.language_profile("checked-f64-utf16-v22");
    fs::write(&file, "export function bad():void{Boolean(1);}").unwrap();
    let check = project.command(&["check"]);
    assert!(!check.status.success());
    assert!(combined(&check).contains("checked-f64-utf16-v23"));
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn guest_truth_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {choose} from './choice.ts';export function main():void{console.log(choose(''));console.log(choose('yes'));console.log(choose(undefined));const a={n:1};const b={n:2};console.log((a||b)===a);console.log((a&&b)===b);if({n:0}){console.log('object');}console.log(![1]);console.log(1/(-0&&7));console.log(0??5);}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/choice.ts"),
        "export function choose(value:string|null|undefined):string{return value||'fallback';}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v23");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v23");
    let expected = [
        "fallback",
        "yes",
        "fallback",
        "1",
        "1",
        "object",
        "0",
        "-Infinity",
        "0",
    ]
    .map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn guest_truth_short_circuit_preserves_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "function left(name:string,value:string):string{varta.env.print(name);return value;}function stop():string{varta.process.exit(7);return 'next';}export function main():void{console.log(left('VARTA_TRUTH_ALLOWED','yes')||left('VARTA_TRUTH_SKIPPED','bad'));console.log(varta.result.status());console.log(left('VARTA_TRUTH_DENIED','')||'fallback');console.log(varta.result.status());try{left('VARTA_TRUTH_ALLOWED','yes')&&stop();}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_TRUTH_ALLOWED"],"get":["VARTA_TRUTH_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v23");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_TRUTH_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_TRUTH_DENIED", "NEVER_DISCLOSE")
                .env("VARTA_TRUTH_SKIPPED", "NEVER_DISCLOSE_SKIPPED");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                if restricted { 0 } else { 2 },
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: yes\nvarta log: {}",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(text.contains("<capability denied: env> (VARTA_TRUTH_DENIED)\nvarta log: fallback\nvarta log: 1"),"{text}");
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("VARTA_TRUTH_SKIPPED")
                    && !text.contains("NEVER_DISCLOSE")
                    && !text.contains("BAD_CATCH"),
                "{text}"
            );
        }
    }
}
#[test]
fn guest_truth_rejection_diagnostics_preserve_opaque_operand_source_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './choice.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/choice.ts");
    project.language_profile("checked-f64-utf16-v23");
    let invalid = "varta.result.capture(varta.env.get('X'))";
    for statement in [format!("if({invalid}){{}}"), format!("'x'&&{invalid};")] {
        let source = format!("export function bad():void{{{statement}}}");
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "type_error");
        let start = source.find(invalid).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + invalid.len()
        );
    }
    project.language_profile("checked-f64-utf16-v22");
    fs::write(&file, "export function bad():void{if({n:0}){}} ").unwrap();
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn truth_guards_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {get,fallback} from './guard.ts';export function main():void{console.log(get({n:7}));console.log(get(undefined));console.log(fallback({n:2}));console.log(fallback(undefined));const value:{n:number}|undefined={n:9};if(value){console.log(value.n);}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/guard.ts"),"function missing(value:undefined):{n:number}{return {n:8};}export function get(value:{n:number}|undefined):number|undefined{return value&&value.n;}export function fallback(value:{n:number}|undefined):number{return (value||missing(value)).n;}").unwrap();
    project.language_profile("checked-f64-utf16-v24");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v24");
    let expected = ["7", "undefined", "2", "8", "9"].map(|s| format!("varta log: {s}"));
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn truth_guards_refined_resource_names_preserve_grants_status_host_ceilings_and_exit() {
    let project = RegressionProject::new(
        "function read(name:string|undefined):void{if(name){varta.env.print(name);}}function stop():void{varta.process.exit(7);}export function main():void{read('VARTA_GUARD_ALLOWED');console.log(varta.result.status());read(undefined);read('');read('VARTA_GUARD_DENIED');console.log(varta.result.status());const value:{n:number}|undefined={n:1};try{if(value&&value.n>0){stop();}}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_GUARD_ALLOWED"],"get":["VARTA_GUARD_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v24");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_GUARD_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_GUARD_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!("varta log: {}", u8::from(restricted))),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_GUARD_DENIED)\nvarta log: 1"),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("NEVER_DISCLOSE") && !text.contains("BAD_CATCH"),
                "{text}"
            );
        }
    }
}
#[test]
fn truth_guards_scope_restoration_diagnostics_keep_dependency_read_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './guard.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/guard.ts");
    project.language_profile("checked-f64-utf16-v24");
    for statement in [
        "if(value){console.log(value.n);}console.log(value.n);",
        "value&&value.n;console.log(value.n);",
        "while(value){break;}console.log(value.n);",
    ] {
        let source = format!(
            "export function bad():void{{const value:{{n:number}}|undefined={{n:1}};{statement}}}"
        );
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "unsupported_syntax");
        let start = source.rfind("value.n").unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(diagnostic["range"]["end"]["byte_offset"], start + 5);
    }
    project.language_profile("checked-f64-utf16-v23");
    fs::write(
        &file,
        "export function bad():void{const value:{n:number}|undefined={n:1};value&&value.n;}",
    )
    .unwrap();
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}
#[test]
fn mutable_flow_captured_names_and_guarded_writes_preserve_permissions_status_and_exit() {
    let project = RegressionProject::new(
        "export function main():void{let name:string|undefined='VARTA_FLOW_ALLOWED';const clear=():void=>{name=undefined;};if(name){varta.env.print(name);console.log(varta.result.status());clear();if(name){varta.env.print(name);console.log('BAD_STALE');}else{console.log('cleared');}}name='VARTA_FLOW_DENIED';if(name){varta.env.print(name);}console.log(varta.result.status());try{if(name){name=undefined;varta.process.exit(7);}}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_FLOW_ALLOWED"],"get":["VARTA_FLOW_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v24");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    for restricted in [false, true] {
        let mut dev = varta();
        dev.arg("dev");
        let mut wasm = varta();
        wasm.args(["run", "dist/app.wasm"]);
        let mut runtime = Command::new(runtime_path());
        runtime.args(["run", "dist/app.wasm"]);
        for mut command in [dev, wasm, runtime, Command::new(project.0.join("dist/app"))] {
            command
                .current_dir(&project.0)
                .env("VARTA_FLOW_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_FLOW_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: {}\nvarta log: cleared",
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_FLOW_DENIED)\nvarta log: 1"),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_STALE")
                    && !text.contains("BAD_CATCH")
                    && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn mutable_flow_stale_read_diagnostics_keep_dependency_receiver_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './flow.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/flow.ts");
    project.language_profile("checked-f64-utf16-v24");
    for statement in [
        "if(value){value=undefined;console.log(value.n);}",
        "if(value){if(true){value=undefined;}console.log(value.n);}",
        "const clear=():void=>{value=undefined;};if(value){clear();console.log(value.n);}",
        "if(value){while(true){console.log(value.n);value=undefined;}}",
    ] {
        let source = format!(
            "export function bad():void{{let value:{{n:number}}|undefined={{n:1}};{statement}}}"
        );
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        assert_eq!(diagnostic["code"], "unsupported_syntax");
        let start = source.rfind("value.n").unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(diagnostic["range"]["end"]["byte_offset"], start + 5);
    }
    project.language_profile("checked-f64-utf16-v23");
    fs::write(&file,"export function bad():void{let value:number|string=1;if(typeof value==='number'){value='text';}}").unwrap();
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    assert!(!project.0.join("dist/app").exists());
}

#[test]
fn mutable_flow_runs_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {update,factory} from './flow.ts';export function main():void{console.log(update());const read=factory();console.log(read());let value:{n:number}|undefined={n:1};while(value){console.log(value.n);value=undefined;}console.log(Boolean(value));}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/flow.ts"),"export function update():number{let value:{n:number}|undefined={n:7};const clear=():void=>{value=undefined;};if(value){const n=value.n;clear();if(value){return value.n;}return n;}return 0;}export function factory():()=>number{const value:{n:number}|undefined={n:9};if(value){return ():number=>value.n;}return ():number=>0;}").unwrap();
    project.language_profile("checked-f64-utf16-v24");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v24");
    let expected = ["7", "9", "1", "0"].map(|s| format!("varta log: {s}"));

    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(expected.len())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn post_write_joins_run_across_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {choose,read} from './flow.ts';export function main():void{console.log(choose(true));console.log(choose(false));console.log(read(undefined));console.log(read({n:9}));let text:string|undefined=undefined;text??='abc';console.log(text.length);}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/flow.ts"), "export function choose(flag:boolean):number{let value:{n:number}|undefined=undefined;if(flag){value={n:7};}else{value={n:8};}return value.n;}export function read(value:{n:number}|undefined):number{if(!value)return 0;return value.n;}").unwrap();
    project.language_profile("checked-f64-utf16-v24");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v24");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            vec![
                "varta log: 7",
                "varta log: 8",
                "varta log: 0",
                "varta log: 9",
                "varta log: 3"
            ]
        );
    }
}

#[test]
fn post_write_skipped_store_diagnostics_keep_dependency_receiver_ranges() {
    let project = RegressionProject::new(
        "import {bad} from './flow.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v24");
    let file = project.0.join("src/flow.ts");
    for flow in [
        "if(flag){value='s';}",
        "flag?(value='s'):0;",
        "flag&&(value='s');",
        "out:{if(flag)break out;value='s';}",
        "try{if(flag)throw 'stop';value='s';}catch(e){}",
    ] {
        let source = format!(
            "export function bad():void{{const flag=true;let value:string|number=0;{flow}console.log(value.length);}}"
        );
        fs::write(&file, &source).unwrap();
        let output = project.command(&["check", "--json"]);
        assert_eq!(output.status.code(), Some(1), "{}", combined(&output));
        let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        let start = source.rfind("value.length").unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + "value".len()
        );
    }
}

#[test]
fn post_write_source_admission_keeps_earlier_profile_contracts() {
    let project = RegressionProject::new(
        "export function main():void{let value:string|number=0;value='abc';console.log(value.length);}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v23");
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    project.language_profile("checked-f64-utf16-v24");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    assert!(project.0.join("dist/app.wasm").exists());
    assert!(project.0.join("dist/app").exists());
}

#[test]
fn exit_flow_runs_modules_and_source_free_artifacts_with_shared_abi() {
    let project = RegressionProject::new(
        "import {choose} from './exit.ts';export function main():void{console.log(choose(true));console.log(choose(false));let value:string|number=0;for(let i=0;i<2;console.log(value.length)){i++;if(i===1){value='a';continue;}value='bb';}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/exit.ts"), "export function choose(flag:boolean):number{let value:{n:number}|undefined=undefined;while(true){try{if(flag){value={n:7};break;}value={n:8};break;}finally{Math.abs(-1);}}return value.n;}").unwrap();
    project.language_profile("checked-f64-utf16-v24");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v24");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            vec![
                "varta log: 7",
                "varta log: 8",
                "varta log: 1",
                "varta log: 2"
            ]
        );
    }
}

#[test]
fn exit_flow_unsafe_backedge_and_finalizer_reads_keep_receiver_diagnostics() {
    let project = RegressionProject::new(
        "import {bad} from './exit.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v24");
    let file = project.0.join("src/exit.ts");
    for statement in [
        "for(value='a';value.length>0;value=1){console.log('body');}",
        "while(true){try{value='a';break;}finally{value=2;}}console.log(value.length);",
        "for(let i=0;i<2;console.log(value.length)){i++;try{value='a';continue;}finally{value=2;}}",
    ] {
        let source = format!("export function bad():void{{let value:string|number=0;{statement}}}");
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1), "{}", combined(&check));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        let start = source.rfind("value.length").unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + "value".len()
        );
    }
}

#[test]
fn exit_flow_earlier_profile_admission_remains_unchanged() {
    let project = RegressionProject::new(
        "export function main():void{let value:string|number=0;while(true){value='abc';break;}console.log(value.length);}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v23");
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    project.language_profile("checked-f64-utf16-v24");
    let output = project.command(&["check"]);
    assert!(output.status.success(), "{}", combined(&output));
}

#[test]
fn call_flow_runs_modules_and_source_free_artifacts_without_new_abi() {
    let project = RegressionProject::new(
        "import {add,invoke} from './call.ts';export function main():void{let value:{n:number}|undefined={n:7};let other:{n:number}|undefined={n:8};const read=():boolean=>Boolean(value);const clear=():void=>{other=undefined;};if(value){console.log(add(1));invoke(clear);console.log(value.n);console.log(read());console.log(value.n);console.log(Boolean(other));}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/call.ts"), "export function add(n:number):number{return n+1;}export function invoke(fn:()=>void):void{fn();}").unwrap();
    project.language_profile("checked-f64-utf16-v24");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v24");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            vec![
                "varta log: 2",
                "varta log: 7",
                "varta log: 1",
                "varta log: 7",
                "varta log: 0"
            ]
        );
    }
}

#[test]
fn call_flow_transitive_writes_keep_dependency_receiver_diagnostics() {
    let project = RegressionProject::new(
        "import {bad} from './call.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v24");
    let file = project.0.join("src/call.ts");
    for call in [
        "clear();",
        "relay();",
        "invoke(clear);",
        "let fn:()=>void=()=>{};fn=clear;fn();",
    ] {
        let source = format!(
            "function invoke(fn:()=>void):void{{fn();}}export function bad():void{{let value:{{n:number}}|undefined={{n:7}};const clear=():void=>{{value=undefined;}};const relay=():void=>clear();if(value){{{call}console.log(value.n);}}}}"
        );
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1), "{}", combined(&check));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        let start = source.rfind("value.n").unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + "value".len()
        );
    }
}

#[test]
fn call_flow_new_source_admission_preserves_earlier_profiles() {
    let project = RegressionProject::new(
        "export function main():void{let value:{n:number}|undefined={n:7};const read=():boolean=>Boolean(value);if(value){read();console.log(value.n);}}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v23");
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    project.language_profile("checked-f64-utf16-v24");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
}

#[test]
fn parameter_flow_runs_modules_and_source_free_artifacts_with_existing_abi() {
    let project = RegressionProject::new(
        "import {run} from './parameter.ts';export function main():void{console.log(run());let value:{n:number}|undefined={n:9};const read=():boolean=>Boolean(value);const invoke=(fn:()=>boolean):boolean=>fn();if(value&&invoke(read)){console.log(value.n);}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/parameter.ts"), "export function run():number{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const read=():void=>{};const invoke=(fn:()=>void,n:number=0):void=>{const copy=fn;const nested=():void=>copy();nested();};if(value){invoke(read);return value.n;}return 0;}").unwrap();
    project.language_profile("checked-f64-utf16-v24");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v24");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(2)
                .collect::<Vec<_>>(),
            vec!["varta log: 7", "varta log: 9"]
        );
    }
}

#[test]
fn parameter_flow_mutated_inputs_keep_dependency_receiver_diagnostics() {
    let project = RegressionProject::new(
        "import {bad} from './parameter.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v24");
    let file = project.0.join("src/parameter.ts");
    for statement in [
        "const invoke=(fn:()=>void):void=>fn();invoke(clear);",
        "const invoke=(fn:()=>void):void=>{fn=clear;fn();};invoke(():void=>{});",
        "const invoke=(fn:()=>void):void=>{const change=():void=>{fn=clear;};change();fn();};invoke(():void=>{});",
        "const read=():void=>{};const invoke=(ignored:undefined,fn:()=>void):void=>fn();invoke(clear(),read);",
    ] {
        let source = format!(
            "export function bad():void{{let value:{{n:number}}|undefined={{n:7}};const clear=():void=>{{value=undefined;}};if(value){{{statement}console.log(value.n);}}}}"
        );
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1), "{}", combined(&check));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        let start = source.rfind("value.n").unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start);
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + "value".len()
        );
    }
}

#[test]
fn parameter_flow_specialization_remains_opt_in_for_source_admission() {
    let project = RegressionProject::new(
        "export function main():void{let value:{n:number}|undefined={n:7};const observe=():boolean=>Boolean(value);const invoke=(fn:()=>void):void=>fn();if(value){invoke(():void=>{});console.log(value.n);}}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v23");
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    project.language_profile("checked-f64-utf16-v24");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
}

#[test]
fn predicate_search_runs_modules_and_source_free_artifacts_with_existing_abi() {
    let project = RegressionProject::new(
        "import {search} from './search.ts';export function main():void{search();}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/search.ts"),"export function search():void{const a=[1,2,3,2];console.log(a.findIndex(v=>v===2));console.log(a.findLastIndex(v=>v===2));console.log(a.findLast(v=>v===2));const holes:number[]=[];holes.length=3;console.log(holes.findLastIndex(v=>v===undefined));console.log(holes.findLast(()=>true));}").unwrap();
    project.language_profile("checked-f64-utf16-v25");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v25");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(5)
                .collect::<Vec<_>>(),
            [
                "varta log: 1",
                "varta log: 3",
                "varta log: 2",
                "varta log: 2",
                "varta log: undefined"
            ]
        );
    }
}

#[test]
fn predicate_search_diagnostics_keep_dependency_locations_and_profile_gate() {
    let project = RegressionProject::new(
        "import {bad} from './search.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/search.ts");
    project.language_profile("checked-f64-utf16-v25");
    for (expression, highlight) in [
        ("a.findIndex((v:number):boolean=>true)", "v:number"),
        ("a.findLastIndex()", "a.findLastIndex()"),
        ("a.findLast(1)", "1"),
    ] {
        let source =
            format!("export function bad():void{{const a=[1,2];console.log({expression});}}");
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1), "{}", combined(&check));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        assert_eq!(
            info["diagnostics"][0]["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        let start = source.find(expression).unwrap() + expression.find(highlight).unwrap();
        assert_eq!(
            info["diagnostics"][0]["range"]["start"]["byte_offset"], start,
            "{info}"
        );
        assert_eq!(
            info["diagnostics"][0]["range"]["end"]["byte_offset"],
            start + highlight.len(),
            "{info}"
        );
    }
    fs::write(
        &file,
        "export function bad():void{console.log([1,2].findLastIndex(v=>v===2));}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v24");
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    project.language_profile("checked-f64-utf16-v25");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
}

#[test]
fn predicate_search_callbacks_preserve_grants_host_ceilings_status_and_exit() {
    let project = RegressionProject::new(
        "export function main():void{const a=[1,2];console.log(a.findLastIndex(()=>{varta.env.print('VARTA_SEARCH_ALLOWED');console.log(varta.result.status());return true;}));a.findIndex(()=>{varta.env.print('VARTA_SEARCH_DENIED');console.log(varta.result.status());return true;});try{a.findLast(()=>{varta.process.exit(7);return true;});}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_SEARCH_ALLOWED"],"get":["VARTA_SEARCH_DENIED"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v25");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for restricted in [false, true] {
        let mut commands = Vec::new();
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for mut command in commands {
            command
                .current_dir(&project.0)
                .env("VARTA_SEARCH_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_SEARCH_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_SEARCH_DENIED)\nvarta log: 1"),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}

#[test]
fn reduce_right_runs_modules_and_source_free_artifacts_with_existing_abi() {
    let project = RegressionProject::new(
        "import {fold} from './fold.ts';export function main():void{fold();}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/fold.ts"),"export function fold():void{const a=[1,2,3];console.log(a.reduceRight((s,v)=>s-v));console.log(a.reduceRight((s,v)=>s-v,10));console.log([1,,3].reduceRight((s,v)=>s+v));const empty:number[]=[];try{empty.reduceRight((s,v)=>s+v);}catch(error){console.log(error instanceof TypeError);}}").unwrap();
    project.language_profile("checked-f64-utf16-v26");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v26");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(4)
                .collect::<Vec<_>>(),
            [
                "varta log: 0",
                "varta log: 4",
                "varta log: 4",
                "varta log: 1"
            ]
        );
    }
}
#[test]
fn reduce_right_diagnostics_preserve_dependency_spans_and_profile_admission() {
    let project = RegressionProject::new(
        "import {bad} from './fold.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    let file = project.0.join("src/fold.ts");
    project.language_profile("checked-f64-utf16-v26");
    for (expression, highlight) in [
        ("a.reduceRight()", "a.reduceRight()"),
        ("a.reduceRight(1)", "1"),
        ("a.reduceRight((s:string,v:number):string=>s)", "s:string"),
        (
            "a.reduceRight((s:number,v:number):number=>s+v,'bad')",
            "'bad'",
        ),
    ] {
        let source =
            format!("export function bad():void{{const a=[1,2];console.log({expression});}}");
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1), "{}", combined(&check));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        let start = source.find(expression).unwrap() + expression.find(highlight).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start, "{info}");
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + highlight.len(),
            "{info}"
        );
    }
    fs::write(
        &file,
        "export function bad():void{console.log([1,2].reduceRight((s,v)=>s+v));}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v25");
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    project.language_profile("checked-f64-utf16-v26");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
}
#[test]
fn reduce_right_callbacks_preserve_operation_grants_host_ceilings_status_and_exit() {
    let project = RegressionProject::new(
        "export function main():void{console.log([1].reduceRight((s,v)=>{varta.env.print('VARTA_FOLD_ALLOWED');console.log(varta.result.status());return s+v;},0,varta.result.capture(varta.env.get('VARTA_FOLD_DENIED'))));console.log(varta.result.status());try{[1].reduceRight((s,v)=>{varta.process.exit(7);return s+v;},0);}catch(error){console.log('BAD_CATCH');}finally{console.log('continued');}}",
        serde_json::json!({"env":{"print":["VARTA_FOLD_ALLOWED"],"get":["VARTA_FOLD_OTHER"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v26");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for restricted in [false, true] {
        let mut commands = Vec::new();
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for mut command in commands {
            command
                .current_dir(&project.0)
                .env("VARTA_FOLD_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_FOLD_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_FOLD_DENIED)"),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: {}\nvarta log: 1\nvarta log: {}",
                    u8::from(restricted),
                    u8::from(restricted)
                )),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}

#[test]
fn array_inference_runs_modules_and_source_free_artifacts_with_shared_abi() {
    let project = RegressionProject::new(
        "import {make} from './infer.ts';export function main():void{console.log(JSON.stringify(make()));console.log(JSON.stringify([[],[2]]));const record={n:7};const values=[record,undefined];const first=values[0];if(first){first.n=9;}console.log(record.n);}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/infer.ts"),"export function make():(number|string|undefined)[]{const values=[1,'x',undefined];values.push(2);return values;}").unwrap();
    project.language_profile("checked-f64-utf16-v27");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 26);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v27");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .take(3)
                .collect::<Vec<_>>(),
            [
                "varta log: [1,\"x\",null,2]",
                "varta log: [[],[2]]",
                "varta log: 9"
            ]
        );
    }
}
#[test]
fn array_inference_diagnostics_keep_conflicting_dependency_spans_and_profile_gates() {
    let project = RegressionProject::new(
        "import {bad} from './infer.ts';export function main():void{bad();}",
        serde_json::json!({}),
    );
    project.language_profile("checked-f64-utf16-v27");
    let file = project.0.join("src/infer.ts");
    for (body, highlight) in [
        ("const values=[{n:1},{n:'bad'}];", "{n:'bad'}"),
        ("const values=[[1],['x']];", "'x'"),
        ("const values=[1,'x'];values.push(true);", "true"),
        (
            "const values=[1,undefined];const numbers:number[]=values;",
            "values",
        ),
    ] {
        let source = format!("export function bad():void{{{body}}}");
        fs::write(&file, &source).unwrap();
        let check = project.command(&["check", "--json"]);
        assert_eq!(check.status.code(), Some(1), "{}", combined(&check));
        let info: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
        let diagnostic = &info["diagnostics"][0];
        assert_eq!(
            diagnostic["file"],
            serde_json::json!(file.canonicalize().unwrap())
        );
        let start = source.rfind(highlight).unwrap();
        assert_eq!(diagnostic["range"]["start"]["byte_offset"], start, "{info}");
        assert_eq!(
            diagnostic["range"]["end"]["byte_offset"],
            start + highlight.len(),
            "{info}"
        );
    }
    fs::write(
        &file,
        "export function bad():void{console.log(JSON.stringify([1,undefined]));}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v26");
    assert!(!project.command(&["check"]).status.success());
    assert!(!project.command(&["build", "--single"]).status.success());
    assert!(!project.0.join("dist/app.wasm").exists());
    project.language_profile("checked-f64-utf16-v27");
    let check = project.command(&["check"]);
    assert!(check.status.success(), "{}", combined(&check));
}
#[test]
fn array_inference_initializers_preserve_grants_status_host_ceilings_and_fatal_exit() {
    let project = RegressionProject::new(
        "export function main():void{const values=[1,varta.env.get('VARTA_INFER_ALLOWED'),undefined];console.log(varta.result.status());console.log(JSON.stringify(values));varta.env.print('VARTA_INFER_DENIED');console.log(varta.result.status());try{const exit=[1,varta.process.exit(7),'after'];console.log('continued');}catch(error){console.log('BAD_CATCH');}finally{console.log('finally');}}",
        serde_json::json!({"env":{"get":["VARTA_INFER_ALLOWED"],"print":["VARTA_INFER_OTHER"]},"process":{"exit":true}}),
    );
    project.language_profile("checked-f64-utf16-v27");
    project.host_policy(serde_json::json!([]));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for restricted in [false, true] {
        let mut commands = Vec::new();
        for artifact in ["dist/app.wasm", "dist/app"] {
            for mut command in [varta(), Command::new(runtime_path())] {
                command.args(["run", artifact]);
                commands.push(command);
            }
        }
        commands.push(Command::new(project.0.join("dist/app")));
        for mut command in commands {
            command
                .current_dir(&project.0)
                .env("VARTA_INFER_ALLOWED", "ALLOWED_VALUE")
                .env("VARTA_INFER_DENIED", "NEVER_DISCLOSE");
            if restricted {
                command.args(["--policy", "host-policy.json"]);
            }
            let output = command.output().unwrap();
            let text = combined(&output);
            assert_eq!(
                output.status.code(),
                Some(if restricted { 0 } else { 7 }),
                "{text}"
            );
            assert_eq!(
                text.matches("ALLOWED_VALUE").count(),
                usize::from(!restricted),
                "{text}"
            );
            assert!(
                text.contains(&format!(
                    "varta log: {}\nvarta log: [1,\"{}\",null]",
                    u8::from(restricted),
                    if restricted { "" } else { "ALLOWED_VALUE" }
                )),
                "{text}"
            );
            assert!(
                text.contains("<capability denied: env> (VARTA_INFER_DENIED)\nvarta log: 1"),
                "{text}"
            );
            assert_eq!(text.contains("varta log: continued"), restricted, "{text}");
            assert_eq!(text.contains("varta log: finally"), restricted, "{text}");
            assert!(
                !text.contains("BAD_CATCH") && !text.contains("NEVER_DISCLOSE"),
                "{text}"
            );
        }
    }
}
#[test]
fn bigint_runs_modules_and_source_free_artifacts_with_abi27() {
    let project = RegressionProject::new(
        "import {add} from './integer.ts';export function main():void{console.log(add(9007199254740993n,2n));try{console.log(1n/0n);}catch(e){console.log(e instanceof RangeError);}console.log(BigInt.asIntN(8,255n));}",
        serde_json::json!({}),
    );
    fs::write(
        project.0.join("src/integer.ts"),
        "export function add(a:bigint,b:bigint):bigint{return a+b;}",
    )
    .unwrap();
    project.language_profile("checked-f64-utf16-v28");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 27);
    assert_eq!(info["valueAbiVersion"], 12);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v28");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .filter(|line| line.starts_with("varta log:"))
                .collect::<Vec<_>>(),
            [
                "varta log: 9007199254740995",
                "varta log: 1",
                "varta log: -1"
            ]
        );
    }
}
#[test]
fn bigint_values_run_modules_and_source_free_artifacts_with_v13_metadata() {
    let project = RegressionProject::new(
        "import {make,fail,post,multiply,ordinary,unary,equal,less} from './values.ts';export function main():void{const values=make();console.log(values[0]+values[1]);console.log(values[0]+=2n);console.log(values[1]++);console.log(++values[1]);console.log(BigInt.asIntN('8','255'));console.log((255n).toString('16'));console.log(post('2'));console.log(multiply('2'));let text='x';console.log(text+=1n);console.log(ordinary('2'));console.log(equal(1n));console.log(less(1));console.log(less(1n));console.log(1==='1');console.log(unary('2'));try{console.log(unary(1n));}catch(e){console.log(e instanceof TypeError);}const numbers:number[]=[];console.log(numbers[0]++);try{console.log(JSON.stringify(values));}catch(e){console.log(e instanceof TypeError);}try{fail();}catch(e){if(typeof e==='bigint'){console.log(e+1n);}}}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/values.ts"),"export function make():bigint[]{return [9007199254740993n,2n];}export function fail():void{throw 7n;}export function post(n:number|string):number{return n++;}export function multiply(n:number|string):number{return n*='3';}export function ordinary(n:number|string):number{return n*'3'+true;}export function unary(n:number|string|bigint):number{return +n;}export function equal(n:number|string|bigint):boolean{return n=='1';}export function less(n:number|string|bigint):boolean{return n<'1.5';}").unwrap();
    project.language_profile("checked-f64-utf16-v29");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 28);
    assert_eq!(info["valueAbiVersion"], 13);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v29");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .filter(|s| s.starts_with("varta log:"))
                .collect::<Vec<_>>(),
            [
                "varta log: 9007199254740995",
                "varta log: 9007199254740995",
                "varta log: 2",
                "varta log: 4",
                "varta log: -1",
                "varta log: ff",
                "varta log: 2",
                "varta log: 6",
                "varta log: x1",
                "varta log: 7",
                "varta log: 1",
                "varta log: 1",
                "varta log: 0",
                "varta log: 0",
                "varta log: 2",
                "varta log: 1",
                "varta log: NaN",
                "varta log: 1",
                "varta log: 8"
            ]
        );
    }
}

#[test]
fn property_deletion_v30_runs_modules_and_source_free_artifacts() {
    let project = RegressionProject::new(
        "import {make,clear,text,reader} from './values.ts';export function main():void{const r=make();console.log(clear(r,'n'));console.log(r.n);console.log(clear(r,'big'));r.n=7;console.log(JSON.stringify(r));console.log(clear(r,'s'));console.log(JSON.stringify(r));console.log(JSON.stringify(Object.entries(r)));console.log(JSON.stringify(Object.entries(text())));console.log(Object.is(r,r));console.log(Object.is(NaN,NaN));console.log(Object.is(-0,0));console.log(Object.is(Object.assign(r,{n:8}),r));console.log(JSON.stringify(Object.assign({},{n:9,s:text()})));const read=reader(5);console.log(read());}",
        serde_json::json!({}),
    );
    fs::write(project.0.join("src/values.ts"),"export function make():{n?:number;s?:string;big?:bigint|undefined}{return {n:1,s:'x',big:3n};}export function clear(v:{n?:number;s?:string;big?:bigint|undefined},key:string):boolean{return delete v[key];}export function text():string{return '😀';}export function reader(n:number):()=>number{return ()=>n;}").unwrap();
    project.language_profile("checked-f64-utf16-v30");
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(info["abiVersion"], 28);
    assert_eq!(info["valueAbiVersion"], 13);
    assert_eq!(info["language"]["profile"], "checked-f64-utf16-v30");
    let mut dev = varta();
    dev.arg("dev");
    let mut commands = vec![dev];
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for (index, mut command) in commands.into_iter().enumerate() {
        if index == 1 {
            fs::remove_dir_all(project.0.join("src")).unwrap();
            fs::remove_file(project.0.join("varta.config.json")).unwrap();
        }
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .filter(|s| s.starts_with("varta log:"))
                .collect::<Vec<_>>(),
            [
                "varta log: 1",
                "varta log: undefined",
                "varta log: 1",
                "varta log: {\"s\":\"x\",\"n\":7}",
                "varta log: 1",
                "varta log: {\"n\":7}",
                "varta log: [[\"n\",7]]",
                "varta log: [[\"0\",\"\\ud83d\"],[\"1\",\"\\ude00\"]]",
                "varta log: 1",
                "varta log: 1",
                "varta log: 0",
                "varta log: 1",
                "varta log: {\"n\":9,\"s\":\"😀\"}",
                "varta log: 5"
            ]
        );
    }
}

/// V37 collections through `varta dev`, both artifacts, both binaries and the
/// packaged executable: a module-constant `Set`, a `Map` with tuple entries,
/// string built-ins, UTF-8 bytes, base64, random bytes and `Date` parsing.
/// `Date.now()` needs the exact `time.now` grant: without it, or under a host
/// ceiling that withholds it, the execution fails before any output.
#[test]
fn pilot_profile_v37_collections_run_everywhere_and_the_clock_needs_time_now() {
    const PROFILE: &str = "checked-f64-utf16-v37";
    let project = RegressionProject::new(
        "const RETRYABLE=new Set([408,429,503]);\
         export function main():void{\
         const cache=new Map<string,[number,string]>();\
         cache.set('a',[1,'one']).set('b',[2,'two']);\
         const keys:string[]=[];for(const [key,[n,word]] of cache){keys.push(key+String(n)+word);}\
         const bytes=crypto.getRandomValues(new Uint8Array(4));\
         const hex=[...new TextEncoder().encode('é')].map((b)=>b.toString(16).padStart(2,'0')).join('');\
         console.log(`${RETRYABLE.has(429)} ${RETRYABLE.has(200)} ${keys.join(',')} ${hex} ${bytes.length}`);\
         console.log(`${btoa('hi')} ${'A,B'.split(',').map((s)=>s.toLowerCase()).join('')} ${Date.parse('Sun, 06 Nov 1994 08:49:37 GMT')}`);\
         console.log(String(Date.now()>0));}",
        serde_json::json!({"time":{"now":true}}),
    );
    project.language_profile(PROFILE);
    let expected = [
        "varta log: true false a1one,b2two c3a9 4",
        "varta log: aGk= ab 784111777000",
        "varta log: true",
    ];
    let logs = |output: &std::process::Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|s| s.starts_with("varta log:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let dev = project.command(&["dev"]);
    assert!(dev.status.success(), "{}", combined(&dev));
    assert_eq!(logs(&dev), expected, "{}", combined(&dev));
    let build = project.command(&["build", "--single"]);
    assert!(build.status.success(), "{}", combined(&build));
    let inspect = project.command(&["inspect", "dist/app.wasm", "--json"]);
    assert!(inspect.status.success(), "{}", combined(&inspect));
    let info = String::from_utf8_lossy(&inspect.stdout).into_owned();
    assert!(
        info.contains("date_now") && info.contains("time.now"),
        "{info}"
    );
    // The host ceiling withholds the grant the package requested.
    project.host_policy(serde_json::json!([]));
    let mut commands = Vec::new();
    for artifact in ["dist/app.wasm", "dist/app"] {
        for mut command in [varta(), Command::new(runtime_path())] {
            command.args(["run", artifact]);
            commands.push(command);
        }
    }
    commands.push(Command::new(project.0.join("dist/app")));
    for mut command in commands {
        let mut restricted = Command::new(command.get_program());
        restricted.args(command.get_args());
        let output = command.current_dir(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(logs(&output), expected, "{}", combined(&output));
        let output = restricted
            .args(["--policy", "host-policy.json"])
            .current_dir(&project.0)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{}", combined(&output));
        assert!(logs(&output).is_empty(), "{}", combined(&output));
        assert!(
            combined(&output).contains("capability denied: time.now"),
            "{}",
            combined(&output)
        );
    }
    // Without the grant, `Date.now()` fails before any later output.
    project.set_capabilities(serde_json::json!({}));
    let dev = project.command(&["dev"]);
    assert!(!dev.status.success(), "{}", combined(&dev));
    assert!(logs(&dev).is_empty(), "{}", combined(&dev));
    assert!(
        combined(&dev).contains("capability denied: time.now"),
        "{}",
        combined(&dev)
    );
}
