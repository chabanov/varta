use varta_common::view::{Attribute as A, Command, DRAFT_MAX_NAMING_WORK as LIMIT, Tag};
use varta_view::{Access, BatchWriter as B, Broker, View, ViewError, Violation};
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
fn commit(view: &mut View, batch: B) {
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
}
fn reject(view: &mut View, batch: B) {
    let before = view.snapshot();
    assert!(matches!(
        view.commit(&batch.finish(), false, &mut Denied),
        Err(ViewError::Invalid {
            operation: None,
            violation: Violation::NamingWorkLimit
        })
    ));
    assert_eq!(view.snapshot(), before);
    commit(view, B::new());
}
fn setup(attached: bool) -> B {
    let text = format!("{}{}", "é".repeat(62), "a".repeat(32768 - 62));
    // The own/descendant/relation recurrence gives exactly LIMIT. The padding
    // is an author-label alternative, so a single UTF-8 byte changes it by 1.
    assert_eq!(79 + 60 * 255 + 255 * text.len() + 1579, LIMIT);
    let mut b = B::new();
    b.create_element(2, Tag::Button)
        .set_attribute(2, A::AriaHidden, "true")
        .set_attribute(2, A::AriaLabelledby, &vec!["label"; 255].join(" "))
        .create_element(3, Tag::Div)
        .set_attribute(3, A::Id, "label")
        .create_text(4, &text)
        .insert(3, 4, 0)
        .set_attribute(2, A::AriaLabel, &"x".repeat(1579));
    if attached {
        b.insert(1, 2, 0).insert(1, 3, 0);
    }
    b
}
#[test]
fn exact_naming_allowance_and_one_byte_overflow_are_atomic() {
    let mut view = View::default();
    commit(&mut view, setup(true));
    reject(
        &mut view,
        B::new()
            .set_title("rollback")
            .set_attribute(2, A::AriaLabel, &"x".repeat(1580))
            .clone(),
    );
    commit(
        &mut view,
        B::new()
            .set_attribute(2, A::AriaLabel, &"x".repeat(1580))
            .set_attribute(2, A::AriaLabel, &"x".repeat(1579))
            .clone(),
    );
    // A final-inventory failure still rolls back earlier command state.
    reject(
        &mut view,
        B::new()
            .command(2, Command::Focus, 0.0)
            .set_attribute(2, A::AriaLabel, &"x".repeat(1580))
            .clone(),
    );
    commit(
        &mut view,
        B::new()
            .remove_attribute(2, A::AriaLabelledby)
            .set_attribute(2, A::AriaLabel, &"x".repeat(1580))
            .clone(),
    );
}
#[test]
fn detached_names_hidden_state_and_author_precedence_cannot_hide_capacity() {
    let mut view = View::default();
    commit(&mut view, setup(false));
    reject(
        &mut view,
        B::new()
            .set_attribute(2, A::AriaLabel, &"x".repeat(1580))
            .clone(),
    );
    commit(&mut view, B::new().insert(1, 2, 0).insert(1, 3, 0).clone());
    commit(
        &mut view,
        B::new().set_attribute(3, A::AriaHidden, "true").clone(),
    );
    reject(
        &mut view,
        B::new()
            .set_attribute(2, A::AriaLabel, &"x".repeat(1580))
            .clone(),
    );
    // Removing a target id reclaims potential reference work; a failed restore
    // must not poison id lookup in the next accepted transaction.
    commit(
        &mut view,
        B::new()
            .remove_attribute(3, A::Id)
            .set_attribute(2, A::AriaLabel, &"x".repeat(1580))
            .clone(),
    );
    reject(&mut view, B::new().set_attribute(3, A::Id, "label").clone());
    commit(
        &mut view,
        B::new()
            .set_attribute(2, A::AriaLabel, &"x".repeat(1579))
            .set_attribute(3, A::Id, "label")
            .clone(),
    );
}
#[test]
fn embedded_live_values_are_reserved_before_local_edits_or_type_changes() {
    let mut view = View::default();
    let mut b = B::new();
    b.create_element(2, Tag::Button)
        .set_attribute(2, A::AriaHidden, "true")
        .set_attribute(2, A::AriaLabelledby, &vec!["field"; 100].join(" "))
        .insert(1, 2, 0)
        .create_element(3, Tag::Input)
        .set_attribute(3, A::Type, "checkbox")
        .set_attribute(3, A::Id, "field")
        .insert(1, 3, 0);
    commit(&mut view, b);
    reject(
        &mut view,
        B::new().set_attribute(3, A::Type, "password").clone(),
    );
    commit(
        &mut view,
        B::new()
            .set_attribute(2, A::AriaLabelledby, "field")
            .set_attribute(3, A::Type, "password")
            .clone(),
    );
    reject(
        &mut view,
        B::new()
            .set_attribute(2, A::AriaLabelledby, &vec!["field"; 100].join(" "))
            .clone(),
    );
    commit(&mut view, B::new().set_value(3, "😀", 0).clone());
}
#[test]
fn host_labels_charge_their_relations_and_relation_cycles_remain_finite() {
    let mut view = View::default();
    let mut b = setup(false);
    b.remove(2)
        .create_element(6, Tag::Input)
        .set_attribute(6, A::Id, "field")
        .create_element(7, Tag::Label)
        .set_attribute(7, A::For, "field")
        .set_attribute(7, A::AriaLabelledby, &vec!["label"; 300].join(" "));
    reject(&mut view, b);
    let mut b = B::new();
    b.create_element(2, Tag::Input)
        .set_attribute(2, A::Id, "field")
        .insert(1, 2, 0)
        .create_element(3, Tag::Label)
        .set_attribute(3, A::For, "field")
        .set_attribute(3, A::AriaLabelledby, "a")
        .insert(1, 3, 0)
        .create_element(4, Tag::Span)
        .set_attribute(4, A::Id, "a")
        .set_attribute(4, A::AriaLabelledby, "b")
        .insert(1, 4, 0)
        .create_element(5, Tag::Span)
        .set_attribute(5, A::Id, "b")
        .set_attribute(5, A::AriaLabelledby, "a")
        .insert(1, 5, 0)
        .create_text(6, "cyclic label")
        .insert(4, 6, 0);
    commit(&mut view, b);
    assert_eq!(
        view.snapshot()["accessibility"]["children"][0]["name"],
        "cyclic label"
    );
    commit(
        &mut view,
        B::new().set_attribute(3, A::For, "other").clone(),
    );
    assert!(
        view.snapshot()["accessibility"]["children"][0]
            .get("name")
            .is_none()
    );
    commit(
        &mut view,
        B::new().set_attribute(3, A::For, "field").clone(),
    );
    assert_eq!(
        view.snapshot()["accessibility"]["children"][0]["name"],
        "cyclic label"
    );
}

