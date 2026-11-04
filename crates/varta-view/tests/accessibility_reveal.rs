use varta_common::view::{Attribute as A, Command, EventKind, Tag};
use varta_view::{
    Access, AccessibilityAction as Action, AccessibilityNode, AccessibilityScrollAction as Scroll,
    AccessibilityScrollHint as Hint, BatchWriter as B, Broker, Filter, FrameMetadata, Input,
    Target, TextMeasurer, View,
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
fn source(view: &View) -> AccessibilityNode {
    view.accessibility_tree().unwrap().nodes[&2].clone()
}
fn field(tag: Tag, value: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1,".field {display:block;width:60px;height:60px;min-width:0;min-height:0;padding:4px;border:1px solid #000;border-radius:0;outline:none;background-color:#fff;color:#000;font-family:\"Varta Password Fixture\";font-size:20px;font-weight:400;font-style:normal;line-height:20px}")
        .create_element(2,tag).set_class(2,"field").set_value(2,value,0).insert(1,2,0)
        .listen(2,EventKind::Scroll,2,Filter::default()).listen(2,EventKind::Input,3,Filter::default()).listen(2,EventKind::Change,4,Filter::default());
    let mut view = View::default();
    commit(&mut view, &b);
    view
}
fn frame(view: &View, scale: f32) -> FrameMetadata {
    let mut font = TextMeasurer::new(
        include_bytes!("fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap();
    view.rasterize_scaled(
        (200.0 * scale) as u32,
        (160.0 * scale) as u32,
        scale,
        Some((&mut font, 20.0)),
    )
    .unwrap()
    .into_metadata()
}
fn reveal(
    view: &mut View,
    old: &AccessibilityNode,
    start: usize,
    end: usize,
    hint: Hint,
    frame: &FrameMetadata,
) -> bool {
    view.accessibility_input_from_frame(
        &old.identity,
        &Action::Scroll(Scroll::TextIntoView {
            revision: old.text.as_ref().unwrap().revision.clone(),
            start,
            end,
            hint,
        }),
        frame,
        &mut Deny,
    )
    .unwrap()
}
fn unchanged(old: &AccessibilityNode, view: &View) {
    let current = source(view);
    assert_eq!(old.value, current.value);
    assert_eq!(
        old.text.as_ref().unwrap().revision,
        current.text.as_ref().unwrap().revision
    );
    assert_eq!(
        old.text.as_ref().unwrap().selection,
        current.text.as_ref().unwrap().selection
    );
}
#[test]
fn single_line_reveal_pans_without_moving_selection_and_normal_input_restores_caret_follow() {
    let value = "A".repeat(40);
    let mut view = field(Tag::Input, &value);
    let mut b = B::new();
    b.command(2, Command::Focus, 0.0);
    commit(&mut view, &b);
    view.drain();
    let old = source(&view);
    let image = frame(&view, 1.0);
    assert!(image.ime_cursor_area().is_some());
    assert!(image.text_reveal_supported(&old.identity, &old.text.as_ref().unwrap().revision));
    assert!(reveal(&mut view, &old, 0, 1, Hint::Nearest, &image));
    unchanged(&old, &view);
    assert!(view.drain().is_empty());
    let fresh = frame(&view, 1.0);
    assert!(fresh.ime_cursor_area().is_none());
    let geometry = fresh
        .field_text_geometry(&old.identity, &old.text.as_ref().unwrap().revision)
        .unwrap();
    assert!(geometry.clusters[0].bounds.x >= geometry.clip.x);
    assert!(!reveal(&mut view, &old, 0, 1, Hint::Nearest, &image));
    // Equal guest echoes preserve the host display pan, just like selection.
    let mut b = B::new();
    b.set_value(2, &value, 0);
    commit(&mut view, &b);
    assert!(frame(&view, 1.0).ime_cursor_area().is_none());
    // Ordinary caret movement clears the assistive reading position.
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: value.len(),
            focus: value.len(),
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
    assert!(frame(&view, 1.0).ime_cursor_area().is_some());
    let fresh = frame(&view, 1.0);
    assert!(reveal(
        &mut view,
        &old,
        0,
        value.len(),
        Hint::LeftEdge,
        &fresh
    ));
    assert!(frame(&view, 1.0).ime_cursor_area().is_none());
    let fresh = frame(&view, 1.0);
    assert!(reveal(
        &mut view,
        &old,
        0,
        value.len(),
        Hint::RightEdge,
        &fresh
    ));
    assert!(frame(&view, 1.0).ime_cursor_area().is_some());
    unchanged(&old, &view);
}
#[test]
fn readonly_crlf_textarea_reveals_trailing_empty_line_in_physical_frames() {
    for scale in [1.0, 2.0] {
        let value = "A\r\n".repeat(20);
        let mut view = field(Tag::Textarea, &value);
        let mut b = B::new();
        b.set_attribute(2, A::Readonly, "");
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
        view.drain();
        let focus = view.accessibility_tree().unwrap().focus;
        let old = source(&view);
        let image = frame(&view, scale);
        assert!(reveal(
            &mut view,
            &old,
            value.len(),
            value.len(),
            Hint::Nearest,
            &image
        ));
        unchanged(&old, &view);
        assert_eq!(view.accessibility_tree().unwrap().focus, focus);
        let events = view.drain();
        assert!(!events.is_empty());
        assert!(
            events
                .iter()
                .all(|event| event.record.kind == EventKind::Scroll && !event.gesture)
        );
        let fresh = frame(&view, scale);
        let g = fresh
            .field_text_geometry(&old.identity, &old.text.as_ref().unwrap().revision)
            .unwrap();
        let last = g.lines.last().unwrap();
        assert!(last.bounds.y >= g.clip.y);
        assert!(last.bounds.y + last.bounds.height <= g.clip.y + g.clip.height + 1.0);
        // A byte boundary inside CRLF is never a legal assistive range.
        assert!(!reveal(&mut view, &old, 2, 2, Hint::Nearest, &fresh));
        assert!(reveal(&mut view, &old, 0, 1, Hint::TopEdge, &fresh));
        assert_eq!(frame(&view, scale).scroll_metrics(2).unwrap().top, 0.0);
    }
}
#[test]
fn password_reveal_uses_mask_grapheme_boundaries_and_rejects_stale_or_composing_ranges() {
    let value = format!("e\u{301}👩\u{200d}💻{}", "A".repeat(40));
    let mut view = field(Tag::Input, &value);
    let mut b = B::new();
    b.set_attribute(2, A::Type, "password")
        .command(2, Command::Focus, 0.0);
    commit(&mut view, &b);
    let old = source(&view);
    let image = frame(&view, 2.0);
    assert!(!old.value.as_ref().unwrap().contains('e'));
    for (start, end) in [(1, 1), (1, 3), (usize::MAX, usize::MAX), (4, 3)] {
        assert!(!reveal(&mut view, &old, start, end, Hint::Nearest, &image));
    }
    assert!(reveal(&mut view, &old, 0, 3, Hint::Nearest, &image));
    unchanged(&old, &view);
    let fresh = frame(&view, 2.0);
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "A".into(),
            cursor: None,
        },
        &mut Deny,
    )
    .unwrap();
    assert!(!reveal(&mut view, &old, 0, 3, Hint::Nearest, &fresh));
    let mut b = B::new();
    b.set_value(2, "different", 0);
    commit(&mut view, &b);
    let fresh = frame(&view, 2.0);
    assert!(!reveal(&mut view, &old, 0, 3, Hint::Nearest, &fresh));
    assert!(view.drain().is_empty());
}

