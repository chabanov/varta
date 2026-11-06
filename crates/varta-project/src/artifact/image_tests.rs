//! The configuration's `images`: validated with the configuration, read
//! through the pinned loader, packaged into the resource section and carried
//! by the browser bundle under the same names.
use super::*;
use crate::ProjectError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A valid 1x1 RGBA PNG.
const PIXEL: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "varta-project-images-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("images")).unwrap();
        std::fs::write(path.join("images/pixel.png"), PIXEL).unwrap();
        let mut animated = PIXEL.to_vec();
        // Retype the IDAT chunk as an animation control chunk.
        animated[37..41].copy_from_slice(b"acTL");
        std::fs::write(path.join("images/animated.png"), animated).unwrap();
        Self(path)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(extra: serde_json::Value) -> std::result::Result<VartaConfig, VartaError> {
    let mut value = serde_json::json!({
        "entry": "main.ts",
        "target": "wasm",
        "out": "app",
        "languageProfile": "checked-f64-utf16-v38",
    });
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    let config: VartaConfig = serde_json::from_value(value)
        .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
    config.validate()?;
    Ok(config)
}

#[test]
fn images_are_validated_with_the_configuration() {
    assert!(config(serde_json::json!({"images": {"icons/pixel.png": "images/pixel.png"}})).is_ok());
    for images in [
        serde_json::json!({"../pixel.png": "images/pixel.png"}),
        serde_json::json!({"pixel.png": "/etc/pixel.png"}),
        serde_json::json!({"pixel.png": "images/../pixel.png"}),
        serde_json::json!({"https://a.test/p.png": "images/pixel.png"}),
    ] {
        assert!(
            config(serde_json::json!({ "images": images })).is_err(),
            "{images}"
        );
    }
    // One inventory per renderer pair: `browser.resources` would be a
    // browser-only image the native shell does not have.
    assert!(
        config(serde_json::json!({
            "images": {"pixel.png": "images/pixel.png"},
            "browser": {"resources": {"other.png": "images/pixel.png"}},
        }))
        .is_err()
    );
    let many: serde_json::Map<_, _> = (0..=MAX_IMAGES)
        .map(|i| (format!("{i}.png"), serde_json::json!("images/pixel.png")))
        .collect();
    assert!(config(serde_json::json!({ "images": many })).is_err());
    let mut earlier = config(serde_json::json!({})).unwrap();
    earlier.language_profile = varta_common::language::LanguageProfile::CheckedF64Utf16V37;
    earlier.images = [("pixel.png".to_owned(), "images/pixel.png".to_owned())].into();
    assert!(earlier.validate().is_err());
}

#[test]
fn images_are_packaged_beside_fonts_and_carried_by_the_browser_bundle() {
    let project = Project::new();
    let declared = config(serde_json::json!({
        "images": {"b.png": "images/pixel.png", "a/pixel.png": "images/pixel.png"},
    }))
    .unwrap();
    let section = resources(&declared, &project.0).unwrap();
    assert!(section.fonts().is_empty());
    let names: Vec<_> = section.images().iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["a/pixel.png", "b.png"]);
    assert_eq!(section.images()[0].data, PIXEL);
    let payload = b"\0asm\x01\0\0\0";
    let bytes = section.encode(payload).unwrap();
    assert_eq!(Resources::decode(&bytes, payload).unwrap(), section);
    let browser = crate::browser::resources::load(&declared, &project.0).unwrap();
    let carried: Vec<_> = browser
        .iter()
        .map(|resource| (resource.name.as_str(), resource.font))
        .collect();
    assert_eq!(carried, [("a/pixel.png", false), ("b.png", false)]);
    let refused = config(serde_json::json!({"images": {"a.png": "images/animated.png"}})).unwrap();
    match resources(&refused, &project.0) {
        Err(ProjectError::Config(VartaError::InvalidArtifact(message))) => {
            assert!(message.contains("animated"), "{message}");
        }
        other => panic!("{:?}", other.map(|section| section.images().len())),
    }
    let missing = config(serde_json::json!({"images": {"a.png": "images/missing.png"}})).unwrap();
    assert!(matches!(
        resources(&missing, &project.0),
        Err(ProjectError::Io(_))
    ));
}
