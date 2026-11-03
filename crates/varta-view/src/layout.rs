//! Initial, deliberately bounded box-only layout using the shared native engine.
use crate::style::{self, Environment, Name};
use crate::text::{TextOptions, TextSource, WhiteSpaceMode};
use crate::tree::{Content, Tree, border_components, geometry_sides};
use crate::units::Units;
use crate::{TextMeasurer, TextMetrics};
use parley::LineHeight;
use std::collections::BTreeMap;
use taffy::prelude::*;
use taffy::style::{GridPlacement, GridTemplateTracks, Overflow, RepetitionCount};
use varta_common::view::{CssProperty, ROOT_NODE, Tag};

/// A border-box rectangle in viewport-relative CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A border box and the rectangular ancestor clip applied when painting it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxGeometry {
    pub rect: BoxRect,
    pub clip: BoxRect,
    /// CSS content box before clipping.
    pub content: BoxRect,
    /// Vertical scroll range when the element has `overflow: auto`.
    pub scroll: Option<ScrollMetrics>,
    /// Programmatic scrolling also includes `overflow: hidden`.
    pub programmatic_scroll: Option<ScrollMetrics>,
}

/// Clamped vertical scroll offset and maximum in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollMetrics {
    pub top: f32,
    pub max: f32,
    /// Padding-box scrollport before clipping by ancestors.
    pub viewport: BoxRect,
}

pub(crate) fn intersect(a: BoxRect, b: BoxRect) -> BoxRect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    BoxRect {
        x,
        y,
        width: ((a.x + a.width).min(b.x + b.width) - x).max(0.0),
        height: ((a.y + a.height).min(b.y + b.height) - y).max(0.0),
    }
}

/// Compute nearest-edge vertical scrolling, from the innermost scrollport out.
pub(crate) fn scroll_into_view_plan(
    tree: &Tree,
    environment: &Environment,
    geometry: &BTreeMap<u32, BoxGeometry>,
    target: u32,
) -> Result<Vec<(u32, f64)>, LayoutError> {
    if !tree.displayed(target, environment) {
        return Ok(Vec::new());
    }
    let path = tree.path(target);
    for id in &path {
        if tree
            .computed(*id, environment)
            .get(&CssProperty::Position)
            .is_some_and(|position| matches!(position.as_str(), "absolute" | "fixed"))
        {
            return Err(LayoutError::Unsupported(*id));
        }
    }
    let mut rect = geometry
        .get(&target)
        .ok_or(LayoutError::Unsupported(target))?
        .rect;
    let mut changes = Vec::new();
    for id in path.into_iter().skip(1) {
        let Some(scroll) = geometry.get(&id).and_then(|item| item.programmatic_scroll) else {
            continue;
        };
        let top = rect.y;
        let bottom = rect.y + rect.height;
        let port_top = scroll.viewport.y;
        let port_bottom = port_top + scroll.viewport.height;
        let above = top < port_top;
        let below = bottom > port_bottom;
        let delta = if above && below {
            0.0
        } else if (above && rect.height <= scroll.viewport.height)
            || (below && rect.height > scroll.viewport.height)
        {
            top - port_top
        } else if (above && rect.height > scroll.viewport.height)
            || (below && rect.height <= scroll.viewport.height)
        {
            bottom - port_bottom
        } else {
            0.0
        };
        let next = (scroll.top + delta).clamp(0.0, scroll.max);
        if next != scroll.top {
            rect.y -= next - scroll.top;
            changes.push((id, f64::from(next)));
        }
    }
    Ok(changes)
}

/// A tree outside the currently implemented box-only subset is rejected.
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("invalid viewport dimensions")]
    Viewport,
    #[error("invalid host reference root font size")]
    RootFont,
    #[error("layout exceeds the initial box or depth limit")]
    Limit,
    #[error("unsupported layout content or style on node {0}")]
    Unsupported(u32),
    #[error("layout engine failed: {0}")]
    Engine(String),
    #[error("text measurement failed: {0}")]
    Text(String),
}

fn factor(value: &str) -> Option<f32> {
    let number: f32 = value.parse().ok()?;
    (number.is_finite() && (0.0..=1000.0).contains(&number)).then_some(number)
}

/// Properties that affect paint or stacking, but not the border-box geometry
/// this API returns. They are still retained by the regular view snapshot.
fn paint_only(property: CssProperty) -> bool {
    matches!(
        property,
        CssProperty::BackgroundColor
            | CssProperty::Color
            | CssProperty::Opacity
            | CssProperty::BorderRadius
            | CssProperty::Outline
            | CssProperty::OutlineWidth
            | CssProperty::OutlineStyle
            | CssProperty::OutlineColor
            | CssProperty::OutlineOffset
            | CssProperty::Cursor
            | CssProperty::TextDecoration
            | CssProperty::ZIndex
    )
}

fn grid_tracks(value: &str, units: Units) -> Option<Vec<GridTemplateComponent<String>>> {
    if value == "none" {
        return Some(Vec::new());
    }
    if !varta_common::view::css::property_value(CssProperty::GridTemplateColumns, value) {
        return None;
    }
    // The profile grammar bounds token count, not numeric magnitude. Reject
    // extreme and negative tracks before passing them to the layout engine.
    for number in value.split(|ch: char| !ch.is_ascii_digit() && ch != '.' && ch != '-') {
        if number.is_empty() {
            continue;
        }
        let value: f64 = number.parse().ok()?;
        if !value.is_finite() || !(0.0..=1_000_000.0).contains(&value) {
            return None;
        }
    }
    let normalized = units.grid(value)?;
    let parsed: GridTemplateTracks<String, GridTemplateComponent<String>> =
        normalized.parse().ok()?;
    let mut count = 0usize;
    for component in &parsed.tracks {
        count = count.checked_add(match component {
            GridTemplateComponent::Single(_) => 1,
            GridTemplateComponent::Repeat(repeat) => match repeat.count {
                RepetitionCount::Count(n) if n > 0 && !repeat.tracks.is_empty() => {
                    usize::from(n).checked_mul(repeat.tracks.len())?
                }
                _ => return None,
            },
        })?;
        if count > 64 {
            return None;
        }
    }
    Some(parsed.tracks)
}

