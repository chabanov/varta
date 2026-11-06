//! One preparation of inline ink, deferred into its source opacity contexts.
//! Source draws keep logical rectangle clips and original physical curve chains;
//! inherited scope removal happens only when the context actually paints.
use super::*;
use std::sync::Arc;

pub(super) struct SourceDraw {
    pub primitive: Primitive,
    pub color: Color,
    pub transform: Transform,
    pub clip: Option<Rect>,
    pub clips: Option<Arc<Clip>>,
}
#[derive(Default)]
pub(super) struct Capture {
    pub owner: u32,
    pub line: usize,
    pub hidden: std::collections::BTreeSet<u32>,
    draws: Vec<(u32, usize, SourceDraw)>,
    outline_draws: Vec<(u32, SourceDraw)>,
    outline_phase: bool,
}
impl Capture {
    pub fn push(&mut self, draw: SourceDraw) -> Result<(), PaintError> {
        if self.hidden.contains(&self.owner) {
            return Ok(());
        }
        let total = self.draws.len().saturating_add(self.outline_draws.len());
        if total >= scene::MAX_FRAME_COMMANDS {
            return Err(PaintError::Unsupported(self.owner));
        }
        if self.outline_phase {
            self.outline_draws
                .try_reserve(1)
                .map_err(|_| PaintError::Dimensions)?;
            self.outline_draws.push((self.owner, draw));
            return Ok(());
        }
        if self.draws.len() == self.draws.capacity() {
            if self.draws.len() < scene::MAX_SCENE_COMMANDS {
                self.draws
                    .try_reserve(1)
                    .map_err(|_| PaintError::Dimensions)?;
            } else {
                let extra = scene::MAX_SCENE_COMMANDS.min(scene::MAX_FRAME_COMMANDS - total);
                self.draws
                    .try_reserve_exact(extra)
                    .map_err(|_| PaintError::Dimensions)?;
            }
        }
        self.draws.push((self.owner, self.line, draw));
        Ok(())
    }
}
impl Surface<'_> {
    fn source_draw(&mut self, source: SourceDraw) -> Result<(), PaintError> {
        self.active_clip = source.clips;
        let mask = source
            .clip
            .map(|r| {
                let r = self.local_clip(BoxRect {
                    x: r.x(),
                    y: r.y(),
                    width: r.width(),
                    height: r.height(),
                });
                if r.x <= 0.0
                    && r.y <= 0.0
                    && (r.x + r.width) * self.scale >= self.width as f32
                    && (r.y + r.height) * self.scale >= self.height as f32
                {
                    Ok(None)
                } else {
                    Rect::from_xywh(r.x, r.y, r.width, r.height)
                        .ok_or(PaintError::Dimensions)
                        .and_then(|r| self.mask(r))
                        .map(Some)
                }
            })
            .transpose()?
            .flatten();
        let mut paint = Paint::default();
        paint.set_color(source.color);
        if self.commands.is_some() {
            self.record(source.primitive, &paint, source.transform, mask.as_ref());
        } else {
            match source.primitive {
                Primitive::Rect(rect) => {
                    self.fill_rect(rect, &paint, source.transform, mask.as_ref());
                }
                Primitive::Path(path, rule) => {
                    self.fill_path(&path, &paint, rule, source.transform, mask.as_ref());
                }
                _ => unreachable!("inline capture admits only background and glyph contours"),
            }
        }
        Ok(())
    }
}
type OutlineOrder = (usize, std::cmp::Reverse<usize>);
type OutlineDraws = Vec<(OutlineOrder, SourceDraw)>;

