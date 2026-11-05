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
    let bridge = Bridge::new(
        None,
        Arc::default(),
        Arc::new(AtomicBool::new(false)),
        crate::shell::window::Console::SILENT,
    );
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
    let plans = text_layout::plan(
        &source,
        &metadata,
        varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT,
        &mut varta_view::AccessibilityWork::default(),
    )
    .unwrap();
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
    let plans = text_layout::plan(
        &source,
        &metadata,
        varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT,
        &mut varta_view::AccessibilityWork::default(),
    )
    .unwrap();
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

fn rounded_ancestors(view: &mut View, count: u32) {
    let mut batch = B::new();
    let mut parent = 1;
    for id in 3..3 + count {
        batch
            .create_element(id, Tag::Div)
            .set_style(id, P::Display, "block")
            .set_style(id, P::Width, "130px")
            .set_style(id, P::Height, "90px")
            .set_style(id, P::Overflow, "hidden")
            .set_style(
                id,
                P::BorderRadius,
                &format!("{}%", 50.0 - (id - 3) as f32 / 1000.0),
            )
            .insert(parent, id, 0);
        parent = id;
    }
    batch.insert(parent, 2, 0);
    commit(view, &batch);
}

#[test]
fn curved_native_spans_keep_source_ranges_and_use_individual_visible_heights() {
    let (bridge, mut view, _, version, field) = setup(Tag::Textarea, "AAAA AAAA AAAA", 120);
    rounded_ancestors(&mut view, 1);
    let metadata = render(&view);
    let source = view.accessibility_tree().unwrap();
    let field_source = &source.nodes[&2];
    let geometry = metadata
        .field_text_geometry(
            &field_source.identity,
            &field_source.text.as_ref().unwrap().revision,
        )
        .unwrap();
    assert!(
        geometry.clusters[0].bounds.x < 10.0,
        "logical geometry remains untrimmed"
    );
    let mut work = source.observer_work();
    let plans = text_layout::plan(
        field_source,
        &metadata,
        varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT,
        &mut work,
    )
    .unwrap();
    bridge
        .publish(
            2,
            &Ok(source),
            &metadata,
            Version {
                revision: 2,
                ..version
            },
            "geometry",
        )
        .unwrap();
    let native = runs(&bridge, field);
    assert_eq!(
        native
            .iter()
            .map(|(_, node)| node.value().unwrap())
            .collect::<String>(),
        "AAAA AAAA AAAA"
    );
    assert_eq!(native.len(), plans.len());
    assert!(plans.windows(2).any(|pair| pair[0].line == pair[1].line
        && pair[0].metrics.as_ref().unwrap().bounds.y0
            != pair[1].metrics.as_ref().unwrap().bounds.y0));
    for ((_, node), plan) in native.iter().zip(&plans) {
        assert_eq!(node.bounds(), Some(plan.metrics.as_ref().unwrap().bounds));
        assert_eq!(
            node.character_positions().unwrap().len(),
            plan.characters.len()
        );
    }
}

#[test]
fn curved_geometry_work_refusal_clears_actions_and_recovers_without_a_guest_failure() {
    let (bridge, mut view, _, version, field) = setup(Tag::Textarea, &"A".repeat(16_000), 600);
    rounded_ancestors(&mut view, 125);
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
    let source = view.accessibility_tree().unwrap();
    let before = view.snapshot();
    let mut work = source.observer_work();
    let refusal = text_layout::plan(
        &source.nodes[&2],
        &metadata,
        varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT,
        &mut work,
    )
    .err();
    assert_eq!(
        refusal,
        Some(varta_view::AccessibilityError::WorkLimit {
            limit: varta_view::ACCESSIBILITY_TEXT_WORK_LIMIT
        }),
        "one publication shares preflight and geometry allowance: {work:?}"
    );
    bridge.enqueue(request(field, Action::Focus, None));
    let update = bridge
        .publish(
            2,
            &Ok(source),
            &metadata,
            Version {
                revision: 2,
                ..version
            },
            "geometry",
        )
        .unwrap();
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Label)
    );
    assert!(bridge.state.lock().unwrap().text_runs.is_empty());
    assert!(bridge.take().is_none());
    assert_eq!(view.snapshot(), before);
    let mut batch = B::new();
    batch.set_value(2, "ok", 0);
    commit(&mut view, &batch);
    let update = bridge
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
    assert!(update.nodes.iter().all(|(id, _)| *id != field));
    assert!(!bridge.state.lock().unwrap().text_runs.is_empty());
}

