//! `img`, the one replaced element of the view subset: a packaged image's
//! natural size and ratio sized as Chrome sizes replaced boxes (CSS 2.1
//! §10.3.2, §10.6.2 and the §10.4 constraint table), as a block-level box (a
//! `display: block`, flex or grid item or out-of-flow image), as the only
//! content of an anonymous line box, or as an atomic inline of a line of
//! text (`atom`, laid out by `inline::atoms`).
//!
//! An inline image alone forms a line when its parent is a block container,
//! no sibling next to it is inline-level content and it is baseline-aligned.
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

/// What sizes a replaced image: its natural size in CSS pixels and its
/// preferred aspect ratio (width over height), if any.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Replaced {
    pub natural: (f32, f32),
    pub ratio: Option<f32>,
}

/// A positive `<ratio>` (`a` or `a / b`), CSS Values 4 §8.
fn ratio(text: &str) -> Option<f32> {
    let mut parts = text.split('/').map(str::trim);
    let first: f32 = parts.next()?.parse().ok()?;
    let second: f32 = parts.next().map_or(Some(1.0), |part| part.parse().ok())?;
    (first > 0.0 && second > 0.0 && parts.next().is_none()).then_some(first / second)
}

/// The HTML `width`/`height` presentational hint of `id` in CSS pixels.
fn dimension_hint(tree: &Tree, id: u32, attribute: Attribute) -> Option<f32> {
    tree.element(id)?.attributes.get(&attribute)?.parse().ok()
}

/// The natural size and preferred ratio of `id` (HTML §4.8.4.3, §15.4.3;
/// CSS Sizing 4 §4.1 `aspect-ratio`). An `img` with an image has the
/// image's density-corrected natural size and ratio, or a ratio its
/// `aspect-ratio` or `width`/`height` attributes (their `aspect-ratio: auto
/// w / h` hint) give. An `img` without an image represents nothing: it is an
/// empty box of zero size, as Chrome lays it out. Refused: one that would
/// render its alternative text (a non-empty `alt`) or a sized broken-image
/// box (`width`/`height` attributes or styles), whose look the user agent
/// chooses (HTML §15.4.2).
pub(super) fn replaced(tree: &Tree, env: &Environment, id: u32) -> Result<Replaced, LayoutError> {
    let refused = || LayoutError::Unsupported(id);
    let element = tree.element(id).ok_or_else(refused)?;
    let shown = crate::image::style::shown(tree, env, id);
    let natural_ratio = shown
        .as_ref()
        .map(|shown| shown.natural.0 / shown.natural.1);
    if shown.is_none()
        && (element
            .attributes
            .get(&Attribute::Alt)
            .is_some_and(|alt| !alt.is_empty())
            || element.attributes.contains_key(&Attribute::Width)
            || element.attributes.contains_key(&Attribute::Height)
            || [
                CssProperty::Width,
                CssProperty::Height,
                CssProperty::MinWidth,
                CssProperty::MinHeight,
                CssProperty::AspectRatio,
            ]
            .into_iter()
            .any(|property| tree.computed_property(id, env, property).is_some()))
    {
        return Err(refused());
    }
    let hint = match (
        dimension_hint(tree, id, Attribute::Width),
        dimension_hint(tree, id, Attribute::Height),
    ) {
        (Some(width), Some(height)) if width > 0.0 && height > 0.0 => Some(width / height),
        _ => None,
    };
    let preferred = match tree.computed_property(id, env, CssProperty::AspectRatio) {
        None => natural_ratio.or(hint),
        Some(value) => {
            let auto = value.split_ascii_whitespace().any(|word| word == "auto");
            let given = value
                .split_ascii_whitespace()
                .filter(|word| *word != "auto")
                .collect::<Vec<_>>()
                .join(" ");
            let given = (!given.is_empty())
                .then(|| ratio(&given).ok_or_else(refused))
                .transpose()?;
            match (auto, given) {
                (true, given) => natural_ratio.or(given),
                (false, given) => given,
            }
        }
    };
    let natural = shown.map_or((0.0, 0.0), |shown| shown.natural);
    Ok(Replaced {
        // A ratio other than the image's keeps its natural width.
        natural: match preferred {
            Some(ratio) if Some(ratio) != natural_ratio && natural.0 > 0.0 => {
                (natural.0, natural.0 / ratio)
            }
            _ => natural,
        },
        ratio: preferred,
    })
}

