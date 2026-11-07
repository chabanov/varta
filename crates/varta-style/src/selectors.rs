//! Selector matching over the host's tree (Selectors 4).
//!
//! Complex selectors match right to left with the restart rules of Servo's
//! matcher (`selectors::matching`), which bound backtracking: a failed
//! descendant search never retries above the closest failing ancestor, so a
//! selector costs at most its compounds times the element's ancestors and
//! siblings. The style session charges that bound before computing values
//! ([`selector_work`]).
use crate::Document;
use crate::prelude::*;
use varta_common::view::stylesheet::{Combinator, Compound, Nth, Pseudo, Selector, Specificity};
use varta_common::view::{ROOT_NODE, Tag};

/// The form controls `:enabled` and `:disabled` apply to (HTML §4.16.3).
const DISABLEABLE: &[Tag] = &[
    Tag::Button,
    Tag::Input,
    Tag::Select,
    Tag::Textarea,
    Tag::Option,
];

/// The most specific selector of `list` that matches `id`.
#[must_use]
pub fn matching(document: &impl Document, id: u32, list: &[Selector]) -> Option<Specificity> {
    list.iter()
        .filter(|selector| matches(document, id, selector))
        .map(Selector::specificity)
        .max()
}

/// Whether `selector` matches element `id`.
#[must_use]
pub fn matches(document: &impl Document, id: u32, selector: &Selector) -> bool {
    complex(document, id, selector, selector.compounds.len() - 1) == Outcome::Matched
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Matched,
    /// Restart from the closest later sibling of a sibling combinator.
    RestartFromLaterSibling,
    /// Restart from the closest descendant combinator.
    RestartFromDescendant,
    /// No element can match (Servo's `NotMatchedGlobally`).
    Global,
}

/// Compounds `..=last` of `selector` against `id`.
fn complex(document: &impl Document, id: u32, selector: &Selector, last: usize) -> Outcome {
    if !compound(document, id, &selector.compounds[last]) {
        return Outcome::RestartFromLaterSibling;
    }
    let Some(previous) = last.checked_sub(1) else {
        return Outcome::Matched;
    };
    let combinator = selector.combinators[previous];
    let not_found = match combinator {
        Combinator::NextSibling | Combinator::SubsequentSibling => Outcome::RestartFromDescendant,
        Combinator::Child | Combinator::Descendant => Outcome::Global,
    };
    let mut next = step(document, id, combinator);
    loop {
        let Some(element) = next else {
            return not_found;
        };
        let outcome = complex(document, element, selector, previous);
        match (outcome, combinator) {
            (Outcome::Matched | Outcome::Global, _) | (_, Combinator::NextSibling) => {
                return outcome;
            }
            (_, Combinator::Child) => return Outcome::RestartFromDescendant,
            (Outcome::RestartFromDescendant, Combinator::SubsequentSibling) => return outcome,
            _ => {}
        }
        next = step(document, element, combinator);
    }
}

/// The next candidate element for `combinator` from `id`.
fn step(document: &impl Document, id: u32, combinator: Combinator) -> Option<u32> {
    match combinator {
        Combinator::Child | Combinator::Descendant => parent(document, id),
        Combinator::NextSibling | Combinator::SubsequentSibling => previous_sibling(document, id),
    }
}

fn parent(document: &impl Document, id: u32) -> Option<u32> {
    document
        .parent(id)
        .filter(|parent| document.element(*parent).is_some())
}

/// The element siblings of `id`, itself included, in order.
fn siblings(document: &impl Document, id: u32) -> Vec<u32> {
    match document.parent(id) {
        Some(parent) => document
            .children(parent)
            .iter()
            .copied()
            .filter(|child| document.element(*child).is_some())
            .collect(),
        None => vec![id],
    }
}

fn previous_sibling(document: &impl Document, id: u32) -> Option<u32> {
    let siblings = siblings(document, id);
    let at = siblings.iter().position(|sibling| *sibling == id)?;
    at.checked_sub(1).map(|before| siblings[before])
}

