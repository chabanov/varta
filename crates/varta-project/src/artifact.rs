//! Tooling compatibility facade; the container protocol has one runtime-independent owner.
//! This is also the packaging step that fills an artifact's resource section
//! from the configuration's declared font faces.
use crate::{Result, VartaConfig};
use std::path::Path;
pub use varta_artifact::Artifact;
use varta_artifact::{
    FontFace, FontStyle, GenericFamily, MAX_FONT_BYTES, MAX_FONT_FACE_BYTES, MAX_FONT_FACES,
    NORMAL_FONT_WEIGHT, Resources,
};
pub(crate) use varta_artifact::{MAX_MANIFEST_BYTES, RESOURCE_SIDECAR_EXTENSION};
use varta_common::VartaError;

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    Ok(varta_artifact::read_bounded(path, limit)?)
}

/// One declared font face of the configuration's `fonts` list:
/// `{"family": "Brand", "path": "fonts/brand-bold.ttf", "weight": 700,
/// "style": "italic", "generic": ["sans-serif"]}`. `weight` defaults to 400,
/// `style` to `normal` and `generic` to none. The face's own `name` and
/// `OS/2` tables must state the declared family, weight and style.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontSpec {
    pub family: String,
    /// A no-follow path under the configuration directory, in the common
    /// view resource-name grammar.
    pub path: String,
    #[serde(default = "normal_weight")]
    pub weight: u16,
    #[serde(default)]
    pub style: Option<String>,
    /// CSS generic families this family stands for on renderers that map
    /// them to package fonts (the native shell).
    #[serde(default)]
    pub generic: Vec<String>,
}

fn normal_weight() -> u16 {
    NORMAL_FONT_WEIGHT
}

impl FontSpec {
    fn style(&self) -> std::result::Result<FontStyle, VartaError> {
        self.style.as_deref().map_or(Ok(FontStyle::Normal), |name| {
            FontStyle::from_name(name).ok_or_else(|| {
                invalid(&format!(
                    "unknown font style {name:?}; use normal or italic"
                ))
            })
        })
    }
    fn generics(&self) -> std::result::Result<Vec<GenericFamily>, VartaError> {
        self.generic
            .iter()
            .map(|name| {
                GenericFamily::from_name(name).ok_or_else(|| {
                    invalid(&format!(
                        "unknown generic family {name:?}; use sans-serif, serif, monospace or system-ui"
                    ))
                })
            })
            .collect()
    }
}

fn invalid(message: &str) -> VartaError {
    VartaError::InvalidManifest(message.into())
}

/// Configuration-time checks of the `fonts` list: the profile, the face
/// count, resource-grammar paths and known styles and generic families.
/// The bytes are checked when the project is packaged ([`resources`]).
pub(crate) fn validate_fonts(config: &VartaConfig) -> std::result::Result<(), VartaError> {
    if config.fonts.is_empty() {
        return Ok(());
    }
    if !config.language_profile.tsx() || config.fonts.len() > MAX_FONT_FACES {
        return Err(invalid(
            "fonts require languageProfile checked-f64-utf16-v38 and a bounded face list",
        ));
    }
    for font in &config.fonts {
        crate::browser::resources::validate_name(&font.path)
            .map_err(|error| invalid(&error.to_string()))?;
        font.style()?;
        font.generics()?;
    }
    Ok(())
}