/// Apply the HTML `width`/`height` presentational hints (§15.4.3, mapped
/// to `width`/`height` in pixels) where no author style sets the property.
pub(super) fn dimension_hints(tree: &Tree, env: &Environment, id: u32, style: &mut Style) {
    for (attribute, property) in [
        (Attribute::Width, CssProperty::Width),
        (Attribute::Height, CssProperty::Height),
    ] {
        if tree.computed_property(id, env, property).is_some() {
            continue;
        }
        if let Some(value) = dimension_hint(tree, id, attribute) {
            match property {
                CssProperty::Width => style.size.width = length(value),
                _ => style.size.height = length(value),
            }
        }
    }
}

/// Whether `id` is an `img` laid out as inline-level content of its parent
/// (its own line box or an atomic inline), rather than as a block-level box.
pub(crate) fn inline(tree: &Tree, env: &Environment, id: u32) -> bool {
    let display = tree.computed_property(id, env, CssProperty::Display);
    let positioned = tree
        .computed_property(id, env, CssProperty::Position)
        .is_some_and(|value| matches!(value.as_str(), "absolute" | "fixed"));
    display.as_deref().is_none_or(|value| value == "inline")
        && !positioned
        && !crate::inline::blockified(tree, env, id)
}

/// Whether inline image `id` alone forms its line (the flex row below):
/// no inline-level neighbour and `vertical-align: baseline`. Any other
/// inline image is an atomic inline of its parent's inline formatting
/// context.
pub(crate) fn alone(tree: &Tree, env: &Environment, id: u32) -> bool {
    let in_inline_box = tree.nodes[&id].parent.is_some_and(|parent| {
        inline_level(tree, env, parent) && !crate::inline::blockified(tree, env, parent)
    });
    inline(tree, env, id)
        && !in_inline_box
        && check_line(tree, env, id).is_ok()
        && tree
            .computed_property(id, env, CssProperty::VerticalAlign)
            .is_none_or(|value| value == "baseline")
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

/// The used content-box size of a replaced box with `natural` size and
/// preferred `ratio` for `width`/`height` (definite or `auto`) and min/max
/// constraints. Without a ratio an `auto` axis keeps its natural size.
fn used_size(
    (natural, ratio): ((f32, f32), Option<f32>),
    width: Option<f32>,
    height: Option<f32>,
    min: (f32, f32),
    max: (f32, f32),
) -> (f32, f32) {
    let clamp = |value: f32, min: f32, max: f32| value.min(max).max(min);
    // CSS 2.1 §10.4: a maximum below the minimum is the minimum.
    let max = (max.0.max(min.0), max.1.max(min.1));
    match (width, height, ratio) {
        (Some(width), Some(height), _) => (clamp(width, min.0, max.0), clamp(height, min.1, max.1)),
        (Some(width), None, Some(ratio)) => {
            let width = clamp(width, min.0, max.0);
            (width, clamp(width / ratio, min.1, max.1))
        }
        (None, Some(height), Some(ratio)) => {
            let height = clamp(height, min.1, max.1);
            (clamp(height * ratio, min.0, max.0), height)
        }
        (None, None, Some(_)) if natural.0 > 0.0 && natural.1 > 0.0 => {
            constrained(natural, min, max)
        }
        (width, height, _) => (
            clamp(width.unwrap_or(natural.0), min.0, max.0),
            clamp(height.unwrap_or(natural.1), min.1, max.1),
        ),
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

/// An image in a line of text, sized as an atomic inline (CSS 2.1 §10.3.2,
/// §10.6.2 with the §10.4 table) from its own lengths. Inline boxes take
/// `auto` margins as zero (§10.3.2, §10.6.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AtomBox {
    /// The border box size.
    pub size: (f32, f32),
    /// Margins in top, right, bottom, left order.
    pub margin: [f32; 4],
    /// Border plus padding in the same order.
    pub inset: [f32; 4],
}

/// Size an image of a line of text from its layout style. `None` when a
/// size, margin or padding is a percentage (or a `calc()` with one): it
/// resolves against the
/// containing block, which the line does not fix while it is measured.
pub(super) fn atom(style: &Style, replaced: Replaced) -> Option<AtomBox> {
    use taffy::style::{ExpandedLengthPercentage, ExpandedLengthPercentageAuto};
    let length = |value: LengthPercentage| match value.expand() {
        ExpandedLengthPercentage::Length(value) => Some(value),
        ExpandedLengthPercentage::Percent(_) | ExpandedLengthPercentage::Calc(_) => None,
    };
    let margin = |value: LengthPercentageAuto| match value.expand() {
        ExpandedLengthPercentageAuto::Length(value) => Some(value),
        ExpandedLengthPercentageAuto::Auto => Some(0.0),
        ExpandedLengthPercentageAuto::Percent(_) | ExpandedLengthPercentageAuto::Calc(_) => None,
    };
    // `Some(None)` is `auto` (or `none`), `None` refuses.
    let dimension = |value: Dimension| {
        if value.is_auto() {
            Some(None)
        } else {
            value.into_option().map(Some)
        }
    };
    let bound = |value: LengthPercentageAuto| match value.expand() {
        ExpandedLengthPercentageAuto::Length(value) => Some(Some(value)),
        ExpandedLengthPercentageAuto::Auto => Some(None),
        ExpandedLengthPercentageAuto::Percent(_) | ExpandedLengthPercentageAuto::Calc(_) => None,
    };
    let sides = |rect: Rect<LengthPercentage>| {
        Some([
            length(rect.top)?,
            length(rect.right)?,
            length(rect.bottom)?,
            length(rect.left)?,
        ])
    };
    let padding = sides(style.padding)?;
    let border = sides(style.border)?;
    let inset: [f32; 4] = std::array::from_fn(|side| padding[side] + border[side]);
    let edges = (inset[1] + inset[3], inset[0] + inset[2]);
    let content = |value: Option<f32>, edge: f32| {
        value.map(|value| match style.box_sizing {
            BoxSizing::ContentBox => value.max(0.0),
            BoxSizing::BorderBox => (value - edge).max(0.0),
        })
    };
    let width = content(dimension(style.size.width)?, edges.0);
    let height = content(dimension(style.size.height)?, edges.1);
    let min = (
        content(bound(style.min_size.width)?, edges.0).unwrap_or(0.0),
        content(bound(style.min_size.height)?, edges.1).unwrap_or(0.0),
    );
    let max = (
        content(bound(style.max_size.width)?, edges.0).unwrap_or(f32::INFINITY),
        content(bound(style.max_size.height)?, edges.1).unwrap_or(f32::INFINITY),
    );
    let used = used_size((replaced.natural, replaced.ratio), width, height, min, max);
    Some(AtomBox {
        size: (used.0 + edges.0, used.1 + edges.1),
        margin: [
            margin(style.margin.top)?,
            margin(style.margin.right)?,
            margin(style.margin.bottom)?,
            margin(style.margin.left)?,
        ],
        inset,
    })
}

/// Taffy's leaf layout for an image: its border box from the known
/// dimensions a parent imposes (a stretched or flexed size, final for that
/// axis), otherwise from its own sizes and natural ratio.
pub(super) fn compute(inputs: LayoutInput, style: &Style, replaced: Replaced) -> LayoutOutput {
    let Replaced { natural, ratio } = replaced;
    let parent = inputs.parent_size;
    // Layout-time `calc()` handles resolve against their basis.
    let calc = crate::calc::resolve;
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
    let sizing = (natural, ratio);
    let (used_width, used_height) = match (known_width, known_height) {
        (Some(width), Some(height)) => (width, height),
        // A parent-imposed size is final on its axis; the other axis follows
        // the ratio unless it has its own size.
        (Some(width), None) => (
            width,
            used_size(sizing, Some(width), height, (width, min.1), (width, max.1)).1,
        ),
        (None, Some(height)) => (
            used_size(
                sizing,
                width,
                Some(height),
                (min.0, height),
                (max.0, height),
            )
            .0,
            height,
        ),
        (None, None) => used_size(sizing, width, height, min, max),
    };
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
        let natural = ((20.0, 10.0), Some(2.0));
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
        // No image: a zero natural size, or the attributes' ratio.
        let empty = ((0.0, 0.0), None);
        assert_eq!(used_size(empty, None, None, none, unbounded), (0.0, 0.0));
        assert_eq!(
            used_size(empty, Some(80.0), None, none, unbounded),
            (80.0, 0.0)
        );
        assert_eq!(
            used_size(((0.0, 0.0), Some(2.0)), Some(80.0), None, none, unbounded),
            (80.0, 40.0)
        );
        assert_eq!(ratio("16 / 9"), Some(16.0 / 9.0));
        assert_eq!(ratio("2"), Some(2.0));
        assert_eq!(ratio("0 / 1"), None);
    }
}
