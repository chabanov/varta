//! Bounded rich-text formatting contexts for inline descendants of a box.
mod atoms;
mod ellipsis;
use crate::layout::{self, BoxGeometry, BoxRect, LayoutError, TextShape};
use crate::style::Environment;
use crate::text::{
    TextSpan, WhiteSpaceMode,
    edges::{EdgeKind, TextEdge},
};
use crate::tree::{Content, Tree};
use crate::{TextMeasureError, TextMeasurer, TextMetrics};
use parley::{InlineBoxKind, Layout, LineHeight, LineItem};
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
        Self::with_line_height(run, run.metrics().line_height)
    }
    fn with_line_height(run: &parley::Run<'_, u32>, line_height: f32) -> Self {
        let m = run.metrics();
        let above =
            m.ascent.round() + ((line_height - m.ascent.round() - m.descent.round()) * 0.5).floor();
        Self {
            above,
            below: line_height - above,
        }
    }
    /// A run's vertical extent with the line height of the span that owns
    /// its first cluster. Parley's run metrics take the line height of the
    /// style that ends the run (its item keeps the next style's index when a
    /// font-size change breaks it), which differs once adjacent spans carry
    /// different explicit line heights (control labels).
    fn owned(run: &parley::Run<'_, u32>, line_heights: &BTreeMap<u32, LineHeight>) -> Self {
        let owner = run
            .clusters()
            .next()
            .and_then(|cluster| line_heights.get(&cluster.first_style().brush));
        Self::owned_by(run, owner)
    }
    /// A run's vertical extent with `owner`'s line height.
    fn owned_by(run: &parley::Run<'_, u32>, owner: Option<&LineHeight>) -> Self {
        let m = run.metrics();
        let line_height = match owner {
            Some(LineHeight::Absolute(value)) => *value,
            Some(LineHeight::FontSizeRelative(value)) => value * run.font_size(),
            Some(LineHeight::MetricsRelative(value)) => (m.ascent + m.descent + m.leading) * value,
            None => m.line_height,
        };
        Self::with_line_height(run, line_height)
    }
}