pub(super) struct Hit {
    target: u32,
    rect: BoxRect,
    rounded: Option<inline_decor::Region>,
    clips: Option<Arc<Clip>>,
}
pub(super) struct Metadata<'a> {
    pub hits: &'a mut Vec<(u32, BoxRect)>,
    pub rounded: &'a mut BTreeMap<usize, inline_decor::Region>,
    pub clips: &'a mut BTreeMap<usize, Arc<Clip>>,
    pub ancestors: &'a mut BTreeMap<u32, Option<u32>>,
    pub cursors: &'a mut BTreeMap<u32, PointerCursor>,
}
impl Metadata<'_> {
    fn push(&mut self, hit: Hit) {
        if let Some(rounded) = hit.rounded {
            self.rounded.insert(self.hits.len(), rounded);
        }
        if let Some(clips) = hit.clips {
            self.clips.insert(self.hits.len(), clips);
        }
        self.hits.push((hit.target, hit.rect));
    }
}
struct Program {
    opacity: f32,
    clip: BoxRect,
    clips: Option<Arc<Clip>>,
    draws: Vec<((usize, usize, usize), SourceDraw)>,
    hits: Vec<((usize, usize, usize), Hit)>,
    children: Vec<u32>,
    outlines: OutlineDraws,
}
impl Program {
    fn new(opacity: f32, clip: BoxRect, clips: Option<Arc<Clip>>) -> Self {
        Self {
            opacity,
            clip,
            clips,
            draws: Vec::new(),
            hits: Vec::new(),
            children: Vec::new(),
            outlines: Vec::new(),
        }
    }
    fn paint(
        mut self,
        pixmap: &mut Surface,
        metadata: &mut Metadata,
        pending: &mut order::Order,
    ) -> Result<OutlineDraws, PaintError> {
        self.draws.sort_unstable_by_key(|(key, _)| *key);
        self.hits.sort_unstable_by_key(|(key, _)| *key);
        for (_, draw) in self.draws {
            pixmap.source_draw(draw)?;
        }
        for (_, hit) in self.hits {
            metadata.push(hit);
        }
        for child in self.children {
            pending.defer_inline(child);
        }
        self.outlines.sort_unstable_by_key(|(order, _)| *order);
        Ok(self.outlines)
    }
}
#[derive(Default)]
pub(super) struct Programs {
    contexts: BTreeMap<u32, Program>,
    outlines: BTreeMap<u32, OutlineDraws>,
}
impl Programs {
    pub fn paint(
        &mut self,
        id: u32,
        pixmap: &mut Surface,
        metadata: &mut Metadata,
        pending: &mut order::Order,
    ) -> Result<(), PaintError> {
        let program = self
            .contexts
            .remove(&id)
            .ok_or(PaintError::Unsupported(id))?;
        pixmap.active_clip = program.clips.clone();
        pixmap.begin_group(id, program.opacity, program.clip)?;
        pending.finish_after_children(id, order::Phase::InlineContext);
        let outlines = program.paint(pixmap, metadata, pending)?;
        self.defer_outlines(id, outlines, pending);
        Ok(())
    }
    fn defer_outlines(&mut self, id: u32, draws: OutlineDraws, pending: &mut order::Order) {
        if !draws.is_empty() {
            self.outlines.insert(id, draws);
            pending.inline_outline(id);
        }
    }
    pub fn paint_outlines(&mut self, id: u32, surface: &mut Surface) -> Result<(), PaintError> {
        let draws = self
            .outlines
            .remove(&id)
            .ok_or(PaintError::Unsupported(id))?;
        for (_, draw) in draws {
            surface.source_draw(draw)?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Owner {
    context: u32,
    hidden: bool,
    order: usize,
    end: usize,
}
struct Contexts {
    owners: BTreeMap<u32, Owner>,
    programs: BTreeMap<u32, Program>,
}
impl Contexts {
    fn new(flow: &Flow, clips: Option<Arc<Clip>>, hidden: bool) -> Result<Self, PaintError> {
        let mut owners = BTreeMap::from([(
            flow.id,
            Owner {
                context: flow.id,
                hidden,
                order: 0,
                end: 0,
            },
        )]);
        let mut programs = BTreeMap::from([(
            flow.id,
            Program::new(1.0, flow.frame.bounds.clip, clips.clone()),
        )]);
        for (index, &id) in flow.shape.nodes.iter().enumerate() {
            let parent = flow.tree.nodes[&id]
                .parent
                .ok_or(PaintError::Unsupported(id))?;
            let mut owner = *owners.get(&parent).ok_or(PaintError::Unsupported(id))?;
            owner.order = index + 1;
            owner.end = owner.order;
            if flow.shape.hidden(id) {
                owner.hidden = true;
            } else if flow.tree.element(id).is_some() && !flow.shape.atom(id) {
                let opacity = flow
                    .tree
                    .computed_property(id, flow.env, CssProperty::Opacity)
                    .map(|raw| {
                        raw.parse::<f32>()
                            .ok()
                            .filter(|a| a.is_finite() && (0.0..=1.0).contains(a))
                            .ok_or(PaintError::Unsupported(id))
                    })
                    .transpose()?
                    .unwrap_or(1.0);
                if opacity != 1.0 {
                    programs
                        .get_mut(&owner.context)
                        .expect("parent context is prepared first")
                        .children
                        .push(id);
                    programs.insert(
                        id,
                        Program::new(opacity, flow.frame.bounds.clip, clips.clone()),
                    );
                    owner.context = id;
                    owner.hidden |= opacity == 0.0;
                }
            }
            owners.insert(id, owner);
        }
        // Outline painting is postorder: descendants precede their ancestor;
        // peer subtrees retain source order. Content still uses preorder.
        for &id in flow.shape.nodes.iter().rev() {
            let end = owners[&id].end;
            let parent = flow.tree.nodes[&id]
                .parent
                .ok_or(PaintError::Unsupported(id))?;
            let p = owners.get_mut(&parent).ok_or(PaintError::Unsupported(id))?;
            p.end = p.end.max(end);
        }
        Ok(Self { owners, programs })
    }
    fn program(&mut self, owner: u32) -> Result<&mut Program, PaintError> {
        let context = self
            .owners
            .get(&owner)
            .ok_or(PaintError::Unsupported(owner))?
            .context;
        self.programs
            .get_mut(&context)
            .ok_or(PaintError::Unsupported(owner))
    }
}
pub(super) struct Flow<'a> {
    pub text: &'a str,
    pub id: u32,
    pub tree: &'a Tree,
    pub env: &'a Environment,
    pub frame: &'a crate::inline::Frame,
    pub shape: &'a crate::inline::ContentFlow,
    pub units: Units,
}
impl Flow<'_> {
    /// A fragment's paint window and, inside a deferred context, the mask of
    /// the flow's inherited clip. `None` when the fragment is clipped away.
    fn window(
        &self,
        pixmap: &mut Surface,
        piece: BoxRect,
    ) -> Result<Option<(Rect, Option<ClipMask>)>, PaintError> {
        // A deferred context must apply its inherited clip once, after
        // isolation. Keep source geometry uncut by that clip.
        let clip = pixmap.local_clip(self.frame.bounds.clip);
        let paint_piece = snap::rect(piece);
        if pixmap.capture.is_none() {
            return Ok(clipped(paint_piece, clip).map(|rect| (rect, None)));
        }
        let Some(rect) = Rect::from_xywh(
            paint_piece.x,
            paint_piece.y,
            paint_piece.width,
            paint_piece.height,
        ) else {
            return Ok(None);
        };
        let mask = Rect::from_xywh(clip.x, clip.y, clip.width, clip.height)
            .map(|r| pixmap.mask(r))
            .transpose()?;
        Ok(Some((rect, mask)))
    }
    /// A solid inline border: its continuous strip, widths and the one color
    /// of its visible sides. Distinct side colors are not admitted inline.
    fn border<'p>(
        &self,
        node: u32,
        pieces: &[BoxRect],
        properties: &'p BTreeMap<CssProperty, String>,
    ) -> Result<Option<(inline_decor::Decoration<'p>, [f64; 4], Color)>, PaintError> {
        let widths = layout::border_widths(self.tree, self.env, node)
            .map_err(|_| PaintError::Unsupported(node))?;
        let Some(colors) = border_colors(self.tree, self.env, node, widths)? else {
            return Ok(None);
        };
        let mut visible = widths
            .into_iter()
            .zip(colors)
            .filter_map(|(width, color)| (width > 0.0).then_some(color));
        let Some(color) = visible.next() else {
            return Ok(None);
        };
        if visible.any(|other| other != color) {
            return Err(PaintError::Unsupported(node));
        }
        if color.alpha() == 0.0 {
            return Ok(None);
        }
        let raw = properties
            .get(&CssProperty::BorderRadius)
            .map_or("0", String::as_str);
        let strip = inline_decor::Decoration::strip(raw, pieces, self.units, node)?;
        Ok(Some((strip, widths.map(f64::from), color)))
    }
    pub fn paint(
        &self,
        pixmap: &mut Surface,
        work: &mut usize,
        metadata: &mut Metadata,
        pending: &mut order::Order,
        programs: &mut Programs,
    ) -> Result<(), PaintError> {
        let hit_scope = pixmap.active_clip.clone();
        let hidden = pixmap.groups.last().is_some_and(|g| g.hidden);
        let mut contexts = self
            .shape
            .needs_paint_order
            .then(|| Contexts::new(self, hit_scope.clone(), hidden))
            .transpose()?;
        if let Some(contexts) = &contexts {
            pixmap.capture = Some(Capture {
                hidden: contexts
                    .owners
                    .iter()
                    .filter_map(|(&id, owner)| owner.hidden.then_some(id))
                    .collect(),
                ..Default::default()
            });
        }
        let mut ordinary_hits = Vec::new();
        // Browsers hit-test line boxes last to first. Inside a later line box,
        // a point between that line's fragments hits the block even where an
        // earlier line's decorated fragment overlaps it; a point within a
        // fragment's rectangle (even outside its curve) is left to the
        // fragments. Insert those gaps after earlier lines, before this one.
        for (line, gap) in line_gaps(self.frame, self.shape) {
            let Some(rect) = clipped(gap, self.frame.bounds.clip) else {
                continue;
            };
            let hit = Hit {
                target: self.id,
                rect: BoxRect {
                    x: rect.x(),
                    y: rect.y(),
                    width: rect.width(),
                    height: rect.height(),
                },
                rounded: None,
                clips: hit_scope.clone(),
            };
            if let Some(contexts) = contexts.as_mut() {
                let program = contexts.program(self.id)?;
                let sequence = program.hits.len();
                program.hits.push(((line, 0, sequence), hit));
            } else {
                ordinary_hits
                    .try_reserve(1)
                    .map_err(|_| PaintError::Dimensions)?;
                let sequence = ordinary_hits.len();
                ordinary_hits.push(((line, 0, sequence), hit));
            }
        }
        for (source_order, &node) in self.shape.nodes.iter().enumerate() {
            // An atomic inline paints and hit-tests as its own box after
            // the flow (`order::Order::inline_atoms`).
            if self.shape.atom(node) {
                continue;
            }
            if let Some(capture) = pixmap.capture.as_mut() {
                capture.owner = node;
            }
            let pieces = self
                .frame
                .fragments
                .get(&node)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if self.tree.element(node).is_some() {
                metadata
                    .ancestors
                    .insert(node, self.tree.nodes[&node].parent);
                metadata
                    .cursors
                    .insert(node, pointer_cursor(self.tree, self.env, node));
            }
            let mut hit = |target, piece, rounded, line| -> Result<(), PaintError> {
                if let Some(rect) = clipped(piece, self.frame.bounds.clip) {
                    let hit = Hit {
                        target,
                        rect: BoxRect {
                            x: rect.x(),
                            y: rect.y(),
                            width: rect.width(),
                            height: rect.height(),
                        },
                        rounded,
                        clips: hit_scope.clone(),
                    };
                    if let Some(contexts) = contexts.as_mut() {
                        let order = contexts.owners[&node].order;
                        let program = contexts.program(node)?;
                        let sequence = program.hits.len();
                        program.hits.push(((line, order, sequence), hit));
                    } else {
                        ordinary_hits
                            .try_reserve(1)
                            .map_err(|_| PaintError::Dimensions)?;
                        let sequence = ordinary_hits.len();
                        ordinary_hits.push(((line, source_order, sequence), hit));
                    }
                }
                Ok(())
            };
            if self.tree.element(node).is_none() {
                let target = self.tree.nodes[&node]
                    .parent
                    .ok_or(PaintError::Unsupported(node))?;
                for (index, &piece) in pieces.iter().enumerate() {
                    hit(target, piece, None, self.frame.fragment_lines[&node][index])?;
                }
                continue;
            }
            let properties = self.tree.computed_read(node, self.env);
            let background = properties
                .get(&CssProperty::BackgroundColor)
                .map(|value| {
                    if value == "currentcolor" {
                        current_color(self.tree, self.env, node)
                    } else {
                        color(value)
                    }
                    .ok_or(PaintError::Unsupported(node))
                })
                .transpose()?;
            let mut decoration = properties
                .get(&CssProperty::BorderRadius)
                .map(|raw| inline_decor::Decoration::new(raw, pieces, self.units, node))
                .transpose()?
                .flatten();
            let mut border = self.border(node, pieces, &properties)?;
            let hidden = hidden
                || pixmap
                    .capture
                    .as_ref()
                    .is_some_and(|c| c.hidden.contains(&node));
            let mut offset = 0.0;
            for (index, &piece) in pieces.iter().enumerate() {
                let line = self.frame.fragment_lines[&node][index];
                if let Some(capture) = pixmap.capture.as_mut() {
                    capture.line = line;
                }
                let slice = decoration
                    .as_mut()
                    .map(|shape| shape.slice(piece, offset, node))
                    .transpose()?
                    .flatten();
                let ring = border
                    .as_mut()
                    .map(|(strip, _, _)| strip.slice(piece, offset, node))
                    .transpose()?
                    .flatten();
                let fill = background.filter(|c| c.alpha() > 0.0);
                if (fill.is_some() || ring.is_some())
                    && !hidden
                    && clipped(piece, self.frame.bounds.clip).is_some()
                    && let Some((rect, mask)) = self.window(pixmap, piece)?
                {
                    if let Some(c) = fill {
                        let mut paint = Paint::default();
                        paint.set_color(c);
                        if let Some(slice) = &slice {
                            charge_paint(work, inline_decor::SLICE_PAINT_WORK, node)?;
                            if let Some(path) = slice.path(rect) {
                                pixmap.fill_path(
                                    &path,
                                    &paint,
                                    FillRule::Winding,
                                    Transform::identity(),
                                    mask.as_ref(),
                                );
                            }
                        } else {
                            pixmap.fill_rect(rect, &paint, Transform::identity(), mask.as_ref());
                        }
                    }
                    // CSS paints each fragment's border over its own background,
                    // before content. The ring is the strip's, sliced here.
                    if let (Some(ring), Some((_, widths, color))) = (&ring, &border) {
                        charge_paint(work, inline_decor::RING_PAINT_WORK, node)?;
                        if let Some(path) = ring.ring(rect, *widths) {
                            let mut paint = Paint::default();
                            paint.set_color(*color);
                            pixmap.fill_path(
                                &path,
                                &paint,
                                FillRule::EvenOdd,
                                Transform::identity(),
                                mask.as_ref(),
                            );
                        }
                    }
                }
                hit(node, piece, slice.map(inline_decor::Region::Slice), line)?;
                offset += f64::from(piece.width);
            }
        }
        let mut underlines = underline::Plan::rich(self, work)?;
        let mut colors = BTreeMap::new();
        paint_shaped_with(
            pixmap,
            &self.frame.shaped.layout,
            paragraph_paint_bounds(self.frame.bounds),
            |owner| {
                if let Some(c) = colors.get(&owner) {
                    return Ok(*c);
                }
                let c = current_color(self.tree, self.env, owner)
                    .ok_or(PaintError::Unsupported(owner))?;
                colors.insert(owner, c);
                Ok(c)
            },
            self.id,
            work,
            TextInk {
                flow: Some(self.frame),
                paragraph: true,
                underlines: Some((&mut underlines, self.text)),
            },
        )?;
        if let Some(capture) = pixmap.capture.as_mut() {
            capture.outline_phase = true;
        }
        for &node in &self.shape.nodes {
            if self.tree.element(node).is_none() || self.shape.atom(node) {
                continue;
            }
            if let Some(capture) = pixmap.capture.as_mut() {
                capture.owner = node;
            }
            inline_outline::Painter {
                tree: self.tree,
                env: self.env,
                node,
                pieces: self
                    .frame
                    .fragments
                    .get(&node)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                clip: self.frame.bounds.clip,
                units: self.units,
            }
            .paint(pixmap, work)?;
        }
        if let Some(mut contexts) = contexts {
            let capture = pixmap.capture.take().expect("inline source capture began");
            for (owner, draw) in capture.outline_draws {
                let o = contexts.owners[&owner];
                let order = (o.end, std::cmp::Reverse(o.order));
                let program = contexts.program(owner)?;
                program
                    .outlines
                    .try_reserve(1)
                    .map_err(|_| PaintError::Dimensions)?;
                program.outlines.push((order, draw));
            }
            for (sequence, (owner, line, draw)) in capture.draws.into_iter().enumerate() {
                let order = contexts.owners[&owner].order;
                let program = contexts.program(owner)?;
                program
                    .draws
                    .try_reserve(1)
                    .map_err(|_| PaintError::Dimensions)?;
                program.draws.push(((line, order, sequence), draw));
            }
            let root = contexts
                .programs
                .remove(&self.id)
                .expect("ordinary inline context exists");
            let outlines = root.paint(pixmap, metadata, pending)?;
            programs.defer_outlines(self.id, outlines, pending);
            programs.contexts.extend(contexts.programs);
        }
        ordinary_hits.sort_unstable_by_key(|(key, _)| *key);
        for (_, hit) in ordinary_hits {
            metadata.push(hit);
        }
        pixmap.active_clip = hit_scope;
        Ok(())
    }
}

