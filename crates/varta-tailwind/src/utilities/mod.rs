//! The utility registry and its registration helpers (port of the top of
//! Tailwind's `utilities.ts`). The built-in utilities are registered by the
//! `part*` modules in Tailwind's source order; `css_utility` ports `@utility`.
use crate::ast::Node;
use crate::candidate::{Candidate, CandidateModifier, UtilityValue};
use crate::design_system::DesignSystem;
use crate::theme::{Theme, options};
use crate::utils::infer_data_type::{
    is_positive_integer, is_valid_opacity_value, is_valid_spacing_multiplier,
};
use crate::utils::js_number::{number_to_string, to_number};
use crate::utils::math_operators::add_whitespace_around_math_operators;
use crate::utils::segment;
use std::collections::BTreeMap;
use std::rc::Rc;

pub mod css_utility;
pub mod legacy;
mod part1;
mod part2;
mod part3;
mod part4;

/// Whether a utility is static (`flex`) or functional (`bg-*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UtilityKind {
    Static,
    Functional,
}

/// The result of a utility's compile function (Tailwind returns
/// `AstNode[] | undefined | null`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compiled {
    /// The declarations (and nested rules) of the utility.
    Nodes(Vec<Node>),
    /// `undefined`: this utility does not handle the candidate.
    Skip,
    /// `null`: the candidate is invalid for this utility.
    Invalid,
}

impl From<Option<Vec<Node>>> for Compiled {
    fn from(nodes: Option<Vec<Node>>) -> Self {
        nodes.map_or(Compiled::Skip, Compiled::Nodes)
    }
}

/// A compile function.
pub type CompileFn = Rc<dyn Fn(&Candidate, &DesignSystem) -> Compiled>;

/// One registered utility.
#[derive(Clone)]
pub struct Utility {
    pub kind: UtilityKind,
    pub compile: CompileFn,
    /// `options.types` (fallback utilities list `any` among several types).
    pub types: Vec<String>,
}

/// The utility registry (port of `Utilities`).
#[derive(Default, Clone)]
pub struct Utilities {
    utilities: BTreeMap<String, Vec<Utility>>,
    /// Names in registration order.
    order: Vec<String>,
}

impl Utilities {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, name: &str, utility: Utility) {
        if !self.utilities.contains_key(name) {
            self.order.push(name.to_owned());
        }
        self.utilities
            .entry(name.to_owned())
            .or_default()
            .push(utility);
    }

    /// Register a static utility.
    pub fn static_fn(
        &mut self,
        name: &str,
        compile: impl Fn(&Candidate, &DesignSystem) -> Compiled + 'static,
    ) {
        self.push(
            name,
            Utility {
                kind: UtilityKind::Static,
                compile: Rc::new(compile),
                types: Vec::new(),
            },
        );
    }

    /// Register a functional utility.
    pub fn functional(
        &mut self,
        name: &str,
        compile: impl Fn(&Candidate, &DesignSystem) -> Compiled + 'static,
    ) {
        self.functional_with_types(name, Vec::new(), compile);
    }

    /// Register a functional utility with `options.types`.
    pub fn functional_with_types(
        &mut self,
        name: &str,
        types: Vec<String>,
        compile: impl Fn(&Candidate, &DesignSystem) -> Compiled + 'static,
    ) {
        self.push(
            name,
            Utility {
                kind: UtilityKind::Functional,
                compile: Rc::new(compile),
                types,
            },
        );
    }

    #[must_use]
    pub fn has(&self, name: &str, kind: UtilityKind) -> bool {
        self.utilities
            .get(name)
            .is_some_and(|list| list.iter().any(|u| u.kind == kind))
    }

    /// The utilities registered under `name`, in registration order.
    #[must_use]
    pub fn get(&self, name: &str) -> &[Utility] {
        self.utilities.get(name).map_or(&[], Vec::as_slice)
    }

    /// Names with a utility of `kind`, in registration order.
    #[must_use]
    pub fn keys(&self, kind: UtilityKind) -> Vec<String> {
        self.order
            .iter()
            .filter(|name| self.has(name, kind))
            .cloned()
            .collect()
    }
}

/// An `@property` rule registering `--tw-*` (port of `property()`).
#[must_use]
pub fn property(ident: &str, initial_value: Option<&str>, syntax: Option<&str>) -> Node {
    let mut nodes = vec![
        Node::decl(
            "syntax",
            match syntax {
                Some(syntax) => format!("\"{syntax}\""),
                None => "\"*\"".to_owned(),
            },
        ),
        Node::decl("inherits", "false"),
    ];
    if let Some(initial) = initial_value.filter(|v| !v.is_empty()) {
        nodes.push(Node::decl("initial-value", initial));
    }
    Node::at_rule("@property", ident, nodes)
}

