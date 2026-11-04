use varta_common::view::{Attribute as A, EventKind, Tag};
use varta_view::{
    ACCESSIBILITY_TEXT_RUN_CHARACTERS as CHUNK, ACCESSIBILITY_TEXT_RUN_LIMIT as RUN_LIMIT, Access,
    AccessibilityAction as Action, AccessibilityError, AccessibilityText, BatchWriter as B, Broker,
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
fn field(value: &str, password: bool) -> View {
    let mut b = B::new();
    b.create_element(2, if password { Tag::Input } else { Tag::Textarea })
        .set_value(2, value, 0)
        .insert(1, 2, 0)
        .listen(2, EventKind::Input, 7, Filter::default());
    if password {
        b.set_attribute(2, A::Type, "password");
    }
    let mut view = View::default();
    commit(&mut view, &b);
    view
}
fn text(view: &View) -> AccessibilityText {
    view.accessibility_tree().unwrap().nodes[&2]
        .text
        .as_ref()
        .unwrap()
        .clone()
}
fn select(view: &mut View, t: &AccessibilityText, anchor: usize, focus: usize) -> bool {
    view.accessibility_input(
        &view.node_identity(2).unwrap(),
        &Action::SelectText {
            revision: t.revision.clone(),
            anchor,
            focus,
        },
        &mut Deny,
    )
    .unwrap()
}

#[test]
fn unicode_crlf_trailing_line_and_directional_selection_round_trip_without_input_events() {
    let mut view = field("A😀e\u{301}\r\nБ\n", false);
    let before = view.snapshot();
    let model = text(&view);
    assert_eq!(model.source_offsets, [0, 1, 5, 6, 8, 10, 12, 13]);
    assert_eq!(model.character_lengths, [1, 4, 1, 2, 2, 2, 1]);
    assert_eq!(model.lines, [0..5, 5..7, 7..7]);
    assert_eq!(view.snapshot(), before);
    assert!(select(&mut view, &model, 12, 1));
    let selected = text(&view);
    assert_eq!(
        selected.selection.unwrap(),
        varta_view::AccessibilitySelection {
            anchor: 12,
            focus: 1
        }
    );
    assert_eq!(model.revision, selected.revision);
    assert!(view.drain().is_empty());
    assert!(select(&mut view, &selected, 5, 8));
    assert!(
        view.accessibility_input(
            &view.node_identity(2).unwrap(),
            &Action::ReplaceSelectedText("X".into()),
            &mut Deny
        )
        .unwrap()
    );
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["value"],
        "A😀X\r\nБ\n"
    );
    let events = view.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].record.kind, EventKind::Input);
    assert!(!events[0].gesture);
    let before = view.snapshot();
    assert!(!select(&mut view, &model, 0, 1));
    assert_eq!(view.snapshot(), before);
}

#[test]
fn echoes_keep_content_identity_but_mutation_and_undo_never_revalidate_old_ranges() {
    let mut view = field("abc", false);
    let original = text(&view);
    let mut b = B::new();
    b.set_value(2, "abc", 0);
    commit(&mut view, &b);
    assert_eq!(original.revision, text(&view).revision);
    assert!(select(&mut view, &original, 1, 2));
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "X".into(),
        },
        &mut Deny,
    )
    .unwrap();
    let edited = text(&view);
    assert_ne!(original.revision, edited.revision);
    view.input(
        &Input::TextKey {
            target: Target::Node(2),
            key: "z".into(),
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
            text: String::new(),
            select_all: false,
            history: Some(varta_view::HistoryAction::Undo),
        },
        &mut Deny,
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "abc");
    assert_ne!(original.revision, text(&view).revision);
    assert_ne!(edited.revision, text(&view).revision);
    assert!(!select(&mut view, &original, 0, 1));
    let undone = text(&view);
    view.input(
        &Input::TextKey {
            target: Target::Node(2),
            key: "z".into(),
            text: String::new(),
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
            select_all: false,
            history: Some(varta_view::HistoryAction::Redo),
        },
        &mut Deny,
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "aXc");
    assert_ne!(edited.revision, text(&view).revision);
    assert_ne!(undone.revision, text(&view).revision);
    assert!(!select(&mut view, &edited, 0, 1));
}

