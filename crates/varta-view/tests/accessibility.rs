use varta_common::view::{ANY_KEY, Attribute as A, CssProperty as P, EventKind, Tag};
use varta_view::{Access, AccessibilityAction as Action, BatchWriter as B, Broker, Filter, View};
struct Denied {
    opens: usize,
}
impl Broker for Denied {
    fn link_requested(&self, _: &str) -> bool {
        true
    }
    fn open_url(&mut self, _: &str) -> Access {
        self.opens += 1;
        Access::Denied
    }
    fn clipboard_write(&mut self) -> Access {
        Access::Denied
    }
}
fn commit(view: &mut View, batch: &B) {
    view.commit(&batch.finish(), false, &mut Denied { opens: 0 })
        .unwrap();
}

#[test]
fn flat_semantics_preserve_names_hidden_relations_and_password_masking() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .create_element(3, Tag::Span)
        .set_attribute(3, A::Id, "secret-label")
        .set_attribute(3, A::AriaHidden, "true")
        .insert(2, 3, 0)
        .create_text(4, "Hidden label")
        .insert(3, 4, 0)
        .create_element(5, Tag::Input)
        .set_attribute(5, A::AriaLabelledby, "secret-label")
        .set_attribute(5, A::Type, "password")
        .set_value(5, "ab", 0)
        .insert(2, 5, 0)
        .create_element(6, Tag::H2)
        .insert(1, 6, 0)
        .create_text(7, "Heading")
        .insert(6, 7, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let before = view.snapshot();
    let tree = view.accessibility_tree().unwrap();
    assert_eq!(tree.nodes[&1].children, [5, 6]);
    assert!(!tree.nodes.contains_key(&2));
    for id in [3, 4, 7] {
        assert!(!tree.nodes.contains_key(&id));
    }
    let field = &tree.nodes[&5];
    assert_eq!(field.name, "Hidden label");
    assert_eq!(field.value.as_deref(), Some("\u{2022}\u{2022}"));
    assert!(field.password && field.editable && field.focusable);
    assert_eq!(tree.nodes[&6].level, Some(2));
    assert_eq!(view.snapshot(), before);
}

#[test]
fn assistive_edits_use_ordinary_events_and_ignore_invalid_or_stale_targets() {
    let mut b = B::new();
    b.create_element(2, Tag::Input)
        .set_value(2, "old", 0)
        .insert(1, 2, 0)
        .listen(
            2,
            EventKind::Input,
            7,
            Filter {
                key: ANY_KEY,
                ..Filter::default()
            },
        );
    let mut view = View::default();
    commit(&mut view, &b);
    let identity = view.accessibility_tree().unwrap().nodes[&2]
        .identity
        .clone();
    assert!(
        view.accessibility_input(
            &identity,
            &Action::SetValue("new".into()),
            &mut Denied { opens: 0 }
        )
        .unwrap()
    );
    let deliveries = view.drain();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].record.kind, EventKind::Input);
    assert_eq!(deliveries[0].record.value, "new");
    // An input record retains the ordinary non-gesture policy.
    assert!(!deliveries[0].gesture);
    let before = view.snapshot();
    let too_long = "a".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1);
    assert!(
        !view
            .accessibility_input(
                &identity,
                &Action::SetValue(too_long),
                &mut Denied { opens: 0 }
            )
            .unwrap()
    );
    assert_eq!(view.snapshot(), before);
    for (attribute, value) in [
        (A::Readonly, ""),
        (A::Disabled, ""),
        (A::AriaHidden, "true"),
    ] {
        let mut b = B::new();
        b.set_attribute(2, attribute, value);
        commit(&mut view, &b);
        view.drain();
        let before = view.snapshot();
        assert!(
            !view
                .accessibility_input(
                    &identity,
                    &Action::SetValue("blocked".into()),
                    &mut Denied { opens: 0 }
                )
                .unwrap()
        );
        assert_eq!(view.snapshot(), before);
        assert!(!view.has_pending());
        let mut b = B::new();
        b.remove_attribute(2, attribute);
        commit(&mut view, &b);
        view.drain();
    }
    let mut b = B::new();
    b.set_style(2, P::Display, "none");
    commit(&mut view, &b);
    view.drain();
    assert!(
        !view
            .accessibility_input(&identity, &Action::Focus, &mut Denied { opens: 0 })
            .unwrap()
    );
    let mut b = B::new();
    b.remove(2).create_element(2, Tag::Input).insert(1, 2, 0);
    commit(&mut view, &b);
    view.drain();
    assert!(
        !view
            .accessibility_input(&identity, &Action::Focus, &mut Denied { opens: 0 })
            .unwrap()
    );
    let current = &view.accessibility_tree().unwrap().nodes[&2].identity;
    assert!(
        view.accessibility_input(current, &Action::Focus, &mut Denied { opens: 0 })
            .unwrap()
    );
}

