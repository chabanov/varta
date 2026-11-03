//! Bounded CPU painting for an explicitly limited subset of the view tree.
mod select;
use crate::TextMeasurer;
use crate::layout::{self, BoxRect, LayoutError, ScrollMetrics, TextShape};
use crate::render::glyphs::{self, Glyphs};
use crate::render::scene::{self, Draw, Primitive, Recording, Scene};
use crate::style::{self, Environment, Name};
use crate::text::{TextSource, WhiteSpaceMode};
use crate::tree::{Content, Tree, border_color_sides, outline_components};
use crate::units::Units;
use parley::{Affinity, Cursor, Layout, PositionedLayoutItem, Selection};
pub use select::{SelectHit, SelectMenuMetrics};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    outline::{DrawSettings, OutlinePen},
};
use std::{borrow::Cow, collections::BTreeMap};
use tiny_skia::{Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Shader, Transform};
use unicode_segmentation::UnicodeSegmentation;
use varta_common::view::{Attribute, CssProperty, ROOT_NODE, Tag};

const PASSWORD_MASK: &str = "•";

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
    ancestors: BTreeMap<u32, Option<u32>>,
    scrolls: BTreeMap<u32, ScrollMetrics>,
    focused: Option<u32>,
    editable: Option<u32>,
    text_hits: BTreeMap<u32, TextHit>,
    select_menu: Option<Box<select::SelectMenu>>,
    cursors: BTreeMap<u32, PointerCursor>,
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
    /// Each visual cluster's midpoint and its leading/trailing byte positions.
    clusters: Vec<(f32, usize, usize)>,
    end: usize,
    composing: bool,
    caret_area: Option<BoxRect>,
    lines: Vec<TextLineHit>,
    scroll: Option<ScrollMetrics>,
    caret_y: f32,
    origin_x: f32,
    preferred_x: f32,
    multiline: bool,
}
#[derive(Debug, Clone, PartialEq)]
struct TextLineHit {
    top: f32,
    bottom: f32,
    clusters: std::ops::Range<usize>,
    start: usize,
    end: usize,
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
            && self.ancestors == other.ancestors
            && self.scrolls == other.scrolls
            && self.focused == other.focused
            && self.editable == other.editable
            && self.text_hits == other.text_hits
            && self.select_menu == other.select_menu
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
        for (midpoint, before, after) in &hit.clusters {
            if x < *midpoint {
                return Some(*before);
            }
            last = *after;
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
            .find(|line| y < line.bottom)
            .or_else(|| hit.lines.last())?;
        let mut last = line.start;
        for (midpoint, before, after) in &hit.clusters[line.clusters.clone()] {
            if x < *midpoint {
                return Some(*before);
            }
            last = *after;
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
            .position(|line| hit.caret_y < line.bottom)
            .unwrap_or(hit.lines.len().checked_sub(1)?);
        let (line, boundary, preferred_x) = match key {
            "Home" => (current, Some(hit.lines[current].start), None),
            "End" => (current, Some(hit.lines[current].end), None),
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
                (destination.top + destination.bottom) / 2.0,
            )
        })?;
        let upstream = position == destination.end
            && hit
                .lines
                .get(line + 1)
                .is_some_and(|next| next.start == position);
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
        self.hit_regions.iter().rev().find_map(|(id, rect)| {
            (x >= rect.x && y >= rect.y && x < rect.x + rect.width && y < rect.y + rect.height)
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
    glyphs: Option<&'a mut Glyphs>,
    pixels: Option<Pixmap>,
    width: u32,
    height: u32,
    scale: f32,
    commands: Option<Recording>,
    failure: Option<PaintError>,
}
impl Surface<'_> {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn mask(&self, rect: Rect) -> Result<ClipMask, PaintError> {
        let image = if self.commands.is_some() {
            None
        } else {
            let mut image = Mask::new(self.width, self.height).ok_or(PaintError::Dimensions)?;
            image.fill_path(
                &PathBuilder::from_rect(rect),
                FillRule::Winding,
                true,
                Transform::from_scale(self.scale, self.scale),
            );
            Some(image)
        };
        Ok(ClipMask { rect, image })
    }
    fn record(
        &mut self,
        primitive: Primitive,
        paint: &Paint<'_>,
        transform: Transform,
        mask: Option<&ClipMask>,
    ) {
        if self.failure.is_some() {
            return;
        }
        let Shader::SolidColor(color) = paint.shader else {
            self.failure = Some(PaintError::Unsupported(ROOT_NODE));
            return;
        };
        let bounds = match &primitive {
            Primitive::Rect(rect) => *rect,
            Primitive::Path(path, _) => path.bounds(),
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
        let draw = Draw {
            primitive,
            color,
            transform,
            clip,
            bounds,
        };
        if let Some(recording) = self.commands.as_mut() {
            let bytes = draw.bytes();
            if recording.commands.len() < scene::MAX_SCENE_COMMANDS
                && recording.bytes.saturating_add(bytes) <= scene::MAX_SCENE_BYTES
            {
                recording.bytes += bytes;
                recording.commands.push(draw);
                return;
            }
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
    fn fallback_mask(&self, mask: Option<&ClipMask>) -> Result<Option<ClipMask>, PaintError> {
        // A mask may have been created before a recording overflow. Subsequent
        // ordinary draws must materialize that same clip rather than drop it.
        mask.filter(|mask| mask.image.is_none())
            .map(|mask| self.mask(mask.rect))
            .transpose()
    }
    fn fill_rect(
        &mut self,
        rect: Rect,
        paint: &Paint<'_>,
        transform: Transform,
        mask: Option<&ClipMask>,
    ) {
        if self.failure.is_some() {
            return;
        }
        if self.commands.is_some() {
            self.record(Primitive::Rect(rect), paint, transform, mask);
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
        if self.failure.is_some() {
            return;
        }
        if self.commands.is_some() {
            self.record(
                Primitive::Path(std::sync::Arc::new(path.clone()), rule),
                paint,
                transform,
                mask,
            );
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
    fn finish(mut self, scene: Option<&mut Scene>) -> Result<Vec<u8>, PaintError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if let Some(recording) = self.commands.take()
            && let Some(scene) = scene
        {
            if scene.record_only {
                scene.record(recording.commands);
                return Ok(Vec::new());
            }
            return scene.update(recording.commands, self.width, self.height);
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

fn current_color(tree: &Tree, env: &Environment, id: u32) -> Option<Color> {
    for ancestor in tree.path(id) {
        if let Some(value) = tree.computed_read(ancestor, env).get(&CssProperty::Color)
            && value != "currentcolor"
        {
            return color(value);
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

fn border_color(
    tree: &Tree,
    env: &Environment,
    id: u32,
    widths: [f32; 4],
) -> Result<Option<Color>, PaintError> {
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
    let mut result = None;
    for (width, value) in widths.into_iter().zip(values) {
        if width == 0.0 {
            continue;
        }
        let next = if value == "currentcolor" {
            current_color(tree, env, id)
        } else {
            color(&value)
        }
        .ok_or(PaintError::Unsupported(id))?;
        if result.is_some_and(|previous| previous != next) {
            return Err(PaintError::Unsupported(id));
        }
        result = Some(next);
    }
    Ok(result)
}

fn outline(
    tree: &Tree,
    env: &Environment,
    id: u32,
) -> Result<Option<(f32, f32, Color)>, PaintError> {
    let units = Units::new(env).ok_or(layout::LayoutError::RootFont)?;
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
    let raw_color = get(CssProperty::OutlineColor).map_or("currentcolor", |(property, raw)| {
        if property == CssProperty::Outline {
            shorthand_color(raw)
        } else {
            raw
        }
    });
    let color = if raw_color == "currentcolor" {
        current_color(tree, env, id)
    } else {
        color(raw_color)
    }
    .ok_or(PaintError::Unsupported(id))?;
    let offset = match get(CssProperty::OutlineOffset) {
        Some((_, "0")) | None => 0.0,
        Some((_, raw)) => units.pixels(raw, true).ok_or(PaintError::Unsupported(id))?,
    };
    Ok(Some((width, offset, color)))
}

fn paint_border(
    pixmap: &mut Surface,
    bounds: BoxRect,
    clip: BoxRect,
    widths: [f32; 4],
    color: Color,
) -> Result<(), PaintError> {
    let Some(outer) = Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height) else {
        return Ok(());
    };
    if clipped(bounds, clip).is_none() {
        return Ok(());
    }
    let mut path = PathBuilder::new();
    path.push_rect(outer);
    if let Some(inner) = Rect::from_xywh(
        bounds.x + widths[3],
        bounds.y + widths[0],
        bounds.width - widths[3] - widths[1],
        bounds.height - widths[0] - widths[2],
    ) {
        path.push_rect(inner);
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
    paint.set_color(color);
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::EvenOdd,
        Transform::identity(),
        mask.as_ref(),
    );
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

fn paint_text(
    pixmap: &mut Surface,
    measurer: &mut TextMeasurer,
    shape: &TextShape,
    bounds: layout::BoxGeometry,
    color: Color,
    id: u32,
    work: &mut usize,
) -> Result<(), PaintError> {
    let (_, layout) = measurer
        .shape_styled(
            &shape.text,
            Some(bounds.rect.width.clamp(0.01, 1_000_000.0)),
            &shape.families,
            shape.options,
        )
        .map_err(|error| PaintError::Layout(LayoutError::Text(error.to_string())))?;
    paint_shaped(pixmap, &layout, bounds, color, id, work)
}

// Field caret/selection/hit geometry and glyph painting must use the same
// shaping result. Keep this separate from paragraph shaping so a field never
// needs a second layout or a cloned display buffer just to paint its glyphs.
fn paint_shaped(
    pixmap: &mut Surface,
    layout: &Layout<u32>,
    bounds: layout::BoxGeometry,
    color: Color,
    id: u32,
    work: &mut usize,
) -> Result<(), PaintError> {
    paint_shaped_with(pixmap, layout, bounds, |_| Ok(color), id, work, None)
}

fn paint_shaped_with(
    pixmap: &mut Surface,
    layout: &Layout<u32>,
    bounds: layout::BoxGeometry,
    mut color: impl FnMut(u32) -> Result<Color, PaintError>,
    id: u32,
    work: &mut usize,
    flow: Option<&crate::inline::Frame>,
) -> Result<(), PaintError> {
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
        let mut line_glyph = 0usize;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                return Err(PaintError::Unsupported(id));
            };
            let base_color = color(glyph_run.style().brush)?;
            paint.set_color(base_color);
            let run = glyph_run.run();
            if run.synthesis().embolden() || run.synthesis().skew().is_some() {
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
                let glyph_color = match flow {
                    Some(frame) => color(
                        *frame.glyph_owners[line_index]
                            .get(line_glyph)
                            .ok_or(PaintError::Unsupported(id))?,
                    )?,
                    None => base_color,
                };
                line_glyph += 1;
                paint.set_color(glyph_color);
                glyph_count += 1;
                if glyph_count > 65_536 {
                    return Err(PaintError::Unsupported(id));
                }
                let x = bounds.rect.x + glyph.x;
                let y =
                    bounds.rect.y + glyph.y + flow.map_or(0.0, |f| f.shaped.offsets[line_index]);
                let key = pixmap.glyphs.as_ref().map(|_| glyphs::Key {
                    font: font_data.data.id(),
                    index: font_data.index,
                    size: run.font_size().to_bits(),
                    coords: run.normalized_coords().to_vec(),
                    glyph: glyph.id,
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
                if *work > 1_000_000 {
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
    if *work > 1_000_000 {
        return Err(PaintError::Unsupported(id));
    }
    if tree.path(id).into_iter().any(|ancestor| {
        tree.computed_read(ancestor, env)
            .get(&CssProperty::TextDecoration)
            .is_some_and(|value| value != "none")
    }) || element
        .value
        .chars()
        .any(|ch| ch.is_control() && !(element.tag == Tag::Textarea && matches!(ch, '\n' | '\t')))
    {
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
    let password_positions = if element.input_type() == "password" {
        let positions: Vec<_> = display
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .chain(std::iter::once(display.len()))
            .collect();
        let masked_position =
            |offset| positions.partition_point(|position| *position < offset) * PASSWORD_MASK.len();
        selection = (masked_position(selection.0), masked_position(selection.1));
        preedit_range = preedit_range.map(|(start, end)| {
            let start = positions
                .partition_point(|position| *position <= start)
                .saturating_sub(1)
                * PASSWORD_MASK.len();
            (start, masked_position(end))
        });
        display = Cow::Owned(PASSWORD_MASK.repeat(positions.len() - 1));
        *work = work.saturating_add(display.len());
        if *work > 1_000_000 {
            return Err(PaintError::Unsupported(id));
        }
        Some(positions)
    } else {
        None
    };
    let source_position = |offset: usize| {
        password_positions.as_ref().map_or_else(
            || offset.min(source_len),
            |positions| positions[(offset / PASSWORD_MASK.len()).min(positions.len() - 1)],
        )
    };
    let (mut options, families) = layout::text_style(tree, env, id, font_size, measurer)?;
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
    let clip = layout::intersect(bounds.clip, content);
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
    let caret_x = caret_bounds.x0 as f32;
    let x = if multiline {
        content.x
    } else {
        content.x - (caret_x - (content.width - 1.0).max(0.0)).max(0.0)
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
                if display.get(range.clone()) == Some("\n") {
                    continue;
                }
                let before = source_position(if cluster.is_rtl() {
                    range.end
                } else {
                    range.start
                });
                let after = source_position(if cluster.is_rtl() {
                    range.start
                } else {
                    range.end
                });
                clusters.push((position + advance / 2.0, before, after));
                position += advance;
                if clusters.len() > 65_536 {
                    return Err(PaintError::Unsupported(id));
                }
            }
        }
        lines.push(TextLineHit {
            top: y + line.metrics().block_min_coord,
            bottom: y + line.metrics().block_max_coord,
            clusters: cluster_start..clusters.len(),
            start: source_position(line.text_range().start),
            end: {
                let end = line.text_range().end.min(display.len());
                source_position(if display[..end].ends_with('\n') {
                    end - 1
                } else {
                    end
                })
            },
        });
    }
    let color = current_color(tree, env, id).ok_or(PaintError::Unsupported(id))?;
    if tree.focus == Some(id) && !element.disabled() && selection.0 != selection.1 {
        let anchor = Cursor::from_byte_index(&shaped, selection.0, Affinity::Downstream);
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
                y,
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
        };
        paint_shaped(pixmap, &shaped, glyph_bounds, color, id, work)?;
    } else if let Some(placeholder) = element.attributes.get(&Attribute::Placeholder)
        && !placeholder.is_empty()
    {
        if placeholder.chars().any(char::is_control) {
            return Err(PaintError::Unsupported(id));
        }
        *work = work.saturating_add(placeholder.len());
        if *work > 1_000_000 {
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
                y,
                width: content.width.max(0.01),
                height: metrics.height,
            },
            clip,
            content,
            scroll: None,
            programmatic_scroll: None,
        };
        paint_shaped(
            pixmap,
            &placeholder_layout,
            glyph_bounds,
            Color::from_rgba8(117, 117, 117, 255),
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
        x: (x + caret_bounds.x0 as f32).min(content.x + (content.width - 1.0).max(0.0)),
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
        clusters,
        end: source_len,
        composing: element.editor.preedit.is_some(),
        lines,
        caret_y: y + ((caret_bounds.y0 + caret_bounds.y1) / 2.0) as f32,
        origin_x: x,
        preferred_x: element.editor.preferred_x.unwrap_or(caret_x),
        multiline,
        scroll: multiline.then_some(ScrollMetrics {
            top: text_scroll,
            max: text_max_scroll,
            viewport: scrollport,
        }),
        caret_area: clipped(caret_rect, clip).map(|rect| BoxRect {
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

pub(crate) fn validate_dimensions(width: u32, height: u32, scale: f32) -> Result<(), PaintError> {
    // A bounded image prevents a guest-controlled viewport exhausting host memory.
    if !scale.is_finite()
        || !(0.25..=8.0).contains(&scale)
        || width == 0
        || height == 0
        || width > 4096
        || height > 4096
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
            layout::geometry(tree, &env, width as f32 / scale, height as f32 / scale)?,
            BTreeMap::new(),
            BTreeMap::new(),
        )
    };
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
        failure: None,
    };
    let mut pending = vec![(ROOT_NODE, false)];
    let mut text_work = 0usize;
    let mut hit_regions = Vec::new();
    let mut ancestors = BTreeMap::new();
    let mut scrolls = BTreeMap::new();
    let mut text_hits = BTreeMap::new();
    let mut menu_control = None;
    let mut cursors = BTreeMap::new();
    while let Some((id, outline_phase)) = pending.pop() {
        if outline_phase {
            if let Some((width, offset, color)) = outline(tree, &env, id)? {
                let bounds = geometry[&id];
                let expansion = width + offset;
                let rect = BoxRect {
                    x: bounds.rect.x - expansion,
                    y: bounds.rect.y - expansion,
                    width: bounds.rect.width + 2.0 * expansion,
                    height: bounds.rect.height + 2.0 * expansion,
                };
                paint_border(&mut pixmap, rect, bounds.clip, [width; 4], color)?;
            }
            continue;
        }
        let node = &tree.nodes[&id];
        let Content::Element(element) = &node.content else {
            let shape = shapes.get(&id).ok_or(PaintError::Unsupported(id))?;
            let (measurer, _) = text_input.as_mut().ok_or(PaintError::Unsupported(id))?;
            let parent = node.parent.ok_or(PaintError::Unsupported(id))?;
            if tree.path(parent).into_iter().any(|ancestor| {
                tree.computed_read(ancestor, &env)
                    .get(&CssProperty::TextDecoration)
                    .is_some_and(|value| value != "none")
            }) {
                return Err(PaintError::Unsupported(id));
            }
            let color = current_color(tree, &env, parent).ok_or(PaintError::Unsupported(id))?;
            paint_text(
                &mut pixmap,
                measurer,
                shape,
                geometry[&id],
                color,
                id,
                &mut text_work,
            )?;
            if let Some(rect) = clipped(geometry[&id].rect, geometry[&id].clip) {
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
            continue;
        };
        let properties = tree.computed_read(id, &env);
        if properties
            .get(&CssProperty::Display)
            .is_some_and(|v| v.eq_ignore_ascii_case("none"))
        {
            continue;
        }
        ancestors.insert(id, node.parent);
        cursors.insert(id, pointer_cursor(tree, &env, id));
        if let Some(scroll) = geometry[&id].scroll {
            scrolls.insert(id, scroll);
        }
        let button_ready = element.tag != Tag::Button
            || (properties
                .get(&CssProperty::Display)
                .is_some_and(|value| value == "flex")
                && properties.contains_key(&CssProperty::BackgroundColor)
                && properties.contains_key(&CssProperty::Color)
                && (properties.contains_key(&CssProperty::Outline)
                    || properties.contains_key(&CssProperty::OutlineStyle))
                && properties
                    .get(&CssProperty::BorderRadius)
                    .is_some_and(|value| {
                        value
                            .split_ascii_whitespace()
                            .all(|part| units.pixels(part, false) == Some(0.0))
                    }));
        let input_ready = !matches!(element.tag, Tag::Input | Tag::Textarea | Tag::Select)
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
                && properties
                    .get(&CssProperty::BorderRadius)
                    .is_some_and(|value| {
                        value
                            .split_ascii_whitespace()
                            .all(|part| units.pixels(part, false) == Some(0.0))
                    }));
        if !input_ready
            || !button_ready
            || properties
                .get(&CssProperty::Position)
                .is_some_and(|v| v != "static")
            || properties
                .get(&CssProperty::ZIndex)
                .is_some_and(|v| v != "auto")
            || properties
                .get(&CssProperty::Opacity)
                .is_some_and(|v| v.parse::<f32>().ok() != Some(1.0))
            || properties
                .get(&CssProperty::BorderRadius)
                .is_some_and(|value| {
                    !value
                        .split_ascii_whitespace()
                        .all(|part| units.pixels(part, false) == Some(0.0))
                })
        {
            return Err(PaintError::Unsupported(id));
        }
        if id != ROOT_NODE
            && let Some(rect) = clipped(geometry[&id].rect, geometry[&id].clip)
        {
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
        if let Some(value) = properties.get(&CssProperty::BackgroundColor) {
            let color = if value == "currentcolor" {
                current_color(tree, &env, id)
            } else {
                color(value)
            }
            .ok_or(PaintError::Unsupported(id))?;
            let bounds = geometry[&id];
            if let Some(rect) = clipped(bounds.rect, bounds.clip) {
                let mut paint = Paint::default();
                paint.set_color(color);
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
        let widths = layout::border_widths(tree, &env, id)?;
        if let Some(color) = border_color(tree, &env, id, widths)? {
            let bounds = geometry[&id];
            paint_border(&mut pixmap, bounds.rect, bounds.clip, widths, color)?;
        }
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
            if element.checked {
                let color = current_color(tree, &env, id).ok_or(PaintError::Unsupported(id))?;
                paint_checkable(
                    &mut pixmap,
                    geometry[&id],
                    color,
                    element.input_type() == "radio",
                )?;
            }
        } else if matches!(element.tag, Tag::Input | Tag::Textarea) {
            let (measurer, font_size) = text_input.as_mut().ok_or(PaintError::Unsupported(id))?;
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
        pending.push((id, true));
        if let Some(frame) = flows.get(&id) {
            let shape = shapes
                .get(&id)
                .and_then(|s| s.rich.as_ref())
                .ok_or(PaintError::Unsupported(id))?;
            // Bare text can overflow its container without belonging to an
            // inline element. Its visible fragments still target the container.
            for &child in &node.children {
                if matches!(tree.nodes[&child].content, Content::Text(_)) {
                    for &piece in frame.fragments.get(&child).into_iter().flatten() {
                        if let Some(rect) = clipped(piece, frame.bounds.clip) {
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
                }
            }
            for &node in &shape.nodes {
                if tree.element(node).is_none() {
                    continue;
                }
                ancestors.insert(node, tree.nodes[&node].parent);
                cursors.insert(node, pointer_cursor(tree, &env, node));
                let background = tree
                    .computed_read(node, &env)
                    .get(&CssProperty::BackgroundColor)
                    .map(|value| {
                        if value == "currentcolor" {
                            current_color(tree, &env, node)
                        } else {
                            color(value)
                        }
                        .ok_or(PaintError::Unsupported(node))
                    })
                    .transpose()?;
                for &rect in frame.fragments.get(&node).into_iter().flatten() {
                    if let Some(rect) = clipped(rect, frame.bounds.clip) {
                        hit_regions.push((
                            node,
                            BoxRect {
                                x: rect.x(),
                                y: rect.y(),
                                width: rect.width(),
                                height: rect.height(),
                            },
                        ));
                        if let Some(c) = background {
                            let mut paint = Paint::default();
                            paint.set_color(c);
                            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                        }
                    }
                }
            }
            let mut colors = BTreeMap::new();
            paint_shaped_with(
                &mut pixmap,
                &frame.shaped.layout,
                frame.bounds,
                |owner| {
                    if let Some(c) = colors.get(&owner) {
                        return Ok(*c);
                    }
                    let c =
                        current_color(tree, &env, owner).ok_or(PaintError::Unsupported(owner))?;
                    colors.insert(owner, c);
                    Ok(c)
                },
                id,
                &mut text_work,
                Some(frame),
            )?;
        } else if element.tag != Tag::Select {
            pending.extend(node.children.iter().rev().map(|id| (*id, false)));
        }
    }
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
        for (midpoint, _, _) in &mut hit.clusters {
            *midpoint *= scale;
        }
        hit.caret_area = hit.caret_area.map(|rect| scaled_rect(rect, scale));
        for line in &mut hit.lines {
            line.top *= scale;
            line.bottom *= scale;
        }
    }
    let pixels = pixmap.finish(scene)?;
    Ok(RasterImage {
        pixels,
        metadata: FrameMetadata {
            width,
            height,
            scale,
            hit_regions,
            ancestors,
            scrolls,
            focused,
            editable,
            text_hits,
            select_menu,
            cursors,
        },
    })
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
        for record_only in [false, true] {
            let mut retained = Surface {
                glyphs: None,
                pixels: None,
                width: 12,
                height: 12,
                scale: 1.25,
                commands: Some(Recording::default()),
                failure: None,
            };
            let mut reference = Surface {
                glyphs: None,
                pixels: Pixmap::new(12, 12),
                width: 12,
                height: 12,
                scale: 1.25,
                commands: None,
                failure: None,
            };
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
