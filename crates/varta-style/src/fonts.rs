//! The first available font of an element (CSS Fonts 4 §5) and its metrics,
//! the references of the font-relative units `ex`, `cap`, `ch`, `ic` and of
//! `lh` under `line-height: normal` (CSS Values 4 §6.1.1).
//!
//! One implementation for both hosts: a packaged face's metrics are read
//! here from its own tables; the families a host renders with its own
//! fonts (the CSS generic families, the platform's default font and, in a
//! native host's system-font set, the platform families themselves) are
//! measured by the host through [`HostFonts`], natively from the platform's
//! font system and on the web by the bridge through the browser's own
//! units. The fallbacks of Values 4 apply only when no font has metrics.
use crate::prelude::*;
use alloc::sync::Arc;
use read_fonts::tables::cmap::Cmap;
use read_fonts::tables::os2::SelectionFlags;
use read_fonts::types::{F2Dot14, Fixed, GlyphId, NameId, Tag};
use read_fonts::{FileRef, FontRef, TableProvider};
use varta_common::view::css::math::FontMetrics;
use varta_common::view::fonts::{MAX_FONT_BYTES, MAX_FONT_FACE_BYTES, MAX_FONT_FACES};
use varta_common::view::syntax::{self, Token};

/// The CSS generic families of the view subset (CSS Fonts 4 §2.1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Generic {
    Serif,
    SansSerif,
    Monospace,
    SystemUi,
}
impl Generic {
    pub const ALL: [Self; 4] = [
        Self::Serif,
        Self::SansSerif,
        Self::Monospace,
        Self::SystemUi,
    ];
    /// The keyword.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Serif => "serif",
            Self::SansSerif => "sans-serif",
            Self::Monospace => "monospace",
            Self::SystemUi => "system-ui",
        }
    }
    /// The generic an unquoted family keyword names (ASCII
    /// case-insensitively).
    #[must_use]
    pub fn parse(keyword: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|generic| generic.name().eq_ignore_ascii_case(keyword))
    }
}

/// One component of a `font-family` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Family {
    Named(String),
    Generic(Generic),
}

/// The families of a computed `font-family` value (CSS Fonts 4 §2.1):
/// strings, and sequences of identifiers joined by single spaces; a sole
/// unquoted generic keyword is that generic family. `None` for a value
/// outside that grammar.
#[must_use]
pub fn family_list(value: &str) -> Option<Vec<Family>> {
    let mut families = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut quoted: Option<String> = None;
    let mut finish = |words: &mut Vec<String>, quoted: &mut Option<String>| -> Option<()> {
        let family = match (quoted.take(), words.as_slice()) {
            (Some(text), []) => Family::Named(text),
            (None, [word]) => {
                Generic::parse(word).map_or_else(|| Family::Named(word.clone()), Family::Generic)
            }
            (None, words) if !words.is_empty() => Family::Named(words.join(" ")),
            _ => return None,
        };
        words.clear();
        families.push(family);
        Some(())
    };
    for spanned in syntax::tokenize(value) {
        match spanned.token {
            Token::Whitespace => {}
            Token::String(text) if words.is_empty() && quoted.is_none() => quoted = Some(text),
            Token::Ident(word) if quoted.is_none() => words.push(word),
            Token::Comma => finish(&mut words, &mut quoted)?,
            _ => return None,
        }
    }
    finish(&mut words, &mut quoted)?;
    Some(families)
}

/// The face a host is asked for (CSS Fonts 4 §5.2: the matched weight and
/// style; `font-stretch` is always `normal` in the subset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceStyle {
    pub weight: f64,
    pub italic: bool,
}
impl FaceStyle {
    /// `font-weight: normal; font-style: normal`.
    pub const NORMAL: Self = Self {
        weight: NORMAL_WEIGHT,
        italic: false,
    };
}

/// The CSS `normal` weight.
pub const NORMAL_WEIGHT: f64 = 400.0;

/// The metrics of one font at one size, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizedMetrics {
    /// `ex`, `cap`, `ch` and `ic`, each the font's own measure or the
    /// Values 4 fallback where the font lacks it (the cap-height falls back
    /// to the font's ascent, so it is always `Some`).
    pub units: FontMetrics,
    /// `line-height: normal` (CSS Inline 3 §4.2): the ascent, descent and
    /// line gap at this size, each rounded to whole pixels as the
    /// specification's oracle (Blink's `SimpleFontData`) does.
    pub line_spacing: f64,
}

/// A family the host renders with its own fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFamily<'a> {
    /// A named family of the host's own face set (a native system-font set
    /// registers the platform's families by name).
    Named(&'a str),
    Generic(Generic),
    /// The host's default font: the family of text without `font-family`
    /// where no packaged family is the default.
    Default,
}

