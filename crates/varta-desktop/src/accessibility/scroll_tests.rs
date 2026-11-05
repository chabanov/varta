use super::{
    tests::{Deny, commit, request},
    *,
};
use accesskit::{Point, Role, ScrollUnit};
use varta_common::view::{Attribute as A, Tag};
use varta_view::{
    AccessibilityScrollAction as Scroll, AccessibilityScrollUnit as Unit, BatchWriter as B, View,
};

fn setup() -> (Bridge, View, FrameMetadata, Version, NodeId, NodeId) {
    let mut view = View::default();
    let mut b = B::new();
    b.stylesheet(
        1,
        ".pane {height:50px;width:100px;overflow:auto}.space {height:120px}.target {height:20px}",
    )
    .create_element(2, Tag::Div)
    .set_class(2, "pane")
    .insert(1, 2, 0)
    .create_element(3, Tag::Div)
    .set_class(3, "space")
    .insert(2, 3, 0)
    .create_element(4, Tag::Section)
    .set_class(4, "target")
    .set_attribute(4, A::AriaLabel, "Target")
    .insert(2, 4, 0);
    commit(&mut view, &b);
    let metadata = view.rasterize_boxes(150, 100).unwrap().into_metadata();
    let bridge = Bridge::new(
        None,
        Arc::default(),
        Arc::new(AtomicBool::new(false)),
        crate::shell::window::Console::SILENT,
    );
    assert!(bridge.activate().is_none());
    let version = Version {
        revision: 1,
        dimensions: 150u64 << 32 | 100,
        dark: false,
        scale: 1.0f64.to_bits(),
    };
    let update = bridge
        .publish(1, &view.accessibility_tree(), &metadata, version, "scroll")
        .unwrap();
    let pane = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::ScrollView)
        .unwrap()
        .0;
    let target = update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Target"))
        .unwrap()
        .0;
    (bridge, view, metadata, version, pane, target)
}

#[test]
fn native_vertical_actions_and_metrics_use_the_bounded_semantic_fifo() {
    let (bridge, mut view, metadata, version, pane, _) = setup();
    {
        let state = bridge.state.lock().unwrap();
        let node = &state
            .latest
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(id, _)| *id == pane)
            .unwrap()
            .1;
        assert_eq!(node.scroll_y(), Some(0.0));
        assert!(node.scroll_y_max().unwrap() > 0.0);
        assert!(node.supports_action(Action::SetScrollOffset));
    }
    bridge.enqueue(request(
        pane,
        Action::ScrollDown,
        Some(ActionData::ScrollUnit(ScrollUnit::Page)),
    ));
    bridge.enqueue(request(
        pane,
        Action::ScrollDown,
        Some(ActionData::ScrollUnit(ScrollUnit::Item)),
    ));
    let first = bridge.take_current(version).unwrap();
    assert_eq!(
        first.action,
        AccessibilityAction::Scroll(Scroll::Down(Unit::Page))
    );
    assert!(
        view.accessibility_input_from_frame(&first.identity, &first.action, &metadata, &mut Deny)
            .unwrap()
    );
    let new_metadata = view.rasterize_boxes(150, 100).unwrap().into_metadata();
    let next = Version {
        revision: 2,
        ..version
    };
    bridge
        .publish(2, &view.accessibility_tree(), &new_metadata, next, "scroll")
        .unwrap();
    let second = bridge.take_current(next).unwrap();
    assert_eq!(
        second.action,
        AccessibilityAction::Scroll(Scroll::Down(Unit::Item))
    );
    assert!(
        view.accessibility_input_from_frame(
            &second.identity,
            &second.action,
            &new_metadata,
            &mut Deny
        )
        .unwrap()
    );
    assert!(bridge.take().is_none());
    assert_eq!(bridge.state.lock().unwrap().bytes, 0);
}
#[test]
fn native_scroll_offsets_use_the_same_physical_coordinate_scale_as_bounds() {
    let (bridge, mut view, _, version, pane, _) = setup();
    let metadata = view
        .rasterize_scaled(300, 200, 2.0, None)
        .unwrap()
        .into_metadata();
    let next = Version {
        revision: 2,
        dimensions: 300u64 << 32 | 200,
        scale: 2.0f64.to_bits(),
        ..version
    };
    bridge
        .publish(2, &view.accessibility_tree(), &metadata, next, "scroll")
        .unwrap();
    {
        let state = bridge.state.lock().unwrap();
        let node = &state
            .latest
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(id, _)| *id == pane)
            .unwrap()
            .1;
        assert_eq!(
            node.scroll_y_max(),
            Some(f64::from(metadata.scroll_metrics(2).unwrap().max) * 2.0)
        );
    }
    bridge.enqueue(request(
        pane,
        Action::SetScrollOffset,
        Some(ActionData::SetScrollOffset(Point::new(0.0, 40.0))),
    ));
    let pending = bridge.take_current(next).unwrap();
    let AccessibilityAction::Scroll(Scroll::To(offset)) = pending.action else {
        panic!("absolute scroll");
    };
    assert_eq!(offset.value(), 20.0);
    assert!(
        view.accessibility_input_from_frame(
            &pending.identity,
            &pending.action,
            &metadata,
            &mut Deny
        )
        .unwrap()
    );
    assert_eq!(
        view.rasterize_boxes(150, 100)
            .unwrap()
            .scroll_metrics(2)
            .unwrap()
            .top,
        20.0
    );
}

