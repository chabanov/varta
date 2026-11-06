//! One-line `text-overflow: ellipsis` for a rich formatting root.
//!
//! The root keeps its untruncated inline geometry and pointer fragments, as
//! Chrome's layout does: truncation only hides the glyphs that do not fit
//! before the ellipsis and paints the ellipsis after the last visible cluster.
//! The accessible name is derived from the tree and keeps the full text.
use super::{ContentFlow, ShapedFlow};
use crate::layout::{LayoutError, TextShape};
use crate::style::Environment;
use crate::text::{TextAlign, TextSpan};
use crate::tree::Tree;
use crate::{TextMeasureError, TextMeasurer};
use varta_common::view::CssProperty as P;

/// U+2026 HORIZONTAL ELLIPSIS, used when the root's primary font has it.
const ELLIPSIS: &str = "\u{2026}";
/// Chrome's fallback when the primary font has no U+2026 glyph.
const ELLIPSIS_FALLBACK: &str = "...";
/// `.notdef`: the shaper's glyph for a character the font does not map.
const MISSING_GLYPH: u32 = 0;
/// Float tolerance when comparing advances with the content width.
const FIT_EPSILON: f32 = 1.0 / 1024.0;

/// Whether `root` truncates with an ellipsis. `clip` and a visible overflow
/// are no-ops; a scroll container's ellipsis is refused, as is any value on
/// an element that is not a block formatting root of inline content.
pub(crate) fn requested(tree: &Tree, env: &Environment, root: u32) -> Result<bool, LayoutError> {
    let Some(value) = tree.computed_property(root, env, P::TextOverflow) else {
        return Ok(false);
    };
    match value.as_str() {
        "clip" => Ok(false),
        "ellipsis" => match tree.computed_property(root, env, P::Overflow).as_deref() {
            None | Some("visible") => Ok(false),
            Some("hidden") => Ok(true),
            _ => Err(LayoutError::Unsupported(root)),
        },
        _ => Err(LayoutError::Unsupported(root)),
    }
}

/// The admitted ellipsis subset: one unbroken line without inline paint
/// decorations or edges, so hiding glyphs never splits a painted box.
pub(crate) fn admitted(root: u32, shape: &TextShape) -> Result<(), LayoutError> {
    let Some(rich) = &shape.rich else {
        return Ok(());
    };
    if !rich.ellipsis {
        return Ok(());
    }
    if shape.options.white_space.wraps()
        || shape.text.contains('\n')
        || rich.needs_paint_order
        || !rich.edges.is_empty()
    {
        return Err(LayoutError::Unsupported(root));
    }
    Ok(())
}

/// The visible glyph projection of an overflowing line, or `None` when the
/// line fits. `full` is the untruncated shaping at `width`.
pub(crate) fn truncate(
    rich: &ContentFlow,
    source: &TextShape,
    full: &ShapedFlow,
    width: f32,
    measurer: &mut TextMeasurer,
    root: u32,
) -> Result<Option<(ContentFlow, TextShape, ShapedFlow)>, LayoutError> {
    let text_error = |error: TextMeasureError| LayoutError::Text(error.to_string());
    let Some(line) = full.layout.lines().next() else {
        return Ok(None);
    };
    if full.layout.len() != 1 {
        return Err(LayoutError::Unsupported(root));
    }
    let metrics = line.metrics();
    let content = metrics.advance - metrics.trailing_whitespace;
    if content <= width + FIT_EPSILON {
        return Ok(None);
    }
    let mut options = source.options;
    options.align = TextAlign::Start;
    let marker = ellipsis_text(measurer, &source.families, options).map_err(text_error)?;
    let (marker_metrics, _) = measurer
        .shape_styled(marker, None, &source.families, options)
        .map_err(text_error)?;
    let available = width - marker_metrics.width;
    // Keep whole clusters (and whole ligatures) whose right edge fits.
    let mut cut = 0usize;
    let mut x = 0.0f32;
    'runs: for run in line.runs() {
        if run.is_rtl() {
            return Err(LayoutError::Unsupported(root));
        }
        let mut clusters = run.clusters().peekable();
        while let Some(first) = clusters.next() {
            measurer
                .charge_inline_work(1)
                .map_err(|_| LayoutError::Limit)?;
            let mut advance = first.advance();
            let mut end = first.text_range().end;
            while first.is_ligature_start()
                && clusters
                    .peek()
                    .is_some_and(|c| c.is_ligature_continuation())
            {
                let next = clusters.next().ok_or(LayoutError::Limit)?;
                advance += next.advance();
                end = next.text_range().end;
            }
            if x + advance > available + FIT_EPSILON {
                break 'runs;
            }
            x += advance;
            cut = end;
        }
    }
    let mut spans: Vec<TextSpan> = rich
        .spans
        .iter()
        .filter(|span| span.range.start < cut)
        .map(|span| TextSpan {
            range: span.range.start..span.range.end.min(cut),
            ..span.clone()
        })
        .collect();
    let mut text = source.text[..cut].to_owned();
    spans.push(TextSpan {
        range: cut..cut + marker.len(),
        owner: root,
        options,
        families: source.families.clone(),
    });
    text.push_str(marker);
    let mut paths = rich.paths.clone();
    paths.insert(root, Vec::new());
    let mut line_heights = rich.line_heights.clone();
    line_heights.insert(root, options.line_height.unwrap_or_default());
    let flow = ContentFlow {
        line_heights,
        spans,
        paths,
        nodes: rich.nodes.clone(),
        needs_paint_order: false,
        hidden: rich.hidden.clone(),
        strut: rich.strut,
        font_boxes: rich.font_boxes.clone(),
        empty: rich
            .empty
            .range(..=cut)
            .map(|(at, ids)| (*at, ids.clone()))
            .collect(),
        edges: Vec::new(),
        spacing: rich.spacing.clone(),
        atoms: std::collections::BTreeMap::new(),
        ellipsis: false,
    };
    let shape = TextShape {
        text,
        metrics: source.metrics,
        options,
        families: source.families.clone(),
        rich: None,
    };
    let mut shaped = flow
        .shape(&shape, measurer, Some(width.max(f32::MIN_POSITIVE)))
        .map_err(text_error)?;
    // The line box and baseline are those of the untruncated line, including
    // hidden content; only the glyphs change.
    let (Some(truncated), Some(&offset)) = (shaped.layout.lines().next(), full.offsets.first())
    else {
        return Err(LayoutError::Limit);
    };
    if shaped.layout.len() != 1 {
        return Err(LayoutError::Unsupported(root));
    }
    shaped.offsets = vec![offset + metrics.baseline - truncated.metrics().baseline];
    shaped.line_boxes.clone_from(&full.line_boxes);
    shaped.metrics = full.metrics;
    Ok(Some((flow, shape, shaped)))
}

/// U+2026 when the primary family maps it, otherwise three full stops.
fn ellipsis_text(
    measurer: &mut TextMeasurer,
    families: &[String],
    options: crate::text::TextOptions,
) -> Result<&'static str, TextMeasureError> {
    let (_, layout) = measurer.shape_styled(ELLIPSIS, None, families, options)?;
    let mapped = layout.lines().all(|line| {
        line.runs().all(|run| {
            run.visual_clusters()
                .all(|cluster| cluster.glyphs().all(|glyph| glyph.id != MISSING_GLYPH))
        })
    });
    Ok(if mapped { ELLIPSIS } else { ELLIPSIS_FALLBACK })
}
