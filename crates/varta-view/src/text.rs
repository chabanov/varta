//! Deterministic text shaping using only explicitly supplied fonts.
mod cache;
pub(crate) mod edges;
pub(crate) mod fallback;
pub(crate) mod field_display;
pub(crate) mod transform;
pub(crate) mod whitespace;
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    FontContext, FontFamilyName, FontStyle, FontVariation, FontVariations, FontWeight, Layout,
    LayoutContext, LineHeight, OverflowWrap, StyleProperty, TextWrapMode, setting::Tag,
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use varta_common::view::fonts::{
    MAX_FONT_ALIASES, MAX_FONT_BYTES, MAX_FONT_FACE_BYTES, MAX_FONT_FACES, MAX_FONT_FAMILY_BYTES,
    MAX_SYSTEM_FONT_FILE_BYTES,
};

// Native shaping requests: completed output, intermediate scratch, charged
// operations and reusable scratch. Independent from source/paint allowances.
/// The `opsz` variation axis. CSS `font-optical-sizing: auto` (the initial
/// value; the property is outside the view subset) sets it to the used font
/// size in CSS pixels, as Chrome does, and the face's axis range clamps it; a
/// face without the axis ignores it.
const OPTICAL_SIZE_AXIS: Tag = Tag::new(b"opsz");

/// The automatic optical size of text at `size` CSS pixels.
fn optical_size<'a>(size: f32) -> StyleProperty<'a, u32> {
    StyleProperty::FontVariations(FontVariations::List(Cow::Owned(vec![FontVariation::new(
        OPTICAL_SIZE_AXIS,
        size,
    )])))
}

const MAX_SHAPED_GLYPHS: usize = 65_536;
const MAX_SHAPING_BUFFER_SLOTS: usize = 262_144;
const MAX_SHAPING_OPERATIONS: u32 = 1_000_000;
const MAX_CACHED_SHAPING_SLOTS: usize = 65_536;
const MAX_VIEW_SHAPING_OPERATIONS: usize = 16 * 1024 * 1024;
const MAX_INLINE_WORK: usize = 1_000_000;
const NO_VIEW_REQUEST: usize = usize::MAX;

struct ViewShapingState {
    remaining: AtomicUsize,
    inline_remaining: AtomicUsize,
    last_used: AtomicUsize,
}

struct ViewShapingGuard {
    state: Arc<ViewShapingState>,
    limit: usize,
    owner: bool,
}
impl Drop for ViewShapingGuard {
    fn drop(&mut self) {
        if self.owner {
            self.state
                .inline_remaining
                .store(NO_VIEW_REQUEST, Ordering::Relaxed);
            let remaining = self
                .state
                .remaining
                .swap(NO_VIEW_REQUEST, Ordering::Relaxed);
            self.state
                .last_used
                .store(self.limit.saturating_sub(remaining), Ordering::Relaxed);
        }
    }
}

/// Where the style engine takes a face's metrics from.
#[derive(Clone, Copy)]
enum FaceMetrics {
    /// The face's own tables.
    Tables,
    /// The host's font system ([`TextMeasurer::set_host_fonts`]).
    Host,
}

/// A font and explicit CSS aliases supplied by the embedding host.
pub struct TextFont {
    pub data: Vec<u8>,
    pub family: String,
    pub aliases: Vec<String>,
}

struct FontEntry {
    family: String,
    aliases: Vec<FontFamilyName<'static>>,
}

/// A bounded explicit-font text shaper. Reuse it to retain shaping caches.
pub struct TextMeasurer {
    cached: Option<cache::Shaped>,
    fonts: FontContext,
    layout: LayoutContext<u32>,
    entries: Vec<FontEntry>,
    /// The style engine's view of the faces ([`Self::font_set`]).
    font_set: Arc<varta_style::fonts::FontSet>,
    /// Generic aliases name packaged families ([`Self::new_with_packaged_fonts`]).
    packaged: bool,
    /// The host's platform fallback ([`Self::set_fallback`]).
    fallback: Option<fallback::Fallbacks>,
    /// The registered family of each loaded font blob.
    blob_families: BTreeMap<u64, String>,
    /// Families whose ascent the platform's browser enlarges
    /// ([`Self::set_enlarged_ascent_families`]).
    enlarged_ascent: Vec<String>,
    /// The platform metrics of registered fallback faces, by font blob.
    platform_metrics: BTreeMap<u64, Arc<dyn fallback::PlatformMetrics>>,
    view_shaping: Arc<ViewShapingState>,
    #[cfg(test)]
    shaping_builds: usize,
}

/// The measured advance and height, in CSS pixels, and number of lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    /// Width when every available soft line break is taken.
    pub min_width: f32,
    pub height: f32,
    pub lines: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct TextOptions {
    pub size: f32,
    pub line_height: Option<LineHeight>,
    pub weight: f32,
    pub italic: bool,
    /// The used `letter-spacing` in CSS pixels (CSS Text 3 §8.2), added
    /// after every typographic character unit.
    pub letter_spacing: f32,
    pub white_space: WhiteSpaceMode,
    pub break_anywhere: bool,
    pub source: TextSource,
    /// Inherited `text-align`; applied to lines broken at a definite width.
    pub align: TextAlign,
}

/// One source text node's normalized byte range and resolved font style.
#[derive(Clone, PartialEq)]
pub(crate) struct TextSpan {
    pub range: std::ops::Range<usize>,
    pub owner: u32,
    pub options: TextOptions,
    pub families: Vec<String>,
}

/// Private provenance: field display is bounded by the existing wire/editor
/// UTF-16 allowance, including the combined IME replacement and password masks.
/// It admits no more than 32768 scalars (less than the general ASCII maximum),
/// but those scalars can occupy 98304 UTF-8 bytes. Never split shaping context
/// or relax the general/public byte limit to account for this encoding growth.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TextSource {
    General,
    Field,
}
impl TextSource {
    fn admits(self, text: &str) -> bool {
        match self {
            Self::General => text.len() <= 64 * 1024,
            Self::Field => {
                use varta_common::view::{
                    DRAFT_FIELD_VALUE_RESERVATION_BYTES, DRAFT_MAX_VALUE_BYTES,
                };
                text.len() <= DRAFT_FIELD_VALUE_RESERVATION_BYTES
                    && text
                        .encode_utf16()
                        .take(DRAFT_MAX_VALUE_BYTES / 2 + 1)
                        .count()
                        <= DRAFT_MAX_VALUE_BYTES / 2
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum WhiteSpaceMode {
    #[default]
    Normal,
    NoWrap,
    PreWrap,
    /// `white-space: pre`: the user-agent default of `pre`. Spaces and
    /// newlines are preserved and only forced breaks end a line.
    Pre,
}
impl WhiteSpaceMode {
    /// Spaces, tabs and newlines are preserved rather than collapsed.
    pub(crate) fn preserves(self) -> bool {
        matches!(self, Self::PreWrap | Self::Pre)
    }
    /// Soft wrap opportunities may break lines.
    pub(crate) fn wraps(self) -> bool {
        matches!(self, Self::Normal | Self::PreWrap)
    }
}

/// Inherited `text-align` (L0: `left | center | right`; initial `start`).
/// The CSS `direction` is always `ltr`, so `start` is the left edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum TextAlign {
    #[default]
    Start,
    Left,
    Center,
    Right,
}
impl TextAlign {
    /// Parse the L0 keyword grammar.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "left" => Self::Left,
            "center" => Self::Center,
            "right" => Self::Right,
            // CSS Text 3 §7.1; the renderer lays out left-to-right only.
            "start" => Self::Start,
            "end" => Self::Right,
            _ => return None,
        })
    }
    /// The physical Parley alignment, or `None` for the unaligned start edge.
    fn parley(self) -> Option<parley::Alignment> {
        match self {
            Self::Start | Self::Left => None,
            Self::Center => Some(parley::Alignment::Center),
            Self::Right => Some(parley::Alignment::Right),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TextMeasureError {
    #[error("invalid, oversized, ambiguous, or mismatched font")]
    Font,
    #[error("invalid text size or layout limit")]
    Limit,
    /// Text whose layout the native white-space model does not reproduce.
    #[error("text layout outside the native white-space model")]
    Unsupported,
    /// The platform would draw this character with a face the native
    /// renderer refuses ([`fallback::FallbackRefusal`]).
    #[error("platform fallback face for {character:?}: {refusal}")]
    Fallback {
        character: char,
        refusal: fallback::FallbackRefusal,
    },
}

/// The share of a face's rounded height Chrome on macOS adds to the ascent of
/// Times, Helvetica and Courier ([`TextMeasurer::set_enlarged_ascent_families`]).
pub const ENLARGED_ASCENT_FRACTION: f32 = 0.15;

/// One flow's shaping input ([`TextMeasurer::build_once`]).
#[derive(Clone, Copy)]
struct Shaping<'a> {
    text: &'a str,
    families: &'a [String],
    options: TextOptions,
    spans: &'a [TextSpan],
    edges: &'a [edges::TextEdge],
    tabs: &'a [whitespace::TabStyle],
}

/// The family list a platform fallback face extends over a text range.
type FamilyOverride = (std::ops::Range<usize>, Vec<String>);
/// A family list and style: the shaping segment fallback groups by.
type StyleKey<'a> = (&'a [String], TextOptions);
/// The ranges of one style and presentation (emoji or text) that ask the
/// platform for one face.
type FallbackGroup<'a> = ((StyleKey<'a>, bool), Vec<std::ops::Range<usize>>);