#[test]
fn range_reveal_scrolls_own_text_and_nested_ancestors_in_one_preserved_selection() {
    let value = "A\n".repeat(20);
    let mut view = field(Tag::Textarea, &value);
    let mut b = B::new();
    b.stylesheet(
        2,
        ".pane {width:100px;height:40px;overflow:auto}.space {height:120px}",
    )
    .create_element(5, Tag::Div)
    .set_class(5, "pane")
    .insert(1, 5, 0)
    .create_element(6, Tag::Div)
    .set_class(6, "space")
    .insert(5, 6, 0)
    .insert(5, 2, 0)
    .listen(5, EventKind::Scroll, 5, Filter::default());
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
    view.drain();
    let old = source(&view);
    let image = frame(&view, 2.0);
    assert!(reveal(
        &mut view,
        &old,
        value.len() - 2,
        value.len() - 1,
        Hint::BottomEdge,
        &image
    ));
    unchanged(&old, &view);
    let events = view.drain();
    assert!(
        events
            .iter()
            .any(|e| e.record.target == 2 && e.record.scroll_top > 0.0)
    );
    assert!(
        events
            .iter()
            .any(|e| e.record.target == 5 && e.record.scroll_top > 0.0)
    );
    assert!(
        events
            .iter()
            .all(|e| e.record.kind == EventKind::Scroll && !e.gesture)
    );
    let fresh = frame(&view, 2.0);
    let g = fresh
        .field_text_geometry(&old.identity, &old.text.as_ref().unwrap().revision)
        .unwrap();
    let cluster = g
        .clusters
        .iter()
        .find(|c| c.source.start == value.len() - 2)
        .unwrap();
    assert!(
        cluster.bounds.y >= g.clip.y
            && cluster.bounds.y + cluster.bounds.height <= g.clip.y + g.clip.height + 1.0
    );
}
