//! CoreText glyph coverage from the exact font bytes already selected by Parley.
//! No system font lookup or process-wide font registration occurs here: a
//! collection member is rebuilt as a standalone face from its own tables, and a
//! variation instance is a CoreGraphics copy of that face at the user values
//! whose normalization equals the shaper's normalized coordinates. CoreGraphics
//! keys those values by axis names of its own choosing and silently ignores a
//! key it does not know, so each key is the name of CoreGraphics's own record
//! at the axis's `fvar` index, whose tag and range are checked, and every
//! instance is read back before use. CoreText's axis names follow the
//! process's preferred language and are never compared.
use crate::sfnt;
#[cfg(test)]
mod tests;
use core_foundation::{base::CFType, dictionary::CFDictionary, number::CFNumber, string::CFString};
use core_graphics::{
    base::kCGImageAlphaNoneSkipLast,
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    data_provider::CGDataProvider,
    font::CGFont,
    geometry::{CGPoint, CGRect, CGSize},
};
use core_text::{
    font::{self, CTFont},
    font_descriptor::kCTFontOrientationHorizontal,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use varta_view::{GlyphRasterBounds, GlyphRasterMask, GlyphRasterRequest, GlyphRasterizer};

/// Bitmap and color glyph tables. Coverage is one alpha channel, so these
/// faces keep the portable outline path rather than lose their colors.
const COLOR_TABLES: [[u8; 4]; 5] = [*b"COLR", *b"CBDT", *b"EBDT", *b"sbix", *b"SVG "];
/// CoreText sizes `opsz` from the point size; Parley keeps the axis default.
const OPTICAL_SIZE_AXIS: [u8; 4] = *b"opsz";
/// Distinct variation instances kept per face; the oldest is replaced.
const MAX_FACE_VARIANTS: usize = 16;
/// Normalized F2Dot14 coordinate scale.
const F2DOT14_ONE: f64 = 16384.0;
/// Quarter-pixel horizontal phases (0..=3) of a request.
const MAX_PHASE: u8 = 3;
const PHASES: f64 = 4.0;
/// CoreText face size before `clone_with_font_size` selects the request size.
const INSTANCE_POINT_SIZE: f64 = 16.0;
/// Keys of CoreGraphics's variation-axis records. Each exported constant
/// (`kCGFontVariationAxisName`, ...) has its own name as its value; the safe
/// wrappers do not bind the constants.
const AXIS_NAME_KEY: &str = "kCGFontVariationAxisName";
/// Min, default and max keys of a CoreGraphics axis record.
const CG_AXIS_RANGE_KEYS: [&str; 3] = [
    "kCGFontVariationAxisMinValue",
    "kCGFontVariationAxisDefaultValue",
    "kCGFontVariationAxisMaxValue",
];
/// Values of CoreText's `kCTFontVariationAxisIdentifierKey` (the `fvar` tag
/// as a number) and its min/default/max keys, which the safe wrappers do not
/// export. Its name key is not read: the name is localised.
const CT_AXIS_TAG_KEY: &str = "NSCTVariationAxisIdentifier";
const CT_AXIS_RANGE_KEYS: [&str; 3] = [
    "NSCTVariationAxisMinimumValue",
    "NSCTVariationAxisDefaultValue",
    "NSCTVariationAxisMaximumValue",
];
/// One 16.16 `fvar` unit: CoreGraphics's axis ranges and read-back values
/// must equal the expected ones to this precision.
const AXIS_VALUE_TOLERANCE: f64 = 1.0 / 65536.0;
/// Native memory charged per retained variation instance. CoreGraphics and
/// CoreText share the face bytes between instances; a copy plus one drawn
/// glyph measured 6 to 20 KiB of process footprint per instance on macOS 27
/// (faces of 1 KiB to 7.5 MiB), so this keeps headroom over that observation.
const INSTANCE_NATIVE_BYTES: usize = 32 * 1024;

/// Glyph requests this rasterizer declined, by reason, and the platform faces
/// it created. A declined request keeps Varta's portable outline.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CoreTextCounts {
    /// Requests for faces with bitmap or color glyph tables.
    pub declined_color: usize,
    /// Requests for faces with an optical-size axis.
    pub declined_optical_size: usize,
    /// Malformed or unsupported faces, members or variation data.
    pub declined_unsupported: usize,
    /// Variation instances CoreGraphics cannot apply exactly: the platform
    /// records at an `fvar` index carry another tag or range, two
    /// CoreGraphics records share a name (and so a variation key), the counts
    /// differ, or the instance reads back other values.
    pub declined_variation: usize,
    /// Faces that do not fit the shared glyph-cache allowance.
    pub declined_resource: usize,
    /// Invalid phase or size.
    pub declined_request: usize,
    /// Faces created from a collection member other than a whole file.
    pub collection_members: usize,
    /// Variation instances created at nondefault coordinates.
    pub variation_instances: usize,
}

