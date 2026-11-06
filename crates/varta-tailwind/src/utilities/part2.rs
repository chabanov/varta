//! Built-in utilities, part 2: a port of lines 2315–3801 of Tailwind CSS
//! v4.3.5 `packages/tailwindcss/src/utilities.ts` (Tailwind Labs, MIT; see
//! LICENSE-TAILWIND), from `scrollbar-gutter-auto` up to (excluding) the
//! `background-clip` section: text overflow and wrapping, border radius,
//! border styles, widths and colors, divide, backgrounds and gradients, and
//! masks. Registration follows Tailwind's source order exactly.
use super::{
    Compiled, DefaultValue, FunctionalDesc, Registrar, as_color, property, resolve_theme_color,
};
use crate::ast::Node;
use crate::candidate::{Candidate, CandidateModifier, UtilityValue};
use crate::design_system::DesignSystem;
use crate::theme::{Theme, options};
use crate::utils::infer_data_type::{
    infer_data_type, is_positive_integer, is_valid_spacing_multiplier,
};
use crate::utils::js_number::to_number;
use std::rc::Rc;

/// Fixed declarations of one static utility.
type StaticTable = &'static [(&'static str, &'static [(&'static str, &'static str)])];

/// Register a table of static utilities in order.
fn static_table(r: &mut Registrar<'_>, table: StaticTable) {
    for (name, declarations) in table {
        r.static_utility(name, declarations);
    }
}

/// A JavaScript truthy string (`if (value)`): present and non-empty.
fn truthy(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

/// `value.slice(0, -1)` in UTF-16 code units, or `None` when that cut would
/// split a surrogate pair (the result could then never be a number).
fn js_drop_last(value: &str) -> Option<&str> {
    let last = value.chars().next_back()?;
    (last.len_utf16() == 1).then(|| &value[..value.len() - last.len_utf8()])
}

/// `value.slice(0, -1)` is a positive integer (Tailwind's percentage check);
/// an empty value slices to `""`, which is not one.
fn percentage_is_positive_integer(value: &str) -> bool {
    js_drop_last(value).is_some_and(is_positive_integer)
}

/* ------------------------------ text and wrap ----------------------------- */

const TEXT_STATICS: StaticTable = &[
    ("scrollbar-gutter-auto", &[("scrollbar-gutter", "auto")]),
    ("scrollbar-gutter-stable", &[("scrollbar-gutter", "stable")]),
    (
        "scrollbar-gutter-both",
        &[("scrollbar-gutter", "stable both-edges")],
    ),
    (
        "truncate",
        &[
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
        ],
    ),
    ("text-ellipsis", &[("text-overflow", "ellipsis")]),
    ("text-clip", &[("text-overflow", "clip")]),
    (
        "hyphens-none",
        &[("-webkit-hyphens", "none"), ("hyphens", "none")],
    ),
    (
        "hyphens-manual",
        &[("-webkit-hyphens", "manual"), ("hyphens", "manual")],
    ),
    (
        "hyphens-auto",
        &[("-webkit-hyphens", "auto"), ("hyphens", "auto")],
    ),
    ("whitespace-normal", &[("white-space", "normal")]),
    ("whitespace-nowrap", &[("white-space", "nowrap")]),
    ("whitespace-pre", &[("white-space", "pre")]),
    ("whitespace-pre-line", &[("white-space", "pre-line")]),
    ("whitespace-pre-wrap", &[("white-space", "pre-wrap")]),
    (
        "whitespace-break-spaces",
        &[("white-space", "break-spaces")],
    ),
];

const WRAP_STATICS: StaticTable = &[
    ("text-wrap", &[("text-wrap", "wrap")]),
    ("text-nowrap", &[("text-wrap", "nowrap")]),
    ("text-balance", &[("text-wrap", "balance")]),
    ("text-pretty", &[("text-wrap", "pretty")]),
    (
        "break-normal",
        &[("overflow-wrap", "normal"), ("word-break", "normal")],
    ),
    ("break-all", &[("word-break", "break-all")]),
    ("break-keep", &[("word-break", "keep-all")]),
    ("wrap-anywhere", &[("overflow-wrap", "anywhere")]),
    ("wrap-break-word", &[("overflow-wrap", "break-word")]),
    ("wrap-normal", &[("overflow-wrap", "normal")]),
];

/* ------------------------------ border radius ----------------------------- */

/// `rounded-*` roots and the radius properties each sets.
const RADIUS_UTILITIES: &[(&str, &[&str])] = &[
    ("rounded", &["border-radius"]),
    (
        "rounded-s",
        &["border-start-start-radius", "border-end-start-radius"],
    ),
    (
        "rounded-e",
        &["border-start-end-radius", "border-end-end-radius"],
    ),
    (
        "rounded-t",
        &["border-top-left-radius", "border-top-right-radius"],
    ),
    (
        "rounded-r",
        &["border-top-right-radius", "border-bottom-right-radius"],
    ),
    (
        "rounded-b",
        &["border-bottom-right-radius", "border-bottom-left-radius"],
    ),
    (
        "rounded-l",
        &["border-top-left-radius", "border-bottom-left-radius"],
    ),
    ("rounded-ss", &["border-start-start-radius"]),
    ("rounded-se", &["border-start-end-radius"]),
    ("rounded-ee", &["border-end-end-radius"]),
    ("rounded-es", &["border-end-start-radius"]),
    ("rounded-tl", &["border-top-left-radius"]),
    ("rounded-tr", &["border-top-right-radius"]),
    ("rounded-br", &["border-bottom-right-radius"]),
    ("rounded-bl", &["border-bottom-left-radius"]),
];

/// The `rounded-full` radius.
const FULL_RADIUS: &str = "calc(infinity * 1px)";

fn decls_for(properties: &[&str], value: &str) -> Vec<Node> {
    properties
        .iter()
        .map(|property| Node::decl(*property, value))
        .collect()
}

fn register_radius(r: &mut Registrar<'_>) {
    for (root, properties) in RADIUS_UTILITIES {
        let desc = FunctionalDesc::new(move |value, _, _| Some(decls_for(properties, value)))
            .theme_keys(&["--radius"])
            .static_value("none", decls_for(properties, "0"))
            .static_value("full", decls_for(properties, FULL_RADIUS));
        r.functional_utility(root, desc);
    }
}

/* --------------------------------- borders -------------------------------- */

/// Border styles of `border-*` and `divide-*`.
const BORDER_STYLES: &[&str] = &["solid", "dashed", "dotted", "double", "hidden", "none"];
const DIVIDE_STYLES: &[&str] = &["solid", "dashed", "dotted", "double", "none"];

/// The border width used without a value when the theme sets none.
const DEFAULT_BORDER_WIDTH: &str = "1px";
const BORDER_STYLE_VAR: &str = "var(--tw-border-style)";
/// The children a `divide-*` utility styles.
const DIVIDE_SELECTOR: &str = ":where(& > :not(:last-child))";

/// One `border*` side: its style, width and color properties.
struct BorderSide {
    root: &'static str,
    style: &'static str,
    width: &'static str,
    color: &'static str,
}

const BORDER_SIDES: &[BorderSide] = &[
    BorderSide {
        root: "border",
        style: "border-style",
        width: "border-width",
        color: "border-color",
    },
    BorderSide {
        root: "border-x",
        style: "border-inline-style",
        width: "border-inline-width",
        color: "border-inline-color",
    },
    BorderSide {
        root: "border-y",
        style: "border-block-style",
        width: "border-block-width",
        color: "border-block-color",
    },
    BorderSide {
        root: "border-s",
        style: "border-inline-start-style",
        width: "border-inline-start-width",
        color: "border-inline-start-color",
    },
    BorderSide {
        root: "border-e",
        style: "border-inline-end-style",
        width: "border-inline-end-width",
        color: "border-inline-end-color",
    },
    BorderSide {
        root: "border-bs",
        style: "border-block-start-style",
        width: "border-block-start-width",
        color: "border-block-start-color",
    },
    BorderSide {
        root: "border-be",
        style: "border-block-end-style",
        width: "border-block-end-width",
        color: "border-block-end-color",
    },
    BorderSide {
        root: "border-t",
        style: "border-top-style",
        width: "border-top-width",
        color: "border-top-color",
    },
    BorderSide {
        root: "border-r",
        style: "border-right-style",
        width: "border-right-width",
        color: "border-right-color",
    },
    BorderSide {
        root: "border-b",
        style: "border-bottom-style",
        width: "border-bottom-width",
        color: "border-bottom-color",
    },
    BorderSide {
        root: "border-l",
        style: "border-left-style",
        width: "border-left-width",
        color: "border-left-color",
    },
];

/// `borderProperties()`.
fn border_properties() -> Node {
    Node::AtRoot(vec![property("--tw-border-style", Some("solid"), None)])
}

impl BorderSide {
    /// The width declarations, preceded by the border `@property`.
    fn width_nodes(&self, value: &str) -> Compiled {
        Compiled::Nodes(vec![
            border_properties(),
            Node::decl(self.style, BORDER_STYLE_VAR),
            Node::decl(self.width, value),
        ])
    }

    fn color_nodes(&self, value: &str) -> Compiled {
        Compiled::Nodes(vec![Node::decl(self.color, value)])
    }

    /// The compile function of `borderSideUtility`.
    fn compile(&self, candidate: &Candidate, ds: &DesignSystem) -> Compiled {
        let theme = &ds.theme;
        let modifier = candidate.modifier();
        let Some(value) = candidate.value() else {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            let value = theme
                .get(&["--default-border-width"])
                .unwrap_or_else(|| DEFAULT_BORDER_WIDTH.to_owned());
            return self.width_nodes(&value);
        };
        let value = match value {
            UtilityValue::Arbitrary { value, data_type } => {
                let data_type = data_type
                    .as_deref()
                    .or_else(|| infer_data_type(value, &["color", "line-width", "length"]));
                return match data_type {
                    Some("line-width" | "length") if modifier.is_some() => Compiled::Skip,
                    Some("line-width" | "length") => self.width_nodes(value),
                    _ => match as_color(value, modifier, theme) {
                        Some(color) => self.color_nodes(&color),
                        None => Compiled::Skip,
                    },
                };
            }
            UtilityValue::Named { value, .. } => value,
        };
        if let Some(color) = truthy(resolve_theme_color(
            candidate,
            theme,
            &["--border-color", "--color"],
        )) {
            return self.color_nodes(&color);
        }
        if modifier.is_some() {
            return Compiled::Skip;
        }
        if let Some(width) = truthy(theme.resolve(Some(value), &["--border-width"], options::NONE))
        {
            return self.width_nodes(&width);
        }
        if is_positive_integer(value) {
            return self.width_nodes(&format!("{value}px"));
        }
        Compiled::Skip
    }
}

/// A `divide-x`/`divide-y` width utility.
struct DivideAxis {
    root: &'static str,
    reverse: &'static str,
    sort: &'static str,
    /// Style properties of the axis.
    styles: &'static [&'static str],
    /// The start and end width properties.
    start: &'static str,
    end: &'static str,
}

