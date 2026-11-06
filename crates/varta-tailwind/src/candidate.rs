//! Candidate and variant parsing (port of Tailwind's `candidate.ts`).
//!
//! A candidate is one class token (`hover:bg-red-500/50`): variants, a root,
//! an optional value and modifier, and an important flag. Parsing yields every
//! interpretation the design system admits, in Tailwind's order.
use crate::design_system::{DesignSystem, UtilityKind, VariantKind};
use crate::utils::decode_arbitrary_value::decode_arbitrary_value;
use crate::utils::is_valid_arbitrary::is_valid_arbitrary;
use crate::utils::segment;

/// A modifier after `/`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateModifier {
    /// `/[0.5]` or `/(--x)` (as `var(--x)`).
    Arbitrary(String),
    /// `/50`.
    Named(String),
}

impl CandidateModifier {
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            CandidateModifier::Arbitrary(value) | CandidateModifier::Named(value) => value,
        }
    }
    #[must_use]
    pub fn is_arbitrary(&self) -> bool {
        matches!(self, CandidateModifier::Arbitrary(_))
    }
}

/// The value of a functional utility.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum UtilityValue {
    /// `bg-[#fff]`, `text-[length:var(--x)]`.
    Arbitrary {
        data_type: Option<String>,
        value: String,
    },
    /// `bg-red-500`; `fraction` is `value/modifier` when a modifier exists.
    Named {
        value: String,
        fraction: Option<String>,
    },
}

impl UtilityValue {
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            UtilityValue::Arbitrary { value, .. } | UtilityValue::Named { value, .. } => value,
        }
    }
}

/// The value of a functional variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VariantValue {
    Arbitrary(String),
    Named(String),
}

impl VariantValue {
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            VariantValue::Arbitrary(value) | VariantValue::Named(value) => value,
        }
    }
}

/// One parsed variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Variant {
    /// `[&_p]`, `[@media(…)]`; `relative` when it starts with `>`, `+` or `~`.
    Arbitrary { selector: String, relative: bool },
    /// `hover`.
    Static { root: String },
    /// `data-[state=open]`, `min-[900px]`, `aria-checked`.
    Functional {
        root: String,
        value: Option<VariantValue>,
        modifier: Option<CandidateModifier>,
    },
    /// `group-hover/name`, `not-first`, `has-checked`.
    Compound {
        root: String,
        modifier: Option<CandidateModifier>,
        variant: Box<Variant>,
    },
}

impl Variant {
    /// The registered root; `None` for an arbitrary variant.
    #[must_use]
    pub fn root(&self) -> Option<&str> {
        match self {
            Variant::Arbitrary { .. } => None,
            Variant::Static { root }
            | Variant::Functional { root, .. }
            | Variant::Compound { root, .. } => Some(root),
        }
    }
    /// The modifier of a functional or compound variant.
    #[must_use]
    pub fn modifier(&self) -> Option<&CandidateModifier> {
        match self {
            Variant::Functional { modifier, .. } | Variant::Compound { modifier, .. } => {
                modifier.as_ref()
            }
            Variant::Arbitrary { .. } | Variant::Static { .. } => None,
        }
    }
    /// The value of a functional variant.
    #[must_use]
    pub fn value(&self) -> Option<&VariantValue> {
        match self {
            Variant::Functional { value, .. } => value.as_ref(),
            _ => None,
        }
    }
}

/// The kind-specific part of a candidate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateKind {
    /// `[color:red]`.
    Arbitrary {
        property: String,
        value: String,
        modifier: Option<CandidateModifier>,
    },
    /// `flex`.
    Static { root: String },
    /// `bg-red-500`.
    Functional {
        root: String,
        value: Option<UtilityValue>,
        modifier: Option<CandidateModifier>,
    },
}

/// One interpretation of a class token.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate {
    pub kind: CandidateKind,
    /// Variants, innermost (closest to the utility) first.
    pub variants: Vec<Variant>,
    pub important: bool,
    /// The class token as written.
    pub raw: String,
}

