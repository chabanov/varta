//! Distribution selection must preserve original bytecode and fail before writes.
use super::*;

fn project(source: &str) -> RegressionProject {
    let project = RegressionProject::new(source, serde_json::json!({}));
    project.configure(
        "languageProfile",
        serde_json::json!("checked-f64-utf16-v38"),
    );
    project
}

#[test]
fn browser_build_preserves_portable_artifact_and_refuses_overwrite() {
    let project = project("export function main():void{console.log(\"Привіт 😀\");}");
    let built = project.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    let output = project.0.join("dist/web");
    let files = [
        "app.wasm",
        "app.private.wasm",
        "app.manifest.json",
        "varta-worker.js",
        "varta-host.js",
        "index.html",
    ];
    let bytes: Vec<_> = files
        .iter()
        .map(|name| fs::read(output.join(name)).unwrap())
        .collect();
    assert_eq!(fs::read_dir(&output).unwrap().count(), files.len());
    let native = project.command(&["build", "--target", "wasm"]);
    assert!(native.status.success(), "{}", combined(&native));
    assert_eq!(bytes[0], fs::read(project.0.join("dist/app.wasm")).unwrap());
    assert_eq!(
        bytes[2],
        fs::read(project.0.join("dist/app.manifest.json")).unwrap()
    );
    let original = project.command(&["validate", "dist/web/app.wasm"]);
    assert!(original.status.success(), "{}", combined(&original));
    let private = project.command(&["validate", "dist/web/app.private.wasm"]);
    assert!(!private.status.success());
    let repeated = project.command(&["build", "--target", "browser"]);
    assert!(!repeated.status.success());
    for (name, old) in files.iter().zip(bytes) {
        assert_eq!(old, fs::read(output.join(name)).unwrap());
    }
}

#[test]
fn browser_gaps_and_source_failures_leave_no_distribution_directory() {
    for source in [
        "export function main():void{console.log(varta.fs.read(\"/data/test.txt\"));}",
        "export function main():void{const value:any=1;console.log(value);}",
    ] {
        let project = project(source);
        let built = project.command(&["build", "--target", "browser"]);
        assert!(!built.status.success(), "{source}");
        assert!(!project.0.join("dist").exists(), "{}", combined(&built));
    }
}

#[test]
fn browser_build_does_not_mount_native_execution_roots() {
    let project = project("export function main():void{console.log(1);}");
    project.configure(
        "fs",
        serde_json::json!({"roots":{"project":"missing-native-root"}}),
    );
    let built = project.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    assert!(project.0.join("dist/web/index.html").is_file());
    assert!(!project.0.join("missing-native-root").exists());
}

#[test]
fn browser_build_includes_the_async_guest_checkpoint() {
    let project = project(
        "async function task():Promise<void>{await 1;console.log(2);}export function main():void{task();}",
    );
    let built = project.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    let worker = fs::read_to_string(project.0.join("dist/web/varta-worker.js")).unwrap();
    assert!(worker.contains("scheduler.drain(instance, checkException)"));
    assert!(worker.contains("promise_unhandled"));
}

#[test]
fn target_flag_conflicts_and_host_policy_are_rejected_before_output() {
    let project = project("export function main():void{}");
    for args in [
        vec!["build", "--target", "browser", "--single"],
        vec!["build", "--target", "browser", "--launcher", "missing"],
        vec!["build", "--target", "wasm", "--launcher", "missing"],
        vec!["build", "--target", "browser", "--policy", "missing"],
    ] {
        let result = project.command(&args);
        assert!(!result.status.success(), "{}", combined(&result));
        assert!(!project.0.join("dist").exists());
    }
    let help = project.command(&["build", "--help"]);
    assert!(help.status.success());
    let text = combined(&help);
    assert!(
        text.contains("browser") && text.contains("never overwritten"),
        "{text}"
    );
}

