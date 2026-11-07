//! CSS border-box corner geometry shared by native painting and hit testing.
//! Source: CSS Backgrounds 3, sections 4.1 and 4.5.
use crate::{BoxRect, units::Units};

const CORNERS: usize = 4;
const TOP_LEFT: usize = 0;
const TOP_RIGHT: usize = 1;
const BOTTOM_RIGHT: usize = 2;
const BOTTOM_LEFT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Corners(pub [[f32; 2]; CORNERS]);
impl Corners {
    pub(super) fn resolve_paint(
        raw: &str,
        logical: BoxRect,
        paint: BoxRect,
        units: Units,
    ) -> Option<Self> {
        Some(Self(
            resolve_radii_in(
                raw,
                [f64::from(logical.width), f64::from(logical.height)],
                [f64::from(paint.width), f64::from(paint.height)],
                units,
            )?
            .map(|r| r.map(|v| v as f32)),
        ))
    }
    pub fn resolve(raw: &str, bounds: BoxRect, units: Units) -> Option<Self> {
        if ![bounds.x, bounds.y, bounds.width, bounds.height]
            .into_iter()
            .all(f32::is_finite)
            || bounds.width < 0.0
            || bounds.height < 0.0
        {
            return None;
        }
        Some(Self(
            resolve_radii(
                raw,
                [f64::from(bounds.width), f64::from(bounds.height)],
                units,
            )?
            .map(|r| r.map(|axis| axis as f32)),
        ))
    }
    pub fn is_square(self) -> bool {
        self.0.into_iter().all(|[x, y]| x == 0.0 || y == 0.0)
    }
    pub fn scaled(self, scale: f32) -> Self {
        Self(self.0.map(|radius| radius.map(|axis| axis * scale)))
    }
    pub fn path(self, bounds: BoxRect) -> Option<tiny_skia::Path> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return None;
        }
        let [tl, tr, br, bl] = self.0.map(|[rx, ry]| {
            if rx == 0.0 || ry == 0.0 {
                [0.0, 0.0]
            } else {
                [rx, ry]
            }
        });
        use super::corner_arc::{Quarter, append};
        let left = f64::from(bounds.x);
        let top = f64::from(bounds.y);
        let right = f64::from(bounds.x + bounds.width);
        let bottom = f64::from(bounds.y + bounds.height);
        let mut path = tiny_skia::PathBuilder::new();
        path.move_to((left + f64::from(tl[0])) as f32, top as f32);
        path.line_to((right - f64::from(tr[0])) as f32, top as f32);
        append(
            &mut path,
            [right - f64::from(tr[0]), top + f64::from(tr[1])],
            tr,
            Quarter::TopRight,
        );
        path.line_to(right as f32, (bottom - f64::from(br[1])) as f32);
        append(
            &mut path,
            [right - f64::from(br[0]), bottom - f64::from(br[1])],
            br,
            Quarter::BottomRight,
        );
        path.line_to((left + f64::from(bl[0])) as f32, bottom as f32);
        append(
            &mut path,
            [left + f64::from(bl[0]), bottom - f64::from(bl[1])],
            bl,
            Quarter::BottomLeft,
        );
        path.line_to(left as f32, (top + f64::from(tl[1])) as f32);
        append(
            &mut path,
            [left + f64::from(tl[0]), top + f64::from(tl[1])],
            tl,
            Quarter::TopLeft,
        );
        path.close();
        path.finish()
    }
    /// CSS padding-edge radii subtract the adjacent widths, without another
    /// overlap normalization. An oversized remaining arc is clipped to the box.
    pub fn inset(self, bounds: BoxRect, widths: [f32; 4]) -> (BoxRect, Self) {
        let [top, right, bottom, left] = widths;
        let inner = BoxRect {
            x: bounds.x + left,
            y: bounds.y + top,
            width: (bounds.width - left - right).max(0.0),
            height: (bounds.height - top - bottom).max(0.0),
        };
        let reductions = [[left, top], [right, top], [right, bottom], [left, bottom]];
        let radii = std::array::from_fn(|index| {
            std::array::from_fn(|axis| (self.0[index][axis] - reductions[index][axis]).max(0.0))
        });
        (inner, Self(radii))
    }
    pub fn outset(self, x: f32, y: f32) -> Self {
        Self(self.0.map(|[rx, ry]| {
            if rx == 0.0 || ry == 0.0 {
                [0.0; 2]
            } else {
                [(rx + x).max(0.0), (ry + y).max(0.0)]
            }
        }))
    }
    pub fn clipped_path(self, bounds: BoxRect) -> Option<tiny_skia::Path> {
        let rect = tiny_skia::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height)?;
        super::corner_clip::to_rect(self.path(bounds)?, rect)
    }
    pub fn contains(self, bounds: BoxRect, x: f32, y: f32) -> bool {
        if !x.is_finite()
            || !y.is_finite()
            || x < bounds.x
            || y < bounds.y
            || x >= bounds.x + bounds.width
            || y >= bounds.y + bounds.height
        {
            return false;
        }
        let edges = [
            (bounds.x, bounds.y, 1.0, 1.0),
            (bounds.x + bounds.width, bounds.y, -1.0, 1.0),
            (
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                -1.0,
                -1.0,
            ),
            (bounds.x, bounds.y + bounds.height, 1.0, -1.0),
        ];
        for ([rx, ry], (edge_x, edge_y, dx, dy)) in self.0.into_iter().zip(edges) {
            // Either zero axis makes this corner square under CSS.
            if rx == 0.0 || ry == 0.0 {
                continue;
            }
            let local_x = (x - edge_x) * dx;
            let local_y = (y - edge_y) * dy;
            if local_x < rx && local_y < ry {
                let nx = (local_x - rx) / rx;
                let ny = (local_y - ry) / ry;
                if nx * nx + ny * ny > 1.0 {
                    return false;
                }
            }
        }
        true
    }
}

