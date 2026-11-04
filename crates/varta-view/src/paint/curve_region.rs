//! Vertical decomposition of cubic regions; output retains exact subcurves.
//! All edge-pair refinement, root solves and output share text preparation work.
use super::clip_curve::{Plane, Point, ROOT_STEPS, Segment};
use super::{BoxRect, MAX_TEXT_PAINT_WORK, PaintError};
use tiny_skia::{Path, PathBuilder};

const GEOMETRY_EPS: f64 = 1e-8;
const REFINEMENT_DEPTH: usize = 48;
const REGION_COUNT: usize = 3;
const EDGE_WORK: usize = 8;
#[derive(Clone, Copy)]
enum Region {
    Outer,
    Inner,
    Clip,
}
impl Region {
    fn index(self) -> usize {
        self as usize
    }
}
#[derive(Clone, Copy)]
struct Edge {
    segment: Segment,
    region: Region,
    delta: i32,
}
#[derive(Clone, Copy)]
struct Crossing {
    edge: Edge,
    segment: Segment,
    x: f64,
}
fn charge(work: &mut usize, amount: usize, node: u32) -> Result<(), PaintError> {
    *work = work.saturating_add(amount);
    if *work > MAX_TEXT_PAINT_WORK {
        Err(PaintError::Unsupported(node))
    } else {
        Ok(())
    }
}
fn reverse(s: Segment) -> Segment {
    match s {
        Segment::Line(a, b) => Segment::Line(b, a),
        Segment::Cubic([a, b, c, d]) => Segment::Cubic([d, c, b, a]),
    }
}
fn bounds(s: Segment) -> [f64; 2] {
    match s {
        Segment::Line(a, b) => [a.0[0].min(b.0[0]), a.0[0].max(b.0[0])],
        Segment::Cubic(p) => p
            .into_iter()
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[a, b], p| {
                [a.min(p.0[0]), b.max(p.0[0])]
            }),
    }
}
fn slice(s: Segment, lo: f64, hi: f64, work: &mut usize, node: u32) -> Result<Segment, PaintError> {
    let mut s = s;
    // Edges are normalized top-to-bottom before any solve, so common source
    // curves receive identical operations even with opposite winding.
    for (y, keep_lower) in [(lo, true), (hi, false)] {
        if y > s.start().0[1] && y < s.end().0[1] {
            charge(work, ROOT_STEPS, node)?;
            let (a, b) = match s {
                Segment::Line(p, q) => s.split((y - p.0[1]) / (q.0[1] - p.0[1])),
                Segment::Cubic(_) => {
                    s.crossing(Plane::new([0.0, 1.0], y).ok_or(PaintError::Unsupported(node))?)
                }
            };
            s = if keep_lower { b } else { a };
        }
    }
    Ok(s)
}
fn same(a: Segment, b: Segment) -> bool {
    match (a, b) {
        (Segment::Line(p, q), Segment::Line(r, s)) => {
            p == r && q == s || p.0[0] == q.0[0] && r.0[0] == s.0[0] && p.0[0] == r.0[0]
        }
        (Segment::Cubic(a), Segment::Cubic(b)) => a == b,
        _ => {
            let x = bounds(a);
            let y = bounds(b);
            x[0] == x[1] && x == y
        }
    }
}
fn intersections(
    a: Segment,
    b: Segment,
    ys: &mut Vec<f64>,
    work: &mut usize,
    node: u32,
) -> Result<(), PaintError> {
    let lo = a.start().0[1].max(b.start().0[1]);
    let hi = a.end().0[1].min(b.end().0[1]);
    if lo >= hi || same(a, b) {
        return Ok(());
    }
    let a = slice(a, lo, hi, work, node)?;
    let b = slice(b, lo, hi, work, node)?;
    let mut pending = vec![(a, b, lo, hi, 0)];
    while let Some((a, b, lo, hi, depth)) = pending.pop() {
        charge(work, EDGE_WORK, node)?;
        let x = bounds(a);
        let y = bounds(b);
        if x[1] < y[0] || y[1] < x[0] || same(a, b) {
            continue;
        }
        // Overlapping boxes are refined geometrically, not flattened. A
        // possible tangency may add a harmless band boundary as well.
        let middle = lo + (hi - lo) * 0.5;
        if hi - lo <= GEOMETRY_EPS || middle == lo || middle == hi || depth == REFINEMENT_DEPTH {
            charge(work, 1, node)?;
            ys.push(middle);
            continue;
        }
        let a0 = slice(a, lo, middle, work, node)?;
        let a1 = slice(a, middle, hi, work, node)?;
        let b0 = slice(b, lo, middle, work, node)?;
        let b1 = slice(b, middle, hi, work, node)?;
        pending.push((a1, b1, middle, hi, depth + 1));
        pending.push((a0, b0, lo, middle, depth + 1));
    }
    Ok(())
}
fn edges(
    contours: &[Vec<Segment>],
    region: Region,
    output: &mut Vec<Edge>,
    ys: &mut Vec<f64>,
    work: &mut usize,
    node: u32,
) -> Result<(), PaintError> {
    for contour in contours {
        for &segment in contour {
            charge(work, EDGE_WORK, node)?;
            let points = match segment {
                Segment::Line(a, b) => [a, b, b, b],
                Segment::Cubic(p) => p,
            };
            if !points
                .into_iter()
                .all(|p| p.0.into_iter().all(f64::is_finite))
            {
                return Err(PaintError::Unsupported(node));
            }
            let (roots, count) =
                segment.projected_extrema(Plane::new([0.0, 1.0], 0.0).expect("unit axis"));
            let mut rest = segment;
            let mut previous = 0.0;
            for t in roots.into_iter().take(count).chain(std::iter::once(1.0)) {
                let (s, tail) = rest.split((t - previous) / (1.0 - previous));
                rest = tail;
                previous = t;
                let a = s.start().0[1];
                let b = s.end().0[1];
                ys.extend([a, b]);
                if a != b {
                    output.push(Edge {
                        segment: if a < b { s } else { reverse(s) },
                        region,
                        delta: if a < b { -1 } else { 1 },
                    });
                }
            }
        }
    }
    Ok(())
}
fn inside(w: [i32; REGION_COUNT]) -> bool {
    // Inner plus one CCW enclosing rectangle is the inverse winding clip.
    w[Region::Outer.index()] != 0 && w[Region::Inner.index()] != 1 && w[Region::Clip.index()] != 0
}
fn emit(
    path: &mut PathBuilder,
    left: Segment,
    right: Segment,
    work: &mut usize,
    node: u32,
) -> Result<(), PaintError> {
    charge(work, EDGE_WORK, node)?;
    let a = left.start();
    let b = right.start();
    let d = left.end();
    path.move_to(a.0[0] as f32, a.0[1] as f32);
    path.line_to(b.0[0] as f32, b.0[1] as f32);
    right.emit(path);
    path.line_to(d.0[0] as f32, d.0[1] as f32);
    reverse(left).emit(path);
    path.close();
    // Both curves were cut at the same y boundaries. Their common cell edges
    // cancel inside this one fill, preserving translucent coverage at joins.
    Ok(())
}
/// Outer nonzero winding intersected with inverse inner winding and the viewport.
/// No raster textures, polygon flattening, or separate allowance per contour.
pub(super) fn difference(
    outer: &[Vec<Segment>],
    inner: &[Vec<Segment>],
    clip: BoxRect,
    work: &mut usize,
    node: u32,
) -> Result<Option<Path>, PaintError> {
    let rect = [
        f64::from(clip.x),
        f64::from(clip.y),
        f64::from(clip.x) + f64::from(clip.width),
        f64::from(clip.y) + f64::from(clip.height),
    ];
    if !rect.into_iter().all(f64::is_finite) {
        return Err(PaintError::Unsupported(node));
    }
    if clip.width <= 0.0 || clip.height <= 0.0 {
        return Ok(None);
    }
    let [l, t, r, b] = rect;
    let p = [Point([l, t]), Point([r, t]), Point([r, b]), Point([l, b])];
    let viewport = vec![
        (0..p.len())
            .map(|i| Segment::Line(p[i], p[(i + 1) % p.len()]))
            .collect(),
    ];
    let mut all = Vec::new();
    let mut ys = vec![t, b];
    edges(outer, Region::Outer, &mut all, &mut ys, work, node)?;
    edges(inner, Region::Inner, &mut all, &mut ys, work, node)?;
    edges(&viewport, Region::Clip, &mut all, &mut ys, work, node)?;
    all.retain(|e| e.segment.end().0[1] > t && e.segment.start().0[1] < b);
    // Charge every candidate pair, including a quick bounding-box rejection.
    for i in 0..all.len() {
        for j in i + 1..all.len() {
            charge(work, 1, node)?;
            let a = all[i].segment;
            let c = all[j].segment;
            let lo = t.max(a.start().0[1]).max(c.start().0[1]);
            let hi = b.min(a.end().0[1]).min(c.end().0[1]);
            if lo < hi {
                intersections(
                    slice(a, lo, hi, work, node)?,
                    slice(c, lo, hi, work, node)?,
                    &mut ys,
                    work,
                    node,
                )?;
            }
        }
    }
    charge(
        work,
        ys.len().saturating_mul(if ys.len() > 1 {
            ys.len().ilog2() as usize + 1
        } else {
            1
        }),
        node,
    )?;
    ys.retain(|y| *y >= t && *y <= b);
    ys.sort_unstable_by(f64::total_cmp);
    ys.dedup();
    let mut path = PathBuilder::new();
    for band in ys.windows(2) {
        let (lo, hi) = (band[0], band[1]);
        if lo >= hi {
            continue;
        }
        let mid = lo + (hi - lo) * 0.5;
        let mut crossings = Vec::new();
        for &edge in &all {
            charge(work, 1, node)?;
            if edge.segment.start().0[1] > lo || edge.segment.end().0[1] < hi {
                continue;
            }
            let segment = slice(edge.segment, lo, hi, work, node)?;
            let x = slice(segment, lo, mid, work, node)?.end().0[0];
            crossings.push(Crossing { edge, segment, x });
        }
        charge(
            work,
            crossings.len().saturating_mul(if crossings.len() > 1 {
                crossings.len().ilog2() as usize + 1
            } else {
                1
            }),
            node,
        )?;
        crossings.sort_unstable_by(|a, b| a.x.total_cmp(&b.x));
        let mut winding = [0; REGION_COUNT];
        let mut left = None;
        let mut index = 0;
        while index < crossings.len() {
            let crossing = crossings[index];
            let before = inside(winding);
            while index < crossings.len() && same(crossings[index].segment, crossing.segment) {
                let e = crossings[index].edge;
                winding[e.region.index()] += e.delta;
                index += 1;
            }
            let after = inside(winding);
            if !before && after {
                left = Some(crossing.segment);
            }
            if before && !after {
                emit(
                    &mut path,
                    left.take().ok_or(PaintError::Unsupported(node))?,
                    crossing.segment,
                    work,
                    node,
                )?;
            }
        }
        if winding != [0; REGION_COUNT] || left.is_some() {
            return Err(PaintError::Unsupported(node));
        }
    }
    Ok(path.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{FillRule, Paint, Pixmap, Transform};
    fn rectangle(r: [f64; 4]) -> Vec<Segment> {
        let [l, t, r, b] = r;
        let p = [Point([l, t]), Point([r, t]), Point([r, b]), Point([l, b])];
        (0..p.len())
            .map(|i| Segment::Line(p[i], p[(i + 1) % p.len()]))
            .collect()
    }
    fn circle(center: [f64; 2], r: f64) -> Vec<Segment> {
        use super::super::corner_arc::{Quarter, cubics};
        let mut out = Vec::new();
        for q in [
            Quarter::TopRight,
            Quarter::BottomRight,
            Quarter::BottomLeft,
            Quarter::TopLeft,
        ] {
            cubics(center, [r, r], q, |c| {
                out.push(Segment::Cubic(c.map(Point)))
            });
        }
        out
    }
    fn raster(outer: &[Vec<Segment>], inner: &[Vec<Segment>]) -> Pixmap {
        let path = difference(
            outer,
            inner,
            BoxRect {
                x: 0.,
                y: 0.,
                width: 32.,
                height: 32.,
            },
            &mut 0,
            2,
        )
        .unwrap()
        .unwrap();
        let mut image = Pixmap::new(32, 32).unwrap();
        let mut p = Paint::default();
        p.set_color_rgba8(255, 0, 0, 128);
        image.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        image
    }
    #[test]
    fn independent_outer_clip_excludes_inverted_inner_regions_and_preserves_winding_folds() {
        let outer = vec![rectangle([2., 2., 20., 20.])];
        let inner = rectangle([6., 6., 16., 16.]);
        let single = raster(&outer, std::slice::from_ref(&inner));
        assert_eq!(single.pixel(10, 10).unwrap().alpha(), 0);
        let double = raster(&outer, &[inner.clone(), inner]);
        assert_eq!(double.pixel(10, 10).unwrap().alpha(), 128);
        let mut outside = rectangle([22., 2., 28., 8.]);
        outside = outside.into_iter().rev().map(reverse).collect();
        let image = raster(&outer, &[outside]);
        assert_eq!(image.pixel(24, 4).unwrap().alpha(), 0);
    }
    #[test]
    fn crossing_cubic_regions_keep_real_curves_and_shared_translucent_coverage() {
        let outer = vec![rectangle([0., 0., 30., 30.])];
        let inner = vec![circle([1., 15.], 10.)];
        let mut work = 0;
        let path = difference(
            &outer,
            &inner,
            BoxRect {
                x: 0.,
                y: 0.,
                width: 32.,
                height: 32.,
            },
            &mut work,
            2,
        )
        .unwrap()
        .unwrap();
        assert!(
            path.segments()
                .any(|s| matches!(s, tiny_skia::PathSegment::CubicTo(..)))
        );
        let image = raster(&outer, &inner);
        assert_eq!(image.pixel(0, 15).unwrap().alpha(), 0);
        assert_eq!(image.pixel(25, 15).unwrap().alpha(), 128);
        assert_eq!(image.pixel(31, 15).unwrap().alpha(), 0);
        assert!(work > ROOT_STEPS);
    }
    #[test]
    fn invalid_curve_and_shared_work_are_refused_before_output() {
        let clip = BoxRect {
            x: 0.,
            y: 0.,
            width: 32.,
            height: 32.,
        };
        let bad = vec![vec![Segment::Cubic([
            Point([0., 0.]),
            Point([f64::NAN, 1.]),
            Point([1., 2.]),
            Point([1., 3.]),
        ])]];
        assert!(difference(&bad, &[], clip, &mut 0, 2).is_err());
        let mut work = MAX_TEXT_PAINT_WORK;
        assert!(difference(&[rectangle([0., 0., 10., 10.])], &[], clip, &mut work, 2).is_err());
    }
    #[test]
    fn nearby_source_endpoints_do_not_disable_an_entire_following_band() {
        let image = raster(
            &[rectangle([1., 0., 20., 20.])],
            &[
                rectangle([6., 5.000000005, 10., 10.]),
                rectangle([22., 5., 28., 10.]),
            ],
        );
        assert_eq!(image.pixel(7, 7).unwrap().alpha(), 0);
        assert_eq!(image.pixel(15, 7).unwrap().alpha(), 128);
    }
}
