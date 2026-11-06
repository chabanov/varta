//! Built-in utilities, part 3: a port of Tailwind CSS v4.3.5
//! `utilities.ts` lines 3802–5005 (the body of `createUtilities` from
//! `background-clip` up to `forced-color-adjust`) and the module-level
//! helper `alphaReplacedDropShadowProperties`. Tailwind CSS is MIT licensed,
//! copyright Tailwind Labs, Inc. (see LICENSE-TAILWIND).
//!
//! Registration order follows the source exactly: it decides which utility
//! wins and the output order.
use super::{
    Compiled, DefaultValue, FunctionalDesc, Registrar, SpacingOptions, as_color, property,
    replace_alpha, resolve_theme_color, with_alpha,
};
use crate::ast::Node;
use crate::candidate::{CandidateModifier, UtilityValue};
use crate::theme::options;
use crate::utils::infer_data_type::{infer_data_type, is_positive_integer, is_valid_opacity_value};
use crate::utils::js_number::{is_integer, to_number};
use crate::utils::replace_shadow_colors::replace_shadow_colors;
use crate::utils::segment;
use std::cell::Cell;

/// Single-declaration static utilities: `(class, property, value)`.
type StaticTable = [(&'static str, &'static str, &'static str)];

fn register_statics(r: &mut Registrar<'_>, table: &StaticTable) {
    for (name, prop, value) in table {
        r.static_utility(name, &[(prop, value)]);
    }
}

const BACKGROUND_CLIP_ORIGIN: &StaticTable = &[
    ("bg-clip-text", "background-clip", "text"),
    ("bg-clip-border", "background-clip", "border-box"),
    ("bg-clip-padding", "background-clip", "padding-box"),
    ("bg-clip-content", "background-clip", "content-box"),
    ("bg-origin-border", "background-origin", "border-box"),
    ("bg-origin-padding", "background-origin", "padding-box"),
    ("bg-origin-content", "background-origin", "content-box"),
];

/// Values shared by `bg-blend-*` and `mix-blend-*`.
const BLEND_MODES: &[&str] = &[
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
];

const MIX_BLEND_ONLY: &StaticTable = &[
    ("mix-blend-plus-darker", "mix-blend-mode", "plus-darker"),
    ("mix-blend-plus-lighter", "mix-blend-mode", "plus-lighter"),
];

const OBJECT_FIT: &StaticTable = &[
    ("object-contain", "object-fit", "contain"),
    ("object-cover", "object-fit", "cover"),
    ("object-fill", "object-fit", "fill"),
    ("object-none", "object-fit", "none"),
    ("object-scale-down", "object-fit", "scale-down"),
];

/// `object-*` static values: `(value, object-position)`.
const OBJECT_POSITIONS: &[(&str, &str)] = &[
    ("top", "top"),
    ("top-left", "left top"),
    ("top-right", "right top"),
    ("bottom", "bottom"),
    ("bottom-left", "left bottom"),
    ("bottom-right", "right bottom"),
    ("left", "left"),
    ("right", "right"),
    ("center", "center"),
];

/// Padding utilities: `(root, property)`.
const PADDING: &[(&str, &str)] = &[
    ("p", "padding"),
    ("px", "padding-inline"),
    ("py", "padding-block"),
    ("ps", "padding-inline-start"),
    ("pe", "padding-inline-end"),
    ("pbs", "padding-block-start"),
    ("pbe", "padding-block-end"),
    ("pt", "padding-top"),
    ("pr", "padding-right"),
    ("pb", "padding-bottom"),
    ("pl", "padding-left"),
];

const TEXT_ALIGN: &StaticTable = &[
    ("text-left", "text-align", "left"),
    ("text-center", "text-align", "center"),
    ("text-right", "text-align", "right"),
    ("text-justify", "text-align", "justify"),
    ("text-start", "text-align", "start"),
    ("text-end", "text-align", "end"),
];

const VERTICAL_ALIGN: &StaticTable = &[
    ("align-baseline", "vertical-align", "baseline"),
    ("align-top", "vertical-align", "top"),
    ("align-middle", "vertical-align", "middle"),
    ("align-bottom", "vertical-align", "bottom"),
    ("align-text-top", "vertical-align", "text-top"),
    ("align-text-bottom", "vertical-align", "text-bottom"),
    ("align-sub", "vertical-align", "sub"),
    ("align-super", "vertical-align", "super"),
];

const TEXT_TRANSFORM_STYLE_DECORATION_LINE: &StaticTable = &[
    ("uppercase", "text-transform", "uppercase"),
    ("lowercase", "text-transform", "lowercase"),
    ("capitalize", "text-transform", "capitalize"),
    ("normal-case", "text-transform", "none"),
    ("italic", "font-style", "italic"),
    ("not-italic", "font-style", "normal"),
    ("underline", "text-decoration-line", "underline"),
    ("overline", "text-decoration-line", "overline"),
    ("line-through", "text-decoration-line", "line-through"),
    ("no-underline", "text-decoration-line", "none"),
];

const FONT_STRETCH: &StaticTable = &[
    ("font-stretch-normal", "font-stretch", "normal"),
    (
        "font-stretch-ultra-condensed",
        "font-stretch",
        "ultra-condensed",
    ),
    (
        "font-stretch-extra-condensed",
        "font-stretch",
        "extra-condensed",
    ),
    ("font-stretch-condensed", "font-stretch", "condensed"),
    (
        "font-stretch-semi-condensed",
        "font-stretch",
        "semi-condensed",
    ),
    (
        "font-stretch-semi-expanded",
        "font-stretch",
        "semi-expanded",
    ),
    ("font-stretch-expanded", "font-stretch", "expanded"),
    (
        "font-stretch-extra-expanded",
        "font-stretch",
        "extra-expanded",
    ),
    (
        "font-stretch-ultra-expanded",
        "font-stretch",
        "ultra-expanded",
    ),
];

/// The `font-stretch` percentage range (inclusive), per
/// <https://developer.mozilla.org/en-US/docs/Web/CSS/font-stretch#percentage>.
const FONT_STRETCH_MIN_PERCENT: f64 = 50.0;
const FONT_STRETCH_MAX_PERCENT: f64 = 200.0;

const DECORATION_STYLE_THICKNESS: &StaticTable = &[
    ("decoration-solid", "text-decoration-style", "solid"),
    ("decoration-double", "text-decoration-style", "double"),
    ("decoration-dotted", "text-decoration-style", "dotted"),
    ("decoration-dashed", "text-decoration-style", "dashed"),
    ("decoration-wavy", "text-decoration-style", "wavy"),
    ("decoration-auto", "text-decoration-thickness", "auto"),
    (
        "decoration-from-font",
        "text-decoration-thickness",
        "from-font",
    ),
];

/// `cssFilterValue`.
const CSS_FILTER_VALUE: &str = "var(--tw-blur, ) var(--tw-brightness, ) var(--tw-contrast, ) var(--tw-grayscale, ) var(--tw-hue-rotate, ) var(--tw-invert, ) var(--tw-saturate, ) var(--tw-sepia, ) var(--tw-drop-shadow, )";

/// `cssBackdropFilterValue`.
const CSS_BACKDROP_FILTER_VALUE: &str = "var(--tw-backdrop-blur, ) var(--tw-backdrop-brightness, ) var(--tw-backdrop-contrast, ) var(--tw-backdrop-grayscale, ) var(--tw-backdrop-hue-rotate, ) var(--tw-backdrop-invert, ) var(--tw-backdrop-opacity, ) var(--tw-backdrop-saturate, ) var(--tw-backdrop-sepia, )";

/// An `@property` registration: `(name, initial value, syntax)`.
type PropertyTable = [(&'static str, Option<&'static str>, Option<&'static str>)];

const FILTER_PROPERTIES: &PropertyTable = &[
    ("--tw-blur", None, None),
    ("--tw-brightness", None, None),
    ("--tw-contrast", None, None),
    ("--tw-grayscale", None, None),
    ("--tw-hue-rotate", None, None),
    ("--tw-invert", None, None),
    ("--tw-opacity", None, None),
    ("--tw-saturate", None, None),
    ("--tw-sepia", None, None),
    ("--tw-drop-shadow", None, None),
    ("--tw-drop-shadow-color", None, None),
    ("--tw-drop-shadow-alpha", Some("100%"), Some("<percentage>")),
    ("--tw-drop-shadow-size", None, None),
];

const BACKDROP_FILTER_PROPERTIES: &PropertyTable = &[
    ("--tw-backdrop-blur", None, None),
    ("--tw-backdrop-brightness", None, None),
    ("--tw-backdrop-contrast", None, None),
    ("--tw-backdrop-grayscale", None, None),
    ("--tw-backdrop-hue-rotate", None, None),
    ("--tw-backdrop-invert", None, None),
    ("--tw-backdrop-opacity", None, None),
    ("--tw-backdrop-saturate", None, None),
    ("--tw-backdrop-sepia", None, None),
];

const CONTAIN_PROPERTIES: &PropertyTable = &[
    ("--tw-contain-size", None, None),
    ("--tw-contain-layout", None, None),
    ("--tw-contain-paint", None, None),
    ("--tw-contain-style", None, None),
];

/// `atRoot([property(…), …])` of a property table.
fn at_root_properties(table: &PropertyTable) -> Node {
    Node::AtRoot(
        table
            .iter()
            .map(|(name, initial, syntax)| property(name, *initial, *syntax))
            .collect(),
    )
}

/// `filterProperties()`.
fn filter_properties() -> Node {
    at_root_properties(FILTER_PROPERTIES)
}

/// `backdropFilterProperties()`.
fn backdrop_filter_properties() -> Node {
    at_root_properties(BACKDROP_FILTER_PROPERTIES)
}

/// The nodes of one filter function: its `--tw-*` variable and `filter`.
fn filter_nodes(variable: &str, value: impl Into<String>) -> Vec<Node> {
    vec![
        filter_properties(),
        Node::decl(variable, value),
        Node::decl("filter", CSS_FILTER_VALUE),
    ]
}

/// The nodes of one backdrop filter function.
fn backdrop_filter_nodes(variable: &str, value: impl Into<String>) -> Vec<Node> {
    vec![
        backdrop_filter_properties(),
        Node::decl(variable, value),
        Node::decl("-webkit-backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
        Node::decl("backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
    ]
}

/// A filter function utility with a backdrop twin (`brightness`,
/// `backdrop-brightness`, …): the CSS function and utility share a name.
struct FilterFunction {
    name: &'static str,
    /// The unit appended to bare integer values.
    bare_unit: &'static str,
    default_value: Option<&'static str>,
    supports_negative: bool,
}

const PERCENT_UNIT: &str = "%";
const DEGREE_UNIT: &str = "deg";
const MILLISECOND_UNIT: &str = "ms";
const FULL_PERCENT: &str = "100%";

/// Filter functions after `blur`, in source order.
const FILTER_FUNCTIONS: &[FilterFunction] = &[
    FilterFunction {
        name: "brightness",
        bare_unit: PERCENT_UNIT,
        default_value: None,
        supports_negative: false,
    },
    FilterFunction {
        name: "contrast",
        bare_unit: PERCENT_UNIT,
        default_value: None,
        supports_negative: false,
    },
    FilterFunction {
        name: "grayscale",
        bare_unit: PERCENT_UNIT,
        default_value: Some(FULL_PERCENT),
        supports_negative: false,
    },
    FilterFunction {
        name: "hue-rotate",
        bare_unit: DEGREE_UNIT,
        default_value: None,
        supports_negative: true,
    },
    FilterFunction {
        name: "invert",
        bare_unit: PERCENT_UNIT,
        default_value: Some(FULL_PERCENT),
        supports_negative: false,
    },
    FilterFunction {
        name: "saturate",
        bare_unit: PERCENT_UNIT,
        default_value: None,
        supports_negative: false,
    },
    FilterFunction {
        name: "sepia",
        bare_unit: PERCENT_UNIT,
        default_value: Some(FULL_PERCENT),
        supports_negative: false,
    },
];

/// `transition`'s default `transition-property` list.
const TRANSITION_DEFAULT_PROPERTIES: &str = "color, background-color, border-color, outline-color, text-decoration-color, fill, stroke, --tw-gradient-from, --tw-gradient-via, --tw-gradient-to, opacity, box-shadow, transform, translate, scale, rotate, filter, -webkit-backdrop-filter, backdrop-filter, display, content-visibility, overlay, pointer-events";

/// `transition-*` static values with timing and duration:
/// `(value, transition-property)`.
const TRANSITION_STATIC_VALUES: &[(&str, &str)] = &[
    ("all", "all"),
    (
        "colors",
        "color, background-color, border-color, outline-color, text-decoration-color, fill, stroke, --tw-gradient-from, --tw-gradient-via, --tw-gradient-to",
    ),
    ("opacity", "opacity"),
    ("shadow", "box-shadow"),
    ("transform", "transform, translate, scale, rotate"),
];

const DEFAULT_TIMING_FUNCTION: &str = "ease";
const DEFAULT_DURATION: &str = "0s";

const WILL_CHANGE: &StaticTable = &[
    ("will-change-auto", "will-change", "auto"),
    ("will-change-scroll", "will-change", "scroll-position"),
    ("will-change-contents", "will-change", "contents"),
    ("will-change-transform", "will-change", "transform"),
];

/// `cssContainValue`.
const CSS_CONTAIN_VALUE: &str = "var(--tw-contain-size, ) var(--tw-contain-layout, ) var(--tw-contain-paint, ) var(--tw-contain-style, )";

const CONTAIN_KEYWORDS: &StaticTable = &[
    ("contain-none", "contain", "none"),
    ("contain-content", "contain", "content"),
    ("contain-strict", "contain", "strict"),
];

/// Composable `contain-*` utilities: `(class, variable, value)`.
const CONTAIN_PARTS: &StaticTable = &[
    ("contain-size", "--tw-contain-size", "size"),
    ("contain-inline-size", "--tw-contain-size", "inline-size"),
    ("contain-layout", "--tw-contain-layout", "layout"),
    ("contain-paint", "--tw-contain-paint", "paint"),
    ("contain-style", "--tw-contain-style", "style"),
];

const SUPPORTS_RELATIVE_COLOR: &str = "@supports (color: lab(from red l a b))";

/// A bare integer value with a unit (`brightness-50` → `50%`).
fn bare_integer_with_unit(value: &str, unit: &str) -> Option<String> {
    is_positive_integer(value).then(|| format!("{value}{unit}"))
}

/// A functional utility with an empty theme-key list and one declaration
/// (`align-[…]`, `font-features-[…]`, `will-change-[…]`, `contain-[…]`).
fn arbitrary_only(r: &mut Registrar<'_>, root: &str, prop: &'static str) {
    r.functional_utility(
        root,
        FunctionalDesc::new(move |value, _, _| Some(vec![Node::decl(prop, value)])),
    );
}

/// Register every utility of this part in Tailwind's source order.
pub(super) fn register(r: &mut Registrar<'_>) {
    register_statics(r, BACKGROUND_CLIP_ORIGIN);
    for mode in BLEND_MODES {
        r.static_utility(
            &format!("bg-blend-{mode}"),
            &[("background-blend-mode", mode)],
        );
        r.static_utility(&format!("mix-blend-{mode}"), &[("mix-blend-mode", mode)]);
    }
    register_statics(r, MIX_BLEND_ONLY);

    register_fill_stroke(r);

    register_statics(r, OBJECT_FIT);
    let mut object =
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("object-position", value)]))
            .theme_keys(&["--object-position"]);
    for (name, position) in OBJECT_POSITIONS {
        object = object.static_value(name, vec![Node::decl("object-position", *position)]);
    }
    r.functional_utility("object", object);

    for (name, prop) in PADDING {
        r.spacing_utility(
            name,
            &["--padding", "--spacing"],
            move |value| Some(vec![Node::decl(*prop, value)]),
            SpacingOptions::default(),
        );
    }

    register_statics(r, TEXT_ALIGN);
    r.spacing_utility(
        "indent",
        &["--text-indent", "--spacing"],
        |value| Some(vec![Node::decl("text-indent", value)]),
        SpacingOptions {
            supports_negative: true,
            ..SpacingOptions::default()
        },
    );

    register_statics(r, VERTICAL_ALIGN);
    arbitrary_only(r, "align", "vertical-align");

    register_font(r);
    arbitrary_only(r, "font-features", "font-feature-settings");

    register_statics(r, TEXT_TRANSFORM_STYLE_DECORATION_LINE);

    register_statics(r, FONT_STRETCH);
    r.functional_utility(
        "font-stretch",
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("font-stretch", value)])).bare(
            |value, _| {
                let number = to_number(value.value.strip_suffix('%')?);
                if !is_integer(number) || number < 0.0 {
                    return None;
                }
                if !(FONT_STRETCH_MIN_PERCENT..=FONT_STRETCH_MAX_PERCENT).contains(&number) {
                    return None;
                }
                Some(value.value.to_owned())
            },
        ),
    );

    r.color_utility(
        "placeholder",
        &["--placeholder-color", "--color"],
        |value| {
            Some(vec![Node::rule(
                "&::placeholder",
                vec![
                    Node::decl("--tw-sort", "placeholder-color"),
                    Node::decl("color", value),
                ],
            )])
        },
    );

    register_statics(r, DECORATION_STYLE_THICKNESS);
    register_decoration(r);

    r.functional_utility(
        "animate",
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("animation", value)]))
            .theme_keys(&["--animate"])
            .static_value("none", vec![Node::decl("animation", "none")]),
    );

    register_filters(r);
    register_transitions(r);

    register_statics(r, WILL_CHANGE);
    arbitrary_only(r, "will-change", "will-change");

    r.static_utility(
        "content-none",
        &[("--tw-content", "none"), ("content", "none")],
    );
    r.functional_utility(
        "content",
        // BC: only the `--content` theme key is read, for compatibility;
        // arbitrary values are recommended instead.
        FunctionalDesc::new(|value, _, _| {
            Some(vec![
                Node::AtRoot(vec![property("--tw-content", Some("\"\""), None)]),
                Node::decl("--tw-content", value),
                Node::decl("content", "var(--tw-content)"),
            ])
        })
        .theme_keys(&["--content"]),
    );

    register_statics(r, CONTAIN_KEYWORDS);
    for (name, variable, value) in CONTAIN_PARTS {
        r.static_nodes(
            name,
            vec![
                at_root_properties(CONTAIN_PROPERTIES),
                Node::decl(*variable, *value),
                Node::decl("contain", CSS_CONTAIN_VALUE),
            ],
        );
    }
    arbitrary_only(r, "contain", "contain");
}

