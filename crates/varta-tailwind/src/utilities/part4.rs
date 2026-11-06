//! Port of Tailwind CSS v4.3.5 `packages/tailwindcss/src/utilities.ts`
//! lines 5006–6092 (the end of `createUtilities`, from
//! `forced-color-adjust-none` to `@container`) and of the module-level helper
//! `alphaReplacedShadowProperties` (lines 6620–6664). Tailwind CSS is
//! copyright Tailwind Labs, Inc., MIT licensed (see LICENSE-TAILWIND).
use super::{
    Compiled, FunctionalDesc, Registrar, SpacingOptions, as_color, property, replace_alpha,
    resolve_theme_color, with_alpha,
};
use crate::ast::Node;
use crate::candidate::{Candidate, CandidateModifier, UtilityValue};
use crate::theme::{Theme, options};
use crate::utils::infer_data_type::{
    infer_data_type, is_positive_integer, is_valid_opacity_value, is_valid_spacing_multiplier,
};
use crate::utils::js_number::js_trim;
use crate::utils::segment;
use std::cell::Cell;

/// The combined `font-variant-numeric` value of the numeric utilities.
const FONT_VARIANT_NUMERIC_VALUE: &str = "var(--tw-ordinal, ) var(--tw-slashed-zero, ) var(--tw-numeric-figure, ) var(--tw-numeric-spacing, ) var(--tw-numeric-fraction, )";
/// The `@property` variables of the numeric utilities, in Tailwind's order.
const FONT_VARIANT_NUMERIC_VARIABLES: &[&str] = &[
    "--tw-ordinal",
    "--tw-slashed-zero",
    "--tw-numeric-figure",
    "--tw-numeric-spacing",
    "--tw-numeric-fraction",
];
/// `(utility, variable, value)` of the `font-variant-numeric` utilities.
const FONT_VARIANT_NUMERIC_UTILITIES: &[(&str, &str, &str)] = &[
    ("ordinal", "--tw-ordinal", "ordinal"),
    ("slashed-zero", "--tw-slashed-zero", "slashed-zero"),
    ("lining-nums", "--tw-numeric-figure", "lining-nums"),
    ("oldstyle-nums", "--tw-numeric-figure", "oldstyle-nums"),
    (
        "proportional-nums",
        "--tw-numeric-spacing",
        "proportional-nums",
    ),
    ("tabular-nums", "--tw-numeric-spacing", "tabular-nums"),
    (
        "diagonal-fractions",
        "--tw-numeric-fraction",
        "diagonal-fractions",
    ),
    (
        "stacked-fractions",
        "--tw-numeric-fraction",
        "stacked-fractions",
    ),
];

/// The `outline-style` keywords with a static utility each.
const OUTLINE_STYLES: &[&str] = &["none", "solid", "dashed", "dotted", "double"];
/// The initial value of `--tw-outline-style`.
const OUTLINE_STYLE_INITIAL: &str = "solid";
/// `outline-width` without a value or theme default.
const DEFAULT_OUTLINE_WIDTH: &str = "1px";
/// The `outline-style` that reads the style chosen by `outline-*` styles.
const OUTLINE_STYLE_VAR: &str = "var(--tw-outline-style)";

/// The `@property` syntax of an alpha percentage.
const PERCENTAGE_SYNTAX: &str = "<percentage>";
/// The initial value of a shadow alpha.
const FULL_ALPHA: &str = "100%";
/// A shadow that draws nothing.
const NULL_SHADOW: &str = "0 0 #0000";
/// The `box-shadow` that layers every shadow and ring.
const BOX_SHADOW_VALUE: &str = "var(--tw-inset-shadow), var(--tw-inset-ring-shadow), var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow)";
/// `(variable, initial value, syntax)` of the box shadow `@property` rules,
/// in Tailwind's order (the last four are the legacy ring offset variables).
const BOX_SHADOW_PROPERTIES: &[(&str, Option<&str>, Option<&str>)] = &[
    ("--tw-shadow", Some(NULL_SHADOW), None),
    ("--tw-shadow-color", None, None),
    (
        "--tw-shadow-alpha",
        Some(FULL_ALPHA),
        Some(PERCENTAGE_SYNTAX),
    ),
    ("--tw-inset-shadow", Some(NULL_SHADOW), None),
    ("--tw-inset-shadow-color", None, None),
    (
        "--tw-inset-shadow-alpha",
        Some(FULL_ALPHA),
        Some(PERCENTAGE_SYNTAX),
    ),
    ("--tw-ring-color", None, None),
    ("--tw-ring-shadow", Some(NULL_SHADOW), None),
    ("--tw-inset-ring-color", None, None),
    ("--tw-inset-ring-shadow", Some(NULL_SHADOW), None),
    ("--tw-ring-inset", None, None),
    ("--tw-ring-offset-width", Some("0px"), Some("<length>")),
    ("--tw-ring-offset-color", Some("#fff"), None),
    ("--tw-ring-offset-shadow", Some(NULL_SHADOW), None),
];
/// `(variable, initial value, syntax)` of the text shadow `@property` rules.
const TEXT_SHADOW_PROPERTIES: &[(&str, Option<&str>, Option<&str>)] = &[
    ("--tw-text-shadow-color", None, None),
    (
        "--tw-text-shadow-alpha",
        Some(FULL_ALPHA),
        Some(PERCENTAGE_SYNTAX),
    ),
];
/// The `@supports` rule guarding relative color syntax with dynamic inputs.
const RELATIVE_COLOR_SUPPORTS: &str = "@supports (color: lab(from red l a b))";
/// The keyword of an inset shadow.
const INSET: &str = "inset";