impl Candidate {
    /// The utility root (`bg`), or the property of an arbitrary property.
    #[must_use]
    pub fn root(&self) -> &str {
        match &self.kind {
            CandidateKind::Arbitrary { property, .. } => property,
            CandidateKind::Static { root } | CandidateKind::Functional { root, .. } => root,
        }
    }
    /// The value of a functional candidate.
    #[must_use]
    pub fn value(&self) -> Option<&UtilityValue> {
        match &self.kind {
            CandidateKind::Functional { value, .. } => value.as_ref(),
            _ => None,
        }
    }
    /// The modifier of a functional or arbitrary candidate.
    #[must_use]
    pub fn modifier(&self) -> Option<&CandidateModifier> {
        match &self.kind {
            CandidateKind::Functional { modifier, .. }
            | CandidateKind::Arbitrary { modifier, .. } => modifier.as_ref(),
            CandidateKind::Static { .. } => None,
        }
    }
    /// The utility kind this candidate is compiled by.
    #[must_use]
    pub fn utility_kind(&self) -> Option<UtilityKind> {
        match &self.kind {
            CandidateKind::Arbitrary { .. } => None,
            CandidateKind::Static { .. } => Some(UtilityKind::Static),
            CandidateKind::Functional { .. } => Some(UtilityKind::Functional),
        }
    }
}

fn is_valid_named_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '%' | '-'))
}

fn decode_valid_arbitrary_value(raw: &str) -> Option<String> {
    is_valid_arbitrary(raw).then(|| decode_arbitrary_value(raw))
}

fn is_blank(value: &str) -> bool {
    value.is_empty() || crate::utils::js_number::js_trim(value).is_empty()
}

/// Parse a class token into every candidate the design system admits.
#[must_use]
pub fn parse_candidate(input: &str, ds: &DesignSystem) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut raw_variants = segment(input, ':');
    if let Some(prefix) = &ds.theme.prefix {
        if raw_variants.len() == 1 || raw_variants[0] != *prefix {
            return out;
        }
        raw_variants.remove(0);
    }
    let Some(mut base) = raw_variants.pop() else {
        return out;
    };
    let mut variants = Vec::with_capacity(raw_variants.len());
    for raw in raw_variants.iter().rev() {
        let Some(variant) = ds.parse_variant(raw) else {
            return out;
        };
        variants.push(variant);
    }

    let mut important = false;
    if let Some(stripped) = base.strip_suffix('!') {
        important = true;
        base = stripped.to_owned();
    } else if let Some(stripped) = base.strip_prefix('!') {
        important = true;
        base = stripped.to_owned();
    }

    if ds.utilities.has(&base, UtilityKind::Static) && !base.contains('[') {
        out.push(Candidate {
            kind: CandidateKind::Static { root: base.clone() },
            variants: variants.clone(),
            important,
            raw: input.to_owned(),
        });
    }

    let parts = segment(&base, '/');
    if parts.len() > 2 {
        return out;
    }
    let mut base_without_modifier = parts[0].clone();
    let modifier_segment = parts.get(1).cloned();
    let parsed_modifier = match &modifier_segment {
        Some(segment) => match parse_modifier(segment) {
            Some(modifier) => Some(modifier),
            None => return out,
        },
        None => None,
    };

    if base_without_modifier.starts_with('[') {
        if !base_without_modifier.ends_with(']') {
            return out;
        }
        let second = base_without_modifier.chars().nth(1);
        if !matches!(second, Some(c) if c == '-' || c.is_ascii_lowercase()) {
            return out;
        }
        base_without_modifier =
            base_without_modifier[1..base_without_modifier.len() - 1].to_owned();
        let Some(index) = base_without_modifier.find(':') else {
            return out;
        };
        if index == 0 || index == base_without_modifier.len() - 1 {
            return out;
        }
        let property = base_without_modifier[..index].to_owned();
        let Some(value) = decode_valid_arbitrary_value(&base_without_modifier[index + 1..]) else {
            return out;
        };
        out.push(Candidate {
            kind: CandidateKind::Arbitrary {
                property,
                value,
                modifier: parsed_modifier,
            },
            variants,
            important,
            raw: input.to_owned(),
        });
        return out;
    }

    let roots: Vec<(String, Option<String>)> = if base_without_modifier.ends_with(']') {
        let Some(index) = base_without_modifier.find("-[") else {
            return out;
        };
        let root = &base_without_modifier[..index];
        if !ds.utilities.has(root, UtilityKind::Functional) {
            return out;
        }
        vec![(
            root.to_owned(),
            Some(base_without_modifier[index + 1..].to_owned()),
        )]
    } else if base_without_modifier.ends_with(')') {
        let Some(index) = base_without_modifier.find("-(") else {
            return out;
        };
        let root = &base_without_modifier[..index];
        if !ds.utilities.has(root, UtilityKind::Functional) {
            return out;
        }
        let mut value =
            base_without_modifier[index + 2..base_without_modifier.len() - 1].to_owned();
        let parts = segment(&value, ':');
        let mut data_type = None;
        if parts.len() == 2 {
            data_type = Some(parts[0].clone());
            value = parts[1].clone();
        }
        if !value.starts_with("--") {
            return out;
        }
        if !is_valid_arbitrary(&value) {
            return out;
        }
        let wrapped = match data_type {
            None => format!("[var({value})]"),
            Some(data_type) => format!("[{data_type}:var({value})]"),
        };
        vec![(root.to_owned(), Some(wrapped))]
    } else {
        find_roots(&base_without_modifier, |root| {
            ds.utilities.has(root, UtilityKind::Functional)
        })
    };

    for (root, value) in roots {
        let mut candidate_value = None;
        if let Some(value) = value {
            if let Some(start) = value.find('[') {
                if !value.ends_with(']') {
                    return out;
                }
                let Some(mut arbitrary) =
                    decode_valid_arbitrary_value(&value[start + 1..value.len() - 1])
                else {
                    continue;
                };
                let mut typehint: Option<String> = None;
                for (i, c) in arbitrary.char_indices() {
                    if c == ':' {
                        typehint = Some(arbitrary[..i].to_owned());
                        arbitrary = arbitrary[i + 1..].to_owned();
                        break;
                    }
                    if c == '-' || c.is_ascii_lowercase() {
                        continue;
                    }
                    break;
                }
                if is_blank(&arbitrary) {
                    continue;
                }
                if typehint.as_deref() == Some("") {
                    continue;
                }
                candidate_value = Some(UtilityValue::Arbitrary {
                    data_type: typehint,
                    value: arbitrary,
                });
            } else {
                let fraction = match (&modifier_segment, &parsed_modifier) {
                    (None, _) | (_, Some(CandidateModifier::Arbitrary(_))) => None,
                    (Some(segment), _) => Some(format!("{value}/{segment}")),
                };
                if !is_valid_named_value(&value) {
                    continue;
                }
                candidate_value = Some(UtilityValue::Named { value, fraction });
            }
        }
        out.push(Candidate {
            kind: CandidateKind::Functional {
                root,
                value: candidate_value,
                modifier: parsed_modifier.clone(),
            },
            variants: variants.clone(),
            important,
            raw: input.to_owned(),
        });
    }
    out
}

