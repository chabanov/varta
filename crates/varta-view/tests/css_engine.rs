//! K8 T2 S1–S3: class tokens and escapes, sheet structure (`@layer`,
//! `@property`, comments, media features) and values (math functions,
//! units, Color 4/5) through the native cascade and renderer.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Environment, Renderer, View};

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
/// A parent `div` with `parent` classes holding a target `span` with
/// `target` classes; returns the target's computed style.
fn styled(
    source: &str,
    parent: &str,
    target: &str,
    environment: Option<Environment>,
) -> serde_json::Value {
    let mut view = View::default();
    if let Some(environment) = environment {
        view.set_environment(environment).unwrap();
    }
    let mut b = B::new();
    b.stylesheet(1, source)
        .create_element(2, Tag::Div)
        .set_class(2, parent)
        .insert(1, 2, 0)
        .create_element(3, Tag::Span)
        .set_class(3, target)
        .insert(2, 3, 0);
    commit(&mut view, &b);
    view.snapshot()["tree"]["children"][0]["children"][0]["style"].clone()
}
fn style(source: &str, target: &str) -> serde_json::Value {
    styled(source, "parent", target, None)
}

#[test]
fn escaped_class_tokens_match_html_class_names() {
    let got = style(
        ".sm\\:flex{display:flex} .w-1\\/2{width:50%} .\\[color\\:red\\]{color:#f00} .\\32 xl\\:p-0\\.5{padding:2px}",
        "sm:flex w-1/2 [color:red] 2xl:p-0.5",
    );
    assert_eq!(got["display"], "flex");
    assert_eq!(got["width"], "50%");
    assert_eq!(got["color"], "#f00");
    assert_eq!(got["padding"], "2px");
    // Controls and oversized tokens are not class names.
    let mut view = View::default();
    let mut b = B::new();
    b.create_element(2, Tag::Div).set_class(2, "a\u{7}");
    assert!(view.commit(&b.finish(), false, &mut Deny).is_err());
}

#[test]
fn comments_in_sheets_and_inline_values_never_reach_substitution() {
    let got = style(
        "/*! tailwindcss */ :root { --fg: /* var(--missing) */ #123 } .target { color: var(--fg) }",
        "target",
    );
    assert_eq!(got["color"], "#123");
    let mut view = View::default();
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Color, "/* var(--x) */ #456");
    commit(&mut view, &b);
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["color"],
        "#456"
    );
}

