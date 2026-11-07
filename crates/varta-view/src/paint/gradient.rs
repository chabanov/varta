//! CSS linear-gradient backgrounds, sampled directly into retained/isolated tiles.
//! No per-background image, blur kernel or additional texture/cache allowance.
use super::{BoxRect, Corners, Environment, PaintError, Surface, Tree, Units, used_color};
use crate::{paint::shadow::Raster, render::scene::Primitive};
use std::sync::Arc;
use tiny_skia::{BlendMode, Color, Paint, Pixmap, Point, Rect, Transform};
use varta_common::view::{
    CssProperty as P,
    css::{
        self,
        color::{self, Space},
    },
};
const CHANNEL: f64 = 255.0;
const EDGE_SAMPLES: usize = 8;
#[derive(Clone, PartialEq)]
struct Stop {
    position: f64,
    color: color::Color,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Gradient {
    pub bounds: Rect,
    rect: BoxRect,
    tile: BoxRect,
    corners: Corners,
    vector: [f64; 2],
    length: f64,
    space: Space,
    stops: Vec<Stop>,
}
impl Gradient {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.stops.len() * std::mem::size_of::<Stop>()
    }
    fn coverage(&self, inverse: Transform, x: f32, y: f32) -> f64 {
        let mut points = [
            Point::from_xy(x, y),
            Point::from_xy(x + 1.0, y),
            Point::from_xy(x, y + 1.0),
            Point::from_xy(x + 1.0, y + 1.0),
        ];
        inverse.map_points(&mut points);
        if points
            .iter()
            .all(|p| self.corners.contains(self.rect, p.x, p.y))
        {
            return 1.0;
        }
        let mut covered = 0;
        for row in 0..EDGE_SAMPLES {
            for col in 0..EDGE_SAMPLES {
                let mut p = Point::from_xy(
                    x + (col as f32 + 0.5) / EDGE_SAMPLES as f32,
                    y + (row as f32 + 0.5) / EDGE_SAMPLES as f32,
                );
                inverse.map_point(&mut p);
                covered += usize::from(self.corners.contains(self.rect, p.x, p.y));
            }
        }
        covered as f64 / (EDGE_SAMPLES * EDGE_SAMPLES) as f64
    }
    fn sample(&self, x: f64, y: f64) -> [f64; 4] {
        let tile = self.tile;
        // The default repeat paints the padding-box-sized generated image
        // underneath transparent borders too.
        let x = f64::from(tile.x) + (x - f64::from(tile.x)).rem_euclid(f64::from(tile.width));
        let y = f64::from(tile.y) + (y - f64::from(tile.y)).rem_euclid(f64::from(tile.height));
        let center = [
            f64::from(tile.x + tile.width * 0.5),
            f64::from(tile.y + tile.height * 0.5),
        ];
        let position = ((x - center[0]) * self.vector[0] + (y - center[1]) * self.vector[1])
            / self.length
            + 0.5;
        let last = self.stops.len() - 1;
        let upper = self.stops.partition_point(|s| s.position <= position);
        let color = if upper == 0 {
            self.stops[0].color
        } else if upper > last {
            self.stops[last].color
        } else {
            let a = &self.stops[upper - 1];
            let b = &self.stops[upper];
            color::interpolation_sample(
                a.color,
                b.color,
                (position - a.position) / (b.position - a.position),
                self.space,
            )
        };
        let [r, g, b] = color::to_srgb(color);
        [r, g, b, color.alpha.unwrap_or(1.0)]
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
        let width = target.width() as usize;
        for y in visible.top().floor().max(extent.top()) as u32
            ..visible.bottom().ceil().min(extent.bottom()) as u32
        {
            for x in visible.left().floor().max(extent.left()) as u32
                ..visible.right().ceil().min(extent.right()) as u32
            {
                let index = (y - origin.1) as usize * width + (x - origin.0) as usize;
                let coverage = self.coverage(inverse, x as f32, y as f32)
                    * mask.map_or(1.0, |m| f64::from(m.data()[index]) / CHANNEL);
                if coverage == 0.0 {
                    continue;
                }
                let mut p = Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
                inverse.map_point(&mut p);
                let [r, g, b, a] = self.sample(f64::from(p.x), f64::from(p.y));
                let alpha = (a * coverage * f64::from(color.alpha()) * CHANNEL)
                    .round()
                    .clamp(0.0, CHANNEL) as u16;
                let source = [r, g, b].map(|v| (v * f64::from(alpha)).round() as u16);
                for (dst, src) in target.data_mut()[index * 4..index * 4 + 4]
                    .iter_mut()
                    .zip(source.into_iter().chain([alpha]))
                {
                    *dst = if blend == BlendMode::Plus {
                        (src + u16::from(*dst)).min(255) as u8
                    } else {
                        (src + (u16::from(*dst) * (255 - alpha) + 127) / 255).min(255) as u8
                    };
                }
            }
        }
    }
}
pub(super) fn paint(
    surface: &mut Surface<'_>,
    (tree, env, id): (&Tree, &Environment, u32),
    bounds: super::shadow::PaintBox,
) -> Result<(), PaintError> {
    let properties = tree.computed_read(id, env);
    let Some(raw) = properties.get(&P::BackgroundImage) else {
        return Ok(());
    };
    let Some(value) = css::gradient::parse(raw).ok_or(PaintError::Unsupported(id))? else {
        return Ok(());
    };
    let super::shadow::PaintBox {
        rect,
        corners,
        widths,
        clip,
    } = bounds;
    let tile = BoxRect {
        x: rect.x + widths[3],
        y: rect.y + widths[0],
        width: (rect.width - widths[1] - widths[3]).max(0.0),
        height: (rect.height - widths[0] - widths[2]).max(0.0),
    };
    if tile.width == 0.0 || tile.height == 0.0 {
        return Ok(());
    }
    let vector = match value.direction {
        css::gradient::Direction::Angle(degrees) => {
            let a = degrees.to_radians();
            [a.sin(), -a.cos()]
        }
        css::gradient::Direction::Corner([x, y]) => {
            let v = if x == 0 {
                [0.0, f64::from(y)]
            } else if y == 0 {
                [f64::from(x), 0.0]
            } else {
                [
                    f64::from(x) * f64::from(tile.height),
                    f64::from(y) * f64::from(tile.width),
                ]
            };
            let size = v[0].hypot(v[1]);
            v.map(|n| n / size)
        }
    };
    let length = f64::from(tile.width) * vector[0].abs() + f64::from(tile.height) * vector[1].abs();
    let units = Units::new(env).ok_or(PaintError::Unsupported(id))?;
    let mut colors = Vec::new();
    let mut positions = Vec::new();
    for stop in value.stops {
        let c = used_color(tree, env, id, &stop.color).ok_or(PaintError::Unsupported(id))?;
        let c = color::interpolation_endpoint(
            color::Color {
                space: Space::Srgb,
                components: [c.red(), c.green(), c.blue()].map(|v| Some(f64::from(v))),
                alpha: Some(f64::from(c.alpha())),
            },
            value.space,
        );
        if stop.positions.is_empty() {
            colors.push(c);
            positions.push(None);
        } else {
            for position in stop.positions {
                let position = if let Some(n) = position.strip_suffix('%') {
                    n.parse::<f64>().ok().map(|n| n / 100.0)
                } else {
                    units.pixels(&position, true).map(|n| f64::from(n) / length)
                }
                .ok_or(PaintError::Unsupported(id))?;
                colors.push(c);
                positions.push(Some(position));
            }
        }
    }
    let last = positions.len() - 1;
    positions[0].get_or_insert(0.0);
    positions[last].get_or_insert(1.0);
    let mut previous = 0;
    for at in 1..positions.len() {
        if let Some(position) = positions[at] {
            let start = positions[previous].ok_or(PaintError::Unsupported(id))?;
            let end = position.max(start);
            for (step, slot) in positions[previous + 1..=at].iter_mut().enumerate() {
                *slot = Some(start + (end - start) * (step + 1) as f64 / (at - previous) as f64);
            }
            previous = at;
        }
    }
    let Some(bounds) = Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) else {
        return Ok(());
    };
    let gradient = Gradient {
        bounds,
        rect,
        tile,
        corners,
        vector,
        length,
        space: value.space,
        stops: colors
            .into_iter()
            .zip(positions)
            .map(|(color, position)| Stop {
                color,
                position: position.unwrap_or(0.0),
            })
            .collect(),
    };
    let clip = surface.local_clip(clip);
    let Some(clip) = Rect::from_xywh(clip.x, clip.y, clip.width, clip.height) else {
        return Ok(());
    };
    let mask = surface.mask(clip)?;
    let mut paint = Paint::default();
    paint.set_color(Color::WHITE);
    surface.record(
        Primitive::Gradient(Arc::new(gradient)),
        &paint,
        Transform::identity(),
        Some(&mask),
    );
    Ok(())
}
