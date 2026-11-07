//! K8 T2 S5 through the native view: attribute and id selectors, `:has()`
//! and the input pseudo-classes follow the tree's attributes, values,
//! checked states and the user's interaction.
use varta_common::view::{Attribute, EventKind, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Filter, Input, Target, View};

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

fn commit(view: &mut View, batch: &B) {
    view.commit(&batch.finish(), false, &mut Deny).unwrap();
}

/// The computed style of node `id` (a child of the root at `path`).
fn style(view: &View, path: &[usize]) -> serde_json::Value {
    let mut node = view.snapshot()["tree"].clone();
    for index in path {
        node = node["children"][*index].clone();
    }
    node["style"].clone()
}

#[test]
fn attribute_selectors_follow_attribute_writes() {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(
        1,
        "[aria-expanded=true] { color: #111 } #main[role=tab] { margin-top: 1px }",
    )
    .create_element(2, Tag::Div)
    .insert(1, 2, 0)
    .set_attribute(2, Attribute::AriaExpanded, "false");
    commit(&mut view, &b);
    assert!(style(&view, &[0])["color"].is_null());
    let mut b = B::new();
    b.set_attribute(2, Attribute::AriaExpanded, "true")
        .set_attribute(2, Attribute::Id, "main")
        .set_attribute(2, Attribute::Role, "tab");
    commit(&mut view, &b);
    assert_eq!(style(&view, &[0])["color"], "#111");
    assert_eq!(style(&view, &[0])["margin-top"], "1px");
    let mut b = B::new();
    b.remove_attribute(2, Attribute::Role);
    commit(&mut view, &b);
    assert!(style(&view, &[0])["margin-top"].is_null());
}

#[test]
fn has_and_the_input_pseudo_classes_follow_values_and_checks() {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(
        1,
        ".g:has(:checked) { color: #111 } input:invalid { color: #c00 } input:user-invalid { margin-top: 1px } input:placeholder-shown { margin-left: 2px }",
    )
    .create_element(2, Tag::Div)
    .set_class(2, "g")
    .insert(1, 2, 0)
    .create_element(3, Tag::Input)
    .set_attribute(3, Attribute::Type, "checkbox")
    .insert(2, 3, 0)
    .create_element(4, Tag::Input)
    .set_attribute(4, Attribute::Type, "number")
    .set_attribute(4, Attribute::Placeholder, "n")
    .listen(4, EventKind::Change, 9, Filter::default())
    .insert(1, 4, 0);
    commit(&mut view, &b);
    assert!(style(&view, &[0])["color"].is_null());
    assert_eq!(style(&view, &[1])["margin-left"], "2px");
    let mut b = B::new();
    b.set_checked(3, 1, 0).set_value(4, "1.5", 0);
    commit(&mut view, &b);
    assert_eq!(style(&view, &[0])["color"], "#111");
    assert_eq!(style(&view, &[1])["color"], "#c00");
    assert!(style(&view, &[1])["margin-left"].is_null());
    // The user's committed change sets its user validity (HTML
    // §4.10.18.4.1): `:user-invalid` matches then.
    assert!(style(&view, &[1])["margin-top"].is_null());
    view.input(
        &Input::Key {
            target: Target::Node(4),
            key: "ArrowUp".into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut Deny,
    )
    .unwrap();
    let value = view.snapshot()["tree"]["children"][1]["value"].clone();
    assert_eq!(value, "2");
    assert!(style(&view, &[1])["color"].is_null());
    let mut b = B::new();
    b.set_value(
        4,
        "2.5",
        view.snapshot()["tree"]["children"][1]["seq"]
            .as_u64()
            .unwrap() as u32,
    );
    commit(&mut view, &b);
    assert_eq!(style(&view, &[1])["margin-top"], "1px");
}
