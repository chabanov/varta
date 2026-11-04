//! Slice an inline's reassembled decoration in visual fragment order. All wide
//! coordinates remain f64 until clipping to a local fragment, before f32 paint.
use super::{
    BoxRect, Corners, PaintError, Units,
    clip_curve::{self, Plane, Point, Segment},
    corner_arc::{self, Quarter},
};
use std::{collections::BTreeMap, sync::Arc};
use tiny_skia::{Path, PathBuilder, Rect};

// Four arcs/straight edges, up to four clipping caps, move/close: reserve before
// generating a fragment contour, within the existing shared text paint work.
pub(super) const SLICE_PAINT_WORK: usize = 4 * (corner_arc::MAX_QUARTER_SEGMENTS as usize + 2) + 3;

#[derive(Debug, Clone, PartialEq)]
struct Shape {
    size: [f64; 2],
    radii: [[f64; 2]; 4],
}
pub(super) struct Decoration<'a> {
    raw: &'a str,
    units: Units,
    size: [f64; 2],
    shapes: BTreeMap<[u32; 2], Arc<Shape>>,
    paint_shapes: BTreeMap<[u64; 4], Arc<Shape>>,
    broken: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Slice {
    shape: Arc<Shape>,
    rect: BoxRect,
    offset: f64,
    paint_shape: Arc<Shape>,
    paint_origin: [f64; 2],
}
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Region {
    Box(BoxRect, Corners),
    Slice(Slice),
}
impl Region {
    pub fn contains(&self, x: f32, y: f32, scale: f32) -> bool {
        match self {
            Self::Box(bounds, radii) => radii.contains(*bounds, x, y),
            Self::Slice(slice) => slice.contains(
                f64::from(x) / f64::from(scale),
                f64::from(y) / f64::from(scale),
            ),
        }
    }
}
impl<'a> Decoration<'a> {
    pub fn new(
        raw: &'a str,
        pieces: &[BoxRect],
        units: Units,
        node: u32,
    ) -> Result<Option<Self>, PaintError> {
        let mut width = 0.0;
        let mut height = None;
        for piece in pieces {
            if !piece.width.is_finite()
                || piece.width < 0.0
                || !piece.height.is_finite()
                || piece.height < 0.0
            {
                return Err(PaintError::Unsupported(node));
            }
            width += f64::from(piece.width);
            if piece.width > 0.0 {
                if height.is_some_and(|old| old != piece.height) {
                    return Err(PaintError::Unsupported(node));
                }
                height = Some(piece.height);
            }
        }
        let size = [width, f64::from(height.unwrap_or(0.0))];
        let radii =
            super::corners::resolve_radii(raw, size, units).ok_or(PaintError::Unsupported(node))?;
        if radii.iter().all(|r| r[0] == 0.0 || r[1] == 0.0) {
            return Ok(None);
        }
        Ok(Some(Self {
            raw,
            units,
            size,
            shapes: BTreeMap::new(),
            paint_shapes: BTreeMap::new(),
            broken: pieces.len() > 1,
        }))
    }
    pub fn slice(
        &mut self,
        rect: BoxRect,
        offset: f64,
        node: u32,
    ) -> Result<Option<Slice>, PaintError> {
        let key = [rect.width.to_bits(), rect.height.to_bits()];
        let shape = if let Some(shape) = self.shapes.get(&key) {
            shape.clone()
        } else {
            // Radius percentages and overlap normalization use the current
            // fragment's border box; painting still spans the whole strip.
            let radii = super::corners::resolve_radii(
                self.raw,
                [f64::from(rect.width), f64::from(rect.height)],
                self.units,
            )
            .ok_or(PaintError::Unsupported(node))?;
            if radii.iter().all(|r| r[0] == 0.0 || r[1] == 0.0) {
                return Ok(None);
            }
            let shape = Arc::new(Shape {
                size: self.size,
                radii,
            });
            self.shapes.insert(key, shape.clone());
            shape
        };
        let (left, width) = super::snap::axis(f64::from(rect.x) - offset, self.size[0]);
        let (top, height) = super::snap::axis(f64::from(rect.y), self.size[1]);
        let paint_key = [
            u64::from(rect.width.to_bits()),
            u64::from(rect.height.to_bits()),
            width.to_bits(),
            height.to_bits(),
        ];
        let paint_shape = if let Some(shape) = self.paint_shapes.get(&paint_key) {
            shape.clone()
        } else {
            // Blink resolves broken-inline corners against the floored flow
            // fragment, then installs them on the snapped continuous strip.
            let (basis, limit) = if self.broken {
                let size = [
                    f64::from(rect.width).floor(),
                    f64::from(rect.height).floor(),
                ];
                (size, size)
            } else {
                (
                    [f64::from(rect.width), f64::from(rect.height)],
                    [width, height],
                )
            };
            let radii = super::corners::resolve_radii_in(self.raw, basis, limit, self.units)
                .ok_or(PaintError::Unsupported(node))?;
            let shape = Arc::new(Shape {
                size: [width, height],
                radii,
            });
            self.paint_shapes.insert(paint_key, shape.clone());
            shape
        };
        Ok(Some(Slice {
            shape,
            rect,
            offset,
            paint_shape,
            paint_origin: [left, top],
        }))
    }
}
impl Shape {
    fn contour(&self, origin: [f64; 2]) -> Vec<Segment> {
        let [left, top] = origin;
        let right = left + self.size[0];
        let bottom = top + self.size[1];
        let [tl, tr, br, bl] = self.radii.map(|r| {
            if r[0] == 0.0 || r[1] == 0.0 {
                [0.0; 2]
            } else {
                r
            }
        });
        let start = Point([left + tl[0], top]);
        let mut current = start;
        let mut result = Vec::new();
        for (end, center, radius, quarter) in [
            (
                [right - tr[0], top],
                [right - tr[0], top + tr[1]],
                tr,
                Quarter::TopRight,
            ),
            (
                [right, bottom - br[1]],
                [right - br[0], bottom - br[1]],
                br,
                Quarter::BottomRight,
            ),
            (
                [left + bl[0], bottom],
                [left + bl[0], bottom - bl[1]],
                bl,
                Quarter::BottomLeft,
            ),
            (
                [left, top + tl[1]],
                [left + tl[0], top + tl[1]],
                tl,
                Quarter::TopLeft,
            ),
        ] {
            let end = Point(end);
            result.push(Segment::Line(current, end));
            current = end;
            if radius[0] > 0.0 && radius[1] > 0.0 {
                corner_arc::cubics(center, radius, quarter, |points| {
                    let mut points = points.map(Point);
                    points[0] = current;
                    current = points[3];
                    result.push(Segment::Cubic(points));
                });
            }
        }
        result.push(Segment::Line(current, start));
        result
    }
}
impl Slice {
    pub fn path(&self, clip: Rect) -> Option<Path> {
        let mut segments = self.paint_shape.contour(self.paint_origin);
        for plane in [
            Plane::new([1.0, 0.0], f64::from(clip.left()))?,
            Plane::new([-1.0, 0.0], -f64::from(clip.right()))?,
            Plane::new([0.0, 1.0], f64::from(clip.top()))?,
            Plane::new([0.0, -1.0], -f64::from(clip.bottom()))?,
        ] {
            segments = clip_curve::half_plane(segments, plane);
        }
        let start = segments.first()?.start();
        let mut path = PathBuilder::new();
        path.move_to(start.0[0] as f32, start.0[1] as f32);
        for segment in segments {
            segment.emit(&mut path);
        }
        path.close();
        path.finish()
    }
    fn contains(&self, x: f64, y: f64) -> bool {
        let [width, height] = self.shape.size;
        let local = [x - f64::from(self.rect.x), y - f64::from(self.rect.y)];
        if local[0] < 0.0
            || local[0] >= f64::from(self.rect.width)
            || local[1] < 0.0
            || local[1] >= f64::from(self.rect.height)
        {
            return false;
        }
        let x = self.offset + local[0];
        let y = local[1];
        for (radius, (ex, ey, sx, sy)) in self.shape.radii.iter().zip([
            (0.0, 0.0, 1.0, 1.0),
            (width, 0.0, -1.0, 1.0),
            (width, height, -1.0, -1.0),
            (0.0, height, 1.0, -1.0),
        ]) {
            let [rx, ry] = *radius;
            if rx == 0.0 || ry == 0.0 {
                continue;
            }
            let dx = (x - ex) * sx;
            let dy = (y - ey) * sy;
            if dx < rx && dy < ry {
                let nx = (dx - rx) / rx;
                let ny = (dy - ry) / ry;
                if nx * nx + ny * ny > 1.0 {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Environment;
    #[test]
    fn clipped_contours_stay_within_the_reserved_segment_work() {
        for width in [37.5_f32, 1000.0, 500_000_000.0] {
            let pieces = [
                BoxRect {
                    x: 2.25,
                    y: 3.5,
                    width,
                    height: 23.75,
                },
                BoxRect {
                    x: 1.5,
                    y: 30.0,
                    width: width * 0.5,
                    height: 23.75,
                },
            ];
            for raw in [
                "50%",
                "100% 0 100% 0",
                "4px 12px 2px 8px",
                "999999999999999999999999999999%",
            ] {
                let mut decoration = Decoration::new(
                    raw,
                    &pieces,
                    Units::new(&Environment::default()).unwrap(),
                    2,
                )
                .unwrap()
                .unwrap();
                let mut offset = 0.0;
                for piece in pieces {
                    let slice = decoration.slice(piece, offset, 2).unwrap().unwrap();
                    let clip =
                        Rect::from_xywh(piece.x, piece.y + 1.0, piece.width, piece.height - 2.0)
                            .unwrap();
                    if let Some(path) = slice.path(clip) {
                        assert!(
                            path.verbs().len() <= SLICE_PAINT_WORK,
                            "{raw}: {}",
                            path.verbs().len()
                        );
                        assert!(
                            path.points()
                                .iter()
                                .all(|p| p.x.is_finite() && p.y.is_finite())
                        );
                    }
                    offset += f64::from(piece.width);
                }
            }
        }
    }
    #[test]
    fn narrow_tail_after_a_wide_composite_survives_until_local_clipping() {
        let pieces = [
            BoxRect {
                x: 0.0,
                y: 0.0,
                width: 500_000_000.0,
                height: 20.0,
            },
            BoxRect {
                x: 0.0,
                y: 20.0,
                width: 500_000_000.0,
                height: 20.0,
            },
            BoxRect {
                x: 2.0,
                y: 40.0,
                width: 3.0,
                height: 20.0,
            },
        ];
        let mut shape = Decoration::new(
            "10px 0 0 10px",
            &pieces,
            Units::new(&Environment::default()).unwrap(),
            2,
        )
        .unwrap()
        .unwrap();
        assert_eq!(shape.size[0], 1_000_000_003.0);
        let slice = shape.slice(pieces[2], 1_000_000_000.0, 2).unwrap().unwrap();
        let path = slice
            .path(Rect::from_xywh(2.0, 40.0, 3.0, 20.0).unwrap())
            .unwrap();
        assert_eq!(path.bounds().width(), 3.0);
        assert!(path.verbs().len() <= SLICE_PAINT_WORK);
        assert!(slice.contains(3.5, 50.0));
        let mut image = tiny_skia::Pixmap::new(3, 20).unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(255, 0, 0, 255);
        image.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::from_translate(-2.0, -40.0),
            None,
        );
        assert!(image.pixels().iter().all(|p| p.alpha() == 255));
    }
}
