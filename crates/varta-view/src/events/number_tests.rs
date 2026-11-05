//! `input type=number` edits its string value with the shared text editor
//! and Undo; the value is never sanitized. Up/Down step it as Chrome does
//! with the default step ([`super::number_step`]), as one Undo entry.
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
    key_with(view, key, false);
}

fn key_with(view: &mut View, key: &str, alt: bool) {
    view.input(
        &Input::Key {
            target: Target::Node(FIELD),
            key: key.into(),
            shift: false,
            ctrl: false,
            alt,
            meta: false,
            composing: false,
        },
        &mut Deny,
    )
    .unwrap();
}

fn kinds(view: &mut View) -> Vec<(EventKind, String)> {
    std::iter::from_fn(|| view.next_delivery())
        .map(|delivery| (delivery.record.kind, delivery.record.value))
        .collect()
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
fn arrow_keys_step_the_value_as_one_undo_entry_each() {
    let mut view = number("1.5", None);
    select_end(&mut view);
    kinds(&mut view);
    key(&mut view, "ArrowUp");
    assert_eq!(value(&view), "2");
    assert_eq!(kinds(&mut view), [(EventKind::Input, "2".to_owned())]);
    key(&mut view, "ArrowUp");
    key(&mut view, "ArrowDown");
    key(&mut view, "ArrowDown");
    assert_eq!(value(&view), "1");
    // Each step is its own Undo entry, back to the typed string.
    for expected in ["2", "3", "2", "1.5"] {
        text_key(&mut view, "z", "", Some(HistoryAction::Undo));
        assert_eq!(value(&view), expected);
    }
    text_key(&mut view, "z", "", Some(HistoryAction::Redo));
    assert_eq!(value(&view), "2");
    // An invalid typed value steps from 0, and typing continues after it.
    let mut view = number("12e", None);
    select_end(&mut view);
    key(&mut view, "ArrowDown");
    assert_eq!(value(&view), "-1");
    text_key(&mut view, "5", "5", None);
    assert_eq!(value(&view), "-15");
}

#[test]
fn a_step_dispatches_input_then_change() {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(FIELD, Tag::Input)
        .set_attribute(FIELD, Attribute::Type, "number")
        .set_value(FIELD, "41", 0)
        .listen(FIELD, EventKind::Input, HANDLER, Filter::default())
        .listen(FIELD, EventKind::Change, HANDLER + 1, Filter::default())
        .insert(1, FIELD, 0);
    view.commit(&batch.finish(), false, &mut Deny).unwrap();
    select_end(&mut view);
    kinds(&mut view);
    key(&mut view, "ArrowUp");
    assert_eq!(
        kinds(&mut view),
        [
            (EventKind::Input, "42".to_owned()),
            (EventKind::Change, "42".to_owned())
        ]
    );
}

#[test]
fn alt_down_inactive_fields_and_inexact_values_do_not_step() {
    let mut view = number("41", None);
    select_end(&mut view);
    kinds(&mut view);
    key_with(&mut view, "ArrowDown", true);
    assert_eq!(value(&view), "41");
    key_with(&mut view, "ArrowUp", true);
    assert_eq!(value(&view), "42");
    // A value Chrome would step with Decimal precision loss is refused.
    let mut view = number("999999999999999999", None);
    select_end(&mut view);
    kinds(&mut view);
    key(&mut view, "ArrowUp");
    assert_eq!(value(&view), "999999999999999999");
    assert!(kinds(&mut view).is_empty());
    // Up/Down do not navigate a number field's text.
    assert!(matches!(
        view.input(
            &Input::TextNavigate {
                target: Target::Node(FIELD),
                key: "ArrowUp".into(),
                position: 0,
                preferred_x: None,
                upstream: false,
                shift: false,
            },
            &mut Deny,
        ),
        Err(crate::InputError::Key(_))
    ));
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
        key(&mut view, "ArrowUp");
        assert_eq!(value(&view), "5", "{attribute:?}");
    }
}
