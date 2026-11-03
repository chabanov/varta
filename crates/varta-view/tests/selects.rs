use varta_common::view::{Attribute, CssProperty, EventKind, Tag};
use varta_view::{Access, BatchWriter, Broker, Filter, Input, Target, View};

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
fn option(
    batch: &mut BatchWriter,
    id: u32,
    parent: u32,
    value: Option<&str>,
    label: &str,
    disabled: bool,
) {
    batch
        .create_element(id, Tag::Option)
        .create_text(id + 100, label)
        .insert(id, id + 100, 0);
    if let Some(value) = value {
        batch.set_attribute(id, Attribute::Value, value);
    }
    if disabled {
        batch.set_attribute(id, Attribute::Disabled, "");
    }
    batch.insert(parent, id, 0);
}
fn field() -> View {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Select)
        .insert(1, 2, 0)
        .listen(2, EventKind::Input, 10, Filter::default())
        .listen(2, EventKind::Change, 11, Filter::default());
    for (property, value) in [
        (CssProperty::Display, "block"),
        (CssProperty::Width, "80px"),
        (CssProperty::Height, "30px"),
        (CssProperty::MinWidth, "0"),
        (CssProperty::MinHeight, "0"),
        (CssProperty::Padding, "4px"),
        (CssProperty::Border, "2px solid #000"),
        (CssProperty::BorderRadius, "0"),
        (CssProperty::Outline, "none"),
        (CssProperty::BackgroundColor, "#fff"),
        (CssProperty::Color, "#00f"),
        (CssProperty::FontFamily, "\"Varta Fixture 400\""),
        (CssProperty::FontSize, "20px"),
        (CssProperty::FontWeight, "normal"),
        (CssProperty::FontStyle, "normal"),
        (CssProperty::LineHeight, "20px"),
    ] {
        batch.set_style(2, property, value);
    }
    commit(&mut view, &batch);
    view
}
fn state(view: &View) -> serde_json::Value {
    view.snapshot()["tree"]["children"][0].clone()
}
fn pick(view: &mut View, id: u32) {
    view.input(
        &Input::SelectOption {
            target: Target::Node(2),
            option: id,
        },
        &mut Denied,
    )
    .unwrap();
}
fn changes(view: &mut View) -> Vec<(EventKind, u32, String)> {
    std::iter::from_fn(|| view.next_delivery())
        .map(|item| (item.record.kind, item.record.seq, item.record.value))
        .collect()
}

#[test]
fn disabled_select_defaults_and_reinsertion_preserve_explicit_no_selection() {
    let mut view = field();
    let mut setup = BatchWriter::new();
    option(&mut setup, 3, 2, Some("a"), "A", true);
    option(&mut setup, 4, 2, Some("b"), "B", true);
    commit(&mut view, &setup);
    assert_eq!(state(&view)["value"], "");
    assert!(
        state(&view)["children"]
            .as_array()
            .unwrap()
            .iter()
            .all(|option| option["selected"] == false)
    );
    let mut enable = BatchWriter::new();
    enable.remove_attribute(3, Attribute::Disabled);
    commit(&mut view, &enable);
    assert_eq!(state(&view)["value"], "");
    let mut selected = BatchWriter::new();
    selected.set_attribute(4, Attribute::Selected, "");
    commit(&mut view, &selected);
    assert_eq!(state(&view)["value"], "b");
    let mut clear = BatchWriter::new();
    clear.set_value(2, "missing", 0);
    commit(&mut view, &clear);
    let mut move_select = BatchWriter::new();
    move_select
        .create_element(1000, Tag::Div)
        .insert(1, 1000, 0)
        .insert(1000, 2, 0);
    commit(&mut view, &move_select);
    let snapshot = view.snapshot();
    let select = &snapshot["tree"]["children"][0]["children"][0];
    assert_eq!(select["value"], "");
    assert_eq!(select["seq"], 0);
    assert!(
        select["children"]
            .as_array()
            .unwrap()
            .iter()
            .all(|option| option["selected"] == false)
    );
    assert!(changes(&mut view).is_empty());
}