/// A host's measurements of the fonts it renders with itself. A host
/// answers `None` for a family it does not render; the engine then tries
/// the next family of the list.
pub trait HostFonts: Send + Sync + core::fmt::Debug {
    fn metrics(&self, family: HostFamily<'_>, style: FaceStyle, size: f64) -> Option<SizedMetrics>;
}

/// Why a face is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceError {
    /// Its tables do not parse, or it has no `head` table.
    Unreadable,
    /// The set's face count or byte limits ([`MAX_FONT_FACES`],
    /// [`MAX_FONT_FACE_BYTES`], [`MAX_FONT_BYTES`]).
    Limit,
    /// An empty family name.
    Family,
}

/// The `OS/2` style class of a face (CSS Fonts 4 §5.2 step 4b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slope {
    Normal,
    Italic,
    Oblique,
}

/// Font-unit values of a static face, or of a variable face at its default
/// location; [`Self::sized`] scales them.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Units {
    per_em: f64,
    ascent: f64,
    /// Below the baseline, positive.
    descent: f64,
    line_gap: f64,
    x_height: Option<f64>,
    cap_height: Option<f64>,
    zero: Option<f64>,
    water: Option<f64>,
}

/// The variation axes Varta sets (CSS Fonts 4 §7: `opsz` from the used font
/// size under `font-optical-sizing: auto`; the matched weight; `ital` for
/// italic), as the native shaper sets them.
const WEIGHT_AXIS: Tag = Tag::new(b"wght");
const OPTICAL_SIZE_AXIS: Tag = Tag::new(b"opsz");
const ITALIC_AXIS: Tag = Tag::new(b"ital");
/// `MVAR` value tags (OpenType 1.9 MVAR): typographic ascender, descender
/// and line gap, x-height and cap-height.
const MVAR_ASCENT: Tag = Tag::new(b"hasc");
const MVAR_DESCENT: Tag = Tag::new(b"hdsc");
const MVAR_LINE_GAP: Tag = Tag::new(b"hlgp");
const MVAR_X_HEIGHT: Tag = Tag::new(b"xhgt");
const MVAR_CAP_HEIGHT: Tag = Tag::new(b"cpht");
/// The characters of `x`, `ch` and `ic` (CSS Values 4 §6.1.1).
const X: char = 'x';
const ZERO: char = '0';
const WATER: char = '\u{6C34}';
/// `usWidthClass` 1–9 as `font-stretch` percentages (OpenType `OS/2`).
const WIDTH_CLASSES: [f64; 9] = [50.0, 62.5, 75.0, 87.5, 100.0, 112.5, 125.0, 150.0, 200.0];
/// The `normal` stretch.
const NORMAL_STRETCH: f64 = 100.0;
/// The lower bound of the weights CSS Fonts 4 §5.2 step 4c searches
/// upwards first from (400 to 500 inclusive).
const WEIGHT_BAND: (f64, f64) = (400.0, 500.0);

/// A packaged face the engine reads.
#[derive(Clone, PartialEq)]
struct DataFace {
    /// The face's bytes, kept only for a variable face.
    variable: Option<(Arc<[u8]>, u32)>,
    units: Units,
    weight: (f64, f64),
    slope: Slope,
    stretch: f64,
}

#[derive(Clone, PartialEq)]
enum Source {
    Data(DataFace),
    /// A face the host measures by its family name.
    Host,
}

#[derive(Clone, PartialEq)]
struct Face {
    family: String,
    /// The other families (generic or named) this face's family stands for.
    aliases: Vec<Family>,
    source: Source,
}

/// The faces of one view: the packaged faces the engine reads, the host's
/// own named faces, the default family and the host's measurements.
#[derive(Clone, Default)]
pub struct FontSet {
    faces: Vec<Face>,
    /// The bytes of the files the faces came from.
    bytes: usize,
    default_family: Option<String>,
    host: Option<Arc<dyn HostFonts>>,
}
impl core::fmt::Debug for FontSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FontSet")
            .field(
                "faces",
                &self
                    .faces
                    .iter()
                    .map(|face| &face.family)
                    .collect::<Vec<_>>(),
            )
            .field("default_family", &self.default_family)
            .field("host", &self.host.is_some())
            .finish()
    }
}

impl FontSet {
    /// A set whose text without `font-family` takes `default_family` (a
    /// family of the set) or else the host's default font.
    #[must_use]
    pub fn new(default_family: Option<String>, host: Option<Arc<dyn HostFonts>>) -> Self {
        Self {
            faces: Vec::new(),
            bytes: 0,
            default_family,
            host,
        }
    }
    /// Set the host's measurements.
    pub fn set_host(&mut self, host: Option<Arc<dyn HostFonts>>) {
        self.host = host;
    }
    /// Set the default family.
    pub fn set_default_family(&mut self, family: Option<String>) {
        self.default_family = family;
    }

