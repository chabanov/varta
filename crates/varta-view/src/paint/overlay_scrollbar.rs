//! Overlay scrollbars as Chrome 156 paints them on macOS while they show
//! (`NativeThemeMac` with `is_overlay`, `ScrollbarThemeMac::PaintThumb` and
//! `PaintTrackBackground`); [`crate::overlay_scrollbar`] decides when and
//! how far they show. Only the vertical bar paints: native layout scrolls
//! only vertically, and a horizontal overlay bar shows only while its axis
//! scrolls.
//!
//! As measured in Chrome 156 at 1x, 1.25x and 2x, the thumb's geometry is
//! Blink's computed in CSS pixels (`scale_from_dip` 1) and drawn scaled,
//! anti-aliased, while the track is painted in device pixels.
use super::{
    BoxRect, Clip, FrameMetadata, PaintError, Surface, clipped, color,
    corners::Corners,
    current_color, layout,
    scrollbar::{Device, constrain, fill},
};
use crate::{Environment, overlay_scrollbar::Visual, tree::Tree};
use std::{collections::BTreeMap, sync::Arc};
use tiny_skia::{Color, FillRule, Paint, Transform};
use varta_common::view::CssProperty;

/// The overlay thumb: black (white on a dark scroller) at alpha 0x80.
const THUMB_ALPHA: f32 = 128.0 / 255.0;
/// Thumb inset from the thumb box on every side, plus the track border on
/// the content side, in CSS pixels.
const THUMB_INSET: f32 = 2.0;
const BORDER_WIDTH: f32 = 1.0;
/// The smallest painted thumb, girth by length.
const THUMB_MIN_GIRTH: f32 = 6.0;
const THUMB_MIN_LENGTH: f32 = 18.0;
/// Blink's smallest thumb along an overlay track (`knob_min_length`).
const MINIMUM_THUMB_LENGTH: f32 = 26.0;
/// The shortest track with a thumb: the minimum thumb and both knob end
/// insets (`ScrollbarThemeMac::HasThumb`), in device pixels.
const MINIMUM_TRACK_FOR_THUMB: f32 = 28.0;
/// HSL lightness at or below which a scroller's background takes the dark
/// overlay colors (`RecalculateOverlayScrollbarColorScheme`).
const DARK_LIGHTNESS: f32 = 0.5;

/// Straight RGBA in `0..=1`.
type Rgba = [f32; 4];

/// One color scheme's track paint.
struct Track {
    /// Gradient stops at 0, ⅓, ⅔ and 1 along the track; the last three are
    /// equal, so only the first third changes.
    start: Rgba,
    rest: Rgba,
    inner: Rgba,
    outer: Rgba,
}

const fn argb(a: u8, r: u8, g: u8, b: u8) -> Rgba {
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}

const LIGHT: Track = Track {
    start: [0.973, 0.973, 0.973, 0.776],
    rest: [0.973, 0.973, 0.973, 0.761],
    inner: argb(0xF9, 0xDF, 0xDF, 0xDF),
    outer: argb(0xC6, 0xE8, 0xE8, 0xE8),
};
const DARK: Track = Track {
    start: [0.847, 0.847, 0.847, 0.157],
    rest: [0.8, 0.8, 0.8, 0.149],
    inner: argb(0x33, 0xE5, 0xE5, 0xE5),
    outer: argb(0x28, 0xD8, 0xD8, 0xD8),
};

/// `top` composited over `bottom` (source-over, straight alpha).
fn over(top: Rgba, bottom: Rgba) -> Rgba {
    let alpha = top[3] + bottom[3] * (1.0 - top[3]);
    if alpha == 0.0 {
        return [0.0; 4];
    }
    let channel = |i: usize| (top[i] * top[3] + bottom[i] * bottom[3] * (1.0 - top[3])) / alpha;
    [channel(0), channel(1), channel(2), alpha]
}

