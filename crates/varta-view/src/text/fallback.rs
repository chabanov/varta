//! Platform fallback faces for characters the resolved CSS families lack.
//!
//! The view never looks up a font itself: a native host that runs with the
//! platform's own fonts (`varta-shell --system-fonts`) supplies a
//! [`FontFallback`] that names the face its platform's text system draws a
//! character with when the element's fonts lack it, exactly as
//! the platform's browser asks it (CoreText's
//! fallback for the element's primary font on macOS, fontconfig's sorted
//! character match on Linux). The shaper first shapes with the CSS families;
//! as Blink does, each range of adjacent clusters of one style that still
//! draw the `.notdef` glyph asks the host for its first character, and the
//! range is shaped again with that face's family after its CSS list (a
//! cluster the face lacks asks again in the next pass), so a covered
//! character never changes font.
//! Without a provider (packaged or explicit fonts), nothing is looked up and
//! such a cluster keeps `.notdef`.
//!
//! As in Blink's emoji segmentation, a grapheme with emoji presentation
//! (Unicode TR51: an `Emoji_Presentation` character, an emoji followed by
//! VS16, a modifier, keycap, flag or ZWJ sequence) drawn with a face that has
//! no colour glyphs asks the host for its emoji face too
//! ([`FallbackRequest::emoji`]), and that face comes before the CSS families
//! for it: Chrome draws `©\u{FE0F}` with the emoji face even where the CSS
//! font has `©`.
use std::{collections::BTreeMap, ops::Range, sync::Arc};

/// At most this many distinct fallback faces one measurer registers.
pub const MAX_FALLBACK_FACES: usize = 16;
/// The largest single fallback face (one face's tables, not its file).
pub const MAX_FALLBACK_FACE_BYTES: usize = 32 * 1024 * 1024;
/// The largest total of fallback face bytes one measurer registers.
pub const MAX_FALLBACK_BYTES: usize = 64 * 1024 * 1024;
/// Shaping passes after the first that may add fallback faces: each pass
/// gives the remaining `.notdef` ranges of a style the face of their first
/// character (Blink's fallback loop), and every pass is a full shaping
/// charged to the view's shaping budget. Ranges still missing after the last
/// pass keep `.notdef`.
pub const MAX_FALLBACK_PASSES: usize = 8;

/// What a host resolves: the first character of a range none of the CSS
/// families covers (Blink's hint character), and the style of its element.
#[derive(Debug, Clone, Copy)]
pub struct FallbackRequest<'a> {
    /// The range's first character.
    pub character: char,
    /// The element's primary family: the first resolved family of its CSS
    /// list, as registered with the measurer.
    pub family: &'a str,
    /// The CSS font weight (1 to 1000).
    pub weight: f32,
    /// Whether the CSS font style is italic or oblique.
    pub italic: bool,
    /// Whether the range presents as emoji (Blink's emoji fallback
    /// priority): the platform's colour emoji face draws it.
    pub emoji: bool,
}

/// One face the platform draws a cluster with.
#[derive(Debug, Clone)]
pub struct FallbackFace {
    /// A stable identity of the face (for example its PostScript name and
    /// file): the measurer registers each identity once.
    pub key: String,
    /// The face as a standalone font file.
    pub data: Arc<[u8]>,
    /// The metrics the platform lays this face out with, when they are not
    /// its tables' (`None`: the tables').
    pub metrics: Option<Arc<dyn PlatformMetrics>>,
}

/// Size-dependent metrics a platform lays a fallback face out with instead
/// of its tables' linear ones, as the platform's browser does: CoreText
/// draws Apple Color Emoji larger than its em at small sizes, and Chrome lays
/// it out at that size. Both are in pixels at the run's font size.
pub trait PlatformMetrics: Send + Sync + std::fmt::Debug {
    /// The advance of `glyph`; `None` keeps its shaped advance.
    fn advance(&self, glyph: u32, size: f32) -> Option<f32>;
    /// The ascent and descent (distances above and below the baseline);
    /// `None` keeps the tables'.
    fn vertical(&self, size: f32) -> Option<(f32, f32)>;
}

/// Why a host cannot supply the face its platform would draw a cluster with.
/// The text is then refused rather than drawn with another face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FallbackRefusal {
    /// The face draws colour glyphs in a format the host does not paint.
    #[error("the platform face draws colour glyphs")]
    Color,
    /// The face, or the total of fallback faces, exceeds the fallback limits.
    #[error("the platform face exceeds the fallback font limits")]
    Limit,
    /// The platform's face could not be read.
    #[error("the platform face could not be read")]
    Unreadable,
}

/// A host's platform fallback lookup. Implementations cache their answers:
/// the measurer asks again for every shaped text that needs a face.
pub trait FontFallback: Send {
    /// The face the platform draws `request.character` with when the element's
    /// fonts lack it: `Ok(None)` when the platform has none (the cluster
    /// keeps `.notdef`, as the browser draws it then).
    fn face(
        &mut self,
        request: &FallbackRequest<'_>,
    ) -> Result<Option<FallbackFace>, FallbackRefusal>;
}

/// The measurer's provider and the faces it registered.
pub(super) struct Fallbacks {
    pub provider: Box<dyn FontFallback>,
    /// Registered family by face key.
    pub registered: BTreeMap<String, String>,
    pub bytes: usize,
}

