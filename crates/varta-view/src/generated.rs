//! The boxes of `::before` and `::after` (CSS Pseudo-Elements 4 §4.1).
//!
//! A pseudo-element whose computed `content` generates a box (CSS Content 3
//! §2: a content list; `normal` and `none` generate nothing) gets a node of
//! its own in the native tree: an element child of its originating element,
//! first for `::before` and last for `::after`, with a text child holding
//! the content's strings. Its node ids are at or above
//! [`DRAFT_FIRST_GENERATED_NODE`], which a guest cannot create or address,
//! and each `(originating element, pseudo-element)` keeps its id across
//! style changes so that its transitions continue. The style engine sees
//! such a node only through [`varta_style::Document::pseudo`]: it is never
//! one of an element's children for selectors, and its cascade is the
//! pseudo-element's rules of its originating element, from which it
//! inherits. Layout and paint read it as any element; hits, events and
//! focus go to the originating element (§4.1: the pseudo-element is not in
//! the document tree), and guest-visible snapshots leave it out.
//!
//! Batches apply to the guest's tree alone ([`Tree::strip_generated`]), and
//! every style change event regenerates the boxes ([`Tree::regenerate`]).
use crate::style::Environment;
use crate::tree::{Content, Element, Node, Tree};
use std::collections::BTreeMap;
use std::sync::Arc;
use varta_common::view::{CssProperty, DRAFT_FIRST_GENERATED_NODE, Tag};
use varta_style::PseudoElement;

/// The pseudo-elements that generate boxes in the tree.
pub(crate) const BOXES: [PseudoElement; 2] = [PseudoElement::Before, PseudoElement::After];
/// The element a generated box presents to layout and paint: an inline box
/// without user-agent rows of its own.
const BOX_TAG: Tag = Tag::Span;

/// What a generated node is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Generated {
    pub kind: PseudoElement,
    /// The content's alternative text (Content 3 §2.2), which names the box
    /// instead of its text.
    pub alternative: Option<String>,
}

/// Whether `id` is in the generated range.
pub(crate) fn generated_id(id: u32) -> bool {
    id >= DRAFT_FIRST_GENERATED_NODE
}

impl Tree {
    /// The element `id` stands for in the document: the originating element
    /// of a generated box or its text, else `id`.
    pub(crate) fn originating(&self, id: u32) -> u32 {
        let mut at = id;
        while generated_id(at) {
            match self.nodes.get(&at).and_then(|node| node.parent) {
                Some(parent) => at = parent,
                None => return id,
            }
        }
        at
    }

    /// The generated pseudo-element of node `id`, if it is a generated box.
    pub(crate) fn generated(&self, id: u32) -> Option<&Generated> {
        self.nodes.get(&id)?.generated.as_ref()
    }

    /// Remove every generated node, keeping the ids of the boxes for the
    /// next [`Tree::regenerate`].
    pub(crate) fn strip_generated(&mut self) {
        if !self
            .nodes
            .keys()
            .next_back()
            .is_some_and(|id| generated_id(*id))
        {
            return;
        }
        let generated: Vec<u32> = self
            .nodes
            .range(DRAFT_FIRST_GENERATED_NODE..)
            .map(|(id, _)| *id)
            .collect();
        for id in generated {
            if let Some(node) = self.nodes.remove(&id)
                && let Some(parent) = node.parent
                && let Some(parent) = self.nodes.get_mut(&parent)
            {
                parent.children.retain(|child| *child != id);
            }
        }
    }

    /// Generate the boxes of `::before` and `::after` of every attached
    /// element whose computed `content` generates one, with ids kept from
    /// the previous generation.
    pub(crate) fn regenerate(&mut self, environment: &Environment) {
        let previous: BTreeMap<(u32, PseudoElement), u32> = self
            .nodes
            .range(DRAFT_FIRST_GENERATED_NODE..)
            .filter_map(|(id, node)| {
                let generated = node.generated.as_ref()?;
                Some(((node.parent?, generated.kind), *id))
            })
            .collect();
        self.strip_generated();
        if !BOXES.iter().any(|kind| self.styles.selects(*kind)) {
            return;
        }
        let style = environment.style();
        let mut wanted = Vec::new();
        for id in self.document_order() {
            if self.element(id).is_none() {
                continue;
            }
            for kind in BOXES {
                let values =
                    self.styles
                        .pseudo_computed(self, id, kind, &style, |property, value| {
                            self.render_reads.validate(environment, property, value)
                        });
                let content = values
                    .get(&CssProperty::Content)
                    .map(varta_style::Computed::text);
                let display_none = values
                    .get(&CssProperty::Display)
                    .is_some_and(|display| display.text() == "none");
                if let Some(generated) = varta_style::pseudo::generated(content)
                    && !display_none
                {
                    wanted.push((id, kind, generated));
                }
            }
        }
        let mut next = DRAFT_FIRST_GENERATED_NODE;
        let mut allocate = |tree: &Tree| {
            while tree.nodes.contains_key(&next) || previous.values().any(|id| *id == next) {
                next += 1;
            }
            next += 1;
            next - 1
        };
        for (origin, kind, content) in wanted {
            let id = match previous.get(&(origin, kind)) {
                Some(id) => *id,
                None => allocate(self),
            };
            let text = (!content.text.is_empty()).then(|| allocate(self));
            let mut element = Element::new(BOX_TAG);
            element.class = String::new();
            self.nodes.insert(
                id,
                Node {
                    identity: Arc::new(()),
                    content: Content::Element(element),
                    parent: Some(origin),
                    children: text.into_iter().collect(),
                    generated: Some(Generated {
                        kind,
                        alternative: content.alternative,
                    }),
                },
            );
            if let Some(text_id) = text {
                self.nodes.insert(
                    text_id,
                    Node {
                        identity: Arc::new(()),
                        content: Content::Text(content.text),
                        parent: Some(id),
                        children: Vec::new(),
                        generated: None,
                    },
                );
            }
            if let Some(parent) = self.nodes.get_mut(&origin) {
                match kind {
                    PseudoElement::Before => parent.children.insert(0, id),
                    _ => parent.children.push(id),
                }
            }
        }
    }

    /// The children of `id` the document has: without its generated boxes.
    pub(crate) fn document_children(&self, id: u32) -> &[u32] {
        let Some(node) = self.nodes.get(&id) else {
            return &[];
        };
        let children = node.children.as_slice();
        let start = usize::from(children.first().is_some_and(|child| generated_id(*child)));
        let end = children.len()
            - usize::from(
                children.len() > start && children.last().is_some_and(|child| generated_id(*child)),
            );
        &children[start..end]
    }
}

impl Tree {
    /// The computed values of pseudo-element `kind` of element `origin`,
    /// which has no box of its own in the tree (`::placeholder`,
    /// `::backdrop`): the CSS-wide keywords as the engine leaves them.
    pub(crate) fn pseudo_values(
        &self,
        origin: u32,
        kind: PseudoElement,
        environment: &Environment,
    ) -> BTreeMap<CssProperty, varta_style::Computed> {
        self.styles.pseudo_computed(
            self,
            origin,
            kind,
            &environment.style(),
            |property, value| self.render_reads.validate(environment, property, value),
        )
    }
}