const DIVIDE_AXES: &[DivideAxis] = &[
    DivideAxis {
        root: "divide-x",
        reverse: "--tw-divide-x-reverse",
        sort: "divide-x-width",
        styles: &["border-inline-style"],
        start: "border-inline-start-width",
        end: "border-inline-end-width",
    },
    DivideAxis {
        root: "divide-y",
        reverse: "--tw-divide-y-reverse",
        sort: "divide-y-width",
        styles: &["border-bottom-style", "border-top-style"],
        start: "border-top-width",
        end: "border-bottom-width",
    },
];

impl DivideAxis {
    fn nodes(&self, value: &str) -> Vec<Node> {
        let reverse = self.reverse;
        let mut inner = vec![
            Node::decl("--tw-sort", self.sort),
            border_properties(),
            Node::decl(reverse, "0"),
        ];
        inner.extend(
            self.styles
                .iter()
                .map(|style| Node::decl(*style, BORDER_STYLE_VAR)),
        );
        inner.push(Node::decl(
            self.start,
            format!("calc({value} * var({reverse}))"),
        ));
        inner.push(Node::decl(
            self.end,
            format!("calc({value} * calc(1 - var({reverse})))"),
        ));
        vec![
            Node::AtRoot(vec![property(reverse, Some("0"), None)]),
            Node::rule(DIVIDE_SELECTOR, inner),
        ]
    }
}

fn register_borders(r: &mut Registrar<'_>) {
    for style in BORDER_STYLES {
        r.static_utility(
            &format!("border-{style}"),
            &[("--tw-border-style", style), ("border-style", style)],
        );
    }

    for side in BORDER_SIDES {
        r.utilities
            .functional(side.root, move |candidate, ds| side.compile(candidate, ds));
    }

    let default_width = r
        .theme
        .get(&["--default-border-width"])
        .unwrap_or_else(|| DEFAULT_BORDER_WIDTH.to_owned());
    for axis in DIVIDE_AXES {
        let desc = FunctionalDesc::new(move |value, _, _| Some(axis.nodes(value)))
            .default_value(DefaultValue::Value(default_width.clone()))
            .theme_keys(&["--divide-width", "--border-width"])
            .bare(|value, _| {
                is_positive_integer(value.value).then(|| format!("{}px", value.value))
            });
        r.functional_utility(axis.root, desc);
    }

    for axis in DIVIDE_AXES {
        r.static_nodes(
            &format!("{}-reverse", axis.root),
            vec![
                Node::AtRoot(vec![property(axis.reverse, Some("0"), None)]),
                Node::rule(DIVIDE_SELECTOR, vec![Node::decl(axis.reverse, "1")]),
            ],
        );
    }

    for style in DIVIDE_STYLES {
        r.static_nodes(
            &format!("divide-{style}"),
            vec![Node::rule(
                DIVIDE_SELECTOR,
                vec![
                    Node::decl("--tw-sort", "divide-style"),
                    Node::decl("--tw-border-style", *style),
                    Node::decl("border-style", *style),
                ],
            )],
        );
    }
}

/* ------------------------------- backgrounds ------------------------------ */

/// A functional utility setting `property` to any non-empty value.
fn plain_value_utility(r: &mut Registrar<'_>, root: &str, property: &'static str) {
    let desc = FunctionalDesc::new(move |value, _, _| {
        (!value.is_empty()).then(|| vec![Node::decl(property, value)])
    });
    r.functional_utility(root, desc);
}

