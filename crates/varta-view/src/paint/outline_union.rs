//! Bounded orthogonal union contours. Keep source coordinates in f64 until paint.
use super::clip_curve::Point;
use super::{BoxRect, MAX_TEXT_PAINT_WORK, PaintError};
use std::collections::BTreeMap;
use tiny_skia::{Path, PathBuilder};

const EDGE_WORK: usize = 8;
const EVENT_WORK: usize = 4;
const DIRECTION_COUNT: usize = 4;
type Interval = (f64, f64);
type Key = (u64, u64);

pub(super) fn charge(work: &mut usize, amount: usize, node: u32) -> Result<(), PaintError> {
    *work = work.saturating_add(amount);
    if *work > MAX_TEXT_PAINT_WORK {
        Err(PaintError::Unsupported(node))
    } else {
        Ok(())
    }
}
fn sort_work(count: usize) -> usize {
    count.saturating_mul(if count > 1 {
        count.ilog2() as usize + 1
    } else {
        1
    })
}
fn key(p: Point) -> Key {
    let bits = |x: f64| if x == 0.0 { 0 } else { x.to_bits() };
    (bits(p.0[0]), bits(p.0[1]))
}
#[derive(Clone, Copy)]
enum Direction {
    East,
    South,
    West,
    North,
}
impl Direction {
    fn index(self) -> usize {
        self as usize
    }
    fn rank(self, next: Self) -> usize {
        // Follow the filled region on the right, including point contacts.
        match (next.index() + DIRECTION_COUNT - self.index()) % DIRECTION_COUNT {
            1 => 0,
            0 => 1,
            3 => 2,
            _ => 3,
        }
    }
}
#[derive(Clone, Copy)]
struct Edge {
    start: Point,
    end: Point,
    direction: Direction,
}
type Edges = BTreeMap<Key, [Option<Edge>; DIRECTION_COUNT]>;
fn edge(
    edges: &mut Edges,
    start: Point,
    end: Point,
    work: &mut usize,
    node: u32,
) -> Result<(), PaintError> {
    if start == end {
        return Ok(());
    }
    charge(work, EDGE_WORK, node)?;
    let direction = if start.0[1] == end.0[1] {
        if start.0[0] < end.0[0] {
            Direction::East
        } else {
            Direction::West
        }
    } else if start.0[0] == end.0[0] {
        if start.0[1] < end.0[1] {
            Direction::South
        } else {
            Direction::North
        }
    } else {
        return Err(PaintError::Unsupported(node));
    };
    let slot = &mut edges.entry(key(start)).or_insert([None; DIRECTION_COUNT])[direction.index()];
    if slot.is_some() {
        return Err(PaintError::Unsupported(node));
    }
    *slot = Some(Edge {
        start,
        end,
        direction,
    });
    Ok(())
}
fn take(
    edges: &mut Edges,
    at: Key,
    incoming: Option<Direction>,
    node: u32,
) -> Result<Edge, PaintError> {
    let choices = edges.get_mut(&at).ok_or(PaintError::Unsupported(node))?;
    let index = choices
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.map(|e| (i, incoming.map_or(i, |d| d.rank(e.direction)))))
        .min_by_key(|(_, rank)| *rank)
        .map(|(i, _)| i)
        .ok_or(PaintError::Unsupported(node))?;
    let result = choices[index].take().ok_or(PaintError::Unsupported(node))?;
    if choices.iter().all(Option::is_none) {
        edges.remove(&at);
    }
    Ok(result)
}
fn difference(
    a: &[Interval],
    b: &[Interval],
    work: &mut usize,
    node: u32,
) -> Result<Vec<Interval>, PaintError> {
    let mut result = Vec::new();
    let mut j = 0;
    for &(mut left, right) in a {
        charge(work, 1, node)?;
        while j < b.len() && b[j].1 <= left {
            charge(work, 1, node)?;
            j += 1;
        }
        let mut k = j;
        while k < b.len() && b[k].0 < right {
            charge(work, 1, node)?;
            if b[k].0 > left {
                result.push((left, right.min(b[k].0)));
            }
            left = left.max(b[k].1);
            if left >= right {
                break;
            }
            k += 1;
        }
        if left < right {
            result.push((left, right));
        }
    }
    Ok(result)
}
fn straight(a: Point, b: Point, c: Point) -> bool {
    a.0[0] == b.0[0] && b.0[0] == c.0[0] || a.0[1] == b.0[1] && b.0[1] == c.0[1]
}
fn simplify(points: Vec<Point>) -> Vec<Point> {
    let mut output = Vec::with_capacity(points.len());
    for p in points {
        while output.len() >= 2 && straight(output[output.len() - 2], output[output.len() - 1], p) {
            output.pop();
        }
        output.push(p);
    }
    while output.len() > 3
        && straight(
            output[output.len() - 2],
            output[output.len() - 1],
            output[0],
        )
    {
        output.pop();
    }
    if output.len() > 3 && straight(output[output.len() - 1], output[0], output[1]) {
        output.remove(0);
    }
    output
}
/// CW exterior and CCW hole contours of half-open rectangles [left,top,right,bottom].
/// Degenerate rectangles carry no area. Invalid input/refusal has no paint effect.
pub(super) fn boundary(
    rects: &[[f64; 4]],
    work: &mut usize,
    node: u32,
) -> Result<Vec<Vec<Point>>, PaintError> {
    charge(
        work,
        rects
            .len()
            .saturating_mul(EVENT_WORK)
            .saturating_add(sort_work(rects.len().saturating_mul(2))),
        node,
    )?;
    let mut ys = Vec::with_capacity(rects.len().saturating_mul(2));
    for &[left, top, right, bottom] in rects {
        if ![left, top, right, bottom].into_iter().all(f64::is_finite)
            || left > right
            || top > bottom
        {
            return Err(PaintError::Unsupported(node));
        }
        if left < right && top < bottom {
            ys.extend([top, bottom]);
        }
    }
    ys.sort_unstable_by(f64::total_cmp);
    ys.dedup();
    let mut edges = Edges::new();
    let mut previous = Vec::<Interval>::new();
    let mut active = Vec::<Interval>::new();
    for band in ys.windows(2) {
        let [top, bottom] = [band[0], band[1]];
        active.clear();
        for &[left, y0, right, y1] in rects {
            charge(work, 1, node)?;
            if left < right && y0 <= top && y1 >= bottom {
                active.push((left, right));
            }
        }
        charge(work, sort_work(active.len()), node)?;
        active.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
        let mut kept = 0;
        for index in 0..active.len() {
            let (left, right) = active[index];
            if kept > 0 && left <= active[kept - 1].1 {
                active[kept - 1].1 = active[kept - 1].1.max(right);
            } else {
                active[kept] = (left, right);
                kept += 1;
            }
        }
        active.truncate(kept);
        for (left, right) in difference(&active, &previous, work, node)? {
            edge(
                &mut edges,
                Point([left, top]),
                Point([right, top]),
                work,
                node,
            )?;
        }
        for (left, right) in difference(&previous, &active, work, node)? {
            edge(
                &mut edges,
                Point([right, top]),
                Point([left, top]),
                work,
                node,
            )?;
        }
        for &(left, right) in &active {
            edge(
                &mut edges,
                Point([left, bottom]),
                Point([left, top]),
                work,
                node,
            )?;
            edge(
                &mut edges,
                Point([right, top]),
                Point([right, bottom]),
                work,
                node,
            )?;
        }
        std::mem::swap(&mut previous, &mut active);
    }
    if let Some(&bottom) = ys.last() {
        for (left, right) in previous {
            edge(
                &mut edges,
                Point([right, bottom]),
                Point([left, bottom]),
                work,
                node,
            )?;
        }
    }
    let mut contours = Vec::new();
    while let Some((&start, _)) = edges.first_key_value() {
        let mut current = take(&mut edges, start, None, node)?;
        let mut points = vec![current.start];
        loop {
            charge(work, 1, node)?;
            if key(current.end) == start {
                break;
            }
            points.push(current.end);
            current = take(&mut edges, key(current.end), Some(current.direction), node)?;
        }
        let points = simplify(points);
        if points.len() < DIRECTION_COUNT {
            return Err(PaintError::Unsupported(node));
        }
        contours.push(points);
    }
    Ok(contours)
}

