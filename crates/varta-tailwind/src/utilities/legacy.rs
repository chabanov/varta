//! Legacy utilities every Tailwind v4 build registers through its
//! compatibility hooks (port of `compat/legacy-utilities.ts`, Tailwind Labs,
//! MIT): v4.0 gradient, position, `max-w-screen`, `flex-grow`/`flex-shrink`,
//! `start`/`end` inset names.
use super::{Compiled, Utilities};
use crate::ast::Node;
use crate::candidate::UtilityValue;
use crate::theme::options;
use crate::utils::infer_data_type::{is_positive_integer, is_valid_spacing_multiplier};
use crate::utils::segment;

/// `bg-gradient-to-*` directions.
const GRADIENT_DIRECTIONS: &[(&str, &str)] = &[
    ("t", "top"),
    ("tr", "top right"),
    ("r", "right"),
    ("br", "bottom right"),
    ("b", "bottom"),
    ("bl", "bottom left"),
    ("l", "left"),
    ("tl", "top left"),
];

/// Static legacy utilities: name and declarations.
const STATIC: &[(&str, &[(&str, &str)])] = &[
    ("bg-left-top", &[("background-position", "left top")]),
    ("bg-right-top", &[("background-position", "right top")]),
    ("bg-left-bottom", &[("background-position", "left bottom")]),
    (
        "bg-right-bottom",
        &[("background-position", "right bottom")],
    ),
    ("object-left-top", &[("object-position", "left top")]),
    ("object-right-top", &[("object-position", "right top")]),
    ("object-left-bottom", &[("object-position", "left bottom")]),
    (
        "object-right-bottom",
        &[("object-position", "right bottom")],
    ),
];

/// `start`/`end` and their logical properties.
const INSETS: &[(&str, &str)] = &[("start", "inset-inline-start"), ("end", "inset-inline-end")];

fn static_decls(
    utilities: &mut Utilities,
    name: &str,
    decls: &'static [(&'static str, &'static str)],
) {
    utilities.static_fn(name, move |_, _| {
        Compiled::Nodes(decls.iter().map(|(p, v)| Node::decl(*p, *v)).collect())
    });
}

fn grow_shrink(utilities: &mut Utilities, name: &'static str) {
    utilities.functional(name, move |candidate, _| {
        if candidate.modifier().is_some() {
            return Compiled::Skip;
        }
        match candidate.value() {
            None => Compiled::Nodes(vec![Node::decl(name, "1")]),
            Some(UtilityValue::Arbitrary { value, .. }) => {
                Compiled::Nodes(vec![Node::decl(name, value.clone())])
            }
            Some(UtilityValue::Named { value, .. }) if is_positive_integer(value) => {
                Compiled::Nodes(vec![Node::decl(name, value.clone())])
            }
            Some(_) => Compiled::Skip,
        }
    });
}

fn inset(utilities: &mut Utilities, name: &str, property: &'static str, negative: bool) {
    let root = if negative {
        format!("-{name}")
    } else {
        name.to_owned()
    };
    utilities.functional(&root, move |candidate, ds| {
        let sign = |value: String| {
            if negative {
                format!("calc({value} * -1)")
            } else {
                value
            }
        };
        let Some(value) = candidate.value() else {
            return Compiled::Skip;
        };
        let (value, fraction) = match value {
            UtilityValue::Arbitrary { value, .. } => {
                if candidate.modifier().is_some() {
                    return Compiled::Skip;
                }
                return Compiled::Nodes(vec![Node::decl(property, sign(value.clone()))]);
            }
            UtilityValue::Named { value, fraction } => (value, fraction),
        };
        let theme = &ds.theme;
        let mut resolved = theme.resolve(
            Some(fraction.as_deref().unwrap_or(value)),
            &["--inset", "--spacing"],
            options::NONE,
        );
        if resolved.is_none()
            && let Some(fraction) = fraction
        {
            let parts = segment(fraction, '/');
            let rhs = parts.get(1).map_or("", String::as_str);
            if !is_positive_integer(&parts[0]) || !is_positive_integer(rhs) {
                return Compiled::Skip;
            }
            resolved = Some(format!("calc({fraction} * 100%)"));
        }
        if resolved.is_none()
            && negative
            && let Some(multiplier) = theme.resolve(None, &["--spacing"], options::NONE)
            && is_valid_spacing_multiplier(value)
        {
            return Compiled::Nodes(vec![Node::decl(
                property,
                format!("calc({multiplier} * -{value})"),
            )]);
        }
        if resolved.is_none()
            && let Some(multiplier) = theme.resolve(None, &["--spacing"], options::NONE)
            && is_valid_spacing_multiplier(value)
        {
            resolved = Some(format!("calc({multiplier} * {value})"));
        }
        match resolved {
            Some(value) => Compiled::Nodes(vec![Node::decl(property, sign(value))]),
            None => Compiled::Skip,
        }
    });
}

/// Register the legacy utilities (port of `registerLegacyUtilities`).
pub fn register(utilities: &mut Utilities) {
    for (value, direction) in GRADIENT_DIRECTIONS {
        let position = format!("to {direction} in oklab");
        utilities.static_fn(&format!("bg-gradient-to-{value}"), move |_, _| {
            Compiled::Nodes(vec![
                Node::decl("--tw-gradient-position", position.clone()),
                Node::decl(
                    "background-image",
                    "linear-gradient(var(--tw-gradient-stops))",
                ),
            ])
        });
    }
    for (name, decls) in STATIC {
        static_decls(utilities, name, decls);
    }
    utilities.functional("max-w-screen", |candidate, ds| {
        let Some(UtilityValue::Named { value, .. }) = candidate.value() else {
            return Compiled::Skip;
        };
        match ds
            .theme
            .resolve(Some(value), &["--breakpoint"], options::NONE)
        {
            Some(value) if !value.is_empty() => {
                Compiled::Nodes(vec![Node::decl("max-width", value)])
            }
            _ => Compiled::Skip,
        }
    });
    static_decls(
        utilities,
        "overflow-ellipsis",
        &[("text-overflow", "ellipsis")],
    );
    static_decls(
        utilities,
        "decoration-slice",
        &[
            ("-webkit-box-decoration-break", "slice"),
            ("box-decoration-break", "slice"),
        ],
    );
    static_decls(
        utilities,
        "decoration-clone",
        &[
            ("-webkit-box-decoration-break", "clone"),
            ("box-decoration-break", "clone"),
        ],
    );
    grow_shrink(utilities, "flex-shrink");
    grow_shrink(utilities, "flex-grow");
    static_decls(utilities, "order-none", &[("order", "0")]);
    static_decls(utilities, "break-words", &[("overflow-wrap", "break-word")]);
    for (name, property) in INSETS {
        let fixed: [(String, &'static str); 5] = [
            (format!("{name}-auto"), "auto"),
            (format!("{name}-full"), "100%"),
            (format!("-{name}-full"), "-100%"),
            (format!("{name}-px"), "1px"),
            (format!("-{name}-px"), "-1px"),
        ];
        for (utility, value) in fixed {
            utilities.static_fn(&utility, move |_, _| {
                Compiled::Nodes(vec![Node::decl(*property, value)])
            });
        }
        inset(utilities, name, property, true);
        inset(utilities, name, property, false);
    }
}
