//! Trusted pointer state must match CSS, preserve input geometry and ID isolation.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, PointerCursor, Renderer, View};
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
fn view() -> View {
    let mut batch = B::new();
    batch.stylesheet(1, ".outer {width:100px; height:100px; background-color:#fff} .inner {width:30px; height:20px; background-color:#888; cursor:pointer} .leaf {display:block; width:10px; height:10px} .outer:hover {background-color:#00f} .inner:hover {background-color:#0f0; cursor:not-allowed} .inner:active {background-color:#f00}")
        .create_element(2,Tag::Div).set_class(2,"outer").insert(1,2,0)
        .create_element(3,Tag::Div).set_class(3,"inner").insert(2,3,0)
        .create_element(4,Tag::Span).set_class(4,"leaf").insert(3,4,0);
    let mut view = View::default();
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    view
}
fn frame(view: &View, renderer: &mut Renderer) -> varta_view::RasterImage {
    let frame = renderer.render(view, 140, 130, 1.0).unwrap();
    let expected = view.rasterize_boxes(140, 130).unwrap();
    assert!(
        frame == expected,
        "retained pointer frame differs from reference"
    );
    frame
}
fn pixel(frame: &varta_view::RasterImage, x: usize, y: usize) -> &[u8] {
    &frame.pixels[(y * 140 + x) * 4..(y * 140 + x + 1) * 4]
}
#[test]
fn hover_and_press_include_ancestors_restore_pixels_and_do_not_activate() {
    let mut view = view();
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let before = frame(&view, &mut renderer);
    assert_eq!(before.cursor_at(5.0, 5.0), PointerCursor::Pointer);
    let commits = view.commits();
    assert!(view.set_pointer_state(Some(4), None));
    assert!(!view.set_pointer_state(Some(4), None));
    let hovered = frame(&view, &mut renderer);
    assert_eq!(pixel(&hovered, 5, 5), [0, 255, 0, 255]);
    assert_eq!(pixel(&hovered, 50, 50), [0, 0, 255, 255]);
    assert_eq!(hovered.cursor_at(5.0, 5.0), PointerCursor::NotAllowed);
    assert!(before.same_input_geometry(&hovered));
    view.set_pointer_state(Some(4), Some(4));
    let pressed = frame(&view, &mut renderer);
    assert_eq!(pixel(&pressed, 5, 5), [255, 0, 0, 255]);
    assert!(pressed.same_input_geometry(&before));
    view.set_pointer_state(None, None);
    assert!(frame(&view, &mut renderer) == before);
    assert_eq!(view.commits(), commits);
    assert_eq!(view.drain().len(), 0);
}
#[test]
fn pointer_identity_does_not_transfer_to_reused_ids_and_tracks_reparenting() {
    let mut view = view();
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    view.set_pointer_state(Some(4), Some(4));
    frame(&view, &mut renderer);
    let mut batch = B::new();
    batch
        .remove(3)
        .create_element(3, Tag::Div)
        .set_class(3, "inner")
        .insert(2, 3, 0)
        .create_element(4, Tag::Span)
        .set_class(4, "leaf")
        .insert(3, 4, 0);
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    let replaced = frame(&view, &mut renderer);
    assert_eq!(pixel(&replaced, 5, 5), [136, 136, 136, 255]);
    view.set_pointer_state(Some(4), None);
    frame(&view, &mut renderer);
    let mut batch = B::new();
    batch.insert(1, 4, 0);
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    let moved = frame(&view, &mut renderer);
    assert_eq!(pixel(&moved, 50, 50), [255, 255, 255, 255]);
    assert_eq!(pixel(&moved, 5, 5), [136, 136, 136, 255]);
}
#[test]
fn layout_changing_hover_invalidates_input_geometry_and_css_cursor_values_are_observed() {
    let mut view = view();
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let mut batch = B::new();
    batch.stylesheet(2, ".inner:hover {width:60px}");
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    let before = frame(&view, &mut renderer);
    view.set_pointer_state(Some(3), None);
    let after = frame(&view, &mut renderer);
    assert!(!after.same_input_geometry(&before));
    for (style, hint) in [
        ("default", PointerCursor::Default),
        ("pointer", PointerCursor::Pointer),
        ("text", PointerCursor::Text),
        ("not-allowed", PointerCursor::NotAllowed),
    ] {
        let mut batch = B::new();
        batch.set_style(4, P::Cursor, style);
        view.commit(&batch.finish(), false, &mut Denied).unwrap();
        assert_eq!(frame(&view, &mut renderer).cursor_at(5.0, 5.0), hint);
    }
}
