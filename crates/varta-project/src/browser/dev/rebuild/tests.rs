use super::*;
use std::sync::atomic::AtomicU64;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "varta-rebuild-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let project = Self(root);
        project.put("main.ts", "export function main():void{console.log(1);}");
        project.config();
        project
    }
    fn put(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).unwrap();
    }
    fn config(&self) {
        self.put("varta.config.json", r#"{"entry":"main.ts","target":"wasm","out":"dist/app","languageProfile":"checked-f64-utf16-v38"}"#);
    }
    fn builder(&self) -> (Rebuilder, Revision) {
        Rebuilder::new(
            &self.0.join("varta.config.json"),
            None,
            None,
            BrowserDevOptions::default(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn rebuild_retries_changed_inputs_and_recovers_from_source_and_config_errors() {
    let project = Project::new();
    let (builder, initial) = project.builder();
    let (builder, result) = builder.tick();
    assert!(result.is_none());
    project.put("unrelated.txt", "ignored");
    let (builder, result) = builder.tick();
    assert!(result.is_none());
    project.put("main.ts", "export function main():void{console.log(2);}");
    let (builder, result) = builder.tick();
    assert_ne!(initial.id, result.unwrap().unwrap().id);
    project.put("main.ts", "not valid TypeScript");
    let (builder, result) = builder.tick();
    assert!(result.unwrap().is_err());
    let (builder, result) = builder.tick();
    assert!(result.is_none(), "do not recompile unchanged invalid input");
    project.put("varta.config.json", "{");
    let (builder, result) = builder.tick();
    assert!(result.unwrap().is_err());
    let (builder, result) = builder.tick();
    assert!(result.is_none());
    project.config();
    project.put("main.ts", "export function main():void{console.log(1);}");
    let (builder, result) = builder.tick();
    assert_eq!(result.unwrap().unwrap().id, initial.id);
    assert!(builder.tick().1.is_none());
    assert!(!project.0.join("dist").exists());
}

#[test]
fn rebuild_recovers_when_missing_entry_dependency_or_ambiguous_candidate_is_fixed() {
    let project = Project::new();
    let (builder, _) = project.builder();
    project.put(
        "main.ts",
        "import {value} from './value.js'; export function main():void{console.log(value);}",
    );
    let (builder, result) = builder.tick();
    assert!(result.unwrap().is_err());
    assert!(!builder.watch.inputs_changed());
    project.put("value.ts", "export const value:number=3;");
    let (builder, result) = builder.tick();
    assert!(result.unwrap().is_ok());
    project.put("value.tsx", "export const value:number=4;");
    let (builder, result) = builder.tick();
    assert!(result.unwrap().is_err());
    let (builder, result) = builder.tick();
    assert!(result.is_none());
    std::fs::remove_file(project.0.join("value.tsx")).unwrap();
    let (builder, result) = builder.tick();
    assert!(result.unwrap().is_ok());
    std::fs::remove_file(project.0.join("main.ts")).unwrap();
    let (builder, result) = builder.tick();
    assert!(result.unwrap().is_err());
    let (builder, result) = builder.tick();
    assert!(result.is_none());
    project.put("main.ts", "export function main():void{}");
    assert!(builder.tick().1.unwrap().is_ok());
}

#[test]
fn rebuild_keeps_host_authority_captured_and_stops_after_cancellation() {
    let project = Project::new();
    let policy = HostPolicy::from_manifest(varta_common::Manifest::new(&[])).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let (builder, _) = Rebuilder::new(
        &project.0.join("varta.config.json"),
        Some(&policy),
        None,
        BrowserDevOptions::default(),
        cancelled.clone(),
    )
    .unwrap();
    project.put("main.ts", "export function main():void{console.log(5);}");
    let (builder, result) = builder.tick();
    let revision = result.unwrap().unwrap();
    let loader = std::str::from_utf8(&revision.files[super::super::LOADER].bytes).unwrap();
    assert!(loader.contains("\"permissionCeiling\":{}"));
    project.put("main.ts", "invalid");
    cancelled.store(true, Ordering::Release);
    assert!(builder.tick().1.is_none());
}
