use super::{geometry_tests::render, live_planner, tests::commit, *};
use accesskit::{Live, Role};
use accesskit_consumer::Tree;
use varta_common::view::{Attribute as A, Tag};
use varta_view::{BatchWriter as B, View};
struct IgnoreChanges;
impl accesskit_consumer::TreeChangeHandler for IgnoreChanges {
    fn node_added(&mut self, _: &accesskit_consumer::NodeRef) {}
    fn node_updated(&mut self, _: &accesskit_consumer::NodeRef, _: &accesskit_consumer::NodeRef) {}
    fn focus_moved(
        &mut self,
        _: Option<&accesskit_consumer::NodeRef>,
        _: Option<&accesskit_consumer::NodeRef>,
    ) {
    }
    fn node_removed(&mut self, _: &accesskit_consumer::NodeRef) {}
}
struct Session {
    bridge: Bridge,
    view: View,
    tree: Tree,
    live: live_planner::State,
    sequence: u64,
}
impl Session {
    fn new(batch: &B) -> Self {
        let mut view = View::default();
        commit(&mut view, batch);
        let bridge = Bridge::new(
            None,
            Arc::default(),
            Arc::new(AtomicBool::new(false)),
            crate::shell::window::Console::SILENT,
        );
        bridge.activate();
        let update = bridge
            .publish(
                1,
                &view.accessibility_tree(),
                &render(&view),
                version(1),
                "live",
            )
            .unwrap();
        let tree = Tree::new(update, false);
        let live = live_planner::State::new(tree.state());
        Self {
            bridge,
            view,
            tree,
            live,
            sequence: 1,
        }
    }
    fn update(&mut self, batch: &B) -> Vec<(String, Live)> {
        commit(&mut self.view, batch);
        self.sequence += 1;
        let update = self
            .bridge
            .publish(
                self.sequence,
                &self.view.accessibility_tree(),
                &render(&self.view),
                version(self.sequence),
                "live",
            )
            .unwrap();
        self.tree
            .update_and_process_changes(update, &mut IgnoreChanges);
        self.live.update(self.tree.state())
    }
}
fn version(revision: u64) -> Version {
    Version {
        revision,
        dimensions: 200u64 << 32 | 160,
        dark: false,
        scale: 1.0f64.to_bits(),
    }
}
fn status() -> B {
    let mut b = B::new();
    b.stylesheet(1,":root {font-family:\"Varta Password Fixture\";font-size:20px;font-weight:400;font-style:normal;line-height:20px}.field {display:block;width:80px;height:40px;min-width:0;min-height:0;padding:4px;border:1px solid #000;border-radius:0;outline:none;background-color:#fff;color:#000;font-family:\"Varta Password Fixture\";font-size:20px;font-weight:400;font-style:normal;line-height:20px}")
        .create_element(2,Tag::Div).set_attribute(2,A::Role,"status").insert(1,2,0)
        .create_text(3,"Saved").insert(2,3,0).create_element(4,Tag::Span).insert(2,4,0).create_text(5,"one").insert(4,5,0);
    b
}
#[test]
fn status_updates_are_atomic_and_only_changed_presented_text_is_announced() {
    let mut session = Session::new(&status());
    let mut b = B::new();
    b.set_text(3, "Saved twice").set_text(5, "two");
    assert_eq!(
        session.update(&b),
        [("Saved twice two".into(), Live::Polite)]
    );
    assert!(session.update(&B::new()).is_empty());
    let mut b = B::new();
    b.set_text(3, "Saved").set_text(5, "one");
    assert_eq!(session.update(&b), [("Saved one".into(), Live::Polite)]);
    let mut b = B::new();
    b.remove(2);
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .set_attribute(2, A::Role, "alert")
        .insert(1, 2, 0)
        .create_text(3, "Failed")
        .insert(2, 3, 0);
    assert_eq!(session.update(&b), [("Failed".into(), Live::Assertive)]);
}
#[test]
fn busy_ancestors_and_children_hold_only_final_atomic_content_without_replaying_repaints() {
    for busy in [1, 2, 4] {
        let mut session = Session::new(&status());
        let mut b = B::new();
        b.set_attribute(busy, A::AriaBusy, "true")
            .set_text(5, "working");
        assert!(session.update(&b).is_empty());
        let mut b = B::new();
        b.set_text(3, "Ready").set_text(5, "final");
        assert!(session.update(&b).is_empty());
        assert!(session.update(&B::new()).is_empty());
        let mut b = B::new();
        b.set_attribute(busy, A::AriaBusy, "false");
        assert_eq!(session.update(&b), [("Ready final".into(), Live::Polite)]);
        assert!(session.update(&B::new()).is_empty());
    }
}
#[test]
fn nested_live_channels_trigger_the_nearest_atomic_region_and_hidden_text_is_excluded() {
    let mut b = status();
    b.create_element(6, Tag::Span)
        .set_attribute(6, A::AriaLive, "assertive")
        .insert(2, 6, 0)
        .create_text(7, "Alarm")
        .insert(6, 7, 0)
        .create_element(8, Tag::Span)
        .set_attribute(8, A::AriaLive, "off")
        .insert(2, 8, 0)
        .create_text(9, "Off")
        .insert(8, 9, 0)
        .create_element(10, Tag::Span)
        .set_attribute(10, A::AriaHidden, "true")
        .insert(2, 10, 0)
        .create_text(11, "Hidden")
        .insert(10, 11, 0);
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.set_text(7, "Urgent")
        .set_text(9, "ignored")
        .set_text(11, "secret");
    assert_eq!(
        session.update(&b),
        [("Saved one Urgent ignored".into(), Live::Assertive)]
    );
    let mut b = B::new();
    b.set_text(5, "two");
    assert_eq!(
        session.update(&b),
        [("Saved two Urgent ignored".into(), Live::Polite)]
    );
}
#[test]
fn implicit_role_defaults_yield_to_explicit_live_ancestors_and_off_can_be_reenabled() {
    let mut b = status();
    b.set_attribute(1, A::AriaLive, "off");
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.set_text(5, "silent");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_attribute(2, A::AriaLive, "assertive");
    assert_eq!(
        session.update(&b),
        [("Saved silent".into(), Live::Assertive)]
    );
    let mut b = B::new();
    b.set_attribute(2, A::AriaLive, "off").set_text(5, "off");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_attribute(2, A::AriaLive, "polite");
    assert_eq!(session.update(&b), [("Saved off".into(), Live::Polite)]);
}
#[test]
fn initially_busy_regions_and_cancelled_updates_never_announce_obsolete_content() {
    let mut b = status();
    b.set_attribute(2, A::AriaBusy, "true");
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.set_text(5, "final");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.remove_attribute(2, A::AriaBusy);
    assert_eq!(session.update(&b), [("Saved final".into(), Live::Polite)]);
    let mut b = B::new();
    b.set_attribute(2, A::AriaBusy, "true")
        .set_text(5, "working");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(5, "final").remove_attribute(2, A::AriaBusy);
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(3, "").set_text(5, "");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(5, "new");
    assert_eq!(session.update(&b), [("new".into(), Live::Polite)]);
}
#[test]
fn live_password_updates_use_rendered_masks_once_without_source_or_duplicate_runs() {
    let mut b = status();
    b.remove(3)
        .remove(4)
        .create_element(6, Tag::Input)
        .set_class(6, "field")
        .set_attribute(6, A::Type, "password")
        .set_attribute(6, A::AriaLabel, "Key")
        .set_value(6, "Secret", 0)
        .insert(2, 6, 0);
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.set_value(6, "e\u{301}👩\u{200d}💻", 0);
    assert_eq!(session.update(&b), [("Key ••".into(), Live::Polite)]);
    let mut b = B::new();
    b.set_value(6, "AB", 0);
    assert!(session.update(&b).is_empty());
    assert!(session.view.drain().is_empty());
    let state = session.bridge.state.lock().unwrap();
    assert!(
        state
            .latest
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Status && node.is_live_atomic())
    );
    drop(state);
    // Platform context activation primes a new baseline, not a backlog.
    session.live = live_planner::State::new(session.tree.state());
    assert!(session.update(&B::new()).is_empty());
}

