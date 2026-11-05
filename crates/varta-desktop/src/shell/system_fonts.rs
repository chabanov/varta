//! Host-explicit system fonts (`varta-shell --system-fonts`): the files the
//! platform's browser uses for the CSS generic families, named by the
//! platform's own font APIs and read by the host, never by the application.
//!
//! Text then depends on the machine: another OS version or another font set
//! lays out and draws differently, so pixel and geometry oracles do not apply
//! to such a run. Nothing changes for an artifact that ships its own fonts
//! (the shell refuses the option for it), and a character outside the mapped
//! faces gets only the next family of its CSS list, never an OS fallback font.
use super::{FontSet, ShellError};
use std::path::PathBuf;

/// Where a generic family's faces come from.
#[derive(Debug, Clone, Copy)]
enum Source {
    /// The platform's UI font (regular and italic files).
    SystemUi,
    /// Every file of a named installed family.
    Family(&'static str),
}

/// One generic family the host maps and its platform source.
#[derive(Debug, Clone, Copy)]
struct Generic {
    alias: &'static str,
    source: Source,
}

/// Chrome's choices on macOS (probed with Chrome 156 on macOS 27: equal
/// advances for each family in weights 400 to 600): the standard font,
/// which is also the family of text without `font-family`, comes first.
#[cfg(target_os = "macos")]
const GENERICS: [Generic; 4] = [
    Generic {
        alias: "serif",
        source: Source::Family("Times"),
    },
    Generic {
        alias: "system-ui",
        source: Source::SystemUi,
    },
    Generic {
        alias: "sans-serif",
        source: Source::Family("Helvetica"),
    },
    Generic {
        alias: "monospace",
        source: Source::Family("Menlo"),
    },
];

/// One file to read and the family it must register.
#[derive(Debug)]
pub(super) struct SystemFile {
    pub path: PathBuf,
    /// The installed family name, or `None` for the UI font, whose platform
    /// name differs from the file's own family (the file's first family is
    /// registered).
    pub family: Option<&'static str>,
    /// The generic family the file stands for (only on a family's first
    /// file).
    pub alias: Option<&'static str>,
}

/// The files of every mapped generic family, in [`GENERICS`] order.
pub(super) fn files() -> Result<Vec<SystemFile>, ShellError> {
    #[cfg(target_os = "macos")]
    {
        let mut files = Vec::new();
        for generic in GENERICS {
            let (paths, family) = match generic.source {
                Source::SystemUi => (macos::ui_font_files(), None),
                Source::Family(name) => (macos::family_files(name), Some(name)),
            };
            if paths.is_empty() {
                return Err(ShellError::SystemFontMissing(generic.alias));
            }
            for (index, path) in paths.into_iter().enumerate() {
                files.push(SystemFile {
                    path,
                    family,
                    alias: (index == 0).then_some(generic.alias),
                });
            }
        }
        Ok(files)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(ShellError::SystemFontsUnsupported)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use core_text::{
        font::{self, kCTFontSystemFontType},
        font_collection,
        font_descriptor::kCTFontItalicTrait,
    };
    use std::path::PathBuf;

    /// CoreText's UI font size used only to name the face; the file is the
    /// same at every size.
    const UI_FONT_POINT_SIZE: f64 = 13.0;

    /// The UI font's regular file and, when it has one, its italic file.
    /// `kCTFontSystemFontType` is a valid UI font type, for which CoreText
    /// always returns a font.
    pub(super) fn ui_font_files() -> Vec<PathBuf> {
        let regular =
            font::new_ui_font_for_language(kCTFontSystemFontType, UI_FONT_POINT_SIZE, None);
        let italic = regular.clone_with_symbolic_traits(kCTFontItalicTrait, kCTFontItalicTrait);
        let mut paths = Vec::new();
        for face in std::iter::once(regular).chain(italic) {
            if let Some(path) = face.url().and_then(|url| url.to_path())
                && !paths.contains(&path)
            {
                paths.push(path);
            }
        }
        paths
    }

    /// The files of an installed family, sorted and without repeats; empty
    /// when the family is not installed (CoreText matches the family name as
    /// a mandatory attribute, so it substitutes no other family).
    pub(super) fn family_files(family: &str) -> Vec<PathBuf> {
        let Some(descriptors) =
            font_collection::create_for_family(family).and_then(|set| set.get_descriptors())
        else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = descriptors
            .iter()
            .filter(|descriptor| descriptor.family_name() == family)
            .filter_map(|descriptor| descriptor.font_path())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }
}

impl FontSet {
    /// The platform's fonts for the generic families (`--system-fonts`):
    /// each file is read bounded by
    /// [`varta_common::view::fonts::MAX_SYSTEM_FONT_FILE_BYTES`] and must
    /// register its family. The text shaper then parses the whole set.
    pub fn system() -> Result<Self, ShellError> {
        let mut faces = Vec::new();
        for file in files()? {
            let data = super::launch::read_system_font(&file.path)?;
            let families = varta_view::TextMeasurer::system_font_families(data.clone())
                .map_err(ShellError::Fonts)?;
            let family = match file.family {
                Some(name) if families.iter().any(|family| family == name) => name.to_owned(),
                Some(name) => {
                    return Err(ShellError::SystemFontFamily {
                        path: file.path,
                        family: name,
                    });
                }
                None => families
                    .into_iter()
                    .next()
                    .ok_or(ShellError::Fonts(varta_view::TextMeasureError::Font))?,
            };
            faces.push((data, family, file.alias));
        }
        Ok(Self::from_system_faces(faces))
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn every_generic_family_maps_to_a_file_with_its_family() {
        let fonts = FontSet::system().unwrap();
        let aliases: Vec<&str> = fonts
            .families()
            .flat_map(|(_, aliases)| aliases.iter().map(String::as_str))
            .collect();
        assert_eq!(aliases, GENERICS.map(|generic| generic.alias));
        let named: Vec<&str> = fonts.families().map(|(family, _)| family).collect();
        for name in ["Times", "Helvetica", "Menlo"] {
            assert!(named.contains(&name), "{named:?}");
        }
        let mut measurer = fonts.measurer().unwrap();
        for generic in GENERICS {
            let width = measurer
                .measure_with_family_list("Varta", 16.0, None, generic.alias)
                .unwrap()
                .width;
            assert!(width > 0.0, "{}", generic.alias);
        }
    }

    #[test]
    fn a_missing_family_has_no_files() {
        assert!(macos::family_files("Varta Family That Is Not Installed").is_empty());
    }
}
