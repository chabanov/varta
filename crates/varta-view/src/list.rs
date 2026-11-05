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
use varta_common::view::{CssProperty, ROOT_NODE, Tag};

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

/// A `li` with no authored `display` keeps the user-agent `list-item` display.
fn list_item(tree: &Tree, env: &Environment, id: u32) -> bool {
    tree.element(id).is_some_and(|e| e.tag == Tag::Li)
        && tree
            .computed_property(id, env, CssProperty::Display)
            .is_none()
}

fn display_none(tree: &Tree, env: &Environment, id: u32) -> bool {
    tree.computed_property(id, env, CssProperty::Display)
        .is_some_and(|value| value == "none")
}

/// The list context a node inherits from its ancestors.
#[derive(Clone, Copy, Default)]
struct ListContext {
    /// The nearest `ul`/`ol` ancestor.
    nearest: Option<(u32, ListKind)>,
    /// `ul`/`ol` ancestors in all.
    lists: usize,
    /// Path length from the root, as text-style resolution walks it.
    depth: usize,
}

/// One rendered list item: its marker style and element path length.
struct Item {
    id: u32,
    style: MarkerStyle,
    depth: usize,
}

/// Every rendered list item in document order, found in one walk from the
/// root that skips `display: none` subtrees. An `ol` assigns its items' CSS
/// `list-item` counter values (rendered items before each, plus one) in one
/// pass over its children. `charge` debits one unit per visited node and per
/// counted child.
fn items(
    tree: &Tree,
    env: &Environment,
    charge: &mut dyn FnMut(usize) -> Result<(), LayoutError>,
) -> Result<Vec<Item>, LayoutError> {
    let mut found = Vec::new();
    let mut ordinals = BTreeMap::new();
    let mut pending = vec![(ROOT_NODE, ListContext::default())];
    while let Some((id, context)) = pending.pop() {
        charge(1)?;
        let Some(node) = tree.nodes.get(&id) else {
            return Err(LayoutError::Unsupported(id));
        };
        let Some(element) = tree.element(id) else {
            continue;
        };
        if display_none(tree, env, id) {
            continue;
        }
        if list_item(tree, env, id) {
            let style = match context.nearest {
                None => MarkerStyle::Disc,
                Some((owner, ListKind::Ordered)) => {
                    if node.parent != Some(owner) {
                        return Err(LayoutError::Unsupported(id));
                    }
                    MarkerStyle::Decimal(*ordinals.get(&id).ok_or(LayoutError::Unsupported(id))?)
                }
                Some((_, ListKind::Unordered)) => match context.lists - 1 {
                    depth if depth >= SQUARE_DEPTH => MarkerStyle::Square,
                    depth if depth >= CIRCLE_DEPTH => MarkerStyle::Circle,
                    _ => MarkerStyle::Disc,
                },
            };
            found.push(Item {
                id,
                style,
                depth: context.depth + 1,
            });
        }
        let mut inner = ListContext {
            depth: context.depth + 1,
            ..context
        };
        if let Some(kind) = crate::style::ua_defaults(element.tag).and_then(|ua| ua.list) {
            inner.nearest = Some((id, kind));
            inner.lists += 1;
            if kind == ListKind::Ordered {
                charge(node.children.len())?;
                let mut count = 0;
                for &child in &node.children {
                    if list_item(tree, env, child) {
                        count += 1;
                        ordinals.insert(child, count);
                    }
                }
            }
        }
        pending.extend(node.children.iter().rev().map(|&child| (child, inner)));
    }
    Ok(found)
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

fn charge(measurer: &mut TextMeasurer, amount: usize) -> Result<(), LayoutError> {
    measurer
        .charge_inline_work(amount)
        .map_err(|_| LayoutError::Limit)
}

/// Resolve every rendered list item's marker. Items need explicit fonts. The
/// walk, ordinal counting, style-path reads and baseline search debit the
/// frame's native inline work allowance.
pub(crate) fn markers(
    inputs: &Inputs<'_>,
    font: Option<(&mut TextMeasurer, f32)>,
) -> Result<BTreeMap<u32, Marker>, LayoutError> {
    let Some((measurer, size)) = font else {
        // A frame without text has no allowance; this one linear walk over
        // the admitted tree only finds the first item to refuse.
        let found = items(inputs.tree, inputs.env, &mut |_| Ok(()))?;
        return match found.first() {
            Some(item) => Err(LayoutError::Unsupported(item.id)),
            None => Ok(BTreeMap::new()),
        };
    };
    let found = items(inputs.tree, inputs.env, &mut |amount| {
        charge(measurer, amount)
    })?;
    let mut result = BTreeMap::new();
    for item in found {
        // Text-style resolution reads each element on the item's path.
        charge(measurer, item.depth)?;
        result.insert(
            item.id,
            marker(inputs, item.id, item.style, (measurer, size))?,
        );
    }
    Ok(result)
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
        // `node` is rendered, so a child is hidden only by its own display.
        let mut leading = None;
        for &child in &inputs.tree.nodes[&node].children {
            charge(measurer, 1)?;
            if !display_none(inputs.tree, inputs.env, child) {
                leading = Some(child);
                break;
            }
        }
        let Some(first) = leading else {
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
        let mut visits = 0;
        let found = items(&view.tree, &env, &mut |amount| {
            visits += amount;
            Ok(())
        })
        .unwrap();
        let styles: Vec<_> = found.iter().map(|item| (item.id, item.style)).collect();
        assert_eq!(
            styles,
            [
                (3, MarkerStyle::Disc),
                (5, MarkerStyle::Circle),
                (7, MarkerStyle::Decimal(1)),
                (9, MarkerStyle::Square),
                (11, MarkerStyle::Decimal(2)),
                (12, MarkerStyle::Disc),
            ]
        );
        // Every visited node and each counted `ol` child is charged once;
        // the hidden item's (empty) subtree is not entered.
        assert_eq!(visits, 13 + 3);
        // An `ol` item that is not a direct child is refused.
        let mut b = B::new();
        b.create_element(2, Tag::Ol).insert(1, 2, 0);
        b.create_element(3, Tag::Div).insert(2, 3, 0);
        b.create_element(4, Tag::Li).insert(3, 4, 0);
        assert!(matches!(
            items(&tree(&b).tree, &env, &mut |_| Ok(())),
            Err(LayoutError::Unsupported(4))
        ));
    }

    /// Item count of the large list. A marker needs a line, so the native
    /// ceiling of 1024 text leaves per frame bounds the markable items.
    const LARGE_LIST_ITEMS: u32 = 1000;
    /// Marker work per item: walk, counting, path reads and the baseline search.
    const MAX_WORK_PER_ITEM: usize = 8;

    #[test]
    fn a_large_list_charges_linear_marker_work_to_the_inline_allowance() {
        const FIRST_ITEM: u32 = 3;
        let mut b = B::new();
        b.create_element(2, Tag::Ol).insert(1, 2, 0);
        for index in 0..LARGE_LIST_ITEMS {
            let id = FIRST_ITEM + 2 * index;
            b.create_element(id, Tag::Li)
                .insert(2, id, 0)
                .create_text(id + 1, "A")
                .insert(id, id + 1, 0);
        }
        let view = tree(&b);
        let env = Environment::default();
        let mut font = TextMeasurer::new(
            include_bytes!("../tests/fixtures/structure-400.ttf").to_vec(),
            "Varta Structure Fixture",
        )
        .unwrap();
        const SIZE: f32 = 16.0;
        const VIEWPORT: f32 = 400.0;
        // `reserve`: leave this much allowance before resolving markers.
        let run = |font: &mut TextMeasurer, reserve: Option<usize>| {
            font.with_view_shaping(|font| {
                let (geometry, shapes, flows) = crate::layout::geometry_and_text(
                    &view.tree, &env, VIEWPORT, VIEWPORT, font, SIZE,
                )
                .unwrap();
                if let Some(reserve) = reserve {
                    let spare = font.inline_work_remaining() - reserve;
                    font.charge_inline_work(spare).unwrap();
                }
                let before = font.inline_work_remaining();
                let inputs = Inputs {
                    tree: &view.tree,
                    env: &env,
                    geometry: &geometry,
                    shapes: &shapes,
                    flows: &flows,
                    snap: f32::round,
                };
                let markers = markers(&inputs, Some((&mut *font, SIZE)));
                (markers, before - font.inline_work_remaining())
            })
        };
        let (markers, used) = run(&mut font, None);
        let markers = markers.unwrap();
        let last = FIRST_ITEM + 2 * (LARGE_LIST_ITEMS - 1);
        assert_eq!(markers.len(), LARGE_LIST_ITEMS as usize);
        assert_eq!(
            markers[&last].style,
            MarkerStyle::Decimal(LARGE_LIST_ITEMS as usize)
        );
        assert!(
            used <= MAX_WORK_PER_ITEM * LARGE_LIST_ITEMS as usize,
            "{used}"
        );
        // One unit short of that work refuses the frame with the budget error.
        let (markers, _) = run(&mut font, Some(used - 1));
        assert!(matches!(markers, Err(LayoutError::Limit)));
    }
}
