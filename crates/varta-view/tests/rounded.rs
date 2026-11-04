//! Rounded backgrounds share their contour with hit testing and retained drawing.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Renderer, View};
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
fn base(radius: &str) -> B {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#00ff00")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "40px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::Margin, "10px")
        .set_style(2, P::BorderRadius, radius)
        .set_style(2, P::BackgroundColor, "#ff0000");
    b
}
fn view(b: B) -> View {
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
#[test]
fn circle_cutouts_and_pointer_targets_match_at_both_scales() {
    for radius in ["50%", "999px", "999999999999999999999999999999%"] {
        let v = view(base(radius));
        let snapshot = v.snapshot();
        for scale in [1.0, 2.0] {
            let image = v
                .rasterize_scaled((80.0 * scale) as u32, (80.0 * scale) as u32, scale, None)
                .unwrap();
            assert_eq!(
                pixel(&image, (11.0 * scale) as u32, (11.0 * scale) as u32),
                [0, 255, 0, 255]
            );
            assert_eq!(
                pixel(&image, (30.0 * scale) as u32, (30.0 * scale) as u32),
                [255, 0, 0, 255]
            );
            assert_eq!(image.hit_test(11.0 * scale, 11.0 * scale), None);
            assert_eq!(image.hit_test(30.0 * scale, 30.0 * scale), Some(2));
        }
        assert_eq!(v.snapshot(), snapshot);
    }
}
#[test]
fn radius_does_not_clip_visible_descendant_boxes() {
    let mut b = base("50%");
    b.create_element(3, Tag::Div)
        .insert(2, 3, 0)
        .set_style(3, P::Width, "10px")
        .set_style(3, P::Height, "10px")
        .set_style(3, P::BackgroundColor, "#0000ff");
    let image = view(b).rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&image, 11, 11), [0, 0, 255, 255]);
    assert_eq!(image.hit_test(11.0, 11.0), Some(3));
}
#[test]
fn rectangular_ancestor_clip_preserves_full_rounded_contour() {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#00ff00")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "20px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::Margin, "10px")
        .set_style(2, P::Overflow, "hidden")
        .create_element(3, Tag::Div)
        .insert(2, 3, 0)
        .set_style(3, P::Width, "40px")
        .set_style(3, P::Height, "40px")
        .set_style(3, P::BorderRadius, "50%")
        .set_style(3, P::BackgroundColor, "#ff0000");
    let image = view(b).rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&image, 11, 11), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 29, 30), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 31, 30), [0, 255, 0, 255]);
    assert_eq!(image.hit_test(11.0, 11.0), Some(2));
    assert_eq!(image.hit_test(29.0, 30.0), Some(3));
    assert_eq!(image.hit_test(31.0, 30.0), None);
}
#[test]
fn dynamic_radii_preserve_reference_retained_and_vector_pixels() {
    let mut v = view(base("50%"));
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for radius in ["50%", "4px 12px 2px 8px", "0", "999px"] {
        let mut b = B::new();
        b.set_style(2, P::BorderRadius, radius);
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        for scale in [1.0, 2.0] {
            let width = (80.0 * scale) as u32;
            let expected = v.rasterize_scaled(width, width, scale, None).unwrap();
            let actual = renderer.render(&v, width, width, scale).unwrap();
            assert_eq!(actual, expected, "radius {radius} at {scale}");
            let varta_view::PreparedFrame::Vector { metadata, scene } =
                renderer.prepare(&v, width, width, scale).unwrap()
            else {
                panic!("bounded rounded vector scene");
            };
            assert_eq!(scene.rasterize(metadata).unwrap(), expected);
        }
    }
}

fn equivalent_consumers(v: &View, width: u32, height: u32) {
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 2.0] {
        let (w, h) = (
            (width as f32 * scale) as u32,
            (height as f32 * scale) as u32,
        );
        let expected = v.rasterize_scaled(w, h, scale, None).unwrap();
        assert_eq!(renderer.render(v, w, h, scale).unwrap(), expected);
        let varta_view::PreparedFrame::Vector { metadata, scene } =
            renderer.prepare(v, w, h, scale).unwrap()
        else {
            panic!("bounded rounded vector scene");
        };
        assert_eq!(scene.rasterize(metadata).unwrap(), expected);
    }
}
#[test]
fn translucent_rounded_border_keeps_its_hole_and_underlying_background() {
    let mut b = base("50%");
    b.set_style(2, P::BackgroundColor, "transparent").set_style(
        2,
        P::Border,
        "6px solid #ff000080",
    );
    let v = view(b);
    let image = v.rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&image, 36, 36), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 11, 11), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 12, 36), [128, 127, 0, 255]);
    equivalent_consumers(&v, 80, 80);
}
#[test]
fn oversized_asymmetric_inner_arc_does_not_punch_through_the_opposite_border() {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#00ff00")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "70px")
        .set_style(2, P::Height, "70px")
        .set_style(2, P::Margin, "2px")
        .set_style(2, P::BorderRadius, "0 100% 0 0")
        .set_style(2, P::Border, "30px solid #ff0000");
    let v = view(b);
    let image = v.rasterize_boxes(140, 140).unwrap();
    assert_eq!(pixel(&image, 33, 33), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 52, 92), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 22, 92), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 131, 3), [0, 255, 0, 255]);
    equivalent_consumers(&v, 140, 140);
}
#[test]
fn rounded_outline_follows_the_curve_without_expanding_hit_regions() {
    let mut b = base("50%");
    b.set_style(2, P::BackgroundColor, "transparent")
        .set_style(2, P::Outline, "4px solid #0000ff")
        .set_style(2, P::OutlineOffset, "3px");
    let v = view(b);
    let image = v.rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&image, 3, 3), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 5, 30), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 8, 30), [0, 255, 0, 255]);
    assert_eq!(image.hit_test(5.0, 30.0), None);
    assert_eq!(image.hit_test(11.0, 11.0), None);
    assert_eq!(image.hit_test(10.0, 30.0), Some(2));
    equivalent_consumers(&v, 80, 80);
}
#[test]
fn extremely_negative_outline_offsets_keep_the_per_axis_minimum_visible_shape() {
    for radius in ["0", "10px"] {
        let mut b = base(radius);
        b.set_style(2, P::BackgroundColor, "transparent")
            .set_style(2, P::Height, "20px")
            .set_style(2, P::Outline, "4px solid #0000ff")
            .set_style(2, P::OutlineOffset, "-1000px");
        let v = view(b);
        let image = v.rasterize_boxes(80, 60).unwrap();
        assert_eq!(pixel(&image, 30, 20), [0, 0, 255, 255]);
        assert_eq!(pixel(&image, 25, 20), [0, 255, 0, 255]);
        assert_eq!(pixel(&image, 34, 20), [0, 255, 0, 255]);
        equivalent_consumers(&v, 80, 60);
    }
}