const BG_SIZE_STATICS: StaticTable = &[
    ("bg-auto", &[("background-size", "auto")]),
    ("bg-cover", &[("background-size", "cover")]),
    ("bg-contain", &[("background-size", "contain")]),
];

const BG_ATTACHMENT_POSITION_STATICS: StaticTable = &[
    ("bg-fixed", &[("background-attachment", "fixed")]),
    ("bg-local", &[("background-attachment", "local")]),
    ("bg-scroll", &[("background-attachment", "scroll")]),
    ("bg-top", &[("background-position", "top")]),
    ("bg-top-left", &[("background-position", "left top")]),
    ("bg-top-right", &[("background-position", "right top")]),
    ("bg-bottom", &[("background-position", "bottom")]),
    ("bg-bottom-left", &[("background-position", "left bottom")]),
    (
        "bg-bottom-right",
        &[("background-position", "right bottom")],
    ),
    ("bg-left", &[("background-position", "left")]),
    ("bg-right", &[("background-position", "right")]),
    ("bg-center", &[("background-position", "center")]),
];

const BG_REPEAT_STATICS: StaticTable = &[
    ("bg-repeat", &[("background-repeat", "repeat")]),
    ("bg-no-repeat", &[("background-repeat", "no-repeat")]),
    ("bg-repeat-x", &[("background-repeat", "repeat-x")]),
    ("bg-repeat-y", &[("background-repeat", "repeat-y")]),
    ("bg-repeat-round", &[("background-repeat", "round")]),
    ("bg-repeat-space", &[("background-repeat", "space")]),
    ("bg-none", &[("background-image", "none")]),
];

/// Named `bg-linear-*` directions.
const LINEAR_GRADIENT_DIRECTIONS: &[(&str, &str)] = &[
    ("to-t", "to top"),
    ("to-tr", "to top right"),
    ("to-r", "to right"),
    ("to-br", "to bottom right"),
    ("to-b", "to bottom"),
    ("to-bl", "to bottom left"),
    ("to-l", "to left"),
    ("to-tl", "to top left"),
];

/// Hue interpolation methods a named modifier may select.
const HUE_INTERPOLATION_METHODS: &[&str] = &["longer", "shorter", "increasing", "decreasing"];
const DEFAULT_INTERPOLATION: &str = "in oklab";
const GRADIENT_POSITION: &str = "--tw-gradient-position";
/// The `@supports` guard of interpolated linear gradients.
const LINEAR_INTERPOLATION_SUPPORTS: &str =
    "@supports (background-image: linear-gradient(in lab, red, red))";

/// `resolveInterpolationModifier`.
fn resolve_interpolation_modifier(modifier: Option<&CandidateModifier>) -> String {
    match modifier {
        Some(CandidateModifier::Named(name))
            if HUE_INTERPOLATION_METHODS.contains(&name.as_str()) =>
        {
            format!("in oklch {name} hue")
        }
        Some(CandidateModifier::Named(name)) => format!("in {name}"),
        Some(CandidateModifier::Arbitrary(value)) => value.clone(),
        None => DEFAULT_INTERPOLATION.to_owned(),
    }
}

/// `--tw-gradient-position` and an image with the position as the fallback
/// of the stops (`linear-gradient(var(--tw-gradient-stops,…))`).
fn arbitrary_gradient(function: &str, value: &str) -> Compiled {
    Compiled::Nodes(vec![
        Node::decl(GRADIENT_POSITION, value),
        Node::decl(
            "background-image",
            format!("{function}(var(--tw-gradient-stops,{value}))"),
        ),
    ])
}

/// `--tw-gradient-position` and an image of the stops.
fn stops_gradient(function: &str, position: String) -> Compiled {
    Compiled::Nodes(vec![
        Node::decl(GRADIENT_POSITION, position),
        Node::decl(
            "background-image",
            format!("{function}(var(--tw-gradient-stops))"),
        ),
    ])
}

/// A bare angle in degrees, negated with `calc(… * -1)`.
fn degrees(value: &str, negative: bool) -> String {
    if negative {
        format!("calc({value}deg * -1)")
    } else {
        format!("{value}deg")
    }
}

/// `handleBgLinear`.
fn bg_linear(candidate: &Candidate, negative: bool) -> Compiled {
    let modifier = candidate.modifier();
    let value = match candidate.value() {
        None => return Compiled::Skip,
        Some(UtilityValue::Arbitrary { value, data_type }) => {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            let data_type = data_type
                .as_deref()
                .or_else(|| infer_data_type(value, &["angle"]));
            let value = match data_type {
                Some("angle") if negative => format!("calc({value} * -1)"),
                Some("angle") => value.clone(),
                _ if negative => return Compiled::Skip,
                _ => value.clone(),
            };
            return arbitrary_gradient("linear-gradient", &value);
        }
        Some(UtilityValue::Named { value, .. }) => value,
    };
    let direction = LINEAR_GRADIENT_DIRECTIONS
        .iter()
        .find(|(name, _)| *name == value)
        .filter(|_| !negative);
    let value = if let Some((_, direction)) = direction {
        (*direction).to_owned()
    } else if is_positive_integer(value) {
        degrees(value, negative)
    } else {
        return Compiled::Skip;
    };
    let interpolation = resolve_interpolation_modifier(modifier);
    Compiled::Nodes(vec![
        Node::decl(GRADIENT_POSITION, value.clone()),
        Node::from_prelude(
            LINEAR_INTERPOLATION_SUPPORTS,
            vec![Node::decl(
                GRADIENT_POSITION,
                format!("{value} {interpolation}"),
            )],
        ),
        Node::decl(
            "background-image",
            "linear-gradient(var(--tw-gradient-stops))",
        ),
    ])
}

/// `handleBgConic`.
fn bg_conic(candidate: &Candidate, negative: bool) -> Compiled {
    let modifier = candidate.modifier();
    if let Some(UtilityValue::Arbitrary { value, .. }) = candidate.value() {
        if modifier.is_some() {
            return Compiled::Skip;
        }
        return arbitrary_gradient("conic-gradient", value);
    }
    let interpolation = resolve_interpolation_modifier(modifier);
    let Some(value) = candidate.value() else {
        return stops_gradient("conic-gradient", interpolation);
    };
    let value = value.value();
    if !is_positive_integer(value) {
        return Compiled::Skip;
    }
    stops_gradient(
        "conic-gradient",
        format!("from {} {interpolation}", degrees(value, negative)),
    )
}

/// The `bg-radial` compile function.
fn bg_radial(candidate: &Candidate) -> Compiled {
    let modifier = candidate.modifier();
    match candidate.value() {
        None => stops_gradient("radial-gradient", resolve_interpolation_modifier(modifier)),
        Some(UtilityValue::Arbitrary { .. }) if modifier.is_some() => Compiled::Skip,
        Some(UtilityValue::Arbitrary { value, .. }) => arbitrary_gradient("radial-gradient", value),
        Some(UtilityValue::Named { .. }) => Compiled::Skip,
    }
}

