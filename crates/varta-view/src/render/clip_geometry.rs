//! Analytic bounds for a rectangle intersected with convex rounded clips.
//! No viewport masks, text reshaping or allocations. Fixed searches reserve
//! their maximum shape-evaluation work before the first search begins.
use super::clips::Clip;
use crate::{AccessibilityError, AccessibilityWork, BoxRect};

const SEARCH_STEPS: usize = 80;
// Two axes, two ternary probes and two endpoint bisections per iteration,
// plus the endpoints/midpoint and final feasibility checks.
const SEARCH_EVALUATIONS: usize = 2 * (4 * SEARCH_STEPS + 8);
const ROUNDING_ULPS: f64 = 64.0;

impl Clip {
    pub(crate) fn visible_bounds(
        &self,
        query: BoxRect,
        work: &mut AccessibilityWork,
    ) -> Result<Option<BoxRect>, AccessibilityError> {
        let Some(mut bounds) = Bounds::new(query) else {
            return Ok(None);
        };
        let mut depth = 0usize;
        let mut next = Some(self);
        while let Some(clip) = next {
            work.charge(1)?;
            depth += 1;
            if clip.logical_bounds.is_none() {
                return Ok(None);
            }
            bounds.intersect(clip.rect);
            if bounds.empty() {
                return Ok(None);
            }
            next = clip.parent.as_deref();
        }
        // The intersection of convex shapes contains the entire rectangle iff
        // it contains all four vertices. This also covers zero-width carets.
        work.charge(depth.saturating_mul(4))?;
        if [bounds.left, bounds.right].into_iter().all(|x| {
            [bounds.top, bounds.bottom]
                .into_iter()
                .all(|y| section(self, bounds, false, x).contains(y))
        }) {
            return Ok(Some(bounds.rect()));
        }
        work.charge(depth.saturating_mul(SEARCH_EVALUATIONS))?;
        let Some((left, right)) = projection(self, bounds, false) else {
            return Ok(None);
        };
        let Some((top, bottom)) = projection(self, bounds, true) else {
            return Ok(None);
        };
        Ok(Some(
            Bounds {
                left,
                right,
                top,
                bottom,
            }
            .rect(),
        ))
    }
}
#[derive(Clone, Copy)]
struct Bounds {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}
impl Bounds {
    fn new(rect: BoxRect) -> Option<Self> {
        if ![
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.x + rect.width,
            rect.y + rect.height,
        ]
        .into_iter()
        .all(f32::is_finite)
            || rect.width < 0.0
            || rect.height < 0.0
        {
            return None;
        }
        Some(Self {
            left: rect.x.into(),
            right: (rect.x + rect.width).into(),
            top: rect.y.into(),
            bottom: (rect.y + rect.height).into(),
        })
    }
    fn intersect(&mut self, rect: BoxRect) {
        self.left = self.left.max(rect.x.into());
        self.right = self.right.min((rect.x + rect.width).into());
        self.top = self.top.max(rect.y.into());
        self.bottom = self.bottom.min((rect.y + rect.height).into());
    }
    fn empty(self) -> bool {
        self.left > self.right || self.top > self.bottom
    }
    fn rect(self) -> BoxRect {
        BoxRect {
            x: self.left as f32,
            y: self.top as f32,
            width: (self.right - self.left).max(0.0) as f32,
            height: (self.bottom - self.top).max(0.0) as f32,
        }
    }
}
#[derive(Clone, Copy)]
struct Interval {
    low: f64,
    high: f64,
}
impl Interval {
    fn gap(self) -> f64 {
        self.low - self.high
    }
    fn contains(self, value: f64) -> bool {
        value >= self.low && value <= self.high
    }
}
fn section(chain: &Clip, bounds: Bounds, transpose: bool, position: f64) -> Interval {
    let mut range = if transpose {
        Interval {
            low: bounds.left,
            high: bounds.right,
        }
    } else {
        Interval {
            low: bounds.top,
            high: bounds.bottom,
        }
    };
    let mut next = Some(chain);
    while let Some(clip) = next {
        let b = Bounds::new(clip.rect).expect("validated clip geometry");
        let (low, high, cross_low, cross_high, radii) = if transpose {
            let [tl, tr, br, bl] = clip.corners.0;
            (
                b.top,
                b.bottom,
                b.left,
                b.right,
                [tl, bl, br, tr].map(|[x, y]| [y, x]),
            )
        } else {
            (b.left, b.right, b.top, b.bottom, clip.corners.0)
        };
        for (index, [rx, ry]) in radii.into_iter().enumerate() {
            if rx == 0.0 || ry == 0.0 {
                continue;
            }
            let right = matches!(index, 1 | 2);
            let bottom = index >= 2;
            let rx = f64::from(rx);
            let ry = f64::from(ry);
            let distance = if right {
                high - position
            } else {
                position - low
            };
            if distance >= rx {
                continue;
            }
            let normalized = (distance - rx) / rx;
            let arc = ry * (1.0 - normalized * normalized).max(0.0).sqrt();
            if bottom {
                range.high = range.high.min(cross_high - ry + arc);
            } else {
                range.low = range.low.max(cross_low + ry - arc);
            }
        }
        next = clip.parent.as_deref();
    }
    range
}
fn projection(chain: &Clip, bounds: Bounds, transpose: bool) -> Option<(f64, f64)> {
    let (start, end) = if transpose {
        (bounds.top, bounds.bottom)
    } else {
        (bounds.left, bounds.right)
    };
    let tolerance = [
        bounds.left.abs(),
        bounds.right.abs(),
        bounds.top.abs(),
        bounds.bottom.abs(),
        1.0,
    ]
    .into_iter()
    .fold(0.0_f64, f64::max)
        * f64::EPSILON
        * ROUNDING_ULPS;
    let gap = |x| section(chain, bounds, transpose, x).gap();
    // The maximum lower boundary minus the minimum upper boundary is convex.
    // Locate a feasible point, then find both ends of that convex sublevel set.
    let (mut low, mut high) = (start, end);
    for _ in 0..SEARCH_STEPS {
        let first = low + (high - low) / 3.0;
        let second = high - (high - low) / 3.0;
        if gap(first) < gap(second) {
            high = second;
        } else {
            low = first;
        }
    }
    let middle = (low + high) * 0.5;
    let feasible = [start, middle, end]
        .into_iter()
        .min_by(|a, b| gap(*a).total_cmp(&gap(*b)))?;
    if gap(feasible) > tolerance {
        return None;
    }
    let (mut outside, mut inside) = (start, feasible);
    for _ in 0..SEARCH_STEPS {
        let midpoint = (outside + inside) * 0.5;
        if gap(midpoint) <= tolerance {
            inside = midpoint;
        } else {
            outside = midpoint;
        }
    }
    let left = inside;
    let (mut inside, mut outside) = (feasible, end);
    for _ in 0..SEARCH_STEPS {
        let midpoint = (outside + inside) * 0.5;
        if gap(midpoint) <= tolerance {
            inside = midpoint;
        } else {
            outside = midpoint;
        }
    }
    Some((left, inside))
}

#[cfg(test)]
#[path = "clip_geometry_tests.rs"]
mod tests;
