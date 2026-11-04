// Varta addition, under the package's Apache-2.0 OR MIT licenses.
//! Typed text scrolling and exact UTF-16 boundary validation; no native calls.
use accesskit::{Action, ActionData, ActionRequest};
use accesskit_consumer::{NodeRef, TextRange, TreeState};

pub(crate) fn exact_range<'a>(
    node: &'a NodeRef<'a>,
    location: usize,
    length: usize,
) -> Option<TextRange<'a>> {
    if !node.supports_text_ranges() {
        return None;
    }
    let end = location.checked_add(length)?;
    let start = node.text_position_from_global_utf16_index(location)?;
    let finish = node.text_position_from_global_utf16_index(end)?;
    if start.to_global_utf16_index() != location || finish.to_global_utf16_index() != end {
        return None;
    }
    let mut range = start.to_degenerate_range();
    range.set_end(finish);
    Some(range)
}
fn request(
    node: &NodeRef,
    tree: &TreeState,
    action: Action,
    data: Option<ActionData>,
) -> Option<ActionRequest> {
    if !node.data().supports_action(action) {
        return None;
    }
    let (target_node, target_tree) = tree.locate_node(node.id())?;
    Some(ActionRequest {
        action,
        target_tree,
        target_node,
        data,
    })
}
pub(crate) fn range_scroll_request(
    node: &NodeRef,
    tree: &TreeState,
    location: usize,
    length: usize,
) -> Option<ActionRequest> {
    let range = exact_range(node, location, length)?;
    request(
        node,
        tree,
        Action::ScrollIntoView,
        Some(ActionData::SetTextSelection(range.to_text_selection())),
    )
}
pub(crate) fn selection_scroll_request(node: &NodeRef, tree: &TreeState) -> Option<ActionRequest> {
    if !node.supports_text_ranges() {
        return None;
    }
    let selection = node.text_selection()?;
    request(
        node,
        tree,
        Action::ScrollIntoView,
        Some(ActionData::SetTextSelection(selection.to_text_selection())),
    )
}

/// AppKit asks for whole vertically visible lines, including horizontally
/// clipped characters. Varta publishes each run's bounds already clipped to
/// the acknowledged field viewport; zero width alone does not hide a line.
pub(crate) fn visible_range(node: &NodeRef) -> Option<std::ops::Range<usize>> {
    if !node.supports_text_ranges() {
        return None;
    }
    let mut offset = 0usize;
    let mut visible: Option<std::ops::Range<usize>> = None;
    let mut missing_bounds = false;
    node.document_range().traverse_text(|run, text| {
        let end = offset + text.chars().map(char::len_utf16).sum::<usize>();
        if let Some(bounds) = run.data().bounds() {
            if bounds.y1 > bounds.y0 {
                if let Some(range) = &mut visible {
                    range.end = end;
                } else {
                    visible = Some(offset..end);
                }
            }
        } else {
            missing_bounds = true;
        }
        offset = end;
        None::<()>
    });
    (!missing_bounds).then(|| visible.unwrap_or(0..0))
}