fn quantize(color: Rgba, opacity: f32) -> [u8; 4] {
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    [
        byte(color[0]),
        byte(color[1]),
        byte(color[2]),
        byte(color[3] * opacity),
    ]
}

/// Whether a scroller's background takes the dark overlay colors: an
/// opaque-enough background whose HSL lightness is at most one half. A
/// transparent one keeps the page's light scheme (the subset has no
/// `color-scheme`).
fn dark(tree: &Tree, env: &Environment, id: u32) -> bool {
    let background = tree
        .computed_read(id, env)
        .get(&CssProperty::BackgroundColor)
        .and_then(|value| {
            if value == "currentcolor" {
                current_color(tree, env, id)
            } else {
                color(value)
            }
        })
        .or_else(|| {
            layout::modal_dialog(tree, id)
                .then(|| color(varta_common::view::DIALOG_BACKGROUND[usize::from(env.dark)]))
                .flatten()
        });
    background.is_some_and(|color| {
        let channels = [color.red(), color.green(), color.blue()];
        let max = channels.into_iter().fold(0.0, f32::max);
        let min = channels.into_iter().fold(1.0, f32::min);
        color.alpha() > 0.0 && (max + min) / 2.0 <= DARK_LIGHTNESS
    })
}

/// The track in device pixels under the layer opacity `opacity`: the
/// gradient along the track with the inner border on the content side and
/// the outer border on the outer edge, row bands of equal colour.
fn track(surface: &mut Surface, frame: Device, scheme: &Track, opacity: f32, clip: BoxRect) {
    let border = (BORDER_WIDTH * surface.scale).floor();
    let rows = frame.height.max(0.0) as usize;
    let at = |row: usize| {
        let along = ((row as f32 + 0.5) / frame.height * 3.0).min(1.0);
        let gradient: Rgba =
            std::array::from_fn(|i| scheme.start[i] + (scheme.rest[i] - scheme.start[i]) * along);
        [
            quantize(over(scheme.inner, gradient), opacity),
            quantize(gradient, opacity),
            quantize(over(scheme.outer, gradient), opacity),
        ]
    };
    let mut band = 0;
    while band < rows {
        let colors = at(band);
        let mut end = band + 1;
        while end < rows && at(end) == colors {
            end += 1;
        }
        let y = frame.y + band as f32;
        let height = (end - band) as f32;
        let strips = [
            (frame.x, border),
            (frame.x + border, frame.width - 2.0 * border),
            (frame.x + frame.width - border, border),
        ];
        for ((x, width), [r, g, b, a]) in strips.into_iter().zip(colors) {
            let rect = Device {
                x,
                y,
                width,
                height,
            };
            fill(surface, rect, clip, Color::from_rgba8(r, g, b, a));
        }
        band = end;
    }
}

/// The thumb along a track of `length` CSS pixels for `[visible, maximum
/// offset, offset]`, as `(position, length)`: the compositor's geometry
/// (`ScrollbarLayerImplBase`), a rounded proportional length of at least
/// [`MINIMUM_THUMB_LENGTH`] at a truncated offset, so unlike Blink's an
/// offset under one pixel leaves the thumb at the start.
fn thumb_span(length: f32, [visible, max, offset]: [f32; 3]) -> Option<(f32, f32)> {
    let total = visible + max;
    if max <= 0.0 || total <= 0.0 {
        return None;
    }
    let thumb = (visible / total * length).round().max(MINIMUM_THUMB_LENGTH);
    if thumb > length {
        return None;
    }
    let position = (offset.clamp(0.0, max) * (length - thumb) / max).trunc();
    Some((position, thumb))
}