#[test]
fn select_initial_values_disabled_options_duplicate_values_and_equal_echoes() {
    let mut view = field();
    let mut batch = BatchWriter::new();
    option(&mut batch, 3, 2, Some("blocked"), "A", true);
    option(&mut batch, 4, 2, Some("same"), "AA", false);
    option(&mut batch, 5, 2, Some("same"), "AAA", false);
    commit(&mut view, &batch);
    assert_eq!(state(&view)["value"], "same");
    assert_eq!(state(&view)["children"][1]["selected"], true);
    pick(&mut view, 5);
    assert_eq!(
        changes(&mut view),
        [
            (EventKind::Input, 1, "same".into()),
            (EventKind::Change, 1, "same".into())
        ]
    );
    assert_eq!(state(&view)["children"][2]["selected"], true);
    let mut echo = BatchWriter::new();
    echo.set_value(2, "same", 1);
    commit(&mut view, &echo);
    assert_eq!(state(&view)["children"][2]["selected"], true);
    pick(&mut view, 5);
    assert!(changes(&mut view).is_empty());
    let before = view.snapshot();
    assert!(
        view.input(
            &Input::SelectOption {
                target: Target::Node(2),
                option: 3
            },
            &mut Denied
        )
        .is_err()
    );
    assert!(
        view.input(
            &Input::Select {
                target: Target::Node(2),
                value: "missing".into()
            },
            &mut Denied
        )
        .is_err()
    );
    assert_eq!(view.snapshot(), before);
    let mut disabled = BatchWriter::new();
    disabled.set_attribute(2, Attribute::Disabled, "");
    commit(&mut view, &disabled);
    pick(&mut view, 4);
    assert!(changes(&mut view).is_empty());
    assert_eq!(state(&view)["children"][2]["selected"], true);
}

#[test]
fn select_mutation_preserves_dirty_selection_and_normalizes_structure() {
    let mut view = field();
    let mut batch = BatchWriter::new();
    option(&mut batch, 3, 2, None, "  A\n AA  ", false);
    option(&mut batch, 4, 2, Some("b"), "AA", false);
    commit(&mut view, &batch);
    assert_eq!(state(&view)["value"], "A AA");
    pick(&mut view, 4);
    changes(&mut view);
    let mut selected = BatchWriter::new();
    selected
        .set_attribute(4, Attribute::Selected, "")
        .remove_attribute(4, Attribute::Selected);
    commit(&mut view, &selected);
    assert_eq!(state(&view)["value"], "b");
    let mut text = BatchWriter::new();
    text.set_text(104, "AAA")
        .set_attribute(4, Attribute::Value, "new");
    commit(&mut view, &text);
    assert_eq!(state(&view)["value"], "new");
    assert_eq!(state(&view)["seq"], 1);
    let mut no_match = BatchWriter::new();
    no_match.set_value(2, "missing", 1);
    commit(&mut view, &no_match);
    assert_eq!(state(&view)["value"], "");
    assert!(
        state(&view)["children"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["selected"] == false)
    );
    let mut appended = BatchWriter::new();
    option(&mut appended, 5, 2, Some("c"), "AAA", false);
    commit(&mut view, &appended);
    assert_eq!(state(&view)["value"], "A AA");
    let mut moved = BatchWriter::new();
    moved
        .create_element(6, Tag::Select)
        .insert(1, 6, 0)
        .insert(6, 3, 0);
    commit(&mut view, &moved);
    assert_eq!(state(&view)["value"], "new");
    assert_eq!(view.snapshot()["tree"]["children"][1]["value"], "A AA");
    let before = view.snapshot();
    let mut bad = BatchWriter::new();
    bad.remove(4).set_text(999, "bad");
    assert!(view.commit(&bad.finish(), false, &mut Denied).is_err());
    assert_eq!(view.snapshot(), before);
    let mut remove = BatchWriter::new();
    remove.remove(4);
    commit(&mut view, &remove);
    assert_eq!(state(&view)["value"], "c");
}

