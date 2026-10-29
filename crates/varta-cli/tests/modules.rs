use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Project {
    workspace: PathBuf,
    root: PathBuf,
}
impl Project {
    fn new(main: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!(
            "varta-modules-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = workspace.join("app");
        fs::create_dir_all(root.join("src")).unwrap();
        let project = Self { workspace, root };
        project.write("varta.config.json",r#"{"entry":"src/main.ts","target":"native","policy":"strict","out":"dist/app","capabilities":{}}"#);
        project.write("src/main.ts", main);
        project
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_varta"))
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
    fn canonical(&self, path: &str) -> PathBuf {
        self.root.join(path).canonicalize().unwrap()
    }
    fn diagnostic(&self) -> serde_json::Value {
        let output = self.command(&["check", "--json"]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["diagnostics"].as_array().unwrap().len(), 1, "{json}");
        json["diagnostics"][0].clone()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.workspace);
    }
}
fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
fn success(output: Output) -> String {
    assert!(output.status.success(), "{}", text(&output));
    text(&output)
}

#[test]
fn named_imports_aliases_private_scopes_and_shared_dependencies_work_in_every_target() {
    let project = Project::new(
        r#"import {advance as left} from "./left.ts";
import {step as right} from "./right.ts";
function helper(n:number):number{return n+1;}
export function main():void {console.log(helper(left(20)+right(10)));}"#,
    );
    project.write(
        "src/shared.ts",
        "export function identity(n:number):number{return n;}",
    );
    project.write("src/left.ts",r#"import {identity as id} from "./shared.ts"; function helper(n:number):number{return n+1;} function step(n:number):number{return id(helper(n));} export {step as advance};"#);
    project.write("src/right.ts",r#"import {identity} from "./nested/../shared.ts"; function helper(n:number):number{return n*2;} export function step(n:number):number{return identity(helper(n));}"#);
    let check = project.command(&["check", "--json"]);
    assert!(check.status.success(), "{}", text(&check));
    let json: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let modules = json["modules"].as_array().unwrap();
    assert_eq!(modules.len(), 4);
    assert_eq!(
        modules[0]["path"],
        serde_json::json!(project.canonical("src/shared.ts"))
    );
    assert_eq!(
        modules.last().unwrap()["path"],
        serde_json::json!(project.canonical("src/main.ts"))
    );
    assert!(success(project.command(&["dev"])).contains("varta log: 42"));
    success(project.command(&["build", "--single"]));
    success(project.command(&["validate", "dist/app.wasm"]));
    success(project.command(&["validate", "dist/app"]));
    // Artifacts contain linked code, not references to source files on the host.
    fs::remove_dir_all(project.root.join("src")).unwrap();
    fs::remove_file(project.root.join("varta.config.json")).unwrap();
    assert!(success(project.command(&["run", "dist/app.wasm"])).contains("varta log: 42"));
    let native = Command::new(project.root.join("dist/app"))
        .current_dir(&project.root)
        .output()
        .unwrap();
    assert!(success(native).contains("varta log: 42"));
}

#[test]
fn import_declaration_order_does_not_change_linked_bytecode() {
    let project = Project::new(
        "import {a} from './a.ts'; import {b} from './b.ts'; export function main():void{console.log(a()+b());}",
    );
    project.write("src/a.ts", "export function a():number{return 20;}");
    project.write("src/b.ts", "export function b():number{return 22;}");
    success(project.command(&["build"]));
    let first = fs::read(project.root.join("dist/app.wasm")).unwrap();
    project.write("src/main.ts","import {b} from './b.ts'; import {a} from './a.ts'; export function main():void{console.log(a()+b());}");
    success(project.command(&["build"]));
    assert_eq!(fs::read(project.root.join("dist/app.wasm")).unwrap(), first);
}

#[test]
fn module_type_errors_point_to_the_correct_source_and_call_argument() {
    let project = Project::new(
        "import {ready} from './lib.ts'; export function main():void {console.log(ready(true));}",
    );
    project.write(
        "src/lib.ts",
        "export function ready(n:number):boolean{return n>0;}",
    );
    let error = project.diagnostic();
    assert_eq!(error["code"], "type_error");
    assert_eq!(
        error["file"],
        serde_json::json!(project.canonical("src/main.ts"))
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("expected number, found boolean")
    );
    let source = fs::read_to_string(project.root.join("src/main.ts")).unwrap();
    let start = error["range"]["start"]["byte_offset"].as_u64().unwrap() as usize;
    let end = error["range"]["end"]["byte_offset"].as_u64().unwrap() as usize;
    assert_eq!(&source[start..end], "true");
    project.write(
        "src/main.ts",
        "import {ready} from './lib.ts'; export function main():void {console.log(ready(1));}",
    );
    project.write(
        "src/lib.ts",
        "export function ready(n:number):boolean {\n return missing;\n}",
    );
    let error = project.diagnostic();
    assert_eq!(
        error["file"],
        serde_json::json!(project.canonical("src/lib.ts"))
    );
    assert_eq!(error["range"]["start"]["line"], 2);
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("unknown identifier: missing")
    );
    project.write(
        "src/lib.ts",
        "export function ready(n:number):boolean { return n>0; }",
    );
    assert!(success(project.command(&["dev"])).contains("varta log: 1"));
}

#[test]
fn missing_private_duplicate_and_reserved_bindings_fail_before_artifact_creation() {
    let project = Project::new(
        "import {hidden} from './lib.ts'; export function main():void {console.log(hidden());}",
    );
    project.write(
        "src/lib.ts",
        "function hidden():number{return 7;} export function visible():number{return hidden();}",
    );
    let error = project.diagnostic();
    assert_eq!(error["code"], "module_error");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("does not export function hidden")
    );
    for source in [
        "import {visible as f} from './lib.ts'; function f():number{return 1;} export function main():void{}",
        "import {visible as varta} from './lib.ts'; export function main():void{}",
        "import {visible as f, visible as f} from './lib.ts'; export function main():void{}",
    ] {
        project.write("src/main.ts", source);
        for command in ["check", "build", "dev"] {
            let result = project.command(&[command]);
            assert_eq!(result.status.code(), Some(1), "{}", text(&result));
        }
    }
    assert!(!project.root.join("dist").exists());
}

#[test]
fn missing_modules_and_cycles_have_import_site_diagnostics() {
    let project = Project::new(
        "import {a} from './missing.ts'; export function main():void {console.log(a());}",
    );
    let error = project.diagnostic();
    assert_eq!(error["code"], "module_error");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("cannot load module")
    );
    assert_eq!(
        error["file"],
        serde_json::json!(project.canonical("src/main.ts"))
    );
    project.write(
        "src/main.ts",
        "import {a} from './a.ts'; export function main():void {console.log(a());}",
    );
    project.write(
        "src/a.ts",
        "import {b} from './b.ts'; export function a():number{return b();}",
    );
    project.write(
        "src/b.ts",
        "import {a} from './a.ts'; export function b():number{return a();}",
    );
    let error = project.diagnostic();
    assert_eq!(error["code"], "module_error");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("cyclic module dependency")
    );
    assert_eq!(
        error["file"],
        serde_json::json!(project.canonical("src/b.ts"))
    );
    project.write(
        "src/main.ts",
        "import {main as again} from './main.ts'; export function main():void {again();}",
    );
    assert!(
        project.diagnostic()["message"]
            .as_str()
            .unwrap()
            .contains("cyclic module dependency")
    );
}