fn parse_modifier(modifier: &str) -> Option<CandidateModifier> {
    if modifier.starts_with('[') && modifier.ends_with(']') && modifier.len() >= 2 {
        let value = decode_valid_arbitrary_value(&modifier[1..modifier.len() - 1])?;
        if is_blank(&value) {
            return None;
        }
        return Some(CandidateModifier::Arbitrary(value));
    }
    if modifier.starts_with('(') && modifier.ends_with(')') && modifier.len() >= 2 {
        let inner = &modifier[1..modifier.len() - 1];
        if !inner.starts_with("--") {
            return None;
        }
        if !is_valid_arbitrary(inner) {
            return None;
        }
        return Some(CandidateModifier::Arbitrary(decode_arbitrary_value(
            &format!("var({inner})"),
        )));
    }
    if !is_valid_named_value(modifier) {
        return None;
    }
    Some(CandidateModifier::Named(modifier.to_owned()))
}

/// Parse one variant (port of `parseVariant`).
#[must_use]
pub fn parse_variant(variant: &str, ds: &DesignSystem) -> Option<Variant> {
    if variant.starts_with('[') && variant.ends_with(']') && variant.len() >= 2 {
        if variant[1..].starts_with('@') && variant.contains('&') {
            return None;
        }
        let mut selector = decode_valid_arbitrary_value(&variant[1..variant.len() - 1])?;
        if is_blank(&selector) {
            return None;
        }
        let relative = matches!(selector.chars().next(), Some('>' | '+' | '~'));
        if !relative && !selector.starts_with('@') && !selector.contains('&') {
            selector = format!("&:is({selector})");
        }
        return Some(Variant::Arbitrary { selector, relative });
    }

    let parts = segment(variant, '/');
    if parts.len() > 2 {
        return None;
    }
    let without_modifier = &parts[0];
    let mut modifier = parts.get(1).cloned();
    let roots = find_roots(without_modifier, |root| ds.variants.has(root));
    for (root, value) in roots {
        match ds.variants.kind(&root)? {
            VariantKind::Static => {
                if value.is_some() || modifier.is_some() {
                    return None;
                }
                return Some(Variant::Static { root });
            }
            VariantKind::Functional => {
                let parsed_modifier = match &modifier {
                    Some(m) => Some(parse_modifier(m)?),
                    None => None,
                };
                let Some(value) = value else {
                    return Some(Variant::Functional {
                        root,
                        modifier: parsed_modifier,
                        value: None,
                    });
                };
                if value.ends_with(']') {
                    if !value.starts_with('[') {
                        continue;
                    }
                    let arbitrary = decode_valid_arbitrary_value(&value[1..value.len() - 1])?;
                    if is_blank(&arbitrary) {
                        return None;
                    }
                    return Some(Variant::Functional {
                        root,
                        modifier: parsed_modifier,
                        value: Some(VariantValue::Arbitrary(arbitrary)),
                    });
                }
                if value.ends_with(')') {
                    if !value.starts_with('(') {
                        continue;
                    }
                    let arbitrary = decode_valid_arbitrary_value(&value[1..value.len() - 1])?;
                    if is_blank(&arbitrary) {
                        return None;
                    }
                    if !arbitrary.starts_with("--") {
                        return None;
                    }
                    return Some(Variant::Functional {
                        root,
                        modifier: parsed_modifier,
                        value: Some(VariantValue::Arbitrary(format!("var({arbitrary})"))),
                    });
                }
                if !is_valid_named_value(&value) {
                    continue;
                }
                return Some(Variant::Functional {
                    root,
                    modifier: parsed_modifier,
                    value: Some(VariantValue::Named(value)),
                });
            }
            VariantKind::Compound => {
                let mut value = value?;
                if let Some(m) = &modifier
                    && matches!(root.as_str(), "not" | "has" | "in")
                {
                    value = format!("{value}/{m}");
                    modifier = None;
                }
                let sub = ds.parse_variant(&value)?;
                if !ds.variants.compounds_with(&root, &sub) {
                    return None;
                }
                let parsed_modifier = match &modifier {
                    Some(m) => Some(parse_modifier(m)?),
                    None => None,
                };
                return Some(Variant::Compound {
                    root,
                    modifier: parsed_modifier,
                    variant: Box::new(sub),
                });
            }
        }
    }
    None
}

