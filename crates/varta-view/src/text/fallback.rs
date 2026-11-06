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
}

/// One face the platform draws a cluster with.
#[derive(Debug, Clone)]
pub struct FallbackFace {
    /// A stable identity of the face (for example its PostScript name and
    /// file): the measurer registers each identity once.
    pub key: String,
    /// The face as a standalone font file.
    pub data: Arc<[u8]>,
}

/// Why a host cannot supply the face its platform would draw a cluster with.
/// The text is then refused rather than drawn with another face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FallbackRefusal {
    /// The face draws colour glyphs (bitmap, `COLR` or SVG tables), which
    /// the native glyph path does not paint.
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
    ranges.sort_by_key(|range| range.start);
    ranges.dedup();
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => merged.push(range),
        }
    }
    merged
}
