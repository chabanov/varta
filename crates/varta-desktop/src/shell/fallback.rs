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
//! Faces past the fallback limits are refused, so such text fails instead of
//! drawing another face. On macOS a face whose glyphs are `sbix` colour
//! bitmaps (Apple Color Emoji) is shaped from its own tables with the bitmap
//! strikes left out, and the platform face draws its glyphs
//! ([`PlatformFaces`]), as Chrome draws them through CoreText. Other colour
//! glyph formats, and colour faces on other platforms, are refused. Answers
//! and faces are cached and shared by every measurer of one font set.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use varta_view::{FallbackFace, FallbackRefusal, FallbackRequest, FontFallback, PlatformMetrics};

/// Bitmap and colour glyph tables: the native glyph path paints outlines.
pub(crate) const COLOR_TABLES: [[u8; 4]; 5] = [*b"COLR", *b"CBDT", *b"EBDT", *b"sbix", *b"SVG "];
/// The colour bitmap table whose strikes the platform face draws.
#[cfg(target_os = "macos")]
const PLATFORM_DRAWN_TABLE: [u8; 4] = *b"sbix";
/// An `sbix` table with no strikes: version 1, flags with only the required
/// bit 0, zero strikes. The shaping face keeps it, so the face still says its
/// glyphs are colour bitmaps (the portable outline path refuses them).
#[cfg(target_os = "macos")]
const EMPTY_SBIX: [u8; 8] = [0, 1, 0, 1, 0, 0, 0, 0];

/// The platform's face object for a face it draws itself.
#[cfg(target_os = "macos")]
pub(crate) type PlatformFont = core_text::font::CTFont;
/// No platform draws faces itself.
#[cfg(not(target_os = "macos"))]
pub(crate) type PlatformFont = std::convert::Infallible;

/// The fallback faces whose shaping bytes leave their glyph images out, with
/// the platform face that draws them. The glyph rasterizer of a system set
/// finds a face here only by its exact shaping bytes, so no other face (a
/// packaged one included) is drawn by a platform face.
/// One registered face: its shaping bytes and platform face.
type PlatformFace = (Arc<[u8]>, PlatformFont);

#[derive(Clone, Default)]
pub(crate) struct PlatformFaces {
    faces: Arc<Mutex<Vec<PlatformFace>>>,
}

impl PlatformFaces {
    /// Keeps `font` for the face with these shaping bytes; past the fallback
    /// face limit the face is not kept, and its glyphs are not drawn.
    fn register(&self, bytes: Arc<[u8]>, font: PlatformFont) {
        if let Ok(mut faces) = self.faces.lock()
            && faces.len() < varta_view::MAX_FALLBACK_FACES
            && !faces.iter().any(|(known, _)| **known == *bytes)
        {
            faces.push((bytes, font));
        }
    }

    /// The platform face for exactly these shaping bytes.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn find(&self, data: &[u8]) -> Option<PlatformFont> {
        let faces = self.faces.lock().ok()?;
        faces
            .iter()
            .find(|(bytes, _)| **bytes == *data)
            .map(|(_, font)| font.clone())
    }
}

/// A fallback face's shaping bytes and, when they leave its glyph images
/// out, the platform face that draws them and the metrics it lays them out
/// with.
pub(crate) struct FaceData {
    pub(crate) bytes: Vec<u8>,
    pub(crate) platform: Option<PlatformFont>,
    pub(crate) metrics: Option<Arc<dyn PlatformMetrics>>,
}
/// The CSS weight from which a face's bold trait is asked for (Chrome's
/// bold threshold).
const BOLD_WEIGHT: f32 = 600.0;
/// Cached answers per font set; past this, answers are recomputed.
const MAX_CACHED_ANSWERS: usize = 4096;

/// One cached question: character, primary family, bold, italic, emoji.
type Question = (char, String, bool, bool, bool);
type Answer = Result<Option<FallbackFace>, FallbackRefusal>;

/// A face's shaping bytes and its platform metrics.
type Face = (Arc<[u8]>, Option<Arc<dyn PlatformMetrics>>);

