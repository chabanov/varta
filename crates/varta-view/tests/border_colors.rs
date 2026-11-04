use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, PreparedFrame, Renderer, View};
struct Deny;
impl Broker for Deny {
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
fn commit(v: &mut View, b: &B) {
    v.commit(&b.finish(), false, &mut Deny).unwrap();
}
fn view(b: B) -> View {
    let mut v = View::default();
    commit(&mut v, &b);
    v
}
fn fixture(radius: &str, alpha: &str) -> B {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "60px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::Margin, "10px")
        .set_style(2, P::Border, &format!("8px solid #ff0000{alpha}"))
        .set_style(2, P::BorderRadius, radius)
        .set_style(2, P::BorderRight, &format!("8px solid #0000ff{alpha}"))
        .set_style(2, P::BorderBottom, &format!("8px solid #00ff00{alpha}"))
        .set_style(2, P::BorderLeft, &format!("8px solid #ffff00{alpha}"));
    b
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
#[test]
fn square_and_rounded_distinct_sides_keep_their_colors_and_transparent_hole() {
    for radius in ["0", "12px", "50%"] {
        let v = view(fixture(radius, ""));
        let before = v.snapshot();
        let frame = v.rasterize_boxes(120, 100).unwrap();
        for (x, y, color) in [
            (48, 12, [255, 0, 0, 255]),
            (83, 38, [0, 0, 255, 255]),
            (48, 63, [0, 255, 0, 255]),
            (12, 38, [255, 255, 0, 255]),
            (48, 38, [0; 4]),
        ] {
            assert_eq!(pixel(&frame, x, y), color, "{radius} at{x},{y}");
        }
        assert_eq!(v.snapshot(), before);
        let mut renderer = Renderer::new(None, 16.0).unwrap();
        assert_eq!(renderer.render(&v, 120, 100, 1.0).unwrap(), frame);
        let PreparedFrame::Raster(actual) = renderer.prepare(&v, 120, 100, 1.0).unwrap() else {
            panic!("colored border has explicit CPU fallback")
        };
        assert_eq!(actual, frame);
        assert_eq!(renderer.stats().dirty_pixels, 0);
    }
}
#[test]
fn oversized_zero_width_corner_uses_its_adjacent_color_until_the_opposite_front_arrives() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "100px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::BorderTop, "30px solid #ff0000")
        .set_style(2, P::BorderBottom, "30px solid #0000ff")
        .set_style(2, P::BorderRadius, "90px 0 0 0");
    let v = view(b);
    let image = v.rasterize_boxes(120, 120).unwrap();
    assert_eq!(pixel(&image, 10, 60), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 10, 80), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 96, 60), [0; 4]);
}
#[test]
fn colored_seams_preserve_common_contour_alpha_without_an_opaque_underpaint() {
    for radius in ["0", "20px", "90px 0 0 0", "50%"] {
        let mut b = fixture(radius, "80");
        b.set_style(2, P::Margin, "10.25px")
            .set_style(2, P::Width, "160px")
            .set_style(2, P::Height, "100px");
        let mut v = view(b);
        for scale in [1.0, 1.25, 2.0] {
            let (w, h) = ((210.0 * scale) as u32, (150.0 * scale) as u32);
            let mixed = v.rasterize_scaled(w, h, scale, None).unwrap();
            let mut common = B::new();
            for property in [P::BorderRight, P::BorderBottom, P::BorderLeft] {
                common.set_style(2, property, "8px solid #ff000080");
            }
            commit(&mut v, &common);
            let reference = v.rasterize_scaled(w, h, scale, None).unwrap();
            assert!(mixed.same_input_geometry(&reference));
            for (index, (a, b)) in mixed
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(reference.pixels.as_chunks::<4>().0)
                .enumerate()
            {
                assert!(
                    a[3].abs_diff(b[3]) <= 1,
                    "{radius} at{scale} pixel{}/{}: alpha{a:?}/{b:?}",
                    index % w as usize,
                    index / w as usize
                );
                assert!(
                    a[0] <= a[3] && a[1] <= a[3] && a[2] <= a[3],
                    "premultiplied color"
                );
            }
            let mut colors = B::new();
            colors
                .set_style(2, P::BorderRight, "8px solid #0000ff80")
                .set_style(2, P::BorderBottom, "8px solid #00ff0080")
                .set_style(2, P::BorderLeft, "8px solid #ffff0080");
            commit(&mut v, &colors);
        }
    }
}
#[test]
fn retained_color_mutations_keep_damage_reuse_and_recover_ordinary_vector_painting() {
    let mut v = view(fixture("20px", "80"));
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let PreparedFrame::Raster(first) = renderer.prepare(&v, 160, 120, 1.0).unwrap() else {
        panic!("colored border fallback")
    };
    assert_eq!(first, v.rasterize_boxes(160, 120).unwrap());
    let PreparedFrame::Raster(repeated) = renderer.prepare(&v, 160, 120, 1.0).unwrap() else {
        panic!("same fallback")
    };
    assert_eq!(first, repeated);
    assert_eq!(renderer.stats().dirty_pixels, 0);
    let mut b = B::new();
    b.set_style(2, P::BorderRight, "8px solid #ff00ff40");
    commit(&mut v, &b);
    let PreparedFrame::Raster(changed) = renderer.prepare(&v, 160, 120, 1.0).unwrap() else {
        panic!("mutated fallback")
    };
    assert_eq!(changed, v.rasterize_boxes(160, 120).unwrap());
    assert_ne!(changed.pixels, first.pixels);
    assert!(renderer.stats().dirty_pixels > 0 && renderer.stats().dirty_pixels < 160 * 120);
    for p in [P::BorderRight, P::BorderBottom, P::BorderLeft] {
        b.set_style(2, p, "8px solid #ff000080");
    }
    commit(&mut v, &b);
    let PreparedFrame::Vector { metadata, scene } = renderer.prepare(&v, 160, 120, 1.0).unwrap()
    else {
        panic!("common colors recover vector drawing")
    };
    let reference = v.rasterize_boxes(160, 120).unwrap();
    assert_eq!(scene.rasterize(metadata).unwrap(), reference);
    assert_eq!(renderer.render(&v, 160, 120, 1.0).unwrap(), reference);
}

#[test]
fn diagonal_seam_integrates_different_alphas_once_in_premultiplied_space() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "20px")
        .set_style(2, P::Height, "20px")
        .set_style(2, P::BorderTop, "8px solid #ff000080")
        .set_style(2, P::BorderLeft, "8px solid #0000ff40");
    let image = view(b).rasterize_boxes(40, 40).unwrap();
    assert_eq!(
        pixel(&image, 4, 4),
        [64, 0, 32, 96],
        "half red/half blue coverage, no SrcOver seam or underpaint"
    );
    assert_eq!(pixel(&image, 5, 4), [128, 0, 0, 128]);
    assert_eq!(pixel(&image, 4, 5), [0, 0, 64, 64]);
}
