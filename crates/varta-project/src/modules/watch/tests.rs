use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use varta_common::language::LanguageProfile;
use varta_compiler::ModuleLoader;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "varta-input-watch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn loader(&self) -> SourceLoader {
        let mut loader = SourceLoader::new(&self.0, LanguageProfile::CheckedF64Utf16V38).unwrap();
        loader.observe_inputs();
        loader
    }
    fn put(&self, name: &str, bytes: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn input_watch_hashes_contents_and_preserves_first_observation() {
    let fixture = Fixture::new();
    let source = fixture.put("main.ts", "1234");
    let loader = fixture.loader();
    assert_eq!(loader.load(&source).unwrap(), "1234");
    assert!(!loader.inputs_changed());
    fixture.put("unrelated.txt", "ignored");
    assert!(!loader.inputs_changed());
    fixture.put("main.ts", "5678");
    assert_eq!(loader.load(&source).unwrap(), "5678");
    assert!(loader.inputs_changed());
    fixture.put("main.ts", "1234");
    assert!(loader.inputs_changed());
    let loader = fixture.loader();
    loader.load(&source).unwrap();
    assert!(!loader.inputs_changed());
}

#[test]
fn input_watch_tracks_absent_resolution_candidates_and_missing_parents() {
    let fixture = Fixture::new();
    let importer = fixture.put("main.ts", "");
    fixture.put("x.ts", "");
    let loader = fixture.loader();
    assert_eq!(
        loader.resolve(&importer, "./x.js").unwrap(),
        fixture.0.join("x.ts")
    );
    assert!(!loader.inputs_changed());
    fixture.put("x.tsx", "");
    assert!(loader.inputs_changed());
    assert!(loader.resolve(&importer, "./x.js").is_err());
    let loader = fixture.loader();
    assert!(loader.resolve(&importer, "./nested/x.js").is_err());
    assert!(!loader.inputs_changed());
    std::fs::create_dir(fixture.0.join("nested")).unwrap();
    fixture.put("nested/x.ts", "");
    assert!(loader.inputs_changed());
}

#[test]
fn input_watch_tracks_failed_reads_bounds_and_explicit_inputs() {
    let fixture = Fixture::new();
    let loader = fixture.loader();
    let path = fixture.0.join("missing.txt");
    assert!(loader.load(&path).is_err());
    assert!(!loader.inputs_changed());
    fixture.put("missing.txt", "fixed");
    assert!(loader.inputs_changed());
    let loader = fixture.loader();
    assert!(loader.read_bytes(&path, 2).is_err());
    assert!(!loader.inputs_changed());
    fixture.put("missing.txt", "ok");
    assert!(loader.inputs_changed());
    let loader = fixture.loader();
    let config = fixture.put("varta.config.json", "{}");
    loader.observe_config(&config, b"{}".to_vec()).unwrap();
    assert!(!loader.inputs_changed());
    fixture.put("varta.config.json", "[]");
    assert!(loader.inputs_changed());
    let external = Fixture::new();
    let entry = external.put("external.ts", "123");
    let loader = fixture.loader();
    loader.entry(&entry).unwrap();
    assert!(!loader.inputs_changed());
    external.put("external.ts", "456");
    assert!(loader.inputs_changed());
}

#[cfg(unix)]
#[test]
fn input_watch_retains_pinned_root_and_rejects_symlink_replacements() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    std::fs::create_dir(fixture.0.join("sources")).unwrap();
    let path = fixture.put("sources/main.ts", "safe");
    outside.put("main.ts", "outside");
    let loader = fixture.loader();
    loader.load(&path).unwrap();
    std::fs::rename(fixture.0.join("sources"), fixture.0.join("original")).unwrap();
    std::os::unix::fs::symlink(&outside.0, fixture.0.join("sources")).unwrap();
    assert!(loader.inputs_changed());
    assert!(loader.load(&path).is_err());

    let root = fixture.0.join("pinned");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("main.ts"), "old").unwrap();
    let mut loader = SourceLoader::new(&root, LanguageProfile::CheckedF64Utf16V38).unwrap();
    loader.observe_inputs();
    loader.load(&root.join("main.ts")).unwrap();
    let old = fixture.0.join("retained");
    std::fs::rename(&root, &old).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("main.ts"), "new").unwrap();
    assert!(!loader.inputs_changed());
    std::fs::write(old.join("main.ts"), "changed pinned input").unwrap();
    assert!(loader.inputs_changed());
}

#[test]
fn input_watch_limit_is_bounded_and_does_not_evict_existing_inputs() {
    let observations = Observations::default();
    for n in 0..MAX_OBSERVATIONS {
        observations
            .probe(Path::new(&n.to_string()), &Ok(false))
            .unwrap();
    }
    assert!(
        observations
            .probe(Path::new("overflow"), &Ok(false))
            .is_err()
    );
    observations.probe(Path::new("0"), &Ok(true)).unwrap();
    assert_eq!(observations.inputs.borrow().len(), MAX_OBSERVATIONS);
    assert!(matches!(
        observations.inputs.borrow().get(&Input::Probe("0".into())),
        Some(State::Exists(false))
    ));
}

#[test]
fn input_watch_shared_compile_and_browser_resource_pipeline_tracks_real_inputs() {
    use base64::Engine;
    let fixture = Fixture::new();
    let config = fixture.put("varta.config.json", r#"{"entry":"main.ts","target":"wasm","out":"dist/app","languageProfile":"checked-f64-utf16-v38","browser":{"resources":{"pixel":"pixel.png"}}}"#);
    fixture.put("main.ts", "import text from './style.css' with { type: 'text' }; export function main():void { console.log(text); }");
    fixture.put("style.css", "first");
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==").unwrap();
    std::fs::write(fixture.0.join("pixel.png"), &png).unwrap();
    let (config, base, _, loader) = crate::compile_inputs(&config, false, true).unwrap();
    crate::browser::resources::load_with_loader(&config, &base, &loader).unwrap();
    assert!(!loader.inputs_changed());
    fixture.put("unrelated.txt", "does not rebuild");
    assert!(!loader.inputs_changed());
    fixture.put("style.css", "other");
    assert!(loader.inputs_changed());
    fixture.put("style.css", "first");
    assert!(!loader.inputs_changed());
    let mut changed_png = png.clone();
    changed_png[45] ^= 1;
    std::fs::write(fixture.0.join("pixel.png"), changed_png).unwrap();
    assert!(loader.inputs_changed());
    std::fs::write(fixture.0.join("pixel.png"), png).unwrap();
    assert!(!loader.inputs_changed());
    fixture.put("varta.config.json", "{}");
    assert!(loader.inputs_changed());
}
