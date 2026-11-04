//! Corner rounding of union boundaries, preserving their convex/concave topology.
use super::clip_curve::{Point, Segment};
use super::corner_arc::{self, Quarter};
use super::corners::Corners;
use super::{BoxRect, PaintError};
use tiny_skia::Path;

const CORNER_WORK: usize = 4 + corner_arc::MAX_QUARTER_SEGMENTS as usize * 4;
#[derive(Clone, Copy, Default)]
struct Corner {
    incoming: [f64; 2],
    outgoing: [f64; 2],
    before: f64,
    after: f64,
    clockwise: bool,
    quadrant: usize,
}
fn direction(a: Point, b: Point) -> [f64; 2] {
    std::array::from_fn(|i| {
        if a.0[i] < b.0[i] {
            1.0
        } else if a.0[i] > b.0[i] {
            -1.0
        } else {
            0.0
        }
    })
}
fn corner(a: Point, b: Point, c: Point, convex: Corners, concave: Corners) -> Corner {
    let incoming = direction(a, b);
    let outgoing = direction(b, c);
    let cross = incoming[0] * outgoing[1] - incoming[1] * outgoing[0];
    let mut result = Corner {
        incoming,
        outgoing,
        clockwise: cross > 0.0,
        ..Corner::default()
    };
    if cross == 0.0 {
        return result;
    }
    let normal = [-incoming[0] + outgoing[0], -incoming[1] + outgoing[1]];
    let quadrant = match (normal[0] > 0.0, normal[1] > 0.0) {
        (true, true) => 0,
        (false, true) => 1,
        (false, false) => 2,
        (true, false) => 3,
    };
    let radii = if cross > 0.0 { convex } else { concave }.0[quadrant].map(f64::from);
    result.before = if incoming[0] != 0.0 {
        radii[0]
    } else {
        radii[1]
    };
    result.after = if outgoing[0] != 0.0 {
        radii[0]
    } else {
        radii[1]
    };
    result.quadrant = quadrant;
    result
}
fn rounded(
    contours: &[Vec<Point>],
    convex: Corners,
    concave: Corners,
    work: &mut usize,
    node: u32,
) -> Result<Vec<Vec<Segment>>, PaintError> {
    let mut result = Vec::new();
    for points in contours {
        super::outline_union::charge(work, points.len().saturating_mul(CORNER_WORK), node)?;
        let count = points.len();
        if count == 0 {
            continue;
        }
        let mut corners = (0..count)
            .map(|i| {
                corner(
                    points[(i + count - 1) % count],
                    points[i],
                    points[(i + 1) % count],
                    convex,
                    concave,
                )
            })
            .collect::<Vec<_>>();
        // Fit the two incident radii independently to each shared edge. Keep
        // the browser's integral shortening without flattening the resulting arc.
        for i in 0..count {
            let next = (i + 1) % count;
            let a = points[i];
            let b = points[next];
            let length = (a.0[0] - b.0[0]).abs() + (a.0[1] - b.0[1]).abs();
            let sum = corners[i].after + corners[next].before;
            if sum > length {
                let scale = length / sum;
                corners[i].after = (corners[i].after * scale).floor();
                corners[next].before = (corners[next].before * scale).floor();
            }
        }
        let mut segments = Vec::new();
        for i in 0..count {
            let p = points[i];
            let c = corners[i];
            let a = Point(std::array::from_fn(|j| p.0[j] - c.incoming[j] * c.before));
            let b = Point(std::array::from_fn(|j| p.0[j] + c.outgoing[j] * c.after));
            let center = std::array::from_fn(|j| a.0[j] + c.outgoing[j] * c.after);
            let radius = std::array::from_fn(|j| (p.0[j] - center[j]).abs());
            if c.before == 0.0 || c.after == 0.0 {
                if a != b {
                    segments.push(Segment::Line(a, b));
                }
            } else {
                let quarter = match c.quadrant {
                    0 => Quarter::TopLeft,
                    1 => Quarter::TopRight,
                    2 => Quarter::BottomRight,
                    _ => Quarter::BottomLeft,
                };
                let mut curves = [[[0.0; 2]; 4]; corner_arc::MAX_QUARTER_SEGMENTS as usize];
                let mut used = 0;
                corner_arc::cubics(center, radius, quarter, |curve| {
                    curves[used] = curve;
                    used += 1;
                });
                for j in 0..used {
                    let mut curve = curves[if c.clockwise { j } else { used - j - 1 }].map(Point);
                    if !c.clockwise {
                        curve.reverse();
                    }
                    segments.push(Segment::Cubic(curve));
                }
            }
            let next = (i + 1) % count;
            let n = corners[next];
            let end = Point(std::array::from_fn(|j| {
                points[next].0[j] - n.incoming[j] * n.before
            }));
            if b != end {
                segments.push(Segment::Line(b, end));
            }
        }
        result.push(segments);
    }
    Ok(result)
}
/// Multi-fragment radii are resolved against the first fragment. Outer/inner
/// corners exchange radii at concavities, including folded negative-offset paths.
pub(super) fn ring(
    rects: &[[f64; 4]],
    style: (f64, f64, Corners),
    clip: BoxRect,
    work: &mut usize,
    node: u32,
) -> Result<Option<Path>, PaintError> {
    let (width, offset, radii) = style;
    let shape = super::outline_union::shape(rects, width, offset, work, node)?;
    let inner_radii = radii.outset(offset as f32, offset as f32);
    let outer_radii = radii.outset((offset + width) as f32, (offset + width) as f32);
    let outer = rounded(&shape.outer, outer_radii, inner_radii, work, node)?;
    let inner = rounded(&shape.inner, inner_radii, outer_radii, work, node)?;
    super::curve_region::difference(&outer, &inner, clip, work, node)
}