/// The published node whose AccessKit label or value is `text`.
fn named<'a>(update: &'a accesskit::TreeUpdate, text: &str) -> &'a accesskit::Node {
    &update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(text) || node.value() == Some(text))
        .unwrap_or_else(|| panic!("{text}"))
        .1
}
fn bounds_of(update: &accesskit::TreeUpdate, text: &str) -> accesskit::Rect {
    named(update, text)
        .bounds()
        .unwrap_or_else(|| panic!("{text} has no bounds"))
}

const SCALE: f32 = 2.0;
const OPTIONS: std::ops::Range<u32> = 20..32;
const FIRST: usize = 4;
const LABEL_OFFSET: u32 = 100;

fn offscreen_view() -> View {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1, ".pane {height:50px;width:100px;overflow:auto}.space {height:120px}.below {height:20px}.clip {height:30px;width:100px;overflow:hidden}.clipped {margin-top:40px;height:10px}.pick {display:block; width:80px; height:30px; min-width:0; min-height:0; padding:4px; border:2px solid #000; border-radius:0; outline:none; background-color:#fff; color:#000; font-family:\"Varta Password Fixture\"; font-size:20px; font-weight:400; font-style:normal; line-height:20px}")
        .create_element(2, Tag::Div).set_class(2, "pane").insert(1, 2, 0)
        .create_element(3, Tag::Div).set_class(3, "space").insert(2, 3, 0)
        .create_element(4, Tag::Section).set_class(4, "below").set_attribute(4, A::AriaLabel, "Below").insert(2, 4, 0)
        .create_element(5, Tag::Div).set_class(5, "clip").insert(1, 5, 0)
        .create_element(6, Tag::Section).set_class(6, "clipped").set_attribute(6, A::AriaLabel, "Clipped").insert(5, 6, 0)
        .create_element(7, Tag::Div).insert(1, 7, 0)
        .create_text(8, "Lone text").insert(7, 8, 0)
        .create_element(9, Tag::Select).set_class(9, "pick").insert(1, 9, 0);
    for id in OPTIONS {
        b.create_element(id, Tag::Option)
            .set_attribute(id, A::Value, &id.to_string())
            .create_text(id + LABEL_OFFSET, &format!("O{id}"))
            .insert(id, id + LABEL_OFFSET, 0)
            .insert(9, id, 0);
    }
    commit(&mut view, &b);
    view
}
/// The presented surface size of `present`, in device pixels.
const PRESENTED_WIDTH: u32 = 400;
const PRESENTED_HEIGHT: u32 = 600;
/// The frame version `present` publishes for `revision`.
fn presented_version(revision: u64) -> Version {
    Version {
        revision,
        dimensions: u64::from(PRESENTED_WIDTH) << 32 | u64::from(PRESENTED_HEIGHT),
        dark: false,
        scale: f64::from(SCALE).to_bits(),
    }
}
fn present(view: &View, revision: u64, bridge: &Bridge) -> (FrameMetadata, accesskit::TreeUpdate) {
    let mut font = TextMeasurer::new(
        include_bytes!("../../../varta-view/tests/fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap();
    let metadata = view
        .rasterize_scaled(
            PRESENTED_WIDTH,
            PRESENTED_HEIGHT,
            SCALE,
            Some((&mut font, 20.0)),
        )
        .unwrap()
        .into_metadata();
    let version = presented_version(revision);
    let update = bridge
        .publish(
            revision,
            &view.accessibility_tree(),
            &metadata,
            version,
            "geometry",
        )
        .unwrap();
    (metadata, update)
}
fn active_bridge() -> Bridge {
    let bridge = Bridge::new(
        None,
        Arc::default(),
        Arc::new(AtomicBool::new(false)),
        crate::shell::window::Console::SILENT,
    );
    assert!(bridge.activate().is_none());
    bridge
}

#[test]
fn scrolled_out_clipped_and_text_nodes_publish_presented_layout_bounds() {
    let mut view = offscreen_view();
    let bridge = active_bridge();
    let (metadata, update) = present(&view, 1, &bridge);
    let visible = metadata.node_bounds_map();
    // Neither offscreen section has a visible fragment.
    assert!(!visible.contains_key(&4) && !visible.contains_key(&6));
    let scale = f64::from(SCALE);
    let below = bounds_of(&update, "Below");
    let pane_top = f64::from(visible[&2].y);
    // Below the 50px scrollport, at its unscrolled layout position.
    assert_eq!(below.y0, pane_top + 120.0 * scale);
    assert_eq!(
        (below.width(), below.height()),
        (100.0 * scale, 20.0 * scale)
    );
    assert!(below.y0 >= f64::from(visible[&2].y + visible[&2].height));
    let clipped = bounds_of(&update, "Clipped");
    let clip_top = f64::from(visible[&5].y);
    assert_eq!(clipped.y0, clip_top + 40.0 * scale);
    assert_eq!(clipped.height(), 10.0 * scale);
    assert!(clipped.y0 >= clip_top + 30.0 * scale);
    // The text node's own box contains its painted line fragment.
    let text = bounds_of(&update, "Lone text");
    let line = visible[&7];
    assert!(text.x0 <= f64::from(line.x) && text.y0 <= f64::from(line.y));
    assert!(
        text.x1 >= f64::from(line.x + line.width) && text.y1 >= f64::from(line.y + line.height)
    );
    // Options of a closed select have no box of their own.
    assert!(named(&update, "O20").bounds().is_none());
    // Bounds follow the presented scroll offset of the acknowledged frame.
    let source = view.accessibility_tree().unwrap().nodes[&2]
        .identity
        .clone();
    assert!(
        view.accessibility_input_from_frame(
            &source,
            &AccessibilityAction::Scroll(varta_view::AccessibilityScrollAction::To(
                varta_view::AccessibilityScrollOffset::new(90.0).unwrap()
            )),
            &metadata,
            &mut Deny
        )
        .unwrap()
    );
    let (metadata, update) = present(&view, 2, &bridge);
    let below = bounds_of(&update, "Below");
    assert_eq!(below.y0, pane_top + 30.0 * scale);
    // Fully scrolled: now inside the scrollport.
    let pane = metadata.node_bounds_map()[&2];
    assert!(below.y1 <= f64::from(pane.y + pane.height));
}

#[test]
fn open_select_list_options_publish_presented_rows_and_no_reachable_hidden_rows() {
    let mut view = offscreen_view();
    view.input(
        &Input::Click {
            target: Target::Node(9),
        },
        &mut Deny,
    )
    .unwrap();
    view.input(
        &Input::SelectScroll {
            target: Target::Node(9),
            first: FIRST,
        },
        &mut Deny,
    )
    .unwrap();
    let bridge = active_bridge();
    let (metadata, update) = present(&view, 1, &bridge);
    let menu = metadata.select_menu().unwrap();
    assert_eq!(menu.first, FIRST);
    let row = f64::from(menu.row_height * SCALE);
    let top = f64::from(menu.viewport.y);
    let bottom = top + f64::from(menu.viewport.height);
    let (mut shown, mut refused, mut outside) = (0, 0, 0);
    for (index, id) in OPTIONS.enumerate() {
        let expected = top + (index as f64 - FIRST as f64) * row;
        let Some(bounds) = named(&update, &format!("O{id}")).bounds() else {
            // Scrolled out of the list: no published node clips to the list,
            // so a hidden row over the page would win the hit test there.
            assert!(expected + row <= top + 1e-3 || expected >= bottom - 1e-3);
            refused += 1;
            continue;
        };
        assert_eq!(bounds.x0, f64::from(menu.viewport.x));
        assert_eq!(bounds.width(), f64::from(menu.viewport.width));
        assert!(
            (bounds.y0 - expected).abs() < 1e-3,
            "O{id}: {} {expected}",
            bounds.y0
        );
        let center = ((bounds.x0 + bounds.x1) / 2.0, (bounds.y0 + bounds.y1) / 2.0);
        if !(center.1 > top && center.1 < bottom) {
            // A published hidden row lies wholly outside the frame.
            assert!(bounds.y1 <= 0.0 || bounds.y0 >= f64::from(metadata.height));
            outside += 1;
            continue;
        }
        // A presented row is the rectangle its pointer hit uses.
        shown += 1;
        assert_eq!(
            metadata.select_hit(center.0 as f32, center.1 as f32),
            Some(varta_view::SelectHit::Choice {
                select: 9,
                option: id
            })
        );
    }
    assert!(shown > 0 && refused > 0 && outside > 0);
    assert_eq!(shown + refused + outside, OPTIONS.len());
}

/// The labelled node `accesskit_consumer`'s `node_at_point` returns at
/// (`x`, `y`) under AccessKit's common filter. Every platform adapter (macOS,
/// AT-SPI, UIA) hit-tests through this one consumer search, which the
/// vendored patch makes clip-aware.
fn hit_label(update: &accesskit::TreeUpdate, x: f64, y: f64) -> Option<String> {
    let tree = accesskit_consumer::Tree::new(update.clone(), true);
    let state = tree.state();
    let node = state.root().node_at_point(
        accesskit::Point::new(x, y),
        &accesskit_consumer::common_filter,
    )?;
    let id = node.locate().0;
    let (_, node) = update.nodes.iter().find(|(native, _)| *native == id)?;
    node.label().map(str::to_owned)
}

/// AccessKit's common filter result for the node labelled `label`.
fn filtered(update: &accesskit::TreeUpdate, label: &str) -> accesskit_consumer::FilterResult {
    let tree = accesskit_consumer::Tree::new(update.clone(), true);
    let state = tree.state();
    let id = update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(label))
        .unwrap_or_else(|| panic!("{label}"))
        .0;
    accesskit_consumer::common_filter(&state.node_by_tree_local_id(id, TreeId::ROOT).unwrap())
}

/// The point lies in `label`'s published box and the filter keeps that node,
/// so a search that ignored `clips_children` (upstream's) would return it.
fn reachable_without_clipping(update: &accesskit::TreeUpdate, label: &str, x: f64, y: f64) {
    assert!(
        bounds_of(update, label).contains(accesskit::Point::new(x, y)),
        "{label}"
    );
    assert_eq!(
        filtered(update, label),
        accesskit_consumer::FilterResult::Include,
        "{label}"
    );
}

/// A 20px heading over a 50px scroll pane of four 40px items.
fn heading_and_pane() -> View {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1, ".head {margin:0;height:20px} .pane {height:50px;width:100px;overflow:auto} .item {height:40px}")
        .create_element(2, Tag::H1).set_class(2, "head").set_attribute(2, A::AriaLabel, "Head").insert(1, 2, 0)
        .create_element(3, Tag::Div).set_class(3, "pane").insert(1, 3, 0);
    for (index, label) in ["One", "Two", "Three", "Four"].into_iter().enumerate() {
        let id = 10 + index as u32;
        b.create_element(id, Tag::Section)
            .set_class(id, "item")
            .set_attribute(id, A::AriaLabel, label)
            .insert(3, id, 0);
    }
    commit(&mut view, &b);
    view
}

