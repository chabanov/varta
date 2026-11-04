//! Default slice decoration uses one rounded box, including wrapping and hits.
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
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap()
}
fn view(b: B) -> View {
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn wrapped(radius: &str) -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#00ff00")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "24px")
        .set_style(2, P::FontFamily, "\"Varta Fixture 400\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "20px")
        .set_style(2, P::Color, "transparent")
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::BackgroundColor, "#ff0000")
        .set_style(3, P::BorderRadius, radius)
        .create_text(4, "A A A A")
        .insert(3, 4, 0);
    view(b)
}
fn reference(radius: &str) -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#00ff00")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "24px")
        .set_style(2, P::Height, "20px")
        .set_style(2, P::BackgroundColor, "#ff0000")
        .set_style(2, P::BorderRadius, radius);
    view(b)
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
#[test]
fn wrapped_background_is_sliced_from_the_whole_box_at_three_scales() {
    for radius in [
        "10px",
        "50%",
        "4px 12px 2px 8px",
        "999999999999999999999999999999%",
        "0",
    ] {
        let v = wrapped(radius);
        let snapshot = v.snapshot();
        for scale in [1.0, 1.25, 2.0] {
            let side = (80.0 * scale) as u32;
            let actual = v
                .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
                .unwrap();
            let expected = reference(radius)
                .rasterize_scaled(side, side, scale, None)
                .unwrap();
            let w = (24.0 * scale) as u32;
            let h = (20.0 * scale) as u32;
            for y in 0..h {
                for x in 0..w {
                    for line in 0..2 {
                        let a = pixel(&actual, x, y + line * h);
                        // The fixture's normalized corner extents are <= half its
                        // 24px fragment. Extend its independent box's middle by
                        // one fragment, retaining the original end-cap pixels.
                        let b = if (line == 0 && x < w / 2) || (line == 1 && x >= w / 2) {
                            pixel(&expected, x, y)
                        } else {
                            [255, 0, 0, 255]
                        };
                        assert!(
                            a.into_iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2),
                            "{radius} scale{scale} at{x}/{y} line{line}: {a:?}/{b:?}"
                        );
                    }
                }
            }
        }
        assert_eq!(v.snapshot(), snapshot);
    }
}
#[test]
fn line_break_edges_are_square_and_text_hits_are_not_clipped_by_background_radius() {
    let v = wrapped("10px");
    let mut f = font();
    let g = v.layout_boxes_with_text(80.0, 80.0, &mut f, 20.0).unwrap();
    assert_eq!((g[&3].width, g[&3].height), (24.0, 40.0));
    let image = v.rasterize_with_text(80, 80, &mut f, 20.0).unwrap();
    assert_eq!(pixel(&image, 0, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 23, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 0, 20), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 23, 20), [0, 255, 0, 255]);
    assert_eq!(
        image.hit_test(0.5, 0.5),
        Some(3),
        "text retains its font-box hit even through a background cutout"
    );
}
#[test]
fn retained_mutation_vector_and_stateless_consumers_share_sliced_geometry() {
    let mut v = wrapped("10px");
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for radius in ["10px", "50%", "3px 20px 0 8px", "0"] {
        let mut b = B::new();
        b.set_style(3, P::BorderRadius, radius);
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        for scale in [1.0, 2.0] {
            let side = (80.0 * scale) as u32;
            let expected = v
                .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
                .unwrap();
            assert_eq!(renderer.render(&v, side, side, scale).unwrap(), expected);
            let PreparedFrame::Vector { metadata, scene } =
                renderer.prepare(&v, side, side, scale).unwrap()
            else {
                panic!("bounded rounded inline vectors")
            };
            assert_eq!(scene.rasterize(metadata).unwrap(), expected);
            assert_eq!(renderer.stats().dirty_pixels, 0);
        }
    }
}
#[test]
fn rounded_parent_hit_does_not_cover_cutouts_outside_smaller_text_descendants() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "100px")
        .set_style(2, P::FontFamily, "\"Varta Fixture 400\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "40px")
        .set_style(2, P::Color, "transparent")
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::FontSize, "40px")
        .set_style(3, P::BorderRadius, "50%")
        .set_style(3, P::BackgroundColor, "#ff0000")
        .create_element(4, Tag::Span)
        .insert(3, 4, 0)
        .set_style(4, P::FontSize, "20px")
        .create_text(5, "A")
        .insert(4, 5, 0);
    let v = view(b);
    let mut f = font();
    let g = v
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    let image = v.rasterize_with_text(100, 100, &mut f, 20.0).unwrap();
    assert_eq!(image.hit_test(g[&3].x + 0.5, g[&3].y + 0.5), Some(2));
    assert_eq!(image.hit_test(g[&4].x + 4.0, g[&4].y + 1.0), Some(4));
}

fn overlapping_fragments() -> View {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "8px")
        .set_style(2, P::FontFamily, "\"Varta Fixture 400\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "0.01px")
        .set_style(2, P::WhiteSpace, "pre-wrap");
    for node in 3..=9 {
        b.create_element(node, Tag::Span)
            .insert(node - 1, node, 0)
            .set_style(node, P::BorderRadius, "10px")
            // Middle fragments of a broken box have no corners; their border
            // ring slices still consume the shared contour work.
            .set_style(node, P::Border, "1px solid #0000ff")
            .set_style(node, P::BackgroundColor, "#ff0000");
    }
    b.create_text(10, &"A\n".repeat(3500)).insert(9, 10, 0);
    view(b)
}
#[test]
fn rounded_inline_work_is_shared_across_overlapping_fragment_backgrounds() {
    let v = overlapping_fragments();
    let before = v.snapshot();
    let error = v
        .rasterize_with_text(80, 80, &mut font(), 20.0)
        .unwrap_err();
    assert!(
        matches!(error,varta_view::PaintError::Unsupported(node) if (3..=9).contains(&node)),
        "expected shared paint work refusal, got {error}"
    );
    assert_eq!(v.snapshot(), before);
}

#[test]
fn rounded_inline_background_never_becomes_a_glyph_clip() {
    let mut v = wrapped("50%");
    let mut b = B::new();
    b.set_style(3, P::Color, "#0000ff");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    let image = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
    assert_eq!(
        pixel(&image, 1, 2),
        [0, 0, 255, 255],
        "fixture glyph ink extends through the rounded background cutout"
    );
}

#[test]
fn invisible_inline_backgrounds_skip_pixel_work_but_preserve_input_geometry() {
    for group_hidden in [false, true] {
        let mut v = overlapping_fragments();
        let mut b = B::new();
        if group_hidden {
            b.set_style(2, P::Opacity, "0");
        } else {
            b.set_style(2, P::Color, "transparent");
            for node in 3..=9 {
                b.set_style(node, P::BackgroundColor, "transparent")
                    .set_style(node, P::BorderColor, "transparent");
            }
        }
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        let before = v.snapshot();
        let image = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
        assert!(image.pixels.iter().all(|b| *b == 0));
        assert_eq!(image.hit_test(4.0, 4.0), Some(9));
        assert_eq!(v.snapshot(), before);
    }
}
