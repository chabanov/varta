use super::*;

fn media(width: f64, dark: bool) -> Media {
    Media {
        width,
        height: 768.0,
        dark,
        preferences: media::Preferences::default(),
    }
}

#[test]
fn media_expansion_is_charged_before_cloning_and_exact_work_is_admitted() {
    let source = ".a{}";
    let parsed = parse_with_work_limit(source, super::super::DRAFT_MAX_STYLESHEET_WORK).unwrap();
    assert_eq!(
        parsed.work,
        source.len() + 3 * super::super::DRAFT_STYLESHEET_NODE_WORK + 1
    );
    assert!(parse_with_work_limit(source, parsed.work).is_ok());
    assert_eq!(
        parse_with_work_limit(source, parsed.work - 1)
            .unwrap_err()
            .violation,
        Violation::WorkLimit
    );
    let media = format!(
        "@media {}{{{}}}",
        vec!["(min-width:0px)"; 500].join(" and "),
        ".a{}".repeat(1100)
    );
    assert!(media.len() < super::super::DRAFT_MAX_VALUE_BYTES / 2);
    assert_eq!(parse(&media).unwrap_err().violation, Violation::WorkLimit);
}

/// The class-only grammar's work charges carry over to complex selectors:
/// a node per compound and per pseudo-class (`:root` included), a node plus
/// the name per class.
#[test]
fn legacy_sheets_keep_their_work_charges() {
    let weight = super::super::DRAFT_STYLESHEET_NODE_WORK;
    let source = ":root{--a:1px} .a.b:hover,.c{color:#fff;padding:var(--a)} @media (min-width:1px) and (prefers-color-scheme:dark){.d{opacity:0.5}}";
    let parsed = parse_with_work_limit(source, usize::MAX).unwrap();
    let expected = source.len()
        // :root rule, selector (compound and pseudo-class), declaration
        + weight + 2 * weight + weight + "1px".len() + "a".len()
        // rule; `.a.b:hover`: selector + 2 classes + 1 pseudo; `.c`
        + weight + (weight + 2 * weight + 2 + weight) + (weight + weight + 1)
        + (weight + "#fff".len()) + (weight + "var(--a)".len())
        // media: 2 conditions; rule weighted by them; selector; declaration
        + 2 * weight + 3 * weight + (weight + weight + 1) + (weight + "0.5".len());
    assert_eq!(parsed.work, expected);
}

