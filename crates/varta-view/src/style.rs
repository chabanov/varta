//! Stylesheets of draft v0: the checked stylesheet grammar and the cascade
//! subset (sheets by ascending id, then specificity, then source order;
//! inline `set_style` wins; custom properties inherit and `var()` is
//! substituted). Only custom properties inherit in the headless output; a
//! property's computed value is its cascaded value.
use std::collections::BTreeMap;

/// The renderer environment media conditions are evaluated against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Viewport width in CSS pixels.
    pub width: f64,
    pub dark: bool,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            width: DEFAULT_VIEWPORT_WIDTH,
            dark: false,
        }
    }
}
/// Default headless viewport width in CSS pixels.
pub const DEFAULT_VIEWPORT_WIDTH: f64 = 1024.0;
pub(crate) use varta_common::view::stylesheet::{Name, Rule, Subject};

/// Whether `rule`'s media conditions hold in `environment`.
pub(crate) fn applies(rule: &Rule, environment: &Environment) -> bool {
    rule.applies(environment.width, environment.dark)
}

/// Parse a whole stylesheet with the shared grammar
/// (`varta_common::view::stylesheet`); `None` for any violation.
pub(crate) fn parse(text: &str) -> Option<Vec<Rule>> {
    varta_common::view::stylesheet::parse(text).ok()
}

/// Substitute `var(--name)` and `var(--name, fallback)` with the custom
/// properties in scope; `None` when a referenced property is missing and
/// has no fallback (the declaration is then invalid at computed-value time).
pub(crate) fn substitute(value: &str, custom: &BTreeMap<String, String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "var(".len()..];
        let end = after.find(')')?;
        let inner = &after[..end];
        let (name, fallback) = match inner.split_once(',') {
            Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
            None => (inner.trim(), None),
        };
        let name = name.strip_prefix("--")?;
        out.push_str(custom.get(name).map(String::as_str).or(fallback)?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_substitution_uses_fallbacks() {
        let custom = BTreeMap::from([("fg".to_string(), "#111".to_string())]);
        assert_eq!(
            substitute("1px solid var(--fg)", &custom).as_deref(),
            Some("1px solid #111")
        );
        assert_eq!(
            substitute("var(--bg, #fff)", &custom).as_deref(),
            Some("#fff")
        );
        assert_eq!(substitute("var(--bg)", &custom), None);
    }
}
