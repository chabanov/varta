//! Selector matching over the host's tree (Selectors 4).
//!
//! Complex selectors match right to left with the restart rules of Servo's
//! matcher (`selectors::matching`), which bound backtracking: a failed
//! descendant search never retries above the closest failing ancestor, so a
//! selector costs at most its compounds times the element's ancestors and
//! siblings. The style session charges that bound before computing values
//! ([`selector_work`]).
use crate::prelude::*;
use crate::{Document, forms};
use varta_common::view::stylesheet::selector::{Direction, PseudoElement, Relative};
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

/// The most specific selector of `list` that matches element `id`, or with
/// `pseudo` its pseudo-element: only selectors of that pseudo-element (none
/// for the element itself) take part.
#[must_use]
pub fn matching(
    document: &impl Document,
    id: u32,
    list: &[Selector],
    pseudo: Option<PseudoElement>,
) -> Option<Specificity> {
    list.iter()
        .filter(|selector| selector.pseudo_element == pseudo && matches(document, id, selector))
        .map(Selector::specificity)
        .max()
}

/// Whether `selector` matches element `id` (its pseudo-element aside).
#[must_use]
pub fn matches(document: &impl Document, id: u32, selector: &Selector) -> bool {
    complex(document, id, selector, selector.compounds.len() - 1, None) == Outcome::Matched
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

/// Compounds `..=last` of `selector` against `id`; a relative selector of
/// `:has()` is anchored at an element by its leading combinator (`anchor`).
fn complex(
    document: &impl Document,
    id: u32,
    selector: &Selector,
    last: usize,
    anchor: Option<(u32, Combinator)>,
) -> Outcome {
    if !compound(document, id, &selector.compounds[last]) {
        return Outcome::RestartFromLaterSibling;
    }
    let Some(previous) = last.checked_sub(1) else {
        return match anchor {
            Some((anchor, leading)) if !anchored(document, id, anchor, leading) => {
                Outcome::RestartFromLaterSibling
            }
            _ => Outcome::Matched,
        };
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
        let outcome = complex(document, element, selector, previous, anchor);
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

/// Whether `id` stands in the relation `leading` to `anchor` (Selectors 4
/// §3.5: the `:has()` element is the anchor of its relative selectors).
fn anchored(document: &impl Document, id: u32, anchor: u32, leading: Combinator) -> bool {
    match leading {
        Combinator::Child => parent(document, id) == Some(anchor),
        Combinator::Descendant => {
            let mut at = parent(document, id);
            while let Some(node) = at {
                if node == anchor {
                    return true;
                }
                at = parent(document, node);
            }
            false
        }
        Combinator::NextSibling => previous_sibling(document, id) == Some(anchor),
        Combinator::SubsequentSibling => {
            let siblings = siblings(document, id);
            let own = siblings.iter().position(|sibling| *sibling == id);
            let other = siblings.iter().position(|sibling| *sibling == anchor);
            matches!((own, other), (Some(own), Some(other)) if other < own)
        }
    }
}

/// The elements of `id`'s subtree, `id` excluded.
fn subtree(document: &impl Document, id: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut pending: Vec<u32> = document.children(id).to_vec();
    while let Some(node) = pending.pop() {
        if document.element(node).is_some() {
            out.push(node);
            pending.extend(document.children(node));
        }
    }
    out
}

/// `:has()` (Selectors 4 §4.5): some element matches a relative selector
/// anchored at `id`: a descendant for `>` and ` `, a later sibling or one
/// of its descendants for `+` and `~`.
fn has(document: &impl Document, id: u32, list: &[Relative]) -> bool {
    list.iter().any(|relative| {
        let candidates = match relative.leading {
            Combinator::Child | Combinator::Descendant => subtree(document, id),
            Combinator::NextSibling | Combinator::SubsequentSibling => {
                let siblings = siblings(document, id);
                let at = siblings.iter().position(|sibling| *sibling == id);
                siblings
                    .iter()
                    .skip(at.map_or(siblings.len(), |at| at + 1))
                    .flat_map(|sibling| {
                        core::iter::once(*sibling).chain(subtree(document, *sibling))
                    })
                    .collect()
            }
        };
        let last = relative.selector.compounds.len() - 1;
        candidates.into_iter().any(|candidate| {
            complex(
                document,
                candidate,
                &relative.selector,
                last,
                Some((id, relative.leading)),
            ) == Outcome::Matched
        })
    })
}

fn compound(document: &impl Document, id: u32, compound: &Compound) -> bool {
    let Some(element) = document.element(id) else {
        return false;
    };
    if compound.tag.is_some_and(|tag| tag != element.tag) {
        return false;
    }
    // HTML ids match exactly (Selectors 4 §6.7, no quirks mode).
    if !compound
        .ids
        .iter()
        .all(|own| document.attribute(id, "id") == Some(own.as_str()))
    {
        return false;
    }
    if !compound
        .attributes
        .iter()
        .all(|attribute| attribute.matches(document.attribute(id, &attribute.name)))
    {
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
        // Selectors 4 §10.2: a stylesheet's scoping root is the root.
        Pseudo::Root | Pseudo::Scope => id == ROOT_NODE,
        // Scoping 1 §3.2.1: outside a shadow tree's own sheets.
        Pseudo::Host => false,
        // §14.2: no element children and no text with data.
        Pseudo::Empty => document
            .children(id)
            .iter()
            .all(|child| document.text_data(*child) == Some(false)),
        // §8.1, §8.2: `a` with `href`; no history is exposed (`:visited`).
        Pseudo::Link | Pseudo::AnyLink => tag == Tag::A && document.attribute(id, "href").is_some(),
        // §8.6: no fragment navigation; §12: no `min`/`max` ranges, no
        // `required`, no autofill; §7.2: no content language.
        Pseudo::Visited
        | Pseudo::Target
        | Pseudo::TargetWithin
        | Pseudo::InRange
        | Pseudo::OutOfRange
        | Pseudo::Required
        | Pseudo::Autofill
        | Pseudo::Lang => false,
        Pseudo::Optional => matches!(tag, Tag::Input | Tag::Select | Tag::Textarea),
        Pseudo::ReadWrite => forms::read_write(document, id),
        Pseudo::ReadOnly => !forms::read_write(document, id),
        Pseudo::PlaceholderShown => forms::placeholder_shown(document, id),
        Pseudo::Default => forms::default(document, id),
        Pseudo::Indeterminate => forms::indeterminate(document, id),
        Pseudo::Valid => forms::validity(document, id) == Some(true),
        Pseudo::Invalid => forms::validity(document, id) == Some(false),
        Pseudo::UserValid => {
            document.state(id).user_validity && forms::validity(document, id) == Some(true)
        }
        Pseudo::UserInvalid => {
            document.state(id).user_validity && forms::validity(document, id) == Some(false)
        }
        // §13.2, §13.3: a shown and a modal dialog.
        Pseudo::Open => tag == Tag::Dialog && document.state(id).open,
        Pseudo::Modal => document.state(id).modal,
        // §13.1: every element of the view is defined.
        Pseudo::Defined => true,
        // §7.1: the view's one direction is left-to-right.
        Pseudo::Dir(direction) => *direction == Direction::Ltr,
        Pseudo::Has(list) => has(document, id, list),
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

/// Each node's subtree size (itself included) and height, for the `:has()`
/// work estimate.
#[must_use]
pub fn subtree_extents(document: &impl Document) -> BTreeMap<u32, (usize, usize)> {
    let mut extents = BTreeMap::new();
    // Post-order: a node's extent after its children's.
    let mut pending: Vec<(u32, bool)> = document
        .top_level()
        .into_iter()
        .map(|id| (id, false))
        .collect();
    while let Some((id, done)) = pending.pop() {
        if done {
            let (size, height) =
                document
                    .children(id)
                    .iter()
                    .fold((1usize, 0usize), |(size, height), child| {
                        let (child_size, child_height) =
                            extents.get(child).copied().unwrap_or((1, 0));
                        (
                            size.saturating_add(child_size),
                            height.max(child_height + 1),
                        )
                    });
            extents.insert(id, (size, height));
        } else {
            pending.push((id, true));
            pending.extend(document.children(id).iter().map(|child| (*child, false)));
        }
    }
    extents
}

/// The matching work estimate of the `:has()` arguments of `selector` at
/// element `id`: each relative selector visits the elements its anchor's
/// parent contains (the anchor's subtree and its later siblings'), each at
/// most once per compound under the restart rules, for every anchor the
/// selector tries: the element itself when `:has()` is in the subject
/// compound, else any of the document's elements.
#[must_use]
pub fn has_work(
    selector: &Selector,
    (document, id, extents): (&impl Document, u32, &BTreeMap<u32, (usize, usize)>),
    (ancestors, siblings): (usize, usize),
) -> usize {
    let extent = |node: u32| extents.get(&node).copied().unwrap_or((1, 0));
    let scope = document.parent(id).unwrap_or(id);
    let (local, height) = extent(scope);
    let total = document
        .top_level()
        .into_iter()
        .map(|top| extent(top).0)
        .fold(0usize, usize::saturating_add);
    let arguments = selector.has_arguments();
    let subject: usize = selector
        .subject()
        .pseudo
        .iter()
        .map(|pseudo| match pseudo {
            Pseudo::Has(list) => list.len(),
            _ => 0,
        })
        .sum();
    let (visited, anchors) = if subject == arguments.len() {
        (local, 1)
    } else {
        (total, ancestors.saturating_add(siblings).saturating_add(1))
    };
    let search = height
        .saturating_add(ancestors)
        .saturating_add(siblings)
        .saturating_add(1);
    arguments
        .iter()
        .map(|relative| {
            relative
                .selector
                .cost()
                .saturating_mul(visited)
                .saturating_mul(search)
                .saturating_mul(anchors)
        })
        .fold(0usize, usize::saturating_add)
}
