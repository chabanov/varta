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
        ..Default::default()
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
    view.layout_boxes(200.0, 100.0).unwrap();
    view.rasterize_boxes(200, 100).unwrap();
}

#[test]
fn empty_reference_fanout_remains_valid_and_renders_without_expansion_work() {
    const CHILD: &str = "VARTA_EMPTY_VARIABLE_FANOUT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // A regression must fail this test instead of hanging the entire suite.
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "empty_reference_fanout_remains_valid_and_renders_without_expansion_work",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() {
            if started.elapsed() > std::time::Duration::from_secs(45) {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "empty-reference rendering exceeded the subprocess deadline: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use std::fmt::Write;
    let mut css = ":root{".to_owned();
    for i in 0..2047 {
        write!(
            css,
            "--e{i:03x}:var(--e{:03x})var(--e{:03x});",
            i + 1,
            i + 1
        )
        .unwrap();
    }
    css.push_str("--e7ff:var(--missing,)}.target{padding:var(--e000,9px) 2px;color:var(--e000,#abc);width:20px;height:20px}");
    let mut view = View::default();
    let mut batch = B::new();
    batch
        .stylesheet(1, &css)
        .create_element(2, Tag::Div)
        .set_attribute(2, Attribute::Tabindex, "0")
        .set_class(2, "target")
        .insert(1, 2, 0)
        .command(2, varta_common::view::Command::Focus, 0.0);
    commit(&mut view, batch);
    let snapshot = view.snapshot();
    let style = &snapshot["tree"]["children"][0]["style"];
    assert_eq!(style["padding"], "2px");
    // Empty is a valid value: it does not select either property's fallback.
    assert!(style.get("color").is_none());
    view.layout_boxes(200.0, 100.0).unwrap();
    view.rasterize_boxes(200, 100).unwrap();
    let mut replacement = B::new();
    replacement.stylesheet(1, ".target{padding:var(--e000,9px) 2px}");
    commit(&mut view, replacement);
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["padding"],
        "9px 2px"
    );
}
