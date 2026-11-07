//! Generated linear images, premultiplied interpolation and retained composition.
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
fn view(image: &str, extra: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1,&format!(".box{{position:absolute;left:10px;top:10px;width:100px;height:20px;background-image:{image};{extra}}}"))
        .create_element(2,Tag::Div).set_class(2,"box").insert(1,2,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v
}
fn pixel(v: &View, x: u32, y: u32) -> [u8; 4] {
    let image = v.rasterize_boxes(160, 100).unwrap();
    let at = ((y * image.width + x) * 4) as usize;
    image.pixels[at..at + 4].try_into().unwrap()
}
#[test]
fn linear_backgrounds_interpolate_in_the_selected_space_and_premultiply_alpha() {
    let v = view("linear-gradient(to right,red,blue)", "");
    let p = pixel(&v, 59, 20);
    assert!((127..=130).contains(&p[0]));
    assert!((125..=128).contains(&p[2]));
    assert_eq!(p[1], 0);
    assert_eq!(p[3], 255);
    let lab = view("linear-gradient(90deg in oklab,red,blue)", "");
    let p = pixel(&lab, 59, 20);
    assert!((135..=145).contains(&p[0]));
    assert!((78..=88).contains(&p[1]));
    assert!((157..=166).contains(&p[2]));
    let transparent = view("linear-gradient(to right,rgba(255,0,0,0),blue)", "");
    let p = pixel(&transparent, 59, 20);
    assert_eq!(p[0], 0);
    assert_eq!(p[1], 0);
    assert!((125..=128).contains(&p[2]));
    assert_eq!(p[2], p[3]);
}
#[test]
fn stops_repeat_under_transparent_borders_and_keep_hard_transitions() {
    let v = view(
        "linear-gradient(to right,red 0% 50%,blue 50% 100%)",
        "border:2px solid transparent",
    );
    assert_eq!(pixel(&v, 11, 20), [0, 0, 255, 255]);
    assert_eq!(pixel(&v, 20, 20), [255, 0, 0, 255]);
    assert_eq!(pixel(&v, 70, 20), [0, 0, 255, 255]);
}
#[test]
fn rounded_transformed_opacity_gradients_keep_retained_reference_and_damage() {
    let mut v = view(
        "linear-gradient(to bottom in oklab,red,blue)",
        "opacity:.5;border-radius:8px;transform:scale(1.25)",
    );
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let image = renderer.render(&v, 200, 150, scale).unwrap();
        assert_eq!(
            image.pixels,
            v.rasterize_scaled(200, 150, scale, None).unwrap().pixels
        );
        assert_eq!(
            renderer.render(&v, 200, 150, scale).unwrap().pixels,
            image.pixels
        );
        assert_eq!(renderer.stats().dirty_pixels, 0);
        assert!(matches!(
            renderer.prepare(&v, 200, 150, scale).unwrap(),
            PreparedFrame::Raster(_)
        ));
    }
    let before = renderer.render(&v, 200, 150, 1.0).unwrap();
    let mut b = B::new();
    b.set_style(2, P::BackgroundImage, "none");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let after = renderer.render(&v, 200, 150, 1.0).unwrap();
    assert_ne!(before.pixels, after.pixels);
}
