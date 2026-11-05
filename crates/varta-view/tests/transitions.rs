//! CSS transitions on the view's animation clock: hover starts one, the
//! retained renderer paints its current value at each clock, it ends at the
//! static value, a first style and a re-displayed element start none, and a
//! snapshot keeps the static value. `scripts/test-native-transitions.mjs`
//! compares the timing, easing, lists, reversing and cancelling with Chrome.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Renderer, View};

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

const SIDE: u32 = 40;
const SHEET: &str = ".b {width:40px; height:40px; background-color:#ffffff; transition-property:background-color; transition-duration:100ms; transition-timing-function:linear} .b:hover {background-color:#000000}";

fn view() -> View {
    let mut batch = B::new();
    batch
        .stylesheet(1, SHEET)
        .create_element(2, Tag::Div)
        .set_class(2, "b")
        .insert(1, 2, 0);
    let mut view = View::default();
    view.set_animation_time(0.0);
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    view
}

/// The red channel at the box's center, from the retained renderer, which
/// must equal the reference raster.
fn red(view: &View, renderer: &mut Renderer) -> u8 {
    let frame = renderer.render(view, SIDE, SIDE, 1.0).unwrap();
    assert!(frame == view.rasterize_boxes(SIDE, SIDE).unwrap());
    frame.pixels[((SIDE / 2 * SIDE + SIDE / 2) * 4) as usize]
}

#[test]
fn hover_runs_a_transition_on_the_animation_clock() {
    let mut view = view();
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    // The first style starts nothing.
    assert!(!view.animating());
    assert_eq!(red(&view, &mut renderer), 255);
    assert!(!view.set_animation_time(10.0));
    assert!(view.set_pointer_state(Some(2), None));
    assert!(view.animating());
    assert_eq!(red(&view, &mut renderer), 255);
    assert!(view.set_animation_time(35.0));
    // Linear: 25% of the way from white to black.
    assert_eq!(red(&view, &mut renderer), 191);
    assert!(view.set_animation_time(60.0));
    assert_eq!(red(&view, &mut renderer), 128);
    // The clock never moves back.
    assert!(!view.set_animation_time(20.0));
    assert_eq!(red(&view, &mut renderer), 128);
    // The snapshot keeps the cascade's static (hover) value, not the frame's.
    let snapshot = view.snapshot().to_string();
    assert!(
        snapshot.contains("\"background-color\":\"#000000\""),
        "{snapshot}"
    );
    assert!(!snapshot.contains("rgba("), "{snapshot}");
    assert!(view.set_animation_time(110.0));
    assert!(!view.animating());
    assert_eq!(red(&view, &mut renderer), 0);
}

#[test]
fn leaving_mid_way_reverses_from_the_current_value() {
    let mut view = view();
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    view.set_pointer_state(Some(2), None);
    view.set_animation_time(40.0);
    assert_eq!(red(&view, &mut renderer), 153);
    view.set_pointer_state(None, None);
    // Shortened to 40ms: half-way back after 20ms.
    view.set_animation_time(60.0);
    assert_eq!(red(&view, &mut renderer), 204);
    view.set_animation_time(80.0);
    assert!(!view.animating());
    assert_eq!(red(&view, &mut renderer), 255);
}

#[test]
fn a_redisplayed_element_and_an_unused_property_start_nothing() {
    let mut view = view();
    let mut batch = B::new();
    batch.set_style(2, P::Display, "none");
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    view.set_animation_time(10.0);
    let mut batch = B::new();
    batch
        .remove_style(2, P::Display)
        .set_style(2, P::BackgroundColor, "#000000");
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    assert!(!view.animating());
    // A change of a property the list does not name is immediate.
    let mut batch = B::new();
    batch.set_style(2, P::Opacity, "0.5");
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    assert!(!view.animating());
}
