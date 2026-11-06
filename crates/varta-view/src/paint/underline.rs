//! Solid underline propagation and bounded unhinted ink interception.
use super::clip_curve::{Plane, Point, Segment};
use super::*;

const AUTO_THICKNESS_EM: f32 = 0.1;
const MIN_THICKNESS: f32 = 1.0;
const STRIPE_INSET: f32 = 0.5;
const MAX_INK_DILATION: f32 = 13.0;
const MAX_GLYPHS: usize = 65_536;
// Includes the temporary stroke and its sparse source index, in shared work.
const STROKE_WORK: usize = 96;
// A cubic has at most three y-monotone pieces, each with two stripe crossings.
const CURVE_WORK: usize = clip_curve::ROOT_STEPS * 6;
/// A preserved tab's source text. Collapsible tabs never reach paragraph
/// shaping, so every tab cluster of a paragraph is a preserved one.
const TAB: &str = "\t";

fn charge(work: &mut usize, amount: usize, id: u32) -> Result<(), PaintError> {
    *work = work.saturating_add(amount);
    if *work > MAX_TEXT_PAINT_WORK {
        Err(PaintError::Unsupported(id))
    } else {
        Ok(())
    }
}

/// Decoration is propagated, not inherited: a descendant `none` does not erase it.
pub(super) fn origins(tree: &Tree, env: &Environment, id: u32, own: bool) -> Vec<u32> {
    let mut result = Vec::new();
    for node in tree.path(id) {
        let Some(element) = tree.element(node) else {
            continue;
        };
        if tree
            .computed_property(node, env, CssProperty::TextDecoration)
            .as_deref()
            == Some("underline")
        {
            result.push(node);
        }
        if own
            || matches!(element.tag, Tag::Input | Tag::Textarea | Tag::Select)
            || tree
                .computed_property(node, env, CssProperty::Display)
                .is_some_and(|s| {
                    matches!(s.as_str(), "inline-block" | "inline-flex" | "inline-grid")
                })
            || tree
                .computed_property(node, env, CssProperty::Position)
                .is_some_and(|s| matches!(s.as_str(), "absolute" | "fixed"))
        {
            break;
        }
    }
    result.reverse();
    result
}

pub(super) struct Style {
    pub color: Color,
    decorations: Vec<Color>,
    pub paragraph: bool,
}
impl Style {
    /// Undecorated glyphs at their own baseline, such as list marker text.
    pub fn plain(color: Color) -> Self {
        Self {
            color,
            decorations: Vec::new(),
            paragraph: false,
        }
    }
    pub fn resolve(
        tree: &Tree,
        env: &Environment,
        id: u32,
        own: bool,
        color: Color,
    ) -> Result<Self, PaintError> {
        let decorations = origins(tree, env, id, own)
            .into_iter()
            .map(|origin| current_color(tree, env, origin).ok_or(PaintError::Unsupported(origin)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            color,
            decorations,
            paragraph: !own,
        })
    }
}

