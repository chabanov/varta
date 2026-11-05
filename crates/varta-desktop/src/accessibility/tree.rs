//! Platform projection of the shared semantic snapshot and presented geometry.
use accesskit::{
    Action, AriaCurrent, Invalid, Live, Node, NodeId, Orientation, Rect, Role, TreeId, TreeInfo,
    TreeUpdate,
};
use std::collections::BTreeMap;
use varta_view::{
    AccessibilityNode, AccessibilityState, AccessibilityTree, FrameMetadata, NodeIdentity,
};

const WINDOW_NODE: NodeId = NodeId(1);
const OBSERVER_STATUS_NODE: NodeId = NodeId(2);
const OBSERVER_UNAVAILABLE: &str =
    "Accessibility content exceeds this preview's supported complexity.";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Version {
    pub revision: u64,
    pub dimensions: u64,
    pub dark: bool,
    pub scale: u64,
}
#[derive(Clone)]
pub(super) struct Binding {
    pub identity: NodeIdentity,
    pub version: Version,
    pub focusable: bool,
    pub clickable: bool,
    pub editable: bool,
    pub selectable: bool,
    pub text_revision: Option<varta_view::AccessibilityTextRevision>,
    pub scrollable: bool,
    pub into_view: bool,
    pub scroll_scale: f64,
    pub text_reveal: bool,
    pub text_range: Option<std::ops::Range<usize>>,
}
impl Binding {
    pub fn allows(&self, action: &varta_view::AccessibilityAction) -> bool {
        use varta_view::AccessibilityAction as A;
        match action {
            A::Scroll(varta_view::AccessibilityScrollAction::TextIntoView { revision, .. }) => {
                self.text_reveal && self.text_revision.as_ref() == Some(revision)
            }
            A::Scroll(
                varta_view::AccessibilityScrollAction::IntoView
                | varta_view::AccessibilityScrollAction::IntoViewHint(_),
            ) => self.into_view,
            A::Scroll(_) => self.scrollable,
            A::Focus => self.focusable,
            A::Click => self.clickable,
            A::SetValue(_) | A::ReplaceSelectedText(_) => self.editable,
            A::SelectText { revision, .. } => {
                self.selectable && self.text_revision.as_ref() == Some(revision)
            }
        }
    }
}
struct Slot {
    identity: NodeIdentity,
    native: NodeId,
    text: Option<super::text::TextSlot>,
    /// The vertical scroll-bar child of a scroll area, kept while its source
    /// identity and scroll range remain.
    scroll_bar: Option<NodeId>,
}
/// A scroll bar's position range, as AppKit's `NSScroller` reports it. The
/// pinned macOS adapter routes its increment, decrement and settable position
/// to the owner's item and absolute scroll actions.
const SCROLL_BAR_MIN: f64 = 0.0;
const SCROLL_BAR_MAX: f64 = 1.0;
pub(super) struct Projection {
    slots: BTreeMap<u32, Slot>,
    next: u64,
}
impl Default for Projection {
    fn default() -> Self {
        Self {
            slots: BTreeMap::new(),
            next: OBSERVER_STATUS_NODE.0 + 1,
        }
    }
}
pub(super) struct Publication {
    pub update: TreeUpdate,
    pub bindings: BTreeMap<NodeId, Binding>,
    pub text_runs: BTreeMap<NodeId, super::text::RunBinding>,
}
#[derive(Debug)]
pub(super) enum ProjectionError {
    IdentityExhausted,
    Observer(varta_view::AccessibilityError),
}
impl Projection {
    pub fn clear(&mut self) {
        self.slots.clear();
    }
    /// A current explicit observer failure replaces old guest nodes and actions.
    /// Native identity allocation remains monotonic across later recovery.
    pub fn unavailable(&mut self, metadata: &FrameMetadata, title: &str) -> Publication {
        self.clear();
        let mut window = Node::new(Role::Window);
        window.set_label(title);
        window.set_children([OBSERVER_STATUS_NODE]);
        // The native window may own its help property; expose a real static
        // text child so assistive clients can read the refusal on every host.
        let mut status = Node::new(Role::Label);
        status.set_value(OBSERVER_UNAVAILABLE);
        window.set_bounds(Rect::new(
            0.0,
            0.0,
            f64::from(metadata.width),
            f64::from(metadata.height),
        ));
        Publication {
            update: TreeUpdate {
                nodes: vec![(WINDOW_NODE, window), (OBSERVER_STATUS_NODE, status)],
                tree: Some(TreeInfo::new(WINDOW_NODE)),
                tree_id: TreeId::ROOT,
                focus: WINDOW_NODE,
            },
            bindings: BTreeMap::new(),
            text_runs: BTreeMap::new(),
        }
    }
    pub fn build(
        &mut self,
        source: &AccessibilityTree,
        metadata: &FrameMetadata,
        version: Version,
        title: &str,
    ) -> Result<Publication, ProjectionError> {
        let mut work = source.observer_work();
        let mut slots = BTreeMap::new();
        for (&id, node) in &source.nodes {
            let native = if let Some(old) = self
                .slots
                .get(&id)
                .filter(|old| old.identity == node.identity)
            {
                old.native
            } else {
                let native = NodeId(self.next);
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or(ProjectionError::IdentityExhausted)?;
                native
            };
            slots.insert(
                id,
                Slot {
                    identity: node.identity.clone(),
                    native,
                    text: None,
                    scroll_bar: None,
                },
            );
        }
        let bounds = metadata.node_bounds_map();
        let mut bindings = BTreeMap::new();
        let mut text_runs = BTreeMap::new();
        let mut nodes = Vec::with_capacity(source.nodes.len() + 1);
        for (&id, source) in &source.nodes {
            let native = slots[&id].native;
            let mut node = project_node(source);
            let scroll = metadata
                .scroll_metrics(id)
                .filter(|s| s.max > 0.0 && s.viewport.height > 0.0);
            let into_view = metadata.scroll_into_view_supported(id);
            if let Some(scroll) = scroll {
                if source.role == "generic" {
                    node.set_role(Role::ScrollView);
                }
                node.set_scroll_y(f64::from(scroll.top) * f64::from(metadata.scale()));
                node.set_scroll_y_min(0.0);
                node.set_scroll_y_max(f64::from(scroll.max) * f64::from(metadata.scale()));
                node.add_action(Action::ScrollDown);
                node.add_action(Action::ScrollUp);
                node.add_action(Action::SetScrollOffset);
            }
            if into_view {
                node.add_action(Action::ScrollIntoView);
            }
            node.set_children(
                source
                    .children
                    .iter()
                    .map(|id| slots[id].native)
                    .collect::<Vec<_>>(),
            );
            // Preserve the resolved accessible name while exposing that its
            // content is already represented by semantic children. The live
            // planner must not announce that derived label a second time.
            if source.name_from_content && !source.children.is_empty() {
                node.set_labelled_by(
                    source
                        .children
                        .iter()
                        .map(|id| slots[id].native)
                        .collect::<Vec<_>>(),
                );
            }
            if let Some(rect) = bounds.get(&id) {
                node.set_bounds(Rect::new(
                    f64::from(rect.x),
                    f64::from(rect.y),
                    f64::from(rect.x + rect.width),
                    f64::from(rect.y + rect.height),
                ));
            }
            let rich_text = source
                .text
                .as_ref()
                .filter(|_| super::text::text_role(node.role()));
            let text_reveal = rich_text.is_some_and(|text| {
                metadata.text_reveal_supported(&source.identity, &text.revision)
            });
            if text_reveal {
                node.add_action(Action::ScrollIntoView);
            }
            if rich_text.is_some() {
                let old = self
                    .slots
                    .get(&id)
                    .filter(|old| old.identity == source.identity)
                    .and_then(|old| old.text.as_ref());
                let remaining =
                    varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT.saturating_sub(text_runs.len());
                let text = super::text::project(
                    source,
                    native,
                    old,
                    &mut self.next,
                    metadata,
                    super::text::TextBudget {
                        runs: remaining,
                        work: &mut work,
                    },
                )?;
                node.set_children(text.slot.runs.clone());
                node.set_clips_children();
                if let Some(selection) = text.selection {
                    node.set_text_selection(selection);
                }
                if rich_text.is_some_and(|text| text.selectable) {
                    node.add_action(Action::SetTextSelection);
                }
                slots.get_mut(&id).expect("allocated source slot").text = Some(text.slot);
                nodes.extend(text.nodes);
                text_runs.extend(text.bindings);
            }
            if let Some(scroll) = scroll.filter(|_| rich_text.is_none()) {
                let bar = match self
                    .slots
                    .get(&id)
                    .filter(|old| old.identity == source.identity)
                    .and_then(|old| old.scroll_bar)
                {
                    Some(bar) => bar,
                    None => {
                        let bar = NodeId(self.next);
                        self.next = self
                            .next
                            .checked_add(1)
                            .ok_or(ProjectionError::IdentityExhausted)?;
                        bar
                    }
                };
                node.push_child(bar);
                nodes.push((
                    bar,
                    scroll_bar(f64::from(scroll.top) / f64::from(scroll.max)),
                ));
                slots
                    .get_mut(&id)
                    .expect("allocated source slot")
                    .scroll_bar = Some(bar);
            }
            bindings.insert(
                native,
                Binding {
                    identity: source.identity.clone(),
                    version,
                    focusable: source.focusable,
                    clickable: source.clickable,
                    editable: source.editable,
                    selectable: rich_text.is_some_and(|text| text.selectable),
                    text_revision: rich_text.map(|text| text.revision.clone()),
                    scrollable: scroll.is_some(),
                    into_view,
                    scroll_scale: f64::from(metadata.scale()),
                    text_reveal,
                    text_range: rich_text.map(|text| {
                        text.selection.map_or_else(
                            || 0..text.source_offsets.last().copied().unwrap_or(0),
                            |selection| {
                                selection.anchor.min(selection.focus)
                                    ..selection.anchor.max(selection.focus)
                            },
                        )
                    }),
                },
            );
            nodes.push((native, node));
        }
        let mut window = Node::new(Role::Window);
        window.set_label(title);
        window.set_children([slots[&source.root].native]);
        window.set_bounds(Rect::new(
            0.0,
            0.0,
            f64::from(metadata.width),
            f64::from(metadata.height),
        ));
        nodes.push((WINDOW_NODE, window));
        let focus = source
            .focus
            .and_then(|id| slots.get(&id))
            .map_or(slots[&source.root].native, |slot| slot.native);
        self.slots = slots;
        let mut tree = TreeInfo::new(WINDOW_NODE);
        tree.toolkit_name = Some("Varta".into());
        tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
        Ok(Publication {
            update: TreeUpdate {
                nodes,
                tree: Some(tree),
                tree_id: TreeId::ROOT,
                focus,
            },
            bindings,
            text_runs,
        })
    }
}

