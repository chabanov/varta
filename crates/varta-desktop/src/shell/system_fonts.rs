//! Host-explicit system fonts (`varta-shell --system-fonts`): the files the
//! platform's browser uses for the CSS generic families, named by the
//! platform's own font APIs and read by the host, never by the application,
//! and the platform's fallback cascade for characters those faces lack
//! ([`super::fallback`]).
//!
//! Text then depends on the machine: another OS version or another font set
//! lays out and draws differently, so pixel and geometry oracles compare a
//! run only with the platform's browser on the same machine. Nothing changes
//! for an artifact that ships its own fonts (the shell refuses the option for
//! it).
use super::{FontSet, ShellError, fallback::SystemFallback};
use std::path::PathBuf;

/// One file to read and the family it must register.
#[derive(Debug)]
pub(super) struct SystemFile {
    pub path: PathBuf,
    /// The installed family name, or `None` for the macOS UI font, whose
    /// platform name differs from the file's own family (the file's first
    /// family is registered).
    pub family: Option<String>,
    /// The generic families the file's family stands for (only on a
    /// family's first file).
    pub aliases: Vec<&'static str>,
}

/// The mapped files and the platform cascade for the faces they register.
struct Mapping {
    files: Vec<SystemFile>,
    /// Indices of the files of the macOS UI font.
    ui_files: Vec<usize>,
}

/// Adds `paths` of `family` for `alias`: a family already mapped takes the
/// alias on its first file, and no file is read twice.
fn add_family(
    files: &mut Vec<SystemFile>,
    family: Option<String>,
    alias: &'static str,
    paths: Vec<PathBuf>,
) -> Result<Vec<usize>, ShellError> {
    if paths.is_empty() {
        return Err(ShellError::SystemFontMissing(alias));
    }
    if let Some(name) = &family
        && let Some(first) = files
            .iter_mut()
            .find(|file| file.family.as_ref() == Some(name))
    {
        first.aliases.push(alias);
        return Ok(Vec::new());
    }
    let mut added = Vec::new();
    for path in paths {
        if files.iter().any(|file| file.path == path) {
            continue;
        }
        let aliases = if added.is_empty() {
            vec![alias]
        } else {
            Vec::new()
        };
        added.push(files.len());
        files.push(SystemFile {
            path,
            family: family.clone(),
            aliases,
        });
    }
    if added.is_empty() {
        return Err(ShellError::SystemFontMissing(alias));
    }
    Ok(added)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Mapping, ShellError, add_family};
    use core_text::{
        font::{self, kCTFontSystemFontType},
        font_collection,
        font_descriptor::kCTFontItalicTrait,
    };
    use std::path::PathBuf;

    /// CoreText's UI font size used only to name the face; the file is the
    /// same at every size.
    const UI_FONT_POINT_SIZE: f64 = 13.0;

    /// Chrome's choices on macOS (probed with Chrome 156 on macOS 27: equal
    /// advances for each family in weights 400 to 600): the standard font,
    /// which is also the family of text without `font-family`, comes first.
    /// `None` is the UI font.
    pub(super) const GENERICS: [(&str, Option<&str>); 4] = [
        ("serif", Some("Times")),
        ("system-ui", None),
        ("sans-serif", Some("Helvetica")),
        ("monospace", Some("Menlo")),
    ];

    pub(super) fn mapping() -> Result<Mapping, ShellError> {
        let mut files = Vec::new();
        let mut ui_files = Vec::new();
        for (alias, family) in GENERICS {
            match family {
                None => ui_files = add_family(&mut files, None, alias, ui_font_files())?,
                Some(name) => {
                    add_family(&mut files, Some(name.into()), alias, family_files(name))?;
                }
            }
        }
        Ok(Mapping { files, ui_files })
    }

    /// The UI font's regular file and, when it has one, its italic file.
    /// `kCTFontSystemFontType` is a valid UI font type, for which CoreText
    /// always returns a font.
    fn ui_font_files() -> Vec<PathBuf> {
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

#[cfg(all(not(target_os = "macos"), target_os = "linux"))]
mod platform {
    use super::{Mapping, ShellError, fontconfig_mapping};

    pub(super) fn mapping() -> Result<Mapping, ShellError> {
        let fc = super::super::fontconfig::open().ok_or(ShellError::SystemFontsUnavailable)?;
        fontconfig_mapping(&fc)
    }
}

/// Chrome on Linux: the standard and serif font first, then the UI font
/// (`system-ui`), sans-serif and monospace, each the family fontconfig
/// accepts for Chrome's default name or else for Blink's last resort.
#[cfg(any(target_os = "linux", all(test, feature = "fontconfig")))]
fn fontconfig_mapping(fc: &::fontconfig::Fontconfig) -> Result<Mapping, ShellError> {
    use super::fontconfig::{self, MONOSPACE, SANS_SERIF, SERIF, UI_FAMILY};
    use ::fontconfig::{FC_SLANT_ROMAN, FC_WEIGHT_REGULAR};
    let ui = fontconfig::default_family(fc, UI_FAMILY)
        .ok_or(ShellError::SystemFontMissing("system-ui"))?;
    let generics: [(&'static str, Vec<String>); 4] = [
        ("serif", SERIF.map(String::from).to_vec()),
        ("system-ui", vec![ui]),
        ("sans-serif", SANS_SERIF.map(String::from).to_vec()),
        ("monospace", MONOSPACE.map(String::from).to_vec()),
    ];
    let mut files = Vec::new();
    for (alias, candidates) in generics {
        let family = candidates
            .iter()
            .find_map(|name| fontconfig::match_family(fc, name, FC_WEIGHT_REGULAR, FC_SLANT_ROMAN))
            .ok_or(ShellError::SystemFontMissing(alias))?
            .family;
        let paths = fontconfig::family_faces(fc, &family)
            .into_iter()
            .map(|face| face.path)
            .collect();
        add_family(&mut files, Some(family), alias, paths)?;
    }
    Ok(Mapping {
        files,
        ui_files: Vec::new(),
    })
}

/// The files of every mapped generic family, in mapping order.
fn mapping() -> Result<Mapping, ShellError> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        platform::mapping()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err(ShellError::SystemFontsUnsupported)
    }
}

