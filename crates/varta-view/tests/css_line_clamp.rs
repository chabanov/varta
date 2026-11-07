//! Legacy vertical line clamping shortens layout and adds an actual ellipsis.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Renderer, TextMeasurer, View};
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
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("../../varta-style/tests/fixtures/case.ttf").to_vec(),
        "Varta Case",
    )
    .unwrap()
}
#[test]
fn line_clamp_keeps_source_names_and_changes_height_and_visible_ink() {
    let text = "aaa aaa aaa aaa aaa aaa aaa aaa";
    let mut b = B::new();
    b.stylesheet(1,".box{display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2;width:60px;font-family:'Varta Case';font-size:16px;line-height:20px}")
        .create_element(2,Tag::Div).set_class(2,"box").insert(1,2,0)
        .create_element(3,Tag::Span).insert(2,3,0).create_text(4,text).insert(3,4,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let mut renderer = Renderer::new(Some(font()), 16.0).unwrap();
    let two = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert_eq!(two[&2].height, 40.0);
    let image = renderer.render(&v, 200, 200, 1.0).unwrap();
    assert!(image.pixels[40 * 200 * 4..].iter().all(|v| *v == 0));
    assert!(v.snapshot().to_string().contains(text));
    assert_eq!(image.hit_test(10.0, 55.0), None);
    let mut b = B::new();
    b.set_style(2, P::WebkitLineClamp, "3");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let three = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert_eq!(three[&2].height, 60.0);
    let changed = renderer.render(&v, 200, 200, 1.0).unwrap();
    assert_ne!(image.pixels, changed.pixels);
    assert_eq!(
        changed.pixels,
        v.rasterize_with_text(200, 200, &mut font(), 16.0)
            .unwrap()
            .pixels
    );
    let mut b = B::new();
    b.set_style(2, P::WebkitLineClamp, "none");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(
        v.layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
            .unwrap()[&2]
            .height,
        80.0
    );
}