/// A geometry-free vertical scroll bar at `position` in its 0..=1 range;
/// Varta paints no scroll bar, so the node has no bounds of its own.
fn scroll_bar(position: f64) -> Node {
    let mut bar = Node::new(Role::ScrollBar);
    bar.set_orientation(Orientation::Vertical);
    bar.set_min_numeric_value(SCROLL_BAR_MIN);
    bar.set_max_numeric_value(SCROLL_BAR_MAX);
    bar.set_numeric_value(position.clamp(SCROLL_BAR_MIN, SCROLL_BAR_MAX));
    bar
}

fn role(name: &str) -> Option<Role> {
    Some(match name {
        "application" => Role::Application,
        "generic" => Role::GenericContainer,
        "text" => Role::Label,
        "textbox" => Role::TextInput,
        "searchbox" => Role::SearchInput,
        "spinbutton" => Role::NumberInput,
        "combobox" => Role::ComboBox,
        "option" => Role::ListBoxOption,
        "button" => Role::Button,
        "checkbox" => Role::CheckBox,
        "radio" => Role::RadioButton,
        "link" => Role::Link,
        "heading" => Role::Heading,
        "list" => Role::List,
        "listitem" => Role::ListItem,
        "navigation" => Role::Navigation,
        "main" => Role::Main,
        "banner" => Role::Banner,
        "contentinfo" => Role::ContentInfo,
        "complementary" => Role::Complementary,
        "form" => Role::Form,
        "paragraph" => Role::Paragraph,
        "dialog" => Role::Dialog,
        "progressbar" => Role::ProgressIndicator,
        "image" => Role::Image,
        "region" => Role::Region,
        "table" => Role::Table,
        "row" => Role::Row,
        "cell" => Role::Cell,
        "columnheader" => Role::ColumnHeader,
        "alert" => Role::Alert,
        "status" => Role::Status,
        "tab" => Role::Tab,
        "tablist" => Role::TabList,
        "tabpanel" => Role::TabPanel,
        _ => return None,
    })
}
fn project_node(source: &AccessibilityNode) -> Node {
    let role = if source.role == "textbox" && source.password {
        Role::PasswordInput
    } else if source.role == "textbox" && source.multiline {
        Role::MultilineTextInput
    } else {
        role(&source.role).unwrap_or(Role::GenericContainer)
    };
    let mut node = Node::new(role);
    if role == Role::Label {
        node.set_value(source.name.as_str());
    } else if !source.name.is_empty() {
        node.set_label(source.name.as_str());
    }
    if let Some(id) = &source.author_id {
        node.set_html_id(id.as_str());
    }
    if let Some(description) = &source.description {
        node.set_description(description.as_str());
    }
    if let Some(value) = &source.value {
        node.set_value(value.as_str());
    }
    // AccessKit levels are zero-based; the shared ARIA/headless levels are not.
    if let Some(level) = source.level {
        node.set_level(usize::from(level.saturating_sub(1)));
    }
    if source.disabled {
        node.set_disabled();
    }
    if source.read_only {
        node.set_read_only();
    }
    if source.modal {
        node.set_modal();
    }
    if source.busy.as_deref() == Some("true") {
        node.set_busy();
    }
    if let Some(value) = source.checked {
        node.set_toggled(value.into());
    }
    if let Some(value) = &source.selected {
        node.set_selected(match value {
            AccessibilityState::Boolean(value) => *value,
            AccessibilityState::Token(value) => value == "true",
        });
    }
    if let Some(value) = &source.expanded {
        node.set_expanded(value == "true");
    }
    if source
        .invalid
        .as_deref()
        .is_some_and(|value| value != "false")
    {
        node.set_invalid(Invalid::True);
    }
    if let Some(value) = &source.current {
        node.set_aria_current(match value.as_str() {
            "false" => AriaCurrent::False,
            "page" => AriaCurrent::Page,
            "step" => AriaCurrent::Step,
            "location" => AriaCurrent::Location,
            "date" => AriaCurrent::Date,
            "time" => AriaCurrent::Time,
            _ => AriaCurrent::True,
        });
    }
    if source.live_atomic {
        node.set_live_atomic();
    }
    if let Some(value) = &source.live {
        node.set_live(match value.as_str() {
            "polite" => Live::Polite,
            "assertive" => Live::Assertive,
            _ => Live::Off,
        });
    }
    if let Some(value) = &source.url {
        node.set_url(value.as_str());
    }
    if source.focusable {
        node.add_action(Action::Focus);
    }
    if source.clickable {
        node.add_action(Action::Click);
    }
    if source.editable {
        node.add_action(Action::SetValue);
        node.add_action(Action::ReplaceSelectedText);
    }
    node
}