fn clipped_base(overflow: &str) -> B {
    let mut b = base("50%");
    b.set_style(2, P::Overflow, overflow)
        .create_element(3, Tag::Div)
        .insert(2, 3, 0)
        .set_style(3, P::Width, "40px")
        .set_style(3, P::Height, "40px")
        .set_style(3, P::BackgroundColor, "#0000ff");
    b
}
#[test]
fn rounded_overflow_clips_descendant_pixels_and_hit_regions_at_both_scales() {
    let v = view(clipped_base("hidden"));
    for scale in [1.0, 2.0] {
        let size = (80.0 * scale) as u32;
        let image = v.rasterize_scaled(size, size, scale, None).unwrap();
        assert_eq!(
            pixel(&image, (11.0 * scale) as u32, (11.0 * scale) as u32),
            [0, 255, 0, 255]
        );
        assert_eq!(
            pixel(&image, (30.0 * scale) as u32, (30.0 * scale) as u32),
            [0, 0, 255, 255]
        );
        assert_eq!(image.hit_test(11.0 * scale, 11.0 * scale), None);
        assert_eq!(image.hit_test(30.0 * scale, 30.0 * scale), Some(3));
    }
    equivalent_consumers(&v, 80, 80);
}
#[test]
fn padding_corner_clip_preserves_the_rounded_border() {
    let mut b = clipped_base("hidden");
    b.set_style(2, P::Border, "6px solid #000000");
    let v = view(b);
    let image = v.rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&image, 18, 18), [0, 0, 0, 255]);
    assert_eq!(pixel(&image, 36, 36), [0, 0, 255, 255]);
    assert_eq!(image.hit_test(18.0, 18.0), Some(2));
    assert_eq!(image.hit_test(36.0, 36.0), Some(3));
    equivalent_consumers(&v, 80, 80);
}
#[test]
fn nested_corner_clips_intersect_without_leaking_into_later_siblings() {
    let mut b = clipped_base("hidden");
    b.set_style(3, P::Margin, "10px")
        .set_style(3, P::BorderRadius, "50%")
        .set_style(3, P::Overflow, "hidden")
        .create_element(4, Tag::Div)
        .insert(3, 4, 0)
        .set_style(4, P::Width, "40px")
        .set_style(4, P::Height, "40px")
        .set_style(4, P::BackgroundColor, "#ff00ff")
        .create_element(5, Tag::Div)
        .insert(1, 5, 0)
        .set_style(5, P::Width, "80px")
        .set_style(5, P::Height, "10px")
        .set_style(5, P::BackgroundColor, "#ffff00");
    let v = view(b);
    let image = v.rasterize_boxes(80, 120).unwrap();
    assert_eq!(pixel(&image, 21, 21), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 40, 40), [255, 0, 255, 255]);
    assert_eq!(pixel(&image, 49, 40), [0, 255, 0, 255]);
    assert_eq!(image.hit_test(21.0, 21.0), Some(2));
    assert_eq!(image.hit_test(40.0, 40.0), Some(4));
    assert_eq!(image.hit_test(49.0, 40.0), None);
    let boxes = v.layout_boxes(80.0, 120.0).unwrap();
    let sibling = boxes[&5];
    assert_eq!(
        pixel(&image, (sibling.x + 1.0) as u32, (sibling.y + 1.0) as u32),
        [255, 255, 0, 255]
    );
    assert_eq!(image.hit_test(sibling.x + 1.0, sibling.y + 1.0), Some(5));
    equivalent_consumers(&v, 80, 120);
}
#[test]
fn changing_overflow_rebuilds_pixels_and_input_geometry_without_stale_clips() {
    let mut v = view(clipped_base("hidden"));
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let initial = renderer.render(&v, 80, 80, 1.0).unwrap();
    for overflow in ["visible", "auto", "hidden", "visible", "hidden"] {
        let mut b = B::new();
        b.set_style(2, P::Overflow, overflow);
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        let image = renderer.render(&v, 80, 80, 1.0).unwrap();
        let visible = overflow == "visible";
        assert_eq!(
            image.hit_test(11.0, 11.0),
            if visible { Some(3) } else { None }
        );
        if overflow == "hidden" {
            assert!(image.same_input_geometry(&initial), "restored hidden clip");
        }
        if visible {
            assert!(!image.same_input_geometry(&initial));
        }
        // Auto and hidden differ in user-scroll metadata even at offset zero.
        assert_eq!(image, v.rasterize_boxes(80, 80).unwrap());
    }
}
