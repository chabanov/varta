//! The input pseudo-classes of the view's form controls (Selectors 4 §12
//! with HTML §4.16.3): mutability, placeholders, defaults, indeterminate
//! radio groups and progress bars, and constraint validation (HTML
//! §4.10.20.3) for the subset's control types. The subset has no `required`,
//! `pattern`, `min`, `max`, `step` or `minlength` attributes, so a control
//! is invalid only for a number field's bad input or step mismatch (the
//! default step 1, HTML §4.10.5.1.12, from the step base of its `value`
//! attribute).
use crate::Document;
use crate::prelude::*;
use varta_common::view::Tag;

/// The input types whose value the user edits as text (`readonly` and
/// `placeholder` apply, HTML §4.10.5.3.3 and §4.10.5.3.10).
const TEXT_TYPES: &[&str] = &["text", "password", "number", "search"];

fn input_type(document: &impl Document, id: u32) -> &str {
    document.attribute(id, "type").unwrap_or("text")
}

fn tag(document: &impl Document, id: u32) -> Option<Tag> {
    document.element(id).map(|element| element.tag)
}

/// Whether `id` is an `input` the user edits as text or a `textarea`.
fn text_control(document: &impl Document, id: u32) -> bool {
    match tag(document, id) {
        Some(Tag::Input) => TEXT_TYPES
            .iter()
            .any(|kind| input_type(document, id).eq_ignore_ascii_case(kind)),
        Some(Tag::Textarea) => true,
        _ => false,
    }
}

fn disabled(document: &impl Document, id: u32) -> bool {
    document.element(id).is_some_and(|element| element.disabled)
}

/// `:read-write` (HTML §4.16.3): a text control that is neither read-only
/// nor disabled. Every other element is `:read-only`.
pub(crate) fn read_write(document: &impl Document, id: u32) -> bool {
    text_control(document, id)
        && document.attribute(id, "readonly").is_none()
        && !disabled(document, id)
}

/// `:placeholder-shown`: a text control with a nonempty placeholder (line
/// breaks removed, HTML §4.10.5.3.10) and an empty value.
pub(crate) fn placeholder_shown(document: &impl Document, id: u32) -> bool {
    text_control(document, id)
        && document
            .attribute(id, "placeholder")
            .is_some_and(|text| text.chars().any(|c| !matches!(c, '\r' | '\n')))
        && document.value(id).is_none_or(str::is_empty)
}

/// The nearest `form` ancestor (the form owner without a `form`
/// attribute, HTML §4.10.17.3).
fn form_owner(document: &impl Document, id: u32) -> Option<u32> {
    let mut at = document.parent(id);
    while let Some(node) = at {
        if tag(document, node) == Some(Tag::Form) {
            return Some(node);
        }
        at = document.parent(node);
    }
    None
}

/// Elements of `root`'s subtree in tree order, `root` excluded.
fn descendants(document: &impl Document, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut pending: Vec<u32> = document.children(root).iter().rev().copied().collect();
    while let Some(id) = pending.pop() {
        if document.element(id).is_some() {
            out.push(id);
        }
        pending.extend(document.children(id).iter().rev());
    }
    out
}

/// A `button` whose type is `submit` (the missing value default).
fn submit_button(document: &impl Document, id: u32) -> bool {
    tag(document, id) == Some(Tag::Button)
        && document
            .attribute(id, "type")
            .is_none_or(|kind| kind.eq_ignore_ascii_case("submit"))
}

/// `:default` (HTML §4.16.3): an `option` with `selected`, a checkable
/// input with `checked` (no such attribute in the subset), and the default
/// button of a form (its first submit button in tree order).
pub(crate) fn default(document: &impl Document, id: u32) -> bool {
    match tag(document, id) {
        Some(Tag::Option) => document.attribute(id, "selected").is_some(),
        Some(Tag::Input) => document.attribute(id, "checked").is_some(),
        Some(Tag::Button) if submit_button(document, id) => {
            form_owner(document, id).and_then(|form| {
                descendants(document, form)
                    .into_iter()
                    .find(|button| submit_button(document, *button))
            }) == Some(id)
        }
        _ => false,
    }
}

fn radio(document: &impl Document, id: u32) -> bool {
    tag(document, id) == Some(Tag::Input) && input_type(document, id).eq_ignore_ascii_case("radio")
}

fn tree_root(document: &impl Document, id: u32) -> u32 {
    let mut root = id;
    while let Some(parent) = document.parent(root) {
        root = parent;
    }
    root
}

/// `:indeterminate` (HTML §4.16.3): a radio button whose group has no
/// checked button, and a `progress` without a `value`.
pub(crate) fn indeterminate(document: &impl Document, id: u32) -> bool {
    match tag(document, id) {
        Some(Tag::Progress) => document.attribute(id, "value").is_none(),
        Some(Tag::Input) if radio(document, id) => {
            let checked = |node: u32| document.element(node).is_some_and(|e| e.checked);
            // HTML §4.10.5.1.16: the same tree, form owner and nonempty name.
            let name = document
                .attribute(id, "name")
                .filter(|name| !name.is_empty());
            let Some(name) = name else {
                return !checked(id);
            };
            let owner = form_owner(document, id);
            let root = tree_root(document, id);
            let mut group = descendants(document, root);
            group.push(root);
            !group.into_iter().any(|peer| {
                radio(document, peer)
                    && document.attribute(peer, "name") == Some(name)
                    && form_owner(document, peer) == owner
                    && checked(peer)
            })
        }
        _ => false,
    }
}

