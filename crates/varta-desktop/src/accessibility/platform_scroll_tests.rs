use super::text_scroll_planner as text_scroll;
use accesskit::{
    Action, ActionData, Node, NodeId, Role, TextPosition, TextSelection, TreeId, TreeInfo,
    TreeUpdate,
};
use accesskit_consumer::Tree;
const FIELD: NodeId = NodeId(2);
const RUN: NodeId = NodeId(3);
fn fixture(role: Role, scroll: bool, value: &str, lengths: &[u8]) -> Tree {
    let mut window = Node::new(Role::Window);
    window.set_children([FIELD]);
    let mut field = Node::new(role);
    field.set_children([RUN]);
    if scroll {
        field.add_action(Action::ScrollIntoView);
    }
    field.set_text_selection(TextSelection {
        anchor: TextPosition {
            node: RUN,
            character_index: 0,
        },
        focus: TextPosition {
            node: RUN,
            character_index: 1.min(lengths.len()),
        },
    });
    let mut run = Node::new(Role::TextRun);
    run.set_value(value);
    run.set_character_lengths(lengths.to_vec());
    Tree::new(
        TreeUpdate {
            nodes: vec![(NodeId(1), window), (FIELD, field), (RUN, run)],
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: FIELD,
        },
        true,
    )
}
#[test]
fn exact_utf16_boundaries_reject_overflow_surrogates_crlf_and_past_end() {
    let tree = fixture(Role::MultilineTextInput, true, "A😀\r\nB", &[1, 4, 2, 1]);
    let node = tree
        .state()
        .node_by_tree_local_id(FIELD, TreeId::ROOT)
        .unwrap();
    for (start, length) in [(0, 6), (1, 2), (3, 2), (5, 1), (6, 0)] {
        let range = text_scroll::exact_range(&node, start, length).unwrap();
        assert_eq!(range.start().to_global_utf16_index(), start);
        assert_eq!(range.end().to_global_utf16_index(), start + length);
    }
    for (start, length) in [
        (1, usize::MAX),
        (usize::MAX, 1),
        (2, 0),
        (1, 1),
        (3, 1),
        (4, 1),
        (7, 0),
    ] {
        assert!(
            text_scroll::exact_range(&node, start, length).is_none(),
            "{start}+{length}"
        );
    }
}
#[test]
fn visible_range_request_has_scroll_action_and_never_changes_selection() {
    let tree = fixture(Role::TextInput, true, "A😀Z", &[1, 4, 1]);
    let node = tree
        .state()
        .node_by_tree_local_id(FIELD, TreeId::ROOT)
        .unwrap();
    let before = node.data().text_selection().cloned();
    let request = text_scroll::range_scroll_request(&node, tree.state(), 1, 2).unwrap();
    assert_eq!(request.action, Action::ScrollIntoView);
    assert_eq!(request.target_node, FIELD);
    assert_eq!(request.target_tree, TreeId::ROOT);
    let Some(ActionData::SetTextSelection(range)) = request.data else {
        panic!("range address")
    };
    assert_eq!(
        range.anchor,
        TextPosition {
            node: RUN,
            character_index: 1
        }
    );
    assert_eq!(
        range.focus,
        TextPosition {
            node: RUN,
            character_index: 2
        }
    );
    assert_eq!(node.data().text_selection(), before.as_ref());
}
#[test]
fn scroll_to_visible_uses_current_selection_and_respects_own_action() {
    for enabled in [false, true] {
        let tree = fixture(Role::TextInput, enabled, "AB", &[1, 1]);
        let node = tree
            .state()
            .node_by_tree_local_id(FIELD, TreeId::ROOT)
            .unwrap();
        let request = text_scroll::selection_scroll_request(&node, tree.state());
        assert_eq!(request.is_some(), enabled);
        assert_eq!(
            text_scroll::range_scroll_request(&node, tree.state(), 0, 1).is_some(),
            enabled
        );
        if let Some(request) = request {
            let Some(ActionData::SetTextSelection(selection)) = request.data else {
                panic!("range address")
            };
            assert_eq!(Some(&selection), node.data().text_selection());
            assert_eq!(request.action, Action::ScrollIntoView);
        }
    }
}
#[test]
fn unsupported_text_role_and_invalid_ranges_have_no_request() {
    let tree = fixture(Role::Button, true, "AB", &[1, 1]);
    let node = tree
        .state()
        .node_by_tree_local_id(FIELD, TreeId::ROOT)
        .unwrap();
    assert!(text_scroll::range_scroll_request(&node, tree.state(), 0, 1).is_none());
    assert!(text_scroll::selection_scroll_request(&node, tree.state()).is_none());
}
#[test]
fn password_range_indexes_address_displayed_masks() {
    let tree = fixture(Role::PasswordInput, true, "••", &[3, 3]);
    let node = tree
        .state()
        .node_by_tree_local_id(FIELD, TreeId::ROOT)
        .unwrap();
    assert!(text_scroll::exact_range(&node, 1, 1).is_some());
    assert!(text_scroll::exact_range(&node, 1, 2).is_none());
}
#[test]
fn visible_range_includes_whole_lines_and_ignores_horizontal_clipping() {
    let mut root = Node::new(Role::TextInput);
    root.set_children([NodeId(2), NodeId(3), NodeId(4), NodeId(5), NodeId(6)]);
    let parts = [
        ("H\r\n", vec![1, 2], -2.0, -2.0, 0.0),
        ("A😀", vec![1, 4], 0.0, 10.0, 20.0),
        ("אב\r\n", vec![2, 2, 2], 0.0, 10.0, 0.0),
        ("Z", vec![1], 10.0, 15.0, 10.0),
        ("\n", vec![1], 15.0, 15.0, 0.0),
    ];
    let mut nodes = vec![(NodeId(1), root)];
    for (i, (value, lengths, top, bottom, width)) in parts.into_iter().enumerate() {
        let mut run = Node::new(Role::TextRun);
        run.set_value(value);
        run.set_character_lengths(lengths);
        run.set_bounds(accesskit::Rect::new(0.0, top, width, bottom));
        nodes.push((NodeId(i as u64 + 2), run));
    }
    let tree = Tree::new(
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        },
        true,
    );
    let node = tree.state().root();
    assert_eq!(text_scroll::visible_range(&node), Some(3..11));
}
#[test]
fn empty_visible_line_is_available_and_missing_geometry_is_unknown() {
    let tree = fixture(Role::TextInput, true, "", &[]);
    let node = tree
        .state()
        .node_by_tree_local_id(FIELD, TreeId::ROOT)
        .unwrap();
    assert_eq!(text_scroll::visible_range(&node), None);
}