struct Stroke {
    owner: u32,
    line: usize,
    rect: BoxRect,
    color: Color,
    raw_thickness: f32,
    gaps: Vec<(f32, f32)>,
}
#[derive(Default)]
pub(super) struct Plan {
    strokes: Vec<Stroke>,
    // Sparse index avoids rescanning the paragraph for every glyph.
    index: BTreeMap<(usize, u32), Vec<usize>>,
    next: usize,
}
impl Plan {
    fn add(
        &mut self,
        source: (u32, usize),
        fragment: BoxRect,
        baseline: f32,
        style: ((f32, f32), Color),
        work: &mut usize,
    ) -> Result<(), PaintError> {
        let (owner, line) = source;
        let (metrics, color) = style;
        if fragment.width <= 0.0 || color.alpha() == 0.0 {
            return Ok(());
        }
        charge(work, STROKE_WORK, owner)?;
        let (size, ascent) = metrics;
        let raw_thickness = (size * AUTO_THICKNESS_EM).max(MIN_THICKNESS);
        let thickness = raw_thickness.floor();
        let gap = (raw_thickness * 0.5).ceil().max(MIN_THICKNESS);
        let y = (baseline - ascent + ascent.round() + gap).floor();
        if ![fragment.x, fragment.width, size, ascent, y, raw_thickness]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(PaintError::Unsupported(owner));
        }
        let index = self.strokes.len();
        self.strokes.push(Stroke {
            owner,
            line,
            rect: BoxRect {
                x: fragment.x,
                y,
                width: fragment.width,
                height: thickness,
            },
            color,
            raw_thickness,
            gaps: Vec::new(),
        });
        self.index.entry((line, owner)).or_default().push(index);
        Ok(())
    }
    pub fn plain(
        layout: &Layout<u32>,
        bounds: layout::BoxGeometry,
        style: &Style,
        white_space: WhiteSpaceMode,
        id: u32,
        work: &mut usize,
    ) -> Result<Self, PaintError> {
        let mut plan = Self::default();
        if style.decorations.is_empty() {
            return Ok(plan);
        }
        let metrics = layout
            .lines()
            .flat_map(|line| line.runs())
            .next()
            .map(|run| {
                (
                    run.font_size(),
                    if style.paragraph {
                        run.metrics().ascent.round()
                    } else {
                        run.metrics().ascent
                    },
                )
            });
        let Some(metrics) = metrics else {
            return Ok(plan);
        };
        for (index, line) in layout.lines().enumerate() {
            let m = line.metrics();
            let fragment = BoxRect {
                x: bounds.rect.x + m.offset + m.inline_min_coord,
                y: 0.0,
                // Preserved trailing spaces are part of the line's text
                // fragment, which the decoration spans.
                width: m.advance
                    - if white_space.preserves() {
                        0.0
                    } else {
                        m.trailing_whitespace
                    },
                height: 0.0,
            };
            for &color in &style.decorations {
                plan.add(
                    (id, index),
                    fragment,
                    bounds.rect.y
                        + m.baseline
                        + if style.paragraph {
                            paragraph_baseline_offset(m)
                        } else {
                            0.0
                        },
                    (metrics, color),
                    work,
                )?;
            }
        }
        Ok(plan)
    }
    pub fn rich(flow: &inline_groups::Flow<'_>, work: &mut usize) -> Result<Self, PaintError> {
        let mut plan = Self::default();
        let bounds = paragraph_paint_bounds(flow.frame.bounds);
        for &owner in &flow.shape.nodes {
            if flow.tree.element(owner).is_some() {
                continue;
            }
            let Some(fragments) = flow.frame.fragments.get(&owner) else {
                continue;
            };
            let origins = origins(flow.tree, flow.env, owner, false);
            for origin in origins {
                let metrics = flow
                    .shape
                    .decoration_metrics(origin)
                    .or_else(|| flow.shape.decoration_metrics(flow.id))
                    .ok_or(PaintError::Unsupported(origin))?;
                let color = current_color(flow.tree, flow.env, origin)
                    .ok_or(PaintError::Unsupported(origin))?;
                for (&fragment, &line) in fragments.iter().zip(&flow.frame.fragment_lines[&owner]) {
                    plan.add(
                        (owner, line),
                        fragment,
                        bounds.rect.y
                            + flow
                                .frame
                                .shaped
                                .layout
                                .lines()
                                .nth(line)
                                .ok_or(PaintError::Unsupported(owner))?
                                .metrics()
                                .baseline
                            + flow.frame.shaped.offsets[line],
                        (metrics, color),
                        work,
                    )?;
                }
            }
        }
        Ok(plan)
    }
    pub fn prepare(
        &mut self,
        surface: &mut Surface,
        source: (
            &Layout<u32>,
            layout::BoxGeometry,
            Option<&crate::inline::Frame>,
            &str,
            u32,
            bool,
        ),
        work: &mut usize,
    ) -> Result<(), PaintError> {
        let (layout, bounds, flow, text, id, paragraph) = source;
        if self.strokes.is_empty() || surface.groups.last().is_some_and(|g| g.hidden) {
            return Ok(());
        }
        if paragraph {
            self.skip_tabs(layout, bounds.rect.x, text, (work, id))?;
        }
        let mut glyph_count = 0usize;
        for (line_index, line) in layout.lines().enumerate() {
            let mut skip = Vec::new();
            for run in line.runs() {
                for cluster in run.visual_clusters() {
                    // CSS auto skip-ink excludes CJK ideographs and syllabaries.
                    let excluded = text
                        .get(cluster.text_range())
                        .is_some_and(|s| s.chars().any(cjk));
                    for _ in cluster.glyphs() {
                        charge(work, 1, id)?;
                        if skip.len() >= MAX_GLYPHS {
                            return Err(PaintError::Unsupported(id));
                        }
                        skip.push(excluded);
                    }
                }
            }
            let mut line_glyph = 0;
            for item in line.items() {
                let glyph_run = match item {
                    PositionedLayoutItem::GlyphRun(glyph_run) => glyph_run,
                    PositionedLayoutItem::InlineBox(edge) if super::glyphless_box(edge.kind) => {
                        continue;
                    }
                    PositionedLayoutItem::InlineBox(_) => return Err(PaintError::Unsupported(id)),
                };
                let run = glyph_run.run();
                let data = run.font();
                let font = FontRef::from_index(data.data.data(), data.index)
                    .map_err(|_| PaintError::Unsupported(id))?;
                let coords: Vec<_> = run
                    .normalized_coords()
                    .iter()
                    .map(|c| NormalizedCoord::from_bits(*c))
                    .collect();
                let location = LocationRef::new(&coords);
                // Static glyf bounds are constant-time. CFF/variable bounds in
                // skrifa redraw the whole outline without our pen's work guard.
                let static_bounds = [*b"gvar", *b"CFF ", *b"CFF2"]
                    .into_iter()
                    .all(|tag| font.table_data(skrifa::Tag::new(&tag)).is_none());
                let metrics = font.glyph_metrics(Size::new(run.font_size()), location);
                let outlines = font.outline_glyphs();
                for glyph in glyph_run.positioned_glyphs() {
                    glyph_count += 1;
                    if glyph_count > MAX_GLYPHS {
                        return Err(PaintError::Unsupported(id));
                    }
                    let owner = match flow {
                        Some(frame) => *frame.glyph_owners[line_index]
                            .get(line_glyph)
                            .ok_or(PaintError::Unsupported(id))?,
                        None => id,
                    };
                    let excluded = *skip.get(line_glyph).ok_or(PaintError::Unsupported(id))?;
                    line_glyph += 1;
                    if excluded
                        || surface
                            .capture
                            .as_ref()
                            .is_some_and(|c| c.hidden.contains(&owner))
                    {
                        continue;
                    }
                    let Some(indices) = self.index.get(&(line_index, owner)) else {
                        continue;
                    };
                    let glyph_id = GlyphId::new(glyph.id);
                    let ink = static_bounds.then(|| metrics.bounds(glyph_id)).flatten();
                    let x = bounds.rect.x + glyph.x;
                    let y = bounds.rect.y
                        + glyph.y
                        + flow.map_or_else(
                            || {
                                if paragraph {
                                    paragraph_baseline_offset(line.metrics())
                                } else {
                                    0.0
                                }
                            },
                            |f| f.shaped.offsets[line_index],
                        );
                    if !x.is_finite() || !y.is_finite() {
                        return Err(PaintError::Unsupported(id));
                    }
                    for &index in indices {
                        let stroke = &mut self.strokes[index];
                        let top = stroke.rect.y + STRIPE_INSET;
                        let bottom = stroke.rect.y + stroke.raw_thickness - STRIPE_INSET;
                        if ink.is_none_or(|ink| y - ink.y_max < bottom && y - ink.y_min > top) {
                            let outline =
                                outlines.get(glyph_id).ok_or(PaintError::Unsupported(id))?;
                            let mut pen = InkPen::new(
                                f64::from(top) - f64::from(y),
                                f64::from(bottom) - f64::from(y),
                                work,
                                id,
                            );
                            outline
                                .draw(
                                    DrawSettings::unhinted(Size::new(run.font_size()), location),
                                    &mut pen,
                                )
                                .map_err(|_| PaintError::Unsupported(id))?;
                            pen.finish()?;
                            let dilation = stroke.raw_thickness.min(MAX_INK_DILATION);
                            let run_left = bounds.rect.x + glyph_run.offset();
                            let run_right = (run_left + glyph_run.advance()).min(
                                bounds.rect.x
                                    + line.metrics().offset
                                    + line.metrics().inline_min_coord
                                    + line.metrics().advance
                                    - line.metrics().trailing_whitespace,
                            );
                            for (left, right) in pen.intervals {
                                // ClipOut uses a hard device-pixel edge. Its
                                // dilation cannot erase a neighboring text run
                                // or a separate hanging-whitespace fragment.
                                let left = (((left + x - dilation) * surface.scale).round()
                                    / surface.scale)
                                    .max(run_left);
                                let right = (((right + x + dilation) * surface.scale).round()
                                    / surface.scale)
                                    .min(run_right);
                                if left < right {
                                    stroke.gaps.push((left, right));
                                }
                            }
                        }
                    }
                }
            }
        }
        for stroke in &mut self.strokes {
            stroke.gaps.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        }
        // The source index is preparation-only. Stable line sorting preserves
        // nested decoration order and lets paint stream once, before each line.
        self.index.clear();
        self.strokes.sort_by_key(|stroke| stroke.line);
        Ok(())
    }
    /// Chrome decorates text fragments, and a preserved tab is a control item
    /// with no text fragment: its whole advance, inside a line or hanging at
    /// its end, is cut from every stroke of its line. Preserved spaces stay
    /// decorated. Field display keeps its own editing model and is unchanged.
    fn skip_tabs(
        &mut self,
        layout: &Layout<u32>,
        left: f32,
        text: &str,
        budget: (&mut usize, u32),
    ) -> Result<(), PaintError> {
        let (work, id) = budget;
        for (line_index, line) in layout.lines().enumerate() {
            let strokes: Vec<usize> = self
                .index
                .range((line_index, u32::MIN)..=(line_index, u32::MAX))
                .flat_map(|(_, indices)| indices.iter().copied())
                .collect();
            if strokes.is_empty() {
                continue;
            }
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let mut x = left + glyph_run.offset();
                for cluster in glyph_run.run().visual_clusters() {
                    let advance = cluster.advance();
                    if text.get(cluster.text_range()) == Some(TAB) {
                        if !x.is_finite() || !advance.is_finite() {
                            return Err(PaintError::Unsupported(id));
                        }
                        for &index in &strokes {
                            charge(work, 1, id)?;
                            self.strokes[index].gaps.push((x, x + advance));
                        }
                    }
                    x += advance;
                }
            }
        }
        Ok(())
    }
    pub fn paint_line(&mut self, surface: &mut Surface, line: usize, mask: Option<&ClipMask>) {
        if surface.groups.last().is_some_and(|g| g.hidden) {
            return;
        }
        while self.next < self.strokes.len() && self.strokes[self.next].line == line {
            let stroke = &self.strokes[self.next];
            self.next += 1;
            if let Some(capture) = surface.capture.as_mut() {
                if capture.hidden.contains(&stroke.owner) {
                    continue;
                }
                capture.owner = stroke.owner;
                capture.line = stroke.line;
            }
            let mut left = stroke.rect.x;
            let end = left + stroke.rect.width;
            let mut paint = Paint::default();
            paint.set_color(stroke.color);
            let mut draw = |left: f32, right: f32| {
                if let Some(rect) =
                    Rect::from_xywh(left, stroke.rect.y, right - left, stroke.rect.height)
                {
                    surface.fill_rect(rect, &paint, Transform::identity(), mask);
                }
            };
            for &(a, b) in &stroke.gaps {
                if a > left {
                    draw(left, a.min(end));
                }
                left = left.max(b);
                if left >= end {
                    break;
                }
            }
            if left < end {
                draw(left, end);
            }
        }
    }
}