/// The ring color without a `--default-ring-color` theme value.
const DEFAULT_RING_COLOR: &str = "currentcolor";
/// The ring width without a value or `--default-ring-width` theme value.
const DEFAULT_RING_WIDTH: &str = "1px";
/// The inset ring width without a value.
const DEFAULT_INSET_RING_WIDTH: &str = "1px";
/// The legacy ring offset shadow.
const RING_OFFSET_SHADOW_VALUE: &str =
    "var(--tw-ring-inset, ) 0 0 0 var(--tw-ring-offset-width) var(--tw-ring-offset-color)";

/// `container-type` of a bare `@container`.
const DEFAULT_CONTAINER_TYPE: &str = "inline-size";
/// Named `container-type` values of `@container-*`.
const CONTAINER_TYPES: &[&str] = &["normal", "size"];

/// The pixel unit appended to bare integer widths and offsets.
const PX: &str = "px";
/// `font-size` arbitrary data types that select `font-size` over `color`.
const FONT_SIZE_TYPES: &[&str] = &[
    "size",
    "length",
    "percentage",
    "absolute-size",
    "relative-size",
];
/// The line height of the `leading-none` shorthand modifier.
const LEADING_NONE: &str = "1";

pub(super) fn register(r: &mut Registrar<'_>) {
    r.static_utility(
        "forced-color-adjust-none",
        &[("forced-color-adjust", "none")],
    );
    r.static_utility(
        "forced-color-adjust-auto",
        &[("forced-color-adjust", "auto")],
    );

    register_typography(r);
    register_outline(r);

    r.functional_utility(
        "opacity",
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("opacity", value)]))
            .theme_keys(&["--opacity"])
            .bare(|value, _| {
                is_valid_opacity_value(value.value).then(|| format!("{}%", value.value))
            }),
    );

    r.functional_utility(
        "underline-offset",
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("text-underline-offset", value)]))
            .negative()
            .theme_keys(&["--text-underline-offset"])
            .bare(|value, _| px_bare_value(value.value))
            .static_value("auto", vec![Node::decl("text-underline-offset", "auto")]),
    );

    r.utilities
        .functional("text", |c, ds| compile_text(c, &ds.theme));

    r.static_nodes(
        "text-shadow-initial",
        vec![
            properties(TEXT_SHADOW_PROPERTIES),
            Node::decl("--tw-text-shadow-color", "initial"),
        ],
    );
    r.utilities.functional("text-shadow", |c, ds| {
        compile_shadow(&TEXT_SHADOW, c, &ds.theme)
    });

    register_box_shadows(r);

    r.utilities
        .functional("@container", |c, _| compile_container(c));
}

/// `leading`, `tracking`, font smoothing and `font-variant-numeric`.
fn register_typography(r: &mut Registrar<'_>) {
    let leading = |value: &str| {
        vec![
            Node::at_root(vec![property("--tw-leading", None, None)]),
            Node::decl("--tw-leading", value),
            Node::decl("line-height", value),
        ]
    };
    r.spacing_utility(
        "leading",
        &["--leading", "--spacing"],
        move |value| Some(leading(value)),
        SpacingOptions {
            static_values: vec![("none".to_owned(), leading(LEADING_NONE))],
            ..SpacingOptions::default()
        },
    );

    r.functional_utility(
        "tracking",
        FunctionalDesc::new(|value, _, _| {
            Some(vec![
                Node::at_root(vec![property("--tw-tracking", None, None)]),
                Node::decl("--tw-tracking", value),
                Node::decl("letter-spacing", value),
            ])
        })
        .negative()
        .theme_keys(&["--tracking"]),
    );

    r.static_utility(
        "antialiased",
        &[
            ("-webkit-font-smoothing", "antialiased"),
            ("-moz-osx-font-smoothing", "grayscale"),
        ],
    );
    r.static_utility(
        "subpixel-antialiased",
        &[
            ("-webkit-font-smoothing", "auto"),
            ("-moz-osx-font-smoothing", "auto"),
        ],
    );

    r.static_utility("normal-nums", &[("font-variant-numeric", "normal")]);
    for (name, variable, value) in FONT_VARIANT_NUMERIC_UTILITIES {
        let numeric_properties = Node::at_root(
            FONT_VARIANT_NUMERIC_VARIABLES
                .iter()
                .map(|variable| property(variable, None, None))
                .collect(),
        );
        r.static_nodes(
            name,
            vec![
                numeric_properties,
                Node::decl(*variable, *value),
                Node::decl("font-variant-numeric", FONT_VARIANT_NUMERIC_VALUE),
            ],
        );
    }
}