fn grid_line(value: &str) -> Option<GridPlacement<String>> {
    let value = value.trim();
    if value == "auto" {
        return Some(GridPlacement::Auto);
    }
    if let Some(span) = value.strip_prefix("span ") {
        let span: u16 = span.trim().parse().ok()?;
        return (1..=64)
            .contains(&span)
            .then(|| GridPlacement::from_span(span));
    }
    let line: i16 = value.parse().ok()?;
    (line != 0 && (-64..=64).contains(&line)).then(|| GridPlacement::from_line_index(line))
}

fn grid_placement(value: &str) -> Option<Line<GridPlacement<String>>> {
    let (start, end) = value.split_once('/').unwrap_or((value, "auto"));
    Some(Line {
        start: grid_line(start)?,
        end: grid_line(end)?,
    })
}

fn border_shorthand(value: &str, units: Units) -> Option<(f32, bool)> {
    if !varta_common::view::css::property_value(CssProperty::Border, value) {
        return None;
    }
    let mut width = 3.0;
    let mut solid = false;
    let mut depth = 0usize;
    for token in value.split_ascii_whitespace() {
        let in_function = depth > 0 || token.contains('(');
        for ch in token.chars() {
            if ch == '(' {
                depth += 1;
            }
            if ch == ')' {
                depth = depth.saturating_sub(1);
            }
        }
        if in_function {
            continue;
        }
        if token == "solid" {
            solid = true;
        } else if token == "0" || token.ends_with("px") || token.ends_with("rem") {
            width = units.pixels(token, false)?;
        } else if token.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
            && token.ends_with(|c: char| c.is_ascii_alphabetic())
        {
            return None;
        }
    }
    Some((width, solid))
}

pub(crate) fn border_component(
    property: CssProperty,
    value: &str,
    units: Units,
) -> Option<(f32, bool)> {
    match property {
        CssProperty::BorderWidth => {
            let number = units.pixels(value, false)?;
            Some((number, false))
        }
        CssProperty::BorderStyle => match value {
            "solid" => Some((3.0, true)),
            "none" => Some((3.0, false)),
            _ => None,
        },
        _ => border_shorthand(value, units),
    }
}

pub(crate) fn border_widths(
    tree: &Tree,
    env: &Environment,
    id: u32,
) -> Result<[f32; 4], LayoutError> {
    let units = Units::new(env).ok_or(LayoutError::RootFont)?;
    let element = tree.element(id).ok_or(LayoutError::Unsupported(id))?;
    let variables = style::Variables::new(tree, id, env);
    let mut widths = [3.0; 4];
    let mut solid = [false; 4];
    for (name, raw) in tree.cascade_refs(id, env) {
        let Name::Property(property) = name else {
            continue;
        };
        let property = *property;
        let components = border_components(property);
        if components.is_empty() {
            continue;
        }
        let value = variables.property(property, raw);
        for (side, is_style) in components {
            let (width, present) = match &value {
                Some(value) => {
                    border_component(property, value, units).ok_or(LayoutError::Unsupported(id))?
                }
                None => (3.0, false),
            };
            if is_style {
                solid[side as usize] = present;
            } else {
                widths[side as usize] = width;
            }
        }
    }
    for (&(side, is_style), (property, raw)) in &element.border_inline {
        let (width, present) = match variables.property(*property, raw) {
            Some(value) => {
                border_component(*property, &value, units).ok_or(LayoutError::Unsupported(id))?
            }
            None => (3.0, false),
        };
        if is_style {
            solid[side as usize] = present;
        } else {
            widths[side as usize] = width;
        }
    }
    for i in 0..4 {
        if !solid[i] {
            widths[i] = 0.0;
        }
    }
    Ok(widths)
}

fn edges(value: &str) -> Option<[&str; 4]> {
    let values: Vec<_> = value.split_ascii_whitespace().collect();
    match values.as_slice() {
        [a] => Some([a, a, a, a]),
        [a, b] => Some([a, b, a, b]),
        [a, b, c] => Some([a, b, c, b]),
        [a, b, c, d] => Some([a, b, c, d]),
        _ => None,
    }
}

/// Expand shorthands in cascade order without altering the normal snapshot.
fn layout_properties(
    tree: &Tree,
    env: &Environment,
    id: u32,
) -> Result<BTreeMap<CssProperty, String>, LayoutError> {
    let element = tree.element(id).ok_or(LayoutError::Unsupported(id))?;
    let variables = style::Variables::new(tree, id, env);
    let declarations = tree
        .cascade_refs(id, env)
        .into_iter()
        .filter_map(|(name, value)| match name {
            Name::Property(property) => Some((*property, value)),
            Name::Custom(_) => None,
        })
        .chain(
            element
                .style
                .iter()
                .filter(|(property, _)| geometry_sides(**property).is_none())
                .map(|(property, value)| (*property, value.as_str())),
        );
    let mut result = BTreeMap::new();
    for (property, value) in declarations {
        let Some(value) = variables.property(property, value) else {
            let affected: &[CssProperty] = match property {
                CssProperty::Margin => &[
                    CssProperty::MarginTop,
                    CssProperty::MarginRight,
                    CssProperty::MarginBottom,
                    CssProperty::MarginLeft,
                ],
                CssProperty::Padding => &[
                    CssProperty::PaddingTop,
                    CssProperty::PaddingRight,
                    CssProperty::PaddingBottom,
                    CssProperty::PaddingLeft,
                ],
                CssProperty::Gap => &[CssProperty::RowGap, CssProperty::ColumnGap],
                _ => std::slice::from_ref(&property),
            };
            for side in affected {
                result.remove(side);
            }
            continue;
        };
        let sides = match property {
            CssProperty::Margin => Some([
                CssProperty::MarginTop,
                CssProperty::MarginRight,
                CssProperty::MarginBottom,
                CssProperty::MarginLeft,
            ]),
            CssProperty::Padding => Some([
                CssProperty::PaddingTop,
                CssProperty::PaddingRight,
                CssProperty::PaddingBottom,
                CssProperty::PaddingLeft,
            ]),
            _ => None,
        };
        if let Some(sides) = sides {
            let values = edges(&value).ok_or(LayoutError::Unsupported(id))?;
            for (side, value) in sides.into_iter().zip(values) {
                result.insert(side, value.to_owned());
            }
        } else if property == CssProperty::Gap {
            let values: Vec<_> = value.split_ascii_whitespace().collect();
            let (row, column) = match values.as_slice() {
                [one] => (*one, *one),
                [row, column] => (*row, *column),
                _ => return Err(LayoutError::Unsupported(id)),
            };
            result.insert(CssProperty::RowGap, row.to_owned());
            result.insert(CssProperty::ColumnGap, column.to_owned());
        } else {
            result.insert(property, value);
        }
    }
    for (side, (source, raw, index)) in &element.layout_inline {
        let Some(value) = variables.property(*source, raw) else {
            result.remove(side);
            continue;
        };
        let normalized = match source {
            CssProperty::Margin | CssProperty::Padding => {
                let values = edges(&value).ok_or(LayoutError::Unsupported(id))?;
                values[*index].to_owned()
            }
            CssProperty::Gap => {
                let values: Vec<_> = value.split_ascii_whitespace().collect();
                let value = match values.as_slice() {
                    [one] => *one,
                    [row, column] => {
                        if *index == 0 {
                            *row
                        } else {
                            *column
                        }
                    }
                    _ => return Err(LayoutError::Unsupported(id)),
                };
                value.to_owned()
            }
            _ => value,
        };
        result.insert(*side, normalized);
    }
    Ok(result)
}

