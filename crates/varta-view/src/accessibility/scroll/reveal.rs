//! Reveal shaped source ranges while preserving the editor's logical selection.
use super::*;

impl View {
    pub(super) fn reveal_text(
        &mut self,
        identity: &NodeIdentity,
        revision: &AccessibilityTextRevision,
        range: std::ops::Range<usize>,
        hint: AccessibilityScrollHint,
        frame: &FrameMetadata,
    ) -> Result<bool, InputError> {
        let (start, end) = (range.start, range.end);
        let Some(element) = self
            .tree
            .element(identity.id)
            .filter(|e| e.text_field() && e.editor.preedit.is_none())
        else {
            return Ok(false);
        };
        let password = element.input_type() == "password";
        if start > end
            || *revision != AccessibilityTextRevision::new(&self.tree.nodes[&identity.id], element)
            || !super::super::text::boundary(&element.value, start, password)
            || !super::super::text::boundary(&element.value, end, password)
        {
            return Ok(false);
        }
        let Some(physical) = frame.text_range_bounds(identity, revision, start, end) else {
            return Ok(false);
        };
        // Reuse the path/style validation of ordinary element reveal.
        if crate::layout::scroll_into_view_plan(
            &self.tree,
            &self.environment,
            frame.layout_for_scroll(),
            identity.id,
        )
        .is_err()
        {
            return Ok(false);
        }
        let factor = frame.scale();
        let mut rect = crate::BoxRect {
            x: physical.x / factor,
            y: physical.y / factor,
            width: physical.width / factor,
            height: physical.height / factor,
        };
        let mut vertical = Vec::new();
        let mut horizontal = None;
        if let Some(metrics) = frame.horizontal_for_text(identity.id) {
            let position = crate::layout::aligned_scroll_offset(
                hint.horizontal(),
                rect.x,
                rect.x + rect.width,
                metrics.viewport.x,
                metrics.viewport.x + metrics.viewport.width,
                metrics.offset,
                metrics.max,
            );
            if position != metrics.offset {
                horizontal = Some(position);
                rect.x -= position - metrics.offset;
            }
        }
        if element.tag == Tag::Textarea
            && let Some(metrics) = frame.vertical_for_text(identity.id)
        {
            let top = crate::layout::aligned_scroll_offset(
                hint.vertical(),
                rect.y,
                rect.y + rect.height,
                metrics.viewport.y,
                metrics.viewport.y + metrics.viewport.height,
                metrics.top,
                metrics.max,
            );
            if top != metrics.top {
                vertical.push((
                    identity.id,
                    f64::from(top),
                    frame
                        .scroll_metrics(identity.id)
                        .expect("text scroll metrics"),
                ));
                rect.y -= top - metrics.top;
            }
        }
        for (id, top) in crate::layout::scroll_path_plan(
            frame.layout_for_scroll(),
            self.tree.path(identity.id).into_iter().skip(1),
            rect,
            hint.vertical(),
        ) {
            vertical.push((
                id,
                top,
                frame.layout_for_scroll()[&id]
                    .programmatic_scroll
                    .expect("planned ancestor scroll"),
            ));
        }
        if horizontal.is_none() && vertical.is_empty() {
            return Ok(true);
        }
        let mut next = self.clone();
        if let Some(offset) = horizontal {
            next.tree
                .element_mut(identity.id)
                .expect("validated text field")
                .editor
                .horizontal_scroll = Some(offset);
        }
        for (id, top, metrics) in vertical {
            next.assistive_scroll(id, top, metrics)?;
        }
        *self = next;
        Ok(true)
    }
}
