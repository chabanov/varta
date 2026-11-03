use varta_common::view::{Attribute, CssProperty, Tag};
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
fn sheet(conditions: usize, rules: usize) -> String {
    format!(
        "@media {}{{{}}}",
        vec!["(min-width:0px)"; conditions].join(" and "),
        ".a{}".repeat(rules)
    )
}
fn rejected(view: &mut View, batch: &B, index: usize) {
    let before = view.snapshot();
    assert!(
        matches!(view.commit(&batch.finish(),false,&mut Denied),Err(ViewError::Invalid{operation:Some(at),violation:Violation::StylesheetWorkLimit}) if at==index)
    );
    assert_eq!(view.snapshot(), before);
    commit(view, &B::new());
}
#[test]
fn parsed_stylesheet_inventory_and_transaction_work_reject_atomically() {
    let mut view = View::default();
    commit(&mut view, &B::new().set_title("before").clone());
    rejected(
        &mut view,
        &B::new()
            .set_title("after")
            .stylesheet(1, &sheet(500, 1100))
            .clone(),
        1,
    );
    let text = sheet(500, 200);
    for id in 1..=5 {
        commit(&mut view, &B::new().stylesheet(id, &text).clone());
    }
    rejected(
        &mut view,
        &B::new().set_title("after").stylesheet(6, &text).clone(),
        1,
    );
    commit(
        &mut view,
        &B::new().stylesheet(1, "").stylesheet(6, &text).clone(),
    );
    let mut repeated = B::new();
    for _ in 0..6 {
        repeated.stylesheet(6, &text);
    }
    rejected(&mut view, &repeated, 5);
    commit(
        &mut view,
        &B::new().stylesheet(6, ".a { color:#abc }").clone(),
    );
}
#[test]
fn indexed_rules_preserve_source_specificity_media_and_transactional_replacement() {
    let mut view = View::default();
    let mut b = B::new();
    for id in 10..1010 {
        b.stylesheet(id, &format!(".other{id} {{ color:#000 }}"));
    }
    b.create_element(2, Tag::Div)
        .set_class(2, "a a b")
        .insert(1, 2, 0)
        .stylesheet(2, ".a.b, .a { color:#123 }")
        .stylesheet(1, ".a.b { color:#456 }");
    commit(&mut view, &b);
    let color = |v: &View| v.snapshot()["tree"]["children"][0]["style"]["color"].clone();
    assert_eq!(color(&view), "#123");
    commit(
        &mut view,
        &B::new().stylesheet(2, ".a { color:#789 }").clone(),
    );
    assert_eq!(color(&view), "#456");
    let mut bad = B::new();
    bad.stylesheet(1, "").remove(9999);
    assert!(view.commit(&bad.finish(), false, &mut Denied).is_err());
    assert_eq!(color(&view), "#456");
    commit(&mut view, &B::new().stylesheet(1, "").clone());
    assert_eq!(color(&view), "#789");
    commit(
        &mut view,
        &B::new()
            .stylesheet(2, "@media (max-width:20px) { .a { color:#fff } }")
            .clone(),
    );
    assert!(color(&view).is_null());
    view.set_environment(varta_view::Environment {
        width: 20.0,
        dark: false,
    })
    .unwrap();
    assert_eq!(color(&view), "#fff");
}
#[test]
fn display_navigation_ignores_unrelated_values_and_tracks_inherited_variable_changes() {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1,":root { --visible:none } .hide { display:var(--visible) } .button { color:var(--missing) }")
        .create_element(2,Tag::Button).insert(1,2,0)
        .create_element(3,Tag::Div).set_class(3,"hide").insert(1,3,0)
        .create_element(4,Tag::Button).set_class(4,"button").insert(3,4,0)
        .create_element(5,Tag::Button).insert(1,5,0);
    commit(&mut view, &b);
    let tab = |v: &mut View| {
        v.input(
            &Input::Key {
                target: Target::Node(2),
                key: "Tab".into(),
                shift: false,
                ctrl: false,
                alt: false,
                meta: false,
                composing: false,
            },
            &mut Denied,
        )
        .unwrap()
    };
    tab(&mut view);
    assert_eq!(view.snapshot()["focus"], 5);
    commit(&mut view,&B::new().stylesheet(1,":root { --visible:block } .hide { display:var(--visible) } .button { color:var(--missing) }").clone());
    tab(&mut view);
    assert_eq!(view.snapshot()["focus"], 4);
    // Inline declaration replacement remains authoritative and does not revive
    // an earlier property when its final var() reference is missing.
    commit(
        &mut view,
        &B::new()
            .set_style(3, CssProperty::Display, "none")
            .set_attribute(4, Attribute::Disabled, "")
            .clone(),
    );
    tab(&mut view);
    assert_eq!(view.snapshot()["focus"], 5);
}

#[test]
fn exact_parsed_work_allowance_is_accepted_and_one_extra_byte_rolls_back() {
    let mut text = sheet(500, 1039);
    let work = varta_common::view::stylesheet::parse_with_work_limit(&text, usize::MAX)
        .unwrap()
        .work;
    text.push_str(&" ".repeat(varta_common::view::DRAFT_MAX_STYLESHEET_WORK - work));
    let mut view = View::default();
    commit(&mut view, &B::new().stylesheet(1, &text).clone());
    rejected(
        &mut view,
        &B::new().stylesheet(1, &format!("{text} ")).clone(),
        0,
    );
    commit(&mut view, &B::new().stylesheet(1, "").clone());
}
