//! CoreText glyph coverage from the exact font bytes already selected by Parley.
//! No system font lookup or process-wide font registration occurs here: a
//! collection member is rebuilt as a standalone face from its own tables, and a
//! variation instance is a CoreGraphics copy of that face at the user values
//! whose normalization equals the shaper's normalized coordinates.
mod sfnt;
#[cfg(test)]
mod tests;
use core_graphics::{
    base::kCGImageAlphaNoneSkipLast,
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    data_provider::CGDataProvider,
    display::CFDictionary,
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
    Resource,
    Request,
}

/// A face usable for variation instances: its standalone bytes stay shared
/// with the CoreGraphics data provider.
struct Variable {
    axes: Vec<sfnt::Axis>,
    data: Arc<Vec<u8>>,
    graphics: Option<CGFont>,
    variants: BTreeMap<Vec<i16>, (CTFont, usize)>,
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
        let added = std::mem::size_of::<CTFont>()
            + std::mem::size_of::<Vec<i16>>()
            + std::mem::size_of_val(r.normalized_coords);
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
        let estimate = overhead
            + parsed.as_ref().map_or(0, |face| {
                // A variable face also retains its standalone bytes for CoreGraphics.
                let copies = if face.has_table(*b"fvar") { 2 } else { 1 };
                r.font_data.len().saturating_mul(copies)
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

fn added_for(coords: &[i16]) -> usize {
    std::mem::size_of::<CTFont>() + std::mem::size_of::<Vec<i16>>() + std::mem::size_of_val(coords)
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
    /// CoreGraphics names axes by their `fvar` name strings and takes user
    /// values; every axis is set, so no axis falls back to a size default.
    fn instance(&mut self, coords: &[i16]) -> Result<CTFont, Decline> {
        if self.graphics.is_none() {
            let provider = CGDataProvider::from_buffer(self.data.clone());
            self.graphics =
                Some(CGFont::from_data_provider(provider).map_err(|_| Decline::Unsupported)?);
        }
        let graphics = self.graphics.as_ref().ok_or(Decline::Unsupported)?;
        let values = self
            .axes
            .iter()
            .enumerate()
            .map(|(index, axis)| {
                let coord = coords.get(index).copied().unwrap_or(0);
                (
                    axis.name.as_str().into(),
                    axis.user_value(f64::from(coord) / F2DOT14_ONE).into(),
                )
            })
            .collect::<Vec<_>>();
        let variations = CFDictionary::from_CFType_pairs(&values);
        let varied = graphics
            .create_copy_from_variations(&variations)
            .map_err(|_| Decline::Unsupported)?;
        Ok(font::new_from_CGFont(&varied, INSTANCE_POINT_SIZE))
    }
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
