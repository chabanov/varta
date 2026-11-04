//! Bounded rich-text formatting contexts for inline descendants of a box.
use crate::layout::{self, BoxGeometry, BoxRect, LayoutError, TextShape};
use crate::style::Environment;
use crate::text::{TextSpan, WhiteSpaceMode};
use crate::tree::{Content, Tree};
use crate::{TextMeasureError, TextMeasurer, TextMetrics};
use parley::Layout;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use taffy::style::{ExpandedLengthPercentage, LengthPercentage};
use varta_common::view::{CssProperty as P, ROOT_NODE, Tag};

pub(crate) const MAX_FRAGMENT_COUNT: usize = 65_536;

#[derive(Default)]
pub(crate) struct Inventory {
    pub nodes: usize,
    bytes: usize,
    leaves: usize,
}
impl Inventory {
    pub fn node(&mut self, depth: usize) -> Result<(), LayoutError> {
        self.nodes += 1;
        if self.nodes > 4096 || depth > 128 {
            return Err(LayoutError::Limit);
        }
        Ok(())
    }
    pub fn text(&mut self, bytes: usize) -> Result<(), LayoutError> {
        self.bytes = self.bytes.checked_add(bytes).ok_or(LayoutError::Limit)?;
        self.leaves += 1;
        if self.bytes > 256 * 1024 || self.leaves > 1024 {
            return Err(LayoutError::Limit);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct Vertical {
    above: f32,
    below: f32,
}
impl Vertical {
    fn run(run: &parley::Run<'_, u32>) -> Self {
        let m = run.metrics();
        let above = m.ascent.round()
            + ((m.line_height - m.ascent.round() - m.descent.round()) * 0.5).floor();
        Self {
            above,
            below: m.line_height - above,
        }
    }
}

pub(crate) struct ContentFlow {
    spans: Vec<TextSpan>,
    paths: BTreeMap<u32, Vec<u32>>,
    pub nodes: Vec<u32>,
    pub needs_paint_order: bool,
    hidden: BTreeSet<u32>,
    strut: Vertical,
    font_boxes: BTreeMap<u32, FontBox>,
    empty: BTreeMap<usize, Vec<u32>>,
}
impl ContentFlow {
    pub(crate) fn decoration_metrics(&self, id: u32) -> Option<(f32, f32)> {
        self.font_boxes.get(&id).map(|f| (f.size, f.ascent))
    }
    pub(crate) fn hidden(&self, id: u32) -> bool {
        self.hidden.contains(&id)
    }
}
pub(crate) struct ShapedFlow {
    pub layout: Layout<u32>,
    pub offsets: Vec<f32>,
    pub metrics: TextMetrics,
}
#[derive(Clone, Copy)]
enum PaddingLength {
    Pixels(f32),
    Fraction(f32),
}
impl From<LengthPercentage> for PaddingLength {
    fn from(value: LengthPercentage) -> Self {
        match value.expand() {
            ExpandedLengthPercentage::Length(value) => Self::Pixels(value),
            ExpandedLengthPercentage::Percent(value) => Self::Fraction(value),
        }
    }
}
#[derive(Clone, Copy)]
struct FontBox {
    size: f32,
    ascent: f32,
    descent: f32,
    vertical: Vertical,
    // Keep plain owned lengths in the shared flow. Taffy's compact layout type
    // can contain an opaque pointer and must not enter the retained Arc.
    padding: [PaddingLength; 2],
}
impl FontBox {
    fn padding(self, width: f32) -> Result<[f32; 2], LayoutError> {
        let mut padding = [0.0; 2];
        for (out, value) in padding.iter_mut().zip(self.padding) {
            *out = match value {
                PaddingLength::Pixels(value) => value,
                PaddingLength::Fraction(value) => value * width,
            };
            if !out.is_finite() || !(0.0..=crate::units::MAX_LENGTH).contains(out) {
                return Err(LayoutError::Limit);
            }
        }
        Ok(padding)
    }
    fn padded(self, mut piece: BoxRect, width: f32) -> Result<BoxRect, LayoutError> {
        let padding = self.padding(width)?;
        piece.y -= padding[0];
        piece.height += padding[0] + padding[1];
        if !piece.y.is_finite()
            || !piece.height.is_finite()
            || piece.height > crate::units::MAX_LENGTH
        {
            return Err(LayoutError::Limit);
        }
        Ok(piece)
    }
}
#[derive(Clone, Copy)]
struct ClusterGroup {
    owner: u32,
    advance: f32,
    offset: f32,
    primary: bool,
}
pub(crate) struct Frame {
    pub shaped: ShapedFlow,
    pub bounds: BoxGeometry,
    pub fragments: BTreeMap<u32, Vec<BoxRect>>,
    pub fragment_lines: BTreeMap<u32, Vec<usize>>,
    pub glyph_owners: Vec<Vec<u32>>,
}

pub(crate) fn inline_tag(tag: Tag) -> bool {
    matches!(
        tag,
        Tag::Span | Tag::Strong | Tag::Em | Tag::Code | Tag::A | Tag::Br
    )
}
pub(crate) fn blockified(tree: &Tree, env: &Environment, id: u32) -> bool {
    tree.nodes[&id].parent.is_some_and(|parent| {
        tree.computed_property(parent, env, P::Display)
            .is_some_and(|d| matches!(d.as_str(), "flex" | "grid"))
    })
}
fn is_inline(tree: &Tree, env: &Environment, id: u32) -> bool {
    match &tree.nodes[&id].content {
        Content::Text(_) => true,
        Content::Element(e) => match tree.computed_property(id, env, P::Display).as_deref() {
            Some("none") => true,
            Some("inline") => {
                matches!(
                    e.tag,
                    Tag::Div
                        | Tag::P
                        | Tag::Header
                        | Tag::Nav
                        | Tag::Main
                        | Tag::Section
                        | Tag::Footer
                        | Tag::Aside
                        | Tag::Form
                ) || inline_tag(e.tag)
            }
            None => inline_tag(e.tag),
            _ => false,
        },
    }
}

fn validate(tree: &Tree, env: &Environment, id: u32) -> Result<[LengthPercentage; 2], LayoutError> {
    let properties = tree.computed_read(id, env);
    for (&property, value) in properties.iter() {
        let supported = match property {
            P::Display => value == "inline",
            P::FontSize
            | P::LineHeight
            | P::FontFamily
            | P::FontWeight
            | P::FontStyle
            | P::WhiteSpace
            | P::OverflowWrap
            | P::Color
            | P::Cursor
            | P::BorderRadius
            | P::BackgroundColor => true,
            P::TextDecoration => matches!(value.as_str(), "none" | "underline"),
            P::Position => value == "static",
            P::ZIndex => value == "auto",
            P::Opacity => value
                .parse::<f32>()
                .ok()
                .is_some_and(|a| a.is_finite() && (0.0..=1.0).contains(&a)),
            // Non-replaced inline boxes ignore these dimensions/flex properties.
            P::Width
            | P::Height
            | P::MinWidth
            | P::MaxWidth
            | P::MinHeight
            | P::MaxHeight
            | P::FlexGrow
            | P::FlexShrink
            | P::FlexBasis
            | P::AlignSelf => true,
            P::Margin
            | P::MarginTop
            | P::MarginRight
            | P::MarginBottom
            | P::MarginLeft
            | P::Padding
            | P::PaddingTop
            | P::PaddingRight
            | P::PaddingBottom
            | P::PaddingLeft => true, // Resolved together in cascade order below.
            P::Border
            | P::BorderTop
            | P::BorderRight
            | P::BorderBottom
            | P::BorderLeft
            | P::BorderWidth
            | P::BorderStyle
            | P::BorderColor => layout::border_widths(tree, env, id)?
                .iter()
                .all(|&w| w == 0.0),
            P::Outline | P::OutlineWidth | P::OutlineStyle | P::OutlineColor | P::OutlineOffset => {
                true
            }
            _ => false,
        };
        if !supported {
            return Err(LayoutError::Unsupported(id));
        }
    }
    let padding = layout::inline_vertical_padding(tree, env, id)?;
    if tree.element(id).is_some_and(|e| {
        e.tag == Tag::A
            && e.attributes
                .contains_key(&varta_common::view::Attribute::Href)
    }) && (!properties.contains_key(&P::Color)
        || properties
            .get(&P::TextDecoration)
            .is_none_or(|v| !matches!(v.as_str(), "none" | "underline")))
    {
        return Err(LayoutError::Unsupported(id));
    }
    Ok(padding)
}

pub(crate) fn collect(
    tree: &Tree,
    env: &Environment,
    location: (u32, usize),
    font: (&mut TextMeasurer, f32),
    inventory: &mut Inventory,
) -> Result<Option<TextShape>, LayoutError> {
    let (root, depth) = location;
    let (measurer, size) = font;
    let node = &tree.nodes[&root];
    let Some(element) = tree.element(root) else {
        return Ok(None);
    };
    let display = tree.computed_property(root, env, P::Display);
    let block = match display.as_deref() {
        Some("block") => true,
        Some("inline") => blockified(tree, env, root),
        None => !inline_tag(element.tag) || blockified(tree, env, root),
        _ => false,
    };
    if !block
        || node.children.is_empty()
        || !node.children.iter().all(|&id| is_inline(tree, env, id))
    {
        return Ok(None);
    }
    let needed = root == ROOT_NODE
        || inline_tag(element.tag)
        || node.children.len() > 1
        || node
            .children
            .iter()
            .any(|id| matches!(tree.nodes[id].content, Content::Element(_)));
    if !needed {
        return Ok(None);
    }
    let (options, families) = layout::text_style(tree, env, root, size, measurer)?;
    let mut text = String::new();
    let mut spans = Vec::<TextSpan>::new();
    let mut paths = BTreeMap::new();
    let mut nodes = Vec::new();
    let mut needs_paint_order = tree.path(root).into_iter().any(|id| {
        tree.computed_property(id, env, P::TextDecoration)
            .as_deref()
            == Some("underline")
    });
    let mut font_boxes = BTreeMap::new();
    let mut anchors = BTreeMap::new();
    let mut pending: Vec<_> = node
        .children
        .iter()
        .rev()
        .map(|&id| (id, depth + 1, Vec::<u32>::new(), false))
        .collect();
    let mut whitespace = None;
    let mut hidden_nodes = BTreeSet::new();
    let mut previous_cr = false;
    while let Some((id, depth, mut path, hidden)) = pending.pop() {
        inventory.node(depth)?;
        measurer
            .charge_inline_work(1)
            .map_err(|_| LayoutError::Limit)?;
        nodes.push(id);
        let node = &tree.nodes[&id];
        let hidden = hidden
            || tree
                .computed_property(id, env, P::Display)
                .is_some_and(|d| d == "none");
        if hidden {
            hidden_nodes.insert(id);
            pending.extend(
                node.children
                    .iter()
                    .rev()
                    .map(|&child| (child, depth + 1, Vec::new(), true)),
            );
            continue;
        }
        path.insert(0, id);
        let source = match &node.content {
            Content::Element(e) => {
                if !is_inline(tree, env, id) {
                    return Err(LayoutError::Unsupported(id));
                }
                let padding = validate(tree, env, id)?;
                needs_paint_order |= tree
                    .computed_property(id, env, P::Opacity)
                    .is_some_and(|raw| raw.parse::<f32>().ok() != Some(1.0));
                needs_paint_order |= tree
                    .computed_property(id, env, P::BackgroundColor)
                    .is_some_and(|raw| raw != "transparent");
                needs_paint_order |= tree
                    .computed_property(id, env, P::TextDecoration)
                    .as_deref()
                    == Some("underline");
                needs_paint_order |= [P::Outline, P::OutlineStyle].into_iter().any(|p| {
                    tree.computed_property(id, env, p)
                        .is_some_and(|v| v != "none")
                });
                anchors.insert(id, text.len() + usize::from(whitespace.is_some()));
                let (style, names) = layout::text_style(tree, env, id, size, measurer)?;
                let (_, sample) = measurer
                    .shape_styled(" ", None, &names, style)
                    .map_err(|e| LayoutError::Text(e.to_string()))?;
                if let Some(run) = sample.lines().next().and_then(|line| line.runs().next()) {
                    font_boxes.insert(
                        id,
                        FontBox {
                            size: run.font_size(),
                            ascent: run.metrics().ascent.round(),
                            descent: run.metrics().descent.round(),
                            vertical: Vertical::run(&run),
                            padding: padding.map(PaddingLength::from),
                        },
                    );
                }
                if e.tag != Tag::Br {
                    measurer
                        .charge_inline_work(node.children.len().saturating_mul(path.len() + 1))
                        .map_err(|_| LayoutError::Limit)?;
                    pending.extend(
                        node.children
                            .iter()
                            .rev()
                            .map(|&child| (child, depth + 1, path.clone(), false)),
                    );
                    continue;
                }
                "\n"
            }
            Content::Text(value) => value,
        };
        inventory.text(source.len())?;
        let (style, names) = layout::text_style(tree, env, id, size, measurer)?;
        if (style.white_space == WhiteSpaceMode::PreWrap)
            != (options.white_space == WhiteSpaceMode::PreWrap)
        {
            return Err(LayoutError::Unsupported(id));
        }
        let index = spans.len();
        spans.push(TextSpan {
            owner: id,
            range: text.len()..text.len(),
            options: style,
            families: names,
        });
        paths.insert(id, path);
        let is_break = tree.element(id).is_some_and(|e| e.tag == Tag::Br);
        if is_break {
            whitespace = None;
            previous_cr = false;
            append(&mut text, &mut spans, index, '\n')?;
            continue;
        }
        for raw in source.chars() {
            measurer
                .charge_inline_work(1)
                .map_err(|_| LayoutError::Limit)?;
            let paired = previous_cr && raw == '\n';
            previous_cr = raw == '\r';
            if paired {
                continue;
            }
            let ch = if raw == '\r' { '\n' } else { raw };
            if ch.is_control() && !matches!(ch, '\t' | '\n' | '\u{000c}') {
                return Err(LayoutError::Unsupported(id));
            }
            if style.white_space != WhiteSpaceMode::PreWrap && ch.is_ascii_whitespace() {
                if !text.is_empty() && !text.ends_with('\n') {
                    whitespace.get_or_insert(index);
                }
                continue;
            }
            if let Some(owner) = whitespace.take() {
                append(&mut text, &mut spans, owner, ' ')?;
            }
            append(&mut text, &mut spans, index, ch)?;
        }
    }
    if options.white_space == WhiteSpaceMode::PreWrap
        && (text.contains('\t')
            || text
                .lines()
                .any(|line| line.starts_with(' ') || line.ends_with(' ')))
    {
        return Err(LayoutError::Unsupported(root));
    }
    let strut = if text.is_empty() {
        Vertical::default()
    } else {
        let (_, layout) = measurer
            .shape_styled(" ", None, &families, options)
            .map_err(|e| LayoutError::Text(e.to_string()))?;
        if let Some(run) = layout.lines().next().and_then(|line| line.runs().next()) {
            font_boxes.insert(
                root,
                FontBox {
                    size: run.font_size(),
                    ascent: run.metrics().ascent.round(),
                    descent: run.metrics().descent.round(),
                    vertical: Vertical::run(&run),
                    padding: [PaddingLength::Pixels(0.0); 2],
                },
            );
            Vertical::run(&run)
        } else {
            Vertical::default()
        }
    };
    let mut occupied = BTreeSet::new();
    for span in &spans {
        if !span.range.is_empty() {
            for &id in &paths[&span.owner] {
                measurer
                    .charge_inline_work(1)
                    .map_err(|_| LayoutError::Limit)?;
                occupied.insert(id);
            }
        }
    }
    let mut empty = BTreeMap::<usize, Vec<u32>>::new();
    for (id, position) in anchors {
        if !occupied.contains(&id) {
            empty.entry(position.min(text.len())).or_default().push(id);
        }
    }
    let rich = Arc::new(ContentFlow {
        spans,
        paths,
        nodes,
        needs_paint_order,
        hidden: hidden_nodes,
        strut,
        font_boxes,
        empty,
    });
    let mut shape = TextShape {
        text,
        options,
        families,
        metrics: TextMetrics {
            width: 0.0,
            min_width: 0.0,
            height: 0.0,
            lines: 0,
        },
        rich: Some(rich),
    };
    shape.metrics = shape
        .measure(measurer, None)
        .map_err(|e| LayoutError::Text(e.to_string()))?;
    Ok(Some(shape))
}

fn append(
    text: &mut String,
    spans: &mut [TextSpan],
    index: usize,
    ch: char,
) -> Result<(), LayoutError> {
    if text.len() + ch.len_utf8() > 64 * 1024 {
        return Err(LayoutError::Limit);
    }
    if spans[index].range.is_empty() {
        spans[index].range.start = text.len();
    }
    text.push(ch);
    spans[index].range.end = text.len();
    Ok(())
}

impl ContentFlow {
    pub fn shape(
        &self,
        shape: &TextShape,
        measurer: &mut TextMeasurer,
        width: Option<f32>,
    ) -> Result<ShapedFlow, TextMeasureError> {
        if shape.text.is_empty() {
            return Ok(ShapedFlow {
                layout: Layout::new(),
                offsets: Vec::new(),
                metrics: shape.metrics,
            });
        }
        let (mut metrics, layout) = measurer.shape_with_spans(
            &shape.text,
            width,
            &shape.families,
            shape.options,
            &self.spans,
        )?;
        let mut y = 0.0f32;
        let mut offsets = Vec::new();
        for (index, line) in layout.lines().enumerate() {
            measurer.charge_inline_work(1)?;
            if index + 1 == layout.len()
                && line.text_range().is_empty()
                && shape.text.ends_with('\n')
            {
                offsets.push(0.0);
                continue;
            }
            let mut vertical = self.strut;
            for run in line.runs() {
                measurer.charge_inline_work(1)?;
                let m = Vertical::run(&run);
                vertical.above = vertical.above.max(m.above);
                vertical.below = vertical.below.max(m.below);
                for cluster in run.clusters() {
                    measurer.charge_inline_work(1)?;
                    if let Some(path) = self.paths.get(&cluster.first_style().brush) {
                        for id in path {
                            measurer.charge_inline_work(1)?;
                            if let Some(font) = self.font_boxes.get(id) {
                                vertical.above = vertical.above.max(font.vertical.above);
                                vertical.below = vertical.below.max(font.vertical.below);
                            }
                        }
                    }
                }
            }
            let range = line.text_range();
            let end = range.end + usize::from(range.end == shape.text.len());
            for (_, ids) in self.empty.range(range.start..end) {
                for id in ids {
                    measurer.charge_inline_work(1)?;
                    if let Some(font) = self.font_boxes.get(id) {
                        vertical.above = vertical.above.max(font.vertical.above);
                        vertical.below = vertical.below.max(font.vertical.below);
                    }
                }
            }
            offsets.push(y + vertical.above - line.metrics().baseline);
            y += vertical.above + vertical.below;
            if !y.is_finite() || y > 1_000_000.0 {
                return Err(TextMeasureError::Limit);
            }
        }
        metrics.height = y;
        Ok(ShapedFlow {
            layout,
            offsets,
            metrics,
        })
    }

    pub fn project(
        &self,
        shaped: ShapedFlow,
        source: &TextShape,
        bounds: BoxGeometry,
        measurer: &mut TextMeasurer,
        fragments_left: &mut usize,
    ) -> Result<Frame, LayoutError> {
        let mut fragments = BTreeMap::<u32, Vec<BoxRect>>::new();
        let mut fragment_lines = BTreeMap::<u32, Vec<usize>>::new();
        let mut glyph_owners = Vec::new();
        for (index, line) in shaped.layout.lines().enumerate() {
            let mut line_owners = Vec::new();
            let range = line.text_range();
            // Parley hangs break spaces beyond the line's advance. CSS normal
            // whitespace removes those spaces from inline fragments and hits.
            let trimmed_end = if source.options.white_space == WhiteSpaceMode::PreWrap {
                range.end
            } else {
                range.start + source.text[range].trim_end_matches([' ', '\n']).len()
            };
            let mut x = line.metrics().offset + line.metrics().inline_min_coord;
            for run in line.runs() {
                // Browser fragments assign a shaped cluster to its first source
                // node. Ligature continuation nodes retain a zero-width box;
                // caret subdivision is a separate editing concern.
                let mut groups = BTreeMap::new();
                let mut logical = run.clusters().peekable();
                while let Some(first) = logical.next() {
                    let mut members = vec![first];
                    let mut advance = first.advance();
                    while first.is_ligature_start()
                        && logical.peek().is_some_and(|c| c.is_ligature_continuation())
                    {
                        let next = logical.next().ok_or(LayoutError::Limit)?;
                        advance += next.advance();
                        members.push(next);
                    }
                    let mut offset = 0.0;
                    for (index, member) in members.into_iter().enumerate() {
                        measurer
                            .charge_inline_work(1)
                            .map_err(|_| LayoutError::Limit)?;
                        groups.insert(
                            member.text_range().start,
                            ClusterGroup {
                                owner: first.first_style().brush,
                                advance,
                                offset,
                                primary: index == 0,
                            },
                        );
                        offset += member.advance();
                    }
                }
                for cluster in run.visual_clusters() {
                    measurer
                        .charge_inline_work(1)
                        .map_err(|_| LayoutError::Limit)?;
                    let group = groups
                        .get(&cluster.text_range().start)
                        .ok_or(LayoutError::Limit)?;
                    for _ in cluster.glyphs() {
                        measurer
                            .charge_inline_work(1)
                            .map_err(|_| LayoutError::Limit)?;
                        line_owners.push(group.owner);
                    }
                    let collapsed = cluster.text_range().start >= trimmed_end
                        && (cluster.is_space_or_nbsp() || cluster.is_hard_line_break());
                    let left = if run.is_rtl() {
                        x - (group.advance - group.offset - cluster.advance())
                    } else {
                        x - group.offset
                    };
                    let width = if group.primary && !collapsed {
                        group.advance
                    } else {
                        0.0
                    };
                    let position = if group.primary || run.is_rtl() || collapsed {
                        left
                    } else {
                        left + group.advance
                    };
                    let path = self
                        .paths
                        .get(&cluster.first_style().brush)
                        .ok_or(LayoutError::Limit)?;
                    for &id in path {
                        measurer
                            .charge_inline_work(1)
                            .map_err(|_| LayoutError::Limit)?;
                        let (ascent, descent) = self.font_boxes.get(&id).map_or(
                            (run.metrics().ascent.round(), run.metrics().descent.round()),
                            |font| (font.ascent, font.descent),
                        );
                        let piece = BoxRect {
                            x: bounds.rect.x + position,
                            y: bounds.rect.y + line.metrics().baseline + shaped.offsets[index]
                                - ascent,
                            width,
                            height: ascent + descent,
                        };
                        let piece = if let Some(font) = self.font_boxes.get(&id) {
                            font.padded(piece, bounds.rect.width)?
                        } else {
                            piece
                        };
                        let items = fragments.entry(id).or_default();
                        let lines = fragment_lines.entry(id).or_default();
                        if let Some(last) = items.last_mut().filter(|last| {
                            lines.last() == Some(&index)
                                && last.y == piece.y
                                && last.height == piece.height
                                && (last.x + last.width - piece.x).abs() < 0.001
                        }) {
                            last.width += piece.width;
                        } else {
                            *fragments_left =
                                fragments_left.checked_sub(1).ok_or(LayoutError::Limit)?;
                            items.push(piece);
                            lines.push(index);
                        }
                    }
                    let range = cluster.text_range();
                    let end = range.end + usize::from(range.end == source.text.len());
                    for (&byte, ids) in self.empty.range(range.start..end) {
                        for &id in ids {
                            measurer
                                .charge_inline_work(1)
                                .map_err(|_| LayoutError::Limit)?;
                            let font = self.font_boxes.get(&id).ok_or(LayoutError::Limit)?;
                            let at = if run.is_rtl()
                                || (byte == range.start && group.primary)
                                || collapsed
                            {
                                left
                            } else {
                                left + group.advance
                            };
                            *fragments_left =
                                fragments_left.checked_sub(1).ok_or(LayoutError::Limit)?;
                            let piece = font.padded(
                                BoxRect {
                                    x: bounds.rect.x + at,
                                    y: bounds.rect.y
                                        + line.metrics().baseline
                                        + shaped.offsets[index]
                                        - font.ascent,
                                    width: 0.0,
                                    height: font.ascent + font.descent,
                                },
                                bounds.rect.width,
                            )?;
                            fragments.entry(id).or_default().push(piece);
                            fragment_lines.entry(id).or_default().push(index);
                        }
                    }
                    if !collapsed {
                        x += cluster.advance();
                    }
                }
            }
            glyph_owners.push(line_owners);
        }
        Ok(Frame {
            glyph_owners,
            shaped,
            bounds,
            fragments,
            fragment_lines,
        })
    }
}

pub(crate) fn frames(
    geometry: &mut BTreeMap<u32, BoxGeometry>,
    shapes: &BTreeMap<u32, TextShape>,
    measurer: &mut TextMeasurer,
) -> Result<BTreeMap<u32, Frame>, LayoutError> {
    let mut result = BTreeMap::new();
    let mut fragments_left = MAX_FRAGMENT_COUNT;
    for (&id, shape) in shapes {
        let Some(rich) = &shape.rich else { continue };
        let root = geometry[&id];
        let mut rect = root.content;
        let mut clip = root.clip;
        if let Some(scroll) = root.programmatic_scroll {
            rect.y -= scroll.top;
            clip = layout::intersect(clip, scroll.viewport);
        }
        let bounds = BoxGeometry {
            rect,
            content: rect,
            clip,
            scroll: None,
            programmatic_scroll: None,
        };
        let shaped = rich
            .shape(shape, measurer, Some(rect.width.clamp(0.01, 1_000_000.0)))
            .map_err(|e| LayoutError::Text(e.to_string()))?;
        let frame = rich.project(shaped, shape, bounds, measurer, &mut fragments_left)?;
        for &node in &rich.nodes {
            let pieces = frame.fragments.get(&node);
            let mut union = BoxRect {
                x: rect.x,
                y: rect.y,
                width: 0.0,
                height: 0.0,
            };
            if let Some(pieces) = pieces {
                for (index, &piece) in pieces.iter().enumerate() {
                    if index == 0 {
                        union = piece;
                    } else {
                        let right = (union.x + union.width).max(piece.x + piece.width);
                        let bottom = (union.y + union.height).max(piece.y + piece.height);
                        union.x = union.x.min(piece.x);
                        union.y = union.y.min(piece.y);
                        union.width = right - union.x;
                        union.height = bottom - union.y;
                    }
                }
            }
            if rich.hidden.contains(&node) {
                union = BoxRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                };
            }
            let clip = if rich.hidden.contains(&node) {
                union
            } else {
                clip
            };
            let mut content = union;
            if !rich.hidden.contains(&node)
                && pieces.is_some_and(|pieces| !pieces.is_empty())
                && let Some(font) = rich.font_boxes.get(&node)
            {
                let [top, bottom] = font.padding(rect.width)?;
                content.y += top;
                content.height = (content.height - top - bottom).max(0.0);
            }
            geometry.insert(
                node,
                BoxGeometry {
                    rect: union,
                    content,
                    clip,
                    ..bounds
                },
            );
        }
        result.insert(id, frame);
    }
    Ok(result)
}
