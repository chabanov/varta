//! Positioned Layout 3 sticky view rectangles, applied after static layout.
//! Reuse the normal bounded ancestor chain and leave siblings/scroll size intact.
use super::{BoxGeometry, BoxRect, Tree};
use std::collections::BTreeMap;
use taffy::{geometry::Rect, style::LengthPercentageAuto, util::MaybeResolve};

pub(super) fn position(
    tree: &Tree,
    id: u32,
    mut rect: BoxRect,
    viewport: BoxRect,
    geometry: &BTreeMap<u32, BoxGeometry>,
    inset: Rect<LengthPercentageAuto>,
    margin: Rect<f32>,
) -> BoxRect {
    let path = tree.path(id);
    let scroller = path
        .iter()
        .skip(1)
        .find_map(|id| geometry.get(id)?.programmatic_scroll.map(|s| (*id, s)));
    let port = scroller.map_or(viewport, |(_, s)| s.viewport);
    let parent = path
        .get(1)
        .and_then(|id| geometry.get(id).map(|g| (*id, *g)));
    let mut cb = parent.map_or(viewport, |(_, g)| g.content);
    if let Some((id, scroll)) = scroller
        && parent.is_some_and(|(parent, _)| parent == id)
    {
        cb.y -= scroll.top;
        cb.height = cb.height.max(scroll.viewport.height + scroll.max);
    }
    let resolve =
        |v: LengthPercentageAuto, basis: f32| v.maybe_resolve(Some(basis), crate::calc::resolve);
    rect.x = axis(
        rect.x,
        rect.width,
        [port.x, port.width],
        [cb.x, cb.width],
        [
            resolve(inset.left, port.width),
            resolve(inset.right, port.width),
        ],
        [margin.left, margin.right],
    );
    rect.y = axis(
        rect.y,
        rect.height,
        [port.y, port.height],
        [cb.y, cb.height],
        [
            resolve(inset.top, port.height),
            resolve(inset.bottom, port.height),
        ],
        [margin.top, margin.bottom],
    );
    rect
}
fn axis(
    normal: f32,
    size: f32,
    [port, extent]: [f32; 2],
    [cb, cb_size]: [f32; 2],
    inset: [Option<f32>; 2],
    margin: [f32; 2],
) -> f32 {
    let [start, end] = inset;
    let start_value = start.unwrap_or(0.0);
    // Reduce the end inset when the sticky view rectangle is smaller than
    // the border box; the start edge has priority in the supported LTR axes.
    let end_value = end.unwrap_or(0.0).min(extent - start_value - size);
    let mut result = normal;
    if start.is_some() {
        result = result.max(port + start_value);
    }
    if end.is_some() {
        result = result.min(port + extent - end_value - size);
    }
    let before = margin[0].min(normal - cb);
    let after = margin[1].min(cb + cb_size - normal - size);
    if result > normal {
        result = result.min(cb + cb_size - after - size).max(normal);
    }
    if result < normal {
        result = result.max(cb + before).min(normal);
    }
    result
}
