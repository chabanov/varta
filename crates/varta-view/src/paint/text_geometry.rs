//! The same shaped clusters used for field painting and pointer navigation.
use crate::{
    AccessibilityError, AccessibilityTextRevision, AccessibilityWork, BoxRect, NodeIdentity,
};
use std::ops::Range;

/// One shaped cluster in visual order. Source ranges address original UTF-8,
/// including for password masks; geometry is in physical viewport pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldTextCluster {
    pub source: Range<usize>,
    pub bounds: BoxRect,
    pub rtl: bool,
    pub line_break: bool,
}
impl FieldTextCluster {
    pub(super) fn midpoint(&self) -> f32 {
        self.bounds.x + self.bounds.width / 2.0
    }
    pub(super) fn before(&self) -> usize {
        if self.rtl {
            self.source.end
        } else {
            self.source.start
        }
    }
    pub(super) fn after(&self) -> usize {
        if self.rtl {
            self.source.start
        } else {
            self.source.end
        }
    }
}

/// One visual line, including soft wrapping. Its source range includes a hard
/// break, while content_end is the keyboard End position before that break.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldTextLine {
    pub source: Range<usize>,
    pub content_end: usize,
    pub bounds: BoxRect,
    pub clusters: Range<usize>,
}

/// Borrowed geometry from a validated matching frame. Bounds retain the full
/// shaped advances; clip separately describes the visible field content.
#[derive(Debug, Clone, Copy)]
pub struct FieldTextGeometry<'a> {
    pub clusters: &'a [FieldTextCluster],
    pub lines: &'a [FieldTextLine],
    pub clip: BoxRect,
    pub source_len: usize,
    /// Painted anchor and focus caret locations, including soft-wrap affinity.
    pub selection_carets: [BoxRect; 2],
    pub(super) clips: Option<&'a crate::render::clips::Clip>,
}

impl FieldTextGeometry<'_> {
    /// Visible physical bounds, separate from full shaped advances. Empty
    /// intersections have no visible bounds, including fully hidden carets.
    pub fn visible_bounds(
        &self,
        bounds: BoxRect,
        work: &mut AccessibilityWork,
    ) -> Result<Option<BoxRect>, AccessibilityError> {
        visible_bounds(bounds, self.clip, self.clips, work)
    }
}
pub(super) fn visible_bounds(
    bounds: BoxRect,
    clip: BoxRect,
    clips: Option<&crate::render::clips::Clip>,
    work: &mut AccessibilityWork,
) -> Result<Option<BoxRect>, AccessibilityError> {
    work.charge(1)?;
    if ![
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        bounds.x + bounds.width,
        bounds.y + bounds.height,
    ]
    .into_iter()
    .all(f32::is_finite)
        || bounds.width < 0.0
        || bounds.height < 0.0
    {
        return Ok(None);
    }
    let x = bounds.x.max(clip.x);
    let y = bounds.y.max(clip.y);
    let right = (bounds.x + bounds.width).min(clip.x + clip.width);
    let bottom = (bounds.y + bounds.height).min(clip.y + clip.height);
    if right < x || bottom < y {
        return Ok(None);
    }
    let bounds = BoxRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    };
    match clips {
        Some(clips) => clips.visible_bounds(bounds, work),
        None => Ok(Some(bounds)),
    }
}

impl super::FrameMetadata {
    pub(crate) fn text_range_bounds(
        &self,
        identity: &NodeIdentity,
        revision: &AccessibilityTextRevision,
        start: usize,
        end: usize,
    ) -> Option<BoxRect> {
        let geometry = self.field_text_geometry(identity, revision)?;
        let mut result: Option<BoxRect> = None;
        for cluster in geometry
            .clusters
            .iter()
            .filter(|c| c.source.start < end && c.source.end > start)
        {
            result = Some(match result {
                None => cluster.bounds,
                Some(old) => {
                    let x = old.x.min(cluster.bounds.x);
                    let y = old.y.min(cluster.bounds.y);
                    BoxRect {
                        x,
                        y,
                        width: (old.x + old.width).max(cluster.bounds.x + cluster.bounds.width) - x,
                        height: (old.y + old.height).max(cluster.bounds.y + cluster.bounds.height)
                            - y,
                    }
                }
            });
        }
        if result.is_some() {
            return result;
        }
        if let Some(line) = geometry
            .lines
            .iter()
            .find(|line| line.source.is_empty() && line.source.start == start)
        {
            return Some(line.bounds);
        }
        let next = geometry
            .clusters
            .iter()
            .filter(|c| c.source.end > start)
            .min_by_key(|c| c.source.start);
        let previous = geometry
            .clusters
            .iter()
            .filter(|c| c.source.end <= start)
            .max_by_key(|c| c.source.end);
        let (cluster, leading) = next
            .map(|c| (c, true))
            .or_else(|| previous.map(|c| (c, false)))?;
        Some(BoxRect {
            x: if leading != cluster.rtl {
                cluster.bounds.x
            } else {
                cluster.bounds.x + cluster.bounds.width
            },
            width: 0.0,
            ..cluster.bounds
        })
    }
    /// No geometry is returned for replaced content, a different source node or
    /// transient IME display. This read never reshapes or mutates the field.
    #[must_use]
    pub fn field_text_geometry(
        &self,
        identity: &NodeIdentity,
        revision: &AccessibilityTextRevision,
    ) -> Option<FieldTextGeometry<'_>> {
        let hit = self.text_hits.get(&identity.id)?;
        if hit.composing
            || hit.lines.is_empty()
            || hit.identity != *identity
            || hit.revision != *revision
        {
            return None;
        }
        Some(FieldTextGeometry {
            clusters: &hit.clusters,
            lines: &hit.lines,
            clip: hit.clip,
            source_len: hit.end,
            selection_carets: hit.selection_carets,
            clips: hit.clips.as_deref(),
        })
    }
}
