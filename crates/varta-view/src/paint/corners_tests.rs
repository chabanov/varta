use super::*;
fn bounds() -> BoxRect {
    BoxRect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
    }
}
fn resolve(raw: &str) -> Corners {
    Corners::resolve(
        raw,
        bounds(),
        Units::new(&crate::Environment::default()).unwrap(),
    )
    .unwrap()
}
#[test]
fn percentages_use_separate_box_axes() {
    assert_eq!(resolve("50%").0, [[50.0, 25.0]; 4]);
}
#[test]
fn shorthand_orders_corners_and_overlap_uses_one_common_factor() {
    // Left: 40+60 > height50 gives factor0.5; all axes/corners scale.
    assert_eq!(
        resolve("40px 20px 10px 60px").0,
        [[20.0, 20.0], [10.0, 10.0], [5.0, 5.0], [30.0, 30.0]]
    );
    assert_eq!(
        resolve("10px 20px 30px").0,
        [[10.0, 10.0], [20.0, 20.0], [30.0, 30.0], [20.0, 20.0]]
    );
    assert_eq!(
        resolve("10px 20px").0,
        [[10.0, 10.0], [20.0, 20.0], [10.0, 10.0], [20.0, 20.0]]
    );
}
#[test]
fn containment_keeps_center_and_straight_edges_but_excludes_corner_cutouts() {
    let corners = resolve("50%");
    assert!(corners.contains(bounds(), 60.0, 45.0));
    assert!(corners.contains(bounds(), 60.0, 20.0));
    assert!(!corners.contains(bounds(), 10.0, 20.0));
    assert!(!corners.contains(bounds(), 110.0, 45.0));
    assert!(!corners.contains(bounds(), f32::NAN, 45.0));
}
#[test]
fn zero_radius_is_square_and_invalid_geometry_is_refused() {
    assert!(resolve("0").contains(bounds(), 10.0, 20.0));
    let units = Units::new(&crate::Environment::default()).unwrap();
    for raw in ["", "-1px", "NaN%", "1px 2px 3px 4px 5px", "1px / 2px"] {
        assert!(Corners::resolve(raw, bounds(), units).is_none(), "{raw}");
    }
    let bad = BoxRect {
        width: f32::INFINITY,
        ..bounds()
    };
    assert!(Corners::resolve("10px", bad, units).is_none());
}

#[test]
fn emitted_ellipse_preserves_bounds_and_rasterizes_cutouts_at_both_scales() {
    let shape = resolve("50%").path(bounds()).unwrap();
    assert_eq!(
        shape.bounds(),
        tiny_skia::Rect::from_xywh(10.0, 20.0, 100.0, 50.0).unwrap()
    );
    for scale in [1.0, 2.0] {
        let mut image =
            tiny_skia::Pixmap::new((120.0 * scale) as u32, (80.0 * scale) as u32).unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(255, 0, 0, 255);
        image.fill_path(
            &shape,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::from_scale(scale, scale),
            None,
        );
        assert_eq!(
            image
                .pixel((10.0 * scale) as u32, (20.0 * scale) as u32)
                .unwrap()
                .alpha(),
            0
        );
        assert_eq!(
            image
                .pixel((60.0 * scale) as u32, (45.0 * scale) as u32)
                .unwrap()
                .alpha(),
            255
        );
        assert_eq!(
            image
                .pixel((110.0 * scale) as u32, (45.0 * scale) as u32)
                .unwrap()
                .alpha(),
            0
        );
    }
}

#[test]
fn enormous_finite_radii_normalize_without_intermediate_overflow() {
    assert_eq!(resolve("1e300%").0, [[50.0, 25.0]; 4]);
    assert_eq!(resolve("1e300px").0, [[25.0, 25.0]; 4]);
    let mixed = resolve("1e300px 0 1e300px 0");
    assert_eq!(
        mixed.0,
        [[50.0, 50.0], [0.0, 0.0], [50.0, 50.0], [0.0, 0.0]]
    );
}

#[test]
fn asymmetric_inner_arc_is_clipped_instead_of_renormalized() {
    let outer = BoxRect {
        x: 0.0,
        y: 0.0,
        width: 130.0,
        height: 130.0,
    };
    let units = Units::new(&crate::Environment::default()).unwrap();
    let radii = Corners::resolve("0 100% 0 0", outer, units).unwrap();
    let (bounds, inner) = radii.inset(outer, [30.0; 4]);
    assert_eq!(inner.0, [[0.0; 2], [100.0; 2], [0.0; 2], [0.0; 2]]);
    let path = inner.clipped_path(bounds).unwrap();
    let mut image = tiny_skia::Pixmap::new(140, 140).unwrap();
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    image.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
    assert_eq!(image.pixel(31, 31).unwrap().alpha(), 0);
    assert_eq!(image.pixel(50, 90).unwrap().alpha(), 255);
    assert_eq!(image.pixel(20, 90).unwrap().alpha(), 0);
    assert_eq!(image.pixel(90, 110).unwrap().alpha(), 0);
}

