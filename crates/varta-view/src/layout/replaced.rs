//! `img`, the one replaced element of the view subset: a packaged image's
//! natural size and ratio sized as Chrome sizes replaced boxes (CSS 2.1
//! §10.3.2, §10.6.2 and the §10.4 constraint table), as a block-level box (a
//! `display: block`, flex or grid item or out-of-flow image) or as the only
//! content of an anonymous line box.
//!
//! An inline image is admitted only where it alone forms a line: its parent
//! is a block container and no sibling next to it is inline-level content.
//! Its line box (CSS 2.1 §10.8) holds the parent's strut, whose ascent and
//! descent with half-leading are `above` and `below`, and the image's margin
//! box standing on the baseline: the line is `max(above, margin height)`
//! high above the baseline and `max(below, 0)` below it, and `text-align`
//! places the margin box, start-aligned when it overflows (CSS Text 3 §6.1).
//! A flex row stands for that line: it is at least `above` high, aligns the
//! image to its end on both axes (safely) and pads `below` under it.
use super::LayoutError;
use crate::style::Environment;
use crate::text::TextAlign;
use crate::tree::{Content, Tree};
use taffy::prelude::*;
use taffy::{LayoutInput, LayoutOutput, MaybeResolve, ResolveOrZero, SizingMode};
use varta_common::view::{Attribute, CssProperty, Tag};

/// The natural size of `id`'s image in CSS pixels; `None` when the view has
/// no images or its `src` names none of them (refused by the caller).
pub(super) fn natural_size(tree: &Tree, id: u32) -> Option<(f32, f32)> {
    let name = tree.element(id)?.attributes.get(&Attribute::Src)?;
    let (width, height) = tree.images.0.as_ref()?.size(name)?;
    Some((width as f32, height as f32))
}

/// Whether `id` is an `img` laid out as inline-level content of its parent
/// (its own line box), rather than as a block-level box.
pub(super) fn inline(tree: &Tree, env: &Environment, id: u32) -> bool {
    let display = tree.computed_property(id, env, CssProperty::Display);
    let positioned = tree
        .computed_property(id, env, CssProperty::Position)
        .is_some_and(|value| matches!(value.as_str(), "absolute" | "fixed"));
    display.as_deref().is_none_or(|value| value == "inline")
        && !positioned
        && !crate::inline::blockified(tree, env, id)
}

/// Whether node `id` is inline-level content that would share a line box
/// with an inline image next to it.
fn inline_level(tree: &Tree, env: &Environment, id: u32) -> bool {
    match &tree.nodes[&id].content {
        Content::Text(_) => true,
        Content::Element(element) => {
            let display = tree.computed_property(id, env, CssProperty::Display);
            match display.as_deref() {
                Some("none") => false,
                Some("inline") | None if element.tag == Tag::Img => inline(tree, env, id),
                Some("inline") => true,
                None => crate::inline::inline_tag(element.tag),
                Some(_) => false,
            }
        }
    }
}

/// Check an inline image's line: a parent block container and no inline
/// neighbour. Refusals name the image.
pub(super) fn check_line(tree: &Tree, env: &Environment, id: u32) -> Result<(), LayoutError> {
    let refused = || LayoutError::Unsupported(id);
    let parent = tree.nodes[&id].parent.ok_or_else(refused)?;
    let siblings = &tree.nodes[&parent].children;
    let at = siblings
        .iter()
        .position(|&child| child == id)
        .ok_or_else(refused)?;
    let neighbours = [at.checked_sub(1), at.checked_add(1)];
    if neighbours
        .into_iter()
        .flatten()
        .filter_map(|index| siblings.get(index))
        .any(|&sibling| inline_level(tree, env, sibling))
    {
        return Err(refused());
    }
    Ok(())
}

/// Author styles an inline image's line cannot reproduce: `auto` margins
/// (zero on an inline box, but free space in the line's flex row) and
/// percentage heights (resolved against the parent, not the line).
pub(super) fn check_inline_style(
    tree: &Tree,
    env: &Environment,
    id: u32,
) -> Result<(), LayoutError> {
    let properties = tree.computed_read(id, env);
    for (property, value) in properties.iter() {
        let refused = match property {
            CssProperty::Margin
            | CssProperty::MarginTop
            | CssProperty::MarginRight
            | CssProperty::MarginBottom
            | CssProperty::MarginLeft => value.split_ascii_whitespace().any(|v| v == "auto"),
            CssProperty::Height | CssProperty::MinHeight | CssProperty::MaxHeight => {
                value.contains('%')
            }
            _ => false,
        };
        if refused {
            return Err(LayoutError::Unsupported(id));
        }
    }
    Ok(())
}