/// The resource section the configuration declares: every face read through
/// the project's pinned no-follow loader, bounded per face and in total,
/// validated against its own tables and put into canonical order. Empty
/// without declared fonts.
pub(crate) fn resources(config: &VartaConfig, base: &Path) -> Result<Resources> {
    if config.fonts.is_empty() {
        return Ok(Resources::default());
    }
    validate_fonts(config)?;
    let root = base.canonicalize()?;
    let loader = crate::modules::SourceLoader::new(&root, config.language_profile)?;
    let mut faces = Vec::with_capacity(config.fonts.len());
    let mut total = 0usize;
    for font in &config.fonts {
        let limit = MAX_FONT_FACE_BYTES.min(MAX_FONT_BYTES - total);
        let data = loader.read_bytes(&root.join(&font.path), limit)?;
        total += data.len();
        faces.push(FontFace {
            family: font.family.clone(),
            weight: font.weight,
            style: font.style()?,
            generics: font.generics()?,
            data,
        });
    }
    Ok(Resources::new(faces).map_err(VartaError::from)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectError;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    const REGULAR: &[u8] = include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf");
    const BOLD: &[u8] = include_bytes!("../../varta-view/tests/fixtures/bold-900.ttf");
    const MONO: &[u8] = include_bytes!("../../varta-view/tests/fixtures/advance-900.ttf");

    struct Project(PathBuf);
    impl Project {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "varta-project-fonts-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(path.join("fonts")).unwrap();
            for (name, bytes) in [
                ("regular.ttf", REGULAR),
                ("bold.ttf", BOLD),
                ("mono.ttf", MONO),
            ] {
                std::fs::write(path.join("fonts").join(name), bytes).unwrap();
            }
            Self(path)
        }
    }
    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn config(fonts: serde_json::Value) -> std::result::Result<VartaConfig, VartaError> {
        let config: VartaConfig = serde_json::from_value(serde_json::json!({
            "entry": "main.ts",
            "target": "wasm",
            "out": "app",
            "languageProfile": "checked-f64-utf16-v38",
            "fonts": fonts,
        }))
        .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }
    fn declared() -> serde_json::Value {
        serde_json::json!([
            {"family": "Varta Fixture 900", "path": "fonts/mono.ttf", "generic": ["monospace"]},
            {"family": "Varta Fixture 400", "path": "fonts/bold.ttf", "weight": 700},
            {"family": "Varta Fixture 400", "path": "fonts/regular.ttf", "style": "normal",
             "generic": []},
        ])
    }

    #[test]
    fn declared_faces_are_read_validated_and_canonically_ordered() {
        let project = Project::new();
        let resources = resources(&config(declared()).unwrap(), &project.0).unwrap();
        let faces: Vec<_> = resources
            .fonts()
            .iter()
            .map(|face| (face.family.as_str(), face.weight, face.generics.clone()))
            .collect();
        assert_eq!(
            faces,
            [
                ("Varta Fixture 900", 400, vec![GenericFamily::Monospace]),
                ("Varta Fixture 400", 400, vec![]),
                ("Varta Fixture 400", 700, vec![]),
            ]
        );
        assert_eq!(resources.fonts()[2].data, BOLD);
        let none = config(serde_json::json!([])).unwrap();
        assert!(resources_of(&none, &project).unwrap().is_empty());
    }

    fn resources_of(config: &VartaConfig, project: &Project) -> Result<Resources> {
        resources(config, &project.0)
    }

    #[test]
    fn configurations_refuse_unknown_styles_generics_paths_and_profiles() {
        for fonts in [
            serde_json::json!([{"family": "A", "path": "fonts/regular.ttf", "style": "oblique"}]),
            serde_json::json!([{"family": "A", "path": "fonts/regular.ttf", "generic": ["cursive"]}]),
            serde_json::json!([{"family": "A", "path": "../regular.ttf"}]),
            serde_json::json!([{"family": "A", "path": "/etc/regular.ttf"}]),
            serde_json::json!([{"family": "A", "path": "fonts/regular.ttf", "url": "x"}]),
        ] {
            assert!(config(fonts.clone()).is_err(), "{fonts}");
        }
        let many: Vec<_> = (0..=MAX_FONT_FACES)
            .map(|_| serde_json::json!({"family": "A", "path": "fonts/regular.ttf"}))
            .collect();
        assert!(config(serde_json::Value::from(many)).is_err());
        let mut earlier: serde_json::Value = serde_json::json!({
            "entry": "main.ts",
            "target": "wasm",
            "out": "app",
            "languageProfile": "checked-f64-utf16-v37",
            "fonts": [{"family": "A", "path": "fonts/regular.ttf"}],
        });
        let parsed: VartaConfig = serde_json::from_value(earlier.take()).unwrap();
        assert!(parsed.validate().is_err());
    }

    #[test]
    fn faces_that_contradict_their_declaration_or_escape_the_project_are_refused() {
        let project = Project::new();
        let refused = |fonts| match resources_of(&config(fonts).unwrap(), &project) {
            Err(ProjectError::Config(VartaError::InvalidArtifact(message))) => message,
            other => panic!("{:?}", other.map(|resources| resources.fonts().len())),
        };
        // The bold face declared as the regular weight, and a family the
        // face's name table does not state.
        assert!(
            refused(serde_json::json!([{"family": "Varta Fixture 400", "path": "fonts/bold.ttf"}]))
                .contains("weight")
        );
        assert!(
            refused(serde_json::json!([{"family": "Brand", "path": "fonts/regular.ttf"}]))
                .contains("name table")
        );
        let missing = config(serde_json::json!([{"family": "A", "path": "fonts/missing.ttf"}]));
        assert!(matches!(
            resources_of(&missing.unwrap(), &project),
            Err(ProjectError::Io(_))
        ));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                project.0.join("fonts/regular.ttf"),
                project.0.join("fonts/link.ttf"),
            )
            .unwrap();
            let linked = config(serde_json::json!([
                {"family": "Varta Fixture 400", "path": "fonts/link.ttf"}
            ]));
            assert!(matches!(
                resources_of(&linked.unwrap(), &project),
                Err(ProjectError::Io(_))
            ));
        }
        let oversized = project.0.join("fonts/large.ttf");
        std::fs::write(&oversized, vec![0; MAX_FONT_FACE_BYTES + 1]).unwrap();
        let large = config(serde_json::json!([{"family": "A", "path": "fonts/large.ttf"}]));
        assert!(matches!(
            resources_of(&large.unwrap(), &project),
            Err(ProjectError::Io(_))
        ));
    }
}