/// Every split of `input` into an existing root and a value, longest root
/// first (port of `findRoots`).
fn find_roots(input: &str, exists: impl Fn(&str) -> bool) -> Vec<(String, Option<String>)> {
    let mut roots = Vec::new();
    if exists(input) {
        roots.push((input.to_owned(), None));
    }
    let mut index = input.rfind('-');
    while let Some(idx) = index {
        if idx == 0 {
            break;
        }
        let maybe_root = &input[..idx];
        if exists(maybe_root) {
            let value = &input[idx + 1..];
            if value.is_empty() {
                break;
            }
            if maybe_root == "@" && exists("@") {
                break;
            }
            roots.push((maybe_root.to_owned(), Some(value.to_owned())));
        }
        index = input[..idx].rfind('-');
    }
    if input.starts_with('@') && exists("@") {
        roots.push(("@".to_owned(), Some(input[1..].to_owned())));
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_found_longest_first() {
        let known = ["bg", "bg-red", "@"];
        let exists = |root: &str| known.contains(&root);
        assert_eq!(
            find_roots("bg-red-500", exists),
            vec![
                ("bg-red".to_owned(), Some("500".to_owned())),
                ("bg".to_owned(), Some("red-500".to_owned()))
            ]
        );
        assert_eq!(find_roots("bg-", exists), vec![]);
        assert_eq!(
            find_roots("@md", exists),
            vec![("@".to_owned(), Some("md".to_owned()))]
        );
    }

    #[test]
    fn modifiers_parse() {
        assert_eq!(
            parse_modifier("50"),
            Some(CandidateModifier::Named("50".into()))
        );
        assert_eq!(
            parse_modifier("[0.5]"),
            Some(CandidateModifier::Arbitrary("0.5".into()))
        );
        assert_eq!(
            parse_modifier("(--alpha)"),
            Some(CandidateModifier::Arbitrary("var(--alpha)".into()))
        );
        assert_eq!(parse_modifier("(alpha)"), None);
        assert_eq!(parse_modifier("a b"), None);
    }
}
