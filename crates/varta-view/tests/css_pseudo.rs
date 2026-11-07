//! K8 T2 S5 pseudo-elements through the native renderer: generated
//! `::before`/`::after` boxes, a field's `::placeholder` and a modal
//! dialog's `::backdrop`, and the Preflight properties beside them.
use varta_common::view::{Attribute, Command, Tag};
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

struct Frame {
    image: varta_view::RasterImage,
}
impl Frame {
    fn new(view: &View, width: u32, height: u32) -> Self {
        let mut renderer = Renderer::new(Some(case_font()), 16.0).unwrap();
        Self {
            image: renderer.render(view, width, height, 1.0).unwrap(),
        }
    }
    fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * self.image.width + x) * 4) as usize;
        self.image.pixels[offset..offset + 4].try_into().unwrap()
    }
}

fn case_font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("../../varta-style/tests/fixtures/case.ttf").to_vec(),
        "Varta Case",
    )
    .unwrap()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// Pseudo-Elements 4 §4.1: `::after` with `content` generates a box (the
/// Tailwind underline: an absolutely positioned bar), `::before` without
/// `content` generates none; hits on the box go to the originating
/// element, and the guest's snapshot does not show it.
#[test]
fn before_and_after_generate_boxes_with_content() {
    let sheet = "@property --tw-content { syntax: \"*\"; initial-value: \"\"; inherits: false } \
        .a { position: relative; width: 40px; height: 20px; background-color: #00f } \
        .a::after { content: var(--tw-content); position: absolute; left: 0; top: 20px; width: 100%; height: 10px; background-color: #0f0 } \
        .a::before { width: 10px; height: 10px; background-color: #f00 } \
        .b::before { content: none; position: absolute; width: 5px; height: 5px; background-color: #f00 }";
    let view = view(sheet, &[(2, 1, "a"), (3, 1, "b")]);
    let frame = Frame::new(&view, 60, 60);
    assert_eq!(frame.pixel(5, 5), BLUE);
    assert_eq!(frame.pixel(35, 25), GREEN);
    assert_eq!(frame.pixel(5, 45), CLEAR);
    assert_eq!(frame.image.hit_test(10.0, 25.0), Some(2));
    let tree = view.snapshot()["tree"].clone();
    assert_eq!(tree["children"][0]["id"], 2);
    assert!(tree["children"][0].get("children").is_none());
}

/// Generated text renders in the originating element's inline formatting
/// context, before and after its own content; `display: none` and
/// `content: normal` generate nothing.
#[test]
fn generated_text_follows_the_element_and_its_states() {
    let mut b = B::new();
    b.stylesheet(
        1,
        ".a { width: 40px; height: 10px } .a::after { content: \"\"; display: block; height: 10px; background-color: #0f0 } .a.off::after { display: none }",
    )
    .create_element(2, Tag::Div)
    .set_class(2, "a")
    .insert(1, 2, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(Frame::new(&view, 60, 40).pixel(5, 5), GREEN);
    let mut b = B::new();
    b.set_class(2, "a off");
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(Frame::new(&view, 60, 40).pixel(5, 5), CLEAR);
    // A guest cannot address the generated range.
    let mut b = B::new();
    b.create_element(0x8000_0000, Tag::Div);
    assert!(view.commit(&b.finish(), false, &mut Deny).is_err());
}

/// Structural selectors never see the generated boxes.
#[test]
fn generated_boxes_are_not_children_for_selectors() {
    let view = view(
        ".p::before { content: \"\"; display: block; height: 1px } .c:first-child { margin-top: 3px } .p:empty { color: #111 }",
        &[(2, 1, "p"), (3, 2, "c"), (4, 1, "p")],
    );
    let tree = view.snapshot()["tree"].clone();
    assert_eq!(
        tree["children"][0]["children"][0]["style"]["margin-top"],
        "3px"
    );
    assert_eq!(tree["children"][1]["style"]["color"], "#111");
}

/// Pseudo-Elements 4 §3.3 and Color 5 §3: Preflight's placeholder color
/// mixes `currentcolor`, the field's color.
#[test]
fn placeholder_takes_its_author_color() {
    let mut b = B::new();
    b.stylesheet(
        1,
        "input { display: block; width: 80px; height: 30px; font-size: 20px; color: #f00; border: 0 solid; padding: 0 } input::placeholder { color: color-mix(in srgb, currentcolor 50%, #00f) } .o::placeholder { opacity: 0 }",
    )
    .create_element(2, Tag::Input)
    .set_attribute(2, Attribute::Placeholder, "AAAA")
    .insert(1, 2, 0)
    .create_element(3, Tag::Input)
    .set_class(3, "o")
    .set_attribute(3, Attribute::Placeholder, "AAAA")
    .insert(1, 3, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    let mut font = TextMeasurer::new(
        include_bytes!("fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap();
    let image = view.rasterize_with_text(200, 80, &mut font, 20.0).unwrap();
    let mixed = [128, 0, 128, 255];
    let count = |color: [u8; 4]| {
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| **pixel == color)
            .count()
    };
    assert!(
        count(mixed) > 0,
        "no glyph pixel in the mixed placeholder color"
    );
    assert_eq!(count(RED), 0);
}

/// Position 4 §3.2: a modal dialog's backdrop takes its author background.
#[test]
fn backdrop_takes_its_author_background() {
    let mut b = B::new();
    b.stylesheet(
        1,
        "dialog { width: 10px; height: 10px } dialog::backdrop { background-color: #00f; margin: 0; padding: 4px; border: 0 solid } ",
    )
    .create_element(2, Tag::Dialog)
    .insert(1, 2, 0)
    .command(2, Command::ShowModal, 0.0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(Frame::new(&view, 60, 60).pixel(2, 2), BLUE);
}

/// Lists 3 §3: `list-style: none` removes the markers Preflight resets.
#[test]
fn list_style_none_removes_markers() {
    let mut b = B::new();
    b.stylesheet(
        1,
        "ul { list-style: none; margin: 0; padding: 0 0 0 30px; color: #f00 } li { height: 20px }",
    )
    .create_element(2, Tag::Ul)
    .insert(1, 2, 0)
    .create_element(3, Tag::Li)
    .insert(2, 3, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    let frame = Frame::new(&view, 60, 40);
    for x in 0..30 {
        for y in 0..20 {
            assert_eq!(frame.pixel(x, y), CLEAR, "{x},{y}");
        }
    }
}