#[test]
fn native_target_uses_the_existing_verified_launcher_path() {
    let project = project("export function main():void{console.log(7);}");
    let output = varta()
        .current_dir(&project.0)
        .args(["build", "--target", "native", "--launcher"])
        .arg(runtime_path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    let run = Command::new(project.0.join("dist/app")).output().unwrap();
    assert!(run.status.success(), "{}", combined(&run));
    assert!(combined(&run).contains("varta log: 7"));
}

#[test]
fn browser_build_includes_timers_with_denial_before_arming() {
    let project = project("export function main():void{setTimeout(()=>{console.log(2);},1);}");
    let built = project.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    let worker = fs::read_to_string(project.0.join("dist/web/varta-worker.js")).unwrap();
    assert!(worker.contains("function createTimers("));
    let denied = project.command(&["run", "dist/web/app.wasm"]);
    assert!(!denied.status.success(), "{}", combined(&denied));
    assert!(
        combined(&denied).contains("time.timer"),
        "{}",
        combined(&denied)
    );
}

#[test]
fn browser_build_includes_network_without_implicitly_granting_it() {
    let project = project(
        "export async function main():Promise<void>{await fetch(\"http://127.0.0.1:1/not-authorized\");}",
    );
    let built = project.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    let worker = fs::read_to_string(project.0.join("dist/web/varta-worker.js")).unwrap();
    assert!(worker.contains("function createNetwork("));
    let denied = project.command(&["run", "dist/web/app.wasm"]);
    assert!(!denied.status.success(), "{}", combined(&denied));
    assert!(
        combined(&denied).contains("capability denied: net.request"),
        "{}",
        combined(&denied)
    );
}

#[test]
fn browser_build_includes_checked_view_sessions_and_keeps_bounded_native_execution() {
    let project = project(
        "export function main():void{const root=View.node();View.createElement(root,1);const text=View.node();View.createText(text,\"Привіт 😀\");View.insert(root,text,0);View.mount(root);console.log(1);}",
    );
    let built = project.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    let worker = fs::read_to_string(project.0.join("dist/web/varta-worker.js")).unwrap();
    let loader = fs::read_to_string(project.0.join("dist/web/varta-host.js")).unwrap();
    let page = fs::read_to_string(project.0.join("dist/web/index.html")).unwrap();
    assert!(worker.contains("function createWorkerView("));
    assert!(loader.contains("export function mount("));
    assert!(page.contains("data-varta-bootstrap"));
    let native = project.command(&["run", "dist/web/app.wasm"]);
    assert!(native.status.success(), "{}", combined(&native));
    assert!(combined(&native).contains("varta log: 1"));
}

#[test]
fn browser_resource_inputs_are_pinned_and_invalid_paths_fail_before_output() {
    let png = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let valid = project("export function main():void{console.log(1);}");
    fs::create_dir(valid.0.join("images")).unwrap();
    fs::write(valid.0.join("images/red.png"), png).unwrap();
    valid.configure(
        "browser",
        serde_json::json!({"resources":{"icons/red.png":"images/red.png"}}),
    );
    let built = valid.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    assert_eq!(
        fs::read(valid.0.join("dist/web/varta-resource-0.png")).unwrap(),
        png
    );
    let legacy = project("export function main():void{}");
    legacy.configure(
        "languageProfile",
        serde_json::json!("checked-f64-utf16-v37"),
    );
    legacy.configure(
        "browser",
        serde_json::json!({"resources":{"red.png":"red.png"}}),
    );
    let built = legacy.command(&["build", "--target", "browser"]);
    assert!(!built.status.success());
    assert!(combined(&built).contains("require v38"));
    assert!(!legacy.0.join("dist").exists());
    for path in [
        "../outside.png",
        "/tmp/outside.png",
        "bad%2epng",
        "missing.png",
    ] {
        let invalid = project("export function main():void{}");
        invalid.configure("browser", serde_json::json!({"resources":{"red.png":path}}));
        let built = invalid.command(&["build", "--target", "browser"]);
        assert!(!built.status.success(), "{path}");
        assert!(!invalid.0.join("dist").exists());
    }
}

#[test]
fn packaged_images_reach_the_resource_section_and_the_browser_bundle() {
    let png = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let packaged = project("export function main():void{console.log(1);}");
    fs::create_dir(packaged.0.join("images")).unwrap();
    fs::write(packaged.0.join("images/red.png"), png).unwrap();
    packaged.configure(
        "images",
        serde_json::json!({"icons/red.png":"images/red.png"}),
    );
    // The portable build writes the resource section beside the Wasm, and
    // the runtime validates it with the artifact.
    let built = packaged.command(&["build"]);
    assert!(built.status.success(), "{}", combined(&built));
    let section = fs::read(packaged.0.join("dist/app.resources")).unwrap();
    assert!(section.windows(png.len()).any(|window| window == png));
    let native = packaged.command(&["run", "dist/app.wasm"]);
    assert!(native.status.success(), "{}", combined(&native));
    // The browser bundle carries the same image under the same name.
    let built = packaged.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    assert_eq!(
        fs::read(packaged.0.join("dist/web/varta-resource-0.png")).unwrap(),
        png
    );
    // `browser.resources` beside `images` would be a browser-only image.
    packaged.configure(
        "browser",
        serde_json::json!({"resources":{"other.png":"images/red.png"}}),
    );
    let refused = packaged.command(&["build", "--target", "browser"]);
    assert!(!refused.status.success());
    assert!(
        combined(&refused).contains("images and browser.resources"),
        "{}",
        combined(&refused)
    );
    // A corrupt image is refused when the project is packaged.
    let corrupt = project("export function main():void{}");
    fs::create_dir(corrupt.0.join("images")).unwrap();
    let mut bytes = png.to_vec();
    bytes[43] ^= 1;
    fs::write(corrupt.0.join("images/a.png"), bytes).unwrap();
    corrupt.configure("images", serde_json::json!({"a.png":"images/a.png"}));
    let refused = corrupt.command(&["build"]);
    assert!(!refused.status.success());
    assert!(
        combined(&refused).contains("invalid image data"),
        "{}",
        combined(&refused)
    );
    assert!(!corrupt.0.join("dist").exists());
}

#[cfg(unix)]
#[test]
fn browser_resource_loader_rejects_leaf_and_directory_symlinks() {
    use std::os::unix::fs::symlink;
    let project = project("export function main():void{}");
    fs::create_dir(project.0.join("images")).unwrap();
    fs::write(
        project.0.join("images/picture.png"),
        b"not important: must not follow links",
    )
    .unwrap();
    symlink("images/picture.png", project.0.join("leaf.png")).unwrap();
    symlink("images", project.0.join("linked")).unwrap();
    for (fonts, path) in [
        (false, "leaf.png"),
        (false, "linked/picture.png"),
        (true, "leaf.png"),
        (true, "linked/picture.png"),
    ] {
        project.configure(
            "browser",
            if fonts {
                serde_json::json!({"fonts":{"Brand":path}})
            } else {
                serde_json::json!({"resources":{"picture.png":path}})
            },
        );
        let built = project.command(&["build", "--target", "browser"]);
        assert!(!built.status.success(), "{}", combined(&built));
        assert!(
            !combined(&built).contains("invalid or oversized PNG")
                && !combined(&built).contains("invalid or oversized browser font"),
            "must reject traversal before parsing file bytes"
        );
        assert!(!project.0.join("dist").exists());
    }
}

#[test]
fn browser_fonts_use_verified_project_bytes_and_validate_family_names() {
    let font =
        include_bytes!("../../../varta-project/tests/fixtures/browser-fonts/advance-400.ttf");
    let valid = project("export function main():void{console.log(1);}");
    fs::create_dir(valid.0.join("fonts")).unwrap();
    fs::write(valid.0.join("fonts/brand.data"), font).unwrap();
    valid.configure(
        "browser",
        serde_json::json!({"fonts":{"Brand":"fonts/brand.data"}}),
    );
    let built = valid.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    assert_eq!(
        fs::read(valid.0.join("dist/web/varta-resource-0.ttf")).unwrap(),
        font
    );
    let loader = fs::read_to_string(valid.0.join("dist/web/varta-host.js")).unwrap();
    assert!(loader.contains("font/ttf"));
    assert!(loader.contains("new FontFace("));
    for fonts in [
        serde_json::json!({"serif":"font.ttf"}),
        serde_json::json!({"Brand":"../font.ttf"}),
        serde_json::json!({"Brand":"a.ttf","brand":"b.ttf"}),
    ] {
        let invalid = project("export function main():void{}");
        invalid.configure("browser", serde_json::json!({"fonts":fonts}));
        let built = invalid.command(&["build", "--target", "browser"]);
        assert!(!built.status.success());
        assert!(!invalid.0.join("dist").exists());
    }
}

#[test]
fn browser_permission_sources_build_without_embedding_host_decisions() {
    let source = "export async function main():Promise<void>{console.log((await varta.permissions.query({name:\"time.timer\"})).state);console.log((await varta.permissions.request({name:\"time.timer\"})).state);}";
    let valid = project(source);
    let built = valid.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    let native = valid.command(&["run", "dist/web/app.wasm"]);
    assert!(native.status.success(), "{}", combined(&native));
    assert_eq!(combined(&native).matches("varta log: denied").count(), 2);
    let loader = fs::read_to_string(valid.0.join("dist/web/varta-host.js")).unwrap();
    assert!(loader.contains("snapshotDecisionFile"));
    assert!(loader.contains("onAudit"));
    let worker = fs::read_to_string(valid.0.join("dist/web/varta-worker.js")).unwrap();
    assert!(worker.contains("permission_query:"));
    assert!(worker.contains("event_watch("));
    let invalid = project(source);
    invalid.configure("fs", serde_json::json!({"roots":{"project":"."}}));
    invalid.configure(
        "capabilities",
        serde_json::json!({"fs":{"read":["/project/**"]}}),
    );
    let built = invalid.command(&["build", "--target", "browser"]);
    assert!(!built.status.success());
    assert!(
        combined(&built).contains("permission resource scopes not implemented"),
        "{}",
        combined(&built)
    );
    assert!(!invalid.0.join("dist").exists());
    let flag = project(source);
    let built = flag.command(&[
        "build",
        "--target",
        "browser",
        "--decisions",
        "not-embedded.json",
    ]);
    assert!(!built.status.success());
    assert!(!flag.0.join("dist").exists());
}

/// The browser host offers no KV storage: a manifest that grants `kv.read`
/// or `kv.write` is refused by name before any output, while the same KV
/// program without a grant builds and is denied at `kv_open`, as natively.
#[test]
fn browser_build_ships_the_kv_codec_for_granted_and_ungranted_kv_programs() {
    // Browser KV storage exists (OPFS with the shared codec): a manifest that
    // grants KV builds, and a KV program without a grant still builds and is
    // denied at open, as natively.
    let source = "export async function main():Promise<void>{try{const kv=await varta.kv.open(\"/data/app.kv\");kv.close();}catch(error){if(error instanceof Error){console.log(error.name);}}}";
    let pattern = serde_json::json!(["/data/*.kv"]);
    for capabilities in [
        serde_json::json!({"kv":{"read":pattern,"write":pattern}}),
        serde_json::json!({"kv":{"read":pattern}}),
    ] {
        let granted = project(source);
        granted.configure("fs", serde_json::json!({"roots":{"data":"data"}}));
        granted.configure("capabilities", capabilities);
        let built = granted.command(&["build", "--target", "browser"]);
        assert!(built.status.success(), "{}", combined(&built));
        assert!(granted.0.join("dist/web/varta-worker.js").is_file());
        assert!(granted.0.join("dist/web/varta-kv-codec.wasm").is_file());
    }
    let ungranted = project(source);
    let built = ungranted.command(&["build", "--target", "browser"]);
    assert!(built.status.success(), "{}", combined(&built));
    assert!(ungranted.0.join("dist/web/varta-worker.js").is_file());
    let native = ungranted.command(&["run", "dist/web/app.wasm"]);
    assert!(native.status.success(), "{}", combined(&native));
    assert!(
        combined(&native).contains("varta log: NotAllowedError"),
        "{}",
        combined(&native)
    );
}
