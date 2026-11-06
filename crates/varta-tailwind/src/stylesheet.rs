//! The embedded Tailwind CSS v4.3.5 stylesheets and the per-candidate entry
//! points (port of `candidatesToCss`).
use crate::ast::Node;
use crate::compile::compile_candidates;
use crate::css_parser;
use crate::design_system::DesignSystem;
use crate::optimize::optimize_ast;
use crate::printer::to_css;
use crate::theme::{Theme, options};
use crate::utils::unescape;

/// `tailwindcss/index.css` (MIT, Tailwind Labs), pinned at v4.3.5.
pub const INDEX_CSS: &str = include_str!("../tailwindcss/index.css");
/// `tailwindcss/theme.css` (MIT, Tailwind Labs), pinned at v4.3.5.
pub const THEME_CSS: &str = include_str!("../tailwindcss/theme.css");
/// `tailwindcss/preflight.css` (MIT, Tailwind Labs), pinned at v4.3.5.
pub const PREFLIGHT_CSS: &str = include_str!("../tailwindcss/preflight.css");
/// `tailwindcss/utilities.css` (MIT, Tailwind Labs), pinned at v4.3.5.
pub const UTILITIES_CSS: &str = include_str!("../tailwindcss/utilities.css");
/// The Tailwind CSS version the embedded stylesheets and the port follow.
pub const TAILWIND_VERSION: &str = "4.3.5";

/// The design system of the default theme (`@import "tailwindcss"` with no
/// customization), as the oracle's `__unstable__loadDesignSystem` builds it.
///
/// # Panics
/// Never: the embedded theme is fixed and parsed by the crate's tests.
#[must_use]
pub fn default_design_system() -> DesignSystem {
    let mut theme = Theme::new();
    // Invariant: the embedded theme.css parses (checked by tests).
    let ast = css_parser::parse(THEME_CSS).unwrap_or_default();
    for node in &ast {
        let Node::AtRule {
            name,
            params,
            nodes,
        } = node
        else {
            continue;
        };
        if name != "@theme" {
            continue;
        }
        let opts = theme_options(params);
        for child in nodes {
            match child {
                Node::AtRule { name, .. } if name == "@keyframes" => {
                    theme.add_keyframes(child.clone());
                }
                Node::Decl {
                    property, value, ..
                } if property.starts_with("--") => {
                    // Invariant: the default theme resets no namespace with a value.
                    let _ = theme.add(
                        &unescape(property),
                        value.as_deref().unwrap_or_default(),
                        opts,
                    );
                }
                _ => {}
            }
        }
    }
    DesignSystem::new(theme)
}

/// Theme options of `@theme` parameters (port of `parseThemeOptions`).
#[must_use]
pub fn theme_options(params: &str) -> u8 {
    let mut opts = options::NONE;
    for option in crate::utils::segment(params, ' ') {
        match option.as_str() {
            "reference" => opts |= options::REFERENCE,
            "inline" => opts |= options::INLINE,
            "default" => opts |= options::DEFAULT,
            "static" => opts |= options::STATIC,
            _ => {}
        }
    }
    opts
}

/// The CSS of each class token compiled on its own, or `None` when it
/// produces nothing (port of `candidatesToCss`, no polyfills).
#[must_use]
pub fn candidates_to_css(ds: &mut DesignSystem, classes: &[&str]) -> Vec<Option<String>> {
    classes
        .iter()
        .map(|class| {
            let compiled = compile_candidates([*class], ds, true);
            if !compiled.invalid.is_empty() || compiled.nodes.is_empty() {
                return None;
            }
            let nodes = optimize_ast(compiled.nodes, &mut ds.theme);
            if nodes.is_empty() {
                None
            } else {
                Some(to_css(&nodes))
            }
        })
        .collect()
}

/// Whether a class token compiles to any CSS on its own.
#[must_use]
pub fn compiles(ds: &DesignSystem, class: &str) -> bool {
    let compiled = compile_candidates([class], ds, true);
    compiled.invalid.is_empty() && !compiled.nodes.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_theme_parses() {
        assert!(css_parser::parse(THEME_CSS).is_ok());
        assert!(css_parser::parse(PREFLIGHT_CSS).is_ok());
        let ds = default_design_system();
        assert!(ds.theme.len() > 300);
        assert_eq!(
            ds.theme.resolve_value(Some("red-500"), &["--color"]),
            Some("oklch(63.7% 0.237 25.331)".into())
        );
        assert!(!ds.theme.keyframes().is_empty());
    }
}