/// Shared counters, readable after the rasterizer moves into a renderer.
#[derive(Debug, Default)]
pub struct CoreTextStatistics {
    declined_color: AtomicUsize,
    declined_optical_size: AtomicUsize,
    declined_unsupported: AtomicUsize,
    declined_variation: AtomicUsize,
    declined_resource: AtomicUsize,
    declined_request: AtomicUsize,
    collection_members: AtomicUsize,
    variation_instances: AtomicUsize,
}
impl CoreTextStatistics {
    #[must_use]
    pub fn counts(&self) -> CoreTextCounts {
        let read = |counter: &AtomicUsize| counter.load(Ordering::Relaxed);
        CoreTextCounts {
            declined_color: read(&self.declined_color),
            declined_optical_size: read(&self.declined_optical_size),
            declined_unsupported: read(&self.declined_unsupported),
            declined_variation: read(&self.declined_variation),
            declined_resource: read(&self.declined_resource),
            declined_request: read(&self.declined_request),
            collection_members: read(&self.collection_members),
            variation_instances: read(&self.variation_instances),
        }
    }
    fn add(counter: &AtomicUsize) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
    fn decline(&self, reason: Decline) {
        Self::add(match reason {
            Decline::Color => &self.declined_color,
            Decline::OpticalSize => &self.declined_optical_size,
            Decline::Unsupported => &self.declined_unsupported,
            Decline::Variation => &self.declined_variation,
            Decline::Resource => &self.declined_resource,
            Decline::Request => &self.declined_request,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decline {
    Color,
    OpticalSize,
    Unsupported,
    Variation,
    Resource,
    Request,
}

/// A face usable for variation instances: its standalone bytes stay shared
/// with the CoreGraphics data provider.
struct Variable {
    axes: Vec<sfnt::Axis>,
    data: Arc<Vec<u8>>,
    /// Resolved on the first instance; a refusal is kept for the face.
    graphics: Option<Result<Graphics, Decline>>,
    variants: BTreeMap<Vec<i16>, (CTFont, usize)>,
}
/// The CoreGraphics face and its own variation key for each `fvar` axis. Keys
/// are kept as Rust strings: CoreFoundation strings are not `Send`.
struct Graphics {
    font: CGFont,
    keys: Vec<String>,
}
struct Face {
    base: Result<CTFont, Decline>,
    variable: Option<Variable>,
    bytes: usize,
    stamp: usize,
}

/// The face at `index` of an explicit font file or collection, rebuilt as a
/// standalone file with the same tables: the face this rasterizer instantiates
/// for a collection member. For diagnostics and browser oracles.
#[must_use]
pub fn standalone_face(data: &[u8], index: u32) -> Option<Vec<u8>> {
    sfnt::Face::parse(data, index)?.standalone()
}

#[derive(Default)]
pub struct CoreTextRasterizer {
    faces: BTreeMap<(u64, u32), Face>,
    bytes: usize,
    stamp: usize,
    statistics: Arc<CoreTextStatistics>,
}
impl CoreTextRasterizer {
    /// Counters shared with this rasterizer.
    #[must_use]
    pub fn statistics(&self) -> Arc<CoreTextStatistics> {
        self.statistics.clone()
    }
    fn font(&mut self, r: &GlyphRasterRequest<'_>) -> Option<CTFont> {
        let result = self.instance(r);
        if let Err(reason) = &result {
            self.statistics.decline(*reason);
        }
        result
            .ok()
            .map(|font| font.clone_with_font_size(f64::from(r.size)))
    }
    fn instance(&mut self, r: &GlyphRasterRequest<'_>) -> Result<CTFont, Decline> {
        if r.phase_x > MAX_PHASE || !r.size.is_finite() || r.size <= 0.0 {
            return Err(Decline::Request);
        }
        let key = (r.font_id, r.font_index);
        self.stamp = self.stamp.saturating_add(1);
        if !self.faces.contains_key(&key) {
            self.insert_face(key, r)?;
        }
        let stamp = self.stamp;
        let face = self.faces.get_mut(&key).ok_or(Decline::Resource)?;
        face.stamp = stamp;
        let base = face.base.clone()?;
        if r.normalized_coords.iter().all(|coord| *coord == 0) {
            return Ok(base);
        }
        let variable = face.variable.as_mut().ok_or(Decline::Unsupported)?;
        if r.normalized_coords.len() > variable.axes.len() {
            return Err(Decline::Unsupported);
        }
        if let Some((font, used)) = variable.variants.get_mut(r.normalized_coords) {
            *used = stamp;
            return Ok(font.clone());
        }
        let added = added_for(r.normalized_coords);
        if self.bytes.saturating_add(added) > r.cache_bytes {
            return Err(Decline::Resource);
        }
        let font = variable.instance(r.normalized_coords)?;
        if variable.variants.len() >= MAX_FACE_VARIANTS
            && let Some(oldest) = variable
                .variants
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(coords, _)| coords.clone())
        {
            variable.variants.remove(&oldest);
            face.bytes -= added_for(&oldest);
            self.bytes -= added_for(&oldest);
        }
        variable
            .variants
            .insert(r.normalized_coords.to_vec(), (font.clone(), stamp));
        face.bytes += added;
        self.bytes += added;
        CoreTextStatistics::add(&self.statistics.variation_instances);
        Ok(font)
    }
    /// Create and retain one face, evicting the least recently used faces
    /// within the shared allowance. A declined face is retained too, so a
    /// refused face is not parsed again for each glyph.
    fn insert_face(&mut self, key: (u64, u32), r: &GlyphRasterRequest<'_>) -> Result<(), Decline> {
        let overhead = std::mem::size_of::<Face>();
        let parsed = sfnt::Face::parse(r.font_data, r.font_index);
        let parsed_collection = parsed.as_ref().is_some_and(|face| !face.is_standalone());
        // CoreText copies the face's standalone bytes (a collection member's
        // own tables, not the whole file); a variable face also retains them
        // for CoreGraphics.
        let estimate = overhead
            + parsed.as_ref().map_or(0, |face| {
                let copies = if face.has_table(*b"fvar") { 2 } else { 1 };
                face.standalone_len()
                    .unwrap_or(r.font_data.len())
                    .saturating_mul(copies)
            });
        if estimate > r.cache_bytes {
            return Err(Decline::Resource);
        }
        while self.bytes.saturating_add(estimate) > r.cache_bytes {
            let oldest = self
                .faces
                .iter()
                .min_by_key(|(_, f)| f.stamp)
                .map(|(key, _)| *key)
                .ok_or(Decline::Resource)?;
            let old = self.faces.remove(&oldest).ok_or(Decline::Resource)?;
            self.bytes -= old.bytes;
        }
        let (base, variable, bytes) = match parsed.ok_or(Decline::Unsupported).and_then(create) {
            Ok((font, variable)) => {
                if parsed_collection {
                    CoreTextStatistics::add(&self.statistics.collection_members);
                }
                (Ok(font), variable, estimate)
            }
            Err(reason) => (Err(reason), None, overhead),
        };
        self.faces.insert(
            key,
            Face {
                base,
                variable,
                bytes,
                stamp: self.stamp,
            },
        );
        self.bytes += bytes;
        Ok(())
    }
}

/// The charge of one retained variation instance.
fn added_for(coords: &[i16]) -> usize {
    INSTANCE_NATIVE_BYTES
        + std::mem::size_of::<CTFont>()
        + std::mem::size_of::<Vec<i16>>()
        + std::mem::size_of_val(coords)
}

/// The exact selected face at its default instance, and its variation data.
fn create(face: sfnt::Face<'_>) -> Result<(CTFont, Option<Variable>), Decline> {
    if COLOR_TABLES.iter().any(|tag| face.has_table(*tag)) {
        return Err(Decline::Color);
    }
    let axes = sfnt::axes(&face).map_err(|_| Decline::Unsupported)?;
    if axes.iter().any(|axis| axis.tag == OPTICAL_SIZE_AXIS) {
        return Err(Decline::OpticalSize);
    }
    // A whole single-face file is used as supplied; a collection member is
    // rebuilt from its own tables.
    let data = if face.is_standalone() {
        std::borrow::Cow::Borrowed(face.bytes())
    } else {
        std::borrow::Cow::Owned(face.standalone().ok_or(Decline::Unsupported)?)
    };
    let font = font::new_from_buffer(&data).map_err(|_| Decline::Unsupported)?;
    // The platform face must carry this member's own header and glyph count.
    let head = font
        .get_font_table(u32::from_be_bytes(*b"head"))
        .ok_or(Decline::Unsupported)?;
    let glyphs = face
        .table(*b"maxp")
        .and_then(|maxp| maxp.get(4..6))
        .map(|count| u16::from_be_bytes([count[0], count[1]]));
    if Some(head.bytes()) != face.table(*b"head")
        || glyphs.and_then(|count| isize::try_from(count).ok()) != Some(font.glyph_count())
    {
        return Err(Decline::Unsupported);
    }
    let variable = (!axes.is_empty()).then(|| Variable {
        axes,
        data: Arc::new(data.into_owned()),
        graphics: None,
        variants: BTreeMap::new(),
    });
    Ok((font, variable))
}

impl Variable {
    /// The instance at the user values whose normalization gives `coords`;
    /// every axis is set, so no axis falls back to a size default. Declined
    /// unless CoreGraphics reads back exactly those values for every axis.
    fn instance(&mut self, coords: &[i16]) -> Result<CTFont, Decline> {
        let (data, axes) = (&self.data, &self.axes);
        let graphics = self
            .graphics
            .get_or_insert_with(|| Graphics::new(data, axes))
            .as_ref()
            .map_err(|reason| *reason)?;
        let values = axes
            .iter()
            .enumerate()
            .map(|(index, axis)| {
                let coord = coords.get(index).copied().unwrap_or(0);
                axis.user_value(f64::from(coord) / F2DOT14_ONE)
            })
            .collect::<Vec<_>>();
        let pairs = graphics
            .keys
            .iter()
            .zip(&values)
            .map(|(key, value)| (CFString::new(key), CFNumber::from(*value)))
            .collect::<Vec<_>>();
        let varied = graphics
            .font
            .create_copy_from_variations(&CFDictionary::from_CFType_pairs(&pairs))
            .map_err(|_| Decline::Unsupported)?;
        let applied = varied.copy_variations().ok_or(Decline::Variation)?;
        let exact = applied.len() == values.len()
            && graphics.keys.iter().zip(&values).all(|(key, value)| {
                applied
                    .find(CFString::new(key))
                    .and_then(|number| number.to_f64())
                    .is_some_and(|read| same(read, *value))
            });
        if !exact {
            return Err(Decline::Variation);
        }
        Ok(font::new_from_CGFont(&varied, INSTANCE_POINT_SIZE))
    }
}

impl Graphics {
    /// The face and CoreGraphics's key for each `fvar` axis, by `fvar` index.
    /// CoreGraphics lists its axis records in `fvar` order and names them
    /// from the font's own name records, never localised; its key for an axis
    /// is the name of its record at that index. CoreText's records for the
    /// same face carry the `fvar` tag, but their names follow the process's
    /// preferred language, so only their tags and ranges are read: at each
    /// index CoreText's tag must be the axis's, and both platforms' ranges
    /// must equal it. Counts must equal `fvar`'s and CoreGraphics's names must
    /// be distinct, or the face has no exact instances.
    fn new(data: &Arc<Vec<u8>>, axes: &[sfnt::Axis]) -> Result<Self, Decline> {
        let provider = CGDataProvider::from_buffer(data.clone());
        let font = CGFont::from_data_provider(provider).map_err(|_| Decline::Unsupported)?;
        let named = font
            .copy_variation_axes()
            .ok_or(Decline::Variation)?
            .iter()
            .map(|record| {
                let name = field::<CFString>(&record, AXIS_NAME_KEY)?.to_string();
                Some((name, axis_range(&record, CG_AXIS_RANGE_KEYS)?))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(Decline::Variation)?;
        let tagged = font::new_from_CGFont(&font, INSTANCE_POINT_SIZE)
            .get_variation_axes()
            .ok_or(Decline::Variation)?
            .iter()
            .map(|record| {
                let tag = field::<CFNumber>(&record, CT_AXIS_TAG_KEY)?.to_i64()?;
                Some((tag, axis_range(&record, CT_AXIS_RANGE_KEYS)?))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(Decline::Variation)?;
        if named.len() != axes.len() || tagged.len() != axes.len() {
            return Err(Decline::Variation);
        }
        let mut keys: Vec<String> = Vec::with_capacity(axes.len());
        for ((axis, (name, named_range)), (tag, tagged_range)) in axes.iter().zip(named).zip(tagged)
        {
            let range = [axis.min, axis.default, axis.max];
            let exact = |read: [f64; 3]| read.iter().zip(range).all(|(&a, b)| same(a, b));
            // Two axes with one name would share one variation key.
            if tag != i64::from(u32::from_be_bytes(axis.tag))
                || !exact(named_range)
                || !exact(tagged_range)
                || keys.contains(&name)
            {
                return Err(Decline::Variation);
            }
            keys.push(name);
        }
        Ok(Self { font, keys })
    }
}

/// A platform variation-axis record's field of type `T`; `None` if absent or
/// of another type.
fn field<T: core_foundation::ConcreteCFType>(
    record: &CFDictionary<CFString, CFType>,
    key: &'static str,
) -> Option<T> {
    record
        .find(CFString::from_static_string(key))?
        .downcast::<T>()
}

/// A platform variation-axis record's min/default/max under the given keys.
fn axis_range(
    record: &CFDictionary<CFString, CFType>,
    keys: [&'static str; 3],
) -> Option<[f64; 3]> {
    let [min, default, max] = keys.map(|key| field::<CFNumber>(record, key)?.to_f64());
    Some([min?, default?, max?])
}

/// Equal to within one 16.16 `fvar` unit.
fn same(read: f64, expected: f64) -> bool {
    (read - expected).abs() <= AXIS_VALUE_TOLERANCE
}

impl GlyphRasterizer for CoreTextRasterizer {
    fn bounds(&mut self, r: &GlyphRasterRequest<'_>) -> Option<GlyphRasterBounds> {
        let font = self.font(r)?;
        let glyph = u16::try_from(r.glyph).ok()?;
        if usize::from(glyph) >= usize::try_from(font.glyph_count()).ok()? {
            return None;
        }
        let rect = font.get_bounding_rects_for_glyphs(kCTFontOrientationHorizontal, &[glyph]);
        if rect.size.width == 0.0 || rect.size.height == 0.0 {
            return Some(GlyphRasterBounds {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
            });
        }
        let phase = f64::from(r.phase_x) / PHASES;
        // CoreGraphics smoothing may extend beyond the unhinted contour.
        // Reserve the same one-device-pixel fringe as Skia's CoreText scaler.
        let left = (rect.origin.x + phase).floor() - 1.0;
        let right = (rect.origin.x + rect.size.width + phase).ceil() + 1.0;
        let top = (-rect.origin.y - rect.size.height).floor() - 1.0;
        let bottom = (-rect.origin.y).ceil() + 1.0;
        if [left, right, top, bottom]
            .into_iter()
            .any(|v| !v.is_finite() || v < f64::from(i32::MIN) || v > f64::from(i32::MAX))
            || right < left
            || bottom < top
        {
            return None;
        }
        let width = u32::try_from(right as i64 - left as i64).ok()?;
        let height = u32::try_from(bottom as i64 - top as i64).ok()?;
        if (width as usize).checked_mul(height as usize)? > r.max_pixels {
            return None;
        }
        Some(GlyphRasterBounds {
            left: left as i32,
            top: top as i32,
            width,
            height,
        })
    }
    fn rasterize(
        &mut self,
        r: &GlyphRasterRequest<'_>,
        b: GlyphRasterBounds,
    ) -> Option<GlyphRasterMask> {
        GlyphRasterMask::new(b, self.coverage(r, b)?).ok()
    }
    fn retained_bytes(&self) -> usize {
        self.bytes
    }
}
impl CoreTextRasterizer {
    /// Row-major coverage for `b`, one byte per physical pixel.
    fn coverage(&mut self, r: &GlyphRasterRequest<'_>, b: GlyphRasterBounds) -> Option<Vec<u8>> {
        let pixels = (b.width as usize).checked_mul(b.height as usize)?;
        if pixels == 0 {
            return Some(Vec::new());
        }
        if pixels > r.max_pixels {
            return None;
        }
        let row = (b.width as usize).checked_mul(4)?;
        let font = self.font(r)?;
        let glyph = u16::try_from(r.glyph).ok()?;
        let mut context = CGContext::create_bitmap_context(
            None,
            b.width as usize,
            b.height as usize,
            8,
            row,
            &CGColorSpace::create_device_rgb(),
            kCGImageAlphaNoneSkipLast,
        );
        context.set_gray_fill_color(1.0, 1.0);
        context.fill_rect(CGRect::new(
            &CGPoint::new(0.0, 0.0),
            &CGSize::new(f64::from(b.width), f64::from(b.height)),
        ));
        context.set_gray_fill_color(0.0, 1.0);
        context.set_allows_font_smoothing(true);
        context.set_should_smooth_fonts(true);
        context.set_allows_antialiasing(true);
        context.set_should_antialias(true);
        context.set_allows_font_subpixel_positioning(true);
        context.set_should_subpixel_position_fonts(true);
        context.set_allows_font_subpixel_quantization(false);
        context.set_should_subpixel_quantize_fonts(false);
        context.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
        let position = CGPoint::new(
            -f64::from(b.left) + f64::from(r.phase_x) / PHASES,
            f64::from(b.height) + f64::from(b.top),
        );
        font.draw_glyphs(&[glyph], &[position], context.clone());
        let mut coverage = Vec::new();
        coverage.try_reserve_exact(pixels).ok()?;
        for pixel in context.data().as_chunks::<4>().0 {
            // The source is black on opaque white. Preserve grayscale smoothing
            // as a single coverage channel; actual text color is applied by Varta.
            let value = (u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2])) / 3;
            coverage.push(u8::MAX - value as u8);
        }
        Some(coverage)
    }
}
