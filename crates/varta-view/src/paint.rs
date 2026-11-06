//! Bounded CPU painting for an explicitly limited subset of the view tree.
pub(crate) mod border;
mod border_partition;
mod clip_curve;
mod corner_arc;
mod corner_clip;
pub(crate) mod corners;
mod curve_region;
mod groups;
mod inline_decor;
mod inline_groups;
mod inline_outline;
mod order;
mod outline_curve;
mod outline_union;
mod scroll;
mod scrollbar;
mod select;
mod snap;
mod text_geometry;
mod theme;
mod underline;
use crate::TextMeasurer;
use crate::layout::{self, BoxRect, LayoutError, ScrollMetrics, TextShape};
use crate::render::clips::{Clip, MaskCache};
use crate::render::glyphs::{self, Glyphs};
use crate::render::scene::{self, Draw, Primitive, Recording, Scene};
use crate::style::{self, Environment, Name};
use crate::text::{TextSource, WhiteSpaceMode, field_display};
use crate::tree::{Content, Tree, border_color_sides, outline_components};
use crate::units::Units;
use corners::Corners;
use parley::{Affinity, Cursor, Layout, PositionedLayoutItem, Selection};
pub use select::{SelectHit, SelectMenuMetrics};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    outline::{DrawSettings, OutlinePen},
};
use std::{borrow::Cow, collections::BTreeMap};
pub use text_geometry::{FieldTextCluster, FieldTextGeometry, FieldTextLine};
use tiny_skia::{Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Shader, Transform};
use varta_common::view::{Attribute, CssProperty, ROOT_NODE, Tag};

/// A transparent, premultiplied RGBA8 image in row-major order.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterImage {
    pub pixels: Vec<u8>,
    metadata: FrameMetadata,
}
impl std::ops::Deref for RasterImage {
    type Target = FrameMetadata;
    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}
impl RasterImage {
    pub fn into_metadata(self) -> FrameMetadata {
        self.metadata
    }
    pub(crate) fn from_parts(metadata: FrameMetadata, pixels: Vec<u8>) -> Self {
        Self { metadata, pixels }
    }
}

/// Input and editing geometry, independent of a pixel or GPU backend.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameMetadata {
    pub width: u32,
    pub height: u32,
    scale: f32,
    hit_regions: Vec<(u32, BoxRect)>,
    /// Hit regions of modal dialogs' backdrops: they take pointers for the
    /// dialog but are not part of its box (no accessibility bounds).
    backdrops: std::collections::BTreeSet<usize>,
    // Only the element's own border-box hit is rounded. Visible overflowing
    // descendant/text fragments retain their independent hit rectangles.
    rounded_regions: BTreeMap<usize, inline_decor::Region>,
    hit_clips: BTreeMap<usize, std::sync::Arc<Clip>>,
    ancestors: BTreeMap<u32, Option<u32>>,
    scrolls: BTreeMap<u32, ScrollMetrics>,
    focused: Option<u32>,
    editable: Option<u32>,
    text_hits: BTreeMap<u32, TextHit>,
    select_menu: Option<Box<select::SelectMenu>>,
    cursors: BTreeMap<u32, PointerCursor>,
    scroll_layout: BTreeMap<u32, layout::BoxGeometry>,
    scroll_sources: BTreeMap<u32, scroll::Source>,
}

/// The four cursor values admitted by the checked CSS subset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PointerCursor {
    #[default]
    Default,
    Pointer,
    Text,
    NotAllowed,
}

#[derive(Debug, Clone, PartialEq)]
struct TextHit {
    identity: crate::NodeIdentity,
    revision: crate::AccessibilityTextRevision,
    clip: BoxRect,
    clips: Option<std::sync::Arc<Clip>>,
    selection_carets: [BoxRect; 2],
    clusters: Vec<FieldTextCluster>,
    end: usize,
    composing: bool,
    caret_area: Option<BoxRect>,
    lines: Vec<FieldTextLine>,
    scroll: Option<ScrollMetrics>,
    horizontal: Option<scroll::Horizontal>,
    text_viewport: BoxRect,
    caret_y: f32,
    origin_x: f32,
    preferred_x: f32,
    multiline: bool,
}
/// A visual caret movement computed from one rendered frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextNavigation {
    pub position: usize,
    pub preferred_x: Option<f32>,
    pub upstream: bool,
}
fn pointer_cursor(tree: &Tree, env: &Environment, id: u32) -> PointerCursor {
    for ancestor in tree.path(id) {
        if let Some(value) = tree.computed_read(ancestor, env).get(&CssProperty::Cursor) {
            return match value.as_str() {
                "pointer" => PointerCursor::Pointer,
                "text" => PointerCursor::Text,
                "not-allowed" => PointerCursor::NotAllowed,
                _ => PointerCursor::Default,
            };
        }
    }
    match tree.element(id) {
        Some(element) if element.disabled() => PointerCursor::NotAllowed,
        Some(element) if element.text_field() => PointerCursor::Text,
        _ => PointerCursor::Default,
    }
}
impl FrameMetadata {
    /// Whether a repaint keeps every input target and editing metric intact.
    /// Cosmetic pointer feedback need not invalidate an in-progress click.
    /// Hosts must also retain their model revision and element-identity guards.
    #[must_use]
    pub fn same_input_geometry(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.scale == other.scale
            && self.hit_regions == other.hit_regions
            && self.backdrops == other.backdrops
            && self.rounded_regions == other.rounded_regions
            && self.hit_clips == other.hit_clips
            && self.ancestors == other.ancestors
            && self.scrolls == other.scrolls
            && self.focused == other.focused
            && self.editable == other.editable
            && self.text_hits == other.text_hits
            && self.select_menu == other.select_menu
            && self.scroll_layout == other.scroll_layout
            && self.scroll_sources == other.scroll_sources
    }
    #[must_use]
    pub fn cursor_at(&self, x: f32, y: f32) -> PointerCursor {
        if let Some(hit) = self.select_hit(x, y) {
            return match hit {
                SelectHit::Choice { .. } => PointerCursor::Pointer,
                SelectHit::Disabled { .. } => PointerCursor::NotAllowed,
                SelectHit::Dismiss { .. } => PointerCursor::Default,
            };
        }
        self.hit_test(x, y)
            .and_then(|id| self.cursors.get(&id).copied())
            .unwrap_or_default()
    }
    /// Native select-list hit testing consumes clicks even outside the list.
    #[must_use]
    pub fn select_hit(&self, x: f32, y: f32) -> Option<SelectHit> {
        self.select_menu.as_ref()?.hit(x, y)
    }
    #[must_use]
    pub fn select_menu(&self) -> Option<SelectMenuMetrics> {
        self.select_menu.as_ref().map(|menu| menu.metrics)
    }
    /// Physical pixels per CSS pixel used for this frame.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }
    /// Visible element-fragment unions, collected once for platform adapters.
    /// Physical pixels; offscreen and text-only nodes can have no visible bounds.
    #[must_use]
    pub fn node_bounds_map(&self) -> BTreeMap<u32, BoxRect> {
        let mut bounds = BTreeMap::<u32, BoxRect>::new();
        for (index, &(id, rect)) in self.hit_regions.iter().enumerate() {
            if self.backdrops.contains(&index) {
                continue;
            }
            bounds
                .entry(id)
                .and_modify(|previous| {
                    let x = previous.x.min(rect.x);
                    let y = previous.y.min(rect.y);
                    previous.width = (previous.x + previous.width).max(rect.x + rect.width) - x;
                    previous.height = (previous.y + previous.height).max(rect.y + rect.height) - y;
                    previous.x = x;
                    previous.y = y;
                })
                .or_insert(rect);
        }
        bounds.insert(
            ROOT_NODE,
            BoxRect {
                x: 0.0,
                y: 0.0,
                width: self.width as f32,
                height: self.height as f32,
            },
        );
        bounds
    }
    /// Focused element if it participates in this rendered frame.
    #[must_use]
    pub fn focused(&self) -> Option<u32> {
        self.focused
    }

    /// Focused enabled text input that accepts edits in this frame.
    #[must_use]
    pub fn editable(&self) -> Option<u32> {
        self.editable
    }
    /// Visible cursor area for the focused editable field, in physical pixels.
    #[must_use]
    pub fn ime_cursor_area(&self) -> Option<BoxRect> {
        self.text_hits.get(&self.editable?)?.caret_area
    }

    /// Resolve an x coordinate to a shaped text field's UTF-8 byte position.
    #[must_use]
    pub fn text_position(&self, node: u32, x: f32) -> Option<usize> {
        if !x.is_finite() {
            return None;
        }
        let hit = self.text_hits.get(&node)?;
        if hit.lines.len() > 1 {
            return None;
        }
        if hit.composing {
            return None;
        }
        let mut last = 0;
        for cluster in hit.clusters.iter().filter(|c| !c.line_break) {
            if x < cluster.midpoint() {
                return Some(cluster.before());
            }
            last = cluster.after();
        }
        Some(if hit.clusters.is_empty() {
            hit.end
        } else {
            last
        })
    }
    /// Resolve physical coordinates to a byte position in a text input or textarea.
    #[must_use]
    pub fn text_position_at(&self, node: u32, x: f32, y: f32) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let hit = self.text_hits.get(&node)?;
        if hit.composing {
            return None;
        }
        let line = hit
            .lines
            .iter()
            .find(|line| y < line.bounds.y + line.bounds.height)
            .or_else(|| hit.lines.last())?;
        let mut last = line.source.start;
        for cluster in hit.clusters[line.clusters.clone()]
            .iter()
            .filter(|c| !c.line_break)
        {
            if x < cluster.midpoint() {
                return Some(cluster.before());
            }
            last = cluster.after();
        }
        Some(last)
    }
    /// Compute unmodified vertical/Home/End movement for a rendered textarea.
    #[must_use]
    pub fn text_navigation(&self, node: u32, key: &str) -> Option<TextNavigation> {
        let hit = self.text_hits.get(&node)?;
        if !hit.multiline || hit.composing {
            return None;
        }
        let current = hit
            .lines
            .iter()
            .position(|line| hit.caret_y < line.bounds.y + line.bounds.height)
            .unwrap_or(hit.lines.len().checked_sub(1)?);
        let (line, boundary, preferred_x) = match key {
            "Home" => (current, Some(hit.lines[current].source.start), None),
            "End" => (current, Some(hit.lines[current].content_end), None),
            "ArrowUp" if current == 0 => (0, Some(0), Some(hit.preferred_x)),
            "ArrowDown" if current + 1 == hit.lines.len() => {
                (current, Some(hit.end), Some(hit.preferred_x))
            }
            "ArrowUp" => (current - 1, None, Some(hit.preferred_x)),
            "ArrowDown" => (current + 1, None, Some(hit.preferred_x)),
            _ => return None,
        };
        let destination = &hit.lines[line];
        let position = boundary.or_else(|| {
            self.text_position_at(
                node,
                hit.origin_x + hit.preferred_x * self.scale,
                destination.bounds.y + destination.bounds.height / 2.0,
            )
        })?;
        let upstream = position == destination.content_end
            && hit
                .lines
                .get(line + 1)
                .is_some_and(|next| next.source.start == position);
        Some(TextNavigation {
            position,
            preferred_x,
            upstream,
        })
    }
    /// Find the topmost supported element at a viewport-relative pixel coordinate.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<u32> {
        if let Some(menu) = &self.select_menu
            && select::contains(menu.metrics.viewport, x, y)
        {
            return Some(menu.metrics.target);
        }
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.hit_regions
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, (id, rect))| {
                (x >= rect.x
                    && y >= rect.y
                    && x < rect.x + rect.width
                    && y < rect.y + rect.height
                    && self
                        .rounded_regions
                        .get(&index)
                        .is_none_or(|region| region.contains(x, y, self.scale))
                    && self
                        .hit_clips
                        .get(&index)
                        .is_none_or(|clip| clip.contains(x, y)))
                .then_some(*id)
            })
    }

    /// Pick the closest ancestor by physical coordinates; delta and returned
    /// offsets remain in CSS pixels.
    #[must_use]
    pub fn scroll_target(&self, x: f32, y: f32, delta: f32) -> Option<(u32, f64, f32)> {
        if !delta.is_finite() || delta == 0.0 {
            return None;
        }
        let mut node = self.hit_test(x, y)?;
        loop {
            if let Some(scroll) = self.scrolls.get(&node) {
                let top = (scroll.top + delta).clamp(0.0, scroll.max);
                if top != scroll.top {
                    return Some((node, f64::from(top), delta - (top - scroll.top)));
                }
            }
            node = self.ancestors.get(&node).copied().flatten()?;
        }
    }
    /// CSS-pixel scroll metrics from this frame, including textarea text.
    #[must_use]
    pub fn scroll_metrics(&self, node: u32) -> Option<ScrollMetrics> {
        self.scrolls.get(&node).copied()
    }
}

// Existing native preparation allowance, shared by every text paint in a frame.
pub(crate) const MAX_TEXT_PAINT_WORK: usize = 1_000_000;

/// Painting rejects effects and content it cannot reproduce.
#[derive(Debug, thiserror::Error)]
pub enum PaintError {
    #[error("invalid raster dimensions or allocation failure")]
    Dimensions,
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error("unsupported paint content or style on node {0}")]
    Unsupported(u32),
}

