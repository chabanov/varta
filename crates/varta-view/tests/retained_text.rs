use varta_common::view::{
    Attribute as A, CssProperty, DRAFT_FIELD_VALUE_RESERVATION_BYTES,
    DRAFT_MAX_RETAINED_TEXT_BYTES, DRAFT_MAX_VALUE_BYTES, Tag,
};
use varta_view::{Access, BatchWriter as B, Broker, Input, Target, View, ViewError, Violation};
struct Denied;
impl Broker for Denied {
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
fn commit(view: &mut View, batch: &B) {
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
}
fn near_limit() -> (View, usize) {
    let mut view = View::default();
    let mut b = B::new();
    let fields = DRAFT_MAX_RETAINED_TEXT_BYTES / DRAFT_FIELD_VALUE_RESERVATION_BYTES;
    for id in 2..2 + fields as u32 {
        b.create_element(id, Tag::Input).insert(1, id, 0);
    }
    let remaining = DRAFT_MAX_RETAINED_TEXT_BYTES - fields * DRAFT_FIELD_VALUE_RESERVATION_BYTES;
    b.create_text(1000, &"X".repeat(remaining))
        .insert(1, 1000, 0);
    commit(&mut view, &b);
    (view, remaining)
}
fn rejected(view: &mut View, batch: &B) {
    let before = view.snapshot();
    assert!(matches!(
        view.commit(&batch.finish(), false, &mut Denied),
        Err(ViewError::Invalid {
            operation: None,
            violation: Violation::RetainedTextLimit
        })
    ));
    assert_eq!(view.snapshot(), before);
}
#[test]
fn payload_counts_title_classes_attributes_styles_sheets_and_detached_nodes() {
    let (mut view, _) = near_limit();
    for b in [
        B::new().set_title("x").clone(),
        B::new().set_class(2, "x").clone(),
        B::new().set_attribute(2, A::Placeholder, "界").clone(),
        B::new().set_style(2, CssProperty::Color, "#abc").clone(),
        B::new().stylesheet(7, ".x { color:#abc; }").clone(),
        B::new().create_text(1001, "x").clone(),
    ] {
        rejected(&mut view, &b);
    }
    let mut resize = B::new();
    resize
        .set_text(1000, &"X".repeat(32760))
        .set_class(2, "x")
        .set_attribute(2, A::Placeholder, "界")
        .set_style(2, CssProperty::Color, "#abc");
    commit(&mut view, &resize);
    let mut replace = B::new();
    replace.remove_attribute(2, A::Placeholder).set_title("界");
    commit(&mut view, &replace);
    rejected(&mut view, &B::new().set_title("界x").clone());
    let mut replace = B::new();
    replace
        .remove_style(2, CssProperty::Color)
        .set_title("界1234");
    commit(&mut view, &replace);
}
#[test]
fn field_capacity_is_independent_of_edits_and_sticky_until_node_removal() {
    let (mut view, remaining) = near_limit();
    // One UTF-16 unit can need three UTF-8 bytes. The reserved capacity already
    // covers this local edit and cannot change subsequent batch admission.
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "界".repeat(DRAFT_MAX_VALUE_BYTES / 2),
        },
        &mut Denied,
    )
    .unwrap();
    rejected(&mut view, &B::new().set_title("x").clone());
    let mut change = B::new();
    change
        .set_text(1000, &"X".repeat(remaining - 8))
        .set_attribute(2, A::Type, "checkbox");
    commit(&mut view, &change);
    rejected(
        &mut view,
        &B::new().create_element(1001, Tag::Input).clone(),
    );
    let mut reclaim = B::new();
    reclaim
        .remove(2)
        .create_element(1001, Tag::Input)
        .insert(1, 1001, 0)
        .set_title("12345678");
    commit(&mut view, &reclaim);
}
#[test]
fn static_text_accumulates_across_batches_and_replacement_reclaims_space() {
    let mut view = View::default();
    let unit = "X".repeat(DRAFT_MAX_VALUE_BYTES / 2);
    let count = DRAFT_MAX_RETAINED_TEXT_BYTES / unit.len();
    for chunk in (0..count).collect::<Vec<_>>().chunks(3) {
        let mut batch = B::new();
        for index in chunk {
            batch
                .create_text(*index as u32 + 2, &unit)
                .insert(1, *index as u32 + 2, 0);
        }
        commit(&mut view, &batch);
    }
    rejected(&mut view, &B::new().create_text(1000, "x").clone());
    let mut smaller = B::new();
    smaller.set_text(2, "").create_text(1000, &unit);
    commit(&mut view, &smaller);
}
#[test]
fn set_value_reserves_capacity_even_when_the_same_batch_changes_type() {
    let mut view = View::default();
    let mut b = B::new();
    for id in 2..87 {
        b.create_element(id, Tag::Input);
    }
    b.set_value(2, "kept", 0)
        .set_attribute(2, A::Type, "checkbox");
    commit(&mut view, &b);
    rejected(
        &mut view,
        &B::new().create_element(1000, Tag::Input).clone(),
    );
    commit(
        &mut view,
        &B::new().remove(2).create_element(1000, Tag::Input).clone(),
    );
}
