//! The explicit faces the shell lays out and renders with: an artifact's
//! resource section or, for an artifact without one, the `--font` argument
//! or the host's system fonts (`--system-fonts`).
use std::sync::Arc;
use varta_artifact::Resources;
use varta_view::{TextFont, TextMeasureError, TextMeasurer};

/// One face: its bytes, the family it is registered under and the generic
/// families that family stands for.
#[derive(Debug, Clone)]
struct Face {
    data: Arc<[u8]>,
    family: String,
    aliases: Vec<String>,
}

/// A validated font collection. The first face's family is the default
/// family; each face's weight and style come from its own tables, so text
/// selects real bold and italic faces by CSS weight and style, and a weight
/// or style without a face stays refused at paint (no synthesis).
#[derive(Debug, Clone)]
pub struct FontSet {
    faces: Vec<Face>,
    origin: Origin,
}

/// Where a set's faces come from, which selects how the shaper loads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// A resource section: generic families name packaged families
    /// (`TextMeasurer::new_with_packaged_fonts`).
    Packaged,
    /// `--font PATH FAMILY` (`TextMeasurer::new_with_fonts`).
    Explicit,
    /// The host's system font files (`TextMeasurer::new_with_system_fonts`).
    System,
}

impl FontSet {
    /// The faces of an artifact's resource section, in its canonical order
    /// (`None` for an artifact that ships none). Each family maps the
    /// generic families its faces declare.
    #[must_use]
    pub fn from_resources(resources: &Resources) -> Option<Self> {
        (!resources.is_empty()).then(|| Self {
            faces: resources
                .fonts()
                .iter()
                .map(|face| Face {
                    data: face.data.as_slice().into(),
                    family: face.family.clone(),
                    aliases: face
                        .generics
                        .iter()
                        .map(|generic| generic.name().into())
                        .collect(),
                })
                .collect(),
            origin: Origin::Packaged,
        })
    }

    /// One explicit face (`--font PATH FAMILY`).
    #[must_use]
    pub fn single(data: Vec<u8>, family: String) -> Self {
        Self {
            faces: vec![Face {
                data: data.into(),
                family,
                aliases: Vec::new(),
            }],
            origin: Origin::Explicit,
        }
    }

    /// System font files with their registered families and the generic
    /// family each stands for, in mapping order.
    pub(super) fn from_system_faces(faces: Vec<(Vec<u8>, String, Option<&str>)>) -> Self {
        Self {
            faces: faces
                .into_iter()
                .map(|(data, family, alias)| Face {
                    data: data.into(),
                    family,
                    aliases: alias.into_iter().map(Into::into).collect(),
                })
                .collect(),
            origin: Origin::System,
        }
    }

    /// The number of faces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Whether the set has no face (never true for a constructed set).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// `(family, generic aliases)` of each face, in collection order.
    pub fn families(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.faces
            .iter()
            .map(|face| (face.family.as_str(), face.aliases.as_slice()))
    }

    /// A text measurer over every face (`TextMeasurer::new_with_packaged_fonts`
    /// for a resource section, `new_with_system_fonts` for system fonts,
    /// otherwise `new_with_fonts`): the shaper parses each face and refuses
    /// one that is malformed or whose tables do not name its family.
    pub fn measurer(&self) -> Result<TextMeasurer, TextMeasureError> {
        let fonts = self
            .faces
            .iter()
            .map(|face| TextFont {
                data: face.data.to_vec(),
                family: face.family.clone(),
                aliases: face.aliases.clone(),
            })
            .collect();
        match self.origin {
            Origin::Packaged => TextMeasurer::new_with_packaged_fonts(fonts),
            Origin::Explicit => TextMeasurer::new_with_fonts(fonts),
            Origin::System => TextMeasurer::new_with_system_fonts(fonts),
        }
    }
}
