//! CSS Backgrounds 3 §6.1. Gaussian convolution of the shadow's opaque
//! rounded box, clipped by the originating border/padding edge. The shader
//! samples in source coordinates, so retained tiles and transforms agree.
use super::{BoxRect, Corners, PaintError, Surface, used_color};
use crate::{render::scene::Primitive, style::Environment, tree::Tree};
use std::sync::Arc;
use tiny_skia::{BlendMode, Color, Mask, Paint, Pixmap, Point, Rect, Transform};
use varta_common::view::{CssProperty, css};

const AA_GRID: usize = 8;
const SHADOW_PAINT_WORK: usize = 1;
mod geometry;
use geometry::{Bounds, CHANNEL_MAX, GAUSSIAN_LIMIT, HALF, Radii, Shape};

#[derive(Clone, PartialEq)]
pub(crate) struct Shadow {
    owner: Shape,
    shape: Shape,
    sigma: f64,
    inset: bool,
    pub bounds: Rect,
}
impl Shadow {
    fn new(
        owner: Shape,
        widths: [f64; 4],
        value: &css::shadow::Shadow,
        clip: Bounds,
    ) -> Option<Self> {
        let owner = if value.inset {
            owner.inset(widths)
        } else {
            owner
        };
        let rect = owner.rect;
        let spread = if value.inset {
            -value.spread
        } else {
            value.spread
        };
        let shape_rect = Bounds {
            x: rect.x + value.x - spread,
            y: rect.y + value.y - spread,
            width: (rect.width + 2.0 * spread).max(0.0),
            height: (rect.height + 2.0 * spread).max(0.0),
        };
        let adjusted = |r: f64| {
            let delta = if spread > 0.0 && r < spread {
                spread * (1.0 + (r / spread - 1.0).powi(3))
            } else {
                spread
            };
            (r + delta).max(0.0)
        };
        let shape = Shape {
            rect: shape_rect,
            corners: Radii(owner.corners.0.map(|r| r.map(adjusted))),
        };
        let reach = value.blur * GAUSSIAN_LIMIT * HALF;
        let extent = if value.inset {
            rect
        } else {
            Bounds {
                x: shape_rect.x - reach,
                y: shape_rect.y - reach,
                width: shape_rect.width + 2.0 * reach,
                height: shape_rect.height + 2.0 * reach,
            }
        };
        let left = extent.x.max(clip.x);
        let top = extent.y.max(clip.y);
        let right = (extent.x + extent.width).min(clip.x + clip.width);
        let bottom = (extent.y + extent.height).min(clip.y + clip.height);
        // Clip in f64 before converting to device drawing coordinates.
        if left >= right || top >= bottom {
            return None;
        }
        let bounds = Rect::from_ltrb(left as f32, top as f32, right as f32, bottom as f32)?;
        Some(Self {
            owner,
            shape,
            sigma: value.blur * HALF,
            inset: value.inset,
            bounds,
        })
    }
    fn sample(&self, x: f64, y: f64) -> f64 {
        let inside = self.owner.contains(x, y);
        if inside != self.inset {
            return 0.0;
        }
        let blurred = self.shape.blurred(x, y, self.sigma);
        if self.inset { 1.0 - blurred } else { blurred }
    }
    fn pixel(&self, inverse: Transform, px: f32, py: f32) -> f64 {
        let mut points = [
            Point::from_xy(px, py),
            Point::from_xy(px + 1.0, py),
            Point::from_xy(px, py + 1.0),
            Point::from_xy(px + 1.0, py + 1.0),
        ];
        inverse.map_points(&mut points);
        if !self.inset
            && points
                .iter()
                .all(|p| self.owner.contains(f64::from(p.x), f64::from(p.y)))
        {
            return 0.0;
        }
        // Blur is smooth within a pixel; integrate just the knockout edge.
        // A sharp shadow integrates both rounded edges on a fixed grid.
        let mut center = Point::from_xy(px + 0.5, py + 0.5);
        inverse.map_point(&mut center);
        let point = |p: Point| (f64::from(p.x), f64::from(p.y));
        let smooth = self.sigma * f64::from(inverse.sx.abs().max(inverse.sy.abs()).recip()) >= HALF;
        let center_inside = self
            .owner
            .contains(f64::from(center.x), f64::from(center.y));
        let uniform = points
            .iter()
            .all(|p| self.owner.contains(f64::from(p.x), f64::from(p.y)) == center_inside);
        let owner = self.owner.rect;
        let pixel_min = points
            .iter()
            .fold([f32::INFINITY; 2], |p, q| [p[0].min(q.x), p[1].min(q.y)]);
        let pixel_max = points.iter().fold([f32::NEG_INFINITY; 2], |p, q| {
            [p[0].max(q.x), p[1].max(q.y)]
        });
        let disjoint = f64::from(pixel_max[0]) <= owner.x
            || f64::from(pixel_min[0]) >= owner.x + owner.width
            || f64::from(pixel_max[1]) <= owner.y
            || f64::from(pixel_min[1]) >= owner.y + owner.height;
        if smooth && uniform && (center_inside || disjoint) {
            let (x, y) = point(center);
            return self.sample(x, y);
        }
        let mut sum = 0.0;
        let center_blur = if smooth {
            Some(
                self.shape
                    .blurred(f64::from(center.x), f64::from(center.y), self.sigma),
            )
        } else {
            None
        };
        for row in 0..AA_GRID {
            for col in 0..AA_GRID {
                let mut p = Point::from_xy(
                    px + (col as f32 + 0.5) / AA_GRID as f32,
                    py + (row as f32 + 0.5) / AA_GRID as f32,
                );
                inverse.map_point(&mut p);
                let (x, y) = point(p);
                if self.owner.contains(x, y) == self.inset {
                    let coverage =
                        center_blur.unwrap_or_else(|| self.shape.blurred(x, y, self.sigma));
                    sum += if self.inset { 1.0 - coverage } else { coverage };
                }
            }
        }
        sum / (AA_GRID * AA_GRID) as f64
    }
    pub fn draw(&self, target: &mut Pixmap, raster: Raster<'_>) {
        let Raster {
            transform,
            origin,
            dirty,
            color,
            blend,
            mask,
        } = raster;
        let Some(inverse) = transform.invert() else {
            return;
        };
        let Some(extent) = Rect::from_xywh(
            origin.0 as f32,
            origin.1 as f32,
            target.width() as f32,
            target.height() as f32,
        ) else {
            return;
        };
        let Some(visible) = self
            .bounds
            .transform(transform)
            .and_then(|r| r.intersect(&dirty))
            .and_then(|r| r.intersect(&extent))
        else {
            return;
        };
        let left = visible.left().floor().max(origin.0 as f32) as u32;
        let right = visible.right().ceil().min(extent.right()) as u32;
        let top = visible.top().floor().max(origin.1 as f32) as u32;
        let bottom = visible.bottom().ceil().min(extent.bottom()) as u32;
        let width = target.width() as usize;
        for y in top..bottom {
            for x in left..right {
                let index = ((y - origin.1) as usize * width) + (x - origin.0) as usize;
                let clip = mask.map_or(1.0, |m| f64::from(m.data()[index]) / CHANNEL_MAX);
                if clip == 0.0 {
                    continue;
                }
                let coverage = self.pixel(inverse, x as f32, y as f32) * clip;
                if coverage == 0.0 {
                    continue;
                }
                let a = (f64::from(color.alpha()) * coverage * CHANNEL_MAX)
                    .round()
                    .clamp(0.0, CHANNEL_MAX) as u16;
                let source = [color.red(), color.green(), color.blue()]
                    .map(|c| (f64::from(c) * f64::from(a)).round() as u16);
                let destination = &mut target.data_mut()[index * 4..index * 4 + 4];
                for (channel, src) in source.into_iter().chain([a]).enumerate() {
                    let dst = u16::from(destination[channel]);
                    destination[channel] = if blend == BlendMode::Plus {
                        (src + dst).min(255) as u8
                    } else {
                        (src + (dst * (255 - a) + 127) / 255).min(255) as u8
                    };
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct PaintBox {
    pub rect: BoxRect,
    pub corners: Corners,
    pub widths: [f32; 4],
    pub clip: BoxRect,
}
pub(crate) struct Raster<'a> {
    pub transform: Transform,
    pub origin: (u32, u32),
    pub dirty: Rect,
    pub color: Color,
    pub blend: BlendMode,
    pub mask: Option<&'a Mask>,
}
pub(super) fn paint(
    surface: &mut Surface<'_>,
    (tree, env, id): (&Tree, &Environment, u32),
    bounds: PaintBox,
    inset: bool,
) -> Result<(), PaintError> {
    let PaintBox {
        rect,
        corners,
        widths,
        clip,
    } = bounds;
    let properties = tree.computed_read(id, env);
    let Some(raw) = properties.get(&CssProperty::BoxShadow) else {
        return Ok(());
    };
    let values = css::shadow::parse(raw).ok_or(PaintError::Unsupported(id))?;
    for value in values.iter().rev().filter(|v| v.inset == inset) {
        let color = used_color(tree, env, id, &value.color).ok_or(PaintError::Unsupported(id))?;
        if color.alpha() == 0.0 {
            continue;
        }
        let Some(shadow) = Shadow::new(
            Shape {
                rect: rect.into(),
                corners: corners.into(),
            },
            widths.map(f64::from),
            value,
            surface.local_clip(clip).into(),
        ) else {
            continue;
        };
        // The shader already integrates its silhouette. Only the actual clip
        // belongs in this mask; masking the ink bounds would square coverage
        // at a fractional edge.
        let clip = surface.local_clip(clip);
        let mask = surface.mask(
            Rect::from_xywh(clip.x, clip.y, clip.width, clip.height)
                .ok_or(PaintError::Unsupported(id))?,
        )?;
        let mut paint = Paint::default();
        paint.set_color(color);
        surface.record(
            Primitive::Shadow(Arc::new(shadow)),
            &paint,
            Transform::identity(),
            Some(&mask),
        );
    }
    Ok(())
}

/// A fragment cuts the reassembled decoration only at its internal line edges;
/// outer edges and vertical shadow ink may overflow the font box.
pub(super) fn inline(
    surface: &mut Surface<'_>,
    (tree, env, id, work): (&Tree, &Environment, u32, &mut usize),
    (slice, widths, clip): (&super::inline_decor::Slice, [f64; 4], BoxRect),
    inset: bool,
) -> Result<(), PaintError> {
    let properties = tree.computed_read(id, env);
    let Some(raw) = properties.get(&CssProperty::BoxShadow) else {
        return Ok(());
    };
    let values = css::shadow::parse(raw).ok_or(PaintError::Unsupported(id))?;
    let (bounds, radii, cuts) = slice.shadow_geometry();
    let owner = Shape {
        rect: Bounds {
            x: bounds[0],
            y: bounds[1],
            width: bounds[2],
            height: bounds[3],
        },
        corners: Radii(radii),
    };
    let mut clip: Bounds = surface.local_clip(clip).into();
    if let Some(left) = cuts[0] {
        let right = clip.x + clip.width;
        clip.x = clip.x.max(left);
        clip.width = (right - clip.x).max(0.0);
    }
    if let Some(right) = cuts[1] {
        clip.width = clip.width.min((right - clip.x).max(0.0));
    }
    for value in values.iter().rev().filter(|v| v.inset == inset) {
        super::inline_groups::charge_paint(work, SHADOW_PAINT_WORK, id)?;
        let color = used_color(tree, env, id, &value.color).ok_or(PaintError::Unsupported(id))?;
        if color.alpha() == 0.0 {
            continue;
        }
        let Some(shadow) = Shadow::new(owner, widths, value, clip) else {
            continue;
        };
        let mask = surface.mask(
            Rect::from_xywh(
                clip.x as f32,
                clip.y as f32,
                clip.width as f32,
                clip.height as f32,
            )
            .ok_or(PaintError::Unsupported(id))?,
        )?;
        let mut paint = Paint::default();
        paint.set_color(color);
        surface.record(
            Primitive::Shadow(Arc::new(shadow)),
            &paint,
            Transform::identity(),
            Some(&mask),
        );
    }
    Ok(())
}
