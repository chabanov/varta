//! K8 T2 S4 remainder through the native renderer: per-side borders and
//! corner radii, `overflow-x`/`overflow-y`, `visibility`, `pointer-events`
//! and `text-transform`.
use varta_common::view::{Attribute, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Input, Renderer, Target, TextMeasurer, View};

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

/// A view of `sheet` with one `div` per `(id, parent, class)`.
fn view(sheet: &str, nodes: &[(u32, u32, &str)]) -> View {
    let mut b = B::new();
    b.stylesheet(1, sheet);
    for &(id, parent, class) in nodes {
        b.create_element(id, Tag::Div)
            .set_class(id, class)
            .insert(parent, id, 0);
    }
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    view
}

fn pixels(view: &View, width: u32, height: u32) -> impl Fn(u32, u32) -> [u8; 4] {
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let image = renderer.render(view, width, height, 1.0).unwrap();
    move |x, y| {
        let offset = ((y * image.width + x) * 4) as usize;
        image.pixels[offset..offset + 4].try_into().unwrap()
    }
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[test]
fn side_longhands_and_box_values_set_each_border_in_cascade_order() {
    let view = view(
        ".a { width: 20px; height: 20px; border-style: solid none; border-width: 1px 2px 3px; border-color: #f00 #0f0 #00f } .a { border-left-style: solid; border-left-width: thick; border-left-color: #0f0 }",
        &[(2, 1, "a")],
    );
    let boxes = view.layout_boxes(100.0, 100.0).unwrap();
    // top 1, right none, bottom 3, left thick (5px).
    assert_eq!(boxes[&2].width, 25.0);
    assert_eq!(boxes[&2].height, 24.0);
    let pixel = pixels(&view, 40, 40);
    assert_eq!(pixel(10, 0), RED);
    assert_eq!(pixel(2, 10), GREEN);
    assert_eq!(pixel(10, 22), BLUE);
    // A later shorthand resets the longhands it covers.
    let view = view_with_shorthand();
    let boxes = view.layout_boxes(100.0, 100.0).unwrap();
    assert_eq!(boxes[&2].width, 22.0);
}

fn view_with_shorthand() -> View {
    view(
        ".a { width: 20px; height: 20px; border-left-width: 9px; border-left-style: solid } .a { border: 1px solid #000 }",
        &[(2, 1, "a")],
    )
}

#[test]
fn logical_borders_and_corners_map_onto_the_physical_sides() {
    let view = view(
        ".a { width: 20px; height: 10px; border-inline: 2px solid #f00; border-block-end-width: 4px; border-block-end-style: solid; border-start-start-radius: 0; background-color: #00f }",
        &[(2, 1, "a")],
    );
    let boxes = view.layout_boxes(100.0, 100.0).unwrap();
    assert_eq!((boxes[&2].width, boxes[&2].height), (24.0, 14.0));
}

#[test]
fn corner_longhands_override_the_shorthand_corner_by_corner() {
    let view = view(
        ".a { width: 40px; height: 40px; background-color: #00f; border-radius: 0; border-bottom-right-radius: 20px }",
        &[(2, 1, "a")],
    );
    let pixel = pixels(&view, 40, 40);
    assert_eq!(pixel(1, 1), BLUE);
    assert_eq!(pixel(38, 1), BLUE);
    assert_eq!(pixel(39, 39), CLEAR);
}

#[test]
fn clip_on_one_axis_leaves_the_other_overflowing() {
    let view = view(
        ".a { width: 20px; height: 20px; overflow-x: clip } .w { width: 40px; height: 40px; background-color: #f00 }",
        &[(2, 1, "a"), (3, 2, "w")],
    );
    let pixel = pixels(&view, 60, 60);
    assert_eq!(pixel(10, 30), RED);
    assert_eq!(pixel(30, 10), CLEAR);
    // `hidden` on one axis makes the other `auto`: both clip.
    let view = view_hidden_x();
    let pixel = pixels(&view, 60, 60);
    assert_eq!(pixel(10, 30), CLEAR);
}

fn view_hidden_x() -> View {
    view(
        ".a { width: 20px; height: 20px; overflow-x: hidden } .w { width: 40px; height: 40px; background-color: #f00 }",
        &[(2, 1, "a"), (3, 2, "w")],
    )
}

#[test]
fn invisible_boxes_paint_nothing_and_take_no_pointer_while_visible_children_do() {
    let view = view(
        ".a { width: 40px; height: 40px; background-color: #f00; visibility: hidden } .v { width: 10px; height: 10px; background-color: #0f0; visibility: visible } .p { width: 40px; height: 10px; background-color: #00f; pointer-events: none }",
        &[(2, 1, "a"), (3, 2, "v"), (4, 1, "p")],
    );
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let image = renderer.render(&view, 60, 60, 1.0).unwrap();
    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let offset = ((y * image.width + x) * 4) as usize;
        image.pixels[offset..offset + 4].try_into().unwrap()
    };
    assert_eq!(pixel(5, 5), GREEN);
    assert_eq!(pixel(20, 20), CLEAR);
    assert_eq!(pixel(20, 45), BLUE);
    assert_eq!(image.hit_test(5.0, 5.0), Some(3));
    // The invisible box and the `pointer-events: none` box pass through.
    assert_eq!(image.hit_test(20.0, 20.0), None);
    assert_eq!(image.hit_test(20.0, 45.0), None);
}