#[test]
fn clipped_inner_contours_agree_with_independent_ellipse_membership() {
    let mut seed = 0x4ec3_5741_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (seed >> 32) as u32
    };
    let units = Units::new(&crate::Environment::default()).unwrap();
    let mut checked = 0;
    let mut filled = 0;
    for _ in 0..100 {
        let outer = BoxRect {
            x: 2.0,
            y: 2.0,
            width: (40 + next() % 80) as f32,
            height: (40 + next() % 80) as f32,
        };
        let raw = format!(
            "{}px {}px {}px {}px",
            next() % 160,
            next() % 160,
            next() % 160,
            next() % 160
        );
        let radii = Corners::resolve(&raw, outer, units).unwrap();
        let widths = std::array::from_fn(|_| (next() % 40) as f32);
        let (bounds, inner) = radii.inset(outer, widths);
        let mut image = tiny_skia::Pixmap::new(125, 125).unwrap();
        let mut paint = tiny_skia::Paint {
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        paint.set_color_rgba8(255, 255, 255, 255);
        if let Some(path) = inner.clipped_path(bounds) {
            assert!(
                path.bounds().left() >= bounds.x
                    && path.bounds().top() >= bounds.y
                    && path.bounds().right() <= bounds.x + bounds.width
                    && path.bounds().bottom() <= bounds.y + bounds.height
            );
            image.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
        for y in 0..125 {
            for x in 0..125 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                // Check full-coverage pixels with a one-pixel safety square.
                // A center exactly near the curve is not a binary coverage
                // oracle: the aliased and AA rasterizers round it differently.
                let clearly_inside = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                    .into_iter()
                    .all(|(dx, dy)| inner.contains(bounds, px + dx, py + dy));
                let mut clearly_outside = bounds.width == 0.0
                    || bounds.height == 0.0
                    || px + 1.0 < bounds.x
                    || py + 1.0 < bounds.y
                    || px - 1.0 >= bounds.x + bounds.width
                    || py - 1.0 >= bounds.y + bounds.height;
                for ([rx, ry], (edge_x, edge_y, dx, dy)) in inner.0.into_iter().zip([
                    (bounds.x, bounds.y, 1.0, 1.0),
                    (bounds.x + bounds.width, bounds.y, -1.0, 1.0),
                    (
                        bounds.x + bounds.width,
                        bounds.y + bounds.height,
                        -1.0,
                        -1.0,
                    ),
                    (bounds.x, bounds.y + bounds.height, 1.0, -1.0),
                ]) {
                    if rx == 0.0 || ry == 0.0 {
                        continue;
                    }
                    let (lx, ly) = ((px - edge_x) * dx, (py - edge_y) * dy);
                    if lx < rx && ly < ry {
                        // The closest point of the safety square to this ellipse
                        // center is still outside: the whole pixel is outside.
                        let nx = ((lx + 1.0).min(rx) - rx) / rx;
                        let ny = ((ly + 1.0).min(ry) - ry) / ry;
                        clearly_outside |= nx * nx + ny * ny > 1.0;
                    }
                }
                assert!(!(clearly_inside && clearly_outside));
                let expected = if clearly_inside {
                    filled += 1;
                    255
                } else if clearly_outside {
                    0
                } else {
                    continue;
                };
                assert_eq!(
                    image.pixel(x, y).unwrap().alpha(),
                    expected,
                    "{raw}, {widths:?}, inner {bounds:?}, {x},{y}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 1_000_000);
    assert!(filled > 5_000);
}

#[test]
fn large_radius_curves_keep_error_below_source_coordinate_precision() {
    let bounds = BoxRect {
        x: 0.0,
        y: 0.0,
        width: 1_000_000.0,
        height: 1_000_000.0,
    };
    let units = Units::new(&crate::Environment::default()).unwrap();
    let path = Corners::resolve("50%", bounds, units)
        .unwrap()
        .path(bounds)
        .unwrap();
    assert!(path.verbs().len() <= 40, "bounded quarter subdivisions");
    let mut current = [0.0_f64; 2];
    let point = |p: tiny_skia::Point| [f64::from(p.x), f64::from(p.y)];
    let mix =
        |a: [f64; 2], b: [f64; 2], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    for segment in path.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(p) | tiny_skia::PathSegment::LineTo(p) => {
                current = point(p)
            }
            tiny_skia::PathSegment::CubicTo(b, c, d) => {
                let (b, c, d) = (point(b), point(c), point(d));
                for sample in 1..8 {
                    let t = f64::from(sample) / 8.0;
                    let ab = mix(current, b, t);
                    let bc = mix(b, c, t);
                    let cd = mix(c, d, t);
                    let p = mix(mix(ab, bc, t), mix(bc, cd, t), t);
                    let distance = ((p[0] - 500_000.0).powi(2) + (p[1] - 500_000.0).powi(2)).sqrt();
                    assert!(
                        (distance - 500_000.0).abs() < 0.1,
                        "point {p:?}, distance {distance}"
                    );
                }
                current = d;
            }
            _ => {}
        }
    }
}