/// The flex row standing for an inline image's line box.
pub(super) fn line_style(strut: (f32, f32), align: TextAlign) -> Style {
    let (above, below) = strut;
    Style {
        display: Display::Flex,
        box_sizing: BoxSizing::ContentBox,
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::NoWrap,
        justify_content: match align {
            TextAlign::Center => JustifyContent::SAFE_CENTER,
            TextAlign::Right => JustifyContent::SAFE_FLEX_END,
            TextAlign::Start | TextAlign::Left => JustifyContent::FLEX_START,
        },
        align_items: AlignItems::SAFE_FLEX_END,
        min_size: Size {
            width: auto(),
            height: length(above.max(0.0)),
        },
        padding: Rect {
            left: length(0.0),
            right: length(0.0),
            top: length(0.0),
            bottom: length(below.max(0.0)),
        },
        ..Default::default()
    }
}

/// An inline image as an item of its line's flex row: author flex item
/// properties do not apply to an inline box.
pub(super) fn line_item(style: &mut Style) {
    style.flex_grow = 0.0;
    style.flex_shrink = 0.0;
    style.flex_basis = Dimension::auto();
    style.align_self = None;
}

/// The used content-box size of a replaced box with natural size `natural`
/// for `width`/`height` (definite or `auto`) and min/max constraints.
fn used_size(
    natural: (f32, f32),
    width: Option<f32>,
    height: Option<f32>,
    min: (f32, f32),
    max: (f32, f32),
) -> (f32, f32) {
    let (natural_width, natural_height) = natural;
    let clamp = |value: f32, min: f32, max: f32| value.min(max).max(min);
    // CSS 2.1 §10.4: a maximum below the minimum is the minimum.
    let max = (max.0.max(min.0), max.1.max(min.1));
    let ratio = natural_width / natural_height;
    match (width, height) {
        (Some(width), Some(height)) => (clamp(width, min.0, max.0), clamp(height, min.1, max.1)),
        (Some(width), None) => {
            let width = clamp(width, min.0, max.0);
            (width, clamp(width / ratio, min.1, max.1))
        }
        (None, Some(height)) => {
            let height = clamp(height, min.1, max.1);
            (clamp(height * ratio, min.0, max.0), height)
        }
        (None, None) => constrained(natural, min, max),
    }
}

/// CSS 2.1 §10.4's table for `auto` width and height with a ratio.
fn constrained(natural: (f32, f32), min: (f32, f32), max: (f32, f32)) -> (f32, f32) {
    let (w, h) = natural;
    let (min_w, min_h) = min;
    let (max_w, max_h) = max;
    let wide = w > max_w;
    let narrow = w < min_w;
    let tall = h > max_h;
    let short = h < min_h;
    match (wide, narrow, tall, short) {
        (true, _, true, _) if max_w / w <= max_h / h => (max_w, min_h.max(max_w * h / w)),
        (true, _, true, _) => (min_w.max(max_h * w / h), max_h),
        (_, true, _, true) if min_w / w <= min_h / h => (max_w.min(min_h * w / h), min_h),
        (_, true, _, true) => (min_w, max_h.min(min_w * h / w)),
        (_, true, true, _) => (min_w, max_h),
        (true, _, _, true) => (max_w, min_h),
        (true, ..) => (max_w, min_h.max(max_w * h / w)),
        (_, true, ..) => (min_w, max_h.min(min_w * h / w)),
        (_, _, true, _) => (min_w.max(max_h * w / h), max_h),
        (.., true) => (max_w.min(min_h * w / h), min_h),
        _ => (w, h),
    }
}

