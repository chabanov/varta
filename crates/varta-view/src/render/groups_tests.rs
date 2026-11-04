use super::*;
fn rgba(color: tiny_skia::PremultipliedColorU8) -> [u8; 4] {
    [color.red(), color.green(), color.blue(), color.alpha()]
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_xywh(x, y, w, h).unwrap()
}
fn draw(primitive: Primitive, color: Color, bounds: Rect) -> Draw {
    Draw {
        primitive,
        color,
        transform: Transform::identity(),
        clip: None,
        clips: None,
        bounds,
    }
}
fn triangle(reverse: bool, color: Color) -> Draw {
    let mut p = PathBuilder::new();
    p.move_to(8.0, 0.0);
    if reverse {
        p.line_to(8.0, 8.0);
    } else {
        p.line_to(0.0, 0.0);
    }
    p.line_to(0.0, 8.0);
    p.close();
    draw(
        Primitive::Path(Arc::new(p.finish().unwrap()), FillRule::Winding),
        color,
        rect(0.0, 0.0, 8.0, 8.0),
    )
}
#[test]
fn isolated_additive_coverage_keeps_opaque_and_translucent_corner_seams_filled() {
    for alpha in [1.0, 0.5] {
        let color = Color::from_rgba(1.0, 0.0, 0.0, alpha).unwrap();
        let bounds = rect(0.0, 0.0, 8.0, 8.0);
        let mut program = vec![
            draw(
                Primitive::BeginGroup {
                    opacity: 1.0,
                    additive: true,
                    end: 3,
                },
                Color::TRANSPARENT,
                bounds,
            ),
            triangle(false, color),
            triangle(true, color),
            draw(Primitive::EndGroup, Color::TRANSPARENT, bounds),
        ];
        let mut image = Pixmap::new(12, 12).unwrap();
        image.fill(Color::from_rgba8(0, 255, 0, 255));
        replay(&mut image, &program, rect(0.0, 0.0, 12.0, 12.0)).unwrap();
        let expected = if alpha == 1.0 {
            [255, 0, 0, 255]
        } else {
            [128, 127, 0, 255]
        };
        for y in 0..8 {
            for x in 0..8 {
                let actual = rgba(image.pixel(x, y).unwrap());
                assert!(
                    actual
                        .into_iter()
                        .zip(expected)
                        .all(|(a, b)| a.abs_diff(b) <= 1),
                    "alpha={alpha} at{x},{y}: {actual:?}"
                );
            }
        }
        let Primitive::BeginGroup { additive, .. } = &mut program[0].primitive else {
            unreachable!()
        };
        *additive = false;
        let mut ordinary = Pixmap::new(12, 12).unwrap();
        ordinary.fill(Color::from_rgba8(0, 255, 0, 255));
        replay(&mut ordinary, &program, rect(0.0, 0.0, 12.0, 12.0)).unwrap();
        assert!(
            ordinary.pixel(3, 4).unwrap().green() > image.pixel(3, 4).unwrap().green(),
            "ordinary over-paint has a coverage seam"
        );
    }
}
#[test]
fn tiles_match_an_independent_full_layer_composite_with_fractional_geometry_and_clips() {
    use crate::{BoxRect, paint::corners::Corners};
    let viewport = rect(0.0, 0.0, 320.0, 280.0);
    let background = draw(
        Primitive::Rect(viewport),
        Color::from_rgba8(0, 255, 0, 255),
        viewport,
    );
    let body = rect(123.25, 120.5, 170.5, 150.25);
    let mut path = PathBuilder::new();
    path.move_to(80.5, 50.25);
    path.cubic_to(300.25, 30.5, 40.25, 240.75, 280.25, 265.5);
    path.line_to(90.5, 250.25);
    path.close();
    let path = path.finish().unwrap();
    let bounds = path.bounds();
    let children = vec![
        draw(
            Primitive::Rect(body),
            Color::from_rgba8(255, 0, 0, 255),
            body,
        ),
        draw(
            Primitive::Path(Arc::new(path), FillRule::Winding),
            Color::from_rgba8(0, 0, 255, 192),
            bounds,
        ),
    ];
    let shape = BoxRect {
        x: 20.0,
        y: 20.0,
        width: 280.0,
        height: 240.0,
    };
    let corners = Corners([[80.0; 2]; 4]);
    let curve = corners.path(shape).unwrap();
    let clip = Clip::new(None, Some(curve.clone()), shape, corners);
    let mut begin = draw(
        Primitive::BeginGroup {
            opacity: 0.375,
            additive: false,
            end: 4,
        },
        Color::TRANSPARENT,
        viewport,
    );
    begin.clips = Some(clip);
    begin.clip = Some(rect(40.25, 35.75, 250.0, 220.0));
    let mut program = vec![background.clone(), begin.clone()];
    program.extend(children.iter().cloned());
    program.push(draw(Primitive::EndGroup, Color::TRANSPARENT, viewport));
    let mut actual = Pixmap::new(320, 280).unwrap();
    replay(&mut actual, &program, viewport).unwrap();
    let mut layer = Pixmap::new(320, 280).unwrap();
    super::super::replay(&mut layer, &children).unwrap();
    let mut mask = Mask::new(320, 280).unwrap();
    mask.fill_path(&curve, FillRule::Winding, true, Transform::identity());
    mask.intersect_path(
        &PathBuilder::from_rect(begin.clip.unwrap()),
        FillRule::Winding,
        true,
        Transform::identity(),
    );
    let mut expected = Pixmap::new(320, 280).unwrap();
    super::super::replay(&mut expected, std::slice::from_ref(&background)).unwrap();
    expected.draw_pixmap(
        0,
        0,
        layer.as_ref(),
        &PixmapPaint {
            opacity: 0.375,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        Some(&mask),
    );
    let max = actual
        .data()
        .iter()
        .zip(expected.data())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(max <= 2, "tile-local AA maximum difference {max}");
}
#[test]
fn invalid_group_programs_refuse_before_any_destination_effect() {
    let bounds = rect(0.0, 0.0, 8.0, 8.0);
    for primitive in [
        Primitive::BeginGroup {
            opacity: f32::NAN,
            additive: false,
            end: 1,
        },
        Primitive::BeginGroup {
            opacity: 0.5,
            additive: false,
            end: 100,
        },
        Primitive::EndGroup,
    ] {
        let mut image = Pixmap::new(8, 8).unwrap();
        image.fill(Color::from_rgba8(0, 255, 0, 255));
        let before = image.data().to_vec();
        let commands = vec![
            draw(primitive, Color::TRANSPARENT, bounds),
            draw(Primitive::EndGroup, Color::TRANSPARENT, bounds),
        ];
        assert!(replay(&mut image, &commands, bounds).is_err());
        assert_eq!(image.data(), before);
    }
}
#[test]
fn maximum_native_group_depth_uses_tile_storage_and_an_explicit_stack() {
    let bounds = rect(0.0, 0.0, 8.0, 8.0);
    let mut program = Vec::new();
    for index in 0..MAX_GROUP_DEPTH {
        program.push(draw(
            Primitive::BeginGroup {
                opacity: 1.0,
                additive: false,
                end: 2 * MAX_GROUP_DEPTH - index,
            },
            Color::TRANSPARENT,
            bounds,
        ));
    }
    program.push(draw(
        Primitive::Rect(bounds),
        Color::from_rgba8(0, 0, 255, 255),
        bounds,
    ));
    for _ in 0..MAX_GROUP_DEPTH {
        program.push(draw(Primitive::EndGroup, Color::TRANSPARENT, bounds));
    }
    assert_eq!(depth(&program).unwrap(), MAX_GROUP_DEPTH);
    assert!((MAX_GROUP_DEPTH + 2) * TILE_EDGE as usize * TILE_EDGE as usize * 4 < 9 * 1024 * 1024);
    let mut image = Pixmap::new(8, 8).unwrap();
    replay(&mut image, &program, bounds).unwrap();
    assert_eq!(rgba(image.pixel(4, 4).unwrap()), [0, 0, 255, 255]);
}
