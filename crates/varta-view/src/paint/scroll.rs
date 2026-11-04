//! Scroll geometry retained in CSS coordinates from the presented layout.
use super::*;
use crate::{AccessibilityTextRevision, NodeIdentity};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Source {
    identity: NodeIdentity,
    parent: Option<u32>,
    top: f64,
    text: Option<AccessibilityTextRevision>,
    horizontal: Option<f32>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Horizontal {
    pub offset: f32,
    pub max: f32,
    pub viewport: BoxRect,
}
pub(super) fn sources(
    tree: &Tree,
    geometry: &BTreeMap<u32, layout::BoxGeometry>,
) -> BTreeMap<u32, Source> {
    geometry
        .keys()
        .map(|&id| {
            let node = &tree.nodes[&id];
            let element = tree.element(id);
            (
                id,
                Source {
                    identity: NodeIdentity {
                        id,
                        token: node.identity.clone(),
                    },
                    parent: node.parent,
                    top: element.map_or(0.0, |e| e.scroll_top),
                    horizontal: element.and_then(|e| e.editor.horizontal_scroll),
                    text: element
                        .filter(|e| e.text_field())
                        .map(|e| AccessibilityTextRevision::new(node, e)),
                },
            )
        })
        .collect()
}
impl FrameMetadata {
    /// Whether this source node has an ancestor with a programmatic vertical
    /// scroll range in this frame. The target itself need not be onscreen.
    #[must_use]
    pub fn scroll_into_view_supported(&self, node: u32) -> bool {
        if !self.scroll_layout.contains_key(&node) {
            return false;
        }
        let mut current = self
            .scroll_sources
            .get(&node)
            .and_then(|source| source.parent);
        while let Some(id) = current {
            if self
                .scroll_layout
                .get(&id)
                .and_then(|g| g.programmatic_scroll)
                .is_some_and(|s| s.max > 0.0)
            {
                return true;
            }
            current = self
                .scroll_sources
                .get(&id)
                .and_then(|source| source.parent);
        }
        false
    }
    pub(crate) fn scroll_path_current(&self, tree: &Tree, target: u32) -> bool {
        tree.path(target).into_iter().all(|id| {
            let Some(source) = self.scroll_sources.get(&id) else {
                return false;
            };
            let Some(node) = tree.nodes.get(&id) else {
                return false;
            };
            let element = tree.element(id);
            node.parent == source.parent
                && std::sync::Arc::ptr_eq(&node.identity, &source.identity.token)
                && element.map_or(0.0, |e| e.scroll_top) == source.top
                && element.and_then(|e| e.editor.horizontal_scroll) == source.horizontal
                && element
                    .filter(|e| e.text_field())
                    .map(|e| AccessibilityTextRevision::new(node, e))
                    == source.text
        })
    }
    pub(crate) fn layout_for_scroll(&self) -> &BTreeMap<u32, layout::BoxGeometry> {
        &self.scroll_layout
    }
    pub(crate) fn scroll_item_height(&self, node: u32, fallback: f32) -> f32 {
        self.text_hits
            .get(&node)
            .and_then(|hit| hit.lines.first())
            .map_or(fallback, |line| line.bounds.height / self.scale)
    }
    pub(crate) fn vertical_for_text(&self, node: u32) -> Option<ScrollMetrics> {
        let hit = self.text_hits.get(&node)?;
        Some(ScrollMetrics {
            viewport: hit.text_viewport,
            ..hit.scroll?
        })
    }
    pub(crate) fn horizontal_for_text(&self, node: u32) -> Option<Horizontal> {
        self.text_hits.get(&node)?.horizontal
    }
    /// Whether text ranges can be revealed inside their field or ancestors.
    #[must_use]
    pub fn text_reveal_supported(
        &self,
        identity: &NodeIdentity,
        revision: &AccessibilityTextRevision,
    ) -> bool {
        self.field_text_geometry(identity, revision).is_some()
            && (self.scroll_into_view_supported(identity.id)
                || self
                    .scroll_metrics(identity.id)
                    .is_some_and(|m| m.max > 0.0)
                || self
                    .horizontal_for_text(identity.id)
                    .is_some_and(|m| m.max > 0.0))
    }
}