/// `fill-*` and `stroke-*`.
fn register_fill_stroke(r: &mut Registrar<'_>) {
    r.static_utility("fill-none", &[("fill", "none")]);
    r.utilities.functional("fill", |candidate, ds| {
        let Some(value) = candidate.value() else {
            return Compiled::Skip;
        };
        let color = match value {
            UtilityValue::Arbitrary { value, .. } => {
                as_color(value, candidate.modifier(), &ds.theme)
            }
            UtilityValue::Named { .. } => {
                resolve_theme_color(candidate, &ds.theme, &["--fill", "--color"])
                    .filter(|v| !v.is_empty())
            }
        };
        match color {
            Some(color) => Compiled::Nodes(vec![Node::decl("fill", color)]),
            None => Compiled::Skip,
        }
    });

    r.static_utility("stroke-none", &[("stroke", "none")]);
    r.utilities.functional("stroke", |candidate, ds| {
        let Some(value) = candidate.value() else {
            return Compiled::Skip;
        };
        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = data_type
                .as_deref()
                .or_else(|| infer_data_type(value, &["color", "number", "length", "percentage"]));
            return match ty {
                Some("number" | "length" | "percentage") => {
                    if candidate.modifier().is_some() {
                        return Compiled::Skip;
                    }
                    Compiled::Nodes(vec![Node::decl("stroke-width", value)])
                }
                _ => match as_color(value, candidate.modifier(), &ds.theme) {
                    Some(color) => Compiled::Nodes(vec![Node::decl("stroke", color)]),
                    None => Compiled::Skip,
                },
            };
        }
        if let Some(color) = resolve_theme_color(candidate, &ds.theme, &["--stroke", "--color"])
            .filter(|v| !v.is_empty())
        {
            return Compiled::Nodes(vec![Node::decl("stroke", color)]);
        }
        // A modifier is only valid as the opacity of a stroke color. The
        // value did not resolve to a color, so a modifier would be silently
        // ignored.
        if candidate.modifier().is_some() {
            return Compiled::Skip;
        }
        let named = value.value();
        if let Some(width) = ds
            .theme
            .resolve(Some(named), &["--stroke-width"], options::NONE)
            .filter(|v| !v.is_empty())
        {
            return Compiled::Nodes(vec![Node::decl("stroke-width", width)]);
        }
        if is_positive_integer(named) {
            return Compiled::Nodes(vec![Node::decl("stroke-width", named)]);
        }
        Compiled::Skip
    });
}

