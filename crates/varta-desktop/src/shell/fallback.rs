//! The platform's fallback cascade for `--system-fonts`: the face the
//! platform's browser draws a character with when the element's fonts lack
//! it ([`varta_view::FontFallback`]).
//!
//! - **macOS**, as Chrome's `FontCache::PlatformFallbackFontForCharacter`:
//!   CoreText's `CTFontCreateForStringWithLanguage` for the element's primary
//!   font (the UI font for the family `system-ui` maps, otherwise the named
//!   family), with the bold and italic traits the CSS style asks for, then
//!   the face of that family with those traits. The face is the platform
//!   font's own tables assembled as a standalone face; no file is read.
//! - **Linux**, as Chrome's `gfx::GetFallbackFontForChar`: fontconfig's
//!   `FcFontSort` for a pattern of the character and the locale, the first
//!   scalable face whose character map has it. Like Chrome, the face is the
//!   one fontconfig names, whatever the weight (bold or italic text on a
//!   regular fallback face then needs synthesis, which native paint refuses).
//!
//! Faces drawing colour glyphs and faces past the fallback limits are
//! refused, so such text fails instead of drawing another face. Answers and
//! faces are cached and shared by every measurer of one font set.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use varta_view::{FallbackFace, FallbackRefusal, FallbackRequest, FontFallback};

/// Bitmap and colour glyph tables: the native glyph path paints outlines.
pub(crate) const COLOR_TABLES: [[u8; 4]; 5] = [*b"COLR", *b"CBDT", *b"EBDT", *b"sbix", *b"SVG "];
/// The CSS weight from which a face's bold trait is asked for (Chrome's
/// bold threshold).
const BOLD_WEIGHT: f32 = 600.0;
/// Cached answers per font set; past this, answers are recomputed.
const MAX_CACHED_ANSWERS: usize = 4096;

/// One cached question: character, primary family, bold, italic.
type Question = (char, String, bool, bool);
type Answer = Result<Option<FallbackFace>, FallbackRefusal>;

#[derive(Default)]
struct Cache {
    answers: BTreeMap<Question, Answer>,
    faces: BTreeMap<String, Result<Arc<[u8]>, FallbackRefusal>>,
}

/// The platform cascade of one system font set; clones share its cache.
#[derive(Clone)]
pub(crate) struct SystemFallback {
    cascade: Arc<platform::Cascade>,
    cache: Arc<Mutex<Cache>>,
}

impl std::fmt::Debug for SystemFallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SystemFallback")
            .finish_non_exhaustive()
    }
}

impl SystemFallback {
    pub(crate) fn new(cascade: platform::Cascade) -> Self {
        Self {
            cascade: Arc::new(cascade),
            cache: Arc::default(),
        }
    }

    fn resolve(&self, request: &FallbackRequest<'_>, bold: bool, italic: bool) -> Answer {
        let Some(found) = self.cascade.face(request, bold, italic)? else {
            return Ok(None);
        };
        let mut cache = self.cache.lock().map_err(|_| FallbackRefusal::Unreadable)?;
        let data = match cache.faces.get(&found.key) {
            Some(data) => data.clone(),
            None => {
                let data = (found.data)().map(Arc::from);
                cache.faces.insert(found.key.clone(), data.clone());
                data
            }
        }?;
        Ok(Some(FallbackFace {
            key: found.key,
            data,
        }))
    }
}

impl FontFallback for SystemFallback {
    fn face(&mut self, request: &FallbackRequest<'_>) -> Answer {
        let bold = request.weight >= BOLD_WEIGHT;
        let question = (
            request.character,
            request.family.to_owned(),
            bold,
            request.italic,
        );
        if let Some(answer) = self
            .cache
            .lock()
            .map_err(|_| FallbackRefusal::Unreadable)?
            .answers
            .get(&question)
        {
            return answer.clone();
        }
        let answer = self.resolve(request, bold, request.italic);
        let mut cache = self.cache.lock().map_err(|_| FallbackRefusal::Unreadable)?;
        if cache.answers.len() < MAX_CACHED_ANSWERS {
            cache.answers.insert(question, answer.clone());
        }
        answer
    }
}

