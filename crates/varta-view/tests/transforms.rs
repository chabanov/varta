//! CSS `transform`: a translation moves the box and its content for paint,
//! hits and geometry without moving any other box; a scale maps the subtree
//! around the box's centre; the box is a stacking context; a fixed or
//! absolutely positioned descendant the box would contain, and a scroll
//! container, text field or select in a scaled subtree, are refused.
//! `scripts/test-native-positioned.mjs` compares geometry, hits and pixels
//! with Chrome.
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
fn a_scale_maps_the_subtree_for_geometry_hits_and_pixels() {
    // 40 by 20 around (20, 10), halved: (10, 5) to (30, 15).
    let view = view("scale(0.5)", &[(P::BackgroundColor, "#0000e0")]);
    let boxes = view.layout_boxes(SIDE as f32, SIDE as f32).unwrap();
    assert_eq!(
        (boxes[&2].x, boxes[&2].y, boxes[&2].width, boxes[&2].height),
        (10.0, 5.0, 20.0, 10.0)
    );
    assert_eq!((boxes[&3].x, boxes[&3].width), (10.0, 5.0));
    // The next block keeps its laid-out place.
    assert_eq!((boxes[&4].x, boxes[&4].y), (0.0, 20.0));
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    assert_eq!(image.hit_test(12.0, 7.0), Some(3));
    assert_eq!(image.hit_test(20.0, 12.0), Some(2));
    assert_eq!(image.hit_test(5.0, 2.0), None);
    let pixel = |x: u32, y: u32| &image.pixels[((y * SIDE + x) * 4) as usize..][..4];
    assert_eq!(pixel(12, 7), [0, 0, 224, 255]);
    assert_eq!(pixel(25, 12), [224, 0, 0, 255]);
    assert_eq!(pixel(35, 12), [0, 0, 0, 0]);
}

#[test]
fn contained_out_of_flow_descendants_and_scaled_fields_are_refused() {
    for factor in ["scale(0)", "scale(-1)"] {
        assert!(matches!(
            view(factor, &[]).layout_boxes(SIDE as f32, SIDE as f32),
            Err(LayoutError::Unsupported(2))
        ));
    }
    for child in [(P::Overflow, "hidden"), (P::Overflow, "auto")] {
        assert!(matches!(
            view("scale(0.98)", &[child]).layout_boxes(SIDE as f32, SIDE as f32),
            Err(LayoutError::Unsupported(3))
        ));
    }
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

#[test]
fn a_scale_transitions_from_none_function_by_function() {
    let mut b = B::new();
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    for (property, value) in [
        (P::Width, "40px"),
        (P::Height, "20px"),
        (P::TransitionProperty, "transform"),
        (P::TransitionDuration, "100ms"),
        (P::TransitionTimingFunction, "linear"),
    ] {
        b.set_style(2, property, value);
    }
    let mut view = View::default();
    view.set_animation_time(0.0);
    view.commit(&b.finish(), false, &mut Denied).unwrap();
    let mut change = B::new();
    change.set_style(2, P::Transform, "translateX(10px) scale(0.5)");
    view.commit(&change.finish(), false, &mut Denied).unwrap();
    // Halfway: translateX(5px) scale(0.75) around the centre (20, 10).
    view.set_animation_time(50.0);
    let boxes = view.layout_boxes(SIDE as f32, SIDE as f32).unwrap();
    assert_eq!(
        (boxes[&2].x, boxes[&2].y, boxes[&2].width, boxes[&2].height),
        (10.0, 2.5, 30.0, 15.0)
    );
    assert!(view.animating());
    // Lists of different primitives do not interpolate: the value jumps.
    let mut other = B::new();
    other.set_style(2, P::Transform, "scale(2) scale(2)");
    view.commit(&other.finish(), false, &mut Denied).unwrap();
    let boxes = view.layout_boxes(SIDE as f32, SIDE as f32).unwrap();
    assert_eq!((boxes[&2].width, boxes[&2].height), (160.0, 80.0));
}
