use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, PreparedFrame, Renderer, TextMeasurer, View};
struct Denied;
impl Broker for Denied {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> Access {
        Access::Denied
    }
    fn clipboard_write(&mut self) -> Access {
        Access::Denied
    }
}
fn commit(v: &mut View, b: B) {
    v.commit(&b.finish(), false, &mut Denied).unwrap();
}
fn set(v: &mut View, id: u32, property: P, value: &str) {
    let mut b = B::new();
    b.set_style(id, property, value);
    commit(v, b);
}
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/inline.ttf").to_vec(),
        "Varta Inline Fixture",
    )
    .unwrap()
}
fn fixture() -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "24px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "20px")
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::Color, "#000000")
        .set_style(3, P::BackgroundColor, "#ff0000")
        .create_text(4, "A A A A")
        .insert(3, 4, 0);
    let mut v = View::default();
    commit(&mut v, b);
    v
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
#[test]
fn whole_inline_subtree_matches_one_outer_group_across_fragments_and_scales() {
    let mut v = fixture();
    set(&mut v, 3, P::BorderRadius, "50%");
    for scale in [1.0, 1.25, 2.0] {
        let side = (80.0 * scale) as u32;
        set(&mut v, 3, P::Opacity, "1");
        set(&mut v, 2, P::Opacity, "0.5");
        let reference = v
            .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
            .unwrap();
        set(&mut v, 3, P::Opacity, "0.5");
        set(&mut v, 2, P::Opacity, "1");
        let before = v.snapshot();
        let actual = v
            .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
            .unwrap();
        assert_eq!(actual.pixels, reference.pixels);
        assert_eq!(actual.node_bounds_map(), reference.node_bounds_map());
        assert_eq!(v.snapshot(), before);
    }
}
#[test]
fn fractional_ancestor_curve_masks_the_completed_inline_layer_once() {
    let mut v = fixture();
    set(&mut v, 3, P::BorderRadius, "50%");
    set(&mut v, 2, P::Margin, "3.25px");
    set(&mut v, 1, P::BackgroundColor, "transparent");
    for scale in [1.0, 1.25, 2.0] {
        let side = (80.0 * scale) as u32;
        set(&mut v, 1, P::BackgroundColor, "transparent");
        set(&mut v, 2, P::Overflow, "visible");
        set(&mut v, 3, P::Opacity, "1");
        let source = v
            .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
            .unwrap();
        set(&mut v, 2, P::Overflow, "hidden");
        set(&mut v, 2, P::BorderRadius, "50%");
        let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
        let PreparedFrame::Vector { scene, .. } = renderer.prepare(&v, side, side, scale).unwrap()
        else {
            panic!("opaque clip is a neutral vector scene")
        };
        let clip = scene
            .commands()
            .iter()
            .find_map(|draw| draw.clips.as_ref())
            .unwrap();
        assert!(clip.parent().is_none());
        let mut path = tiny_skia::PathBuilder::new();
        for segment in clip.segments() {
            match *segment {
                varta_view::PathSegment::Move([x, y]) => path.move_to(x, y),
                varta_view::PathSegment::Line([x, y]) => path.line_to(x, y),
                varta_view::PathSegment::Quad([x, y], [a, b]) => path.quad_to(x, y, a, b),
                varta_view::PathSegment::Cubic([x, y], [a, b], [c, d]) => {
                    path.cubic_to(x, y, a, b, c, d)
                }
                varta_view::PathSegment::Close => path.close(),
            }
        }
        let mut coverage = tiny_skia::Mask::new(side, side).unwrap();
        coverage.fill_path(
            &path.finish().unwrap(),
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );
        set(&mut v, 1, P::BackgroundColor, "#ffffff");
        set(&mut v, 3, P::Opacity, "0.5");
        let actual = v
            .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
            .unwrap();
        for (index, ((s, m), a)) in source
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(coverage.data())
            .zip(actual.pixels.as_chunks::<4>().0)
            .enumerate()
        {
            let alpha = (f64::from(s[3]) * f64::from(*m) / 255.0 * 0.5).round() as u8;
            for channel in 0..3 {
                let color = (f64::from(s[channel]) * f64::from(*m) / 255.0 * 0.5).round() as u8;
                let expected = color.saturating_add(255 - alpha);
                assert!(
                    a[channel].abs_diff(expected) <= 1,
                    "scale={scale},at={index},channel={channel}: {a:?} vs {expected},source={s:?},mask={m:?}"
                );
            }
            assert_eq!(a[3], 255);
        }
    }
}
#[test]
fn ligatures_remain_shared_and_whole_glyph_opacity_follows_first_source_owner() {
    let mut v = fixture();
    set(&mut v, 3, P::BackgroundColor, "transparent");
    let mut b = B::new();
    b.set_text(4, "A")
        .create_element(5, Tag::Span)
        .insert(2, 5, 0)
        .create_text(6, "A")
        .insert(5, 6, 0)
        .set_style(5, P::Color, "#000000");
    commit(&mut v, b);
    let geometry = v
        .layout_boxes_with_text(80.0, 80.0, &mut font(), 20.0)
        .unwrap();
    assert_eq!(geometry[&3].width, 14.0);
    assert_eq!(geometry[&5].width, 0.0);
    for (first, second, ink) in [
        ("1", "1", 0),
        ("0.5", "1", 128),
        ("1", "0.5", 0),
        ("0", "1", 255),
        ("1", "0", 0),
    ] {
        set(&mut v, 3, P::Opacity, first);
        set(&mut v, 5, P::Opacity, second);
        assert_eq!(
            v.layout_boxes_with_text(80.0, 80.0, &mut font(), 20.0)
                .unwrap(),
            geometry
        );
        let image = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
        for x in [3, 10] {
            assert!(
                pixel(&image, x, 6)
                    .into_iter()
                    .zip([ink, ink, ink, 255])
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "{first},{second}"
            );
        }
    }
}
#[test]
fn inline_context_lifts_over_later_blocks_but_stays_inside_its_parent_context() {
    let mut v = fixture();
    set(&mut v, 3, P::Opacity, "0.5");
    let mut b = B::new();
    b.create_element(5, Tag::Div)
        .insert(1, 5, 0)
        .set_style(5, P::Width, "24px")
        .set_style(5, P::Height, "40px")
        .set_style(5, P::MarginTop, "-40px")
        .set_style(5, P::BackgroundColor, "#0000ff");
    commit(&mut v, b);
    let actual = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
    assert_eq!(actual.hit_test(3.0, 6.0), Some(3));
    assert!(
        pixel(&actual, 3, 6)
            .into_iter()
            .zip([0, 0, 128, 255])
            .all(|(a, b)| a.abs_diff(b) <= 1)
    );
    set(&mut v, 2, P::Opacity, "0.5");
    set(&mut v, 5, P::Opacity, "0.5");
    let nested = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
    assert_eq!(nested.hit_test(3.0, 6.0), Some(5));
    assert!(
        pixel(&nested, 3, 6)
            .into_iter()
            .zip([95, 95, 223, 255])
            .all(|(a, b)| a.abs_diff(b) <= 1)
    );
    set(&mut v, 5, P::Opacity, "1");
    set(&mut v, 2, P::Opacity, "1");
    set(&mut v, 3, P::Opacity, "0");
    let hidden = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
    assert_eq!(hidden.hit_test(3.0, 6.0), Some(3));
    assert_eq!(pixel(&hidden, 3, 6), [0, 0, 255, 255]);
}
#[test]
fn retained_inline_groups_repaint_locally_and_recover_vector_output() {
    let mut v = fixture();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for alpha in ["0.5", "0", "1", "0.75", "1"] {
        set(&mut v, 3, P::Opacity, alpha);
        let expected = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
        assert_eq!(renderer.render(&v, 80, 80, 1.0).unwrap(), expected);
        assert_eq!(renderer.render(&v, 80, 80, 1.0).unwrap(), expected);
        assert_eq!(renderer.stats().dirty_pixels, 0);
        let actual = match renderer.prepare(&v, 80, 80, 1.0).unwrap() {
            PreparedFrame::Raster(image) => image,
            PreparedFrame::Vector { metadata, scene } => scene.rasterize(metadata).unwrap(),
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn overlapping_line_backgrounds_cover_earlier_ink_inside_one_opacity_group() {
    let mut v = fixture();
    set(&mut v, 2, P::LineHeight, "10px");
    set(&mut v, 3, P::Opacity, "0.5");
    let image = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
    // Chrome's independent synthetic-font fixture: line two's background
    // covers line one's glyph before line two's glyph is painted.
    assert_eq!(pixel(&image, 3, 6), [255, 128, 128, 255]);
    assert_eq!(pixel(&image, 3, 9), [128, 128, 128, 255]);
    assert_eq!(image.hit_test(3.0, 6.0), Some(3));
}

#[test]
fn nested_inline_contexts_share_the_contour_allowance_and_zero_keeps_targets() {
    let mut v = fixture();
    set(&mut v, 2, P::Width, "8px");
    set(&mut v, 2, P::LineHeight, "0.01px");
    let mut b = B::new();
    let mut parent = 3;
    for id in 10..17 {
        b.create_element(id, Tag::Span)
            .insert(parent, id, 0)
            .set_style(id, P::BackgroundColor, "#ff0000")
            .set_style(id, P::BorderRadius, "50%")
            .set_style(id, P::Border, "1px solid #0000ff")
            .set_style(id, P::Opacity, "0.99");
        parent = id;
    }
    b.insert(parent, 4, 0).set_text(4, &"A ".repeat(3500));
    commit(&mut v, b);
    let before = v.snapshot();
    assert!(matches!(
        v.rasterize_with_text(80, 80, &mut font(), 20.0),
        Err(varta_view::PaintError::Unsupported(_))
    ));
    assert_eq!(v.snapshot(), before);
    set(&mut v, 3, P::Opacity, "0");
    let hidden = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
    assert!(hidden.pixels.iter().all(|&v| v == 255));
    assert_eq!(hidden.hit_test(3.0, 3.0), Some(parent));
}

#[test]
fn inline_capture_crosses_the_retained_command_limit_without_renewing_the_cache() {
    let make_font = || {
        TextMeasurer::new(
            include_bytes!("fixtures/advance-400.ttf").to_vec(),
            "Varta Fixture 400",
        )
        .unwrap()
    };
    let mut v = fixture();
    set(&mut v, 2, P::FontFamily, "\"Varta Fixture 400\"");
    let mut b = B::new();
    b.set_text(4, &"A".repeat(16_384));
    for id in 20..23 {
        b.create_text(id, &"A".repeat(16_384)).insert(3, id, 0);
    }
    commit(&mut v, b);
    let reference = v
        .rasterize_with_text(80, 80, &mut make_font(), 20.0)
        .unwrap();
    set(&mut v, 3, P::Opacity, "0.5");
    let before = v.snapshot();
    let mut renderer = Renderer::new(Some(make_font()), 20.0).unwrap();
    let actual = renderer.render(&v, 80, 80, 1.0).unwrap();
    for (a, b) in actual.pixels.iter().zip(&reference.pixels) {
        assert!(a.abs_diff((u16::from(*b) + 255).div_ceil(2) as u8) <= 1);
    }
    assert_eq!(v.snapshot(), before);
    assert_eq!(renderer.render(&v, 80, 80, 1.0).unwrap(), actual);
    assert_eq!(
        renderer.stats().dirty_pixels,
        80 * 80,
        "oversize inline program is temporary"
    );
    let PreparedFrame::Raster(prepared) = renderer.prepare(&v, 80, 80, 1.0).unwrap() else {
        panic!("isolated source uses CPU composition")
    };
    assert_eq!(prepared, actual);
    let mut b = B::new();
    b.set_text(4, "A");
    for id in 20..23 {
        b.remove(id);
    }
    commit(&mut v, b);
    let expected = v
        .rasterize_with_text(80, 80, &mut make_font(), 20.0)
        .unwrap();
    assert_eq!(renderer.render(&v, 80, 80, 1.0).unwrap(), expected);
    assert_eq!(renderer.render(&v, 80, 80, 1.0).unwrap(), expected);
    assert_eq!(renderer.stats().dirty_pixels, 0);
}