/// The `bg` compile function.
fn bg(candidate: &Candidate, ds: &DesignSystem) -> Compiled {
    let theme = &ds.theme;
    let modifier = candidate.modifier();
    let value = match candidate.value() {
        None => return Compiled::Skip,
        Some(UtilityValue::Arbitrary { value, data_type }) => {
            let data_type = data_type.as_deref().or_else(|| {
                infer_data_type(
                    value,
                    &[
                        "image",
                        "color",
                        "percentage",
                        "position",
                        "bg-size",
                        "length",
                        "url",
                    ],
                )
            });
            let property = match data_type {
                Some("percentage" | "position") => "background-position",
                Some("bg-size" | "length" | "size") => "background-size",
                Some("image" | "url") => "background-image",
                _ => {
                    return match as_color(value, modifier, theme) {
                        Some(color) => Compiled::Nodes(vec![Node::decl("background-color", color)]),
                        None => Compiled::Skip,
                    };
                }
            };
            if modifier.is_some() {
                return Compiled::Skip;
            }
            return Compiled::Nodes(vec![Node::decl(property, value)]);
        }
        Some(UtilityValue::Named { value, .. }) => value,
    };
    if let Some(color) = truthy(resolve_theme_color(
        candidate,
        theme,
        &["--background-color", "--color"],
    )) {
        return Compiled::Nodes(vec![Node::decl("background-color", color)]);
    }
    if modifier.is_some() {
        return Compiled::Skip;
    }
    match truthy(theme.resolve(Some(value), &["--background-image"], options::NONE)) {
        Some(image) => Compiled::Nodes(vec![Node::decl("background-image", image)]),
        None => Compiled::Skip,
    }
}

fn register_backgrounds(r: &mut Registrar<'_>) {
    static_table(r, BG_SIZE_STATICS);
    plain_value_utility(r, "bg-size", "background-size");
    static_table(r, BG_ATTACHMENT_POSITION_STATICS);
    plain_value_utility(r, "bg-position", "background-position");
    static_table(r, BG_REPEAT_STATICS);

    r.utilities
        .functional("-bg-linear", |candidate, _| bg_linear(candidate, true));
    r.utilities
        .functional("bg-linear", |candidate, _| bg_linear(candidate, false));
    r.utilities
        .functional("-bg-conic", |candidate, _| bg_conic(candidate, true));
    r.utilities
        .functional("bg-conic", |candidate, _| bg_conic(candidate, false));
    r.utilities
        .functional("bg-radial", |candidate, _| bg_radial(candidate));
    r.utilities.functional("bg", bg);
}

/* ----------------------------- gradient stops ----------------------------- */

/// The `@property` rules of gradient stops: name, initial value, syntax.
const GRADIENT_STOP_PROPERTIES: &[(&str, Option<&str>, Option<&str>)] = &[
    ("--tw-gradient-position", None, None),
    ("--tw-gradient-from", Some("#0000"), Some("<color>")),
    ("--tw-gradient-via", Some("#0000"), Some("<color>")),
    ("--tw-gradient-to", Some("#0000"), Some("<color>")),
    ("--tw-gradient-stops", None, None),
    ("--tw-gradient-via-stops", None, None),
    (
        "--tw-gradient-from-position",
        Some("0%"),
        Some("<length-percentage>"),
    ),
    (
        "--tw-gradient-via-position",
        Some("50%"),
        Some("<length-percentage>"),
    ),
    (
        "--tw-gradient-to-position",
        Some("100%"),
        Some("<length-percentage>"),
    ),
];

/// The `from`/`to` stop list (the via stops when set).
const GRADIENT_FROM_TO_STOPS: &str = "var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position))";
/// The stop list of `via`.
const GRADIENT_VIA_STOPS: &str = "var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-via) var(--tw-gradient-via-position), var(--tw-gradient-to) var(--tw-gradient-to-position)";

/// `gradientStopProperties()`.
fn gradient_stop_properties() -> Node {
    Node::AtRoot(
        GRADIENT_STOP_PROPERTIES
            .iter()
            .map(|(name, initial, syntax)| property(name, *initial, *syntax))
            .collect(),
    )
}

/// Which half of a stop description a value selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Color,
    Position,
}

impl Stop {
    /// The suffix of the custom property a stop value sets.
    fn suffix(self) -> &'static str {
        match self {
            Stop::Color => "color",
            Stop::Position => "position",
        }
    }
}

/// A stop description: color or position value to nodes.
type StopFn = Rc<dyn Fn(Stop, &str) -> Vec<Node>>;

/// `gradientStopUtility`.
fn gradient_stop_utility(r: &mut Registrar<'_>, root: &str, desc: StopFn) {
    r.utilities.functional(root, move |candidate, ds| {
        let theme = &ds.theme;
        let modifier = candidate.modifier();
        let value = match candidate.value() {
            None => return Compiled::Skip,
            Some(UtilityValue::Arbitrary { value, data_type }) => {
                let data_type = data_type
                    .as_deref()
                    .or_else(|| infer_data_type(value, &["color", "length", "percentage"]));
                return match data_type {
                    Some("length" | "percentage") if modifier.is_some() => Compiled::Skip,
                    Some("length" | "percentage") => Compiled::Nodes(desc(Stop::Position, value)),
                    _ => match as_color(value, modifier, theme) {
                        Some(color) => Compiled::Nodes(desc(Stop::Color, &color)),
                        None => Compiled::Skip,
                    },
                };
            }
            Some(UtilityValue::Named { value, .. }) => value,
        };
        if let Some(color) = truthy(resolve_theme_color(
            candidate,
            theme,
            &["--background-color", "--color"],
        )) {
            return Compiled::Nodes(desc(Stop::Color, &color));
        }
        if modifier.is_some() {
            return Compiled::Skip;
        }
        if let Some(position) = truthy(theme.resolve(
            Some(value),
            &["--gradient-color-stop-positions"],
            options::NONE,
        )) {
            return Compiled::Nodes(desc(Stop::Position, &position));
        }
        if value.ends_with('%') && percentage_is_positive_integer(value) {
            return Compiled::Nodes(desc(Stop::Position, value));
        }
        Compiled::Skip
    });
}

/// A gradient stop (`from`, `via`, `to`): its color declarations after the
/// color itself.
fn gradient_stop(
    name: &'static str,
    color_stops: &'static [(&'static str, &'static str)],
) -> StopFn {
    Rc::new(move |stop, value| {
        let mut nodes = vec![gradient_stop_properties()];
        match stop {
            Stop::Color => {
                let property = format!("--tw-gradient-{name}");
                nodes.push(Node::decl("--tw-sort", property.clone()));
                nodes.push(Node::decl(property, value));
                nodes.extend(
                    color_stops
                        .iter()
                        .map(|(property, value)| Node::decl(*property, *value)),
                );
            }
            Stop::Position => {
                nodes.push(Node::decl(format!("--tw-gradient-{name}-position"), value));
            }
        }
        nodes
    })
}

fn register_gradient_stops(r: &mut Registrar<'_>) {
    gradient_stop_utility(
        r,
        "from",
        gradient_stop("from", &[("--tw-gradient-stops", GRADIENT_FROM_TO_STOPS)]),
    );
    r.static_utility("via-none", &[("--tw-gradient-via-stops", "initial")]);
    gradient_stop_utility(
        r,
        "via",
        gradient_stop(
            "via",
            &[
                ("--tw-gradient-via-stops", GRADIENT_VIA_STOPS),
                ("--tw-gradient-stops", "var(--tw-gradient-via-stops)"),
            ],
        ),
    );
    gradient_stop_utility(
        r,
        "to",
        gradient_stop("to", &[("--tw-gradient-stops", GRADIENT_FROM_TO_STOPS)]),
    );
}