/// Convert a numeric alpha (`0.5`) to a percentage (`50%`) with JavaScript
/// number formatting; anything else passes through.
fn alpha_percentage(alpha: &str) -> String {
    let number = to_number(alpha);
    if number.is_nan() {
        alpha.to_owned()
    } else {
        format!("{}%", number_to_string(number * 100.0))
    }
}

/// Apply opacity to a color with `color-mix` (port of `withAlpha`).
#[must_use]
pub fn with_alpha(value: &str, alpha: &str) -> String {
    let alpha = alpha_percentage(alpha);
    if alpha == "100%" {
        return value.to_owned();
    }
    format!("color-mix(in oklab, {value} {alpha}, transparent)")
}

/// Replace the opacity of a color with relative color syntax
/// (port of `replaceAlpha`).
#[must_use]
pub fn replace_alpha(value: &str, alpha: &str) -> String {
    let alpha = alpha_percentage(alpha);
    format!("oklab(from {value} l a b / {alpha})")
}

/// Resolve a color and an optional opacity modifier (port of `asColor`).
#[must_use]
pub fn as_color(
    value: &str,
    modifier: Option<&CandidateModifier>,
    theme: &Theme,
) -> Option<String> {
    let Some(modifier) = modifier else {
        return Some(value.to_owned());
    };
    match modifier {
        CandidateModifier::Arbitrary(alpha) => Some(with_alpha(value, alpha)),
        CandidateModifier::Named(name) => {
            if let Some(alpha) = theme.resolve(Some(name), &["--opacity"], options::NONE) {
                return Some(with_alpha(value, &alpha));
            }
            if !is_valid_opacity_value(name) {
                return None;
            }
            Some(with_alpha(value, &format!("{name}%")))
        }
    }
}

/// Resolve a named color candidate in `theme_keys` (port of
/// `resolveThemeColor`); `inherit`, `transparent` and `current` are universal.
#[must_use]
pub fn resolve_theme_color(
    candidate: &Candidate,
    theme: &Theme,
    theme_keys: &[&str],
) -> Option<String> {
    let value = candidate.value()?.value();
    let resolved = match value {
        "inherit" => Some("inherit".to_owned()),
        "transparent" => Some("transparent".to_owned()),
        "current" => Some("currentcolor".to_owned()),
        _ => theme.resolve(Some(value), theme_keys, options::NONE),
    }?;
    as_color(&resolved, candidate.modifier(), theme)
}

/// A named utility value as passed to bare-value handlers.
#[derive(Debug, Clone, Copy)]
pub struct NamedValue<'a> {
    pub value: &'a str,
    pub fraction: Option<&'a str>,
}

/// A bare-value handler: maps `p-4` style values to a CSS value.
pub type BareValueFn = Rc<dyn Fn(NamedValue<'_>, &Theme) -> Option<String>>;
/// A value handler: `(value, data type, theme)` to nodes; `None` skips.
pub type HandleFn = Rc<dyn Fn(&str, Option<&str>, &Theme) -> Option<Vec<Node>>>;

/// A single-value handler: value to nodes; `None` skips.
pub type ValueFn = Rc<dyn Fn(&str) -> Option<Vec<Node>>>;

/// `defaultValue` of a functional utility description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DefaultValue {
    /// `undefined`: resolve the bare theme keys.
    #[default]
    Undefined,
    /// `null`: no default value.
    Null,
    Value(String),
}

/// A functional utility description (port of `UtilityDescription`).
#[derive(Clone)]
pub struct FunctionalDesc {
    pub supports_negative: bool,
    pub supports_fractions: bool,
    pub theme_keys: Vec<String>,
    pub default_value: DefaultValue,
    /// Named static values (`auto`), in declaration order.
    pub static_values: Vec<(String, Vec<Node>)>,
    pub handle_bare_value: Option<BareValueFn>,
    pub handle_negative_bare_value: Option<BareValueFn>,
    pub handle: HandleFn,
}