/// For each line after the first, the parts of its line box between the
/// horizontal extents of its visible fragments, within their overall extent.
/// Bounded by the fragment count: each fragment adds at most one gap.
fn line_gaps(
    frame: &crate::inline::Frame,
    shape: &crate::inline::ContentFlow,
) -> Vec<(usize, BoxRect)> {
    let mut spans = BTreeMap::<usize, Vec<[f32; 2]>>::new();
    for &node in &shape.nodes {
        if shape.hidden(node) {
            continue;
        }
        let (Some(pieces), Some(lines)) =
            (frame.fragments.get(&node), frame.fragment_lines.get(&node))
        else {
            continue;
        };
        for (piece, &line) in pieces.iter().zip(lines) {
            if line > 0 && piece.width > 0.0 {
                spans
                    .entry(line)
                    .or_default()
                    .push([piece.x, piece.x + piece.width]);
            }
        }
    }
    let mut gaps = Vec::new();
    for (line, mut spans) in spans {
        let Some(&[top, bottom]) = frame.shaped.line_boxes.get(line) else {
            continue;
        };
        spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut reach = spans[0][1];
        for &[left, right] in &spans[1..] {
            if left > reach {
                gaps.push((
                    line,
                    BoxRect {
                        x: reach,
                        y: frame.bounds.rect.y + top,
                        width: left - reach,
                        height: bottom - top,
                    },
                ));
            }
            reach = reach.max(right);
        }
    }
    gaps
}

/// Reserve contour work against the shared text paint allowance.
fn charge_paint(work: &mut usize, amount: usize, node: u32) -> Result<(), PaintError> {
    *work = work.saturating_add(amount);
    if *work > MAX_TEXT_PAINT_WORK {
        return Err(PaintError::Unsupported(node));
    }
    Ok(())
}
