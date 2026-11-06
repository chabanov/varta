//! Built-in utilities, first part: a port of Tailwind CSS v4.3.5
//! `utilities.ts` lines 592–2314 (Tailwind Labs, MIT; see LICENSE-TAILWIND),
//! the body of `createUtilities` from `sr-only` up to, excluding,
//! `scrollbar-gutter-auto`: layout, positioning, sizing, flex and grid,
//! transforms, interactivity, scroll snapping, alignment, gaps and spacing.
//!
//! Registration order is Tailwind's (it decides which utility wins and the
//! output order). Suggestions are not ported.
use super::{
    Compiled, DefaultValue, FunctionalDesc, NamedValue, Registrar, SpacingOptions, property,
};
use crate::ast::Node;
use crate::candidate::{Candidate, UtilityValue};
use crate::design_system::DesignSystem;
use crate::theme::{Theme, options};
use crate::utils::compare_breakpoints::{Direction, compare_breakpoints};
use crate::utils::dimensions::parse_dimension;
use crate::utils::infer_data_type::{
    infer_data_type, is_length, is_positive_integer, is_strict_positive_integer,
    is_valid_spacing_multiplier,
};
use crate::utils::segment;

/// A static utility of one declaration: `(class, property, value)`.
type StaticDecl = (&'static str, &'static str, &'static str);

/// The selector of the children spaced or divided by `space-*`/`divide-*`.
const CHILDREN_SELECTOR: &str = ":where(& > :not(:last-child))";

/* ------------------------------- helpers -------------------------------- */

/// Declarations from `(property, value)` pairs.
fn decls(list: &[(&str, &str)]) -> Vec<Node> {
    list.iter()
        .map(|(property, value)| Node::decl(*property, *value))
        .collect()
}

/// Register a run of one-declaration static utilities in order.
fn static_decls(r: &mut Registrar<'_>, table: &[StaticDecl]) {
    for &(name, property, value) in table {
        r.static_utility(name, &[(property, value)]);
    }
}

/// A handler producing `property: value`.
fn single(property: &'static str) -> impl Fn(&str, Option<&str>, &Theme) -> Option<Vec<Node>> {
    move |value, _, _| Some(vec![Node::decl(property, value)])
}

/// A spacing handler producing `property: value`.
fn spacing_decl(property: &'static str) -> impl Fn(&str) -> Option<Vec<Node>> {
    move |value| Some(vec![Node::decl(property, value)])
}

/// Bare values that are positive integers, unchanged.
fn bare_integer(value: NamedValue<'_>, _: &Theme) -> Option<String> {
    is_positive_integer(value.value).then(|| value.value.to_owned())
}

/// Bare positive integers with a unit suffix (`45` → `45deg`).
fn bare_integer_with(unit: &'static str) -> impl Fn(NamedValue<'_>, &Theme) -> Option<String> {
    move |value, _| is_positive_integer(value.value).then(|| format!("{}{unit}", value.value))
}

/// Bare spacing multipliers as `--spacing(n)` (needs a `--spacing` theme value).
fn bare_spacing(value: NamedValue<'_>, theme: &Theme) -> Option<String> {
    theme.resolve(None, &["--spacing"], options::NONE)?;
    if !is_valid_spacing_multiplier(value.value) {
        return None;
    }
    Some(format!("--spacing({})", value.value))
}

/// Bare strictly positive integers as `repeat(n, minmax(0, 1fr))`.
fn bare_grid_template(value: NamedValue<'_>, _: &Theme) -> Option<String> {
    is_strict_positive_integer(value.value)
        .then(|| format!("repeat({}, minmax(0, 1fr))", value.value))
}

/// A theme value that JavaScript treats as truthy (`!value` is false).
fn truthy(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

/// Wrap a value in `calc(… * -1)` for a negative candidate.
fn negate(value: String, negative: bool) -> String {
    if negative {
        format!("calc({value} * -1)")
    } else {
        value
    }
}

/// A functional utility of one property with positive-integer bare values,
/// optional negatives and named static values.
fn integer_utility(
    r: &mut Registrar<'_>,
    root: &str,
    negative: bool,
    theme_keys: &[&str],
    property: &'static str,
    static_values: &[(&str, &str)],
) {
    let mut desc = FunctionalDesc::new(single(property))
        .theme_keys(theme_keys)
        .bare(bare_integer);
    desc.supports_negative = negative;
    for (name, value) in static_values {
        desc = desc.static_value(name, vec![Node::decl(property, *value)]);
    }
    r.functional_utility(root, desc);
}

/// A functional utility of one property with named static values only.
fn keyword_utility(
    r: &mut Registrar<'_>,
    root: &str,
    theme_keys: &[&str],
    property: &'static str,
    static_values: &[(&str, &str)],
) {
    let mut desc = FunctionalDesc::new(single(property)).theme_keys(theme_keys);
    for (name, value) in static_values {
        desc = desc.static_value(name, vec![Node::decl(property, *value)]);
    }
    r.functional_utility(root, desc);
}

/// Spacing options with negatives and/or fractions.
fn spacing_options(supports_negative: bool, supports_fractions: bool) -> SpacingOptions {
    SpacingOptions {
        supports_negative,
        supports_fractions,
        static_values: Vec::new(),
    }
}

/* ------------------------------ registration ---------------------------- */

pub(super) fn register(r: &mut Registrar<'_>) {
    register_layout(r);
    register_grid_placement(r);
    register_box(r);
    register_sizing(r);
    register_flex_and_table(r);
    register_transforms(r);
    register_interactivity(r);
    register_scroll_and_lists(r);
    register_grid_and_alignment(r);
    register_gaps_and_spacing(r);
    register_self_and_overflow(r);
}

const SR_ONLY: &[(&str, &str)] = &[
    ("position", "absolute"),
    ("width", "1px"),
    ("height", "1px"),
    ("padding", "0"),
    ("margin", "-1px"),
    ("overflow", "hidden"),
    ("clip-path", "inset(50%)"),
    ("white-space", "nowrap"),
    ("border-width", "0"),
];

const NOT_SR_ONLY: &[(&str, &str)] = &[
    ("position", "static"),
    ("width", "auto"),
    ("height", "auto"),
    ("padding", "0"),
    ("margin", "0"),
    ("overflow", "visible"),
    ("clip-path", "none"),
    ("white-space", "normal"),
];

/// `pointer-events`, `visibility` and `position`.
const POSITIONING: &[StaticDecl] = &[
    ("pointer-events-none", "pointer-events", "none"),
    ("pointer-events-auto", "pointer-events", "auto"),
    ("visible", "visibility", "visible"),
    ("invisible", "visibility", "hidden"),
    ("collapse", "visibility", "collapse"),
    ("static", "position", "static"),
    ("fixed", "position", "fixed"),
    ("absolute", "position", "absolute"),
    ("relative", "position", "relative"),
    ("sticky", "position", "sticky"),
];

/// `inset` utilities: `(root, property)`.
const INSETS: &[(&str, &str)] = &[
    ("inset", "inset"),
    ("inset-x", "inset-inline"),
    ("inset-y", "inset-block"),
    ("inset-s", "inset-inline-start"),
    ("inset-e", "inset-inline-end"),
    ("inset-bs", "inset-block-start"),
    ("inset-be", "inset-block-end"),
    ("top", "top"),
    ("right", "right"),
    ("bottom", "bottom"),
    ("left", "left"),
];

const ISOLATION: &[StaticDecl] = &[
    ("isolate", "isolation", "isolate"),
    ("isolation-auto", "isolation", "auto"),
];

fn register_layout(r: &mut Registrar<'_>) {
    r.static_utility("sr-only", SR_ONLY);
    r.static_utility("not-sr-only", NOT_SR_ONLY);
    static_decls(r, POSITIONING);

    for &(name, property) in INSETS {
        r.static_utility(&format!("{name}-auto"), &[(property, "auto")]);
        r.static_utility(&format!("{name}-full"), &[(property, "100%")]);
        r.static_utility(&format!("-{name}-full"), &[(property, "-100%")]);
        r.spacing_utility(
            name,
            &["--inset", "--spacing"],
            spacing_decl(property),
            spacing_options(true, true),
        );
    }

    static_decls(r, ISOLATION);
    integer_utility(r, "z", true, &["--z-index"], "z-index", &[("auto", "auto")]);
    integer_utility(
        r,
        "order",
        true,
        &["--order"],
        "order",
        &[("first", "-9999"), ("last", "9999")],
    );
}

/// A `span n / span n` utility (`col-span`, `row-span`).
fn span_utility(r: &mut Registrar<'_>, root: &str, property: &'static str) {
    r.functional_utility(
        root,
        FunctionalDesc::new(move |value, _, _| {
            Some(vec![Node::decl(
                property,
                format!("span {value} / span {value}"),
            )])
        })
        .bare(bare_integer)
        .static_value("full", vec![Node::decl(property, "1 / -1")]),
    );
}

const FLOAT_AND_CLEAR: &[StaticDecl] = &[
    ("float-start", "float", "inline-start"),
    ("float-end", "float", "inline-end"),
    ("float-right", "float", "right"),
    ("float-left", "float", "left"),
    ("float-none", "float", "none"),
    ("clear-start", "clear", "inline-start"),
    ("clear-end", "clear", "inline-end"),
    ("clear-right", "clear", "right"),
    ("clear-left", "clear", "left"),
    ("clear-both", "clear", "both"),
    ("clear-none", "clear", "none"),
];

const AUTO: &[(&str, &str)] = &[("auto", "auto")];

fn register_grid_placement(r: &mut Registrar<'_>) {
    integer_utility(r, "col", true, &["--grid-column"], "grid-column", AUTO);
    span_utility(r, "col-span", "grid-column");
    integer_utility(
        r,
        "col-start",
        true,
        &["--grid-column-start"],
        "grid-column-start",
        AUTO,
    );
    integer_utility(
        r,
        "col-end",
        true,
        &["--grid-column-end"],
        "grid-column-end",
        AUTO,
    );
    integer_utility(r, "row", true, &["--grid-row"], "grid-row", AUTO);
    span_utility(r, "row-span", "grid-row");
    integer_utility(
        r,
        "row-start",
        true,
        &["--grid-row-start"],
        "grid-row-start",
        AUTO,
    );
    integer_utility(
        r,
        "row-end",
        true,
        &["--grid-row-end"],
        "grid-row-end",
        AUTO,
    );
    static_decls(r, FLOAT_AND_CLEAR);
}

/// `margin` utilities: `(root, property)`.
const MARGINS: &[(&str, &str)] = &[
    ("m", "margin"),
    ("mx", "margin-inline"),
    ("my", "margin-block"),
    ("ms", "margin-inline-start"),
    ("me", "margin-inline-end"),
    ("mbs", "margin-block-start"),
    ("mbe", "margin-block-end"),
    ("mt", "margin-top"),
    ("mr", "margin-right"),
    ("mb", "margin-bottom"),
    ("ml", "margin-left"),
];

const BOX_SIZING: &[StaticDecl] = &[
    ("box-border", "box-sizing", "border-box"),
    ("box-content", "box-sizing", "content-box"),
];

const LINE_CLAMP: &[(&str, &str)] = &[
    ("overflow", "hidden"),
    ("display", "-webkit-box"),
    ("-webkit-box-orient", "vertical"),
];

const LINE_CLAMP_NONE: &[(&str, &str)] = &[
    ("overflow", "visible"),
    ("display", "block"),
    ("-webkit-box-orient", "horizontal"),
    ("-webkit-line-clamp", "unset"),
];

/// `display` and `field-sizing`.
const DISPLAY: &[StaticDecl] = &[
    ("block", "display", "block"),
    ("inline-block", "display", "inline-block"),
    ("inline", "display", "inline"),
    ("hidden", "display", "none"),
    ("inline-flex", "display", "inline-flex"),
    ("table", "display", "table"),
    ("inline-table", "display", "inline-table"),
    ("table-caption", "display", "table-caption"),
    ("table-cell", "display", "table-cell"),
    ("table-column", "display", "table-column"),
    ("table-column-group", "display", "table-column-group"),
    ("table-footer-group", "display", "table-footer-group"),
    ("table-header-group", "display", "table-header-group"),
    ("table-row-group", "display", "table-row-group"),
    ("table-row", "display", "table-row"),
    ("flow-root", "display", "flow-root"),
    ("flex", "display", "flex"),
    ("grid", "display", "grid"),
    ("inline-grid", "display", "inline-grid"),
    ("contents", "display", "contents"),
    ("list-item", "display", "list-item"),
    ("field-sizing-content", "field-sizing", "content"),
    ("field-sizing-fixed", "field-sizing", "fixed"),
];

fn register_box(r: &mut Registrar<'_>) {
    for &(name, property) in MARGINS {
        r.static_utility(&format!("{name}-auto"), &[(property, "auto")]);
        r.spacing_utility(
            name,
            &["--margin", "--spacing"],
            spacing_decl(property),
            spacing_options(true, false),
        );
    }
    static_decls(r, BOX_SIZING);

    r.functional_utility(
        "line-clamp",
        FunctionalDesc::new(|value, _, _| {
            let mut nodes = decls(LINE_CLAMP);
            nodes.push(Node::decl("-webkit-line-clamp", value));
            Some(nodes)
        })
        .theme_keys(&["--line-clamp"])
        .bare(bare_integer)
        .static_value("none", decls(LINE_CLAMP_NONE)),
    );

    static_decls(r, DISPLAY);

    r.functional_utility(
        "aspect",
        FunctionalDesc::new(single("aspect-ratio"))
            .theme_keys(&["--aspect"])
            .bare(|value, _| {
                let fraction = value.fraction?;
                let parts = segment(fraction, '/');
                let lhs = parts.first().map_or("", String::as_str);
                let rhs = parts.get(1).map_or("", String::as_str);
                if !is_valid_spacing_multiplier(lhs) || !is_valid_spacing_multiplier(rhs) {
                    return None;
                }
                Some(fraction.to_owned())
            })
            .static_value("auto", vec![Node::decl("aspect-ratio", "auto")])
            .static_value("square", vec![Node::decl("aspect-ratio", "1 / 1")]),
    );
}

/// Keyword sizes of `size`/`w`/`h` and their min/max forms.
const SIZE_KEYWORDS: &[(&str, &str)] = &[
    ("full", "100%"),
    ("svw", "100svw"),
    ("lvw", "100lvw"),
    ("dvw", "100dvw"),
    ("svh", "100svh"),
    ("lvh", "100lvh"),
    ("dvh", "100dvh"),
    ("min", "min-content"),
    ("max", "max-content"),
    ("fit", "fit-content"),
];

/// The physical sizing utilities of each keyword: `(prefix, property)`.
const PHYSICAL_SIZES: &[(&str, &str)] = &[
    ("w", "width"),
    ("h", "height"),
    ("min-w", "min-width"),
    ("min-h", "min-height"),
    ("max-w", "max-width"),
    ("max-h", "max-height"),
];

const PHYSICAL_SIZE_STATICS: &[StaticDecl] = &[
    ("w-auto", "width", "auto"),
    ("h-auto", "height", "auto"),
    ("min-w-auto", "min-width", "auto"),
    ("min-h-auto", "min-height", "auto"),
    ("h-lh", "height", "1lh"),
    ("min-h-lh", "min-height", "1lh"),
    ("max-h-lh", "max-height", "1lh"),
    ("w-screen", "width", "100vw"),
    ("min-w-screen", "min-width", "100vw"),
    ("max-w-screen", "max-width", "100vw"),
    ("h-screen", "height", "100vh"),
    ("min-h-screen", "min-height", "100vh"),
    ("max-h-screen", "max-height", "100vh"),
    ("max-w-none", "max-width", "none"),
    ("max-h-none", "max-height", "none"),
];

/// Spacing sizes: `(root, theme keys, property)`.
type SpacingSize = (&'static str, &'static [&'static str], &'static str);

const PHYSICAL_SPACING_SIZES: &[SpacingSize] = &[
    ("w", &["--width", "--spacing", "--container"], "width"),
    (
        "min-w",
        &["--min-width", "--spacing", "--container"],
        "min-width",
    ),
    (
        "max-w",
        &["--max-width", "--spacing", "--container"],
        "max-width",
    ),
    ("h", &["--height", "--spacing"], "height"),
    (
        "min-h",
        &["--min-height", "--height", "--spacing"],
        "min-height",
    ),
    (
        "max-h",
        &["--max-height", "--height", "--spacing"],
        "max-height",
    ),
];

/// Keyword sizes shared by inline and block sizes.
const LOGICAL_KEYWORDS: &[(&str, &str)] = &[
    ("full", "100%"),
    ("min", "min-content"),
    ("max", "max-content"),
    ("fit", "fit-content"),
];

/// The logical sizing utilities of each shared keyword, in Tailwind's order.
const LOGICAL_SIZES: &[(&str, &str)] = &[
    ("inline", "inline-size"),
    ("block", "block-size"),
    ("min-inline", "min-inline-size"),
    ("min-block", "min-block-size"),
    ("max-inline", "max-inline-size"),
    ("max-block", "max-block-size"),
];

const INLINE_VIEWPORT: &[(&str, &str)] = &[("svw", "100svw"), ("lvw", "100lvw"), ("dvw", "100dvw")];
const BLOCK_VIEWPORT: &[(&str, &str)] = &[("svh", "100svh"), ("lvh", "100lvh"), ("dvh", "100dvh")];

const INLINE_SIZES: &[(&str, &str)] = &[
    ("inline", "inline-size"),
    ("min-inline", "min-inline-size"),
    ("max-inline", "max-inline-size"),
];

const BLOCK_SIZES: &[(&str, &str)] = &[
    ("block", "block-size"),
    ("min-block", "min-block-size"),
    ("max-block", "max-block-size"),
];

const LOGICAL_SIZE_STATICS: &[StaticDecl] = &[
    ("inline-auto", "inline-size", "auto"),
    ("block-auto", "block-size", "auto"),
    ("min-inline-auto", "min-inline-size", "auto"),
    ("min-block-auto", "min-block-size", "auto"),
    ("block-lh", "block-size", "1lh"),
    ("min-block-lh", "min-block-size", "1lh"),
    ("max-block-lh", "max-block-size", "1lh"),
    ("inline-screen", "inline-size", "100vw"),
    ("min-inline-screen", "min-inline-size", "100vw"),
    ("max-inline-screen", "max-inline-size", "100vw"),
    ("block-screen", "block-size", "100vh"),
    ("min-block-screen", "min-block-size", "100vh"),
    ("max-block-screen", "max-block-size", "100vh"),
    ("max-inline-none", "max-inline-size", "none"),
    ("max-block-none", "max-block-size", "none"),
];

const LOGICAL_SPACING_SIZES: &[SpacingSize] = &[
    ("inline", &["--spacing", "--container"], "inline-size"),
    (
        "min-inline",
        &["--spacing", "--container"],
        "min-inline-size",
    ),
    (
        "max-inline",
        &["--spacing", "--container"],
        "max-inline-size",
    ),
    ("block", &["--spacing"], "block-size"),
    ("min-block", &["--spacing"], "min-block-size"),
    ("max-block", &["--spacing"], "max-block-size"),
];

/// The `size-*` declarations of `value`.
fn size_decls(value: &str) -> Vec<Node> {
    vec![
        Node::decl("--tw-sort", "size"),
        Node::decl("width", value),
        Node::decl("height", value),
    ]
}

fn register_spacing_sizes(r: &mut Registrar<'_>, table: &[SpacingSize]) {
    for &(name, namespaces, property) in table {
        r.spacing_utility(
            name,
            namespaces,
            spacing_decl(property),
            spacing_options(false, true),
        );
    }
}

fn register_sizing(r: &mut Registrar<'_>) {
    for &(key, value) in SIZE_KEYWORDS {
        r.static_nodes(&format!("size-{key}"), size_decls(value));
        for &(prefix, property) in PHYSICAL_SIZES {
            r.static_utility(&format!("{prefix}-{key}"), &[(property, value)]);
        }
    }
    r.static_nodes("size-auto", size_decls("auto"));
    static_decls(r, PHYSICAL_SIZE_STATICS);
    r.spacing_utility(
        "size",
        &["--size", "--spacing"],
        |value| Some(size_decls(value)),
        spacing_options(false, true),
    );
    register_spacing_sizes(r, PHYSICAL_SPACING_SIZES);

    for &(key, value) in LOGICAL_KEYWORDS {
        for &(prefix, property) in LOGICAL_SIZES {
            r.static_utility(&format!("{prefix}-{key}"), &[(property, value)]);
        }
    }
    for (viewport, sizes) in [
        (INLINE_VIEWPORT, INLINE_SIZES),
        (BLOCK_VIEWPORT, BLOCK_SIZES),
    ] {
        for &(key, value) in viewport {
            for &(prefix, property) in sizes {
                r.static_utility(&format!("{prefix}-{key}"), &[(property, value)]);
            }
        }
    }
    static_decls(r, LOGICAL_SIZE_STATICS);
    register_spacing_sizes(r, LOGICAL_SPACING_SIZES);

    r.utilities.static_fn("container", |_, ds| {
        let mut breakpoints: Vec<String> = ds
            .theme
            .namespace("--breakpoint")
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        breakpoints.sort_by(|a, z| compare_breakpoints(a, z, Direction::Asc));
        let mut nodes = vec![
            Node::decl("--tw-sort", "--tw-container-component"),
            Node::decl("width", "100%"),
        ];
        for breakpoint in breakpoints {
            nodes.push(Node::at_rule(
                "@media",
                format!("(width >= {breakpoint})"),
                vec![Node::decl("max-width", breakpoint)],
            ));
        }
        Compiled::Nodes(nodes)
    });
}

const FLEX_KEYWORDS: &[StaticDecl] = &[
    ("flex-auto", "flex", "auto"),
    ("flex-initial", "flex", "0 auto"),
    ("flex-none", "flex", "none"),
];

/// Whether both halves of a fraction are positive integers.
fn is_integer_fraction(fraction: &str) -> bool {
    let parts = segment(fraction, '/');
    let lhs = parts.first().map_or("", String::as_str);
    let rhs = parts.get(1).map_or("", String::as_str);
    is_positive_integer(lhs) && is_positive_integer(rhs)
}

/// `flex-*`: `display: flex` is static, the values set `flex`.
fn compile_flex(candidate: &Candidate, _: &DesignSystem) -> Compiled {
    let flex = |value: &str| Compiled::Nodes(vec![Node::decl("flex", value)]);
    match candidate.value() {
        None => Compiled::Skip,
        Some(UtilityValue::Arbitrary { value, .. }) => {
            if candidate.modifier().is_some() {
                Compiled::Skip
            } else {
                flex(value)
            }
        }
        Some(UtilityValue::Named {
            fraction: Some(fraction),
            ..
        }) => {
            if is_integer_fraction(fraction) {
                flex(&format!("calc({fraction} * 100%)"))
            } else {
                Compiled::Skip
            }
        }
        Some(UtilityValue::Named { value, .. }) => {
            if is_positive_integer(value) && candidate.modifier().is_none() {
                flex(value)
            } else {
                Compiled::Skip
            }
        }
    }
}

/// `shrink`/`grow`: `(root, property)`, both defaulting to `1`.
const FLEX_FACTORS: &[(&str, &str)] = &[("shrink", "flex-shrink"), ("grow", "flex-grow")];

const TABLE_KEYWORDS: &[StaticDecl] = &[
    ("table-auto", "table-layout", "auto"),
    ("table-fixed", "table-layout", "fixed"),
    ("caption-top", "caption-side", "top"),
    ("caption-bottom", "caption-side", "bottom"),
    ("border-collapse", "border-collapse", "collapse"),
    ("border-separate", "border-collapse", "separate"),
];

const BORDER_SPACING_VALUE: &str = "var(--tw-border-spacing-x) var(--tw-border-spacing-y)";

fn border_spacing_properties() -> Node {
    Node::AtRoot(vec![
        property("--tw-border-spacing-x", Some("0"), Some("<length>")),
        property("--tw-border-spacing-y", Some("0"), Some("<length>")),
    ])
}

/// `border-spacing` utilities: `(root, the axis variables set)`.
const BORDER_SPACINGS: &[(&str, &[&str])] = &[
    (
        "border-spacing",
        &["--tw-border-spacing-x", "--tw-border-spacing-y"],
    ),
    ("border-spacing-x", &["--tw-border-spacing-x"]),
    ("border-spacing-y", &["--tw-border-spacing-y"]),
];

/// Named positions of `origin` and `perspective-origin`.
const ORIGINS: &[(&str, &str)] = &[
    ("center", "center"),
    ("top", "top"),
    ("top-right", "100% 0"),
    ("right", "100%"),
    ("bottom-right", "100% 100%"),
    ("bottom", "bottom"),
    ("bottom-left", "0 100%"),
    ("left", "0"),
    ("top-left", "0 0"),
];

fn register_flex_and_table(r: &mut Registrar<'_>) {
    static_decls(r, FLEX_KEYWORDS);
    r.utilities.functional("flex", compile_flex);

    for &(root, property) in FLEX_FACTORS {
        r.functional_utility(
            root,
            FunctionalDesc::new(single(property))
                .default_value(DefaultValue::Value("1".to_owned()))
                .bare(bare_integer),
        );
    }

    r.static_utility("basis-auto", &[("flex-basis", "auto")]);
    r.static_utility("basis-full", &[("flex-basis", "100%")]);
    r.spacing_utility(
        "basis",
        &["--flex-basis", "--spacing", "--container"],
        spacing_decl("flex-basis"),
        spacing_options(false, true),
    );

    static_decls(r, TABLE_KEYWORDS);
    for &(name, variables) in BORDER_SPACINGS {
        r.spacing_utility(
            name,
            &["--border-spacing", "--spacing"],
            move |value| {
                let mut nodes = vec![border_spacing_properties()];
                nodes.extend(variables.iter().map(|v| Node::decl(*v, value)));
                nodes.push(Node::decl("border-spacing", BORDER_SPACING_VALUE));
                Some(nodes)
            },
            SpacingOptions::default(),
        );
    }

    keyword_utility(
        r,
        "origin",
        &["--transform-origin"],
        "transform-origin",
        ORIGINS,
    );
    keyword_utility(
        r,
        "perspective-origin",
        &["--perspective-origin"],
        "perspective-origin",
        ORIGINS,
    );
    keyword_utility(
        r,
        "perspective",
        &["--perspective"],
        "perspective",
        &[("none", "none")],
    );
}

/* ------------------------------ transforms ------------------------------ */

const TRANSLATE_2D: &str = "var(--tw-translate-x) var(--tw-translate-y)";
const TRANSLATE_3D: &str = "var(--tw-translate-x) var(--tw-translate-y) var(--tw-translate-z)";

fn translate_properties() -> Node {
    Node::AtRoot(vec![
        property("--tw-translate-x", Some("0"), None),
        property("--tw-translate-y", Some("0"), None),
        property("--tw-translate-z", Some("0"), None),
    ])
}

/// Translate declarations: the properties, each variable set to `value`,
/// then `translate`.
fn translate_nodes(variables: &[&str], value: &str, translate: &str) -> Vec<Node> {
    let mut nodes = vec![translate_properties()];
    nodes.extend(variables.iter().map(|v| Node::decl(*v, value)));
    nodes.push(Node::decl("translate", translate));
    nodes
}

const TRANSLATE_XY: &[&str] = &["--tw-translate-x", "--tw-translate-y"];

/// Axis translations: `(axis, variable)`.
const TRANSLATE_AXES: &[(&str, &str)] = &[("x", "--tw-translate-x"), ("y", "--tw-translate-y")];

fn register_translate(r: &mut Registrar<'_>) {
    r.static_utility("translate-none", &[("translate", "none")]);
    r.static_nodes(
        "-translate-full",
        translate_nodes(TRANSLATE_XY, "-100%", TRANSLATE_2D),
    );
    r.static_nodes(
        "translate-full",
        translate_nodes(TRANSLATE_XY, "100%", TRANSLATE_2D),
    );
    r.spacing_utility(
        "translate",
        &["--translate", "--spacing"],
        |value| Some(translate_nodes(TRANSLATE_XY, value, TRANSLATE_2D)),
        spacing_options(true, true),
    );
    for &(axis, variable) in TRANSLATE_AXES {
        r.static_nodes(
            &format!("-translate-{axis}-full"),
            translate_nodes(&[variable], "-100%", TRANSLATE_2D),
        );
        r.static_nodes(
            &format!("translate-{axis}-full"),
            translate_nodes(&[variable], "100%", TRANSLATE_2D),
        );
        r.spacing_utility(
            &format!("translate-{axis}"),
            &["--translate", "--spacing"],
            move |value| Some(translate_nodes(&[variable], value, TRANSLATE_2D)),
            spacing_options(true, true),
        );
    }
    r.spacing_utility(
        "translate-z",
        &["--translate", "--spacing"],
        |value| Some(translate_nodes(&["--tw-translate-z"], value, TRANSLATE_3D)),
        spacing_options(true, false),
    );
    r.static_nodes("translate-3d", translate_nodes(&[], "", TRANSLATE_3D));
}

fn scale_properties() -> Node {
    Node::AtRoot(vec![
        property("--tw-scale-x", Some("1"), None),
        property("--tw-scale-y", Some("1"), None),
        property("--tw-scale-z", Some("1"), None),
    ])
}

const SCALE_2D: &str = "var(--tw-scale-x) var(--tw-scale-y)";
const SCALE_3D: &str = "var(--tw-scale-x) var(--tw-scale-y) var(--tw-scale-z)";

/// `scale-*` (port of `handleScale`).
fn compile_scale(candidate: &Candidate, ds: &DesignSystem, negative: bool) -> Compiled {
    let Some(value) = candidate.value() else {
        return Compiled::Skip;
    };
    if candidate.modifier().is_some() {
        return Compiled::Skip;
    }
    let value = match value {
        UtilityValue::Arbitrary { value, .. } => {
            return Compiled::Nodes(vec![Node::decl("scale", negate(value.clone(), negative))]);
        }
        UtilityValue::Named { value, .. } => {
            let resolved = truthy(ds.theme.resolve(Some(value), &["--scale"], options::NONE));
            match resolved {
                Some(resolved) => resolved,
                None if is_positive_integer(value) => format!("{value}%"),
                None => return Compiled::Skip,
            }
        }
    };
    let value = negate(value, negative);
    Compiled::Nodes(vec![
        scale_properties(),
        Node::decl("--tw-scale-x", value.as_str()),
        Node::decl("--tw-scale-y", value.as_str()),
        Node::decl("--tw-scale-z", value.as_str()),
        Node::decl("scale", SCALE_2D),
    ])
}

/// Axis scales: `(axis, variable, scale value)`.
const SCALE_AXES: &[(&str, &str, &str)] = &[
    ("x", "--tw-scale-x", SCALE_2D),
    ("y", "--tw-scale-y", SCALE_2D),
    ("z", "--tw-scale-z", SCALE_3D),
];

fn register_scale(r: &mut Registrar<'_>) {
    r.static_utility("scale-none", &[("scale", "none")]);
    r.utilities
        .functional("-scale", |c, ds| compile_scale(c, ds, true));
    r.utilities
        .functional("scale", |c, ds| compile_scale(c, ds, false));
    for &(axis, variable, scale) in SCALE_AXES {
        r.functional_utility(
            &format!("scale-{axis}"),
            FunctionalDesc::new(move |value, _, _| {
                Some(vec![
                    scale_properties(),
                    Node::decl(variable, value),
                    Node::decl("scale", scale),
                ])
            })
            .negative()
            .theme_keys(&["--scale"])
            .bare(bare_integer_with("%")),
        );
    }
    r.static_nodes(
        "scale-3d",
        vec![scale_properties(), Node::decl("scale", SCALE_3D)],
    );
}

/// `rotate-*` (port of `handleRotate`).
fn compile_rotate(candidate: &Candidate, ds: &DesignSystem, negative: bool) -> Compiled {
    let Some(value) = candidate.value() else {
        return Compiled::Skip;
    };
    if candidate.modifier().is_some() {
        return Compiled::Skip;
    }
    let value = match value {
        UtilityValue::Arbitrary { value, data_type } => {
            let data_type = data_type
                .as_deref()
                .or_else(|| infer_data_type(value, &["angle", "vector"]));
            if data_type == Some("vector") {
                return Compiled::Nodes(vec![Node::decl(
                    "rotate",
                    format!("{value} var(--tw-rotate)"),
                )]);
            }
            value.clone()
        }
        UtilityValue::Named { value, .. } => {
            let resolved = truthy(ds.theme.resolve(Some(value), &["--rotate"], options::NONE));
            match resolved {
                Some(resolved) => resolved,
                None if is_positive_integer(value) => format!("{value}deg"),
                None => return Compiled::Skip,
            }
        }
    };
    Compiled::Nodes(vec![Node::decl("rotate", negate(value, negative))])
}

/// The composed `transform` of the rotate and skew variables.
const TRANSFORM_VALUE: &str = "var(--tw-rotate-x, ) var(--tw-rotate-y, ) var(--tw-rotate-z, ) var(--tw-skew-x, ) var(--tw-skew-y, )";

const TRANSFORM_VARIABLES: &[&str] = &[
    "--tw-rotate-x",
    "--tw-rotate-y",
    "--tw-rotate-z",
    "--tw-skew-x",
    "--tw-skew-y",
];

fn transform_properties() -> Node {
    Node::AtRoot(
        TRANSFORM_VARIABLES
            .iter()
            .map(|ident| property(ident, None, None))
            .collect(),
    )
}

/// Axis rotations: `(axis, variable, transform function)`.
const ROTATE_AXES: &[(&str, &str, &str)] = &[
    ("x", "--tw-rotate-x", "rotateX"),
    ("y", "--tw-rotate-y", "rotateY"),
    ("z", "--tw-rotate-z", "rotateZ"),
];

/// Skews: `(root, [(variable, transform function)])`.
const SKEWS: &[(&str, &[(&str, &str)])] = &[
    (
        "skew",
        &[("--tw-skew-x", "skewX"), ("--tw-skew-y", "skewY")],
    ),
    ("skew-x", &[("--tw-skew-x", "skewX")]),
    ("skew-y", &[("--tw-skew-y", "skewY")]),
];

/// `transform` with no value or an arbitrary value.
fn compile_transform(candidate: &Candidate, _: &DesignSystem) -> Compiled {
    if candidate.modifier().is_some() {
        return Compiled::Skip;
    }
    let value = match candidate.value() {
        None => TRANSFORM_VALUE,
        Some(UtilityValue::Arbitrary { value, .. }) => value.as_str(),
        Some(UtilityValue::Named { .. }) => return Compiled::Skip,
    };
    Compiled::Nodes(vec![transform_properties(), Node::decl("transform", value)])
}

const TRANSFORM_STYLES: &[StaticDecl] = &[
    ("transform-flat", "transform-style", "flat"),
    ("transform-3d", "transform-style", "preserve-3d"),
    ("transform-content", "transform-box", "content-box"),
    ("transform-border", "transform-box", "border-box"),
    ("transform-fill", "transform-box", "fill-box"),
    ("transform-stroke", "transform-box", "stroke-box"),
    ("transform-view", "transform-box", "view-box"),
    ("backface-visible", "backface-visibility", "visible"),
    ("backface-hidden", "backface-visibility", "hidden"),
];

fn register_rotate_and_skew(r: &mut Registrar<'_>) {
    r.static_utility("rotate-none", &[("rotate", "none")]);
    r.utilities
        .functional("-rotate", |c, ds| compile_rotate(c, ds, true));
    r.utilities
        .functional("rotate", |c, ds| compile_rotate(c, ds, false));

    for &(axis, variable, function) in ROTATE_AXES {
        r.functional_utility(
            &format!("rotate-{axis}"),
            FunctionalDesc::new(move |value, _, _| {
                Some(vec![
                    transform_properties(),
                    Node::decl(variable, format!("{function}({value})")),
                    Node::decl("transform", TRANSFORM_VALUE),
                ])
            })
            .negative()
            .theme_keys(&["--rotate"])
            .bare(bare_integer_with("deg")),
        );
    }
    for &(root, functions) in SKEWS {
        r.functional_utility(
            root,
            FunctionalDesc::new(move |value, _, _| {
                let mut nodes = vec![transform_properties()];
                nodes.extend(functions.iter().map(|(variable, function)| {
                    Node::decl(*variable, format!("{function}({value})"))
                }));
                nodes.push(Node::decl("transform", TRANSFORM_VALUE));
                Some(nodes)
            })
            .negative()
            .theme_keys(&["--skew"])
            .bare(bare_integer_with("deg")),
        );
    }

    r.utilities.functional("transform", compile_transform);
    r.static_utility("transform-cpu", &[("transform", TRANSFORM_VALUE)]);
    r.static_utility(
        "transform-gpu",
        &[("transform", &format!("translateZ(0) {TRANSFORM_VALUE}"))],
    );
    r.static_utility("transform-none", &[("transform", "none")]);
}

fn register_transforms(r: &mut Registrar<'_>) {
    register_translate(r);
    register_scale(r);
    register_rotate_and_skew(r);
    r.functional_utility(
        "zoom",
        FunctionalDesc::new(single("zoom")).bare(bare_integer_with("%")),
    );
    static_decls(r, TRANSFORM_STYLES);
}

/* ---------------------------- interactivity ----------------------------- */

const CURSORS: &[&str] = &[
    "auto",
    "default",
    "pointer",
    "wait",
    "text",
    "move",
    "help",
    "not-allowed",
    "none",
    "context-menu",
    "progress",
    "cell",
    "crosshair",
    "vertical-text",
    "alias",
    "copy",
    "no-drop",
    "grab",
    "grabbing",
    "all-scroll",
    "col-resize",
    "row-resize",
    "n-resize",
    "e-resize",
    "s-resize",
    "w-resize",
    "ne-resize",
    "nw-resize",
    "se-resize",
    "sw-resize",
    "ew-resize",
    "ns-resize",
    "nesw-resize",
    "nwse-resize",
    "zoom-in",
    "zoom-out",
];

const TOUCH_KEYWORDS: &[&str] = &["auto", "none", "manipulation"];
const TOUCH_ACTION_VALUE: &str = "var(--tw-pan-x, ) var(--tw-pan-y, ) var(--tw-pinch-zoom, )";

/// Touch pans: `(value, variable)`.
const TOUCH_PANS: &[(&str, &str)] = &[
    ("x", "--tw-pan-x"),
    ("left", "--tw-pan-x"),
    ("right", "--tw-pan-x"),
    ("y", "--tw-pan-y"),
    ("up", "--tw-pan-y"),
    ("down", "--tw-pan-y"),
];

fn touch_properties() -> Node {
    Node::AtRoot(vec![
        property("--tw-pan-x", None, None),
        property("--tw-pan-y", None, None),
        property("--tw-pinch-zoom", None, None),
    ])
}

fn touch_nodes(variable: &str, value: &str) -> Vec<Node> {
    vec![
        touch_properties(),
        Node::decl(variable, value),
        Node::decl("touch-action", TOUCH_ACTION_VALUE),
    ]
}

const USER_SELECT: &[&str] = &["none", "text", "all", "auto"];

const RESIZE: &[StaticDecl] = &[
    ("resize-none", "resize", "none"),
    ("resize-x", "resize", "horizontal"),
    ("resize-y", "resize", "vertical"),
    ("resize", "resize", "both"),
    ("snap-none", "scroll-snap-type", "none"),
];

const SNAP_AXES: &[&str] = &["x", "y", "both"];
const SNAP_STRICTNESS: &[&str] = &["mandatory", "proximity"];

const SNAP_ALIGN: &[StaticDecl] = &[
    ("snap-align-none", "scroll-snap-align", "none"),
    ("snap-start", "scroll-snap-align", "start"),
    ("snap-end", "scroll-snap-align", "end"),
    ("snap-center", "scroll-snap-align", "center"),
    ("snap-normal", "scroll-snap-stop", "normal"),
    ("snap-always", "scroll-snap-stop", "always"),
];

fn snap_properties() -> Node {
    Node::AtRoot(vec![property(
        "--tw-scroll-snap-strictness",
        Some("proximity"),
        Some("*"),
    )])
}

fn register_interactivity(r: &mut Registrar<'_>) {
    for value in CURSORS {
        r.static_utility(&format!("cursor-{value}"), &[("cursor", value)]);
    }
    r.functional_utility(
        "cursor",
        FunctionalDesc::new(single("cursor")).theme_keys(&["--cursor"]),
    );

    for value in TOUCH_KEYWORDS {
        r.static_utility(&format!("touch-{value}"), &[("touch-action", value)]);
    }
    for &(value, variable) in TOUCH_PANS {
        r.static_nodes(
            &format!("touch-pan-{value}"),
            touch_nodes(variable, &format!("pan-{value}")),
        );
    }
    r.static_nodes(
        "touch-pinch-zoom",
        touch_nodes("--tw-pinch-zoom", "pinch-zoom"),
    );

    for value in USER_SELECT {
        r.static_utility(
            &format!("select-{value}"),
            &[("-webkit-user-select", value), ("user-select", value)],
        );
    }

    static_decls(r, RESIZE);
    for axis in SNAP_AXES {
        r.static_nodes(
            &format!("snap-{axis}"),
            vec![
                snap_properties(),
                Node::decl(
                    "scroll-snap-type",
                    format!("{axis} var(--tw-scroll-snap-strictness)"),
                ),
            ],
        );
    }
    for strictness in SNAP_STRICTNESS {
        r.static_nodes(
            &format!("snap-{strictness}"),
            vec![
                snap_properties(),
                Node::decl("--tw-scroll-snap-strictness", *strictness),
            ],
        );
    }
    static_decls(r, SNAP_ALIGN);
}

/* --------------------------- scroll and lists --------------------------- */

const SCROLL_MARGINS: &[(&str, &str)] = &[
    ("scroll-m", "scroll-margin"),
    ("scroll-mx", "scroll-margin-inline"),
    ("scroll-my", "scroll-margin-block"),
    ("scroll-ms", "scroll-margin-inline-start"),
    ("scroll-me", "scroll-margin-inline-end"),
    ("scroll-mbs", "scroll-margin-block-start"),
    ("scroll-mbe", "scroll-margin-block-end"),
    ("scroll-mt", "scroll-margin-top"),
    ("scroll-mr", "scroll-margin-right"),
    ("scroll-mb", "scroll-margin-bottom"),
    ("scroll-ml", "scroll-margin-left"),
];

const SCROLL_PADDINGS: &[(&str, &str)] = &[
    ("scroll-p", "scroll-padding"),
    ("scroll-px", "scroll-padding-inline"),
    ("scroll-py", "scroll-padding-block"),
    ("scroll-ps", "scroll-padding-inline-start"),
    ("scroll-pe", "scroll-padding-inline-end"),
    ("scroll-pbs", "scroll-padding-block-start"),
    ("scroll-pbe", "scroll-padding-block-end"),
    ("scroll-pt", "scroll-padding-top"),
    ("scroll-pr", "scroll-padding-right"),
    ("scroll-pb", "scroll-padding-bottom"),
    ("scroll-pl", "scroll-padding-left"),
];

const LIST_POSITION: &[StaticDecl] = &[
    ("list-inside", "list-style-position", "inside"),
    ("list-outside", "list-style-position", "outside"),
];

const LIST_TYPES: &[(&str, &str)] = &[("none", "none"), ("disc", "disc"), ("decimal", "decimal")];

const APPEARANCE_AND_SCHEME: &[StaticDecl] = &[
    ("appearance-none", "appearance", "none"),
    ("appearance-auto", "appearance", "auto"),
    ("scheme-normal", "color-scheme", "normal"),
    ("scheme-dark", "color-scheme", "dark"),
    ("scheme-light", "color-scheme", "light"),
    ("scheme-light-dark", "color-scheme", "light dark"),
    ("scheme-only-dark", "color-scheme", "only dark"),
    ("scheme-only-light", "color-scheme", "only light"),
];

/// Values of `break-before`/`break-after`.
const BREAK_AROUND: &[&str] = &[
    "auto",
    "avoid",
    "all",
    "avoid-page",
    "page",
    "left",
    "right",
    "column",
];
const BREAK_INSIDE: &[&str] = &["auto", "avoid", "avoid-page", "avoid-column"];

fn register_scroll_and_lists(r: &mut Registrar<'_>) {
    for &(name, property) in SCROLL_MARGINS {
        r.spacing_utility(
            name,
            &["--scroll-margin", "--spacing"],
            spacing_decl(property),
            spacing_options(true, false),
        );
    }
    for &(name, property) in SCROLL_PADDINGS {
        r.spacing_utility(
            name,
            &["--scroll-padding", "--spacing"],
            spacing_decl(property),
            SpacingOptions::default(),
        );
    }

    static_decls(r, LIST_POSITION);
    keyword_utility(
        r,
        "list",
        &["--list-style-type"],
        "list-style-type",
        LIST_TYPES,
    );
    keyword_utility(
        r,
        "list-image",
        &["--list-style-image"],
        "list-style-image",
        &[("none", "none")],
    );
    static_decls(r, APPEARANCE_AND_SCHEME);

    integer_utility(
        r,
        "columns",
        false,
        &["--columns", "--container"],
        "columns",
        AUTO,
    );

    for value in BREAK_AROUND {
        r.static_utility(&format!("break-before-{value}"), &[("break-before", value)]);
    }
    for value in BREAK_INSIDE {
        r.static_utility(&format!("break-inside-{value}"), &[("break-inside", value)]);
    }
    for value in BREAK_AROUND {
        r.static_utility(&format!("break-after-{value}"), &[("break-after", value)]);
    }
}

/* -------------------------- grid and alignment -------------------------- */

const GRID_FLOW: &[StaticDecl] = &[
    ("grid-flow-row", "grid-auto-flow", "row"),
    ("grid-flow-col", "grid-auto-flow", "column"),
    ("grid-flow-dense", "grid-auto-flow", "dense"),
    ("grid-flow-row-dense", "grid-auto-flow", "row dense"),
    ("grid-flow-col-dense", "grid-auto-flow", "column dense"),
];

const GRID_AUTO_SIZES: &[(&str, &str)] = &[
    ("auto", "auto"),
    ("min", "min-content"),
    ("max", "max-content"),
    ("fr", "minmax(0, 1fr)"),
];

/// `auto-cols`/`auto-rows`: `(root, theme key, property)`.
const GRID_AUTO: &[(&str, &str, &str)] = &[
    ("auto-cols", "--grid-auto-columns", "grid-auto-columns"),
    ("auto-rows", "--grid-auto-rows", "grid-auto-rows"),
];

/// `grid-cols`/`grid-rows`: `(root, theme key, property)`.
const GRID_TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "grid-cols",
        "--grid-template-columns",
        "grid-template-columns",
    ),
    ("grid-rows", "--grid-template-rows", "grid-template-rows"),
];

const GRID_TEMPLATE_KEYWORDS: &[(&str, &str)] = &[("none", "none"), ("subgrid", "subgrid")];

/// Flex direction/wrap and `place-*`, `content-*`, `items-*`, `justify-*`.
const ALIGNMENT: &[StaticDecl] = &[
    ("flex-row", "flex-direction", "row"),
    ("flex-row-reverse", "flex-direction", "row-reverse"),
    ("flex-col", "flex-direction", "column"),
    ("flex-col-reverse", "flex-direction", "column-reverse"),
    ("flex-wrap", "flex-wrap", "wrap"),
    ("flex-nowrap", "flex-wrap", "nowrap"),
    ("flex-wrap-reverse", "flex-wrap", "wrap-reverse"),
    ("place-content-center", "place-content", "center"),
    ("place-content-start", "place-content", "start"),
    ("place-content-end", "place-content", "end"),
    ("place-content-center-safe", "place-content", "safe center"),
    ("place-content-end-safe", "place-content", "safe end"),
    ("place-content-between", "place-content", "space-between"),
    ("place-content-around", "place-content", "space-around"),
    ("place-content-evenly", "place-content", "space-evenly"),
    ("place-content-baseline", "place-content", "baseline"),
    ("place-content-stretch", "place-content", "stretch"),
    ("place-items-center", "place-items", "center"),
    ("place-items-start", "place-items", "start"),
    ("place-items-end", "place-items", "end"),
    ("place-items-center-safe", "place-items", "safe center"),
    ("place-items-end-safe", "place-items", "safe end"),
    ("place-items-baseline", "place-items", "baseline"),
    ("place-items-stretch", "place-items", "stretch"),
    ("content-normal", "align-content", "normal"),
    ("content-center", "align-content", "center"),
    ("content-start", "align-content", "flex-start"),
    ("content-end", "align-content", "flex-end"),
    ("content-center-safe", "align-content", "safe center"),
    ("content-end-safe", "align-content", "safe flex-end"),
    ("content-between", "align-content", "space-between"),
    ("content-around", "align-content", "space-around"),
    ("content-evenly", "align-content", "space-evenly"),
    ("content-baseline", "align-content", "baseline"),
    ("content-stretch", "align-content", "stretch"),
    ("items-center", "align-items", "center"),
    ("items-start", "align-items", "flex-start"),
    ("items-end", "align-items", "flex-end"),
    ("items-center-safe", "align-items", "safe center"),
    ("items-end-safe", "align-items", "safe flex-end"),
    ("items-baseline", "align-items", "baseline"),
    ("items-baseline-last", "align-items", "last baseline"),
    ("items-stretch", "align-items", "stretch"),
    ("justify-normal", "justify-content", "normal"),
    ("justify-center", "justify-content", "center"),
    ("justify-start", "justify-content", "flex-start"),
    ("justify-end", "justify-content", "flex-end"),
    ("justify-center-safe", "justify-content", "safe center"),
    ("justify-end-safe", "justify-content", "safe flex-end"),
    ("justify-between", "justify-content", "space-between"),
    ("justify-around", "justify-content", "space-around"),
    ("justify-evenly", "justify-content", "space-evenly"),
    ("justify-baseline", "justify-content", "baseline"),
    ("justify-stretch", "justify-content", "stretch"),
    ("justify-items-normal", "justify-items", "normal"),
    ("justify-items-center", "justify-items", "center"),
    ("justify-items-start", "justify-items", "start"),
    ("justify-items-end", "justify-items", "end"),
    ("justify-items-center-safe", "justify-items", "safe center"),
    ("justify-items-end-safe", "justify-items", "safe end"),
    ("justify-items-stretch", "justify-items", "stretch"),
];

fn register_grid_and_alignment(r: &mut Registrar<'_>) {
    static_decls(r, GRID_FLOW);
    for &(root, theme_key, property) in GRID_AUTO {
        let mut desc = FunctionalDesc::new(single(property))
            .theme_keys(&[theme_key])
            .bare(bare_spacing);
        for (name, value) in GRID_AUTO_SIZES {
            desc = desc.static_value(name, vec![Node::decl(property, *value)]);
        }
        r.functional_utility(root, desc);
    }
    for &(root, theme_key, property) in GRID_TEMPLATES {
        let mut desc = FunctionalDesc::new(single(property))
            .theme_keys(&[theme_key])
            .bare(bare_grid_template);
        for (name, value) in GRID_TEMPLATE_KEYWORDS {
            desc = desc.static_value(name, vec![Node::decl(property, *value)]);
        }
        r.functional_utility(root, desc);
    }
    static_decls(r, ALIGNMENT);
}

/* --------------------------- gaps and spacing --------------------------- */

const GAPS: &[(&str, &str)] = &[
    ("gap", "gap"),
    ("gap-x", "column-gap"),
    ("gap-y", "row-gap"),
];

/// `space-x`/`space-y`: the axis-specific names.
struct SpaceAxis {
    root: &'static str,
    reverse: &'static str,
    sort: &'static str,
    start: &'static str,
    end: &'static str,
}

const SPACE_AXES: &[SpaceAxis] = &[
    SpaceAxis {
        root: "space-x",
        reverse: "--tw-space-x-reverse",
        sort: "row-gap",
        start: "margin-inline-start",
        end: "margin-inline-end",
    },
    SpaceAxis {
        root: "space-y",
        reverse: "--tw-space-y-reverse",
        sort: "column-gap",
        start: "margin-block-start",
        end: "margin-block-end",
    },
];

/// Whether a spacing value is zero, so the margins are plain `0`.
fn is_zero_spacing(value: &str) -> bool {
    if value == "--spacing(0)" || value == "--spacing(-0)" {
        return true;
    }
    parse_dimension(value)
        .is_some_and(|(number, unit)| number == 0.0 && (unit.is_none() || is_length(value)))
}

fn space_nodes(axis: &SpaceAxis, value: &str) -> Vec<Node> {
    let zero = is_zero_spacing(value);
    let reverse = axis.reverse;
    let (start, end) = if zero {
        ("0".to_owned(), "0".to_owned())
    } else {
        (
            format!("calc({value} * var({reverse}))"),
            format!("calc({value} * calc(1 - var({reverse})))"),
        )
    };
    vec![
        Node::AtRoot(vec![property(reverse, Some("0"), None)]),
        Node::rule(
            CHILDREN_SELECTOR,
            vec![
                Node::decl("--tw-sort", axis.sort),
                Node::decl(reverse, "0"),
                Node::decl(axis.start, start),
                Node::decl(axis.end, end),
            ],
        ),
    ]
}

fn register_gaps_and_spacing(r: &mut Registrar<'_>) {
    for &(name, property) in GAPS {
        r.spacing_utility(
            name,
            &["--gap", "--spacing"],
            spacing_decl(property),
            SpacingOptions::default(),
        );
    }
    for axis in SPACE_AXES {
        r.spacing_utility(
            axis.root,
            &["--space", "--spacing"],
            move |value| Some(space_nodes(axis, value)),
            spacing_options(true, false),
        );
    }
    for axis in SPACE_AXES {
        r.static_nodes(
            &format!("{}-reverse", axis.root),
            vec![
                Node::AtRoot(vec![property(axis.reverse, Some("0"), None)]),
                Node::rule(
                    CHILDREN_SELECTOR,
                    vec![
                        Node::decl("--tw-sort", axis.sort),
                        Node::decl(axis.reverse, "1"),
                    ],
                ),
            ],
        );
    }

    r.static_utility("accent-auto", &[("accent-color", "auto")]);
    r.color_utility("accent", &["--accent-color", "--color"], |value| {
        Some(vec![Node::decl("accent-color", value)])
    });
    r.color_utility("caret", &["--caret-color", "--color"], |value| {
        Some(vec![Node::decl("caret-color", value)])
    });
    r.color_utility(
        "divide",
        &["--divide-color", "--border-color", "--color"],
        |value| {
            Some(vec![Node::rule(
                CHILDREN_SELECTOR,
                vec![
                    Node::decl("--tw-sort", "divide-color"),
                    Node::decl("border-color", value),
                ],
            )])
        },
    );
}

/* --------------------------- self and overflow -------------------------- */

const SELF_ALIGNMENT: &[StaticDecl] = &[
    ("place-self-auto", "place-self", "auto"),
    ("place-self-start", "place-self", "start"),
    ("place-self-end", "place-self", "end"),
    ("place-self-center", "place-self", "center"),
    ("place-self-end-safe", "place-self", "safe end"),
    ("place-self-center-safe", "place-self", "safe center"),
    ("place-self-stretch", "place-self", "stretch"),
    ("self-auto", "align-self", "auto"),
    ("self-start", "align-self", "flex-start"),
    ("self-end", "align-self", "flex-end"),
    ("self-center", "align-self", "center"),
    ("self-end-safe", "align-self", "safe flex-end"),
    ("self-center-safe", "align-self", "safe center"),
    ("self-stretch", "align-self", "stretch"),
    ("self-baseline", "align-self", "baseline"),
    ("self-baseline-last", "align-self", "last baseline"),
    ("justify-self-auto", "justify-self", "auto"),
    ("justify-self-start", "justify-self", "flex-start"),
    ("justify-self-end", "justify-self", "flex-end"),
    ("justify-self-center", "justify-self", "center"),
    ("justify-self-end-safe", "justify-self", "safe flex-end"),
    ("justify-self-center-safe", "justify-self", "safe center"),
    ("justify-self-stretch", "justify-self", "stretch"),
];

const OVERFLOW: &[&str] = &["auto", "hidden", "clip", "visible", "scroll"];
const OVERSCROLL: &[&str] = &["auto", "contain", "none"];

const SCROLL_BEHAVIOR: &[StaticDecl] = &[
    ("scroll-auto", "scroll-behavior", "auto"),
    ("scroll-smooth", "scroll-behavior", "smooth"),
    ("scrollbar-auto", "scrollbar-width", "auto"),
    ("scrollbar-thin", "scrollbar-width", "thin"),
    ("scrollbar-none", "scrollbar-width", "none"),
];

const SCROLLBAR_COLOR_VALUE: &str = "var(--tw-scrollbar-thumb) var(--tw-scrollbar-track)";

/// Scrollbar colors: `(root, variable)`.
const SCROLLBAR_COLORS: &[(&str, &str)] = &[
    ("scrollbar-thumb", "--tw-scrollbar-thumb"),
    ("scrollbar-track", "--tw-scrollbar-track"),
];

fn scrollbar_color_properties() -> Node {
    Node::AtRoot(vec![
        property("--tw-scrollbar-thumb", Some("#0000"), Some("<color>")),
        property("--tw-scrollbar-track", Some("#0000"), Some("<color>")),
    ])
}

fn register_self_and_overflow(r: &mut Registrar<'_>) {
    static_decls(r, SELF_ALIGNMENT);
    for value in OVERFLOW {
        r.static_utility(&format!("overflow-{value}"), &[("overflow", value)]);
        r.static_utility(&format!("overflow-x-{value}"), &[("overflow-x", value)]);
        r.static_utility(&format!("overflow-y-{value}"), &[("overflow-y", value)]);
    }
    for value in OVERSCROLL {
        r.static_utility(
            &format!("overscroll-{value}"),
            &[("overscroll-behavior", value)],
        );
        r.static_utility(
            &format!("overscroll-x-{value}"),
            &[("overscroll-behavior-x", value)],
        );
        r.static_utility(
            &format!("overscroll-y-{value}"),
            &[("overscroll-behavior-y", value)],
        );
    }
    static_decls(r, SCROLL_BEHAVIOR);
    for &(root, variable) in SCROLLBAR_COLORS {
        r.color_utility(root, &["--color"], move |value| {
            Some(vec![
                scrollbar_color_properties(),
                Node::decl(variable, value),
                Node::decl("scrollbar-color", SCROLLBAR_COLOR_VALUE),
            ])
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::stylesheet::{candidates_to_css, default_design_system};

    /// Compile one class with the default design system.
    fn css(class: &str) -> Option<String> {
        let mut ds = default_design_system();
        candidates_to_css(&mut ds, &[class]).pop().flatten()
    }

    fn assert_contains(class: &str, fragments: &[&str]) {
        let Some(out) = css(class) else {
            panic!("{class} produced no CSS");
        };
        for fragment in fragments {
            assert!(
                out.contains(fragment),
                "{class}: missing {fragment:?} in\n{out}"
            );
        }
    }

    #[test]
    fn statics_and_display() {
        assert_contains("sr-only", &["clip-path: inset(50%)", "border-width: 0"]);
        assert_contains("hidden", &["display: none"]);
        assert_contains("flex", &["display: flex"]);
        assert_contains("cursor-not-allowed", &["cursor: not-allowed"]);
        assert_contains(
            "select-none",
            &["-webkit-user-select: none", "user-select: none"],
        );
    }

    #[test]
    fn inset_and_margin_spacing() {
        assert_contains("inset-x-4", &["inset-inline: calc(var(--spacing) * 4)"]);
        assert_contains("-top-1/2", &["top: calc(calc(1 / 2 * 100%) * -1)"]);
        assert_contains("-m-px", &["margin: -1px"]);
        assert_contains("mx-auto", &["margin-inline: auto"]);
        assert_eq!(css("m-1/2"), None);
    }

    #[test]
    fn integers_and_grid() {
        assert_contains("z-10", &["z-index: 10"]);
        assert_contains("-order-2", &["order: calc(2 * -1)"]);
        assert_contains("order-first", &["order: -9999"]);
        assert_contains("col-span-3", &["grid-column: span 3 / span 3"]);
        assert_contains(
            "grid-cols-4",
            &["grid-template-columns: repeat(4, minmax(0, 1fr))"],
        );
        assert_eq!(css("grid-cols-0"), None);
        assert_contains("auto-cols-fr", &["grid-auto-columns: minmax(0, 1fr)"]);
        assert_contains("auto-rows-4", &["grid-auto-rows: calc(var(--spacing) * 4)"]);
    }

    #[test]
    fn flex_values() {
        assert_contains("flex-1", &["flex: 1"]);
        assert_contains("flex-1/2", &["flex: calc(1/2 * 100%)"]);
        assert_contains("flex-[2_2_0%]", &["flex: 2 2 0%"]);
        assert_contains("shrink", &["flex-shrink: 1"]);
        assert_contains("grow-0", &["flex-grow: 0"]);
        assert_eq!(css("flex-1/foo"), None);
    }

    #[test]
    fn transforms() {
        assert_contains("rotate-45", &["rotate: 45deg"]);
        assert_contains("-rotate-45", &["rotate: calc(45deg * -1)"]);
        assert_contains("rotate-[x_45deg]", &["rotate: x 45deg"]);
        assert_contains("rotate-x-12", &["--tw-rotate-x: rotateX(12deg)"]);
        assert_contains(
            "scale-50",
            &[
                "--tw-scale-x: 50%",
                "scale: var(--tw-scale-x) var(--tw-scale-y)",
            ],
        );
        assert_contains("-scale-[2]", &["scale: calc(2 * -1)"]);
        assert_contains("skew-x-3", &["--tw-skew-x: skewX(3deg)"]);
        assert_contains("translate-x-full", &["--tw-translate-x: 100%"]);
        assert_contains(
            "transform-gpu",
            &["transform: translateZ(0) var(--tw-rotate-x, )"],
        );
        assert_contains("transform-[scale(2)]", &["transform: scale(2)"]);
        assert_eq!(css("transform-foo"), None);
        assert_contains("zoom-110", &["zoom: 110%"]);
    }

    #[test]
    fn space_zero_and_reverse() {
        assert_contains(
            "space-x-0",
            &["margin-inline-start: 0", "margin-inline-end: 0"],
        );
        assert_contains(
            "space-y-4",
            &["margin-block-start: calc(calc(var(--spacing) * 4) * var(--tw-space-y-reverse))"],
        );
        assert_contains("space-x-reverse", &["--tw-space-x-reverse: 1"]);
    }

    #[test]
    fn container_and_aspect() {
        assert_contains(
            "container",
            &["width: 100%", "@media (width >= 40rem)", "max-width: 40rem"],
        );
        assert_contains("aspect-3/2", &["aspect-ratio: 3/2"]);
        assert_contains("aspect-video", &["aspect-ratio: var(--aspect-video)"]);
        assert_eq!(css("aspect-3/0.3"), None);
    }

    /// Invalid candidates from upstream `utilities.test.ts` (`toEqual('')`).
    #[test]
    fn upstream_invalid_candidates() {
        let invalid = [
            "z",
            "z--1",
            "-z-auto",
            "z-unknown",
            "z-123.5",
            "z-auto/foo",
            "z-10/foo",
            "-z-10/foo",
            "z-[123]/foo",
            "-z-[var(--value)]/foo",
            "aspect",
            "aspect-potato",
            "-aspect-video",
            "-aspect-[10/9]",
            "aspect-foo/bar",
            "aspect-video/foo",
            "aspect-[10/9]/foo",
            "aspect-4/3/foo",
            "aspect--4/3",
            "aspect--4/-3",
            "aspect-4/-3",
            "aspect-1.23/4.56",
            "-flex-1",
            "flex--1",
            "-flex-auto",
            "-flex-initial",
            "-flex-none",
            "-flex-[123]",
            "flex-unknown",
            "flex-1/foo",
            "flex-99/foo",
            "flex--1/2",
            "flex--1/-2",
            "flex-1/-2",
            "flex-1/2/foo",
            "flex-auto/foo",
            "flex-initial/foo",
            "flex-none/foo",
            "flex-[123]/foo",
        ];
        for class in invalid {
            assert_eq!(css(class), None, "{class}");
        }
    }

    /// Valid candidates from upstream `utilities.test.ts` (unoptimized values).
    #[test]
    fn upstream_valid_candidates() {
        assert_contains("-z-[var(--value)]", &["z-index: calc(var(--value) * -1)"]);
        assert_contains("z-[123]", &["z-index: 123"]);
        assert_contains("aspect-8.5/11", &["aspect-ratio: 8.5/11"]);
        assert_contains("aspect-[10/9]", &["aspect-ratio: 10/9"]);
        assert_contains("flex-99", &["flex: 99"]);
        assert_contains("flex-initial", &["flex: 0 auto"]);
    }

    #[test]
    fn colors() {
        assert_contains("accent-red-500/50", &["accent-color:"]);
        assert_contains("divide-red-500", &["border-color: var(--color-red-500)"]);
        assert_contains(
            "scrollbar-thumb-red-500",
            &["--tw-scrollbar-thumb: var(--color-red-500)"],
        );
    }
}
