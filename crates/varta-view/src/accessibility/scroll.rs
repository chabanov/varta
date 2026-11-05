//! Trusted assistive scrolling from an acknowledged frame, never guest floats.
use super::*;
use crate::{FrameMetadata, ScrollMetrics};
mod reveal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibilityScrollUnit {
    Item,
    Page,
}

/// A finite nonnegative CSS offset. Untrusted platform payloads must construct
/// this before entering the host FIFO; NaN/infinities cannot enter the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilityScrollOffset(u64);
impl AccessibilityScrollOffset {
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then(|| Self(value.max(0.0).to_bits()))
    }
    #[must_use]
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}
/// Preferred edges for revealing a target; unspecified axes use nearest-edge behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibilityScrollHint {
    Nearest,
    TopLeft,
    BottomRight,
    TopEdge,
    BottomEdge,
    LeftEdge,
    RightEdge,
}
impl AccessibilityScrollHint {
    fn horizontal(self) -> crate::layout::ScrollAlignment {
        use crate::layout::ScrollAlignment as A;
        match self {
            Self::TopLeft | Self::LeftEdge => A::Start,
            Self::BottomRight | Self::RightEdge => A::End,
            _ => A::Nearest,
        }
    }
    fn vertical(self) -> crate::layout::ScrollAlignment {
        use crate::layout::ScrollAlignment as A;
        match self {
            Self::TopLeft | Self::TopEdge => A::Start,
            Self::BottomRight | Self::BottomEdge => A::End,
            _ => A::Nearest,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessibilityScrollAction {
    IntoView,
    IntoViewHint(AccessibilityScrollHint),
    Up(AccessibilityScrollUnit),
    Down(AccessibilityScrollUnit),
    To(AccessibilityScrollOffset),
    TextIntoView {
        revision: AccessibilityTextRevision,
        start: usize,
        end: usize,
        hint: AccessibilityScrollHint,
    },
}
impl View {
    /// Apply a trusted assistive input using the acknowledged frame's layout.
    /// Hosts must retain model revision/viewport/theme/scale guards. Source path,
    /// content and scroll-state guards additionally reject stale identities here.
    pub fn accessibility_input_from_frame(
        &mut self,
        identity: &NodeIdentity,
        action: &AccessibilityAction,
        frame: &FrameMetadata,
        broker: &mut dyn Broker,
    ) -> Result<bool, InputError> {
        let AccessibilityAction::Scroll(action) = action else {
            return self.accessibility_input(identity, action, broker);
        };
        let Some(node) = self.tree.nodes.get(&identity.id) else {
            return Ok(false);
        };
        if !std::sync::Arc::ptr_eq(&node.identity, &identity.token)
            || !self.tree.attached(identity.id)
            || !frame.scroll_path_current(&self.tree, identity.id)
        {
            return Ok(false);
        }
        let names = Names::new(&self.tree, &self.environment);
        if self
            .tree
            .path(identity.id)
            .into_iter()
            .any(|id| names.hidden(id))
        {
            return Ok(false);
        }
        if let AccessibilityScrollAction::TextIntoView {
            revision,
            start,
            end,
            hint,
        } = action
        {
            return self.reveal_text(identity, revision, *start..*end, *hint, frame);
        }
        let changes = match action {
            AccessibilityScrollAction::IntoView | AccessibilityScrollAction::IntoViewHint(_) => {
                if !frame.scroll_into_view_supported(identity.id) {
                    return Ok(false);
                }
                let Ok(plan) = crate::layout::scroll_into_view_plan(
                    &self.tree,
                    &self.environment,
                    frame.layout_for_scroll(),
                    identity.id,
                ) else {
                    return Ok(false);
                };
                let hint = match action {
                    AccessibilityScrollAction::IntoViewHint(hint) => *hint,
                    _ => AccessibilityScrollHint::Nearest,
                };
                let plan = if hint == AccessibilityScrollHint::Nearest {
                    plan
                } else {
                    crate::layout::scroll_path_plan(
                        frame.layout_for_scroll(),
                        // Absolute and fixed boxes skip scrollers outside their containing block.
                        crate::layout::scroll_chain(&self.tree, &self.environment, identity.id)
                            .map_err(|_| InputError::ScrollGeometry)?,
                        frame.layout_for_scroll()[&identity.id].rect,
                        hint.vertical(),
                    )
                };
                plan.into_iter()
                    .map(|(id, top)| {
                        (
                            id,
                            top,
                            frame.layout_for_scroll()[&id]
                                .programmatic_scroll
                                .expect("planned scrollport"),
                        )
                    })
                    .collect::<Vec<_>>()
            }
            _ => {
                let Some(metrics) = frame
                    .scroll_metrics(identity.id)
                    .filter(|m| m.max > 0.0 && m.viewport.height > 0.0)
                else {
                    return Ok(false);
                };
                let amount = |unit| match unit {
                    AccessibilityScrollUnit::Page => metrics.viewport.height,
                    AccessibilityScrollUnit::Item => frame
                        .scroll_item_height(identity.id, self.environment.root_font_size as f32),
                };
                let next = match action {
                    AccessibilityScrollAction::Up(unit) => (metrics.top - amount(*unit)).max(0.0),
                    AccessibilityScrollAction::Down(unit) => {
                        (metrics.top + amount(*unit)).min(metrics.max)
                    }
                    AccessibilityScrollAction::To(offset) => {
                        offset.value().min(f64::from(metrics.max)) as f32
                    }
                    AccessibilityScrollAction::IntoView
                    | AccessibilityScrollAction::IntoViewHint(_)
                    | AccessibilityScrollAction::TextIntoView { .. } => {
                        unreachable!("handled above")
                    }
                };
                if next == metrics.top {
                    Vec::new()
                } else {
                    vec![(identity.id, f64::from(next), metrics)]
                }
            }
        };
        if changes.is_empty() {
            return Ok(true);
        }
        let mut next = self.clone();
        for (id, top, metrics) in changes {
            next.assistive_scroll(id, top, metrics)?;
        }
        *self = next;
        Ok(true)
    }
    fn assistive_scroll(
        &mut self,
        id: u32,
        top: f64,
        metrics: ScrollMetrics,
    ) -> Result<(), InputError> {
        let Some(element) = self.tree.element_mut(id) else {
            return Err(InputError::ScrollGeometry);
        };
        element.scroll_top = top;
        if element.tag == Tag::Textarea {
            element.editor.manual_text_scroll = true;
        }
        let mut record = crate::EventRecord::occurrence(EventKind::Scroll, id);
        record.scroll_top = top;
        record.client_height = f64::from(metrics.viewport.height);
        record.scroll_height = f64::from(metrics.viewport.height + metrics.max);
        self.dispatch(record, false)?;
        Ok(())
    }
}