#[test]
fn offscreen_live_text_is_announced_but_semantically_hidden_text_is_silent() {
    use accesskit::{Node, Rect, TreeInfo};
    let mut window = Node::new(Role::Window);
    window.set_children([NodeId(2)]);
    window.set_bounds(Rect::new(0.0, 0.0, 100.0, 100.0));
    window.set_clips_children();
    let mut status = Node::new(Role::Status);
    status.set_live(Live::Polite);
    status.set_live_atomic();
    status.set_children([NodeId(3)]);
    status.set_bounds(Rect::new(300.0, 300.0, 400.0, 400.0));
    let mut label = Node::new(Role::Label);
    label.set_value("before");
    label.set_bounds(Rect::new(300.0, 300.0, 400.0, 400.0));
    let mut tree = Tree::new(
        TreeUpdate {
            nodes: vec![
                (NodeId(1), window),
                (NodeId(2), status),
                (NodeId(3), label.clone()),
            ],
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        },
        false,
    );
    let mut state = live_planner::State::new(tree.state());
    label.set_value("offscreen update");
    let update = |label| TreeUpdate {
        nodes: vec![(NodeId(3), label)],
        tree: None,
        tree_id: TreeId::ROOT,
        focus: NodeId(1),
    };
    tree.update_and_process_changes(update(label.clone()), &mut IgnoreChanges);
    assert_eq!(
        state.update(tree.state()),
        [("offscreen update".into(), Live::Polite)]
    );
    label.set_hidden();
    label.set_value("hidden update");
    tree.update_and_process_changes(update(label), &mut IgnoreChanges);
    assert!(state.update(tree.state()).is_empty());
}