/// The thumb pill in CSS pixels along `frame` (Blink's integer geometry at
/// one device pixel per CSS pixel), or `None` when no thumb shows.
fn thumb_bounds(frame: BoxRect, range: [f32; 3], thumb_box: i32, scale: f32) -> Option<BoxRect> {
    if frame.height * scale < MINIMUM_TRACK_FOR_THUMB {
        return None;
    }
    let (position, length) = thumb_span(frame.height, range)?;
    let girth = thumb_box as f32;
    let bounds = BoxRect {
        x: frame.x + frame.width - girth,
        y: frame.y + position,
        width: girth,
        height: length,
    };
    let (mut top, mut bottom, mut right) = (THUMB_INSET, THUMB_INSET, THUMB_INSET);
    let mut left = THUMB_INSET + BORDER_WIDTH;
    constrain(bounds.width, THUMB_MIN_GIRTH, &mut left, &mut right);
    constrain(bounds.height, THUMB_MIN_LENGTH, &mut top, &mut bottom);
    Some(BoxRect {
        x: bounds.x + left,
        y: bounds.y + top,
        width: bounds.width - left - right,
        height: bounds.height - top - bottom,
    })
}

/// Paints the vertical overlay bar of scroll container `id` as `visual`
/// shows it, clipped to `clip`. Returns the bar frame's visible rectangle,
/// which takes the pointer for the container while the bar shows.
fn paint_bar(
    surface: &mut Surface,
    (tree, env, id): (&Tree, &Environment, u32),
    bars: layout::ScrollbarGeometry,
    visual: Visual,
    clip: BoxRect,
) -> Result<Option<BoxRect>, PaintError> {
    let Some(frame) = bars.vertical else {
        return Ok(None);
    };
    let hit = clipped(frame, clip);
    let clip = surface.local_clip(clip);
    let scale = surface.scale;
    let dark = dark(tree, env, id);
    if visual.track_alpha > 0.0 {
        let scheme = if dark { &DARK } else { &LIGHT };
        track(
            surface,
            Device::of(frame, scale),
            scheme,
            visual.track_alpha,
            clip,
        );
    }
    if let Some(bounds) = thumb_bounds(frame, bars.vertical_range, visual.thumb_box, scale)
        && let Some(path) = Corners([[bounds.width.min(bounds.height) / 2.0; 2]; 4]).path(bounds)
        && clipped(bounds, clip).is_some()
        && let Some(clip) = clipped(clip, clip)
    {
        // Only the clip masks the pill: its own edges are anti-aliased once,
        // at fractional device positions when scaled.
        let mask = surface.mask(clip)?;
        let channel = if dark { 1.0 } else { 0.0 };
        let mut paint = Paint::default();
        paint.set_color(
            Color::from_rgba(channel, channel, channel, THUMB_ALPHA * visual.thumb_alpha)
                .unwrap_or(Color::TRANSPARENT),
        );
        paint.anti_alias = true;
        surface.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            Some(&mask),
        );
    }
    Ok(hit.map(|rect| BoxRect {
        x: rect.x(),
        y: rect.y(),
        width: rect.width(),
        height: rect.height(),
    }))
}

/// Input geometry the overlay bars add to a frame.
pub(super) struct Hits<'a> {
    pub regions: &'a mut Vec<(u32, BoxRect)>,
    pub clips: &'a mut BTreeMap<usize, Arc<Clip>>,
    /// Every overlay bar frame, shown or not, in paint order.
    pub frames: &'a mut Vec<(u32, BoxRect)>,
}

/// The overlay bar of `id` in its outline phase, after its content: its
/// frame for pointer tracking and, while it shows, its paint and a hit
/// region over the content.
pub(super) fn paint(
    surface: &mut Surface,
    (tree, env, id): (&Tree, &Environment, u32),
    bounds: layout::BoxGeometry,
    hits: Hits,
) -> Result<(), PaintError> {
    let Some(bars) = bounds.scrollbars.filter(|bars| bars.overlay) else {
        return Ok(());
    };
    if let Some(frame) = bars.vertical.and_then(|frame| clipped(frame, bounds.clip)) {
        hits.frames.push((
            id,
            BoxRect {
                x: frame.x(),
                y: frame.y(),
                width: frame.width(),
                height: frame.height(),
            },
        ));
    }
    let Some(visual) = tree
        .nodes
        .get(&id)
        .and_then(|node| tree.scrollbars.visual(id, &node.identity))
    else {
        return Ok(());
    };
    if let Some(frame) = paint_bar(surface, (tree, env, id), bars, visual, bounds.clip)? {
        if let Some(clip) = &surface.active_clip {
            hits.clips.insert(hits.regions.len(), clip.clone());
        }
        hits.regions.push((id, frame));
    }
    Ok(())
}

