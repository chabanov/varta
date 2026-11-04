//! Shared bounded line/cubic clipping operations; never flatten generated arcs.
use tiny_skia::{Path, PathBuilder, PathSegment};

pub(super) const ROOT_STEPS: usize = 48;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Point(pub [f64; 2]);
impl Point {
    fn mix(self, other: Self, t: f64) -> Self {
        Self(std::array::from_fn(|axis| {
            self.0[axis] + (other.0[axis] - self.0[axis]) * t
        }))
    }
}
/// Inside means dot(normal, point) >= offset. Normalized before any products.
#[derive(Clone, Copy, Debug)]
pub(super) struct Plane {
    normal: [f64; 2],
    offset: f64,
}
impl Plane {
    pub fn new(normal: [f64; 2], offset: f64) -> Option<Self> {
        if !normal.into_iter().all(f64::is_finite) {
            return None;
        }
        let magnitude = normal[0].abs().max(normal[1].abs());
        if magnitude == 0.0 || !magnitude.is_finite() || !offset.is_finite() {
            return None;
        }
        let normal = normal.map(|axis| axis / magnitude);
        let offset = offset / magnitude;
        if !offset.is_finite() {
            return None;
        }
        Some(Self { normal, offset })
    }
    pub fn value(self, p: Point) -> f64 {
        self.normal[0] * p.0[0] + self.normal[1] * p.0[1] - self.offset
    }
    fn snap(self, mut p: Point) -> Point {
        if self.normal[1] == 0.0 {
            p.0[0] = self.offset / self.normal[0];
        } else if self.normal[0] == 0.0 {
            p.0[1] = self.offset / self.normal[1];
        } else {
            let correction =
                self.value(p) / (self.normal[0] * self.normal[0] + self.normal[1] * self.normal[1]);
            for (axis, normal) in p.0.iter_mut().zip(self.normal) {
                *axis -= correction * normal;
            }
        }
        p
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Segment {
    Line(Point, Point),
    Cubic([Point; 4]),
}
impl Segment {
    pub fn start(self) -> Point {
        match self {
            Self::Line(a, _) => a,
            Self::Cubic(p) => p[0],
        }
    }
    pub fn end(self) -> Point {
        match self {
            Self::Line(_, b) => b,
            Self::Cubic(p) => p[3],
        }
    }
    pub fn split(self, t: f64) -> (Self, Self) {
        match self {
            Self::Line(a, b) => {
                let p = a.mix(b, t);
                (Self::Line(a, p), Self::Line(p, b))
            }
            Self::Cubic([a, b, c, d]) => {
                let (ab, bc, cd) = (a.mix(b, t), b.mix(c, t), c.mix(d, t));
                let (abc, bcd) = (ab.mix(bc, t), bc.mix(cd, t));
                let p = abc.mix(bcd, t);
                (Self::Cubic([a, ab, abc, p]), Self::Cubic([p, bcd, cd, d]))
            }
        }
    }
    fn snap_end(&mut self, p: Point) {
        match self {
            Self::Line(_, b) => *b = p,
            Self::Cubic(points) => points[3] = p,
        }
    }
    fn snap_start(&mut self, p: Point) {
        match self {
            Self::Line(a, _) => *a = p,
            Self::Cubic(points) => points[0] = p,
        }
    }
    /// Caller first splits at every projected derivative root. The resulting
    /// segment is monotone for this plane and has at most one crossing.
    pub(super) fn crossing(self, plane: Plane) -> (Self, Self) {
        let increasing = plane.value(self.start()) < plane.value(self.end());
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..ROOT_STEPS {
            let middle = (low + high) * 0.5;
            if (plane.value(self.split(middle).0.end()) < 0.0) == increasing {
                low = middle;
            } else {
                high = middle;
            }
        }
        let (mut left, mut right) = self.split((low + high) * 0.5);
        let shared = plane.snap(left.end());
        left.snap_end(shared);
        right.snap_start(shared);
        (left, right)
    }
    pub fn emit(self, path: &mut PathBuilder) {
        match self {
            Self::Line(_, p) => path.line_to(p.0[0] as f32, p.0[1] as f32),
            Self::Cubic([_, b, c, d]) => path.cubic_to(
                b.0[0] as f32,
                b.0[1] as f32,
                c.0[0] as f32,
                c.0[1] as f32,
                d.0[0] as f32,
                d.0[1] as f32,
            ),
        }
    }
    pub(super) fn projected_extrema(self, plane: Plane) -> ([f64; 2], usize) {
        let Self::Cubic(p) = self else {
            return ([0.0; 2], 0);
        };
        let [p0, p1, p2, p3] = p.map(|p| plane.value(p));
        // Monotone control values suffice by the Bezier convex-hull property.
        // Generated axis-aligned quarter clips use this inexpensive path.
        if p0 <= p1 && p1 <= p2 && p2 <= p3 || p0 >= p1 && p1 >= p2 && p2 >= p3 {
            return ([0.0; 2], 0);
        }
        let coefficients = [
            -p0 + 3.0 * p1 - 3.0 * p2 + p3,
            2.0 * (p0 - 2.0 * p1 + p2),
            p1 - p0,
        ];
        let scale = coefficients.into_iter().map(f64::abs).fold(0.0, f64::max);
        if scale == 0.0 {
            return ([0.0; 2], 0);
        }
        let [a, b, c] = coefficients.map(|value| value / scale);
        let mut roots = [0.0; 2];
        let mut count = 0;
        let mut add = |root: f64| {
            if root > 0.0 && root < 1.0 && root.is_finite() && (count == 0 || roots[0] != root) {
                roots[count] = root;
                count += 1;
            }
        };
        if a == 0.0 {
            if b != 0.0 {
                add(-c / b);
            }
        } else {
            let discriminant = b * b - 4.0 * a * c;
            if discriminant >= 0.0 {
                let q = -0.5 * (b + discriminant.sqrt().copysign(b));
                if q != 0.0 {
                    add(q / a);
                    add(c / q);
                } else {
                    add(-b / (2.0 * a));
                }
            }
        }
        if count == 2 && roots[0] > roots[1] {
            roots.swap(0, 1);
        }
        (roots, count)
    }
}
fn connect(output: &mut Vec<Segment>, segment: Segment) {
    if let Some(previous) = output.last()
        && previous.end() != segment.start()
    {
        output.push(Segment::Line(previous.end(), segment.start()));
    }
    output.push(segment);
}
fn admit(output: &mut Vec<Segment>, segment: Segment, plane: Plane) {
    match (
        plane.value(segment.start()) >= 0.0,
        plane.value(segment.end()) >= 0.0,
    ) {
        (true, true) => connect(output, segment),
        (true, false) => connect(output, segment.crossing(plane).0),
        (false, true) => connect(output, segment.crossing(plane).1),
        (false, false) => {}
    }
}
pub(super) fn half_plane(input: Vec<Segment>, plane: Plane) -> Vec<Segment> {
    let mut output = Vec::with_capacity(input.len() + 1);
    for segment in input {
        let (extrema, count) = segment.projected_extrema(plane);
        let mut remainder = segment;
        let mut previous = 0.0;
        for t in extrema.into_iter().take(count) {
            let (first, last) = remainder.split((t - previous) / (1.0 - previous));
            admit(&mut output, first, plane);
            remainder = last;
            previous = t;
        }
        admit(&mut output, remainder, plane);
    }
    if let (Some(first), Some(last)) = (output.first(), output.last())
        && first.start() != last.end()
    {
        output.push(Segment::Line(last.end(), first.start()));
    }
    output
}
/// Closed generated line/cubic contours, including a separate inner hole.
/// Each component is clipped independently; callers preserve their fill rule.
/// Oversized inset arcs can backtrack outside the padding rectangle; boundary
/// connectors retain their winding while the half-planes remove those tails.
pub(super) fn contours(path: &Path) -> Option<Vec<Vec<Segment>>> {
    let point = |p: tiny_skia::Point| Point([f64::from(p.x), f64::from(p.y)]);
    let mut result = Vec::new();
    let mut segments = Vec::new();
    let mut current = None;
    let mut first = None;
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                if first.is_some() {
                    return None;
                }
                current = Some(point(p));
                first = current;
            }
            PathSegment::LineTo(p) => {
                let p = point(p);
                segments.push(Segment::Line(current?, p));
                current = Some(p);
            }
            PathSegment::CubicTo(b, c, d) => {
                let d = point(d);
                segments.push(Segment::Cubic([current?, point(b), point(c), d]));
                current = Some(d);
            }
            PathSegment::Close => {
                if current != first {
                    segments.push(Segment::Line(current?, first?));
                }
                result.push(std::mem::take(&mut segments));
                current = None;
                first = None;
            }
            PathSegment::QuadTo(_, _) => return None,
        }
    }
    if first.is_some() {
        return None;
    }
    Some(result)
}

#[cfg(test)]
#[path = "clip_curve_tests.rs"]
mod tests;