    fn admit(&self, family: &str, faces: usize, bytes: usize) -> Result<(), FaceError> {
        if family.is_empty() {
            return Err(FaceError::Family);
        }
        if self.faces.len().saturating_add(faces) > MAX_FONT_FACES
            || bytes > MAX_FONT_FACE_BYTES
            || self.bytes.saturating_add(bytes) > MAX_FONT_BYTES
        {
            return Err(FaceError::Limit);
        }
        Ok(())
    }

    /// Add the faces of `family` in a packaged font file (`data`: one face,
    /// or a collection whose faces of that family are taken), standing for
    /// `aliases` too. Each face's weight, style and stretch are its `OS/2`
    /// table's (the resource section checks that they are the declared
    /// ones). Returns the number of faces added.
    ///
    /// # Errors
    /// An unreadable file, a collection without a face of `family`, or the
    /// set's limits.
    pub fn add_file(
        &mut self,
        family: &str,
        aliases: &[Family],
        data: &[u8],
    ) -> Result<usize, FaceError> {
        let file = FileRef::new(data).map_err(|_| FaceError::Unreadable)?;
        let single = matches!(file, FileRef::Font(_));
        let mut faces = Vec::new();
        for (index, font) in (0u32..).zip(file.fonts()) {
            let font = font.map_err(|_| FaceError::Unreadable)?;
            if single
                || family_names(&font)
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(family))
            {
                faces.push(data_face(&font, data, index)?);
            }
        }
        if faces.is_empty() {
            return Err(FaceError::Unreadable);
        }
        self.admit(family, faces.len(), data.len())?;
        self.bytes += data.len();
        let added = faces.len();
        self.faces.extend(faces.into_iter().map(|face| Face {
            family: family.to_owned(),
            aliases: aliases.to_vec(),
            source: Source::Data(face),
        }));
        Ok(added)
    }

    /// Add a family the host renders and measures itself (a native
    /// system-font set's platform family), standing for `aliases`.
    ///
    /// # Errors
    /// An empty family or the set's face limit.
    pub fn add_host_family(&mut self, family: &str, aliases: &[Family]) -> Result<(), FaceError> {
        self.admit(family, 1, 0)?;
        self.faces.push(Face {
            family: family.to_owned(),
            aliases: aliases.to_vec(),
            source: Source::Host,
        });
        Ok(())
    }

    /// The faces of the set named `family` (by name, or by a generic alias).
    fn faces_of(&self, family: &Family) -> impl Iterator<Item = &Face> {
        self.faces.iter().filter(move |face| {
            let named = |name: &str| match family {
                Family::Named(wanted) => wanted.eq_ignore_ascii_case(name),
                Family::Generic(_) => false,
            };
            named(&face.family)
                || face.aliases.iter().any(|alias| match alias {
                    Family::Named(alias) => named(alias),
                    generic => generic == family,
                })
        })
    }

    /// The metrics of `family`'s face for `style` at `size`, or `None` when
    /// neither the set nor the host has a face for it.
    fn family_metrics(&self, family: &Family, style: FaceStyle, size: f64) -> Option<SizedMetrics> {
        let faces: Vec<&Face> = self.faces_of(family).collect();
        if let Some(first) = faces.first() {
            if matches!(first.source, Source::Host) {
                return self
                    .host
                    .as_ref()?
                    .metrics(HostFamily::Named(&first.family), style, size);
            }
            let data: Vec<&DataFace> = faces
                .iter()
                .filter_map(|face| match &face.source {
                    Source::Data(data) => Some(data),
                    Source::Host => None,
                })
                .collect();
            return matched(&data, style)?.sized(style, size);
        }
        match family {
            Family::Generic(generic) => {
                self.host
                    .as_ref()?
                    .metrics(HostFamily::Generic(*generic), style, size)
            }
            Family::Named(_) => None,
        }
    }

    /// The metrics of the first available font of `families` (a computed
    /// `font-family` value; `None` for the initial family) for `style` at
    /// `size` CSS pixels: the first family with a face, else the default
    /// family, else the host's default font. `None` when no font has
    /// metrics.
    #[must_use]
    pub fn metrics(
        &self,
        families: Option<&str>,
        style: FaceStyle,
        size: f64,
    ) -> Option<SizedMetrics> {
        if !size.is_finite() || size <= 0.0 {
            return None;
        }
        for family in families.and_then(family_list).unwrap_or_default() {
            if let Some(metrics) = self.family_metrics(&family, style, size) {
                return Some(metrics);
            }
        }
        if let Some(default) = &self.default_family
            && let Some(metrics) = self.family_metrics(&Family::Named(default.clone()), style, size)
        {
            return Some(metrics);
        }
        self.host
            .as_ref()?
            .metrics(HostFamily::Default, style, size)
    }
}

