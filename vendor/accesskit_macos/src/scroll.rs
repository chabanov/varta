// Varta addition, under the package's Apache-2.0 OR MIT licenses.
//! Typed routes for AppKit's scroll-area and scroll-bar actions; no native calls.
//!
//! AppKit's own scroll area (`NSScrollView`, role `AXScrollArea`) advertises the
//! legacy actions `AXScrollUpByPage`/`AXScrollDownByPage` (and left/right), and
//! exposes its `AXVerticalScrollBar`, whose `AXValue` is a settable position
//! in 0..=1. A scroll area here is any node supporting AccessKit's vertical
//! scroll actions. Its vertical scroll bar is a vertical `ScrollBar` child the
//! producer publishes with that 0..=1 position. The bar routes increment and
//! decrement (one item, as AX documents a scroll bar's line actions) and its
//! settable value to the area's existing typed actions.
use accesskit::{Action, ActionData, ActionRequest, Orientation, Point, Role, ScrollUnit};
use accesskit_consumer::{NodeRef, TreeState};

/// AppKit's scroll-area page actions, as `NSScrollView` advertises them.
pub(crate) const SCROLL_UP_BY_PAGE: &str = "AXScrollUpByPage";
pub(crate) const SCROLL_DOWN_BY_PAGE: &str = "AXScrollDownByPage";
/// The scroll-bar value range, as `NSScroller` reports it.
const POSITION_MIN: f64 = 0.0;
const POSITION_MAX: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
}
impl Direction {
    fn action(self) -> Action {
        match self {
            Self::Up => Action::ScrollUp,
            Self::Down => Action::ScrollDown,
        }
    }
}

fn request(
    node: &NodeRef,
    tree: &TreeState,
    action: Action,
    data: ActionData,
) -> Option<ActionRequest> {
    if !node.data().supports_action(action) {
        return None;
    }
    let (target_node, target_tree) = tree.locate_node(node.id())?;
    Some(ActionRequest {
        action,
        target_tree,
        target_node,
        data: Some(data),
    })
}

/// The page action names a node supports, in AppKit's order.
pub(crate) fn page_action_names(node: &NodeRef) -> impl Iterator<Item = &'static str> {
    [
        (Direction::Up, SCROLL_UP_BY_PAGE),
        (Direction::Down, SCROLL_DOWN_BY_PAGE),
    ]
    .into_iter()
    .filter(|(direction, _)| node.data().supports_action(direction.action()))
    .map(|(_, name)| name)
}

/// A page scroll for a named AppKit action, if this node supports it.
pub(crate) fn page_request(node: &NodeRef, tree: &TreeState, name: &str) -> Option<ActionRequest> {
    let direction = match name {
        SCROLL_UP_BY_PAGE => Direction::Up,
        SCROLL_DOWN_BY_PAGE => Direction::Down,
        _ => return None,
    };
    request(
        node,
        tree,
        direction.action(),
        ActionData::ScrollUnit(ScrollUnit::Page),
    )
}

fn is_vertical_bar(node: &NodeRef) -> bool {
    node.role() == Role::ScrollBar && node.orientation() == Some(Orientation::Vertical)
}

/// The scroll area a vertical scroll bar controls: its parent, when that
/// parent has a vertical scroll range.
pub(crate) fn scrolled_area<'a>(bar: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    if !is_vertical_bar(bar) {
        return None;
    }
    let area = bar.parent()?;
    (area.scroll_y_max().is_some()
        && [
            Action::ScrollUp,
            Action::ScrollDown,
            Action::SetScrollOffset,
        ]
        .into_iter()
        .any(|action| area.data().supports_action(action)))
    .then_some(area)
}

/// An area's vertical scroll bar child.
pub(crate) fn vertical_scroll_bar<'a>(area: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    area.children()
        .find(|child| scrolled_area(child).is_some_and(|parent| parent.id() == area.id()))
}

/// One item up or down, for the bar's decrement and increment.
pub(crate) fn item_request(
    bar: &NodeRef,
    tree: &TreeState,
    direction: Direction,
) -> Option<ActionRequest> {
    let area = scrolled_area(bar)?;
    request(
        &area,
        tree,
        direction.action(),
        ActionData::ScrollUnit(ScrollUnit::Item),
    )
}

/// An absolute offset for a settable bar position, in the area's own scroll
/// coordinates. Non-finite positions are refused; others are clamped.
pub(crate) fn position_request(
    bar: &NodeRef,
    tree: &TreeState,
    position: f64,
) -> Option<ActionRequest> {
    if !position.is_finite() {
        return None;
    }
    let area = scrolled_area(bar)?;
    let min = area.scroll_y_min().unwrap_or(0.0);
    let max = area.scroll_y_max()?;
    if !min.is_finite() || !max.is_finite() || max < min {
        return None;
    }
    let fraction = position.clamp(POSITION_MIN, POSITION_MAX);
    request(
        &area,
        tree,
        Action::SetScrollOffset,
        ActionData::SetScrollOffset(Point {
            x: area.scroll_x().unwrap_or(0.0),
            y: min + fraction * (max - min),
        }),
    )
}

/// Whether a bar routes increment/decrement or a settable position.
pub(crate) fn routes_items(bar: &NodeRef) -> bool {
    scrolled_area(bar).is_some_and(|area| {
        area.data().supports_action(Action::ScrollUp)
            || area.data().supports_action(Action::ScrollDown)
    })
}
pub(crate) fn routes_position(bar: &NodeRef) -> bool {
    scrolled_area(bar).is_some_and(|area| area.data().supports_action(Action::SetScrollOffset))
}