impl FunctionalDesc {
    /// A description with only a handler.
    #[must_use]
    pub fn new(handle: impl Fn(&str, Option<&str>, &Theme) -> Option<Vec<Node>> + 'static) -> Self {
        Self {
            supports_negative: false,
            supports_fractions: false,
            theme_keys: Vec::new(),
            default_value: DefaultValue::Undefined,
            static_values: Vec::new(),
            handle_bare_value: None,
            handle_negative_bare_value: None,
            handle: Rc::new(handle),
        }
    }
    #[must_use]
    pub fn theme_keys(mut self, keys: &[&str]) -> Self {
        self.theme_keys = keys.iter().map(|k| (*k).to_owned()).collect();
        self
    }
    #[must_use]
    pub fn negative(mut self) -> Self {
        self.supports_negative = true;
        self
    }
    #[must_use]
    pub fn fractions(mut self) -> Self {
        self.supports_fractions = true;
        self
    }
    #[must_use]
    pub fn default_value(mut self, value: DefaultValue) -> Self {
        self.default_value = value;
        self
    }
    #[must_use]
    pub fn static_value(mut self, name: &str, nodes: Vec<Node>) -> Self {
        self.static_values.push((name.to_owned(), nodes));
        self
    }
    #[must_use]
    pub fn bare(mut self, f: impl Fn(NamedValue<'_>, &Theme) -> Option<String> + 'static) -> Self {
        self.handle_bare_value = Some(Rc::new(f));
        self
    }
    #[must_use]
    pub fn negative_bare(
        mut self,
        f: impl Fn(NamedValue<'_>, &Theme) -> Option<String> + 'static,
    ) -> Self {
        self.handle_negative_bare_value = Some(Rc::new(f));
        self
    }
}

/// Keys of a description as `&str`.
fn keys_of(keys: &[String]) -> Vec<&str> {
    keys.iter().map(String::as_str).collect()
}

/// The registration context of the built-in utilities: the registry and the
/// theme at creation time (Tailwind's `createUtilities` closure scope).
pub struct Registrar<'a> {
    pub utilities: &'a mut Utilities,
    pub theme: &'a Theme,
}

impl Registrar<'_> {
    /// A static utility of fixed declarations (port of `staticUtility`).
    pub fn static_utility(&mut self, name: &str, declarations: &[(&str, &str)]) {
        let nodes: Vec<Node> = declarations
            .iter()
            .map(|(property, value)| Node::decl(*property, *value))
            .collect();
        self.static_nodes(name, nodes);
    }

    /// A static utility of fixed nodes (declarations or `@property` rules).
    pub fn static_nodes(&mut self, name: &str, nodes: Vec<Node>) {
        self.utilities
            .static_fn(name, move |_, _| Compiled::Nodes(nodes.clone()));
    }

    /// A functional utility (port of `functionalUtility`).
    pub fn functional_utility(&mut self, root: &str, desc: FunctionalDesc) {
        if desc.supports_negative {
            let negative = desc.clone();
            self.utilities
                .functional(&format!("-{root}"), move |c, ds| {
                    handle_functional(&negative, true, c, &ds.theme)
                });
        }
        self.utilities.functional(root, move |c, ds| {
            handle_functional(&desc, false, c, &ds.theme)
        });
    }

    /// A color utility with opacity modifiers (port of `colorUtility`).
    pub fn color_utility(
        &mut self,
        root: &str,
        theme_keys: &[&str],
        handle: impl Fn(&str) -> Option<Vec<Node>> + 'static,
    ) {
        let keys: Vec<String> = theme_keys.iter().map(|k| (*k).to_owned()).collect();
        self.utilities.functional(root, move |candidate, ds| {
            let Some(value) = candidate.value() else {
                return Compiled::Skip;
            };
            let value = match value {
                UtilityValue::Arbitrary { value, .. } => {
                    as_color(value, candidate.modifier(), &ds.theme)
                }
                UtilityValue::Named { .. } => {
                    resolve_theme_color(candidate, &ds.theme, &keys_of(&keys))
                }
            };
            match value {
                Some(value) => handle(&value).into(),
                None => Compiled::Skip,
            }
        });
    }

    /// A spacing utility (port of `spacingUtility`).
    pub fn spacing_utility(
        &mut self,
        name: &str,
        theme_keys: &[&str],
        handle: impl Fn(&str) -> Option<Vec<Node>> + 'static,
        options: SpacingOptions,
    ) {
        let handle: ValueFn = Rc::new(handle);
        if options.supports_negative {
            let h = handle.clone();
            self.utilities
                .static_fn(&format!("-{name}-px"), move |_, _| h("-1px").into());
        }
        let h = handle.clone();
        self.utilities
            .static_fn(&format!("{name}-px"), move |_, _| h("1px").into());
        let mut desc = FunctionalDesc::new(move |value, _, _| handle(value))
            .theme_keys(theme_keys)
            .default_value(DefaultValue::Null)
            .bare(|value, theme| {
                theme.resolve(None, &["--spacing"], options::NONE)?;
                if !is_valid_spacing_multiplier(value.value) {
                    return None;
                }
                Some(format!("--spacing({})", value.value))
            })
            .negative_bare(|value, theme| {
                theme.resolve(None, &["--spacing"], options::NONE)?;
                if !is_valid_spacing_multiplier(value.value) {
                    return None;
                }
                Some(format!("--spacing(-{})", value.value))
            });
        desc.supports_negative = options.supports_negative;
        desc.supports_fractions = options.supports_fractions;
        desc.static_values = options.static_values;
        self.functional_utility(name, desc);
    }
}

