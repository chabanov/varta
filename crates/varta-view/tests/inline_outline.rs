//! Inline outlines follow union contours without changing their logical targets.
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
        include_bytes!("fixtures/decoration.ttf").to_vec(),
        "Varta Decoration Fixture",
    )
    .unwrap()
}
fn fixture(text: &str) -> B {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "24px")
        .set_style(2, P::Padding, "12px")
        .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "24px")
        .set_style(2, P::Color, "transparent")
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::Outline, "2px solid #ff0000")
        .create_text(4, text)
        .insert(3, 4, 0);
    b
}
fn view(b: B) -> View {
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn render(v: &View) -> varta_view::RasterImage {
    v.rasterize_with_text(100, 120, &mut font(), 20.0).unwrap()
}
fn pixel(i: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    i.pixels[((y * i.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
const RED: [u8; 4] = [255, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
#[test]
fn touching_line_boxes_have_one_outer_outline_and_no_internal_line() {
    let image = render(&view(fixture("A A A A")));
    for y in 10..62 {
        for x in 10..38 {
            let border = !(12..36).contains(&x) || !(12..60).contains(&y);
            assert_eq!(
                pixel(&image, x, y),
                if border { RED } else { WHITE },
                "{x}/{y}"
            );
        }
    }
    assert_eq!(image.hit_test(10.5, 20.5), Some(2));
    assert_eq!(image.hit_test(12.5, 20.5), Some(3));
}
#[test]
fn separated_lines_keep_separate_outlines_and_small_gaps_do_not_get_internal_bars() {
    let mut b = fixture("A A A A");
    b.set_style(2, P::LineHeight, "40px");
    let image = render(&view(b));
    assert_eq!(pixel(&image, 20, 44), RED);
    assert_eq!(pixel(&image, 10, 50), WHITE);
    assert_eq!(pixel(&image, 20, 58), RED);
    let mut b = fixture("A A A A A A A A");
    b.create_element(5, Tag::Span)
        .insert(3, 5, 0)
        .insert(5, 4, 0)
        .set_style(5, P::FontSize, "10px");
    let image = render(&view(b));
    assert_eq!(pixel(&image, 20, 37), WHITE);
    assert_eq!(pixel(&image, 10, 37), RED);
}
#[test]
fn outline_foreground_covers_ink_and_ancestor_outline_follows_descendants() {
    let mut b = fixture("A A");
    b.set_style(2, P::Width, "80px")
        .set_style(3, P::Color, "#0000ff")
        .set_style(3, P::OutlineOffset, "-4px");
    assert_eq!(pixel(&render(&view(b)), 14, 20), RED);
    let mut b = fixture("A A");
    b.set_style(2, P::Width, "80px")
        .set_style(3, P::OutlineOffset, "-4px")
        .create_element(5, Tag::Span)
        .insert(3, 5, 0)
        .insert(5, 4, 0)
        .set_style(5, P::Outline, "2px solid #0000ff")
        .set_style(5, P::OutlineOffset, "-4px");
    assert_eq!(pixel(&render(&view(b)), 14, 20), RED);
}
#[test]
fn opacity_composites_the_whole_outline_once_and_zero_keeps_targets() {
    let mut b = fixture("A A A A");
    b.set_style(3, P::Opacity, "0.5");
    let mut v = view(b);
    let image = render(&v);
    assert_eq!(pixel(&image, 10, 36), [255, 128, 128, 255]);
    let mut b = B::new();
    b.set_style(3, P::Opacity, "0");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    let image = render(&v);
    assert_eq!(pixel(&image, 10, 36), WHITE);
    assert_eq!(image.hit_test(12.5, 20.5), Some(3));
}
#[test]
fn retained_vector_and_stateless_paths_share_clip_and_dpi_without_expanding_hits() {
    let mut v = view(fixture("A A A A A"));
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for offset in ["0", "3px", "-3px", "-100px"] {
        let mut b = B::new();
        b.set_style(3, P::OutlineOffset, offset);
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        let snapshot = v.snapshot();
        for scale in [1.0, 1.25, 2.0] {
            let expected = v
                .rasterize_scaled(
                    (100.0 * scale) as u32,
                    (120.0 * scale) as u32,
                    scale,
                    Some((&mut font(), 20.0)),
                )
                .unwrap();
            assert_eq!(
                renderer
                    .render(&v, expected.width, expected.height, scale)
                    .unwrap(),
                expected
            );
            let PreparedFrame::Vector { metadata, scene } = renderer
                .prepare(&v, expected.width, expected.height, scale)
                .unwrap()
            else {
                panic!("ordinary outline paths")
            };
            assert_eq!(scene.rasterize(metadata).unwrap(), expected);
        }
        assert_eq!(v.snapshot(), snapshot);
    }
    for style in ["none", "2px solid #ff0000", "none", "2px solid #ff0000"] {
        let mut b = B::new();
        b.set_style(3, P::Outline, style)
            .set_style(3, P::OutlineOffset, "0");
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        let expected = render(&v);
        let actual = renderer.render(&v, 100, 120, 1.0).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            pixel(&actual, 10, 10),
            if style == "none" { WHITE } else { RED }
        );
    }
}

#[test]
fn rounded_fragment_unions_keep_curves_in_vector_output_at_all_three_scales() {
    let mut v = view(fixture("A A A A A"));
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for radius in ["10px", "50%", "40px 10px 40px 5px"] {
        for offset in ["0", "-3px", "3px", "-100px"] {
            let mut b = B::new();
            b.set_style(3, P::BorderRadius, radius)
                .set_style(3, P::OutlineOffset, offset);
            v.commit(&b.finish(), false, &mut Denied).unwrap();
            let snapshot = v.snapshot();
            for scale in [1.0, 1.25, 2.0] {
                let expected = v
                    .rasterize_scaled(
                        (100.0 * scale) as u32,
                        (120.0 * scale) as u32,
                        scale,
                        Some((&mut font(), 20.0)),
                    )
                    .unwrap();
                assert_eq!(
                    renderer
                        .render(&v, expected.width, expected.height, scale)
                        .unwrap(),
                    expected
                );
                let PreparedFrame::Vector { metadata, scene } = renderer
                    .prepare(&v, expected.width, expected.height, scale)
                    .unwrap()
                else {
                    panic!("rounded outlines retain ordinary paths")
                };
                assert_eq!(scene.rasterize(metadata).unwrap(), expected);
            }
            assert_eq!(v.snapshot(), snapshot);
        }
    }
}
#[test]
fn fractional_outline_lengths_match_used_css_values_and_do_not_change_source_styles() {
    let mut a = fixture("A A A A");
    a.set_style(3, P::OutlineWidth, "0.5px")
        .set_style(3, P::OutlineOffset, "-0.25px");
    let mut b = fixture("A A A A");
    b.set_style(3, P::OutlineWidth, "1px")
        .set_style(3, P::OutlineOffset, "-1px");
    let a = view(a);
    let snapshot = a.snapshot();
    let b = view(b);
    assert_eq!(render(&a).pixels, render(&b).pixels);
    assert_eq!(a.snapshot(), snapshot);
}
#[test]
fn containing_block_outline_paints_after_inline_and_block_descendants() {
    for tag in [Tag::Span, Tag::Div] {
        let mut b = B::new();
        b.set_style(1, P::BackgroundColor, "#ffffff")
            .create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::Width, "24px")
            .set_style(2, P::Height, "24px")
            .set_style(2, P::Margin, "12px")
            .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
            .set_style(2, P::FontSize, "20px")
            .set_style(2, P::LineHeight, "24px")
            .set_style(2, P::Color, "transparent")
            .set_style(2, P::Outline, "2px solid #ff0000")
            .set_style(2, P::OutlineOffset, "-4px")
            .create_element(3, tag)
            .insert(2, 3, 0)
            .set_style(3, P::Outline, "2px solid #0000ff")
            .set_style(3, P::OutlineOffset, "-4px")
            .create_text(4, "A A")
            .insert(3, 4, 0);
        assert_eq!(pixel(&render(&view(b)), 14, 20), RED, "{tag:?}");
    }
}
