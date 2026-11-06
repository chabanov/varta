//! Always-shown (classic) scrollbars as Chrome 156 paints them on macOS
//! (`NativeThemeMac::PaintMacScrollBarTrackOrCorner` and
//! `PaintMacScrollbarThumb`) with Blink's thumb geometry
//! (`ScrollbarTheme::ThumbLength` and `ThumbPosition`). Colors are the light
//! scheme's: the view subset has no `color-scheme`, and the official host
//! never sets it, so Chrome paints light scrollbars in either OS scheme.
//! Geometry is computed in device pixels, as Chrome lays scrollbars out.
use super::{BoxRect, PaintError, Surface, clipped, corners::Corners, layout};
use tiny_skia::{Color, FillRule, Paint, Transform};

/// Track fill: the two equal stops of the light gradient (0.98).
const TRACK: [u8; 3] = [0xFA, 0xFA, 0xFA];
/// The border on the content side of a track.
const INNER_BORDER: [u8; 3] = [0xE8, 0xE8, 0xE8];
/// The border on the outer edge of a track (and of the corner).
const OUTER_BORDER: [u8; 3] = [0xED, 0xED, 0xED];
/// The idle thumb: black at alpha 0x3A.
const THUMB_ALPHA: u8 = 0x3A;
/// Track border width in device-independent pixels (floored when scaled).
const BORDER_WIDTH: f32 = 1.0;
/// Thumb inset from the track on every side (rounded when scaled), plus the
/// inner border on the content side.
const THUMB_INSET: f32 = 3.0;
/// The smallest painted thumb, girth by length (rounded when scaled).
const THUMB_MIN_GIRTH: f32 = 6.0;
const THUMB_MIN_LENGTH: f32 = 18.0;
/// Blink's smallest thumb rectangle along the track on macOS, before the
/// paint insets.
const MINIMUM_THUMB_LENGTH: f32 = 20.0;

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Vertical,
    Horizontal,
}

