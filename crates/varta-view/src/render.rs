//! Stateful native rendering, separate from the stateless reference raster APIs.
mod clip_geometry;
pub(crate) mod clips;
pub(crate) mod glyphs;
pub(crate) mod scene;
mod styles;
use crate::tree::Tree;
use crate::{LayoutError, PaintError, RasterImage, TextMeasurer, View};
pub(crate) use styles::ReadCache;

/// Work avoided by the last successful native frame. These are counters, not
/// clocks, total CPU-time measurements or allocator/RSS limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub shaping_cache_hits: usize,
    pub shaping_cache_misses: usize,
    pub glyph_cache_hits: usize,
    pub dirty_pixels: usize,
    pub drawing_cache_hits: usize,
}

/// A reusable native CPU renderer. Font collection and render caches are local
/// to this renderer; the guest model and stateless raster APIs remain separate.
pub struct Renderer {
    font: Option<TextMeasurer>,
    font_size: f32,
    previous: Option<Tree>,
    scene: scene::Scene,
    glyphs: glyphs::Glyphs,
    stats: RenderStats,
    segments: crate::drawing::SegmentCache,
}
impl Renderer {
    pub fn new(mut font: Option<TextMeasurer>, font_size: f32) -> Result<Self, LayoutError> {
        if !font_size.is_finite() || !(0.01..=4096.0).contains(&font_size) {
            return Err(LayoutError::Text("invalid base font size".into()));
        }
        if let Some(font) = font.as_mut() {
            font.cache_shapes();
        }
        Ok(Self {
            font,
            font_size,
            previous: None,
            scene: scene::Scene::default(),
            glyphs: glyphs::Glyphs::default(),
            stats: RenderStats::default(),
            segments: Default::default(),
        })
    }
    /// Diagnostic work counters for the last successful frame.
    #[must_use]
    pub fn stats(&self) -> RenderStats {
        self.stats
    }
    /// Integer-aligned physical pixel damage; `None` means identical pixels.
    /// Hit/focus metadata is still rebuilt even when there is no pixel damage.
    #[must_use]
    pub fn damage(&self) -> Option<crate::BoxRect> {
        self.scene.damage.map(|r| crate::BoxRect {
            x: r.x(),
            y: r.y(),
            width: r.width(),
            height: r.height(),
        })
    }
    pub fn render(
        &mut self,
        view: &View,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<RasterImage, PaintError> {
        self.scene.record_only = false;
        self.segments.clear();
        self.render_inner(view, width, height, scale)
    }
    /// Prepare bounded drawing commands and input geometry. Unsupported vector
    /// operations or cache overflow use an explicit software raster fallback.
    pub fn prepare(
        &mut self,
        view: &View,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<crate::PreparedFrame, PaintError> {
        self.scene.record_only = true;
        let frame = self.render_inner(view, width, height, scale)?;
        if !frame.pixels.is_empty() {
            self.segments.clear();
            return Ok(crate::PreparedFrame::Raster(frame));
        }
        let metadata = frame.into_metadata();
        let before = self.segments.hits;
        if let Some(scene) =
            crate::DisplayList::from_commands(&self.scene.commands, &mut self.segments)
        {
            self.stats.drawing_cache_hits = self.segments.hits.saturating_sub(before);
            Ok(crate::PreparedFrame::Vector { metadata, scene })
        } else {
            self.segments.clear();
            let pixels = crate::drawing::software_pixels(&self.scene.commands, width, height)?;
            // A software fallback is a real full pixel frame even though the
            // preceding command-only pass did not rasterize a dirty region.
            self.scene.damage = tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32);
            self.stats.dirty_pixels = width as usize * height as usize;
            Ok(crate::PreparedFrame::Raster(
                crate::RasterImage::from_parts(metadata, pixels),
            ))
        }
    }
    fn render_inner(
        &mut self,
        view: &View,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<RasterImage, PaintError> {
        crate::paint::validate_dimensions(width, height, scale)?;
        let mut env = view.environment;
        env.width = f64::from(width) / f64::from(scale);
        let prepared = styles::prepare(&view.tree, env, self.previous.as_ref())?;
        let (hits, misses) = self
            .font
            .as_ref()
            .map_or((0, 0), TextMeasurer::cache_counts);
        let glyph_hits = self.glyphs.hits;
        let frame = crate::paint::rasterize_retained(
            &prepared,
            &env,
            (width, height, scale),
            self.font.as_mut().map(|font| (font, self.font_size)),
            (&mut self.scene, &mut self.glyphs),
        )?;
        let (after_hits, after_misses) = self
            .font
            .as_ref()
            .map_or((0, 0), TextMeasurer::cache_counts);
        self.stats = RenderStats {
            shaping_cache_hits: after_hits.saturating_sub(hits),
            shaping_cache_misses: after_misses.saturating_sub(misses),
            glyph_cache_hits: self.glyphs.hits.saturating_sub(glyph_hits),
            dirty_pixels: self.scene.dirty_pixels,
            drawing_cache_hits: 0,
        };
        self.previous = Some(prepared);
        Ok(frame)
    }
}