#[test]
fn atomic_messages_include_off_text_without_treating_its_changes_as_triggers() {
    let mut b = status();
    b.create_element(6, Tag::Span)
        .set_attribute(6, A::AriaLive, "off")
        .insert(2, 6, 0)
        .create_text(7, "quiet")
        .insert(6, 7, 0);
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.set_text(7, "latest");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(5, "two");
    assert_eq!(
        session.update(&b),
        [("Saved two latest".into(), Live::Polite)]
    );
    let mut b = B::new();
    b.set_attribute(6, A::AriaBusy, "true").set_text(5, "three");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(7, "complete").remove_attribute(6, A::AriaBusy);
    assert_eq!(
        session.update(&b),
        [("Saved three complete".into(), Live::Polite)]
    );
}
#[test]
fn nested_atomic_regions_keep_full_contents_and_only_changed_channels_set_priority() {
    let mut b = status();
    b.create_element(6, Tag::Span)
        .set_attribute(6, A::AriaLive, "assertive")
        .insert(2, 6, 0)
        .create_text(7, "urgent")
        .insert(6, 7, 0)
        .create_element(8, Tag::Span)
        .set_attribute(8, A::Role, "status")
        .insert(2, 8, 0)
        .create_text(9, "inner")
        .insert(8, 9, 0);
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.set_text(5, "two");
    assert_eq!(
        session.update(&b),
        [("Saved two urgent inner".into(), Live::Polite)]
    );
    let mut b = B::new();
    b.set_text(7, "immediate");
    assert_eq!(
        session.update(&b),
        [("Saved two immediate inner".into(), Live::Assertive)]
    );
    let mut b = B::new();
    b.set_text(5, "three").set_text(7, "now");
    assert_eq!(
        session.update(&b),
        [("Saved three now inner".into(), Live::Assertive)]
    );
    let mut b = B::new();
    b.set_text(9, "changed");
    assert_eq!(session.update(&b), [("changed".into(), Live::Polite)]);
    let mut b = B::new();
    b.set_text(5, "four");
    assert_eq!(
        session.update(&b),
        [("Saved four now changed".into(), Live::Polite)]
    );
}
#[test]
fn busy_empty_intermediate_content_keeps_the_delivered_digest_for_cancellation() {
    let mut session = Session::new(&status());
    let mut b = B::new();
    b.set_attribute(2, A::AriaBusy, "true")
        .set_text(3, "")
        .set_text(5, "");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(3, "Saved")
        .set_text(5, "one")
        .remove_attribute(2, A::AriaBusy);
    assert!(session.update(&b).is_empty());
}