fn compound(document: &impl Document, id: u32, compound: &Compound) -> bool {
    let Some(element) = document.element(id) else {
        return false;
    };
    if compound.tag.is_some_and(|tag| tag != element.tag) {
        return false;
    }
    if !compound.classes.iter().all(|class| {
        element
            .class
            .split_ascii_whitespace()
            .any(|own| own == class)
    }) {
        return false;
    }
    compound
        .pseudo
        .iter()
        .all(|pseudo| pseudo_class(document, id, element.tag, pseudo))
}

fn pseudo_class(document: &impl Document, id: u32, tag: Tag, pseudo: &Pseudo) -> bool {
    let element = document.element(id);
    let disabled = element.is_some_and(|element| element.disabled);
    match pseudo {
        Pseudo::Hover => document.hovered(id),
        Pseudo::Active => document.active(id),
        Pseudo::Focus => document.focused(id),
        // Selectors 4 §9.4: the element or a descendant has the focus.
        Pseudo::FocusWithin => focus_within(document, id),
        Pseudo::FocusVisible => document.focus_visible(id),
        Pseudo::Disabled => disabled,
        Pseudo::Enabled => DISABLEABLE.contains(&tag) && !disabled,
        Pseudo::Checked => element.is_some_and(|element| element.checked),
        Pseudo::Root => id == ROOT_NODE,
        // Scoping 1 §3.2.1: outside a shadow tree's own sheets.
        Pseudo::Host => false,
        Pseudo::FirstChild => position(document, id, false, false).0 == 1,
        Pseudo::LastChild => position(document, id, true, false).0 == 1,
        Pseudo::OnlyChild => position(document, id, false, false).1 == 1,
        Pseudo::FirstOfType => position(document, id, false, true).0 == 1,
        Pseudo::LastOfType => position(document, id, true, true).0 == 1,
        Pseudo::OnlyOfType => position(document, id, false, true).1 == 1,
        Pseudo::NthChild(nth) => nth_matches(*nth, position(document, id, false, false).0),
        Pseudo::NthLastChild(nth) => nth_matches(*nth, position(document, id, true, false).0),
        Pseudo::NthOfType(nth) => nth_matches(*nth, position(document, id, false, true).0),
        Pseudo::NthLastOfType(nth) => nth_matches(*nth, position(document, id, true, true).0),
        Pseudo::Is(list) | Pseudo::Where(list) => {
            list.iter().any(|selector| matches(document, id, selector))
        }
        Pseudo::Not(list) => !list.iter().any(|selector| matches(document, id, selector)),
    }
}

/// Whether `id` or one of its descendants has the focus.
fn focus_within(document: &impl Document, id: u32) -> bool {
    let mut pending = vec![id];
    while let Some(node) = pending.pop() {
        if document.focused(node) {
            return true;
        }
        pending.extend(document.children(node).iter().copied());
    }
    false
}

fn nth_matches(nth: Nth, index: usize) -> bool {
    i32::try_from(index).is_ok_and(|index| nth.matches(index))
}

/// The one-based position of `id` among its element siblings (counted from
/// the end with `reverse`, among those of its type with `of_type`), and
/// their number.
fn position(document: &impl Document, id: u32, reverse: bool, of_type: bool) -> (usize, usize) {
    let tag = document.element(id).map(|element| element.tag);
    let mut siblings = siblings(document, id);
    if of_type {
        siblings.retain(|sibling| document.element(*sibling).map(|e| e.tag) == tag);
    }
    if reverse {
        siblings.reverse();
    }
    let at = siblings
        .iter()
        .position(|sibling| *sibling == id)
        .map_or(0, |at| at + 1);
    (at, siblings.len())
}

/// The matching work estimate of `selector` at an element with `ancestors`
/// ancestors and `siblings` siblings: each compound, nested ones included,
/// visits the ancestors and siblings it searches at most once under the
/// restart rules above.
#[must_use]
pub fn selector_work(selector: &Selector, ancestors: usize, siblings: usize) -> usize {
    selector
        .cost()
        .saturating_mul(ancestors.saturating_add(siblings).saturating_add(1))
}