#[test]
fn option_activation_preserves_concrete_choice_and_disabled_owner_rules() {
    let mut b = B::new();
    b.create_element(2, Tag::Select)
        .set_attribute(2, A::Disabled, "")
        .insert(1, 2, 0)
        .create_element(3, Tag::Option)
        .set_attribute(3, A::Value, "same")
        .insert(2, 3, 0)
        .create_text(5, "A")
        .insert(3, 5, 0)
        .create_element(4, Tag::Option)
        .set_attribute(4, A::Value, "same")
        .insert(2, 4, 0)
        .create_text(6, "B")
        .insert(4, 6, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let tree = view.accessibility_tree().unwrap();
    assert!(tree.nodes[&4].disabled);
    assert!(!tree.nodes[&4].clickable);
    assert!(
        view.snapshot()["accessibility"]["children"][0]["children"][1]
            .get("disabled")
            .is_none()
    );
    let identity = tree.nodes[&4].identity.clone();
    assert!(
        !view
            .accessibility_input(&identity, &Action::Click, &mut Denied { opens: 0 })
            .unwrap()
    );
    let mut b = B::new();
    b.remove_attribute(2, A::Disabled);
    commit(&mut view, &b);
    assert!(
        view.accessibility_input(&identity, &Action::Click, &mut Denied { opens: 0 })
            .unwrap()
    );
    let snapshot = view.snapshot();
    assert_eq!(snapshot["tree"]["children"][0]["seq"], 1);
    assert_eq!(
        snapshot["tree"]["children"][0]["children"][1]["selected"],
        true
    );
}

#[test]
fn assistive_link_activation_keeps_permission_denial() {
    let mut b = B::new();
    b.create_element(2, Tag::A)
        .set_attribute(2, A::Href, "https://example.test/")
        .insert(1, 2, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let identity = view.accessibility_tree().unwrap().nodes[&2]
        .identity
        .clone();
    let mut broker = Denied { opens: 0 };
    assert!(
        view.accessibility_input(&identity, &Action::Click, &mut broker)
            .unwrap()
    );
    assert_eq!(broker.opens, 1);
    assert!(view.snapshot()["opened"].as_array().unwrap().is_empty());
    let records = view.drain();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].record.status,
        varta_common::view::CommandStatus::NotAllowed.code()
    );
}

#[test]
fn actionable_roleless_containers_remain_reachable_without_changing_legacy_json() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .set_attribute(2, A::Tabindex, "0")
        .insert(1, 2, 0)
        .listen(2, EventKind::Click, 7, Filter::default())
        .create_text(3, "Activate")
        .insert(2, 3, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let semantics = view.accessibility_tree().unwrap();
    assert_eq!(semantics.nodes[&1].children, [2]);
    assert_eq!(semantics.nodes[&2].role, "generic");
    assert!(semantics.nodes[&2].focusable && semantics.nodes[&2].clickable);
    assert_eq!(
        view.snapshot()["accessibility"]["children"][0]["role"],
        "text"
    );
    assert!(
        view.accessibility_input(
            &semantics.nodes[&2].identity,
            &Action::Click,
            &mut Denied { opens: 0 }
        )
        .unwrap()
    );
    let deliveries = view.drain();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].record.kind, EventKind::Click);
    assert!(deliveries[0].gesture);
}

#[test]
fn semantic_geometry_uses_the_presented_physical_viewport_and_element_fragments() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Display, "block")
        .set_style(2, P::Width, "40px")
        .set_style(2, P::Height, "30px")
        .set_style(2, P::BackgroundColor, "#f00");
    let mut view = View::default();
    commit(&mut view, &b);
    let frame = view.rasterize_scaled(160, 120, 2.0, None).unwrap();
    let bounds = frame.node_bounds_map();
    assert_eq!(
        bounds[&1],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 120.0
        }
    );
    assert_eq!(
        bounds[&2],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 60.0
        }
    );
    assert_eq!(frame.hit_test(79.0, 59.0), Some(2));
    assert_ne!(frame.hit_test(81.0, 61.0), Some(2));
}