/// The family names of `font` (`name` IDs 1 and 16).
fn family_names(font: &FontRef<'_>) -> Vec<String> {
    let Ok(name) = font.name() else {
        return Vec::new();
    };
    name.name_record()
        .iter()
        .filter(|record| {
            matches!(
                record.name_id(),
                NameId::FAMILY_NAME | NameId::TYPOGRAPHIC_FAMILY_NAME
            )
        })
        .filter_map(|record| record.string(name.string_data()).ok())
        .map(|text| text.chars().collect())
        .collect()
}

/// A packaged face of `data` (face `index`).
fn data_face(font: &FontRef<'_>, data: &[u8], index: u32) -> Result<DataFace, FaceError> {
    let units = units(font, &[]).ok_or(FaceError::Unreadable)?;
    let os2 = font.os2().ok();
    let slope = match os2.as_ref().map(|os2| os2.fs_selection()) {
        Some(flags) if flags.contains(SelectionFlags::ITALIC) => Slope::Italic,
        Some(flags) if flags.contains(SelectionFlags::OBLIQUE) => Slope::Oblique,
        _ => Slope::Normal,
    };
    let class = os2
        .as_ref()
        .map_or(NORMAL_WEIGHT, |os2| f64::from(os2.us_weight_class()));
    let axes = font.fvar().ok().and_then(|fvar| fvar.axes().ok());
    let weight = axes
        .and_then(|axes| {
            axes.iter()
                .find(|axis| axis.axis_tag() == WEIGHT_AXIS)
                .map(|axis| (axis.min_value().to_f64(), axis.max_value().to_f64()))
        })
        .unwrap_or((class, class));
    let stretch = os2
        .as_ref()
        .and_then(|os2| WIDTH_CLASSES.get(usize::from(os2.us_width_class()).checked_sub(1)?))
        .copied()
        .unwrap_or(NORMAL_STRETCH);
    let variable = font
        .fvar()
        .is_ok_and(|fvar| fvar.axis_count() > 0)
        .then(|| (Arc::<[u8]>::from(data), index));
    Ok(DataFace {
        variable,
        units,
        weight,
        slope,
        stretch,
    })
}

/// CSS Fonts 4 §5.2 step 4: the face of a family for `style` (stretch,
/// then style, then weight).
fn matched<'a>(faces: &[&'a DataFace], style: FaceStyle) -> Option<&'a DataFace> {
    // 4a: `normal` stretch; else the nearest narrower, else wider.
    let stretch = faces
        .iter()
        .map(|face| face.stretch)
        .filter(|stretch| *stretch <= NORMAL_STRETCH)
        .fold(None, |best: Option<f64>, stretch| {
            Some(best.map_or(stretch, |best| best.max(stretch)))
        })
        .or_else(|| {
            faces
                .iter()
                .map(|face| face.stretch)
                .fold(None, |best: Option<f64>, stretch| {
                    Some(best.map_or(stretch, |best| best.min(stretch)))
                })
        })?;
    let faces: Vec<&DataFace> = faces
        .iter()
        .copied()
        .filter(|face| face.stretch == stretch)
        .collect();
    // 4b: italic, then oblique, then normal; or normal, oblique, italic.
    let order = if style.italic {
        [Slope::Italic, Slope::Oblique, Slope::Normal]
    } else {
        [Slope::Normal, Slope::Oblique, Slope::Italic]
    };
    let slope = order
        .into_iter()
        .find(|slope| faces.iter().any(|face| face.slope == *slope))?;
    let faces: Vec<&DataFace> = faces
        .into_iter()
        .filter(|face| face.slope == slope)
        .collect();
    // 4c: weights.
    let desired = style.weight;
    if let Some(face) = faces
        .iter()
        .find(|face| face.weight.0 <= desired && desired <= face.weight.1)
    {
        return Some(face);
    }
    let below = |limit: f64| {
        faces
            .iter()
            .filter(move |face| face.weight.1 < desired && face.weight.1 >= limit)
            .max_by(|a, b| a.weight.1.total_cmp(&b.weight.1))
            .copied()
    };
    let above = |limit: f64| {
        faces
            .iter()
            .filter(move |face| face.weight.0 > desired && face.weight.0 <= limit)
            .min_by(|a, b| a.weight.0.total_cmp(&b.weight.0))
            .copied()
    };
    if (WEIGHT_BAND.0..=WEIGHT_BAND.1).contains(&desired) {
        above(WEIGHT_BAND.1)
            .or_else(|| below(f64::NEG_INFINITY))
            .or_else(|| above(f64::INFINITY))
    } else if desired < WEIGHT_BAND.0 {
        below(f64::NEG_INFINITY).or_else(|| above(f64::INFINITY))
    } else {
        above(f64::INFINITY).or_else(|| below(f64::NEG_INFINITY))
    }
}

