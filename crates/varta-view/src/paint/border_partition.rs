//! Color ownership on the nested CSS padding-edge contours. A point belongs
//! to the first inward-moving edge/arc that reaches it. Unlike whole-ring
//! nearest-edge assignment, a large inset corner keeps its adjacent colors.
//! The transition angle is monotone in adjacent widths (CSS Backgrounds3§4.4).
use super::{BoxRect, Corners};

const TOP: usize = 0;
const RIGHT: usize = 1;
const BOTTOM: usize = 2;
const LEFT: usize = 3;
const SIDES: usize = 4;
const DEPTH_STEPS: usize = 48;

#[derive(Clone, Copy, PartialEq)]
struct Corner {
    center: [f64; 2],
    radius: [f64; 2],
    widths: [f64; 2],
    direction: [f64; 2],
    horizontal: usize,
    vertical: usize,
    split: [f64; 2],
}
#[derive(Clone, PartialEq)]
pub(super) struct Partition {
    edges: [f64; SIDES],
    widths: [f64; SIDES],
    corners: [Corner; SIDES],
}
#[derive(Clone, Copy)]
struct Active<'a> {
    corner: &'a Corner,
    delta: [f64; 2],
}
impl Active<'_> {
    fn arrival(self, limit: f64) -> f64 {
        if self.value(0.0) >= 1.0 {
            return 0.0;
        }
        let mut high = limit;
        for axis in 0..2 {
            if self.corner.widths[axis] > 0.0 {
                high = high.min(self.corner.radius[axis] / self.corner.widths[axis]);
            }
        }
        if self.value(high) <= 1.0 {
            return f64::INFINITY;
        }
        let mut low = 0.0;
        for _ in 0..DEPTH_STEPS {
            let mid = (low + high) * 0.5;
            if self.value(mid) > 1.0 {
                high = mid;
            } else {
                low = mid;
            }
        }
        high
    }
    fn value(self, depth: f64) -> f64 {
        let x = self.delta[0] / (self.corner.radius[0] - depth * self.corner.widths[0]);
        let y = self.delta[1] / (self.corner.radius[1] - depth * self.corner.widths[1]);
        x * x + y * y
    }
    fn owner(self, depth: f64) -> usize {
        let c = self.corner;
        let [dx, dy] = self.delta;
        let [sin, cos] = c.split;
        if dx * (c.radius[1] - depth * c.widths[1]) * cos
            <= dy * (c.radius[0] - depth * c.widths[0]) * sin
        {
            c.horizontal
        } else {
            c.vertical
        }
    }
    fn owner_before(self, limit: f64) -> usize {
        let c = self.corner;
        if c.widths[0] == 0.0 {
            return c.horizontal;
        }
        if c.widths[1] == 0.0 {
            return c.vertical;
        }
        // At the arc's arrival depth, the angle comparison is linear in depth.
        // If its sign changes, test the unique zero against the monotone ellipse
        // equation instead of solving a quartic for every ordinary corner pixel.
        let [dx, dy] = self.delta;
        let [sin, cos] = c.split;
        let a = dx * c.radius[1] * cos - dy * c.radius[0] * sin;
        let b = dx * c.widths[1] * cos - dy * c.widths[0] * sin;
        let end = a - b * limit;
        let horizontal = if a <= 0.0 && end <= 0.0 {
            true
        } else if a >= 0.0 && end >= 0.0 {
            false
        } else {
            let crossing = a / b;
            if b > 0.0 {
                self.value(crossing) <= 1.0
            } else {
                self.value(crossing) >= 1.0
            }
        };
        if horizontal { c.horizontal } else { c.vertical }
    }
}
impl Partition {
    /// Prove a whole color block has one owner by interval bounds. Uncertain
    /// blocks use point sampling; no corner/edge is dropped as an optimization.
    pub fn uniform(&self, x: f64, y: f64, w: f64, h: f64) -> Option<usize> {
        let distances = [
            [y - self.edges[TOP], y + h - self.edges[TOP]],
            [self.edges[RIGHT] - x - w, self.edges[RIGHT] - x],
            [self.edges[BOTTOM] - y - h, self.edges[BOTTOM] - y],
            [x - self.edges[LEFT], x + w - self.edges[LEFT]],
        ];
        let times: [[f64; 2]; SIDES] = std::array::from_fn(|i| {
            if self.widths[i] == 0.0 {
                [f64::INFINITY; 2]
            } else {
                distances[i].map(|v| v / self.widths[i])
            }
        });
        let low = times
            .into_iter()
            .map(|r| r[0])
            .fold(f64::INFINITY, f64::min);
        let high = times
            .into_iter()
            .map(|r| r[1])
            .fold(f64::INFINITY, f64::min);
        if low < 0.0 || !high.is_finite() {
            return None;
        }
        let mut potential = [None; SIDES];
        let mut count = 0;
        for corner in &self.corners {
            if corner.radius.contains(&0.0) || corner.widths == [0.0; 2] {
                continue;
            }
            let a = [
                (corner.center[0] - x) * corner.direction[0],
                (corner.center[1] - y) * corner.direction[1],
            ];
            let b = [
                (corner.center[0] - x - w) * corner.direction[0],
                (corner.center[1] - y - h) * corner.direction[1],
            ];
            let min = [a[0].min(b[0]), a[1].min(b[1])];
            let max = [a[0].max(b[0]), a[1].max(b[1])];
            if max.into_iter().any(|v| v <= 0.0) {
                continue;
            }
            let within = corner
                .radius
                .into_iter()
                .zip(corner.widths)
                .all(|(r, w)| r - high * w > 0.0)
                && (Active { corner, delta: max }).value(high) <= 1.0;
            if within {
                continue;
            }
            potential[count] = Some((corner, min, max));
            count += 1;
        }
        if count > 1 {
            for index in 0..count {
                let (corner, min, _) = potential[index].expect("candidate exists");
                if min.into_iter().any(|v| v <= 0.0) {
                    continue;
                }
                let upper = (Active { corner, delta: min }).arrival(high);
                if !upper.is_finite() || upper >= low {
                    continue;
                }
                let mut first = true;
                for (other_index, other) in potential[..count].iter().enumerate() {
                    if other_index == index {
                        continue;
                    }
                    let (corner, _, max) = other.expect("candidate exists");
                    if upper >= (Active { corner, delta: max }).arrival(high) {
                        first = false;
                        break;
                    }
                }
                if first {
                    return Self::corner_color(corner, x, y, w, h, upper);
                }
            }
            return None;
        }
        if let Some((corner, min, _)) = potential[0] {
            if min.into_iter().any(|v| v <= 0.0)
                || (Active { corner, delta: min }).value(low) <= 1.0
            {
                return None;
            }
            return Self::corner_color(corner, x, y, w, h, high);
        }
        (0..SIDES).find(|i| {
            self.widths[*i] > 0.0 && (0..SIDES).all(|j| i == &j || times[*i][1] < times[j][0])
        })
    }
    fn corner_color(corner: &Corner, x: f64, y: f64, w: f64, h: f64, limit: f64) -> Option<usize> {
        if corner.widths[0] == 0.0 {
            return Some(corner.horizontal);
        }
        if corner.widths[1] == 0.0 {
            return Some(corner.vertical);
        }
        let points = [[x, y], [x + w, y], [x, y + h], [x + w, y + h]];
        let active = |x, y| Active {
            corner,
            delta: [
                (corner.center[0] - x) * corner.direction[0],
                (corner.center[1] - y) * corner.direction[1],
            ],
        };
        let expected = active(x, y).owner(0.0);
        points
            .into_iter()
            .all(|[x, y]| {
                let c = active(x, y);
                c.owner(0.0) == expected && c.owner(limit) == expected
            })
            .then_some(expected)
    }
    fn contains(&self, depth: f64, x: f64, y: f64) -> bool {
        let [top, right, bottom, left] = std::array::from_fn(|i| {
            self.edges[i] + depth * self.widths[i] * if i == TOP || i == LEFT { 1.0 } else { -1.0 }
        });
        if x < left || x >= right || y < top || y >= bottom {
            return false;
        }
        for corner in &self.corners {
            if corner
                .radius
                .into_iter()
                .zip(corner.widths)
                .any(|(radius, width)| radius - depth * width <= 0.0)
            {
                continue;
            }
            let delta = [
                (corner.center[0] - x) * corner.direction[0],
                (corner.center[1] - y) * corner.direction[1],
            ];
            if delta.into_iter().all(|v| v > 0.0) && (Active { corner, delta }).value(depth) > 1.0 {
                return false;
            }
        }
        true
    }
    pub fn in_ring(&self, x: f64, y: f64) -> bool {
        self.contains(0.0, x, y) && !self.contains(1.0, x, y)
    }
    pub fn new(bounds: BoxRect, widths: [f32; SIDES], radii: Corners) -> Self {
        let left = f64::from(bounds.x);
        let top = f64::from(bounds.y);
        let right = left + f64::from(bounds.width);
        let bottom = top + f64::from(bounds.height);
        let widths = widths.map(f64::from);
        let corners = std::array::from_fn(|index| {
            let horizontal = if index < 2 { TOP } else { BOTTOM };
            let vertical = if index == 0 || index == 3 {
                LEFT
            } else {
                RIGHT
            };
            let direction = [
                if vertical == LEFT { 1.0 } else { -1.0 },
                if horizontal == TOP { 1.0 } else { -1.0 },
            ];
            let radius = radii.0[index].map(f64::from);
            let center = [
                (if vertical == LEFT { left } else { right }) + direction[0] * radius[0],
                (if horizontal == TOP { top } else { bottom }) + direction[1] * radius[1],
            ];
            let total = widths[horizontal] + widths[vertical];
            let angle = if total == 0.0 {
                0.0
            } else {
                std::f64::consts::FRAC_PI_2 * widths[horizontal] / total
            };
            let (sin, cos) = angle.sin_cos();
            Corner {
                center,
                radius,
                direction,
                horizontal,
                vertical,
                widths: [widths[vertical], widths[horizontal]],
                // Exact endpoint ownership must not depend on cos(pi/2)'s tail.
                split: if widths[vertical] == 0.0 {
                    [1.0, 0.0]
                } else if widths[horizontal] == 0.0 {
                    [0.0, 1.0]
                } else {
                    [sin, cos]
                },
            }
        });
        Self {
            edges: [top, right, bottom, left],
            widths,
            corners,
        }
    }
    pub fn owner(&self, x: f64, y: f64) -> usize {
        let distances = [
            y - self.edges[TOP],
            self.edges[RIGHT] - x,
            self.edges[BOTTOM] - y,
            x - self.edges[LEFT],
        ];
        let mut side = TOP;
        let mut limit = f64::INFINITY;
        for (index, (distance, width)) in distances.into_iter().zip(self.widths).enumerate() {
            if width > 0.0 && distance / width < limit {
                limit = distance / width;
                side = index;
            }
        }
        if limit <= 0.0 {
            return side;
        }
        let mut candidates = [None; SIDES];
        let mut count = 0;
        for corner in &self.corners {
            if corner.radius.contains(&0.0) || corner.widths == [0.0; 2] {
                continue;
            }
            let delta = [
                (corner.center[0] - x) * corner.direction[0],
                (corner.center[1] - y) * corner.direction[1],
            ];
            if delta.into_iter().any(|axis| axis <= 0.0) {
                continue;
            }
            let active = Active { corner, delta };
            if active.value(0.0) >= 1.0 {
                return active.owner(0.0);
            }
            if active.value(limit) > 1.0 {
                candidates[count] = Some(active);
                count += 1;
            }
        }
        if count == 0 {
            return side;
        }
        let first = candidates[0].expect("nonempty candidates");
        if count == 1 {
            return first.owner_before(limit);
        }
        // Only opposing corner quadrants can overlap after CSS normalization.
        // Their arrival order is determined under one bounded monotone search;
        // the ordinary one-corner path above performs no iterative solve.
        let mut low = 0.0;
        let mut high = limit;
        for _ in 0..DEPTH_STEPS {
            let mid = (low + high) * 0.5;
            let mut index = 0;
            let mut outside = false;
            while index < count {
                if candidates[index].expect("candidate exists").value(mid) > 1.0 {
                    outside = true;
                    break;
                }
                index += 1;
            }
            if outside {
                high = mid;
            } else {
                low = mid;
            }
        }
        let mut winner = first;
        for candidate in candidates[..count].iter().flatten().skip(1) {
            if candidate.value(high) > winner.value(high) {
                winner = *candidate;
            }
        }
        winner.owner(high)
    }
}

#[cfg(test)]
#[path = "border_partition_tests.rs"]
mod tests;
