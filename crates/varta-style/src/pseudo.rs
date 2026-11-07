//! Pseudo-elements (CSS Pseudo-Elements 4): the style of a pseudo-element
//! of an originating element, for a host whose tree has no node for it.
//!
//! [`WithPseudo`] presents a host's tree with one more node,
//! [`PSEUDO_NODE`], the pseudo-element's box: a child of the originating
//! element in the box tree (it inherits from it, §2.3 and §4.1), outside
//! the element's [`Document::children`] (selectors never see it), whose
//! cascade takes the rules that select that pseudo-element
//! ([`Document::pseudo`]). The engine then computes its values with the
//! same cascade, `var()` scope and font references as an element's.
use crate::prelude::*;
use crate::{Document, Element, ElementState, PseudoElement};
use varta_common::view::Tag;

/// The node id of the pseudo-element's box in a [`WithPseudo`] tree; no
/// host node has it (guest ids stay below
/// `varta_common::view::DRAFT_FIRST_GENERATED_NODE`).
pub const PSEUDO_NODE: u32 = u32::MAX;

/// The element a pseudo-element's box presents to the cascade: no class,
/// no inline style and no states of its own. Its tag has no user-agent
/// row ([`crate::font::UA_FONTS`]).
const PSEUDO_TAG: Tag = Tag::Span;

/// A host tree with the box of one pseudo-element of one element.
pub struct WithPseudo<'a, D> {
    pub document: &'a D,
    pub origin: u32,
    pub kind: PseudoElement,
    empty: BTreeMap<varta_common::view::CssProperty, String>,
}
impl<'a, D: Document> WithPseudo<'a, D> {
    #[must_use]
    pub fn new(document: &'a D, origin: u32, kind: PseudoElement) -> Self {
        Self {
            document,
            origin,
            kind,
            empty: BTreeMap::new(),
        }
    }
}

impl<D: Document> Document for WithPseudo<'_, D> {
    fn element(&self, id: u32) -> Option<Element<'_>> {
        if id == PSEUDO_NODE {
            return Some(Element {
                tag: PSEUDO_TAG,
                class: "",
                style: &self.empty,
                disabled: false,
                checked: false,
            });
        }
        self.document.element(id)
    }
    fn parent(&self, id: u32) -> Option<u32> {
        if id == PSEUDO_NODE {
            return Some(self.origin);
        }
        self.document.parent(id)
    }
    fn children(&self, id: u32) -> &[u32] {
        if id == PSEUDO_NODE {
            return &[];
        }
        self.document.children(id)
    }
    fn top_level(&self) -> Vec<u32> {
        self.document.top_level()
    }
    fn focused(&self, id: u32) -> bool {
        id != PSEUDO_NODE && self.document.focused(id)
    }
    fn focus_visible(&self, id: u32) -> bool {
        id != PSEUDO_NODE && self.document.focus_visible(id)
    }
    fn hovered(&self, id: u32) -> bool {
        id != PSEUDO_NODE && self.document.hovered(id)
    }
    fn active(&self, id: u32) -> bool {
        id != PSEUDO_NODE && self.document.active(id)
    }
    fn attribute(&self, id: u32, name: &str) -> Option<&str> {
        if id == PSEUDO_NODE {
            return None;
        }
        self.document.attribute(id, name)
    }
    fn value(&self, id: u32) -> Option<&str> {
        if id == PSEUDO_NODE {
            return None;
        }
        self.document.value(id)
    }
    fn text_data(&self, id: u32) -> Option<bool> {
        if id == PSEUDO_NODE {
            return None;
        }
        self.document.text_data(id)
    }
    fn state(&self, id: u32) -> ElementState {
        if id == PSEUDO_NODE {
            return ElementState::default();
        }
        self.document.state(id)
    }
    fn pseudo(&self, id: u32) -> Option<(u32, PseudoElement)> {
        if id == PSEUDO_NODE {
            return Some((self.origin, self.kind));
        }
        self.document.pseudo(id)
    }
}

/// The CSS Content 3 §2 `content` of a pseudo-element, as the boxes that
/// render it read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// The concatenated strings of the content list.
    pub text: String,
    /// The alternative text after `/`, for its accessible name (§2.2).
    pub alternative: Option<String>,
}

/// Whether `content` (a computed value: `normal`, `none` or a list of
/// strings with optional alternative text) generates a box for `::before`
/// or `::after` (Pseudo-Elements 4 §4.1: `normal` computes to `none` for
/// them, and `none` generates nothing), and its text.
#[must_use]
pub fn generated(content: Option<&str>) -> Option<Generated> {
    let content = content?;
    let tokens = varta_common::view::syntax::tokenize(content);
    let mut text = String::new();
    let mut alternative: Option<String> = None;
    let mut any = false;
    for spanned in tokens {
        use varta_common::view::syntax::Token;
        match spanned.token {
            Token::Whitespace => {}
            Token::String(value) => {
                any = true;
                match alternative.as_mut() {
                    Some(alternative) => alternative.push_str(&value),
                    None => text.push_str(&value),
                }
            }
            Token::Delim('/') if alternative.is_none() && any => {
                alternative = Some(String::new());
            }
            // `normal`, `none` and anything unrendered generate nothing.
            _ => return None,
        }
    }
    any.then_some(Generated { text, alternative })
}