#[test]
fn the_stylesheet_grammar_is_checked() {
    let rules = parse(
        ":root { --fg: #111; --gap: 8px }
         .row.active:checked, .x { color: var(--fg); padding: 4px 8px; }
         @media (prefers-color-scheme: dark) and (min-width: 40rem) { .row { color: #eee } }",
    )
    .unwrap();
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[1].selectors[0].specificity(), (0, 3, 0));
    assert!(!rules[2].applies(&media(1024.0, false)));
    assert!(rules[2].applies(&media(800.0, true)));
    for invalid in [
        ".a { colour: #fff }",
        ".a { background-color: url(x) }",
        ".a { background-color: \\75 rl(x) }",
        ".a { background-color: u\\72l(\"x\") }",
        "@import \"x\";",
        "@\\69mport \"x\";",
        ".a::before { color: #fff }",
        "#a { color: #fff }",
        "frob { color: #fff }",
        ".a > { color: #fff }",
        ".a:nth-child(x) { color: #fff }",
        ".a:has(.b) { color: #fff }",
        "[data-x] { color: #fff }",
        ".a { color: #fff",
        ".a { ; color: #fff }",
        ".a { color: #fff;; }",
        "@media (orientation: sideways) { .a { color: #fff } }",
        "@media (prefers-color-scheme: dark) { .a { color: #fff }",
        "<!-- .a { color: #fff } -->",
        ".a { color: #fff } }",
        "@property --x { syntax: '<length>'; inherits: false }",
        "@media screen { @property --x { syntax: '*'; inherits: false } }",
        "@layer initial;",
        "@layer a, b { .a {} }",
        "@layer a,;",
        ".a { color: #fff ! importantx }",
        ".a { color: ! important }",
        "@supports display: grid { .a {} }",
        "@property --x { syntax: '*'; inherits: false !important }",
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
    assert_eq!(parse("").unwrap().len(), 0);
    assert_eq!(parse(".a { }").unwrap()[0].declarations.len(), 0);
}

#[test]
fn escaped_class_selectors_follow_the_html_class_model() {
    let rules = parse(
        ".sm\\:flex{display:flex} .w-1\\/2{width:50%} .p-0\\.5{padding:2px} .\\[color\\:\\#fff\\]{color:#fff} .\\!p-1{padding:4px} .\\32 xl\\:x:HOVER{color:#000} .é{color:#fff}",
    )
    .unwrap();
    let classes: Vec<_> = rules
        .iter()
        .map(|rule| rule.selectors[0].subject().classes[0].as_str())
        .collect();
    assert_eq!(
        classes,
        [
            "sm:flex",
            "w-1/2",
            "p-0.5",
            "[color:#fff]",
            "!p-1",
            "2xl:x",
            "é"
        ]
    );
    assert_eq!(rules[5].selectors[0].subject().pseudo, [Pseudo::Hover]);
}

#[test]
fn comments_are_stripped_and_values_serialized_without_them() {
    let rules =
        parse("/*! tailwindcss v4 */ .a { /* c */ color: /* x */ #fff /* y */; --v: a/**/b }")
            .unwrap();
    assert_eq!(
        rules[0].declarations,
        [
            (Name::Property(CssProperty::Color), "#fff".into()),
            (Name::Custom("v".into()), "a/**/b".into())
        ]
    );
}

#[test]
fn layers_are_declared_in_order_with_nested_names() {
    let parsed = parse_with_work_limit(
        "@layer theme, base, components, utilities;
         @layer utilities { .a { color: #000 } @media (hover: hover) { .a:hover { color: #fff } } }
         @layer base { .a { color: #111 } @layer reset { .b { color: #222 } } }
         @layer { .c { color: #333 } }
         .d { color: #444 }
         @layer theme.colors { .e { color: #555 } }",
        usize::MAX,
    )
    .unwrap();
    let named = |path: &[&str]| -> Vec<LayerName> {
        path.iter().map(|s| LayerName::Named((*s).into())).collect()
    };
    assert_eq!(
        parsed.layers,
        [
            named(&["theme"]),
            named(&["base"]),
            named(&["components"]),
            named(&["utilities"]),
            named(&["base", "reset"]),
            vec![LayerName::Anonymous(1)],
            named(&["theme", "colors"]),
        ]
    );
    let layers: Vec<_> = parsed.rules.iter().map(|rule| rule.layer).collect();
    assert_eq!(
        layers,
        [Some(3), Some(3), Some(1), Some(4), Some(5), None, Some(6)]
    );
    assert_eq!(parsed.rules[1].media.len(), 1);
}

#[test]
fn registered_properties_follow_properties_and_values() {
    let parsed = parse_with_work_limit(
        "@property --tw-space-x-reverse { syntax: \"*\"; inherits: false; initial-value: 0; }
         @property --tw-border-style { syntax: \"*\"; inherits: false; initial-value: solid }
         @property --tw-ring-color { syntax: \"*\"; inherits: false }
         @property --gap { syntax: \"<length> | auto\"; inherits: true; initial-value: 0px }
         @property --c { syntax: '<color>'; inherits: false; initial-value: #000 }
         @property --list { syntax: '<length>+'; inherits: false; initial-value: 1px 2px }",
        usize::MAX,
    )
    .unwrap();
    assert_eq!(parsed.properties.len(), 6);
    assert_eq!(parsed.properties[0].initial.as_deref(), Some("0"));
    assert_eq!(parsed.properties[2].initial, None);
    assert!(parsed.properties[3].admits("auto"));
    assert!(parsed.properties[3].admits("calc(1px + 2px)"));
    assert!(!parsed.properties[3].admits("red"));
    assert!(parsed.properties[4].admits("oklch(50% 0.1 10)"));
    assert!(parsed.properties[5].admits("1px 2px 3px"));
    for invalid in [
        "@property x { syntax: '*'; inherits: false }",
        "@property --x { syntax: '<length>'; inherits: false }",
        "@property --x { syntax: '<length>'; inherits: false; initial-value: 1em }",
        "@property --x { syntax: '<length>'; inherits: false; initial-value: var(--y) }",
        "@property --x { syntax: '<image>'; inherits: false; initial-value: none }",
        "@property --x { syntax: '*'; initial-value: 1 }",
        "@property --x { syntax: '*'; inherits: maybe }",
        "@property --x { syntax: '*'; inherits: false; unknown: 1 }",
        "@property --x { syntax: '<length> | inherit'; inherits: false; initial-value: 0px }",
        "@layer a { @property --x { syntax: '*'; inherits: false } }",
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn violations_carry_utf8_byte_offsets() {
    let error = |text: &str| parse(text).unwrap_err();
    assert_eq!(
        error(".a { color: #fff; colour: red }"),
        StylesheetError {
            violation: Violation::Declaration,
            offset: 18
        }
    );
    // A multi-byte character before the violation counts in bytes.
    let text = ":root { --label: x }\n/* é */ .a > #b {}";
    assert_eq!(error(text).offset, text.find("#b").unwrap());
    let text = "/* é */ frob { color: #fff }";
    assert_eq!(error(text).violation, Violation::Selector);
    assert_eq!(error(text).offset, text.find("frob").unwrap());
    assert_eq!(error(".a { color: #fff").violation, Violation::Unterminated);
    assert_eq!(
        error(".a { color: #fff; font-family: \"A\", Inherit }").violation,
        Violation::FontFamilyKeyword("inherit")
    );
    assert_eq!(
        error(".a { font-family: 1px }").violation,
        Violation::Declaration
    );
    assert_eq!(
        error(".a { border-radius: calc(100% - 1rem) }").violation,
        Violation::Unrenderable
    );
    assert_eq!(
        error("@media (orientation: sideways) { }").violation,
        Violation::Media
    );
    assert_eq!(error("@media (orientation: sideways) { }").offset, 21);
    assert_eq!(error("x{} .a{background:\\75 rl(x)}").offset, 18);
}

/// CSS Logical 1 §4 in the horizontal left-to-right writing mode, `inset`
/// and `flex`: each longhand declared at the mapped declaration's place.
#[test]
fn mapped_properties_declare_their_physical_longhands() {
    let declarations = |text: &str| parse(text).unwrap()[0].declarations.clone();
    let error = |text: &str| parse(text).unwrap_err();
    let property =
        |property: CssProperty, value: &str| (Name::Property(property), value.to_owned());
    assert_eq!(
        declarations(
            ".a { padding-left: 9px; padding-inline: 1px 2px; margin-block: auto; inset: 0 }"
        ),
        vec![
            property(CssProperty::PaddingLeft, "9px"),
            property(CssProperty::PaddingLeft, "1px"),
            property(CssProperty::PaddingRight, "2px"),
            property(CssProperty::MarginTop, "auto"),
            property(CssProperty::MarginBottom, "auto"),
            property(CssProperty::Top, "0"),
            property(CssProperty::Right, "0"),
            property(CssProperty::Bottom, "0"),
            property(CssProperty::Left, "0"),
        ]
    );
    assert_eq!(
        declarations(".a { flex: 1; margin-inline-end: -4px }"),
        vec![
            property(CssProperty::FlexGrow, "1"),
            property(CssProperty::FlexShrink, "1"),
            property(CssProperty::FlexBasis, "0%"),
            property(CssProperty::MarginRight, "-4px"),
        ]
    );
    let pending = declarations(".a { padding-block: var(--spacing) }");
    assert_eq!(pending.len(), 2);
    assert!(matches!(
        &pending[1],
        (Name::Mapped(CssProperty::PaddingBottom, part), value)
            if part.index == 1 && value == "var(--spacing)"
    ));
    assert_eq!(
        error(".a { padding-inline: -1px }").violation,
        Violation::Declaration
    );
    assert_eq!(
        error(".a { flex: 1 2 3 4 }").violation,
        Violation::Declaration
    );
    assert!(parse("@supports (padding-inline: 1px) { .a { color: red } }").is_ok());
}

#[test]
fn importance_supports_and_conditional_layers_follow_the_specifications() {
    let parsed = parse_with_work_limit(
        ".a { color: #fff ! IMPORTANT; padding: 1px; --x: 2 !important }
         @supports (display: grid) and (not (display: inline-grid)) { .b { color: #000 } }
         @supports (margin-trim: block) { .c { unknown-property: 1 } @layer never; }
         @media (width >= 40rem) { @layer wide { .d { color: #111 } } }
         @layer wide, narrow;",
        usize::MAX,
    )
    .unwrap();
    let rules = &parsed.rules;
    assert_eq!(rules.len(), 4);
    assert!(!rules[0].important && rules[1].important);
    assert_eq!(
        rules[1].declarations,
        [
            (Name::Property(CssProperty::Color), "#fff".into()),
            (Name::Custom("x".into()), "2".into())
        ]
    );
    assert_eq!(rules[2].selectors[0].subject().classes, ["b"]);
    assert_eq!(rules[3].media.len(), 1);
    // `never` sits in a false @supports block: it is not declared.
    assert_eq!(parsed.layers.len(), 2);
    assert_eq!(parsed.layer_declarations.len(), 3);
    assert_eq!(parsed.layer_declarations[0].media.len(), 1);
    assert!(parsed.layer_declarations[1].media.is_empty());
}