/* ---------------------------------- masks --------------------------------- */

/// The `mask` compile function (arbitrary values only).
fn mask(candidate: &Candidate) -> Compiled {
    let Some(UtilityValue::Arbitrary { value, data_type }) = candidate.value() else {
        return Compiled::Skip;
    };
    if candidate.modifier().is_some() {
        return Compiled::Skip;
    }
    let data_type = data_type.as_deref().or_else(|| {
        infer_data_type(
            value,
            &[
                "image",
                "percentage",
                "position",
                "bg-size",
                "length",
                "url",
            ],
        )
    });
    let property = match data_type {
        Some("percentage" | "position") => "mask-position",
        Some("bg-size" | "length" | "size") => "mask-size",
        _ => "mask-image",
    };
    Compiled::Nodes(vec![Node::decl(property, value)])
}

const MASK_COMPOSITE_MODE_STATICS: StaticTable = &[
    ("mask-add", &[("mask-composite", "add")]),
    ("mask-subtract", &[("mask-composite", "subtract")]),
    ("mask-intersect", &[("mask-composite", "intersect")]),
    ("mask-exclude", &[("mask-composite", "exclude")]),
    ("mask-alpha", &[("mask-mode", "alpha")]),
    ("mask-luminance", &[("mask-mode", "luminance")]),
    ("mask-match", &[("mask-mode", "match-source")]),
    ("mask-type-alpha", &[("mask-type", "alpha")]),
    ("mask-type-luminance", &[("mask-type", "luminance")]),
    ("mask-auto", &[("mask-size", "auto")]),
    ("mask-cover", &[("mask-size", "cover")]),
    ("mask-contain", &[("mask-size", "contain")]),
];

const MASK_POSITION_STATICS: StaticTable = &[
    ("mask-top", &[("mask-position", "top")]),
    ("mask-top-left", &[("mask-position", "left top")]),
    ("mask-top-right", &[("mask-position", "right top")]),
    ("mask-bottom", &[("mask-position", "bottom")]),
    ("mask-bottom-left", &[("mask-position", "left bottom")]),
    ("mask-bottom-right", &[("mask-position", "right bottom")]),
    ("mask-left", &[("mask-position", "left")]),
    ("mask-right", &[("mask-position", "right")]),
    ("mask-center", &[("mask-position", "center")]),
];

const MASK_REPEAT_CLIP_ORIGIN_STATICS: StaticTable = &[
    ("mask-repeat", &[("mask-repeat", "repeat")]),
    ("mask-no-repeat", &[("mask-repeat", "no-repeat")]),
    ("mask-repeat-x", &[("mask-repeat", "repeat-x")]),
    ("mask-repeat-y", &[("mask-repeat", "repeat-y")]),
    ("mask-repeat-round", &[("mask-repeat", "round")]),
    ("mask-repeat-space", &[("mask-repeat", "space")]),
    ("mask-clip-border", &[("mask-clip", "border-box")]),
    ("mask-clip-padding", &[("mask-clip", "padding-box")]),
    ("mask-clip-content", &[("mask-clip", "content-box")]),
    ("mask-clip-fill", &[("mask-clip", "fill-box")]),
    ("mask-clip-stroke", &[("mask-clip", "stroke-box")]),
    ("mask-clip-view", &[("mask-clip", "view-box")]),
    ("mask-no-clip", &[("mask-clip", "no-clip")]),
    ("mask-origin-border", &[("mask-origin", "border-box")]),
    ("mask-origin-padding", &[("mask-origin", "padding-box")]),
    ("mask-origin-content", &[("mask-origin", "content-box")]),
    ("mask-origin-fill", &[("mask-origin", "fill-box")]),
    ("mask-origin-stroke", &[("mask-origin", "stroke-box")]),
    ("mask-origin-view", &[("mask-origin", "view-box")]),
];

const MASK_RADIAL_STATICS: StaticTable = &[
    ("mask-circle", &[("--tw-mask-radial-shape", "circle")]),
    ("mask-ellipse", &[("--tw-mask-radial-shape", "ellipse")]),
    (
        "mask-radial-closest-side",
        &[("--tw-mask-radial-size", "closest-side")],
    ),
    (
        "mask-radial-farthest-side",
        &[("--tw-mask-radial-size", "farthest-side")],
    ),
    (
        "mask-radial-closest-corner",
        &[("--tw-mask-radial-size", "closest-corner")],
    ),
    (
        "mask-radial-farthest-corner",
        &[("--tw-mask-radial-size", "farthest-corner")],
    ),
    (
        "mask-radial-at-top",
        &[("--tw-mask-radial-position", "top")],
    ),
    (
        "mask-radial-at-top-left",
        &[("--tw-mask-radial-position", "top left")],
    ),
    (
        "mask-radial-at-top-right",
        &[("--tw-mask-radial-position", "top right")],
    ),
    (
        "mask-radial-at-bottom",
        &[("--tw-mask-radial-position", "bottom")],
    ),
    (
        "mask-radial-at-bottom-left",
        &[("--tw-mask-radial-position", "bottom left")],
    ),
    (
        "mask-radial-at-bottom-right",
        &[("--tw-mask-radial-position", "bottom right")],
    ),
    (
        "mask-radial-at-left",
        &[("--tw-mask-radial-position", "left")],
    ),
    (
        "mask-radial-at-right",
        &[("--tw-mask-radial-position", "right")],
    ),
    (
        "mask-radial-at-center",
        &[("--tw-mask-radial-position", "center")],
    ),
];

const BOX_DECORATION_STATICS: StaticTable = &[
    (
        "box-decoration-slice",
        &[
            ("-webkit-box-decoration-break", "slice"),
            ("box-decoration-break", "slice"),
        ],
    ),
    (
        "box-decoration-clone",
        &[
            ("-webkit-box-decoration-break", "clone"),
            ("box-decoration-break", "clone"),
        ],
    ),
];

/// The opaque gradient every mask layer starts as.
const OPAQUE_MASK: &str = "linear-gradient(#fff, #fff)";
/// The combined mask layers.
const MASK_IMAGE_LAYERS: &str =
    "var(--tw-mask-linear), var(--tw-mask-radial), var(--tw-mask-conic)";
/// The combined edge masks.
const MASK_EDGE_LAYERS: &str =
    "var(--tw-mask-left), var(--tw-mask-right), var(--tw-mask-bottom), var(--tw-mask-top)";

/// `@property` rules: name and initial value (universal syntax).
type PropertyTable = &'static [(&'static str, &'static str)];