#[test]
fn hit_tests_over_visible_content_never_return_scrolled_out_items() {
    let mut view = heading_and_pane();
    let bridge = active_bridge();
    let (metadata, _) = present(&view, 1, &bridge);
    let source = view.accessibility_tree().unwrap().nodes[&3]
        .identity
        .clone();
    assert!(
        view.accessibility_input_from_frame(
            &source,
            &AccessibilityAction::Scroll(varta_view::AccessibilityScrollAction::To(
                varta_view::AccessibilityScrollOffset::new(90.0).unwrap()
            )),
            &metadata,
            &mut Deny
        )
        .unwrap()
    );
    let (_, update) = present(&view, 2, &bridge);
    let scale = f64::from(SCALE);
    // "Two" (40..80 in the pane) is scrolled to -50..-10 under the pane top at
    // 20: its published box overlaps the visible heading.
    let two = bounds_of(&update, "Two");
    assert_eq!((two.y0, two.y1), (-30.0 * scale, 10.0 * scale));
    let pane = &update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == accesskit::Role::ScrollView)
        .unwrap()
        .1;
    assert!(pane.clips_children());
    let on_heading = (10.0 * scale, 5.0 * scale);
    // Without the clip the invisible item would answer there.
    reachable_without_clipping(&update, "Two", on_heading.0, on_heading.1);
    assert_eq!(
        hit_label(&update, on_heading.0, on_heading.1).as_deref(),
        Some("Head")
    );
    // Inside the pane the visible items answer: "Three" at 80..120 is shown
    // at 10..50 and "Four" at 50..90, cut at the pane bottom (70).
    for (y, label) in [(30.0, "Three"), (65.0, "Four")] {
        assert_eq!(
            hit_label(&update, 10.0 * scale, y * scale).as_deref(),
            Some(label),
            "{y}"
        );
    }
    // Below the pane, where "Four"'s layout box continues (it is published
    // only as its presented part), nothing is hit but the window.
    assert_ne!(
        hit_label(&update, 10.0 * scale, 75.0 * scale).as_deref(),
        Some("Four")
    );
    // AccessKit's own filter, which every adapter uses for the platform tree,
    // keeps a clipping area's children inside it and the nearest one beyond
    // each edge ("Two"), and omits farther ones ("One") until scrolling
    // brings them next to the area.
    use accesskit_consumer::FilterResult::{ExcludeSubtree, Include};
    assert_eq!(
        ["One", "Two", "Three", "Four"].map(|label| filtered(&update, label)),
        [ExcludeSubtree, Include, Include, Include]
    );
}