/// A face the platform named, before its bytes are read.
pub(crate) struct Found {
    /// Its identity: PostScript name or file and index.
    pub(crate) key: String,
    /// Reads the face as a standalone font file (only for a face not cached).
    pub(crate) data: Box<dyn FnOnce() -> Result<Vec<u8>, FallbackRefusal>>,
}

/// Refuses colour glyph tables.
pub(crate) fn check_tables<'a>(
    mut tags: impl Iterator<Item = &'a [u8; 4]>,
) -> Result<(), FallbackRefusal> {
    if tags.any(|tag| COLOR_TABLES.contains(tag)) {
        return Err(FallbackRefusal::Color);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) mod platform {
    use super::{FallbackRefusal, Found, check_tables};
    use core_text::{
        font::{self, CTFont, kCTFontSystemFontType},
        font_descriptor::{kCTFontBoldTrait, kCTFontItalicTrait},
    };
    use varta_view::{FallbackRequest, MAX_FALLBACK_FACE_BYTES};

    /// The point size of the fonts CoreText resolves with; the face is the
    /// same at every size.
    const POINT_SIZE: f64 = 16.0;
    /// CoreText's face for a code point no installed face covers.
    const LAST_RESORT: &str = "LastResort";

    /// The registered families the UI font stands for: their primary font is
    /// CoreText's UI font, which is not reachable by name.
    pub(crate) struct Cascade {
        pub ui_families: Vec<String>,
    }

    /// `font` with exactly the asked bold and italic traits when its family
    /// has such a face, otherwise `font` itself.
    fn with_traits(font: CTFont, bold: bool, italic: bool) -> CTFont {
        let mask = kCTFontBoldTrait | kCTFontItalicTrait;
        let value =
            if bold { kCTFontBoldTrait } else { 0 } | if italic { kCTFontItalicTrait } else { 0 };
        if font.symbolic_traits() & mask == value {
            return font;
        }
        font.clone_with_symbolic_traits(value, mask).unwrap_or(font)
    }

    impl Cascade {
        pub(super) fn face(
            &self,
            request: &FallbackRequest<'_>,
            bold: bool,
            italic: bool,
        ) -> Result<Option<Found>, FallbackRefusal> {
            let primary = if self
                .ui_families
                .iter()
                .any(|family| family == request.family)
            {
                font::new_ui_font_for_language(kCTFontSystemFontType, POINT_SIZE, None)
            } else {
                font::new_from_name(request.family, POINT_SIZE)
                    .map_err(|()| FallbackRefusal::Unreadable)?
            };
            let primary = with_traits(primary, bold, italic);
            let mut buffer = [0u8; 4];
            let text = request.character.encode_utf8(&mut buffer);
            let fallback = primary.create_for_string_with_language(text, None);
            // The primary itself, or CoreText's last resort: no installed face
            // draws the text, so it keeps `.notdef` as in the browser.
            if fallback.postscript_name() == primary.postscript_name()
                || fallback.family_name() == LAST_RESORT
            {
                return Ok(None);
            }
            let font = with_traits(fallback, bold, italic);
            let file = font
                .url()
                .and_then(|url| url.to_path())
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            Ok(Some(Found {
                key: format!("{}\0{file}", font.postscript_name()),
                data: Box::new(move || data(&font)),
            }))
        }
    }

    /// The face's own tables as a standalone face.
    fn data(font: &CTFont) -> Result<Vec<u8>, FallbackRefusal> {
        let tags = font
            .get_available_font_tables()
            .ok_or(FallbackRefusal::Unreadable)?;
        let tags: Vec<[u8; 4]> = tags.iter().map(|tag| (*tag).to_be_bytes()).collect();
        check_tables(tags.iter())?;
        let mut tables = Vec::with_capacity(tags.len());
        let mut lengths = Vec::with_capacity(tags.len());
        for tag in tags {
            let table = font
                .get_font_table(u32::from_be_bytes(tag))
                .ok_or(FallbackRefusal::Unreadable)?;
            lengths.push(table.bytes().len());
            let size =
                crate::sfnt::tables_len(lengths.iter().copied()).ok_or(FallbackRefusal::Limit)?;
            if size > MAX_FALLBACK_FACE_BYTES {
                return Err(FallbackRefusal::Limit);
            }
            tables.push((tag, table.bytes().to_vec()));
        }
        crate::sfnt::from_tables(tables).ok_or(FallbackRefusal::Unreadable)
    }
}

#[cfg(target_os = "linux")]
pub(crate) use super::fontconfig::fallback as platform;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) mod platform {
    //! No platform cascade: `--system-fonts` is refused before a set exists.
    use super::{FallbackRefusal, Found};
    use varta_view::FallbackRequest;

    pub(crate) struct Cascade;

    impl Cascade {
        pub(super) fn face(
            &self,
            _: &FallbackRequest<'_>,
            _: bool,
            _: bool,
        ) -> Result<Option<Found>, FallbackRefusal> {
            Ok(None)
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::shell::FontSet;

    fn cascade() -> SystemFallback {
        SystemFallback::new(platform::Cascade {
            ui_families: Vec::new(),
        })
    }

    fn request<'a>(text: &str, family: &'a str, weight: f32) -> FallbackRequest<'a> {
        FallbackRequest {
            character: text.chars().next().unwrap(),
            family,
            weight,
            italic: false,
        }
    }

    fn postscript(answer: &Answer) -> &str {
        let face = answer.as_ref().unwrap().as_ref().unwrap();
        face.key.split('\0').next().unwrap()
    }

    /// Chrome 156 on macOS 27 (`CSS.getPlatformFontsForNode`) draws these
    /// with these faces; CoreText's fallback names the same.
    #[test]
    fn coretext_names_the_faces_chrome_draws_with() {
        let mut fallback = cascade();
        for (text, family, weight, face) in [
            ("\u{2699}", "Helvetica", 400.0, "Menlo-Regular"),
            ("\u{2699}", "Helvetica", 700.0, "Menlo-Bold"),
            ("\u{2699}", "Times", 400.0, "Menlo-Regular"),
            ("\u{2192}", "Helvetica", 400.0, "LucidaGrande"),
            ("\u{2192}", "Helvetica", 700.0, "LucidaGrande-Bold"),
            ("\u{2192}", "Times", 400.0, "TimesNewRomanPSMT"),
            ("\u{2713}", "Times", 400.0, "ZapfDingbatsITC"),
            ("\u{2605}", "Helvetica", 400.0, "HiraginoSans-W3"),
            ("\u{2605}", "Helvetica", 700.0, "HiraginoSans-W6"),
        ] {
            let answer = fallback.face(&request(text, family, weight));
            assert_eq!(postscript(&answer), face, "{text} in {family} {weight}");
            let data = &answer.unwrap().unwrap().data;
            assert!(
                !varta_view::TextMeasurer::system_font_families(data.to_vec())
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn covered_text_has_no_fallback_and_colour_faces_are_refused() {
        let mut fallback = cascade();
        assert!(matches!(
            fallback.face(&request("A", "Helvetica", 400.0)),
            Ok(None)
        ));
        assert_eq!(
            fallback
                .face(&request("\u{1F600}", "Helvetica", 400.0))
                .unwrap_err(),
            FallbackRefusal::Color
        );
    }

    #[test]
    fn a_system_set_draws_the_gear_with_the_platform_face() {
        let fonts = FontSet::system().unwrap();
        let measure = |measurer: &mut varta_view::TextMeasurer, family: &str| {
            measurer
                .measure_with_family_list("\u{2699}", 16.0, None, family)
                .unwrap()
                .width
        };
        // The UI font lacks U+2699; CoreText (and Chrome) draw it with Menlo,
        // which the set already loads for `monospace`.
        let mut measurer = fonts.measurer().unwrap();
        let ui = measure(&mut measurer, "system-ui, sans-serif");
        assert_eq!(ui, measure(&mut measurer, "monospace"));
        let mut plain = fonts.measurer().unwrap();
        plain.set_fallback(None);
        assert_ne!(measure(&mut plain, "system-ui, sans-serif"), ui);
    }
}