/// Normalize before converting coordinates: an inline composite can be much
/// wider than its individual fragments. Keep its offsets/radii in f64 until
/// clipping the contour to a local fragment.
pub(super) fn resolve_radii(
    raw: &str,
    size: [f64; 2],
    units: Units,
) -> Option<[[f64; 2]; CORNERS]> {
    resolve_radii_in(raw, size, size, units)
}
pub(super) fn resolve_radii_in(
    raw: &str,
    size: [f64; 2],
    limit: [f64; 2],
    units: Units,
) -> Option<[[f64; 2]; CORNERS]> {
    resolve_slice_radii(raw, size, limit, units, [true; 2])
}
/// Radii of one `slice` fragment of a broken inline box. Corners on an
/// excluded left/right edge are zeroed before side-overlap normalization, as
/// Blink does when it builds a fragment's rounded border, so absent corners do
/// not shrink the present ones.
pub(super) fn resolve_slice_radii(
    raw: &str,
    size: [f64; 2],
    limit: [f64; 2],
    units: Units,
    [left, right]: [bool; 2],
) -> Option<[[f64; 2]; CORNERS]> {
    if size
        .into_iter()
        .chain(limit)
        .any(|v| !v.is_finite() || v < 0.0)
    {
        return None;
    }
    // Normalize magnitudes before applying axis factors. Even very large
    // finite percentages/lengths reduce to a bounded contour without an
    // overflowing intermediate or an arbitrary radius-admission ceiling.
    // CSS Backgrounds 3 §5.1: horizontal radii, then after `/` vertical
    // ones (the horizontal radii when omitted), each list by the box rule.
    let mut maximum = 0.0_f64;
    let mut list = |text: &str| -> Option<[(f64, [f64; 2]); CORNERS]> {
        let mut parsed = [(0.0_f64, [1.0_f64; 2]); CORNERS];
        let mut count = 0;
        for value in text.split_ascii_whitespace() {
            if count == CORNERS {
                return None;
            }
            let (number, factors) = if value == "0" {
                ("0", [1.0; 2])
            } else if let Some(number) = value.strip_suffix('%') {
                (number, [size[0] / 100.0, size[1] / 100.0])
            } else if let Some(number) = value.strip_suffix("px") {
                (number, [1.0; 2])
            } else {
                (
                    value.strip_suffix("rem")?,
                    [f64::from(units.pixels("1rem", false)?); 2],
                )
            };
            let number = number.parse::<f64>().ok()?;
            if !number.is_finite() || number < 0.0 {
                return None;
            }
            parsed[count] = (number, factors);
            maximum = maximum.max(number);
            count += 1;
        }
        Some(match count {
            1 => [parsed[0]; CORNERS],
            2 => [parsed[0], parsed[1], parsed[0], parsed[1]],
            3 => [parsed[0], parsed[1], parsed[2], parsed[1]],
            4 => parsed,
            _ => return None,
        })
    };
    let (horizontal, vertical) = match raw.split_once('/') {
        Some((horizontal, vertical)) => (list(horizontal)?, list(vertical)?),
        None => {
            let both = list(raw)?;
            (both, both)
        }
    };
    if maximum == 0.0 {
        return Some([[0.0; 2]; CORNERS]);
    }
    let mut values: [[f64; 2]; CORNERS] = std::array::from_fn(|corner| {
        let (h, h_factors) = horizontal[corner];
        let (v, v_factors) = vertical[corner];
        [h / maximum * h_factors[0], v / maximum * v_factors[1]]
    });
    for (corner, present) in [
        (TOP_LEFT, left),
        (BOTTOM_LEFT, left),
        (TOP_RIGHT, right),
        (BOTTOM_RIGHT, right),
    ] {
        if !present {
            values[corner] = [0.0; 2];
        }
    }
    let mut pixel_scale = maximum;
    for (extent, sum) in [
        (limit[0], values[TOP_LEFT][0] + values[TOP_RIGHT][0]),
        (limit[0], values[BOTTOM_LEFT][0] + values[BOTTOM_RIGHT][0]),
        (limit[1], values[TOP_LEFT][1] + values[BOTTOM_LEFT][1]),
        (limit[1], values[TOP_RIGHT][1] + values[BOTTOM_RIGHT][1]),
    ] {
        if sum > 0.0 {
            pixel_scale = pixel_scale.min(extent / sum);
        }
    }
    Some(values.map(|radius| radius.map(|axis| axis * pixel_scale)))
}

#[cfg(test)]
#[path = "corners_tests.rs"]
mod tests;
