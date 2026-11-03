use varta_common::view::{Attribute, CssProperty, EventFlag, EventKind, Key, ListenOption, Tag};
use varta_view::{Access, BatchWriter, Broker, EventRecord, Filter, Input, Target, View};

struct Denied;
impl Broker for Denied {
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
fn commit(view: &mut View, batch: &BatchWriter) {
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
}
fn add(batch: &mut BatchWriter, id: u32, parent: u32, kind: &str, name: &str) {
    batch
        .create_element(id, Tag::Input)
        .set_attribute(id, Attribute::Type, kind)
        .set_attribute(id, Attribute::Name, name)
        .insert(parent, id, 0);
    for (property, value) in [
        (CssProperty::Display, "block"),
        (CssProperty::Width, "28px"),
        (CssProperty::Height, "28px"),
        (CssProperty::MinWidth, "0"),
        (CssProperty::MinHeight, "0"),
        (CssProperty::Padding, "4px"),
        (CssProperty::Border, "2px solid #000"),
        (CssProperty::BorderRadius, "0"),
        (CssProperty::Outline, "none"),
        (CssProperty::BackgroundColor, "#fff"),
        (CssProperty::Color, "#00f"),
    ] {
        batch.set_style(id, property, value);
    }
}
fn node(view: &View, id: u32) -> serde_json::Value {
    let snapshot = view.snapshot();
    let mut stack = vec![&snapshot["tree"]];
    while let Some(node) = stack.pop() {
        if node["id"] == id {
            return node.clone();
        }
        if let Some(children) = node["children"].as_array() {
            stack.extend(children);
        }
    }
    panic!("node {id} is not attached");
}
fn checked(view: &View, id: u32) -> bool {
    node(view, id)["checked"].as_bool().unwrap()
}
fn click(view: &mut View, id: u32) {
    view.input(
        &Input::Click {
            target: Target::Node(id),
        },
        &mut Denied,
    )
    .unwrap();
}
fn key(view: &mut View, id: u32, key: &str, shift: bool) {
    view.input(
        &Input::Key {
            target: Target::Node(id),
            key: key.into(),
            shift,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut Denied,
    )
    .unwrap();
}
fn records(view: &mut View) -> Vec<EventRecord> {
    std::iter::from_fn(|| view.next_delivery())
        .map(|item| item.record)
        .collect()
}

#[test]
fn checkable_marks_paint_without_fonts_and_activation_reports_final_state() {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    add(&mut batch, 2, 1, "checkbox", "");
    add(&mut batch, 3, 1, "radio", "g");
    for (kind, handler) in [
        (EventKind::Click, 10),
        (EventKind::Input, 11),
        (EventKind::Change, 12),
    ] {
        batch.listen(2, kind, handler, Filter::default());
    }
    commit(&mut view, &batch);
    let off = view.rasterize_boxes(80, 80).unwrap();
    assert_eq!(off.hit_test(14.0, 14.0), Some(2));
    assert_eq!(off.hit_test(14.0, 42.0), Some(3));
    click(&mut view, 2);
    let events = records(&mut view);
    assert_eq!(
        events.iter().map(|e| (e.kind, e.seq)).collect::<Vec<_>>(),
        [
            (EventKind::Click, 0),
            (EventKind::Input, 1),
            (EventKind::Change, 1)
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.flags & EventFlag::Checked as u32 != 0 && e.value.is_empty())
    );
    let on = view.rasterize_boxes(80, 80).unwrap();
    assert_ne!(off.pixels, on.pixels);
    assert_eq!(on.editable(), None);
    assert_eq!(on.focused(), Some(2));
    key(&mut view, 2, "Space", false);
    assert!(!checked(&view, 2));
    records(&mut view);
    let mut prevent = BatchWriter::new();
    prevent.unlisten(2, EventKind::Click, 10).listen(
        2,
        EventKind::Click,
        13,
        Filter {
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut view, &prevent);
    click(&mut view, 2);
    assert!(!checked(&view, 2));
    assert_eq!(node(&view, 2)["seq"], 2);
    let events = records(&mut view);
    assert_eq!(events.len(), 1);
    assert!(
        events
            .iter()
            .all(|e| e.kind == EventKind::Click && e.flags & EventFlag::Checked as u32 != 0)
    );
    click(&mut view, 3);
    let radio = view.rasterize_boxes(80, 80).unwrap();
    assert_eq!(
        &radio.pixels[(42 * 80 + 14) * 4..(42 * 80 + 15) * 4],
        &[0, 0, 255, 255]
    );
    let scaled = view.rasterize_scaled(160, 160, 2.0, None).unwrap();
    assert_eq!(scaled.hit_test(28.0, 84.0), Some(3));
    assert_eq!(
        &scaled.pixels[(84 * 160 + 28) * 4..(84 * 160 + 29) * 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(scaled.text_position(3, 28.0), None);
    let mut disabled = BatchWriter::new();
    disabled.set_attribute(3, Attribute::Disabled, "");
    commit(&mut view, &disabled);
    key(&mut view, 3, "Space", false);
    click(&mut view, 3);
    assert!(checked(&view, 3));
}

#[test]
fn radio_groups_follow_form_owner_tree_and_nonempty_name_during_patches() {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Form)
        .insert(1, 2, 0)
        .create_element(3, Tag::Form)
        .insert(1, 3, 0);
    for (id, parent, name) in [
        (4, 2, "g"),
        (5, 2, "g"),
        (6, 3, "g"),
        (7, 1, "g"),
        (9, 1, ""),
        (10, 1, ""),
        (11, 2, "other"),
    ] {
        add(&mut batch, id, parent, "radio", name);
        batch.set_checked(id, 1, 0);
    }
    // A detached checked radio must not change any connected form/root group.
    batch
        .create_element(8, Tag::Input)
        .set_attribute(8, Attribute::Type, "radio")
        .set_attribute(8, Attribute::Name, "g")
        .set_checked(8, 1, 0);
    commit(&mut view, &batch);
    assert!(!checked(&view, 4));
    for id in [5, 6, 7, 9, 10, 11] {
        assert!(checked(&view, id), "{id}");
    }
    let mut moved = BatchWriter::new();
    moved.insert(3, 5, 0);
    commit(&mut view, &moved);
    assert!(checked(&view, 5));
    assert!(!checked(&view, 6));
    assert!(checked(&view, 7));
    let mut rename = BatchWriter::new();
    rename
        .set_attribute(11, Attribute::Name, "g")
        .set_checked(4, 1, 0);
    commit(&mut view, &rename);
    assert!(checked(&view, 4));
    assert!(!checked(&view, 11));
    let mut split = BatchWriter::new();
    split
        .set_attribute(4, Attribute::Name, "")
        .set_checked(11, 1, 0);
    commit(&mut view, &split);
    assert!(checked(&view, 4));
    assert!(checked(&view, 11));
    let before = view.snapshot();
    let mut bad = BatchWriter::new();
    bad.set_checked(6, 1, 0)
        .set_attribute(11, Attribute::Name, "")
        .set_style(999, CssProperty::Width, "1px");
    assert!(view.commit(&bad.finish(), false, &mut Denied).is_err());
    assert_eq!(view.snapshot(), before);
    click(&mut view, 6);
    assert!(checked(&view, 6));
    assert!(!checked(&view, 5));
    assert!(checked(&view, 7));
    assert_eq!(node(&view, 5)["seq"], 0);
}

#[test]
fn radio_keyboard_skips_hidden_disabled_peers_wraps_and_has_one_tab_stop() {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    add(&mut batch, 2, 1, "checkbox", "");
    for id in 3..=6 {
        add(&mut batch, id, 1, "radio", "g");
    }
    add(&mut batch, 7, 1, "checkbox", "");
    batch
        .set_attribute(4, Attribute::Disabled, "")
        .set_style(5, CssProperty::Display, "none")
        .set_checked(3, 1, 0)
        .create_element(8, Tag::Form)
        .insert(1, 8, 0);
    add(&mut batch, 9, 8, "radio", "g");
    batch.set_checked(9, 1, 0);
    commit(&mut view, &batch);
    click(&mut view, 2);
    key(&mut view, 2, "Tab", false);
    assert_eq!(view.snapshot()["focus"], 3);
    key(&mut view, 3, "ArrowDown", false);
    assert_eq!(view.snapshot()["focus"], 6);
    assert!(checked(&view, 6));
    assert!(!checked(&view, 3));
    assert!(checked(&view, 9));
    key(&mut view, 6, "Tab", false);
    assert_eq!(view.snapshot()["focus"], 7);
    key(&mut view, 7, "Tab", true);
    assert_eq!(view.snapshot()["focus"], 6);
    key(&mut view, 6, "ArrowUp", false);
    assert_eq!(view.snapshot()["focus"], 3);
    key(&mut view, 3, "ArrowLeft", false);
    assert_eq!(view.snapshot()["focus"], 6);
    let mut prevent = BatchWriter::new();
    prevent.listen(
        6,
        EventKind::Keydown,
        10,
        Filter {
            key: Key::ArrowLeft.code(),
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut view, &prevent);
    key(&mut view, 6, "ArrowLeft", false);
    assert_eq!(view.snapshot()["focus"], 6);
    let mut cancel = BatchWriter::new();
    cancel.listen(
        3,
        EventKind::Click,
        11,
        Filter {
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut view, &cancel);
    key(&mut view, 6, "ArrowRight", false);
    assert_eq!(view.snapshot()["focus"], 3);
    assert!(checked(&view, 6));
    assert!(!checked(&view, 3));
    key(&mut view, 3, "Tab", false);
    assert_eq!(view.snapshot()["focus"], 7);
}

#[test]
fn checkable_marks_clip_to_ancestor_scrollports() {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, CssProperty::Width, "28px")
        .set_style(2, CssProperty::Height, "14px")
        .set_style(2, CssProperty::Overflow, "hidden");
    add(&mut batch, 3, 2, "checkbox", "");
    batch.set_checked(3, 1, 0);
    commit(&mut view, &batch);
    let image = view.rasterize_boxes(40, 40).unwrap();
    assert_eq!(image.hit_test(14.0, 10.0), Some(3));
    assert_eq!(image.hit_test(14.0, 20.0), None);
    assert!(image.pixels[14 * 40 * 4..].iter().all(|byte| *byte == 0));
}

fn radio_work_view() -> View {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .create_element(1000, Tag::Div)
        .insert(1, 1000, 0)
        .set_title("before");
    let name = "Ж".repeat(512);
    for id in 3..103 {
        batch
            .create_element(id, Tag::Input)
            .set_attribute(id, Attribute::Type, "radio")
            .set_attribute(id, Attribute::Name, &name)
            .insert(2, id, 0);
    }
    batch.set_checked(3, 1, 0);
    commit(&mut view, &batch);
    view
}

#[test]
fn radio_work_limits_repeated_checked_type_and_name_writes_atomically() {
    use varta_view::{ViewError, Violation};
    for kind in ["checked", "type", "name", "stale"] {
        let mut view = radio_work_view();
        let before = view.snapshot();
        let mut excessive = BatchWriter::new();
        excessive.set_title("must roll back");
        for i in 0..128 {
            let id = 3 + i % 2;
            match kind {
                "type" => {
                    excessive.set_attribute(id, Attribute::Type, "radio");
                }
                "name" => {
                    excessive.set_attribute(id, Attribute::Name, &"Ж".repeat(512));
                }
                _ => {
                    excessive.set_checked(id, 1, if kind == "stale" { 1 } else { 0 });
                }
            }
        }
        assert!(
            matches!(
                view.commit(&excessive.finish(), false, &mut Denied),
                Err(ViewError::Invalid {
                    operation: Some(_),
                    violation: Violation::RadioWorkLimit
                })
            ),
            "{kind}"
        );
        assert_eq!(view.snapshot(), before, "{kind}: rollback");
        assert!(view.next_delivery().is_none());
        let mut small = BatchWriter::new();
        for i in 0..60 {
            small.set_checked(3 + i % 2, 1, 0);
        }
        commit(&mut view, &small);
        assert!(!checked(&view, 3));
        assert!(checked(&view, 4));
        assert_eq!(node(&view, 4)["seq"], 0);
    }
}

#[test]
fn radio_work_accounts_unchecked_moved_subtrees_and_retains_parent_on_rejection() {
    use varta_view::{ViewError, Violation};
    let mut view = radio_work_view();
    let mut clear = BatchWriter::new();
    clear.set_checked(3, 0, 0);
    commit(&mut view, &clear);
    let before = view.snapshot();
    let mut move_all = BatchWriter::new();
    move_all.set_title("must roll back").insert(1000, 2, 0);
    assert!(matches!(
        view.commit(&move_all.finish(), false, &mut Denied),
        Err(ViewError::Invalid {
            operation: Some(1),
            violation: Violation::RadioWorkLimit
        })
    ));
    assert_eq!(view.snapshot(), before);
    let mut move_one = BatchWriter::new();
    move_one.insert(1000, 3, 0);
    commit(&mut view, &move_one);
    assert_eq!(node(&view, 1000)["children"][0]["id"], 3);
}

#[test]
fn radio_work_accepts_the_exact_allowance_and_rejects_the_next_scan() {
    use varta_common::view::{DRAFT_MAX_RADIO_WORK, DRAFT_RADIO_NODE_WORK};
    use varta_view::{ViewError, Violation};
    let mut view = View::default();
    let mut setup = BatchWriter::new();
    // Two ancestor visits, two group visits and 960 UTF-8 name bytes per scan.
    setup
        .create_element(2, Tag::Input)
        .set_attribute(2, Attribute::Type, "radio")
        .set_attribute(
            2,
            Attribute::Name,
            &"X".repeat(1024 - 4 * DRAFT_RADIO_NODE_WORK),
        )
        .insert(1, 2, 0);
    commit(&mut view, &setup);
    let count = DRAFT_MAX_RADIO_WORK / 1024;
    let mut exact = BatchWriter::new();
    for _ in 0..count {
        exact.set_checked(2, 1, 0);
    }
    commit(&mut view, &exact);
    assert!(checked(&view, 2));
    let before = view.snapshot();
    exact.set_checked(2, 1, 0);
    assert!(matches!(view.commit(&exact.finish(), false, &mut Denied),
        Err(ViewError::Invalid { operation: Some(index), violation: Violation::RadioWorkLimit }) if index == count));
    assert_eq!(view.snapshot(), before);
}

#[test]
fn tab_partitions_many_distinct_radio_groups_once_and_rebuilds_after_name_changes() {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch.create_element(2, Tag::Button).insert(1, 2, 0);
    for group in 0..64 {
        for offset in 0..2 {
            let id = 10 + group * 2 + offset;
            batch
                .create_element(id, Tag::Input)
                .set_attribute(id, Attribute::Type, "radio")
                .set_attribute(id, Attribute::Name, &format!("g{group}"))
                .insert(1, id, 0);
        }
        batch.set_checked(11 + group * 2, 1, 0);
    }
    batch.create_element(1000, Tag::Button).insert(1, 1000, 0);
    commit(&mut view, &batch);
    let mut current = 2;
    for group in 0..64 {
        key(&mut view, current, "Tab", false);
        current = 11 + group * 2;
        assert_eq!(view.snapshot()["focus"], current);
    }
    key(&mut view, current, "Tab", false);
    assert_eq!(view.snapshot()["focus"], 1000);
    key(&mut view, 1000, "Tab", true);
    assert_eq!(view.snapshot()["focus"], 137);
    // Changing membership invalidates no persistent cache: the next Tab sees
    // the new partition and the existing checked-peer reconciliation.
    let mut rename = BatchWriter::new();
    rename.set_attribute(13, Attribute::Name, "g0");
    commit(&mut view, &rename);
    key(&mut view, 2, "Tab", false);
    assert_eq!(view.snapshot()["focus"], 12);
    key(&mut view, 12, "Tab", false);
    assert_eq!(view.snapshot()["focus"], 13);
    key(&mut view, 13, "Tab", true);
    assert_eq!(view.snapshot()["focus"], 12);
}
