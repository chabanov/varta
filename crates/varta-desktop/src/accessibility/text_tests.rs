use super::{tests::*, *};
use accesskit::{Role, TextPosition, TextSelection};
use varta_common::view::{Attribute as A, Tag};
use varta_view::{AccessibilityAction as Input, BatchWriter, Target};

pub(super) fn selection(
    owner: NodeId,
    anchor: (NodeId, usize),
    focus: (NodeId, usize),
) -> ActionRequest {
    request(
        owner,
        Action::SetTextSelection,
        Some(ActionData::SetTextSelection(TextSelection {
            anchor: TextPosition {
                node: anchor.0,
                character_index: anchor.1,
            },
            focus: TextPosition {
                node: focus.0,
                character_index: focus.1,
            },
        })),
    )
}
pub(super) fn runs(bridge: &Bridge, owner: NodeId) -> Vec<(NodeId, accesskit::Node)> {
    let state = bridge.state.lock().unwrap();
    let nodes = &state.latest.as_ref().unwrap().nodes;
    let children = nodes
        .iter()
        .find(|(id, _)| *id == owner)
        .unwrap()
        .1
        .children();
    children
        .iter()
        .map(|id| nodes.iter().find(|(n, _)| n == id).unwrap().clone())
        .collect()
}
fn publish(
    bridge: &Bridge,
    view: &varta_view::View,
    metadata: &FrameMetadata,
    sequence: u64,
    version: Version,
) {
    bridge
        .publish(
            sequence,
            &view.accessibility_tree(),
            metadata,
            version,
            "text",
        )
        .unwrap();
}

#[test]
fn long_unicode_runs_preserve_text_words_lines_and_all_checked_source_offsets() {
    let (bridge, mut view, metadata, version, field) = fixture();
    let value = "word ".repeat(60) + "😀e\u{301}\r\nБ\n";
    let mut b = BatchWriter::new();
    b.set_value(2, &value, 0);
    commit(&mut view, &b);
    publish(&bridge, &view, &metadata, 2, version);
    let native = runs(&bridge, field);
    assert_eq!(
        native
            .iter()
            .map(|(_, n)| n.value().unwrap())
            .collect::<String>(),
        value
    );
    assert_eq!(native.len(), 4);
    assert_eq!(native[0].1.next_on_line(), Some(native[1].0));
    assert_eq!(native[1].1.previous_on_line(), Some(native[0].0));
    assert_eq!(native[1].1.next_on_line(), None);
    assert_eq!(native.last().unwrap().1.value(), Some(""));
    assert_eq!(
        native[0].1.word_starts(),
        (0..255).step_by(5).map(|n| n as u8).collect::<Vec<_>>()
    );
    let state = bridge.state.lock().unwrap();
    let model = view.accessibility_tree().unwrap().nodes[&2]
        .text
        .clone()
        .unwrap();
    for ((id, node), range) in native.iter().zip(&model.runs) {
        assert_eq!(node.role(), Role::TextRun);
        assert!(node.character_lengths().len() <= varta_view::ACCESSIBILITY_TEXT_RUN_CHARACTERS);
        for index in 0..=range.characters.len() {
            assert_eq!(
                state.text_runs[id].source_offset(index),
                Some(model.source_offsets[range.characters.start + index])
            );
        }
        assert_eq!(state.text_runs[id].source_offset(usize::MAX), None);
        assert_eq!(
            state.text_runs[id].source_offset(range.characters.len() + 1),
            None
        );
    }
}

#[test]
fn directional_selection_and_queued_replace_survive_selection_acknowledgement() {
    let (bridge, mut view, metadata, version, field) = fixture();
    let mut b = BatchWriter::new();
    b.set_value(2, "A😀\r\nБ", 0);
    commit(&mut view, &b);
    publish(&bridge, &view, &metadata, 2, version);
    let native = runs(&bridge, field);
    bridge.enqueue(selection(field, (native[1].0, 1), (native[0].0, 1)));
    bridge.enqueue(request(
        field,
        Action::ReplaceSelectedText,
        Some(ActionData::Value("X".into())),
    ));
    let pending = bridge.take_current(version).unwrap();
    assert!(matches!(
        pending.action,
        Input::SelectText {
            anchor: 9,
            focus: 1,
            ..
        }
    ));
    assert!(
        view.accessibility_input(&pending.identity, &pending.action, &mut Deny)
            .unwrap()
    );
    let next = Version {
        revision: 2,
        ..version
    };
    publish(&bridge, &view, &metadata, 3, next);
    assert_eq!(
        runs(&bridge, field)
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        native.iter().map(|(id, _)| *id).collect::<Vec<_>>()
    );
    let state = bridge.state.lock().unwrap();
    let owner = &state
        .latest
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(id, _)| *id == field)
        .unwrap()
        .1;
    let selected = owner.text_selection().unwrap();
    assert_eq!(
        selected.anchor,
        TextPosition {
            node: native[1].0,
            character_index: 1
        }
    );
    assert_eq!(
        selected.focus,
        TextPosition {
            node: native[0].0,
            character_index: 1
        }
    );
    drop(state);
    let replace = bridge.take_current(next).unwrap();
    assert!(
        view.accessibility_input(&replace.identity, &replace.action, &mut Deny)
            .unwrap()
    );
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AX");
    assert_eq!(bridge.state.lock().unwrap().bytes, 0);
}

