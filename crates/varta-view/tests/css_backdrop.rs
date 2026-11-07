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

fn view(extra: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1,&format!(".base{{width:160px;height:100px;background-image:linear-gradient(to right,red 50%,blue 50%)}}.filter{{position:absolute;left:40px;top:10px;width:80px;height:80px;backdrop-filter:blur(8px);{extra}}}"))
 .create_element(2,Tag::Div).set_class(2,"base").insert(1,2,0)
 .create_element(3,Tag::Div).set_class(3,"filter").insert(1,3,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v
}
#[test]
fn backdrop_blurs_prior_content_across_group_tiles_and_keeps_own_ink_sharp() {
    let mut v = view("background-color:rgba(255,255,255,.2);border-radius:8px;opacity:.75");
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let (w, h) = ((160.0 * scale) as u32, (100.0 * scale) as u32);
        let image = renderer.render(&v, w, h, scale).unwrap();
        assert_eq!(
            image.pixels,
            v.rasterize_scaled(w, h, scale, None).unwrap().pixels
        );
        let at = (((50.0 * scale) as u32 * w + (80.0 * scale) as u32) * 4) as usize;
        let p = &image.pixels[at..at + 4];
        assert!(p[0] > 50 && p[2] > 100, "{p:?}");
        assert_eq!(p[3], 255);
        assert_eq!(
            renderer.render(&v, w, h, scale).unwrap().pixels,
            image.pixels
        );
        assert_eq!(renderer.stats().dirty_pixels, 0);
        assert!(matches!(
            renderer.prepare(&v, w, h, scale).unwrap(),
            PreparedFrame::Raster(_)
        ));
    }
    let before = renderer.render(&v, 160, 100, 1.0).unwrap();
    let mut b = B::new();
    b.set_style(
        2,
        P::BackgroundImage,
        "linear-gradient(to right,green 50%,blue 50%)",
    );
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let after = renderer.render(&v, 160, 100, 1.0).unwrap();
    assert_ne!(before.pixels, after.pixels);
    assert_eq!(after.pixels, v.rasterize_boxes(160, 100).unwrap().pixels);
}
#[test]
fn backdrop_alias_obeys_cascade_and_negative_blur_is_refused() {
    let v = view("-webkit-backdrop-filter:blur(0px)");
    let image = v.rasterize_boxes(160, 100).unwrap();
    let at = ((50 * 160 + 79) * 4) as usize;
    assert_eq!(&image.pixels[at..at + 4], &[255, 0, 0, 255]);
    let mut b = B::new();
    b.stylesheet(9, ".x{backdrop-filter:blur(-1px)}");
    let mut v = View::default();
    assert!(v.commit(&b.finish(), false, &mut Deny).is_err());
}

#[test]
fn backdrop_edges_mirror_the_clipped_element_instead_of_sampling_exterior_ink() {
    let mut b = B::new();
    b.stylesheet(1,".base{width:160px;height:100px;background-color:red}.inside,.blur{position:absolute;left:40px;top:10px;width:80px;height:80px}.inside{background-color:blue}.blur{backdrop-filter:blur(8px)}")
      .create_element(2,Tag::Div).set_class(2,"base").insert(1,2,0)
      .create_element(3,Tag::Div).set_class(3,"inside").insert(1,3,0)
      .create_element(4,Tag::Div).set_class(4,"blur").insert(1,4,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let (w, h) = ((160.0 * scale) as u32, (100.0 * scale) as u32);
        let image = renderer.render(&v, w, h, scale).unwrap();
        assert_eq!(
            image.pixels,
            v.rasterize_scaled(w, h, scale, None).unwrap().pixels
        );
        for x in [40, 44, 50, 110, 119] {
            let at = (((50.0 * scale) as u32 * w + (x as f32 * scale) as u32) * 4) as usize;
            assert_eq!(
                &image.pixels[at..at + 4],
                &[0, 0, 255, 255],
                "{scale} at {x}"
            );
        }
    }
}
#[test]
fn a_huge_backdrop_blur_averages_mirror_periods_with_bounded_storage() {
    let mut b = B::new();
    b.stylesheet(1,".base{width:40px;height:30px;background-color:green}.inside,.blur{position:absolute;left:10px;top:5px;width:8px;height:20px}.inside{background-image:linear-gradient(to right,red 50%,blue 50%)}.blur{backdrop-filter:blur(1000000px)}")
      .create_element(2,Tag::Div).set_class(2,"base").insert(1,2,0)
      .create_element(3,Tag::Div).set_class(3,"inside").insert(1,3,0)
      .create_element(4,Tag::Div).set_class(4,"blur").insert(1,4,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let image = v.rasterize_boxes(40, 30).unwrap();
    for x in 10..18 {
        let at = ((15 * 40 + x) * 4) as usize;
        let p = &image.pixels[at..at + 4];
        assert!((127..=128).contains(&p[0]), "{p:?}");
        assert!((127..=128).contains(&p[2]), "{p:?}");
        assert_eq!(p[1], 0);
        assert_eq!(p[3], 255);
    }
}

#[test]
fn backdrop_blur_uses_the_host_root_font_and_accepts_unitless_zero() {
    let mut relative = view("backdrop-filter:blur(.25rem)");
    relative
        .set_environment(
            varta_view::Environment::default()
                .with_root_font_size(32.0)
                .unwrap(),
        )
        .unwrap();
    let mut pixels = view("");
    pixels
        .set_environment(
            varta_view::Environment::default()
                .with_root_font_size(32.0)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        relative.rasterize_boxes(160, 100).unwrap().pixels,
        pixels.rasterize_boxes(160, 100).unwrap().pixels
    );
    let zero = view("backdrop-filter:blur(0)");
    assert!(zero.rasterize_boxes(160, 100).is_ok());
}
