//! Build-time CSS functions `--theme()`, `--spacing()`, `--alpha()` and the
//! legacy `theme()` (port of Tailwind's `css-functions.ts`).
use crate::ast::Node;
use crate::design_system::DesignSystem;
use crate::utilities::with_alpha;
use crate::utils::dimensions::parse_dimension;
use crate::utils::segment;
use crate::value_parser::{self, ValueNode};

/// The deepest function nesting substituted in one value.
pub const MAX_FUNCTION_DEPTH: usize = 64;

/// A failed build-time function call (Tailwind's thrown errors).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CssFunctionError {
    #[error(
        "the --alpha(…) function requires a color and an alpha value, e.g.: `--alpha({color} / {alpha})`"
    )]
    AlphaArguments { color: String, alpha: String },
    #[error("the --alpha(…) function only accepts one argument")]
    AlphaArity,
    #[error("the --spacing(…) function requires an argument, but received none")]
    SpacingMissing,
    #[error("the --spacing(…) function only accepts a single argument, but received {0}")]
    SpacingArity(usize),
    #[error(
        "the --spacing(…) function requires that the `--spacing` theme variable exists, but it was not found"
    )]
    SpacingTheme,
    #[error("the --theme(…) function can only be used with CSS variables from your theme")]
    ThemeNotVariable,
    #[error(
        "could not resolve value for theme function: `theme({0})`; check the variable name or provide a fallback value"
    )]
    ThemeUnresolved(String),
    #[error("function calls nest deeper than {MAX_FUNCTION_DEPTH}")]
    TooDeep,
}

const FUNCTIONS: &[&str] = &["--alpha", "--spacing", "--theme", "theme"];

/// Whether `value` invokes one of the build-time functions.
#[must_use]
pub fn has_theme_function(value: &str) -> bool {
    FUNCTIONS.iter().any(|name| {
        value
            .match_indices(name)
            .any(|(index, _)| value[index + name.len()..].starts_with('('))
    })
}

fn is_condition_at_rule(name: &str) -> bool {
    matches!(
        name,
        "@media" | "@custom-media" | "@container" | "@supports"
    )
}

/// Substitute build-time functions in declarations and conditional at-rule
/// preludes (port of `substituteFunctions`). Returns whether any was used.
pub fn substitute_functions(ast: &mut [Node], ds: &DesignSystem) -> Result<bool, CssFunctionError> {
    let mut used = false;
    for node in ast.iter_mut() {
        match node {
            Node::Decl {
                value: Some(value), ..
            } if has_theme_function(value) => {
                used = true;
                *value = substitute_functions_in_value(value, false, ds)?;
            }
            Node::AtRule { name, params, .. }
                if is_condition_at_rule(name) && has_theme_function(params) =>
            {
                used = true;
                *params = substitute_functions_in_value(params, true, ds)?;
            }
            _ => {}
        }
        if let Some(children) = node.children_mut() {
            used |= substitute_functions(children, ds)?;
        }
    }
    Ok(used)
}

/// Substitute build-time functions in one value; `at_rule` when the value is
/// an at-rule prelude (where `--theme()` always inlines).
pub fn substitute_functions_in_value(
    value: &str,
    at_rule: bool,
    ds: &DesignSystem,
) -> Result<String, CssFunctionError> {
    let mut ast = value_parser::parse(value);
    substitute_nodes(&mut ast, at_rule, ds, 0)?;
    Ok(value_parser::to_css(&ast))
}

fn substitute_nodes(
    nodes: &mut Vec<ValueNode>,
    at_rule: bool,
    ds: &DesignSystem,
    depth: usize,
) -> Result<(), CssFunctionError> {
    if depth > MAX_FUNCTION_DEPTH {
        return Err(CssFunctionError::TooDeep);
    }
    let mut out = Vec::with_capacity(nodes.len());
    for mut node in std::mem::take(nodes) {
        if let ValueNode::Function { name, nodes: args } = &mut node {
            if FUNCTIONS.contains(&name.as_str()) {
                let text = value_parser::to_css(args);
                let args: Vec<String> = segment(text.trim(), ',')
                    .into_iter()
                    .map(|a| a.trim().to_owned())
                    .collect();
                let result = call(name, &args, at_rule, ds)?;
                out.extend(value_parser::parse(&result));
                continue;
            }
            substitute_nodes(args, at_rule, ds, depth + 1)?;
        }
        out.push(node);
    }
    *nodes = out;
    Ok(())
}

fn call(
    name: &str,
    args: &[String],
    at_rule: bool,
    ds: &DesignSystem,
) -> Result<String, CssFunctionError> {
    let first = args.first().map(String::as_str).unwrap_or_default();
    let rest = args.get(1..).unwrap_or_default();
    match name {
        "--alpha" => alpha(first, rest),
        "--spacing" => spacing(first, rest, ds),
        "--theme" => theme(first, rest, at_rule, ds),
        _ => legacy_theme(first, rest, ds),
    }
}