/// The `@property` rule of `--tw-outline-style`.
fn outline_properties() -> Node {
    Node::at_root(vec![property(
        "--tw-outline-style",
        Some(OUTLINE_STYLE_INITIAL),
        None,
    )])
}

/// The nodes of an `outline-width`.
fn outline_width(value: &str) -> Vec<Node> {
    vec![
        outline_properties(),
        Node::decl("outline-style", OUTLINE_STYLE_VAR),
        Node::decl("outline-width", value),
    ]
}

/// `outline-hidden`, the `outline-style` statics, `outline` and
/// `outline-offset`.
fn register_outline(r: &mut Registrar<'_>) {
    r.static_nodes(
        "outline-hidden",
        vec![
            Node::decl("--tw-outline-style", "none"),
            Node::decl("outline-style", "none"),
            Node::at_rule(
                "@media",
                "(forced-colors: active)",
                vec![
                    Node::decl("outline", "2px solid transparent"),
                    Node::decl("outline-offset", "2px"),
                ],
            ),
        ],
    );

    for style in OUTLINE_STYLES {
        r.static_utility(
            &format!("outline-{style}"),
            &[("--tw-outline-style", style), ("outline-style", style)],
        );
    }

    r.utilities
        .functional("outline", |c, ds| compile_outline(c, &ds.theme));

    r.functional_utility(
        "outline-offset",
        FunctionalDesc::new(|value, _, _| Some(vec![Node::decl("outline-offset", value)]))
            .negative()
            .theme_keys(&["--outline-offset"])
            .bare(|value, _| px_bare_value(value.value)),
    );
}

/// `${value}px` for a positive integer bare value.
fn px_bare_value(value: &str) -> Option<String> {
    is_positive_integer(value).then(|| format!("{value}{PX}"))
}

/// The data type of an arbitrary value: the explicit hint, else inferred.
fn arbitrary_type<'a>(
    data_type: Option<&'a str>,
    value: &str,
    types: &[&'a str],
) -> Option<&'a str> {
    data_type.or_else(|| infer_data_type(value, types))
}

fn compile_outline(candidate: &Candidate, theme: &Theme) -> Compiled {
    let modifier = candidate.modifier();
    let Some(value) = candidate.value() else {
        if modifier.is_some() {
            return Compiled::Skip;
        }
        let width = theme
            .get(&["--default-outline-width"])
            .unwrap_or_else(|| DEFAULT_OUTLINE_WIDTH.to_owned());
        return Compiled::Nodes(outline_width(&width));
    };
    let named = match value {
        UtilityValue::Arbitrary { data_type, value } => {
            let kind = arbitrary_type(
                data_type.as_deref(),
                value,
                &["color", "length", "number", "percentage"],
            );
            return match kind {
                Some("length" | "number" | "percentage") => {
                    if modifier.is_some() {
                        return Compiled::Skip;
                    }
                    Compiled::Nodes(outline_width(value))
                }
                _ => as_color(value, modifier, theme).map_or(Compiled::Skip, |color| {
                    Compiled::Nodes(vec![Node::decl("outline-color", color)])
                }),
            };
        }
        UtilityValue::Named { value, .. } => value,
    };

    if let Some(color) = resolve_theme_color(candidate, theme, &["--outline-color", "--color"])
        .filter(|color| !color.is_empty())
    {
        return Compiled::Nodes(vec![Node::decl("outline-color", color)]);
    }

    if modifier.is_some() {
        return Compiled::Skip;
    }
    if let Some(width) = theme
        .resolve(Some(named), &["--outline-width"], options::NONE)
        .filter(|width| !width.is_empty())
    {
        return Compiled::Nodes(outline_width(&width));
    }
    if is_positive_integer(named) {
        return Compiled::Nodes(outline_width(&format!("{named}{PX}")));
    }
    Compiled::Skip
}

/// The `line-height` of a `text-*/<modifier>` candidate; `None` is
/// Tailwind's `null` (an invalid candidate).
fn text_line_height(modifier: &CandidateModifier, theme: &Theme) -> Option<String> {
    let mut line_height = match modifier {
        CandidateModifier::Arbitrary(value) => Some(value.clone()),
        CandidateModifier::Named(value) => {
            theme.resolve(Some(value), &["--leading"], options::NONE)
        }
    }
    .filter(|value| !value.is_empty());
    if line_height.is_none() && is_valid_spacing_multiplier(modifier.value()) {
        theme.resolve(None, &["--spacing"], options::NONE)?;
        line_height = Some(format!("--spacing({})", modifier.value()));
    }
    if line_height.is_none() && modifier.value() == "none" {
        line_height = Some(LEADING_NONE.to_owned());
    }
    line_height
}