/// A rectangle in device pixels.
#[derive(Clone, Copy)]
struct Device {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Device {
    fn of(rect: BoxRect, scale: f32) -> Self {
        let left = (rect.x * scale).round();
        let top = (rect.y * scale).round();
        Self {
            x: left,
            y: top,
            width: ((rect.x + rect.width) * scale).round() - left,
            height: ((rect.y + rect.height) * scale).round() - top,
        }
    }
    fn css(self, scale: f32) -> BoxRect {
        BoxRect {
            x: self.x / scale,
            y: self.y / scale,
            width: self.width / scale,
            height: self.height / scale,
        }
    }
}

fn color([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgba8(r, g, b, u8::MAX)
}

fn fill(surface: &mut Surface, rect: Device, clip: BoxRect, color: Color) {
    let scale = surface.scale;
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    if let Some(rect) = clipped(rect.css(scale), clip) {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = false;
        surface.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

/// Fills a track (or the corner) and draws its inner and outer borders.
fn track(surface: &mut Surface, rect: Device, axis: Option<Axis>, clip: BoxRect) {
    let border = (BORDER_WIDTH * surface.scale).floor();
    fill(surface, rect, clip, color(TRACK));
    let horizontal_strip = axis != Some(Axis::Vertical);
    let vertical_strip = axis != Some(Axis::Horizontal);
    // The inner border: the top strip of a horizontal track, the left strip
    // of a vertical one, both starting at the corner's top left.
    let inner = Device {
        width: if vertical_strip { border } else { rect.width },
        height: if horizontal_strip {
            border
        } else {
            rect.height
        },
        ..rect
    };
    fill(surface, inner, clip, color(INNER_BORDER));
    if horizontal_strip {
        let bottom = Device {
            y: rect.y + rect.height - border,
            height: border,
            ..rect
        };
        fill(surface, bottom, clip, color(OUTER_BORDER));
    }
    if vertical_strip {
        let right = Device {
            x: rect.x + rect.width - border,
            width: border,
            ..rect
        };
        fill(surface, right, clip, color(OUTER_BORDER));
    }
}

/// Native `ConstrainInsets`: shrink the insets so the extent keeps `min`.
fn constrain(extent: f32, min: f32, start: &mut f32, end: &mut f32) {
    let requested = *start + *end;
    if requested == 0.0 {
        return;
    }
    let available = extent - min;
    if requested < available {
        return;
    }
    if available < 0.0 {
        *start = 0.0;
        *end = 0.0;
        return;
    }
    *end = (*end * available / requested).trunc();
    *start = available - *end;
}

/// Blink's thumb rectangle along a track of `length` device pixels for
/// `[visible, maximum offset, offset]`, as `(position, length)`; `None`
/// when no thumb fits.
fn thumb_span(length: f32, range: [f32; 3], scale: f32) -> Option<(f32, f32)> {
    let [visible, max, offset] = range.map(|value| value * scale);
    let total = visible + max;
    if max <= 0.0 || total <= 0.0 {
        return None;
    }
    let thumb = (visible / total * length)
        .round()
        .max((MINIMUM_THUMB_LENGTH * scale).round());
    if thumb > length {
        return None;
    }
    let position = offset.max(0.0) * (length - thumb) / max;
    let position = if position > 0.0 && position < 1.0 {
        1.0
    } else {
        position.trunc()
    };
    Some((position, thumb))
}

fn thumb(surface: &mut Surface, track: Device, axis: Axis, range: [f32; 3], clip: BoxRect) {
    let scale = surface.scale;
    let along = match axis {
        Axis::Vertical => track.height,
        Axis::Horizontal => track.width,
    };
    let Some((position, length)) = thumb_span(along, range, scale) else {
        return;
    };
    let mut bounds = match axis {
        Axis::Vertical => Device {
            y: track.y + position,
            height: length,
            ..track
        },
        Axis::Horizontal => Device {
            x: track.x + position,
            width: length,
            ..track
        },
    };
    let inset = (THUMB_INSET * scale).round();
    let border = (BORDER_WIDTH * scale).floor();
    let (mut top, mut left, mut bottom, mut right) = (inset, inset, inset, inset);
    match axis {
        Axis::Vertical => left += border,
        Axis::Horizontal => top += border,
    }
    let (min_width, min_height) = match axis {
        Axis::Vertical => (THUMB_MIN_GIRTH, THUMB_MIN_LENGTH),
        Axis::Horizontal => (THUMB_MIN_LENGTH, THUMB_MIN_GIRTH),
    };
    constrain(
        bounds.width,
        (min_width * scale).round(),
        &mut left,
        &mut right,
    );
    constrain(
        bounds.height,
        (min_height * scale).round(),
        &mut top,
        &mut bottom,
    );
    bounds = Device {
        x: bounds.x + left,
        y: bounds.y + top,
        width: bounds.width - left - right,
        height: bounds.height - top - bottom,
    };
    // Skia clamps the corner radius to half the thinner side: a pill.
    let radius = bounds.width.min(bounds.height) / 2.0 / scale;
    let css = bounds.css(scale);
    let Some(path) = Corners([[radius; 2]; 4]).path(css) else {
        return;
    };
    let Some(visible) = clipped(css, clip) else {
        return;
    };
    let Ok(mask) = surface.mask(visible) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, THUMB_ALPHA));
    paint.anti_alias = true;
    surface.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        Some(&mask),
    );
}

/// Paints a scroll container's classic scrollbars and their corner.
pub(super) fn paint(
    surface: &mut Surface,
    bars: layout::ScrollbarGeometry,
    clip: BoxRect,
) -> Result<(), PaintError> {
    let clip = surface.local_clip(clip);
    let scale = surface.scale;
    let vertical = bars.vertical.map(|rect| Device::of(rect, scale));
    let horizontal = bars.horizontal.map(|rect| Device::of(rect, scale));
    if let Some(rect) = vertical {
        track(surface, rect, Some(Axis::Vertical), clip);
        thumb(surface, rect, Axis::Vertical, bars.vertical_range, clip);
    }
    if let Some(rect) = horizontal {
        track(surface, rect, Some(Axis::Horizontal), clip);
        thumb(surface, rect, Axis::Horizontal, bars.horizontal_range, clip);
    }
    if let (Some(vertical), Some(horizontal)) = (vertical, horizontal) {
        let corner = Device {
            x: vertical.x,
            y: horizontal.y,
            width: vertical.width,
            height: horizontal.height,
        };
        track(surface, corner, None, clip);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbs_follow_blinks_length_position_and_minimum() {
        // 100 of 300 at offset 100 on a 100px track: length 33, position
        // trunc(33.5).
        assert_eq!(
            thumb_span(100.0, [100.0, 200.0, 100.0], 1.0),
            Some((33.0, 33.0))
        );
        // A long document keeps the 20px minimum; a tiny offset moves 1px.
        assert_eq!(
            thumb_span(100.0, [100.0, 19_900.0, 0.0], 1.0),
            Some((0.0, 20.0))
        );
        assert_eq!(
            thumb_span(100.0, [100.0, 19_900.0, 10.0], 1.0),
            Some((1.0, 20.0))
        );
        // Nothing to scroll, or no room for a thumb.
        assert_eq!(thumb_span(100.0, [100.0, 0.0, 0.0], 1.0), None);
        assert_eq!(thumb_span(10.0, [10.0, 100.0, 0.0], 1.0), None);
        // Device pixels at 2x.
        assert_eq!(
            thumb_span(200.0, [100.0, 200.0, 100.0], 2.0),
            Some((66.0, 67.0))
        );
    }

    #[test]
    fn insets_shrink_to_keep_the_minimum_thumb() {
        let (mut start, mut end) = (3.0, 3.0);
        constrain(20.0, 18.0, &mut start, &mut end);
        assert_eq!((start, end), (1.0, 1.0));
        let (mut start, mut end) = (3.0, 3.0);
        constrain(33.0, 18.0, &mut start, &mut end);
        assert_eq!((start, end), (3.0, 3.0));
        let (mut start, mut end) = (4.0, 3.0);
        constrain(10.0, 18.0, &mut start, &mut end);
        assert_eq!((start, end), (0.0, 0.0));
    }
}
