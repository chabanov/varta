//! Outside list markers of `li` items with the user-agent `list-item` display.
//!
//! Marker styles, ordinals and geometry follow Chrome: `disc`, `circle` and
//! `square` are painted shapes sized from the item's rounded font ascent in
//! integer CSS pixels; `decimal` is the text `N. ` in the item's font, ending at
//! the item's content edge. The marker's baseline is the first line's baseline
//! found in the item's content. Markers never change layout geometry.
use crate::TextMeasurer;
use crate::inline::Frame;
use crate::layout::{BoxGeometry, BoxRect, LayoutError, TextShape};
use crate::style::{Environment, ListKind};
use crate::text::{TextAlign, WhiteSpaceMode};
use crate::tree::{Content, Tree};
use parley::Layout;
use std::collections::BTreeMap;
use varta_common::view::{CssProperty, Tag};

/// Blink's padding between a symbol marker and the item content (`kCMarkerPaddingPx`).
const SYMBOL_PADDING_PX: i32 = 7;
/// A symbol is inset by one pixel inside its marker box, which is two pixels
/// wider than the symbol.
const SYMBOL_INSET_PX: i32 = 1;
/// The decimal marker suffix of the `decimal` counter style.
const DECIMAL_SUFFIX: &str = ". ";
/// Nested `ul` depths: `ul` alone is a disc, inside one list a circle and
/// inside two or more lists a square (`ul ul`, `ol ul ul`, …).
const CIRCLE_DEPTH: usize = 1;
const SQUARE_DEPTH: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerStyle {
    Disc,
    Circle,
    Square,
    Decimal(usize),
}

pub(crate) enum MarkerInk {
    /// A filled ellipse, a one-pixel ring or a filled square in this rect.
    Symbol(BoxRect),
    /// Shaped marker text; glyph positions are relative to `origin`.
    Text {
        layout: Box<Layout<u32>>,
        origin: [f32; 2],
    },
}

pub(crate) struct Marker {
    pub style: MarkerStyle,
    /// The marker box: the pointer target of the item.
    pub area: BoxRect,
    pub ink: MarkerInk,
    /// The item's clip; an outside marker lies beyond its border box.
    pub clip: BoxRect,
}

/// The marker style of a visible `li`, or `None` when it is not a list item.
pub(crate) fn style(
    tree: &Tree,
    env: &Environment,
    id: u32,
) -> Result<Option<MarkerStyle>, LayoutError> {
    let Some(element) = tree.element(id) else {
        return Ok(None);
    };
    if element.tag != Tag::Li
        || tree
            .computed_property(id, env, CssProperty::Display)
            .is_some()
    {
        return Ok(None);
    }
    let mut list = None;
    let mut depth = 0usize;
    for ancestor in tree.path(id).into_iter().skip(1) {
        let kind = tree
            .element(ancestor)
            .and_then(|e| crate::style::ua_defaults(e.tag))
            .and_then(|ua| ua.list);
        if let Some(kind) = kind {
            if list.is_none() {
                list = Some((ancestor, kind));
            } else {
                depth += 1;
            }
        }
    }
    Ok(Some(match list {
        None => MarkerStyle::Disc,
        Some((owner, ListKind::Ordered)) => {
            let parent = tree.nodes[&id].parent;
            if parent != Some(owner) {
                return Err(LayoutError::Unsupported(id));
            }
            MarkerStyle::Decimal(ordinal(tree, env, owner, id))
        }
        Some((_, ListKind::Unordered)) if depth >= SQUARE_DEPTH => MarkerStyle::Square,
        Some((_, ListKind::Unordered)) if depth >= CIRCLE_DEPTH => MarkerStyle::Circle,
        Some((_, ListKind::Unordered)) => MarkerStyle::Disc,
    }))
}

/// The CSS `list-item` counter: rendered list items before `id`, plus one.
fn ordinal(tree: &Tree, env: &Environment, list: u32, id: u32) -> usize {
    1 + tree.nodes[&list]
        .children
        .iter()
        .take_while(|&&child| child != id)
        .filter(|&&child| {
            tree.element(child).is_some_and(|e| e.tag == Tag::Li)
                && tree
                    .computed_property(child, env, CssProperty::Display)
                    .is_none()
        })
        .count()
}