fn inward(a: Point, b: Point) -> [f64; 2] {
    if a.0[0] == b.0[0] {
        [if a.0[1] < b.0[1] { -1.0 } else { 1.0 }, 0.0]
    } else {
        [0.0, if a.0[0] < b.0[0] { 1.0 } else { -1.0 }]
    }
}
pub(super) fn inset(
    contours: &[Vec<Point>],
    width: f64,
    work: &mut usize,
    node: u32,
) -> Result<Vec<Vec<Point>>, PaintError> {
    let mut result = Vec::new();
    for contour in contours {
        charge(work, contour.len().saturating_mul(EVENT_WORK), node)?;
        let mut next = Vec::with_capacity(contour.len());
        for index in 0..contour.len() {
            let a = contour[(index + contour.len() - 1) % contour.len()];
            let b = contour[index];
            let c = contour[(index + 1) % contour.len()];
            let (u, v) = (inward(a, b), inward(b, c));
            next.push(Point(std::array::from_fn(|axis| {
                b.0[axis] + (u[axis] + v[axis]) * width
            })));
        }
        result.push(next);
    }
    Ok(result)
}
fn occupied(
    contours: &[Vec<Point>],
    top: f64,
    bottom: f64,
    work: &mut usize,
    node: u32,
) -> Result<Vec<Interval>, PaintError> {
    let mut events = Vec::new();
    for contour in contours {
        for (a, b) in contour
            .iter()
            .zip(contour.iter().cycle().skip(1))
            .take(contour.len())
        {
            charge(work, 1, node)?;
            if a.0[0] == b.0[0]
                && a.0[1] != b.0[1]
                && a.0[1].min(b.0[1]) <= top
                && a.0[1].max(b.0[1]) >= bottom
            {
                events.push((a.0[0], if a.0[1] > b.0[1] { 1 } else { -1 }));
            }
        }
    }
    charge(work, sort_work(events.len()), node)?;
    events.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let mut intervals = Vec::new();
    let mut winding = 0;
    let mut left = None;
    let mut at = 0;
    while at < events.len() {
        let x = events[at].0;
        let before = winding;
        while at < events.len() && events[at].0 == x {
            winding += events[at].1;
            at += 1;
        }
        if before != 1 && winding == 1 {
            left = Some(x);
        }
        if before == 1 && winding != 1 {
            let left = left.take().ok_or(PaintError::Unsupported(node))?;
            if left < x {
                intervals.push((left, x));
            }
        }
    }
    if winding != 0 {
        return Err(PaintError::Unsupported(node));
    }
    Ok(intervals)
}
pub(super) struct Shape {
    pub outer: Vec<Vec<Point>>,
    pub inner: Vec<Vec<Point>>,
}
pub(super) fn shape(
    rects: &[[f64; 4]],
    width: f64,
    offset: f64,
    work: &mut usize,
    node: u32,
) -> Result<Shape, PaintError> {
    if !width.is_finite() || width < 0.0 || !offset.is_finite() {
        return Err(PaintError::Unsupported(node));
    }
    charge(work, rects.len().saturating_mul(EVENT_WORK), node)?;
    let mut expanded = Vec::with_capacity(rects.len());
    for &[left, top, right, bottom] in rects {
        if ![left, top, right, bottom].into_iter().all(f64::is_finite)
            || left > right
            || top > bottom
        {
            return Err(PaintError::Unsupported(node));
        }
        let x = offset.max(-(right - left) * 0.5) + width;
        let y = offset.max(-(bottom - top) * 0.5) + width;
        expanded.push([left - x, top - y, right + x, bottom + y]);
    }
    let outer = boundary(&expanded, work, node)?;
    let inner = inset(&outer, width, work, node)?;
    Ok(Shape { outer, inner })
}
/// Ring around the union, with inner corners derived from the *outer boundary*.
/// Shrinking each original fragment separately leaves incorrect internal bars
/// when the outer outlines bridge a narrow line gap. Only winding exactly +1 is
/// the cutout; inverted/folded inset pieces retain the outline's ink.
pub(super) fn ring(
    rects: &[[f64; 4]],
    width: f64,
    offset: f64,
    clip: BoxRect,
    work: &mut usize,
    node: u32,
) -> Result<Option<Path>, PaintError> {
    if !width.is_finite() || width < 0.0 || !offset.is_finite() {
        return Err(PaintError::Unsupported(node));
    }
    if width == 0.0 || clip.width <= 0.0 || clip.height <= 0.0 {
        return Ok(None);
    }
    let Shape { outer, inner } = shape(rects, width, offset, work, node)?;
    let mut ys = Vec::new();
    for c in outer.iter().chain(&inner) {
        charge(work, c.len(), node)?;
        ys.extend(c.iter().map(|p| p.0[1]));
    }
    charge(work, sort_work(ys.len()), node)?;
    ys.sort_unstable_by(f64::total_cmp);
    ys.dedup();
    let clip = [
        f64::from(clip.x),
        f64::from(clip.y),
        f64::from(clip.x) + f64::from(clip.width),
        f64::from(clip.y) + f64::from(clip.height),
    ];
    if !clip.into_iter().all(f64::is_finite) {
        return Err(PaintError::Unsupported(node));
    }
    let mut path = PathBuilder::new();
    for band in ys.windows(2) {
        let top = band[0].max(clip[1]);
        let bottom = band[1].min(clip[3]);
        if top >= bottom {
            continue;
        }
        let a = occupied(&outer, band[0], band[1], work, node)?;
        let b = occupied(&inner, band[0], band[1], work, node)?;
        for (left, right) in difference(&a, &b, work, node)? {
            let left = left.max(clip[0]);
            let right = right.min(clip[2]);
            if left >= right {
                continue;
            }
            charge(work, EDGE_WORK, node)?;
            path.move_to(left as f32, top as f32);
            path.line_to(right as f32, top as f32);
            path.line_to(right as f32, bottom as f32);
            path.line_to(left as f32, bottom as f32);
            path.close();
        }
    }
    Ok(path.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn winding(contour: &[Point], p: [f64; 2]) -> i32 {
        let mut count = 0;
        for (a, b) in contour
            .iter()
            .zip(contour.iter().cycle().skip(1))
            .take(contour.len())
        {
            if a.0[0] == b.0[0] && a.0[0] > p[0] {
                if a.0[1] <= p[1] && b.0[1] > p[1] {
                    count += 1;
                }
                if b.0[1] <= p[1] && a.0[1] > p[1] {
                    count -= 1;
                }
            }
        }
        count
    }
    fn area(p: &[Point]) -> f64 {
        p.iter()
            .zip(p.iter().cycle().skip(1))
            .take(p.len())
            .map(|(a, b)| a.0[0] * b.0[1] - a.0[1] * b.0[0])
            .sum::<f64>()
            * 0.5
    }
    #[test]
    fn adjacent_rectangles_merge_and_holes_keep_opposite_winding() {
        let p = boundary(&[[0.0, 0.0, 4.0, 4.0], [4.0, 0.0, 8.0, 4.0]], &mut 0, 2).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].len(), 4);
        assert_eq!(area(&p[0]), 32.0);
        let p = boundary(
            &[
                [0.0, 0.0, 6.0, 2.0],
                [0.0, 4.0, 6.0, 6.0],
                [0.0, 2.0, 2.0, 4.0],
                [4.0, 2.0, 6.0, 4.0],
            ],
            &mut 0,
            2,
        )
        .unwrap();
        assert_eq!(p.len(), 2);
        let mut areas = p.iter().map(|p| area(p)).collect::<Vec<_>>();
        areas.sort_by(f64::total_cmp);
        assert_eq!(areas, [-4.0, 36.0]);
    }
    #[test]
    fn point_contacts_do_not_join_disconnected_contours() {
        let p = boundary(&[[0.0, 0.0, 2.0, 2.0], [2.0, 2.0, 4.0, 4.0]], &mut 0, 2).unwrap();
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|p| p.len() == 4 && area(p) == 4.0));
    }
    #[test]
    fn unions_match_independent_cell_occupancy_for_generated_rectangles() {
        let mut seed = 0x4167_8331_u32;
        for _ in 0..256 {
            let mut rects = Vec::new();
            let mut next = || {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                f64::from((seed >> 16) % 12)
            };
            for _ in 0..20 {
                let (x, y, u, v) = (next(), next(), next(), next());
                rects.push([x.min(u), y.min(v), x.max(u), y.max(v)]);
            }
            let contours = boundary(&rects, &mut 0, 2).unwrap();
            for y in 0..12 {
                for x in 0..12 {
                    let p = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                    let expected = rects
                        .iter()
                        .any(|&[a, b, c, d]| p[0] >= a && p[0] < c && p[1] >= b && p[1] < d);
                    assert_eq!(
                        contours.iter().map(|c| winding(c, p)).sum::<i32>() > 0,
                        expected,
                        "{rects:?} at {p:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn invalid_coordinates_and_shared_budget_refuse_without_retaining_state() {
        for bad in [
            [0.0, f64::NAN, 1.0, 2.0],
            [2.0, 0.0, 1.0, 2.0],
            [0.0, 0.0, f64::INFINITY, 2.0],
        ] {
            assert!(boundary(&[bad], &mut 0, 2).is_err());
        }
        let rect = [[0.0, 0.0, 2.0, 2.0]];
        let mut exhausted = MAX_TEXT_PAINT_WORK;
        assert!(boundary(&rect, &mut exhausted, 2).is_err());
        assert_eq!(boundary(&rect, &mut 0, 2).unwrap().len(), 1);
        let n = 1e12;
        let p = boundary(&[[n, n, n + 1.0, n + 2.0]], &mut 0, 2).unwrap();
        assert!(p[0].iter().any(|p| p.0[0] == n + 1.0));
    }
    fn raster(rects: &[[f64; 4]], width: f64, offset: f64) -> tiny_skia::Pixmap {
        let mut pixmap = tiny_skia::Pixmap::new(64, 64).unwrap();
        let path = ring(
            rects,
            width,
            offset,
            BoxRect {
                x: 0.0,
                y: 0.0,
                width: 64.0,
                height: 64.0,
            },
            &mut 0,
            2,
        )
        .unwrap()
        .unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(255, 0, 0, 128);
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
        pixmap
    }
    #[test]
    fn union_boundary_inset_bridges_small_gaps_without_internal_bars_or_double_alpha() {
        let image = raster(&[[2.0, 2.0, 8.0, 6.0], [2.0, 9.0, 8.0, 13.0]], 2.0, 0.0);
        assert_eq!(image.pixel(5, 7).unwrap().alpha(), 0);
        for y in 0..15 {
            assert_eq!(image.pixel(1, y).unwrap().alpha(), 128);
        }
        let collapsed = raster(&[[12.0, 12.0, 36.0, 36.0]], 2.0, -100.0);
        assert_eq!(collapsed.pixel(22, 22).unwrap().alpha(), 128);
        assert_eq!(collapsed.pixel(25, 25).unwrap().alpha(), 128);
        assert_eq!(collapsed.pixel(21, 22).unwrap().alpha(), 0);
    }
    #[test]
    fn collapsed_inner_crossings_never_paint_outside_the_outer_union() {
        let source = [
            [5.0, 5.0, 10.0, 10.0],
            [10.0, 7.0, 15.0, 8.0],
            [15.0, 5.0, 20.0, 10.0],
        ];
        let image = raster(&source, 2.0, -1.0);
        for y in 0..24 {
            for x in 0..24 {
                let p = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let outside = source.iter().all(|&[a, b, c, d]| {
                    let dx = (-1.0_f64).max(-(c - a) * 0.5) + 2.0;
                    let dy = (-1.0_f64).max(-(d - b) * 0.5) + 2.0;
                    p[0] + 0.5 <= a - dx
                        || p[0] - 0.5 >= c + dx
                        || p[1] + 0.5 <= b - dy
                        || p[1] - 0.5 >= d + dy
                });
                if outside {
                    assert_eq!(image.pixel(x, y).unwrap().alpha(), 0, "outside at {p:?}");
                }
            }
        }
    }

    #[test]
    fn collapsed_offsets_can_put_inner_winding_outside_the_outer_region() {
        // A simple combined path would paint this outside point. The curved
        // region boolean must retain a separate outer membership condition.
        let rects = [
            [8., 2., 14., 16.],
            [12., 0., 13., 7.],
            [0., 9., 18., 18.],
            [3., 2., 19., 13.],
            [8., 0., 11., 11.],
            [1., 0., 3., 13.],
            [3., 6., 5., 12.],
            [2., 3., 16., 13.],
        ];
        let width = 2.5;
        let offset = -8.0_f64;
        let expanded = rects
            .iter()
            .map(|&[a, b, c, d]| {
                let x = offset.max(-(c - a) * 0.5) + width;
                let y = offset.max(-(d - b) * 0.5) + width;
                [a - x, b - y, c + x, d + y]
            })
            .collect::<Vec<_>>();
        let outer = boundary(&expanded, &mut 0, 2).unwrap();
        let inner = inset(&outer, width, &mut 0, 2).unwrap();
        let p = [4.25, 12.25];
        assert_eq!(outer.iter().map(|c| winding(c, p)).sum::<i32>(), 0);
        assert_eq!(inner.iter().map(|c| winding(c, p)).sum::<i32>(), -1);
    }
}
