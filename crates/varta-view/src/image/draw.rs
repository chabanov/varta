//! Drawing one image into an axis-aligned device rectangle with Chrome's
//! sampling: the covered device pixels are sampled at their centers (see
//! the module documentation of `image`) into a small pixel block, which is
//! then composited through the ordinary rectangle path, so edge coverage,
//! clip masks, opacity and blending behave as for any other fill.
use super::{Image, Level};
use std::sync::Arc;
use tiny_skia::{
    BlendMode, FilterQuality, Mask, Paint, Pattern, Pixmap, Rect, SpreadMode, Transform,
};
use varta_common::view::images::DECODED_PIXEL_BYTES;

/// Skia's level-of-detail bias: levels are chosen half a level sharper
/// than the scale alone would (`SkMipmap::ComputeLevel`).
const LEVEL_BIAS: f32 = 0.5;
const CHANNEL_MAX: f32 = 255.0;
const HALF: f32 = 0.5;

/// One image drawn into `rect` (CSS pixels before the draw transform).
#[derive(Debug, Clone)]
pub(crate) struct ImageDraw {
    pub image: Arc<Image>,
    pub rect: Rect,
    /// The image is clipped by its rounded content box, whose coverage is
    /// then its only edge coverage: Blink fills the rounded rectangle with
    /// the image (`DrawImageRRect`), so its pixels are sampled up to the
    /// whole pixels the rectangle touches.
    pub rounded: bool,
}
impl PartialEq for ImageDraw {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.image, &other.image)
            && self.rect == other.rect
            && self.rounded == other.rounded
    }
}

/// Whether `transform` keeps rectangles axis-aligned with positive extents,
/// the only transforms an image is drawn with.
pub(crate) fn axis_aligned(transform: Transform) -> bool {
    transform.kx == 0.0 && transform.ky == 0.0 && transform.sx > 0.0 && transform.sy > 0.0
}

/// The fractional level of detail for an image drawn at `scale` device
/// pixels per image pixel on its less reduced axis; zero samples the image
/// itself.
fn level_of_detail(scale: f32) -> f32 {
    if scale >= 1.0 || scale <= 0.0 || !scale.is_finite() {
        return 0.0;
    }
    (-scale.log2() - LEVEL_BIAS).max(0.0)
}

/// Bilinear sample of `level` at `(u, v)` in its own texel units, edges
/// clamped, premultiplied channels in 0..=255.
fn bilinear(level: &Level, u: f32, v: f32) -> [f32; 4] {
    let x = u - HALF;
    let y = v - HALF;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let clamp = |value: f32, length: u32| (value.max(0.0) as u32).min(length - 1) as usize;
    let (left, right) = (clamp(x0, level.width), clamp(x0 + 1.0, level.width));
    let (top, bottom) = (clamp(y0, level.height), clamp(y0 + 1.0, level.height));
    let width = level.width as usize;
    let mut out = [0.0; 4];
    for (row, wy) in [(top, 1.0 - fy), (bottom, fy)] {
        for (column, wx) in [(left, 1.0 - fx), (right, fx)] {
            let at = (row * width + column) * DECODED_PIXEL_BYTES;
            for (channel, total) in out.iter_mut().enumerate() {
                *total += f32::from(level.pixels[at + channel]) * wx * wy;
            }
        }
    }
    out
}

/// The image's color at level of detail `lod`, at `(u, v)` in image pixels:
/// bilinear within the two nearest levels and linear between them.
fn sample(image: &Image, lod: f32, u: f32, v: f32) -> [u8; 4] {
    let last = image.levels.len() - 1;
    let lower = (lod.floor() as usize).min(last);
    let weight = if lower == last {
        0.0
    } else {
        lod - lod.floor()
    };
    let at = |index: usize| {
        let level = &image.levels[index];
        bilinear(
            level,
            u * level.width as f32 / image.width() as f32,
            v * level.height as f32 / image.height() as f32,
        )
    };
    let near = at(lower);
    let far = if weight > 0.0 { at(lower + 1) } else { near };
    let mut out = [0; 4];
    for channel in 0..DECODED_PIXEL_BYTES {
        let value = near[channel] + (far[channel] - near[channel]) * weight;
        out[channel] = (value + HALF).floor().clamp(0.0, CHANNEL_MAX) as u8;
    }
    // Interpolated premultiplied colors never exceed their alpha but for
    // rounding; keep the block premultiplied.
    for channel in 0..3 {
        out[channel] = out[channel].min(out[3]);
    }
    out
}

/// Draw `draw` with the device `transform` into `target`, whose pixel (0, 0)
/// is the device pixel `origin`, within the device rectangle `dirty`.
/// `mask` is in target pixels.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_image(
    target: &mut Pixmap,
    draw: &ImageDraw,
    transform: Transform,
    origin: (u32, u32),
    dirty: Rect,
    opacity: f32,
    blend: BlendMode,
    mask: Option<&Mask>,
) {
    if !axis_aligned(transform) {
        return;
    }
    let Some(device) = draw.rect.transform(transform) else {
        return;
    };
    let (ox, oy) = (origin.0 as f32, origin.1 as f32);
    let Some(extent) = Rect::from_xywh(ox, oy, target.width() as f32, target.height() as f32)
    else {
        return;
    };
    let Some(visible) = device.intersect(&dirty).and_then(|r| r.intersect(&extent)) else {
        return;
    };
    // Every pixel the rectangle touches (`Rect::round_out` rounds the
    // extent, not the far edge).
    let Some(area) = tiny_skia::IntRect::from_ltrb(
        visible.left().floor() as i32,
        visible.top().floor() as i32,
        visible.right().ceil() as i32,
        visible.bottom().ceil() as i32,
    ) else {
        return;
    };
    let image = &draw.image;
    let (scale_x, scale_y) = (
        device.width() / image.width() as f32,
        device.height() / image.height() as f32,
    );
    let lod = level_of_detail(scale_x.max(scale_y));
    let Some(mut block) = Pixmap::new(area.width(), area.height()) else {
        return;
    };
    let block_width = area.width() as usize;
    for row in 0..area.height() {
        let v = (area.y() as f32 + row as f32 + HALF - device.y()) / scale_y;
        for column in 0..area.width() {
            let u = (area.x() as f32 + column as f32 + HALF - device.x()) / scale_x;
            let at = (row as usize * block_width + column as usize) * DECODED_PIXEL_BYTES;
            block.data_mut()[at..at + DECODED_PIXEL_BYTES]
                .copy_from_slice(&sample(image, lod, u, v));
        }
    }
    // A rounded image takes its edge coverage from the clip alone.
    let (fill, anti_alias) = if draw.rounded {
        let whole = Rect::from_xywh(
            area.x() as f32,
            area.y() as f32,
            area.width() as f32,
            area.height() as f32,
        );
        (whole, false)
    } else {
        (Some(visible), true)
    };
    let mut paint = Paint {
        shader: Pattern::new(
            block.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Nearest,
            opacity,
            Transform::from_translate(area.x() as f32 - ox, area.y() as f32 - oy),
        ),
        blend_mode: blend,
        ..Paint::default()
    };
    paint.anti_alias = anti_alias;
    let local = Transform::from_translate(-ox, -oy);
    if let Some(rect) = fill.and_then(|fill| fill.transform(local)) {
        crate::render::scene::fill_rect(target, rect, &paint, Transform::identity(), mask);
    }
}