#[test]
fn descriptions_preserve_reference_order_hidden_exceptions_and_legacy_json() {
    let mut b = B::new();
    b.create_element(2, Tag::Button)
        .set_attribute(2, A::AriaLabel, "Save")
        .set_attribute(2, A::AriaDescribedby, "hidden mixed hidden missing")
        .insert(1, 2, 0)
        .create_element(3, Tag::Span)
        .set_attribute(3, A::Id, "hidden")
        .set_attribute(3, A::AriaHidden, "true")
        .insert(1, 3, 0)
        .create_text(4, "  Help\n text ")
        .insert(3, 4, 0)
        .create_element(5, Tag::Div)
        .set_attribute(5, A::Id, "mixed")
        .insert(1, 5, 0)
        .create_text(6, "Visible ")
        .insert(5, 6, 0)
        .create_element(7, Tag::Span)
        .set_attribute(7, A::AriaHidden, "true")
        .insert(5, 7, 0)
        .create_text(8, "omit")
        .insert(7, 8, 0)
        .create_text(9, " tail")
        .insert(5, 9, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let before = view.snapshot();
    let tree = view.accessibility_tree().unwrap();
    assert_eq!(tree.nodes[&2].name, "Save");
    assert_eq!(
        tree.nodes[&2].description.as_deref(),
        Some("Help text Visible tail Help text")
    );
    assert!(!tree.nodes.contains_key(&3));
    assert_eq!(view.snapshot(), before);
    assert!(
        before["accessibility"]["children"][0]
            .get("description")
            .is_none()
    );
    let mut b = B::new();
    b.remove(3)
        .create_element(3, Tag::Span)
        .set_attribute(3, A::Id, "hidden")
        .set_attribute(3, A::AriaHidden, "true")
        .insert(1, 3, 0)
        .create_text(4, "Updated")
        .insert(3, 4, 0);
    commit(&mut view, &b);
    assert_eq!(
        view.accessibility_tree().unwrap().nodes[&2]
            .description
            .as_deref(),
        Some("Updated Visible tail Updated")
    );
}

#[test]
fn description_cycles_are_finite_and_embedded_password_values_stay_masked() {
    let mut b = B::new();
    b.create_element(2, Tag::Button)
        .set_attribute(2, A::Id, "control")
        .set_attribute(2, A::AriaLabel, "Save")
        .set_attribute(2, A::AriaDescribedby, "a secret picture")
        .insert(1, 2, 0)
        .create_element(3, Tag::Span)
        .set_attribute(3, A::Id, "a")
        .set_attribute(3, A::AriaLabelledby, "b")
        .set_attribute(3, A::AriaDescribedby, "b")
        .insert(1, 3, 0)
        .create_text(4, "A")
        .insert(3, 4, 0)
        .create_element(5, Tag::Span)
        .set_attribute(5, A::Id, "b")
        .set_attribute(5, A::AriaLabelledby, "a")
        .set_attribute(5, A::AriaDescribedby, "a")
        .insert(1, 5, 0)
        .create_text(6, "B")
        .insert(5, 6, 0)
        .create_element(7, Tag::Input)
        .set_attribute(7, A::Id, "secret")
        .set_attribute(7, A::Type, "password")
        .set_value(7, "A😀", 0)
        .insert(1, 7, 0)
        .create_element(8, Tag::Img)
        .set_attribute(8, A::Id, "picture")
        .set_attribute(8, A::Alt, "Diagram")
        .insert(1, 8, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    assert_eq!(
        view.accessibility_tree().unwrap().nodes[&2]
            .description
            .as_deref(),
        Some("B •• Diagram")
    );
    let mut b = B::new();
    b.set_attribute(2, A::AriaDescribedby, "control");
    commit(&mut view, &b);
    assert_eq!(
        view.accessibility_tree().unwrap().nodes[&2]
            .description
            .as_deref(),
        Some("Save")
    );
    let mut b = B::new();
    b.set_attribute(2, A::AriaDescribedby, "missing");
    commit(&mut view, &b);
    assert!(
        view.accessibility_tree().unwrap().nodes[&2]
            .description
            .is_none()
    );
}

#[test]
fn observer_exact_work_boundary_and_one_byte_overflow_do_not_change_guest_admission() {
    use varta_common::view::{DRAFT_MAX_VALUE_BYTES, DRAFT_NAMING_NODE_WORK as NODE};
    use varta_view::{ACCESSIBILITY_TEXT_WORK_LIMIT as LIMIT, AccessibilityError};
    let repeats = 255;
    let refs = vec!["help"; repeats].join(" ");
    // Root/control names, source id, referenced container/text and id lookups.
    let fixed = 2 * NODE + refs.len() + repeats * (3 * NODE + 1 + "help".len());
    let text_bytes = (LIMIT - fixed) / repeats;
    let padding = (LIMIT - fixed) % repeats;
    let units = DRAFT_MAX_VALUE_BYTES / 2;
    let extra = text_bytes - units;
    let text = "é".repeat(extra) + &"a".repeat(units - extra);
    assert_eq!(text.len(), text_bytes);
    let mut b = B::new();
    b.create_element(2, Tag::Button)
        .set_attribute(2, A::AriaLabel, &"x".repeat(padding))
        .set_attribute(2, A::AriaDescribedby, &refs)
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_attribute(3, A::Id, "help")
        .set_attribute(3, A::AriaHidden, "true")
        .insert(1, 3, 0)
        .create_text(4, &text)
        .insert(3, 4, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    assert_eq!(
        view.accessibility_tree().unwrap().nodes[&2]
            .description
            .as_ref()
            .unwrap()
            .len(),
        repeats * text_bytes + repeats - 1
    );
    let mut b = B::new();
    b.set_attribute(2, A::AriaLabel, &"x".repeat(padding + 1));
    commit(&mut view, &b);
    let before = view.snapshot();
    assert_eq!(
        view.accessibility_tree().unwrap_err(),
        AccessibilityError::WorkLimit { limit: LIMIT }
    );
    assert_eq!(view.snapshot(), before);
    let mut b = B::new();
    b.set_attribute(2, A::AriaDescribedby, "help");
    commit(&mut view, &b);
    assert_eq!(
        view.accessibility_tree().unwrap().nodes[&2]
            .description
            .as_deref(),
        Some(text.as_str())
    );
}

#[test]
fn generic_names_are_bounded_and_action_validation_never_evaluates_them() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .set_attribute(2, A::Tabindex, "0")
        .set_attribute(2, A::AriaLabelledby, &vec!["help"; 255].join(" "))
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_attribute(3, A::Id, "help")
        .set_attribute(3, A::AriaHidden, "true")
        .insert(1, 3, 0)
        .create_text(
            4,
            &"é".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2),
        )
        .insert(3, 4, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    assert!(
        view.snapshot()["accessibility"]["children"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(view.accessibility_tree().is_err());
    let identity = view.node_identity(2).unwrap();
    assert!(
        view.accessibility_input(&identity, &Action::Focus, &mut Denied { opens: 0 })
            .unwrap()
    );
    let mut b = B::new();
    b.set_attribute(2, A::AriaLabelledby, "help");
    commit(&mut view, &b);
    let semantics = view.accessibility_tree().unwrap();
    assert_eq!(semantics.focus, Some(2));
    assert_eq!(
        semantics.nodes[&2].name.len(),
        varta_common::view::DRAFT_MAX_VALUE_BYTES
    );
}

#[test]
fn native_live_defaults_and_busy_containers_preserve_the_frozen_json_projection() {
    let mut b = B::new();
    b.set_attribute(1, A::AriaLive, "off")
        .set_attribute(1, A::AriaBusy, "true")
        .create_element(2, Tag::Div)
        .set_attribute(2, A::AriaBusy, "true")
        .insert(1, 2, 0)
        .create_text(3, "Work")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_attribute(4, A::Role, "status")
        .insert(1, 4, 0)
        .create_element(5, Tag::Div)
        .set_attribute(5, A::Role, "alert")
        .set_attribute(5, A::AriaLive, "polite")
        .insert(1, 5, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let before = view.snapshot();
    let native = view.accessibility_tree().unwrap();
    assert_eq!(native.nodes[&1].live.as_deref(), Some("off"));
    assert_eq!(native.nodes[&1].busy.as_deref(), Some("true"));
    assert_eq!(native.nodes[&2].role, "generic");
    assert_eq!(native.nodes[&2].busy.as_deref(), Some("true"));
    assert!(native.nodes[&4].live.is_none());
    assert!(native.nodes[&4].live_atomic);
    assert_eq!(native.nodes[&5].live.as_deref(), Some("polite"));
    assert!(native.nodes[&5].live_atomic);
    assert_eq!(view.snapshot(), before);
    let json = &before["accessibility"];
    assert!(json.get("busy").is_none() && json.get("live").is_none());
    assert_eq!(json["children"][0]["role"], "text");
    assert!(json["children"][1].get("live").is_none());
    let mut b = B::new();
    b.remove_attribute(1, A::AriaLive);
    commit(&mut view, &b);
    let native = view.accessibility_tree().unwrap();
    assert_eq!(native.nodes[&4].live.as_deref(), Some("polite"));
    let mut b = B::new();
    b.remove_attribute(5, A::AriaLive);
    commit(&mut view, &b);
    assert_eq!(
        view.accessibility_tree().unwrap().nodes[&5].live.as_deref(),
        Some("assertive")
    );
}

#[test]
fn atomic_message_work_is_bounded_without_changing_guest_admission_and_recovers() {
    let mut b = B::new();
    for id in 2..202 {
        b.create_element(id, Tag::Div)
            .set_attribute(id, A::Role, "status")
            .insert(id - 1, id, 0);
    }
    let text = "A".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2);
    b.create_text(202, &text)
        .insert(201, 202, 0)
        .create_text(203, &text)
        .insert(201, 203, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let before = view.snapshot();
    assert!(matches!(
        view.accessibility_tree(),
        Err(varta_view::AccessibilityError::WorkLimit {
            limit: varta_view::ACCESSIBILITY_TEXT_WORK_LIMIT
        })
    ));
    assert_eq!(view.snapshot(), before);
    let mut b = B::new();
    b.set_text(202, "ready").remove(203);
    commit(&mut view, &b);
    assert!(view.accessibility_tree().is_ok());
}

#[test]
fn native_text_folding_follows_effective_roles_and_preserves_legacy_json() {
    let mut b = B::new();
    b.create_element(2, Tag::Button)
        .set_attribute(2, A::Role, "status")
        .insert(1, 2, 0)
        .create_text(3, "Ready")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_attribute(4, A::Role, "tab")
        .insert(1, 4, 0)
        .create_text(5, "Run")
        .insert(4, 5, 0)
        .create_element(6, Tag::H2)
        .set_attribute(6, A::Role, "status")
        .insert(1, 6, 0)
        .create_text(7, "Text")
        .insert(6, 7, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let before = view.snapshot();
    let native = view.accessibility_tree().unwrap();
    assert_eq!(native.nodes[&2].children, [3]);
    assert_eq!(native.nodes[&3].name, "Ready");
    assert_eq!(native.nodes[&4].name, "Run");
    assert!(native.nodes[&4].children.is_empty());
    assert!(!native.nodes.contains_key(&5));
    assert_eq!(native.nodes[&6].children, [7]);
    assert_eq!(native.nodes[&7].name, "Text");
    assert_eq!(view.snapshot(), before);
    let legacy = &before["accessibility"]["children"];
    assert!(legacy[0].get("children").is_none());
    assert_eq!(legacy[1]["children"][0]["name"], "Run");
    assert!(legacy[2].get("children").is_none());
}

#[test]
fn native_name_and_description_password_masks_match_painted_graphemes_and_legacy_stays_scalar() {
    let mut b = B::new();
    b.create_element(2, Tag::Span)
        .set_attribute(2, A::Id, "secret")
        .set_attribute(2, A::AriaHidden, "true")
        .insert(1, 2, 0)
        .create_element(3, Tag::Input)
        .set_attribute(3, A::Type, "password")
        .set_value(3, "e\u{301}👩\u{200d}💻", 0)
        .insert(2, 3, 0)
        .create_element(4, Tag::Button)
        .set_attribute(4, A::AriaLabelledby, "secret")
        .set_attribute(4, A::AriaDescribedby, "secret")
        .insert(1, 4, 0);
    let mut view = View::default();
    commit(&mut view, &b);
    let before = view.snapshot();
    let native = view.accessibility_tree().unwrap();
    assert_eq!(native.nodes[&4].name, "••");
    assert_eq!(native.nodes[&4].description.as_deref(), Some("••"));
    assert!(!native.nodes[&4].name_from_content);
    assert_eq!(before["accessibility"]["children"][0]["name"], "•••••");
    assert_eq!(view.snapshot(), before);
}
