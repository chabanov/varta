//! Non-replaced inline padding paints outside a line without moving its text.
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
fn fixture() -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "24px")
        .set_style(2, P::Padding, "20px")
        .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "24px")
        .set_style(2, P::Color, "transparent")
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::BackgroundColor, "#ff0000")
        .create_text(4, "A A A A A")
        .insert(3, 4, 0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn apply(v: &mut View, property: P, value: &str) {
    let mut b = B::new();
    b.set_style(3, property, value);
    v.commit(&b.finish(), false, &mut Denied).unwrap();
}
fn boxes(v: &View) -> std::collections::BTreeMap<u32, varta_view::BoxRect> {
    v.layout_boxes_with_text(100.0, 140.0, &mut font(), 20.0)
        .unwrap()
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
#[test]
fn vertical_padding_expands_background_and_hit_but_preserves_lines_and_text() {
    let mut v = fixture();
    let old = boxes(&v);
    apply(&mut v, P::Padding, "4px 0 8px");
    let new = boxes(&v);
    for id in [1, 2, 4] {
        assert_eq!(old[&id], new[&id], "node {id}");
    }
    assert_eq!(new[&3].y, old[&3].y - 4.0);
    assert_eq!(new[&3].height, old[&3].height + 12.0);
    let geometry = v
        .layout_geometry_with_text(100.0, 140.0, &mut font(), 20.0)
        .unwrap();
    assert_eq!(geometry[&3].content, old[&3]);
    let image = v.rasterize_with_text(100, 140, &mut font(), 20.0).unwrap();
    assert_eq!(pixel(&image, 21, 18), [255, 0, 0, 255]);
    assert_eq!(image.hit_test(21.5, 18.5), Some(3));
    assert_eq!(image.hit_test(19.5, 18.5), Some(2));
}
#[test]
fn vertical_margins_and_auto_horizontal_margins_do_not_change_inline_geometry() {
    let mut v = fixture();
    let old = boxes(&v);
    for value in ["100px auto -20px", "25% 0 5rem", "-4px 0", "auto"] {
        apply(&mut v, P::Margin, value);
        assert_eq!(boxes(&v), old, "{value}");
    }
}
#[test]
fn percentages_use_containing_content_width_and_cascade_uses_resolved_sides() {
    let mut v = fixture();
    let old = boxes(&v);
    apply(&mut v, P::Padding, "25% 0 50%");
    let new = boxes(&v);
    assert_eq!(new[&3].y, old[&3].y - 6.0);
    assert_eq!(new[&3].height, old[&3].height + 18.0);
    // A shorthand's unsupported horizontal value is overridden by the later side.
    apply(&mut v, P::Padding, "4px 12px");
    apply(&mut v, P::PaddingLeft, "0");
    apply(&mut v, P::PaddingRight, "0");
    let new = boxes(&v);
    assert_eq!(new[&3].y, old[&3].y - 4.0);
    assert_eq!(new[&3].height, old[&3].height + 8.0);
    let mut b = B::new();
    b.remove_style(3, P::PaddingRight);
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    // CSSOM removal resets that expanded side; it does not restore an older
    // shorthand declaration that the longhand replaced.
    assert_eq!(boxes(&v), new);
    apply(&mut v, P::PaddingRight, "1%");
    assert!(
        v.layout_boxes_with_text(100.0, 140.0, &mut font(), 20.0)
            .is_err()
    );
}
#[test]
fn nested_padding_does_not_expand_ancestor_font_boxes_or_descendant_text_hits() {
    let mut v = fixture();
    let old = boxes(&v);
    let mut b = B::new();
    b.create_element(5, Tag::Span)
        .insert(3, 5, 0)
        .insert(5, 4, 0)
        .set_style(5, P::Padding, "8px 0 4px")
        .set_style(5, P::BackgroundColor, "#0000ff");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    let new = boxes(&v);
    assert_eq!(new[&3], old[&3]);
    assert_eq!(new[&4], old[&4]);
    let image = v.rasterize_with_text(100, 140, &mut font(), 20.0).unwrap();
    assert_eq!(image.hit_test(21.5, 14.5), Some(5));
    assert_eq!(pixel(&image, 21, 14), [0, 0, 255, 255]);
}
#[test]
fn padding_mutations_share_retained_and_vector_geometry_with_clips_radii_and_opacity() {
    let mut v = fixture();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for padding in ["4px 0 8px", "25% 0", "0", "6px 0"] {
        apply(&mut v, P::Padding, padding);
        apply(&mut v, P::BorderRadius, "50%");
        apply(&mut v, P::Outline, "2px solid #0000ff");
        for alpha in ["1", "0.5", "0"] {
            apply(&mut v, P::Opacity, alpha);
            let snapshot = v.snapshot();
            for scale in [1.0, 1.25, 2.0] {
                let (width, height) = ((100.0 * scale) as u32, (140.0 * scale) as u32);
                let expected = v
                    .rasterize_scaled(width, height, scale, Some((&mut font(), 20.0)))
                    .unwrap();
                assert_eq!(renderer.render(&v, width, height, scale).unwrap(), expected);
                match renderer.prepare(&v, width, height, scale).unwrap() {
                    PreparedFrame::Vector { metadata, scene } => {
                        assert_eq!(scene.rasterize(metadata).unwrap(), expected)
                    }
                    PreparedFrame::Raster(image) => assert_eq!(image, expected),
                }
            }
            assert_eq!(v.snapshot(), snapshot);
        }
    }
}
#[test]
fn horizontal_percentages_and_excessive_used_padding_fail_explicitly() {
    for (property, value) in [
        (P::Padding, "0 1%"),
        (P::Margin, "0 -1%"),
        (P::Padding, "1000000px 0"),
    ] {
        let mut v = fixture();
        apply(&mut v, property, value);
        assert!(
            v.rasterize_with_text(100, 140, &mut font(), 20.0).is_err(),
            "{value}"
        );
    }
}