#[test]
fn invisible_controls_are_not_focusable() {
    let mut b = B::new();
    b.stylesheet(1, ".h { visibility: hidden }")
        .create_element(2, Tag::Button)
        .set_class(2, "h")
        .insert(1, 2, 0)
        .create_element(3, Tag::Button)
        .insert(1, 3, 0)
        .create_element(4, Tag::Input)
        .set_attribute(4, Attribute::Type, "text")
        .insert(1, 4, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    let tab = |view: &mut View, from: u32| {
        view.input(
            &Input::Key {
                target: Target::Node(from),
                key: "Tab".into(),
                shift: false,
                ctrl: false,
                alt: false,
                meta: false,
                composing: false,
            },
            &mut Deny,
        )
        .unwrap();
    };
    tab(&mut view, 1);
    assert_eq!(view.snapshot()["focus"], 3);
    let mut b = B::new();
    b.command(2, varta_common::view::Command::Focus, 0.0);
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(view.snapshot()["focus"], 3);
    // Visible again, it takes the focus.
    let mut b = B::new();
    b.set_style(2, P::Visibility, "visible")
        .command(2, varta_common::view::Command::Focus, 0.0);
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(view.snapshot()["focus"], 2);
}

/// CSS Box Sizing 3 §4.1.
#[test]
fn box_sizing_chooses_the_box_width_and_height_size() {
    let view = view(
        ".a { width: 20px; height: 10px; padding: 2px; border: 1px solid #000 } .b { box-sizing: border-box }",
        &[(2, 1, "a"), (3, 1, "a b")],
    );
    let boxes = view.layout_boxes(100.0, 100.0).unwrap();
    assert_eq!((boxes[&2].width, boxes[&2].height), (26.0, 16.0));
    assert_eq!((boxes[&3].width, boxes[&3].height), (20.0, 10.0));
}

fn case_font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("../../varta-style/tests/fixtures/case.ttf").to_vec(),
        "Varta Case",
    )
    .unwrap()
}

/// The width of a flex item (class `t`) holding `text` under `sheet`, in
/// the case fixture at 10px.
fn text_width(sheet: &str, text: &str) -> f32 {
    let mut b = B::new();
    b.stylesheet(
        1,
        &format!(".r {{ display: flex; font-family: \"Varta Case\"; font-size: 10px }} {sheet}"),
    )
    .create_element(2, Tag::Div)
    .set_class(2, "r")
    .insert(1, 2, 0)
    .create_element(3, Tag::Div)
    .set_class(3, "t")
    .insert(2, 3, 0)
    .create_text(4, text)
    .insert(3, 4, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    let boxes = view
        .layout_boxes_with_text(400.0, 100.0, &mut case_font(), 16.0)
        .unwrap();
    boxes[&3].width
}

/// CSS Text 3 §2.1 through the fixture's distinct advances (A 700, B 720,
/// S 650, a 500, b 520, ß 560, hyphen 310, space 250, Ａ 1030, ideographic
/// space 1010 per 1000 units).
#[test]
fn text_transform_changes_the_shaped_text() {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(close(text_width("", "ab-ß"), 18.9));
    assert!(close(
        text_width(".t { text-transform: uppercase }", "ab-ß"),
        30.3
    ));
    assert!(close(
        text_width(".t { text-transform: capitalize }", "ab ab"),
        26.9
    ));
    assert!(close(
        text_width(".r { text-transform: lowercase }", "AB"),
        10.2
    ));
    assert!(close(
        text_width(".t { text-transform: uppercase full-width }", "a a"),
        30.7
    ));
}

#[test]
fn text_fields_refuse_a_text_transform_and_controls_reset_inheritance() {
    let field = |class: &str| {
        let mut b = B::new();
        b.stylesheet(
            1,
            ".u { text-transform: uppercase } .f { display: block; width: 80px; height: 20px; font-family: \"Varta Case\"; font-size: 10px }",
        )
        .create_element(2, Tag::Div)
        .set_class(2, "u")
        .insert(1, 2, 0)
        .create_element(3, Tag::Input)
        .set_attribute(3, Attribute::Type, "text")
        .set_class(3, class)
        .insert(2, 3, 0);
        let mut view = View::default();
        view.commit(&b.finish(), false, &mut Deny).unwrap();
        view.layout_boxes_with_text(200.0, 100.0, &mut case_font(), 16.0)
    };
    // The user-agent `text-transform: none` of controls stops inheritance.
    field("f").unwrap();
    assert!(field("f u").is_err());
}
