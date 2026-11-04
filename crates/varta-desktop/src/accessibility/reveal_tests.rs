use super::{
    geometry_tests::{render, setup},
    tests::{Deny, commit, request},
    text_tests::runs,
    *,
};
use accesskit::ScrollHint;
use varta_common::view::Tag;
use varta_view::{
    AccessibilityAction as Input, AccessibilityScrollAction as Scroll,
    AccessibilityScrollHint as Hint, BatchWriter as B,
};

#[test]
fn native_text_run_reveal_decodes_hints_to_source_ranges_and_keeps_selection() {
    let value = "A".repeat(40);
    let (bridge, mut view, metadata, version, field) = setup(Tag::Input, &value, 60);
    let before = view.accessibility_tree().unwrap();
    let children = runs(&bridge, field);
    assert_eq!(children.len(), 1);
    assert!(children[0].1.supports_action(Action::ScrollIntoView));
    for hint in [ScrollHint::LeftEdge, ScrollHint::RightEdge] {
        bridge.enqueue(request(
            children[0].0,
            Action::ScrollIntoView,
            Some(ActionData::ScrollHint(hint)),
        ));
    }
    let pending = bridge.take_current(version).unwrap();
    assert!(matches!(
        &pending.action,
        Input::Scroll(Scroll::TextIntoView {
            start: 0,
            end: 40,
            hint: Hint::LeftEdge,
            ..
        })
    ));
    assert!(
        view.accessibility_input_from_frame(
            &pending.identity,
            &pending.action,
            &metadata,
            &mut Deny
        )
        .unwrap()
    );
    let fresh = render(&view);
    let next = Version {
        revision: 2,
        ..version
    };
    bridge
        .publish(2, &view.accessibility_tree(), &fresh, next, "reveal")
        .unwrap();
    let pending = bridge.take_current(next).unwrap();
    assert!(matches!(
        &pending.action,
        Input::Scroll(Scroll::TextIntoView {
            hint: Hint::RightEdge,
            ..
        })
    ));
    assert!(
        view.accessibility_input_from_frame(&pending.identity, &pending.action, &fresh, &mut Deny)
            .unwrap()
    );
    let after = view.accessibility_tree().unwrap();
    assert_eq!(
        before.nodes[&2].text.as_ref().unwrap().revision,
        after.nodes[&2].text.as_ref().unwrap().revision
    );
    assert_eq!(
        before.nodes[&2].text.as_ref().unwrap().selection,
        after.nodes[&2].text.as_ref().unwrap().selection
    );
    assert_eq!(before.focus, after.focus);
    assert!(view.drain().is_empty());
}
#[test]
fn native_reveal_rejects_removed_runs_old_content_and_invalid_payloads() {
    let (bridge, mut view, metadata, version, field) = setup(Tag::Input, &"A".repeat(40), 60);
    let child = runs(&bridge, field)[0].0;
    bridge.enqueue(request(
        child,
        Action::ScrollIntoView,
        Some(ActionData::NumericValue(f64::NAN)),
    ));
    assert!(bridge.take().is_none());
    bridge.enqueue(request(child, Action::ScrollIntoView, None));
    let mut b = B::new();
    b.set_value(2, &"B".repeat(50), 0);
    commit(&mut view, &b);
    let fresh = render(&view);
    let next = Version {
        revision: 2,
        ..version
    };
    bridge
        .publish(2, &view.accessibility_tree(), &fresh, next, "reveal")
        .unwrap();
    assert!(bridge.take_current(next).is_none());
    bridge.enqueue(request(child, Action::ScrollIntoView, None));
    assert!(bridge.take().is_none());
    assert!(
        !metadata.text_reveal_supported(
            &view.node_identity(2).unwrap(),
            &view.accessibility_tree().unwrap().nodes[&2]
                .text
                .as_ref()
                .unwrap()
                .revision
        )
    );
    assert!(view.drain().is_empty());
    let (bridge, _, _, _, field) = setup(Tag::Input, "A", 100);
    assert!(
        !runs(&bridge, field)[0]
            .1
            .supports_action(Action::ScrollIntoView)
    );
    bridge.enqueue(request(
        runs(&bridge, field)[0].0,
        Action::ScrollIntoView,
        None,
    ));
    assert!(bridge.take().is_none());
}