/// Options of [`Registrar::spacing_utility`].
#[derive(Clone, Default)]
pub struct SpacingOptions {
    pub supports_negative: bool,
    pub supports_fractions: bool,
    pub static_values: Vec<(String, Vec<Node>)>,
}

/// The compile function of a functional utility (port of
/// `handleFunctionalUtility`).
fn handle_functional(
    desc: &FunctionalDesc,
    negative: bool,
    candidate: &Candidate,
    theme: &Theme,
) -> Compiled {
    let keys = keys_of(&desc.theme_keys);
    let modifier = candidate.modifier();
    let mut data_type: Option<&str> = None;
    let value: Option<String> = match candidate.value() {
        None => {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            match &desc.default_value {
                DefaultValue::Undefined => theme.resolve(None, &keys, options::NONE),
                DefaultValue::Null => None,
                DefaultValue::Value(value) => Some(value.clone()),
            }
        }
        Some(UtilityValue::Arbitrary {
            value,
            data_type: dt,
        }) => {
            if modifier.is_some() {
                return Compiled::Skip;
            }
            data_type = dt.as_deref();
            Some(value.clone())
        }
        Some(UtilityValue::Named { value, fraction }) => {
            let named = NamedValue {
                value,
                fraction: fraction.as_deref(),
            };
            let mut resolved = theme.resolve(
                Some(fraction.as_deref().unwrap_or(value)),
                &keys,
                options::NONE,
            );
            if resolved.is_some() && modifier.is_some() && fraction.is_none() {
                return Compiled::Skip;
            }
            if resolved.is_none()
                && desc.supports_fractions
                && let Some(fraction) = fraction
            {
                let parts = segment(fraction, '/');
                let (lhs, rhs) = (&parts[0], parts.get(1).map_or("", String::as_str));
                if !is_positive_integer(lhs) || !is_positive_integer(rhs) {
                    return Compiled::Skip;
                }
                resolved = Some(format!("calc({lhs} / {rhs} * 100%)"));
            }
            if resolved.is_none()
                && negative
                && let Some(bare) = &desc.handle_negative_bare_value
            {
                resolved = bare(named, theme);
                if !resolved.as_deref().is_some_and(|v| v.contains('/')) && modifier.is_some() {
                    return Compiled::Skip;
                }
                if let Some(value) = &resolved {
                    return (desc.handle)(value, None, theme).into();
                }
            }
            if resolved.is_none()
                && let Some(bare) = &desc.handle_bare_value
            {
                resolved = bare(named, theme);
                if !resolved.as_deref().is_some_and(|v| v.contains('/')) && modifier.is_some() {
                    return Compiled::Skip;
                }
            }
            if resolved.is_none()
                && !negative
                && modifier.is_none()
                && let Some((_, nodes)) = desc.static_values.iter().find(|(name, _)| name == value)
            {
                return Compiled::Nodes(nodes.clone());
            }
            resolved
        }
    };
    let Some(value) = value else {
        return Compiled::Skip;
    };
    let value = if negative {
        add_whitespace_around_math_operators(&format!("calc({value} * -1)"))
    } else {
        value
    };
    (desc.handle)(&value, data_type, theme).into()
}

/// Create the built-in utilities (port of `createUtilities`).
#[must_use]
pub fn create_utilities(theme: &Theme) -> Utilities {
    let mut utilities = Utilities::new();
    let mut registrar = Registrar {
        utilities: &mut utilities,
        theme,
    };
    part1::register(&mut registrar);
    part2::register(&mut registrar);
    part3::register(&mut registrar);
    part4::register(&mut registrar);
    utilities
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_uses_javascript_numbers() {
        assert_eq!(
            with_alpha("red", "0.5"),
            "color-mix(in oklab, red 50%, transparent)"
        );
        assert_eq!(with_alpha("red", "1"), "red");
        assert_eq!(
            with_alpha("red", "0.07"),
            "color-mix(in oklab, red 7.000000000000001%, transparent)"
        );
        assert_eq!(
            with_alpha("red", "var(--a)"),
            "color-mix(in oklab, red var(--a), transparent)"
        );
        assert_eq!(replace_alpha("red", "0.5"), "oklab(from red l a b / 50%)");
    }

    #[test]
    fn property_rules() {
        assert_eq!(
            property("--tw-x", Some("0"), None),
            Node::at_rule(
                "@property",
                "--tw-x",
                vec![
                    Node::decl("syntax", "\"*\""),
                    Node::decl("inherits", "false"),
                    Node::decl("initial-value", "0"),
                ]
            )
        );
    }
}