#[test]
fn malformed_horizontal_and_nonfinite_offset_payloads_are_rejected_before_effect() {
    let (bridge, _, _, _, pane, _) = setup();
    for (x, y) in [
        (1.0, 10.0),
        (f64::NAN, 10.0),
        (0.0, f64::INFINITY),
        (0.0, f64::NAN),
        (0.0, -1.0),
    ] {
        bridge.enqueue(request(
            pane,
            Action::SetScrollOffset,
            Some(ActionData::SetScrollOffset(Point::new(x, y))),
        ));
        assert!(bridge.take().is_none());
    }
    bridge.enqueue(request(
        pane,
        Action::ScrollDown,
        Some(ActionData::NumericValue(3.0)),
    ));
    assert!(bridge.take().is_none());
    bridge.enqueue(request(
        pane,
        Action::SetScrollOffset,
        Some(ActionData::SetScrollOffset(Point::new(0.0, f64::MAX))),
    ));
    assert!(bridge.take().is_some());
}
#[test]
fn offscreen_element_scroll_does_not_become_activation_and_newly_unsupported_actions_drop() {
    let (bridge, mut view, metadata, version, pane, target) = setup();
    bridge.enqueue(request(target, Action::ScrollIntoView, None));
    let input = bridge.take_current(version).unwrap();
    assert_eq!(input.action, AccessibilityAction::Scroll(Scroll::IntoView));
    assert!(
        view.accessibility_input_from_frame(&input.identity, &input.action, &metadata, &mut Deny)
            .unwrap()
    );
    assert!(view.accessibility_tree().unwrap().focus.is_none());
    let metadata = view.rasterize_boxes(150, 100).unwrap().into_metadata();
    let next = Version {
        revision: 2,
        ..version
    };
    bridge
        .publish(2, &view.accessibility_tree(), &metadata, next, "scroll")
        .unwrap();
    bridge.enqueue(request(pane, Action::ScrollUp, None));
    let mut b = B::new();
    b.set_style(2, varta_common::view::CssProperty::Overflow, "hidden");
    commit(&mut view, &b);
    bridge
        .publish(
            3,
            &view.accessibility_tree(),
            &view.rasterize_boxes(150, 100).unwrap().into_metadata(),
            Version {
                revision: 3,
                ..version
            },
            "scroll",
        )
        .unwrap();
    assert!(
        bridge
            .take_current(Version {
                revision: 3,
                ..version
            })
            .is_none()
    );
}

