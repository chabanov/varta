//! fontconfig lookups for `--system-fonts` on Linux, made as Chrome on Linux
//! makes them: its generic-family defaults matched through Skia's
//! fontconfig interface, `system-ui` as the UI's default font, and the
//! character fallback of `gfx::GetFallbackFontForChar`.
//!
//! Built on Linux, and for its tests on other hosts with the `fontconfig`
//! feature (to test these rules against a local fontconfig); libfontconfig
//! is opened at run time, so a host without it refuses `--system-fonts`
//! clearly.
use fontconfig::{
    CharSet, FC_FAMILY, FC_LANG, FC_SLANT, FC_SLANT_ITALIC, FC_SLANT_ROMAN, FC_WEIGHT,
    FC_WEIGHT_BOLD, FC_WEIGHT_REGULAR, FC_WIDTH, FontFormat, Fontconfig, Pattern, UnicodeCoverage,
};
use std::ffi::CString;
use std::path::PathBuf;

/// fontconfig's `FC_WIDTH_NORMAL`.
const WIDTH_NORMAL: i32 = 100;
/// Sorted fonts examined for one match; fontconfig sorts the acceptable
/// ones first.
const MAX_CANDIDATES: usize = 32;

/// Families Skia's fontconfig interface accepts whatever fontconfig
/// substitutes for them (`IsFallbackFontAllowed`), compared without case.
const FALLBACK_ALLOWED: [&str; 3] = ["sans", "serif", "monospace"];
/// Skia's metric-compatible classes (`GetFontEquivClass`): a match from the
/// same class as the requested family is an acceptable substitute. Only the
/// classes of Chrome's own generic defaults are listed, since only those
/// families are requested here.
const METRIC_CLASSES: [&[&str]; 3] = [
    &["Arial", "Arimo", "Liberation Sans"],
    &["Times New Roman", "Tinos", "Liberation Serif"],
    &["Courier New", "Cousine", "Liberation Mono"],
];

/// Chrome's Linux defaults (`locale_settings_linux.grd`): the standard and
/// serif font is Times New Roman, sans-serif Arial and fixed Monospace.
/// When fontconfig has no acceptable face for one, Blink's last resort for
/// the generic is the fontconfig family `Serif`, `Sans` or `Monospace`.
pub(super) const SERIF: [&str; 2] = ["Times New Roman", "Serif"];
pub(super) const SANS_SERIF: [&str; 2] = ["Arial", "Sans"];
pub(super) const MONOSPACE: [&str; 2] = ["Monospace", "Monospace"];
/// Chrome's UI font without a desktop toolkit (`PlatformFontSkia`): the
/// family fontconfig matches for `sans`.
pub(super) const UI_FAMILY: &str = "sans";

/// The styles whose faces a mapped family loads: regular and bold, roman
/// and italic (other weights select among these faces).
const STYLES: [(i32, i32); 4] = [
    (FC_WEIGHT_REGULAR, FC_SLANT_ROMAN),
    (FC_WEIGHT_BOLD, FC_SLANT_ROMAN),
    (FC_WEIGHT_REGULAR, FC_SLANT_ITALIC),
    (FC_WEIGHT_BOLD, FC_SLANT_ITALIC),
];

/// One face fontconfig named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Face {
    pub family: String,
    pub path: PathBuf,
    pub index: u32,
}

/// fontconfig, or `None` when the library is missing or fails to start.
pub(super) fn open() -> Option<Fontconfig> {
    Fontconfig::new()
}

/// Whether the shaper reads this format (Skia's `valid_pattern` requires a
/// scalable face; the native shaper reads TrueType and CFF outlines).
fn scalable(pattern: &Pattern<'_>) -> bool {
    matches!(pattern.format(), Ok(FontFormat::TrueType | FontFormat::CFF))
}