/// The nodes of a `font-weight` value.
fn font_weight_nodes(value: &str) -> Vec<Node> {
    vec![
        Node::AtRoot(vec![property("--tw-font-weight", None, None)]),
        Node::decl("--tw-font-weight", value),
        Node::decl("font-weight", value),
    ]
}

/// `font-*`: families, weights and arbitrary values.
fn register_font(r: &mut Registrar<'_>) {
    const FEATURE_SETTINGS: &str = "--font-feature-settings";
    const VARIATION_SETTINGS: &str = "--font-variation-settings";
    r.utilities.functional("font", |candidate, ds| {
        let Some(value) = candidate.value() else {
            return Compiled::Skip;
        };
        if candidate.modifier().is_some() {
            return Compiled::Skip;
        }
        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = data_type
                .as_deref()
                .or_else(|| infer_data_type(value, &["number", "generic-name", "family-name"]));
            return Compiled::Nodes(match ty {
                Some("generic-name" | "family-name") => vec![Node::decl("font-family", value)],
                _ => font_weight_nodes(value),
            });
        }
        let named = value.value();
        if let Some((families, nested)) =
            ds.theme
                .resolve_with(named, &["--font"], &[FEATURE_SETTINGS, VARIATION_SETTINGS])
        {
            return Compiled::Nodes(vec![
                Node::decl("font-family", families),
                Node::decl_opt(
                    "font-feature-settings",
                    nested.get(FEATURE_SETTINGS).cloned(),
                ),
                Node::decl_opt(
                    "font-variation-settings",
                    nested.get(VARIATION_SETTINGS).cloned(),
                ),
            ]);
        }
        match ds
            .theme
            .resolve(Some(named), &["--font-weight"], options::NONE)
            .filter(|v| !v.is_empty())
        {
            Some(weight) => Compiled::Nodes(font_weight_nodes(&weight)),
            None => Compiled::Skip,
        }
    });
}