impl DataFace {
    /// The face's metrics for `style` at `size` CSS pixels: a variable face
    /// at its location for them.
    fn sized(&self, style: FaceStyle, size: f64) -> Option<SizedMetrics> {
        let units = match &self.variable {
            None => self.units,
            Some((data, index)) => {
                let font = FontRef::from_index(data, *index).ok()?;
                let coords = location(&font, style, size, self.weight);
                units(&font, &coords)?
            }
        };
        Some(units.sized(size))
    }
}

/// The normalized variation coordinates of `font` for `style` at `size`.
fn location(font: &FontRef<'_>, style: FaceStyle, size: f64, weight: (f64, f64)) -> Vec<F2Dot14> {
    let Ok(fvar) = font.fvar() else {
        return Vec::new();
    };
    let mut coords = vec![F2Dot14::ZERO; usize::from(fvar.axis_count())];
    let user = [
        (WEIGHT_AXIS, style.weight.clamp(weight.0, weight.1)),
        (OPTICAL_SIZE_AXIS, size),
        (ITALIC_AXIS, if style.italic { 1.0 } else { 0.0 }),
    ];
    let avar = font.avar().ok();
    fvar.user_to_normalized(
        avar.as_ref(),
        user.into_iter()
            .map(|(tag, value)| (tag, Fixed::from_f64(value))),
        &mut coords,
    );
    coords
}

/// The used ascent, descent and line gap in font units, as the native
/// shaper's (Skrifa's, FreeType's) selection: `OS/2` typographic metrics
/// under `USE_TYPO_METRICS`, else `hhea`, else typographic, else Windows.
fn vertical(font: &FontRef<'_>) -> (f64, f64, f64) {
    let os2 = font.os2().ok();
    if let Some(os2) = &os2
        && os2
            .fs_selection()
            .contains(SelectionFlags::USE_TYPO_METRICS)
    {
        return (
            f64::from(os2.s_typo_ascender()),
            -f64::from(os2.s_typo_descender()),
            f64::from(os2.s_typo_line_gap()),
        );
    }
    let hhea = font.hhea().ok().map(|hhea| {
        (
            f64::from(hhea.ascender().to_i16()),
            -f64::from(hhea.descender().to_i16()),
            f64::from(hhea.line_gap().to_i16()),
        )
    });
    match (hhea, &os2) {
        (Some((ascent, descent, gap)), _) if ascent != 0.0 || descent != 0.0 => {
            (ascent, descent, gap)
        }
        (_, Some(os2)) if os2.s_typo_ascender() != 0 || os2.s_typo_descender() != 0 => (
            f64::from(os2.s_typo_ascender()),
            -f64::from(os2.s_typo_descender()),
            f64::from(os2.s_typo_line_gap()),
        ),
        (_, Some(os2)) => (
            f64::from(os2.us_win_ascent()),
            f64::from(os2.us_win_descent()),
            0.0,
        ),
        (hhea, None) => hhea.unwrap_or((0.0, 0.0, 0.0)),
    }
}

/// The advance of `character`'s glyph in font units at `coords` (`HVAR`
/// deltas truncated, as the native shaper applies them), or `None` when the
/// face does not map it.
fn advance(
    font: &FontRef<'_>,
    cmap: Option<&Cmap<'_>>,
    character: char,
    coords: &[F2Dot14],
) -> Option<f64> {
    let glyph = cmap?.map_codepoint(character)?;
    let mut advance = f64::from(font.hmtx().ok()?.advance(glyph)?);
    if !coords.is_empty()
        && let Ok(hvar) = font.hvar()
        && let Ok(delta) = hvar.advance_width_delta(glyph, coords)
    {
        advance += libm::trunc(delta.to_f64());
    }
    Some(advance)
}

/// The top of `glyph`'s outline in font units (its `glyf` bounding box at
/// the default location), when it has one.
fn glyph_top(font: &FontRef<'_>, glyph: GlyphId) -> Option<f64> {
    let glyf = font.glyf().ok()?;
    let outline = font.loca(None).ok()?.get_glyf(glyph, &glyf).ok()??;
    let top = f64::from(outline.y_max());
    (top > 0.0).then_some(top)
}