fn compile_text(candidate: &Candidate, theme: &Theme) -> Compiled {
    let modifier = candidate.modifier();
    let Some(value) = candidate.value() else {
        return Compiled::Skip;
    };
    let named = match value {
        UtilityValue::Arbitrary { data_type, value } => {
            let kind = arbitrary_type(
                data_type.as_deref(),
                value,
                &[
                    "color",
                    "length",
                    "percentage",
                    "absolute-size",
                    "relative-size",
                ],
            );
            if kind.is_some_and(|kind| FONT_SIZE_TYPES.contains(&kind)) {
                let Some(modifier) = modifier else {
                    return Compiled::Nodes(vec![Node::decl("font-size", value)]);
                };
                return match text_line_height(modifier, theme) {
                    Some(line_height) => Compiled::Nodes(vec![
                        Node::decl("font-size", value),
                        Node::decl("line-height", line_height),
                    ]),
                    None => Compiled::Invalid,
                };
            }
            return as_color(value, modifier, theme).map_or(Compiled::Skip, |color| {
                Compiled::Nodes(vec![Node::decl("color", color)])
            });
        }
        UtilityValue::Named { value, .. } => value,
    };

    if let Some(color) = resolve_theme_color(candidate, theme, &["--text-color", "--color"])
        .filter(|color| !color.is_empty())
    {
        return Compiled::Nodes(vec![Node::decl("color", color)]);
    }

    let Some((font_size, nested)) = theme.resolve_with(
        named,
        &["--text"],
        &["--line-height", "--letter-spacing", "--font-weight"],
    ) else {
        return Compiled::Skip;
    };
    if let Some(modifier) = modifier {
        return match text_line_height(modifier, theme) {
            Some(line_height) => Compiled::Nodes(vec![
                Node::decl("font-size", font_size),
                Node::decl("line-height", line_height),
            ]),
            None => Compiled::Invalid,
        };
    }
    let nested_var = |key: &str, variable: &str| {
        nested
            .get(key)
            .filter(|value| !value.is_empty())
            .map(|value| format!("var({variable}, {value})"))
    };
    Compiled::Nodes(vec![
        Node::decl("font-size", font_size),
        Node::decl_opt("line-height", nested_var("--line-height", "--tw-leading")),
        Node::decl_opt(
            "letter-spacing",
            nested_var("--letter-spacing", "--tw-tracking"),
        ),
        Node::decl_opt(
            "font-weight",
            nested_var("--font-weight", "--tw-font-weight"),
        ),
    ])
}

/// An at-root group of `@property` rules from a table.
fn properties(table: &[(&str, Option<&str>, Option<&str>)]) -> Node {
    Node::at_root(
        table
            .iter()
            .map(|(ident, initial, syntax)| property(ident, *initial, *syntax))
            .collect(),
    )
}

/// One shadow utility family (`text-shadow`, `shadow`, `inset-shadow`): the
/// three share Tailwind's compile function shape.
struct ShadowFamily {
    /// The property the shadow is written to.
    property: &'static str,
    /// The variable carrying the shadow color.
    color_var: &'static str,
    /// The variable carrying the shadow alpha.
    alpha_var: &'static str,
    /// The theme key of the default shadow; sizes are `{key}-{name}`.
    theme_key: &'static str,
    color_theme_keys: &'static [&'static str],
    /// The value of `property` for `{root}-none`.
    none_value: &'static str,
    /// The keyword prefixed to each arbitrary shadow (`inset`), or empty.
    arbitrary_prefix: &'static str,
    /// The `@property` rules of the family.
    properties: &'static [(&'static str, Option<&'static str>, Option<&'static str>)],
    /// Whether the shadow is composed through `box-shadow`.
    box_shadow: bool,
}

const TEXT_SHADOW: ShadowFamily = ShadowFamily {
    property: "text-shadow",
    color_var: "--tw-text-shadow-color",
    alpha_var: "--tw-text-shadow-alpha",
    theme_key: "--text-shadow",
    color_theme_keys: &["--text-shadow-color", "--color"],
    none_value: "none",
    arbitrary_prefix: "",
    properties: TEXT_SHADOW_PROPERTIES,
    box_shadow: false,
};

const BOX_SHADOW: ShadowFamily = ShadowFamily {
    property: "--tw-shadow",
    color_var: "--tw-shadow-color",
    alpha_var: "--tw-shadow-alpha",
    theme_key: "--shadow",
    color_theme_keys: &["--box-shadow-color", "--color"],
    none_value: NULL_SHADOW,
    arbitrary_prefix: "",
    properties: BOX_SHADOW_PROPERTIES,
    box_shadow: true,
};

const INSET_SHADOW: ShadowFamily = ShadowFamily {
    property: "--tw-inset-shadow",
    color_var: "--tw-inset-shadow-color",
    alpha_var: "--tw-inset-shadow-alpha",
    theme_key: "--inset-shadow",
    color_theme_keys: &["--box-shadow-color", "--color"],
    none_value: "inset 0 0 #0000",
    arbitrary_prefix: INSET,
    properties: BOX_SHADOW_PROPERTIES,
    box_shadow: true,
};