#[test]
fn foreign_invalid_and_old_content_positions_are_ignored_before_queueing_or_rebinding() {
    let (bridge, mut view, metadata, version, field) = fixture();
    let mut b = BatchWriter::new();
    b.set_value(2, "abc", 0)
        .create_element(3, Tag::Input)
        .set_value(3, "other", 0)
        .insert(1, 3, 0);
    commit(&mut view, &b);
    publish(&bridge, &view, &metadata, 2, version);
    let run = runs(&bridge, field)[0].0;
    let other = bridge
        .state
        .lock()
        .unwrap()
        .text_runs
        .iter()
        .find(|(_, r)| r.owner != field)
        .map(|(id, _)| *id)
        .unwrap();
    for invalid in [
        selection(field, (run, usize::MAX), (run, 0)),
        selection(field, (run, 0), (other, 1)),
        selection(field, (NodeId(u64::MAX), 0), (run, 0)),
    ] {
        bridge.enqueue(invalid);
        assert!(bridge.take().is_none());
    }
    bridge.enqueue(selection(field, (run, 0), (run, 2)));
    let mut b = BatchWriter::new();
    b.set_value(2, "changed", 0);
    commit(&mut view, &b);
    let next = Version {
        revision: 2,
        ..version
    };
    publish(&bridge, &view, &metadata, 3, next);
    assert_ne!(runs(&bridge, field)[0].0, run);
    assert!(bridge.take_current(next).is_none());
    bridge.enqueue(selection(field, (run, 0), (run, 1)));
    assert!(bridge.take().is_none());
    // Returning to identical bytes cannot resurrect old ranges.
    let mut b = BatchWriter::new();
    b.set_value(2, "abc", 0);
    commit(&mut view, &b);
    publish(&bridge, &view, &metadata, 4, next);
    assert_ne!(runs(&bridge, field)[0].0, run);
}

#[test]
fn password_runs_expose_only_masks_and_readonly_fields_allow_selection() {
    let (bridge, mut view, metadata, version, field) = fixture();
    let mut b = BatchWriter::new();
    b.set_attribute(2, A::Type, "password")
        .set_attribute(2, A::Readonly, "")
        .set_value(2, "A😀\r\n", 0);
    commit(&mut view, &b);
    publish(&bridge, &view, &metadata, 2, version);
    let native = runs(&bridge, field);
    assert_eq!(native.len(), 1);
    assert_eq!(native[0].1.value(), Some("•••"));
    assert_eq!(native[0].1.character_lengths(), [3; 3]);
    assert!(native[0].1.word_starts().is_empty());
    bridge.enqueue(selection(field, (native[0].0, 2), (native[0].0, 1)));
    let selected = bridge.take().unwrap();
    assert!(matches!(
        selected.action,
        Input::SelectText {
            anchor: 5,
            focus: 1,
            ..
        }
    ));
    assert!(
        view.accessibility_input(&selected.identity, &selected.action, &mut Deny)
            .unwrap()
    );
    bridge.enqueue(request(
        field,
        Action::ReplaceSelectedText,
        Some(ActionData::Value("secret".into())),
    ));
    assert!(bridge.take().is_none());
    // A selection-only physical input preserves immutable native run identities.
    view.input(
        &varta_view::Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 1,
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
    publish(&bridge, &view, &metadata, 3, version);
    assert_eq!(runs(&bridge, field)[0].0, native[0].0);
    let mut b = BatchWriter::new();
    b.set_attribute(2, A::Disabled, "");
    commit(&mut view, &b);
    publish(&bridge, &view, &metadata, 4, version);
    bridge.enqueue(selection(field, (native[0].0, 0), (native[0].0, 1)));
    assert!(bridge.take().is_none());
}