/// The family list and style at `offset`: the last span holding it, else
/// the flow's own.
fn style_at<'a>(
    families: &'a [String],
    options: TextOptions,
    spans: &'a [TextSpan],
    offset: usize,
) -> (&'a [String], TextOptions) {
    spans
        .iter()
        .rev()
        .find(|span| span.range.contains(&offset))
        .map_or((families, options), |span| {
            (span.families.as_slice(), span.options)
        })
}

fn same_family(a: &FontFamilyName<'_>, b: &FontFamilyName<'_>) -> bool {
    match (a, b) {
        (FontFamilyName::Named(a), FontFamilyName::Named(b)) => a.eq_ignore_ascii_case(b),
        _ => a == b,
    }
}

impl TextMeasurer {
    /// Charged shaping operations in the last outer native view request.
    /// Standalone measurements do not update this diagnostic. This excludes
    /// unmetered analysis/plan work and is not a CPU-time or RSS measurement.
    #[must_use]
    pub fn last_view_shaping_operations(&self) -> usize {
        self.view_shaping.last_used.load(Ordering::Relaxed)
    }

    pub(crate) fn with_view_shaping<T>(&mut self, action: impl FnOnce(&mut Self) -> T) -> T {
        self.with_view_shaping_limit(MAX_VIEW_SHAPING_OPERATIONS, action)
    }