fn face(pattern: &Pattern<'_>) -> Option<Face> {
    let path = PathBuf::from(pattern.filename().ok()?);
    Some(Face {
        family: pattern.get_string(FC_FAMILY).ok()?.to_owned(),
        index: u32::try_from(pattern.face_index().unwrap_or(0)).ok()?,
        path,
    })
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Skia's `SkFontConfigInterfaceDirect::matchFamilyName`: the first
/// scalable, readable face fontconfig sorts for `family` in this style, if
/// it is an acceptable substitute: the family fontconfig's configuration
/// put first, the requested family, one metric-compatible with it, or any
/// face for `sans`, `serif` and `monospace`.
pub(super) fn match_family(fc: &Fontconfig, family: &str, weight: i32, slant: i32) -> Option<Face> {
    let mut pattern = Pattern::new(fc).ok()?;
    pattern
        .add_string(FC_FAMILY, &CString::new(family).ok()?)
        .ok()?;
    pattern.add_integer(FC_WEIGHT, weight).ok()?;
    pattern.add_integer(FC_SLANT, slant).ok()?;
    pattern.add_integer(FC_WIDTH, WIDTH_NORMAL).ok()?;
    let fonts = pattern.sort_fonts(UnicodeCoverage::NoTrim).ok()?;
    // The pattern after configuration and default substitution.
    let configured = pattern.get_string(FC_FAMILY).ok()?.to_owned();
    let found = fonts
        .iter()
        .take(MAX_CANDIDATES)
        .filter(scalable)
        .filter_map(|font| face(&font))
        .find(|face| std::fs::File::open(&face.path).is_ok())?;
    acceptable(family, &configured, &found.family).then_some(found)
}

/// Skia's `MatchFont` acceptance: whether `found`, fontconfig's best face
/// for the requested `family` whose configuration put `configured` first,
/// may stand for it.
fn acceptable(family: &str, configured: &str, found: &str) -> bool {
    FALLBACK_ALLOWED.iter().any(|allowed| same(allowed, family))
        || same(found, configured)
        || same(found, family)
        || METRIC_CLASSES.iter().any(|class| {
            class.iter().any(|member| same(member, family))
                && class.iter().any(|member| same(member, found))
        })
}

/// The family fontconfig matches for `family` with no acceptance rule
/// (`FcFontMatch`, as Chrome names its default UI font).
pub(super) fn default_family(fc: &Fontconfig, family: &str) -> Option<String> {
    let mut pattern = Pattern::new(fc).ok()?;
    pattern
        .add_string(FC_FAMILY, &CString::new(family).ok()?)
        .ok()?;
    let found = pattern.font_match().ok()?;
    Some(found.get_string(FC_FAMILY).ok()?.to_owned())
}

/// The faces of `family` for the four basic styles, as fontconfig matches
/// each, without repeats and only from that family.
pub(super) fn family_faces(fc: &Fontconfig, family: &str) -> Vec<Face> {
    let mut faces: Vec<Face> = Vec::new();
    for (weight, slant) in STYLES {
        if let Some(found) = match_family(fc, family, weight, slant)
            && same(&found.family, family)
            && !faces.contains(&found)
        {
            faces.push(found);
        }
    }
    faces
}

/// The fontconfig language of the process locale, as Chrome derives its
/// locale on Linux: the first set of `LC_ALL`, `LC_MESSAGES` and `LANG`,
/// without encoding or modifier, `_` as `-`; `C` and `POSIX` are `en-US`.
pub(super) fn locale() -> Option<CString> {
    locale_of(
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .map(|name| std::env::var(name).ok()),
    )
}

/// [`locale`] of these variable values, in that order.
fn locale_of(values: impl IntoIterator<Item = Option<String>>) -> Option<CString> {
    let value = values
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())?;
    let base = value.split(['.', '@']).next().unwrap_or_default();
    let language = match base {
        "" => return None,
        "C" | "POSIX" => "en-US".to_owned(),
        tag => tag.replace('_', "-"),
    };
    CString::new(language).ok()
}

/// The character fallback (Chrome's `gfx::GetFallbackFontForChar`).
pub(crate) mod fallback {
    use super::super::fallback::{COLOR_TABLES, FaceData, Found, check_tables};
    use super::{CharSet, FC_LANG, MAX_CANDIDATES, Pattern, UnicodeCoverage, face, scalable};
    use skrifa::{FontRef, MetadataProvider};
    use std::ffi::CString;
    use varta_view::{FallbackRefusal, FallbackRequest, MAX_FALLBACK_FACE_BYTES};

    /// The largest font file read to take one fallback face from it (a
    /// collection holds several faces).
    const MAX_FALLBACK_FILE_BYTES: usize = 4 * MAX_FALLBACK_FACE_BYTES;

    pub(crate) struct Cascade {
        pub locale: Option<CString>,
    }