/// Rendering inputs shared by every marker of one frame.
pub(crate) struct Inputs<'a> {
    pub tree: &'a Tree,
    pub env: &'a Environment,
    pub geometry: &'a BTreeMap<u32, BoxGeometry>,
    pub shapes: &'a BTreeMap<u32, TextShape>,
    pub flows: &'a BTreeMap<u32, Frame>,
    /// Whole-pixel vertical paint origin of a paragraph, as text painting uses.
    pub snap: fn(f32) -> f32,
}

/// Resolve every rendered list item's marker. Items need explicit fonts.
pub(crate) fn markers(
    inputs: &Inputs<'_>,
    mut font: Option<(&mut TextMeasurer, f32)>,
) -> Result<BTreeMap<u32, Marker>, LayoutError> {
    let mut result = BTreeMap::new();
    for &id in inputs.tree.nodes.keys() {
        if hidden(inputs.tree, inputs.env, id) {
            continue;
        }
        let Some(style) = style(inputs.tree, inputs.env, id)? else {
            continue;
        };
        let (measurer, size) = font.as_mut().ok_or(LayoutError::Unsupported(id))?;
        result.insert(id, marker(inputs, id, style, (measurer, *size))?);
    }
    Ok(result)
}

fn hidden(tree: &Tree, env: &Environment, id: u32) -> bool {
    tree.path(id).into_iter().any(|node| {
        tree.computed_property(node, env, CssProperty::Display)
            .is_some_and(|value| value == "none")
    })
}

fn marker(
    inputs: &Inputs<'_>,
    id: u32,
    style: MarkerStyle,
    font: (&mut TextMeasurer, f32),
) -> Result<Marker, LayoutError> {
    let (measurer, size) = font;
    let text_error = |error: crate::TextMeasureError| LayoutError::Text(error.to_string());
    if inputs
        .tree
        .computed_property(id, inputs.env, CssProperty::Overflow)
        .is_some_and(|value| value != "visible")
    {
        // An item's own clip would cut its outside marker; not modeled.
        return Err(LayoutError::Unsupported(id));
    }
    let item = inputs
        .geometry
        .get(&id)
        .ok_or(LayoutError::Unsupported(id))?;
    let baseline = first_baseline(inputs, id, measurer)?.ok_or(LayoutError::Unsupported(id))?;
    let (mut options, families) =
        crate::layout::text_style(inputs.tree, inputs.env, id, size, measurer)?;
    options.align = TextAlign::Start;
    options.white_space = WhiteSpaceMode::Pre;
    let (_, strut) = measurer
        .shape_styled(" ", None, &families, options)
        .map_err(text_error)?;
    let run_metrics = strut
        .lines()
        .next()
        .and_then(|line| line.runs().next().map(|run| *run.metrics()))
        .ok_or(LayoutError::Unsupported(id))?;
    let ascent = run_metrics.ascent.round();
    let above =
        ascent + ((run_metrics.line_height - ascent - run_metrics.descent.round()) * 0.5).floor();
    // The marker box spans its line box and, where the font's content area
    // overflows a small line height, that content area too.
    let top = (baseline - above).min(baseline - ascent);
    let bottom =
        (baseline - above + run_metrics.line_height).max(baseline + run_metrics.descent.round());
    let row = (top, bottom - top);
    let edge = item.content.x;
    let (area, ink) = if let MarkerStyle::Decimal(ordinal) = style {
        let text = format!("{ordinal}{DECIMAL_SUFFIX}");
        let (_, layout) = measurer
            .shape_styled(&text, None, &families, options)
            .map_err(text_error)?;
        let width = layout.full_width();
        let line_baseline = layout
            .lines()
            .next()
            .map(|line| line.metrics().baseline)
            .ok_or(LayoutError::Unsupported(id))?;
        let left = edge - width;
        (
            BoxRect {
                x: left,
                y: row.0,
                width,
                height: row.1,
            },
            MarkerInk::Text {
                layout: Box::new(layout),
                origin: [left, baseline - line_baseline],
            },
        )
    } else {
        // Blink's integer symbol arithmetic on the rounded primary ascent.
        let ascent = ascent as i32;
        let offset = ascent * 2 / 3;
        let bullet = (offset + 1) / 2;
        let start = edge - (offset + SYMBOL_PADDING_PX + SYMBOL_INSET_PX) as f32;
        let top = baseline - ascent as f32 + (3 * (ascent - offset) / 2) as f32;
        let left = start + SYMBOL_INSET_PX as f32;
        let snapped_left = (inputs.snap)(left);
        let snapped_top = (inputs.snap)(top);
        let symbol = BoxRect {
            x: snapped_left,
            y: snapped_top,
            width: (inputs.snap)(left + bullet as f32) - snapped_left,
            height: (inputs.snap)(top + bullet as f32) - snapped_top,
        };
        (
            BoxRect {
                x: start,
                y: row.0,
                width: (bullet + 2 * SYMBOL_INSET_PX) as f32,
                height: row.1,
            },
            MarkerInk::Symbol(symbol),
        )
    };
    Ok(Marker {
        style,
        area,
        ink,
        clip: item.clip,
    })
}