#[test]
fn readonly_selection_is_allowed_and_invalid_disabled_or_composing_selection_is_ignored() {
    let mut view = field("A😀\r\nB", false);
    let mut b = B::new();
    b.set_attribute(2, A::Readonly, "");
    commit(&mut view, &b);
    let model = text(&view);
    assert!(select(&mut view, &model, 1, 5));
    let before = view.snapshot();
    for invalid in [2, 6, usize::MAX] {
        assert!(!select(&mut view, &model, invalid, 0));
        assert_eq!(view.snapshot(), before);
    }
    let mut b = B::new();
    b.set_attribute(2, A::Disabled, "");
    commit(&mut view, &b);
    assert!(!text(&view).selectable);
    let before = view.snapshot();
    assert!(!select(&mut view, &model, 0, 1));
    assert_eq!(view.snapshot(), before);
    let mut b = B::new();
    b.remove_attribute(2, A::Disabled)
        .remove_attribute(2, A::Readonly);
    commit(&mut view, &b);
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "нове".into(),
            cursor: None,
        },
        &mut Deny,
    )
    .unwrap();
    let composing = text(&view);
    assert!(!composing.selectable && composing.selection.is_none());
    let before = view.snapshot();
    assert!(!select(&mut view, &model, 0, 1));
    assert_eq!(view.snapshot(), before);
}

#[test]
fn legacy_crlf_interior_selection_is_unavailable_without_rounding_or_mutation() {
    let mut view = field("a\r\nb", false);
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 2,
            focus: 2,
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
    let before = view.snapshot();
    let model = text(&view);
    assert!(model.selectable && model.selection.is_none());
    assert_eq!(view.snapshot(), before);
    assert!(select(&mut view, &model, 1, 3));
    assert_eq!(text(&view).selection.unwrap().focus, 3);
}

#[test]
fn password_runs_match_rendered_grapheme_masks_and_keep_source_offsets_in_the_host() {
    let mut view = field("A😀\r\ne\u{301}", true);
    let legacy = view.snapshot();
    assert_eq!(legacy["accessibility"]["children"][0]["value"], "••••••");
    let tree = view.accessibility_tree().unwrap();
    assert_eq!(tree.nodes[&2].value.as_deref(), Some("••••"));
    assert_eq!(view.snapshot(), legacy);
    let model = text(&view);
    assert_eq!(model.source_offsets, [0, 1, 5, 7, 10]);
    assert_eq!(model.character_lengths, [3; 4]);
    assert_eq!(model.lines.len(), 1);
    assert_eq!(model.lines[0], 0..4);
    for interior in [6, 8] {
        assert!(!select(&mut view, &model, 0, interior));
    }
    assert!(model.word_starts.is_empty());
    assert!(select(&mut view, &model, 1, 5));
    assert!(
        view.accessibility_input(
            &view.node_identity(2).unwrap(),
            &Action::ReplaceSelectedText("X".into()),
            &mut Deny
        )
        .unwrap()
    );
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["value"],
        "AX\r\ne\u{301}"
    );
    let previous = text(&view);
    let mut b = B::new();
    b.set_attribute(2, A::Type, "text");
    commit(&mut view, &b);
    assert_ne!(previous.revision, text(&view).revision);
    assert!(!select(&mut view, &previous, 0, 1));
    let mut b = B::new();
    b.set_attribute(2, A::Type, "password");
    commit(&mut view, &b);
    assert_ne!(previous.revision, text(&view).revision);
    assert!(!select(&mut view, &previous, 0, 1));
    let masked = text(&view);
    let mut b = B::new();
    b.remove_attribute(2, A::Type)
        .set_attribute(2, A::Type, "password");
    commit(&mut view, &b);
    assert_ne!(masked.revision, text(&view).revision);
}

#[test]
fn long_combining_text_and_word_boundaries_fit_native_byte_indices() {
    let value = "e".to_owned() + &"\u{301}".repeat(CHUNK + 7) + " word\n";
    let view = field(&value, false);
    let model = text(&view);
    assert!(model.runs.len() > 1);
    assert_eq!(
        model
            .character_lengths
            .iter()
            .map(|n| usize::from(*n))
            .sum::<usize>(),
        value.len()
    );
    assert!(model.runs.iter().all(|run| run.characters.len() <= CHUNK));
    assert!(
        model
            .word_starts
            .iter()
            .all(|start| *start < model.character_lengths.len())
    );
    assert_eq!(model.runs.last().unwrap().characters.len(), 0);
}

#[test]
fn exact_auxiliary_run_cap_and_one_more_line_are_observer_only_limits() {
    let mut view = field(&"\n".repeat(RUN_LIMIT - 1), false);
    assert_eq!(text(&view).runs.len(), RUN_LIMIT);
    let mut b = B::new();
    b.set_value(2, &"\n".repeat(RUN_LIMIT), 0);
    commit(&mut view, &b);
    let before = view.snapshot();
    assert_eq!(
        view.accessibility_tree().unwrap_err(),
        AccessibilityError::TextRunLimit { limit: RUN_LIMIT }
    );
    assert_eq!(view.snapshot(), before);
    let mut b = B::new();
    b.set_value(2, "ok", 0);
    commit(&mut view, &b);
    assert_eq!(text(&view).runs.len(), 1);
}