fn alpha(value: &str, rest: &[String]) -> Result<String, CssFunctionError> {
    let parts: Vec<String> = segment(value, '/')
        .into_iter()
        .map(|v| v.trim().to_owned())
        .collect();
    let color = parts.first().cloned().unwrap_or_default();
    let alpha = parts.get(1).cloned().unwrap_or_default();
    if color.is_empty() || alpha.is_empty() {
        return Err(CssFunctionError::AlphaArguments {
            color: if color.is_empty() {
                "var(--my-color)".into()
            } else {
                color
            },
            alpha: if alpha.is_empty() {
                "50%".into()
            } else {
                alpha
            },
        });
    }
    if !rest.is_empty() {
        return Err(CssFunctionError::AlphaArity);
    }
    Ok(with_alpha(&color, &alpha))
}

fn spacing(value: &str, rest: &[String], ds: &DesignSystem) -> Result<String, CssFunctionError> {
    if value.is_empty() {
        return Err(CssFunctionError::SpacingMissing);
    }
    if !rest.is_empty() {
        return Err(CssFunctionError::SpacingArity(rest.len() + 1));
    }
    let multiplier = ds
        .theme
        .resolve(None, &["--spacing"], crate::theme::options::NONE)
        .ok_or(CssFunctionError::SpacingTheme)?;
    if let Some((number, _)) = parse_dimension(value) {
        if number == 0.0 {
            return Ok("0px".into());
        }
        if number == 1.0 {
            return Ok(multiplier);
        }
    }
    Ok(format!("calc({multiplier} * {value})"))
}

fn theme(
    path: &str,
    fallback: &[String],
    at_rule: bool,
    ds: &DesignSystem,
) -> Result<String, CssFunctionError> {
    if !path.starts_with("--") {
        return Err(CssFunctionError::ThemeNotVariable);
    }
    let mut inline = false;
    let mut path = path;
    if let Some(stripped) = path.strip_suffix(" inline") {
        inline = true;
        path = stripped;
    }
    if at_rule {
        inline = true;
    }
    let Some(resolved) = ds.resolve_theme_value(path, inline) else {
        if !fallback.is_empty() {
            return Ok(fallback.join(", "));
        }
        return Err(CssFunctionError::ThemeUnresolved(path.to_owned()));
    };
    if fallback.is_empty() {
        return Ok(resolved);
    }
    let joined = fallback.join(", ");
    if joined == "initial" {
        return Ok(resolved);
    }
    if resolved == "initial" {
        return Ok(joined);
    }
    if resolved.starts_with("var(")
        || resolved.starts_with("theme(")
        || resolved.starts_with("--theme(")
    {
        let mut ast = value_parser::parse(&resolved);
        inject_fallback(&mut ast, &joined);
        return Ok(value_parser::to_css(&ast));
    }
    Ok(resolved)
}

fn legacy_theme(
    path: &str,
    fallback: &[String],
    ds: &DesignSystem,
) -> Result<String, CssFunctionError> {
    let path = eventually_unquote(path);
    match ds.resolve_theme_value(&path, true) {
        Some(value) => Ok(value),
        None if !fallback.is_empty() => Ok(fallback.join(", ")),
        None => Err(CssFunctionError::ThemeUnresolved(path)),
    }
}

fn eventually_unquote(value: &str) -> String {
    let mut chars = value.chars();
    let Some(quote) = chars.next().filter(|c| *c == '\'' || *c == '"') else {
        return value.to_owned();
    };
    let inner: Vec<char> = value.chars().skip(1).collect();
    let end = inner.len().saturating_sub(1);
    let mut out = String::new();
    let mut i = 0;
    while i < end {
        let c = inner[i];
        let next = inner.get(i + 1).copied();
        if c == '\\' && (next == Some(quote) || next == Some('\\')) && i + 1 < end {
            out.push(next.unwrap_or(c));
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn inject_fallback(ast: &mut [ValueNode], fallback: &str) {
    for node in ast.iter_mut() {
        if let ValueNode::Function { name, nodes } = node {
            if matches!(name.as_str(), "var" | "theme" | "--theme") {
                if nodes.len() == 1 {
                    nodes.push(ValueNode::Word(format!(", {fallback}")));
                } else if let Some(ValueNode::Word(last)) = nodes.last_mut()
                    && last == "initial"
                {
                    *last = fallback.to_owned();
                }
            }
            inject_fallback(nodes, fallback);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_invocations() {
        assert!(has_theme_function("--spacing(4)"));
        assert!(has_theme_function("calc(--theme(--x) * 2)"));
        assert!(!has_theme_function("var(--spacing)"));
    }

    #[test]
    fn unquotes() {
        assert_eq!(eventually_unquote("'colors.red'"), "colors.red");
        assert_eq!(eventually_unquote("plain"), "plain");
    }
}