/// The baseline of the first line in `id`'s content, descending through
/// leading block children, in absolute CSS pixels.
fn first_baseline(
    inputs: &Inputs<'_>,
    id: u32,
    measurer: &mut TextMeasurer,
) -> Result<Option<f32>, LayoutError> {
    let mut node = id;
    for _ in 0..crate::layout::MAX_LAYOUT_DEPTH {
        if let Some(frame) = inputs.flows.get(&node) {
            let Some(line) = frame.shaped.layout.lines().next() else {
                return Ok(None);
            };
            let offset = frame.shaped.offsets.first().copied().unwrap_or_default();
            return Ok(Some(
                (inputs.snap)(frame.bounds.rect.y) + line.metrics().baseline + offset,
            ));
        }
        let Some(&first) = inputs.tree.nodes[&node]
            .children
            .iter()
            .find(|&&child| !hidden(inputs.tree, inputs.env, child))
        else {
            return Ok(None);
        };
        match &inputs.tree.nodes[&first].content {
            Content::Text(_) => {
                let shape = inputs
                    .shapes
                    .get(&first)
                    .ok_or(LayoutError::Unsupported(id))?;
                let rect = inputs
                    .geometry
                    .get(&first)
                    .ok_or(LayoutError::Unsupported(id))?
                    .rect;
                let (_, layout) = measurer
                    .shape_styled(
                        &shape.text,
                        Some(rect.width.clamp(0.01, 1_000_000.0)),
                        &shape.families,
                        shape.options,
                    )
                    .map_err(|error| LayoutError::Text(error.to_string()))?;
                let Some(line) = layout.lines().next() else {
                    return Ok(None);
                };
                let m = line.metrics();
                let above = m.ascent.round()
                    + ((m.line_height - m.ascent.round() - m.descent.round()) * 0.5).floor();
                return Ok(Some((inputs.snap)(rect.y) + above));
            }
            Content::Element(_) => node = first,
        }
    }
    Err(LayoutError::Limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Access, BatchWriter as B, Broker};
    use varta_common::view::CssProperty as P;

    struct Denied;
    impl Broker for Denied {
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

    fn tree(batch: &B) -> crate::View {
        let mut view = crate::View::default();
        view.commit(&batch.finish(), false, &mut Denied).unwrap();
        view
    }

    #[test]
    fn marker_styles_follow_list_nesting_and_ordinals_skip_hidden_items() {
        let mut b = B::new();
        b.create_element(2, Tag::Ul).insert(1, 2, 0);
        b.create_element(3, Tag::Li).insert(2, 3, 0);
        b.create_element(4, Tag::Ul).insert(3, 4, 0);
        b.create_element(5, Tag::Li).insert(4, 5, 0);
        b.create_element(6, Tag::Ol).insert(5, 6, 0);
        b.create_element(7, Tag::Li).insert(6, 7, 0);
        b.create_element(8, Tag::Ul).insert(7, 8, 0);
        b.create_element(9, Tag::Li).insert(8, 9, 0);
        b.create_element(10, Tag::Li).insert(6, 10, 0);
        b.set_style(10, P::Display, "none");
        b.create_element(11, Tag::Li).insert(6, 11, 0);
        b.create_element(12, Tag::Li).insert(1, 12, 0);
        b.create_element(13, Tag::Li).insert(2, 13, 0);
        b.set_style(13, P::Display, "block");
        let view = tree(&b);
        let env = Environment::default();
        let style = |id| super::style(&view.tree, &env, id).unwrap();
        assert_eq!(style(3), Some(MarkerStyle::Disc));
        assert_eq!(style(5), Some(MarkerStyle::Circle));
        assert_eq!(style(7), Some(MarkerStyle::Decimal(1)));
        assert_eq!(style(9), Some(MarkerStyle::Square));
        assert_eq!(style(11), Some(MarkerStyle::Decimal(2)));
        assert_eq!(style(12), Some(MarkerStyle::Disc));
        assert_eq!(style(13), None);
        assert_eq!(style(2), None);
    }
}
