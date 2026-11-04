use super::*;
use crate::paint::corner_clip::to_planes;
use tiny_skia::{FillRule, Mask, Rect, Transform};

fn plane(normal: [f64; 2], offset: f64) -> Plane {
    Plane::new(normal, offset).unwrap()
}
fn mask(path: &Path, rule: FillRule) -> Mask {
    let mut mask = Mask::new(24, 24).unwrap();
    mask.fill_path(path, rule, false, Transform::identity());
    mask
}
fn quarter() -> Path {
    let mut path = PathBuilder::new();
    path.move_to(0.0, 20.0);
    path.cubic_to(0.0, 20.0 / 3.0, 20.0 / 3.0, 0.0, 20.0, 0.0);
    path.line_to(20.0, 20.0);
    path.close();
    path.finish().unwrap()
}
#[test]
fn a_diagonal_can_enter_and_leave_one_cubic_with_both_endpoints_outside() {
    let clipped = to_planes(&quarter(), &[plane([-1.0, -1.0], -15.0)]).unwrap();
    assert!(
        clipped
            .segments()
            .any(|s| matches!(s, PathSegment::CubicTo(..)))
    );
    let actual = mask(&clipped, FillRule::Winding);
    assert_eq!(actual.data()[6 * 24 + 6], 255);
    assert_eq!(actual.data()[24 + 1], 0);
    assert_eq!(actual.data()[13 * 24 + 13], 0);
    let first = 20.0 * ((1.0 - 0.5_f64.sqrt()) / 2.0).powi(2);
    assert!((f64::from(clipped.bounds().left()) - first).abs() < 0.0001);
    assert!((f64::from(clipped.bounds().top()) - first).abs() < 0.0001);

    // The dual case starts/ends inside but leaves the half-plane in between.
    let mut bowed = PathBuilder::new();
    bowed.move_to(0.0, 0.0);
    bowed.cubic_to(20.0, 0.0, 20.0, 20.0, 0.0, 20.0);
    bowed.close();
    let clipped = to_planes(&bowed.finish().unwrap(), &[plane([-1.0, 0.0], -10.0)]).unwrap();
    let actual = mask(&clipped, FillRule::Winding);
    assert_eq!(actual.data()[10 * 24 + 5], 255);
    assert_eq!(actual.data()[10 * 24 + 14], 0);
}
#[test]
fn derivative_roots_are_sorted_and_stable_for_linear_quadratic_and_three_crossing_curves() {
    let x = plane([1.0, 0.0], 0.0);
    for (points, expected) in [
        ([0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0], vec![]),
        ([1.0, 1.0 / 3.0, 1.0 / 3.0, 1.0], vec![0.5]),
        (
            [0.0, 1.0, -1.0, 0.0],
            vec![(3.0 - 3.0_f64.sqrt()) / 6.0, (3.0 + 3.0_f64.sqrt()) / 6.0],
        ),
    ] {
        let curve = Segment::Cubic(points.map(|x| Point([x, 0.0])));
        let (roots, count) = curve.projected_extrema(x);
        assert_eq!(count, expected.len());
        for (actual, expected) in roots.into_iter().take(count).zip(expected) {
            assert!((actual - expected).abs() < 0.000_000_001);
        }
    }
}
#[test]
fn a_clipped_hole_keeps_even_odd_transparency_across_multiple_closed_components() {
    let mut path = PathBuilder::new();
    path.push_rect(Rect::from_xywh(0.0, 0.0, 20.0, 20.0).unwrap());
    path.push_rect(Rect::from_xywh(5.0, 5.0, 10.0, 10.0).unwrap());
    let p = plane([2.0, 1.0], 15.0);
    let clipped = to_planes(&path.finish().unwrap(), &[p]).unwrap();
    let actual = mask(&clipped, FillRule::EvenOdd);
    for y in 0..24 {
        for x in 0..24 {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let expected = px < 20.0
                && py < 20.0
                && !(px > 5.0 && px < 15.0 && py > 5.0 && py < 15.0)
                && p.value(Point([px, py])) >= 0.0;
            assert_eq!(
                actual.data()[y * 24 + x],
                if expected { 255 } else { 0 },
                "{x},{y}"
            );
        }
    }
}
#[test]
fn normalized_planes_keep_large_finite_coefficients_and_reject_invalid_construction() {
    let ordinary = plane([1.0, 2.0], 3.0);
    let huge = plane([1.0e300, 2.0e300], 3.0e300);
    for point in [
        Point([0.0, 0.0]),
        Point([5.0, -10.0]),
        Point([1.0e20, 1.0e20]),
    ] {
        assert!(
            (ordinary.value(point) - huge.value(point)).abs()
                <= ordinary.value(point).abs() * f64::EPSILON
        );
    }
    assert!(Plane::new([0.0, 0.0], 0.0).is_none());
    assert!(Plane::new([f64::INFINITY, 1.0], 1.0).is_none());
    assert!(Plane::new([f64::NAN, 1.0], 1.0).is_none());
    assert!(Plane::new([1.0, f64::NAN], 1.0).is_none());
    assert!(Plane::new([1.0, 1.0], f64::NAN).is_none());
}
#[test]
fn rejecting_a_contour_does_not_drop_other_components_or_invent_open_paths() {
    let mut path = PathBuilder::new();
    path.push_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0).unwrap());
    path.push_rect(Rect::from_xywh(10.0, 10.0, 10.0, 10.0).unwrap());
    let clipped = to_planes(&path.finish().unwrap(), &[plane([1.0, 0.0], 12.0)]).unwrap();
    assert_eq!(
        clipped.bounds(),
        Rect::from_xywh(12.0, 10.0, 8.0, 10.0).unwrap()
    );
    assert_eq!(contours(&clipped).unwrap().len(), 1);
    assert!(to_planes(&quarter(), &[plane([1.0, 1.0], 100.0)]).is_none());
    let mut open = PathBuilder::new();
    open.move_to(0.0, 0.0);
    open.line_to(10.0, 0.0);
    open.line_to(10.0, 10.0);
    assert!(to_planes(&open.finish().unwrap(), &[plane([1.0, 0.0], 1.0)]).is_none());
}