#[test]
fn unsupported_module_forms_are_explicit_and_never_silently_ignored() {
    for source in [
        "import value from './lib.ts'; export function main():void{}",
        "import * as lib from './lib.ts'; export function main():void{}",
        "import './lib.ts'; export function main():void{}",
        "export * from './lib.ts'; export function main():void{}",
        "export {f} from './lib.ts'; export function main():void{}",
        "import type {T} from './lib.ts'; export function main():void{}",
        "export function main():void{import('./lib.ts');}",
    ] {
        let project = Project::new(source);
        let error = project.diagnostic();
        assert_eq!(error["code"], "unsupported_syntax", "{error}");
        assert!(!project.root.join("dist").exists());
    }
}

#[test]
fn dependency_paths_cannot_escape_the_configured_project_directory() {
    let project = Project::new("export function main():void{}");
    fs::write(
        project.workspace.join("outside.ts"),
        "PRIVATE_SOURCE_MUST_NOT_BE_READ",
    )
    .unwrap();
    for specifier in [
        "../../outside.ts",
        "/outside.ts",
        "node:fs",
        "https://example.com/file.ts",
        "./lib",
        "./lib.js",
        "./lib.ts?x=1",
        "./lib.ts#fragment",
        "..\\outside.ts",
    ] {
        project.write(
            "src/main.ts",
            &format!(
                "import {{f}} from {:?}; export function main():void{{console.log(f());}}",
                specifier
            ),
        );
        let error = project.diagnostic();
        assert_eq!(error["code"], "module_error", "{error}");
        assert!(
            !error
                .to_string()
                .contains("PRIVATE_SOURCE_MUST_NOT_BE_READ")
        );
    }
    // Parent segments inside the boundary are valid and identify one module.
    project.write("math.ts", "export function f():number{return 42;}");
    project.write(
        "src/main.ts",
        "import {f} from '../math.ts'; export function main():void{console.log(f());}",
    );
    assert!(success(project.command(&["dev"])).contains("varta log: 42"));
}