#[test]
fn snapshot_indices_follow_document_order_reparenting_renaming_and_removal() {
    let mut view = View::default();
    let mut b = B::new();
    b.create_element(2, Tag::Input)
        .set_attribute(2, A::Id, "field")
        .insert(1, 2, 0)
        .create_element(50, Tag::Label)
        .set_attribute(50, A::For, "field")
        .create_text(51, "A")
        .insert(50, 51, 0)
        .insert(1, 50, 0)
        .create_element(10, Tag::Label)
        .set_attribute(10, A::For, "field")
        .create_text(11, "B")
        .insert(10, 11, 0)
        .insert(1, 10, 0);
    commit(&mut view, b);
    let name = |view: &View| view.snapshot()["accessibility"]["children"][0]["name"].clone();
    assert_eq!(name(&view), "A B");
    commit(&mut view, B::new().insert(1, 10, 50).clone());
    assert_eq!(name(&view), "B A");
    commit(
        &mut view,
        B::new().set_attribute(50, A::For, "other").clone(),
    );
    assert_eq!(name(&view), "B");
    commit(&mut view, B::new().set_attribute(2, A::Id, "other").clone());
    assert_eq!(name(&view), "A");
    commit(&mut view, B::new().remove(50).clone());
    assert!(name(&view).is_null());
    commit(
        &mut view,
        B::new()
            .set_attribute(2, A::Id, "field")
            .create_element(3, Tag::Div)
            .set_attribute(3, A::AriaHidden, "true")
            .insert(1, 3, 0)
            .insert(3, 10, 0)
            .clone(),
    );
    assert_eq!(
        name(&view),
        "B",
        "a relation starting under a hidden ancestor includes the label"
    );
    commit(&mut view, B::new().remove(3).clone());
    assert!(name(&view).is_null());
}

#[test]
fn author_only_roles_do_not_charge_content_they_cannot_read_as_a_root_name() {
    use varta_common::view::DRAFT_MAX_TREE_DEPTH as DEPTH;
    let mut b = B::new();
    for id in 2..=DEPTH as u32 {
        b.create_element(id, Tag::Span)
            .set_attribute(id, A::Role, "list")
            .insert(id - 1, id, 0);
    }
    b.create_text(DEPTH as u32 + 1, &"a".repeat(32768))
        .insert(DEPTH as u32, DEPTH as u32 + 1, 0);
    let mut view = View::default();
    commit(&mut view, b);
    let snapshot = view.snapshot();
    let mut node = &snapshot["accessibility"]["children"][0];
    for _ in 1..DEPTH {
        assert_eq!(node["role"], "list");
        assert!(node.get("name").is_none());
        node = &node["children"][0];
    }
    assert_eq!(node["name"].as_str().unwrap().len(), 32768);
    let mut all_content = B::new();
    for id in 2..=DEPTH as u32 {
        all_content.set_attribute(id, A::Role, "cell");
    }
    reject(&mut view, all_content);
    commit(
        &mut view,
        B::new().set_attribute(2, A::Role, "cell").clone(),
    );
    assert_eq!(
        view.snapshot()["accessibility"]["children"][0]["name"]
            .as_str()
            .unwrap()
            .len(),
        32768
    );
}