#[test]
fn scrolling_an_included_neighbour_into_view_brings_an_omitted_item_into_the_tree() {
    use accesskit_consumer::FilterResult::{ExcludeSubtree, Include};
    let mut view = heading_and_pane();
    let bridge = active_bridge();
    let (metadata, _) = present(&view, 1, &bridge);
    let source = view.accessibility_tree().unwrap().nodes[&3]
        .identity
        .clone();
    assert!(
        view.accessibility_input_from_frame(
            &source,
            &AccessibilityAction::Scroll(varta_view::AccessibilityScrollAction::To(
                varta_view::AccessibilityScrollOffset::new(90.0).unwrap()
            )),
            &metadata,
            &mut Deny
        )
        .unwrap()
    );
    let revision = 2;
    let (metadata, update) = present(&view, revision, &bridge);
    assert_eq!(filtered(&update, "One"), ExcludeSubtree);
    // An assistive client reaches the nearest included item past the top
    // edge and asks it to scroll into view; the item before it, omitted until
    // then, joins the platform tree.
    let two = update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Two"))
        .unwrap()
        .0;
    bridge.enqueue(request(two, Action::ScrollIntoView, None));
    let input = bridge.take_current(presented_version(revision)).unwrap();
    assert!(
        view.accessibility_input_from_frame(&input.identity, &input.action, &metadata, &mut Deny)
            .unwrap()
    );
    let (_, update) = present(&view, revision + 1, &bridge);
    let scale = f64::from(SCALE);
    // "Two" (40..80) now starts at the pane top (20); "One" is the nearest
    // item past the top edge and "Four" the nearest past the bottom.
    assert_eq!(bounds_of(&update, "Two").y0, 20.0 * scale);
    assert_eq!(
        ["One", "Two", "Three", "Four"].map(|label| filtered(&update, label)),
        [Include; 4]
    );
}

