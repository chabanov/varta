//! CSS `transform` translations: the box and its content move for paint,
//! hits and geometry without moving any other box; the box is a stacking
//! context; a scaling list, and a fixed or absolutely positioned descendant
//! the box would contain, are refused. `scripts/test-native-positioned.mjs`
//! compares geometry, hits and pixels with Chrome.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, LayoutError, PaintError, View};

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

const SIDE: u32 = 100;

fn view(transform: &str, child: &[(P, &str)]) -> View {
    let mut b = B::new();
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    for (property, value) in [
        (P::Width, "40px"),
        (P::Height, "20px"),
        (P::BackgroundColor, "#e00000"),
        (P::Transform, transform),
    ] {
        b.set_style(2, property, value);
    }
    b.create_element(3, Tag::Div).insert(2, 3, 0);
    b.set_style(3, P::Width, "10px")
        .set_style(3, P::Height, "10px");
    for &(property, value) in child {
        b.set_style(3, property, value);
    }
    b.create_element(4, Tag::Div).insert(1, 4, 0);
    b.set_style(4, P::Width, "40px")
        .set_style(4, P::Height, "20px");
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Denied).unwrap();
    view
}

#[test]
fn a_translation_moves_the_box_and_its_content_only() {
    let view = view("translate(50%, 15px)", &[(P::BackgroundColor, "#0000e0")]);
    let boxes = view.layout_boxes(SIDE as f32, SIDE as f32).unwrap();
    assert_eq!((boxes[&2].x, boxes[&2].y), (20.0, 15.0));
    assert_eq!((boxes[&3].x, boxes[&3].y), (20.0, 15.0));
    // The next block keeps its place.
    assert_eq!((boxes[&4].x, boxes[&4].y), (0.0, 20.0));
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    assert_eq!(image.hit_test(25.0, 20.0), Some(3));
    assert_eq!(image.hit_test(5.0, 5.0), None);
    // Above the later sibling it overlaps: a stacking context at level zero.
    assert_eq!(image.hit_test(30.0, 25.0), Some(2));
    let pixel = |x: u32, y: u32| &image.pixels[((y * SIDE + x) * 4) as usize..][..4];
    assert_eq!(pixel(30, 25), [224, 0, 0, 255]);
}

#[test]
fn scaling_and_contained_out_of_flow_descendants_are_refused() {
    assert!(matches!(
        view("scale(0.98)", &[]).layout_boxes(SIDE as f32, SIDE as f32),
        Err(LayoutError::Unsupported(2))
    ));
    view("scale(1) translateX(1px)", &[])
        .layout_boxes(SIDE as f32, SIDE as f32)
        .unwrap();
    for position in ["fixed", "absolute"] {
        assert!(matches!(
            view("translateY(1px)", &[(P::Position, position)]).rasterize_boxes(SIDE, SIDE),
            Err(PaintError::Layout(LayoutError::Unsupported(3)))
        ));
    }
}
