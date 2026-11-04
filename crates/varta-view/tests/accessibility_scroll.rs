use varta_common::view::{Attribute as A, DRAFT_MAX_PENDING_EVENTS, EventKind, Tag};
use varta_view::{
    Access, AccessibilityAction as Action, AccessibilityScrollAction as Scroll,
    AccessibilityScrollOffset as Offset, AccessibilityScrollUnit as Unit, BatchWriter as B, Broker,
    Filter, Input, Target, View,
};
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
fn commit(view: &mut View, b: &B) {
    view.commit(&b.finish(), false, &mut Deny).unwrap();
}
fn pane(nested: bool) -> View {
    let mut b = B::new();
    b.stylesheet(
        1,
        ".pane {width:100px;height:50px;overflow:auto} .space {height:120px} .target {height:20px}",
    )
    .create_element(2, Tag::Div)
    .set_class(2, "pane")
    .insert(1, 2, 0)
    .create_element(3, Tag::Div)
    .set_class(3, "space")
    .insert(2, 3, 0)
    .create_element(4, Tag::Div)
    .set_class(4, if nested { "pane" } else { "target" })
    .insert(2, 4, 0);
    if nested {
        b.create_element(5, Tag::Div)
            .set_class(5, "space")
            .insert(4, 5, 0)
            .create_element(6, Tag::Section)
            .set_attribute(6, A::AriaLabel, "Target")
            .set_class(6, "target")
            .insert(4, 6, 0);
    }
    b.listen(2, EventKind::Scroll, 20, Filter::default())
        .listen(4, EventKind::Scroll, 40, Filter::default());
    let mut view = View::default();
    commit(&mut view, &b);
    view
}
fn frame(view: &View) -> varta_view::FrameMetadata {
    view.rasterize_boxes(150, 100).unwrap().into_metadata()
}
fn scroll(view: &mut View, id: u32, action: Scroll, frame: &varta_view::FrameMetadata) -> bool {
    let identity = view.node_identity(id).unwrap();
    view.accessibility_input_from_frame(&identity, &Action::Scroll(action), frame, &mut Deny)
        .unwrap()
}
#[test]
fn page_item_and_absolute_scroll_use_current_metrics_and_never_grant_gesture() {
    let mut view = pane(false);
    let original = view.snapshot();
    let semantics = view.accessibility_tree().unwrap();
    assert_eq!(semantics.nodes[&2].role, "generic");
    assert_eq!(view.snapshot(), original);
    let image = frame(&view);
    assert!(
        !view
            .accessibility_input(
                &view.node_identity(2).unwrap(),
                &Action::Scroll(Scroll::Down(Unit::Page)),
                &mut Deny
            )
            .unwrap()
    );
    assert!(scroll(&mut view, 2, Scroll::Down(Unit::Page), &image));
    let events = view.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].record.scroll_top, 50.0);
    assert_eq!(events[0].record.client_height, 50.0);
    assert!(!events[0].gesture);
    let before = view.snapshot();
    assert!(!scroll(&mut view, 2, Scroll::Down(Unit::Page), &image));
    assert_eq!(view.snapshot(), before);
    let image = frame(&view);
    assert!(scroll(
        &mut view,
        2,
        Scroll::To(Offset::new(f64::MAX).unwrap()),
        &image
    ));
    let maximum = image.scroll_metrics(2).unwrap().max;
    assert_eq!(view.drain()[0].record.scroll_top, f64::from(maximum));
    let image = frame(&view);
    assert!(scroll(&mut view, 2, Scroll::Down(Unit::Page), &image));
    assert!(view.drain().is_empty());
    assert!(scroll(&mut view, 2, Scroll::Up(Unit::Item), &image));
    assert_eq!(
        view.drain()[0].record.scroll_top,
        f64::from(maximum) - view.environment().root_font_size
    );
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert!(Offset::new(bad).is_none());
    }
}
#[test]
fn into_view_uses_unclipped_nested_geometry_without_focus_or_click() {
    let mut view = pane(true);
    let image = frame(&view);
    let identity = view.node_identity(6).unwrap();
    assert!(image.scroll_into_view_supported(6));
    assert!(!image.node_bounds_map().contains_key(&6));
    assert!(
        view.accessibility_input_from_frame(
            &identity,
            &Action::Scroll(Scroll::IntoView),
            &image,
            &mut Deny
        )
        .unwrap()
    );
    let events = view.drain();
    assert!(
        events
            .iter()
            .all(|event| event.record.kind == EventKind::Scroll && !event.gesture)
    );
    assert!(
        events
            .iter()
            .any(|event| event.record.target == 4 && event.record.scroll_top > 0.0)
    );
    assert!(
        events
            .iter()
            .any(|event| event.record.target == 2 && event.record.scroll_top > 0.0)
    );
    assert!(view.accessibility_tree().unwrap().focus.is_none());
    let image = frame(&view);
    assert!(
        view.accessibility_input_from_frame(
            &identity,
            &Action::Scroll(Scroll::IntoView),
            &image,
            &mut Deny
        )
        .unwrap()
    );
    assert!(view.drain().is_empty());
}
#[test]
fn hidden_reparented_or_replaced_targets_cannot_reuse_frame_authority() {
    for mode in 0..3 {
        let mut view = pane(true);
        let image = frame(&view);
        let identity = view.node_identity(6).unwrap();
        let mut b = B::new();
        match mode {
            0 => {
                b.set_attribute(4, A::AriaHidden, "true");
            }
            1 => {
                b.insert(2, 6, 0);
            }
            _ => {
                b.remove(6)
                    .create_element(6, Tag::Section)
                    .set_attribute(6, A::AriaLabel, "Target")
                    .set_class(6, "target")
                    .insert(4, 6, 0);
            }
        }
        commit(&mut view, &b);
        let before = view.snapshot();
        assert!(
            !view
                .accessibility_input_from_frame(
                    &identity,
                    &Action::Scroll(Scroll::IntoView),
                    &image,
                    &mut Deny
                )
                .unwrap()
        );
        assert_eq!(view.snapshot(), before);
        assert!(view.drain().is_empty());
    }
}
#[test]
fn multi_ancestor_scroll_preserves_coalescible_event_policy_at_queue_capacity() {
    let mut view = pane(true);
    let mut b = B::new();
    b.listen(6, EventKind::Click, 60, Filter::default());
    commit(&mut view, &b);
    for _ in 0..DRAFT_MAX_PENDING_EVENTS - 1 {
        view.input(
            &Input::Click {
                target: Target::Node(6),
            },
            &mut Deny,
        )
        .unwrap();
    }
    let image = frame(&view);
    assert!(
        view.accessibility_input_from_frame(
            &view.node_identity(6).unwrap(),
            &Action::Scroll(Scroll::IntoView),
            &image,
            &mut Deny
        )
        .unwrap()
    );
    let updated = frame(&view);
    assert!(updated.scroll_metrics(2).unwrap().top > 0.0);
    assert!(updated.scroll_metrics(4).unwrap().top > 0.0);
    let events = view.drain();
    assert_eq!(events.len(), DRAFT_MAX_PENDING_EVENTS);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.record.kind == EventKind::Click)
            .count(),
        DRAFT_MAX_PENDING_EVENTS - 1
    );
    assert!(
        events
            .iter()
            .filter(|event| event.record.kind == EventKind::Scroll)
            .all(|event| !event.gesture)
    );
}
#[test]
fn readonly_textarea_pages_preserve_value_selection_and_reject_old_content_frames() {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1,".field {display:block;width:80px;height:60px;padding:0;border:1px solid #000;min-height:0;min-width:0;outline:none;border-radius:0;background-color:#fff;color:#000;font-family:\"Varta Fixture 400\";font-size:20px;font-style:normal;font-weight:400;line-height:20px}")
        .create_element(2,Tag::Textarea).set_class(2,"field").set_attribute(2,A::Readonly,"").set_value(2,&"A\n".repeat(12),0).insert(1,2,0).listen(2,EventKind::Scroll,2,Filter::default()).listen(2,EventKind::Input,3,Filter::default());
    commit(&mut view, &b);
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 0,
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let image = view
        .rasterize_with_text(150, 100, &mut font, 20.0)
        .unwrap()
        .into_metadata();
    let before = view.accessibility_tree().unwrap().nodes[&2].clone();
    assert!(scroll(&mut view, 2, Scroll::Down(Unit::Item), &image));
    assert_eq!(view.drain()[0].record.scroll_top, 20.0);
    let after = view.accessibility_tree().unwrap().nodes[&2].clone();
    assert_eq!(before.value, after.value);
    assert_eq!(
        before.text.unwrap().selection,
        after.text.unwrap().selection
    );
    let image = view
        .rasterize_with_text(150, 100, &mut font, 20.0)
        .unwrap()
        .into_metadata();
    let mut b = B::new();
    b.set_value(2, "new value", 0);
    commit(&mut view, &b);
    assert!(!scroll(&mut view, 2, Scroll::Down(Unit::Page), &image));
}