fn font() -> varta_view::TextMeasurer {
    varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap()
}
fn key(view: &mut View, key: &str) {
    view.input(
        &Input::Key {
            target: Target::Node(2),
            key: key.into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut Denied,
    )
    .unwrap();
}
fn choices() -> View {
    let mut view = field();
    let mut batch = BatchWriter::new();
    option(&mut batch, 3, 2, Some("a"), "A", true);
    option(&mut batch, 4, 2, Some("b"), "AA", false);
    option(&mut batch, 5, 2, Some("c"), "AAA", false);
    commit(&mut view, &batch);
    view
}

#[test]
fn native_select_popup_hit_testing_scaling_and_dismissal() {
    let mut view = choices();
    let mut font = font();
    let closed = view.rasterize_with_text(100, 200, &mut font, 20.0).unwrap();
    let boxes = view
        .layout_boxes_with_text(100.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(boxes[&3].width, 0.0);
    assert_eq!(boxes[&103].height, 0.0);
    assert_eq!(closed.hit_test(10.0, 15.0), Some(2));
    assert_eq!(closed.text_position(2, 10.0), None);
    assert!(closed.select_menu().is_none());
    view.input(
        &Input::Click {
            target: Target::Node(2),
        },
        &mut Denied,
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 200, &mut font, 20.0).unwrap();
    let menu = image.select_menu().unwrap();
    assert_eq!(menu.viewport.y, 30.0);
    assert_eq!(menu.row_height, 28.0);
    assert_eq!(
        image.select_hit(10.0, 40.0),
        Some(varta_view::SelectHit::Disabled { select: 2 })
    );
    assert_eq!(
        image.select_hit(10.0, 100.0),
        Some(varta_view::SelectHit::Choice {
            select: 2,
            option: 5
        })
    );
    assert_eq!(
        image.select_hit(99.0, 199.0),
        Some(varta_view::SelectHit::Dismiss { select: 2 })
    );
    let scaled = view
        .rasterize_scaled(200, 400, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    assert_eq!(
        scaled.select_hit(20.0, 200.0),
        Some(varta_view::SelectHit::Choice {
            select: 2,
            option: 5
        })
    );
    view.input(
        &Input::SelectClose {
            target: Target::Node(2),
        },
        &mut Denied,
    )
    .unwrap();
    assert!(
        view.rasterize_with_text(100, 200, &mut font, 20.0)
            .unwrap()
            .select_menu()
            .is_none()
    );
    assert_eq!(state(&view)["value"], "b");
    assert!(changes(&mut view).is_empty());
}

#[test]
fn native_select_keyboard_preview_cancel_commit_and_focus_exit() {
    let mut view = choices();
    let mut font = font();
    key(&mut view, "ArrowDown");
    assert_eq!(state(&view)["value"], "c");
    changes(&mut view);
    key(&mut view, "Enter");
    key(&mut view, "ArrowUp");
    assert_eq!(state(&view)["value"], "c");
    assert!(changes(&mut view).is_empty());
    key(&mut view, "Escape");
    assert_eq!(state(&view)["value"], "c");
    assert!(
        view.rasterize_with_text(100, 200, &mut font, 20.0)
            .unwrap()
            .select_menu()
            .is_none()
    );
    key(&mut view, "Space");
    key(&mut view, "Home");
    key(&mut view, "Enter");
    assert_eq!(state(&view)["value"], "b");
    assert_eq!(
        changes(&mut view),
        [
            (EventKind::Input, 2, "b".into()),
            (EventKind::Change, 2, "b".into())
        ]
    );
    let mut next = BatchWriter::new();
    next.create_element(6, Tag::Select).insert(1, 6, 0);
    commit(&mut view, &next);
    key(&mut view, "Space");
    key(&mut view, "Tab");
    assert_eq!(view.snapshot()["focus"], 6);
    let mut removed = BatchWriter::new();
    removed.remove(6);
    commit(&mut view, &removed);
    assert!(
        view.rasterize_with_text(100, 200, &mut font, 20.0)
            .unwrap()
            .select_menu()
            .is_none()
    );
}

#[test]
fn native_select_popup_scrolls_and_escapes_ancestor_clipping() {
    let mut view = field();
    let mut batch = BatchWriter::new();
    batch
        .create_element(9, Tag::Div)
        .insert(1, 9, 0)
        .set_style(9, CssProperty::Width, "80px")
        .set_style(9, CssProperty::Height, "40px")
        .set_style(9, CssProperty::Overflow, "hidden")
        .insert(9, 2, 0);
    for id in 10..22 {
        option(&mut batch, id, 2, Some(&id.to_string()), "A", false);
    }
    commit(&mut view, &batch);
    key(&mut view, "Space");
    let mut font = font();
    let image = view.rasterize_with_text(100, 300, &mut font, 20.0).unwrap();
    let menu = image.select_menu().unwrap();
    assert_eq!(menu.max_first, 4);
    assert!(matches!(
        image.select_hit(10.0, 100.0),
        Some(varta_view::SelectHit::Choice { .. })
    ));
    assert_ne!(
        &image.pixels[(100 * 100 + 10) * 4..(100 * 100 + 11) * 4],
        &[0, 0, 0, 0]
    );
    view.input(
        &Input::SelectScroll {
            target: Target::Node(2),
            first: 4,
        },
        &mut Denied,
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 300, &mut font, 20.0).unwrap();
    assert_eq!(image.select_menu().unwrap().first, 4);
    assert_eq!(
        image.select_hit(10.0, 40.0),
        Some(varta_view::SelectHit::Choice {
            select: 2,
            option: 14
        })
    );
    key(&mut view, "End");
    let image = view.rasterize_with_text(100, 300, &mut font, 20.0).unwrap();
    assert_eq!(image.select_menu().unwrap().first, 4);
    key(&mut view, "Enter");
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["children"][0]["value"],
        "21"
    );
}

#[test]
fn select_default_prevention_and_guest_commits_preserve_transient_state() {
    use varta_common::view::{Key, ListenOption};
    let mut view = choices();
    let mut batch = BatchWriter::new();
    let mut font = font();
    batch.listen(
        2,
        EventKind::Click,
        12,
        Filter {
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut view, &batch);
    view.input(
        &Input::Click {
            target: Target::Node(2),
        },
        &mut Denied,
    )
    .unwrap();
    assert!(
        view.rasterize_with_text(100, 200, &mut font, 20.0)
            .unwrap()
            .select_menu()
            .is_none()
    );
    changes(&mut view);
    let mut prevent = BatchWriter::new();
    prevent.unlisten(2, EventKind::Click, 12).listen(
        2,
        EventKind::Keydown,
        12,
        Filter {
            key: Key::ArrowDown.code(),
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut view, &prevent);
    key(&mut view, "ArrowDown");
    assert_eq!(state(&view)["value"], "b");
    changes(&mut view);
    key(&mut view, "Enter");
    commit(&mut view, &BatchWriter::new());
    assert!(
        view.rasterize_with_text(100, 200, &mut font, 20.0)
            .unwrap()
            .select_menu()
            .is_some()
    );
    let mut change = BatchWriter::new();
    option(&mut change, 6, 2, Some("new"), "A", false);
    commit(&mut view, &change);
    assert!(
        view.rasterize_with_text(100, 200, &mut font, 20.0)
            .unwrap()
            .select_menu()
            .is_none()
    );
    assert_eq!(state(&view)["seq"], 0);
}

#[test]
fn unchanged_selected_attributes_do_not_revive_or_move_live_selection() {
    let mut view = field();
    let mut batch = BatchWriter::new();
    option(&mut batch, 3, 2, Some("a"), "A", false);
    batch.set_attribute(3, Attribute::Selected, "");
    option(&mut batch, 4, 2, Some("b"), "AA", false);
    batch.set_attribute(4, Attribute::Selected, "");
    commit(&mut view, &batch);
    assert_eq!(state(&view)["value"], "b");
    let mut same = BatchWriter::new();
    same.set_attribute(3, Attribute::Selected, "");
    commit(&mut view, &same);
    assert_eq!(state(&view)["value"], "b");
    let mut none = BatchWriter::new();
    none.set_value(2, "missing", 0)
        .remove_attribute(4, Attribute::Value);
    commit(&mut view, &none);
    assert_eq!(state(&view)["value"], "");
    let mut absent = BatchWriter::new();
    absent
        .remove_attribute(3, Attribute::Disabled)
        .set_attribute(3, Attribute::Selected, "");
    commit(&mut view, &absent);
    assert_eq!(state(&view)["value"], "");
    let mut changed = BatchWriter::new();
    changed
        .remove_attribute(3, Attribute::Selected)
        .set_attribute(3, Attribute::Selected, "");
    commit(&mut view, &changed);
    assert_eq!(state(&view)["value"], "a");
}

#[test]
fn select_character_search_cycles_and_previews_without_editing_until_acceptance() {
    let mut view = choices();
    let mut font = font();
    let character = Input::TextKey {
        target: Target::Node(2),
        key: "a".into(),
        text: "a".into(),
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
        select_all: false,
        history: None,
    };
    view.input(&character, &mut Denied).unwrap();
    assert_eq!(state(&view)["value"], "c");
    changes(&mut view);
    key(&mut view, "Space");
    view.input(&character, &mut Denied).unwrap();
    assert_eq!(state(&view)["value"], "c");
    assert!(changes(&mut view).is_empty());
    assert!(
        view.rasterize_with_text(100, 200, &mut font, 20.0)
            .unwrap()
            .select_menu()
            .is_some()
    );
    key(&mut view, "Enter");
    assert_eq!(state(&view)["value"], "b");
    assert_eq!(
        changes(&mut view),
        [
            (EventKind::Input, 2, "b".into()),
            (EventKind::Change, 2, "b".into())
        ]
    );
}

#[test]
fn native_select_search_uses_the_logical_key_when_no_text_was_committed() {
    let mut view = choices();
    view.input(
        &Input::TextKey {
            target: Target::Node(2),
            key: "a".into(),
            text: String::new(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            select_all: false,
            history: None,
        },
        &mut Denied,
    )
    .unwrap();
    assert_eq!(state(&view)["value"], "c");
    // Text fields must still wait for committed text rather than insert a key name.
    let mut text = View::default();
    let mut setup = BatchWriter::new();
    setup
        .create_element(2, Tag::Input)
        .set_value(2, "original", 0)
        .insert(1, 2, 0);
    commit(&mut text, &setup);
    text.input(
        &Input::TextKey {
            target: Target::Node(2),
            key: "a".into(),
            text: String::new(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            select_all: false,
            history: None,
        },
        &mut Denied,
    )
    .unwrap();
    assert_eq!(state(&text)["value"], "original");
}

#[test]
fn derived_option_values_keep_the_existing_value_budget_atomically() {
    let mut view = field();
    let before = view.snapshot();
    let mut batch = BatchWriter::new();
    let part = "A".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 4 + 1);
    option(&mut batch, 3, 2, None, &part, true);
    batch.create_text(104, &part).insert(3, 104, 0);
    assert!(matches!(
        view.commit(&batch.finish(), false, &mut Denied),
        Err(varta_view::ViewError::Invalid {
            violation: varta_view::Violation::StringTooLong,
            ..
        })
    ));
    assert_eq!(view.snapshot(), before);
}

#[test]
fn repeated_select_scans_share_a_transaction_budget_and_reject_atomically() {
    // Equal UTF-8 sizes with different UTF-16 lengths must charge equally in
    // native and browser hosts. Neither fixture exceeds a wire/value limit.
    for value in ["X".repeat(4096), "😀".repeat(1024)] {
        let mut view = field();
        let mut setup = BatchWriter::new();
        for id in 3..23 {
            option(&mut setup, id, 2, Some(&value), "A", false);
        }
        commit(&mut view, &setup);
        let mut ordinary = BatchWriter::new();
        for index in 0..100 {
            ordinary.set_value(2, &format!("missing{index}"), 0);
        }
        commit(&mut view, &ordinary);
        assert_eq!(state(&view)["value"], "");
        let mut noops = BatchWriter::new();
        for _ in 0..512 {
            noops.set_value(2, "missing", 99);
        }
        for _ in 0..512 {
            noops.set_value(2, "", 0);
        }
        commit(&mut view, &noops);
        let mut reset = BatchWriter::new();
        reset.set_value(2, &value, 0);
        commit(&mut view, &reset);
        let before = view.snapshot();
        let mut excessive = BatchWriter::new();
        excessive.set_title("must roll back");
        for index in 0..128 {
            excessive.set_value(2, &format!("missing{index}"), 0);
        }
        let failure = view
            .commit(&excessive.finish(), false, &mut Denied)
            .unwrap_err();
        assert!(matches!(
            failure,
            varta_view::ViewError::Invalid {
                operation: Some(_),
                violation: varta_view::Violation::SelectWorkLimit,
            }
        ));
        assert_eq!(view.snapshot(), before);
        view.input(
            &Input::SelectOption {
                target: Target::Node(2),
                option: 4,
            },
            &mut Denied,
        )
        .unwrap();
        let edited = view.snapshot();
        assert_eq!(
            view.commit(&excessive.finish(), false, &mut Denied)
                .unwrap_err(),
            failure
        );
        assert_eq!(view.snapshot(), edited);
        // A rejected batch does not consume the next transaction's allowance.
        commit(&mut view, &ordinary);
    }
}

#[test]
fn identical_select_writes_collapse_but_catalog_and_base_changes_do_not() {
    let mut view = choices();
    let mut repeated = BatchWriter::new();
    for _ in 0..1024 {
        repeated.set_value(2, "missing", 0);
    }
    commit(&mut view, &repeated);
    assert_eq!(state(&view)["value"], "");
    let mut changed = BatchWriter::new();
    changed
        .set_attribute(3, Attribute::Value, "missing")
        .set_value(2, "missing", 0)
        .set_value(2, "other", 99)
        .set_value(2, "other", 0);
    commit(&mut view, &changed);
    assert_eq!(state(&view)["value"], "");
    // A catalog mutation between identical writes must invalidate the shortcut.
    let mut catalog = BatchWriter::new();
    catalog
        .set_value(2, "new", 0)
        .set_attribute(3, Attribute::Value, "new")
        .set_value(2, "new", 0);
    commit(&mut view, &catalog);
    assert_eq!(state(&view)["value"], "new");
}

#[test]
fn detached_option_inventory_is_charged_before_final_value_validation() {
    let mut view = View::default();
    let value = "😀".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 4);
    // Individually legal values, in legal batches, accumulate near the shared
    // work ceiling. Detached nodes cannot bypass final inventory accounting.
    for group in 0..41 {
        let mut batch = BatchWriter::new();
        for offset in 0..3 {
            let id = 2 + group * 3 + offset;
            batch
                .create_element(id, Tag::Option)
                .set_attribute(id, Attribute::Value, &value);
        }
        commit(&mut view, &batch);
    }
    let before = view.snapshot();
    let mut batch = BatchWriter::new();
    batch.set_title("must roll back");
    for id in 125..128 {
        batch
            .create_element(id, Tag::Option)
            .set_attribute(id, Attribute::Value, &value);
    }
    assert!(matches!(
        view.commit(&batch.finish(), false, &mut Denied),
        Err(varta_view::ViewError::Invalid {
            operation: None,
            violation: varta_view::Violation::SelectWorkLimit,
        })
    ));
    assert_eq!(view.snapshot(), before);
    let mut reuse = BatchWriter::new();
    reuse
        .create_element(125, Tag::Option)
        .set_attribute(125, Attribute::Value, "small");
    commit(&mut view, &reuse);
}

#[test]
fn repeated_owner_searches_in_a_deep_tree_consume_the_same_work_allowance() {
    let mut view = View::default();
    let mut setup = BatchWriter::new();
    setup.set_title("before");
    for id in 2..402 {
        setup.create_element(id, Tag::Div).insert(id - 1, id, 0);
    }
    setup.create_text(402, "A").insert(401, 402, 0);
    commit(&mut view, &setup);
    let commits = view.commits();
    let mut excessive = BatchWriter::new();
    excessive.set_title("must roll back");
    for _ in 0..2000 {
        excessive.set_text(402, "B");
    }
    assert!(matches!(
        view.commit(&excessive.finish(), false, &mut Denied),
        Err(varta_view::ViewError::Invalid {
            operation: Some(_),
            violation: varta_view::Violation::SelectWorkLimit,
        })
    ));
    assert_eq!(view.title(), "before");
    assert_eq!(view.commits(), commits);
    let mut ordinary = BatchWriter::new();
    ordinary.set_text(402, "C");
    commit(&mut view, &ordinary);
}