#[test]
fn effective_content_roles_do_not_duplicate_named_text_in_atomic_messages() {
    let mut b = status();
    b.set_attribute(4, A::Role, "tab");
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.set_text(5, "two");
    assert_eq!(session.update(&b), [("Saved two".into(), Live::Polite)]);
}

#[test]
fn derived_names_with_semantic_children_are_not_repeated_but_author_labels_remain() {
    let mut b = status();
    b.remove(4)
        .create_element(4, Tag::Span)
        .set_attribute(4, A::Role, "row")
        .insert(2, 4, 0)
        .create_element(5, Tag::Span)
        .set_attribute(5, A::Role, "cell")
        .insert(4, 5, 0)
        .create_text(6, "one")
        .insert(5, 6, 0)
        .create_element(7, Tag::Span)
        .set_attribute(7, A::Role, "cell")
        .insert(4, 7, 0)
        .create_text(8, "two")
        .insert(7, 8, 0);
    let mut session = Session::new(&b);
    assert!(session.view.accessibility_tree().unwrap().nodes[&4].name_from_content);
    let mut b = B::new();
    b.set_text(8, "three");
    assert_eq!(
        session.update(&b),
        [("Saved one three".into(), Live::Polite)]
    );
    let mut b = B::new();
    b.set_attribute(4, A::AriaLabel, "Cells");
    assert_eq!(
        session.update(&b),
        [("Saved Cells one three".into(), Live::Polite)]
    );
    assert!(!session.view.accessibility_tree().unwrap().nodes[&4].name_from_content);
    let mut b = B::new();
    b.remove_attribute(4, A::AriaLabel)
        .set_attribute(4, A::AriaLabelledby, "missing");
    assert_eq!(
        session.update(&b),
        [("Saved one three".into(), Live::Polite)]
    );
    assert!(session.view.accessibility_tree().unwrap().nodes[&4].name_from_content);
    // Equal resolved bytes still change whether the label is authored content.
    let mut b = B::new();
    b.set_attribute(4, A::AriaLabel, "onethree");
    assert_eq!(
        session.update(&b),
        [("Saved onethree one three".into(), Live::Polite)]
    );
}

#[test]
fn partial_removals_are_silent_even_through_busy_cancellation_and_new_text_still_announces() {
    let mut session = Session::new(&status());
    let mut b = B::new();
    b.remove(5);
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(3, "Changed");
    assert_eq!(session.update(&b), [("Changed".into(), Live::Polite)]);
    let mut session = Session::new(&status());
    let mut b = B::new();
    b.set_attribute(2, A::AriaBusy, "true")
        .set_text(3, "working");
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.set_text(3, "Saved").remove(5);
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.remove_attribute(2, A::AriaBusy);
    assert!(session.update(&b).is_empty());
    let mut b = B::new();
    b.create_text(6, "added").insert(4, 6, 0);
    assert_eq!(session.update(&b), [("Saved added".into(), Live::Polite)]);
    // Same-content replacement within one presentation is a coalesced no-op.
    let mut b = B::new();
    b.remove(6).create_text(7, "added").insert(4, 7, 0);
    assert!(session.update(&b).is_empty());
}
#[test]
fn reordered_live_content_announces_at_the_priority_of_the_reordered_parts() {
    let mut b = status();
    b.create_element(6, Tag::Span)
        .set_attribute(6, A::AriaLive, "assertive")
        .insert(2, 6, 0)
        .create_text(7, "urgent")
        .insert(6, 7, 0);
    let mut session = Session::new(&b);
    let mut b = B::new();
    b.insert(2, 5, 3);
    assert_eq!(
        session.update(&b),
        [("one Saved urgent".into(), Live::Polite)]
    );
    let mut b = B::new();
    b.insert(2, 6, 5);
    assert_eq!(
        session.update(&b),
        [("urgent one Saved".into(), Live::Assertive)]
    );
    let mut b = B::new();
    b.remove(6);
    assert!(session.update(&b).is_empty());
    assert!(session.update(&B::new()).is_empty());
}
