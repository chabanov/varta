//! Rounded decoration rasterization must not quantize layout or input geometry.
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
fn circle(margin: &str) -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "24px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::Margin, margin)
        .set_style(2, P::BorderRadius, "50%")
        .set_style(2, P::BackgroundColor, "#ff0000");
    let mut v = View::default();
    commit(&mut v, b);
    v
}
#[test]
fn decoration_rounds_before_dpi_while_bounds_and_targets_retain_fractional_coordinates() {
    for scale in [1.0, 1.25, 2.0] {
        let side = (80.0 * scale) as u32;
        let reference = circle("3px")
            .rasterize_scaled(side, side, scale, None)
            .unwrap();
        for (raw, coordinate) in [("3.25px", 3.25), ("3.49px", 3.49)] {
            let v = circle(raw);
            let before = v.snapshot();
            let geometry = v.layout_boxes(80.0, 80.0).unwrap();
            assert_eq!(geometry[&2].x, coordinate);
            let expected = v.rasterize_scaled(side, side, scale, None).unwrap();
            assert_eq!(expected.pixels, reference.pixels);
            assert_eq!(expected.node_bounds_map()[&2].x, coordinate * scale);
            assert_eq!(expected.hit_test(3.125 * scale, 23.0 * scale), None);
            let mut renderer = Renderer::new(None, 16.0).unwrap();
            assert_eq!(renderer.render(&v, side, side, scale).unwrap(), expected);
            let PreparedFrame::Vector { metadata, scene } =
                renderer.prepare(&v, side, side, scale).unwrap()
            else {
                panic!("decoration remains a vector operation")
            };
            assert_eq!(scene.rasterize(metadata).unwrap(), expected);
            assert_eq!(v.snapshot(), before);
        }
        assert_ne!(
            circle("3.51px")
                .rasterize_scaled(side, side, scale, None)
                .unwrap()
                .pixels,
            reference.pixels
        );
    }
}
#[test]
fn collapsed_paint_clip_keeps_its_logical_pointer_region() {
    let mut v = circle("3.25px");
    let mut b = B::new();
    b.set_style(2, P::Width, "0.03125px")
        .set_style(2, P::Overflow, "hidden")
        .set_style(2, P::BackgroundColor, "transparent")
        .create_element(3, Tag::Div)
        .insert(2, 3, 0)
        .set_style(3, P::Width, "10px")
        .set_style(3, P::Height, "40px")
        .set_style(3, P::BackgroundColor, "#0000ff");
    commit(&mut v, b);
    for scale in [1.0, 1.25, 2.0] {
        let side = (80.0 * scale) as u32;
        let image = v.rasterize_scaled(side, side, scale, None).unwrap();
        assert!(image.pixels.iter().all(|&v| v == 255));
        assert_eq!(image.hit_test(3.265625 * scale, 23.25 * scale), Some(3));
        assert_eq!(image.node_bounds_map()[&3].width, 0.03125 * scale);
        let mut renderer = Renderer::new(None, 16.0).unwrap();
        assert_eq!(renderer.render(&v, side, side, scale).unwrap(), image);
    }
}
#[test]
fn glyph_positions_keep_subpixel_offsets_when_decoration_edges_snap() {
    let font = || {
        TextMeasurer::new(
            include_bytes!("fixtures/inline.ttf").to_vec(),
            "Varta Inline Fixture",
        )
        .unwrap()
    };
    let mut images = Vec::new();
    for margin in ["3px", "3.25px"] {
        let mut v = circle(margin);
        let mut b = B::new();
        b.set_style(2, P::BackgroundColor, "transparent")
            .set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
            .set_style(2, P::FontSize, "20px")
            .set_style(2, P::LineHeight, "20px")
            .create_text(3, "A")
            .insert(2, 3, 0);
        commit(&mut v, b);
        images.push(
            v.rasterize_with_text(80, 80, &mut font(), 20.0)
                .unwrap()
                .pixels,
        );
    }
    assert_ne!(images[0], images[1]);
}
#[test]
fn implicit_canvas_keeps_its_physical_far_edges_at_nonintegral_dpi() {
    let mut v = View::default();
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#00ff00");
    commit(&mut v, b);
    for scale in [1.3, 1.7, 2.3] {
        let image = v.rasterize_scaled(101, 79, scale, None).unwrap();
        assert!(
            image
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p == &[0, 255, 0, 255])
        );
    }
}

#[test]
fn paragraph_vertical_paint_origin_rounds_without_quantizing_logical_text_geometry() {
    let font = || {
        TextMeasurer::new(
            include_bytes!("fixtures/inline.ttf").to_vec(),
            "Varta Inline Fixture",
        )
        .unwrap()
    };
    for rich in [false, true] {
        let mut v = circle("3px");
        let mut b = B::new();
        b.set_style(2, P::BackgroundColor, "transparent")
            .set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
            .set_style(2, P::FontSize, "20px")
            .set_style(2, P::LineHeight, "20px")
            .create_text(3, "A A A A")
            .insert(2, 3, 0);
        if rich {
            b.create_element(4, Tag::Span)
                .insert(2, 4, 0)
                .insert(4, 3, 0);
        }
        commit(&mut v, b);
        let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
        for scale in [1.0, 1.25, 2.0] {
            let side = (80.0 * scale) as u32;
            let mut b = B::new();
            b.set_style(2, P::MarginTop, "3px");
            commit(&mut v, b);
            let reference = v
                .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
                .unwrap();
            for (raw, y) in [("3.25px", 3.25), ("3.49px", 3.49)] {
                let mut b = B::new();
                b.set_style(2, P::MarginTop, raw);
                commit(&mut v, b);
                let before = v.snapshot();
                let image = v
                    .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
                    .unwrap();
                assert_eq!(
                    image.pixels, reference.pixels,
                    "rich{rich} scale{scale} top{raw}"
                );
                assert!((image.node_bounds_map()[&2].y - y * scale).abs() < 0.00001);
                assert_ne!(image.node_bounds_map(), reference.node_bounds_map());
                assert_eq!(renderer.render(&v, side, side, scale).unwrap(), image);
                let PreparedFrame::Vector { metadata, scene } =
                    renderer.prepare(&v, side, side, scale).unwrap()
                else {
                    panic!("paragraph is vector")
                };
                assert_eq!(scene.rasterize(metadata).unwrap(), image);
                assert_eq!(v.snapshot(), before);
            }
            let mut b = B::new();
            b.set_style(2, P::MarginTop, "3.51px");
            commit(&mut v, b);
            let moved = v
                .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
                .unwrap();
            assert_ne!(moved.pixels, reference.pixels);
        }
    }
}