const MASK_GRADIENT_PROPERTIES: PropertyTable = &[
    ("--tw-mask-linear", OPAQUE_MASK),
    ("--tw-mask-radial", OPAQUE_MASK),
    ("--tw-mask-conic", OPAQUE_MASK),
];
const MASK_EDGE_PROPERTIES: PropertyTable = &[
    ("--tw-mask-left", OPAQUE_MASK),
    ("--tw-mask-right", OPAQUE_MASK),
    ("--tw-mask-bottom", OPAQUE_MASK),
    ("--tw-mask-top", OPAQUE_MASK),
];
const MASK_LINEAR_PROPERTIES: PropertyTable = &[
    ("--tw-mask-linear-position", "0deg"),
    ("--tw-mask-linear-from-position", "0%"),
    ("--tw-mask-linear-to-position", "100%"),
    ("--tw-mask-linear-from-color", "black"),
    ("--tw-mask-linear-to-color", "transparent"),
];
const MASK_RADIAL_PROPERTIES: PropertyTable = &[
    ("--tw-mask-radial-from-position", "0%"),
    ("--tw-mask-radial-to-position", "100%"),
    ("--tw-mask-radial-from-color", "black"),
    ("--tw-mask-radial-to-color", "transparent"),
    ("--tw-mask-radial-shape", "ellipse"),
    ("--tw-mask-radial-size", "farthest-corner"),
    ("--tw-mask-radial-position", "center"),
];
const MASK_CONIC_PROPERTIES: PropertyTable = &[
    ("--tw-mask-conic-position", "0deg"),
    ("--tw-mask-conic-from-position", "0%"),
    ("--tw-mask-conic-to-position", "100%"),
    ("--tw-mask-conic-from-color", "black"),
    ("--tw-mask-conic-to-color", "transparent"),
];

/// Hoisted `@property` rules of a table.
fn at_root_properties(table: PropertyTable) -> Node {
    Node::AtRoot(
        table
            .iter()
            .map(|(name, initial)| property(name, Some(initial), None))
            .collect(),
    )
}

/// The declarations shared by every mask-gradient utility.
fn mask_common(nodes: &mut Vec<Node>) {
    nodes.push(Node::decl("mask-image", MASK_IMAGE_LAYERS));
    nodes.push(Node::decl("mask-composite", "intersect"));
}

/// `maskStopUtility`.
fn mask_stop_utility(r: &mut Registrar<'_>, root: &str, desc: StopFn) {
    r.utilities.functional(root, move |candidate, ds| {
        let theme = &ds.theme;
        let modifier = candidate.modifier();
        let value = match candidate.value() {
            None => return Compiled::Skip,
            Some(UtilityValue::Arbitrary { value, data_type }) => {
                let data_type = data_type
                    .as_deref()
                    .or_else(|| infer_data_type(value, &["length", "percentage", "color"]));
                return match data_type {
                    Some("color") => match as_color(value, modifier, theme) {
                        Some(color) => Compiled::Nodes(desc(Stop::Color, &color)),
                        None => Compiled::Skip,
                    },
                    _ if modifier.is_some() => Compiled::Skip,
                    Some("percentage") if !percentage_is_positive_integer(value) => Compiled::Skip,
                    _ => Compiled::Nodes(desc(Stop::Position, value)),
                };
            }
            Some(UtilityValue::Named { value, .. }) => value,
        };
        if let Some(color) = truthy(resolve_theme_color(
            candidate,
            theme,
            &["--background-color", "--color"],
        )) {
            return Compiled::Nodes(desc(Stop::Color, &color));
        }
        if modifier.is_some() {
            return Compiled::Skip;
        }
        mask_named_position(value, theme).map_or(Compiled::Skip, |position| {
            Compiled::Nodes(desc(Stop::Position, &position))
        })
    });
}

/// A named mask stop position: a spacing multiple or a whole percentage.
fn mask_named_position(value: &str, theme: &Theme) -> Option<String> {
    match infer_data_type(value, &["number", "percentage"])? {
        "number" => {
            truthy(theme.resolve(None, &["--spacing"], options::NONE))?;
            is_valid_spacing_multiplier(value).then(|| format!("--spacing({value})"))
        }
        "percentage" => percentage_is_positive_integer(value).then(|| value.to_owned()),
        _ => None,
    }
}

/// The edges of a mask, in Tailwind's emission order.
const MASK_EDGES: &[&str] = &["top", "right", "bottom", "left"];

/// `maskEdgeUtility` roots: name, stop and the edges it masks.
const MASK_EDGE_UTILITIES: &[(&str, &str, &[&str])] = &[
    ("mask-x-from", "from", &["right", "left"]),
    ("mask-x-to", "to", &["right", "left"]),
    ("mask-y-from", "from", &["top", "bottom"]),
    ("mask-y-to", "to", &["top", "bottom"]),
    ("mask-t-from", "from", &["top"]),
    ("mask-t-to", "to", &["top"]),
    ("mask-r-from", "from", &["right"]),
    ("mask-r-to", "to", &["right"]),
    ("mask-b-from", "from", &["bottom"]),
    ("mask-b-to", "to", &["bottom"]),
    ("mask-l-from", "from", &["left"]),
    ("mask-l-to", "to", &["left"]),
];