/// The pinned macOS adapter's exact scroll-route module, applied to Varta's
/// real publication through the AccessKit consumer.
#[test]
fn macos_page_item_and_position_routes_reach_the_typed_core_actions() {
    use super::scroll_route_planner as routes;
    let (bridge, mut view, metadata, version, pane, target) = setup();
    let update = bridge.state.lock().unwrap().latest.clone().unwrap();
    let node = |id| {
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == id)
            .unwrap()
            .1
            .clone()
    };
    let bar = *node(pane).children().last().unwrap();
    let published = node(bar);
    assert_eq!(published.role(), Role::ScrollBar);
    assert_eq!(
        published.orientation(),
        Some(accesskit::Orientation::Vertical)
    );
    assert_eq!(
        (
            published.numeric_value(),
            published.min_numeric_value(),
            published.max_numeric_value()
        ),
        (Some(0.0), Some(0.0), Some(1.0))
    );
    assert!(published.bounds().is_none());
    let tree = accesskit_consumer::Tree::new(update.clone(), true);
    let state = tree.state();
    let area = state.node_by_tree_local_id(pane, TreeId::ROOT).unwrap();
    let scroll_bar = state.node_by_tree_local_id(bar, TreeId::ROOT).unwrap();
    let section = state.node_by_tree_local_id(target, TreeId::ROOT).unwrap();
    assert_eq!(
        routes::page_action_names(&area).collect::<Vec<_>>(),
        [routes::SCROLL_UP_BY_PAGE, routes::SCROLL_DOWN_BY_PAGE]
    );
    assert_eq!(routes::page_action_names(&section).count(), 0);
    assert_eq!(
        routes::vertical_scroll_bar(&area).unwrap().id(),
        scroll_bar.id()
    );
    assert!(routes::vertical_scroll_bar(&section).is_none());
    assert!(routes::routes_items(&scroll_bar) && routes::routes_position(&scroll_bar));
    assert!(!routes::routes_items(&area) && !routes::routes_position(&section));
    assert!(routes::page_request(&area, state, "AXScrollToVisible").is_none());
    assert!(routes::item_request(&area, state, routes::Direction::Down).is_none());
    for position in [f64::NAN, f64::INFINITY] {
        assert!(routes::position_request(&scroll_bar, state, position).is_none());
    }
    let max = area.scroll_y_max().unwrap();
    let requests = [
        routes::page_request(&area, state, routes::SCROLL_DOWN_BY_PAGE).unwrap(),
        routes::page_request(&area, state, routes::SCROLL_UP_BY_PAGE).unwrap(),
        routes::item_request(&scroll_bar, state, routes::Direction::Down).unwrap(),
        routes::item_request(&scroll_bar, state, routes::Direction::Up).unwrap(),
        routes::position_request(&scroll_bar, state, 0.5).unwrap(),
        // Out-of-range positions clamp to the scroll range.
        routes::position_request(&scroll_bar, state, 2.0).unwrap(),
    ];
    assert!(requests.iter().all(|r| r.target_node == pane));
    let expected = [
        Scroll::Down(Unit::Page),
        Scroll::Up(Unit::Page),
        Scroll::Down(Unit::Item),
        Scroll::Up(Unit::Item),
    ];
    let mut version = version;
    let mut metadata = metadata;
    for (index, request) in requests.into_iter().enumerate() {
        bridge.enqueue(request);
        let pending = bridge.take_current(version).unwrap();
        match (index, &pending.action) {
            (0..=3, AccessibilityAction::Scroll(action)) => assert_eq!(*action, expected[index]),
            (4, AccessibilityAction::Scroll(Scroll::To(offset))) => {
                assert_eq!(offset.value(), max / 2.0)
            }
            (5, AccessibilityAction::Scroll(Scroll::To(offset))) => {
                assert_eq!(offset.value(), max)
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(
            view.accessibility_input_from_frame(
                &pending.identity,
                &pending.action,
                &metadata,
                &mut Deny
            )
            .unwrap()
        );
        version.revision += 1;
        metadata = view.rasterize_boxes(150, 100).unwrap().into_metadata();
        bridge
            .publish(
                version.revision,
                &view.accessibility_tree(),
                &metadata,
                version,
                "scroll",
            )
            .unwrap();
    }
    // The bar keeps its native identity across republications and tracks the
    // presented offset: fully scrolled after the last request.
    let update = bridge.state.lock().unwrap().latest.clone().unwrap();
    let (_, bar_node) = update.nodes.iter().find(|(n, _)| *n == bar).unwrap();
    assert_eq!(bar_node.numeric_value(), Some(1.0));
}