impl ShadowFamily {
    /// `var(--tw-…-color, <color>)`.
    fn inject(&self, color: &str) -> String {
        format!("var({}, {color})", self.color_var)
    }

    /// The nodes of a shadow value with an optional alpha.
    fn shadow(&self, value: &str, alpha: Option<&str>, prefix: &str) -> Vec<Node> {
        let mut nodes = vec![
            properties(self.properties),
            Node::decl_opt(self.alpha_var, alpha.map(str::to_owned)),
        ];
        nodes.extend(alpha_replaced_shadow_properties(
            self.property,
            value,
            alpha,
            &|color| self.inject(color),
            prefix,
        ));
        self.push_box_shadow(&mut nodes);
        nodes
    }

    /// The nodes of a shadow color.
    fn color(&self, color: &str) -> Vec<Node> {
        vec![
            properties(self.properties),
            Node::decl(
                self.color_var,
                with_alpha(color, &format!("var({})", self.alpha_var)),
            ),
        ]
    }

    fn push_box_shadow(&self, nodes: &mut Vec<Node>) {
        if self.box_shadow {
            nodes.push(Node::decl("box-shadow", BOX_SHADOW_VALUE));
        }
    }
}

/// The alpha of a shadow modifier: an arbitrary value, or a valid opacity
/// as a percentage.
fn shadow_alpha(modifier: Option<&CandidateModifier>) -> Option<String> {
    match modifier? {
        CandidateModifier::Arbitrary(value) => Some(value.clone()),
        CandidateModifier::Named(value) => {
            is_valid_opacity_value(value).then(|| format!("{value}%"))
        }
    }
    .filter(|alpha| !alpha.is_empty())
}

/// The compile function of a shadow family.
fn compile_shadow(family: &ShadowFamily, candidate: &Candidate, theme: &Theme) -> Compiled {
    let modifier = candidate.modifier();
    let alpha = shadow_alpha(modifier);
    // A modifier that is not a valid alpha rejects every shadow value.
    let rejected = modifier.is_some() && alpha.is_none();

    let Some(value) = candidate.value() else {
        let Some(shadow) = theme.get(&[family.theme_key]) else {
            return Compiled::Skip;
        };
        if rejected {
            return Compiled::Skip;
        }
        return Compiled::Nodes(family.shadow(&shadow, alpha.as_deref(), ""));
    };

    let named = match value {
        UtilityValue::Arbitrary { data_type, value } => {
            if arbitrary_type(data_type.as_deref(), value, &["color"]) == Some("color") {
                return as_color(value, modifier, theme).map_or(Compiled::Skip, |color| {
                    Compiled::Nodes(family.color(&color))
                });
            }
            if rejected {
                return Compiled::Skip;
            }
            return Compiled::Nodes(family.shadow(
                value,
                alpha.as_deref(),
                family.arbitrary_prefix,
            ));
        }
        UtilityValue::Named { value, .. } => value.as_str(),
    };

    match named {
        "none" => {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            let mut nodes = vec![
                properties(family.properties),
                Node::decl(family.property, family.none_value),
            ];
            family.push_box_shadow(&mut nodes);
            return Compiled::Nodes(nodes);
        }
        "inherit" => {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            return Compiled::Nodes(vec![
                properties(family.properties),
                Node::decl(family.color_var, "inherit"),
            ]);
        }
        _ => {}
    }

    if let Some(shadow) = theme
        .get(&[&format!("{}-{named}", family.theme_key)])
        .filter(|shadow| !shadow.is_empty())
    {
        if rejected {
            return Compiled::Skip;
        }
        return Compiled::Nodes(family.shadow(&shadow, alpha.as_deref(), ""));
    }

    resolve_theme_color(candidate, theme, family.color_theme_keys)
        .filter(|color| !color.is_empty())
        .map_or(Compiled::Skip, |color| {
            Compiled::Nodes(family.color(&color))
        })
}

/// Replace the colors of a shadow with the injected variable, applying the
/// alpha with relative color syntax (port of `alphaReplacedShadowProperties`).
fn alpha_replaced_shadow_properties(
    property: &str,
    value: &str,
    alpha: Option<&str>,
    inject: &dyn Fn(&str) -> String,
    prefix: &str,
) -> Vec<Node> {
    let requires_fallback = Cell::new(false);
    let replaced = crate::utils::replace_shadow_colors::replace_shadow_colors(value, &|color| {
        let Some(alpha) = alpha else {
            return inject(color);
        };
        // `currentcolor` keeps the `color-mix(…)` form for browser support.
        if color.starts_with("current") {
            return inject(&with_alpha(color, alpha));
        }
        // Dynamic inputs cannot be resolved statically.
        if color.starts_with("var(") || alpha.starts_with("var(") {
            requires_fallback.set(true);
        }
        inject(&replace_alpha(color, alpha))
    });

    let apply_prefix = |shadows: &str| -> String {
        if prefix.is_empty() {
            return shadows.to_owned();
        }
        segment(shadows, ',')
            .iter()
            .map(|shadow| format!("{} {}", js_trim(prefix), js_trim(shadow)))
            .collect::<Vec<_>>()
            .join(", ")
    };

    if requires_fallback.get() {
        let fallback = crate::utils::replace_shadow_colors::replace_shadow_colors(value, inject);
        vec![
            Node::decl(property, apply_prefix(&fallback)),
            Node::from_prelude(
                RELATIVE_COLOR_SUPPORTS,
                vec![Node::decl(property, apply_prefix(&replaced))],
            ),
        ]
    } else {
        vec![Node::decl(property, apply_prefix(&replaced))]
    }
}

