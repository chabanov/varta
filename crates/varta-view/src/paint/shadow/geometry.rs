//! Rounded shadow geometry stays in f64, including wide inline strips.
use super::{BoxRect, Corners};
pub(super) const HALF: f64 = 0.5;
pub(super) const GAUSSIAN_LIMIT: f64 = 4.0;
const INV_SQRT_TWO: f64 = std::f64::consts::FRAC_1_SQRT_2;
const INV_SQRT_TAU: f64 = 0.398_942_280_401_432_7;
pub(super) const CHANNEL_MAX: f64 = 255.0;
// Positive nodes and weights of the 16-point Gauss-Legendre rule.
const QUADRATURE: [(f64, f64); 8] = [
    (0.095_012_509_837_637_44, 0.189_450_610_455_068_5),
    (0.281_603_550_779_258_9, 0.182_603_415_044_923_6),
    (0.458_016_777_657_227_4, 0.169_156_519_395_002_54),
    (0.617_876_244_402_643_8, 0.149_595_988_816_576_74),
    (0.755_404_408_355_003, 0.124_628_971_255_533_88),
    (0.865_631_202_387_831_8, 0.095_158_511_682_492_79),
    (0.944_575_023_073_232_6, 0.062_253_523_938_647_89),
    (0.989_400_934_991_649_9, 0.027_152_459_411_754_096),
];

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Shape {
    pub rect: Bounds,
    pub corners: Radii,
}
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl From<BoxRect> for Bounds {
    fn from(r: BoxRect) -> Self {
        Self {
            x: f64::from(r.x),
            y: f64::from(r.y),
            width: f64::from(r.width),
            height: f64::from(r.height),
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Radii(pub [[f64; 2]; 4]);
impl From<Corners> for Radii {
    fn from(c: Corners) -> Self {
        Self(c.0.map(|r| r.map(f64::from)))
    }
}
impl Radii {
    pub fn is_square(self) -> bool {
        self.0.iter().all(|r| r[0] == 0.0 || r[1] == 0.0)
    }
    fn contains(self, r: Bounds, x: f64, y: f64) -> bool {
        if x < r.x || y < r.y || x >= r.x + r.width || y >= r.y + r.height {
            return false;
        }
        for (i, [rx, ry]) in self.0.into_iter().enumerate() {
            if rx <= 0.0 || ry <= 0.0 {
                continue;
            }
            let dx = if i == 0 || i == 3 {
                x - r.x
            } else {
                r.x + r.width - x
            };
            let dy = if i < 2 { y - r.y } else { r.y + r.height - y };
            if dx < rx && dy < ry && ((dx - rx) / rx).powi(2) + ((dy - ry) / ry).powi(2) > 1.0 {
                return false;
            }
        }
        true
    }
}
impl Shape {
    pub fn inset(self, widths: [f64; 4]) -> Self {
        let [top, right, bottom, left] = widths;
        let r = self.rect;
        let sides = [[left, top], [right, top], [right, bottom], [left, bottom]];
        Self {
            rect: Bounds {
                x: r.x + left,
                y: r.y + top,
                width: (r.width - left - right).max(0.0),
                height: (r.height - top - bottom).max(0.0),
            },
            corners: Radii(core::array::from_fn(|i| {
                core::array::from_fn(|j| (self.corners.0[i][j] - sides[i][j]).max(0.0))
            })),
        }
    }
    pub fn contains(self, x: f64, y: f64) -> bool {
        self.corners.contains(self.rect, x, y)
    }
    fn x_range(self, y: f64) -> (f64, f64) {
        let r = self.rect;
        let mut left = r.x;
        let mut right = r.x + r.width;
        for (index, [rx, ry]) in self.corners.0.into_iter().enumerate() {
            if rx <= 0.0 || ry <= 0.0 {
                continue;
            }
            let (rx, ry) = (rx, ry);
            let dy = if index < 2 {
                y - r.y
            } else {
                r.y + r.height - y
            };
            if dy >= ry {
                continue;
            }
            let t = ((ry - dy) / ry).clamp(0.0, 1.0);
            let inset = rx * (1.0 - (1.0 - t * t).sqrt());
            if index == 0 || index == 3 {
                left = left.max(r.x + inset);
            } else {
                right = right.min(r.x + r.width - inset);
            }
        }
        (left, right.max(left))
    }
    pub fn blurred(self, x: f64, y: f64, sigma: f64) -> f64 {
        if sigma == 0.0 {
            return f64::from(self.contains(x, y));
        }
        let r = self.rect;
        if r.width <= 0.0 || r.height <= 0.0 {
            return 0.0;
        }
        let interval = |lo: f64, hi: f64, p: f64| cdf((hi - p) / sigma) - cdf((lo - p) / sigma);
        let vertical = interval(r.y, r.y + r.height, y);
        let horizontal = interval(r.x, r.x + r.width, x);
        let rectangular = vertical * horizontal;
        if rectangular < 0.5 / CHANNEL_MAX {
            return 0.0;
        }
        if self.corners.is_square() {
            return rectangular;
        }
        let reach = GAUSSIAN_LIMIT * sigma;
        let nearby = self.corners.0.iter().enumerate().any(|(i, &[rx, ry])| {
            let cx = if i == 0 || i == 3 {
                r.x
            } else {
                r.x + r.width - rx
            };
            let cy = if i < 2 { r.y } else { r.y + r.height - ry };
            rx > 0.0
                && ry > 0.0
                && x >= cx - reach
                && x <= cx + rx + reach
                && y >= cy - reach
                && y <= cy + ry + reach
        });
        if !nearby {
            return rectangular;
        }
        let low = ((r.y - y) / sigma).max(-GAUSSIAN_LIMIT);
        let high = ((r.y + r.height - y) / sigma).min(GAUSSIAN_LIMIT);
        if low >= high {
            return 0.0;
        }
        // Split at every corner's tangent, where the cross-section changes
        // formula. Overlapping padding radii use the same clipped arcs as hits.
        let mut cuts = [high; 6];
        cuts[0] = low;
        for (i, &[_rx, ry]) in self.corners.0.iter().enumerate() {
            let edge = if i < 2 { r.y + ry } else { r.y + r.height - ry };
            cuts[i + 1] = ((edge - y) / sigma).clamp(low, high);
        }
        cuts.sort_by(f64::total_cmp);
        let mut sum = 0.0;
        for pair in cuts.windows(2) {
            let middle = (pair[0] + pair[1]) * HALF;
            let radius = (pair[1] - pair[0]) * HALF;
            if radius == 0.0 {
                continue;
            }
            for &(node, weight) in &QUADRATURE {
                for sign in [-1.0, 1.0] {
                    let t = middle + sign * radius * node;
                    let (left, right) = self.x_range(y + sigma * t);
                    let x_weight = interval(left, right, x);
                    sum += radius * weight * INV_SQRT_TAU * (-HALF * t * t).exp() * x_weight;
                }
            }
        }
        sum.clamp(0.0, 1.0)
    }
}
// Abramowitz-Stegun 7.1.26, absolute erf error <1.5e-7. No libm lookup or
// blur-sized kernel allocation, even when a CSS length is very large.
fn cdf(x: f64) -> f64 {
    if x <= -GAUSSIAN_LIMIT {
        return 0.0;
    }
    if x >= GAUSSIAN_LIMIT {
        return 1.0;
    }
    let z = x.abs() * INV_SQRT_TWO;
    let t = 1.0 / (1.0 + 0.327_591_1 * z);
    let p = (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
        + 0.254_829_592)
        * t;
    let erf = 1.0 - p * (-z * z).exp();
    HALF * (1.0 + if x < 0.0 { -erf } else { erf })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gaussian_disk_center_matches_its_exact_integral() {
        let circle = Shape {
            rect: Bounds {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
            corners: Radii([[10.0; 2]; 4]),
        };
        for sigma in [0.5_f64, 2.0, 5.0, 10.0, 100.0] {
            let expected = 1.0 - (-100.0 / (2.0 * sigma * sigma)).exp();
            assert!(
                (circle.blurred(10.0, 10.0, sigma) - expected).abs() < 0.002,
                "{sigma}"
            );
        }
    }
}