impl Fallbacks {
    pub fn new(provider: Box<dyn FontFallback>) -> Self {
        Self {
            provider,
            registered: BTreeMap::new(),
            bytes: 0,
        }
    }
    pub const fn passes() -> usize {
        MAX_FALLBACK_PASSES
    }
}

/// Zero width joiner, variation selectors 15 and 16, and the combining
/// enclosing keycap.
const ZWJ: char = '\u{200D}';
const TEXT_PRESENTATION: char = '\u{FE0E}';
const EMOJI_PRESENTATION: char = '\u{FE0F}';
const KEYCAP: char = '\u{20E3}';

/// Whether `grapheme` presents as emoji (Unicode TR51, as Blink's emoji
/// segmentation classifies it): VS15 after its first character asks for
/// text; VS16, a keycap, a regional-indicator pair, a modifier sequence, an
/// `Emoji_Presentation` first character or a ZWJ sequence of emoji asks for
/// emoji.
pub(super) fn emoji_presentation(grapheme: &str) -> bool {
    use icu_properties::{
        CodePointSetData,
        props::{Emoji, EmojiModifier, EmojiModifierBase, EmojiPresentation, RegionalIndicator},
    };
    let mut chars = grapheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let second = chars.next();
    let emoji = CodePointSetData::new::<Emoji>();
    if second == Some(TEXT_PRESENTATION) {
        return false;
    }
    let after = |c: char| grapheme.chars().skip(1).any(|other| other == c);
    second == Some(EMOJI_PRESENTATION)
        || (emoji.contains(first) && after(KEYCAP))
        || second.is_some_and(|second| {
            let regional = CodePointSetData::new::<RegionalIndicator>();
            (regional.contains(first) && regional.contains(second))
                || (CodePointSetData::new::<EmojiModifierBase>().contains(first)
                    && CodePointSetData::new::<EmojiModifier>().contains(second))
        })
        || CodePointSetData::new::<EmojiPresentation>().contains(first)
        || (emoji.contains(first)
            && grapheme
                .split(ZWJ)
                .skip(1)
                .any(|element| element.chars().next().is_some_and(|c| emoji.contains(c))))
}

/// Faces with colour glyph tables (`sbix`, `CBDT`, `COLR`, SVG).
const COLOR_GLYPH_TABLES: [skrifa::Tag; 4] = [
    skrifa::Tag::new(b"sbix"),
    skrifa::Tag::new(b"CBDT"),
    skrifa::Tag::new(b"COLR"),
    skrifa::Tag::new(b"SVG "),
];

/// The ranges of graphemes with emoji presentation that `layout` draws with
/// a face without colour glyphs, in text order (adjacent graphemes form one
/// range). Breaks the layout into one unbounded line, as
/// [`missing_clusters`].
pub(super) fn uncolored_emoji(layout: &mut parley::Layout<u32>, text: &str) -> Vec<Range<usize>> {
    use unicode_segmentation::UnicodeSegmentation;
    let graphemes: Vec<Range<usize>> = text
        .grapheme_indices(true)
        .filter(|(_, grapheme)| emoji_presentation(grapheme))
        .map(|(start, grapheme)| start..start + grapheme.len())
        .collect();
    if graphemes.is_empty() {
        return Vec::new();
    }
    layout.break_all_lines(None);
    let mut colored: BTreeMap<(u64, u32), bool> = BTreeMap::new();
    let mut uncolored: Vec<Range<usize>> = Vec::new();
    for line in layout.lines() {
        for run in line.runs() {
            let font = run.font();
            let key = (font.data.id(), font.index);
            let color = *colored.entry(key).or_insert_with(|| {
                skrifa::FontRef::from_index(font.data.data(), font.index).is_ok_and(|face| {
                    COLOR_GLYPH_TABLES
                        .iter()
                        .any(|tag| face.table_data(*tag).is_some())
                })
            });
            if color {
                continue;
            }
            for cluster in run.clusters() {
                let start = cluster.text_range().start;
                if let Some(grapheme) = graphemes
                    .iter()
                    .find(|grapheme| grapheme.start <= start && start < grapheme.end)
                {
                    uncolored.push(grapheme.clone());
                }
            }
        }
    }
    merge(uncolored)
}

/// Sorted, deduplicated ranges with adjacent ranges joined.
fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| range.start);
    ranges.dedup();
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if last.end >= range.start => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// The text ranges of clusters that draw glyph 0 (`.notdef`) in `layout`,
/// in text order, without line breaks and control characters (which draw
/// nothing); adjacent clusters form one range. Breaks the layout into one
/// unbounded line to read its clusters; callers break it again at their
/// width.
pub(super) fn missing_clusters(layout: &mut parley::Layout<u32>, text: &str) -> Vec<Range<usize>> {
    const NOTDEF: u32 = 0;
    layout.break_all_lines(None);
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for line in layout.lines() {
        for run in line.runs() {
            for cluster in run.clusters() {
                let range = cluster.text_range();
                if range.is_empty()
                    || cluster.is_hard_line_break()
                    || text
                        .get(range.clone())
                        .is_none_or(|value| value.chars().all(char::is_control))
                    || !cluster.glyphs().any(|glyph| glyph.id == NOTDEF)
                {
                    continue;
                }
                ranges.push(range);
            }
        }
    }
    merge(ranges)
}