#[test]
fn clipped_empty_and_nonprinting_lines_have_defined_ranges() {
    for (value, lengths, top, bottom, expected) in [
        ("", Vec::<u8>::new(), 0.0, 10.0, Some(0..0)),
        ("\r\n", vec![2], 0.0, 10.0, Some(0..2)),
        ("ABC", vec![1, 1, 1], 10.0, 10.0, Some(0..0)),
    ] {
        let mut field = Node::new(Role::TextInput);
        field.set_children([RUN]);
        let mut run = Node::new(Role::TextRun);
        run.set_value(value);
        run.set_character_lengths(lengths);
        run.set_bounds(accesskit::Rect::new(5.0, top, 5.0, bottom));
        let tree = Tree::new(
            TreeUpdate {
                nodes: vec![(FIELD, field), (RUN, run)],
                tree: Some(TreeInfo::new(FIELD)),
                tree_id: TreeId::ROOT,
                focus: FIELD,
            },
            true,
        );
        assert_eq!(text_scroll::visible_range(&tree.state().root()), expected);
    }
}

#[test]
fn platform_range_scroll_reaches_field_without_moving_caret_or_focus() {
    use super::{geometry_tests::setup, tests::Deny};
    use varta_common::view::Tag;
    use varta_view::{
        AccessibilityAction as Input, AccessibilityScrollAction as Scroll,
        AccessibilityScrollHint as Hint,
    };
    let value = format!("{}e\u{301}😀", "A".repeat(35));
    let (bridge, mut view, metadata, version, field) = setup(Tag::Input, &value, 60);
    let before = view.accessibility_tree().unwrap();
    let tree = Tree::new(bridge.state.lock().unwrap().latest.clone().unwrap(), true);
    let node = tree
        .state()
        .node_by_tree_local_id(field, TreeId::ROOT)
        .unwrap();
    assert!(node.data().supports_action(Action::ScrollIntoView));
    let request = text_scroll::range_scroll_request(&node, tree.state(), 37, 2).unwrap();
    bridge.enqueue(request);
    let pending = bridge.take_current(version).unwrap();
    assert!(matches!(
        pending.action,
        Input::Scroll(Scroll::TextIntoView {
            start: 38,
            end: 42,
            hint: Hint::Nearest,
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
    let after = view.accessibility_tree().unwrap();
    assert_eq!(
        before.nodes[&2].text.as_ref().unwrap().selection,
        after.nodes[&2].text.as_ref().unwrap().selection
    );
    assert_eq!(before.nodes[&2].value, after.nodes[&2].value);
    assert_eq!(before.focus, after.focus);
    assert!(view.drain().is_empty());
}

#[test]
fn platform_range_scroll_rejects_foreign_positions_and_old_content() {
    use super::{
        geometry_tests::{render, setup},
        tests::{commit, request},
    };
    use varta_common::view::Tag;
    use varta_view::BatchWriter;
    let (bridge, mut view, _, version, field) = setup(Tag::Input, &"A".repeat(40), 60);
    let tree = Tree::new(bridge.state.lock().unwrap().latest.clone().unwrap(), true);
    let node = tree
        .state()
        .node_by_tree_local_id(field, TreeId::ROOT)
        .unwrap();
    let old = text_scroll::range_scroll_request(&node, tree.state(), 30, 5).unwrap();
    let Some(ActionData::SetTextSelection(mut foreign)) = old.data.clone() else {
        panic!("range payload")
    };
    foreign.focus.node = NodeId(u64::MAX);
    bridge.enqueue(request(
        field,
        Action::ScrollIntoView,
        Some(ActionData::SetTextSelection(foreign)),
    ));
    assert!(bridge.take().is_none());
    bridge.enqueue(old.clone());
    let mut b = BatchWriter::new();
    b.set_value(2, &"B".repeat(40), 0);
    commit(&mut view, &b);
    let next = super::Version {
        revision: 2,
        ..version
    };
    bridge
        .publish(
            2,
            &view.accessibility_tree(),
            &render(&view),
            next,
            "changed",
        )
        .unwrap();
    assert!(bridge.take_current(next).is_none());
    bridge.enqueue(old);
    assert!(bridge.take().is_none());
}

#[test]
fn owner_scroll_to_visible_decodes_logical_selection_in_source_bytes() {
    use super::{
        geometry_tests::{render, setup},
        tests::{commit, request},
    };
    use varta_common::view::{Attribute, Tag};
    use varta_view::{
        AccessibilityAction as Input, AccessibilityScrollAction as Scroll, BatchWriter,
    };
    let (bridge, mut view, _, version, field) = setup(Tag::Input, &"A".repeat(40), 60);
    let mut b = BatchWriter::new();
    b.set_attribute(2, Attribute::Readonly, "");
    commit(&mut view, &b);
    let next = super::Version {
        revision: 2,
        ..version
    };
    bridge
        .publish(
            2,
            &view.accessibility_tree(),
            &render(&view),
            next,
            "readonly",
        )
        .unwrap();
    let selection = view.accessibility_tree().unwrap().nodes[&2]
        .text
        .as_ref()
        .unwrap()
        .selection
        .unwrap();
    bridge.enqueue(request(field, Action::ScrollIntoView, None));
    let pending = bridge.take_current(next).unwrap();
    assert!(
        matches!(pending.action,Input::Scroll(Scroll::TextIntoView{start,end,..}) if start==selection.anchor.min(selection.focus)&&end==selection.anchor.max(selection.focus))
    );
}
