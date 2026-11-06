//! The replaced content of an `img`: its current image frame in its object
//! rectangle (CSS Images 3 `object-fit`/`object-position`; the initial
//! `fill` stretches it over the content box), whose edges are snapped to
//! whole paint pixels before DPI as Blink snaps its destination rectangle,
//! clipped to the content box.
//! The content-box curve of a rounded image clips it (the content clip of
//! `clip_scopes`), the ancestors' rectangular clip as for a background, and
//! opacity and scaling come from the element's group and transform scopes.
use super::{ClipMask, PaintError, Primitive, Recording, Surface, clipped, snap};
use crate::image::ImageDraw;
use crate::layout::BoxGeometry;
use crate::style::Environment;
use crate::tree::Tree;
use std::sync::Arc;
use tiny_skia::{Color, Paint, Rect, Transform};

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

/// Paint `id`'s image: its current frame in its object rectangle
/// (`object-fit`, `object-position`), snapped to whole paint pixels and
/// clipped to its content box; `rounded` when its content-box curve clips
/// it. An `img` without an image paints nothing.
pub(super) fn paint(
    surface: &mut Surface<'_>,
    (tree, env): (&Tree, &Environment),
    id: u32,
    (bounds, rounded): (BoxGeometry, bool),
) -> Result<(), PaintError> {
    let Some(shown) = crate::image::style::shown(tree, env, id) else {
        return Ok(());
    };
    let object = crate::image::style::object_rect(tree, env, id, shown.natural, bounds.content)
        .ok_or(PaintError::Unsupported(id))?;
    let object = snap::rect(layout_units(object));
    let content = snap::rect(layout_units(bounds.content));
    let Some(rect) = Rect::from_xywh(object.x, object.y, object.width, object.height) else {
        // An empty object draws nothing.
        return Ok(());
    };
    let Some(within) = clipped(content, surface.local_clip(bounds.clip)) else {
        return Ok(());
    };
    let Some(visible) = within.intersect(&rect) else {
        return Ok(());
    };
    let mask = if visible == rect {
        None
    } else {
        // The image's own edges already take their coverage once; the mask
        // cuts only where the clip does.
        let open = |clip: f32, edge: f32, outward: f32| {
            if clip == edge { clip + outward } else { clip }
        };
        let cut = Rect::from_ltrb(
            open(within.left(), rect.left(), -1.0),
            open(within.top(), rect.top(), -1.0),
            open(within.right(), rect.right(), 1.0),
            open(within.bottom(), rect.bottom(), 1.0),
        )
        .ok_or(PaintError::Unsupported(id))?;
        Some(surface.mask(cut)?)
    };
    let elapsed = tree.images.now
        - tree
            .images
            .started
            .get(&id)
            .copied()
            .unwrap_or(tree.images.now);
    surface.fill_image(
        id,
        ImageDraw {
            frame: shown.image.frame_at(elapsed),
            image: Arc::clone(&shown.image),
            orientation: shown.orientation,
            filter: crate::image::style::filter(tree, env, id),
            rect,
            rounded,
        },
        mask.as_ref(),
    );
    Ok(())
}
