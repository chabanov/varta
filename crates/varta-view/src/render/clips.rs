//! Shared physical clip chains and bounded reusable software masks.
use crate::{BoxRect, PaintError, paint::corners::Corners};
use std::sync::Arc;
use tiny_skia::{FillRule, Mask, Path, PathBuilder, Rect, Transform};

#[derive(Debug)]
pub(crate) struct Clip {
    pub parent: Option<Arc<Self>>,
    pub path: Option<Arc<Path>>,
    pub bounds: Option<Rect>,
    pub(super) logical_bounds: Option<Rect>,
    pub(super) rect: BoxRect,
    pub(super) corners: Corners,
    pub bytes: usize,
}
impl PartialEq for Clip {
    fn eq(&self, other: &Self) -> bool {
        self.rect == other.rect
            && self.corners == other.corners
            && self.logical_bounds == other.logical_bounds
            && (match (&self.path, &other.path) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
                _ => false,
            })
            && same(&self.parent, &other.parent)
    }
}
pub(crate) fn same(left: &Option<Arc<Clip>>, right: &Option<Arc<Clip>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
        _ => false,
    }
}
impl Clip {
    /// Reuse validated source path storage while changing only inherited scope.
    pub(crate) fn reparent(source: &Arc<Self>, parent: Option<Arc<Self>>) -> Arc<Self> {
        let bounds = source.path.as_ref().and_then(|path| match &parent {
            Some(parent) => parent.bounds.and_then(|b| b.intersect(&path.bounds())),
            None => Some(path.bounds()),
        });
        let bytes = std::mem::size_of::<Self>()
            + source
                .path
                .as_ref()
                .map_or(0, |p| std::mem::size_of_val(p.points()) + p.verbs().len())
            + parent.as_ref().map_or(0, |p| p.bytes);
        let logical = source
            .logical_bounds
            .and_then(|_| logical_bounds(&parent, source.rect));
        Arc::new(Self {
            parent,
            path: source.path.clone(),
            bounds,
            logical_bounds: logical,
            rect: source.rect,
            corners: source.corners,
            bytes,
        })
    }
    pub fn new(
        parent: Option<Arc<Self>>,
        path: Option<Path>,
        rect: BoxRect,
        corners: Corners,
    ) -> Arc<Self> {
        let logical = path.as_ref().and_then(|_| logical_bounds(&parent, rect));
        Self::build(parent, path, rect, corners, logical)
    }
    pub(crate) fn with_paint(
        parent: Option<Arc<Self>>,
        path: Option<Path>,
        rect: BoxRect,
        corners: Corners,
    ) -> Arc<Self> {
        let logical = logical_bounds(&parent, rect);
        Self::build(parent, path, rect, corners, logical)
    }
    fn build(
        parent: Option<Arc<Self>>,
        path: Option<Path>,
        rect: BoxRect,
        corners: Corners,
        logical: Option<Rect>,
    ) -> Arc<Self> {
        // Repeating the same geometric intersection is idempotent. Reusing the
        // scope also avoids multiplying identical AA fringes and observer work.
        if let Some(previous) = &parent
            && previous.rect == rect
            && previous.corners == corners
            && previous.logical_bounds == logical
            && previous.path.as_deref() == path.as_ref()
        {
            return previous.clone();
        }
        let bounds = path.as_ref().and_then(|path| match &parent {
            Some(p) => p.bounds.and_then(|b| b.intersect(&path.bounds())),
            None => Some(path.bounds()),
        });
        let bytes = std::mem::size_of::<Self>()
            + path
                .as_ref()
                .map_or(0, |p| std::mem::size_of_val(p.points()) + p.verbs().len())
            + parent.as_ref().map_or(0, |p| p.bytes);
        Arc::new(Self {
            parent,
            path: path.map(Arc::new),
            bounds,
            logical_bounds: logical,
            rect,
            corners,
            bytes,
        })
    }
    /// Strict interior test with room for AA and source-coordinate rounding.
    /// The rounded rectangle is convex, so its four safety-box vertices suffice.
    pub fn contains_rect(&self, rect: Rect) -> bool {
        let mut current = Some(self);
        while let Some(clip) = current {
            if clip.bounds.is_none() {
                return false;
            }
            let extent = [
                clip.rect.x.abs(),
                clip.rect.y.abs(),
                clip.rect.width,
                clip.rect.height,
            ]
            .into_iter()
            .chain(clip.corners.0.into_iter().flatten())
            .fold(0.0_f32, f32::max);
            let margin = 2.0 + extent * f32::EPSILON * 4.0;
            let Some(safe) = rect.outset(margin, margin) else {
                return false;
            };
            if ![
                (safe.left(), safe.top()),
                (safe.right(), safe.top()),
                (safe.right(), safe.bottom()),
                (safe.left(), safe.bottom()),
            ]
            .into_iter()
            .all(|(x, y)| clip.corners.contains(clip.rect, x, y))
            {
                return false;
            }
            current = clip.parent.as_deref();
        }
        true
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        if self.logical_bounds.is_none() {
            return false;
        }
        let mut current = Some(self);
        while let Some(clip) = current {
            if !clip.corners.contains(clip.rect, x, y) {
                return false;
            }
            current = clip.parent.as_deref();
        }
        true
    }
}
fn logical_bounds(parent: &Option<Arc<Clip>>, rect: BoxRect) -> Option<Rect> {
    let rect = Rect::from_xywh(rect.x, rect.y, rect.width, rect.height)?;
    match parent {
        Some(parent) => parent.logical_bounds.and_then(|p| p.intersect(&rect)),
        None => Some(rect),
    }
}