fn cjk(c: char) -> bool {
    matches!(u32::from(c), 0x2e80..=0xa4cf | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xffef | 0x1b000..=0x1b2ff | 0x20000..=0x323af)
}

/// Track each contour's projection through the narrow stripe. Curves are split
/// at derivative roots and clipped analytically with bounded root refinement.
struct InkPen<'a> {
    top: f64,
    bottom: f64,
    current: Point,
    first: Point,
    range: Option<(f64, f64)>,
    intervals: Vec<(f32, f32)>,
    work: &'a mut usize,
    id: u32,
    failed: bool,
}
impl<'a> InkPen<'a> {
    fn new(top: f64, bottom: f64, work: &'a mut usize, id: u32) -> Self {
        Self {
            top,
            bottom,
            current: Point([0.0; 2]),
            first: Point([0.0; 2]),
            range: None,
            intervals: Vec::new(),
            work,
            id,
            failed: !top.is_finite() || !bottom.is_finite() || top > bottom,
        }
    }
    fn include(&mut self, x: f64) {
        self.range = Some(self.range.map_or((x, x), |(a, b)| (a.min(x), b.max(x))));
    }
    fn segment(&mut self, segment: Segment) {
        if self.failed {
            return;
        }
        let cost = match segment {
            Segment::Line(..) => 1,
            Segment::Cubic(..) => CURVE_WORK,
        };
        if charge(self.work, cost, self.id).is_err() {
            self.failed = true;
            return;
        }
        let finite = match segment {
            Segment::Line(a, b) => [a, b]
                .into_iter()
                .all(|p| p.0.into_iter().all(f64::is_finite)),
            Segment::Cubic(points) => points
                .into_iter()
                .all(|p| p.0.into_iter().all(f64::is_finite)),
        };
        if !finite {
            self.failed = true;
            return;
        }
        let axis = Plane::new([0.0, 1.0], 0.0).expect("unit axis");
        let (roots, count) = segment.projected_extrema(axis);
        let mut rest = segment;
        let mut previous = 0.0;
        for t in roots.into_iter().take(count).chain(std::iter::once(1.0)) {
            let (mut piece, tail) = rest.split((t - previous) / (1.0 - previous));
            rest = tail;
            previous = t;
            let increasing = piece.start().0[1] <= piece.end().0[1];
            let low = piece.start().0[1].min(piece.end().0[1]);
            let high = piece.start().0[1].max(piece.end().0[1]);
            if high < self.top || low > self.bottom {
                continue;
            }
            for (boundary, keep_above) in [(self.top, true), (self.bottom, false)] {
                let a = piece.start().0[1];
                let b = piece.end().0[1];
                if a.min(b) < boundary && a.max(b) > boundary {
                    let plane =
                        Plane::new([0.0, 1.0], boundary).expect("validated finite font stripe");
                    let (first, last) = match piece {
                        Segment::Line(p, q) => piece.split((boundary - p.0[1]) / (q.0[1] - p.0[1])),
                        Segment::Cubic(..) => piece.crossing(plane),
                    };
                    piece = if increasing == keep_above {
                        last
                    } else {
                        first
                    };
                }
            }
            self.include(piece.start().0[0]);
            self.include(piece.end().0[0]);
            let x_axis = Plane::new([1.0, 0.0], 0.0).expect("unit axis");
            let (roots, count) = piece.projected_extrema(x_axis);
            for t in roots.into_iter().take(count) {
                self.include(piece.split(t).0.end().0[0]);
            }
        }
    }
    fn finish(&mut self) -> Result<(), PaintError> {
        self.close();
        if self.failed {
            Err(PaintError::Unsupported(self.id))
        } else {
            Ok(())
        }
    }
}
impl OutlinePen for InkPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.current = Point([f64::from(x), -f64::from(y)]);
        self.first = self.current;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let end = Point([f64::from(x), -f64::from(y)]);
        self.segment(Segment::Line(self.current, end));
        self.current = end;
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let control = [f64::from(cx), -f64::from(cy)];
        let end = Point([f64::from(x), -f64::from(y)]);
        let a = Point(std::array::from_fn(|i| {
            self.current.0[i] + (control[i] - self.current.0[i]) * (2.0 / 3.0)
        }));
        let b = Point(std::array::from_fn(|i| {
            end.0[i] + (control[i] - end.0[i]) * (2.0 / 3.0)
        }));
        self.segment(Segment::Cubic([self.current, a, b, end]));
        self.current = end;
    }
    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        let end = Point([f64::from(x), -f64::from(y)]);
        self.segment(Segment::Cubic([
            self.current,
            Point([f64::from(ax), -f64::from(ay)]),
            Point([f64::from(bx), -f64::from(by)]),
            end,
        ]));
        self.current = end;
    }
    fn close(&mut self) {
        if self.current != self.first {
            self.segment(Segment::Line(self.current, self.first));
        }
        self.current = self.first;
        if let Some((a, b)) = self.range.take() {
            self.intervals.push((a as f32, b as f32));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_outline_coordinates_are_rejected_before_root_math() {
        let mut work = 0;
        assert!(InkPen::new(f64::NAN, 2.0, &mut work, 2).finish().is_err());
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut work = 0;
            let mut pen = InkPen::new(1.0, 2.0, &mut work, 2);
            pen.move_to(0.0, 0.0);
            pen.quad_to(value, -3.0, 4.0, 0.0);
            assert!(pen.finish().is_err());
        }
    }
    #[test]
    fn quadratic_stripe_extrema_are_kept_without_flattening() {
        let mut work = 0;
        let mut pen = InkPen::new(4.0, 5.0, &mut work, 2);
        pen.move_to(0.0, 0.0);
        pen.quad_to(10.0, -10.0, 20.0, 0.0);
        pen.close();
        pen.finish().unwrap();
        let [(left, right)] = pen.intervals.as_slice() else {
            panic!("one curved contour")
        };
        let expected = 10.0 * (1.0 - 0.2_f32.sqrt());
        assert!((left - expected).abs() < 0.0001);
        assert!((right - (20.0 - expected)).abs() < 0.0001);
        assert!(work >= CURVE_WORK);
    }
    #[test]
    fn independent_contours_and_shared_work_refusal_do_not_bridge_empty_ink() {
        let mut work = 0;
        let mut pen = InkPen::new(1.0, 2.0, &mut work, 2);
        for x in [0.0, 20.0] {
            pen.move_to(x, 0.0);
            pen.line_to(x, -3.0);
            pen.line_to(x + 4.0, -3.0);
            pen.line_to(x + 4.0, 0.0);
            pen.close();
        }
        pen.finish().unwrap();
        assert_eq!(pen.intervals, [(0.0, 4.0), (20.0, 24.0)]);
        let mut work = MAX_TEXT_PAINT_WORK;
        let mut pen = InkPen::new(1.0, 2.0, &mut work, 2);
        pen.move_to(0.0, 0.0);
        pen.curve_to(0.0, -5.0, 5.0, -5.0, 5.0, 0.0);
        assert!(pen.finish().is_err());
    }
}
