//! CoreText glyph coverage from the exact font bytes already selected by Parley.
//! No system font lookup or process-wide font registration occurs here.
use core_graphics::{
    base::kCGImageAlphaNoneSkipLast,
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    geometry::{CGPoint, CGRect, CGSize},
};
use core_text::{
    font::{self, CTFont},
    font_descriptor::kCTFontOrientationHorizontal,
};
use std::collections::BTreeMap;
use varta_view::{GlyphRasterBounds, GlyphRasterMask, GlyphRasterRequest, GlyphRasterizer};
struct Face {
    font: CTFont,
    bytes: usize,
    stamp: usize,
}
#[derive(Default)]
pub struct CoreTextRasterizer {
    faces: BTreeMap<(u64, u32), Face>,
    bytes: usize,
    stamp: usize,
}
impl CoreTextRasterizer {
    fn font(&mut self, r: &GlyphRasterRequest<'_>) -> Option<CTFont> {
        // Collection members, variations and color glyphs keep the portable path
        // until their platform instantiation can preserve the selected face.
        if r.font_index != 0
            || !r.normalized_coords.is_empty()
            || r.phase_x > 3
            || !r.size.is_finite()
            || r.size <= 0.0
        {
            return None;
        }
        let key = (r.font_id, r.font_index);
        self.stamp = self.stamp.saturating_add(1);
        if let Some(face) = self.faces.get_mut(&key) {
            face.stamp = self.stamp;
            return Some(face.font.clone_with_font_size(f64::from(r.size)));
        }
        let bytes = r.font_data.len().checked_add(std::mem::size_of::<Face>())?;
        if bytes > r.cache_bytes {
            return None;
        }
        while self.bytes.saturating_add(bytes) > r.cache_bytes {
            let key = self
                .faces
                .iter()
                .min_by_key(|(_, f)| f.stamp)
                .map(|(key, _)| *key)?;
            let old = self.faces.remove(&key)?;
            self.bytes -= old.bytes;
        }
        let base = font::new_from_buffer(r.font_data).ok()?;
        const COLOR_TABLES: [[u8; 4]; 5] = [*b"COLR", *b"CBDT", *b"EBDT", *b"sbix", *b"SVG "];
        if base
            .get_variation_axes()
            .is_some_and(|axes| !axes.is_empty())
            || base.get_available_font_tables().is_some_and(|tables| {
                tables.iter().any(|tag| {
                    COLOR_TABLES
                        .into_iter()
                        .any(|color| *tag == u32::from_be_bytes(color))
                })
            })
        {
            return None;
        }
        let result = base.clone_with_font_size(f64::from(r.size));
        self.faces.insert(
            key,
            Face {
                font: base,
                bytes,
                stamp: self.stamp,
            },
        );
        self.bytes += bytes;
        Some(result)
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
        let phase = f64::from(r.phase_x) / 4.0;
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
        let pixels = (b.width as usize).checked_mul(b.height as usize)?;
        if pixels == 0 {
            return GlyphRasterMask::new(b, Vec::new()).ok();
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
            -f64::from(b.left) + f64::from(r.phase_x) / 4.0,
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
        GlyphRasterMask::new(b, coverage).ok()
    }
    fn retained_bytes(&self) -> usize {
        self.bytes
    }
}
