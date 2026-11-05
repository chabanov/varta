//! Bounded CSS paint phases after [CSS2 Appendix E](https://www.w3.org/TR/CSS2/zindex.html)
//! for the admitted subset. A real stacking context (the root, opacity below 1,
//! a transform, `position: fixed`, or a z-index on a positioned box or
//! flex/grid item) paints
//! its own background, then negative layers, ordinary block backgrounds,
//! atomic/inline content, zero and positive layers, then outlines. Layers are
//! ordered by z-index and then tree order; z-index applies only to positioned
//! boxes and flex/grid items, so any other context (opacity, a transform)
//! takes level zero.
//! Positioned boxes with `z-index: auto` paint atomically in the zero layer,
//! like flex/grid items in the content phase: these pseudo contexts let
//! descendant layers join the nearest real context.
//!
//! Modal dialogs form the top layer: after the root context has painted
//! everything else, each one paints its backdrop over the viewport and then
//! itself as a real context, in the order the dialogs were shown.
use super::*;
use crate::layout::Scheme;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Enter,
    Context,
    Atomic,
    Content,
    Outline,
    InlineContext,
    InlineOutline,
    InlineEnd,
    /// A modal dialog's backdrop, over the whole viewport.
    Backdrop,
}
/// A stacking layer position: z-index level, then tree order.
type LayerKey = (i32, usize);
type Layers = BTreeMap<LayerKey, (u32, Phase)>;
enum Step {
    Visit(u32, Phase),
    Content,
    /// Negative (`true`) or zero/positive (`false`) layers of the innermost
    /// real context, one at a time so a layer found while painting an earlier
    /// pseudo context still takes its tree-order place.
    Layers(bool),
    Outlines,
}
struct Context {
    real: bool,
    content: Vec<(u32, Phase)>,
    layers: Layers,
    outlines: Vec<(u32, Phase)>,
}

/// `z-index: auto` is `None`; other values are CSS integers.
fn z_index(value: Option<&String>) -> Option<Option<i32>> {
    match value.map(String::as_str) {
        None | Some("auto") => Some(None),
        Some(value) => value.parse().ok().map(Some),
    }
}

/// Where one painted element takes its clips and scroll offsets from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Placement {
    /// The element whose descendant clip scope applies: the parent for
    /// in-flow boxes, the containing block for absolute boxes, and none for
    /// fixed boxes or absolute boxes of the initial containing block.
    pub scope: Option<u32>,
    /// Absolute and fixed boxes do not inherit their parent's clip.
    pub out_of_flow: bool,
}