#[test]
fn layers_cascade_before_specificity_and_unlayered_rules_win() {
    let layered = "@layer base, utilities;
        @layer utilities { .target { color: #222 } }
        @layer base { .parent .x, .target.target.target { color: #111 } }";
    // `base` is declared first, so `utilities` wins despite lower specificity.
    let source = layered.replace(".parent .x, ", "");
    assert_eq!(style(&source, "target")["color"], "#222");
    // An unlayered rule beats every layer.
    let source = format!("{} .target {{ color: #333 }}", source);
    assert_eq!(style(&source, "target")["color"], "#333");
    // Within a layer, its own rules come after its sublayers.
    let nested = "@layer a { .target { color: #444 } @layer b { .target.target { color: #555 } } }";
    assert_eq!(style(nested, "target")["color"], "#444");
    // Layer order is shared across sheets, in ascending sheet order.
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(1, "@layer low, high; @layer high { .t { color: #600 } }")
        .stylesheet(2, "@layer low { .t.t { color: #700 } }")
        .create_element(2, Tag::Div)
        .set_class(2, "t")
        .insert(1, 2, 0);
    commit(&mut view, &b);
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["color"],
        "#600"
    );
}

#[test]
fn registered_properties_have_initial_values_and_may_not_inherit() {
    let registrations =
        "@property --tw-border-style { syntax: \"*\"; inherits: false; initial-value: solid }
        @property --gap { syntax: \"<length>\"; inherits: true; initial-value: 3px }
        @property --local { syntax: \"<length>\"; inherits: false; initial-value: 1px }";
    let got = style(
        &format!(
            "{registrations} .parent {{ --local: 9px; --gap: 5px }} .target {{ border-style: var(--tw-border-style); padding: var(--local); margin: var(--gap) }}"
        ),
        "target",
    );
    assert_eq!(got["border-style"], "solid");
    // `--local` does not inherit: the target sees its initial value.
    assert_eq!(got["padding"], "1px");
    assert_eq!(got["margin"], "5px");
    // A value outside the registered syntax is invalid at computed-value
    // time: the property takes its initial (non-inherited) value.
    let got = style(
        &format!("{registrations} .target {{ --local: red; padding: var(--local) }}"),
        "target",
    );
    assert_eq!(got["padding"], "1px");
    // `inherit` takes the parent's value even for a non-inheriting property.
    let got = style(
        &format!(
            "{registrations} .parent {{ --local: 9px }} .target {{ --local: inherit; padding: var(--local) }}"
        ),
        "target",
    );
    assert_eq!(got["padding"], "9px");
}

#[test]
fn theme_colours_in_oklch_through_var_render_natively() {
    let got = style(
        ":root { --color-indigo-600: oklch(51.1% 0.262 276.966); --color-black: #000 }
         .target { color: var(--color-indigo-600); background-color: color-mix(in oklab, var(--color-black) 50%, transparent) }",
        "target",
    );
    assert_eq!(got["color"], "rgba(79, 57, 246, 1)");
    assert_eq!(got["background-color"], "rgba(0, 0, 0, 0.5)");
    let mut b = B::new();
    b.stylesheet(
        1,
        ":root { --brand: oklch(51.1% 0.262 276.966) } .box { width: calc(var(--s) * 10); height: 10px; background-color: var(--brand) } :root { --s: 0.25rem }",
    )
    .create_element(2, Tag::Div)
    .set_class(2, "box")
    .insert(1, 2, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let image = renderer.render(&view, 60, 20, 1.0).unwrap();
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let offset = (y * image.width as usize + x) * 4;
        image.pixels[offset..offset + 4].try_into().unwrap()
    };
    assert_eq!(pixel(5, 5), [79, 57, 246, 255]);
    // `calc(0.25rem * 10)` is 40px wide.
    assert_eq!(pixel(39, 5), [79, 57, 246, 255]);
    assert_ne!(pixel(41, 5), [79, 57, 246, 255]);
}

#[test]
fn math_units_and_keywords_lower_to_the_native_grammar() {
    let got = style(
        ":root { --spacing: 0.25rem } .target {
            margin-top: calc(var(--spacing) * 4);
            padding: calc(var(--spacing) * -2) 1in;
            line-height: calc(1.25 / 0.875);
            font-size: 1.5em;
            opacity: 50%;
            font-weight: 350;
            border-radius: calc(infinity * 1px);
            transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1);
            white-space: pre;
            text-align: start;
            width: 50vw;
            height: 10dvh }",
        "target",
    );
    assert_eq!(got["margin-top"], "16px");
    assert_eq!(got["padding"], "0px 96px");
    assert_eq!(got["line-height"], "1.4285714285714286");
    assert_eq!(got["font-size"], "150%");
    assert_eq!(got["opacity"], "0.5");
    assert_eq!(got["font-weight"], "350");
    assert_eq!(got["border-radius"], "1000000px");
    assert_eq!(got["width"], "512px");
    assert_eq!(got["height"], "76.8px");
    // A value the native renderer cannot render through `var()` is invalid
    // at computed-value time natively.
    let got = style(
        ":root { --w: calc(100% - 1rem) } .target { width: var(--w) }",
        "target",
    );
    assert!(got.get("width").is_none());
}

#[test]
fn media_features_are_recorded_host_inputs() {
    let source = "@media (width >= 40rem) { .target { color: #001 } }
        @media (hover: none) { .target { background-color: #002 } }
        @media (orientation: portrait) { .target { opacity: 0.5 } }
        @media (prefers-reduced-motion: reduce) { .target { transition-duration: 0s } }
        @media not (height < 600px) { .target { padding: 1px } }";
    let wide = styled(source, "p", "target", None);
    assert_eq!(wide["color"], "#001");
    assert!(wide.get("background-color").is_none());
    assert!(wide.get("opacity").is_none());
    assert_eq!(wide["padding"], "1px");
    let mut environment = Environment {
        width: 400.0,
        height: 500.0,
        ..Environment::default()
    };
    environment.preferences.hover = varta_common::view::stylesheet::media::Hover::None;
    environment.preferences.reduced_motion = true;
    let narrow = styled(source, "p", "target", Some(environment));
    assert!(narrow.get("color").is_none());
    assert_eq!(narrow["background-color"], "#002");
    assert_eq!(narrow["opacity"], "0.5");
    assert_eq!(narrow["transition-duration"], "0s");
    assert!(narrow.get("padding").is_none());
}

#[test]
fn forbidden_constructs_cannot_hide_behind_escapes_or_comments() {
    for source in [
        ".a { color: #fff } @\\69mport \"x\";",
        ".a { background-color: \\75 rl(x) }",
        ".a { color: #fff !/**/important }",
        ".a { color: attr(x) }",
        "<!-- .a {} -->",
    ] {
        let mut view = View::default();
        let mut b = B::new();
        b.stylesheet(1, source);
        assert!(
            view.commit(&b.finish(), false, &mut Deny).is_err(),
            "{source}"
        );
    }
}