#[test]
fn hit_tests_over_visible_content_never_return_clipped_away_boxes() {
    let mut view = View::default();
    let mut b = B::new();
    // The section is moved 30px up out of its 30px clip box, back over the
    // heading before it.
    b.stylesheet(1, ".head {margin:0;height:20px} .clip {height:30px;width:100px;overflow:hidden} .up {position:relative;top:-30px;height:20px}")
        .create_element(2, Tag::H1).set_class(2, "head").set_attribute(2, A::AriaLabel, "Head").insert(1, 2, 0)
        .create_element(3, Tag::Div).set_class(3, "clip").insert(1, 3, 0)
        .create_element(4, Tag::Section).set_class(4, "up").set_attribute(4, A::AriaLabel, "Up").insert(3, 4, 0);
    commit(&mut view, &b);
    let bridge = active_bridge();
    let (metadata, update) = present(&view, 1, &bridge);
    let scale = f64::from(SCALE);
    assert!(!metadata.node_bounds_map().contains_key(&4));
    let up = bounds_of(&update, "Up");
    assert_eq!((up.y0, up.y1), (-10.0 * scale, 10.0 * scale));
    reachable_without_clipping(&update, "Up", 10.0 * scale, 5.0 * scale);
    assert_eq!(
        hit_label(&update, 10.0 * scale, 5.0 * scale).as_deref(),
        Some("Head")
    );
}

#[test]
fn hidden_boxes_under_a_scroll_border_stay_unbounded() {
    let mut view = View::default();
    let mut b = B::new();
    // The pane's bottom border lies inside its bounds but outside its
    // scrollport; the third item's box ends under it.
    b.stylesheet(1, ".pane {height:50px;width:100px;overflow:auto;border-bottom:40px solid #000} .item {height:40px}")
        .create_element(3, Tag::Div).set_class(3, "pane").insert(1, 3, 0);
    for (index, label) in ["One", "Two", "Three", "Four"].into_iter().enumerate() {
        let id = 10 + index as u32;
        b.create_element(id, Tag::Section)
            .set_class(id, "item")
            .set_attribute(id, A::AriaLabel, label)
            .insert(3, id, 0);
    }
    commit(&mut view, &b);
    let bridge = active_bridge();
    let (_, update) = present(&view, 1, &bridge);
    assert!(named(&update, "Three").bounds().is_none());
    // "Four" (120..160) lies wholly below the border (50..90).
    assert_eq!(bounds_of(&update, "Four").y0, 120.0 * f64::from(SCALE));
}