/// The validity of `id` (HTML §4.10.20.3): `None` when it is not a
/// candidate for constraint validation (not a control, disabled, read-only
/// or a non-submit button), else whether it satisfies its constraints. A
/// `form` is valid when all its candidate controls are.
pub(crate) fn validity(document: &impl Document, id: u32) -> Option<bool> {
    match tag(document, id)? {
        Tag::Form => Some(
            descendants(document, id)
                .into_iter()
                .filter(|control| form_owner(document, *control) == Some(id))
                .all(|control| control_validity(document, control) != Some(false)),
        ),
        _ => control_validity(document, id),
    }
}

fn control_validity(document: &impl Document, id: u32) -> Option<bool> {
    if disabled(document, id) {
        return None;
    }
    match tag(document, id)? {
        Tag::Button => submit_button(document, id).then_some(true),
        Tag::Select => Some(true),
        Tag::Textarea => document.attribute(id, "readonly").is_none().then_some(true),
        Tag::Input => {
            let kind = input_type(document, id);
            if TEXT_TYPES
                .iter()
                .any(|text| kind.eq_ignore_ascii_case(text))
                && document.attribute(id, "readonly").is_some()
            {
                return None;
            }
            if !kind.eq_ignore_ascii_case("number") {
                return Some(true);
            }
            if document.state(id).bad_input {
                return Some(false);
            }
            let value = document.value(id).unwrap_or("");
            if value.is_empty() {
                return Some(true);
            }
            // HTML §4.10.5.1.12: step 1, the step base the `value`
            // attribute's number, else 0.
            let Some(number) = Decimal::parse(value) else {
                // A value that is not a number is the user's bad input.
                return Some(false);
            };
            let base = document
                .attribute(id, "value")
                .and_then(Decimal::parse)
                .unwrap_or_default();
            Some(number.fraction() == base.fraction())
        }
        _ => None,
    }
}

/// Whether `text` is a valid floating-point number (HTML §2.3.4.3).
#[must_use]
pub fn valid_float(text: &str) -> bool {
    Decimal::parse(text).is_some()
}

/// Digits of a valid floating-point number beyond which it is not exact
/// here (and far beyond binary64).
const MAX_DIGITS: usize = 1024;

/// A valid floating-point number (HTML §2.3.4.3) as an exact decimal: its
/// sign and its digits around the decimal point.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Decimal {
    negative: bool,
    /// The fractional digits, without trailing zeros.
    fraction: Vec<u8>,
}
impl Decimal {
    fn parse(text: &str) -> Option<Self> {
        let (negative, rest) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (mantissa, exponent) = match rest.find(['e', 'E']) {
            Some(at) => {
                let exponent = &rest[at + 1..];
                let digits = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                (&rest[..at], exponent.parse::<i64>().ok()?)
            }
            None => (rest, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
        if !digits(whole)
            || !digits(fraction)
            || whole.is_empty() && fraction.is_empty()
            || mantissa.ends_with('.')
            || whole.len() + fraction.len() > MAX_DIGITS
            || exponent.unsigned_abs() > MAX_DIGITS as u64
        {
            return None;
        }
        // Shift the decimal point by the exponent.
        let all: Vec<u8> = whole
            .bytes()
            .chain(fraction.bytes())
            .map(|b| b - b'0')
            .collect();
        let point = i64::try_from(whole.len()).ok()? + exponent;
        let fraction: Vec<u8> = (point.max(0)..i64::try_from(all.len()).ok()?)
            .map(|at| all[usize::try_from(at).unwrap_or(0)])
            .collect();
        let leading = usize::try_from((-point).max(0)).ok()?;
        let mut fraction: Vec<u8> = core::iter::repeat_n(0, leading).chain(fraction).collect();
        while fraction.last() == Some(&0) {
            fraction.pop();
        }
        Some(Self { negative, fraction })
    }
    /// The number modulo 1, as fractional digits: `-0.25` is `0.75`.
    fn fraction(&self) -> Vec<u8> {
        if !self.negative || self.fraction.is_empty() {
            return self.fraction.clone();
        }
        // 1 - f, digit by digit from the right.
        let mut out = vec![0u8; self.fraction.len()];
        let mut borrow = false;
        for (at, digit) in self.fraction.iter().enumerate().rev() {
            let minuend: i8 = if at + 1 == self.fraction.len() { 10 } else { 9 };
            let value = minuend - i8::try_from(*digit).unwrap_or(0) - i8::from(borrow);
            borrow = value < 0;
            out[at] = u8::try_from(value.rem_euclid(10)).unwrap_or(0);
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_are_exact_valid_floating_point_numbers() {
        let fraction = |text: &str| Decimal::parse(text).map(|d| d.fraction());
        assert_eq!(fraction("1"), Some(vec![]));
        assert_eq!(fraction("1.50"), Some(vec![5]));
        assert_eq!(fraction("-0.25"), Some(vec![7, 5]));
        assert_eq!(fraction("12.5e-1"), Some(vec![2, 5]));
        assert_eq!(fraction("1e2"), Some(vec![]));
        assert_eq!(fraction(".5"), Some(vec![5]));
        assert_eq!(
            fraction("1.0000000000000000001"),
            Some(vec![0; 18].into_iter().chain([1]).collect())
        );
        for invalid in ["", "+1", "1.", "1e", "1e+", "a", "--1", "1.2.3", "-"] {
            assert_eq!(fraction(invalid), None, "{invalid}");
        }
    }
}
