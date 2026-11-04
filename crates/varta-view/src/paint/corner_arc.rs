//! Cubic quarter ellipses with bounded geometric error at large radii.
use tiny_skia::PathBuilder;

const QUARTER_ERROR_BOUND: f64 = 0.000_273;
const TARGET_ERROR_CSS: f64 = 0.005;
const MAX_QUARTER_SEGMENTS: u32 = 8;
#[derive(Clone, Copy)]
pub(super) enum Quarter {
    TopRight,
    BottomRight,
    BottomLeft,
    TopLeft,
}
impl Quarter {
    fn unit(self, sin: f64, cos: f64) -> ([f64; 2], [f64; 2]) {
        match self {
            Self::TopRight => ([sin, -cos], [cos, sin]),
            Self::BottomRight => ([cos, sin], [-sin, cos]),
            Self::BottomLeft => ([-sin, cos], [-cos, -sin]),
            Self::TopLeft => ([-cos, -sin], [sin, -cos]),
        }
    }
}
fn direction(index: u32, count: u32) -> (f64, f64) {
    if index == 0 {
        (0.0, 1.0)
    } else if index == count {
        (1.0, 0.0)
    } else {
        (f64::from(index) * std::f64::consts::FRAC_PI_2 / f64::from(count)).sin_cos()
    }
}
pub(super) fn append(path: &mut PathBuilder, center: [f64; 2], radius: [f32; 2], quarter: Quarter) {
    let [rx, ry] = radius.map(f64::from);
    let maximum = rx.max(ry);
    // Subdividing an existing cubic would retain its old circle error. Emit
    // separate circle-tangent cubics instead. Beyond the target, source f32
    // coordinate precision is the meaningful floor; at most eight are needed.
    let tolerance = TARGET_ERROR_CSS.max(maximum * f64::from(f32::EPSILON) * 0.5);
    let mut count = 1_u32;
    // The radial-error estimate decreases with the sixth power of the count.
    while count < MAX_QUARTER_SEGMENTS
        && maximum * QUARTER_ERROR_BOUND / f64::from(count).powi(6) > tolerance
    {
        count *= 2;
    }
    let tangent = 4.0 / 3.0 * (std::f64::consts::FRAC_PI_2 / f64::from(count) / 4.0).tan();
    for index in 0..count {
        let (sin, cos) = direction(index, count);
        let (start, start_tangent) = quarter.unit(sin, cos);
        let (sin, cos) = direction(index + 1, count);
        let (end, end_tangent) = quarter.unit(sin, cos);
        let point = |unit: [f64; 2]| {
            [
                (center[0] + rx * unit[0]) as f32,
                (center[1] + ry * unit[1]) as f32,
            ]
        };
        let first = point([
            start[0] + tangent * start_tangent[0],
            start[1] + tangent * start_tangent[1],
        ]);
        let second = point([
            end[0] - tangent * end_tangent[0],
            end[1] - tangent * end_tangent[1],
        ]);
        let end = point(end);
        path.cubic_to(first[0], first[1], second[0], second[1], end[0], end[1]);
    }
}
