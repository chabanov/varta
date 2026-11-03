//! The checked stylesheet grammar of draft v0, shared by the headless
//! renderer (`varta-view`, which applies it at patch time) and the compiler
//! (which checks a statically known `View.stylesheet` text at build time).
//! A violation reports the zero-based UTF-8 byte offset where it was found.
use super::{CssProperty, FORBIDDEN_CSS, PSEUDO_CLASSES, css};

/// CSS pixels per `rem` (the initial font size).
pub const REM_PIXELS: f64 = 16.0;

/// A declared name: a subset property or a custom property (`--name`).
#[derive(Debug, Clone, PartialEq)]
pub enum Name {
    Property(CssProperty),
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pseudo {
    Hover,
    Active,
    FocusVisible,
    Disabled,
    Checked,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub root: bool,
    pub classes: Vec<String>,
    pub pseudo: Vec<Pseudo>,
}
impl Selector {
    /// Classes plus pseudo-classes; `:root` counts as one pseudo-class.
    #[must_use]
    pub fn specificity(&self) -> usize {
        self.classes.len() + self.pseudo.len() + usize::from(self.root)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Dark(bool),
    MinWidth(f64),
    MaxWidth(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<(Name, String)>,
    pub media: Vec<Condition>,
}

/// What an element exposes to selector matching.
pub struct Subject<'a> {
    pub root: bool,
    pub classes: &'a [&'a str],
    pub focused: bool,
    pub disabled: bool,
    pub checked: bool,
}

impl Rule {
    /// Canonical syntax/source cost of considering this rule, independent of
    /// media or pseudo-class state. Uses the parsed stylesheet node weight.
    #[must_use]
    pub fn cascade_work(&self) -> usize {
        let weight = super::DRAFT_STYLESHEET_NODE_WORK;
        let mut work = weight.saturating_mul(1 + self.media.len());
        for selector in &self.selectors {
            work = work.saturating_add(weight.saturating_mul(1 + selector.pseudo.len()));
            for class in &selector.classes {
                work = work.saturating_add(weight + class.len());
            }
        }
        for (name, value) in &self.declarations {
            work = work.saturating_add(weight + value.len());
            if let Name::Custom(name) = name {
                work = work.saturating_add(name.len());
            }
        }
        work
    }
    /// Whether every media condition holds for a viewport `width` in CSS
    /// pixels and colour scheme.
    #[must_use]
    pub fn applies(&self, width: f64, dark: bool) -> bool {
        self.media.iter().all(|condition| match condition {
            Condition::Dark(scheme) => dark == *scheme,
            Condition::MinWidth(min) => width >= *min,
            Condition::MaxWidth(max) => width <= *max,
        })
    }
    /// The highest specificity among the selectors that match `subject`.
    #[must_use]
    pub fn matching(&self, subject: &Subject<'_>) -> Option<usize> {
        self.matching_with_classes(subject, |class| subject.classes.contains(&class))
    }
    /// The same matcher with a caller-owned class membership index.
    #[must_use]
    pub fn matching_with_classes(
        &self,
        subject: &Subject<'_>,
        contains: impl Fn(&str) -> bool,
    ) -> Option<usize> {
        self.selectors
            .iter()
            .filter(|selector| {
                selector.root == subject.root
                    && (selector.root || !selector.classes.is_empty())
                    && selector.classes.iter().all(|class| contains(class))
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

/// What a stylesheet violates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    #[error("forbidden CSS token")]
    Forbidden,
    #[error("expected a selector (`.class`, `.a.b:pseudo` or `:root`)")]
    Selector,
    #[error("expected `{0}`")]
    Expected(&'static str),
    #[error("invalid @media condition (prefers-color-scheme, min-width or max-width)")]
    Media,
    #[error("invalid declaration: unknown property or value outside the subset grammar")]
    Declaration,
    #[error("empty declaration")]
    EmptyDeclaration,
    #[error("stylesheet work limit exceeded")]
    WorkLimit,
    #[error("unterminated rule")]
    Unterminated,
}

/// A stylesheet violation and the zero-based UTF-8 byte offset where it was
/// found in the stylesheet text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{violation} at byte {offset}")]
pub struct StylesheetError {
    pub violation: Violation,
    pub offset: usize,
}

type Parsed<T> = Result<T, StylesheetError>;

struct Parser {
    chars: Vec<char>,
    /// The byte offset of every char index, plus the end.
    offsets: Vec<usize>,
    at: usize,
    work: usize,
    allowance: usize,
}

/// Checked rules and their canonical parsed-work charge.
#[derive(Debug, Clone)]
pub struct ParsedStylesheet {
    pub rules: Vec<Rule>,
    pub work: usize,
}

impl Parser {
    fn charge(&mut self, work: usize) -> Parsed<()> {
        self.work = self.work.saturating_add(work);
        if self.work > self.allowance {
            return self.fail(Violation::WorkLimit);
        }
        Ok(())
    }
    fn node(&mut self) -> Parsed<()> {
        self.charge(super::DRAFT_STYLESHEET_NODE_WORK)
    }
    fn fail<T>(&self, violation: Violation) -> Parsed<T> {
        self.fail_at(self.at, violation)
    }
    fn fail_at<T>(&self, at: usize, violation: Violation) -> Parsed<T> {
        Err(StylesheetError {
            violation,
            offset: self.offsets[at.min(self.chars.len())],
        })
    }
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
    fn expect(&mut self, text: &'static str) -> Parsed<()> {
        self.skip();
        if self.eat(text) {
            Ok(())
        } else {
            self.fail(Violation::Expected(text))
        }
    }
    /// Characters up to (not including) the first of `stops`.
    fn until(&mut self, stops: &[char], violation: Violation) -> Parsed<String> {
        let start = self.at;
        while let Some(c) = self.peek() {
            if stops.contains(&c) {
                return Ok(self.chars[start..self.at].iter().collect());
            }
            self.at += 1;
        }
        self.fail_at(start, violation)
    }
    fn ident(&mut self) -> Parsed<String> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            self.at += 1;
        }
        let word: String = self.chars[start..self.at].iter().collect();
        if css::ident(&word) {
            Ok(word)
        } else {
            self.fail_at(start, Violation::Selector)
        }
    }
    fn sheet(&mut self) -> Parsed<Vec<Rule>> {
        let mut rules = Vec::new();
        loop {
            self.skip();
            if self.peek().is_none() {
                return Ok(rules);
            }
            if self.eat("@media") {
                let media = self.conditions()?;
                self.expect("{")?;
                loop {
                    self.skip();
                    if self.eat("}") {
                        break;
                    }
                    if self.peek().is_none() {
                        return self.fail(Violation::Unterminated);
                    }
                    rules.push(self.rule(&media)?);
                }
            } else {
                rules.push(self.rule(&[])?);
            }
        }
    }
    fn conditions(&mut self) -> Parsed<Vec<Condition>> {
        self.node()?;
        let mut conditions = vec![self.condition()?];
        loop {
            self.skip();
            if !self.eat("and") {
                return Ok(conditions);
            }
            self.node()?;
            conditions.push(self.condition()?);
        }
    }
    fn condition(&mut self) -> Parsed<Condition> {
        self.expect("(")?;
        self.skip();
        let start = self.at;
        let feature = self.until(&[':', ')'], Violation::Media)?;
        self.expect(":")?;
        let value = self.until(&[')'], Violation::Media)?;
        self.expect(")")?;
        let value = value.trim();
        let condition = match feature.trim() {
            "prefers-color-scheme" => match value {
                "light" => Some(Condition::Dark(false)),
                "dark" => Some(Condition::Dark(true)),
                _ => None,
            },
            "min-width" => length(value).map(Condition::MinWidth),
            "max-width" => length(value).map(Condition::MaxWidth),
            _ => None,
        };
        condition.map_or_else(|| self.fail_at(start, Violation::Media), Ok)
    }
    fn rule(&mut self, media: &[Condition]) -> Parsed<Rule> {
        self.charge(
            super::DRAFT_STYLESHEET_NODE_WORK.saturating_mul(media.len().saturating_add(1)),
        )?;
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
        let open = self.at;
        let body = self.until(&['}'], Violation::Unterminated)?;
        self.at += 1;
        let mut declarations = Vec::new();
        let parts: Vec<&str> = body.split(';').collect();
        let mut offset = self.offsets[open];
        for (index, part) in parts.iter().enumerate() {
            let start = offset + (part.len() - part.trim_start().len());
            offset += part.len() + ';'.len_utf8();
            if part.trim().is_empty() {
                // Only a final `;` may leave an empty declaration.
                if index + 1 == parts.len() && (index > 0 || parts.len() == 1) {
                    continue;
                }
                return Err(StylesheetError {
                    violation: Violation::EmptyDeclaration,
                    offset: start,
                });
            }
            let entry = declaration(part).ok_or(StylesheetError {
                violation: Violation::Declaration,
                offset: start,
            })?;
            self.node()?;
            self.charge(
                entry.1.len()
                    + match &entry.0 {
                        Name::Custom(name) => name.len(),
                        Name::Property(_) => 0,
                    },
            )?;
            declarations.push(entry);
        }
        Ok(Rule {
            selectors,
            declarations,
            media: media.to_vec(),
        })
    }
    fn selector(&mut self) -> Parsed<Selector> {
        self.node()?;
        self.skip();
        let start = self.at;
        if self.eat(":root") {
            if self.peek().is_some_and(|c| c == '.' || c == ':') {
                return self.fail_at(start, Violation::Selector);
            }
            return Ok(Selector {
                root: true,
                classes: Vec::new(),
                pseudo: Vec::new(),
            });
        }
        let mut classes = Vec::new();
        while self.eat(".") {
            self.node()?;
            let class = self.ident()?;
            self.charge(class.len())?;
            classes.push(class);
        }
        if classes.is_empty() {
            return self.fail_at(start, Violation::Selector);
        }
        let mut pseudo = Vec::new();
        while self.peek() == Some(':') {
            self.node()?;
            let Some(found) = PSEUDO_CLASSES.iter().position(|name| self.starts(name)) else {
                return self.fail(Violation::Selector);
            };
            self.at += PSEUDO_CLASSES[found].chars().count();
            pseudo.push(match PSEUDO_CLASSES[found] {
                ":hover" => Pseudo::Hover,
                ":active" => Pseudo::Active,
                ":focus-visible" => Pseudo::FocusVisible,
                ":disabled" => Pseudo::Disabled,
                ":checked" => Pseudo::Checked,
                _ => return self.fail(Violation::Selector),
            });
        }
        if self
            .peek()
            .is_some_and(|c| !c.is_whitespace() && c != ',' && c != '{')
        {
            return self.fail(Violation::Selector);
        }
        Ok(Selector {
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

/// The first forbidden token of `text` (ASCII case-insensitively).
fn forbidden(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    FORBIDDEN_CSS
        .iter()
        .filter_map(|token| lower.find(token))
        .min()
}

/// Parse a whole stylesheet: the grammar, forbidden tokens and every
/// declaration against its property grammar.
///
/// # Errors
/// The first violation, with its byte offset.
pub fn parse(text: &str) -> Result<Vec<Rule>, StylesheetError> {
    parse_with_work_limit(text, super::DRAFT_MAX_STYLESHEET_WORK).map(|parsed| parsed.rules)
}

/// Parse within a remaining transaction allowance, capped by the shared limit.
/// The counter is checked before copying media conditions into another rule.
pub fn parse_with_work_limit(
    text: &str,
    allowance: usize,
) -> Result<ParsedStylesheet, StylesheetError> {
    let allowance = allowance.min(super::DRAFT_MAX_STYLESHEET_WORK);
    if text.len() > allowance {
        return Err(StylesheetError {
            violation: Violation::WorkLimit,
            offset: 0,
        });
    }
    if let Some(offset) = forbidden(text) {
        return Err(StylesheetError {
            violation: Violation::Forbidden,
            offset,
        });
    }
    let chars: Vec<char> = text.chars().collect();
    let offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut parser = Parser {
        chars,
        offsets,
        at: 0,
        work: text.len(),
        allowance,
    };
    let rules = parser.sheet()?;
    Ok(ParsedStylesheet {
        rules,
        work: parser.work,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_expansion_is_charged_before_cloning_and_exact_work_is_admitted() {
        let source = ".a{}";
        let parsed =
            parse_with_work_limit(source, super::super::DRAFT_MAX_STYLESHEET_WORK).unwrap();
        assert_eq!(
            parsed.work,
            source.len() + 3 * super::super::DRAFT_STYLESHEET_NODE_WORK + 1
        );
        assert!(parse_with_work_limit(source, parsed.work).is_ok());
        assert_eq!(
            parse_with_work_limit(source, parsed.work - 1)
                .unwrap_err()
                .violation,
            Violation::WorkLimit
        );
        let media = format!(
            "@media {}{{{}}}",
            vec!["(min-width:0px)"; 500].join(" and "),
            ".a{}".repeat(1100)
        );
        assert!(media.len() < super::super::DRAFT_MAX_VALUE_BYTES / 2);
        assert_eq!(parse(&media).unwrap_err().violation, Violation::WorkLimit);
    }

    #[test]
    fn indexed_class_membership_keeps_comma_and_duplicate_specificity() {
        let rules = parse(".a.a:checked, .a.b { color:#abc }").unwrap();
        for classes in [vec!["a", "a"], vec!["a", "b"], vec!["b"], vec![]] {
            let index: std::collections::BTreeSet<_> = classes.iter().copied().collect();
            for checked in [false, true] {
                let subject = Subject {
                    root: false,
                    classes: &classes,
                    focused: false,
                    disabled: false,
                    checked,
                };
                assert_eq!(
                    rules[0].matching(&subject),
                    rules[0].matching_with_classes(&subject, |name| index.contains(name))
                );
            }
        }
    }

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
        assert!(!rules[2].applies(1024.0, false));
        assert!(rules[2].applies(800.0, true));
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
            "@media (prefers-color-scheme: dark) { .a { color: #fff }",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
        assert_eq!(parse("").unwrap().len(), 0);
        assert_eq!(parse(".a { }").unwrap()[0].declarations.len(), 0);
    }

    #[test]
    fn violations_carry_utf8_byte_offsets() {
        let error = |text: &str| parse(text).unwrap_err();
        assert_eq!(
            error(".a { color: #fff; colour: red }"),
            StylesheetError {
                violation: Violation::Declaration,
                offset: 18
            }
        );
        // A multi-byte character before the violation counts in bytes.
        let text = ":root { --label: x }\n/* é */";
        assert_eq!(error(text).offset, text.find("/*").unwrap());
        let text = ".é { color: #fff }";
        assert_eq!(error(text).violation, Violation::Selector);
        assert_eq!(error(text).offset, 1);
        assert_eq!(error(".a { color: #fff").violation, Violation::Unterminated);
        assert_eq!(
            error("@media (orientation: landscape) { }").violation,
            Violation::Media
        );
        assert_eq!(error("@media (orientation: landscape) { }").offset, 8);
    }
}