/// `shadow`, `inset-shadow`, `ring`, `inset-ring` and `ring-offset`.
fn register_box_shadows(r: &mut Registrar<'_>) {
    r.static_nodes(
        "shadow-initial",
        vec![
            properties(BOX_SHADOW_PROPERTIES),
            Node::decl("--tw-shadow-color", "initial"),
        ],
    );
    r.utilities
        .functional("shadow", |c, ds| compile_shadow(&BOX_SHADOW, c, &ds.theme));

    r.static_nodes(
        "inset-shadow-initial",
        vec![
            properties(BOX_SHADOW_PROPERTIES),
            Node::decl("--tw-inset-shadow-color", "initial"),
        ],
    );
    r.utilities.functional("inset-shadow", |c, ds| {
        compile_shadow(&INSET_SHADOW, c, &ds.theme)
    });

    r.static_nodes(
        "ring-inset",
        vec![
            properties(BOX_SHADOW_PROPERTIES),
            Node::decl("--tw-ring-inset", INSET),
        ],
    );

    // Read once at creation time, as Tailwind does.
    let ring_color = r
        .theme
        .get(&["--default-ring-color"])
        .unwrap_or_else(|| DEFAULT_RING_COLOR.to_owned());
    let ring = RingFamily {
        shadow_var: "--tw-ring-shadow",
        color_var: "--tw-ring-color",
        default_width_key: Some("--default-ring-width"),
        default_width: DEFAULT_RING_WIDTH,
        shadow: Box::new(move |width| {
            format!(
                "var(--tw-ring-inset, ) 0 0 0 calc({width} + var(--tw-ring-offset-width)) var(--tw-ring-color, {ring_color})"
            )
        }),
    };
    r.utilities
        .functional("ring", move |c, ds| ring.compile(c, &ds.theme));

    let inset_ring = RingFamily {
        shadow_var: "--tw-inset-ring-shadow",
        color_var: "--tw-inset-ring-color",
        default_width_key: None,
        default_width: DEFAULT_INSET_RING_WIDTH,
        shadow: Box::new(|width| {
            format!("inset 0 0 0 {width} var(--tw-inset-ring-color, currentcolor)")
        }),
    };
    r.utilities
        .functional("inset-ring", move |c, ds| inset_ring.compile(c, &ds.theme));

    r.utilities
        .functional("ring-offset", |c, ds| compile_ring_offset(c, &ds.theme));
}

/// One ring utility (`ring`, `inset-ring`).
struct RingFamily {
    shadow_var: &'static str,
    color_var: &'static str,
    /// The theme key of the default width, if the family reads one.
    default_width_key: Option<&'static str>,
    default_width: &'static str,
    /// The ring shadow of a width.
    shadow: Box<dyn Fn(&str) -> String>,
}

impl RingFamily {
    fn width(&self, width: &str) -> Compiled {
        Compiled::Nodes(vec![
            properties(BOX_SHADOW_PROPERTIES),
            Node::decl(self.shadow_var, (self.shadow)(width)),
            Node::decl("box-shadow", BOX_SHADOW_VALUE),
        ])
    }

    fn compile(&self, candidate: &Candidate, theme: &Theme) -> Compiled {
        let modifier = candidate.modifier();
        let Some(value) = candidate.value() else {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            let width = self
                .default_width_key
                .and_then(|key| theme.get(&[key]))
                .unwrap_or_else(|| self.default_width.to_owned());
            return self.width(&width);
        };
        let named = match value {
            UtilityValue::Arbitrary { data_type, value } => {
                return match arbitrary_type(data_type.as_deref(), value, &["color", "length"]) {
                    Some("length") => {
                        if modifier.is_some() {
                            return Compiled::Skip;
                        }
                        self.width(value)
                    }
                    _ => as_color(value, modifier, theme).map_or(Compiled::Skip, |color| {
                        Compiled::Nodes(vec![Node::decl(self.color_var, color)])
                    }),
                };
            }
            UtilityValue::Named { value, .. } => value,
        };

        if let Some(color) = resolve_theme_color(candidate, theme, &["--ring-color", "--color"])
            .filter(|color| !color.is_empty())
        {
            return Compiled::Nodes(vec![Node::decl(self.color_var, color)]);
        }

        if modifier.is_some() {
            return Compiled::Skip;
        }
        let width = theme
            .resolve(Some(named), &["--ring-width"], options::NONE)
            .or_else(|| px_bare_value(named))
            .filter(|width| !width.is_empty());
        width.map_or(Compiled::Skip, |width| self.width(&width))
    }
}

