//! Host-supplied glyph coverage. Layout, font selection and shaping stay in Varta;
//! a platform rasterizer receives explicit bytes and a checked glyph identifier.

/// Physical ink rectangle relative to an integer glyph baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphRasterBounds {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}
impl GlyphRasterBounds {
    pub(crate) fn pixels(self) -> Option<usize> {
        self.left.checked_add(i32::try_from(self.width).ok()?)?;
        self.top.checked_add(i32::try_from(self.height).ok()?)?;
        (self.width as usize).checked_mul(self.height as usize)
    }
}

/// A request for one shaped glyph, independent of its integer screen position.
/// `phase_x` is the horizontal quarter-pixel phase (0..=3). The vertical
/// baseline is integer-aligned; logical text and selection geometry are separate.
pub struct GlyphRasterRequest<'a> {
    pub font_id: u64,
    pub font_data: &'a [u8],
    pub font_index: u32,
    pub glyph: u32,
    pub size: f32,
    pub normalized_coords: &'a [i16],
    pub phase_x: u8,
    /// Maximum coverage cells for this request, from the existing text allowance.
    pub max_pixels: usize,
    /// The shared glyph-cache byte allowance, including host-retained font bytes.
    pub cache_bytes: usize,
}

/// Invalid or oversized host coverage never becomes drawing commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GlyphRasterError {
    #[error("glyph coverage dimensions do not match its data")]
    InvalidMask,
    #[error("glyph coverage exceeds the text preparation allowance")]
    Limit,
}

/// Grayscale alpha coverage, row-major, one byte per physical pixel.
pub struct GlyphRasterMask {
    pub(crate) bounds: GlyphRasterBounds,
    pub(crate) coverage: Vec<u8>,
}
impl GlyphRasterMask {
    pub fn new(bounds: GlyphRasterBounds, coverage: Vec<u8>) -> Result<Self, GlyphRasterError> {
        let pixels = bounds.pixels().ok_or(GlyphRasterError::InvalidMask)?;
        if pixels > crate::paint::MAX_TEXT_PAINT_WORK {
            return Err(GlyphRasterError::Limit);
        }
        if pixels != coverage.len() {
            return Err(GlyphRasterError::InvalidMask);
        }
        Ok(Self { bounds, coverage })
    }
}

/// Platform-only adapter. The headless engine never discovers system fonts or
/// performs I/O through this interface. Returning `None` selects the portable
/// outline fallback; a provider must not substitute another font or glyph.
pub trait GlyphRasterizer: Send {
    fn bounds(&mut self, request: &GlyphRasterRequest<'_>) -> Option<GlyphRasterBounds>;
    fn rasterize(
        &mut self,
        request: &GlyphRasterRequest<'_>,
        bounds: GlyphRasterBounds,
    ) -> Option<GlyphRasterMask>;
    /// Logical bytes retained by this provider, including backing font data.
    /// Varta subtracts these from its existing glyph-cache allowance.
    fn retained_bytes(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimensions_length_and_existing_text_allowance_are_checked_before_conversion() {
        let mut bounds = GlyphRasterBounds {
            left: 0,
            top: 0,
            width: 2,
            height: 2,
        };
        assert!(matches!(
            GlyphRasterMask::new(bounds, vec![0; 3]),
            Err(GlyphRasterError::InvalidMask)
        ));
        bounds.left = i32::MAX;
        assert!(matches!(
            GlyphRasterMask::new(bounds, vec![0; 4]),
            Err(GlyphRasterError::InvalidMask)
        ));
        bounds = GlyphRasterBounds {
            left: 0,
            top: 0,
            width: crate::paint::MAX_TEXT_PAINT_WORK as u32 + 1,
            height: 1,
        };
        assert!(matches!(
            GlyphRasterMask::new(bounds, Vec::new()),
            Err(GlyphRasterError::Limit)
        ));
    }
}