#[cfg(test)]
mod tests {
    use super::super::tests::commit;
    use super::*;
    use varta_common::view::{Attribute as A, Tag};
    use varta_view::{BatchWriter, View};
    fn version() -> Version {
        Version {
            revision: 1,
            dimensions: 80u64 << 32 | 60,
            dark: false,
            scale: 1.0f64.to_bits(),
        }
    }
    #[test]
    fn native_roles_cover_all_admitted_explicit_and_implicit_roles() {
        for name in varta_common::view::ROLES {
            assert!(role(name).is_some(), "{name}");
        }
        for tag in Tag::ALL {
            for kind in varta_common::view::INPUT_TYPES {
                if let Some(name) =
                    varta_common::view::accessibility::implicit_role(*tag, kind, true)
                {
                    assert!(role(name).is_some(), "{tag:?}: {kind}: {name}");
                }
            }
        }
    }
    #[test]
    fn ids_survive_updates_but_never_source_replacement_or_deactivation() {
        let mut view = View::default();
        let mut batch = BatchWriter::new();
        batch.create_element(2, Tag::Button).insert(1, 2, 0);
        commit(&mut view, &batch);
        // Semantic/action tests do not need a font-backed control renderer.
        let metadata = View::default()
            .rasterize_boxes(80, 60)
            .unwrap()
            .into_metadata();
        let mut projection = Projection::default();
        let first = projection
            .build(
                &view.accessibility_tree().unwrap(),
                &metadata,
                version(),
                "test",
            )
            .unwrap();
        let button = first
            .update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::Button)
            .unwrap()
            .0;
        let next = projection
            .build(
                &view.accessibility_tree().unwrap(),
                &metadata,
                version(),
                "test",
            )
            .unwrap();
        assert!(next.bindings.contains_key(&button));
        let mut batch = BatchWriter::new();
        batch
            .remove(2)
            .create_element(2, Tag::Button)
            .insert(1, 2, 0);
        commit(&mut view, &batch);
        let replaced = projection
            .build(
                &view.accessibility_tree().unwrap(),
                &metadata,
                version(),
                "test",
            )
            .unwrap();
        assert!(!replaced.bindings.contains_key(&button));
        let ids: Vec<_> = replaced.bindings.keys().copied().collect();
        projection.clear();
        let activated = projection
            .build(
                &view.accessibility_tree().unwrap(),
                &metadata,
                version(),
                "test",
            )
            .unwrap();
        assert!(ids.iter().all(|id| !activated.bindings.contains_key(id)));
    }
    #[test]
    fn structure_headings_lists_and_items_publish_levels_roles_and_frame_bounds() {
        use varta_common::view::CssProperty as P;
        let mut view = View::default();
        let mut batch = BatchWriter::new();
        batch.create_element(2, Tag::Div).insert(1, 2, 0);
        for (property, value) in [
            (P::FontFamily, "\"Varta Structure Fixture\""),
            (P::FontSize, "10px"),
            (P::LineHeight, "10px"),
        ] {
            batch.set_style(2, property, value);
        }
        batch
            .create_element(3, Tag::H3)
            .create_text(4, "A")
            .insert(3, 4, 0)
            .insert(2, 3, 0)
            .create_element(5, Tag::Ol)
            .insert(2, 5, 0)
            .create_element(6, Tag::Li)
            .create_text(7, "B")
            .insert(6, 7, 0)
            .insert(5, 6, 0);
        commit(&mut view, &batch);
        let mut font = varta_view::TextMeasurer::new_with_fonts(
            [
                include_bytes!("../../../varta-view/tests/fixtures/structure-400.ttf").as_slice(),
                include_bytes!("../../../varta-view/tests/fixtures/structure-700.ttf").as_slice(),
            ]
            .into_iter()
            .map(|data| varta_view::TextFont {
                data: data.to_vec(),
                family: "Varta Structure Fixture".into(),
                aliases: Vec::new(),
            })
            .collect(),
        )
        .unwrap();
        let geometry = view
            .layout_boxes_with_text(80.0, 60.0, &mut font, 16.0)
            .unwrap();
        let metadata = view
            .rasterize_with_text(80, 60, &mut font, 16.0)
            .unwrap()
            .into_metadata();
        let publication = Projection::default()
            .build(
                &view.accessibility_tree().unwrap(),
                &metadata,
                version(),
                "test",
            )
            .unwrap();
        let find = |role| {
            &publication
                .update
                .nodes
                .iter()
                .find(|(_, node)| node.role() == role)
                .unwrap()
                .1
        };
        let heading = find(Role::Heading);
        assert_eq!(heading.level(), Some(2));
        let bounds = heading.bounds().unwrap();
        assert_eq!((bounds.x0, bounds.x1), (0.0, 80.0));
        assert!(bounds.y0 <= f64::from(geometry[&3].y));
        let list = find(Role::List).bounds().unwrap();
        assert_eq!(list.y0, f64::from(geometry[&5].y));
        // The item's frame spans its box and its outside "1. " marker.
        let item = find(Role::ListItem).bounds().unwrap();
        assert!(item.x0 < f64::from(geometry[&6].x));
        assert_eq!(item.x1, f64::from(geometry[&6].x + geometry[&6].width));
    }
    #[test]
    fn native_control_states_actions_heading_levels_and_geometry_match_source() {
        let mut view = View::default();
        let mut batch = BatchWriter::new();
        batch
            .create_element(2, Tag::Input)
            .set_attribute(2, A::Type, "password")
            .set_attribute(2, A::AriaLabel, "Secret")
            .set_attribute(2, A::AriaDescribedby, "help")
            .set_value(2, "abc", 0)
            .insert(1, 2, 0)
            .create_element(3, Tag::Input)
            .set_attribute(3, A::Type, "checkbox")
            .set_attribute(3, A::Disabled, "")
            .insert(1, 3, 0)
            .create_element(4, Tag::H2)
            .insert(1, 4, 0)
            .create_element(5, Tag::Span)
            .set_attribute(5, A::Id, "help")
            .set_attribute(5, A::AriaHidden, "true")
            .insert(1, 5, 0)
            .create_text(6, "Keep private")
            .insert(5, 6, 0);
        commit(&mut view, &batch);
        // Semantic/action tests do not need a font-backed control renderer.
        let metadata = View::default()
            .rasterize_boxes(80, 60)
            .unwrap()
            .into_metadata();
        let publication = Projection::default()
            .build(
                &view.accessibility_tree().unwrap(),
                &metadata,
                version(),
                "test",
            )
            .unwrap();
        let find = |role| {
            &publication
                .update
                .nodes
                .iter()
                .find(|(_, node)| node.role() == role)
                .unwrap()
                .1
        };
        let password = find(Role::PasswordInput);
        assert_eq!(password.value(), Some("•••"));
        assert_eq!(password.label(), Some("Secret"));
        assert_eq!(password.description(), Some("Keep private"));
        assert!(password.supports_action(Action::SetValue));
        let check = find(Role::CheckBox);
        assert!(check.is_disabled());
        assert!(!check.supports_action(Action::Click));
        assert_eq!(find(Role::Heading).level(), Some(1));
        assert_eq!(
            find(Role::Window).bounds(),
            Some(Rect::new(0.0, 0.0, 80.0, 60.0))
        );
    }
}