#[test]
#[cfg(unix)]
fn dependency_symlinks_are_rejected_at_every_component() {
    use std::os::unix::fs::symlink;
    let project = Project::new("export function main():void{}");
    fs::create_dir(project.workspace.join("outside")).unwrap();
    fs::write(
        project.workspace.join("outside/private.ts"),
        "PRIVATE_SOURCE_MUST_NOT_BE_READ",
    )
    .unwrap();
    symlink(
        project.workspace.join("outside/private.ts"),
        project.root.join("src/alias.ts"),
    )
    .unwrap();
    symlink(
        project.workspace.join("outside"),
        project.root.join("src/directory"),
    )
    .unwrap();
    project.write("src/real.ts", "export function f():number{return 42;}");
    symlink(Path::new("real.ts"), project.root.join("src/internal.ts")).unwrap();
    for specifier in ["./alias.ts", "./directory/private.ts", "./internal.ts"] {
        project.write(
            "src/main.ts",
            &format!(
                "import {{f}} from {:?}; export function main():void{{console.log(f());}}",
                specifier
            ),
        );
        let error = project.diagnostic();
        assert_eq!(error["code"], "module_error");
        assert!(
            !error
                .to_string()
                .contains("PRIVATE_SOURCE_MUST_NOT_BE_READ")
        );
    }
}

#[test]
fn named_types_and_js_specifiers_link_typescript_sources_in_v31() {
    let main = r#"import type {Tag} from "./types.js";
import {label} from "./lib.js";
export function main():void{const t:Tag={name:"pilot",weight:2};console.log(label(t));}"#;
    let project = Project::new(main);
    project.write(
        "varta.config.json",
        r#"{"entry":"src/main.ts","target":"native","out":"dist/app","languageProfile":"checked-f64-utf16-v31","capabilities":{}}"#,
    );
    project.write(
        "src/types.ts",
        "export interface Tag{name:string;weight:number}",
    );
    project.write(
        "src/lib.ts",
        r#"import type {Tag} from "./types.js";export function label(t:Tag):string{return t.name+"/"+String(t.weight);}"#,
    );
    // A `.js` specifier names the TypeScript source; JavaScript files are never read.
    project.write("src/lib.js", "PRIVATE_JS_MUST_NOT_BE_READ");
    assert!(success(project.command(&["dev"])).contains("varta log: pilot/2"));
    success(project.command(&["build", "--single"]));
    fs::remove_dir_all(project.root.join("src")).unwrap();
    assert!(success(project.command(&["run", "dist/app.wasm"])).contains("varta log: pilot/2"));

    // Earlier profiles keep the explicit `.ts` rule and reject type-only imports.
    let old = Project::new(main);
    old.write(
        "varta.config.json",
        r#"{"entry":"src/main.ts","target":"wasm","out":"dist/app","languageProfile":"checked-f64-utf16-v30","capabilities":{}}"#,
    );
    let diagnostic = old.diagnostic();
    assert_eq!(diagnostic["code"], "unsupported_syntax", "{diagnostic}");
    let unresolved =
        Project::new(r#"import {label} from "./lib.js";export function main():void{}"#);
    unresolved.write(
        "varta.config.json",
        r#"{"entry":"src/main.ts","target":"wasm","out":"dist/app","languageProfile":"checked-f64-utf16-v30","capabilities":{}}"#,
    );
    let diagnostic = unresolved.diagnostic();
    assert_eq!(diagnostic["code"], "module_error", "{diagnostic}");
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains(".ts paths"),
        "{diagnostic}"
    );
}

#[test]
fn module_constants_link_and_run_from_packaged_artifacts_in_v32() {
    let main = r#"import {SDK_VERSION,SCOPES} from "./meta.js";
import {describe} from "./lib.js";
export function main():void{console.log(SDK_VERSION);console.log(describe());console.log(SCOPES.length);}"#;
    let project = Project::new(main);
    project.write(
        "varta.config.json",
        r#"{"entry":"src/main.ts","target":"native","out":"dist/app","languageProfile":"checked-f64-utf16-v32","capabilities":{}}"#,
    );
    project.write(
        "src/meta.ts",
        "export const SDK_VERSION='0.12.0';export const SCOPES=['admin','agents:read'] as const;",
    );
    project.write(
        "src/lib.ts",
        r#"import {SCOPES} from "./meta.js";const SEPARATOR=", ";export function describe():string{return String(SCOPES[0])+SEPARATOR+String(SCOPES[1]);}"#,
    );
    let dev = success(project.command(&["dev"]));
    assert!(dev.contains("varta log: 0.12.0"), "{dev}");
    assert!(dev.contains("varta log: admin, agents:read"), "{dev}");
    success(project.command(&["build", "--single"]));
    fs::remove_dir_all(project.root.join("src")).unwrap();
    let packaged = success(project.command(&["run", "dist/app.wasm"]));
    assert!(
        packaged.contains("varta log: admin, agents:read"),
        "{packaged}"
    );
    assert!(packaged.contains("varta log: 2"), "{packaged}");

    // Earlier profiles keep rejecting top-level constants with a located diagnostic.
    let old = Project::new("const LIMIT=3;export function main():void{console.log(LIMIT);}");
    old.write(
        "varta.config.json",
        r#"{"entry":"src/main.ts","target":"wasm","out":"dist/app","languageProfile":"checked-f64-utf16-v31","capabilities":{}}"#,
    );
    let diagnostic = old.diagnostic();
    assert_eq!(diagnostic["code"], "unsupported_syntax", "{diagnostic}");
}