/// The nodes of a legacy ring offset width.
fn ring_offset_width(width: &str) -> Compiled {
    Compiled::Nodes(vec![
        Node::decl("--tw-ring-offset-width", width),
        Node::decl("--tw-ring-offset-shadow", RING_OFFSET_SHADOW_VALUE),
    ])
}

fn compile_ring_offset(candidate: &Candidate, theme: &Theme) -> Compiled {
    let modifier = candidate.modifier();
    let Some(value) = candidate.value() else {
        return Compiled::Skip;
    };
    let named = match value {
        UtilityValue::Arbitrary { data_type, value } => {
            return match arbitrary_type(data_type.as_deref(), value, &["color", "length"]) {
                Some("length") => {
                    if modifier.is_some() {
                        return Compiled::Skip;
                    }
                    ring_offset_width(value)
                }
                _ => as_color(value, modifier, theme).map_or(Compiled::Skip, |color| {
                    Compiled::Nodes(vec![Node::decl("--tw-ring-offset-color", color)])
                }),
            };
        }
        UtilityValue::Named { value, .. } => value,
    };

    if let Some(width) = theme
        .resolve(Some(named), &["--ring-offset-width"], options::NONE)
        .filter(|width| !width.is_empty())
    {
        if modifier.is_some() {
            return Compiled::Skip;
        }
        return ring_offset_width(&width);
    }
    if let Some(width) = px_bare_value(named) {
        if modifier.is_some() {
            return Compiled::Skip;
        }
        return ring_offset_width(&width);
    }

    resolve_theme_color(candidate, theme, &["--ring-offset-color", "--color"])
        .filter(|color| !color.is_empty())
        .map_or(Compiled::Skip, |color| {
            Compiled::Nodes(vec![Node::decl("--tw-ring-offset-color", color)])
        })
}