/// The color/position nodes of an edge mask (`maskEdgeUtility`).
fn mask_edge(stop_name: &'static str, edges: &'static [&'static str]) -> StopFn {
    Rc::new(move |stop, value| {
        let mut nodes = vec![
            at_root_properties(MASK_GRADIENT_PROPERTIES),
            at_root_properties(MASK_EDGE_PROPERTIES),
        ];
        mask_common(&mut nodes);
        nodes.push(Node::decl("--tw-mask-linear", MASK_EDGE_LAYERS));
        for edge in MASK_EDGES.iter().filter(|edge| edges.contains(edge)) {
            nodes.push(Node::decl(
                format!("--tw-mask-{edge}"),
                format!(
                    "linear-gradient(to {edge}, var(--tw-mask-{edge}-from-color) var(--tw-mask-{edge}-from-position), var(--tw-mask-{edge}-to-color) var(--tw-mask-{edge}-to-position))"
                ),
            ));
            nodes.push(Node::AtRoot(vec![
                property(&format!("--tw-mask-{edge}-from-position"), Some("0%"), None),
                property(&format!("--tw-mask-{edge}-to-position"), Some("100%"), None),
                property(&format!("--tw-mask-{edge}-from-color"), Some("black"), None),
                property(
                    &format!("--tw-mask-{edge}-to-color"),
                    Some("transparent"),
                    None,
                ),
            ]));
            nodes.push(Node::decl(
                format!("--tw-mask-{edge}-{stop_name}-{}", stop.suffix()),
                value,
            ));
        }
        nodes
    })
}

/// One gradient mask kind (`linear`, `radial`, `conic`).
struct MaskGradient {
    kind: &'static str,
    properties: PropertyTable,
    /// `--tw-mask-<kind>-stops` of its `-from`/`-to` utilities.
    stops: &'static str,
    function: &'static str,
}

static MASK_LINEAR: MaskGradient = MaskGradient {
    kind: "linear",
    properties: MASK_LINEAR_PROPERTIES,
    stops: "var(--tw-mask-linear-position), var(--tw-mask-linear-from-color) var(--tw-mask-linear-from-position), var(--tw-mask-linear-to-color) var(--tw-mask-linear-to-position)",
    function: "linear-gradient",
};
static MASK_RADIAL: MaskGradient = MaskGradient {
    kind: "radial",
    properties: MASK_RADIAL_PROPERTIES,
    stops: "var(--tw-mask-radial-shape) var(--tw-mask-radial-size) at var(--tw-mask-radial-position), var(--tw-mask-radial-from-color) var(--tw-mask-radial-from-position), var(--tw-mask-radial-to-color) var(--tw-mask-radial-to-position)",
    function: "radial-gradient",
};
static MASK_CONIC: MaskGradient = MaskGradient {
    kind: "conic",
    properties: MASK_CONIC_PROPERTIES,
    stops: "from var(--tw-mask-conic-position), var(--tw-mask-conic-from-color) var(--tw-mask-conic-from-position), var(--tw-mask-conic-to-color) var(--tw-mask-conic-to-position)",
    function: "conic-gradient",
};

impl MaskGradient {
    fn header(&self) -> Vec<Node> {
        let mut nodes = vec![
            at_root_properties(MASK_GRADIENT_PROPERTIES),
            at_root_properties(self.properties),
        ];
        mask_common(&mut nodes);
        nodes
    }

    /// The base utility (`mask-linear-45`): `fallback` is the variable the
    /// gradient falls back to without stops; `value` sets it.
    fn base(&self, fallback: &str, value: &str) -> Vec<Node> {
        let kind = self.kind;
        let mut nodes = self.header();
        nodes.push(Node::decl(
            format!("--tw-mask-{kind}"),
            format!(
                "{}(var(--tw-mask-{kind}-stops, var({fallback})))",
                self.function
            ),
        ));
        nodes.push(Node::decl(fallback, value));
        nodes
    }

    /// A `-from`/`-to` stop utility of this gradient.
    fn stop(&'static self, stop_name: &'static str) -> StopFn {
        Rc::new(move |stop, value| {
            let kind = self.kind;
            let mut nodes = self.header();
            nodes.push(Node::decl(format!("--tw-mask-{kind}-stops"), self.stops));
            nodes.push(Node::decl(
                format!("--tw-mask-{kind}"),
                format!("{}(var(--tw-mask-{kind}-stops))", self.function),
            ));
            nodes.push(Node::decl(
                format!("--tw-mask-{kind}-{stop_name}-{}", stop.suffix()),
                value,
            ));
            nodes
        })
    }
}

/// An angle of `mask-linear-*`/`mask-conic-*` (`handleBareValue`).
fn mask_angle(value: &str) -> Option<String> {
    if !is_positive_integer(value) {
        return None;
    }
    Some(match to_number(value) {
        0.0 => "0deg".to_owned(),
        1.0 => "1deg".to_owned(),
        _ => format!("calc(1deg * {value})"),
    })
}

/// A negative angle of `-mask-linear-*`/`-mask-conic-*`
/// (`handleNegativeBareValue`).
fn mask_negative_angle(value: &str) -> Option<String> {
    if !is_positive_integer(value) {
        return None;
    }
    Some(match to_number(value) {
        0.0 => "0deg".to_owned(),
        1.0 => "-1deg".to_owned(),
        _ => format!("calc(1deg * -{value})"),
    })
}

/// `mask-linear` and `mask-conic`: angles with negative forms.
fn register_mask_angle(
    r: &mut Registrar<'_>,
    root: &str,
    gradient: &'static MaskGradient,
    fallback: &'static str,
) {
    let desc = FunctionalDesc::new(move |value, _, _| Some(gradient.base(fallback, value)))
        .default_value(DefaultValue::Null)
        .negative()
        .bare(|value, _| mask_angle(value.value))
        .negative_bare(|value, _| mask_negative_angle(value.value));
    r.functional_utility(root, desc);
}

fn register_masks(r: &mut Registrar<'_>) {
    r.static_utility("mask-none", &[("mask-image", "none")]);
    r.utilities
        .functional("mask", |candidate, _| mask(candidate));
    static_table(r, MASK_COMPOSITE_MODE_STATICS);
    plain_value_utility(r, "mask-size", "mask-size");
    static_table(r, MASK_POSITION_STATICS);
    plain_value_utility(r, "mask-position", "mask-position");
    static_table(r, MASK_REPEAT_CLIP_ORIGIN_STATICS);

    for (root, stop, edges) in MASK_EDGE_UTILITIES {
        mask_stop_utility(r, root, mask_edge(stop, edges));
    }

    register_mask_angle(r, "mask-linear", &MASK_LINEAR, "--tw-mask-linear-position");
    mask_stop_utility(r, "mask-linear-from", MASK_LINEAR.stop("from"));
    mask_stop_utility(r, "mask-linear-to", MASK_LINEAR.stop("to"));

    static_table(r, MASK_RADIAL_STATICS);
    r.functional_utility(
        "mask-radial-at",
        FunctionalDesc::new(|value, _, _| {
            Some(vec![Node::decl("--tw-mask-radial-position", value)])
        })
        .default_value(DefaultValue::Null),
    );
    r.functional_utility(
        "mask-radial",
        FunctionalDesc::new(|value, _, _| Some(MASK_RADIAL.base("--tw-mask-radial-size", value)))
            .default_value(DefaultValue::Null),
    );
    mask_stop_utility(r, "mask-radial-from", MASK_RADIAL.stop("from"));
    mask_stop_utility(r, "mask-radial-to", MASK_RADIAL.stop("to"));

    register_mask_angle(r, "mask-conic", &MASK_CONIC, "--tw-mask-conic-position");
    mask_stop_utility(r, "mask-conic-from", MASK_CONIC.stop("from"));
    mask_stop_utility(r, "mask-conic-to", MASK_CONIC.stop("to"));

    static_table(r, BOX_DECORATION_STATICS);
}

/// Register lines 2315–3801 of `createUtilities` in source order.
pub(super) fn register(r: &mut Registrar<'_>) {
    static_table(r, TEXT_STATICS);
    r.functional_utility(
        "tab",
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("tab-size", value)]))
            .bare(|value, _| is_positive_integer(value.value).then(|| value.value.to_owned())),
    );
    static_table(r, WRAP_STATICS);
    register_radius(r);
    register_borders(r);
    register_backgrounds(r);
    register_gradient_stops(r);
    register_masks(r);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stylesheet::{candidates_to_css, default_design_system};

    /// The CSS of one candidate in `ds`.
    fn css(ds: &mut DesignSystem, candidate: &str) -> Option<String> {
        candidates_to_css(ds, &[candidate]).pop().flatten()
    }

    /// Ported from `utilities.test.ts` (outputs as `candidatesToCss` prints
    /// them, checked against the official compiler).
    #[test]
    fn upstream_outputs() {
        let mut ds = default_design_system();
        let cases: &[(&str, &str)] = &[
            ("tab-[12px]", ".tab-\\[12px\\] {\n  tab-size: 12px;\n}\n"),
            (
                "rounded-t-full",
                ".rounded-t-full {\n  border-top-left-radius: calc(infinity * 1px);\n  border-top-right-radius: calc(infinity * 1px);\n}\n",
            ),
            (
                "border-x-2",
                ".border-x-2 {\n  border-inline-style: var(--tw-border-style);\n  border-inline-width: 2px;\n}\n@property --tw-border-style {\n  syntax: \"*\";\n  inherits: false;\n  initial-value: solid;\n}\n",
            ),
            (
                "divide-x-4",
                ":where(.divide-x-4 > :not(:last-child)) {\n  --tw-divide-x-reverse: 0;\n  border-inline-style: var(--tw-border-style);\n  border-inline-start-width: calc(4px * var(--tw-divide-x-reverse));\n  border-inline-end-width: calc(4px * calc(1 - var(--tw-divide-x-reverse)));\n}\n@property --tw-divide-x-reverse {\n  syntax: \"*\";\n  inherits: false;\n  initial-value: 0;\n}\n@property --tw-border-style {\n  syntax: \"*\";\n  inherits: false;\n  initial-value: solid;\n}\n",
            ),
            (
                "-bg-conic-30",
                ".-bg-conic-30 {\n  --tw-gradient-position: from calc(30deg * -1) in oklab;\n  background-image: conic-gradient(var(--tw-gradient-stops));\n}\n",
            ),
            (
                "bg-red-500/50",
                ".bg-red-500\\/50 {\n  background-color: color-mix(in oklab, var(--color-red-500) 50%, transparent);\n}\n",
            ),
            (
                "bg-[url(/img.png)]",
                ".bg-\\[url\\(\\/img\\.png\\)\\] {\n  background-image: url(/img.png);\n}\n",
            ),
        ];
        for (candidate, expected) in cases {
            assert_eq!(
                css(&mut ds, candidate).as_deref(),
                Some(*expected),
                "{candidate}"
            );
        }
    }

    #[test]
    fn upstream_gradients_and_masks() {
        let mut ds = default_design_system();
        let contains: &[(&str, &[&str])] = &[
            (
                "bg-linear-45/oklch",
                &[
                    "--tw-gradient-position: 45deg;",
                    "@supports (background-image: linear-gradient(in lab, red, red))",
                    "--tw-gradient-position: 45deg in oklch;",
                    "background-image: linear-gradient(var(--tw-gradient-stops));",
                ],
            ),
            (
                "bg-linear-to-r/longer",
                &["--tw-gradient-position: to right in oklch longer hue;"],
            ),
            (
                "-mask-linear-45",
                &["--tw-mask-linear-position: calc(1deg * -45);"],
            ),
            (
                "mask-r-to-red-500",
                &["--tw-mask-right-to-color: var(--color-red-500);"],
            ),
            (
                "mask-x-from-4",
                &["--tw-mask-left-from-position: calc(var(--spacing) * 4);"],
            ),
            ("from-10%", &["--tw-gradient-from-position: 10%;"]),
            (
                "via-red-500",
                &["--tw-gradient-stops: var(--tw-gradient-via-stops);"],
            ),
        ];
        for (candidate, needles) in contains {
            let Some(output) = css(&mut ds, candidate) else {
                panic!("{candidate} compiled to nothing");
            };
            for needle in *needles {
                assert!(output.contains(needle), "{candidate}: {needle} in {output}");
            }
        }
    }

    /// Candidates Tailwind's tests require to produce nothing.
    #[test]
    fn upstream_invalid_candidates() {
        let mut ds = default_design_system();
        let invalid: &[&str] = &[
            "tab",
            "-tab-2",
            "tab-2.5",
            "tab-1/2",
            "tab-unknown",
            "tab-2/foo",
            "tab-[12px]/foo",
            "-rounded",
            "-rounded-full",
            "rounded/foo",
            "rounded-full/foo",
            "rounded-sm/[5]",
            "rounded-sm/(--foo)",
            "-divide-x",
            "divide-x--4",
            "-divide-x-4",
            "divide-x-unknown",
            "divide-x/foo",
            "divide-x-4/foo",
            "divide-x-[4px]/foo",
            "bg",
            "bg-unknown",
            "-bg-red-500",
            "-bg-[#0088cc]/50",
            "-bg-linear-to-br",
            "-bg-linear-[to_bottom]",
            "bg-none/foo",
            "bg-[url(/image.png)]/foo",
            "bg-linear-[to_bottom]/hsl",
            "bg-conic-[45deg]/hsl",
            "bg-[50%]/foo",
            "bg-[120px_120px]/foo",
            "bg-[size:120px_120px]/foo",
            "bg-size",
            "bg-size/foo",
            "bg-size-[120px_120px]/foo",
            "bg-position",
            "-bg-position-[120px_120px]",
            "from",
            "from-25.%",
            "from-25.0%",
            "from-123",
            "from--5%",
            "from-unknown%",
            "-from-red-500",
            "-from-5%",
            "via-123",
            "to-unknown",
            "mask",
            "mask-unknown",
            "-mask-none",
            "mask-[var(--some-var)]/foo",
            "mask-add/foo",
            "mask-x-from",
            "mask-x-from-2.8175",
            "mask-x-from--2",
            "mask-x-from-2.5%",
            "mask-x-from-unknown%",
            "-mask-x-from-0",
            "mask-x-from-[-25%]",
            "mask-x-from-[25%]/foo",
            "mask-linear",
            "-mask-linear",
            "mask-linear--75",
            "mask-linear-45/foo",
            "mask-linear-[3rad]/foo",
            "mask-radial",
            "mask-radial/foo",
            "-mask-radial-[25%_25%]",
            "mask-radial-at",
            "mask-radial-at-25",
            "mask-radial-at-top/foo",
            "mask-conic--75",
            "-mask-conic-45/foo",
            "mask-conic-to-2.5%",
            "-mask-conic-to-(--my-var)",
        ];
        for candidate in invalid {
            assert_eq!(css(&mut ds, candidate), None, "{candidate}");
        }
    }

    /// `--default-border-width` sets `border` at call time and `divide-x`
    /// at creation time.
    #[test]
    fn custom_default_border_width() {
        let mut theme = Theme::new();
        assert!(
            theme
                .add("--default-border-width", "2px", options::NONE)
                .is_ok()
        );
        let mut ds = DesignSystem::new(theme);
        let border = css(&mut ds, "border").unwrap_or_default();
        assert!(border.contains("border-width: 2px;"), "{border}");
        let divide = css(&mut ds, "divide-x").unwrap_or_default();
        assert!(
            divide.contains("calc(2px * var(--tw-divide-x-reverse))"),
            "{divide}"
        );
    }

    #[test]
    fn javascript_slices() {
        assert_eq!(js_drop_last("50%"), Some("50"));
        assert_eq!(js_drop_last(""), None);
        assert!(percentage_is_positive_integer("25%"));
        assert!(!percentage_is_positive_integer("2.5%"));
        // A supplementary last character leaves a lone surrogate in JS.
        assert!(!percentage_is_positive_integer("5\u{1F600}"));
    }

    #[test]
    fn interpolation_modifiers() {
        assert_eq!(resolve_interpolation_modifier(None), "in oklab");
        assert_eq!(
            resolve_interpolation_modifier(Some(&CandidateModifier::Named("longer".into()))),
            "in oklch longer hue"
        );
        assert_eq!(
            resolve_interpolation_modifier(Some(&CandidateModifier::Named("srgb".into()))),
            "in srgb"
        );
        assert_eq!(
            resolve_interpolation_modifier(Some(&CandidateModifier::Arbitrary(
                "in hsl longer hue".into()
            ))),
            "in hsl longer hue"
        );
    }

    #[test]
    fn mask_angles() {
        assert_eq!(mask_angle("0").as_deref(), Some("0deg"));
        assert_eq!(mask_angle("1").as_deref(), Some("1deg"));
        assert_eq!(mask_angle("45").as_deref(), Some("calc(1deg * 45)"));
        assert_eq!(mask_angle("4.5"), None);
        assert_eq!(mask_negative_angle("1").as_deref(), Some("-1deg"));
        assert_eq!(
            mask_negative_angle("45").as_deref(),
            Some("calc(1deg * -45)")
        );
    }
}