fn alignment(value: &str) -> Option<AlignItems> {
    Some(match value {
        "stretch" => AlignItems::STRETCH,
        "start" => AlignItems::START,
        "end" => AlignItems::END,
        "flex-start" => AlignItems::FLEX_START,
        "flex-end" => AlignItems::FLEX_END,
        "center" => AlignItems::CENTER,
        "baseline" => AlignItems::BASELINE,
        _ => return None,
    })
}

#[derive(Clone)]
pub(crate) struct TextShape {
    pub text: String,
    pub metrics: TextMetrics,
    pub options: TextOptions,
    pub families: Vec<String>,
    pub rich: Option<std::sync::Arc<crate::inline::ContentFlow>>,
}
impl TextShape {
    pub(crate) fn measure(
        &self,
        measurer: &mut TextMeasurer,
        width: Option<f32>,
    ) -> Result<TextMetrics, crate::TextMeasureError> {
        match &self.rich {
            Some(rich) => rich
                .shape(self, measurer, width)
                .map(|shaped| shaped.metrics),
            None => measurer.measure_styled(&self.text, width, &self.families, self.options),
        }
    }
}
type TextGeometry = (
    BTreeMap<u32, BoxGeometry>,
    BTreeMap<u32, TextShape>,
    BTreeMap<u32, crate::inline::Frame>,
);

pub(crate) fn text_style(
    tree: &Tree,
    env: &Environment,
    id: u32,
    base_size: f32,
    measurer: &TextMeasurer,
) -> Result<(TextOptions, Vec<String>), LayoutError> {
    let units = Units::new(env).ok_or(LayoutError::RootFont)?;
    let mut size = base_size;
    let mut line_height = None;
    let mut weight = 400.0;
    let mut italic = false;
    let mut white_space = WhiteSpaceMode::Normal;
    let mut break_anywhere = false;
    let mut family = None;
    for ancestor in tree.path(id).into_iter().rev() {
        let properties = tree.computed(ancestor, env);
        if let Some(element) = tree.element(ancestor) {
            if element.tag == Tag::Strong && !properties.contains_key(&CssProperty::FontWeight) {
                weight = if weight < 400.0 {
                    400.0
                } else if weight < 600.0 {
                    700.0
                } else {
                    900.0
                };
            }
            if element.tag == Tag::Em && !properties.contains_key(&CssProperty::FontStyle) {
                italic = true;
            }
            if element.tag == Tag::Code && !properties.contains_key(&CssProperty::FontFamily) {
                family = Some((ancestor, "monospace".into()));
            }
        }
        if let Some(value) = properties.get(&CssProperty::OverflowWrap) {
            break_anywhere = match value.as_str() {
                "normal" => false,
                "anywhere" => true,
                _ => return Err(LayoutError::Unsupported(ancestor)),
            };
        }
        if let Some(value) = properties.get(&CssProperty::WhiteSpace) {
            white_space = match value.as_str() {
                "normal" => WhiteSpaceMode::Normal,
                "nowrap" => WhiteSpaceMode::NoWrap,
                "pre-wrap" => WhiteSpaceMode::PreWrap,
                _ => return Err(LayoutError::Unsupported(ancestor)),
            };
        }
        if let Some(value) = properties.get(&CssProperty::FontFamily) {
            family = Some((ancestor, value.clone()));
        }
        if let Some(value) = properties.get(&CssProperty::FontWeight) {
            weight = match value.as_str() {
                "normal" => 400.0,
                "bold" => 700.0,
                "400" => 400.0,
                "500" => 500.0,
                "600" => 600.0,
                "700" => 700.0,
                _ => return Err(LayoutError::Unsupported(ancestor)),
            };
        }
        if let Some(value) = properties.get(&CssProperty::FontStyle) {
            italic = match value.as_str() {
                "normal" => false,
                "italic" => true,
                _ => return Err(LayoutError::Unsupported(ancestor)),
            };
        }
        if let Some(value) = properties.get(&CssProperty::FontSize) {
            size = if let Some(number) = value.strip_suffix('%') {
                let number: f32 = number
                    .parse()
                    .map_err(|_| LayoutError::Unsupported(ancestor))?;
                size * number / 100.0
            } else {
                units
                    .pixels(value, false)
                    .ok_or(LayoutError::Unsupported(ancestor))?
            };
            if !size.is_finite() || !(0.01..=4096.0).contains(&size) {
                return Err(LayoutError::Unsupported(ancestor));
            }
        }
        if let Some(value) = properties.get(&CssProperty::LineHeight) {
            line_height = if value == "normal" {
                None
            } else if value.ends_with("px") || value.ends_with("rem") {
                let px = units
                    .pixels(value, false)
                    .ok_or(LayoutError::Unsupported(ancestor))?;
                if !px.is_finite() || !(0.01..=1_000_000.0).contains(&px) {
                    return Err(LayoutError::Unsupported(ancestor));
                }
                Some(LineHeight::Absolute(px))
            } else if let Some(value) = value.strip_suffix('%') {
                let percentage: f32 = value
                    .parse()
                    .map_err(|_| LayoutError::Unsupported(ancestor))?;
                let px = size * percentage / 100.0;
                if !px.is_finite() || !(0.01..=1_000_000.0).contains(&px) {
                    return Err(LayoutError::Unsupported(ancestor));
                }
                Some(LineHeight::Absolute(px))
            } else {
                let multiplier: f32 = value
                    .parse()
                    .map_err(|_| LayoutError::Unsupported(ancestor))?;
                if !multiplier.is_finite() || !(0.001..=1000.0).contains(&multiplier) {
                    return Err(LayoutError::Unsupported(ancestor));
                }
                Some(LineHeight::FontSizeRelative(multiplier))
            };
        }
    }
    let ancestor = family.as_ref().map_or(id, |(ancestor, _)| *ancestor);
    let families = measurer
        .resolve_families(family.as_ref().map(|(_, family)| family.as_str()))
        .ok_or(LayoutError::Unsupported(ancestor))?;
    Ok((
        TextOptions {
            size,
            line_height,
            weight,
            italic,
            white_space,
            break_anywhere,
            source: TextSource::General,
        },
        families,
    ))
}