/// The font-unit metrics of `font` at `coords`.
fn units(font: &FontRef<'_>, coords: &[F2Dot14]) -> Option<Units> {
    let per_em = f64::from(font.head().ok()?.units_per_em());
    if per_em <= 0.0 {
        return None;
    }
    let (mut ascent, mut descent, mut line_gap) = vertical(font);
    let os2 = font.os2().ok();
    let positive = |value: Option<i16>| value.map(f64::from).filter(|value| *value > 0.0);
    let mut x_height = positive(os2.as_ref().and_then(|os2| os2.sx_height()));
    let mut cap_height = positive(os2.as_ref().and_then(|os2| os2.s_cap_height()));
    if !coords.is_empty()
        && let Ok(mvar) = font.mvar()
    {
        let delta = |tag| {
            mvar.metric_delta(tag, coords)
                .map_or(0.0, |delta| delta.to_f64())
        };
        ascent += delta(MVAR_ASCENT);
        descent -= delta(MVAR_DESCENT);
        line_gap += delta(MVAR_LINE_GAP);
        x_height = x_height.map(|value| value + delta(MVAR_X_HEIGHT));
        cap_height = cap_height.map(|value| value + delta(MVAR_CAP_HEIGHT));
    }
    let cmap = font.cmap().ok();
    // Values 4 §6.1.1: without a reliable x-height metric, the height of
    // the lowercase `x`.
    let x_height = x_height.or_else(|| glyph_top(font, cmap.as_ref()?.map_codepoint(X)?));
    Some(Units {
        per_em,
        ascent,
        descent,
        line_gap,
        x_height,
        cap_height,
        zero: advance(font, cmap.as_ref(), ZERO, coords),
        water: advance(font, cmap.as_ref(), WATER, coords),
    })
}

impl Units {
    /// The metrics at `size` CSS pixels, with the Values 4 fallbacks for
    /// what the face lacks.
    fn sized(&self, size: f64) -> SizedMetrics {
        let scale = size / self.per_em;
        let ascent = self.ascent * scale;
        SizedMetrics {
            units: FontMetrics {
                ex: self.x_height.map_or(size / 2.0, |value| value * scale),
                cap: Some(self.cap_height.map_or(ascent, |value| value * scale)),
                ch: self.zero.map_or(size / 2.0, |value| value * scale),
                ic: self.water.map_or(size, |value| value * scale),
            },
            line_spacing: libm::round(ascent)
                + libm::round(self.descent * scale)
                + libm::round(self.line_gap * scale),
        }
    }
}

/// The metrics of face `index` of font file `data` for `style` at `size`
/// CSS pixels, read from its own tables as a packaged face's are: for a
/// host that measures a platform face partly from its tables.
#[must_use]
pub fn face_metrics(data: &[u8], index: u32, style: FaceStyle, size: f64) -> Option<SizedMetrics> {
    let font = FontRef::from_index(data, index).ok()?;
    data_face(&font, data, index).ok()?.sized(style, size)
}

/// The glyph face `index` of font file `data` maps `character` to.
#[must_use]
pub fn glyph_id(data: &[u8], index: u32, character: char) -> Option<u32> {
    let font = FontRef::from_index(data, index).ok()?;
    Some(font.cmap().ok()?.map_codepoint(character)?.to_u32())
}

