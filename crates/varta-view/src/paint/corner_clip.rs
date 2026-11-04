//! Bounded half-plane clipping of generated rounded contours, retaining
//! cubic arcs. Projected extrema split a cubic before any crossing search, so
//! a diagonal plane can enter and leave the same quarter without being lost.
use super::clip_curve::{self, Plane};
use tiny_skia::{Path, PathBuilder, Rect};

pub(super) fn to_planes(path: &Path, planes: &[Plane]) -> Option<Path> {
    let contours = clip_curve::contours(path)?;
    let mut result = PathBuilder::new();
    for mut segments in contours {
        for &plane in planes {
            segments = clip_curve::half_plane(segments, plane);
            if segments.is_empty() {
                break;
            }
        }
        if let Some(first) = segments.first() {
            let start = first.start();
            result.move_to(start.0[0] as f32, start.0[1] as f32);
            for segment in segments {
                segment.emit(&mut result);
            }
            result.close();
        }
    }
    result.finish()
}
pub(super) fn to_rect(path: Path, rect: Rect) -> Option<Path> {
    let bounds = path.bounds();
    if rect.left() <= bounds.left()
        && rect.top() <= bounds.top()
        && rect.right() >= bounds.right()
        && rect.bottom() >= bounds.bottom()
    {
        return Some(path);
    }
    let planes = [
        Plane::new([1.0, 0.0], f64::from(rect.left()))?,
        Plane::new([-1.0, 0.0], -f64::from(rect.right()))?,
        Plane::new([0.0, 1.0], f64::from(rect.top()))?,
        Plane::new([0.0, -1.0], -f64::from(rect.bottom()))?,
    ];
    to_planes(&path, &planes)
}