/// `decoration-*`: thickness and color.
fn register_decoration(r: &mut Registrar<'_>) {
    r.utilities.functional("decoration", |candidate, ds| {
        let Some(value) = candidate.value() else {
            return Compiled::Skip;
        };
        let modifier = candidate.modifier();
        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = data_type
                .as_deref()
                .or_else(|| infer_data_type(value, &["color", "length", "percentage"]));
            return match ty {
                Some("length" | "percentage") => {
                    if modifier.is_some() {
                        return Compiled::Skip;
                    }
                    Compiled::Nodes(vec![Node::decl("text-decoration-thickness", value)])
                }
                _ => match as_color(value, modifier, &ds.theme) {
                    Some(color) => {
                        Compiled::Nodes(vec![Node::decl("text-decoration-color", color)])
                    }
                    None => Compiled::Skip,
                },
            };
        }
        // `text-decoration-thickness`
        let named = value.value();
        if let Some(thickness) = ds
            .theme
            .resolve(Some(named), &["--text-decoration-thickness"], options::NONE)
            .filter(|v| !v.is_empty())
        {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            return Compiled::Nodes(vec![Node::decl("text-decoration-thickness", thickness)]);
        }
        if is_positive_integer(named) {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            return Compiled::Nodes(vec![Node::decl(
                "text-decoration-thickness",
                format!("{named}px"),
            )]);
        }
        // `text-decoration-color`
        match resolve_theme_color(
            candidate,
            &ds.theme,
            &["--text-decoration-color", "--color"],
        )
        .filter(|v| !v.is_empty())
        {
            Some(color) => Compiled::Nodes(vec![Node::decl("text-decoration-color", color)]),
            None => Compiled::Skip,
        }
    });
}

