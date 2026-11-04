use varta_common::view::{
    Attribute as A, DRAFT_MAX_PENDING_EVENTS, DRAFT_MAX_VALUE_BYTES, EventFlag, EventKind,
    ListenOption, Tag,
};
use varta_view::{
    Access, BatchWriter as B, Broker, ClipboardAction as Action, ClipboardData as Data,
    ClipboardError, ClipboardKey, ClipboardTask as Task, Filter, HistoryAction, Input, InputError,
    Target, View, ViewError,
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
        panic!("host clipboard defaults must not use guest copy_text authority")
    }
}
fn commit(v: &mut View, b: &B) {
    v.commit(&b.finish(), false, &mut Deny).unwrap();
}
fn fixture(tag: Tag, kind: &str, value: &str) -> View {
    let mut v = View::default();
    let mut b = B::new();
    b.create_element(2, tag)
        .set_value(2, value, 0)
        .listen(2, EventKind::Input, 10, Filter::default())
        .listen(2, EventKind::Keydown, 11, Filter::default())
        .insert(1, 2, 0);
    if tag == Tag::Input {
        b.set_attribute(2, A::Type, kind);
    }
    commit(&mut v, &b);
    select(&mut v, 0, value.len());
    v.drain();
    v
}
fn select(v: &mut View, a: usize, f: usize) {
    v.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: a,
            focus: f,
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
}
fn key(action: Action) -> ClipboardKey {
    ClipboardKey {
        action,
        key: match action {
            Action::Copy => "c",
            Action::Cut => "x",
            Action::Paste => "v",
        }
        .into(),
        shift: false,
        ctrl: false,
        alt: false,
        meta: true,
    }
}
fn start(v: &mut View, action: Action) -> Option<Task> {
    let target = v.clipboard_target().unwrap();
    v.begin_clipboard(Some(&target), &key(action)).unwrap()
}
fn value(v: &View) -> String {
    v.snapshot()["tree"]["children"][0]["value"]
        .as_str()
        .unwrap()
        .into()
}
fn paste(v: &mut View, text: Option<&str>) -> bool {
    let task = start(v, Action::Paste).unwrap();
    v.complete_clipboard(task.token(), Ok(Data::Text(text.map(str::to_owned))))
        .unwrap()
}
fn undo(v: &mut View) {
    v.input(
        &Input::TextKey {
            target: Target::Node(2),
            key: "z".into(),
            text: String::new(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: true,
            select_all: false,
            history: Some(HistoryAction::Undo),
        },
        &mut Deny,
    )
    .unwrap();
}
#[test]
fn copying_unicode_is_bound_and_preserves_field_and_frozen_snapshot() {
    let mut v = fixture(Tag::Input, "text", "A😀éB");
    select(&mut v, 1, 7);
    let before = v.snapshot();
    let binding = v.clipboard_target().unwrap();
    let task = start(&mut v, Action::Copy).unwrap();
    assert!(v.clipboard_pending());
    assert_eq!(before, v.snapshot());
    let Task::Write { token, text } = task else {
        panic!("copy writes")
    };
    assert_eq!(text, "😀é");
    assert!(v.complete_clipboard(&token, Ok(Data::Written)).unwrap());
    assert!(!v.clipboard_pending());
    assert_eq!(v.clipboard_target(), Some(binding));
    assert_eq!(before, v.snapshot());
    let events = v.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].record.kind, EventKind::Keydown);
    assert_eq!(events[0].record.key, "c");
    assert_ne!(events[0].record.flags & EventFlag::Meta as u32, 0);
    assert!(events[0].gesture);
}
#[test]
fn cut_waits_for_successful_write_is_one_edit_and_can_be_undone() {
    let mut v = fixture(Tag::Input, "text", "A😀éB");
    select(&mut v, 1, 7);
    let task = start(&mut v, Action::Cut).unwrap();
    assert_eq!(value(&v), "A😀éB");
    assert!(
        !v.complete_clipboard(task.token(), Err(ClipboardError::Failed))
            .unwrap()
    );
    assert_eq!(value(&v), "A😀éB");
    assert!(v.drain().iter().all(|e| e.record.kind != EventKind::Input));
    let task = start(&mut v, Action::Cut).unwrap();
    assert!(
        v.complete_clipboard(task.token(), Ok(Data::Written))
            .unwrap()
    );
    assert_eq!(value(&v), "AB");
    let events = v.drain();
    let edits: Vec<_> = events
        .iter()
        .filter(|e| e.record.kind == EventKind::Input)
        .collect();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].record.seq, 1);
    assert!(!edits[0].gesture);
    undo(&mut v);
    assert_eq!(value(&v), "A😀éB");
}
#[test]
fn paste_normalizes_control_line_endings_without_mutating_copy_sources() {
    for (tag, expected) in [
        (Tag::Input, "one two three"),
        (Tag::Textarea, "one\ntwo\nthree\n\n"),
    ] {
        let mut v = fixture(tag, "text", "old");
        assert!(paste(&mut v, Some("one\r\ntwo\rthree\n\n")));
        assert_eq!(value(&v), expected);
        undo(&mut v);
        assert_eq!(value(&v), "old");
    }
}
#[test]
fn empty_or_missing_clipboard_never_deletes_selection() {
    for text in [None, Some(""), Some("\r\n\r\n")] {
        let mut v = fixture(Tag::Input, "text", "kept");
        paste(&mut v, text);
        assert_eq!(value(&v), "kept");
        assert!(v.drain().iter().all(|e| e.record.kind != EventKind::Input));
    }
}
#[test]
fn passwords_never_offer_copy_or_cut_and_allow_paste() {
    let mut v = fixture(Tag::Input, "password", "private😀");
    assert!(start(&mut v, Action::Copy).is_none());
    assert!(start(&mut v, Action::Cut).is_none());
    assert!(!v.clipboard_pending());
    assert!(paste(&mut v, Some("replacement")));
    assert_eq!(value(&v), "replacement");
}
#[test]
fn readonly_copy_is_available_but_disabled_hidden_and_composing_fields_do_not_start_io() {
    let mut v = fixture(Tag::Input, "text", "kept");
    let mut b = B::new();
    b.set_attribute(2, A::Readonly, "");
    commit(&mut v, &b);
    for action in [Action::Cut, Action::Paste] {
        assert!(start(&mut v, action).is_none());
    }
    let task = start(&mut v, Action::Copy).unwrap();
    assert!(
        v.complete_clipboard(task.token(), Ok(Data::Written))
            .unwrap()
    );
    b = B::new();
    b.set_attribute(2, A::Disabled, "");
    commit(&mut v, &b);
    assert!(v.clipboard_target().is_none());
    let mut v = fixture(Tag::Input, "text", "kept");
    b = B::new();
    b.set_style(2, varta_common::view::CssProperty::Display, "none");
    commit(&mut v, &b);
    assert!(v.clipboard_target().is_none());
    let mut v = fixture(Tag::Input, "text", "kept");
    v.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "中".into(),
            cursor: None,
        },
        &mut Deny,
    )
    .unwrap();
    assert!(v.clipboard_target().is_none());
}
#[test]
fn static_prevent_default_stops_io_before_a_task_exists() {
    let mut v = fixture(Tag::Input, "text", "kept");
    let mut b = B::new();
    b.unlisten(2, EventKind::Keydown, 11).listen(
        2,
        EventKind::Keydown,
        12,
        Filter {
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut v, &b);
    for action in [Action::Copy, Action::Cut, Action::Paste] {
        assert!(start(&mut v, action).is_none());
        assert!(!v.clipboard_pending());
    }
    assert_eq!(value(&v), "kept");
}
#[test]
fn completion_rejects_changed_selection_content_focus_and_reused_ids() {
    for mode in 0..4 {
        let mut v = fixture(Tag::Input, "text", "kept");
        let task = start(&mut v, Action::Cut).unwrap();
        let mut b = B::new();
        match mode {
            0 => select(&mut v, 0, 0),
            1 => {
                b.set_value(2, "changed", 0);
                commit(&mut v, &b);
            }
            2 => {
                b.create_element(3, Tag::Input).insert(1, 3, 0).command(
                    3,
                    varta_common::view::Command::Focus,
                    0.0,
                );
                commit(&mut v, &b);
            }
            _ => {
                b.remove(2)
                    .create_element(2, Tag::Input)
                    .set_value(2, "new", 0)
                    .insert(1, 2, 0);
                commit(&mut v, &b);
            }
        }
        let before = v.snapshot();
        assert!(
            !v.complete_clipboard(task.token(), Ok(Data::Written))
                .unwrap()
        );
        assert_eq!(before, v.snapshot());
        assert!(!v.clipboard_pending());
    }
}
#[test]
fn equal_echoes_preserve_pending_sources_but_content_aba_cannot_revalidate_them() {
    for changed in [false, true] {
        let mut v = fixture(Tag::Input, "text", "old");
        let task = start(&mut v, Action::Paste).unwrap();
        if changed {
            let mut b = B::new();
            b.set_value(2, "temporary", 0);
            commit(&mut v, &b);
        }
        let mut b = B::new();
        b.set_value(2, "old", 0);
        commit(&mut v, &b);
        let accepted = v
            .complete_clipboard(task.token(), Ok(Data::Text(Some("new".into()))))
            .unwrap();
        assert_eq!(accepted, !changed);
        assert_eq!(value(&v), if changed { "old" } else { "new" });
    }
}
#[test]
fn one_pending_operation_and_nonce_guard_reject_foreign_duplicate_and_old_completions() {
    let mut a = fixture(Tag::Input, "text", "a");
    let mut b = fixture(Tag::Input, "text", "b");
    let first = start(&mut a, Action::Paste).unwrap();
    let foreign = start(&mut b, Action::Paste).unwrap();
    assert!(start(&mut a, Action::Copy).is_none());
    assert!(
        !a.complete_clipboard(foreign.token(), Ok(Data::Text(Some("wrong".into()))))
            .unwrap()
    );
    assert!(a.clipboard_pending());
    assert!(
        a.complete_clipboard(first.token(), Ok(Data::Text(Some("one".into()))))
            .unwrap()
    );
    let second = start(&mut a, Action::Paste).unwrap();
    assert!(
        !a.complete_clipboard(first.token(), Ok(Data::Text(Some("replayed".into()))))
            .unwrap()
    );
    assert!(a.clipboard_pending());
    assert!(
        a.complete_clipboard(second.token(), Ok(Data::Text(Some("two".into()))))
            .unwrap()
    );
    assert_eq!(value(&a), "onetwo");
}
#[test]
fn clipboard_and_value_limits_refuse_whole_oversized_payloads() {
    for text in [
        "A".repeat(DRAFT_MAX_VALUE_BYTES / 2 + 1),
        "界".repeat(DRAFT_MAX_VALUE_BYTES / 2 + 1),
    ] {
        let mut v = fixture(Tag::Input, "text", "kept");
        assert!(!paste(&mut v, Some(&text)));
        assert_eq!(value(&v), "kept");
    }
    let mut v = fixture(Tag::Input, "text", "");
    let exact = "😀".repeat(DRAFT_MAX_VALUE_BYTES / 4);
    assert!(paste(&mut v, Some(&exact)));
    assert_eq!(value(&v), exact);
}
#[test]
fn maxlength_is_utf16_and_preserves_scalar_boundaries_and_history() {
    let mut v = fixture(Tag::Input, "text", "old");
    let mut b = B::new();
    b.set_attribute(2, A::Maxlength, "2");
    commit(&mut v, &b);
    assert!(paste(&mut v, Some("😀B")));
    assert_eq!(value(&v), "😀");
    undo(&mut v);
    assert_eq!(value(&v), "old");
}
#[test]
fn queued_keydown_overflow_cannot_start_external_work() {
    let mut v = fixture(Tag::Input, "text", "kept");
    let target = v.clipboard_target().unwrap();
    for _ in 0..DRAFT_MAX_PENDING_EVENTS {
        v.input(
            &Input::TextKey {
                target: Target::Node(2),
                key: "q".into(),
                text: String::new(),
                shift: false,
                ctrl: true,
                alt: false,
                meta: false,
                select_all: false,
                history: None,
            },
            &mut Deny,
        )
        .unwrap();
    }
    assert!(matches!(
        v.begin_clipboard(Some(&target), &key(Action::Cut)),
        Err(InputError::View(ViewError::QueueOverflow))
    ));
    assert!(!v.clipboard_pending());
    assert_eq!(value(&v), "kept");
}
#[test]
fn logical_non_latin_key_is_preserved_independently_of_host_shortcut() {
    let mut v = fixture(Tag::Input, "text", "kept");
    let target = v.clipboard_target().unwrap();
    let mut key = key(Action::Copy);
    key.key = "с".into();
    let task = v.begin_clipboard(Some(&target), &key).unwrap().unwrap();
    assert_eq!(v.drain()[0].record.key, "с");
    v.complete_clipboard(task.token(), Ok(Data::Written))
        .unwrap();
}

#[test]
fn clipboard_shortcuts_on_non_fields_dispatch_keydown_without_io_or_typeahead() {
    for tag in [Tag::Button, Tag::Input] {
        let mut v = View::default();
        let mut b = B::new();
        b.create_element(2, tag).insert(1, 2, 0);
        if tag == Tag::Input {
            b.set_attribute(2, A::Type, "checkbox").set_checked(2, 1, 0);
        }
        b.listen(1, EventKind::Keydown, 12, Filter::default())
            .command(2, varta_common::view::Command::Focus, 0.0);
        commit(&mut v, &b);
        v.drain();
        assert!(v.clipboard_target().is_none());
        let before = v.snapshot();
        let mut k = key(Action::Paste);
        k.key = "Insert".into();
        k.meta = false;
        k.shift = true;
        assert!(v.begin_clipboard(None, &k).unwrap().is_none());
        assert!(!v.clipboard_pending());
        assert_eq!(before, v.snapshot());
        let events = v.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].record.key, "Insert");
        assert_ne!(events[0].record.flags & EventFlag::Shift as u32, 0);
        assert_eq!(events[0].record.flags & EventFlag::Meta as u32, 0);
        assert_eq!(
            events[0].record.flags & EventFlag::Checked as u32 != 0,
            tag == Tag::Input
        );
    }
}