/// Three viewport masks, independent of clip depth. Parent paths are multiplied
/// in tree order; the explicit rectangular draw clip is applied last. No SDK
/// intersect_path temporary is allocated for every ancestor or draw.
pub(crate) struct MaskCache {
    base: Mask,
    scratch: Mask,
    composite: Mask,
    chain: Option<Arc<Clip>>,
    rectangle: Option<Rect>,
}
impl MaskCache {
    pub fn new(width: u32, height: u32) -> Result<Self, PaintError> {
        Ok(Self {
            base: Mask::new(width, height).ok_or(PaintError::Dimensions)?,
            scratch: Mask::new(width, height).ok_or(PaintError::Dimensions)?,
            composite: Mask::new(width, height).ok_or(PaintError::Dimensions)?,
            chain: None,
            rectangle: None,
        })
    }
    pub fn mask(&mut self, chain: &Arc<Clip>, rectangle: Option<Rect>) -> &Mask {
        if !same(&self.chain, &Some(chain.clone())) {
            self.base.clear();
            let mut parents = Vec::new();
            let mut current = Some(chain.as_ref());
            while let Some(clip) = current {
                parents.push(clip);
                current = clip.parent.as_deref();
            }
            if chain.bounds.is_some() {
                for (index, clip) in parents.into_iter().rev().enumerate() {
                    let Some(path) = clip.path.as_ref() else {
                        self.base.clear();
                        break;
                    };
                    if index == 0 {
                        self.base
                            .fill_path(path, FillRule::Winding, true, Transform::identity());
                    } else {
                        self.scratch.clear();
                        self.scratch.fill_path(
                            path,
                            FillRule::Winding,
                            true,
                            Transform::identity(),
                        );
                        multiply(&mut self.base, &self.scratch);
                    }
                }
            }
            self.chain = Some(chain.clone());
            self.rectangle = None;
        }
        if chain.bounds.is_none() {
            return &self.base;
        }
        let Some(rectangle) = rectangle else {
            return &self.base;
        };
        if self.rectangle != Some(rectangle) {
            self.scratch.clear();
            self.scratch.fill_path(
                &PathBuilder::from_rect(rectangle),
                FillRule::Winding,
                true,
                Transform::identity(),
            );
            self.composite.data_mut().copy_from_slice(self.base.data());
            multiply(&mut self.composite, &self.scratch);
            self.rectangle = Some(rectangle);
        }
        &self.composite
    }
}
fn multiply(destination: &mut Mask, source: &Mask) {
    for (a, b) in destination.data_mut().iter_mut().zip(source.data()) {
        *a = ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn circle(parent: Option<Arc<Clip>>, x: f32) -> Arc<Clip> {
        let rect = BoxRect {
            x,
            y: 1.0,
            width: 8.0,
            height: 8.0,
        };
        let corners = Corners([[4.0; 2]; 4]);
        Clip::new(parent, corners.path(rect), rect, corners)
    }
    fn expected(chain: &Arc<Clip>, rectangle: Option<Rect>) -> Mask {
        let mut paths = Vec::new();
        let mut next = Some(chain.as_ref());
        while let Some(clip) = next {
            paths.push(clip);
            next = clip.parent.as_deref();
        }
        let mut mask = Mask::new(12, 12).unwrap();
        for (index, clip) in paths.into_iter().rev().enumerate() {
            if index == 0 {
                mask.fill_path(
                    clip.path.as_ref().unwrap(),
                    FillRule::Winding,
                    true,
                    Transform::identity(),
                );
            } else {
                mask.intersect_path(
                    clip.path.as_ref().unwrap(),
                    FillRule::Winding,
                    true,
                    Transform::identity(),
                );
            }
        }
        if let Some(rect) = rectangle {
            mask.intersect_path(
                &PathBuilder::from_rect(rect),
                FillRule::Winding,
                true,
                Transform::identity(),
            );
        }
        mask
    }
    #[test]
    fn reusable_masks_match_sdk_intersection_and_reset_the_rectangle_after_chain_changes() {
        let first = circle(None, 1.0);
        let nested = circle(Some(first.clone()), 3.0);
        let rect = Rect::from_xywh(2.25, 2.5, 5.5, 4.0).unwrap();
        let mut cache = MaskCache::new(12, 12).unwrap();
        let mut address = None;
        for (chain, rectangle) in [
            (&first, Some(rect)),
            (&nested, Some(rect)),
            (&nested, None),
            (&first, Some(rect)),
            (&first, Some(rect)),
        ] {
            let actual = cache.mask(chain, rectangle);
            assert_eq!(actual.data(), expected(chain, rectangle).data());
            if rectangle.is_some() {
                let current = actual.data().as_ptr() as usize;
                if let Some(address) = address {
                    assert_eq!(address, current);
                } else {
                    address = Some(current);
                }
            }
        }
    }
    #[test]
    fn empty_clip_cannot_reuse_prior_coverage_or_accept_a_hit() {
        let first = circle(None, 1.0);
        let mut cache = MaskCache::new(12, 12).unwrap();
        assert!(
            cache
                .mask(&first, None)
                .data()
                .iter()
                .any(|alpha| *alpha != 0)
        );
        let empty = Clip::new(
            Some(first),
            None,
            BoxRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 8.0,
            },
            Corners::default(),
        );
        assert!(
            cache
                .mask(&empty, None)
                .data()
                .iter()
                .all(|alpha| *alpha == 0)
        );
        assert!(!empty.contains(1.0, 2.0));
    }
    #[test]
    fn maximum_tree_depth_uses_the_same_bounded_mask_storage() {
        let mut chain = circle(None, 1.0);
        for depth in 1..varta_common::view::DRAFT_MAX_TREE_DEPTH {
            chain = circle(Some(chain), 1.0 + depth as f32 / 10_000.0);
        }
        let mut cache = MaskCache::new(12, 12).unwrap();
        let mask = cache.mask(&chain, None);
        assert_eq!(mask.data().len(), 144);
        assert_eq!(mask.data()[5 * 12 + 5], 255);
        assert_eq!(mask.data()[0], 0);
        assert!(chain.contains(5.0, 5.0));
        assert!(!chain.contains(1.0, 1.0));
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn repeated_identical_clip_reuses_scope_and_does_not_square_coverage() {
        let rect = BoxRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let corners = Corners([[5.0; 2]; 4]);
        let first = Clip::new(None, corners.path(rect), rect, corners);
        let repeated = Clip::new(Some(first.clone()), corners.path(rect), rect, corners);
        assert!(Arc::ptr_eq(&first, &repeated));
        let empty = Clip::new(Some(first.clone()), None, rect, corners);
        assert!(!Arc::ptr_eq(&first, &empty));
    }
}
