//! Atomic inline contents retain their own layout, baseline, input and pixels.
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
fn scene(display: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1,&format!(".flow{{width:120px;font-family:'Varta Case';font-size:16px;line-height:20px}}.atom{{display:{display};padding:2px;border:1px solid black;background-color:yellow;gap:4px}}.cell{{background-color:red}}"))
        .create_element(2,Tag::Div).set_class(2,"flow").insert(1,2,0)
        .create_text(3,"aaa ").insert(2,3,0)
        .create_element(4,Tag::Span).set_class(4,"atom").insert(2,4,0)
        .create_element(5,Tag::Span).set_class(5,"cell").insert(4,5,0).create_text(6,"aaa").insert(5,6,0)
        .create_element(7,Tag::Span).set_class(7,"cell").insert(4,7,0).create_text(8,"aaa").insert(7,8,0)
        .create_text(9," aaa").insert(2,9,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v
}
#[test]
fn inline_block_and_flex_keep_atomic_width_and_render_their_children() {
    for (display, width) in [("inline-block", 54.0), ("inline-flex", 58.0)] {
        let v = scene(display);
        let boxes = v
            .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
            .unwrap();
        assert_eq!(boxes[&4].width, width, "{display}");
        assert!(boxes[&5].x >= boxes[&4].x + 3.0);
        assert!(boxes[&7].x >= boxes[&5].x + boxes[&5].width);
        let mut renderer = Renderer::new(Some(font()), 16.0).unwrap();
        let image = renderer.render(&v, 200, 200, 1.0).unwrap();
        assert_eq!(
            image.pixels,
            v.rasterize_with_text(200, 200, &mut font(), 16.0)
                .unwrap()
                .pixels
        );
        assert!(
            image
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] == 255 && p[1] == 0 && p[2] == 0 && p[3] == 255)
        );
        assert_eq!(
            renderer.render(&v, 200, 200, 1.0).unwrap().pixels,
            image.pixels
        );
        assert_eq!(renderer.stats().dirty_pixels, 0);
    }
}
#[test]
fn atomic_boxes_wrap_as_whole_units_and_follow_changed_content() {
    let mut v = scene("inline-block");
    let mut b = B::new();
    b.set_style(2, P::Width, "60px");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let boxes = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert!(boxes[&4].y > boxes[&3].y);
    assert_eq!(boxes[&4].width, 54.0);
    let mut b = B::new();
    b.set_text(6, "aaa aaa");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let changed = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert_eq!(changed[&4].width, 60.0);
    assert!(changed[&4].height > boxes[&4].height);
}