fn compile_container(candidate: &Candidate) -> Compiled {
    let container_type = match candidate.value() {
        None => Some(DEFAULT_CONTAINER_TYPE),
        Some(UtilityValue::Arbitrary { value, .. }) => Some(value.as_str()),
        Some(UtilityValue::Named { value, .. }) => CONTAINER_TYPES
            .iter()
            .copied()
            .find(|container_type| container_type == value),
    };
    let Some(container_type) = container_type else {
        return Compiled::Skip;
    };
    let mut nodes = vec![Node::decl("container-type", container_type)];
    if let Some(modifier) = candidate.modifier() {
        nodes.push(Node::decl("container-name", modifier.value()));
    }
    Compiled::Nodes(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decls(nodes: &[Node]) -> Vec<(String, Option<String>)> {
        nodes
            .iter()
            .filter_map(|node| match node {
                Node::Decl {
                    property, value, ..
                } => Some((property.clone(), value.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn shadow_alpha_replaces_colors() {
        let inject = |color: &str| format!("var(--tw-shadow-color, {color})");
        let nodes = alpha_replaced_shadow_properties(
            "--tw-shadow",
            "0 1px 3px 0 rgb(0 0 0 / 0.1)",
            Some("50%"),
            &inject,
            "",
        );
        assert_eq!(
            decls(&nodes),
            vec![(
                "--tw-shadow".to_owned(),
                Some(
                    "0 1px 3px 0 var(--tw-shadow-color, oklab(from rgb(0 0 0 / 0.1) l a b / 50%))"
                        .to_owned()
                )
            )]
        );
    }

    #[test]
    fn dynamic_alpha_needs_a_fallback() {
        let inject = |color: &str| format!("var(--tw-shadow-color, {color})");
        let nodes = alpha_replaced_shadow_properties(
            "--tw-inset-shadow",
            "0 0 2px var(--my-color)",
            Some("50%"),
            &inject,
            INSET,
        );
        assert_eq!(nodes.len(), 2);
        assert_eq!(
            decls(&nodes[..1]),
            vec![(
                "--tw-inset-shadow".to_owned(),
                Some("inset 0 0 2px var(--tw-shadow-color, var(--my-color))".to_owned())
            )]
        );
        assert!(matches!(
            &nodes[1],
            Node::AtRule { name, params, .. }
                if name == "@supports" && params == "(color: lab(from red l a b))"
        ));
    }

    #[test]
    fn current_color_keeps_color_mix() {
        let inject = |color: &str| format!("var(--x, {color})");
        let nodes =
            alpha_replaced_shadow_properties("text-shadow", "0 0 1px", Some("25%"), &inject, "");
        assert_eq!(
            decls(&nodes),
            vec![(
                "text-shadow".to_owned(),
                Some(
                    "0 0 1px var(--x, color-mix(in oklab, currentcolor 25%, transparent))"
                        .to_owned()
                )
            )]
        );
    }

    /// The rule of each candidate (without hoisted `@property` rules), as
    /// the default design system prints it; `None` for an invalid candidate.
    fn rules(candidates: &[&str]) -> Vec<Option<String>> {
        let mut ds = crate::stylesheet::default_design_system();
        crate::stylesheet::candidates_to_css(&mut ds, candidates)
            .into_iter()
            .map(|css| css.map(|css| css.split("@property").next().unwrap_or("").to_owned()))
            .collect()
    }

    fn assert_rules(cases: &[(&str, Option<&str>)]) {
        let candidates: Vec<&str> = cases.iter().map(|(candidate, _)| *candidate).collect();
        for ((candidate, expected), actual) in cases.iter().zip(rules(&candidates)) {
            assert_eq!(actual.as_deref(), *expected, "{candidate}");
        }
    }

    #[test]
    fn container_utilities() {
        assert_rules(&[
            (
                "@container/sidebar",
                Some(
                    ".\\@container\\/sidebar {\n  container-type: inline-size;\n  container-name: sidebar;\n}\n",
                ),
            ),
            (
                "@container-size",
                Some(".\\@container-size {\n  container-type: size;\n}\n"),
            ),
            ("@container-bad", None),
        ]);
    }

    #[test]
    fn text_sizes_and_line_heights() {
        assert_rules(&[
            (
                "text-lg/7",
                Some(
                    ".text-lg\\/7 {\n  font-size: var(--text-lg);\n  line-height: calc(var(--spacing) * 7);\n}\n",
                ),
            ),
            (
                "text-[12px]/[1.5]",
                Some(
                    ".text-\\[12px\\]\\/\\[1\\.5\\] {\n  font-size: 12px;\n  line-height: 1.5;\n}\n",
                ),
            ),
            (
                "text-lg/none",
                Some(".text-lg\\/none {\n  font-size: var(--text-lg);\n  line-height: 1;\n}\n"),
            ),
            ("text-sm/foo", None),
        ]);
    }

    #[test]
    fn shadows_with_alpha() {
        assert_rules(&[
            (
                "inset-shadow-[12px_12px_var(--value)]/25",
                Some(
                    ".inset-shadow-\\[12px_12px_var\\(--value\\)\\]\\/25 {\n  --tw-inset-shadow-alpha: 25%;\n  --tw-inset-shadow: inset 12px 12px var(--tw-inset-shadow-color, var(--value));\n  @supports (color: lab(from red l a b)) {\n    --tw-inset-shadow: inset 12px 12px var(--tw-inset-shadow-color, oklab(from var(--value) l a b / 25%));\n  }\n  box-shadow: var(--tw-inset-shadow), var(--tw-inset-ring-shadow), var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow);\n}\n",
                ),
            ),
            (
                "inset-shadow-[10px_10px]/25",
                Some(
                    ".inset-shadow-\\[10px_10px\\]\\/25 {\n  --tw-inset-shadow-alpha: 25%;\n  --tw-inset-shadow: inset 10px 10px var(--tw-inset-shadow-color, color-mix(in oklab, currentcolor 25%, transparent));\n  box-shadow: var(--tw-inset-shadow), var(--tw-inset-ring-shadow), var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow);\n}\n",
                ),
            ),
            ("inset-shadow-sm/foo", None),
            ("shadow-inherit/50", None),
            (
                "text-shadow-none",
                Some(".text-shadow-none {\n  text-shadow: none;\n}\n"),
            ),
        ]);
    }

    #[test]
    fn rings_and_outlines() {
        assert_rules(&[
            (
                "ring",
                Some(
                    ".ring {\n  --tw-ring-shadow: var(--tw-ring-inset, ) 0 0 0 calc(1px + var(--tw-ring-offset-width)) var(--tw-ring-color, currentcolor);\n  box-shadow: var(--tw-inset-shadow), var(--tw-inset-ring-shadow), var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow);\n}\n",
                ),
            ),
            (
                "ring-offset-[3px]",
                Some(
                    ".ring-offset-\\[3px\\] {\n  --tw-ring-offset-width: 3px;\n  --tw-ring-offset-shadow: var(--tw-ring-inset, ) 0 0 0 var(--tw-ring-offset-width) var(--tw-ring-offset-color);\n}\n",
                ),
            ),
            (
                "outline",
                Some(
                    ".outline {\n  outline-style: var(--tw-outline-style);\n  outline-width: 1px;\n}\n",
                ),
            ),
            (
                "-outline-offset-2",
                Some(".-outline-offset-2 {\n  outline-offset: calc(2px * -1);\n}\n"),
            ),
            (
                "leading-6",
                Some(
                    ".leading-6 {\n  --tw-leading: calc(var(--spacing) * 6);\n  line-height: calc(var(--spacing) * 6);\n}\n",
                ),
            ),
        ]);
    }

    #[test]
    fn shadow_modifiers_must_be_opacities() {
        assert_eq!(
            shadow_alpha(Some(&CandidateModifier::Named("50".to_owned()))),
            Some("50%".to_owned())
        );
        assert_eq!(
            shadow_alpha(Some(&CandidateModifier::Named("foo".to_owned()))),
            None
        );
        assert_eq!(
            shadow_alpha(Some(&CandidateModifier::Arbitrary("0.5".to_owned()))),
            Some("0.5".to_owned())
        );
    }
}
