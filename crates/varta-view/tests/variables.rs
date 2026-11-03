use varta_common::view::{Attribute, CssProperty, Tag};
use varta_view::{Access, BatchWriter as B, Broker, View};
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
fn commit(view: &mut View, b: B) {
    view.commit(&b.finish(), false, &mut Denied).unwrap();
}
fn style(source: &str) -> serde_json::Value {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1, source)
        .create_element(2, Tag::Div)
        .set_class(2, "parent")
        .insert(1, 2, 0)
        .create_element(3, Tag::Span)
        .set_class(3, "target")
        .insert(2, 3, 0);
    commit(&mut view, b);
    view.snapshot()["tree"]["children"][0]["children"][0]["style"].clone()
}
#[test]
fn computed_aliases_freeze_at_their_declaring_scope_and_invalid_overrides_do_not_resurrect_parent_values()
 {
    assert_eq!(
        style(":root{--base:#123;--alias:var(--base)}.target{--base:#456;color:var(--alias)}")["color"],
        "#123"
    );
    assert_eq!(
        style(
            ":root{--base:#123;--alias:var(--base)}.parent{--alias:var(--missing)}.target{color:var(--alias,#abc)}"
        )["color"],
        "#abc"
    );
}
#[test]
fn cycle_fallbacks_are_invalid_but_acyclic_dependents_can_use_their_fallback() {
    let source = ":root{--a:var(--b,#123);--b:var(--a,#456);--safe:#111;--self:var(--safe,var(--self));--c:var(--a,#abc)}.target{color:var(--c);background-color:var(--self,#789)}";
    let got = style(source);
    assert_eq!(got["color"], "#abc");
    assert_eq!(got["background-color"], "#111");
}
#[test]
fn nested_and_function_fallbacks_are_validated_after_selection() {
    let got = style(
        ":root{--red:10;--alpha:.5;--tone:rgb(var(--red),20,30)}.target{color:var(--missing,var(--tone));padding:var(--gap,1px 2px);opacity:var(--alpha,2)}",
    );
    assert_eq!(got["color"], "rgb(10,20,30)");
    assert_eq!(got["padding"], "1px 2px");
    assert_eq!(got["opacity"], ".5");
    let got = style(
        ":root{--bad:-1px;--n:10;--width:var(--n)px;--alpha:2}.target{width:9px;width:var(--width);padding:5px;padding:var(--bad,3px);opacity:.25;opacity:var(--alpha,.75)}",
    );
    for property in ["width", "padding", "opacity"] {
        assert!(got.get(property).is_none(), "{property}: {got}");
    }
}
#[test]
fn literal_strings_are_not_substituted_and_custom_names_remain_case_sensitive() {
    let got = style(
        ":root{--literal:\"var(--missing)\";--Tone:#123;--tone:#456;--alias:VAR(--Tone)}.target{font-family:var(--literal);color:VAR(--alias);background-color:var(--tone)}",
    );
    assert_eq!(got["font-family"], "\"var(--missing)\"");
    assert_eq!(got["color"], "#123");
    assert_eq!(got["background-color"], "#456");
}
#[test]
fn custom_wide_keywords_and_live_media_changes_resolve_before_inheritance() {
    for keyword in ["inherit", "unset", "revert", "revert-layer"] {
        assert_eq!(
            style(&format!(
                ":root{{--tone:#123}}.target{{--tone:{keyword};color:var(--tone,#abc)}}"
            ))["color"],
            "#123"
        );
    }
    assert_eq!(
        style(":root{--tone:#123}.target{--tone:initial;color:var(--tone,#abc)}")["color"],
        "#abc"
    );
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1,":root{--tone:#123;--alias:var(--tone)}@media (max-width:20px){:root{--tone:#456}}.target{color:var(--alias)}")
 .create_element(2,Tag::Div).set_class(2,"target").insert(1,2,0);
    commit(&mut view, b);
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["color"],
        "#123"
    );
    view.set_environment(varta_view::Environment {
        width: 20.0,
        dark: false,
    })
    .unwrap();
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["color"],
        "#456"
    );
}
#[test]
fn resolved_invalid_display_and_geometry_values_do_not_escape_native_readers() {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1,":root{--bad:-1px;--display:NoNe}.box{padding:9px;padding:var(--bad);width:100px;height:20px}.hidden{display:var(--display)}")
 .create_element(2,Tag::Div).set_class(2,"box").insert(1,2,0)
 .create_element(3,Tag::Button).set_class(3,"hidden").set_attribute(3,Attribute::AriaLabel,"hidden").insert(1,3,0)
 .set_style(2,CssProperty::Border,"var(--missing,2px solid #abc)");
    commit(&mut view, b);
    assert!(
        view.snapshot()["tree"]["children"][0]["style"]
            .get("padding")
            .is_none()
    );
    assert!(
        !view.snapshot()["accessibility"]
            .to_string()
            .contains("hidden")
    );
    assert!(view.layout_boxes(200.0, 100.0).is_ok());
    assert!(view.rasterize_boxes(200, 100).is_ok());
}
