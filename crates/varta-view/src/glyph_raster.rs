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

/// Bytes per pixel of a color glyph image.
const COLOR_CHANNELS: usize = 4;

/// Glyph pixels, row-major, one physical pixel per entry: grayscale alpha
/// coverage that Varta tints with the text color, or a color glyph image
/// (emoji) drawn in its own colors.
pub struct GlyphRasterMask {
    pub(crate) bounds: GlyphRasterBounds,
    pub(crate) coverage: Vec<u8>,
    /// Unpremultiplied sRGB of each pixel of a color glyph, whose alpha is
    /// `coverage`; `None` for coverage tinted with the text color.
    pub(crate) color: Option<Vec<[u8; 3]>>,
}
impl GlyphRasterMask {
    fn pixels(bounds: GlyphRasterBounds) -> Result<usize, GlyphRasterError> {
        let pixels = bounds.pixels().ok_or(GlyphRasterError::InvalidMask)?;
        if pixels > crate::paint::MAX_TEXT_PAINT_WORK {
            return Err(GlyphRasterError::Limit);
        }
        Ok(pixels)
    }
    /// Alpha coverage, one byte per physical pixel.
    pub fn new(bounds: GlyphRasterBounds, coverage: Vec<u8>) -> Result<Self, GlyphRasterError> {
        if Self::pixels(bounds)? != coverage.len() {
            return Err(GlyphRasterError::InvalidMask);
        }
        Ok(Self {
            bounds,
            coverage,
            color: None,
        })
    }
    /// A color glyph image: premultiplied sRGB and alpha, four bytes per
    /// physical pixel, the platform's own glyph bitmap format. A component
    /// above its alpha is invalid. The text color's alpha still applies, as
    /// in the browser; its color does not.
    pub fn color(
        bounds: GlyphRasterBounds,
        premultiplied: &[u8],
    ) -> Result<Self, GlyphRasterError> {
        let pixels = Self::pixels(bounds)?;
        if pixels.checked_mul(COLOR_CHANNELS) != Some(premultiplied.len()) {
            return Err(GlyphRasterError::InvalidMask);
        }
        let mut coverage = Vec::new();
        let mut color = Vec::new();
        coverage
            .try_reserve_exact(pixels)
            .and_then(|()| color.try_reserve_exact(pixels))
            .map_err(|_| GlyphRasterError::Limit)?;
        for &[red, green, blue, alpha] in premultiplied.as_chunks::<COLOR_CHANNELS>().0 {
            if red.max(green).max(blue) > alpha {
                return Err(GlyphRasterError::InvalidMask);
            }
            let unpremultiply = |component: u8| {
                if alpha == 0 {
                    return 0;
                }
                let (component, alpha) = (u32::from(component), u32::from(alpha));
                // At most 255: the component is at most the alpha.
                ((component * u32::from(u8::MAX) + alpha / 2) / alpha) as u8
            };
            coverage.push(alpha);
            color.push([
                unpremultiply(red),
                unpremultiply(green),
                unpremultiply(blue),
            ]);
        }
        Ok(Self {
            bounds,
            coverage,
            color: Some(color),
        })
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
    /// Whether the request's glyphs take a horizontal subpixel phase. A
    /// color glyph image is placed at the nearest whole pixel with phase 0,
    /// as the platform's browser places color glyphs. `phase_x` is ignored.
    fn subpixel(&mut self, _request: &GlyphRasterRequest<'_>) -> bool {
        true
    }
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
    #[test]
    fn color_images_are_checked_and_unpremultiplied() {
        let bounds = GlyphRasterBounds {
            left: 0,
            top: 0,
            width: 2,
            height: 1,
        };
        assert!(matches!(
            GlyphRasterMask::color(bounds, &[0; 7]),
            Err(GlyphRasterError::InvalidMask)
        ));
        // A component above its alpha is not premultiplied.
        assert!(matches!(
            GlyphRasterMask::color(bounds, &[0, 0, 0, 0, 9, 0, 0, 8]),
            Err(GlyphRasterError::InvalidMask)
        ));
        let mask = GlyphRasterMask::color(bounds, &[0, 0, 0, 0, 64, 32, 128, 128]).unwrap();
        assert_eq!(mask.coverage, [0, 128]);
        assert_eq!(mask.color.unwrap(), [[0, 0, 0], [128, 64, 255]]);
    }
}