/// `filter`, `backdrop-filter` and every filter function utility.
fn register_filters(r: &mut Registrar<'_>) {
    r.utilities.functional("filter", |candidate, _| {
        if candidate.modifier().is_some() {
            return Compiled::Skip;
        }
        match candidate.value() {
            None => Compiled::Nodes(vec![
                filter_properties(),
                Node::decl("filter", CSS_FILTER_VALUE),
            ]),
            Some(UtilityValue::Arbitrary { value, .. }) => {
                Compiled::Nodes(vec![Node::decl("filter", value)])
            }
            Some(UtilityValue::Named { value, .. }) if value == "none" => {
                Compiled::Nodes(vec![Node::decl("filter", "none")])
            }
            Some(UtilityValue::Named { .. }) => Compiled::Skip,
        }
    });

    r.utilities.functional("backdrop-filter", |candidate, _| {
        if candidate.modifier().is_some() {
            return Compiled::Skip;
        }
        let backdrop = |value: &str| {
            Compiled::Nodes(vec![
                Node::decl("-webkit-backdrop-filter", value),
                Node::decl("backdrop-filter", value),
            ])
        };
        match candidate.value() {
            None => Compiled::Nodes(vec![
                backdrop_filter_properties(),
                Node::decl("-webkit-backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
                Node::decl("backdrop-filter", CSS_BACKDROP_FILTER_VALUE),
            ]),
            Some(UtilityValue::Arbitrary { value, .. }) => backdrop(value),
            Some(UtilityValue::Named { value, .. }) if value == "none" => backdrop("none"),
            Some(UtilityValue::Named { .. }) => Compiled::Skip,
        }
    });

    r.functional_utility(
        "blur",
        FunctionalDesc::new(|value, _, _| {
            Some(filter_nodes("--tw-blur", format!("blur({value})")))
        })
        .theme_keys(&["--blur"])
        .static_value("none", filter_nodes("--tw-blur", " ")),
    );
    r.functional_utility(
        "backdrop-blur",
        FunctionalDesc::new(|value, _, _| {
            Some(backdrop_filter_nodes(
                "--tw-backdrop-blur",
                format!("blur({value})"),
            ))
        })
        .theme_keys(&["--backdrop-blur", "--blur"])
        .static_value("none", backdrop_filter_nodes("--tw-backdrop-blur", " ")),
    );

    for function in FILTER_FUNCTIONS {
        register_filter_function(r, function);
    }

    r.static_nodes("drop-shadow-none", filter_nodes("--tw-drop-shadow", " "));
    register_drop_shadow(r);

    r.functional_utility(
        "backdrop-opacity",
        FunctionalDesc::new(|value, _, _| {
            Some(backdrop_filter_nodes(
                "--tw-backdrop-opacity",
                format!("opacity({value})"),
            ))
        })
        .theme_keys(&["--backdrop-opacity", "--opacity"])
        .bare(|value, _| is_valid_opacity_value(value.value).then(|| format!("{}%", value.value))),
    );
}

/// One filter function and its backdrop twin.
fn register_filter_function(r: &mut Registrar<'_>, function: &'static FilterFunction) {
    let name = function.name;
    let unit = function.bare_unit;
    let default_value = function.default_value.map_or(DefaultValue::Undefined, |v| {
        DefaultValue::Value(v.to_owned())
    });

    let variable = format!("--tw-{name}");
    let mut desc = FunctionalDesc::new(move |value, _, _| {
        Some(filter_nodes(&variable, format!("{name}({value})")))
    })
    .bare(move |value, _| bare_integer_with_unit(value.value, unit))
    .default_value(default_value.clone());
    desc.theme_keys = vec![format!("--{name}")];
    desc.supports_negative = function.supports_negative;
    r.functional_utility(name, desc);

    let backdrop_variable = format!("--tw-backdrop-{name}");
    let mut backdrop = FunctionalDesc::new(move |value, _, _| {
        Some(backdrop_filter_nodes(
            &backdrop_variable,
            format!("{name}({value})"),
        ))
    })
    .bare(move |value, _| bare_integer_with_unit(value.value, unit))
    .default_value(default_value);
    backdrop.theme_keys = vec![format!("--backdrop-{name}"), format!("--{name}")];
    backdrop.supports_negative = function.supports_negative;
    r.functional_utility(&format!("backdrop-{name}"), backdrop);
}

/// `var(--tw-drop-shadow-color, <color>)`.
fn drop_shadow_color_var(color: &str) -> String {
    format!("var(--tw-drop-shadow-color, {color})")
}

/// `drop-shadow(a) drop-shadow(b)` of a comma-separated value.
fn drop_shadow_functions(value: &str) -> String {
    segment(value, ',')
        .iter()
        .map(|shadow| format!("drop-shadow({shadow})"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The nodes of a drop shadow size with an optional alpha; `drop_shadow`
/// is the final `--tw-drop-shadow` value.
fn drop_shadow_size_nodes(size: &str, alpha: Option<&str>, drop_shadow: String) -> Vec<Node> {
    let mut nodes = vec![
        filter_properties(),
        Node::decl_opt("--tw-drop-shadow-alpha", alpha.map(str::to_owned)),
    ];
    nodes.extend(alpha_replaced_drop_shadow_properties(
        "--tw-drop-shadow-size",
        size,
        alpha,
        &drop_shadow_color_var,
    ));
    nodes.push(Node::decl("--tw-drop-shadow", drop_shadow));
    nodes.push(Node::decl("filter", CSS_FILTER_VALUE));
    nodes
}

/// The nodes of a drop shadow color.
fn drop_shadow_color_nodes(color: String) -> Vec<Node> {
    vec![
        filter_properties(),
        Node::decl("--tw-drop-shadow-color", color),
        Node::decl("--tw-drop-shadow", "var(--tw-drop-shadow-size)"),
    ]
}

const DROP_SHADOW_SIZE_VAR: &str = "var(--tw-drop-shadow-size)";
const DROP_SHADOW_ALPHA_VAR: &str = "var(--tw-drop-shadow-alpha)";

/// `drop-shadow-*`: sizes, colors and their opacity.
fn register_drop_shadow(r: &mut Registrar<'_>) {
    r.utilities.functional("drop-shadow", |candidate, ds| {
        let theme = &ds.theme;
        let modifier = candidate.modifier();
        let alpha: Option<String> = match modifier {
            Some(CandidateModifier::Arbitrary(value)) => Some(value.clone()),
            Some(CandidateModifier::Named(value)) => {
                is_valid_opacity_value(value).then(|| format!("{value}%"))
            }
            None => None,
        }
        // `!alpha`: an empty alpha counts as absent.
        .filter(|a| !a.is_empty());
        let alpha = alpha.as_deref();
        let rejects_modifier = modifier.is_some() && alpha.is_none();

        let Some(value) = candidate.value() else {
            let (Some(size), Some(resolved)) = (
                theme.get(&["--drop-shadow"]),
                theme.resolve(None, &["--drop-shadow"], options::NONE),
            ) else {
                return Compiled::Skip;
            };
            if rejects_modifier {
                return Compiled::Skip;
            }
            return Compiled::Nodes(drop_shadow_size_nodes(
                &size,
                alpha,
                drop_shadow_functions(&resolved),
            ));
        };

        if let UtilityValue::Arbitrary { value, data_type } = value {
            let ty = data_type
                .as_deref()
                .or_else(|| infer_data_type(value, &["color"]));
            if ty == Some("color") {
                return match as_color(value, modifier, theme) {
                    Some(color) => Compiled::Nodes(drop_shadow_color_nodes(with_alpha(
                        &color,
                        DROP_SHADOW_ALPHA_VAR,
                    ))),
                    None => Compiled::Skip,
                };
            }
            if rejects_modifier {
                return Compiled::Skip;
            }
            return Compiled::Nodes(drop_shadow_size_nodes(
                value,
                alpha,
                DROP_SHADOW_SIZE_VAR.to_owned(),
            ));
        }

        // Shadow size
        let named = value.value();
        let size = theme
            .get(&[&format!("--drop-shadow-{named}")])
            .filter(|v| !v.is_empty());
        let resolved = theme
            .resolve(Some(named), &["--drop-shadow"], options::NONE)
            .filter(|v| !v.is_empty());
        if let (Some(size), Some(resolved)) = (size, resolved) {
            if rejects_modifier {
                return Compiled::Skip;
            }
            let drop_shadow = if alpha.is_some() {
                DROP_SHADOW_SIZE_VAR.to_owned()
            } else {
                drop_shadow_functions(&resolved)
            };
            return Compiled::Nodes(drop_shadow_size_nodes(&size, alpha, drop_shadow));
        }

        // Shadow color
        match resolve_theme_color(candidate, theme, &["--drop-shadow-color", "--color"])
            .filter(|v| !v.is_empty())
        {
            Some(color) if color == "inherit" => Compiled::Nodes(drop_shadow_color_nodes(color)),
            Some(color) => Compiled::Nodes(drop_shadow_color_nodes(with_alpha(
                &color,
                DROP_SHADOW_ALPHA_VAR,
            ))),
            None => Compiled::Skip,
        }
    });
}

/// Port of `alphaReplacedDropShadowProperties`: the `drop-shadow(…)` list
/// of `value` with each shadow color passed through `var_injector`, its
/// alpha replaced by `alpha`; a dynamic color or alpha adds a fallback and
/// a relative-color `@supports` override.
fn alpha_replaced_drop_shadow_properties(
    prop: &str,
    value: &str,
    alpha: Option<&str>,
    var_injector: &dyn Fn(&str) -> String,
) -> Vec<Node> {
    let requires_fallback = Cell::new(false);
    let replaced_value = segment(value, ',')
        .iter()
        .map(|shadow| {
            let replaced = replace_shadow_colors(shadow, &|color: &str| {
                let Some(alpha) = alpha else {
                    return var_injector(color);
                };
                // `currentcolor` keeps the `color-mix(…)` approach for wider
                // browser support; its fallback is handled in
                // post-processing.
                if color.starts_with("current") {
                    return var_injector(&with_alpha(color, alpha));
                }
                // Dynamic values in the relative color syntax cannot be
                // resolved statically, so a fallback is needed.
                if color.starts_with("var(") || alpha.starts_with("var(") {
                    requires_fallback.set(true);
                }
                var_injector(&replace_alpha(color, alpha))
            });
            format!("drop-shadow({replaced})")
        })
        .collect::<Vec<_>>()
        .join(" ");

    if requires_fallback.get() {
        let fallback = segment(value, ',')
            .iter()
            .map(|shadow| {
                format!(
                    "drop-shadow({})",
                    replace_shadow_colors(shadow, var_injector)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        vec![
            Node::decl(prop, fallback),
            Node::from_prelude(
                SUPPORTS_RELATIVE_COLOR,
                vec![Node::decl(prop, replaced_value)],
            ),
        ]
    } else {
        vec![Node::decl(prop, replaced_value)]
    }
}

/// `transition`, `transition-discrete`/`-normal`, `delay`, `duration` and
/// `ease`.
fn register_transitions(r: &mut Registrar<'_>) {
    let timing = r
        .theme
        .resolve(
            None,
            &["--default-transition-timing-function"],
            options::NONE,
        )
        .unwrap_or_else(|| DEFAULT_TIMING_FUNCTION.to_owned());
    let default_timing_function = format!("var(--tw-ease, {timing})");
    let duration = r
        .theme
        .resolve(None, &["--default-transition-duration"], options::NONE)
        .unwrap_or_else(|| DEFAULT_DURATION.to_owned());
    let default_duration = format!("var(--tw-duration, {duration})");

    let transition_nodes = {
        let timing = default_timing_function.clone();
        let duration = default_duration.clone();
        move |value: &str| {
            vec![
                Node::decl("transition-property", value),
                Node::decl("transition-timing-function", timing.as_str()),
                Node::decl("transition-duration", duration.as_str()),
            ]
        }
    };
    let mut transition = FunctionalDesc::new({
        let nodes = transition_nodes.clone();
        move |value, _, _| Some(nodes(value))
    })
    .default_value(DefaultValue::Value(
        TRANSITION_DEFAULT_PROPERTIES.to_owned(),
    ))
    .theme_keys(&["--transition-property"])
    .static_value("none", vec![Node::decl("transition-property", "none")]);
    for (name, properties) in TRANSITION_STATIC_VALUES {
        transition = transition.static_value(name, transition_nodes(properties));
    }
    r.functional_utility("transition", transition);

    r.static_utility(
        "transition-discrete",
        &[("transition-behavior", "allow-discrete")],
    );
    r.static_utility("transition-normal", &[("transition-behavior", "normal")]);

    r.functional_utility(
        "delay",
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("transition-delay", value)]))
            .bare(|value, _| bare_integer_with_unit(value.value, MILLISECOND_UNIT))
            .theme_keys(&["--transition-delay"]),
    );

    let duration_property = || Node::AtRoot(vec![property("--tw-duration", None, None)]);
    r.static_nodes(
        "duration-initial",
        vec![duration_property(), Node::decl("--tw-duration", "initial")],
    );
    r.utilities.functional("duration", move |candidate, ds| {
        // This utility supports neither modifiers nor `DEFAULT` values.
        if candidate.modifier().is_some() {
            return Compiled::Skip;
        }
        let value = match candidate.value() {
            None => return Compiled::Skip,
            Some(UtilityValue::Arbitrary { value, .. }) => Some(value.clone()),
            Some(UtilityValue::Named { value, fraction }) => ds
                .theme
                .resolve(
                    Some(fraction.as_deref().unwrap_or(value)),
                    &["--transition-duration"],
                    options::NONE,
                )
                .or_else(|| {
                    is_positive_integer(value).then(|| format!("{value}{MILLISECOND_UNIT}"))
                }),
        };
        // A value that does not resolve generates no rules.
        let Some(value) = value else {
            return Compiled::Skip;
        };
        Compiled::Nodes(vec![
            duration_property(),
            Node::decl("--tw-duration", value.as_str()),
            Node::decl("transition-duration", value),
        ])
    });

    let ease_property = || Node::AtRoot(vec![property("--tw-ease", None, None)]);
    r.functional_utility(
        "ease",
        FunctionalDesc::new(move |value, _, _| {
            Some(vec![
                ease_property(),
                Node::decl("--tw-ease", value),
                Node::decl("transition-timing-function", value),
            ])
        })
        .theme_keys(&["--ease"])
        .static_value(
            "initial",
            vec![ease_property(), Node::decl("--tw-ease", "initial")],
        )
        .static_value(
            "linear",
            vec![
                ease_property(),
                Node::decl("--tw-ease", "linear"),
                Node::decl("transition-timing-function", "linear"),
            ],
        ),
    );
}

#[cfg(test)]
mod tests {
    use crate::design_system::DesignSystem;
    use crate::stylesheet::candidates_to_css;
    use crate::theme::{Theme, options};

    const FILTER: &str = "filter: var(--tw-blur, ) var(--tw-brightness, ) var(--tw-contrast, ) var(--tw-grayscale, ) var(--tw-hue-rotate, ) var(--tw-invert, ) var(--tw-saturate, ) var(--tw-sepia, ) var(--tw-drop-shadow, );";
    const BACKDROP: &str = "var(--tw-backdrop-blur, ) var(--tw-backdrop-brightness, ) var(--tw-backdrop-contrast, ) var(--tw-backdrop-grayscale, ) var(--tw-backdrop-hue-rotate, ) var(--tw-backdrop-invert, ) var(--tw-backdrop-opacity, ) var(--tw-backdrop-saturate, ) var(--tw-backdrop-sepia, )";

    /// The theme of the upstream test cases ported below.
    fn design_system() -> DesignSystem {
        let vars: &[(&str, &str, u8)] = &[
            ("--spacing", "0.25rem", options::NONE),
            ("--color-red-500", "#ef4444", options::NONE),
            (
                "--drop-shadow",
                "0 1px 1px rgb(0 0 0 / 0.05)",
                options::NONE,
            ),
            (
                "--drop-shadow-xl",
                "0 9px 7px rgb(0 0 0 / 0.1)",
                options::NONE,
            ),
            (
                "--drop-shadow-multi",
                "0 1px 1px rgb(0 0 0 / 0.05), 0 9px 7px rgb(0 0 0 / 0.1)",
                options::INLINE,
            ),
            ("--font-sans", "ui-sans-serif, system-ui", options::NONE),
            (
                "--font-sans--font-feature-settings",
                "\"cv11\"",
                options::NONE,
            ),
            ("--font-weight-bold", "650", options::NONE),
            ("--default-transition-duration", "150ms", options::NONE),
        ];
        let mut theme = Theme::new();
        for (key, value, opts) in vars {
            theme.add(key, value, *opts).expect("valid theme variable");
        }
        DesignSystem::new(theme)
    }

    /// The declarations of the first rule a class generates, one per line.
    fn rule(ds: &mut DesignSystem, class: &str) -> Option<String> {
        let css = candidates_to_css(ds, &[class]).pop().flatten()?;
        let start = css.find("{\n")? + 2;
        let end = css.find("\n}")?;
        Some(
            css[start..end]
                .lines()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    fn assert_rules(ds: &mut DesignSystem, cases: &[(&str, String)]) {
        for (class, expected) in cases {
            assert_eq!(
                rule(ds, class).as_deref(),
                Some(expected.as_str()),
                "{class}"
            );
        }
    }

    fn assert_invalid(ds: &mut DesignSystem, classes: &[&str]) {
        for class in classes {
            assert_eq!(candidates_to_css(ds, &[class]), vec![None], "{class}");
        }
    }

    fn cases(list: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
        list.iter()
            .map(|(class, expected)| (*class, (*expected).to_owned()))
            .collect()
    }

    #[test]
    fn font_families_and_weights() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &cases(&[
                (
                    "font-sans",
                    "font-family: var(--font-sans);\nfont-feature-settings: var(--font-sans--font-feature-settings);",
                ),
                ("font-[ui-sans-serif]", "font-family: ui-sans-serif;"),
                (
                    "font-[family-name:var(--my-family)]",
                    "font-family: var(--my-family);",
                ),
                (
                    "font-[var(--my-family)]",
                    "--tw-font-weight: var(--my-family);\nfont-weight: var(--my-family);",
                ),
                (
                    "font-bold",
                    "--tw-font-weight: var(--font-weight-bold);\nfont-weight: var(--font-weight-bold);",
                ),
                ("font-[100]", "--tw-font-weight: 100;\nfont-weight: 100;"),
            ]),
        );
        assert_invalid(
            ds,
            &[
                "font",
                "-font-sans",
                "-font-bold",
                "font-weight-bold",
                "font-sans/foo",
                "font-[ui-sans-serif]/foo",
                "font-bold/foo",
                "font-[100]/foo",
            ],
        );
    }

    #[test]
    fn font_stretch_percentages() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &cases(&[
                (
                    "font-stretch-ultra-expanded",
                    "font-stretch: ultra-expanded;",
                ),
                ("font-stretch-50%", "font-stretch: 50%;"),
                ("font-stretch-200%", "font-stretch: 200%;"),
            ]),
        );
        assert_invalid(
            ds,
            &[
                "font-stretch",
                "font-stretch-20%",
                "font-stretch-50",
                "font-stretch-400%",
                "font-stretch-50.5%",
                "font-stretch-potato",
                "font-stretch-ultra-expanded/foo",
                "font-stretch-50%/foo",
            ],
        );
    }

    #[test]
    fn stroke_colors_and_widths() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &cases(&[
                (
                    "stroke-red-500/50",
                    "stroke: color-mix(in oklab, var(--color-red-500) 50%, transparent);",
                ),
                ("stroke-current", "stroke: currentcolor;"),
                ("stroke-[color:var(--my-color)]", "stroke: var(--my-color);"),
                ("stroke-2", "stroke-width: 2;"),
                ("stroke-[1.5]", "stroke-width: 1.5;"),
                ("stroke-[12px]", "stroke-width: 12px;"),
                ("stroke-none", "stroke: none;"),
            ]),
        );
        assert_invalid(
            ds,
            &[
                "stroke",
                "stroke-unknown",
                "stroke-2/50",
                "stroke-[12px]/50",
            ],
        );
    }

    #[test]
    fn decoration_thickness_and_color() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &cases(&[
                ("decoration-2", "text-decoration-thickness: 2px;"),
                ("decoration-[12px]", "text-decoration-thickness: 12px;"),
                (
                    "decoration-red-500/50",
                    "text-decoration-color: color-mix(in oklab, var(--color-red-500) 50%, transparent);",
                ),
                (
                    "decoration-from-font",
                    "text-decoration-thickness: from-font;",
                ),
            ]),
        );
        assert_invalid(
            ds,
            &["decoration", "decoration-2/50", "decoration-[12px]/50"],
        );
    }

    #[test]
    fn filter_functions() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &[
                (
                    "grayscale",
                    format!("--tw-grayscale: grayscale(100%);\n{FILTER}"),
                ),
                (
                    "brightness-50",
                    format!("--tw-brightness: brightness(50%);\n{FILTER}"),
                ),
                (
                    "-hue-rotate-15",
                    format!("--tw-hue-rotate: hue-rotate(calc(15deg * -1));\n{FILTER}"),
                ),
                ("filter-none", "filter: none;".to_owned()),
                (
                    "backdrop-opacity-50",
                    format!(
                        "--tw-backdrop-opacity: opacity(50%);\n-webkit-backdrop-filter: {BACKDROP};\nbackdrop-filter: {BACKDROP};"
                    ),
                ),
                (
                    "backdrop-filter-[blur(1px)]",
                    "-webkit-backdrop-filter: blur(1px);\nbackdrop-filter: blur(1px);".to_owned(),
                ),
            ],
        );
        assert_invalid(
            ds,
            &[
                "filter/foo",
                "filter-unknown",
                "grayscale/foo",
                "brightness-50.5",
                "-grayscale",
            ],
        );
    }

    #[test]
    fn drop_shadow_sizes_colors_and_alpha() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &[
                (
                    "drop-shadow",
                    format!(
                        "--tw-drop-shadow-size: drop-shadow(0 1px 1px var(--tw-drop-shadow-color, rgb(0 0 0 / 0.05)));\n--tw-drop-shadow: drop-shadow(var(--drop-shadow));\n{FILTER}"
                    ),
                ),
                (
                    "drop-shadow/25",
                    format!(
                        "--tw-drop-shadow-alpha: 25%;\n--tw-drop-shadow-size: drop-shadow(0 1px 1px var(--tw-drop-shadow-color, oklab(from rgb(0 0 0 / 0.05) l a b / 25%)));\n--tw-drop-shadow: drop-shadow(var(--drop-shadow));\n{FILTER}"
                    ),
                ),
                (
                    "drop-shadow-xl/[var(--a)]",
                    format!(
                        "--tw-drop-shadow-alpha: var(--a);\n--tw-drop-shadow-size: drop-shadow(0 9px 7px var(--tw-drop-shadow-color, rgb(0 0 0 / 0.1)));\n@supports (color: lab(from red l a b)) {{\n--tw-drop-shadow-size: drop-shadow(0 9px 7px var(--tw-drop-shadow-color, oklab(from rgb(0 0 0 / 0.1) l a b / var(--a))));\n}}\n--tw-drop-shadow: var(--tw-drop-shadow-size);\n{FILTER}"
                    ),
                ),
                (
                    "drop-shadow-[0_0_red]",
                    format!(
                        "--tw-drop-shadow-size: drop-shadow(0 0 var(--tw-drop-shadow-color, red));\n--tw-drop-shadow: var(--tw-drop-shadow-size);\n{FILTER}"
                    ),
                ),
                (
                    "drop-shadow-red-500",
                    "--tw-drop-shadow-color: color-mix(in oklab, var(--color-red-500) var(--tw-drop-shadow-alpha), transparent);\n--tw-drop-shadow: var(--tw-drop-shadow-size);".to_owned(),
                ),
                (
                    "drop-shadow-inherit",
                    "--tw-drop-shadow-color: inherit;\n--tw-drop-shadow: var(--tw-drop-shadow-size);"
                        .to_owned(),
                ),
            ],
        );
        assert_invalid(
            ds,
            &[
                "-drop-shadow",
                "drop-shadow/foo",
                "drop-shadow-xl/foo",
                "drop-shadow-unknown",
            ],
        );
    }

    #[test]
    fn transitions_durations_and_easing() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &cases(&[
                (
                    "transition-opacity",
                    "transition-property: opacity;\ntransition-timing-function: var(--tw-ease, ease);\ntransition-duration: var(--tw-duration, var(--default-transition-duration));",
                ),
                ("transition-none", "transition-property: none;"),
                (
                    "duration-150",
                    "--tw-duration: 150ms;\ntransition-duration: 150ms;",
                ),
                ("delay-75", "transition-delay: 75ms;"),
                ("ease-initial", "--tw-ease: initial;"),
                (
                    "ease-[cubic-bezier(0.4,0,0.2,1)]",
                    "--tw-ease: cubic-bezier(0.4,0,0.2,1);\ntransition-timing-function: cubic-bezier(0.4,0,0.2,1);",
                ),
            ]),
        );
        assert_invalid(
            ds,
            &[
                "duration",
                "duration/foo",
                "duration-150/foo",
                "delay-1.5",
                "-ease-linear",
            ],
        );
    }

    #[test]
    fn spacing_position_and_containment() {
        let ds = &mut design_system();
        assert_rules(
            ds,
            &cases(&[
                ("p-4", "padding: calc(var(--spacing) * 4);"),
                ("pbs-px", "padding-block-start: 1px;"),
                ("-indent-4", "text-indent: calc(var(--spacing) * -4);"),
                ("object-top-left", "object-position: left top;"),
                ("align-(--my-align)", "vertical-align: var(--my-align);"),
                ("mix-blend-plus-lighter", "mix-blend-mode: plus-lighter;"),
                (
                    "contain-inline-size",
                    "--tw-contain-size: inline-size;\ncontain: var(--tw-contain-size, ) var(--tw-contain-layout, ) var(--tw-contain-paint, ) var(--tw-contain-style, );",
                ),
                (
                    "content-[\"a\"]",
                    "--tw-content: \"a\";\ncontent: var(--tw-content);",
                ),
            ]),
        );
        assert_invalid(ds, &["-p-4", "object-top/foo", "align", "contain"]);
    }
}
