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
fn set(v: &mut View, id: u32, p: P, value: &str) {
    let mut b = B::new();
    b.set_style(id, p, value);
    commit(v, b);
}
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/inline.ttf").to_vec(),
        "Varta Inline Fixture",
    )
    .unwrap()
}
fn fixture(display: &str, nested: bool) -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(10, Tag::Div)
        .insert(1, 10, 0)
        .set_style(10, P::Width, "80px")
        .set_style(10, P::Display, display)
        .set_style(10, P::FontFamily, "\"Varta Inline Fixture\"")
        .set_style(10, P::FontSize, "20px")
        .set_style(10, P::LineHeight, "20px")
        .create_element(2, Tag::Div)
        .insert(10, 2, 0)
        .set_style(2, P::Width, "40px")
        .set_style(2, P::Height, "20px")
        .set_style(2, P::FlexShrink, "0")
        .set_style(2, P::Color, "#000000")
        .create_text(4, "A")
        .insert(2, 4, 0)
        .create_element(3, Tag::Div)
        .insert(if nested { 1 } else { 10 }, 3, 0)
        .set_style(3, P::Width, "40px")
        .set_style(3, P::Height, "20px")
        .set_style(3, P::FlexShrink, "0")
        .set_style(3, P::BackgroundColor, "#0000ff");
    if display == "grid" && !nested {
        b.set_style(10, P::GridTemplateColumns, "40px")
            .set_style(10, P::GridTemplateRows, "20px");
        for id in [2, 3] {
            b.set_style(id, P::GridColumn, "1")
                .set_style(id, P::GridRow, "1");
        }
    } else {
        b.set_style(
            3,
            if display == "flex" && !nested {
                P::MarginLeft
            } else {
                P::MarginTop
            },
            if display == "flex" && !nested {
                "-40px"
            } else {
                "-20px"
            },
        );
    }
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
fn ordinary_block_backgrounds_precede_earlier_inline_ink_and_pointer_regions() {
    for nested in [false, true] {
        let v = fixture("block", nested);
        let before = v.snapshot();
        let image = v.rasterize_with_text(80, 60, &mut font(), 20.0).unwrap();
        assert_eq!(pixel(&image, 3, 6), [0, 0, 0, 255]);
        assert_eq!(image.hit_test(3.25, 6.25), Some(2));
        assert_eq!(image.hit_test(30.25, 6.25), Some(3));
        assert_eq!(v.snapshot(), before);
    }
}
#[test]
fn flex_and_grid_items_paint_atomically_while_opacity_descendants_lift() {
    for display in ["flex", "grid"] {
        let mut v = fixture(display, false);
        let image = v.rasterize_with_text(80, 60, &mut font(), 20.0).unwrap();
        assert_eq!(pixel(&image, 3, 6), [0, 0, 255, 255]);
        assert_eq!(image.hit_test(3.25, 6.25), Some(3));
        let mut b = B::new();
        b.create_element(5, Tag::Span)
            .insert(2, 5, 0)
            .insert(5, 4, 0)
            .set_style(5, P::Opacity, "0.5");
        commit(&mut v, b);
        let lifted = v.rasterize_with_text(80, 60, &mut font(), 20.0).unwrap();
        assert!(pixel(&lifted, 3, 6)[2].abs_diff(128) <= 1);
        assert_eq!(&pixel(&lifted, 3, 6)[..2], &[0, 0]);
        assert_eq!(lifted.hit_test(3.25, 6.25), Some(5));
        set(&mut v, 5, P::Opacity, "0");
        let hidden = v.rasterize_with_text(80, 60, &mut font(), 20.0).unwrap();
        assert_eq!(pixel(&hidden, 3, 6), [0, 0, 255, 255]);
        assert_eq!(hidden.hit_test(3.25, 6.25), Some(5));
    }
}
#[test]
fn deferred_content_preserves_nonisolating_ancestor_curves_and_retained_mutation() {
    let mut v = fixture("block", true);
    set(&mut v, 10, P::Width, "40px");
    set(&mut v, 10, P::BorderRadius, "50%");
    set(&mut v, 10, P::Overflow, "hidden");
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for alpha in ["1", "0.5", "0", "1"] {
        set(&mut v, 10, P::Opacity, alpha);
        let expected = v.rasterize_with_text(80, 60, &mut font(), 20.0).unwrap();
        assert_eq!(expected.hit_test(3.25, 6.25), Some(2));
        assert_eq!(expected.hit_test(0.25, 6.25), Some(3));
        assert_eq!(pixel(&expected, 0, 6), [0, 0, 255, 255]);
        assert_eq!(renderer.render(&v, 80, 60, 1.0).unwrap(), expected);
        assert_eq!(renderer.render(&v, 80, 60, 1.0).unwrap(), expected);
        assert_eq!(renderer.stats().dirty_pixels, 0);
        let actual = match renderer.prepare(&v, 80, 60, 1.0).unwrap() {
            PreparedFrame::Raster(r) => r,
            PreparedFrame::Vector { metadata, scene } => scene.rasterize(metadata).unwrap(),
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn decorated_inline_lines_keep_source_order_without_an_opacity_context() {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "24px")
        .set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "10px")
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::BackgroundColor, "#ff0000")
        .set_style(3, P::Color, "#000000")
        .create_text(4, "A A A A")
        .insert(3, 4, 0);
    let mut v = View::default();
    commit(&mut v, b);
    // A near-unit real context uses the same source ordering. Rounded byte
    // opacity is identical, while the baseline has no isolating context at all.
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        set(&mut v, 3, P::Opacity, "1");
        let side = (80.0 * scale) as u32;
        let plain = v
            .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
            .unwrap();
        assert_eq!(renderer.render(&v, side, side, scale).unwrap(), plain);
        set(&mut v, 3, P::Opacity, "0.999999");
        let context = v
            .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
            .unwrap();
        assert_eq!(plain.pixels, context.pixels);
        assert_eq!(plain.node_bounds_map(), context.node_bounds_map());
    }
}
