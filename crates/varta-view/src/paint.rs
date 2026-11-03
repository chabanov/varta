//! Bounded CPU painting for an explicitly limited subset of the view tree.
mod select;
use crate::TextMeasurer;
use crate::layout::{self, BoxRect, LayoutError, ScrollMetrics, TextShape};
use crate::style::{self, Environment, Name};
use crate::text::{TextSource, WhiteSpaceMode};
use crate::tree::{Content, Tree, border_color_sides, outline_components};
use parley::{Affinity, Cursor, Layout, PositionedLayoutItem, Selection};
pub use select::{SelectHit, SelectMenuMetrics};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    outline::{DrawSettings, OutlinePen},
};
use std::{borrow::Cow, collections::BTreeMap};
use tiny_skia::{Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Transform};
use unicode_segmentation::UnicodeSegmentation;
use varta_common::view::{Attribute, CssProperty, ROOT_NODE, Tag};

const PASSWORD_MASK: &str = "•";

/// A transparent, premultiplied RGBA8 image in row-major order.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    scale: f32,
    hit_regions: Vec<(u32, BoxRect)>,
    ancestors: BTreeMap<u32, Option<u32>>,
    scrolls: BTreeMap<u32, ScrollMetrics>,
    focused: Option<u32>,
    editable: Option<u32>,
    text_hits: BTreeMap<u32, TextHit>,
    select_menu: Option<Box<select::SelectMenu>>,
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
impl RasterImage {
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
struct Surface {
    pixels: Pixmap,
    scale: f32,
}
impl Surface {
    fn width(&self) -> u32 {
        self.pixels.width()
    }
    fn height(&self) -> u32 {
        self.pixels.height()
    }
    fn fill_rect(
        &mut self,
        rect: Rect,
        paint: &Paint<'_>,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        self.pixels.fill_rect(
            rect,
            paint,
            transform.post_scale(self.scale, self.scale),
            mask,
        );
    }
    fn fill_path(
        &mut self,
        path: &tiny_skia::Path,
        paint: &Paint<'_>,
        rule: FillRule,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        self.pixels.fill_path(
            path,
            paint,
            rule,
            transform.post_scale(self.scale, self.scale),
            mask,
        );
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
        if let Some(value) = tree.computed(ancestor, env).get(&CssProperty::Color)
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
        Some((_, raw)) => raw
            .strip_suffix("px")
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite() && v.abs() <= 1_000_000.0)
            .ok_or(PaintError::Unsupported(id))?,
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
        let mut mask = Mask::new(pixmap.width(), pixmap.height()).ok_or(PaintError::Dimensions)?;
        let Some(clip_rect) = Rect::from_xywh(clip.x, clip.y, clip.width, clip.height) else {
            return Ok(());
        };
        mask.fill_path(
            &PathBuilder::from_rect(clip_rect),
            FillRule::Winding,
            true,
            Transform::from_scale(pixmap.scale, pixmap.scale),
        );
        Some(mask)
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
    layout: &Layout<()>,
    bounds: layout::BoxGeometry,
    color: Color,
    id: u32,
    work: &mut usize,
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
        let mut mask = Mask::new(pixmap.width(), pixmap.height()).ok_or(PaintError::Dimensions)?;
        mask.fill_path(
            &PathBuilder::from_rect(clip),
            FillRule::Winding,
            true,
            Transform::from_scale(pixmap.scale, pixmap.scale),
        );
        Some(mask)
    };
    let mut paint = Paint::default();
    paint.set_color(color);
    let mut glyph_count = 0usize;
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                return Err(PaintError::Unsupported(id));
            };
            let run = glyph_run.run();
            if run.synthesis().embolden() || run.synthesis().skew().is_some() {
                return Err(PaintError::Unsupported(id));
            }
            let font_data = run.font();
            let font = FontRef::from_index(font_data.data.data(), font_data.index)
                .map_err(|_| PaintError::Unsupported(id))?;
            let outlines = font.outline_glyphs();
            let coords: Vec<_> = run
                .normalized_coords()
                .iter()
                .map(|coord| NormalizedCoord::from_bits(*coord))
                .collect();
            for glyph in glyph_run.positioned_glyphs() {
                glyph_count += 1;
                if glyph_count > 65_536 {
                    return Err(PaintError::Unsupported(id));
                }
                let outline = outlines
                    .get(GlyphId::new(glyph.id))
                    .ok_or(PaintError::Unsupported(id))?;
                let mut pen = GlyphPen {
                    path: PathBuilder::new(),
                    x: bounds.rect.x + glyph.x,
                    y: bounds.rect.y + glyph.y,
                    commands: 0,
                    exceeded: false,
                };
                let settings =
                    DrawSettings::unhinted(Size::new(run.font_size()), LocationRef::new(&coords));
                outline
                    .draw(settings, &mut pen)
                    .map_err(|_| PaintError::Unsupported(id))?;
                *work = work.saturating_add(pen.commands.saturating_add(1));
                if pen.exceeded || *work > 1_000_000 {
                    return Err(PaintError::Unsupported(id));
                }
                if let Some(path) = pen.path.finish() {
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
        let mut mask =
            Mask::new(surface.width(), surface.height()).ok_or(PaintError::Dimensions)?;
        mask.fill_path(
            &PathBuilder::from_rect(clip),
            FillRule::Winding,
            true,
            Transform::from_scale(surface.scale, surface.scale),
        );
        Some(mask)
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
        tree.computed(ancestor, env)
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
            rasterize_inner_scoped(tree, env, width, height, Some((measurer, size)), scale)
        }),
        None => rasterize_inner_scoped(tree, env, width, height, None, scale),
    }
}

fn rasterize_inner_scoped(
    tree: &Tree,
    env: &Environment,
    width: u32,
    height: u32,
    mut text_input: Option<(&mut TextMeasurer, f32)>,
    scale: f32,
) -> Result<RasterImage, PaintError> {
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
    let env = Environment {
        width: f64::from(width) / f64::from(scale),
        dark: env.dark,
    };
    let (geometry, shapes) = if let Some((measurer, font_size)) = text_input.as_mut() {
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
        )
    };
    let mut pixmap = Surface {
        pixels: Pixmap::new(width, height).ok_or(PaintError::Dimensions)?,
        scale,
    };
    let mut pending = vec![(ROOT_NODE, false)];
    let mut text_work = 0usize;
    let mut hit_regions = Vec::new();
    let mut ancestors = BTreeMap::new();
    let mut scrolls = BTreeMap::new();
    let mut text_hits = BTreeMap::new();
    let mut menu_control = None;
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
                tree.computed(ancestor, &env)
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
        let properties = tree.computed(id, &env);
        if properties
            .get(&CssProperty::Display)
            .is_some_and(|v| v.eq_ignore_ascii_case("none"))
        {
            continue;
        }
        ancestors.insert(id, node.parent);
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
                            .all(|part| matches!(part, "0" | "0px"))
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
                            .all(|part| matches!(part, "0" | "0px"))
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
                        .all(|part| matches!(part, "0" | "0px"))
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
        if element.tag != Tag::Select {
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
    Ok(RasterImage {
        width,
        height,
        pixels: pixmap.pixels.take(),
        scale,
        hit_regions,
        ancestors,
        scrolls,
        focused,
        editable,
        text_hits,
        select_menu,
    })
}