/// Paint logical geometry directly at device resolution, including glyph paths.
struct ClipMask {
    rect: Rect,
    image: Option<Mask>,
}
struct Surface<'a> {
    /// Open scaling transforms (`begin_transform`).
    transforms: usize,
    glyphs: Option<&'a mut Glyphs>,
    pixels: Option<Pixmap>,
    width: u32,
    height: u32,
    scale: f32,
    commands: Option<Recording>,
    active_clip: Option<std::sync::Arc<Clip>>,
    clip_masks: Option<MaskCache>,
    failure: Option<PaintError>,
    groups: Vec<groups::Scope>,
    relative_clips: BTreeMap<(usize, usize), groups::RelativeClip>,
    isolation_required: bool,
    capture: Option<inline_groups::Capture>,
}
impl Surface<'_> {
    fn push_clip(
        &mut self,
        rect: BoxRect,
        corners: Corners,
        paint_rect: BoxRect,
        paint_corners: Corners,
    ) -> Result<(), PaintError> {
        let path = paint_corners
            .clipped_path(paint_rect)
            .map(|path| {
                path.transform(Transform::from_scale(self.scale, self.scale))
                    .ok_or(PaintError::Dimensions)
            })
            .transpose()?;
        self.active_clip = Some(Clip::with_paint(
            self.active_clip.clone(),
            path,
            scaled_rect(rect, self.scale),
            corners.scaled(self.scale),
        ));
        Ok(())
    }
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn rectangle_mask(&self, rect: Rect) -> Result<ClipMask, PaintError> {
        let mut image = Mask::new(self.width, self.height).ok_or(PaintError::Dimensions)?;
        image.fill_path(
            &PathBuilder::from_rect(rect),
            FillRule::Winding,
            true,
            Transform::from_scale(self.scale, self.scale),
        );
        Ok(ClipMask {
            rect,
            image: Some(image),
        })
    }
    fn mask(&self, rect: Rect) -> Result<ClipMask, PaintError> {
        if self.capture.is_some() || self.commands.is_some() || self.active_clip.is_some() {
            Ok(ClipMask { rect, image: None })
        } else {
            self.rectangle_mask(rect)
        }
    }
    fn record(
        &mut self,
        primitive: Primitive,
        paint: &Paint<'_>,
        transform: Transform,
        mask: Option<&ClipMask>,
    ) {
        if self.failure.is_some()
            || self.groups.last().is_some_and(|scope| scope.hidden)
            || self
                .active_clip
                .as_ref()
                .is_some_and(|clip| clip.bounds.is_none())
        {
            return;
        }
        let Shader::SolidColor(color) = paint.shader else {
            self.failure = Some(PaintError::Unsupported(ROOT_NODE));
            return;
        };
        if let Some(capture) = self.capture.as_mut() {
            if let Err(error) = capture.push(inline_groups::SourceDraw {
                primitive,
                color,
                transform,
                clip: mask.map(|m| m.rect),
                clips: self.active_clip.clone(),
            }) {
                self.failure = Some(error);
            }
            return;
        }
        let bounds = match &primitive {
            Primitive::Rect(rect) => *rect,
            Primitive::Path(path, _) => path.bounds(),
            Primitive::Border(border) => border.path.bounds(),
            Primitive::BeginGroup { .. } | Primitive::EndGroup => {
                unreachable!("group markers are recorded by scope")
            }
        };
        let transform = transform.post_scale(self.scale, self.scale);
        let Some(bounds) = bounds.transform(transform) else {
            self.failure = Some(PaintError::Dimensions);
            return;
        };
        let clip = mask.and_then(|m| {
            m.rect
                .transform(Transform::from_scale(self.scale, self.scale))
        });
        let relative = match self.relative_clip(self.active_clip.clone()) {
            Ok(clip) => clip,
            Err(error) => {
                self.failure = Some(error);
                return;
            }
        };
        let draw = Draw {
            primitive,
            color,
            transform,
            clip,
            clips: relative.filter(|clip| !clip.contains_rect(bounds)),
            bounds,
        };
        if let Some(recording) = self.commands.as_mut()
            && (self.isolation_required || recording.fits(&draw))
        {
            if let Err(error) = recording.push(draw) {
                self.failure = Some(error);
            }
            return;
        }
        // Cache capacity never changes content admission. A large scene flushes
        // to the ordinary full-frame path instead of retaining an unbounded list.
        let Some(mut pixels) = Pixmap::new(self.width, self.height) else {
            self.failure = Some(PaintError::Dimensions);
            return;
        };
        if let Some(recording) = self.commands.take()
            && let Err(error) = scene::replay(&mut pixels, &recording.commands)
        {
            self.failure = Some(error);
            return;
        }
        if let Err(error) = scene::replay(&mut pixels, std::slice::from_ref(&draw)) {
            self.failure = Some(error);
            return;
        }
        self.pixels = Some(pixels);
    }
    /// Opens a scaling transform: what paints until [`Self::end_transform`]
    /// is recorded and then mapped. Returns where its draws start.
    fn begin_transform(&mut self, node: u32) -> Result<usize, PaintError> {
        // An inline capture or a curved clip from outside cannot be mapped.
        if self.capture.is_some() || self.active_clip.is_some() {
            return Err(PaintError::Unsupported(node));
        }
        self.isolation_required = true;
        let recording = self.commands.get_or_insert_with(Recording::default);
        self.transforms += 1;
        Ok(recording.commands.len())
    }
    /// Maps the draws recorded since `start` through `affine` (CSS pixels).
    fn end_transform(&mut self, start: usize, affine: crate::transform::Affine) {
        self.transforms = self.transforms.saturating_sub(1);
        let device = Transform::from_row(
            affine.scale_x,
            0.0,
            0.0,
            affine.scale_y,
            affine.x * self.scale,
            affine.y * self.scale,
        );
        let Some(recording) = self.commands.as_mut() else {
            return;
        };
        for draw in recording.commands.iter_mut().skip(start) {
            draw.transform = device.pre_concat(draw.transform);
            if let Some(bounds) = draw.bounds.transform(device) {
                draw.bounds = bounds;
            }
            draw.clip = draw.clip.and_then(|clip| clip.transform(device));
        }
    }
    fn fallback_mask(&self, mask: Option<&ClipMask>) -> Result<Option<ClipMask>, PaintError> {
        // A mask may have been created before a recording overflow. Subsequent
        // ordinary draws must materialize that same clip rather than drop it.
        mask.filter(|mask| mask.image.is_none())
            .map(|mask| self.rectangle_mask(mask.rect))
            .transpose()
    }
    fn fill_rect(
        &mut self,
        rect: Rect,
        paint: &Paint<'_>,
        transform: Transform,
        mask: Option<&ClipMask>,
    ) {
        if self.failure.is_some() || self.groups.last().is_some_and(|scope| scope.hidden) {
            return;
        }
        if self.capture.is_some() || self.commands.is_some() {
            self.record(Primitive::Rect(rect), paint, transform, mask);
        } else if let Some(chain) = self
            .active_clip
            .as_ref()
            .filter(|clip| {
                !rect
                    .transform(transform.post_scale(self.scale, self.scale))
                    .is_some_and(|bounds| clip.contains_rect(bounds))
            })
            .cloned()
        {
            if self.clip_masks.is_none() {
                match MaskCache::new(self.width, self.height) {
                    Ok(cache) => self.clip_masks = Some(cache),
                    Err(error) => {
                        self.failure = Some(error);
                        return;
                    }
                }
            }
            let rectangle = mask.and_then(|mask| {
                mask.rect
                    .transform(Transform::from_scale(self.scale, self.scale))
            });
            let mask = self
                .clip_masks
                .as_mut()
                .expect("clip scratch was prepared")
                .mask(&chain, rectangle);
            if let Some(pixels) = self.pixels.as_mut() {
                scene::fill_rect(
                    pixels,
                    rect,
                    paint,
                    transform.post_scale(self.scale, self.scale),
                    Some(mask),
                );
            }
        } else {
            let materialized = match self.fallback_mask(mask) {
                Ok(mask) => mask,
                Err(error) => {
                    self.failure = Some(error);
                    return;
                }
            };
            let mask = materialized.as_ref().or(mask);
            let Some(pixels) = self.pixels.as_mut() else {
                return;
            };
            scene::fill_rect(
                pixels,
                rect,
                paint,
                transform.post_scale(self.scale, self.scale),
                mask.and_then(|m| m.image.as_ref()),
            );
        }
    }
    fn fill_path(
        &mut self,
        path: &tiny_skia::Path,
        paint: &Paint<'_>,
        rule: FillRule,
        transform: Transform,
        mask: Option<&ClipMask>,
    ) {
        if self.failure.is_some() || self.groups.last().is_some_and(|scope| scope.hidden) {
            return;
        }
        if self.capture.is_some() || self.commands.is_some() {
            self.record(
                Primitive::Path(std::sync::Arc::new(path.clone()), rule),
                paint,
                transform,
                mask,
            );
        } else if let Some(chain) = self
            .active_clip
            .as_ref()
            .filter(|clip| {
                !path
                    .bounds()
                    .transform(transform.post_scale(self.scale, self.scale))
                    .is_some_and(|bounds| clip.contains_rect(bounds))
            })
            .cloned()
        {
            if self.clip_masks.is_none() {
                match MaskCache::new(self.width, self.height) {
                    Ok(cache) => self.clip_masks = Some(cache),
                    Err(error) => {
                        self.failure = Some(error);
                        return;
                    }
                }
            }
            let rectangle = mask.and_then(|mask| {
                mask.rect
                    .transform(Transform::from_scale(self.scale, self.scale))
            });
            let mask = self
                .clip_masks
                .as_mut()
                .expect("clip scratch was prepared")
                .mask(&chain, rectangle);
            if let Some(pixels) = self.pixels.as_mut() {
                pixels.fill_path(
                    path,
                    paint,
                    rule,
                    transform.post_scale(self.scale, self.scale),
                    Some(mask),
                );
            }
        } else {
            let materialized = match self.fallback_mask(mask) {
                Ok(mask) => mask,
                Err(error) => {
                    self.failure = Some(error);
                    return;
                }
            };
            let mask = materialized.as_ref().or(mask);
            let Some(pixels) = self.pixels.as_mut() else {
                return;
            };
            pixels.fill_path(
                path,
                paint,
                rule,
                transform.post_scale(self.scale, self.scale),
                mask.and_then(|m| m.image.as_ref()),
            );
        }
    }
    fn finish(mut self, mut scene: Option<&mut Scene>) -> Result<Vec<u8>, PaintError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if let Some(recording) = self.commands.take() {
            if recording.cacheable
                && self.pixels.is_none()
                && let Some(scene) = scene.as_deref_mut()
            {
                return scene.prepare(recording.commands, self.width, self.height);
            }
            let mut pixels = self
                .pixels
                .take()
                .or_else(|| Pixmap::new(self.width, self.height))
                .ok_or(PaintError::Dimensions)?;
            // The materialized prefix already contains its clips; release its
            // scratch masks before allocating the suffix compositor's masks.
            self.clip_masks = None;
            scene::replay(&mut pixels, &recording.commands)?;
            if let Some(scene) = scene.as_deref_mut() {
                scene.reset(self.width, self.height);
            }
            return Ok(pixels.take());
        }
        if let Some(scene) = scene {
            scene.reset(self.width, self.height);
        }
        Ok(self.pixels.take().ok_or(PaintError::Dimensions)?.take())
    }
}

