//! Lazy native accessibility and bounded ingress from platform callbacks.
#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
mod reveal_tests;
#[cfg(test)]
mod scroll_tests;
mod text;
mod text_layout;
#[cfg(test)]
mod text_tests;
mod tree;
use crate::input_queue::{MAX_PENDING_TEXT_BYTES, WORKER_QUEUE_CAPACITY};
use crate::shell::window::Console;
use accesskit::{
    Action, ActionData, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler,
    NodeId, TreeId, TreeUpdate,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
pub(crate) use tree::Version;
use tree::{Binding, Projection, ProjectionError};
use varta_view::{AccessibilityAction, AccessibilityTree, FrameMetadata, NodeIdentity};
use winit::{
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window,
};

pub(crate) struct Pending {
    native: NodeId,
    pub trace: Option<crate::timing::InputStamp>,
    pub identity: NodeIdentity,
    pub action: AccessibilityAction,
    pub version: Version,
}
impl Pending {
    fn bytes(&self) -> usize {
        match &self.action {
            AccessibilityAction::SetValue(value)
            | AccessibilityAction::ReplaceSelectedText(value) => value.capacity(),
            _ => 0,
        }
    }
}
#[derive(Default)]
struct State {
    projection: Projection,
    bindings: BTreeMap<NodeId, Binding>,
    text_runs: BTreeMap<NodeId, text::RunBinding>,
    latest: Option<TreeUpdate>,
    sequence: Option<u64>,
    queue: VecDeque<Pending>,
    bytes: usize,
    wake_pending: bool,
    observer_warning_emitted: bool,
}
impl State {
    /// Both endpoints must belong to the current owner's immutable source map.
    fn source_selection(
        &self,
        owner: NodeId,
        selection: &accesskit::TextSelection,
    ) -> Option<(varta_view::AccessibilityTextRevision, usize, usize)> {
        let anchor = self.text_runs.get(&selection.anchor.node)?;
        let focus = self.text_runs.get(&selection.focus.node)?;
        if anchor.owner != owner || focus.owner != owner || anchor.revision != focus.revision {
            return None;
        }
        Some((
            anchor.revision.clone(),
            anchor.source_offset(selection.anchor.character_index)?,
            focus.source_offset(selection.focus.character_index)?,
        ))
    }
}
#[derive(Clone)]
pub(crate) struct Bridge {
    state: Arc<Mutex<State>>,
    pub active: Arc<AtomicBool>,
    wake: Arc<crate::wake::Wake>,
    events: Option<EventLoopProxy<crate::GuiEvent>>,
    /// Where observer warnings go, labelled with the running binary.
    console: Console,
}
impl Bridge {
    pub fn new(
        events: Option<EventLoopProxy<crate::GuiEvent>>,
        wake: Arc<crate::wake::Wake>,
        active: Arc<AtomicBool>,
        console: Console,
    ) -> Self {
        Self {
            state: Arc::default(),
            active,
            wake,
            events,
            console,
        }
    }
    pub fn install(
        &self,
        event_loop: &ActiveEventLoop,
        window: &Window,
    ) -> accesskit_winit::Adapter {
        accesskit_winit::Adapter::with_direct_handlers(
            event_loop,
            window,
            Activation(self.clone()),
            Actions(self.clone()),
            Deactivation(self.clone()),
        )
    }
    fn notify(&self, state: &mut State) {
        if state.wake_pending {
            return;
        }
        state.wake_pending = true;
        if let Some(events) = &self.events {
            let _ = events.send_event(crate::GuiEvent::AccessibilityWake);
        }
    }
    pub fn acknowledge(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .wake_pending = false;
    }
    pub fn take(&self) -> Option<Pending> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let pending = state.queue.pop_front()?;
        state.bytes -= pending.bytes();
        Some(pending)
    }
    /// Semantic FIFO requests can survive an intervening acknowledged repaint.
    /// Bind them to that presented frame only if identity and action still match.
    pub fn take_current(&self, version: Version) -> Option<Pending> {
        while let Some(mut pending) = self.take() {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(binding) = state.bindings.get(&pending.native) else {
                continue;
            };
            if binding.identity != pending.identity
                || binding.version != version
                || !binding.allows(&pending.action)
            {
                continue;
            }
            pending.version = version;
            return Some(pending);
        }
        None
    }
    pub fn publish(
        &self,
        sequence: u64,
        tree: &Result<AccessibilityTree, varta_view::AccessibilityError>,
        metadata: &FrameMetadata,
        version: Version,
        title: &str,
    ) -> Option<TreeUpdate> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if !self.active.load(Ordering::Acquire) || state.sequence == Some(sequence) {
            return None;
        }
        let result = match tree {
            Ok(tree) => state.projection.build(tree, metadata, version, title),
            Err(error) => Err(ProjectionError::Observer(*error)),
        };
        let publication = match result {
            Ok(publication) => publication,
            Err(ProjectionError::Observer(error)) => {
                if !state.observer_warning_emitted {
                    self.console.report(error);
                    state.observer_warning_emitted = true;
                }
                state.queue.clear();
                state.bytes = 0;
                state.projection.unavailable(metadata, title)
            }
            Err(ProjectionError::IdentityExhausted) => {
                state.bindings.clear();
                state.text_runs.clear();
                state.latest = None;
                state.queue.clear();
                state.bytes = 0;
                return None;
            }
        };
        state.sequence = Some(sequence);
        state.bindings = publication.bindings;
        state.text_runs = publication.text_runs;
        state.latest = Some(publication.update.clone());
        Some(publication.update)
    }
    fn activate(&self) -> Option<TreeUpdate> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        self.active.store(true, Ordering::Release);
        self.wake.notify();
        self.notify(&mut state);
        state.latest.clone()
    }
    fn deactivate(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        self.active.store(false, Ordering::Release);
        state.latest = None;
        state.sequence = None;
        state.bindings.clear();
        state.text_runs.clear();
        state.projection.clear();
        state.queue.clear();
        state.bytes = 0;
        state.observer_warning_emitted = false;
        self.wake.notify();
        self.notify(&mut state);
    }
    fn enqueue(&self, request: ActionRequest) {
        if request.target_tree != TreeId::ROOT {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if !self.active.load(Ordering::Acquire) || state.queue.len() >= WORKER_QUEUE_CAPACITY {
            return;
        }
        let text_reveal = if request.action == Action::ScrollIntoView {
            state.text_runs.get(&request.target_node)
        } else {
            None
        };
        let native = text_reveal.map_or(request.target_node, |run| run.owner);
        let Some(binding) = state.bindings.get(&native) else {
            return;
        };
        let text = match &request.data {
            Some(ActionData::Value(text)) => text.len(),
            _ => 0,
        };
        if text > varta_common::view::DRAFT_FIELD_VALUE_RESERVATION_BYTES
            || state.bytes.saturating_add(text) > MAX_PENDING_TEXT_BYTES
        {
            return;
        }
        if let Some(ActionData::Value(text)) = &request.data
            && text
                .encode_utf16()
                .take(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
                .count()
                > varta_common::view::DRAFT_MAX_VALUE_BYTES / 2
        {
            return;
        }
        let action = match (request.action, request.data) {
            (Action::ScrollIntoView, data @ (None | Some(ActionData::ScrollHint(_)))) => {
                use varta_view::AccessibilityScrollHint as H;
                let hint = match data {
                    Some(ActionData::ScrollHint(hint)) => match hint {
                        accesskit::ScrollHint::TopLeft => H::TopLeft,
                        accesskit::ScrollHint::BottomRight => H::BottomRight,
                        accesskit::ScrollHint::TopEdge => H::TopEdge,
                        accesskit::ScrollHint::BottomEdge => H::BottomEdge,
                        accesskit::ScrollHint::LeftEdge => H::LeftEdge,
                        accesskit::ScrollHint::RightEdge => H::RightEdge,
                    },
                    _ => H::Nearest,
                };
                let reveal = if let Some(run) = text_reveal {
                    let Some(range) = run.source_range() else {
                        return;
                    };
                    varta_view::AccessibilityScrollAction::TextIntoView {
                        revision: run.revision.clone(),
                        start: range.start,
                        end: range.end,
                        hint,
                    }
                } else if binding.text_reveal {
                    let (Some(revision), Some(range)) =
                        (&binding.text_revision, &binding.text_range)
                    else {
                        return;
                    };
                    varta_view::AccessibilityScrollAction::TextIntoView {
                        revision: revision.clone(),
                        start: range.start,
                        end: range.end,
                        hint,
                    }
                } else if hint == H::Nearest {
                    varta_view::AccessibilityScrollAction::IntoView
                } else {
                    varta_view::AccessibilityScrollAction::IntoViewHint(hint)
                };
                AccessibilityAction::Scroll(reveal)
            }
            (
                direction @ (Action::ScrollUp | Action::ScrollDown),
                None | Some(ActionData::ScrollUnit(accesskit::ScrollUnit::Page)),
            ) => AccessibilityAction::Scroll(if direction == Action::ScrollUp {
                varta_view::AccessibilityScrollAction::Up(varta_view::AccessibilityScrollUnit::Page)
            } else {
                varta_view::AccessibilityScrollAction::Down(
                    varta_view::AccessibilityScrollUnit::Page,
                )
            }),
            (
                direction @ (Action::ScrollUp | Action::ScrollDown),
                Some(ActionData::ScrollUnit(accesskit::ScrollUnit::Item)),
            ) => AccessibilityAction::Scroll(if direction == Action::ScrollUp {
                varta_view::AccessibilityScrollAction::Up(varta_view::AccessibilityScrollUnit::Item)
            } else {
                varta_view::AccessibilityScrollAction::Down(
                    varta_view::AccessibilityScrollUnit::Item,
                )
            }),
            (Action::SetScrollOffset, Some(ActionData::SetScrollOffset(point)))
                if point.x == 0.0 =>
            {
                let Some(top) =
                    varta_view::AccessibilityScrollOffset::new(point.y / binding.scroll_scale)
                else {
                    return;
                };
                AccessibilityAction::Scroll(varta_view::AccessibilityScrollAction::To(top))
            }
            (
                action @ (Action::SetTextSelection | Action::ScrollIntoView),
                Some(ActionData::SetTextSelection(selection)),
            ) => {
                let Some((revision, anchor, focus)) =
                    state.source_selection(request.target_node, &selection)
                else {
                    return;
                };
                if action == Action::ScrollIntoView {
                    AccessibilityAction::Scroll(
                        varta_view::AccessibilityScrollAction::TextIntoView {
                            revision,
                            start: anchor.min(focus),
                            end: anchor.max(focus),
                            hint: varta_view::AccessibilityScrollHint::Nearest,
                        },
                    )
                } else {
                    AccessibilityAction::SelectText {
                        revision,
                        anchor,
                        focus,
                    }
                }
            }
            (Action::Focus, None) => AccessibilityAction::Focus,
            (Action::Click, None) => AccessibilityAction::Click,
            (Action::SetValue, Some(ActionData::Value(text))) => {
                AccessibilityAction::SetValue(text.into_string())
            }
            (Action::ReplaceSelectedText, Some(ActionData::Value(text))) => {
                AccessibilityAction::ReplaceSelectedText(text.into_string())
            }
            _ => return,
        };
        if !binding.allows(&action) {
            return;
        }
        let pending = Pending {
            native,
            trace: crate::timing::InputStamp::now("accessibility"),
            identity: binding.identity.clone(),
            version: binding.version,
            action,
        };
        state.bytes += pending.bytes();
        state.queue.push_back(pending);
        self.notify(&mut state);
    }
}
struct Activation(Bridge);
impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.activate()
    }
}
struct Actions(Bridge);
impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        self.0.enqueue(request);
    }
}
struct Deactivation(Bridge);
impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        self.0.deactivate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::view::Tag;
    use varta_view::{Access, BatchWriter, Broker, View};
    pub(super) struct Deny;
    impl Broker for Deny {
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
    pub(super) fn commit(view: &mut View, batch: &BatchWriter) {
        view.commit(&batch.finish(), false, &mut Deny).unwrap();
    }
    pub(super) fn fixture() -> (Bridge, View, FrameMetadata, Version, NodeId) {
        let bridge = Bridge::new(
            None,
            Arc::default(),
            Arc::new(AtomicBool::new(false)),
            crate::shell::window::Console::SILENT,
        );
        let mut view = View::default();
        let mut batch = BatchWriter::new();
        batch.create_element(2, Tag::Input).insert(1, 2, 0);
        commit(&mut view, &batch);
        // Semantic/action tests do not need a font-backed control renderer.
        let metadata = View::default()
            .rasterize_boxes(80, 60)
            .unwrap()
            .into_metadata();
        let version = Version {
            revision: 1,
            dimensions: 80u64 << 32 | 60,
            dark: false,
            scale: 1.0f64.to_bits(),
        };
        assert!(bridge.activate().is_none());
        let update = bridge
            .publish(1, &view.accessibility_tree(), &metadata, version, "test")
            .unwrap();
        let field = update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == accesskit::Role::TextInput)
            .unwrap()
            .0;
        (bridge, view, metadata, version, field)
    }
    pub(super) fn request(node: NodeId, action: Action, data: Option<ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: node,
            data,
        }
    }
    #[test]
    fn lazy_publication_cached_activation_and_monotonic_ids_after_deactivation() {
        let (bridge, view, metadata, version, field) = fixture();
        assert!(bridge.activate().is_some());
        assert!(
            bridge
                .publish(1, &view.accessibility_tree(), &metadata, version, "test")
                .is_none()
        );
        bridge.enqueue(request(field, Action::Focus, None));
        bridge.deactivate();
        assert!(bridge.take().is_none());
        assert!(
            bridge
                .publish(2, &view.accessibility_tree(), &metadata, version, "test")
                .is_none()
        );
        assert_eq!(bridge.state.lock().unwrap().bytes, 0);
        assert!(bridge.activate().is_none());
        let update = bridge
            .publish(2, &view.accessibility_tree(), &metadata, version, "test")
            .unwrap();
        assert!(update.nodes.iter().all(|(id, _)| *id != field));
        bridge.enqueue(request(field, Action::Focus, None));
        assert!(bridge.take().is_none());
    }
    #[test]
    fn callback_ingress_is_bounded_coalesced_and_releases_its_text_reservations() {
        let (bridge, _, _, version, field) = fixture();
        bridge.acknowledge();
        let text = "€".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2);
        for _ in 0..WORKER_QUEUE_CAPACITY + 2 {
            bridge.enqueue(request(
                field,
                Action::SetValue,
                Some(ActionData::Value(text.clone().into_boxed_str())),
            ));
        }
        {
            let state = bridge.state.lock().unwrap();
            assert_eq!(state.queue.len(), WORKER_QUEUE_CAPACITY);
            assert_eq!(state.bytes, text.len() * WORKER_QUEUE_CAPACITY);
            assert!(state.bytes <= MAX_PENDING_TEXT_BYTES);
            assert!(state.wake_pending);
        }
        let pending = bridge.take().unwrap();
        assert_eq!(pending.version, version);
        assert_eq!(pending.action, AccessibilityAction::SetValue(text.clone()));
        assert_eq!(
            bridge.state.lock().unwrap().bytes,
            text.len() * (WORKER_QUEUE_CAPACITY - 1)
        );
        bridge.acknowledge();
        assert!(!bridge.state.lock().unwrap().wake_pending);
        while bridge.take().is_some() {}
        assert_eq!(bridge.state.lock().unwrap().bytes, 0);
    }
    #[test]
    fn unsupported_unknown_and_oversized_requests_have_no_side_effects() {
        let (bridge, _, _, _, field) = fixture();
        for req in [
            request(NodeId(u64::MAX), Action::Focus, None),
            request(field, Action::ScrollIntoView, None),
            request(field, Action::SetValue, Some(ActionData::NumericValue(3.0))),
            request(
                field,
                Action::SetValue,
                Some(ActionData::Value(
                    "x".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
                        .into_boxed_str(),
                )),
            ),
            request(
                field,
                Action::SetValue,
                Some(ActionData::Value(
                    "x".repeat(varta_common::view::DRAFT_FIELD_VALUE_RESERVATION_BYTES + 1)
                        .into_boxed_str(),
                )),
            ),
        ] {
            bridge.enqueue(req);
            assert!(bridge.take().is_none());
            assert_eq!(bridge.state.lock().unwrap().bytes, 0);
        }
    }

    #[test]
    fn observer_refusal_clears_old_guest_actions_and_recovery_uses_fresh_ids() {
        let (bridge, view, metadata, version, field) = fixture();
        bridge.enqueue(request(
            field,
            Action::SetValue,
            Some(ActionData::Value("queued".into())),
        ));
        let error = varta_view::AccessibilityError::WorkLimit {
            limit: varta_view::ACCESSIBILITY_TEXT_WORK_LIMIT,
        };
        let unavailable = bridge
            .publish(2, &Err(error), &metadata, version, "test")
            .unwrap();
        assert_eq!(unavailable.nodes.len(), 2);
        let (root, node) = &unavailable.nodes[0];
        assert_eq!(node.role(), accesskit::Role::Window);
        let (_, status) = &unavailable.nodes[1];
        assert_eq!(status.role(), accesskit::Role::Label);
        assert!(
            status
                .value()
                .is_some_and(|value| value.contains("supported complexity"))
        );
        assert_eq!(unavailable.focus, *root);
        assert!(bridge.take().is_none());
        {
            let state = bridge.state.lock().unwrap();
            assert!(state.bindings.is_empty());
            assert_eq!(state.bytes, 0);
            assert!(state.observer_warning_emitted);
        }
        bridge.enqueue(request(field, Action::Focus, None));
        assert!(bridge.take().is_none());
        let recovered = bridge
            .publish(3, &view.accessibility_tree(), &metadata, version, "test")
            .unwrap();
        assert!(recovered.nodes.iter().all(|(id, _)| *id != field));
        assert!(!bridge.state.lock().unwrap().bindings.contains_key(&field));
    }

    #[test]
    fn focus_then_set_value_survives_the_focus_acknowledgement_frame() {
        let (bridge, view, metadata, version, field) = fixture();
        bridge.enqueue(request(field, Action::Focus, None));
        bridge.enqueue(request(
            field,
            Action::SetValue,
            Some(ActionData::Value("typed".into())),
        ));
        assert_eq!(
            bridge.take_current(version).unwrap().action,
            AccessibilityAction::Focus
        );
        let next = Version {
            revision: version.revision + 1,
            ..version
        };
        bridge
            .publish(2, &view.accessibility_tree(), &metadata, next, "test")
            .unwrap();
        let edit = bridge
            .take_current(next)
            .expect("queued edit survives the focus frame");
        assert_eq!(edit.action, AccessibilityAction::SetValue("typed".into()));
        assert_eq!(edit.version, next);
        assert!(bridge.take_current(next).is_none());
        assert_eq!(bridge.state.lock().unwrap().bytes, 0);
    }

    #[test]
    fn semantic_rebinding_rejects_newly_readonly_and_replaced_targets() {
        use varta_common::view::Attribute as A;
        for replaced in [false, true] {
            let (bridge, mut view, metadata, version, field) = fixture();
            bridge.enqueue(request(
                field,
                Action::SetValue,
                Some(ActionData::Value("stale".into())),
            ));
            let mut batch = BatchWriter::new();
            if replaced {
                batch
                    .remove(2)
                    .create_element(2, Tag::Input)
                    .insert(1, 2, 0);
            } else {
                batch.set_attribute(2, A::Readonly, "");
            }
            commit(&mut view, &batch);
            let next = Version {
                revision: version.revision + 1,
                ..version
            };
            bridge
                .publish(2, &view.accessibility_tree(), &metadata, next, "test")
                .unwrap();
            assert!(bridge.take_current(next).is_none());
            assert_eq!(bridge.state.lock().unwrap().bytes, 0);
        }
    }
}

#[cfg(test)]
#[path = "../../../../vendor/accesskit_macos/src/live.rs"]
mod live_planner;
#[cfg(test)]
mod live_tests;

#[cfg(test)]
mod platform_scroll_tests;
#[cfg(test)]
#[path = "../../../../vendor/accesskit_macos/src/scroll.rs"]
mod scroll_route_planner;
#[cfg(test)]
#[path = "../../../../vendor/accesskit_macos/src/text_scroll.rs"]
mod text_scroll_planner;