pub(super) struct Order {
    pending: Vec<Step>,
    contexts: Vec<Context>,
    outline_order: BTreeMap<u32, usize>,
    tree_order: BTreeMap<u32, usize>,
    /// Layer members of each real context, found before painting so negative
    /// layers precede the context's in-flow descendants.
    layers: BTreeMap<u32, Layers>,
    layered: BTreeSet<u32>,
    /// Painted elements, each after its parent and containing block.
    elements: Vec<u32>,
    placements: BTreeMap<u32, Placement>,
}
impl Order {
    /// Plan the painted element tree. `flow` elements own inline content and
    /// select elements own their options: neither has block descendants here.
    pub fn new(
        tree: &Tree,
        env: &Environment,
        geometry: &BTreeMap<u32, layout::BoxGeometry>,
        flow: impl Fn(u32) -> bool,
    ) -> Result<Self, PaintError> {
        let mut outline_order = BTreeMap::new();
        let mut tree_order = BTreeMap::new();
        let mut visit = vec![(ROOT_NODE, false)];
        while let Some((id, end)) = visit.pop() {
            if end {
                outline_order.insert(id, outline_order.len());
            } else {
                tree_order.insert(id, tree_order.len());
                visit.push((id, true));
                visit.extend(tree.nodes[&id].children.iter().rev().map(|&id| (id, false)));
            }
        }
        // The top layer paints after the root: each modal dialog's backdrop,
        // then the dialog (the pending steps are a stack: last pops first).
        let top_layer: Vec<u32> = tree
            .modals
            .iter()
            .map(|modal| modal.dialog)
            .filter(|dialog| geometry.contains_key(dialog) && layout::modal_dialog(tree, *dialog))
            .collect();
        let mut pending: Vec<Step> = top_layer
            .iter()
            .rev()
            .flat_map(|&dialog| {
                [
                    Step::Visit(dialog, Phase::Context),
                    Step::Visit(dialog, Phase::Backdrop),
                ]
            })
            .collect();
        pending.push(Step::Visit(ROOT_NODE, Phase::Context));
        let mut order = Self {
            pending,
            contexts: Vec::new(),
            outline_order,
            tree_order,
            layers: BTreeMap::new(),
            layered: BTreeSet::new(),
            elements: vec![ROOT_NODE],
            placements: BTreeMap::from([(
                ROOT_NODE,
                Placement {
                    scope: None,
                    out_of_flow: false,
                },
            )]),
        };
        order.plan(tree, env, geometry, flow)?;
        Ok(order)
    }
    fn plan(
        &mut self,
        tree: &Tree,
        env: &Environment,
        geometry: &BTreeMap<u32, layout::BoxGeometry>,
        flow: impl Fn(u32) -> bool,
    ) -> Result<(), PaintError> {
        let root = Scheme::parse(
            tree.computed_read(ROOT_NODE, env)
                .get(&CssProperty::Position),
        )
        .ok_or(PaintError::Unsupported(ROOT_NODE))?;
        // (element, nearest real context, nearest positioned ancestor)
        let mut pending = vec![(ROOT_NODE, ROOT_NODE, root.positioned().then_some(ROOT_NODE))];
        while let Some((parent, owner, containing)) = pending.pop() {
            if flow(parent)
                || tree
                    .element(parent)
                    .is_none_or(|element| element.tag == Tag::Select)
            {
                continue;
            }
            let flex_parent = tree
                .computed_read(parent, env)
                .get(&CssProperty::Display)
                .is_some_and(|value| matches!(value.as_str(), "flex" | "grid"));
            for &id in tree.nodes[&parent].children.iter().rev() {
                if tree.element(id).is_none() || !geometry.contains_key(&id) {
                    continue;
                }
                let properties = tree.computed_read(id, env);
                if tree.closed_dialog(id)
                    || properties
                        .get(&CssProperty::Display)
                        .is_some_and(|value| value.eq_ignore_ascii_case("none"))
                {
                    continue;
                }
                // A modal dialog is in the top layer: fixed, and painted
                // after the root context instead of in any layer of it.
                if layout::modal_dialog(tree, id) {
                    self.placements.insert(
                        id,
                        Placement {
                            scope: None,
                            out_of_flow: true,
                        },
                    );
                    self.elements.push(id);
                    self.layered.insert(id);
                    pending.push((id, id, Some(id)));
                    continue;
                }
                let scheme = Scheme::parse(properties.get(&CssProperty::Position))
                    .ok_or(PaintError::Unsupported(id))?;
                let z = z_index(properties.get(&CssProperty::ZIndex))
                    .ok_or(PaintError::Unsupported(id))?;
                let translucent = properties
                    .get(&CssProperty::Opacity)
                    .map(|raw| {
                        raw.parse::<f32>()
                            .ok()
                            .filter(|a| a.is_finite() && (0.0..=1.0).contains(a))
                            .ok_or(PaintError::Unsupported(id))
                    })
                    .transpose()?
                    .is_some_and(|alpha| alpha != 1.0);
                // CSS Flexbox/Grid: z-index applies to in-flow items even
                // when they are not positioned.
                let item = flex_parent && !scheme.out_of_flow();
                // z-index has no effect on any other box, even one that
                // opacity makes a stacking context: that box paints at level
                // zero in tree order.
                let z = if scheme.positioned() || item { z } else { None };
                // Any transform makes a stacking context (CSS Transforms 1 §3).
                let transformed = properties
                    .get(&CssProperty::Transform)
                    .is_some_and(|value| value.trim() != "none");
                let real = translucent || transformed || scheme == Scheme::Fixed || z.is_some();
                self.placements.insert(
                    id,
                    Placement {
                        scope: match scheme {
                            Scheme::Absolute => containing,
                            Scheme::Fixed => None,
                            Scheme::Static | Scheme::Relative => Some(parent),
                        },
                        out_of_flow: scheme.out_of_flow(),
                    },
                );
                self.elements.push(id);
                if real || scheme.positioned() {
                    self.layered.insert(id);
                    let level = z.unwrap_or(0);
                    self.layers.entry(owner).or_default().insert(
                        (level, self.tree_order[&id]),
                        (id, if real { Phase::Context } else { Phase::Atomic }),
                    );
                }
                pending.push((
                    id,
                    if real { id } else { owner },
                    if scheme.positioned() {
                        Some(id)
                    } else {
                        containing
                    },
                ));
            }
        }
        Ok(())
    }
    /// Painted elements, each after its parent and containing block.
    pub fn elements(&self) -> impl Iterator<Item = (u32, Placement)> + '_ {
        self.elements.iter().map(|id| (*id, self.placements[id]))
    }
    /// The planned placement of a painted element.
    pub fn placement(&self, id: u32) -> Option<Placement> {
        self.placements.get(&id).copied()
    }
    /// Whether an element paints in a stacking layer instead of in flow.
    pub fn layered(&self, id: u32) -> bool {
        self.layered.contains(&id)
    }
    fn real_context(&mut self) -> &mut Context {
        self.contexts
            .iter_mut()
            .rev()
            .find(|c| c.real)
            .expect("every source has a root or opacity context")
    }
    pub fn defer_inline(&mut self, node: u32) {
        let key = (0, self.tree_order[&node]);
        self.real_context()
            .layers
            .insert(key, (node, Phase::InlineContext));
    }
    pub fn atomic(&mut self, node: u32) {
        self.contexts
            .last_mut()
            .expect("containing paint context")
            .content
            .push((node, Phase::Atomic));
    }
    pub fn content(&mut self, node: u32) {
        self.contexts
            .last_mut()
            .expect("containing paint context")
            .content
            .push((node, Phase::Content));
    }
    pub fn outline(&mut self, node: u32) {
        self.contexts
            .last_mut()
            .expect("containing paint context")
            .outlines
            .push((node, Phase::Outline));
    }
    pub fn inline_outline(&mut self, node: u32) {
        self.contexts
            .last_mut()
            .expect("containing outline context")
            .outlines
            .push((node, Phase::InlineOutline));
    }
    pub fn finish_after_children(&mut self, node: u32, phase: Phase) {
        debug_assert!(matches!(
            phase,
            Phase::Context | Phase::InlineContext | Phase::Atomic
        ));
        let end = if phase == Phase::InlineContext {
            Phase::InlineEnd
        } else {
            Phase::Outline
        };
        let real = phase != Phase::Atomic;
        self.pending.push(Step::Visit(node, end));
        self.pending.push(Step::Outlines);
        if real {
            self.pending.push(Step::Layers(false));
        }
        self.pending.push(Step::Content);
        if real {
            // `children` keeps this step above the in-flow descendants.
            self.pending.push(Step::Layers(true));
        }
        self.contexts.push(Context {
            real,
            content: Vec::new(),
            layers: if real {
                self.layers.remove(&node).unwrap_or_default()
            } else {
                Layers::new()
            },
            outlines: Vec::new(),
        });
    }
    pub fn children(&mut self, nodes: &[u32]) {
        let negative = matches!(self.pending.last(), Some(Step::Layers(true)));
        if negative {
            self.pending.pop();
        }
        self.pending
            .extend(nodes.iter().rev().map(|&id| Step::Visit(id, Phase::Enter)));
        if negative {
            self.pending.push(Step::Layers(true));
        }
    }
}
impl Iterator for Order {
    type Item = (u32, Phase);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.pending.pop()? {
                Step::Visit(node, phase) => return Some((node, phase)),
                Step::Content => {
                    let jobs = std::mem::take(
                        &mut self
                            .contexts
                            .last_mut()
                            .expect("matching content phase")
                            .content,
                    );
                    self.pending.extend(
                        jobs.into_iter()
                            .rev()
                            .map(|(id, phase)| Step::Visit(id, phase)),
                    );
                }
                Step::Layers(negative) => {
                    let context = self.contexts.last_mut().expect("matching layer phase");
                    debug_assert!(context.real);
                    let next = context
                        .layers
                        .first_key_value()
                        .is_some_and(|((level, _), _)| !negative || *level < 0);
                    if next && let Some((_, job)) = context.layers.pop_first() {
                        self.pending.push(Step::Layers(negative));
                        return Some(job);
                    }
                }
                Step::Outlines => {
                    let mut context = self.contexts.pop().expect("matching outline phase");
                    // Descendants precede their ancestor, while peer subtrees
                    // retain source order. A flow's inline outlines precede its
                    // own containing block outline at the same closing rank.
                    context.outlines.sort_by_key(|(id, phase)| {
                        (self.outline_order[id], *phase == Phase::Outline)
                    });
                    debug_assert!(context.content.is_empty() && context.layers.is_empty());
                    self.pending.extend(
                        context
                            .outlines
                            .into_iter()
                            .rev()
                            .map(|(id, phase)| Step::Visit(id, phase)),
                    );
                }
            }
        }
    }
}
