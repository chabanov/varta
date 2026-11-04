//! Bounded CSS paint phases: ordinary block backgrounds, atomic/inline content,
//! zero-level contexts, then outlines. Pseudo contexts isolate flex/grid item
//! painting while their opacity descendants still join the nearest real context.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Enter,
    Context,
    Atomic,
    Content,
    Outline,
    InlineContext,
    InlineOutline,
    InlineEnd,
}
enum Step {
    Visit(u32, Phase),
    Content,
    Contexts,
    Outlines,
}
struct Context {
    real: bool,
    content: Vec<(u32, Phase)>,
    contexts: Vec<(u32, Phase)>,
    outlines: Vec<(u32, Phase)>,
}
pub(super) struct Order {
    pending: Vec<Step>,
    contexts: Vec<Context>,
    outline_order: BTreeMap<u32, usize>,
}
impl Order {
    pub fn new(tree: &Tree) -> Self {
        let mut outline_order = BTreeMap::new();
        let mut visit = vec![(ROOT_NODE, false)];
        while let Some((id, end)) = visit.pop() {
            if end {
                outline_order.insert(id, outline_order.len());
            } else {
                visit.push((id, true));
                visit.extend(tree.nodes[&id].children.iter().rev().map(|&id| (id, false)));
            }
        }
        Self {
            pending: vec![Step::Visit(ROOT_NODE, Phase::Context)],
            contexts: Vec::new(),
            outline_order,
        }
    }
    fn real_context(&mut self) -> &mut Context {
        self.contexts
            .iter_mut()
            .rev()
            .find(|c| c.real)
            .expect("every source has a root or opacity context")
    }
    pub fn defer(&mut self, node: u32) {
        self.real_context().contexts.push((node, Phase::Context));
    }
    pub fn defer_inline(&mut self, node: u32) {
        self.real_context()
            .contexts
            .push((node, Phase::InlineContext));
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
            self.pending.push(Step::Contexts);
        }
        self.pending.push(Step::Content);
        self.contexts.push(Context {
            real,
            content: Vec::new(),
            contexts: Vec::new(),
            outlines: Vec::new(),
        });
    }
    pub fn children(&mut self, nodes: &[u32]) {
        self.pending
            .extend(nodes.iter().rev().map(|&id| Step::Visit(id, Phase::Enter)));
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
                Step::Contexts => {
                    let jobs = std::mem::take(
                        &mut self
                            .contexts
                            .last_mut()
                            .expect("matching zero-level phase")
                            .contexts,
                    );
                    self.pending.extend(
                        jobs.into_iter()
                            .rev()
                            .map(|(id, phase)| Step::Visit(id, phase)),
                    );
                }
                Step::Outlines => {
                    let mut context = self.contexts.pop().expect("matching outline phase");
                    // Descendants precede their ancestor, while peer subtrees
                    // retain source order. A flow's inline outlines precede its
                    // own containing block outline at the same closing rank.
                    context.outlines.sort_by_key(|(id, phase)| {
                        (self.outline_order[id], *phase == Phase::Outline)
                    });
                    debug_assert!(context.content.is_empty() && context.contexts.is_empty());
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
