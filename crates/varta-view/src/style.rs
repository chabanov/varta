//! Stylesheets of draft v0: the checked stylesheet grammar and the cascade
//! subset (sheets by ascending id, then specificity, then source order;
//! inline `set_style` wins; custom properties inherit and `var()` is
//! substituted). Only custom properties inherit in the headless output; a
//! property's computed value is its cascaded value.
use std::collections::BTreeMap;
use varta_common::view::{CssProperty, PSEUDO_CLASSES, css};

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
/// CSS pixels per `rem` (the initial font size).
pub(crate) const REM_PIXELS: f64 = 16.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Name {
    Property(CssProperty),
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pseudo {
    Hover,
    Active,
    FocusVisible,
    Disabled,
    Checked,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Selector {
    pub root: bool,
    pub classes: Vec<String>,
    pub pseudo: Vec<Pseudo>,
}
impl Selector {
    /// Classes plus pseudo-classes; `:root` counts as one pseudo-class.
    pub(crate) fn specificity(&self) -> usize {
        self.classes.len() + self.pseudo.len() + usize::from(self.root)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Condition {
    Dark(bool),
    MinWidth(f64),
    MaxWidth(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<(Name, String)>,
    media: Vec<Condition>,
}

/// What an element exposes to selector matching.
pub(crate) struct Subject<'a> {
    pub root: bool,
    pub classes: &'a [&'a str],
    pub focused: bool,
    pub disabled: bool,
    pub checked: bool,
}

impl Rule {
    pub(crate) fn applies(&self, environment: &Environment) -> bool {
        self.media.iter().all(|condition| match condition {
            Condition::Dark(dark) => environment.dark == *dark,
            Condition::MinWidth(width) => environment.width >= *width,
            Condition::MaxWidth(width) => environment.width <= *width,
        })
    }
    /// The highest specificity among the selectors that match `subject`.
    pub(crate) fn matching(&self, subject: &Subject<'_>) -> Option<usize> {
        self.selectors
            .iter()
            .filter(|selector| {
                selector.root == subject.root
                    && (selector.root || !selector.classes.is_empty())
                    && selector
                        .classes
                        .iter()
                        .all(|class| subject.classes.contains(&class.as_str()))
                    && selector.pseudo.iter().all(|pseudo| match pseudo {
                        Pseudo::Hover | Pseudo::Active => false,
                        Pseudo::FocusVisible => subject.focused,
                        Pseudo::Disabled => subject.disabled,
                        Pseudo::Checked => subject.checked,
                    })
            })
            .map(Selector::specificity)
            .max()
    }
}

struct Parser<'a> {
    chars: Vec<char>,
    at: usize,
    _text: &'a str,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }
    fn starts(&self, text: &str) -> bool {
        (self.at..)
            .zip(text.chars())
            .all(|(at, c)| self.chars.get(at) == Some(&c))
    }
    fn eat(&mut self, text: &str) -> bool {
        if self.starts(text) {
            self.at += text.chars().count();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, text: &str) -> Option<()> {
        self.skip();
        self.eat(text).then_some(())
    }
    /// Characters up to (not including) the first of `stops`.
    fn until(&mut self, stops: &[char]) -> Option<String> {
        let start = self.at;
        while let Some(c) = self.peek() {
            if stops.contains(&c) {
                return Some(self.chars[start..self.at].iter().collect());
            }
            self.at += 1;
        }
        None
    }
    fn ident(&mut self) -> Option<String> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            self.at += 1;
        }
        let word: String = self.chars[start..self.at].iter().collect();
        css::ident(&word).then_some(word)
    }
    fn sheet(&mut self) -> Option<Vec<Rule>> {
        let mut rules = Vec::new();
        loop {
            self.skip();
            if self.peek().is_none() {
                return Some(rules);
            }
            if self.eat("@media") {
                let media = self.conditions()?;
                self.expect("{")?;
                loop {
                    self.skip();
                    if self.eat("}") {
                        break;
                    }
                    rules.push(self.rule(media.clone())?);
                }
            } else {
                rules.push(self.rule(Vec::new())?);
            }
        }
    }
    fn conditions(&mut self) -> Option<Vec<Condition>> {
        let mut conditions = vec![self.condition()?];
        loop {
            self.skip();
            if !self.eat("and") {
                return Some(conditions);
            }
            conditions.push(self.condition()?);
        }
    }
    fn condition(&mut self) -> Option<Condition> {
        self.expect("(")?;
        self.skip();
        let feature = self.until(&[':', ')'])?;
        self.expect(":")?;
        let value = self.until(&[')'])?;
        self.expect(")")?;
        let value = value.trim();
        match feature.trim() {
            "prefers-color-scheme" => match value {
                "light" => Some(Condition::Dark(false)),
                "dark" => Some(Condition::Dark(true)),
                _ => None,
            },
            "min-width" => Some(Condition::MinWidth(length(value)?)),
            "max-width" => Some(Condition::MaxWidth(length(value)?)),
            _ => None,
        }
    }
    fn rule(&mut self, media: Vec<Condition>) -> Option<Rule> {
        let mut selectors = vec![self.selector()?];
        loop {
            self.skip();
            if self.eat(",") {
                selectors.push(self.selector()?);
            } else {
                break;
            }
        }
        self.expect("{")?;
        let body = self.until(&['}'])?;
        self.at += 1;
        let mut declarations = Vec::new();
        let parts: Vec<&str> = body.split(';').collect();
        for (index, part) in parts.iter().enumerate() {
            if part.trim().is_empty() {
                // Only a final `;` may leave an empty declaration.
                if index + 1 == parts.len() && (index > 0 || parts.len() == 1) {
                    continue;
                }
                return None;
            }
            declarations.push(declaration(part)?);
        }
        Some(Rule {
            selectors,
            declarations,
            media,
        })
    }
    fn selector(&mut self) -> Option<Selector> {
        self.skip();
        if self.eat(":root") {
            if self.peek().is_some_and(|c| c == '.' || c == ':') {
                return None;
            }
            return Some(Selector {
                root: true,
                classes: Vec::new(),
                pseudo: Vec::new(),
            });
        }
        let mut classes = Vec::new();
        while self.eat(".") {
            classes.push(self.ident()?);
        }
        if classes.is_empty() {
            return None;
        }
        let mut pseudo = Vec::new();
        while self.peek() == Some(':') {
            let found = PSEUDO_CLASSES.iter().position(|name| self.starts(name))?;
            self.at += PSEUDO_CLASSES[found].chars().count();
            pseudo.push(match PSEUDO_CLASSES[found] {
                ":hover" => Pseudo::Hover,
                ":active" => Pseudo::Active,
                ":focus-visible" => Pseudo::FocusVisible,
                ":disabled" => Pseudo::Disabled,
                ":checked" => Pseudo::Checked,
                _ => return None,
            });
        }
        if self
            .peek()
            .is_some_and(|c| !c.is_whitespace() && c != ',' && c != '{')
        {
            return None;
        }
        Some(Selector {
            root: false,
            classes,
            pseudo,
        })
    }
}

/// A `<length>` in CSS pixels.
fn length(text: &str) -> Option<f64> {
    if text == "0" {
        return Some(0.0);
    }
    let (number, scale) = if let Some(number) = text.strip_suffix("px") {
        (number, 1.0)
    } else {
        (text.strip_suffix("rem")?, REM_PIXELS)
    };
    css::matches("<number>", number)
        .then(|| number.parse::<f64>().ok())
        .flatten()
        .map(|value| value * scale)
}

fn declaration(text: &str) -> Option<(Name, String)> {
    let (name, value) = text.split_once(':')?;
    let (name, value) = (name.trim(), value.trim());
    if let Some(custom) = name.strip_prefix("--") {
        return (css::ident(custom) && css::custom_value(value))
            .then(|| (Name::Custom(custom.into()), value.into()));
    }
    let property = CssProperty::ALL
        .iter()
        .copied()
        .find(|property| property.name() == name)?;
    css::property_value(property, value).then(|| (Name::Property(property), value.into()))
}

/// Parse a whole stylesheet; `None` for any violation of the grammar, a
/// forbidden token or an invalid declaration.
pub(crate) fn parse(text: &str) -> Option<Vec<Rule>> {
    if css::forbidden(text) {
        return None;
    }
    Parser {
        chars: text.chars().collect(),
        at: 0,
        _text: text,
    }
    .sheet()
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
    fn the_stylesheet_grammar_is_checked() {
        let rules = parse(
            ":root { --fg: #111; --gap: 8px }
             .row.active:checked, .x { color: var(--fg); padding: 4px 8px; }
             @media (prefers-color-scheme: dark) and (min-width: 40rem) { .row { color: #eee } }",
        )
        .unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[1].selectors[0].specificity(), 3);
        assert!(!rules[2].applies(&Environment::default()));
        assert!(rules[2].applies(&Environment {
            width: 800.0,
            dark: true
        }));
        for invalid in [
            "div { color: red }",
            ".a > .b { color: #fff }",
            ".a { color: red }",
            ".a { colour: #fff }",
            ".a { color: #fff !important }",
            ".a { color: #fff } /* x */",
            ".a { background-color: url(x) }",
            "@import \"x\";",
            ".a:nth-child(2) { color: #fff }",
            ".a { color: #fff",
            ".a { ; color: #fff }",
            "@media (orientation: landscape) { .a { color: #fff } }",
            ":root.a { --x: 1 }",
        ] {
            assert!(parse(invalid).is_none(), "{invalid}");
        }
        assert_eq!(parse("").unwrap().len(), 0);
        assert_eq!(parse(".a { }").unwrap()[0].declarations.len(), 0);
    }

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
