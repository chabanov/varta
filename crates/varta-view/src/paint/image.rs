//! The replaced content of an `img`: its packaged image stretched over the
//! content box (Chrome's initial `object-fit: fill`), whose edges are snapped
//! to whole paint pixels before DPI as Blink snaps its destination rectangle.
//! The content-box curve of a rounded image clips it (the content clip of
//! `clip_scopes`), the ancestors' rectangular clip as for a background, and
//! opacity and scaling come from the element's group and transform scopes.
use super::{ClipMask, PaintError, Primitive, Recording, Surface, clipped, snap};
use crate::image::ImageDraw;
use crate::layout::BoxGeometry;
use crate::tree::Tree;
use std::sync::Arc;
use tiny_skia::{Color, Paint, Rect, Transform};
use varta_common::view::Attribute;

impl Surface<'_> {
    /// Record one image draw. Images are always recorded (an immediate
    /// frame keeps the pixels painted so far and replays the rest on top),
    /// so they composite through the same clip, group and transform scopes
    /// as recorded fills. An inline capture cannot hold one.
    fn fill_image(&mut self, node: u32, draw: ImageDraw, mask: Option<&ClipMask>) {
        if self.failure.is_some() || self.groups.last().is_some_and(|scope| scope.hidden) {
            return;
        }
        if self.capture.is_some() {
            self.failure = Some(PaintError::Unsupported(node));
            return;
        }
        self.commands.get_or_insert_with(Recording::default);
        let mut paint = Paint::default();
        paint.set_color(Color::WHITE);
        self.record(Primitive::Image(draw), &paint, Transform::identity(), mask);
    }
}

/// Blink's layout unit: a sixty-fourth of a CSS pixel.
const LAYOUT_UNITS_PER_PIXEL: f32 = 64.0;

/// `rect` in Blink's fixed-point layout units, which truncate: an image's
/// size follows from its ratio in those units, so a fractional box can snap
/// to another pixel edge than its exact value would.
fn layout_units(rect: crate::BoxRect) -> crate::BoxRect {
    let unit = |value: f32| (value * LAYOUT_UNITS_PER_PIXEL).trunc() / LAYOUT_UNITS_PER_PIXEL;
    crate::BoxRect {
        x: unit(rect.x),
        y: unit(rect.y),
        width: unit(rect.width),
        height: unit(rect.height),
    }
}

/// Paint `id`'s image into its snapped content box; `rounded` when its
/// content-box curve clips it.
pub(super) fn paint(
    surface: &mut Surface<'_>,
    tree: &Tree,
    id: u32,
    (bounds, rounded): (BoxGeometry, bool),
) -> Result<(), PaintError> {
    let image = tree
        .element(id)
        .and_then(|element| element.attributes.get(&Attribute::Src))
        .and_then(|name| tree.images.0.as_ref()?.get(name).cloned())
        .ok_or(PaintError::Unsupported(id))?;
    let content = snap::rect(layout_units(bounds.content));
    let Some(rect) = Rect::from_xywh(content.x, content.y, content.width, content.height) else {
        // An empty content box draws nothing.
        return Ok(());
    };
    let Some(visible) = clipped(content, surface.local_clip(bounds.clip)) else {
        return Ok(());
    };
    let mask = if visible == rect {
        None
    } else {
        Some(surface.mask(visible)?)
    };
    surface.fill_image(
        id,
        ImageDraw {
            image: Arc::clone(&image),
            rect,
            rounded,
        },
        mask.as_ref(),
    );
    Ok(())
}
