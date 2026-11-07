//! Inset clips affect pixels and pointer geometry, preserving source semantics.
use varta_common::view::{Attribute as A, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Renderer, View};
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
fn scene(clip: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1,&format!(".box{{position:absolute;left:10px;top:10px;width:20px;height:20px;background-color:red;clip-path:{clip}}}.child{{position:fixed;left:0;top:0;width:60px;height:60px;background-color:blue}}"))
        .create_element(2,Tag::Div).set_class(2,"box").set_attribute(2,A::AriaLabel,"kept label").insert(1,2,0)
        .create_element(3,Tag::Div).set_class(3,"child").insert(2,3,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v
}
#[test]
fn empty_sr_only_shape_hides_its_ink_and_fixed_descendants_without_erasing_names() {
    let v = scene("inset(50%)");
    let image = v.rasterize_boxes(80, 80).unwrap();
    assert!(image.pixels.iter().all(|v| *v == 0));
    assert_eq!(image.hit_test(15.0, 15.0), None);
    assert_eq!(v.layout_boxes(80.0, 80.0).unwrap()[&2].width, 20.0);
    assert!(v.snapshot().to_string().contains("kept label"));
}
#[test]
fn clips_apply_to_fixed_descendants_and_retained_damage_at_fractional_scale() {
    let mut v = scene("inset(5px 25% 0 -5px)");
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let image = renderer.render(&v, 80, 80, scale).unwrap();
        assert_eq!(
            image.pixels,
            v.rasterize_scaled(80, 80, scale, None).unwrap().pixels
        );
        assert_eq!(image.hit_test(7.0 * scale, 16.0 * scale), Some(3));
        assert_eq!(image.hit_test(3.0 * scale, 16.0 * scale), None);
        assert_eq!(
            renderer.render(&v, 80, 80, scale).unwrap().pixels,
            image.pixels
        );
        assert_eq!(renderer.stats().dirty_pixels, 0);
    }
    let before = renderer.render(&v, 80, 80, 1.0).unwrap();
    let mut b = B::new();
    b.set_style(2, P::ClipPath, "none");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let after = renderer.render(&v, 80, 80, 1.0).unwrap();
    assert_ne!(before.pixels, after.pixels);
    assert_eq!(after.hit_test(3.0, 16.0), Some(3));
}