#[derive(Default)]
struct Cache {
    answers: BTreeMap<Question, Answer>,
    faces: BTreeMap<String, Result<Face, FallbackRefusal>>,
}

/// The platform cascade of one system font set; clones share its cache.
#[derive(Clone)]
pub(crate) struct SystemFallback {
    cascade: Arc<platform::Cascade>,
    cache: Arc<Mutex<Cache>>,
    platform_faces: PlatformFaces,
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
            platform_faces: PlatformFaces::default(),
        }
    }

    /// The faces this cascade's platform draws itself.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn platform_faces(&self) -> PlatformFaces {
        self.platform_faces.clone()
    }

    fn resolve(&self, request: &FallbackRequest<'_>, bold: bool, italic: bool) -> Answer {
        let Some(found) = self.cascade.face(request, bold, italic)? else {
            return Ok(None);
        };
        let mut cache = self.cache.lock().map_err(|_| FallbackRefusal::Unreadable)?;
        let data = match cache.faces.get(&found.key) {
            Some(data) => data.clone(),
            None => {
                let data = (found.data)().map(|face| {
                    let bytes = Arc::<[u8]>::from(face.bytes);
                    if let Some(font) = face.platform {
                        self.platform_faces.register(bytes.clone(), font);
                    }
                    (bytes, face.metrics)
                });
                cache.faces.insert(found.key.clone(), data.clone());
                data
            }
        }?;
        Ok(Some(FallbackFace {
            key: found.key,
            data: data.0,
            metrics: data.1,
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
            request.emoji,
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
    pub(crate) data: Box<dyn FnOnce() -> Result<FaceData, FallbackRefusal>>,
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
    use super::{
        EMPTY_SBIX, FaceData, FallbackRefusal, Found, PLATFORM_DRAWN_TABLE, PlatformMetrics,
        check_tables,
    };
    use core_text::{
        font::{self, CTFont, kCTFontSystemFontType},
        font_descriptor::{kCTFontBoldTrait, kCTFontItalicTrait},
    };
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };
    use varta_view::{FallbackRequest, MAX_FALLBACK_FACE_BYTES};

    /// The point size of the fonts CoreText resolves with; the face is the
    /// same at every size.
    const POINT_SIZE: f64 = 16.0;
    /// CoreText's face for a code point no installed face covers.
    const LAST_RESORT: &str = "LastResort";
    /// The colour emoji face Chrome draws emoji presentation with
    /// (`kColorEmojiFontMac`).
    const EMOJI_FACE: &str = "AppleColorEmoji";
    /// Font sizes whose CoreText metrics one face keeps.
    const MAX_CACHED_SIZES: usize = 64;

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
            // Emoji presentation: Chrome's colour emoji face, whatever
            // CoreText's cascade for the character.
            let fallback = if request.emoji {
                font::new_from_name(EMOJI_FACE, POINT_SIZE)
                    .map_err(|()| FallbackRefusal::Unreadable)?
            } else {
                primary.create_for_string_with_language(text, None)
            };
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

    /// The face's own tables as a standalone face. An `sbix` face's strikes
    /// are left out (an empty `sbix` stands for them) and the face itself
    /// draws its glyphs; other colour tables are refused.
    fn data(font: &CTFont) -> Result<FaceData, FallbackRefusal> {
        let tags = font
            .get_available_font_tables()
            .ok_or(FallbackRefusal::Unreadable)?;
        let tags: Vec<[u8; 4]> = tags.iter().map(|tag| (*tag).to_be_bytes()).collect();
        let platform_drawn = tags.contains(&PLATFORM_DRAWN_TABLE);
        check_tables(tags.iter().filter(|tag| **tag != PLATFORM_DRAWN_TABLE))?;
        let mut tables = Vec::with_capacity(tags.len());
        let mut lengths = Vec::with_capacity(tags.len());
        for tag in tags {
            let bytes = if tag == PLATFORM_DRAWN_TABLE {
                EMPTY_SBIX.to_vec()
            } else {
                font.get_font_table(u32::from_be_bytes(tag))
                    .ok_or(FallbackRefusal::Unreadable)?
                    .bytes()
                    .to_vec()
            };
            lengths.push(bytes.len());
            let size =
                crate::sfnt::tables_len(lengths.iter().copied()).ok_or(FallbackRefusal::Limit)?;
            if size > MAX_FALLBACK_FACE_BYTES {
                return Err(FallbackRefusal::Limit);
            }
            tables.push((tag, bytes));
        }
        let bytes = crate::sfnt::from_tables(tables).ok_or(FallbackRefusal::Unreadable)?;
        if !platform_drawn {
            return Ok(FaceData {
                bytes,
                platform: None,
                metrics: None,
            });
        }
        let metrics = CoreTextMetrics::new(font, &bytes).ok_or(FallbackRefusal::Unreadable)?;
        Ok(FaceData {
            bytes,
            platform: Some(font.clone()),
            metrics: Some(Arc::new(metrics)),
        })
    }

    /// How Chrome lays out a face CoreText draws itself. CoreText draws
    /// Apple Color Emoji larger than its em at small sizes (by a quarter up
    /// to 16 px, by nothing from 24 px) and reports that size's ascent and
    /// descent; Chrome lays the face out at that size, with each glyph's own
    /// (`hmtx`) advance scaled by the same enlargement and rounded to whole
    /// pixels, as it advances colour glyphs. The enlargement at a size is
    /// CoreText's ascent there over the face's own (`hhea`) ascent at that
    /// size.
    #[derive(Debug)]
    pub(crate) struct CoreTextMetrics {
        font: CTFont,
        /// The face's own ascent per em.
        reference_ascent: f64,
        advances: crate::sfnt::Advances,
        /// Ascent and descent by size (the bits of an `f32`).
        sizes: Mutex<BTreeMap<u32, (f64, f64)>>,
    }

    impl CoreTextMetrics {
        fn new(font: &CTFont, bytes: &[u8]) -> Option<Self> {
            let face = crate::sfnt::Face::parse(bytes, 0)?;
            let advances = crate::sfnt::Advances::parse(&face)?;
            let reference_ascent =
                f64::from(advances.ascender()) / f64::from(advances.units_per_em());
            (reference_ascent.is_finite() && reference_ascent > 0.0).then(|| Self {
                font: font.clone(),
                reference_ascent,
                advances,
                sizes: Mutex::default(),
            })
        }

        /// CoreText's ascent and descent at `size`.
        fn at(&self, size: f32) -> Option<(f64, f64)> {
            if !size.is_finite() || size <= 0.0 {
                return None;
            }
            let mut sizes = self.sizes.lock().ok()?;
            if let Some(found) = sizes.get(&size.to_bits()) {
                return Some(*found);
            }
            let sized = self.font.clone_with_font_size(f64::from(size));
            let found = (sized.ascent(), sized.descent());
            if sizes.len() < MAX_CACHED_SIZES {
                sizes.insert(size.to_bits(), found);
            }
            Some(found)
        }
    }

    impl PlatformMetrics for CoreTextMetrics {
        fn advance(&self, glyph: u32, size: f32) -> Option<f32> {
            let units = f64::from(self.advances.advance(glyph)?);
            let (ascent, _) = self.at(size)?;
            // The em at `size` enlarged: `size` times the enlargement.
            let em = ascent / self.reference_ascent;
            let advance = (units / f64::from(self.advances.units_per_em()) * em).round();
            advance.is_finite().then_some(advance as f32)
        }

        fn vertical(&self, size: f32) -> Option<(f32, f32)> {
            let (ascent, descent) = self.at(size)?;
            Some((ascent as f32, descent as f32))
        }
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
            emoji: false,
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
    fn covered_text_has_no_fallback() {
        let mut fallback = cascade();
        assert!(matches!(
            fallback.face(&request("A", "Helvetica", 400.0)),
            Ok(None)
        ));
    }

    /// Apple Color Emoji shapes from its own tables without its `sbix`
    /// strikes (about 1 MiB of 183 MiB) and is drawn by the platform face
    /// registered for exactly those bytes.
    #[test]
    fn emoji_faces_shape_without_strikes_and_register_their_platform_face() {
        let mut fallback = cascade();
        let face = fallback
            .face(&request("\u{1F600}", "Helvetica", 400.0))
            .unwrap()
            .unwrap();
        assert_eq!(postscript(&Ok(Some(face.clone()))), "AppleColorEmoji");
        assert!(face.data.len() < varta_view::MAX_FALLBACK_FACE_BYTES / 8);
        let parsed = crate::sfnt::Face::parse(&face.data, 0).unwrap();
        assert_eq!(parsed.table(*b"sbix"), Some(&EMPTY_SBIX[..]));
        assert!(face.metrics.is_some());
        assert!(fallback.platform_faces().find(&face.data).is_some());
        assert!(fallback.platform_faces().find(&face.data[1..]).is_none());
        // Emoji presentation takes the emoji face even for a character the
        // primary font has (Chrome draws `©\u{FE0F}` with it).
        let copyright = FallbackRequest {
            emoji: true,
            ..request("\u{A9}", "Helvetica", 400.0)
        };
        assert_eq!(postscript(&fallback.face(&copyright)), "AppleColorEmoji");
        let text = request("\u{A9}", "Helvetica", 400.0);
        assert!(matches!(fallback.face(&text), Ok(None)));
    }

    /// Chrome 156 on macOS 27 lays one emoji out this wide (and a Helvetica
    /// line with one this tall) at these sizes: CoreText's enlargement of
    /// small emoji, with rounded advances.
    #[test]
    fn emoji_advances_and_line_heights_are_chromes() {
        let fonts = FontSet::system().unwrap();
        let mut measurer = fonts.measurer().unwrap();
        for (size, advance, height) in [
            (8.0, 10.0, None),
            (12.0, 15.0, Some(20.0)),
            (13.3, 17.0, None),
            (16.0, 20.0, Some(26.0)),
            (17.0, 21.0, None),
            (18.7, 21.0, None),
            (20.0, 22.0, Some(29.0)),
            (24.0, 24.0, Some(32.0)),
            (30.0, 30.0, None),
        ] {
            let emoji = measurer
                .measure_with_family_list("\u{1F600}\u{1F600}", size, None, "Helvetica")
                .unwrap();
            assert_eq!(emoji.width, 2.0 * advance, "{size}");
            if let Some(height) = height {
                let line = measurer
                    .measure_with_family_list("a\u{1F600}", size, None, "Helvetica")
                    .unwrap();
                assert_eq!(line.height, height, "{size}");
            }
        }
    }

    /// An emoji draws in colour with the set's rasterizer; a rasterizer
    /// without the set's platform faces cannot draw the strikeless face, and
    /// the portable path refuses it.
    #[test]
    fn emoji_draw_in_colour_only_with_the_platform_face() {
        use varta_common::view::{CssProperty as P, Tag};
        let fonts = FontSet::system().unwrap();
        let mut batch = varta_view::BatchWriter::new();
        batch
            .create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::FontFamily, "Helvetica")
            .set_style(2, P::FontSize, "16px")
            .create_text(3, "\u{1F600}")
            .insert(2, 3, 0);
        let mut view = varta_view::View::default();
        view.commit(&batch.finish(), false, &mut Denied).unwrap();
        let render = |raster: crate::text_raster::CoreTextRasterizer| {
            let mut renderer =
                varta_view::Renderer::new(Some(fonts.measurer().unwrap()), 16.0).unwrap();
            renderer.set_glyph_rasterizer(Some(Box::new(raster)));
            renderer.render(&view, 40, 40, 1.0)
        };
        let image = render(fonts.glyph_rasterizer()).unwrap();
        let colourful = image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 200 && pixel[1] > 150 && pixel[2] < 80)
            .count();
        // The face is yellow: a good part of its 20x20 pixels.
        assert!(colourful > 50, "{colourful}");
        assert!(render(crate::text_raster::CoreTextRasterizer::default()).is_err());
    }

    struct Denied;
    impl varta_view::Broker for Denied {
        fn link_requested(&self, _: &str) -> bool {
            false
        }
        fn open_url(&mut self, _: &str) -> varta_view::Access {
            varta_view::Access::Denied
        }
        fn clipboard_write(&mut self) -> varta_view::Access {
            varta_view::Access::Denied
        }
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