impl FrameMetadata {
    /// The scroll container whose overlay scrollbar frame is under the
    /// physical point, shown or not: the last painted such frame whose
    /// container is the hit element or an ancestor of it (an ancestor's bar
    /// paints over its descendants'). Hosts pass it to
    /// [`crate::View::set_scrollbar_hover`].
    #[must_use]
    pub fn scrollbar_at(&self, x: f32, y: f32) -> Option<u32> {
        let hit = self.hit_test(x, y)?;
        let reaches = |container: u32| {
            let mut node = Some(hit);
            for _ in 0..=self.ancestors.len() {
                match node {
                    Some(id) if id == container => return true,
                    Some(id) => node = self.ancestors.get(&id).copied().flatten(),
                    None => return false,
                }
            }
            false
        };
        self.overlay_scrollbars
            .iter()
            .rev()
            .find(|(id, rect)| {
                x >= rect.x
                    && y >= rect.y
                    && x < rect.x + rect.width
                    && y < rect.y + rect.height
                    && reaches(*id)
            })
            .map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: BoxRect = BoxRect {
        x: 204.0,
        y: 20.0,
        width: 16.0,
        height: 150.0,
    };

    #[test]
    fn the_thumb_follows_blinks_overlay_geometry_in_css_pixels() {
        // Measured in Chrome 156: 150 of 600 at offset 100 on a 150px track
        // is a 7 by 34 pill at (211, 46); expanded it is 11 wide from 207.
        let range = [150.0, 450.0, 100.0];
        let narrow = thumb_bounds(FRAME, range, 12, 1.0).unwrap();
        assert_eq!(
            (narrow.x, narrow.y, narrow.width, narrow.height),
            (211.0, 46.0, 7.0, 34.0)
        );
        let wide = thumb_bounds(FRAME, range, 16, 2.0).unwrap();
        assert_eq!((wide.x, wide.width), (207.0, 11.0));
        // A long document keeps the 26px minimum; a short track has none.
        let long = thumb_bounds(FRAME, [150.0, 100_000.0, 0.0], 12, 1.0).unwrap();
        assert_eq!(long.height, 22.0);
        // An offset under one pixel leaves the thumb at the start.
        let start = thumb_bounds(FRAME, [150.0, 450.0, 1.0], 12, 1.0).unwrap();
        assert_eq!(start.y, 22.0);
        let short = BoxRect {
            height: 27.0,
            ..FRAME
        };
        assert_eq!(thumb_bounds(short, range, 12, 1.0), None);
    }

    #[test]
    fn track_layers_composite_borders_over_the_gradient() {
        // Over white Chrome draws the gradient 250, the inner border 224 and
        // the outer border 236; its layer rounds once more, within a level.
        let white = |c: [u8; 4]| {
            let a = f32::from(c[3]) / 255.0;
            f32::from(c[0]) * a + 255.0 * (1.0 - a)
        };
        let rest = LIGHT.rest;
        for (layer, chrome) in [
            (rest, 250.0),
            (over(LIGHT.inner, rest), 224.0),
            (over(LIGHT.outer, rest), 236.0),
        ] {
            let native = white(quantize(layer, 1.0));
            assert!((native - chrome).abs() <= 1.0, "{native} for {chrome}");
        }
        // A half-faded track halves every layer's alpha.
        assert_eq!(quantize(rest, 0.5)[3], 97);
    }
}
