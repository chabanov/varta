// Varta addition, under the package's Apache-2.0 OR MIT licenses.
//! A clip-aware hit test; no native calls.
//!
//! `accesskit_consumer`'s `node_at_point` returns the deepest filtered node
//! whose bounds contain the point and ignores `clips_children`. A producer that
//! publishes the layout boxes of scrolled-out or clipped-away content (so that
//! assistive clients can see where it is) would then answer a point over
//! visible content with an invisible node. This search is the same, except
//! that a node which clips its children has its descendants searched only
//! where the point lies inside its own bounds. A clipping node without bounds
//! has no area to clip to and is searched as before.
use accesskit::Point;
use accesskit_consumer::{FilterResult, NodeRef};

/// The deepest filtered node, either `root` or a descendant, at `point` in
/// `root`'s coordinate space, outside no clipping ancestor's bounds.
pub(crate) fn node_at_point<'a>(
    root: &NodeRef<'a>,
    point: Point,
    filter: &impl Fn(&NodeRef) -> FilterResult,
) -> Option<NodeRef<'a>> {
    // As upstream: a node's `Test` frame is pushed before its children, then
    // children in forward order, so children are searched last-to-first and
    // the node's own bounds are tested only after all descendants miss.
    enum Frame<'n> {
        Visit(NodeRef<'n>, Point),
        Test(NodeRef<'n>, Point),
    }
    let mut stack = vec![Frame::Visit(*root, point)];
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Test(node, point) => {
                if node.raw_bounds().is_some_and(|rect| rect.contains(point)) {
                    return Some(node);
                }
            }
            Frame::Visit(node, point) => {
                let filter_result = filter(&node);
                if filter_result == FilterResult::ExcludeSubtree {
                    continue;
                }
                if filter_result == FilterResult::Include {
                    stack.push(Frame::Test(node, point));
                }
                let clipped = node.clips_children()
                    && node.raw_bounds().is_some_and(|rect| !rect.contains(point));
                if clipped {
                    continue;
                }
                for child in node.children() {
                    let child_point = child.direct_transform().inverse() * point;
                    stack.push(Frame::Visit(child, child_point));
                }
            }
        }
    }
    None
}