/// Taffy's leaf layout for an image: its border box from the known
/// dimensions a parent imposes (a stretched or flexed size, final for that
/// axis), otherwise from its own sizes and natural ratio.
pub(super) fn compute(inputs: LayoutInput, style: &Style, natural: (f32, f32)) -> LayoutOutput {
    let parent = inputs.parent_size;
    let calc = |_: *const (), value: f32| value;
    let padding = style.padding.resolve_or_zero(parent.width, calc);
    let border = style.border.resolve_or_zero(parent.width, calc);
    let edges = Size {
        width: padding.left + padding.right + border.left + border.right,
        height: padding.top + padding.bottom + border.top + border.bottom,
    };
    let content = |value: Option<f32>, edge: f32| {
        value.map(|value| match style.box_sizing {
            BoxSizing::ContentBox => value.max(0.0),
            BoxSizing::BorderBox => (value - edge).max(0.0),
        })
    };
    let (width, height, min, max) = match inputs.sizing_mode {
        SizingMode::ContentSize => (None, None, (0.0, 0.0), (f32::INFINITY, f32::INFINITY)),
        SizingMode::InherentSize => (
            content(
                style.size.width.maybe_resolve(parent.width, calc),
                edges.width,
            ),
            content(
                style.size.height.maybe_resolve(parent.height, calc),
                edges.height,
            ),
            (
                content(
                    style.min_size.width.maybe_resolve(parent.width, calc),
                    edges.width,
                )
                .unwrap_or(0.0),
                content(
                    style.min_size.height.maybe_resolve(parent.height, calc),
                    edges.height,
                )
                .unwrap_or(0.0),
            ),
            (
                content(
                    style.max_size.width.maybe_resolve(parent.width, calc),
                    edges.width,
                )
                .unwrap_or(f32::INFINITY),
                content(
                    style.max_size.height.maybe_resolve(parent.height, calc),
                    edges.height,
                )
                .unwrap_or(f32::INFINITY),
            ),
        ),
    };
    let known = inputs.known_dimensions;
    let known_width = known.width.map(|value| (value - edges.width).max(0.0));
    let known_height = known.height.map(|value| (value - edges.height).max(0.0));
    let ratio = natural.0 / natural.1;
    let (used_width, used_height) = match (known_width, known_height) {
        (Some(width), Some(height)) => (width, height),
        // A parent-imposed size is final on its axis; the other axis follows
        // the ratio unless it has its own size.
        (Some(width), None) => (
            width,
            used_size(natural, Some(width), height, (width, min.1), (width, max.1)).1,
        ),
        (None, Some(height)) => (
            used_size(
                natural,
                width,
                Some(height),
                (min.0, height),
                (max.0, height),
            )
            .0,
            height,
        ),
        (None, None) => used_size(natural, width, height, min, max),
    };
    debug_assert!(ratio.is_finite());
    LayoutOutput::from_outer_size(Size {
        width: known.width.unwrap_or(used_width + edges.width),
        height: known.height.unwrap_or(used_height + edges.height),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constraint_table_keeps_the_ratio_where_css_does() {
        let natural = (20.0, 10.0);
        let none = (0.0, 0.0);
        let unbounded = (f32::INFINITY, f32::INFINITY);
        assert_eq!(
            used_size(natural, None, None, none, unbounded),
            (20.0, 10.0)
        );
        assert_eq!(
            used_size(natural, Some(40.0), None, none, unbounded),
            (40.0, 20.0)
        );
        assert_eq!(
            used_size(natural, None, Some(5.0), none, unbounded),
            (10.0, 5.0)
        );
        // `max-width: 100%` of a narrower parent scales both axes.
        assert_eq!(
            used_size(natural, None, None, none, (10.0, f32::INFINITY)),
            (10.0, 5.0)
        );
        // `min-width` beyond the natural width.
        assert_eq!(
            used_size(natural, None, None, (40.0, 0.0), unbounded),
            (40.0, 20.0)
        );
        // An explicit width with `max-height` breaks the ratio.
        assert_eq!(
            used_size(natural, Some(30.0), None, none, (f32::INFINITY, 5.0)),
            (30.0, 5.0)
        );
        // Both axes over their maximum: the tighter one wins.
        assert_eq!(
            used_size(natural, None, None, none, (16.0, 4.0)),
            (8.0, 4.0)
        );
        // Minimum width above maximum height.
        assert_eq!(
            used_size(natural, None, None, (30.0, 0.0), (f32::INFINITY, 12.0)),
            (30.0, 12.0)
        );
        // A maximum below the minimum is the minimum.
        assert_eq!(
            used_size(natural, None, None, (30.0, 0.0), (10.0, f32::INFINITY)),
            (30.0, 15.0)
        );
    }
}