    impl Cascade {
        pub(in super::super) fn face(
            &self,
            request: &FallbackRequest<'_>,
            _bold: bool,
            _italic: bool,
        ) -> Result<Option<Found>, FallbackRefusal> {
            let character = request.character;
            let fc = super::open().ok_or(FallbackRefusal::Unreadable)?;
            let fonts = (|| {
                let mut pattern = Pattern::new(&fc).ok()?;
                let mut set = CharSet::new(&fc).ok()?;
                set.add_char(character).ok()?;
                pattern.add_charset(set).ok()?;
                if let Some(locale) = &self.locale {
                    pattern.add_string(FC_LANG, locale).ok()?;
                }
                pattern.sort_fonts(UnicodeCoverage::NoTrim).ok()
            })()
            .ok_or(FallbackRefusal::Unreadable)?;
            for font in fonts.iter().take(MAX_CANDIDATES).filter(scalable) {
                let Some(found) = face(&font) else {
                    continue;
                };
                let Ok(file) = varta_artifact::read_bounded(&found.path, MAX_FALLBACK_FILE_BYTES)
                else {
                    continue;
                };
                let Some(member) = crate::sfnt::Face::parse(&file, found.index) else {
                    continue;
                };
                let Ok(parsed) = FontRef::from_index(&file, found.index) else {
                    continue;
                };
                if parsed.charmap().map(character).is_none() {
                    continue;
                }
                check_tables(COLOR_TABLES.iter().filter(|tag| member.has_table(**tag)))?;
                let data = if member.is_standalone() {
                    file
                } else {
                    member.standalone().ok_or(FallbackRefusal::Unreadable)?
                };
                if data.len() > MAX_FALLBACK_FACE_BYTES {
                    return Err(FallbackRefusal::Limit);
                }
                return Ok(Some(Found {
                    key: format!("{}\0{}", found.path.display(), found.index),
                    data: Box::new(move || {
                        Ok(FaceData {
                            bytes: data,
                            platform: None,
                            metrics: None,
                        })
                    }),
                }));
            }
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_view::{FallbackRefusal, FallbackRequest};

    #[test]
    fn substitutes_are_accepted_as_skia_accepts_them() {
        // Metric-compatible, configured first, requested, or a generic.
        assert!(acceptable("Arial", "Arial", "Liberation Sans"));
        assert!(acceptable("Times New Roman", "Times New Roman", "Tinos"));
        assert!(acceptable("Arial", "Helvetica", "Helvetica"));
        assert!(acceptable(
            "Bitstream Vera Sans",
            "Arial",
            "Bitstream Vera Sans"
        ));
        assert!(acceptable("Sans", "DejaVu Sans", "Noto Sans"));
        assert!(acceptable(
            "monospace",
            "DejaVu Sans Mono",
            "Liberation Mono"
        ));
        // A default face for a family fontconfig does not know.
        assert!(!acceptable("Arial", "Arial", "DejaVu Sans"));
        assert!(!acceptable("Monaco", "Monaco", "Times New Roman"));
    }

    #[test]
    fn the_locale_follows_the_first_set_variable() {
        let locale = |values: [Option<&str>; 3]| {
            locale_of(values.map(|value| value.map(str::to_owned)))
                .map(|value| value.into_string().unwrap())
        };
        assert_eq!(
            locale([None, None, Some("uk_UA.UTF-8")]).as_deref(),
            Some("uk-UA")
        );
        assert_eq!(
            locale([Some("de_DE@euro"), None, Some("uk_UA.UTF-8")]).as_deref(),
            Some("de-DE")
        );
        assert_eq!(
            locale([Some(""), Some("C"), None]).as_deref(),
            Some("en-US")
        );
        assert_eq!(locale([None, None, None]), None);
    }

    fn request(character: char) -> FallbackRequest<'static> {
        FallbackRequest {
            character,
            family: "sans-serif",
            weight: 400.0,
            italic: false,
            emoji: false,
        }
    }

    #[test]
    fn the_fallback_face_covers_its_character() {
        // Whatever the process locale, these faces cover the characters.
        let cascade = fallback::Cascade { locale: locale() };
        for character in ['\u{2192}', 'A'] {
            let found = cascade
                .face(&request(character), false, false)
                .unwrap()
                .expect("an installed face covers the character");
            let data = (found.data)().unwrap().bytes;
            assert!(
                skrifa::MetadataProvider::charmap(&skrifa::FontRef::new(&data).unwrap())
                    .map(character)
                    .is_some(),
                "{character}"
            );
        }
        // A noncharacter no face maps keeps `.notdef`.
        assert!(matches!(
            cascade.face(&request('\u{FFFF}'), false, false),
            Ok(None) | Err(FallbackRefusal::Color)
        ));
    }
}
