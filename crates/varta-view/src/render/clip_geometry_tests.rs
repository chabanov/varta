use super::*;
use crate::paint::corners::Corners;
use std::sync::Arc;

#[test]
fn empty_pixel_clip_keeps_logical_visibility_for_text_observers() {
    let bounds = rect(3.25, 3.25, 0.03125, 40.0);
    let clip = Clip::with_paint(None, None, bounds, Corners([[0.015625, 20.0]; 4]));
    let query = rect(3.265625, 20.0, 0.0, 6.0);
    assert!(clip.bounds.is_none());
    assert!(clip.contains(3.265625, 23.25));
    assert_eq!(visible(&clip, query), Some(query));
}

fn rounded(parent: Option<Arc<Clip>>, rect: BoxRect, corners: Corners) -> Arc<Clip> {
    Clip::new(parent, corners.clipped_path(rect), rect, corners)
}
fn rect(x: f32, y: f32, width: f32, height: f32) -> BoxRect {
    BoxRect {
        x,
        y,
        width,
        height,
    }
}
fn circle(parent: Option<Arc<Clip>>, x: f32, y: f32) -> Arc<Clip> {
    rounded(parent, rect(x, y, 100.0, 100.0), Corners([[50.0; 2]; 4]))
}
fn visible(clip: &Clip, query: BoxRect) -> Option<BoxRect> {
    clip.visible_bounds(query, &mut AccessibilityWork::default())
        .unwrap()
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}
#[test]
fn analytic_circle_intersections_cover_corners_tangencies_and_carets() {
    let clip = circle(None, 0.0, 0.0);
    let quarter = visible(&clip, rect(0.0, 0.0, 20.0, 20.0)).unwrap();
    close(quarter.x, 10.0);
    close(quarter.y, 10.0);
    close(quarter.width, 10.0);
    close(quarter.height, 10.0);
    assert!(visible(&clip, rect(0.0, 0.0, 10.0, 10.0)).is_none());
    let tangent = visible(&clip, rect(0.0, 0.0, 100.0, 0.0)).unwrap();
    close(tangent.x, 50.0);
    close(tangent.width, 0.0);
    close(tangent.height, 0.0);
    let caret = visible(&clip, rect(10.0, 0.0, 0.0, 100.0)).unwrap();
    close(caret.y, 20.0);
    close(caret.height, 60.0);
    close(caret.width, 0.0);
    assert_eq!(
        visible(&clip, rect(40.0, 40.0, 20.0, 20.0)),
        Some(rect(40.0, 40.0, 20.0, 20.0))
    );
}
#[test]
fn overlapping_boxes_do_not_fabricate_a_visible_intersection_between_disjoint_curves() {
    let clip = circle(Some(circle(None, 0.0, 0.0)), 75.0, 75.0);
    assert!(visible(&clip, rect(0.0, 0.0, 200.0, 200.0)).is_none());
    let lens = circle(Some(circle(None, 0.0, 0.0)), 60.0, 0.0);
    let bounds = visible(&lens, rect(0.0, 0.0, 200.0, 200.0)).unwrap();
    close(bounds.x, 60.0);
    close(bounds.y, 10.0);
    close(bounds.width, 40.0);
    close(bounds.height, 80.0);
}
#[test]
fn asymmetric_oversized_inner_arcs_bound_independently_sampled_visible_points() {
    let outer = rect(3.0, 5.0, 120.0, 90.0);
    let (inner, corners) = Corners([[60.0, 45.0], [30.0, 20.0], [10.0, 40.0], [45.0, 35.0]])
        .inset(outer, [1.0, 65.0, 3.0, 2.0]);
    let clip = rounded(Some(circle(None, -5.0, -5.0)), inner, corners);
    for query in [
        rect(0.0, 0.0, 130.0, 100.0),
        rect(3.0, 5.0, 30.0, 35.0),
        rect(0.0, 55.0, 65.0, 40.0),
    ] {
        let bounds = visible(&clip, query);
        let mut count = 0;
        for y in 0..200 {
            for x in 0..260 {
                let (x, y) = (x as f32 / 2.0 + 0.25, y as f32 / 2.0 + 0.25);
                if x >= query.x
                    && y >= query.y
                    && x < query.x + query.width
                    && y < query.y + query.height
                    && clip.contains(x, y)
                {
                    count += 1;
                    let b = bounds.expect("sampled visible point requires nonempty bounds");
                    assert!(
                        x >= b.x - 0.001
                            && y >= b.y - 0.001
                            && x <= b.x + b.width + 0.001
                            && y <= b.y + b.height + 0.001
                    );
                }
            }
        }
        assert!(count > 0);
        let b = bounds.unwrap();
        assert!(
            b.x >= query.x
                && b.y >= query.y
                && b.x + b.width <= query.x + query.width + 0.001
                && b.y + b.height <= query.y + query.height + 0.001
        );
    }
}
#[test]
fn one_allowance_covers_all_ancestors_and_queries_and_refuses_before_search() {
    let clip = circle(Some(circle(None, 10.0, 10.0)), 0.0, 0.0);
    let mut work = AccessibilityWork::default();
    work.charge(crate::ACCESSIBILITY_TEXT_WORK_LIMIT - 12)
        .unwrap();
    assert!(
        clip.visible_bounds(rect(40.0, 40.0, 10.0, 10.0), &mut work)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        clip.visible_bounds(rect(0.0, 0.0, 20.0, 20.0), &mut work),
        Err(AccessibilityError::WorkLimit {
            limit: crate::ACCESSIBILITY_TEXT_WORK_LIMIT
        })
    );
    assert!(visible(&clip, rect(40.0, 40.0, 10.0, 10.0)).is_some());
}
#[test]
fn invalid_queries_and_empty_paths_return_no_visible_geometry() {
    let clip = circle(None, 0.0, 0.0);
    for query in [
        rect(f32::NAN, 0.0, 1.0, 1.0),
        rect(0.0, 0.0, -1.0, 1.0),
        rect(200.0, 0.0, 1.0, 1.0),
    ] {
        assert!(visible(&clip, query).is_none());
    }
    let empty = Clip::new(
        Some(clip),
        None,
        rect(0.0, 0.0, 0.0, 100.0),
        Corners::default(),
    );
    assert!(visible(&empty, rect(0.0, 0.0, 100.0, 100.0)).is_none());
}