impl FontSet {
    /// The platform's fonts for the generic families (`--system-fonts`):
    /// each file is read bounded by
    /// [`varta_common::view::fonts::MAX_SYSTEM_FONT_FILE_BYTES`] and must
    /// register its family. The text shaper then parses the whole set, and
    /// every measurer of the set asks the platform's fallback cascade for
    /// characters the faces lack.
    pub fn system() -> Result<Self, ShellError> {
        let (faces, ui_families) = read_faces(mapping()?)?;
        Ok(Self::from_system_faces(
            faces,
            SystemFallback::new(cascade(ui_families)),
            ENLARGED_ASCENT_FAMILIES,
        ))
    }
}

/// A face's bytes, registered family and generic aliases.
type SystemFace = (Vec<u8>, String, Vec<&'static str>);

/// Reads every mapped file, checks the family it registers, and names the
/// registered families of the UI font's files.
fn read_faces(mapping: Mapping) -> Result<(Vec<SystemFace>, Vec<String>), ShellError> {
    let mut faces = Vec::new();
    let mut ui_families = Vec::new();
    for (index, file) in mapping.files.into_iter().enumerate() {
        let data = super::launch::read_system_font(&file.path)?;
        let families = varta_view::TextMeasurer::system_font_families(data.clone())
            .map_err(ShellError::Fonts)?;
        let family = match file.family {
            Some(name) if families.contains(&name) => name,
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
        if mapping.ui_files.contains(&index) && !ui_families.contains(&family) {
            ui_families.push(family.clone());
        }
        faces.push((data, family, file.aliases));
    }
    Ok((faces, ui_families))
}

/// Chrome on macOS enlarges the ascent of these families by 15% of their
/// height (`FontMetrics::AscentDescentWithHacks`, after Safari) to match
/// Times New Roman, Arial and Courier New on Windows.
#[cfg(target_os = "macos")]
const ENLARGED_ASCENT_FAMILIES: &[&str] = &["Times", "Helvetica", "Courier"];
/// Chrome on Linux keeps every face's own ascent.
#[cfg(not(target_os = "macos"))]
const ENLARGED_ASCENT_FAMILIES: &[&str] = &[];

#[cfg(target_os = "macos")]
fn cascade(ui_families: Vec<String>) -> super::fallback::platform::Cascade {
    super::fallback::platform::Cascade { ui_families }
}

#[cfg(all(not(target_os = "macos"), target_os = "linux"))]
fn cascade(_: Vec<String>) -> super::fallback::platform::Cascade {
    super::fallback::platform::Cascade {
        locale: super::fontconfig::locale(),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn cascade(_: Vec<String>) -> super::fallback::platform::Cascade {
    super::fallback::platform::Cascade
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
        assert_eq!(aliases, platform::GENERICS.map(|(alias, _)| alias));
        let named: Vec<&str> = fonts.families().map(|(family, _)| family).collect();
        for name in ["Times", "Helvetica", "Menlo"] {
            assert!(named.contains(&name), "{named:?}");
        }
        let mut measurer = fonts.measurer().unwrap();
        for (alias, _) in platform::GENERICS {
            let width = measurer
                .measure_with_family_list("Varta", 16.0, None, alias)
                .unwrap()
                .width;
            assert!(width > 0.0, "{alias}");
        }
    }

    #[test]
    fn a_missing_family_has_no_files() {
        assert!(platform::family_files("Varta Family That Is Not Installed").is_empty());
    }
}

#[cfg(all(test, any(target_os = "linux", feature = "fontconfig")))]
mod fontconfig_tests {
    use super::*;

    /// Chrome on Linux: serif (the standard font) first, then the UI font,
    /// sans-serif and monospace; each family's files register it.
    #[test]
    fn fontconfig_maps_every_generic_family_to_faces_that_register_it() {
        let fc = super::super::fontconfig::open().expect("libfontconfig loads");
        let mapping = fontconfig_mapping(&fc).unwrap();
        let aliases: Vec<&str> = mapping
            .files
            .iter()
            .flat_map(|file| file.aliases.iter().copied())
            .collect();
        let mut sorted = aliases.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, ["monospace", "sans-serif", "serif", "system-ui"]);
        assert_eq!(aliases.first(), Some(&"serif"));
        let (faces, _) = read_faces(mapping).unwrap();
        let fonts =
            FontSet::from_system_faces(faces, SystemFallback::new(cascade(Vec::new())), &[]);
        let mut measurer = fonts.measurer().unwrap();
        for alias in ["serif", "system-ui", "sans-serif", "monospace"] {
            let width = measurer
                .measure_with_family_list("Varta", 16.0, None, alias)
                .unwrap()
                .width;
            assert!(width > 0.0, "{alias}");
        }
    }
}
