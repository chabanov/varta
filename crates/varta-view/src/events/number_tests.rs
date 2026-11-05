//! `input type=number` edits its string value with the shared text editor
//! and Undo. L0 decides no numeric semantics: the value is never sanitized,
//! and Up/Down do not step it (no stepping is decided yet).
use super::{HistoryAction, Input, Target};
use crate::{Access, BatchWriter, Broker, Filter, View};
use varta_common::view::{Attribute, EventKind, Tag};

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

const FIELD: u32 = 2;
const HANDLER: u32 = 7;

fn number(value: &str, attribute: Option<Attribute>) -> View {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(FIELD, Tag::Input)
        .set_attribute(FIELD, Attribute::Type, "number")
        .set_value(FIELD, value, 0)
        .listen(FIELD, EventKind::Input, HANDLER, Filter::default())
        .insert(1, FIELD, 0);
    if let Some(attribute) = attribute {
        batch.set_attribute(FIELD, attribute, "");
    }
    view.commit(&batch.finish(), false, &mut Deny).unwrap();
    view
}

fn value(view: &View) -> String {
    view.snapshot()["tree"]["children"][0]["value"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn select_end(view: &mut View) {
    let end = value(view).len();
    view.input(
        &Input::TextSelect {
            target: Target::Node(FIELD),
            anchor: end,
            focus: end,
            activate: true,
        },
        &mut Deny,
    )
    .unwrap();
}

fn text_key(view: &mut View, key: &str, text: &str, history: Option<HistoryAction>) {
    view.input(
        &Input::TextKey {
            target: Target::Node(FIELD),
            key: key.into(),
            text: text.into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: history.is_some(),
            select_all: false,
            history,
        },
        &mut Deny,
    )
    .unwrap();
}

fn key(view: &mut View, key: &str) {
    view.input(
        &Input::Key {
            target: Target::Node(FIELD),
            key: key.into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut Deny,
    )
    .unwrap();
}

#[test]
fn number_fields_edit_and_undo_their_string_value() {
    let mut view = number("1", None);
    select_end(&mut view);
    text_key(&mut view, "2", "2", None);
    text_key(&mut view, "e", "e", None);
    // The delivered and stored values stay the typed strings, unsanitized.
    assert_eq!(value(&view), "12e");
    let delivered: Vec<_> = std::iter::from_fn(|| view.next_delivery())
        .map(|delivery| delivery.record.value)
        .collect();
    assert_eq!(delivered.last().map(String::as_str), Some("12e"));
    text_key(&mut view, "z", "", Some(HistoryAction::Undo));
    assert_ne!(value(&view), "12e");
    while value(&view) != "1" {
        let before = value(&view);
        text_key(&mut view, "z", "", Some(HistoryAction::Undo));
        assert_ne!(value(&view), before, "Undo makes progress");
    }
    text_key(&mut view, "z", "", Some(HistoryAction::Redo));
    assert_ne!(value(&view), "1");
    key(&mut view, "Backspace");
    assert_eq!(value(&view), "1");
}

#[test]
fn number_arrow_keys_do_not_step_and_inactive_fields_refuse_edits() {
    let mut view = number("41", None);
    select_end(&mut view);
    while view.next_delivery().is_some() {}
    for arrow in ["ArrowUp", "ArrowDown"] {
        key(&mut view, arrow);
        assert_eq!(value(&view), "41", "{arrow}");
    }
    assert!(
        std::iter::from_fn(|| view.next_delivery())
            .all(|delivery| delivery.record.kind != EventKind::Input)
    );
    for attribute in [Attribute::Disabled, Attribute::Readonly] {
        let mut view = number("5", Some(attribute));
        let _ = view.input(
            &Input::Type {
                target: Target::Node(FIELD),
                text: "6".into(),
            },
            &mut Deny,
        );
        key(&mut view, "Backspace");
        assert_eq!(value(&view), "5", "{attribute:?}");
    }
}