fn scaled_rect(rect: BoxRect, scale: f32) -> BoxRect {
    BoxRect {
        x: rect.x * scale,
        y: rect.y * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

/// A CSS color value of the view subset (also the transitions' parser).
pub(crate) fn parse_color(value: &str) -> Option<Color> {
    color(value)
}

fn color(value: &str) -> Option<Color> {
    if value == "transparent" {
        return Some(Color::from_rgba8(0, 0, 0, 0));
    }
    if let Some(hex) = value.strip_prefix('#') {
        let digit = |i| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok();
        let pair = |i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        return match hex.len() {
            3 => Some(Color::from_rgba8(
                digit(0)? * 17,
                digit(1)? * 17,
                digit(2)? * 17,
                255,
            )),
            6 => Some(Color::from_rgba8(pair(0)?, pair(2)?, pair(4)?, 255)),
            8 => Some(Color::from_rgba8(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
            _ => None,
        };
    }
    let (args, alpha) =
        if let Some(args) = value.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
            (args, false)
        } else {
            (value.strip_prefix("rgba(")?.strip_suffix(')')?, true)
        };
    let parts: Vec<_> = args.split(',').map(str::trim).collect();
    if parts.len() != if alpha { 4 } else { 3 } {
        return None;
    }
    let channel = |i: usize| parts[i].parse::<i64>().ok().map(|v| v.clamp(0, 255) as u8);
    let a = if alpha {
        let value = parts[3].parse::<f64>().ok()?;
        if !value.is_finite() {
            return None;
        }
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    } else {
        255
    };
    Some(Color::from_rgba8(channel(0)?, channel(1)?, channel(2)?, a))
}

fn clipped(a: BoxRect, b: BoxRect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect::from_xywh(
        x,
        y,
        (a.x + a.width).min(b.x + b.width) - x,
        (a.y + a.height).min(b.y + b.height) - y,
    )
}

/// The used `color` of `id`: the nearest author color, or a form control's
/// user-agent color (`buttontext`/`fieldtext`, or its disabled color),
/// which stops inheritance from outside the control.
fn current_color(tree: &Tree, env: &Environment, id: u32) -> Option<Color> {
    for ancestor in tree.path(id) {
        if let Some(value) = tree.computed_read(ancestor, env).get(&CssProperty::Color)
            && value != "currentcolor"
        {
            return color(value);
        }
        if ancestor != ROOT_NODE
            && let Some(element) = tree.element(ancestor)
            && let Some(kind) = style::ControlKind::of(element)
        {
            return Some(theme::text_color(kind, element.disabled()));
        }
        // A modal dialog's user-agent text color stops inheritance too.
        if layout::modal_dialog(tree, ancestor) {
            return color(varta_common::view::DIALOG_TEXT[usize::from(env.dark)]);
        }
    }
    Some(Color::BLACK)
}

// Extract the color token from a border shorthand, preserving spaces inside rgb().
fn shorthand_color(value: &str) -> &str {
    let mut depth = 0usize;
    let mut start = None;
    for (index, ch) in value
        .char_indices()
        .chain(std::iter::once((value.len(), ' ')))
    {
        if ch.is_ascii_whitespace() && depth == 0 {
            if let Some(begin) = start.take() {
                let token = &value[begin..index];
                if token == "currentcolor"
                    || token == "transparent"
                    || token.starts_with('#')
                    || token.starts_with("rgb(")
                    || token.starts_with("rgba(")
                {
                    return token;
                }
            }
        } else {
            start.get_or_insert(index);
            if ch == '(' {
                depth += 1;
            }
            if ch == ')' {
                depth = depth.saturating_sub(1);
            }
        }
    }
    "currentcolor"
}

fn border_colors(
    tree: &Tree,
    env: &Environment,
    id: u32,
    widths: [f32; 4],
) -> Result<Option<[Color; 4]>, PaintError> {
    if widths.iter().all(|width| *width == 0.0) {
        return Ok(None);
    }
    let mut values = std::array::from_fn::<_, 4, _>(|_| "currentcolor".to_owned());
    let variables = style::Variables::new(tree, id, env);
    let set = |values: &mut [String; 4], property, raw: &str, sides: &[u8]| {
        let substituted = variables.property(property, raw);
        let value = substituted.as_deref().map_or("currentcolor", |v| {
            if property == CssProperty::BorderColor {
                v
            } else {
                shorthand_color(v)
            }
        });
        for side in sides {
            values[usize::from(*side)] = value.to_owned();
        }
    };
    for (name, value) in tree.cascade_refs(id, env) {
        if let Name::Property(property) = name {
            let property = *property;
            set(&mut values, property, value, border_color_sides(property));
        }
    }
    for (&side, (property, raw)) in &tree
        .element(id)
        .ok_or(PaintError::Unsupported(id))?
        .border_color_inline
    {
        set(&mut values, *property, raw, &[side]);
    }
    let mut result = [Color::TRANSPARENT; 4];
    for (index, (width, value)) in widths.into_iter().zip(values).enumerate() {
        if width == 0.0 {
            continue;
        }
        let next = if value == "currentcolor" {
            current_color(tree, env, id)
        } else {
            color(&value)
        }
        .ok_or(PaintError::Unsupported(id))?;
        result[index] = next;
    }
    Ok(Some(result))
}

// Nonzero CSS outline lengths retain a one-pixel minimum and truncate the
// remaining magnitude before host DPI. Authored values remain untouched.
fn outline_used_length(value: f32) -> f32 {
    const MINIMUM: f32 = 1.0;
    if value == 0.0 {
        0.0
    } else {
        value.abs().floor().max(MINIMUM).copysign(value)
    }
}

/// The cascaded outline longhands of `id`: each component with the property
/// that declared it (a longhand or the `outline` shorthand) and its value,
/// `None` when a variable leaves it invalid at computed-value time.
type OutlineValues = BTreeMap<CssProperty, (CssProperty, Option<String>)>;

fn outline_values(tree: &Tree, env: &Environment, id: u32) -> Result<OutlineValues, PaintError> {
    let variables = style::Variables::new(tree, id, env);
    let mut values = BTreeMap::new();
    for (name, raw) in tree.cascade_refs(id, env) {
        if let Name::Property(property) = name {
            let property = *property;
            for component in outline_components(property) {
                values.insert(*component, (property, variables.property(property, raw)));
            }
        }
    }
    for (component, (property, raw)) in tree
        .element(id)
        .ok_or(PaintError::Unsupported(id))?
        .outline_inline
        .values
        .iter()
    {
        values.insert(*component, (*property, variables.property(*property, raw)));
    }
    Ok(values)
}

/// A cascaded `outline-offset` as a used length (`None` when undeclared; a
/// value invalid at computed-value time is the initial `0`).
fn outline_offset(
    values: &OutlineValues,
    units: Units,
    id: u32,
) -> Result<Option<f32>, PaintError> {
    match values.get(&CssProperty::OutlineOffset) {
        None => Ok(None),
        Some((_, None)) => Ok(Some(0.0)),
        Some((_, Some(raw))) if raw == "0" => Ok(Some(0.0)),
        Some((_, Some(raw))) => units
            .pixels(raw, true)
            .map(|offset| Some(outline_used_length(offset)))
            .ok_or(PaintError::Unsupported(id)),
    }
}

/// A cascaded outline color (`currentcolor` when undeclared or invalid at
/// computed-value time).
fn outline_color(
    tree: &Tree,
    env: &Environment,
    values: &OutlineValues,
    id: u32,
) -> Result<Color, PaintError> {
    let raw_color = values
        .get(&CssProperty::OutlineColor)
        .and_then(|(property, raw)| raw.as_deref().map(|raw| (*property, raw)))
        .map_or("currentcolor", |(property, raw)| {
            if property == CssProperty::Outline {
                shorthand_color(raw)
            } else {
                raw
            }
        });
    if raw_color == "currentcolor" {
        current_color(tree, env, id)
    } else {
        color(raw_color)
    }
    .ok_or(PaintError::Unsupported(id))
}

fn outline(
    tree: &Tree,
    env: &Environment,
    id: u32,
) -> Result<Option<(f32, f32, Color)>, PaintError> {
    let units = Units::new(env).ok_or(layout::LayoutError::RootFont)?;
    let values = outline_values(tree, env, id)?;
    let get = |component| {
        values
            .get(&component)
            .and_then(|(property, raw)| raw.as_ref().map(|raw| (*property, raw.as_str())))
    };
    let width = match get(CssProperty::OutlineWidth) {
        Some((property, raw)) => {
            layout::border_component(
                if property == CssProperty::Outline {
                    CssProperty::Border
                } else {
                    CssProperty::BorderWidth
                },
                raw,
                units,
                |p, v| tree.render_reads.validate(env, p, v),
            )
            .ok_or(PaintError::Unsupported(id))?
            .0
        }
        None => 3.0,
    };
    let width = outline_used_length(width);
    let solid = match get(CssProperty::OutlineStyle) {
        Some((property, raw)) => {
            layout::border_component(
                if property == CssProperty::Outline {
                    CssProperty::Border
                } else {
                    CssProperty::BorderStyle
                },
                raw,
                units,
                |p, v| tree.render_reads.validate(env, p, v),
            )
            .ok_or(PaintError::Unsupported(id))?
            .1
        }
        None => false,
    };
    if !solid || width == 0.0 {
        return Ok(None);
    }
    let color = outline_color(tree, env, &values, id)?;
    let offset = outline_offset(&values, units, id)?.unwrap_or(0.0);
    Ok(Some((width, offset, color)))
}

/// Chrome's default focus ring ([`theme::focus_ring`]) when `id` is a focused
/// form control whose `outline-style` stays the user agent's `auto`: no author
/// `outline` shorthand or `outline-style` applies. An author `outline-color`
/// or `outline-offset` replaces the ring's color or offset, as in Chrome; an
/// author `outline-width` does not change the auto ring's width. Native
/// `:focus-visible` matches any focus, so the ring follows the same rule.
fn default_focus_ring(
    tree: &Tree,
    env: &Environment,
    id: u32,
    bounds: BoxRect,
    units: Units,
) -> Result<Option<(BoxRect, Corners, f32, Color)>, PaintError> {
    if tree.focus != Some(id) || id == ROOT_NODE {
        return Ok(None);
    }
    let Some(element) = tree.element(id) else {
        return Ok(None);
    };
    let Some(kind) = style::ControlKind::of(element) else {
        return Ok(None);
    };
    let values = outline_values(tree, env, id)?;
    // A declared style (solid or none, from either form) ends `auto`.
    if values.contains_key(&CssProperty::OutlineStyle) {
        return Ok(None);
    }
    let author = theme::AuthorRing {
        color: values
            .contains_key(&CssProperty::OutlineColor)
            .then(|| outline_color(tree, env, &values, id))
            .transpose()?,
        offset: outline_offset(&values, units, id)?,
    };
    let properties = tree.computed_read(id, env);
    let paint_bounds = snap::rect(bounds);
    let radii = properties
        .get(&CssProperty::BorderRadius)
        .map(|raw| {
            Corners::resolve_paint(raw, bounds, paint_bounds, units)
                .ok_or(PaintError::Unsupported(id))
        })
        .transpose()?
        .unwrap_or_default();
    Ok(Some(theme::focus_ring(
        kind,
        style::themed(&properties),
        paint_bounds,
        layout::border_widths(tree, env, id)?,
        radii,
        author,
    )))
}

fn paint_border(
    pixmap: &mut Surface,
    bounds: BoxRect,
    clip: BoxRect,
    widths: [f32; 4],
    radii: Corners,
    colors: [Color; 4],
) -> Result<(), PaintError> {
    let bounds = snap::rect(bounds);
    let clip = pixmap.local_clip(clip);
    let Some(outer) = Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height) else {
        return Ok(());
    };
    if clipped(bounds, clip).is_none() {
        return Ok(());
    }
    let mut path = PathBuilder::new();
    if radii.is_square() {
        // Preserve the established rectangular path and raster rounding.
        path.push_rect(outer);
        if let Some(inner) = Rect::from_xywh(
            bounds.x + widths[3],
            bounds.y + widths[0],
            bounds.width - widths[3] - widths[1],
            bounds.height - widths[0] - widths[2],
        ) {
            path.push_rect(inner);
        }
    } else {
        let Some(contour) = radii.clipped_path(bounds) else {
            return Ok(());
        };
        path.push_path(&contour);
        let (inner, inner_radii) = radii.inset(bounds, widths);
        if let Some(hole) = inner_radii.clipped_path(inner) {
            path.push_path(&hole);
        }
    }
    let Some(path) = path.finish() else {
        return Ok(());
    };
    let contains = clip.x <= bounds.x
        && clip.y <= bounds.y
        && clip.x + clip.width >= bounds.x + bounds.width
        && clip.y + clip.height >= bounds.y + bounds.height;
    let mask = if contains {
        None
    } else {
        let Some(clip_rect) = Rect::from_xywh(clip.x, clip.y, clip.width, clip.height) else {
            return Ok(());
        };
        Some(pixmap.mask(clip_rect)?)
    };
    let mut paint = Paint::default();
    let common = widths
        .into_iter()
        .zip(colors)
        .find_map(|(width, color)| (width > 0.0).then_some(color));
    let Some(common) = common else {
        return Ok(());
    };
    paint.set_color(common);
    if widths
        .into_iter()
        .zip(colors)
        .all(|(width, color)| width == 0.0 || color == common)
    {
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::EvenOdd,
            Transform::identity(),
            mask.as_ref(),
        );
    } else if !pixmap.groups.last().is_some_and(|scope| scope.hidden) {
        // Keep a streamed ordinary prefix, then capture this bounded software
        // operation and following draws exactly as for isolated subtrees.
        pixmap.isolation_required = true;
        pixmap.commands.get_or_insert_with(Recording::default);
        let border = border::Border::new(path, bounds, widths, radii, colors);
        pixmap.record(
            Primitive::Border(std::sync::Arc::new(border)),
            &paint,
            Transform::identity(),
            mask.as_ref(),
        );
    }
    Ok(())
}

struct GlyphPen {
    path: PathBuilder,
    x: f32,
    y: f32,
    commands: usize,
    exceeded: bool,
}
impl GlyphPen {
    fn admit(&mut self) -> bool {
        self.commands += 1;
        self.exceeded |= self.commands > 16_384;
        !self.exceeded
    }
}
impl OutlinePen for GlyphPen {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.admit() {
            return;
        }
        self.path.move_to(self.x + x, self.y - y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        if !self.admit() {
            return;
        }
        self.path.line_to(self.x + x, self.y - y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        if !self.admit() {
            return;
        }
        self.path
            .quad_to(self.x + x1, self.y - y1, self.x + x, self.y - y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if !self.admit() {
            return;
        }
        self.path.cubic_to(
            self.x + x1,
            self.y - y1,
            self.x + x2,
            self.y - y2,
            self.x + x,
            self.y - y,
        );
    }
    fn close(&mut self) {
        if !self.admit() {
            return;
        }
        self.path.close();
    }
}

/// Paint a select label or other text whose own node owns its flow.
fn paint_text(
    pixmap: &mut Surface,
    measurer: &mut TextMeasurer,
    shape: &TextShape,
    bounds: layout::BoxGeometry,
    style: underline::Style,
    id: u32,
    work: &mut usize,
) -> Result<Layout<u32>, PaintError> {
    paint_flow_text(
        pixmap,
        measurer,
        shape,
        bounds,
        style,
        (id, id, false),
        work,
    )
}

/// Paint plain text node `id`; a reshaping refusal names its flow's node,
/// the parent element `flow`, as layout does. A form control's label puts
/// its first baseline on a device pixel ([`device_baseline`]).
fn paint_flow_text(
    pixmap: &mut Surface,
    measurer: &mut TextMeasurer,
    shape: &TextShape,
    bounds: layout::BoxGeometry,
    style: underline::Style,
    (id, flow, control): (u32, u32, bool),
    work: &mut usize,
) -> Result<Layout<u32>, PaintError> {
    let (_, layout) = measurer
        .shape_styled(
            &shape.text,
            Some(bounds.rect.width.clamp(0.01, 1_000_000.0)),
            &shape.families,
            shape.options,
        )
        .map_err(|error| PaintError::Layout(LayoutError::text(flow)(error)))?;
    let mut bounds = paragraph_paint_bounds(bounds);
    if control {
        bounds.rect.y += device_baseline(&layout, bounds.rect.y, pixmap.scale, true);
    }
    paint_shaped(
        pixmap,
        &layout,
        bounds,
        (&style, &shape.text, shape.options.white_space),
        id,
        work,
    )?;
    Ok(layout)
}

/// The vertical shift that puts `layout`'s first baseline, painted from
/// `origin`, on a whole device pixel (half up). Chrome places control text
/// this way at every device scale; other text keeps its paragraph origin.
fn device_baseline(layout: &Layout<u32>, origin: f32, scale: f32, paragraph: bool) -> f32 {
    let Some(line) = layout.lines().next() else {
        return 0.0;
    };
    let metrics = line.metrics();
    let baseline = origin
        + metrics.baseline
        + if paragraph {
            paragraph_baseline_offset(metrics)
        } else {
            0.0
        };
    (baseline * scale + 0.5).floor() / scale - baseline
}

// The horizontal position and all logical text geometry remain fractional.
// Round only the containing paragraph's vertical paint origin, before DPI;
// per-line font offsets and glyph outlines retain their original coordinates.
fn paragraph_paint_bounds(bounds: layout::BoxGeometry) -> layout::BoxGeometry {
    layout::BoxGeometry {
        rect: BoxRect {
            y: snap::edge(f64::from(bounds.rect.y)) as f32,
            ..bounds.rect
        },
        ..bounds
    }
}

// CSS rounds primary ascent/descent before sharing line leading, placing the
// smaller half above. Preserve the containing line's fractional advance and all
// horizontal shaping. Fields keep their shared editing layout unchanged.
fn paragraph_baseline_offset(m: &parley::LineMetrics) -> f32 {
    let above =
        m.ascent.round() + ((m.line_height - m.ascent.round() - m.descent.round()) * 0.5).floor();
    above - (m.ascent + m.leading * 0.5)
}

// Field caret/selection/hit geometry and glyph painting must use the same
// shaping result. Keep this separate from paragraph shaping so a field never
// needs a second layout or a cloned display buffer just to paint its glyphs.
fn paint_shaped(
    pixmap: &mut Surface,
    layout: &Layout<u32>,
    bounds: layout::BoxGeometry,
    style: (&underline::Style, &str, WhiteSpaceMode),
    id: u32,
    work: &mut usize,
) -> Result<(), PaintError> {
    let (style, text, white_space) = style;
    let mut plan = underline::Plan::plain(layout, bounds, style, white_space, id, work)?;
    paint_shaped_with(
        pixmap,
        layout,
        bounds,
        |_| Ok(style.color),
        id,
        work,
        TextInk {
            flow: None,
            paragraph: style.paragraph,
            underlines: Some((&mut plan, text)),
        },
    )
}

/// Blink's bold threshold: a requested weight from which a face lighter than
/// it is drawn with synthetic bold (probed with Chrome 156). Below it, and
/// for a face at or above it, the selected face is drawn as is.
const SYNTHETIC_BOLD_THRESHOLD: f32 = 600.0;
/// The weight of a face whose tables cannot be read (CSS `normal`).
const DEFAULT_WEIGHT_CLASS: f32 = 400.0;

/// Whether Chrome would embolden this run, which the native painter refuses.
/// The shaper flags emboldening for any request heavier than a face without
/// a weight axis, so its flag alone is not Chrome's rule.
fn synthetic_bold(run: &parley::Run<'_, u32>) -> bool {
    if !run.synthesis().embolden() || run.font_attrs().weight.value() < SYNTHETIC_BOLD_THRESHOLD {
        return false;
    }
    let font = run.font();
    FontRef::from_index(font.data.as_ref(), font.index).map_or(DEFAULT_WEIGHT_CLASS, |face| {
        face.attributes().weight.value()
    }) < SYNTHETIC_BOLD_THRESHOLD
}

#[derive(Default)]
struct TextInk<'a> {
    flow: Option<&'a crate::inline::Frame>,
    paragraph: bool,
    underlines: Option<(&'a mut underline::Plan, &'a str)>,
}
fn paint_shaped_with(
    pixmap: &mut Surface,
    layout: &Layout<u32>,
    bounds: layout::BoxGeometry,
    mut color: impl FnMut(u32) -> Result<Color, PaintError>,
    id: u32,
    work: &mut usize,
    ink: TextInk<'_>,
) -> Result<(), PaintError> {
    let flow = ink.flow;
    let mut underlines = ink.underlines;
    if let Some((plan, text)) = underlines.as_mut() {
        plan.prepare(
            pixmap,
            (layout, bounds, flow, text, id, ink.paragraph),
            work,
        )?;
    }
    let bounds = layout::BoxGeometry {
        clip: pixmap.local_clip(bounds.clip),
        ..bounds
    };
    let Some(clip) = Rect::from_xywh(
        bounds.clip.x,
        bounds.clip.y,
        bounds.clip.width,
        bounds.clip.height,
    ) else {
        return Ok(());
    };
    let mask = if bounds.clip.x <= 0.0
        && bounds.clip.y <= 0.0
        && (bounds.clip.x + bounds.clip.width) * pixmap.scale >= pixmap.width() as f32
        && (bounds.clip.y + bounds.clip.height) * pixmap.scale >= pixmap.height() as f32
    {
        None
    } else {
        Some(pixmap.mask(clip)?)
    };
    let mut paint = Paint::default();
    let mut glyph_count = 0usize;
    for (line_index, line) in layout.lines().enumerate() {
        if let Some((plan, _)) = underlines.as_mut() {
            plan.paint_line(pixmap, line_index, mask.as_ref());
        }
        let mut line_glyph = 0usize;
        for item in line.items() {
            let glyph_run = match item {
                PositionedLayoutItem::GlyphRun(glyph_run) => glyph_run,
                // Decoration edges only advance; their boxes paint separately.
                PositionedLayoutItem::InlineBox(edge) if decoration_edge(edge.kind) => continue,
                PositionedLayoutItem::InlineBox(_) => return Err(PaintError::Unsupported(id)),
            };
            let base_color = color(glyph_run.style().brush)?;
            paint.set_color(base_color);
            let run = glyph_run.run();
            if synthetic_bold(run) || run.synthesis().skew().is_some() {
                return Err(PaintError::Unsupported(id));
            }
            let font_data = run.font();
            let mut outlines: Option<skrifa::outline::OutlineGlyphCollection<'_>> = None;
            let coords: Vec<_> = run
                .normalized_coords()
                .iter()
                .map(|coord| NormalizedCoord::from_bits(*coord))
                .collect();
            for glyph in glyph_run.positioned_glyphs() {
                let owner = match flow {
                    Some(frame) => *frame.glyph_owners[line_index]
                        .get(line_glyph)
                        .ok_or(PaintError::Unsupported(id))?,
                    None => glyph_run.style().brush,
                };
                let glyph_color = if flow.is_some() {
                    color(owner)?
                } else {
                    base_color
                };
                line_glyph += 1;
                paint.set_color(glyph_color);
                glyph_count += 1;
                if glyph_count > 65_536 {
                    return Err(PaintError::Unsupported(id));
                }
                if let Some(capture) = pixmap.capture.as_mut() {
                    capture.owner = owner;
                    capture.line = line_index;
                    if capture.hidden.contains(&owner) {
                        continue;
                    }
                }
                if pixmap.groups.last().is_some_and(|group| group.hidden) {
                    continue;
                }
                let x = bounds.rect.x + glyph.x;
                let y = bounds.rect.y
                    + glyph.y
                    + flow.map_or_else(
                        || {
                            if ink.paragraph {
                                paragraph_baseline_offset(line.metrics())
                            } else {
                                0.0
                            }
                        },
                        |f| f.shaped.offsets[line_index],
                    );
                // Host coverage is device pixels at the unscaled size; text
                // under a scaling transform draws the portable outlines,
                // which the transform maps exactly.
                let flat = pixmap.transforms == 0;
                if let Some(cache) = pixmap
                    .glyphs
                    .as_mut()
                    .filter(|cache| flat && cache.has_provider())
                {
                    const PHASES: f32 = 4.0;
                    let quarter = (x * pixmap.scale * PHASES + 0.5).floor() / PHASES;
                    let origin = (quarter.floor(), (y * pixmap.scale + 0.5).floor());
                    let phase = ((quarter - origin.0) * PHASES) as u8;
                    let request = crate::GlyphRasterRequest {
                        font_id: font_data.data.id(),
                        font_data: font_data.data.data(),
                        font_index: font_data.index,
                        glyph: glyph.id,
                        size: run.font_size() * pixmap.scale,
                        normalized_coords: run.normalized_coords(),
                        phase_x: phase,
                        max_pixels: MAX_TEXT_PAINT_WORK,
                        cache_bytes: glyphs::MAX_BYTES,
                    };
                    let key = glyphs::Key {
                        font: request.font_id,
                        index: request.font_index,
                        size: request.size.to_bits(),
                        coords: request.normalized_coords.to_vec(),
                        glyph: request.glyph,
                        phase: Some(phase),
                    };
                    let physical_clip = clip
                        .transform(Transform::from_scale(pixmap.scale, pixmap.scale))
                        .ok_or(PaintError::Unsupported(id))?;
                    match cache.raster(key, &request, origin, physical_clip, work)? {
                        glyphs::Outcome::Culled => {
                            if *work > MAX_TEXT_PAINT_WORK {
                                return Err(PaintError::Unsupported(id));
                            }
                            continue;
                        }
                        glyphs::Outcome::Ready(coverage) => {
                            let transform =
                                Transform::from_scale(1.0 / pixmap.scale, 1.0 / pixmap.scale);
                            for span in &coverage.spans {
                                let rect = Rect::from_xywh(
                                    origin.0 + coverage.bounds.left as f32 + span.x as f32,
                                    origin.1 + coverage.bounds.top as f32 + span.y as f32,
                                    span.width as f32,
                                    span.height as f32,
                                )
                                .ok_or(PaintError::Unsupported(id))?;
                                let color = Color::from_rgba(
                                    glyph_color.red(),
                                    glyph_color.green(),
                                    glyph_color.blue(),
                                    glyph_color.alpha() * f32::from(span.alpha)
                                        / f32::from(u8::MAX),
                                )
                                .ok_or(PaintError::Unsupported(id))?;
                                paint.set_color(color);
                                pixmap.fill_rect(rect, &paint, transform, mask.as_ref());
                            }
                            continue;
                        }
                        glyphs::Outcome::Missing => {}
                    }
                }
                if let Some(cache) = pixmap.glyphs.as_mut() {
                    cache.outline_glyphs = cache.outline_glyphs.saturating_add(1);
                }
                let key = pixmap.glyphs.as_ref().map(|_| glyphs::Key {
                    font: font_data.data.id(),
                    index: font_data.index,
                    size: run.font_size().to_bits(),
                    coords: run.normalized_coords().to_vec(),
                    glyph: glyph.id,
                    phase: None,
                });
                let cached = key
                    .as_ref()
                    .and_then(|key| pixmap.glyphs.as_mut().and_then(|cache| cache.get(key)));
                let retained = key.is_some();
                let outline = if let Some(cached) = cached {
                    cached
                } else {
                    if outlines.is_none() {
                        let font = FontRef::from_index(font_data.data.data(), font_data.index)
                            .map_err(|_| PaintError::Unsupported(id))?;
                        outlines = Some(font.outline_glyphs());
                    }
                    let outline = outlines
                        .as_ref()
                        .and_then(|outlines| outlines.get(GlyphId::new(glyph.id)))
                        .ok_or(PaintError::Unsupported(id))?;
                    let mut pen = GlyphPen {
                        path: PathBuilder::new(),
                        x: if retained { 0.0 } else { x },
                        y: if retained { 0.0 } else { y },
                        commands: 0,
                        exceeded: false,
                    };
                    outline
                        .draw(
                            DrawSettings::unhinted(
                                Size::new(run.font_size()),
                                LocationRef::new(&coords),
                            ),
                            &mut pen,
                        )
                        .map_err(|_| PaintError::Unsupported(id))?;
                    if pen.exceeded {
                        return Err(PaintError::Unsupported(id));
                    }
                    let result = glyphs::Outline {
                        path: pen.path.finish().map(std::sync::Arc::new),
                        commands: pen.commands,
                    };
                    if let Some(key) = key
                        && let Some(cache) = pixmap.glyphs.as_mut()
                    {
                        cache.insert(key, result.clone());
                    }
                    result
                };
                *work = work.saturating_add(outline.commands.saturating_add(1));
                if *work > MAX_TEXT_PAINT_WORK {
                    return Err(PaintError::Unsupported(id));
                }
                if let Some(path) = outline.path {
                    if retained {
                        let positioned = path
                            .as_ref()
                            .clone()
                            .transform(Transform::from_translate(x, y))
                            .ok_or(PaintError::Unsupported(id))?;
                        pixmap.fill_path(
                            &positioned,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            mask.as_ref(),
                        );
                    } else {
                        pixmap.fill_path(
                            &path,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            mask.as_ref(),
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// Font-independent checked marks, inside the explicitly styled control box.
fn paint_checkable(
    surface: &mut Surface,
    bounds: layout::BoxGeometry,
    color: Color,
    radio: bool,
) -> Result<(), PaintError> {
    let bounds = layout::BoxGeometry {
        clip: surface.local_clip(bounds.clip),
        ..bounds
    };
    let size = bounds.content.width.min(bounds.content.height);
    if size <= 0.0 {
        return Ok(());
    }
    let x = bounds.content.x + (bounds.content.width - size) / 2.0;
    let y = bounds.content.y + (bounds.content.height - size) / 2.0;
    let mut path = PathBuilder::new();
    if radio {
        path.push_circle(x + size / 2.0, y + size / 2.0, size * 0.3);
    } else {
        // A filled check mark scales with the content box, without a font or
        // device-pixel stroke-width assumption.
        for (index, (px, py)) in [
            (0.08, 0.5),
            (0.36, 0.78),
            (0.92, 0.18),
            (0.8, 0.08),
            (0.34, 0.58),
            (0.2, 0.42),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 0 {
                path.move_to(x + size * px, y + size * py);
            } else {
                path.line_to(x + size * px, y + size * py);
            }
        }
        path.close();
    }
    let Some(path) = path.finish() else {
        return Ok(());
    };
    let Some(clip) = clipped(bounds.content, bounds.clip) else {
        return Ok(());
    };
    let mask = if bounds.clip.x <= x
        && bounds.clip.y <= y
        && bounds.clip.x + bounds.clip.width >= x + size
        && bounds.clip.y + bounds.clip.height >= y + size
    {
        None
    } else {
        Some(surface.mask(clip)?)
    };
    let mut paint = Paint::default();
    paint.set_color(color);
    surface.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        mask.as_ref(),
    );
    Ok(())
}

/// Blink strokes a `circle` marker with a one-pixel line centered on the
/// ellipse inscribed in the symbol rect.
const MARKER_RING_HALF_WIDTH: f32 = 0.5;

/// Paint an outside list marker in the item's color and clip.
fn paint_marker(
    surface: &mut Surface,
    marker: &crate::list::Marker,
    color: Color,
    id: u32,
    work: &mut usize,
) -> Result<(), PaintError> {
    use crate::list::{MarkerInk, MarkerStyle};
    let clip = surface.local_clip(marker.clip);
    match &marker.ink {
        MarkerInk::Symbol(symbol) => {
            let rect = |inset: f32| {
                Rect::from_xywh(
                    symbol.x + inset,
                    symbol.y + inset,
                    symbol.width - 2.0 * inset,
                    symbol.height - 2.0 * inset,
                )
            };
            let mut path = PathBuilder::new();
            let rule = match marker.style {
                MarkerStyle::Square => {
                    path.push_rect(rect(0.0).ok_or(PaintError::Unsupported(id))?);
                    FillRule::Winding
                }
                MarkerStyle::Circle => {
                    path.push_oval(
                        rect(-MARKER_RING_HALF_WIDTH).ok_or(PaintError::Unsupported(id))?,
                    );
                    if let Some(inner) = rect(MARKER_RING_HALF_WIDTH) {
                        path.push_oval(inner);
                    }
                    FillRule::EvenOdd
                }
                MarkerStyle::Disc | MarkerStyle::Decimal(_) => {
                    path.push_oval(rect(0.0).ok_or(PaintError::Unsupported(id))?);
                    FillRule::Winding
                }
            };
            let Some(path) = path.finish() else {
                return Ok(());
            };
            let ink = BoxRect {
                x: symbol.x - MARKER_RING_HALF_WIDTH,
                y: symbol.y - MARKER_RING_HALF_WIDTH,
                width: symbol.width + 2.0 * MARKER_RING_HALF_WIDTH,
                height: symbol.height + 2.0 * MARKER_RING_HALF_WIDTH,
            };
            let Some(visible) = clipped(ink, clip) else {
                return Ok(());
            };
            let contained = clip.x <= ink.x
                && clip.y <= ink.y
                && clip.x + clip.width >= ink.x + ink.width
                && clip.y + clip.height >= ink.y + ink.height;
            let mask = if contained {
                None
            } else {
                Some(surface.mask(visible)?)
            };
            let mut paint = Paint::default();
            paint.set_color(color);
            surface.fill_path(&path, &paint, rule, Transform::identity(), mask.as_ref());
            Ok(())
        }
        MarkerInk::Text { layout, origin } => {
            let rect = BoxRect {
                x: origin[0],
                y: origin[1],
                width: layout.full_width(),
                height: layout.height(),
            };
            paint_shaped(
                surface,
                layout,
                layout::BoxGeometry {
                    rect,
                    content: rect,
                    clip,
                    scroll: None,
                    programmatic_scroll: None,
                    scrollbars: None,
                    transform: None,
                },
                (&underline::Style::plain(color), "", WhiteSpaceMode::Pre),
                id,
                work,
            )
        }
    }
}

fn paint_input(
    pixmap: &mut Surface,
    tree: &Tree,
    env: &Environment,
    id: u32,
    bounds: layout::BoxGeometry,
    font: (&mut TextMeasurer, f32),
    work: &mut usize,
) -> Result<TextHit, PaintError> {
    let (measurer, font_size) = font;
    let element = tree.element(id).ok_or(PaintError::Unsupported(id))?;
    *work = work.saturating_add(element.value.len()).saturating_add(
        element
            .editor
            .preedit
            .as_ref()
            .map_or(0, |value| value.text.len()),
    );
    if *work > MAX_TEXT_PAINT_WORK {
        return Err(PaintError::Unsupported(id));
    }
    if element.value.char_indices().any(|(byte, ch)| {
        ch.is_control()
            && !(element.tag == Tag::Textarea
                && (matches!(ch, '\n' | '\t')
                    || ch == '\r' && element.value.as_bytes().get(byte + 1) == Some(&b'\n')))
    }) {
        return Err(PaintError::Unsupported(id));
    }
    let base_start = element.editor.anchor.min(element.editor.focus);
    let base_end = element.editor.anchor.max(element.editor.focus);
    let mut display = Cow::Borrowed(element.value.as_str());
    let mut selection = (element.editor.anchor, element.editor.focus);
    let mut caret_visible = true;
    let mut preedit_range = None;
    if let Some(preedit) = &element.editor.preedit {
        display
            .to_mut()
            .replace_range(base_start..base_end, &preedit.text);
        preedit_range = Some((base_start, base_start + preedit.text.len()));
        if let Some((start, end)) = preedit.cursor {
            selection = (base_start + start, base_start + end);
        } else {
            selection = (
                base_start + preedit.text.len(),
                base_start + preedit.text.len(),
            );
            caret_visible = false;
        }
    }
    let source_len = display.len();
    // The password's shaped buffer contains only masks. Retain byte boundaries
    // separately so hit testing still addresses the original UTF-8 value.
    let password = element.input_type() == "password";
    let transformed = if password {
        Some(field_display::password(&display))
    } else {
        field_display::normalize_crlf(&display)
    };
    let source_map = if let Some((text, map)) = transformed {
        let display_position = |source| {
            if password {
                map.display_ceil(source)
            } else {
                map.display_floor(source)
            }
        };
        selection = (display_position(selection.0), display_position(selection.1));
        preedit_range =
            preedit_range.map(|(start, end)| (map.display_floor(start), display_position(end)));
        display = Cow::Owned(text);
        *work = work.saturating_add(display.len());
        if *work > MAX_TEXT_PAINT_WORK {
            return Err(PaintError::Unsupported(id));
        }
        Some(map)
    } else {
        None
    };
    let source_position = |offset: usize| {
        source_map
            .as_ref()
            .map_or_else(|| offset.min(source_len), |map| map.source(offset))
    };
    let (options, families) = layout::text_style(tree, env, id, font_size, measurer)?;
    let mut options =
        layout::controls::control_line_height(tree, id, measurer, &families, options)?;
    let multiline = element.tag == Tag::Textarea;
    options.source = TextSource::Field;
    options.white_space = if multiline {
        WhiteSpaceMode::PreWrap
    } else {
        WhiteSpaceMode::NoWrap
    };
    if multiline {
        options.break_anywhere = true;
    }
    let content = bounds.content;
    let (metrics, shaped) = measurer
        .shape_styled(
            if display.is_empty() { " " } else { &display },
            multiline.then_some(content.width.max(0.01)),
            &families,
            options,
        )
        .map_err(|error| PaintError::Layout(LayoutError::Text(error.to_string())))?;
    let source_clip = layout::intersect(bounds.clip, content);
    let clip = layout::intersect(pixmap.local_clip(bounds.clip), content);
    let width = if display.is_empty() {
        0.0
    } else {
        shaped.full_width()
    };
    let caret = Cursor::from_byte_index(
        &shaped,
        selection.1,
        if element.editor.upstream && element.editor.preedit.is_none() {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        },
    );
    let caret_bounds = caret.geometry(&shaped, 1.0);
    let anchor = if selection.0 == selection.1 {
        caret
    } else {
        Cursor::from_byte_index(&shaped, selection.0, Affinity::Downstream)
    };
    let anchor_bounds = if selection.0 == selection.1 {
        caret_bounds
    } else {
        anchor.geometry(&shaped, 1.0)
    };
    let caret_x = caret_bounds.x0 as f32;
    let automatic_x = (caret_x - (content.width - 1.0).max(0.0)).max(0.0);
    let horizontal_max = (width - (content.width - 1.0).max(0.0))
        .max(automatic_x)
        .max(0.0);
    let horizontal_offset = element
        .editor
        .horizontal_scroll
        .unwrap_or(automatic_x)
        .clamp(0.0, horizontal_max);
    let x = if multiline {
        content.x
    } else {
        content.x - horizontal_offset
    };
    let text_max_scroll = (metrics.height - content.height).max(0.0);
    let text_scroll = if element.editor.manual_text_scroll {
        (element.scroll_top as f32).clamp(0.0, text_max_scroll)
    } else {
        (caret_bounds.y1 as f32 - content.height).clamp(0.0, text_max_scroll)
    };
    let y = if multiline {
        content.y - text_scroll
    } else {
        content.y + (content.height - metrics.height) / 2.0
    };
    let mut clusters = Vec::new();
    let mut lines = Vec::new();
    for line in shaped.lines() {
        let cluster_start = clusters.len();
        let mut position = x + line.metrics().offset;
        for run in line.runs() {
            for cluster in run.visual_clusters() {
                let advance = cluster.advance();
                let range = cluster.text_range();
                if !display.is_empty() {
                    clusters.push(FieldTextCluster {
                        source: source_position(range.start)..source_position(range.end),
                        bounds: BoxRect {
                            x: position,
                            y: y + line.metrics().block_min_coord,
                            width: advance,
                            height: line.metrics().block_max_coord - line.metrics().block_min_coord,
                        },
                        rtl: cluster.is_rtl(),
                        line_break: matches!(display.get(range), Some("\n" | "\r\n")),
                    });
                }
                position += advance;
                if clusters.len() > 65_536 {
                    return Err(PaintError::Unsupported(id));
                }
            }
        }
        lines.push(FieldTextLine {
            bounds: BoxRect {
                x: x + line.metrics().offset,
                y: y + line.metrics().block_min_coord,
                width: if display.is_empty() {
                    0.0
                } else {
                    position - x - line.metrics().offset
                },
                height: line.metrics().block_max_coord - line.metrics().block_min_coord,
            },
            clusters: cluster_start..clusters.len(),
            source: source_position(line.text_range().start)
                ..source_position(line.text_range().end),
            content_end: {
                let end = line.text_range().end.min(display.len());
                source_position(if display[..end].ends_with("\r\n") {
                    end - 2
                } else if display[..end].ends_with('\n') {
                    end - 1
                } else {
                    end
                })
            },
        });
    }
    let color = current_color(tree, env, id).ok_or(PaintError::Unsupported(id))?;
    if tree.focus == Some(id) && !element.disabled() && selection.0 != selection.1 {
        let selection = Selection::new(anchor, caret);
        for (rect, _) in selection.geometry(&shaped) {
            let highlight = BoxRect {
                x: x + rect.x0 as f32,
                y: y + rect.y0 as f32,
                width: (rect.x1 - rect.x0) as f32,
                height: (rect.y1 - rect.y0) as f32,
            };
            if let Some(rect) = clipped(highlight, clip) {
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgba8(160, 200, 255, 255));
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
    }
    if !display.is_empty() {
        let glyph_bounds = layout::BoxGeometry {
            rect: BoxRect {
                x,
                y: y + device_baseline(&shaped, y, pixmap.scale, false),
                width: if multiline {
                    content.width.max(0.01)
                } else {
                    width.max(0.01)
                },
                height: metrics.height,
            },
            clip,
            content,
            scroll: None,
            programmatic_scroll: None,
            scrollbars: None,
            transform: None,
        };
        let style = underline::Style::resolve(tree, env, id, true, color)?;
        paint_shaped(
            pixmap,
            &shaped,
            glyph_bounds,
            (&style, &display, options.white_space),
            id,
            work,
        )?;
    } else if let Some(placeholder) = element.attributes.get(&Attribute::Placeholder)
        && !placeholder.is_empty()
    {
        if placeholder.chars().any(char::is_control) {
            return Err(PaintError::Unsupported(id));
        }
        *work = work.saturating_add(placeholder.len());
        if *work > MAX_TEXT_PAINT_WORK {
            return Err(PaintError::Unsupported(id));
        }
        let (_, placeholder_layout) = measurer
            .shape_styled(
                placeholder,
                Some(content.width.clamp(0.01, 1_000_000.0)),
                &families,
                options,
            )
            .map_err(|error| PaintError::Layout(LayoutError::Text(error.to_string())))?;
        let glyph_bounds = layout::BoxGeometry {
            rect: BoxRect {
                x: content.x,
                y: y + device_baseline(&placeholder_layout, y, pixmap.scale, false),
                width: content.width.max(0.01),
                height: metrics.height,
            },
            clip,
            content,
            scroll: None,
            programmatic_scroll: None,
            scrollbars: None,
            transform: None,
        };
        let style = underline::Style::resolve(tree, env, id, true, theme::PLACEHOLDER.color())?;
        paint_shaped(
            pixmap,
            &placeholder_layout,
            glyph_bounds,
            (&style, placeholder, options.white_space),
            id,
            work,
        )?;
    }
    if let Some((start, end)) = preedit_range {
        let selection = Selection::new(
            Cursor::from_byte_index(&shaped, start, Affinity::Downstream),
            Cursor::from_byte_index(&shaped, end, Affinity::Upstream),
        );
        for (rect, _) in selection.geometry(&shaped) {
            let underline = BoxRect {
                x: x + rect.x0 as f32,
                y: y + rect.y1 as f32 - 1.0,
                width: (rect.x1 - rect.x0) as f32,
                height: 1.0,
            };
            if let Some(rect) = clipped(underline, clip) {
                let mut paint = Paint::default();
                paint.set_color(color);
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
    }
    let caret_rect = BoxRect {
        x: if element.editor.horizontal_scroll.is_some() {
            x + caret_bounds.x0 as f32
        } else {
            (x + caret_bounds.x0 as f32).min(content.x + (content.width - 1.0).max(0.0))
        },
        y: y + caret_bounds.y0 as f32,
        width: 1.0,
        height: (caret_bounds.y1 - caret_bounds.y0) as f32,
    };
    if tree.focus == Some(id)
        && !element.disabled()
        && caret_visible
        && let Some(rect) = clipped(caret_rect, clip)
    {
        let mut paint = Paint::default();
        paint.set_color(color);
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
    let scrollport = if multiline {
        let widths = layout::border_widths(tree, env, id)?;
        BoxRect {
            x: bounds.rect.x + widths[3],
            y: bounds.rect.y + widths[0],
            width: (bounds.rect.width - widths[1] - widths[3]).max(0.0),
            height: (bounds.rect.height - widths[0] - widths[2]).max(0.0),
        }
    } else {
        content
    };
    Ok(TextHit {
        identity: crate::NodeIdentity {
            id,
            token: tree.nodes[&id].identity.clone(),
        },
        revision: crate::AccessibilityTextRevision::new(&tree.nodes[&id], element),
        clip: source_clip,
        clips: pixmap.active_clip.clone(),
        selection_carets: [
            BoxRect {
                x: x + anchor_bounds.x0 as f32,
                y: y + anchor_bounds.y0 as f32,
                width: 1.0,
                height: (anchor_bounds.y1 - anchor_bounds.y0) as f32,
            },
            caret_rect,
        ],
        clusters,
        end: source_len,
        composing: element.editor.preedit.is_some(),
        lines,
        caret_y: y + ((caret_bounds.y0 + caret_bounds.y1) / 2.0) as f32,
        origin_x: x,
        preferred_x: element.editor.preferred_x.unwrap_or(caret_x),
        multiline,
        text_viewport: content,
        horizontal: (!multiline).then_some(scroll::Horizontal {
            offset: horizontal_offset,
            max: horizontal_max,
            viewport: BoxRect {
                width: (content.width - 1.0).max(0.0),
                ..content
            },
        }),
        scroll: multiline.then_some(ScrollMetrics {
            top: text_scroll,
            max: text_max_scroll,
            viewport: scrollport,
        }),
        caret_area: clipped(caret_rect, source_clip).map(|rect| BoxRect {
            x: rect.x(),
            y: rect.y(),
            width: rect.width(),
            height: rect.height(),
        }),
    })
}

pub(crate) fn rasterize(
    tree: &Tree,
    env: &Environment,
    width: u32,
    height: u32,
) -> Result<RasterImage, PaintError> {
    rasterize_inner(tree, env, width, height, None, 1.0)
}

pub(crate) fn rasterize_with_text(
    tree: &Tree,
    env: &Environment,
    width: u32,
    height: u32,
    measurer: &mut TextMeasurer,
    font_size: f32,
) -> Result<RasterImage, PaintError> {
    rasterize_inner(tree, env, width, height, Some((measurer, font_size)), 1.0)
}

pub(crate) fn rasterize_scaled(
    tree: &Tree,
    env: &Environment,
    width: u32,
    height: u32,
    scale: f32,
    font: Option<(&mut TextMeasurer, f32)>,
) -> Result<RasterImage, PaintError> {
    rasterize_inner(tree, env, width, height, font, scale)
}

fn rasterize_inner(
    tree: &Tree,
    env: &Environment,
    width: u32,
    height: u32,
    text_input: Option<(&mut TextMeasurer, f32)>,
    scale: f32,
) -> Result<RasterImage, PaintError> {
    match text_input {
        Some((measurer, size)) => measurer.with_view_shaping(|measurer| {
            rasterize_inner_scoped(
                tree,
                env,
                width,
                height,
                Some((measurer, size)),
                scale,
                None,
            )
        }),
        None => rasterize_inner_scoped(tree, env, width, height, None, scale, None),
    }
}

/// The largest native viewport edge, in physical pixels, that
/// `validate_dimensions` admits. Vector consumers never present a pixel at or
/// beyond it.
pub(crate) const MAX_VIEWPORT_EDGE: u32 = 4096;

pub(crate) fn validate_dimensions(width: u32, height: u32, scale: f32) -> Result<(), PaintError> {
    // A bounded image prevents a guest-controlled viewport exhausting host memory.
    if !scale.is_finite()
        || !(0.25..=8.0).contains(&scale)
        || width == 0
        || height == 0
        || width > MAX_VIEWPORT_EDGE
        || height > MAX_VIEWPORT_EDGE
        || u64::from(width) * u64::from(height) > 16_777_216
    {
        return Err(PaintError::Dimensions);
    }
    Ok(())
}

pub(crate) fn rasterize_retained(
    tree: &Tree,
    env: &Environment,
    viewport: (u32, u32, f32),
    font: Option<(&mut TextMeasurer, f32)>,
    cache: (&mut Scene, &mut Glyphs),
) -> Result<RasterImage, PaintError> {
    let (width, height, scale) = viewport;
    match font {
        Some((font, size)) => font.with_view_shaping(|font| {
            rasterize_inner_scoped(
                tree,
                env,
                width,
                height,
                Some((font, size)),
                scale,
                Some(cache),
            )
        }),
        None => rasterize_inner_scoped(tree, env, width, height, None, scale, Some(cache)),
    }
}

/// Curved clip scopes of every painted element. They are built in tree order
/// before painting, because a stacking layer can paint before the in-flow
/// ancestors that clip it.
#[derive(Default)]
struct Scopes {
    /// The scope of an element's own background, border and hit region.
    own: BTreeMap<u32, std::sync::Arc<Clip>>,
    /// Adds the element's curved overflow clip, for its descendants.
    children: BTreeMap<u32, std::sync::Arc<Clip>>,
    /// Adds a native control's curved content clip, for its own content.
    content: BTreeMap<u32, std::sync::Arc<Clip>>,
}
fn clip_scopes(
    tree: &Tree,
    env: &Environment,
    geometry: &BTreeMap<u32, layout::BoxGeometry>,
    units: Units,
    order: &order::Order,
    surface: &mut Surface,
) -> Result<Scopes, PaintError> {
    let mut scopes = Scopes::default();
    for (id, placement) in order.elements() {
        let Some(element) = tree.element(id) else {
            continue;
        };
        surface.active_clip = placement
            .scope
            .and_then(|scope| scopes.children.get(&scope).cloned());
        if let Some(clip) = &surface.active_clip {
            scopes.own.insert(id, clip.clone());
        }
        let properties = tree.computed_read(id, env);
        let bounds = geometry[&id];
        let radii = properties
            .get(&CssProperty::BorderRadius)
            .map(|value| {
                Corners::resolve(value, bounds.rect, units).ok_or(PaintError::Unsupported(id))
            })
            .transpose()?
            .unwrap_or_default();
        let paint_rect = if id == ROOT_NODE {
            bounds.rect
        } else {
            snap::rect(bounds.rect)
        };
        let paint_radii = properties
            .get(&CssProperty::BorderRadius)
            .map(|raw| {
                Corners::resolve_paint(raw, bounds.rect, paint_rect, units)
                    .ok_or(PaintError::Unsupported(id))
            })
            .transpose()?
            .unwrap_or_default();
        if !radii.is_square()
            && properties
                .get(&CssProperty::Overflow)
                .is_some_and(|value| value != "visible")
        {
            let widths = layout::border_widths(tree, env, id)?;
            let (padding, inner) = radii.inset(bounds.rect, widths);
            let (paint_padding, paint_inner) = paint_radii.inset(paint_rect, widths);
            if !inner.is_square()
                || !paint_inner.is_square()
                || padding.width == 0.0
                || padding.height == 0.0
                || paint_padding.width == 0.0
                || paint_padding.height == 0.0
            {
                surface.push_clip(padding, inner, paint_padding, paint_inner)?;
            }
        }
        if let Some(clip) = &surface.active_clip {
            scopes.children.insert(id, clip.clone());
        }
        if matches!(element.tag, Tag::Input | Tag::Textarea | Tag::Select) && !radii.is_square() {
            let content = bounds.content;
            let insets = [
                content.y - bounds.rect.y,
                bounds.rect.x + bounds.rect.width - content.x - content.width,
                bounds.rect.y + bounds.rect.height - content.y - content.height,
                content.x - bounds.rect.x,
            ]
            .map(|inset| inset.max(0.0));
            let (content, inner) = radii.inset(bounds.rect, insets);
            let (paint_content, paint_inner) = paint_radii.inset(paint_rect, insets);
            if !inner.is_square()
                || !paint_inner.is_square()
                || content.width == 0.0
                || content.height == 0.0
                || paint_content.width == 0.0
                || paint_content.height == 0.0
            {
                surface.push_clip(content, inner, paint_content, paint_inner)?;
            }
        }
        if let Some(clip) = &surface.active_clip {
            scopes.content.insert(id, clip.clone());
        }
    }
    surface.active_clip = None;
    Ok(scopes)
}

/// Rectangle containment up to f32 rounding of repeated clip intersections.
fn contains(outer: BoxRect, inner: BoxRect) -> bool {
    // CSS pixels; far below one device pixel at the admitted 8x scale.
    const TOLERANCE: f32 = 1.0 / 1024.0;
    inner.x >= outer.x - TOLERANCE
        && inner.y >= outer.y - TOLERANCE
        && inner.x + inner.width <= outer.x + outer.width + TOLERANCE
        && inner.y + inner.height <= outer.y + outer.height + TOLERANCE
}

fn rasterize_inner_scoped(
    tree: &Tree,
    env: &Environment,
    width: u32,
    height: u32,
    mut text_input: Option<(&mut TextMeasurer, f32)>,
    scale: f32,
    cache: Option<(&mut Scene, &mut Glyphs)>,
) -> Result<RasterImage, PaintError> {
    validate_dimensions(width, height, scale)?;
    let env = Environment {
        width: f64::from(width) / f64::from(scale),
        ..*env
    };
    let units = Units::new(&env).ok_or(layout::LayoutError::RootFont)?;
    let (geometry, shapes, flows) = if let Some((measurer, font_size)) = text_input.as_mut() {
        layout::geometry_and_text(
            tree,
            &env,
            width as f32 / scale,
            height as f32 / scale,
            measurer,
            *font_size,
        )?
    } else {
        (
            layout::geometry_local(tree, &env, width as f32 / scale, height as f32 / scale)?,
            BTreeMap::new(),
            BTreeMap::new(),
        )
    };
    let markers = crate::list::markers(
        &crate::list::Inputs {
            tree,
            env: &env,
            geometry: &geometry,
            shapes: &shapes,
            flows: &flows,
            snap: |value| snap::edge(f64::from(value)) as f32,
        },
        text_input
            .as_mut()
            .map(|(measurer, size)| (&mut **measurer, *size)),
    )?;
    let (scene, glyphs) = match cache {
        Some((s, g)) => (Some(s), Some(g)),
        None => (None, None),
    };
    let retained = scene.is_some();
    let mut pixmap = Surface {
        glyphs,
        pixels: if retained {
            None
        } else {
            Some(Pixmap::new(width, height).ok_or(PaintError::Dimensions)?)
        },
        width,
        height,
        scale,
        commands: retained.then(Recording::default),
        active_clip: None,
        clip_masks: None,
        failure: None,
        groups: Vec::new(),
        relative_clips: BTreeMap::new(),
        isolation_required: false,
        transforms: 0,
        capture: None,
    };
    let mut pending = order::Order::new(tree, &env, &geometry, |id| flows.contains_key(&id))?;
    let scopes = clip_scopes(tree, &env, &geometry, units, &pending, &mut pixmap)?;
    // Rectangular clips of the open opacity groups, innermost last.
    let mut group_clips = Vec::<(u32, BoxRect)>::new();
    let mut text_work = 0usize;
    let mut inline_programs = inline_groups::Programs::default();
    let mut hit_regions = Vec::new();
    let mut backdrops = std::collections::BTreeSet::new();
    let mut rounded_regions = BTreeMap::new();
    let mut hit_clips = BTreeMap::new();
    let mut inherited_clips = BTreeMap::<u32, std::sync::Arc<Clip>>::new();
    let mut ancestors = BTreeMap::new();
    let mut scrolls = BTreeMap::new();
    let mut text_hits = BTreeMap::new();
    let mut menu_control = None;
    let mut cursors = BTreeMap::new();
    // Open scaling transforms: the box, its first draw and hit region, and
    // its transform in CSS pixels.
    let mut transform_scopes: Vec<(u32, usize, usize, crate::transform::Affine)> = Vec::new();
    while let Some((id, phase)) = pending.next() {
        if phase == order::Phase::InlineOutline {
            inline_programs.paint_outlines(id, &mut pixmap)?;
            continue;
        }
        if phase == order::Phase::InlineEnd {
            pixmap.end_group(id)?;
            continue;
        }
        if phase == order::Phase::Backdrop {
            // A modal dialog's backdrop covers the viewport behind it and
            // takes every pointer that does not land on the dialog itself.
            pixmap.active_clip = None;
            let viewport = geometry[&ROOT_NODE].rect;
            let backdrop = color(varta_common::view::DIALOG_BACKDROP[usize::from(env.dark)])
                .ok_or(PaintError::Unsupported(id))?;
            if let Some(rect) =
                Rect::from_xywh(viewport.x, viewport.y, viewport.width, viewport.height)
            {
                let mut paint = Paint::default();
                paint.set_color(backdrop);
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
            ancestors.insert(id, None);
            backdrops.insert(hit_regions.len());
            hit_regions.push((id, viewport));
            continue;
        }
        if phase == order::Phase::InlineContext {
            inline_programs.paint(
                id,
                &mut pixmap,
                &mut inline_groups::Metadata {
                    hits: &mut hit_regions,
                    rounded: &mut rounded_regions,
                    clips: &mut hit_clips,
                    ancestors: &mut ancestors,
                    cursors: &mut cursors,
                },
                &mut pending,
            )?;
            continue;
        }
        // A layer paints from its owning context's layer phase instead.
        if phase == order::Phase::Enter && pending.layered(id) {
            continue;
        }
        let outline_phase = phase == order::Phase::Outline;
        pixmap.active_clip = if outline_phase {
            inherited_clips.get(&id).cloned()
        } else if tree.element(id).is_none() {
            tree.nodes[&id]
                .parent
                .and_then(|parent| scopes.children.get(&parent).cloned())
        } else if phase == order::Phase::Content {
            scopes.content.get(&id).cloned()
        } else {
            scopes.own.get(&id).cloned()
        };
        if phase != order::Phase::Content
            && !outline_phase
            && let Some(clip) = &pixmap.active_clip
        {
            inherited_clips.insert(id, clip.clone());
        }
        if outline_phase {
            if let Some((rect, radii, width, color)) =
                default_focus_ring(tree, &env, id, geometry[&id].rect, units)?
            {
                let bounds = geometry[&id];
                let (halo, halo_radii, halo_width, halo_color) =
                    theme::focus_ring_halo(rect, radii);
                for (rect, radii, width, color) in [
                    (halo, halo_radii, halo_width, halo_color),
                    (rect, radii, width, color),
                ] {
                    paint_border(
                        &mut pixmap,
                        rect,
                        bounds.clip,
                        [width; 4],
                        radii,
                        [color; 4],
                    )?;
                }
            } else if let Some((width, offset, color)) = outline(tree, &env, id)? {
                let bounds = geometry[&id];
                // CSS UI4 keeps an outline visible even when a very negative
                // offset would otherwise collapse it; clamp the axes separately.
                let paint_bounds = snap::rect(bounds.rect);
                let expansion_x = (width + offset).max(width - paint_bounds.width * 0.5);
                let expansion_y = (width + offset).max(width - paint_bounds.height * 0.5);
                let rect = BoxRect {
                    x: paint_bounds.x - expansion_x,
                    y: paint_bounds.y - expansion_y,
                    width: paint_bounds.width + 2.0 * expansion_x,
                    height: paint_bounds.height + 2.0 * expansion_y,
                };
                let radii = tree
                    .computed_read(id, &env)
                    .get(&CssProperty::BorderRadius)
                    .map(|raw| {
                        Corners::resolve_paint(raw, bounds.rect, paint_bounds, units)
                            .ok_or(PaintError::Unsupported(id))
                    })
                    .transpose()?
                    .unwrap_or_default()
                    .outset(expansion_x, expansion_y);
                paint_border(
                    &mut pixmap,
                    rect,
                    bounds.clip,
                    [width; 4],
                    radii,
                    [color; 4],
                )?;
            }
            pixmap.end_group(id)?;
            if let Some((_, start, hits, affine)) = transform_scopes.pop_if(|scope| scope.0 == id) {
                pixmap.end_transform(start, affine);
                if hit_clips.range(hits..).next().is_some() {
                    return Err(PaintError::Unsupported(id));
                }
                for (_, rect) in &mut hit_regions[hits..] {
                    *rect = affine.rect(*rect);
                }
                let device = crate::transform::Affine {
                    x: affine.x * scale,
                    y: affine.y * scale,
                    ..affine
                };
                for (_, region) in rounded_regions.range_mut(hits..) {
                    let inline_decor::Region::Box(bounds, radii) = region else {
                        return Err(PaintError::Unsupported(id));
                    };
                    *bounds = device.rect(*bounds);
                    for [x, y] in &mut radii.0 {
                        *x *= affine.scale_x;
                        *y *= affine.scale_y;
                    }
                }
            }
            if group_clips.last().is_some_and(|(group, _)| *group == id) {
                group_clips.pop();
            }
            continue;
        }
        let node = &tree.nodes[&id];
        let Content::Element(element) = &node.content else {
            if phase != order::Phase::Content {
                pending.content(id);
                continue;
            }
            let shape = shapes.get(&id).ok_or(PaintError::Unsupported(id))?;
            let (measurer, _) = text_input.as_mut().ok_or(PaintError::Unsupported(id))?;
            let parent = node.parent.ok_or(PaintError::Unsupported(id))?;
            let color = current_color(tree, &env, parent).ok_or(PaintError::Unsupported(id))?;
            let layout = paint_flow_text(
                &mut pixmap,
                measurer,
                shape,
                geometry[&id],
                underline::Style::resolve(tree, &env, parent, false, color)?,
                (
                    id,
                    parent,
                    parent != ROOT_NODE
                        && tree
                            .element(parent)
                            .is_some_and(|element| style::ControlKind::of(element).is_some()),
                ),
                &mut text_work,
            )?;
            // The anonymous text box can fill the available block width. Only
            // occupied line fragments join the inline pointer phase.
            for line in layout.lines() {
                let metrics = line.metrics();
                let bounds = geometry[&id];
                let fragment = BoxRect {
                    x: bounds.rect.x + metrics.offset + metrics.inline_min_coord,
                    y: bounds.rect.y + metrics.baseline + paragraph_baseline_offset(metrics)
                        - metrics.ascent.round(),
                    // Preserved trailing spaces stay in the text fragment.
                    width: metrics.advance
                        - if shape.options.white_space.preserves() {
                            0.0
                        } else {
                            metrics.trailing_whitespace
                        },
                    height: metrics.ascent.round() + metrics.descent.round(),
                };
                if let Some(rect) = clipped(fragment, bounds.clip) {
                    if let Some(clip) = &pixmap.active_clip {
                        hit_clips.insert(hit_regions.len(), clip.clone());
                    }
                    hit_regions.push((
                        parent,
                        BoxRect {
                            x: rect.x(),
                            y: rect.y(),
                            width: rect.width(),
                            height: rect.height(),
                        },
                    ));
                }
            }
            continue;
        };
        if phase == order::Phase::Content {
            if element.tag == Tag::Select {
                let (measurer, size) = text_input.as_mut().ok_or(PaintError::Unsupported(id))?;
                select::paint_control(
                    &mut pixmap,
                    tree,
                    &env,
                    id,
                    geometry[&id],
                    (measurer, *size),
                    &mut text_work,
                )?;
                if tree.focus == Some(id)
                    && element.editor.select_popup.is_some()
                    && !element.disabled()
                {
                    menu_control = Some((id, geometry[&id]));
                }
            } else if element.checkable() {
                if element.checked && !style::themed(&tree.computed_read(id, &env)) {
                    let color = current_color(tree, &env, id).ok_or(PaintError::Unsupported(id))?;
                    paint_checkable(
                        &mut pixmap,
                        geometry[&id],
                        color,
                        element.input_type() == "radio",
                    )?;
                }
            } else if matches!(element.tag, Tag::Input | Tag::Textarea) {
                let (measurer, font_size) =
                    text_input.as_mut().ok_or(PaintError::Unsupported(id))?;
                let hit = paint_input(
                    &mut pixmap,
                    tree,
                    &env,
                    id,
                    geometry[&id],
                    (measurer, *font_size),
                    &mut text_work,
                )?;
                if let Some(scroll) = hit.scroll {
                    scrolls.insert(id, scroll);
                }
                text_hits.insert(id, hit);
            }
            pixmap.active_clip = scopes.children.get(&id).cloned();
            if let Some(frame) = flows.get(&id) {
                let shape = shapes
                    .get(&id)
                    .and_then(|s| s.rich.as_ref())
                    .ok_or(PaintError::Unsupported(id))?;
                inline_groups::Flow {
                    text: &shapes[&id].text,
                    id,
                    tree,
                    env: &env,
                    frame,
                    shape,
                    units,
                }
                .paint(
                    &mut pixmap,
                    &mut text_work,
                    &mut inline_groups::Metadata {
                        hits: &mut hit_regions,
                        rounded: &mut rounded_regions,
                        clips: &mut hit_clips,
                        ancestors: &mut ancestors,
                        cursors: &mut cursors,
                    },
                    &mut pending,
                    &mut inline_programs,
                )?;
            }
            if let Some(marker) = markers.get(&id) {
                let color = current_color(tree, &env, id).ok_or(PaintError::Unsupported(id))?;
                paint_marker(&mut pixmap, marker, color, id, &mut text_work)?;
                // The marker box is the item's pointer target.
                if let Some(rect) = clipped(marker.area, marker.clip) {
                    if let Some(clip) = &pixmap.active_clip {
                        hit_clips.insert(hit_regions.len(), clip.clone());
                    }
                    hit_regions.push((
                        id,
                        BoxRect {
                            x: rect.x(),
                            y: rect.y(),
                            width: rect.width(),
                            height: rect.height(),
                        },
                    ));
                }
            }
            continue;
        }
        let properties = tree.computed_read(id, &env);
        // Hidden elements, a closed dialog included, paint nothing.
        if tree.closed_dialog(id)
            || properties
                .get(&CssProperty::Display)
                .is_some_and(|v| v.eq_ignore_ascii_case("none"))
        {
            continue;
        }
        let opacity = properties
            .get(&CssProperty::Opacity)
            .map(|raw| {
                raw.parse::<f32>()
                    .ok()
                    .filter(|a| a.is_finite() && (0.0..=1.0).contains(a))
                    .ok_or(PaintError::Unsupported(id))
            })
            .transpose()?
            .unwrap_or(1.0);
        if phase == order::Phase::Enter
            && (matches!(element.tag, Tag::Input | Tag::Textarea | Tag::Select)
                || node.parent.is_some_and(|parent| {
                    tree.computed_property(parent, &env, CssProperty::Display)
                        .is_some_and(|value| matches!(value.as_str(), "flex" | "grid"))
                }))
        {
            pending.atomic(id);
            continue;
        }
        // Wheel scrolling chains through the boxes whose offsets move this one.
        let placement = pending.placement(id).ok_or(PaintError::Unsupported(id))?;
        ancestors.insert(id, placement.scope);
        cursors.insert(id, pointer_cursor(tree, &env, id));
        if let Some(scroll) = geometry[&id].scroll {
            scrolls.insert(id, scroll);
        }
        let control = (id != ROOT_NODE)
            .then(|| style::ControlKind::of(element))
            .flatten();
        let themed = control.is_some() && style::themed(&properties);
        let button_ready = element.tag != Tag::Button
            || themed
            || properties.contains_key(&CssProperty::Border)
            || (properties
                .get(&CssProperty::Display)
                .is_some_and(|value| value == "flex")
                && properties.contains_key(&CssProperty::BackgroundColor)
                && properties.contains_key(&CssProperty::Color)
                && (properties.contains_key(&CssProperty::Outline)
                    || properties.contains_key(&CssProperty::OutlineStyle))
                && properties.contains_key(&CssProperty::BorderRadius));
        let input_ready = !matches!(element.tag, Tag::Input | Tag::Textarea | Tag::Select)
            || (themed && element.tag != Tag::Select)
            // An author-styled text field takes user-agent defaults for the
            // rest (layout already required its `border` shorthand).
            || (element.text_field() && properties.contains_key(&CssProperty::Border))
            || ([CssProperty::BackgroundColor, CssProperty::Color]
                .iter()
                .all(|property| properties.contains_key(property))
                && (!(element.text_field() || element.tag == Tag::Select)
                    || [
                        CssProperty::FontFamily,
                        CssProperty::FontSize,
                        CssProperty::FontWeight,
                        CssProperty::FontStyle,
                        CssProperty::LineHeight,
                    ]
                    .iter()
                    .all(|property| properties.contains_key(property)))
                && (properties.contains_key(&CssProperty::Outline)
                    || properties.contains_key(&CssProperty::OutlineStyle))
                && properties.contains_key(&CssProperty::BorderRadius));
        let radii = properties
            .get(&CssProperty::BorderRadius)
            .map(|value| {
                Corners::resolve(value, geometry[&id].rect, units)
                    .ok_or(PaintError::Unsupported(id))
            })
            .transpose()?
            .unwrap_or_else(|| {
                control
                    .filter(|_| themed)
                    .map(|kind| theme::user_agent_radii(kind, geometry[&id].rect))
                    .unwrap_or_default()
            });
        let paint_rect = if id == ROOT_NODE {
            geometry[&id].rect
        } else {
            snap::rect(geometry[&id].rect)
        };
        let paint_radii = properties
            .get(&CssProperty::BorderRadius)
            .map(|raw| {
                Corners::resolve_paint(raw, geometry[&id].rect, paint_rect, units)
                    .ok_or(PaintError::Unsupported(id))
            })
            .transpose()?
            .unwrap_or_default();
        let widths = layout::border_widths(tree, &env, id)?;
        if !input_ready || !button_ready {
            return Err(PaintError::Unsupported(id));
        }
        // An opacity group composites under its own rectangular clip. An
        // absolute or fixed descendant that escapes that clip in CSS would
        // need a separate composite scope; curved scopes fail in
        // `relative_clip` likewise.
        if placement.out_of_flow
            && group_clips
                .last()
                .is_some_and(|(_, group)| !contains(*group, geometry[&id].clip))
        {
            return Err(PaintError::Unsupported(id));
        }
        // A scaling transform maps everything its stacking context paints,
        // and the hit regions it adds, until its outline phase ends it.
        if phase == order::Phase::Context
            && let Some(affine) = geometry[&id].transform
        {
            let start = pixmap.begin_transform(id)?;
            transform_scopes.push((id, start, hit_regions.len(), affine));
        }
        if opacity != 1.0 {
            pixmap.begin_group(id, opacity, geometry[&id].clip)?;
            group_clips.push((id, geometry[&id].clip));
        }
        if id != ROOT_NODE
            && let Some(rect) = clipped(geometry[&id].rect, geometry[&id].clip)
        {
            if !radii.is_square() {
                rounded_regions.insert(
                    hit_regions.len(),
                    inline_decor::Region::Box(
                        scaled_rect(geometry[&id].rect, scale),
                        radii.scaled(scale),
                    ),
                );
            }
            if let Some(clip) = &pixmap.active_clip {
                hit_clips.insert(hit_regions.len(), clip.clone());
            }
            hit_regions.push((
                id,
                BoxRect {
                    x: rect.x(),
                    y: rect.y(),
                    width: rect.width(),
                    height: rect.height(),
                },
            ));
        }
        if let Some(kind) = control.filter(|_| themed) {
            theme::paint(
                &mut pixmap,
                (id, kind),
                paint_rect,
                geometry[&id].clip,
                theme::State {
                    checked: element.checked,
                    disabled: element.disabled(),
                },
            )?;
        } else if let Some(color) = properties
            .get(&CssProperty::BackgroundColor)
            .map(|value| {
                if value == "currentcolor" {
                    current_color(tree, &env, id)
                } else {
                    color(value)
                }
                .ok_or(PaintError::Unsupported(id))
            })
            .transpose()?
            .or_else(|| control.and_then(theme::background))
            .or_else(|| {
                layout::modal_dialog(tree, id)
                    .then(|| color(varta_common::view::DIALOG_BACKGROUND[usize::from(env.dark)]))
                    .flatten()
            })
        {
            let bounds = geometry[&id];
            if let Some(rect) = clipped(paint_rect, pixmap.local_clip(bounds.clip)) {
                let mut paint = Paint::default();
                paint.set_color(color);
                if paint_radii.is_square() {
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                } else if let Some(path) = paint_radii.path(paint_rect) {
                    let mask = pixmap.mask(rect)?;
                    pixmap.fill_path(
                        &path,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        Some(&mask),
                    );
                }
            }
        }
        if !themed && let Some(colors) = border_colors(tree, &env, id, widths)? {
            let bounds = geometry[&id];
            paint_border(
                &mut pixmap,
                paint_rect,
                bounds.clip,
                widths,
                paint_radii,
                colors,
            )?;
        }
        if let Some(bars) = geometry[&id].scrollbars {
            // Classic scrollbars follow the box's square border edge; a
            // rounded scroll container would clip them.
            if !paint_radii.is_square() {
                return Err(PaintError::Unsupported(id));
            }
            scrollbar::paint(&mut pixmap, bars, geometry[&id].clip)?;
        }
        if matches!(phase, order::Phase::Context | order::Phase::Atomic) {
            pending.finish_after_children(id, phase);
        } else {
            pending.outline(id);
        }
        if flows.contains_key(&id)
            || markers.contains_key(&id)
            || matches!(element.tag, Tag::Input | Tag::Textarea | Tag::Select)
        {
            pending.content(id);
        }
        if !flows.contains_key(&id) && element.tag != Tag::Select {
            pending.children(&node.children);
        }
    }
    pixmap.active_clip = None;
    let select_menu = if let Some((id, bounds)) = menu_control {
        let (measurer, size) = text_input.as_mut().ok_or(PaintError::Unsupported(id))?;
        Some(Box::new(select::paint_menu(
            &mut pixmap,
            tree,
            &env,
            id,
            bounds,
            (measurer, *size),
            &mut text_work,
        )?))
    } else {
        None
    };
    let focused = tree
        .focus
        .filter(|id| hit_regions.iter().any(|(node, _)| node == id));
    let editable = focused.filter(|id| {
        tree.element(*id).is_some_and(|element| {
            element.text_field() && !element.disabled() && !element.readonly()
        })
    });
    for (_, rect) in &mut hit_regions {
        *rect = scaled_rect(*rect, scale);
    }
    for hit in text_hits.values_mut() {
        hit.caret_y *= scale;
        hit.origin_x *= scale;
        hit.clip = scaled_rect(hit.clip, scale);
        for caret in &mut hit.selection_carets {
            *caret = scaled_rect(*caret, scale);
        }
        for cluster in &mut hit.clusters {
            cluster.bounds = scaled_rect(cluster.bounds, scale);
        }
        hit.caret_area = hit.caret_area.map(|rect| scaled_rect(rect, scale));
        for line in &mut hit.lines {
            line.bounds = scaled_rect(line.bounds, scale);
        }
    }
    // Only the focused editable field can supply IME geometry. One bounded
    // query avoids repeating curved searches for every unfocused field.
    if let Some(hit) = editable.and_then(|id| text_hits.get_mut(&id)) {
        hit.caret_area = hit.caret_area.and_then(|rect| {
            text_geometry::visible_bounds(
                rect,
                hit.clip,
                hit.clips.as_deref(),
                &mut crate::AccessibilityWork::default(),
            )
            .ok()
            .flatten()
        });
    }
    let pixels = pixmap.finish(scene)?;
    Ok(RasterImage {
        pixels,
        metadata: FrameMetadata {
            width,
            height,
            scale,
            hit_regions,
            backdrops,
            rounded_regions,
            hit_clips,
            ancestors,
            scrolls,
            focused,
            editable,
            text_hits,
            select_menu,
            cursors,
            scroll_sources: scroll::sources(tree, &geometry, &pending),
            scroll_layout: layout::visual(tree, geometry),
        },
    })
}

/// Inline decoration edges contribute advance but no glyphs.
pub(crate) fn decoration_edge(kind: parley::InlineBoxKind) -> bool {
    matches!(
        kind,
        parley::InlineBoxKind::InlineStart | parley::InlineBoxKind::InlineEnd
    )
}

#[cfg(test)]
mod positioned_tests;
#[cfg(test)]
mod flow_text_tests {
    use super::*;
    /// A paint-time reshaping refusal names the text's flow node, as layout
    /// does. Preserved tabs in right-to-left text are outside the native
    /// white-space model; layout refuses them first, so this shape is built
    /// directly.
    #[test]
    fn paint_time_reshaping_refusal_names_the_flow_node() {
        const TEXT: u32 = 4;
        const FLOW: u32 = 3;
        let mut measurer = TextMeasurer::new(
            include_bytes!("../tests/fixtures/unicode.ttf").to_vec(),
            "Varta Unicode Fixture",
        )
        .unwrap();
        let families = measurer.resolve_families(None).unwrap();
        let options = crate::text::TextOptions {
            size: 16.0,
            line_height: None,
            weight: style::NORMAL_FONT_WEIGHT,
            italic: false,
            white_space: WhiteSpaceMode::Pre,
            break_anywhere: false,
            source: TextSource::General,
            align: crate::text::TextAlign::Start,
        };
        let text = "\u{05d0}\t\u{05d1}".to_owned();
        let shape = TextShape {
            text,
            metrics: crate::TextMetrics {
                width: 0.0,
                min_width: 0.0,
                height: 0.0,
                lines: 1,
            },
            options,
            families,
            rich: None,
        };
        let rect = BoxRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let mut surface = Surface {
            glyphs: None,
            pixels: Pixmap::new(12, 12),
            width: 12,
            height: 12,
            scale: 1.0,
            commands: None,
            active_clip: None,
            clip_masks: None,
            failure: None,
            groups: Vec::new(),
            relative_clips: BTreeMap::new(),
            isolation_required: false,
            transforms: 0,
            capture: None,
        };
        let result = paint_flow_text(
            &mut surface,
            &mut measurer,
            &shape,
            layout::BoxGeometry {
                rect,
                clip: rect,
                content: rect,
                scroll: None,
                programmatic_scroll: None,
                scrollbars: None,
                transform: None,
            },
            underline::Style::plain(Color::BLACK),
            (TEXT, FLOW, false),
            &mut 0,
        );
        assert!(matches!(
            result,
            Err(PaintError::Layout(LayoutError::Unsupported(FLOW)))
        ));
    }
}
#[cfg(test)]
mod retained_tests {
    use super::*;
    #[test]
    fn fractional_one_pixel_caret_is_valid_in_debug_and_retained_frames() {
        let rect = Rect::from_xywh(1.5, 1.25, 1.0, 7.0).unwrap();
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 255, 255);
        let mut expected = Pixmap::new(12, 12).unwrap();
        expected.fill_path(
            &PathBuilder::from_rect(rect),
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let mut ordinary = Pixmap::new(12, 12).unwrap();
        scene::fill_rect(&mut ordinary, rect, &paint, Transform::identity(), None);
        let draw = Draw {
            primitive: Primitive::Rect(rect),
            color: Color::from_rgba8(0, 0, 255, 255),
            transform: Transform::identity(),
            clip: None,
            clips: None,
            bounds: rect,
        };
        let mut scene = Scene::default();
        assert_eq!(ordinary.data(), expected.data());
        assert_eq!(
            scene.update(vec![draw.clone()], 12, 12).unwrap(),
            expected.data()
        );
        assert_eq!(scene.update(vec![draw], 12, 12).unwrap(), expected.data());
        assert_eq!(scene.dirty_pixels, 0);
    }
    #[test]
    fn recording_overflow_preserves_existing_clips_and_following_draws() {
        for (record_only, complex) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut retained = Surface {
                glyphs: None,
                pixels: None,
                width: 12,
                height: 12,
                scale: 1.25,
                commands: Some(Recording::default()),
                active_clip: None,
                clip_masks: None,
                failure: None,
                groups: Vec::new(),
                relative_clips: BTreeMap::new(),
                isolation_required: false,
                transforms: 0,
                capture: None,
            };
            let mut reference = Surface {
                glyphs: None,
                pixels: Pixmap::new(12, 12),
                width: 12,
                height: 12,
                scale: 1.25,
                commands: None,
                active_clip: None,
                clip_masks: None,
                failure: None,
                groups: Vec::new(),
                relative_clips: BTreeMap::new(),
                isolation_required: false,
                transforms: 0,
                capture: None,
            };
            if complex {
                let bounds = BoxRect {
                    x: 0.0,
                    y: 0.0,
                    width: 12.0,
                    height: 12.0,
                };
                let radii = Corners([[6.0; 2]; 4]);
                let clip = Clip::new(None, radii.path(bounds), bounds, radii);
                retained.active_clip = Some(clip.clone());
                reference.active_clip = Some(clip);
            }
            let rect = Rect::from_xywh(0.0, 0.0, 9.0, 9.0).unwrap();
            let clip = Rect::from_xywh(1.25, 2.25, 4.5, 3.5).unwrap();
            let deferred = retained.mask(clip).unwrap();
            let ordinary = reference.mask(clip).unwrap();
            let mut paint = Paint::default();
            paint.set_color_rgba8(200, 40, 10, 130);
            retained.fill_rect(rect, &paint, Transform::identity(), Some(&deferred));
            reference.fill_rect(rect, &paint, Transform::identity(), Some(&ordinary));
            // Fill the byte budget without constructing an unnecessarily huge fixture.
            retained.commands.as_mut().unwrap().bytes = scene::MAX_SCENE_BYTES;
            retained.fill_rect(rect, &paint, Transform::identity(), Some(&deferred));
            reference.fill_rect(rect, &paint, Transform::identity(), Some(&ordinary));
            assert!(retained.commands.is_none());
            retained.fill_rect(rect, &paint, Transform::identity(), Some(&deferred));
            reference.fill_rect(rect, &paint, Transform::identity(), Some(&ordinary));
            let path = PathBuilder::from_rect(rect);
            retained.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                Some(&deferred),
            );
            reference.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                Some(&ordinary),
            );
            let mut scene = Scene::default();
            scene.record_only = record_only;
            assert_eq!(
                retained.finish(Some(&mut scene)).unwrap(),
                reference.finish(None).unwrap()
            );
            assert_eq!(scene.dirty_pixels, 144);
            assert!(!scene.record_only);
        }
    }
}