/// [`SizedMetrics`] from a host's measures, refusing values that are not
/// finite nonnegative lengths.
#[must_use]
pub fn host_metrics(
    ex: f64,
    cap: f64,
    ch: f64,
    ic: f64,
    line_spacing: f64,
) -> Option<SizedMetrics> {
    let valid = |value: f64| value.is_finite() && value >= 0.0;
    [ex, cap, ch, ic, line_spacing]
        .into_iter()
        .all(valid)
        .then_some(SizedMetrics {
            units: FontMetrics {
                ex,
                cap: Some(cap),
                ch,
                ic,
            },
            line_spacing,
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn family_lists_follow_the_font_family_grammar() {
        assert_eq!(
            family_list("\"A B\", Times  New Roman, MONOSPACE, \"serif\""),
            Some(vec![
                Family::Named("A B".into()),
                Family::Named("Times New Roman".into()),
                Family::Generic(Generic::Monospace),
                Family::Named("serif".into()),
            ])
        );
        assert_eq!(family_list("a,"), None);
        assert_eq!(family_list("\"a\" b"), None);
        assert_eq!(family_list("1px"), None);
    }

    fn face(weight: (f64, f64), slope: Slope, stretch: f64) -> DataFace {
        DataFace {
            variable: None,
            units: Units {
                per_em: 1000.0,
                ascent: 800.0,
                descent: 200.0,
                line_gap: 0.0,
                x_height: None,
                cap_height: None,
                zero: None,
                water: None,
            },
            weight,
            slope,
            stretch,
        }
    }

    /// CSS Fonts 4 §5.2 step 4.
    #[test]
    fn matching_prefers_stretch_then_style_then_weight() {
        let faces = [
            face((300.0, 300.0), Slope::Normal, 100.0),
            face((400.0, 400.0), Slope::Normal, 100.0),
            face((600.0, 600.0), Slope::Normal, 100.0),
            face((900.0, 900.0), Slope::Normal, 100.0),
            face((400.0, 400.0), Slope::Oblique, 100.0),
            face((700.0, 700.0), Slope::Normal, 75.0),
        ];
        let faces: Vec<&DataFace> = faces.iter().collect();
        let pick = |weight, italic| {
            matched(&faces, FaceStyle { weight, italic }).map(|face| (face.weight.0, face.slope))
        };
        assert_eq!(pick(400.0, false), Some((400.0, Slope::Normal)));
        // 400–500: up to 500, then down, then up.
        assert_eq!(pick(450.0, false), Some((400.0, Slope::Normal)));
        assert_eq!(pick(500.0, false), Some((400.0, Slope::Normal)));
        // Below 400: down, then up; above 500: up, then down.
        assert_eq!(pick(350.0, false), Some((300.0, Slope::Normal)));
        assert_eq!(pick(200.0, false), Some((300.0, Slope::Normal)));
        assert_eq!(pick(700.0, false), Some((900.0, Slope::Normal)));
        assert_eq!(pick(950.0, false), Some((900.0, Slope::Normal)));
        // Italic takes the oblique face before a normal one.
        assert_eq!(pick(700.0, true), Some((400.0, Slope::Oblique)));
        // A weight range contains the desired weight.
        let variable = [face((100.0, 900.0), Slope::Normal, 100.0)];
        let variable: Vec<&DataFace> = variable.iter().collect();
        assert!(
            matched(
                &variable,
                FaceStyle {
                    weight: 650.0,
                    italic: false
                }
            )
            .is_some()
        );
    }

    #[test]
    fn sizes_round_line_spacing_and_apply_fallbacks() {
        let units = face((400.0, 400.0), Slope::Normal, 100.0).units;
        let sized = Units {
            ascent: 830.0,
            descent: 170.0,
            line_gap: 90.0,
            x_height: Some(520.0),
            cap_height: Some(700.0),
            zero: Some(555.0),
            ..units
        }
        .sized(16.0);
        // 13.28 + 2.72 + 1.44 rounds to 13 + 3 + 1.
        assert_eq!(sized.line_spacing, 17.0);
        assert_eq!(sized.units.ex, 8.32);
        assert_eq!(sized.units.ch, 8.88);
        assert_eq!(sized.units.ic, 16.0);
        let bare = units.sized(20.0);
        assert_eq!(
            bare.units,
            FontMetrics {
                ex: 10.0,
                cap: Some(16.0),
                ch: 10.0,
                ic: 20.0
            }
        );
        assert!(host_metrics(1.0, 1.0, f64::NAN, 1.0, 1.0).is_none());
    }

    pub(crate) const REGULAR: &[u8] = include_bytes!("../tests/fixtures/metrics.ttf");
    pub(crate) const BOLD: &[u8] = include_bytes!("../tests/fixtures/metrics-bold.ttf");
    const BARE: &[u8] = include_bytes!("../tests/fixtures/metrics-bare.ttf");
    const VARIABLE: &[u8] = include_bytes!("../tests/fixtures/metrics-variable.ttf");

    /// A host that renders every generic and its default with one font.
    #[derive(Debug)]
    struct Host;
    impl HostFonts for Host {
        fn metrics(
            &self,
            family: HostFamily<'_>,
            style: FaceStyle,
            size: f64,
        ) -> Option<SizedMetrics> {
            let line = match family {
                HostFamily::Generic(Generic::Serif) | HostFamily::Default => 1.0,
                HostFamily::Named("Platform") => 2.0,
                _ => return None,
            };
            host_metrics(
                size * 0.25,
                size * style.weight / 1000.0,
                size * 0.5,
                size,
                line * size,
            )
        }
    }

    pub(crate) fn set() -> FontSet {
        let mut set = FontSet::new(Some("Varta Metrics".into()), Some(Arc::new(Host)));
        let monospace = [
            Family::Generic(Generic::Monospace),
            Family::Named("Alias".into()),
        ];
        set.add_file("Varta Metrics", &monospace, REGULAR).unwrap();
        set.add_file("Varta Metrics", &[], BOLD).unwrap();
        set.add_file("Varta Metrics Bare", &[], BARE).unwrap();
        set.add_file("Varta Metrics Variable", &[], VARIABLE)
            .unwrap();
        set.add_host_family("Platform", &[]).unwrap();
        set
    }

    /// The fixtures' values, which Chrome reproduces for the same bytes
    /// (`native_css_engine_probe`'s font scenes).
    #[test]
    fn packaged_faces_give_their_tables_metrics() {
        let set = set();
        let normal = FaceStyle::NORMAL;
        let at = |families: &str, style: FaceStyle, size: f64| {
            set.metrics(Some(families), style, size).unwrap()
        };
        let regular = at("\"Varta Metrics\"", normal, 16.0);
        assert_eq!(
            regular.units,
            FontMetrics {
                ex: 8.32,
                cap: Some(11.200000000000001),
                ch: 8.88,
                ic: 15.52
            }
        );
        assert_eq!(regular.line_spacing, 17.0);
        // Weights above 500 search upwards first; below 400 downwards, then up.
        let bold = at(
            "Varta Metrics",
            FaceStyle {
                weight: 600.0,
                italic: false,
            },
            16.0,
        );
        assert_eq!((bold.units.ex, bold.line_spacing), (8.64, 18.0));
        assert_eq!(
            at(
                "Varta Metrics",
                FaceStyle {
                    weight: 300.0,
                    italic: true
                },
                16.0
            ),
            regular
        );
        // A missing family is skipped; the bare face takes every fallback,
        // its cap-height the ascent.
        let bare = at("Missing, \"Varta Metrics Bare\", serif", normal, 16.0);
        assert_eq!(
            bare.units,
            FontMetrics {
                ex: 8.0,
                cap: Some(14.4),
                ch: 8.0,
                ic: 16.0
            }
        );
        assert_eq!(bare.line_spacing, 19.0);
        // A generic or named alias names its packaged family.
        assert_eq!(at("monospace", normal, 16.0), regular);
        assert_eq!(at("alias", normal, 16.0), regular);
        // The variable face at the matched weight (`MVAR`, `HVAR`).
        let light = at(
            "\"Varta Metrics Variable\"",
            FaceStyle {
                weight: 100.0,
                italic: false,
            },
            100.0,
        );
        let middle = at(
            "\"Varta Metrics Variable\"",
            FaceStyle {
                weight: 500.0,
                italic: false,
            },
            100.0,
        );
        let heavy = at(
            "\"Varta Metrics Variable\"",
            FaceStyle {
                weight: 900.0,
                italic: false,
            },
            100.0,
        );
        assert_eq!((light.units.ex, light.units.ch), (48.0, 50.0));
        assert_eq!((middle.units.ex, middle.units.ch), (52.0, 60.0));
        assert_eq!(
            (heavy.units.ex, heavy.units.cap, heavy.units.ch),
            (56.0, Some(74.0), 70.0)
        );
    }

    #[test]
    fn host_families_generics_and_the_default_go_to_the_host() {
        let set = set();
        let normal = FaceStyle::NORMAL;
        // An unaliased generic, a host family by name, the default family.
        assert_eq!(
            set.metrics(Some("serif"), normal, 20.0)
                .unwrap()
                .line_spacing,
            20.0
        );
        assert_eq!(
            set.metrics(Some("platform"), normal, 20.0)
                .unwrap()
                .line_spacing,
            40.0
        );
        assert_eq!(
            set.metrics(Some("sans-serif"), normal, 16.0)
                .unwrap()
                .line_spacing,
            17.0
        );
        assert_eq!(set.metrics(None, normal, 16.0).unwrap().line_spacing, 17.0);
        let host = FontSet::new(None, Some(Arc::new(Host)));
        assert_eq!(
            host.metrics(Some("Missing"), normal, 10.0)
                .unwrap()
                .line_spacing,
            10.0
        );
        assert_eq!(
            FontSet::default().metrics(Some("serif"), normal, 10.0),
            None
        );
        assert_eq!(set.metrics(Some("serif"), normal, f64::NAN), None);
    }

    #[test]
    fn faces_are_bounded_and_checked() {
        let mut set = FontSet::default();
        assert_eq!(
            set.add_file("A", &[], b"not a font"),
            Err(FaceError::Unreadable)
        );
        assert_eq!(set.add_file("", &[], REGULAR), Err(FaceError::Family));
        for _ in 0..MAX_FONT_FACES {
            set.add_file("A", &[], REGULAR).unwrap();
        }
        assert_eq!(set.add_file("A", &[], REGULAR), Err(FaceError::Limit));
        assert_eq!(set.add_host_family("B", &[]), Err(FaceError::Limit));
    }
}