    fn with_view_shaping_limit<T>(
        &mut self,
        limit: usize,
        action: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let owner = self
            .view_shaping
            .remaining
            .compare_exchange(NO_VIEW_REQUEST, limit, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok();
        if owner {
            self.view_shaping
                .inline_remaining
                .store(MAX_INLINE_WORK, Ordering::Relaxed);
        }
        let _guard = ViewShapingGuard {
            state: Arc::clone(&self.view_shaping),
            limit,
            owner,
        };
        action(self)
    }

    pub(crate) fn charge_inline_work(&mut self, amount: usize) -> Result<(), TextMeasureError> {
        self.view_shaping
            .inline_remaining
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                (remaining != NO_VIEW_REQUEST)
                    .then(|| remaining.checked_sub(amount))
                    .flatten()
            })
            .map(|_| ())
            .map_err(|_| TextMeasureError::Limit)
    }

    /// Inline work left in the current outer native view request.
    #[cfg(test)]
    pub(crate) fn inline_work_remaining(&self) -> usize {
        self.view_shaping.inline_remaining.load(Ordering::Relaxed)
    }

    pub(crate) fn cache_counts(&self) -> (usize, usize) {
        self.cached
            .as_ref()
            .map_or((0, 0), |cache| (cache.hits, cache.misses))
    }
    pub(crate) fn cache_shapes(&mut self) {
        self.cached = Some(cache::Shaped::default());
    }
    /// Resolve only registered names and explicit aliases. A generic family
    /// preceding any match is rejected unless the host explicitly mapped it.
    pub(crate) fn resolve_families(&self, css_family: Option<&str>) -> Option<Vec<String>> {
        let Some(css_family) = css_family else {
            return Some(vec![self.entries.first()?.family.clone()]);
        };
        let mut resolved = Vec::new();
        for name in FontFamilyName::parse_css_list(css_family) {
            let name = name.ok()?;
            let found = self.entries.iter().find(|entry| {
                same_family(&FontFamilyName::named(&entry.family), &name)
                    || entry.aliases.iter().any(|alias| same_family(alias, &name))
            });
            if let Some(entry) = found {
                if !resolved.contains(&entry.family) {
                    resolved.push(entry.family.clone());
                }
            } else if matches!(name, FontFamilyName::Generic(_)) && resolved.is_empty() {
                return None;
            }
        }
        (!resolved.is_empty()).then_some(resolved)
    }

    /// The style engine's view of these faces: the first available fonts of
    /// the font-relative units (CSS Values 4 §6.1.1). A view takes it with
    /// `View::set_fonts`.
    #[must_use]
    pub fn font_set(&self) -> Arc<varta_style::fonts::FontSet> {
        Arc::clone(&self.font_set)
    }

    /// Let the host measure the faces of a system-font set and the CSS
    /// generic families and default font no face names (natively from the
    /// platform's font system, as its browser does).
    pub fn set_host_fonts(&mut self, host: Option<Arc<dyn varta_style::fonts::HostFonts>>) {
        Arc::make_mut(&mut self.font_set).set_host(host);
    }

    /// Whether the generic `monospace` keeps the browser's fixed-font size
    /// where text switches to or from it ([`crate::style::FontBase`]). It does
    /// not once a packaged face maps it: the generic then names that family.
    pub(crate) fn fixed_monospace(&self) -> bool {
        !(self.packaged
            && self
                .resolve_families(Some(varta_style::font::UA_MONOSPACE_FAMILY))
                .is_some())
    }

    /// Let `provider` name the platform face of every cluster the CSS
    /// families lack (see [`fallback`]); `None` keeps `.notdef` for such a
    /// cluster. Faces registered by an earlier provider stay registered.
    pub fn set_fallback(&mut self, provider: Option<Box<dyn fallback::FontFallback>>) {
        self.fallback = provider.map(fallback::Fallbacks::new);
    }

    /// The registered family of the platform face for `character` (its
    /// emoji face for `emoji`), registering the face on first use; `None`
    /// when the platform has none.
    fn fallback_family(
        &mut self,
        character: char,
        emoji: bool,
        families: &[String],
        options: TextOptions,
    ) -> Result<Option<String>, TextMeasureError> {
        let Some(state) = self.fallback.as_mut() else {
            return Ok(None);
        };
        let refused = |refusal| TextMeasureError::Fallback { character, refusal };
        let primary = families.first().ok_or(TextMeasureError::Limit)?;
        let Some(face) = state
            .provider
            .face(&fallback::FallbackRequest {
                character,
                family: primary,
                weight: options.weight,
                italic: options.italic,
                emoji,
            })
            .map_err(refused)?
        else {
            return Ok(None);
        };
        if let Some(family) = state.registered.get(&face.key) {
            return Ok(Some(family.clone()));
        }
        // A face of a family the measurer already loaded (a system family a
        // generic maps) adds nothing: that family's own faces are used.
        if face.data.len() > fallback::MAX_FALLBACK_FACE_BYTES {
            return Err(refused(fallback::FallbackRefusal::Limit));
        }
        let names =
            Self::registered_families(face.data.to_vec(), fallback::MAX_FALLBACK_FACE_BYTES)
                .map_err(|_| refused(fallback::FallbackRefusal::Unreadable))?;
        if let Some(entry) = self.entries.iter().find(|entry| {
            names.iter().any(|name| {
                same_family(
                    &FontFamilyName::named(&entry.family),
                    &FontFamilyName::named(name),
                )
            })
        }) {
            let family = entry.family.clone();
            state.registered.insert(face.key, family.clone());
            return Ok(Some(family));
        }
        let bytes = state
            .bytes
            .checked_add(face.data.len())
            .filter(|&bytes| {
                bytes <= fallback::MAX_FALLBACK_BYTES
                    && face.data.len() <= fallback::MAX_FALLBACK_FACE_BYTES
                    && state.registered.len() < fallback::MAX_FALLBACK_FACES
            })
            .ok_or_else(|| refused(fallback::FallbackRefusal::Limit))?;
        let blob = Blob::new(Arc::new(face.data));
        let blob_id = blob.id();
        let registered = self.fonts.collection.register_fonts(blob, None);
        let family = registered
            .first()
            .and_then(|(id, _)| self.fonts.collection.family_name(*id))
            .ok_or_else(|| refused(fallback::FallbackRefusal::Unreadable))?
            .to_owned();
        state.bytes = bytes;
        state.registered.insert(face.key, family.clone());
        self.blob_families.insert(blob_id, family.clone());
        if let Some(metrics) = face.metrics {
            self.platform_metrics.insert(blob_id, metrics);
        }
        Ok(Some(family))
    }

    /// Enlarge the ascent of every face of these registered families by
    /// [`ENLARGED_ASCENT_FRACTION`] of its rounded height, rounded, as the
    /// platform's browser does for them (Chrome on macOS: Times, Helvetica
    /// and Courier, to match the vertical metrics of their Windows
    /// counterparts). A host sets it for the platform's own faces only.
    pub fn set_enlarged_ascent_families(&mut self, families: &[&str]) {
        self.enlarged_ascent = families.iter().map(|family| (*family).to_owned()).collect();
    }

    /// A run's ascent and descent at `size`, rounded, in a platform fallback
    /// face with its own metrics, or else in a face of an
    /// [`Self::set_enlarged_ascent_families`] family.
    fn platform_or_enlarged(
        &self,
        font: &parley::FontData,
        size: f32,
        metrics: &parley::RunMetrics,
    ) -> Option<(f32, f32)> {
        // Chrome rounds a font's ascent and descent.
        if let Some(platform) = self.platform_metrics.get(&font.data.id()) {
            return platform
                .vertical(size)
                .map(|(ascent, descent)| (ascent.round(), descent.round()));
        }
        self.enlarged(font, metrics)
    }

    /// The enlarged ascent and descent of a run in a font of an
    /// [`Self::set_enlarged_ascent_families`] family.
    fn enlarged(
        &self,
        font: &parley::FontData,
        metrics: &parley::RunMetrics,
    ) -> Option<(f32, f32)> {
        let family = self.blob_families.get(&font.data.id())?;
        if !self
            .enlarged_ascent
            .iter()
            .any(|enlarged| enlarged.eq_ignore_ascii_case(family))
        {
            return None;
        }
        let (ascent, descent) = (metrics.ascent.round(), metrics.descent.round());
        Some((
            ascent + ((ascent + descent) * ENLARGED_ASCENT_FRACTION + 0.5).floor(),
            descent,
        ))
    }

    /// Gives runs the vertical metrics Chrome lays lines out with: an
    /// enlarged-ascent family's runs their enlarged ascent, a platform
    /// fallback face's runs under a line height other than `normal` the
    /// metrics of their element's primary font (Blink adds the metrics of
    /// other used fonts to an inline box only for `line-height: normal`),
    /// and otherwise a fallback face with platform metrics its platform
    /// ascent and descent.
    /// Returns the charged operations of shaping the primary fonts.
    fn adjust_vertical_metrics(
        &mut self,
        layout: &mut Layout<u32>,
        input: Shaping<'_>,
        overrides: &[FamilyOverride],
    ) -> Result<usize, TextMeasureError> {
        let Shaping {
            families,
            options,
            spans,
            ..
        } = input;
        const SPACE: &str = " ";
        if self.enlarged_ascent.is_empty() && overrides.is_empty() {
            return Ok(0);
        }
        let mut operations = 0usize;
        let mut primaries = fallback::RangeMap::new();
        // The primary metrics of each style, shaped once per style.
        let mut sampled: Vec<(StyleKey<'_>, Option<(f32, f32)>)> = Vec::new();
        for (range, _) in overrides {
            let (list, style) = style_at(families, options, spans, range.start);
            if !matches!(
                style.line_height,
                Some(LineHeight::Absolute(_) | LineHeight::FontSizeRelative(_))
            ) {
                continue;
            }
            if let Some((_, primary)) = sampled.iter().find(|(key, _)| *key == (list, style)) {
                if let Some(primary) = *primary {
                    primaries.insert(range.clone(), primary);
                }
                continue;
            }
            let sample_input = Shaping {
                text: SPACE,
                families: list,
                options: style,
                spans: &[],
                edges: &[],
                tabs: &[],
            };
            let (mut sample, used) = self.build_once(sample_input, &[])?;
            operations = operations.saturating_add(used);
            let mut primary = None;
            sample.adjust_run_metrics(|_, font, _, metrics| {
                primary.get_or_insert_with(|| {
                    self.enlarged(font, metrics)
                        .unwrap_or((metrics.ascent, metrics.descent))
                });
                None
            });
            sampled.push(((list, style), primary));
            if let Some(primary) = primary {
                primaries.insert(range.clone(), primary);
            }
        }
        // One run of a fallback face may join several adjacent ranges (an
        // emoji sequence and the emoji after it); any of them gives its
        // primary metrics.
        layout.adjust_run_metrics(|range, font, size, metrics| {
            primaries
                .overlapping(range)
                .copied()
                .or_else(|| self.platform_or_enlarged(font, size, metrics))
        });
        Ok(operations)
    }

    /// Load one font family from bytes. No OS font lookup is enabled.
    pub fn new(font: Vec<u8>, family: &str) -> Result<Self, TextMeasureError> {
        Self::new_with_aliases(font, family, &[])
    }

    /// Map named or generic CSS families to this explicit font.
    pub fn new_with_aliases(
        font: Vec<u8>,
        family: &str,
        aliases: &[&str],
    ) -> Result<Self, TextMeasureError> {
        Self::new_with_fonts(vec![TextFont {
            data: font,
            family: family.into(),
            aliases: aliases.iter().map(|value| (*value).into()).collect(),
        }])
    }

    /// Load an application's packaged faces (its artifact resource section)
    /// as [`Self::new_with_fonts`] does. A generic family a face declares
    /// names that face's family, exactly as the official browser host
    /// rewrites the generic to the packaged family before the browser sees
    /// it: the generic `monospace` (guest-set or the user-agent family of
    /// `pre` and `code`) is then sized as that named family, without the
    /// browser's fixed-font size ratio. With [`Self::new_with_fonts`] a mapped
    /// generic stands in for the browser's own generic font and keeps it.
    pub fn new_with_packaged_fonts(fonts: Vec<TextFont>) -> Result<Self, TextMeasureError> {
        let mut measurer = Self::new_with_fonts(fonts)?;
        measurer.packaged = true;
        Ok(measurer)
    }

    /// Load a bounded collection of explicit fonts. Alias collisions are rejected.
    /// The first family is the default when CSS does not specify a family list.
    pub fn new_with_fonts(fonts: Vec<TextFont>) -> Result<Self, TextMeasureError> {
        Self::load(fonts, MAX_FONT_FACE_BYTES, FaceMetrics::Tables)
    }

    /// Load font files a native host read from its own system fonts, as
    /// [`Self::new_with_fonts`] does, with each file bounded by
    /// [`MAX_SYSTEM_FONT_FILE_BYTES`] instead of the packaged-face limit (a
    /// platform's variable UI font is larger). Nothing is looked up here: the
    /// host names every file, family and generic alias.
    pub fn new_with_system_fonts(fonts: Vec<TextFont>) -> Result<Self, TextMeasureError> {
        Self::load(fonts, MAX_SYSTEM_FONT_FILE_BYTES, FaceMetrics::Host)
    }

    /// The family names a system font file registers, in file order and
    /// without repeats: the names [`Self::new_with_system_fonts`] accepts for
    /// it. A host reads them from the file because a platform may know its
    /// UI font under another name than the file's own.
    pub fn system_font_families(data: Vec<u8>) -> Result<Vec<String>, TextMeasureError> {
        Self::registered_families(data, MAX_SYSTEM_FONT_FILE_BYTES)
    }

    /// The family names `data` registers, in order and without repeats, for
    /// a face of at most `limit` bytes.
    fn registered_families(data: Vec<u8>, limit: usize) -> Result<Vec<String>, TextMeasureError> {
        if data.is_empty() || data.len() > limit {
            return Err(TextMeasureError::Font);
        }
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let registered = collection.register_fonts(Blob::new(Arc::new(data)), None);
        let mut families: Vec<String> = Vec::new();
        for (id, _) in registered {
            let name = collection.family_name(id).ok_or(TextMeasureError::Font)?;
            if !families.iter().any(|family| family == name) {
                families.push(name.into());
            }
        }
        if families.is_empty() {
            return Err(TextMeasureError::Font);
        }
        Ok(families)
    }

    fn load(
        fonts: Vec<TextFont>,
        face_limit: usize,
        metrics: FaceMetrics,
    ) -> Result<Self, TextMeasureError> {
        if fonts.is_empty() || fonts.len() > MAX_FONT_FACES {
            return Err(TextMeasureError::Font);
        }
        // The style engine's view of the same faces: the first available
        // font of the font-relative units. The first family is the default.
        let mut font_set =
            varta_style::fonts::FontSet::new(fonts.first().map(|font| font.family.clone()), None);
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let mut entries: Vec<FontEntry> = Vec::new();
        let mut blob_families = BTreeMap::new();
        let mut bytes = 0usize;
        for font in fonts {
            bytes = bytes
                .checked_add(font.data.len())
                .ok_or(TextMeasureError::Font)?;
            if font.data.is_empty()
                || font.data.len() > face_limit
                || bytes > MAX_FONT_BYTES
                || font.family.is_empty()
                || font.family.len() > MAX_FONT_FAMILY_BYTES
                || font.aliases.len() > MAX_FONT_ALIASES
            {
                return Err(TextMeasureError::Font);
            }
            let aliases = font
                .aliases
                .iter()
                .map(|alias| {
                    if alias.is_empty()
                        || alias.len() > MAX_FONT_FAMILY_BYTES
                        || alias.trim_end().ends_with(',')
                    {
                        return Err(TextMeasureError::Font);
                    }
                    let mut parsed = FontFamilyName::parse_css_list(alias);
                    let name = parsed
                        .next()
                        .ok_or(TextMeasureError::Font)?
                        .map_err(|_| TextMeasureError::Font)?;
                    if parsed.next().is_some() {
                        return Err(TextMeasureError::Font);
                    }
                    Ok(name.into_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let name = FontFamilyName::named(&font.family);
            let same = entries
                .iter()
                .position(|existing| same_family(&name, &FontFamilyName::named(&existing.family)));
            for (index, existing) in entries.iter().enumerate() {
                if Some(index) == same {
                    continue;
                }
                let existing_name = FontFamilyName::named(&existing.family);
                if existing
                    .aliases
                    .iter()
                    .any(|alias| same_family(alias, &name))
                    || aliases.iter().any(|alias| {
                        same_family(alias, &existing_name)
                            || existing
                                .aliases
                                .iter()
                                .any(|other| same_family(alias, other))
                    })
                {
                    return Err(TextMeasureError::Font);
                }
            }
            let engine_aliases = font
                .aliases
                .iter()
                .filter_map(|alias| varta_style::fonts::family_list(alias)?.pop())
                .collect::<Vec<_>>();
            match metrics {
                FaceMetrics::Tables => font_set
                    .add_file(&font.family, &engine_aliases, &font.data)
                    .map(drop),
                FaceMetrics::Host => font_set.add_host_family(&font.family, &engine_aliases),
            }
            .map_err(|_| TextMeasureError::Font)?;
            let blob = Blob::new(Arc::new(font.data));
            blob_families.insert(blob.id(), font.family.clone());
            let registered = collection.register_fonts(blob, None);
            let Some(family_id) = collection.family_id(&font.family) else {
                return Err(TextMeasureError::Font);
            };
            if !registered.iter().any(|(id, _)| *id == family_id) {
                return Err(TextMeasureError::Font);
            }
            if let Some(index) = same {
                entries[index].aliases.extend(aliases);
            } else {
                entries.push(FontEntry {
                    family: font.family,
                    aliases,
                });
            }
        }
        Ok(Self {
            cached: None,
            fonts: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            layout: {
                let mut layout = LayoutContext::new();
                layout.set_shaping_limits(
                    MAX_SHAPED_GLYPHS,
                    MAX_SHAPING_BUFFER_SLOTS,
                    MAX_SHAPING_OPERATIONS,
                    MAX_CACHED_SHAPING_SLOTS,
                );
                layout
            },
            entries,
            font_set: Arc::new(font_set),
            packaged: false,
            fallback: None,
            blob_families,
            enlarged_ascent: Vec::new(),
            platform_metrics: BTreeMap::new(),
            view_shaping: Arc::new(ViewShapingState {
                remaining: AtomicUsize::new(NO_VIEW_REQUEST),
                inline_remaining: AtomicUsize::new(NO_VIEW_REQUEST),
                last_used: AtomicUsize::new(0),
            }),
            #[cfg(test)]
            shaping_builds: 0,
        })
    }

    /// Shape a bounded UTF-8 string with the first supplied font family.
    pub fn measure(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
    ) -> Result<TextMetrics, TextMeasureError> {
        let families = self.resolve_families(None).ok_or(TextMeasureError::Font)?;
        self.measure_styled(
            text,
            max_width,
            &families,
            TextOptions {
                size,
                line_height: None,
                weight: 400.0,
                italic: false,
                letter_spacing: 0.0,
                white_space: WhiteSpaceMode::Normal,
                break_anywhere: false,
                source: TextSource::General,
                align: TextAlign::Start,
            },
        )
    }

    /// Shape using a CSS family list resolved against the supplied font collection.
    pub fn measure_with_family_list(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        css_family: &str,
    ) -> Result<TextMetrics, TextMeasureError> {
        let families = self
            .resolve_families(Some(css_family))
            .ok_or(TextMeasureError::Font)?;
        self.measure_styled(
            text,
            max_width,
            &families,
            TextOptions {
                size,
                line_height: None,
                weight: 400.0,
                italic: false,
                letter_spacing: 0.0,
                white_space: WhiteSpaceMode::Normal,
                break_anywhere: false,
                source: TextSource::General,
                align: TextAlign::Start,
            },
        )
    }

    pub(crate) fn measure_styled(
        &mut self,
        text: &str,
        max_width: Option<f32>,
        families: &[String],
        options: TextOptions,
    ) -> Result<TextMetrics, TextMeasureError> {
        self.shape_styled(text, max_width, families, options)
            .map(|(metrics, _)| metrics)
    }

    pub(crate) fn shape_styled(
        &mut self,
        text: &str,
        max_width: Option<f32>,
        families: &[String],
        options: TextOptions,
    ) -> Result<(TextMetrics, Layout<u32>), TextMeasureError> {
        self.shape_with_spans(text, max_width, families, options, &[])
    }

    pub(crate) fn shape_with_spans(
        &mut self,
        text: &str,
        max_width: Option<f32>,
        families: &[String],
        options: TextOptions,
        spans: &[TextSpan],
    ) -> Result<(TextMetrics, Layout<u32>), TextMeasureError> {
        self.shape_with_edges(text, max_width, families, options, spans, &[])
    }

    pub(crate) fn shape_with_edges(
        &mut self,
        text: &str,
        max_width: Option<f32>,
        families: &[String],
        options: TextOptions,
        spans: &[TextSpan],
        edges: &[edges::TextEdge],
    ) -> Result<(TextMetrics, Layout<u32>), TextMeasureError> {
        let edge_work = edges::validate(text, edges)?;
        if edge_work != 0 {
            self.charge_inline_work(edge_work)?;
        }
        if !options.source.admits(text)
            || !options.size.is_finite()
            || !(0.01..=4096.0).contains(&options.size)
            || max_width.is_some_and(|v| !v.is_finite() || !(0.01..=1_000_000.0).contains(&v))
            || families.is_empty()
        {
            return Err(TextMeasureError::Limit);
        }
        if spans.len() > 1024
            || spans.iter().any(|span| {
                span.range.start > span.range.end
                    || span.range.end > text.len()
                    || !text.is_char_boundary(span.range.start)
                    || !text.is_char_boundary(span.range.end)
                    || !span.options.size.is_finite()
                    || !(0.01..=4096.0).contains(&span.options.size)
                    || span.families.is_empty()
            })
        {
            return Err(TextMeasureError::Limit);
        }
        if !spans.is_empty() {
            self.charge_inline_work(spans.len())?;
        }
        let request = cache::Request {
            text,
            width: max_width,
            families,
            options,
            spans,
            edges,
        };
        let remaining = self.view_shaping.remaining.load(Ordering::Relaxed);
        let scoped = remaining != NO_VIEW_REQUEST;
        if let Some((value, operations)) = self.cached.as_mut().and_then(|cache| cache.get(request))
        {
            if scoped
                && self
                    .view_shaping
                    .remaining
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                        left.checked_sub(operations)
                    })
                    .is_err()
            {
                self.view_shaping.remaining.store(0, Ordering::Relaxed);
                return Err(TextMeasureError::Limit);
            }
            return Ok(value.clone());
        }
        let (result, operations) = match whitespace::Flow::analyze(text, options, spans)? {
            Some(flow) => whitespace::shape(
                self, &flow, text, max_width, families, options, spans, edges,
            )?,
            None => {
                let (mut layout, operations) =
                    self.build_layout(text, families, options, spans, edges, &[])?;
                let widths = layout.calculate_content_widths();
                // Parley's min-content honours each cluster's own wrap mode,
                // so a flow wraps at min-content when any of its text does:
                // a `pre` or `nowrap` root may hold a wrapping span.
                let wraps = options.white_space.wraps()
                    || spans
                        .iter()
                        .any(|span| !span.range.is_empty() && span.options.white_space.wraps());
                let min_width = if wraps { widths.min } else { widths.max };
                layout.break_all_lines(max_width);
                // Alignment needs the definite line width; intrinsic measurement
                // (no width) keeps every line at the start edge.
                if let (Some(alignment), Some(_)) = (options.align.parley(), max_width) {
                    layout.align(alignment, parley::AlignmentOptions::default());
                }
                (
                    (
                        TextMetrics {
                            width: layout.width(),
                            min_width,
                            height: layout.height(),
                            lines: layout.len(),
                        },
                        layout,
                    ),
                    operations,
                )
            }
        };
        if let Some(cache) = self.cached.as_mut() {
            cache.insert(request, &result, operations);
        }
        Ok(result)
    }

    /// Builds one shaped layout, charging its shaping operations to the
    /// current view request. Preserved tabs shape as spaces with `tabs`'
    /// spacing. Returns the layout and its charged operations.
    fn build_layout(
        &mut self,
        text: &str,
        families: &[String],
        options: TextOptions,
        spans: &[TextSpan],
        edges: &[edges::TextEdge],
        tabs: &[whitespace::TabStyle],
    ) -> Result<(Layout<u32>, usize), TextMeasureError> {
        let input = Shaping {
            text,
            families,
            options,
            spans,
            edges,
            tabs,
        };
        let (mut layout, mut operations) = self.build_once(input, &[])?;
        // Platform fallback faces, appended to the CSS families of the
        // `.notdef` ranges; a later override of a position wins.
        let mut overrides: Vec<FamilyOverride> = Vec::new();
        let passes = if self.fallback.is_some() {
            fallback::Fallbacks::passes()
        } else {
            0
        };
        for _ in 0..passes {
            if !self.fallback_pass(&mut layout, input, &mut overrides)? {
                break;
            }
            let (rebuilt, used) = self.build_once(input, &overrides)?;
            layout = rebuilt;
            operations = operations.saturating_add(used);
        }
        if !self.platform_metrics.is_empty() {
            let platform = &self.platform_metrics;
            layout.adjust_glyph_advances(|font, size, glyph| {
                platform.get(&font.data.id())?.advance(glyph, size)
            });
        }
        let used = self.adjust_vertical_metrics(&mut layout, input, &overrides)?;
        Ok((layout, operations.saturating_add(used)))
    }

    /// One fallback pass over `layout`, as Blink's shaper falls back: the
    /// `.notdef` ranges of one style (one shaping segment) all try the face
    /// the platform gives the first range's first character, after the
    /// families they had. Returns whether a range took a new face.
    fn fallback_pass(
        &mut self,
        layout: &mut Layout<u32>,
        input: Shaping<'_>,
        overrides: &mut Vec<FamilyOverride>,
    ) -> Result<bool, TextMeasureError> {
        let Shaping {
            text,
            families,
            options,
            spans,
            ..
        } = input;
        // Ranges split where the style changes, grouped by style and
        // presentation in the order of their first range. Emoji graphemes
        // without a colour face are their own ranges, whether or not their
        // face lacks them.
        // Both lists are sorted and disjoint: one walk cuts the emoji out of
        // the missing ranges.
        let emoji = fallback::uncolored_emoji(layout, text);
        let mut ranges: Vec<(std::ops::Range<usize>, bool)> =
            emoji.iter().map(|range| (range.clone(), true)).collect();
        let mut next = 0;
        for missing in fallback::missing_clusters(layout, text) {
            while emoji
                .get(next)
                .is_some_and(|covered| covered.end <= missing.start)
            {
                next += 1;
            }
            let mut start = missing.start;
            let mut index = next;
            while let Some(covered) = emoji
                .get(index)
                .filter(|covered| covered.start < missing.end)
            {
                if start < covered.start {
                    ranges.push((start..covered.start, false));
                }
                start = start.max(covered.end);
                index += 1;
            }
            if start < missing.end {
                ranges.push((start..missing.end, false));
            }
        }
        ranges.sort_by_key(|(range, _)| range.start);
        let mut groups: Vec<FallbackGroup<'_>> = Vec::new();
        for (range, emoji) in ranges {
            let mut start = range.start;
            while start < range.end {
                let style = style_at(families, options, spans, start);
                let end = spans
                    .iter()
                    .flat_map(|span| [span.range.start, span.range.end])
                    .filter(|&boundary| boundary > start && boundary < range.end)
                    .filter(|&boundary| style_at(families, options, spans, boundary) != style)
                    .min()
                    .unwrap_or(range.end);
                let key = (style, emoji);
                let index = match groups.iter().position(|(known, _)| *known == key) {
                    Some(index) => index,
                    None => {
                        groups.push((key, Vec::new()));
                        groups.len() - 1
                    }
                };
                groups[index].1.push(start..end);
                start = end;
            }
        }
        // The family stack each position has after the earlier passes (a
        // later override wins). This pass's ranges are disjoint, so none of
        // them changes another's.
        let mut stacks = fallback::RangeMap::new();
        for (index, (range, _)) in overrides.iter().enumerate() {
            stacks.insert(range.clone(), index);
        }
        let earlier = overrides.len();
        let mut added = false;
        for (((list, style), emoji), ranges) in groups {
            let Some(first) = ranges.first() else {
                continue;
            };
            let character = text[first.clone()]
                .chars()
                .next()
                .ok_or(TextMeasureError::Limit)?;
            let Some(family) = self.fallback_family(character, emoji, list, style)? else {
                continue;
            };
            for range in ranges {
                let current = stacks
                    .get(range.start)
                    .filter(|&&index| index < earlier)
                    .map_or(list, |&index| overrides[index].1.as_slice());
                if current.contains(&family) {
                    continue;
                }
                // The emoji face draws emoji before the CSS families.
                let mut stack = current.to_vec();
                if emoji {
                    stack.insert(0, family.clone());
                } else {
                    stack.push(family.clone());
                }
                overrides.push((range, stack));
                added = true;
            }
        }
        Ok(added)
    }

    /// One shaping build; `overrides` replace the family list of their
    /// ranges after every span (platform fallback faces).
    fn build_once(
        &mut self,
        input: Shaping<'_>,
        overrides: &[FamilyOverride],
    ) -> Result<(Layout<u32>, usize), TextMeasureError> {
        let Shaping {
            text,
            families,
            options,
            spans,
            edges,
            tabs,
        } = input;
        let remaining = self.view_shaping.remaining.load(Ordering::Relaxed);
        let scoped = remaining != NO_VIEW_REQUEST;
        self.layout.set_shaping_operation_ceiling(
            scoped.then_some(remaining.min(u32::MAX as usize) as u32),
        );
        let family_names: Vec<_> = families
            .iter()
            .map(|name| FontFamilyName::Named(Cow::Borrowed(name.as_str())))
            .collect();
        let mut builder = self
            .layout
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        #[cfg(test)]
        {
            self.shaping_builds += 1;
        }
        builder.push_default(StyleProperty::FontFamily(family_names.as_slice().into()));
        builder.push_default(StyleProperty::FontSize(options.size));
        builder.push_default(optical_size(options.size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(options.weight)));
        builder.push_default(StyleProperty::FontStyle(if options.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }));
        builder.push_default(StyleProperty::LetterSpacing(options.letter_spacing));
        if !options.white_space.wraps() {
            builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        }
        if options.break_anywhere {
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        }
        if let Some(line_height) = options.line_height {
            builder.push_default(StyleProperty::LineHeight(line_height));
        }
        for edge in edges {
            builder.push_inline_box(edge.inline_box());
        }
        for span in spans {
            if span.range.is_empty() {
                continue;
            }
            let names: Vec<_> = span
                .families
                .iter()
                .map(|name| FontFamilyName::Named(Cow::Borrowed(name.as_str())))
                .collect();
            let range = span.range.clone();
            let style = span.options;
            builder.push(StyleProperty::Brush(span.owner), range.clone());
            builder.push(
                StyleProperty::FontFamily(names.as_slice().into()),
                range.clone(),
            );
            builder.push(StyleProperty::FontSize(style.size), range.clone());
            builder.push(optical_size(style.size), range.clone());
            builder.push(
                StyleProperty::FontWeight(FontWeight::new(style.weight)),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontStyle(if style.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                }),
                range.clone(),
            );
            builder.push(
                StyleProperty::LineHeight(style.line_height.unwrap_or_default()),
                range.clone(),
            );
            builder.push(
                StyleProperty::LetterSpacing(style.letter_spacing),
                range.clone(),
            );
            builder.push(
                StyleProperty::TextWrapMode(if !style.white_space.wraps() {
                    TextWrapMode::NoWrap
                } else {
                    TextWrapMode::Wrap
                }),
                range.clone(),
            );
            builder.push(
                StyleProperty::OverflowWrap(if style.break_anywhere {
                    OverflowWrap::Anywhere
                } else {
                    OverflowWrap::Normal
                }),
                range,
            );
        }
        for (range, stack) in overrides {
            let names: Vec<_> = stack
                .iter()
                .map(|name| FontFamilyName::Named(Cow::Borrowed(name.as_str())))
                .collect();
            builder.push(
                StyleProperty::FontFamily(names.as_slice().into()),
                range.clone(),
            );
        }
        for &tab in tabs {
            builder.push(StyleProperty::WordSpacing(tab.spacing), tab.range());
            builder.push(
                StyleProperty::FontFeatures(whitespace::TabStyle::features()),
                tab.range(),
            );
        }
        let layout: Layout<u32> = builder.build(text);
        if scoped {
            let used = self.layout.shaping_operations();
            if self
                .view_shaping
                .remaining
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    usize::try_from(used)
                        .ok()
                        .and_then(|used| value.checked_sub(used))
                })
                .is_err()
            {
                self.view_shaping.remaining.store(0, Ordering::Relaxed);
                return Err(TextMeasureError::Limit);
            }
        }
        if !self.layout.shaping_succeeded() {
            return Err(TextMeasureError::Limit);
        }
        let operations = usize::try_from(self.layout.shaping_operations())
            .map_err(|_| TextMeasureError::Limit)?;
        Ok((layout, operations))
    }

    /// The space (U+0020) advance in the flow root's font, which sets the
    /// tab stops of the whole flow. Returns it and its charged operations.
    fn root_space_advance(
        &mut self,
        families: &[String],
        options: TextOptions,
    ) -> Result<(f32, usize), TextMeasureError> {
        const SPACE: &str = " ";
        let (mut layout, operations) =
            self.build_layout(SPACE, families, options, &[], &[], &[])?;
        layout.break_all_lines(None);
        let advance = layout
            .lines()
            .flat_map(|line| line.runs().collect::<Vec<_>>())
            .flat_map(|run| {
                run.clusters()
                    .map(|cluster| cluster.advance())
                    .collect::<Vec<_>>()
            })
            .next()
            .ok_or(TextMeasureError::Unsupported)?;
        Ok((advance, operations))
    }

    /// Charges `amount` units of line-model work to the current view request
    /// as shaping operations. Returns the amount a cached result repays.
    fn debit_shaping(&self, amount: usize) -> Result<usize, TextMeasureError> {
        if self.view_shaping.remaining.load(Ordering::Relaxed) != NO_VIEW_REQUEST
            && self
                .view_shaping
                .remaining
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                    (left != NO_VIEW_REQUEST)
                        .then(|| left.checked_sub(amount))
                        .flatten()
                })
                .is_err()
        {
            self.view_shaping.remaining.store(0, Ordering::Relaxed);
            return Err(TextMeasureError::Limit);
        }
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cluster_measurer() -> TextMeasurer {
        TextMeasurer::new(
            include_bytes!("../tests/fixtures/clusters.ttf").to_vec(),
            "Varta Cluster Fixture",
        )
        .unwrap()
    }

    #[derive(Default)]
    struct ScopeBroker {
        writes: usize,
    }
    impl crate::Broker for ScopeBroker {
        fn link_requested(&self, _: &str) -> bool {
            true
        }
        fn open_url(&mut self, _: &str) -> crate::Access {
            crate::Access::Denied
        }
        fn clipboard_write(&mut self) -> crate::Access {
            self.writes += 1;
            crate::Access::Granted
        }
    }

    fn scoped_view() -> (crate::View, TextMeasurer) {
        scoped_view_with_fields(1)
    }

    fn scoped_view_with_fields(fields: u32) -> (crate::View, TextMeasurer) {
        use varta_common::view::{CssProperty, Tag};
        let mut view = crate::View::default();
        let mut batch = crate::BatchWriter::new();
        for id in 2..2 + fields {
            batch
                .create_element(id, Tag::Textarea)
                .set_value(id, "AA", 0)
                .insert(1, id, 0);
            for (property, value) in [
                (CssProperty::Display, "block"),
                (CssProperty::Width, "80px"),
                (CssProperty::Height, "auto"),
                (CssProperty::MaxHeight, "60px"),
                (CssProperty::MinWidth, "0"),
                (CssProperty::MinHeight, "0"),
                (CssProperty::FieldSizing, "content"),
                (CssProperty::Resize, "none"),
                (CssProperty::Padding, "4px"),
                (CssProperty::Border, "2px solid #000"),
                (CssProperty::BorderRadius, "0"),
                (CssProperty::Outline, "none"),
                (CssProperty::BackgroundColor, "#fff"),
                (CssProperty::Color, "#00f"),
                (CssProperty::FontFamily, "\"Varta AAT Marked\""),
                (CssProperty::FontSize, "20px"),
                (CssProperty::FontWeight, "normal"),
                (CssProperty::FontStyle, "normal"),
                (CssProperty::LineHeight, "20px"),
            ] {
                batch.set_style(id, property, value);
            }
        }
        view.commit(&batch.finish(), false, &mut ScopeBroker::default())
            .unwrap();
        let measurer = TextMeasurer::new(
            include_bytes!("../tests/fixtures/aat-marked.ttf").to_vec(),
            "Varta AAT Marked",
        )
        .unwrap();
        (view, measurer)
    }

    #[test]
    fn view_shaping_failure_stops_further_taffy_measurements() {
        #[derive(Debug)]
        enum Request {
            Geometry,
            Boxes,
            Raster,
            Scaled,
        }
        const FIELDS: u32 = 8;
        let (view, mut measurer) = scoped_view_with_fields(FIELDS);
        // Each fixture shapes in eight charged units. All intrinsic builds
        // succeed, but the first width-constrained build cannot complete.
        measurer.measure("AA", 20.0, None).unwrap();
        let one = measurer.layout.shaping_operations() as usize;
        assert_eq!(one, 8);
        let intrinsic = FIELDS as usize;
        let limit = intrinsic * one + one - 1;
        let snapshot = view.snapshot();
        let expected = view
            .rasterize_with_text(100, 100, &mut measurer, 20.0)
            .unwrap();
        for request in [
            Request::Geometry,
            Request::Boxes,
            Request::Raster,
            Request::Scaled,
        ] {
            let before = measurer.shaping_builds;
            let error = measurer
                .with_view_shaping_limit(limit, |font| match request {
                    Request::Geometry => view
                        .layout_geometry_with_text(100.0, 100.0, font, 20.0)
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                    Request::Boxes => view
                        .layout_boxes_with_text(100.0, 100.0, font, 20.0)
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                    Request::Raster => view
                        .rasterize_with_text(100, 100, font, 20.0)
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                    Request::Scaled => view
                        .rasterize_scaled(200, 200, 2.0, Some((font, 20.0)))
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                })
                .unwrap_err();
            assert_eq!(
                error,
                format!("text measurement failed: {}", TextMeasureError::Limit)
            );
            assert_eq!(
                measurer.shaping_builds - before,
                intrinsic + 1,
                "{request:?}: a refused layout must not construct any more shaping builders"
            );
            assert!(measurer.last_view_shaping_operations() <= limit);
            assert_eq!(view.snapshot(), snapshot);
            assert_eq!(
                view.rasterize_with_text(100, 100, &mut measurer, 20.0)
                    .unwrap()
                    .pixels,
                expected.pixels
            );
        }
    }

    #[test]
    fn renderer_shape_cache_keeps_hot_entries_across_eviction() {
        let mut font = cluster_measurer();
        font.cache_shapes();
        font.measure("A", 20.0, None).unwrap();
        for width in 1..=200 {
            font.measure("A", 20.0, Some(width as f32)).unwrap();
            font.measure("A", 20.0, None).unwrap();
        }
        assert_eq!(font.shaping_builds, 201);
        assert_eq!(font.cache_counts(), (200, 201));
    }

    #[test]
    fn renderer_shape_cache_keeps_exact_work_charges_and_rejects_an_unfunded_hit() {
        let mut font = cluster_measurer();
        font.cache_shapes();
        let expected = font
            .with_view_shaping(|font| font.measure("A\u{035c}", 20.0, None))
            .unwrap();
        let operations = font.last_view_shaping_operations();
        assert!(operations > 0);
        let builds = font.shaping_builds;
        assert!(matches!(
            font.with_view_shaping_limit(operations - 1, |font| font.measure(
                "A\u{035c}",
                20.0,
                None
            )),
            Err(TextMeasureError::Limit)
        ));
        assert_eq!(font.shaping_builds, builds);
        assert_eq!(
            font.with_view_shaping_limit(operations, |font| font.measure("A\u{035c}", 20.0, None))
                .unwrap(),
            expected
        );
        assert_eq!(font.last_view_shaping_operations(), operations);
        assert_eq!(font.shaping_builds, builds);
    }

    #[test]
    fn inline_work_scope_is_shared_and_failed_layout_keeps_batch_effects_atomic() {
        use varta_common::view::{Command, CssProperty as P, Tag};
        let mut view = crate::View::default();
        let mut font = cluster_measurer();
        let mut broker = ScopeBroker::default();
        let mut b = crate::BatchWriter::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .create_element(3, Tag::Span)
            .insert(2, 3, 0)
            .create_text(4, "AA")
            .insert(3, 4, 0)
            .set_style(2, P::FontFamily, "\"Varta Cluster Fixture\"")
            .set_style(2, P::FontSize, "20px")
            .set_style(2, P::LineHeight, "20px");
        view.commit(&b.finish(), false, &mut broker).unwrap();
        let snapshot = view.snapshot();
        let commits = view.commits();
        let mut b = crate::BatchWriter::new();
        b.set_title("only after geometry")
            .copy_text(77, "deferred")
            .command(3, Command::ScrollIntoView, 0.0);
        let bytes = b.finish();
        font.with_view_shaping(|font| {
            font.view_shaping
                .inline_remaining
                .store(1, Ordering::Relaxed);
            let error = view
                .commit_with_layout(&bytes, true, &mut broker, 100.0, 100.0, Some((font, 20.0)))
                .unwrap_err();
            assert!(matches!(
                error,
                crate::ViewError::Invalid {
                    violation: crate::Violation::ScrollLayout,
                    ..
                }
            ));
            assert!(
                view.layout_boxes_with_text(100.0, 100.0, font, 20.0)
                    .is_err(),
                "a nested API cannot refill inline work"
            );
            assert_eq!(view.snapshot(), snapshot);
            assert_eq!(view.commits(), commits);
            assert_eq!(broker.writes, 0);
        });
        view.commit_with_layout(
            &bytes,
            true,
            &mut broker,
            100.0,
            100.0,
            Some((&mut font, 20.0)),
        )
        .unwrap();
        assert_eq!(view.title(), "only after geometry");
        assert_eq!(broker.writes, 1);
        assert_eq!(view.commits(), commits + 1);
    }

    #[test]
    fn view_shaping_failure_stops_measurements_before_batch_effects() {
        use varta_common::view::Command;
        const FIELDS: u32 = 8;
        const OPERATIONS_PER_FIELD: usize = 8;
        let (mut view, mut measurer) = scoped_view_with_fields(FIELDS);
        let intrinsic = FIELDS as usize;
        let limit = intrinsic * OPERATIONS_PER_FIELD + OPERATIONS_PER_FIELD - 1;
        let snapshot = view.snapshot();
        let commits = view.commits();
        let mut batch = crate::BatchWriter::new();
        batch
            .set_title("after complete layout")
            .copy_text(77, "deferred clipboard")
            .command(2, Command::ScrollIntoView, 0.0)
            .command(3, Command::ScrollTo, 0.0);
        let bytes = batch.finish();
        let mut broker = ScopeBroker::default();
        let error = measurer
            .with_view_shaping_limit(limit, |font| {
                view.commit_with_layout(&bytes, true, &mut broker, 100.0, 100.0, Some((font, 20.0)))
            })
            .unwrap_err();
        assert!(matches!(
            error,
            crate::ViewError::Invalid {
                operation: Some(2),
                violation: crate::Violation::ScrollLayout,
            }
        ));
        assert_eq!(measurer.shaping_builds, intrinsic + 1);
        assert!(measurer.last_view_shaping_operations() <= limit);
        assert_eq!(view.snapshot(), snapshot);
        assert_eq!(view.commits(), commits);
        assert_eq!(broker.writes, 0);
        view.commit_with_layout(
            &bytes,
            true,
            &mut broker,
            100.0,
            100.0,
            Some((&mut measurer, 20.0)),
        )
        .unwrap();
        assert_eq!(view.title(), "after complete layout");
        assert_eq!(view.commits(), commits + 1);
        assert_eq!(broker.writes, 1);
    }

    #[test]
    fn view_shaping_scope_covers_geometry_remeasurement_paint_and_scaled_frames() {
        let (view, mut measurer) = scoped_view();
        view.layout_geometry_with_text(100.0, 100.0, &mut measurer, 20.0)
            .unwrap();
        let geometry_ops = measurer.last_view_shaping_operations();
        assert!(geometry_ops > 8, "remeasurement must be included");
        let expected = view
            .rasterize_with_text(100, 100, &mut measurer, 20.0)
            .unwrap();
        let frame_ops = measurer.last_view_shaping_operations();
        assert!(frame_ops > geometry_ops, "painting must share the scope");
        let before = view.snapshot();
        let exact = measurer
            .with_view_shaping_limit(frame_ops, |font| {
                view.rasterize_with_text(100, 100, font, 20.0)
            })
            .unwrap();
        assert_eq!(exact.pixels, expected.pixels);
        assert_eq!(measurer.last_view_shaping_operations(), frame_ops);
        assert!(
            measurer
                .with_view_shaping_limit(geometry_ops, |font| view
                    .rasterize_with_text(100, 100, font, 20.0))
                .is_err()
        );
        assert!(measurer.last_view_shaping_operations() <= geometry_ops);
        assert!(
            measurer
                .with_view_shaping_limit(frame_ops - 1, |font| view.rasterize_scaled(
                    200,
                    200,
                    2.0,
                    Some((font, 20.0))
                ))
                .is_err()
        );
        assert!(
            measurer
                .with_view_shaping_limit(geometry_ops - 1, |font| view
                    .layout_boxes_with_text(100.0, 100.0, font, 20.0))
                .is_err()
        );
        measurer
            .with_view_shaping_limit(geometry_ops, |font| {
                view.layout_boxes_with_text(100.0, 100.0, font, 20.0)
            })
            .unwrap();
        assert_eq!(view.snapshot(), before);
        measurer.measure("AA", 20.0, None).unwrap();
        let recovered = view
            .rasterize_scaled(200, 200, 2.0, Some((&mut measurer, 20.0)))
            .unwrap();
        assert_eq!(measurer.last_view_shaping_operations(), frame_ops);
        assert_eq!(recovered.width, 200);
        assert_eq!(
            view.rasterize_with_text(100, 100, &mut measurer, 20.0)
                .unwrap()
                .pixels,
            expected.pixels
        );
        eprintln!("view geometry operations={geometry_ops}, frame operations={frame_ops}");
    }

    #[test]
    fn view_shaping_scope_is_shared_by_layout_commands_and_preserves_batch_atomicity() {
        use varta_common::view::Command;
        let (mut view, mut measurer) = scoped_view();
        view.layout_geometry_with_text(100.0, 100.0, &mut measurer, 20.0)
            .unwrap();
        let one = measurer.last_view_shaping_operations();
        let mut batch = crate::BatchWriter::new();
        batch
            .set_title("new title")
            .copy_text(77, "deferred clipboard")
            .command(2, Command::ScrollIntoView, 0.0)
            .command(2, Command::ScrollTo, 0.0);
        let bytes = batch.finish();
        let before = view.snapshot();
        let mut broker = ScopeBroker::default();
        let result = measurer.with_view_shaping_limit(one, |font| {
            view.commit_with_layout(&bytes, true, &mut broker, 100.0, 100.0, Some((font, 20.0)))
        });
        assert!(matches!(
            result,
            Err(crate::ViewError::Invalid {
                violation: crate::Violation::ScrollLayout,
                ..
            })
        ));
        assert!(measurer.last_view_shaping_operations() <= one);
        assert_eq!(view.snapshot(), before);
        assert_eq!(broker.writes, 0);
        measurer
            .with_view_shaping_limit(one * 2, |font| {
                view.commit_with_layout(&bytes, true, &mut broker, 100.0, 100.0, Some((font, 20.0)))
            })
            .unwrap();
        assert_eq!(measurer.last_view_shaping_operations(), one * 2);
        assert_eq!(view.title(), "new title");
        assert_eq!(broker.writes, 1);
    }

    #[test]
    fn view_shaping_scope_does_not_reset_on_nested_calls_or_handled_refusal() {
        let (_, mut measurer) = scoped_view();
        measurer.with_view_shaping_limit(8, |font| {
            font.measure("AA", 20.0, None).unwrap();
            assert!(
                font.with_view_shaping(|font| font.measure("AA", 20.0, None))
                    .is_err()
            );
            assert!(font.measure("AA", 20.0, None).is_err());
        });
        assert_eq!(measurer.last_view_shaping_operations(), 8);
        measurer.measure("AA", 20.0, None).unwrap();
        assert_eq!(
            measurer.last_view_shaping_operations(),
            8,
            "standalone measurement is separate"
        );
    }

    #[test]
    fn view_shaping_scope_releases_its_state_during_unwinding() {
        let (_, mut measurer) = scoped_view();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            measurer.with_view_shaping_limit(8, |font| {
                font.measure("AA", 20.0, None).unwrap();
                panic!("intentional view-scope unwind probe");
            });
        }));
        assert!(result.is_err());
        assert_eq!(measurer.last_view_shaping_operations(), 8);
        measurer.measure("AA", 20.0, None).unwrap();
        measurer
            .with_view_shaping(|font| font.measure("AA", 20.0, None))
            .unwrap();
        assert_eq!(measurer.last_view_shaping_operations(), 8);
    }

    #[test]
    fn aat_transition_and_insertion_debits_are_atomic_and_exact() {
        for (data, family, text, expected_glyphs) in [
            (
                include_bytes!("../tests/fixtures/aat-current.ttf").as_slice(),
                "Varta AAT Current",
                "A",
                4,
            ),
            (
                include_bytes!("../tests/fixtures/aat-marked.ttf").as_slice(),
                "Varta AAT Marked",
                "AA",
                7,
            ),
            (
                include_bytes!("../tests/fixtures/aat-kern.ttf").as_slice(),
                "Varta AAT Kern",
                "A",
                1,
            ),
            (
                include_bytes!("../tests/fixtures/aat-kerx.ttf").as_slice(),
                "Varta AAT Kerx",
                "A",
                1,
            ),
        ] {
            let mut measurer = TextMeasurer::new(data.to_vec(), family).unwrap();
            let families = vec![family.into()];
            let options = TextOptions {
                size: 20.0,
                line_height: None,
                weight: 400.0,
                italic: false,
                letter_spacing: 0.0,
                white_space: WhiteSpaceMode::NoWrap,
                break_anywhere: false,
                source: TextSource::General,
                align: TextAlign::Start,
            };
            let (expected, shaped) = measurer
                .shape_styled(text, None, &families, options)
                .unwrap();
            let mut count = 0;
            for line in shaped.lines() {
                for item in line.items() {
                    let parley::PositionedLayoutItem::GlyphRun(run) = item else {
                        panic!("inline box")
                    };
                    count += run.positioned_glyphs().count();
                }
            }
            assert_eq!(count, expected_glyphs, "{family}");
            let operations = u32::try_from(measurer.layout.shaping_operations()).unwrap();
            assert!(operations > 1);
            eprintln!("{family}: glyphs={count}, operations={operations}");
            for budget in 0..operations {
                measurer.layout.set_shaping_limits(
                    MAX_SHAPED_GLYPHS,
                    MAX_SHAPING_BUFFER_SLOTS,
                    budget,
                    MAX_CACHED_SHAPING_SLOTS,
                );
                assert!(
                    matches!(
                        measurer.measure(text, 20.0, None),
                        Err(TextMeasureError::Limit)
                    ),
                    "{family}, {budget}"
                );
                assert!(
                    measurer.layout.shaping_operations() <= u64::from(budget),
                    "{family}, {budget}"
                );
                assert_eq!(measurer.layout.shaping_buffer_capacities(), (0, 0));
            }
            measurer.layout.set_shaping_limits(
                MAX_SHAPED_GLYPHS,
                MAX_SHAPING_BUFFER_SLOTS,
                operations,
                MAX_CACHED_SHAPING_SLOTS,
            );
            assert_eq!(measurer.measure(text, 20.0, None).unwrap(), expected);
        }
    }

    #[test]
    fn aat_nonadvancing_state_loop_refuses_at_its_budget_and_can_retry() {
        let mut measurer = TextMeasurer::new(
            include_bytes!("../tests/fixtures/aat-loop.ttf").to_vec(),
            "Varta AAT Loop",
        )
        .unwrap();
        let expected = measurer.measure(" ", 20.0, None).unwrap();
        for budget in [1, 2, 8, 64] {
            measurer.layout.set_shaping_limits(
                MAX_SHAPED_GLYPHS,
                MAX_SHAPING_BUFFER_SLOTS,
                budget,
                MAX_CACHED_SHAPING_SLOTS,
            );
            assert!(matches!(
                measurer.measure("A", 20.0, None),
                Err(TextMeasureError::Limit)
            ));
            assert_eq!(measurer.layout.shaping_operations(), u64::from(budget));
            assert_eq!(measurer.layout.shaping_buffer_capacities(), (0, 0));
        }
        assert_eq!(measurer.measure(" ", 20.0, None).unwrap(), expected);
    }

    #[test]
    fn large_intermediate_expansion_stops_before_exceeding_the_array_budget() {
        let mut measurer = TextMeasurer::new(
            include_bytes!("../tests/fixtures/expansion.ttf").to_vec(),
            "Varta Expansion Fixture",
        )
        .unwrap();
        let small = format!("A{}", "\u{035c}".repeat(4));
        let expected = measurer.measure(&small, 20.0, None).unwrap();
        let large = format!("A{}", "\u{035c}".repeat(20_000));
        assert!(matches!(
            measurer.measure(&large, 20.0, None),
            Err(TextMeasureError::Limit)
        ));
        assert_eq!(
            measurer.layout.shaping_peak_buffer_capacities(),
            (MAX_SHAPING_BUFFER_SLOTS, MAX_SHAPING_BUFFER_SLOTS)
        );
        assert_eq!(
            measurer.layout.shaping_peak_buffer_bytes(),
            10 * 1024 * 1024
        );
        assert!(measurer.layout.shaping_operations() < u64::from(MAX_SHAPING_OPERATIONS));
        assert_eq!(measurer.layout.shaping_buffer_capacities(), (0, 0));
        eprintln!(
            "intermediate expansion refused: peak_bytes={}, operations={}, retained={:?}",
            measurer.layout.shaping_peak_buffer_bytes(),
            measurer.layout.shaping_operations(),
            measurer.layout.shaping_buffer_capacities()
        );
        assert_eq!(measurer.measure(&small, 20.0, None).unwrap(), expected);
    }

    #[test]
    fn shaping_buffer_boundary_releases_refused_scratch_and_recovers() {
        let mut measurer = cluster_measurer();
        measurer
            .layout
            .set_shaping_limits(MAX_SHAPED_GLYPHS, 64, MAX_SHAPING_OPERATIONS, 64);
        for (count, accepted) in [(21, true), (22, false), (4, true)] {
            let text = format!("A{}", "\u{035c}".repeat(count));
            assert_eq!(measurer.measure(&text, 20.0, None).is_ok(), accepted);
            let peak = measurer.layout.shaping_peak_buffer_capacities();
            assert!(peak.0 <= 64 && peak.1 <= 64, "{peak:?}");
            assert!(measurer.layout.shaping_peak_buffer_bytes() <= 64 * 40);
            let cached = measurer.layout.shaping_buffer_capacities();
            if accepted {
                assert!(cached.0 <= 64 && cached.1 <= 64);
            } else {
                assert_eq!(cached, (0, 0));
            }
        }
    }

    #[test]
    fn shaping_glyph_and_operation_budgets_are_shared_across_script_runs() {
        let text = format!("A{}ЖA{}", "\u{035c}".repeat(21), "\u{035c}".repeat(21));
        let mut measurer = cluster_measurer();
        measurer
            .layout
            .set_shaping_limits(129, 256, MAX_SHAPING_OPERATIONS, 256);
        let expected = measurer.measure(&text, 20.0, None).unwrap();
        let operations = u32::try_from(measurer.layout.shaping_operations()).unwrap();
        assert!(operations > 1);
        eprintln!(
            "three-run shaping operations={operations}, buffers={:?}",
            measurer.layout.shaping_peak_buffer_capacities()
        );
        measurer
            .layout
            .set_shaping_limits(128, 256, MAX_SHAPING_OPERATIONS, 256);
        measurer.layout = measurer.layout.clone();
        assert!(matches!(
            measurer.measure(&text, 20.0, None),
            Err(TextMeasureError::Limit)
        ));
        assert_eq!(measurer.layout.shaping_buffer_capacities(), (0, 0));
        measurer
            .layout
            .set_shaping_limits(129, 256, operations - 1, 256);
        assert!(matches!(
            measurer.measure(&text, 20.0, None),
            Err(TextMeasureError::Limit)
        ));
        assert_eq!(measurer.layout.shaping_buffer_capacities(), (0, 0));
        assert!(measurer.layout.shaping_operations() <= u64::from(operations - 1));
        measurer
            .layout
            .set_shaping_limits(129, 256, operations, 256);
        assert_eq!(measurer.measure(&text, 20.0, None).unwrap(), expected);
    }

    #[test]
    fn shaping_output_refusal_drops_scratch_and_maximum_output_remains_supported() {
        let mut measurer = cluster_measurer();
        for (count, accepted) in [(21_845, true), (21_846, false), (4, true)] {
            let text = format!("A{}", "\u{035c}".repeat(count));
            assert_eq!(measurer.measure(&text, 20.0, None).is_ok(), accepted);
            let peak = measurer.layout.shaping_peak_buffer_capacities();
            assert!(
                peak.0 <= MAX_SHAPING_BUFFER_SLOTS && peak.1 <= MAX_SHAPING_BUFFER_SLOTS,
                "{peak:?}"
            );
            let peak_bytes = measurer.layout.shaping_peak_buffer_bytes();
            assert!(peak_bytes <= 10 * 1024 * 1024, "{peak_bytes}");
            let retained = measurer.layout.shaping_buffer_capacities();
            if accepted {
                assert!(
                    retained.0 <= MAX_CACHED_SHAPING_SLOTS
                        && retained.1 <= MAX_CACHED_SHAPING_SLOTS
                );
            } else {
                assert_eq!(retained, (0, 0));
            }
            eprintln!(
                "marks={count}, accepted={accepted}, operations={}, peak={peak:?}, peak_bytes={peak_bytes}, retained={retained:?}",
                measurer.layout.shaping_operations()
            );
        }
    }

    #[test]
    fn long_cluster_font_coverage_includes_characters_after_the_old_scratch_prefix() {
        let mut measurer = TextMeasurer::new_with_fonts(vec![
            TextFont {
                data: include_bytes!("../tests/fixtures/clusters-partial.ttf").to_vec(),
                family: "Varta Cluster Partial".into(),
                aliases: vec![],
            },
            TextFont {
                data: include_bytes!("../tests/fixtures/clusters.ttf").to_vec(),
                family: "Varta Cluster Fixture".into(),
                aliases: vec![],
            },
        ])
        .unwrap();
        for count in [31, 32, 64, 253, 256, 1024] {
            let text = format!("A{}\u{035c}", "\u{035d}".repeat(count));
            let metrics = measurer
                .measure_with_family_list(
                    &text,
                    20.0,
                    None,
                    "\"Varta Cluster Partial\", \"Varta Cluster Fixture\"",
                )
                .unwrap();
            // The first font advances 18px and omits the final character;
            // only the fallback covers the complete grapheme and advances 8px.
            assert!(
                (metrics.width - 8.0).abs() < 0.01,
                "count={count}, {metrics:?}"
            );
        }
    }

    #[test]
    fn large_combining_clusters_retain_every_shaped_glyph() {
        let mut measurer = TextMeasurer::new(
            include_bytes!("../tests/fixtures/clusters.ttf").to_vec(),
            "Varta Cluster Fixture",
        )
        .unwrap();
        let families = vec!["Varta Cluster Fixture".into()];
        for (mark, multiplier, count) in [
            ('\u{035d}', 1, 253),
            ('\u{035d}', 1, 254),
            ('\u{035d}', 1, 255),
            ('\u{035d}', 1, 256),
            ('\u{035c}', 3, 300),
        ] {
            let text = format!("A{}", mark.to_string().repeat(count));
            let (_, shaped) = measurer
                .shape_styled(
                    &text,
                    None,
                    &families,
                    TextOptions {
                        size: 20.0,
                        line_height: None,
                        weight: 400.0,
                        italic: false,
                        letter_spacing: 0.0,
                        white_space: WhiteSpaceMode::NoWrap,
                        break_anywhere: false,
                        source: TextSource::General,
                        align: TextAlign::Start,
                    },
                )
                .unwrap();
            let mut glyphs = 0;
            for line in shaped.lines() {
                for item in line.items() {
                    let parley::PositionedLayoutItem::GlyphRun(run) = item else {
                        panic!("unexpected inline box")
                    };
                    glyphs += run.positioned_glyphs().count();
                }
            }
            assert_eq!(
                glyphs,
                1 + count * multiplier,
                "mark={mark:?}, count={count}"
            );
        }
    }
}