fn build(
    tree: &Tree,
    env: &Environment,
    engine: &mut TaffyTree<TextShape>,
    ids: &mut BTreeMap<u32, NodeId>,
    text_metrics: &BTreeMap<u32, TextShape>,
    location: (u32, usize),
    viewport: (f32, f32),
) -> Result<NodeId, LayoutError> {
    let (id, depth) = location;
    let units = Units::new(env).ok_or(LayoutError::RootFont)?;
    if depth > 128 || ids.len() >= 4096 {
        return Err(LayoutError::Limit);
    }
    let node = tree.nodes.get(&id).ok_or(LayoutError::Unsupported(id))?;
    let Content::Element(element) = &node.content else {
        let metrics = text_metrics
            .get(&id)
            .cloned()
            .ok_or(LayoutError::Unsupported(id))?;
        let node_id = engine
            .new_leaf_with_context(Style::default(), metrics)
            .map_err(|e| LayoutError::Engine(e.to_string()))?;
        ids.insert(id, node_id);
        return Ok(node_id);
    };
    // Hidden elements and their descendants contribute no layout, even if
    // their tag or CSS is not supported by the visible layout path.
    if id != ROOT_NODE
        && tree
            .computed(id, env)
            .get(&CssProperty::Display)
            .is_some_and(|value| value.eq_ignore_ascii_case("none"))
    {
        let hidden = engine
            .new_leaf(Style {
                display: Display::None,
                ..Default::default()
            })
            .map_err(|e| LayoutError::Engine(e.to_string()))?;
        ids.insert(id, hidden);
        return Ok(hidden);
    }
    if id != ROOT_NODE
        && !(matches!(
            element.tag,
            Tag::Div
                | Tag::P
                | Tag::Header
                | Tag::Nav
                | Tag::Main
                | Tag::Section
                | Tag::Footer
                | Tag::Aside
                | Tag::Form
                | Tag::Button
                | Tag::Input
                | Tag::Textarea
                | Tag::Select
        ) || (crate::inline::inline_tag(element.tag)
            && element.tag != Tag::Br
            && (crate::inline::blockified(tree, env, id)
                || tree
                    .computed_property(id, env, CssProperty::Display)
                    .is_some_and(|d| matches!(d.as_str(), "block" | "flex" | "grid")))))
    {
        return Err(LayoutError::Unsupported(id));
    }
    let mut style = Style {
        display: Display::Block,
        box_sizing: BoxSizing::ContentBox,
        ..Default::default()
    };
    if id == ROOT_NODE {
        if tree.computed(id, env).keys().any(|property| {
            !matches!(
                property,
                CssProperty::FontSize
                    | CssProperty::LineHeight
                    | CssProperty::FontFamily
                    | CssProperty::FontWeight
                    | CssProperty::FontStyle
                    | CssProperty::WhiteSpace
                    | CssProperty::OverflowWrap
            ) && !paint_only(*property)
        }) {
            return Err(LayoutError::Unsupported(id));
        }
        style.size = Size {
            width: length(viewport.0),
            height: length(viewport.1),
        };
    } else {
        if matches!(
            element.tag,
            Tag::Button | Tag::Input | Tag::Textarea | Tag::Select
        ) {
            let properties = layout_properties(tree, env, id)?;
            let computed = tree.computed(id, env);
            let content_sizing = element.tag == Tag::Textarea
                && computed
                    .get(&CssProperty::FieldSizing)
                    .is_some_and(|value| value == "content");
            // UA control styles and intrinsic sizes vary across platforms. Only
            // accept controls whose box metrics are explicitly defined.
            if [
                CssProperty::Display,
                CssProperty::Width,
                CssProperty::Height,
                CssProperty::MinWidth,
                CssProperty::MinHeight,
                CssProperty::PaddingTop,
                CssProperty::PaddingRight,
                CssProperty::PaddingBottom,
                CssProperty::PaddingLeft,
            ]
            .iter()
            .any(|property| {
                !properties.contains_key(property)
                    && !(*property == CssProperty::Height && content_sizing)
            }) || !computed.contains_key(&CssProperty::Border)
                || matches!(
                    properties.get(&CssProperty::Width).map(String::as_str),
                    Some("auto")
                )
                || matches!(
                    properties.get(&CssProperty::Height).map(String::as_str),
                    Some("auto")
                ) && !content_sizing
                || matches!(
                    properties.get(&CssProperty::MinWidth).map(String::as_str),
                    Some("auto")
                )
                || matches!(
                    properties.get(&CssProperty::MinHeight).map(String::as_str),
                    Some("auto")
                )
                || (element.tag == Tag::Input
                    && (!matches!(
                        element.input_type(),
                        "text" | "search" | "password" | "checkbox" | "radio"
                    ) || !node.children.is_empty()))
                || (element.tag == Tag::Textarea && !node.children.is_empty())
            {
                return Err(LayoutError::Unsupported(id));
            }
            if element.tag == Tag::Button
                && !node.children.is_empty()
                && (properties
                    .get(&CssProperty::Display)
                    .is_none_or(|value| value != "flex")
                    || [
                        CssProperty::FontFamily,
                        CssProperty::FontSize,
                        CssProperty::FontWeight,
                        CssProperty::FontStyle,
                        CssProperty::LineHeight,
                        CssProperty::JustifyContent,
                        CssProperty::AlignItems,
                    ]
                    .iter()
                    .any(|property| !computed.contains_key(property)))
            {
                return Err(LayoutError::Unsupported(id));
            }
            style.box_sizing = BoxSizing::BorderBox;
        }
        if element.tag == Tag::P {
            let font_size = if let Some(shape) = text_metrics.get(&id).filter(|s| s.rich.is_some())
            {
                shape.options.size
            } else {
                let [child] = node.children.as_slice() else {
                    return Err(LayoutError::Unsupported(id));
                };
                text_metrics
                    .get(child)
                    .ok_or(LayoutError::Unsupported(id))?
                    .options
                    .size
            };
            // HTML's default paragraph margins are one em vertically.
            style.margin.top = length(font_size);
            style.margin.bottom = length(font_size);
        }
        let computed = layout_properties(tree, env, id)?;
        let widths = border_widths(tree, env, id)?;
        style.border = Rect {
            top: length(widths[0]),
            right: length(widths[1]),
            bottom: length(widths[2]),
            left: length(widths[3]),
        };
        for (property, value) in computed {
            match property {
                CssProperty::Display => {
                    style.display = match value.as_str() {
                        "flex" => Display::Flex,
                        "block" => Display::Block,
                        "grid" => Display::Grid,
                        "none" => Display::None,
                        "inline" if crate::inline::blockified(tree, env, id) => Display::Block,
                        _ => return Err(LayoutError::Unsupported(id)),
                    }
                }
                CssProperty::Border
                | CssProperty::BorderTop
                | CssProperty::BorderRight
                | CssProperty::BorderBottom
                | CssProperty::BorderLeft
                | CssProperty::BorderWidth
                | CssProperty::BorderStyle
                | CssProperty::BorderColor => {}
                CssProperty::FieldSizing
                    if element.tag == Tag::Textarea
                        && matches!(value.as_str(), "fixed" | "content") => {}
                CssProperty::Resize if element.tag == Tag::Textarea && value == "none" => {}
                CssProperty::FontSize
                | CssProperty::LineHeight
                | CssProperty::FontFamily
                | CssProperty::FontWeight
                | CssProperty::FontStyle
                | CssProperty::WhiteSpace => {}
                CssProperty::OverflowWrap => {}
                property if paint_only(property) => {}
                CssProperty::Width => {
                    style.size.width = if value == "auto" {
                        auto()
                    } else {
                        units
                            .length(&value, false)
                            .ok_or(LayoutError::Unsupported(id))?
                    }
                }
                CssProperty::Height => {
                    style.size.height = if value == "auto" {
                        auto()
                    } else {
                        units
                            .length(&value, false)
                            .ok_or(LayoutError::Unsupported(id))?
                    }
                }
                CssProperty::MinWidth => {
                    style.min_size.width = if value == "auto" {
                        auto()
                    } else {
                        units
                            .length(&value, false)
                            .ok_or(LayoutError::Unsupported(id))?
                    }
                }
                CssProperty::MinHeight => {
                    style.min_size.height = if value == "auto" {
                        auto()
                    } else {
                        units
                            .length(&value, false)
                            .ok_or(LayoutError::Unsupported(id))?
                    }
                }
                CssProperty::MaxWidth => {
                    style.max_size.width = if value == "none" {
                        auto()
                    } else {
                        units
                            .length(&value, false)
                            .ok_or(LayoutError::Unsupported(id))?
                    }
                }
                CssProperty::MaxHeight => {
                    style.max_size.height = if value == "none" {
                        auto()
                    } else {
                        units
                            .length(&value, false)
                            .ok_or(LayoutError::Unsupported(id))?
                    }
                }
                CssProperty::PaddingTop => {
                    style.padding.top = units
                        .length(&value, false)
                        .ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::PaddingRight => {
                    style.padding.right = units
                        .length(&value, false)
                        .ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::PaddingBottom => {
                    style.padding.bottom = units
                        .length(&value, false)
                        .ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::PaddingLeft => {
                    style.padding.left = units
                        .length(&value, false)
                        .ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::MarginTop => {
                    style.margin.top = units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::MarginRight => {
                    style.margin.right = units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::MarginBottom => {
                    style.margin.bottom =
                        units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::MarginLeft => {
                    style.margin.left = units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::FlexDirection => {
                    style.flex_direction = match value.as_str() {
                        "row" => FlexDirection::Row,
                        "column" => FlexDirection::Column,
                        "row-reverse" => FlexDirection::RowReverse,
                        "column-reverse" => FlexDirection::ColumnReverse,
                        _ => return Err(LayoutError::Unsupported(id)),
                    }
                }
                CssProperty::GridTemplateColumns => {
                    style.grid_template_columns =
                        grid_tracks(&value, units).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::GridTemplateRows => {
                    style.grid_template_rows =
                        grid_tracks(&value, units).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::GridColumn => {
                    style.grid_column =
                        grid_placement(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::GridRow => {
                    style.grid_row = grid_placement(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::Position => {
                    style.position = match value.as_str() {
                        "static" => Position::Static,
                        "relative" => Position::Relative,
                        "absolute" => Position::Absolute,
                        "fixed" => Position::Fixed,
                        _ => return Err(LayoutError::Unsupported(id)),
                    }
                }
                CssProperty::Overflow => {
                    let overflow = match value.as_str() {
                        "visible" => Overflow::Visible,
                        "hidden" => Overflow::Hidden,
                        "auto" => Overflow::Scroll,
                        _ => return Err(LayoutError::Unsupported(id)),
                    };
                    style.overflow = taffy::geometry::Point {
                        x: overflow,
                        y: overflow,
                    };
                }
                CssProperty::Top => {
                    style.inset.top = units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::Right => {
                    style.inset.right = units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::Bottom => {
                    style.inset.bottom = units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::Left => {
                    style.inset.left = units.margin(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::RowGap => {
                    style.gap.height = units
                        .length(&value, false)
                        .ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::ColumnGap => {
                    style.gap.width = units
                        .length(&value, false)
                        .ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::FlexWrap => {
                    style.flex_wrap = match value.as_str() {
                        "nowrap" => FlexWrap::NoWrap,
                        "wrap" => FlexWrap::Wrap,
                        "wrap-reverse" => FlexWrap::WrapReverse,
                        _ => return Err(LayoutError::Unsupported(id)),
                    }
                }
                CssProperty::JustifyContent => {
                    style.justify_content = match value.as_str() {
                        "start" => JustifyContent::START,
                        "end" => JustifyContent::END,
                        "flex-start" => JustifyContent::FLEX_START,
                        "flex-end" => JustifyContent::FLEX_END,
                        "center" => JustifyContent::CENTER,
                        "space-between" => JustifyContent::SPACE_BETWEEN,
                        "space-around" => JustifyContent::SPACE_AROUND,
                        "space-evenly" => JustifyContent::SPACE_EVENLY,
                        _ => return Err(LayoutError::Unsupported(id)),
                    }
                }
                CssProperty::AlignItems => {
                    style.align_items = alignment(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::AlignSelf => {
                    style.align_self = if value == "auto" {
                        None
                    } else {
                        Some(alignment(&value).ok_or(LayoutError::Unsupported(id))?)
                    }
                }
                CssProperty::FlexGrow => {
                    style.flex_grow = factor(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::FlexShrink => {
                    style.flex_shrink = factor(&value).ok_or(LayoutError::Unsupported(id))?
                }
                CssProperty::FlexBasis => {
                    if value != "auto" {
                        style.flex_basis = units
                            .length(&value, false)
                            .ok_or(LayoutError::Unsupported(id))?
                    }
                }
                _ => return Err(LayoutError::Unsupported(id)),
            }
        }
    }
    if style.display == Display::Grid && node.children.len() > 128 {
        return Err(LayoutError::Limit);
    }
    // Count before descending, so a deep or wide tree is bounded before allocation.
    ids.insert(id, NodeId::from(0u64));
    let children = node
        .children
        .iter()
        .filter(|_| {
            element.tag != Tag::Select && text_metrics.get(&id).is_none_or(|s| s.rich.is_none())
        })
        .map(|child| {
            build(
                tree,
                env,
                engine,
                ids,
                text_metrics,
                (*child, depth + 1),
                viewport,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let node_id = if text_metrics.get(&id).is_some_and(|s| s.rich.is_some())
        || (element.tag == Tag::Textarea
            && tree
                .computed(id, env)
                .get(&CssProperty::FieldSizing)
                .is_some_and(|value| value == "content"))
    {
        let shape = text_metrics.get(&id).ok_or(LayoutError::Unsupported(id))?;
        engine.new_leaf_with_context(style, shape.clone())
    } else {
        engine.new_with_children(style, &children)
    }
    .map_err(|e| LayoutError::Engine(e.to_string()))?;
    ids.insert(id, node_id);
    Ok(node_id)
}

pub(crate) fn boxes(
    tree: &Tree,
    env: &Environment,
    width: f32,
    height: f32,
) -> Result<BTreeMap<u32, BoxRect>, LayoutError> {
    Ok(geometry(tree, env, width, height)?
        .into_iter()
        .map(|(id, value)| (id, value.rect))
        .collect())
}

pub(crate) fn geometry(
    tree: &Tree,
    env: &Environment,
    width: f32,
    height: f32,
) -> Result<BTreeMap<u32, BoxGeometry>, LayoutError> {
    boxes_inner(tree, env, width, height, &BTreeMap::new(), None)
}

pub(crate) fn boxes_with_text(
    tree: &Tree,
    env: &Environment,
    width: f32,
    height: f32,
    measurer: &mut TextMeasurer,
    font_size: f32,
) -> Result<BTreeMap<u32, BoxRect>, LayoutError> {
    Ok(
        geometry_with_text(tree, env, width, height, measurer, font_size)?
            .into_iter()
            .map(|(id, value)| (id, value.rect))
            .collect(),
    )
}

pub(crate) fn geometry_with_text(
    tree: &Tree,
    env: &Environment,
    width: f32,
    height: f32,
    measurer: &mut TextMeasurer,
    font_size: f32,
) -> Result<BTreeMap<u32, BoxGeometry>, LayoutError> {
    Ok(geometry_and_text(tree, env, width, height, measurer, font_size)?.0)
}

pub(crate) fn geometry_and_text(
    tree: &Tree,
    env: &Environment,
    width: f32,
    height: f32,
    measurer: &mut TextMeasurer,
    font_size: f32,
) -> Result<TextGeometry, LayoutError> {
    measurer.with_view_shaping(|measurer| {
        geometry_and_text_inner(tree, env, width, height, measurer, font_size)
    })
}

fn geometry_and_text_inner(
    tree: &Tree,
    env: &Environment,
    width: f32,
    height: f32,
    measurer: &mut TextMeasurer,
    font_size: f32,
) -> Result<TextGeometry, LayoutError> {
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || width > 1_000_000.0
        || height > 1_000_000.0
    {
        return Err(LayoutError::Viewport);
    }
    // Isolated flex/grid text and a single text child of a block can be measured independently.
    // Adjacent text nodes form one anonymous box in CSS and are not handled yet.
    let layout_env = Environment {
        width: f64::from(width),
        ..*env
    };
    let mut pending = vec![(ROOT_NODE, 0usize)];
    let mut metrics = BTreeMap::new();
    let mut inventory = crate::inline::Inventory::default();
    while let Some((id, depth)) = pending.pop() {
        inventory.node(depth)?;
        let node = tree.nodes.get(&id).ok_or(LayoutError::Unsupported(id))?;
        if id != ROOT_NODE
            && matches!(node.content, Content::Element(_))
            && tree
                .computed(id, &layout_env)
                .get(&CssProperty::Display)
                .is_some_and(|value| value.eq_ignore_ascii_case("none"))
        {
            continue;
        }
        if let Content::Element(element) = &node.content
            && element.tag == Tag::Select
        {
            continue;
        }
        if let Some(shape) = crate::inline::collect(
            tree,
            &layout_env,
            (id, depth),
            (measurer, font_size),
            &mut inventory,
        )? {
            metrics.insert(id, shape);
            continue;
        }
        if let Content::Element(element) = &node.content
            && element.tag == Tag::Textarea
            && tree
                .computed(id, &layout_env)
                .get(&CssProperty::FieldSizing)
                .is_some_and(|value| value == "content")
        {
            let mut text = element.value.clone();
            if let Some(preedit) = &element.editor.preedit {
                let start = element.editor.anchor.min(element.editor.focus);
                let end = element.editor.anchor.max(element.editor.focus);
                text.replace_range(start..end, &preedit.text);
            }
            if text.is_empty() {
                text = element
                    .attributes
                    .get(&varta_common::view::Attribute::Placeholder)
                    .cloned()
                    .unwrap_or_else(|| " ".into());
                if text.is_empty() {
                    text.push(' ');
                }
            }
            if text
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t'))
            {
                return Err(LayoutError::Unsupported(id));
            }
            inventory.text(text.len())?;
            let (mut options, families) = text_style(tree, &layout_env, id, font_size, measurer)?;
            options.white_space = WhiteSpaceMode::PreWrap;
            options.break_anywhere = true;
            options.source = TextSource::Field;
            let measured = measurer
                .measure_styled(&text, None, &families, options)
                .map_err(|error| LayoutError::Text(error.to_string()))?;
            metrics.insert(
                id,
                TextShape {
                    text,
                    metrics: measured,
                    options,
                    families,
                    rich: None,
                },
            );
        }
        if let Content::Text(text) = &node.content {
            let parent = node.parent.ok_or(LayoutError::Unsupported(id))?;
            let parent_node = tree
                .nodes
                .get(&parent)
                .ok_or(LayoutError::Unsupported(id))?;
            let parent_style = tree.computed(parent, &layout_env);
            let is_flex_or_grid = parent_style
                .get(&CssProperty::Display)
                .is_some_and(|value| matches!(value.as_str(), "flex" | "grid"));
            let is_simple_block = parent != ROOT_NODE
                && parent_node.children.len() == 1
                && parent_style
                    .get(&CssProperty::Display)
                    .is_none_or(|value| value == "block");
            let adjacent_text = parent_node.children.windows(2).any(|pair| {
                pair.contains(&id)
                    && pair.iter().all(|child| {
                        matches!(
                            tree.nodes.get(child).map(|n| &n.content),
                            Some(Content::Text(_))
                        )
                    })
            });
            if !(is_flex_or_grid || is_simple_block)
                || adjacent_text
                || text.is_empty()
                || text
                    .chars()
                    .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\n' | '\r' | '\u{000c}'))
                || parent_style
                    .get(&CssProperty::AlignItems)
                    .is_some_and(|value| value == "baseline")
            {
                return Err(LayoutError::Unsupported(id));
            }
            inventory.text(text.len())?;
            let (options, families) = text_style(tree, &layout_env, parent, font_size, measurer)?;
            let text = if options.white_space == WhiteSpaceMode::PreWrap {
                // Tab stops and hanging edge spaces are not yet modeled.
                if text.contains('\t')
                    || text
                        .lines()
                        .any(|line| line.starts_with(' ') || line.ends_with(' '))
                {
                    return Err(LayoutError::Unsupported(id));
                }
                text.replace("\r\n", "\n").replace('\r', "\n")
            } else {
                text.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
            };
            if text.is_empty() {
                return Err(LayoutError::Unsupported(id));
            }
            let measured = measurer
                .measure_styled(&text, None, &families, options)
                .map_err(|e| LayoutError::Text(e.to_string()))?;
            metrics.insert(
                id,
                TextShape {
                    text,
                    metrics: measured,
                    options,
                    families,
                    rich: None,
                },
            );
        }
        pending.extend(node.children.iter().map(|child| (*child, depth + 1)));
    }
    let mut boxes = boxes_inner(tree, env, width, height, &metrics, Some(measurer))?;
    let frames = crate::inline::frames(&mut boxes, &metrics, measurer)?;
    Ok((boxes, metrics, frames))
}

fn boxes_inner(
    tree: &Tree,
    env: &Environment,
    width: f32,
    height: f32,
    text_metrics: &BTreeMap<u32, TextShape>,
    mut text_input: Option<&mut TextMeasurer>,
) -> Result<BTreeMap<u32, BoxGeometry>, LayoutError> {
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || width > 1_000_000.0
        || height > 1_000_000.0
    {
        return Err(LayoutError::Viewport);
    }
    let mut engine = TaffyTree::new();
    engine.disable_rounding();
    let mut ids = BTreeMap::new();
    let layout_env = Environment {
        width: f64::from(width),
        ..*env
    };
    let root = build(
        tree,
        &layout_env,
        &mut engine,
        &mut ids,
        text_metrics,
        (ROOT_NODE, 0),
        (width, height),
    )?;
    // Bound worst-case implicit-grid work as well as explicit track counts.
    // A child may place lines up to 64 in either direction or create implicit
    // tracks through auto placement. Reject excessive aggregate grid area.
    let mut grid_work = 0usize;
    for (id, node_id) in &ids {
        if engine
            .style(*node_id)
            .map_err(|e| LayoutError::Engine(e.to_string()))?
            .display
            == Display::Grid
        {
            let extent = 128usize.saturating_add(tree.nodes[id].children.len());
            grid_work = grid_work.saturating_add(extent.saturating_mul(extent));
            if grid_work > 4_194_304 {
                return Err(LayoutError::Limit);
            }
        }
    }
    let mut measurement_error = None;
    let mut measured_bytes = 0usize;
    engine
        .compute_layout_with_measure(
            root,
            Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            },
            |inputs, _, context, style| {
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, available| {
                        let Some(shape) = context.as_deref() else {
                            return Size {
                                width: known.width.unwrap_or(0.0),
                                height: known.height.unwrap_or(0.0),
                            };
                        };
                        // Taffy's measurement callback cannot return an error.
                        // Once one is recorded, finish its traversal with cheap
                        // fallback sizes: the whole geometry is rejected below.
                        // Do not repeat shaping/analysis for an unusable layout
                        // or replace its first failure with a later diagnostic.
                        if measurement_error.is_some() {
                            return Size {
                                width: known.width.unwrap_or(shape.metrics.width),
                                height: known.height.unwrap_or(shape.metrics.height),
                            };
                        }
                        let target = known.width.unwrap_or(match available.width {
                            AvailableSpace::MinContent => shape.metrics.min_width,
                            AvailableSpace::MaxContent => shape.metrics.width,
                            AvailableSpace::Definite(value) => {
                                if shape.options.white_space == WhiteSpaceMode::PreWrap {
                                    value.max(shape.metrics.min_width)
                                } else {
                                    value.max(shape.metrics.min_width).min(shape.metrics.width)
                                }
                            }
                        });
                        measured_bytes = measured_bytes.saturating_add(shape.text.len());
                        let measured = if measured_bytes > 8 * 1024 * 1024 {
                            measurement_error = Some("text shaping work limit exceeded".to_owned());
                            shape.metrics
                        } else if let Some(measurer) = text_input.as_mut() {
                            match shape.measure(measurer, Some(target.clamp(0.01, 1_000_000.0))) {
                                Ok(metrics) => metrics,
                                Err(error) => {
                                    measurement_error = Some(error.to_string());
                                    shape.metrics
                                }
                            }
                        } else {
                            shape.metrics
                        };
                        Size {
                            width: if shape.rich.is_some() {
                                measured.width
                            } else {
                                known.width.unwrap_or(measured.width)
                            },
                            height: if shape.rich.is_some() {
                                measured.height
                            } else {
                                known.height.unwrap_or(measured.height)
                            },
                        }
                    },
                )
            },
        )
        .map_err(|e| LayoutError::Engine(e.to_string()))?;
    if let Some(error) = measurement_error {
        return Err(LayoutError::Text(error));
    }
    let mut result = BTreeMap::new();
    let viewport = BoxRect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    };
    // Absolute descendants use the containing block's scroll offset, skipping
    // intermediate scrollers that do not establish a containing block.
    let mut pending = vec![(
        ROOT_NODE,
        (0.0, 0.0),
        viewport,
        (0.0, 0.0, 0.0, viewport),
        false,
        0usize,
    )];
    while let Some((id, parent, parent_clip, containing, parent_hidden, depth)) = pending.pop() {
        if depth > 128 || result.len() >= 4096 {
            return Err(LayoutError::Limit);
        }
        let hidden = parent_hidden
            || engine
                .style(*ids.get(&id).ok_or(LayoutError::Unsupported(id))?)
                .map_err(|e| LayoutError::Engine(e.to_string()))?
                .display
                == Display::None;
        let mut next_containing = containing;
        let mut own_scroll = 0.0;
        let mut scroll = None;
        let mut programmatic_scroll = None;
        let mut content = BoxRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        let mut clip = parent_clip;
        let child_clip;
        let rect = if hidden {
            clip = BoxRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            };
            child_clip = clip;
            BoxRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            }
        } else {
            let node_id = ids[&id];
            let layout = engine
                .layout(node_id)
                .map_err(|e| LayoutError::Engine(e.to_string()))?;
            let style = engine
                .style(node_id)
                .map_err(|e| LayoutError::Engine(e.to_string()))?;
            let position = style.position;
            let (origin_x, origin_y) = match position {
                Position::Absolute => {
                    clip = containing.3;
                    (containing.0, containing.1 - containing.2)
                }
                Position::Fixed => {
                    clip = viewport;
                    (0.0, 0.0)
                }
                _ => parent,
            };
            let rect = BoxRect {
                x: origin_x + layout.location.x,
                y: origin_y + layout.location.y,
                width: layout.size.width,
                height: layout.size.height,
            };
            content = BoxRect {
                x: rect.x + layout.border.left + layout.padding.left,
                y: rect.y + layout.border.top + layout.padding.top,
                width: (rect.width
                    - layout.border.left
                    - layout.border.right
                    - layout.padding.left
                    - layout.padding.right)
                    .max(0.0),
                height: (rect.height
                    - layout.border.top
                    - layout.border.bottom
                    - layout.padding.top
                    - layout.padding.bottom)
                    .max(0.0),
            };
            if matches!(style.overflow.y, Overflow::Hidden | Overflow::Scroll) {
                own_scroll = tree
                    .element(id)
                    .map_or(0.0, |element| element.scroll_top as f32)
                    .clamp(0.0, layout.scroll_height());
                let scrollport = BoxRect {
                    x: rect.x + layout.border.left,
                    y: rect.y + layout.border.top,
                    width: (rect.width - layout.border.left - layout.border.right).max(0.0),
                    height: (rect.height - layout.border.top - layout.border.bottom).max(0.0),
                };
                let metrics = ScrollMetrics {
                    top: own_scroll,
                    max: layout.scroll_height(),
                    viewport: scrollport,
                };
                programmatic_scroll = Some(metrics);
                if style.overflow.y == Overflow::Scroll {
                    scroll = Some(metrics);
                }
                child_clip = intersect(clip, scrollport);
            } else {
                child_clip = clip;
            }
            if matches!(
                position,
                Position::Relative | Position::Absolute | Position::Fixed
            ) {
                next_containing = (rect.x, rect.y, own_scroll, child_clip);
            }
            rect
        };
        result.insert(
            id,
            BoxGeometry {
                rect,
                clip,
                content,
                scroll,
                programmatic_scroll,
            },
        );
        if tree
            .element(id)
            .is_some_and(|element| element.tag == Tag::Select)
        {
            // Collapsed options have zero ordinary boxes; popup rows are
            // frame-owned geometry exposed separately by the raster image.
            let mut options = tree.nodes[&id].children.clone();
            while let Some(option) = options.pop() {
                if result.len() >= 4096 {
                    return Err(LayoutError::Limit);
                }
                let zero = BoxRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                };
                result.insert(
                    option,
                    BoxGeometry {
                        rect: zero,
                        clip: zero,
                        content: zero,
                        scroll: None,
                        programmatic_scroll: None,
                    },
                );
                options.extend(tree.nodes[&option].children.iter().copied());
            }
            continue;
        }
        if text_metrics.get(&id).is_some_and(|s| s.rich.is_some()) {
            continue;
        }
        for child in &tree.nodes[&id].children {
            pending.push((
                *child,
                (rect.x, rect.y - own_scroll),
                child_clip,
                next_containing,
                hidden,
                depth + 1,
            ));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{Environment, Units, border_shorthand, grid_tracks};

    #[test]
    fn border_width_ignores_numeric_components_of_color_functions() {
        let units = Units::new(&Environment::default()).unwrap();
        assert_eq!(
            border_shorthand("solid rgb(0, 0, 0)", units),
            Some((3.0, true))
        );
        assert_eq!(
            border_shorthand("4px solid rgb(0, 0, 0)", units),
            Some((4.0, true))
        );
        assert_eq!(border_shorthand("none", units), Some((3.0, false)));
    }

    #[test]
    fn grid_parser_bounds_track_counts_and_numeric_values() {
        let units = Units::new(&Environment::default()).unwrap();
        assert!(grid_tracks("repeat(2, minmax(0, 1fr))", units).is_some());
        assert!(grid_tracks("repeat(65, 1fr)", units).is_none());
        assert!(grid_tracks("1000001px", units).is_none());
        assert!(grid_tracks("-1fr", units).is_none());
    }
}
