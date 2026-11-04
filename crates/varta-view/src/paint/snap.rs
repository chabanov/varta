//! Decoration edges use whole paint pixels before DPI; input/text geometry stays logical.
//! Match Blink's edge rounding and visible-size floor without quantizing layout.
use super::BoxRect;

const MIN_VISIBLE_EXTENT: f64 = 5.0 / 64.0;
pub(super) fn edge(value: f64) -> f64 {
    (value + 0.5).floor()
}
pub(super) fn axis(origin: f64, extent: f64) -> (f64, f64) {
    let start = edge(origin);
    let size = if extent <= 0.0 {
        0.0
    } else {
        let size = edge(origin + extent) - start;
        if size == 0.0 && extent >= MIN_VISIBLE_EXTENT {
            1.0
        } else {
            size
        }
    };
    (start, size)
}
pub(super) fn rect(bounds: BoxRect) -> BoxRect {
    let (x, width) = axis(f64::from(bounds.x), f64::from(bounds.width));
    let (y, height) = axis(f64::from(bounds.y), f64::from(bounds.height));
    BoxRect {
        x: x as f32,
        y: y as f32,
        width: width as f32,
        height: height as f32,
    }
}
