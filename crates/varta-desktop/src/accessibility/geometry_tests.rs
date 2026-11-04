use super::{
    tests::{Deny, commit, request},
    text_tests::{runs, selection},
    *,
};
use accesskit::{Role, TextDirection};
use varta_common::view::{Attribute as A, CssProperty as P, Tag};
use varta_view::{BatchWriter as B, Input, Target, TextMeasurer, View};

pub(super) fn render(view: &View) -> FrameMetadata {
    let mut font = TextMeasurer::new(
        include_bytes!("../../../varta-view/tests/fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap();
    view.rasterize_with_text(200, 160, &mut font, 20.0)
        .unwrap()
        .into_metadata()
}
pub(super) fn setup(
    tag: Tag,
    value: &str,
    width: u32,
) -> (Bridge, View, FrameMetadata, Version, NodeId) {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1,".field {display:block; height:80px; min-width:0; min-height:0; padding:4px; border:1px solid #000; border-radius:0; outline:none; background-color:#fff; color:#000; font-family:\"Varta Password Fixture\"; font-size:20px; font-weight:400; font-style:normal; line-height:20px}")
        .create_element(2,tag).set_class(2,"field").set_style(2,P::Width,&format!("{width}px")).set_value(2,value,0).insert(1,2,0);
    commit(&mut view, &b);
    let metadata = render(&view);
    let bridge = Bridge::new(None, Arc::default(), Arc::new(AtomicBool::new(false)));
    assert!(bridge.activate().is_none());
    let version = Version {
        revision: 1,
        dimensions: 200u64 << 32 | 160,
        dark: false,
        scale: 1.0f64.to_bits(),
    };
    let update = bridge
        .publish(
            1,
            &view.accessibility_tree(),
            &metadata,
            version,
            "geometry",
        )
        .unwrap();
    let field = update
        .nodes
        .iter()
        .find(|(_, node)| matches!(node.role(), Role::TextInput | Role::MultilineTextInput))
        .unwrap()
        .0;
    (bridge, view, metadata, version, field)
}

#[test]
fn native_spans_cover_visual_lines_bidi_and_clipping_without_losing_text() {
    let value = "AA אבג BBBB e\u{301}😀\r\nX\n";
    let (bridge, view, metadata, _, field) = setup(Tag::Textarea, value, 60);
    let source = view.accessibility_tree().unwrap().nodes[&2].clone();
    let geometry = metadata
        .field_text_geometry(&source.identity, &source.text.as_ref().unwrap().revision)
        .unwrap();
    let plans =
        text_layout::plan(&source, &metadata, varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT).unwrap();
    let native = runs(&bridge, field);
    assert_eq!(
        native
            .iter()
            .map(|(_, node)| node.value().unwrap())
            .collect::<String>(),
        value
    );
    assert_eq!(native.last().unwrap().1.value(), Some(""));
    assert!(
        native
            .iter()
            .any(|(_, node)| node.text_direction() == Some(TextDirection::RightToLeft))
    );
    let state = bridge.state.lock().unwrap();
    let text = source.text.as_ref().unwrap();
    for (index, ((id, node), plan)) in native.iter().zip(&plans).enumerate() {
        let bounds = node.bounds().unwrap();
        assert!(
            bounds.x0 >= f64::from(geometry.clip.x)
                && bounds.x1 <= f64::from(geometry.clip.x + geometry.clip.width)
        );
        assert!(
            bounds.y0 >= f64::from(geometry.clip.y)
                && bounds.y1 <= f64::from(geometry.clip.y + geometry.clip.height)
        );
        assert_eq!(
            node.character_positions().unwrap().len(),
            node.character_lengths().len()
        );
        assert_eq!(
            node.character_widths().unwrap().len(),
            node.character_lengths().len()
        );
        assert!(
            node.character_widths()
                .unwrap()
                .iter()
                .all(|width| *width >= 0.0)
        );
        assert_eq!(
            state.text_runs[id].source_offset(0),
            Some(text.source_offsets[plan.characters.start])
        );
        assert_eq!(
            state.text_runs[id].source_offset(plan.characters.len()),
            Some(text.source_offsets[plan.characters.end])
        );
        assert_eq!(
            node.previous_on_line(),
            index
                .checked_sub(1)
                .filter(|previous| plans[*previous].line == plan.line)
                .map(|previous| native[previous].0)
        );
        assert_eq!(
            node.next_on_line(),
            plans
                .get(index + 1)
                .filter(|next| next.line == plan.line)
                .map(|_| native[index + 1].0)
        );
    }
}

#[test]
fn reflow_never_reuses_a_text_id_for_different_source_bytes() {
    let (bridge, mut view, _, version, field) = setup(Tag::Textarea, &"A".repeat(30), 80);
    let old = runs(&bridge, field);
    let old_ranges = {
        let state = bridge.state.lock().unwrap();
        old.iter()
            .map(|(id, node)| {
                (
                    *id,
                    (
                        state.text_runs[id].source_offset(0),
                        state.text_runs[id].source_offset(node.character_lengths().len()),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    bridge.enqueue(selection(field, (old[0].0, 0), (old[0].0, 1)));
    let mut b = B::new();
    b.set_style(2, P::Width, "110px");
    commit(&mut view, &b);
    let next = Version {
        revision: 2,
        ..version
    };
    bridge
        .publish(
            2,
            &view.accessibility_tree(),
            &render(&view),
            next,
            "geometry",
        )
        .unwrap();
    let new = runs(&bridge, field);
    let state = bridge.state.lock().unwrap();
    for (id, node) in &new {
        if let Some(old) = old_ranges.get(id) {
            assert_eq!(
                *old,
                (
                    state.text_runs[id].source_offset(0),
                    state.text_runs[id].source_offset(node.character_lengths().len())
                )
            );
        }
    }
    let removed = old
        .iter()
        .find(|(id, _)| !state.text_runs.contains_key(id))
        .unwrap()
        .0;
    drop(state);
    bridge.enqueue(selection(field, (removed, 0), (removed, 1)));
    let pending = bridge
        .take_current(next)
        .expect("already decoded absolute selection survives reflow");
    assert!(
        view.accessibility_input(&pending.identity, &pending.action, &mut Deny)
            .unwrap()
    );
    assert!(bridge.take_current(next).is_none());
}

#[test]
fn wrapped_end_selection_keeps_the_painted_upstream_caret() {
    let (bridge, mut view, _, version, field) = setup(Tag::Textarea, &"A".repeat(20), 60);
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
    let metadata = render(&view);
    let movement = metadata.text_navigation(2, "End").unwrap();
    assert!(movement.upstream);
    view.input(
        &Input::TextNavigate {
            target: Target::Node(2),
            key: "End".into(),
            position: movement.position,
            preferred_x: None,
            upstream: true,
            shift: false,
        },
        &mut Deny,
    )
    .unwrap();
    let metadata = render(&view);
    let source = view.accessibility_tree().unwrap().nodes[&2].clone();
    let plans =
        text_layout::plan(&source, &metadata, varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT).unwrap();
    bridge
        .publish(
            2,
            &view.accessibility_tree(),
            &metadata,
            Version {
                revision: 2,
                ..version
            },
            "geometry",
        )
        .unwrap();
    let native = runs(&bridge, field);
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
        selected.anchor, selected.focus,
        "a collapsed upstream caret stays collapsed"
    );
    let focus = selected.focus;
    let index = native.iter().position(|(id, _)| *id == focus.node).unwrap();
    assert_eq!(plans[index].line, 0);
    assert_eq!(focus.character_index, plans[index].characters.len());
    assert_eq!(
        state.text_runs[&focus.node].source_offset(focus.character_index),
        Some(movement.position)
    );
}

#[test]
fn actual_visual_run_limit_refuses_only_the_observer_and_recovers_with_new_ids() {
    let limit = varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT;
    let value = "Aא".repeat(limit / 2);
    let (bridge, mut view, _, version, field) = setup(Tag::Input, &value, 80);
    assert_eq!(runs(&bridge, field).len(), limit);
    bridge.enqueue(request(field, Action::Focus, None));
    let mut b = B::new();
    b.set_value(2, &(value + "A"), 0);
    commit(&mut view, &b);
    assert!(view.accessibility_tree().is_ok());
    let refused = bridge
        .publish(
            2,
            &view.accessibility_tree(),
            &render(&view),
            Version {
                revision: 2,
                ..version
            },
            "geometry",
        )
        .unwrap();
    assert!(
        refused
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Label)
    );
    assert!(bridge.state.lock().unwrap().text_runs.is_empty());
    assert!(bridge.take().is_none());
    let mut b = B::new();
    b.set_value(2, "ok", 0);
    commit(&mut view, &b);
    let recovered = bridge
        .publish(
            3,
            &view.accessibility_tree(),
            &render(&view),
            Version {
                revision: 3,
                ..version
            },
            "geometry",
        )
        .unwrap();
    assert!(recovered.nodes.iter().all(|(id, _)| *id != field));
    assert!(!bridge.state.lock().unwrap().text_runs.is_empty());
}

#[test]
fn every_unicode_selection_boundary_matches_the_painted_caret() {
    let (bridge, mut view, _, version, field) =
        setup(Tag::Textarea, "Aאב e\u{301}😀\u{200f}B", 120);
    let source = view.accessibility_tree().unwrap().nodes[&2].clone();
    let offsets = source.text.as_ref().unwrap().source_offsets.clone();
    for (step, position) in offsets.into_iter().enumerate() {
        view.input(
            &Input::TextSelect {
                target: Target::Node(2),
                anchor: position,
                focus: position,
                activate: false,
            },
            &mut Deny,
        )
        .unwrap();
        let metadata = render(&view);
        let g = metadata
            .field_text_geometry(&source.identity, &source.text.as_ref().unwrap().revision)
            .unwrap();
        let caret = g.selection_carets[1];
        let clip = g.clip;
        let expected_x = caret.x.clamp(clip.x, clip.x + clip.width);
        let expected_y = (caret.y.clamp(clip.y, clip.y + clip.height)
            + (caret.y + caret.height).clamp(clip.y, clip.y + clip.height))
            / 2.0;
        bridge
            .publish(
                step as u64 + 2,
                &view.accessibility_tree(),
                &metadata,
                Version {
                    revision: step as u64 + 2,
                    ..version
                },
                "geometry",
            )
            .unwrap();
        let state = bridge.state.lock().unwrap();
        let nodes = &state.latest.as_ref().unwrap().nodes;
        let owner = &nodes.iter().find(|(id, _)| *id == field).unwrap().1;
        let focus = owner.text_selection().unwrap().focus;
        let node = &nodes.iter().find(|(id, _)| *id == focus.node).unwrap().1;
        let bounds = node.bounds().unwrap();
        let positions = node.character_positions().unwrap();
        let widths = node.character_widths().unwrap();
        let index = focus.character_index;
        let offset = if index < positions.len() {
            positions[index]
        } else if index == 0 {
            0.0
        } else {
            positions[index - 1] + widths[index - 1]
        };
        let actual_x = if node.text_direction() == Some(TextDirection::RightToLeft) {
            bounds.x1 - f64::from(offset)
        } else {
            bounds.x0 + f64::from(offset)
        };
        assert!(
            (actual_x - f64::from(expected_x)).abs() < 0.01,
            "byte {position}: native {actual_x}, painted {expected_x}"
        );
        assert!(
            ((bounds.y0 + bounds.y1) / 2.0 - f64::from(expected_y)).abs() < 0.01,
            "byte {position}: wrong visual line"
        );
    }
}

#[test]
fn password_native_metrics_follow_uniform_rendered_grapheme_masks() {
    let (bridge, mut view, _, version, field) = setup(Tag::Input, "Ae\u{301}👨‍👩‍👧B", 80);
    let mut b = B::new();
    b.set_attribute(2, A::Type, "password");
    commit(&mut view, &b);
    bridge
        .publish(
            2,
            &view.accessibility_tree(),
            &render(&view),
            Version {
                revision: 2,
                ..version
            },
            "geometry",
        )
        .unwrap();
    let native = runs(&bridge, field);
    assert_eq!(native.len(), 1);
    let node = &native[0].1;
    assert_eq!(node.value(), Some("••••"));
    assert_eq!(node.character_lengths(), [3; 4]);
    assert!(node.word_starts().is_empty());
    let widths = node.character_widths().unwrap();
    assert!(widths[0] > 0.0);
    assert!(widths.iter().all(|width| *width == widths[0]));
    let positions = node.character_positions().unwrap();
    assert!(
        positions
            .windows(2)
            .all(|pair| pair[1] - pair[0] == widths[0])
    );
}
