//! Outer/inset shadow paint, knockout, geometry, clipping and retained damage.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, PreparedFrame, RasterImage, Renderer, View};
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
fn view(sheet: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1, sheet)
        .create_element(2, Tag::Div)
        .set_class(2, "box")
        .insert(1, 2, 0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v
}
fn pixel(image: &RasterImage, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * image.width + x) * 4) as usize;
    image.pixels[at..at + 4].try_into().unwrap()
}
const BOX: &str = ".box { position: absolute; left: 20px; top: 20px; width: 20px; height: 20px; ";
#[test]
fn outer_ring_preserves_transparency_geometry_and_pointer_extent() {
    let v = view(&format!("{BOX} box-shadow: 0 0 0 4px red; }}"));
    let frame = v.rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&frame, 18, 30), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 30, 30), [0; 4]);
    assert_eq!(pixel(&frame, 14, 30), [0; 4]);
    assert_eq!(frame.hit_test(18.0, 30.0), None);
    assert_eq!(frame.hit_test(30.0, 30.0), Some(2));
    assert_eq!(v.layout_boxes(80.0, 80.0).unwrap()[&2].width, 20.0);
    let empty = view(&format!(
        "{BOX} border-radius: 8px; box-shadow: 0 0 red; }}"
    ));
    assert!(
        empty
            .rasterize_boxes(80, 80)
            .unwrap()
            .pixels
            .iter()
            .all(|v| *v == 0)
    );
}
#[test]
fn front_to_back_lists_inset_padding_and_background_order_are_preserved() {
    let v = view(&format!(
        "{BOX} background-color: blue; border: 2px solid green; box-shadow: inset 4px 0 red, 0 0 0 2px red, 0 0 0 4px blue; }}"
    ));
    let frame = v.rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&frame, 17, 30), [0, 0, 255, 255]);
    assert_eq!(pixel(&frame, 19, 30), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 20, 30), [0, 128, 0, 255]);
    assert_eq!(pixel(&frame, 23, 30), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 30, 30), [0, 0, 255, 255]);
}
#[test]
fn gaussian_blur_spreads_without_filling_the_originating_box() {
    let v = view(&format!("{BOX} box-shadow: 0 0 8px black; }}"));
    let frame = v.rasterize_boxes(80, 80).unwrap();
    // Gaussian sigma=4: at 0.5 px outside a straight edge alpha≈0.45.
    assert!((108..=120).contains(&pixel(&frame, 19, 30)[3]));
    assert!(pixel(&frame, 16, 30)[3] < pixel(&frame, 19, 30)[3]);
    assert_eq!(pixel(&frame, 30, 30), [0; 4]);
    assert!(pixel(&frame, 19, 19)[3] < pixel(&frame, 19, 30)[3]);
}
#[test]
fn retained_reference_opacity_scaled_and_software_preparation_agree() {
    let mut v = view(&format!(
        "{BOX} border-radius: 7px; background-color: blue; opacity: .5; transform: scale(1.5); box-shadow: 3px -2px 5px 2px red, inset 1px 0 2px #0008; }}"
    ));
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let reference = v.rasterize_scaled(100, 100, scale, None).unwrap();
        let retained = renderer.render(&v, 100, 100, scale).unwrap();
        assert_eq!(retained.pixels, reference.pixels);
        let again = renderer.render(&v, 100, 100, scale).unwrap();
        assert_eq!(retained.pixels, again.pixels);
        assert_eq!(renderer.stats().dirty_pixels, 0);
        match renderer.prepare(&v, 100, 100, scale).unwrap() {
            PreparedFrame::Raster(frame) => assert_eq!(frame.pixels, reference.pixels),
            _ => panic!("shadow GPU fallback is explicit"),
        }
    }
    let before = renderer.render(&v, 100, 100, 1.0).unwrap();
    let mut b = B::new();
    b.set_style(2, P::BoxShadow, "none");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let after = renderer.render(&v, 100, 100, 1.0).unwrap();
    assert_ne!(before.pixels, after.pixels);
    assert_eq!(after.pixels, v.rasterize_boxes(100, 100).unwrap().pixels);
}

#[test]
fn wrapped_inline_shadows_are_sliced_retained_and_visible_outside_font_boxes() {
    let mut b = B::new();
    b.stylesheet(1, ".flow{width:80px;padding:16px;font-family:'Varta Case';font-size:16px;line-height:24px}span{background-color:blue;border-radius:6px;box-shadow:0 0 0 3px red,2px 3px 4px #0008}")
        .create_element(2, Tag::Div).set_class(2,"flow").insert(1,2,0)
        .create_element(3, Tag::Span).insert(2,3,0)
        .create_text(4,"aaa aaa aaa aaa aaa aaa").insert(3,4,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let font = || {
        varta_view::TextMeasurer::new(
            include_bytes!("../../varta-style/tests/fixtures/case.ttf").to_vec(),
            "Varta Case",
        )
        .unwrap()
    };
    let mut renderer = Renderer::new(Some(font()), 16.0).unwrap();
    let mut measurer = font();
    let reference = v
        .rasterize_with_text(140, 150, &mut measurer, 16.0)
        .unwrap();
    let actual = renderer.render(&v, 140, 150, 1.0).unwrap();
    assert_eq!(reference.pixels, actual.pixels);
    assert!(
        actual
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 0 && p[1] == 0 && p[2] == 0)
    );
    assert_eq!(
        renderer.render(&v, 140, 150, 1.0).unwrap().pixels,
        actual.pixels
    );
    assert_eq!(renderer.stats().dirty_pixels, 0);
}

#[test]
fn fractional_ink_edges_integrate_coverage_once_and_huge_ink_is_clipped() {
    let v = view(&format!("{BOX} box-shadow: 0 0 0 2px red; }}"));
    let image = v.rasterize_scaled(80, 80, 1.25, None).unwrap();
    // CSS x=18 is device x=22.5. The pixel is half covered, not a
    // quarter covered by a second mask of the shadow's own ink bounds.
    assert_eq!(pixel(&image, 22, 37), [128, 0, 0, 128]);
    assert_eq!(pixel(&image, 23, 37), [255, 0, 0, 255]);
    let far = view(&format!("{BOX} box-shadow: 100000000px 0 red; }}"));
    assert!(
        far.rasterize_boxes(80, 80)
            .unwrap()
            .pixels
            .iter()
            .all(|v| *v == 0)
    );
    let wide = view(&format!("{BOX} box-shadow: 0 0 100000000px red; }}"));
    assert!(
        wide.rasterize_boxes(80, 80)
            .unwrap()
            .pixels
            .iter()
            .all(|v| *v == 0)
    );
}

#[test]
fn opacity_group_bounds_include_shadow_ink_across_a_tile_boundary() {
    let v = view(
        ".box{position:absolute;left:18px;top:78px;width:44px;height:36px;background-color:blue;opacity:.5;box-shadow:2px 3px 4px red}",
    );
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let image = renderer.render(&v, 320, 320, 2.0).unwrap();
    // The border ends before device x=128. Its shadow continues into the
    // next tile, so neither group culling nor composition may discard it.
    let color = pixel(&image, 130, 184);
    assert!((30..=40).contains(&color[3]), "{color:?}");
    assert_eq!(color[0], color[3]);
    assert_eq!(&color[1..3], &[0, 0]);
    assert_eq!(
        image.pixels,
        v.rasterize_scaled(320, 320, 2.0, None).unwrap().pixels
    );
}