pub(crate) struct ContentFlow {
    spans: Vec<TextSpan>,
    paths: BTreeMap<u32, Vec<u32>>,
    pub nodes: Vec<u32>,
    pub needs_paint_order: bool,
    hidden: BTreeSet<u32>,
    strut: Vertical,
    /// Each text span owner's line height, for [`Vertical::owned`].
    line_heights: BTreeMap<u32, LineHeight>,
    font_boxes: BTreeMap<u32, FontBox>,
    empty: BTreeMap<usize, Vec<u32>>,
    /// Nonzero horizontal decoration edges in source order, and the physical
    /// left/right margin and padding of each owning element.
    edges: Vec<TextEdge>,
    spacing: BTreeMap<u32, Horizontal>,
    /// Inline images of the flow, atomic inlines (`atoms`).
    atoms: BTreeMap<u32, atoms::Atom>,
    /// `text-overflow: ellipsis` truncates the root's single line.
    ellipsis: bool,
}
/// Per-byte white-space preservation of a flow's text nodes.
enum Preserved {
    Uniform(bool),
    Mixed(Vec<bool>),
}
impl Preserved {
    /// Empty spans keep their creation offset, so ranges are filled rather
    /// than searched.
    fn new(text: &str, spans: &[TextSpan], root: WhiteSpaceMode) -> Self {
        let mut modes = spans
            .iter()
            .filter(|span| !span.range.is_empty())
            .map(|span| span.options.white_space.preserves());
        let first = modes.next().unwrap_or(root.preserves());
        if modes.all(|value| value == first) {
            return Self::Uniform(first);
        }
        let mut bytes = vec![root.preserves(); text.len()];
        for span in spans {
            if let Some(target) = bytes.get_mut(span.range.clone()) {
                target.fill(span.options.white_space.preserves());
            }
        }
        Self::Mixed(bytes)
    }
    fn at(&self, byte: usize) -> bool {
        match self {
            Self::Uniform(value) => *value,
            Self::Mixed(bytes) => bytes.get(byte).copied().unwrap_or(false),
        }
    }
}
#[derive(Clone, Copy)]
struct Horizontal {
    margin: [f32; 2],
    border: [f32; 2],
    padding: [f32; 2],
}
impl Horizontal {
    /// The box's own extent on one side: border plus padding.
    fn inset(self, side: usize) -> f32 {
        self.border[side] + self.padding[side]
    }
    fn advance(self, side: usize) -> f32 {
        self.margin[side] + self.inset(side)
    }
    fn nonzero(self, side: usize) -> bool {
        self.margin[side] != 0.0 || self.border[side] != 0.0 || self.padding[side] != 0.0
    }
}
/// One traversal step: a node to open or the end of an element's children.
enum Visit {
    Open {
        id: u32,
        depth: usize,
        path: Vec<u32>,
        hidden: bool,
    },
    Close(u32),
}
impl ContentFlow {
    pub(crate) fn decoration_metrics(&self, id: u32) -> Option<(f32, f32)> {
        self.font_boxes.get(&id).map(|f| (f.size, f.ascent))
    }
    pub(crate) fn hidden(&self, id: u32) -> bool {
        self.hidden.contains(&id)
    }
    /// Whether `id` is an atomic inline of the flow, painted as its own box.
    pub(crate) fn atom(&self, id: u32) -> bool {
        self.atoms.contains_key(&id)
    }
}
pub(crate) struct ShapedFlow {
    pub layout: Layout<u32>,
    pub offsets: Vec<f32>,
    /// Each line box's top and bottom, relative to the flow's content top.
    pub line_boxes: Vec<[f32; 2]>,
    /// How far each atomic inline's margin box bottom lies below its line's
    /// baseline.
    pub atoms: BTreeMap<u32, f32>,
    pub metrics: TextMetrics,
}
#[derive(Clone, Copy)]
enum PaddingLength {
    Pixels(f32),
    Fraction(f32),
    /// A layout-time `calc()` by its table key ([`crate::calc::key`]).
    Calc(usize),
}
impl From<LengthPercentage> for PaddingLength {
    fn from(value: LengthPercentage) -> Self {
        match value.expand() {
            ExpandedLengthPercentage::Length(value) => Self::Pixels(value),
            ExpandedLengthPercentage::Percent(value) => Self::Fraction(value),
            ExpandedLengthPercentage::Calc(handle) => Self::Calc(crate::calc::key(handle)),
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
    /// Top and bottom border widths; like padding they paint outside the line.
    border: [f32; 2],
}
impl FontBox {
    /// Top/bottom padding plus border: the border box's extent beyond the font box.
    fn inset(self, width: f32) -> Result<[f32; 2], LayoutError> {
        let mut inset = [0.0; 2];
        for ((out, value), border) in inset.iter_mut().zip(self.padding).zip(self.border) {
            *out = match value {
                PaddingLength::Pixels(value) => value,
                PaddingLength::Fraction(value) => value * width,
                PaddingLength::Calc(key) => crate::calc::resolve_key(key, width),
            } + border;
            if !out.is_finite() || !(0.0..=crate::units::MAX_LENGTH).contains(out) {
                return Err(LayoutError::Limit);
            }
        }
        Ok(inset)
    }
    fn padded(self, mut piece: BoxRect, width: f32) -> Result<BoxRect, LayoutError> {
        let padding = self.inset(width)?;
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
    /// The flow's laid out images, each painted as its own atomic box after
    /// the flow's text, in tree order.
    pub atoms: Vec<u32>,
}

/// The strut of `id`'s line boxes: the distances its first available font
/// reaches above and below the baseline with half of its line height's
/// leading on each side, as a rich flow's strut.
pub(crate) fn strut(
    tree: &Tree,
    env: &Environment,
    id: u32,
    font: (&mut TextMeasurer, f32),
) -> Result<(f32, f32), LayoutError> {
    let (measurer, size) = font;
    let (options, families) = layout::text_style(tree, env, id, size, measurer)?;
    let options = layout::controls::control_line_height(tree, id, measurer, &families, options)?;
    let (_, layout) = measurer
        .shape_styled(" ", None, &families, options)
        .map_err(LayoutError::text(id))?;
    let run = layout
        .lines()
        .next()
        .and_then(|line| line.runs().next())
        .ok_or(LayoutError::Unsupported(id))?;
    // The element's own line height, as `Vertical::owned` takes a span's:
    // a lone space's run metrics need not carry it.
    let vertical = Vertical::owned_by(&run, Some(&options.line_height.unwrap_or_default()));
    Ok((vertical.above, vertical.below))
}

pub(crate) fn inline_tag(tag: Tag) -> bool {
    matches!(
        tag,
        Tag::Span | Tag::Strong | Tag::Em | Tag::Code | Tag::A | Tag::Br | Tag::Label
    )
}
/// Whether an inline-level box's display is blockified (CSS Display 3
/// §2.7): a flex or grid item, or an absolutely positioned box (CSS 2
/// §9.7).
pub(crate) fn blockified(tree: &Tree, env: &Environment, id: u32) -> bool {
    out_of_flow(tree, env, id)
        || tree.nodes[&id].parent.is_some_and(|parent| {
            tree.computed_property(parent, env, P::Display)
                .is_some_and(|d| matches!(d.as_str(), "flex" | "grid"))
        })
}
/// Whether `id` is absolutely positioned (`absolute` or `fixed`).
fn out_of_flow(tree: &Tree, env: &Environment, id: u32) -> bool {
    tree.computed_property(id, env, P::Position)
        .is_some_and(|position| matches!(position.as_str(), "absolute" | "fixed"))
}
fn is_inline(tree: &Tree, env: &Environment, id: u32) -> bool {
    match &tree.nodes[&id].content {
        Content::Text(_) => true,
        // An image that does not form its own line is an atomic inline.
        Content::Element(e) if e.tag == Tag::Img => {
            layout::replaced::inline(tree, env, id) && !layout::replaced::alone(tree, env, id)
        }
        // An absolutely positioned box is blockified and out of the line.
        Content::Element(_) if out_of_flow(tree, env, id) => false,
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

fn validate(tree: &Tree, env: &Environment, id: u32) -> Result<layout::InlineSpacing, LayoutError> {
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
            | P::TextAlign
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
            | P::BorderColor
            | P::BorderTopWidth
            | P::BorderRightWidth
            | P::BorderBottomWidth
            | P::BorderLeftWidth
            | P::BorderTopStyle
            | P::BorderRightStyle
            | P::BorderBottomStyle
            | P::BorderLeftStyle
            | P::BorderTopColor
            | P::BorderRightColor
            | P::BorderBottomColor
            | P::BorderLeftColor => true, // Widths are resolved with spacing below.
            P::BorderTopLeftRadius
            | P::BorderTopRightRadius
            | P::BorderBottomRightRadius
            | P::BorderBottomLeftRadius => true,
            // Inherited by its text (`layout::text_transform`), its glyphs
            // and boxes (`style::visible`, `style::pointer_target`).
            P::TextTransform | P::Visibility | P::PointerEvents | P::UserSelect => true,
            // A non-replaced inline box has no `width` or `height` to size.
            P::BoxSizing => true,
            // A generated box's content (`crate::generated`); an element's
            // own content replacement is not rendered.
            P::Content => {
                tree.generated(id).is_some() || matches!(value.as_str(), "normal" | "none")
            }
            // Inherited by text (`layout::resolve_text`) and list items.
            P::FontFeatureSettings
            | P::FontVariationSettings
            | P::ListStyleType
            | P::ListStylePosition
            | P::ListStyleImage => true,
            // `appearance` has no effect on a non-replaced inline box.
            P::Appearance => true,
            P::Outline | P::OutlineWidth | P::OutlineStyle | P::OutlineColor | P::OutlineOffset => {
                true
            }
            _ => false,
        };
        if !supported {
            return Err(LayoutError::Unsupported(id));
        }
    }
    let spacing = layout::inline_spacing(tree, env, id)?;
    // A line break has no inline box to decorate; refuse rather than ignore.
    if tree.element(id).is_some_and(|e| e.tag == Tag::Br)
        && spacing
            .margin
            .iter()
            .chain(&spacing.padding)
            .chain(&spacing.border)
            .any(|&v| v != 0.0)
    {
        return Err(LayoutError::Unsupported(id));
    }
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
    Ok(spacing)
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
    let ellipsis = ellipsis::requested(tree, env, root)?;
    let needed = root == ROOT_NODE
        || ellipsis
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
    // Control labels use Chrome's whole-pixel line spacing in rich flows too.
    let options = layout::controls::control_line_height(tree, root, measurer, &families, options)?;
    let mut text = String::new();
    let mut spans = Vec::<TextSpan>::new();
    // Each span's `text-transform` (CSS Text 3 §2.1), applied to its
    // characters after white-space collapsing.
    let mut transforms = Vec::<crate::text::transform::TextTransform>::new();
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
        .map(|&id| Visit::Open {
            id,
            depth: depth + 1,
            path: Vec::new(),
            hidden: false,
        })
        .collect();
    let mut whitespace = None;
    let mut hidden_nodes = BTreeSet::new();
    let mut previous_cr = false;
    let mut edges = Vec::<TextEdge>::new();
    let mut spacing = BTreeMap::<u32, Horizontal>::new();
    let mut atoms = BTreeMap::new();
    // The text length at the last atomic inline: a collapsible space right
    // after it is content, even at the start of a line.
    let mut atom_at = None;
    // Edges recorded while a collapsible space is pending follow that space
    // only if it is emitted; a forced break or the end discards it instead.
    let mut deferred = Vec::<usize>::new();
    while let Some(visit) = pending.pop() {
        let (id, depth, mut path, hidden) = match visit {
            Visit::Open {
                id,
                depth,
                path,
                hidden,
            } => (id, depth, path, hidden),
            Visit::Close(id) => {
                let space = spacing.get(&id).copied().ok_or(LayoutError::Limit)?;
                edge(
                    &mut edges,
                    &mut deferred,
                    &whitespace,
                    &text,
                    id,
                    false,
                    space,
                )?;
                continue;
            }
        };
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
            pending.extend(node.children.iter().rev().map(|&child| Visit::Open {
                id: child,
                depth: depth + 1,
                path: Vec::new(),
                hidden: true,
            }));
            continue;
        }
        path.insert(0, id);
        let source = match &node.content {
            Content::Element(e) if e.tag == Tag::Img => {
                if !is_inline(tree, env, id)
                    || path[1..].iter().any(|&ancestor| {
                        tree.computed_property(ancestor, env, P::Opacity)
                            .is_some_and(|raw| raw.parse::<f32>().ok() != Some(1.0))
                    })
                {
                    return Err(LayoutError::Unsupported(id));
                }
                let atom = atoms::measure(tree, env, id, (measurer, size))?;
                // A collapsible space before an atomic inline is content.
                if let Some(owner) = whitespace.take() {
                    append_mapped(&mut text, &mut spans, (owner, transforms[owner]), ' ', "")?;
                    for edge in deferred.drain(..) {
                        edges[edge].index = text.len();
                    }
                }
                if edges.len() >= crate::text::edges::MAX_EDGES {
                    return Err(LayoutError::Limit);
                }
                edges.push(TextEdge {
                    owner: id,
                    index: text.len(),
                    kind: EdgeKind::Atom,
                    advance: atom.advance(),
                });
                atom_at = Some(text.len());
                previous_cr = false;
                paths.insert(id, path);
                atoms.insert(id, atom);
                continue;
            }
            Content::Element(e) => {
                if !is_inline(tree, env, id) {
                    return Err(LayoutError::Unsupported(id));
                }
                let resolved = validate(tree, env, id)?;
                let padding = resolved.vertical;
                needs_paint_order |= resolved.border.iter().any(|&width| width != 0.0);
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
                let style =
                    layout::controls::control_line_height(tree, id, measurer, &names, style)?;
                let (_, sample) = measurer
                    .shape_styled(" ", None, &names, style)
                    .map_err(LayoutError::text(id))?;
                if let Some(run) = sample.lines().next().and_then(|line| line.runs().next()) {
                    font_boxes.insert(
                        id,
                        FontBox {
                            size: run.font_size(),
                            ascent: run.metrics().ascent.round(),
                            descent: run.metrics().descent.round(),
                            vertical: Vertical::run(&run),
                            padding: padding.map(PaddingLength::from),
                            border: [resolved.border[0], resolved.border[2]],
                        },
                    );
                }
                if e.tag != Tag::Br {
                    measurer
                        .charge_inline_work(node.children.len().saturating_mul(path.len() + 1))
                        .map_err(|_| LayoutError::Limit)?;
                    let space = Horizontal {
                        margin: resolved.margin,
                        border: [resolved.border[3], resolved.border[1]],
                        padding: resolved.padding,
                    };
                    if space.nonzero(0) || space.nonzero(1) {
                        needs_paint_order = true;
                        spacing.insert(id, space);
                        paths.insert(id, path.clone());
                        edge(
                            &mut edges,
                            &mut deferred,
                            &whitespace,
                            &text,
                            id,
                            true,
                            space,
                        )?;
                        pending.push(Visit::Close(id));
                    }
                    pending.extend(node.children.iter().rev().map(|&child| Visit::Open {
                        id: child,
                        depth: depth + 1,
                        path: path.clone(),
                        hidden: false,
                    }));
                    continue;
                }
                "\n"
            }
            Content::Text(value) => value,
        };
        inventory.text(source.len())?;
        // Each text node keeps its own white-space mode: collapsing and
        // preserving modes mix in one flow, as CSS Text 3 defines.
        let (style, names) = layout::text_style(tree, env, id, size, measurer)?;
        let style = layout::controls::control_line_height(tree, id, measurer, &names, style)?;
        transforms.push(layout::text_transform(tree, env, id)?);
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
            deferred.clear();
            previous_cr = false;
            append(&mut text, &mut spans, index, '\n')?;
            continue;
        }
        for (at, raw) in source.char_indices() {
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
            if !style.white_space.preserves() && ch.is_ascii_whitespace() {
                if (!text.is_empty() && !text.ends_with('\n')) || atom_at == Some(text.len()) {
                    whitespace.get_or_insert(index);
                }
                continue;
            }
            if let Some(owner) = whitespace.take() {
                append_mapped(&mut text, &mut spans, (owner, transforms[owner]), ' ', "")?;
                for edge in deferred.drain(..) {
                    edges[edge].index = text.len();
                }
            }
            let after = &source[at + raw.len_utf8()..];
            append_mapped(&mut text, &mut spans, (index, transforms[index]), ch, after)?;
        }
    }
    // Every tab left after collapsing is preserved.
    if !crate::text::whitespace::tabs_admitted(&text) {
        return Err(LayoutError::Unsupported(root));
    }
    let strut = if text.is_empty() && edges.is_empty() {
        Vertical::default()
    } else {
        let (_, layout) = measurer
            .shape_styled(" ", None, &families, options)
            .map_err(LayoutError::text(root))?;
        if let Some(run) = layout.lines().next().and_then(|line| line.runs().next()) {
            font_boxes.insert(
                root,
                FontBox {
                    size: run.font_size(),
                    ascent: run.metrics().ascent.round(),
                    descent: run.metrics().descent.round(),
                    vertical: Vertical::run(&run),
                    padding: [PaddingLength::Pixels(0.0); 2],
                    border: [0.0; 2],
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
    let line_heights = spans
        .iter()
        .map(|span| (span.owner, span.options.line_height.unwrap_or_default()))
        .collect();
    let rich = Arc::new(ContentFlow {
        line_heights,
        spans,
        paths,
        nodes,
        needs_paint_order,
        hidden: hidden_nodes,
        strut,
        font_boxes,
        empty,
        edges,
        spacing,
        atoms,
        ellipsis,
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
    if let Some(rich) = &shape.rich
        && let Some(first) = rich.edges.first()
    {
        let shaped = rich
            .shape(&shape, measurer, None)
            .map_err(LayoutError::text(root))?;
        // Decoration edges are placed in logical order. Bidi reordering of
        // a decorated box's fragments is not admitted yet.
        if shaped
            .layout
            .lines()
            .flat_map(|line| line.runs())
            .any(|run| run.is_rtl())
        {
            return Err(LayoutError::Unsupported(first.owner));
        }
        shape.metrics = shaped.metrics;
    } else if let Some(rich) = &shape.rich {
        let shaped = rich
            .shape(&shape, measurer, None)
            .map_err(LayoutError::text(root))?;
        if !layout::alignment_admitted(&shaped.layout, shape.options) {
            return Err(LayoutError::Unsupported(root));
        }
        shape.metrics = shaped.metrics;
    }
    ellipsis::admitted(root, &shape)?;
    if ellipsis
        && shape
            .rich
            .as_ref()
            .is_some_and(|rich| !rich.atoms.is_empty())
    {
        return Err(LayoutError::Unsupported(root));
    }
    // Truncation measures the untruncated line by Parley's trailing white
    // space, not by the native white-space model's extents.
    if ellipsis
        && let Some(rich) = &shape.rich
        && crate::text::whitespace::native(&shape.text, shape.options, &rich.spans)
    {
        return Err(LayoutError::Unsupported(root));
    }
    Ok(Some(shape))
}

/// Records one nonzero opening or closing decoration edge of `owner`.
fn edge(
    edges: &mut Vec<TextEdge>,
    deferred: &mut Vec<usize>,
    whitespace: &Option<usize>,
    text: &str,
    owner: u32,
    opening: bool,
    space: Horizontal,
) -> Result<(), LayoutError> {
    let side = usize::from(!opening);
    // CSS shaping continues across a boundary whose margin, border and
    // padding are all zero; such an edge must not split ligatures/joining.
    if !space.nonzero(side) {
        return Ok(());
    }
    let advance = space.advance(side);
    if edges.len() >= crate::text::edges::MAX_EDGES {
        return Err(LayoutError::Limit);
    }
    if whitespace.is_some() {
        deferred.push(edges.len());
    }
    edges.push(TextEdge {
        owner,
        index: text.len(),
        kind: if opening {
            EdgeKind::Opening
        } else {
            EdgeKind::Closing
        },
        advance,
    });
    Ok(())
}

/// Preserved white space the native line model reproduces: tab stops,
/// hanging and conditionally hanging spaces and `pre` trailing spaces are
/// modeled by `text::whitespace`. Only the number of preserved tabs, each
/// shaped as its own run, is bounded.
pub(crate) fn preserved_text_admitted(text: &str, mode: WhiteSpaceMode) -> bool {
    !mode.preserves() || crate::text::whitespace::tabs_admitted(text)
}

/// Append `ch` with its span's text transform (`after` is the rest of its
/// text node, the transform's context).
fn append_mapped(
    text: &mut String,
    spans: &mut [TextSpan],
    (index, transform): (usize, crate::text::transform::TextTransform),
    ch: char,
    after: &str,
) -> Result<(), LayoutError> {
    if transform.is_none() {
        return append(text, spans, index, ch);
    }
    for mapped in transform.map(ch, text, after).chars() {
        append(text, spans, index, mapped)?;
    }
    Ok(())
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
        if shape.text.is_empty() && self.edges.is_empty() {
            return Ok(ShapedFlow {
                layout: Layout::new(),
                offsets: Vec::new(),
                line_boxes: Vec::new(),
                atoms: BTreeMap::new(),
                metrics: shape.metrics,
            });
        }
        let (mut metrics, layout) = measurer.shape_with_edges(
            &shape.text,
            width,
            &shape.families,
            shape.options,
            &self.spans,
            &self.edges,
        )?;
        let mut y = 0.0f32;
        let mut offsets = Vec::new();
        let mut line_boxes = Vec::new();
        let mut placed = BTreeMap::new();
        for (index, line) in layout.lines().enumerate() {
            measurer.charge_inline_work(1)?;
            if index + 1 == layout.len()
                && line.text_range().is_empty()
                && shape.text.ends_with('\n')
            {
                offsets.push(0.0);
                line_boxes.push([y, y]);
                continue;
            }
            let mut vertical = self.strut;
            for run in line.runs() {
                measurer.charge_inline_work(1)?;
                let m = Vertical::owned(&run, &self.line_heights);
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
            let line_atoms = self.line_atoms(&line, measurer)?;
            for atom in &line_atoms {
                if let atoms::Align::Bottom(bottom) = atom.1.align {
                    vertical.above = vertical.above.max(atom.1.height() - bottom);
                    vertical.below = vertical.below.max(bottom);
                }
            }
            // Line-relative images extend the line box only when taller than
            // everything else in it (CSS 2.1 §10.8.1 `top`/`bottom`).
            for (_, atom) in &line_atoms {
                let excess = atom.height() - (vertical.above + vertical.below);
                match atom.align {
                    atoms::Align::LineTop if excess > 0.0 => vertical.below += excess,
                    atoms::Align::LineBottom if excess > 0.0 => vertical.above += excess,
                    _ => {}
                }
            }
            for (id, atom) in line_atoms {
                placed.insert(id, atom.bottom((vertical.above, vertical.below)));
            }
            offsets.push(y + vertical.above - line.metrics().baseline);
            line_boxes.push([y, y + vertical.above + vertical.below]);
            y += vertical.above + vertical.below;
            if !y.is_finite() || y > 1_000_000.0 {
                return Err(TextMeasureError::Limit);
            }
        }
        metrics.height = y;
        Ok(ShapedFlow {
            layout,
            offsets,
            line_boxes,
            atoms: placed,
            metrics,
        })
    }

    /// The atomic inlines on `line`.
    fn line_atoms(
        &self,
        line: &parley::Line<'_, u32>,
        measurer: &mut TextMeasurer,
    ) -> Result<Vec<(u32, atoms::Atom)>, TextMeasureError> {
        let mut found = Vec::new();
        if self.atoms.is_empty() {
            return Ok(found);
        }
        for item in line.items_unpositioned() {
            measurer.charge_inline_work(1)?;
            if let LineItem::InlineBox(atom) = item
                && atom.kind == InlineBoxKind::InFlow
            {
                let id = u32::try_from(atom.id).map_err(|_| TextMeasureError::Limit)?;
                let atom = self.atoms.get(&id).ok_or(TextMeasureError::Limit)?;
                found.push((id, *atom));
            }
        }
        Ok(found)
    }

    pub fn project(
        &self,
        shaped: ShapedFlow,
        source: &TextShape,
        bounds: BoxGeometry,
        measurer: &mut TextMeasurer,
        fragments_left: &mut usize,
    ) -> Result<Frame, LayoutError> {
        let mut sink = Fragments {
            fragments: BTreeMap::new(),
            lines: BTreeMap::new(),
            left: fragments_left,
        };
        let mut glyph_owners = Vec::new();
        let preserved = Preserved::new(&source.text, &self.spans, source.options.white_space);
        if matches!(preserved, Preserved::Mixed(_)) {
            measurer
                .charge_inline_work(self.spans.len())
                .map_err(|_| LayoutError::Limit)?;
        }
        for (index, line) in shaped.layout.lines().enumerate() {
            let mut line_owners = Vec::new();
            let range = line.text_range();
            // Parley hangs break spaces beyond the line's advance. CSS removes
            // collapsible spaces at a line end from inline fragments and hits;
            // preserved spaces keep their (hanging) advance. The scan stays
            // within the line's clusters, which are charged below.
            let mut trimmed_end = range.end;
            for (offset, ch) in source.text[range.clone()].char_indices().rev() {
                let byte = range.start + offset;
                if !matches!(ch, ' ' | '\n') || preserved.at(byte) {
                    break;
                }
                trimmed_end = byte;
            }
            let mut x = line.metrics().offset + line.metrics().inline_min_coord;
            let top = bounds.rect.y + line.metrics().baseline + shaped.offsets[index];
            for item in line.items_unpositioned() {
                let run = match item {
                    LineItem::Run(run) => run,
                    LineItem::InlineBox(edge) => {
                        let owner = u32::try_from(edge.id).map_err(|_| LayoutError::Limit)?;
                        let opening = match edge.kind {
                            InlineBoxKind::InlineStart => true,
                            InlineBoxKind::InlineEnd => false,
                            InlineBoxKind::InFlow => {
                                let bottom = *shaped.atoms.get(&owner).ok_or(LayoutError::Limit)?;
                                self.project_atom(
                                    owner,
                                    [bounds.rect.x + x, top, bottom],
                                    (index, bounds.rect.width),
                                    measurer,
                                    &mut sink,
                                )?;
                                x += edge.width;
                                continue;
                            }
                            _ => return Err(LayoutError::Limit),
                        };
                        self.edge(
                            owner,
                            opening,
                            [bounds.rect.x + x, top],
                            (index, bounds.rect.width),
                            measurer,
                            &mut sink,
                        )?;
                        x += edge.width;
                        continue;
                    }
                };
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
                            y: top - ascent,
                            width,
                            height: ascent + descent,
                        };
                        let piece = if let Some(font) = self.font_boxes.get(&id) {
                            font.padded(piece, bounds.rect.width)?
                        } else {
                            piece
                        };
                        sink.push(id, index, piece)?;
                    }
                    let range = cluster.text_range();
                    let end = range.end + usize::from(range.end == source.text.len());
                    for (&byte, ids) in self.empty.range(range.start..end) {
                        // Decorated boxes are placed by their own edges.
                        for &id in ids.iter().filter(|id| !self.spacing.contains_key(id)) {
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
                            let piece = font.padded(
                                BoxRect {
                                    x: bounds.rect.x + at,
                                    y: top - font.ascent,
                                    width: 0.0,
                                    height: font.ascent + font.descent,
                                },
                                bounds.rect.width,
                            )?;
                            sink.separate(id, index, piece)?;
                        }
                    }
                    if !collapsed {
                        x += cluster.advance();
                    }
                }
            }
            glyph_owners.push(line_owners);
        }
        let Fragments {
            mut fragments,
            lines: fragment_lines,
            ..
        } = sink;
        // A negative margin can pull later content before an ancestor
        // fragment's start; its own inline size does not become negative.
        for piece in fragments.values_mut().flatten() {
            piece.width = piece.width.max(0.0);
        }
        Ok(Frame {
            glyph_owners,
            shaped,
            bounds,
            fragments,
            fragment_lines,
            atoms: self
                .edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::Atom)
                .map(|edge| edge.owner)
                .collect(),
        })
    }

    /// Projects one atomic inline whose margin box starts at `x` on the line
    /// with baseline `baseline`, its bottom `bottom` below it. The image
    /// receives its border box; enclosing inline boxes receive its advance
    /// at their own font box.
    fn project_atom(
        &self,
        owner: u32,
        origin: [f32; 3],
        line: (usize, f32),
        measurer: &mut TextMeasurer,
        sink: &mut Fragments<'_>,
    ) -> Result<(), LayoutError> {
        let (index, containing_width) = line;
        let [x, baseline, bottom] = origin;
        let atom = self.atoms.get(&owner).ok_or(LayoutError::Limit)?;
        sink.separate(owner, index, atom.border_box(x, baseline, bottom))?;
        let path = self.paths.get(&owner).ok_or(LayoutError::Limit)?;
        for &id in path.iter().skip(1) {
            measurer
                .charge_inline_work(1)
                .map_err(|_| LayoutError::Limit)?;
            let font = self.font_boxes.get(&id).ok_or(LayoutError::Limit)?;
            let piece = font.padded(
                BoxRect {
                    x,
                    y: baseline - font.ascent,
                    width: atom.advance(),
                    height: font.ascent + font.descent,
                },
                containing_width,
            )?;
            sink.push(id, index, piece)?;
        }
        Ok(())
    }

    /// Projects one decoration edge at absolute `[x, line top]`. The owner
    /// receives its padding; enclosing boxes receive the whole margin box.
    fn edge(
        &self,
        owner: u32,
        opening: bool,
        origin: [f32; 2],
        line: (usize, f32),
        measurer: &mut TextMeasurer,
        sink: &mut Fragments<'_>,
    ) -> Result<(), LayoutError> {
        let (index, containing_width) = line;
        let space = self.spacing.get(&owner).ok_or(LayoutError::Limit)?;
        let side = usize::from(!opening);
        let [x, top] = origin;
        let path = self.paths.get(&owner).ok_or(LayoutError::Limit)?;
        for &id in path {
            measurer
                .charge_inline_work(1)
                .map_err(|_| LayoutError::Limit)?;
            let font = self.font_boxes.get(&id).ok_or(LayoutError::Limit)?;
            let (left, width) = if id != owner {
                (x, space.advance(side))
            } else if opening {
                (x + space.margin[0], space.inset(0))
            } else {
                (x, space.inset(1))
            };
            let piece = font.padded(
                BoxRect {
                    x: left,
                    y: top - font.ascent,
                    width,
                    height: font.ascent + font.descent,
                },
                containing_width,
            )?;
            if id == owner && opening {
                // The first fragment starts after the opening margin, not
                // where the preceding content of the line ended.
                sink.separate(id, index, piece)?;
            } else {
                sink.push(id, index, piece)?;
            }
        }
        Ok(())
    }
}

/// Inline fragments under construction, with the shared fragment allowance.
struct Fragments<'a> {
    fragments: BTreeMap<u32, Vec<BoxRect>>,
    lines: BTreeMap<u32, Vec<usize>>,
    left: &'a mut usize,
}
impl Fragments<'_> {
    /// Extends the box's adjacent fragment on this line or starts a new one.
    /// Widths are signed until projection ends so negative margins shorten
    /// an enclosing fragment exactly as they move later content.
    fn push(&mut self, id: u32, line: usize, piece: BoxRect) -> Result<(), LayoutError> {
        let items = self.fragments.entry(id).or_default();
        let lines = self.lines.entry(id).or_default();
        if let Some(last) = items.last_mut().filter(|last| {
            lines.last() == Some(&line)
                && last.y == piece.y
                && last.height == piece.height
                && (last.x + last.width - piece.x).abs() < 0.001
        }) {
            last.width += piece.width;
            return Ok(());
        }
        self.separate(id, line, piece)
    }
    fn separate(&mut self, id: u32, line: usize, piece: BoxRect) -> Result<(), LayoutError> {
        *self.left = self.left.checked_sub(1).ok_or(LayoutError::Limit)?;
        self.fragments.entry(id).or_default().push(piece);
        self.lines.entry(id).or_default().push(line);
        Ok(())
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
            scrollbars: None,
            transform: None,
        };
        let shaped = rich
            .shape(shape, measurer, Some(rect.width.clamp(0.01, 1_000_000.0)))
            .map_err(LayoutError::text(id))?;
        let mut frame = rich.project(shaped, shape, bounds, measurer, &mut fragments_left)?;
        if rich.ellipsis
            && let Some((flow, visible, shaped)) =
                ellipsis::truncate(rich, shape, &frame.shaped, rect.width, measurer, id)?
        {
            // Pointer fragments and geometry stay untruncated; glyphs do not.
            let projected =
                flow.project(shaped, &visible, bounds, measurer, &mut fragments_left)?;
            frame.shaped = projected.shaped;
            frame.glyph_owners = projected.glyph_owners;
        }
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
            if let Some(atom) = rich.atoms.get(&node)
                && pieces.is_some_and(|pieces| !pieces.is_empty())
            {
                content = atom.content(union);
            }
            if !rich.hidden.contains(&node)
                && pieces.is_some_and(|pieces| !pieces.is_empty())
                && let Some(font) = rich.font_boxes.get(&node)
            {
                let [top, bottom] = font.inset(rect.width)?;
                content.y += top;
                content.height = (content.height - top - bottom).max(0.0);
                // An unbroken box has one physical padding box; a broken box's
                // union has no single content edge, so only the vertical inset
                // applies to it.
                if let Some(space) = rich.spacing.get(&node)
                    && pieces.is_some_and(|pieces| pieces.len() == 1)
                {
                    content.x += space.inset(0);
                    content.width = (content.width - space.inset(0) - space.inset(1)).max(0.0);
                }
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

#[cfg(test)]
mod tests {
    use crate::{Access, BatchWriter, Broker, TextFont, TextMeasurer, View};
    use varta_common::view::{CssProperty as P, Tag};

    struct Deny;
    impl Broker for Deny {
        fn link_requested(&self, _: &str) -> bool {
            false
        }
        fn open_url(&mut self, _: &str) -> Access {
            Access::Denied
        }
        fn clipboard_write(&mut self) -> Access {
            Access::Denied
        }
    }

    const FAMILY: &str = "\"Varta Controls Fixture\"";

    fn boxes(
        fill: impl FnOnce(&mut BatchWriter),
    ) -> std::collections::BTreeMap<u32, crate::BoxRect> {
        let mut b = BatchWriter::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::FontFamily, FAMILY);
        fill(&mut b);
        let mut view = View::default();
        view.commit(&b.finish(), false, &mut Deny).unwrap();
        let mut font = TextMeasurer::new_with_fonts(vec![TextFont {
            data: include_bytes!("../tests/fixtures/controls.ttf").to_vec(),
            family: "Varta Controls Fixture".into(),
            aliases: Vec::new(),
        }])
        .unwrap();
        view.layout_boxes_with_text(400.0, 200.0, &mut font, 16.0)
            .unwrap()
    }

    /// A run takes the line height of the span owning it, not of the span
    /// whose font-size change ends it (parley reports the latter). Chrome
    /// 156 line heights and span offsets for `A<span>B</span>C` with the
    /// root's and the span's font-size/line-height; parley's own run line
    /// heights give 44, 44, 53, 66, 66, 39 and 39.
    #[test]
    fn runs_take_their_owning_span_line_height() {
        for ((root_size, root_line), (span_size, span_line), height, offset) in [
            (("20px", "20px"), ("8px", "40px"), 40.0, 16.0),
            (("8px", "40px"), ("20px", "20px"), 40.0, 6.0),
            (("20px", "10px"), ("30px", "50px"), 50.0, 10.0),
            (("30px", "10px"), ("10px", "60px"), 60.0, 25.0),
            (("10px", "60px"), ("30px", "10px"), 60.0, 9.0),
            (("40px", "4px"), ("10px", "30px"), 30.0, 10.0),
            (("10px", "30px"), ("40px", "4px"), 30.0, -14.0),
        ] {
            let boxes = boxes(|b| {
                b.set_style(2, P::FontSize, root_size)
                    .set_style(2, P::LineHeight, root_line)
                    .create_text(3, "A")
                    .insert(2, 3, 0)
                    .create_element(4, Tag::Span)
                    .set_style(4, P::FontSize, span_size)
                    .set_style(4, P::LineHeight, span_line)
                    .insert(2, 4, 0)
                    .create_text(5, "B")
                    .insert(4, 5, 0)
                    .create_text(6, "C")
                    .insert(2, 6, 0);
            });
            assert_eq!(
                (boxes[&2].height, boxes[&4].y - boxes[&2].y),
                (height, offset),
                "{root_size}/{root_line} {span_size}/{span_line}"
            );
        }
    }

    /// A rich button label keeps the control's whole-pixel line spacing in
    /// its text and inline elements: Chrome 156 gives 20px for `A <span>B`
    /// and 23px with a 17px span.
    #[test]
    fn rich_control_labels_keep_whole_pixel_line_spacing() {
        let boxes = boxes(|b| {
            for (button, span, size, text) in [
                (3, 5, None, ["A ", "B"]),
                (8, 10, Some("17px"), ["x ", "AB"]),
            ] {
                b.create_element(button, Tag::Button)
                    .set_style(button, P::Display, "block")
                    .set_style(button, P::Width, "60px")
                    .insert(2, button, 0)
                    .create_text(button + 1, text[0])
                    .insert(button, button + 1, 0)
                    .create_element(span, Tag::Span)
                    .insert(button, span, 0)
                    .create_text(span + 1, text[1])
                    .insert(span, span + 1, 0);
                if let Some(size) = size {
                    b.set_style(span, P::FontSize, size);
                }
            }
        });
        assert_eq!(boxes[&3].height, 20.0);
        assert_eq!(boxes[&8].height, 23.0);
    }
}
