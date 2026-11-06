//! The headless renderer against draft v0: whole-batch validation (fail
//! closed, atomic), event dispatch, key filters, gestures and command results.
use varta_common::view::{
    ANY_KEY, Attribute, Command, CommandStatus, CssProperty, DRAFT_MAX_PENDING_COMMANDS,
    DRAFT_MAX_PENDING_EVENTS, EventFlag, EventKind, Key, ListenOption, Opcode, Tag,
};
use varta_view::{
    Access, BatchWriter, Broker, Environment, Filter, Input, Platform, PromptAnswer, Target, View,
    ViewError, Violation,
};

/// A broker with fixed answers; `links` are the package request's patterns.
struct Fixed {
    links: Vec<&'static str>,
    open: Access,
    clipboard: Access,
    asked: Vec<String>,
}
impl Default for Fixed {
    fn default() -> Self {
        Self {
            links: vec!["https://docs.test/"],
            open: Access::Granted,
            clipboard: Access::Granted,
            asked: Vec::new(),
        }
    }
}
impl Broker for Fixed {
    fn link_requested(&self, url: &str) -> bool {
        self.links.iter().any(|prefix| url.starts_with(prefix))
    }
    fn open_url(&mut self, url: &str) -> Access {
        self.asked.push(url.into());
        self.open
    }
    fn clipboard_write(&mut self) -> Access {
        self.clipboard
    }
}

fn view() -> View {
    View::new(Environment::default(), Platform::default())
}

fn commit(view: &mut View, batch: &BatchWriter) -> Result<(), ViewError> {
    view.commit(&batch.finish(), false, &mut Fixed::default())
}

#[test]
fn optional_flex_projection_preserves_the_original_cascade_snapshot() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .create_element(3, Tag::Span)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(3, CssProperty::Display, "inline")
        .insert(1, 2, 0)
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let original = view.snapshot();
    let projected = view.snapshot_flex_blockified();
    assert_eq!(
        original["tree"]["children"][0]["children"][0]["style"]["display"],
        "inline"
    );
    assert_eq!(
        projected["tree"]["children"][0]["children"][0]["style"]["display"],
        "block"
    );
    assert_eq!(view.snapshot(), original);
    assert_eq!(projected["accessibility"], original["accessibility"]);
}

fn violation(result: Result<(), ViewError>) -> Violation {
    match result {
        Err(ViewError::Invalid { violation, .. }) => violation,
        other => panic!("expected an invalid batch, got {other:?}"),
    }
}

const ANY: Filter = Filter {
    options: 0,
    key: ANY_KEY,
    required: 0,
    forbidden: 0,
};

/// A button (2) with text (3) and a textarea (4) under the root.
fn app() -> (View, BatchWriter) {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Button)
        .set_attribute(2, Attribute::Id, "inc")
        .create_text(3, "+")
        .insert(2, 3, 0)
        .insert(1, 2, 0)
        .create_element(4, Tag::Textarea)
        .set_attribute(4, Attribute::Id, "box")
        .insert(1, 4, 0)
        .listen(2, EventKind::Click, 10, ANY);
    commit(&mut view, &batch).unwrap();
    (view, BatchWriter::new())
}

fn deliveries(view: &mut View) -> Vec<(EventKind, u32, String, bool)> {
    std::iter::from_fn(|| view.next_delivery())
        .map(|d| (d.record.kind, d.record.handler, d.record.key, d.gesture))
        .collect()
}

#[test]
fn a_valid_batch_builds_the_tree_and_snapshot() {
    let (view, _) = app();
    let snapshot = view.snapshot();
    let button = &snapshot["tree"]["children"][0];
    assert_eq!(button["tag"], "button");
    assert_eq!(button["attributes"]["id"], "inc");
    assert_eq!(button["children"][0]["text"], "+");
    assert_eq!(snapshot["tree"]["children"][1]["tag"], "textarea");
    let a11y = &snapshot["accessibility"]["children"];
    assert_eq!(a11y[0]["role"], "button");
    assert_eq!(a11y[0]["name"], "+");
    assert_eq!(a11y[1]["role"], "textbox");
    assert_eq!(a11y[1]["multiline"], true);
}

#[test]
fn box_layout_computes_flex_geometry_and_rejects_unsupported_content() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "120px")
        .set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::Gap, "10px")
        .insert(1, 2, 0);
    for id in [3, 4] {
        batch
            .create_element(id, Tag::Div)
            .set_style(id, CssProperty::Width, "20px")
            .set_style(id, CssProperty::Height, "10px")
            .insert(2, id, 0);
    }
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        rects[&2],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 40.0
        }
    );
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 10.0
        }
    );
    assert_eq!(
        rects[&4],
        varta_view::BoxRect {
            x: 30.0,
            y: 0.0,
            width: 20.0,
            height: 10.0
        }
    );
    assert!(view.layout_boxes(f32::NAN, 100.0).is_err());
    let mut text = BatchWriter::new();
    text.create_text(5, "unmeasured").insert(3, 5, 0);
    commit(&mut view, &text).unwrap();
    assert!(matches!(
        view.layout_boxes(200.0, 100.0),
        Err(varta_view::LayoutError::Unsupported(5))
    ));
}

#[test]
fn box_layout_flex_growth_and_alignment_match_expected_rectangles() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "160px")
        .set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::AlignItems, "center")
        .insert(1, 2, 0);
    for (id, growth) in [(3, "1"), (4, "2")] {
        batch
            .create_element(id, Tag::Div)
            .set_style(id, CssProperty::Width, "20px")
            .set_style(id, CssProperty::Height, "10px")
            .set_style(id, CssProperty::FlexGrow, growth)
            .insert(2, id, 0);
    }
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 0.0,
            y: 15.0,
            width: 60.0,
            height: 10.0
        }
    );
    assert_eq!(
        rects[&4],
        varta_view::BoxRect {
            x: 60.0,
            y: 15.0,
            width: 100.0,
            height: 10.0
        }
    );
}

#[test]
fn box_layout_wraps_and_distributes_items() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "20px")
        .set_style(2, CssProperty::FlexWrap, "wrap")
        .set_style(2, CssProperty::JustifyContent, "space-between")
        .set_style(2, CssProperty::ColumnGap, "10px")
        .insert(1, 2, 0);
    for id in [3, 4, 5] {
        batch
            .create_element(id, Tag::Div)
            .set_style(id, CssProperty::Width, "40px")
            .set_style(id, CssProperty::Height, "10px")
            .insert(2, id, 0);
    }
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 10.0
        }
    );
    assert_eq!(
        rects[&4],
        varta_view::BoxRect {
            x: 60.0,
            y: 0.0,
            width: 40.0,
            height: 10.0
        }
    );
    assert_eq!(
        rects[&5],
        varta_view::BoxRect {
            x: 0.0,
            y: 10.0,
            width: 40.0,
            height: 10.0
        }
    );
}

#[test]
fn box_layout_nested_padding_margin_and_size_limits() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::Padding, "5px 10px")
        .insert(1, 2, 0);
    batch
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "20px")
        .set_style(3, CssProperty::Height, "10px")
        .set_style(3, CssProperty::MinWidth, "30px")
        .set_style(3, CssProperty::Margin, "2px 3px 4px 6px")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        rects[&2],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 50.0
        }
    );
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 16.0,
            y: 7.0,
            width: 30.0,
            height: 10.0
        }
    );
    let mut conflicting = BatchWriter::new();
    conflicting.set_style(3, CssProperty::MarginLeft, "1px");
    commit(&mut view, &conflicting).unwrap();
    assert_eq!(view.layout_boxes(200.0, 100.0).unwrap()[&3].x, 11.0);
    let mut invalid = BatchWriter::new();
    invalid
        .set_style(3, CssProperty::MarginLeft, "9px")
        .create_element(3, Tag::Div);
    assert!(commit(&mut view, &invalid).is_err());
    assert_eq!(view.layout_boxes(200.0, 100.0).unwrap()[&3].x, 11.0);
    let mut reset = BatchWriter::new();
    reset.set_style(3, CssProperty::Margin, "2px 3px 4px 6px");
    commit(&mut view, &reset).unwrap();
    assert_eq!(view.layout_boxes(200.0, 100.0).unwrap()[&3].x, 16.0);
    let mut remove_left = BatchWriter::new();
    remove_left.remove_style(3, CssProperty::MarginLeft);
    commit(&mut view, &remove_left).unwrap();
    assert_eq!(view.layout_boxes(200.0, 100.0).unwrap()[&3].x, 10.0);
    let mut remove_all = BatchWriter::new();
    remove_all.remove_style(3, CssProperty::Margin);
    commit(&mut view, &remove_all).unwrap();
    assert_eq!(view.layout_boxes(200.0, 100.0).unwrap()[&3].y, 5.0);
}

#[test]
fn box_layout_auto_and_negative_percentage_margins() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "200px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "100px")
        .set_style(3, CssProperty::Height, "10px")
        .set_style(3, CssProperty::Margin, "0 auto")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Display, "flex")
        .set_style(4, CssProperty::Width, "200px")
        .set_style(4, CssProperty::Height, "10px")
        .insert(2, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::Width, "50px")
        .set_style(5, CssProperty::Height, "10px")
        .set_style(5, CssProperty::MarginLeft, "-10%")
        .insert(4, 5, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(300.0, 100.0).unwrap();
    assert_eq!((rects[&3].x, rects[&3].y), (50.0, 0.0));
    assert_eq!((rects[&5].x, rects[&5].y), (-20.0, 10.0));
}

#[test]
fn box_layout_explicit_auto_and_none_sizing() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "200px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "auto")
        .set_style(3, CssProperty::Height, "auto")
        .set_style(3, CssProperty::MinWidth, "auto")
        .set_style(3, CssProperty::MinHeight, "auto")
        .set_style(3, CssProperty::MaxHeight, "none")
        .set_style(3, CssProperty::MaxWidth, "100px")
        .set_style(3, CssProperty::Margin, "0 auto")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "10px")
        .set_style(4, CssProperty::Height, "10px")
        .insert(3, 4, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(300.0, 100.0).unwrap();
    assert_eq!(
        (rects[&3].x, rects[&3].width, rects[&3].height),
        (50.0, 100.0, 10.0)
    );
}

#[test]
fn grid_tracks_placement_and_limits() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "grid")
        .set_style(2, CssProperty::Width, "500px")
        .set_style(2, CssProperty::Height, "80px")
        .set_style(2, CssProperty::GridTemplateColumns, "1fr 128px")
        .set_style(2, CssProperty::GridTemplateRows, "30px 40px")
        .set_style(2, CssProperty::Gap, "10px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .insert(2, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::GridColumn, "1 / 3")
        .set_style(5, CssProperty::GridRow, "2")
        .insert(2, 5, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(600.0, 200.0).unwrap();
    assert_eq!(
        (rects[&3].x, rects[&3].y, rects[&3].width, rects[&3].height),
        (0.0, 0.0, 362.0, 30.0)
    );
    assert_eq!((rects[&4].x, rects[&4].width), (372.0, 128.0));
    assert_eq!(
        (rects[&5].x, rects[&5].y, rects[&5].width, rects[&5].height),
        (0.0, 40.0, 500.0, 40.0)
    );
    let mut repeat = BatchWriter::new();
    repeat
        .set_style(2, CssProperty::Width, "220px")
        .set_style(
            2,
            CssProperty::GridTemplateColumns,
            "repeat(2, minmax(0, 1fr))",
        )
        .set_style(2, CssProperty::Gap, "20px")
        .remove_style(5, CssProperty::GridColumn)
        .remove_style(5, CssProperty::GridRow);
    commit(&mut view, &repeat).unwrap();
    let rects = view.layout_boxes(600.0, 200.0).unwrap();
    assert_eq!(
        (rects[&3].width, rects[&4].x, rects[&4].width),
        (100.0, 120.0, 100.0)
    );
    let mut excessive = BatchWriter::new();
    excessive.set_style(2, CssProperty::GridTemplateColumns, "repeat(65, 1fr)");
    commit(&mut view, &excessive).unwrap();
    assert!(matches!(
        view.layout_boxes(600.0, 200.0),
        Err(varta_view::LayoutError::Unsupported(2))
    ));
}

#[test]
fn scroll_offsets_affect_descendants_and_clamp_to_content() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Position, "relative")
        .set_style(2, CssProperty::Width, "120px")
        .set_style(2, CssProperty::Height, "60px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Overflow, "auto")
        .set_style(3, CssProperty::MarginLeft, "20px")
        .set_style(3, CssProperty::Width, "100px")
        .set_style(3, CssProperty::Height, "40px")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Height, "100px")
        .insert(3, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::Position, "absolute")
        .set_style(5, CssProperty::Top, "20px")
        .set_style(5, CssProperty::Left, "10px")
        .set_style(5, CssProperty::Width, "10px")
        .set_style(5, CssProperty::Height, "10px")
        .insert(4, 5, 0)
        .create_element(6, Tag::Div)
        .set_style(6, CssProperty::Position, "fixed")
        .set_style(6, CssProperty::Top, "30px")
        .set_style(6, CssProperty::Left, "0px")
        .set_style(6, CssProperty::Width, "10px")
        .set_style(6, CssProperty::Height, "10px")
        .insert(4, 6, 0);
    commit(&mut view, &batch).unwrap();
    for (top, child_y) in [(15.0, -15.0), (999.0, -60.0)] {
        view.input(
            &Input::Scroll {
                target: Target::Node(3),
                top,
            },
            &mut Fixed::default(),
        )
        .unwrap();
        let rects = view.layout_boxes(200.0, 100.0).unwrap();
        assert_eq!(rects[&4].y, child_y);
        assert_eq!((rects[&5].x, rects[&5].y), (10.0, 20.0));
        assert_eq!(rects[&6].y, 30.0);
    }
    let geometry = view.layout_geometry(200.0, 100.0).unwrap();
    let viewport = varta_view::BoxRect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 100.0,
    };
    let scroll_clip = varta_view::BoxRect {
        x: 20.0,
        y: 0.0,
        width: 100.0,
        height: 40.0,
    };
    assert_eq!(geometry[&3].clip, viewport);
    assert_eq!(geometry[&4].clip, scroll_clip);
    assert_eq!(geometry[&5].clip, viewport);
    assert_eq!(geometry[&6].clip, viewport);
    let mut positioned = BatchWriter::new();
    positioned.set_style(3, CssProperty::Position, "relative");
    commit(&mut view, &positioned).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!((rects[&5].x, rects[&5].y), (30.0, -40.0));
    assert_eq!(rects[&6].y, 30.0);
    let geometry = view.layout_geometry(200.0, 100.0).unwrap();
    assert_eq!(geometry[&5].clip, scroll_clip);
    assert_eq!(geometry[&6].clip, viewport);
}

#[test]
fn geometry_intersects_nested_padding_box_clips() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "60px")
        .set_style(2, CssProperty::Border, "5px solid")
        .set_style(2, CssProperty::Overflow, "hidden")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::MarginLeft, "80px")
        .set_style(3, CssProperty::MarginTop, "10px")
        .set_style(3, CssProperty::Width, "40px")
        .set_style(3, CssProperty::Height, "40px")
        .set_style(3, CssProperty::Border, "2px solid")
        .set_style(3, CssProperty::Overflow, "hidden")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "80px")
        .set_style(4, CssProperty::Height, "80px")
        .insert(3, 4, 0);
    commit(&mut view, &batch).unwrap();
    let geometry = view.layout_geometry(200.0, 100.0).unwrap();
    assert_eq!(
        geometry[&3].clip,
        varta_view::BoxRect {
            x: 5.0,
            y: 5.0,
            width: 100.0,
            height: 60.0
        }
    );
    assert_eq!(
        geometry[&4].clip,
        varta_view::BoxRect {
            x: 87.0,
            y: 17.0,
            width: 18.0,
            height: 40.0
        }
    );
}

#[test]
fn positioned_boxes_use_correct_containing_block_and_viewport() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Position, "relative")
        .set_style(2, CssProperty::MarginLeft, "20px")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "80px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::MarginLeft, "25px")
        .set_style(3, CssProperty::Width, "50px")
        .set_style(3, CssProperty::Height, "30px")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Position, "absolute")
        .set_style(4, CssProperty::Left, "10px")
        .set_style(4, CssProperty::Top, "20px")
        .set_style(4, CssProperty::Width, "10px")
        .set_style(4, CssProperty::Height, "10px")
        .insert(3, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::Position, "fixed")
        .set_style(5, CssProperty::Right, "15px")
        .set_style(5, CssProperty::Bottom, "10px")
        .set_style(5, CssProperty::Width, "10px")
        .set_style(5, CssProperty::Height, "10px")
        .insert(3, 5, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!((rects[&3].x, rects[&3].y), (45.0, 0.0));
    assert_eq!((rects[&4].x, rects[&4].y), (30.0, 20.0));
    assert_eq!((rects[&5].x, rects[&5].y), (175.0, 80.0));
}

#[test]
fn relative_offsets_preserve_flow_and_absolute_percent_uses_ancestor() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Position, "relative")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "50px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Position, "relative")
        .set_style(3, CssProperty::Left, "5px")
        .set_style(3, CssProperty::Top, "-3px")
        .set_style(3, CssProperty::Width, "10px")
        .set_style(3, CssProperty::Height, "10px")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "10px")
        .set_style(4, CssProperty::Height, "10px")
        .insert(2, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::Position, "absolute")
        .set_style(5, CssProperty::Left, "50%")
        .set_style(5, CssProperty::Top, "20%")
        .set_style(5, CssProperty::Width, "10px")
        .set_style(5, CssProperty::Height, "10px")
        .insert(2, 5, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!((rects[&3].x, rects[&3].y), (5.0, -3.0));
    assert_eq!((rects[&4].x, rects[&4].y), (0.0, 10.0));
    assert_eq!((rects[&5].x, rects[&5].y), (50.0, 10.0));
}

#[test]
fn box_layout_resolves_stylesheet_shorthands_before_inline_longhands() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch.stylesheet(1, ".box { padding: 2px 3px; padding-left: 7px; gap: 2px 6px; column-gap: 10px } .item { margin: 1px 2px; margin-left: 4px }")
        .create_element(2, Tag::Div).set_class(2, "box").set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px").set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::PaddingRight, "11px").insert(1, 2, 0);
    for id in [3, 4] {
        batch
            .create_element(id, Tag::Div)
            .set_class(id, "item")
            .set_style(id, CssProperty::Width, "20px")
            .set_style(id, CssProperty::Height, "10px")
            .insert(2, id, 0);
    }
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        rects[&2],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 118.0,
            height: 44.0
        }
    );
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 11.0,
            y: 3.0,
            width: 20.0,
            height: 10.0
        }
    );
    assert_eq!(
        rects[&4],
        varta_view::BoxRect {
            x: 47.0,
            y: 3.0,
            width: 20.0,
            height: 10.0
        }
    );
}

#[test]
fn box_layout_unresolved_variable_does_not_restore_earlier_shorthand() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .stylesheet(1, ".box { padding: 9px; padding: var(--missing) }")
        .create_element(2, Tag::Div)
        .set_class(2, "box")
        .set_style(2, CssProperty::Width, "30px")
        .set_style(2, CssProperty::Height, "10px")
        .insert(1, 2, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(rects[&2].width, 30.0);
    assert_eq!(rects[&2].height, 10.0);
}

#[test]
fn box_layout_percentage_dimensions_and_spacing() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "200px")
        .set_style(2, CssProperty::Height, "100px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "50%")
        .set_style(3, CssProperty::Height, "50%")
        .set_style(3, CssProperty::Padding, "5%")
        .set_style(3, CssProperty::MarginLeft, "10%")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "50%")
        .set_style(4, CssProperty::Height, "10px")
        .insert(3, 4, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(300.0, 200.0).unwrap();
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 20.0,
            y: 0.0,
            width: 120.0,
            height: 70.0
        }
    );
    assert_eq!(
        rects[&4],
        varta_view::BoxRect {
            x: 30.0,
            y: 10.0,
            width: 50.0,
            height: 10.0
        }
    );
}

#[test]
fn box_layout_percentage_gap_and_flex_basis() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "200px")
        .set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::Gap, "10%")
        .insert(1, 2, 0);
    for id in [3, 4] {
        batch
            .create_element(id, Tag::Div)
            .set_style(id, CssProperty::FlexBasis, "25%")
            .set_style(id, CssProperty::Height, "10px")
            .insert(2, id, 0);
    }
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(300.0, 100.0).unwrap();
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 10.0
        }
    );
    assert_eq!(
        rects[&4],
        varta_view::BoxRect {
            x: 70.0,
            y: 0.0,
            width: 50.0,
            height: 10.0
        }
    );
}

#[test]
fn text_editing_moves_selects_and_deletes_graphemes() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Input)
        .insert(1, 2, 0)
        .set_value(2, "A👩‍👩‍👧‍👦B", 0);
    commit(&mut view, &batch).unwrap();
    let key = |name: &str, shift| Input::Key {
        target: Target::Node(2),
        key: name.into(),
        shift,
        ctrl: false,
        alt: false,
        meta: false,
        composing: false,
    };
    view.input(&key("ArrowLeft", false), &mut Fixed::default())
        .unwrap();
    view.input(&key("Backspace", false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AB");
    view.input(&key("ArrowLeft", true), &mut Fixed::default())
        .unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "Z".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "ZB");
    view.input(&key("Delete", false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "Z");
    view.input(&key("Home", false), &mut Fixed::default())
        .unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "A".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let mut maxlength = BatchWriter::new();
    maxlength.set_attribute(2, Attribute::Maxlength, "3");
    commit(&mut view, &maxlength).unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "XY".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AXZ");
    let mut stale = BatchWriter::new();
    stale.set_value(2, "stale", 0);
    commit(&mut view, &stale).unwrap();
    view.input(&key("Backspace", false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AZ");
}

#[test]
fn select_command_focuses_enabled_attached_controls() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Input)
        .insert(1, 2, 0)
        .create_element(3, Tag::Textarea)
        .set_value(3, "abc", 0)
        .set_attribute(3, Attribute::Readonly, "")
        .listen(3, EventKind::Focusin, 10, Filter::default())
        .insert(1, 3, 0)
        .create_element(4, Tag::Input)
        .set_attribute(4, Attribute::Disabled, "")
        .insert(1, 4, 0)
        .create_element(5, Tag::Input)
        .set_attribute(5, Attribute::Type, "checkbox")
        .insert(1, 5, 0)
        .create_element(6, Tag::Input)
        .create_element(7, Tag::Div)
        .set_style(7, CssProperty::Display, "none")
        .insert(1, 7, 0)
        .create_element(8, Tag::Input)
        .insert(7, 8, 0)
        .command(2, Command::Focus, 0.0);
    commit(&mut view, &batch).unwrap();
    deliveries(&mut view);

    let mut select = BatchWriter::new();
    select.command(3, Command::Select, 0.0);
    commit(&mut view, &select).unwrap();
    assert_eq!(view.snapshot()["focus"], 3);
    assert!(
        deliveries(&mut view)
            .iter()
            .any(|e| e.0 == EventKind::Focusin && e.1 == 10)
    );
    for node in [4, 6, 8] {
        let mut select = BatchWriter::new();
        select.command(node, Command::Select, 0.0);
        commit(&mut view, &select).unwrap();
        assert_eq!(view.snapshot()["focus"], 3);
    }
    let mut focus_hidden = BatchWriter::new();
    focus_hidden.command(8, Command::Focus, 0.0);
    commit(&mut view, &focus_hidden).unwrap();
    assert_eq!(view.snapshot()["focus"], 3);
    let mut select = BatchWriter::new();
    select.command(5, Command::Select, 0.0);
    commit(&mut view, &select).unwrap();
    assert_eq!(view.snapshot()["focus"], 5);
}

#[test]
fn select_command_replaces_full_value_and_invalid_batch_preserves_caret() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Input)
        .insert(1, 2, 0)
        .set_value(2, "abc", 0)
        .listen(2, EventKind::Input, 10, Filter::default())
        .command(2, Command::Focus, 0.0);
    commit(&mut view, &batch).unwrap();
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 0,
            activate: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    deliveries(&mut view);

    let mut invalid = BatchWriter::new();
    invalid
        .command(2, Command::Select, 0.0)
        .command(2, Command::ShowModal, 0.0);
    assert!(commit(&mut view, &invalid).is_err());
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "X".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "Xabc");
    deliveries(&mut view);

    let mut select = BatchWriter::new();
    select.command(2, Command::Select, 0.0);
    commit(&mut view, &select).unwrap();
    assert!(deliveries(&mut view).is_empty());
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "Я".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "Я");
    assert_eq!(
        deliveries(&mut view)
            .iter()
            .filter(|e| e.0 == EventKind::Input)
            .count(),
        1
    );
}

#[test]
fn cpu_textarea_content_sizing_grows_wraps_shrinks_and_caps_height() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup.create_element(2, Tag::Textarea).insert(1, 2, 0);
    for (property, value) in [
        (CssProperty::Display, "block"),
        (CssProperty::Width, "60px"),
        (CssProperty::Height, "auto"),
        (CssProperty::MinWidth, "0"),
        (CssProperty::MinHeight, "0"),
        (CssProperty::MaxHeight, "60px"),
        (CssProperty::FieldSizing, "content"),
        (CssProperty::Resize, "none"),
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
        setup.set_style(2, property, value);
    }
    commit(&mut view, &setup).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    for (value, height) in [
        ("", 32.0),
        ("AA\nAA", 52.0),
        ("AAAAAAA", 52.0),
        ("AA\nAA\nAA", 60.0),
        ("A", 32.0),
    ] {
        let mut update = BatchWriter::new();
        update.set_value(2, value, 0);
        commit(&mut view, &update).unwrap();
        let boxes = view
            .layout_boxes_with_text(100.0, 100.0, &mut font, 20.0)
            .unwrap();
        assert_eq!(boxes[&2].height, height, "{value:?}");
        view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    }
    let mut minimum = BatchWriter::new();
    minimum.set_style(2, CssProperty::MinHeight, "40px");
    commit(&mut view, &minimum).unwrap();
    assert_eq!(
        view.layout_boxes_with_text(100.0, 100.0, &mut font, 20.0)
            .unwrap()[&2]
            .height,
        40.0
    );
}

#[test]
fn cpu_textarea_renders_multiple_lines_and_resolves_two_dimensional_caret_positions() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup
        .create_element(2, Tag::Textarea)
        .insert(1, 2, 0)
        .set_value(2, "AA\nAA", 0);
    for (property, value) in [
        (CssProperty::Display, "block"),
        (CssProperty::Width, "60px"),
        (CssProperty::Height, "60px"),
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
        setup.set_style(2, property, value);
    }
    commit(&mut view, &setup).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    assert_eq!(
        &image.pixels[(10 * 100 + 8) * 4..(10 * 100 + 9) * 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(
        &image.pixels[(30 * 100 + 8) * 4..(30 * 100 + 9) * 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(image.text_position(2, 6.0), None);
    assert_eq!(image.text_position_at(2, 6.0, 10.0), Some(0));
    assert_eq!(image.text_position_at(2, 6.0, 30.0), Some(3));
    let scaled = view
        .rasterize_scaled(200, 200, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    assert_eq!(scaled.text_position_at(2, 12.0, 60.0), Some(3));
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 3,
            focus: 3,
            activate: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    view.input(
        &Input::Key {
            target: Target::Node(2),
            key: "Enter".into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AA\n\nAA");
    assert!(
        view.rasterize_with_text(100, 100, &mut font, 20.0)
            .unwrap()
            .ime_cursor_area()
            .is_some()
    );
    let mut wrap = BatchWriter::new();
    wrap.set_value(2, "AAAAAAA", 1);
    commit(&mut view, &wrap).unwrap();
    let wrapped = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    assert_eq!(wrapped.text_position_at(2, 6.0, 30.0), Some(6));
    assert_eq!(wrapped.text_position_at(2, 99.0, 10.0), Some(6));
    assert_eq!(wrapped.text_position_at(2, 99.0, 30.0), Some(7));
    let mut long = BatchWriter::new();
    long.set_value(2, "AA\nAA\nAA\nAA\nAA", 1)
        .listen(2, EventKind::Scroll, 90, Filter::default());
    commit(&mut view, &long).unwrap();
    let frame = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    assert_eq!(
        frame.scroll_target(10.0, 10.0, -100.0),
        Some((2, 0.0, -48.0))
    );
    let metrics = frame.scroll_metrics(2).unwrap();
    assert_eq!((metrics.max, metrics.viewport.height), (52.0, 56.0));
    view.input(
        &Input::ScrollMeasured {
            target: Target::Node(2),
            top: 0.0,
            scroll_height: 108.0,
            client_height: 56.0,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let event = view.next_delivery().unwrap().record;
    assert_eq!(
        (
            event.handler,
            event.scroll_top,
            event.scroll_height,
            event.client_height
        ),
        (90, 0.0, 108.0, 56.0)
    );
    let before = view.snapshot();
    assert!(
        view.input(
            &Input::ScrollMeasured {
                target: Target::Node(2),
                top: 53.0,
                scroll_height: 108.0,
                client_height: 56.0
            },
            &mut Fixed::default()
        )
        .is_err()
    );
    assert_eq!(view.snapshot(), before);
    let frame = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    for (top, scroll_height, client_height) in [
        (f64::NAN, 108.0, 56.0),
        (0.0, f64::INFINITY, 56.0),
        (-1.0, 108.0, 56.0),
        (0.0, 40.0, 56.0),
        (0.0, 108.0, -1.0),
    ] {
        assert!(
            view.input(
                &Input::ScrollMeasured {
                    target: Target::Node(2),
                    top,
                    scroll_height,
                    client_height
                },
                &mut Fixed::default()
            )
            .is_err()
        );
        assert_eq!(view.snapshot(), before);
    }
    assert_eq!(frame.text_position_at(2, 6.0, 10.0), Some(0));
    assert!(frame.ime_cursor_area().is_none());
    assert_eq!(
        frame.scroll_target(10.0, 10.0, 999.0),
        Some((2, 52.0, 947.0))
    );
    view.input(
        &Input::Scroll {
            target: Target::Node(2),
            top: 999.0,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let frame = view
        .rasterize_scaled(200, 200, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    assert_eq!(frame.scroll_target(20.0, 20.0, 10.0), None);
    view.input(
        &Input::Scroll {
            target: Target::Node(2),
            top: 0.0,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "A".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert!(
        view.rasterize_with_text(100, 100, &mut font, 20.0)
            .unwrap()
            .ime_cursor_area()
            .is_some()
    );
    let mut parent = BatchWriter::new();
    parent
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "60px")
        .set_style(3, CssProperty::Height, "40px")
        .set_style(3, CssProperty::Overflow, "auto")
        .insert(1, 3, 0)
        .insert(3, 2, 0);
    commit(&mut view, &parent).unwrap();
    let nested = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    assert_eq!(nested.scroll_target(10.0, 10.0, 10.0), Some((3, 10.0, 0.0)));
    fn navigate(
        view: &mut View,
        font: &mut varta_view::TextMeasurer,
        key: &str,
        shift: bool,
    ) -> varta_view::TextNavigation {
        let frame = view
            .rasterize_scaled(200, 200, 2.0, Some((font, 20.0)))
            .unwrap();
        let movement = frame.text_navigation(2, key).unwrap();
        view.input(
            &Input::TextNavigate {
                target: Target::Node(2),
                key: key.into(),
                position: movement.position,
                preferred_x: movement.preferred_x,
                upstream: movement.upstream,
                shift,
            },
            &mut Fixed::default(),
        )
        .unwrap();
        movement
    }
    let seq = view.snapshot()["tree"]["children"][0]["children"][0]["seq"]
        .as_u64()
        .unwrap() as u32;
    let mut navigation = BatchWriter::new();
    navigation
        .insert(1, 2, 0)
        .remove(3)
        .set_value(2, "AAAA\nA\nAAAA", seq);
    commit(&mut view, &navigation).unwrap();
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 3,
            focus: 3,
            activate: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(
        navigate(&mut view, &mut font, "ArrowDown", false).position,
        6
    );
    assert_eq!(
        navigate(&mut view, &mut font, "ArrowDown", false).position,
        10
    );
    assert_eq!(navigate(&mut view, &mut font, "ArrowUp", false).position, 6);
    assert_eq!(navigate(&mut view, &mut font, "ArrowUp", false).position, 3);
    assert_eq!(navigate(&mut view, &mut font, "End", false).position, 4);
    assert_eq!(
        navigate(&mut view, &mut font, "ArrowDown", true).position,
        6
    );
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "Z".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["value"],
        "AAAAZ\nAAAA"
    );
    let seq = view.snapshot()["tree"]["children"][0]["seq"]
        .as_u64()
        .unwrap() as u32;
    let mut soft = BatchWriter::new();
    soft.set_value(2, "AAAAAAA", seq);
    commit(&mut view, &soft).unwrap();
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 0,
            activate: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let end = navigate(&mut view, &mut font, "End", false);
    assert_eq!(end.position, 6);
    assert!(end.upstream);
    assert_eq!(navigate(&mut view, &mut font, "Home", false).position, 0);
    assert_eq!(
        navigate(&mut view, &mut font, "ArrowDown", false).position,
        6
    );
    let mut prevent = BatchWriter::new();
    prevent.listen(
        2,
        EventKind::Keydown,
        10,
        Filter {
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut view, &prevent).unwrap();
    navigate(&mut view, &mut font, "End", false);
    let frame = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    assert_eq!(frame.text_navigation(2, "ArrowUp").unwrap().position, 0);
    let mut allow = BatchWriter::new();
    allow.unlisten(2, EventKind::Keydown, 10);
    commit(&mut view, &allow).unwrap();
    navigate(&mut view, &mut font, "ArrowUp", false);
    navigate(&mut view, &mut font, "End", false);
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "X".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    view.input(
        &Input::TextKey {
            target: Target::Node(2),
            key: "z".into(),
            text: String::new(),
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
            select_all: false,
            history: None,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let frame = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    assert_eq!(frame.text_navigation(2, "Home").unwrap().position, 0);
}

#[test]
fn cpu_text_input_renders_edits_caret_and_enforces_value_limit() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Input)
        .set_style(2, CssProperty::Display, "block")
        .set_style(2, CssProperty::Width, "80px")
        .set_style(2, CssProperty::Height, "30px")
        .set_style(2, CssProperty::MinWidth, "0")
        .set_style(2, CssProperty::MinHeight, "0")
        .set_style(2, CssProperty::Padding, "4px")
        .set_style(2, CssProperty::Border, "2px solid #000")
        .set_style(2, CssProperty::BorderRadius, "0")
        .set_style(2, CssProperty::Outline, "none")
        .set_style(2, CssProperty::BackgroundColor, "#fff")
        .set_style(2, CssProperty::Color, "#00f")
        .set_style(2, CssProperty::FontFamily, "\"Varta Fixture 400\"")
        .set_style(2, CssProperty::FontSize, "20px")
        .set_style(2, CssProperty::FontWeight, "normal")
        .set_style(2, CssProperty::FontStyle, "normal")
        .set_style(2, CssProperty::LineHeight, "20px")
        .insert(1, 2, 0)
        .set_value(2, "AA", 0)
        .listen(2, EventKind::Input, 10, Filter::default());
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    let pixel = |x: usize, y: usize| &image.pixels[(y * 100 + x) * 4..(y * 100 + x + 1) * 4];
    assert_eq!(pixel(0, 0), &[0, 0, 0, 255]);
    assert_eq!(pixel(8, 10), &[0, 0, 255, 255]);
    assert_eq!(pixel(22, 10), &[255, 255, 255, 255]);
    assert_eq!(image.editable(), None);
    assert_eq!(image.text_position(2, 8.0), Some(0));
    assert_eq!(image.text_position(2, 12.0), Some(1));
    assert_eq!(image.text_position(2, 24.0), Some(2));
    view.input(
        &Input::Click {
            target: Target::Node(2),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(image.editable(), Some(2));
    let scaled = view
        .rasterize_scaled(200, 100, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    assert_eq!(
        &scaled.pixels[(20 * 200 + 16) * 4..(20 * 200 + 17) * 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(scaled.text_position(2, 24.0), image.text_position(2, 12.0));
    let caret = image.ime_cursor_area().unwrap();
    let scaled_caret = scaled.ime_cursor_area().unwrap();
    assert_eq!(
        (
            scaled_caret.x,
            scaled_caret.y,
            scaled_caret.width,
            scaled_caret.height
        ),
        (
            caret.x * 2.0,
            caret.y * 2.0,
            caret.width * 2.0,
            caret.height * 2.0
        )
    );
    assert_eq!(
        &image.pixels[(10 * 100 + 22) * 4..(10 * 100 + 23) * 4],
        &[0, 0, 255, 255]
    );
    let key = |name: &str, shift| Input::Key {
        target: Target::Node(2),
        key: name.into(),
        shift,
        ctrl: false,
        alt: false,
        meta: false,
        composing: false,
    };
    view.input(&key("ArrowLeft", false), &mut Fixed::default())
        .unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(
        &image.pixels[(10 * 100 + 14) * 4..(10 * 100 + 15) * 4],
        &[0, 0, 255, 255]
    );
    view.input(&key("ArrowLeft", true), &mut Fixed::default())
        .unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(
        &image.pixels[(10 * 100 + 13) * 4..(10 * 100 + 14) * 4],
        &[160, 200, 255, 255]
    );
    view.input(&key("End", false), &mut Fixed::default())
        .unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "A".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AAA");
    assert!(
        deliveries(&mut view)
            .iter()
            .any(|item| item.0 == EventKind::Input && item.1 == 10)
    );
    let mut prevent = BatchWriter::new();
    prevent.listen(
        2,
        EventKind::Keydown,
        11,
        Filter {
            options: ListenOption::PreventDefault as u32,
            key: Key::Space.code(),
            ..Filter::default()
        },
    );
    commit(&mut view, &prevent).unwrap();
    let space = Input::Key {
        target: Target::Node(2),
        key: "Space".into(),
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
        composing: false,
    };
    view.input(&space, &mut Fixed::default()).unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AAA");
    let mut allow = BatchWriter::new();
    allow.unlisten(2, EventKind::Keydown, 11);
    commit(&mut view, &allow).unwrap();
    view.input(&space, &mut Fixed::default()).unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AAA ");
    view.input(
        &Input::Key {
            target: Target::Node(2),
            key: "Backspace".into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AAA");
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "A".repeat(32_766),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AAA");
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "A".repeat(32_769),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AAA");
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "Ф".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    // An uncovered character uses the explicitly supplied font's .notdef glyph.
    assert!(view.rasterize_with_text(100, 50, &mut font, 20.0).is_ok());
    let mut click_listener = BatchWriter::new();
    click_listener.listen(2, EventKind::Click, 12, Filter::default());
    commit(&mut view, &click_listener).unwrap();
    deliveries(&mut view);
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 1,
            activate: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert!(
        !deliveries(&mut view)
            .iter()
            .any(|item| item.0 == EventKind::Click)
    );
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 1,
            activate: true,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "Ф".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "ФAAФ");
    assert!(matches!(
        view.input(
            &Input::TextSelect {
                target: Target::Node(2),
                anchor: 1,
                focus: 1,
                activate: true,
            },
            &mut Fixed::default()
        ),
        Err(varta_view::InputError::Selection)
    ));
    view.input(&key("End", false), &mut Fixed::default())
        .unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "A".repeat(20),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert!(image.text_position(2, 6.0).unwrap() > 0);
    deliveries(&mut view);
    let before = view.snapshot()["tree"]["children"][0].clone();
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "Ф".into(),
            cursor: Some((2, 2)),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let during = view.snapshot()["tree"]["children"][0].clone();
    assert_eq!(during["value"], before["value"]);
    assert_eq!(during["seq"], before["seq"]);
    assert!(deliveries(&mut view).is_empty());
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(image.text_position(2, 6.0), None);
    assert!(image.ime_cursor_area().is_some());
    assert!(matches!(
        view.input(
            &Input::ImePreedit {
                target: Target::Node(2),
                text: "Ф".into(),
                cursor: Some((1, 1))
            },
            &mut Fixed::default()
        ),
        Err(varta_view::InputError::Selection)
    ));
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: String::new(),
            cursor: None,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    view.input(
        &Input::ImeCommit {
            target: Target::Node(2),
            text: "Ф".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert!(
        view.snapshot()["tree"]["children"][0]["value"]
            .as_str()
            .unwrap()
            .ends_with('Ф')
    );
    assert_eq!(
        deliveries(&mut view)
            .iter()
            .filter(|item| item.0 == EventKind::Input)
            .count(),
        1
    );
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "A".into(),
            cursor: None,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    view.cancel_preedit();
    assert!(
        view.rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .text_position(2, 6.0)
            .is_some()
    );

    // A composition replaces the selection only at commit, including when
    // the selected and replacement strings have different UTF-8 lengths.
    let original = view.snapshot()["tree"]["children"][0]["value"]
        .as_str()
        .unwrap()
        .to_owned();
    let selected_end = original.chars().next().unwrap().len_utf8();
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: selected_end,
            activate: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    deliveries(&mut view);
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "A".into(),
            cursor: Some((0, 1)),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], original);
    assert!(deliveries(&mut view).is_empty());
    view.input(
        &Input::ImeCommit {
            target: Target::Node(2),
            text: "A".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["value"],
        format!("A{}", &original[selected_end..])
    );
    assert_eq!(
        deliveries(&mut view)
            .iter()
            .filter(|item| item.0 == EventKind::Input)
            .count(),
        1
    );

    let seq = view.snapshot()["tree"]["children"][0]["seq"]
        .as_u64()
        .unwrap() as u32;
    let mut placeholder = BatchWriter::new();
    placeholder
        .set_value(2, "", seq)
        .set_attribute(2, Attribute::Placeholder, "A");
    commit(&mut view, &placeholder).unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(
        &image.pixels[(10 * 100 + 8) * 4..(10 * 100 + 9) * 4],
        &[117, 117, 117, 255]
    );
    assert_eq!(image.text_position(2, 12.0), Some(0));
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "");
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "A".into(),
            cursor: Some((1, 1)),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(
        &image.pixels[(10 * 100 + 8) * 4..(10 * 100 + 9) * 4],
        &[0, 0, 255, 255]
    );
    let mut readonly = BatchWriter::new();
    readonly.set_attribute(2, Attribute::Readonly, "");
    commit(&mut view, &readonly).unwrap();
    assert!(
        view.rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .text_position(2, 8.0)
            .is_some()
    );
    let mut writable = BatchWriter::new();
    writable.remove_attribute(2, Attribute::Readonly);
    commit(&mut view, &writable).unwrap();
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "A".into(),
            cursor: Some((1, 1)),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let mut disabled = BatchWriter::new();
    disabled.set_attribute(2, Attribute::Disabled, "");
    commit(&mut view, &disabled).unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(image.editable(), None);
    assert_eq!(
        &image.pixels[(10 * 100 + 8) * 4..(10 * 100 + 9) * 4],
        &[117, 117, 117, 255]
    );
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "A".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "");
    assert!(deliveries(&mut view).is_empty());
}

#[test]
fn ime_commit_is_one_undo_step_and_history_respects_readonly_prevention_and_echoes() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup
        .create_element(2, Tag::Input)
        .insert(1, 2, 0)
        .set_value(2, "AB", 0);
    commit(&mut view, &setup).unwrap();
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 1,
            activate: false,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    for text in ["я", "Я"] {
        view.input(
            &Input::ImePreedit {
                target: Target::Node(2),
                text: text.into(),
                cursor: Some((2, 2)),
            },
            &mut Fixed::default(),
        )
        .unwrap();
    }
    view.input(
        &Input::ImeCommit {
            target: Target::Node(2),
            text: "Я".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let undo = Input::TextKey {
        target: Target::Node(2),
        key: "z".into(),
        text: String::new(),
        shift: false,
        ctrl: true,
        alt: false,
        meta: false,
        select_all: false,
        history: None,
    };
    let mut readonly = BatchWriter::new();
    readonly.set_attribute(2, Attribute::Readonly, "");
    commit(&mut view, &readonly).unwrap();
    view.input(&undo, &mut Fixed::default()).unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "ЯB");
    let mut prevent = BatchWriter::new();
    prevent.remove_attribute(2, Attribute::Readonly).listen(
        2,
        EventKind::Keydown,
        10,
        Filter {
            options: ListenOption::PreventDefault as u32,
            ..Filter::default()
        },
    );
    commit(&mut view, &prevent).unwrap();
    view.input(&undo, &mut Fixed::default()).unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "ЯB");
    let seq = view.snapshot()["tree"]["children"][0]["seq"]
        .as_u64()
        .unwrap() as u32;
    let mut allow_and_echo = BatchWriter::new();
    allow_and_echo
        .unlisten(2, EventKind::Keydown, 10)
        .set_value(2, "ЯB", seq)
        .set_value(2, "stale", 0);
    commit(&mut view, &allow_and_echo).unwrap();
    view.input(&undo, &mut Fixed::default()).unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "AB");
    view.input(
        &Input::Type {
            target: Target::Node(2),
            text: "Z".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "ZB");
}

#[test]
fn undo_redo_restores_edits_and_stops_at_guest_value_changes_and_history_limit() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup
        .create_element(2, Tag::Input)
        .insert(1, 2, 0)
        .set_value(2, "A", 0)
        .listen(2, EventKind::Input, 10, Filter::default());
    commit(&mut view, &setup).unwrap();
    let edit = |text: &str| Input::Type {
        target: Target::Node(2),
        text: text.into(),
    };
    let history = |action| Input::TextKey {
        target: Target::Node(2),
        key: "я".into(),
        text: String::new(),
        shift: false,
        ctrl: false,
        alt: false,
        meta: true,
        select_all: false,
        history: Some(action),
    };
    let value = |view: &View| {
        view.snapshot()["tree"]["children"][0]["value"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    view.input(&edit("Б"), &mut Fixed::default()).unwrap();
    view.input(
        &history(varta_view::HistoryAction::Undo),
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(value(&view), "A");
    assert_eq!(deliveries(&mut view).len(), 1);
    view.input(
        &history(varta_view::HistoryAction::Redo),
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(value(&view), "AБ");
    view.input(
        &history(varta_view::HistoryAction::Undo),
        &mut Fixed::default(),
    )
    .unwrap();
    view.input(&edit("X"), &mut Fixed::default()).unwrap();
    view.input(
        &history(varta_view::HistoryAction::Redo),
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(value(&view), "AX");
    let seq = view.snapshot()["tree"]["children"][0]["seq"]
        .as_u64()
        .unwrap() as u32;
    let mut patch = BatchWriter::new();
    patch.set_value(2, "server", seq);
    commit(&mut view, &patch).unwrap();
    view.input(
        &history(varta_view::HistoryAction::Undo),
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(value(&view), "server");
    for _ in 0..80 {
        view.input(&edit("a"), &mut Fixed::default()).unwrap();
    }
    for _ in 0..80 {
        view.input(
            &history(varta_view::HistoryAction::Undo),
            &mut Fixed::default(),
        )
        .unwrap();
    }
    assert_eq!(value(&view), format!("server{}", "a".repeat(16)));
}

#[test]
fn character_keydown_can_prevent_typing_and_select_all_before_replacement() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup
        .create_element(2, Tag::Input)
        .set_value(2, "abc", 0)
        .listen(
            2,
            EventKind::Keydown,
            10,
            Filter {
                options: ListenOption::PreventDefault as u32,
                ..Filter::default()
            },
        )
        .listen(2, EventKind::Input, 11, Filter::default())
        .insert(1, 2, 0)
        .command(2, Command::Focus, 0.0);
    commit(&mut view, &setup).unwrap();
    let key = |name: &str, text: &str, ctrl, alt| Input::TextKey {
        target: Target::Node(2),
        key: name.into(),
        text: text.into(),
        shift: false,
        ctrl,
        alt,
        meta: false,
        select_all: false,
        history: None,
    };
    view.input(&key("a", "a", false, false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "abc");
    let events = deliveries(&mut view);
    assert_eq!(events.len(), 1);
    assert_eq!(
        (events[0].0, events[0].2.as_str(), events[0].3),
        (EventKind::Keydown, "a", true)
    );

    let mut allow = BatchWriter::new();
    allow
        .unlisten(2, EventKind::Keydown, 10)
        .listen(2, EventKind::Keydown, 12, Filter::default());
    commit(&mut view, &allow).unwrap();
    view.input(&key("a", "", true, false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "abc");
    assert_eq!(deliveries(&mut view).len(), 1);
    view.input(&key("Я", "Я", false, false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "Я");
    assert_eq!(
        deliveries(&mut view)
            .iter()
            .map(|e| e.0)
            .collect::<Vec<_>>(),
        [EventKind::Keydown, EventKind::Input]
    );
    view.input(&key("€", "€", true, true), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "Я€");
    view.input(
        &Input::TextKey {
            target: Target::Node(2),
            key: "ф".into(),
            text: String::new(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: true,
            select_all: true,
            history: None,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    view.input(&key("z", "z", false, false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "z");
}

#[test]
fn disabled_focused_input_keeps_keydown_but_cannot_submit_or_edit() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Form)
        .listen(2, EventKind::Submit, 20, Filter::default())
        .insert(1, 2, 0)
        .create_element(3, Tag::Input)
        .set_value(3, "abc", 0)
        .listen(3, EventKind::Keydown, 30, Filter::default())
        .listen(3, EventKind::Input, 31, Filter::default())
        .insert(2, 3, 0)
        .command(3, Command::Focus, 0.0)
        .set_attribute(3, Attribute::Disabled, "");
    commit(&mut view, &batch).unwrap();
    deliveries(&mut view);
    for key in ["Enter", "Backspace"] {
        view.input(
            &Input::Key {
                target: Target::Node(3),
                key: key.into(),
                shift: false,
                ctrl: false,
                alt: false,
                meta: false,
                composing: false,
            },
            &mut Fixed::default(),
        )
        .unwrap();
    }
    let events = deliveries(&mut view);
    assert_eq!(
        events.iter().map(|e| e.0).collect::<Vec<_>>(),
        [EventKind::Keydown, EventKind::Keydown]
    );
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["children"][0]["value"],
        "abc"
    );
}

#[test]
fn keyboard_tab_skips_hidden_ancestors_and_uses_updated_viewport() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .stylesheet(
            1,
            "@media (max-width: 400px) { .hidden-small { display: none } }",
        )
        .create_element(2, Tag::Div)
        .set_class(2, "hidden-small")
        .insert(1, 2, 0)
        .create_element(3, Tag::Button)
        .insert(2, 3, 0)
        .create_element(4, Tag::Button)
        .insert(1, 4, 0)
        .listen(4, EventKind::Click, 77, Filter::default());
    commit(&mut view, &batch).unwrap();
    view.set_environment(Environment {
        width: 300.0,
        dark: false,
        ..Default::default()
    })
    .unwrap();
    let key = |target, key: &str, shift| Input::Key {
        target: Target::Node(target),
        key: key.into(),
        shift,
        ctrl: false,
        alt: false,
        meta: false,
        composing: false,
    };
    view.input(&key(View::root(), "Tab", false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["focus"], 4);
    view.input(&key(4, "Enter", false), &mut Fixed::default())
        .unwrap();
    assert!(
        deliveries(&mut view)
            .iter()
            .any(|record| record.0 == EventKind::Click && record.1 == 77)
    );
    view.set_environment(Environment {
        width: 800.0,
        dark: false,
        ..Default::default()
    })
    .unwrap();
    view.input(&key(4, "Tab", false), &mut Fixed::default())
        .unwrap();
    assert_eq!(view.snapshot()["focus"], 3);
    assert!(
        view.set_environment(Environment {
            width: f64::NAN,
            dark: false,
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn device_scale_preserves_css_viewport_and_scales_clips_and_hit_regions() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup.stylesheet(1, ".box { background-color:#0f0 } @media (max-width:25px) { .box { background-color:#f00 } }")
        .create_element(2, Tag::Div).set_style(2, CssProperty::Width, "10px")
        .set_style(2, CssProperty::Height, "10px").set_style(2, CssProperty::Margin, "5px")
        .set_style(2, CssProperty::Overflow, "hidden").insert(1, 2, 0)
        .create_element(3, Tag::Div).set_class(3, "box")
        .set_style(3, CssProperty::Width, "20px").set_style(3, CssProperty::Height, "20px")
        .set_style(3, CssProperty::Border, "2px solid #00f").insert(2, 3, 0);
    commit(&mut view, &setup).unwrap();
    let frame = view.rasterize_scaled(40, 40, 2.0, None).unwrap();
    assert_eq!(frame.scale(), 2.0);
    let pixel = |x: usize, y: usize| &frame.pixels[(y * 40 + x) * 4..(y * 40 + x + 1) * 4];
    assert_eq!(pixel(10, 12), &[0, 0, 255, 255]);
    assert_eq!(pixel(28, 20), &[255, 0, 0, 255]);
    assert_eq!(pixel(30, 20), &[0, 0, 0, 0]);
    assert_eq!(frame.hit_test(29.0, 20.0), Some(3));
    assert_eq!(frame.hit_test(30.0, 20.0), None);
    let fractional = view.rasterize_scaled(30, 30, 1.5, None).unwrap();
    assert_eq!(
        &fractional.pixels[(15 * 30 + 21) * 4..(15 * 30 + 22) * 4],
        &[255, 0, 0, 255]
    );
    assert_eq!(
        &fractional.pixels[(15 * 30 + 23) * 4..(15 * 30 + 24) * 4],
        &[0, 0, 0, 0]
    );
    assert_eq!(fractional.hit_test(22.0, 15.0), Some(3));
    assert_eq!(fractional.hit_test(23.0, 15.0), None);
    for scale in [0.0, f32::NAN, f32::INFINITY, 9.0] {
        assert!(view.rasterize_scaled(40, 40, scale, None).is_err());
    }
}

#[test]
fn cpu_outline_tracks_focus_cascade_and_does_not_expand_hit_regions() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup
        .stylesheet(
            1,
            ".box:focus-visible { outline: 2px solid #00f; outline-offset: 2px }",
        )
        .create_element(2, Tag::Div)
        .set_class(2, "box")
        .set_attribute(2, Attribute::Tabindex, "0")
        .set_style(2, CssProperty::Width, "20px")
        .set_style(2, CssProperty::Height, "20px")
        .set_style(2, CssProperty::Margin, "10px")
        .set_style(2, CssProperty::BackgroundColor, "#f00")
        .insert(1, 2, 0);
    commit(&mut view, &setup).unwrap();
    let rects = view.layout_boxes(60.0, 60.0).unwrap();
    let image = view.rasterize_boxes(60, 60).unwrap();
    let pixel = |image: &varta_view::RasterImage, x: usize, y: usize| {
        image.pixels[(y * 60 + x) * 4..(y * 60 + x + 1) * 4].to_vec()
    };
    assert_eq!(pixel(&image, 6, 12), [0, 0, 0, 0]);
    view.input(
        &Input::Focus {
            target: Target::Node(2),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let image = view.rasterize_boxes(60, 60).unwrap();
    assert_eq!(pixel(&image, 6, 12), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 8, 12), [0, 0, 0, 0]);
    assert_eq!(image.hit_test(6.0, 12.0), None);
    assert_eq!(view.layout_boxes(60.0, 60.0).unwrap(), rects);
    let mut override_style = BatchWriter::new();
    override_style
        .set_style(2, CssProperty::OutlineWidth, "4px")
        .set_style(2, CssProperty::Outline, "1px solid #0f0");
    commit(&mut view, &override_style).unwrap();
    let image = view.rasterize_boxes(60, 60).unwrap();
    assert_eq!(pixel(&image, 6, 12), [0, 0, 0, 0]);
    assert_eq!(pixel(&image, 7, 12), [0, 255, 0, 255]);
    let mut remove = BatchWriter::new();
    remove.remove_style(2, CssProperty::Outline);
    commit(&mut view, &remove).unwrap();
    assert_eq!(
        pixel(&view.rasterize_boxes(60, 60).unwrap(), 6, 12),
        [0, 0, 255, 255]
    );
}

#[test]
fn cpu_raster_paints_explicit_button_and_dispatches_click() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Button)
        .set_attribute(2, Attribute::Type, "button")
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "80px")
        .set_style(2, CssProperty::Height, "30px")
        .set_style(2, CssProperty::MinWidth, "0")
        .set_style(2, CssProperty::MinHeight, "0")
        .set_style(2, CssProperty::Padding, "4px")
        .set_style(2, CssProperty::Border, "2px solid #00f")
        .set_style(2, CssProperty::BorderRadius, "0")
        .set_style(2, CssProperty::Outline, "none")
        .set_style(2, CssProperty::BackgroundColor, "#f00")
        .set_style(2, CssProperty::Color, "#000")
        .set_style(2, CssProperty::JustifyContent, "center")
        .set_style(2, CssProperty::AlignItems, "center")
        .set_style(2, CssProperty::FontFamily, "\"Varta Fixture 400\"")
        .set_style(2, CssProperty::FontSize, "20px")
        .set_style(2, CssProperty::FontWeight, "normal")
        .set_style(2, CssProperty::FontStyle, "normal")
        .set_style(2, CssProperty::LineHeight, "20px")
        .insert(1, 2, 0)
        .create_text(3, "AAAA")
        .insert(2, 3, 0)
        .listen(2, EventKind::Click, 10, Filter::default());
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    let pixel = |x: usize, y: usize| &image.pixels[(y * 100 + x) * 4..(y * 100 + x + 1) * 4];
    assert_eq!(pixel(0, 0), &[0, 0, 255, 255]);
    assert_eq!(pixel(4, 4), &[255, 0, 0, 255]);
    assert_eq!(pixel(26, 10), &[0, 0, 0, 255]);
    let target = image.hit_test(26.0, 10.0).unwrap();
    assert_eq!(target, 2);
    view.input(
        &Input::Click {
            target: Target::Node(target),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert_eq!(view.snapshot()["focus"], 2);
    assert_eq!(deliveries(&mut view)[0].1, 10);
    assert!(view.rasterize_with_text(100, 50, &mut font, 20.0).is_ok());
    let mut disabled = BatchWriter::new();
    disabled
        .set_attribute(2, Attribute::Disabled, "")
        .set_style(2, CssProperty::BackgroundColor, "#0f0");
    commit(&mut view, &disabled).unwrap();
    let image = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
    assert_eq!(
        &image.pixels[(4 * 100 + 4) * 4..(4 * 100 + 5) * 4],
        &[0, 255, 0, 255]
    );
    view.input(
        &Input::Click {
            target: Target::Node(2),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    assert!(deliveries(&mut view).is_empty());
}

#[test]
fn layout_commit_scrolls_in_batch_order_and_rolls_back_on_failure() {
    let mut view = view();
    let mut setup = BatchWriter::new();
    setup
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "50px")
        .set_style(2, CssProperty::Overflow, "auto")
        .listen(2, EventKind::Scroll, 20, Filter::default())
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Height, "120px")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Height, "10px")
        .insert(2, 4, 0);
    commit(&mut view, &setup).unwrap();

    let mut batch = BatchWriter::new();
    batch
        .command(4, Command::ScrollIntoView, 0.0)
        .set_style(3, CssProperty::Height, "160px");
    view.commit_with_layout(
        &batch.finish(),
        false,
        &mut Fixed::default(),
        200.0,
        100.0,
        None,
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["scrollTop"], 80.0);
    let event = view.next_delivery().unwrap();
    assert_eq!((event.record.handler, event.record.scroll_top), (20, 80.0));
    assert!(!event.gesture);
    assert!(view.next_delivery().is_none());

    let mut ordered = BatchWriter::new();
    ordered
        .command(4, Command::ScrollIntoView, 0.0)
        .command(2, Command::ScrollTo, 10.0);
    view.commit_with_layout(
        &ordered.finish(),
        false,
        &mut Fixed::default(),
        200.0,
        100.0,
        None,
    )
    .unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][0]["scrollTop"], 10.0);
    let event = view.next_delivery().unwrap();
    assert_eq!((event.record.handler, event.record.scroll_top), (20, 10.0));
    assert!(view.next_delivery().is_none());

    let before = view.snapshot();
    let mut invalid = BatchWriter::new();
    invalid
        .create_text(5, "A")
        .insert(4, 5, 0)
        .command(4, Command::ScrollIntoView, 0.0);
    assert!(
        view.commit_with_layout(
            &invalid.finish(),
            false,
            &mut Fixed::default(),
            200.0,
            100.0,
            None
        )
        .is_err()
    );
    assert_eq!(view.snapshot(), before);
    assert!(view.next_delivery().is_none());

    let mut excessive = BatchWriter::new();
    for _ in 0..=DRAFT_MAX_PENDING_COMMANDS {
        excessive.command(4, Command::ScrollIntoView, 0.0);
    }
    assert_eq!(
        violation(view.commit_with_layout(
            &excessive.finish(),
            false,
            &mut Fixed::default(),
            200.0,
            100.0,
            None
        )),
        Violation::TooManyPendingCommands
    );
    assert_eq!(view.snapshot(), before);

    let mut replace = BatchWriter::new();
    replace
        .command(2, Command::ScrollTo, 30.0)
        .remove(2)
        .create_element(2, Tag::Div)
        .listen(2, EventKind::Scroll, 20, Filter::default())
        .insert(1, 2, 0);
    view.commit_with_layout(
        &replace.finish(),
        false,
        &mut Fixed::default(),
        200.0,
        100.0,
        None,
    )
    .unwrap();
    assert!(view.next_delivery().is_none());
}

#[test]
fn scroll_into_view_plan_handles_nested_and_hidden_scrollports() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "50px")
        .set_style(2, CssProperty::Overflow, "auto")
        .listen(2, EventKind::Scroll, 20, Filter::default())
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Height, "80px")
        .set_style(3, CssProperty::Overflow, "hidden")
        .listen(3, EventKind::Scroll, 30, Filter::default())
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Height, "120px")
        .insert(3, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::Height, "10px")
        .insert(3, 5, 0);
    commit(&mut view, &batch).unwrap();
    let identity = view.node_identity(5).unwrap();
    let mut invalid = BatchWriter::new();
    invalid
        .remove(5)
        .create_element(5, Tag::Div)
        .create_element(5, Tag::Div);
    assert!(commit(&mut view, &invalid).is_err());
    assert_eq!(view.resolve_node_identity(&identity), Some(5));
    let mut other = View::default();
    commit(&mut other, &batch).unwrap();
    assert_eq!(other.resolve_node_identity(&identity), None);
    let mut fork = view.clone();
    let mut recreate = BatchWriter::new();
    recreate
        .remove(5)
        .create_element(5, Tag::Div)
        .insert(3, 5, 0);
    commit(&mut fork, &recreate).unwrap();
    assert_eq!(fork.resolve_node_identity(&identity), None);
    assert_eq!(view.resolve_node_identity(&identity), Some(5));
    let before = view.snapshot();
    assert_eq!(
        view.scroll_into_view_plan(5, 200.0, 100.0, None).unwrap(),
        vec![(3, 50.0), (2, 30.0)]
    );
    assert_eq!(view.snapshot(), before);
    assert_eq!(
        view.scroll_into_view_identity(&identity, 200.0, 100.0, None)
            .unwrap(),
        2
    );
    let events: Vec<_> = std::iter::from_fn(|| view.next_delivery()).collect();
    assert_eq!(events.len(), 2);
    assert_eq!(
        (events[0].record.handler, events[0].record.scroll_top),
        (30, 50.0)
    );
    assert_eq!(
        (
            events[0].record.client_height,
            events[0].record.scroll_height
        ),
        (80.0, 130.0)
    );
    assert_eq!(
        (events[1].record.handler, events[1].record.scroll_top),
        (20, 30.0)
    );
    assert!(events.iter().all(|event| !event.gesture));
    assert!(
        view.scroll_into_view_plan(5, 200.0, 100.0, None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(view.scroll_into_view(5, 200.0, 100.0, None).unwrap(), 0);
    assert!(view.next_delivery().is_none());
    let mut unsupported = BatchWriter::new();
    // Box-only geometry refuses text content.
    unsupported.create_text(6, "A").insert(5, 6, 0);
    commit(&mut view, &unsupported).unwrap();
    let before = view.snapshot();
    assert!(view.scroll_into_view(5, 200.0, 100.0, None).is_err());
    assert_eq!(view.snapshot(), before);
    let mut replace = BatchWriter::new();
    replace
        .remove(5)
        .create_element(5, Tag::Div)
        .insert(3, 5, 0);
    commit(&mut view, &replace).unwrap();
    assert_ne!(view.node_identity(5), Some(identity.clone()));
    assert_eq!(view.resolve_node_identity(&identity), None);
    let before = view.snapshot();
    assert_eq!(
        view.scroll_into_view_identity(&identity, 200.0, 100.0, None)
            .unwrap(),
        0
    );
    assert_eq!(view.snapshot(), before);
    assert!(view.next_delivery().is_none());
}

#[test]
fn cpu_scroll_targets_nearest_scrollable_ancestor_and_respects_bounds() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "50px")
        .set_style(2, CssProperty::Overflow, "auto")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "100px")
        .set_style(3, CssProperty::Height, "80px")
        .set_style(3, CssProperty::Overflow, "auto")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Height, "160px")
        .set_style(4, CssProperty::BackgroundColor, "#f00")
        .insert(3, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::Height, "10px")
        .set_style(5, CssProperty::BackgroundColor, "#00f")
        .insert(4, 5, 0);
    commit(&mut view, &batch).unwrap();
    let image = view.rasterize_boxes(200, 100).unwrap();
    assert_eq!(
        &image.pixels[(5 * 200 + 5) * 4..(5 * 200 + 6) * 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(
        image.scroll_target(10.0, 10.0, 999.0),
        Some((3, 80.0, 919.0))
    );
    assert_eq!(image.scroll_target(150.0, 10.0, 10.0), None);
    view.input(
        &Input::Scroll {
            target: Target::Node(3),
            top: 80.0,
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let image = view.rasterize_boxes(200, 100).unwrap();
    assert_eq!(
        &image.pixels[(5 * 200 + 5) * 4..(5 * 200 + 6) * 4],
        &[255, 0, 0, 255]
    );
    assert_eq!(
        image.scroll_target(10.0, 10.0, 999.0),
        Some((2, 30.0, 969.0))
    );
    assert_eq!(image.scroll_target(10.0, 10.0, -10.0), Some((3, 70.0, 0.0)));
    let mut hidden = BatchWriter::new();
    hidden.set_style(3, CssProperty::Overflow, "hidden");
    commit(&mut view, &hidden).unwrap();
    let image = view.rasterize_boxes(200, 100).unwrap();
    assert_eq!(image.scroll_target(10.0, 10.0, 10.0), Some((2, 10.0, 0.0)));
}

#[test]
fn cpu_raster_draws_explicit_font_text_and_clips_it() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "12px")
        .set_style(2, CssProperty::Height, "20px")
        .set_style(2, CssProperty::Overflow, "hidden")
        .set_style(2, CssProperty::Color, "#00f")
        .set_style(2, CssProperty::FontSize, "20px")
        .set_style(2, CssProperty::LineHeight, "20px")
        .insert(1, 2, 0)
        .create_text(3, "AA")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let image = view.rasterize_with_text(40, 40, &mut font, 20.0).unwrap();
    let pixel = |x: usize, y: usize| &image.pixels[(y * 40 + x) * 4..(y * 40 + x + 1) * 4];
    assert_eq!(pixel(3, 5), &[0, 0, 255, 255]);
    assert_eq!(pixel(10, 5), &[0, 0, 255, 255]);
    assert_eq!(pixel(13, 5), &[0, 0, 0, 0]);
    assert_eq!(pixel(3, 25), &[0, 0, 0, 0]);
    let mut wrap = BatchWriter::new();
    wrap.set_style(2, CssProperty::Width, "8px")
        .set_style(2, CssProperty::Height, "40px")
        .set_text(3, "A A");
    commit(&mut view, &wrap).unwrap();
    let image = view.rasterize_with_text(40, 40, &mut font, 20.0).unwrap();
    assert_eq!(
        &image.pixels[(5 * 40 + 3) * 4..(5 * 40 + 4) * 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(
        &image.pixels[(25 * 40 + 3) * 4..(25 * 40 + 4) * 4],
        &[0, 0, 255, 255]
    );
}

#[test]
fn cpu_raster_paints_uniform_borders_and_honors_color_cascade() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Color, "#00f")
        .set_style(2, CssProperty::Width, "12px")
        .set_style(2, CssProperty::Height, "12px")
        .set_style(2, CssProperty::BackgroundColor, "#f00")
        .set_style(2, CssProperty::Border, "4px solid rgba(0, 0, 255, 0.5)")
        .insert(1, 2, 0);
    commit(&mut view, &batch).unwrap();
    let image = view.rasterize_boxes(40, 40).unwrap();
    let pixel = |x: usize, y: usize| &image.pixels[(y * 40 + x) * 4..(y * 40 + x + 1) * 4];
    assert_eq!(pixel(1, 1), &[127, 0, 128, 255]);
    assert_eq!(pixel(5, 5), &[255, 0, 0, 255]);
    assert_eq!(pixel(21, 1), &[0, 0, 0, 0]);
    let mut update = BatchWriter::new();
    update
        .set_style(2, CssProperty::BorderColor, "#0f0")
        .set_style(2, CssProperty::Border, "4px solid");
    commit(&mut view, &update).unwrap();
    let image = view.rasterize_boxes(40, 40).unwrap();
    assert_eq!(&image.pixels[(40 + 1) * 4..(40 + 2) * 4], &[0, 0, 255, 255]);
    let mut mismatch = BatchWriter::new();
    mismatch.set_style(2, CssProperty::BorderTop, "4px solid #f00");
    commit(&mut view, &mismatch).unwrap();
    let image = view.rasterize_boxes(40, 40).unwrap();
    for (x, y, color) in [
        (10, 1, [255, 0, 0, 255]),
        (1, 10, [0, 0, 255, 255]),
        (5, 5, [255, 0, 0, 255]),
    ] {
        let at = (y * 40 + x) * 4;
        assert_eq!(&image.pixels[at..at + 4], &color);
        assert_eq!(image.hit_test(x as f32, y as f32), Some(2));
    }
}

#[test]
fn cpu_raster_paints_backgrounds_and_clips_descendants() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "20px")
        .set_style(2, CssProperty::Height, "20px")
        .set_style(2, CssProperty::BackgroundColor, "#f00")
        .set_style(2, CssProperty::Overflow, "hidden")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::MarginLeft, "10px")
        .set_style(3, CssProperty::MarginTop, "10px")
        .set_style(3, CssProperty::Width, "20px")
        .set_style(3, CssProperty::Height, "20px")
        .set_style(3, CssProperty::BackgroundColor, "#0000ff")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let image = view.rasterize_boxes(40, 40).unwrap();
    let pixel = |x: usize, y: usize| &image.pixels[(y * 40 + x) * 4..(y * 40 + x + 1) * 4];
    assert_eq!(pixel(5, 5), &[255, 0, 0, 255]);
    assert_eq!(pixel(15, 15), &[0, 0, 255, 255]);
    assert_eq!(pixel(25, 15), &[0, 0, 0, 0]);
    assert_eq!(pixel(15, 25), &[0, 0, 0, 0]);
    assert_eq!(image.hit_test(5.0, 5.0), Some(2));
    assert_eq!(image.hit_test(15.0, 15.0), Some(3));
    assert_eq!(image.hit_test(25.0, 15.0), None);
    assert_eq!(image.hit_test(f32::NAN, 5.0), None);
    assert!(matches!(
        view.rasterize_boxes(0, 40),
        Err(varta_view::PaintError::Dimensions)
    ));
    let mut translucent = BatchWriter::new();
    translucent.set_style(3, CssProperty::BackgroundColor, "rgba(0, 0, 255, 0.5)");
    commit(&mut view, &translucent).unwrap();
    let image = view.rasterize_boxes(40, 40).unwrap();
    assert_eq!(
        &image.pixels[(15 * 40 + 15) * 4..(15 * 40 + 16) * 4],
        &[127, 0, 128, 255]
    );
    let mut change = BatchWriter::new();
    change.set_style(3, CssProperty::Opacity, "0.5");
    commit(&mut view, &change).unwrap();
    let image = view.rasterize_boxes(40, 40).unwrap();
    assert_eq!(
        &image.pixels[(15 * 40 + 15) * 4..(15 * 40 + 16) * 4],
        &[191, 0, 64, 255]
    );
    assert_eq!(image.hit_test(15.0, 15.0), Some(3));
    assert_eq!(
        &image.pixels[(15 * 40 + 25) * 4..(15 * 40 + 26) * 4],
        &[0; 4],
        "opacity must preserve ancestor clipping"
    );
    let mut hidden = BatchWriter::new();
    hidden.set_style(3, CssProperty::Display, "none");
    commit(&mut view, &hidden).unwrap();
    let image = view.rasterize_boxes(40, 40).unwrap();
    assert_eq!(
        &image.pixels[(15 * 40 + 15) * 4..(15 * 40 + 16) * 4],
        &[255, 0, 0, 255]
    );
}

#[test]
fn explicitly_styled_button_and_text_input_layout() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "200px")
        .set_style(2, CssProperty::Gap, "10px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Button)
        .set_style(3, CssProperty::Display, "flex")
        .set_style(3, CssProperty::Width, "80px")
        .set_style(3, CssProperty::Height, "30px")
        .set_style(3, CssProperty::MinWidth, "0")
        .set_style(3, CssProperty::MinHeight, "0")
        .set_style(3, CssProperty::Padding, "4px")
        .set_style(3, CssProperty::Border, "2px solid")
        .set_style(3, CssProperty::JustifyContent, "center")
        .set_style(3, CssProperty::AlignItems, "center")
        .set_style(3, CssProperty::FontFamily, "\"Varta Fixture 400\"")
        .set_style(3, CssProperty::FontSize, "20px")
        .set_style(3, CssProperty::FontWeight, "normal")
        .set_style(3, CssProperty::FontStyle, "normal")
        .set_style(3, CssProperty::LineHeight, "20px")
        .insert(2, 3, 0)
        .create_text(4, "AAAA")
        .insert(3, 4, 0)
        .create_element(5, Tag::Input)
        .set_style(5, CssProperty::Display, "block")
        .set_style(5, CssProperty::Width, "100px")
        .set_style(5, CssProperty::Height, "30px")
        .set_style(5, CssProperty::MinWidth, "0")
        .set_style(5, CssProperty::MinHeight, "0")
        .set_style(5, CssProperty::Padding, "4px")
        .set_style(5, CssProperty::Border, "2px solid")
        .insert(2, 5, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let boxes = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(
        (boxes[&3].x, boxes[&3].width, boxes[&3].height),
        (0.0, 80.0, 30.0)
    );
    assert_eq!(
        (boxes[&4].x, boxes[&4].y, boxes[&4].width, boxes[&4].height),
        (24.0, 5.0, 32.0, 20.0)
    );
    assert_eq!(
        (boxes[&5].x, boxes[&5].width, boxes[&5].height),
        (90.0, 100.0, 30.0)
    );
    // Absent box properties take user-agent defaults (`min-width: auto`),
    // and `number` lays out as a text field.
    let mut defaults = BatchWriter::new();
    defaults.remove_style(5, CssProperty::MinWidth);
    defaults.set_attribute(5, Attribute::Type, "number");
    commit(&mut view, &defaults).unwrap();
    let boxes = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(
        (boxes[&5].x, boxes[&5].width, boxes[&5].height),
        (90.0, 100.0, 30.0)
    );
    // An author background without the `border` shorthand would need
    // Chrome's inset user-agent border.
    let mut unsupported = BatchWriter::new();
    unsupported.remove_style(5, CssProperty::Border);
    unsupported.set_style(5, CssProperty::BackgroundColor, "#ffffff");
    commit(&mut view, &unsupported).unwrap();
    assert!(matches!(
        view.layout_boxes_with_text(200.0, 100.0, &mut font, 20.0),
        Err(varta_view::LayoutError::Unsupported(5))
    ));
}

#[test]
fn flex_layout_measures_isolated_text_with_an_explicit_font() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "20px")
        .insert(1, 2, 0)
        .create_text(3, "AAAA")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "10px")
        .set_style(4, CssProperty::Height, "10px")
        .insert(2, 4, 0);
    commit(&mut view, &batch).unwrap();
    assert!(view.layout_boxes(200.0, 100.0).is_err());
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 20.0
        }
    );
    assert_eq!(
        rects[&4],
        varta_view::BoxRect {
            x: 32.0,
            y: 0.0,
            width: 10.0,
            height: 10.0
        }
    );
    let mut clip = BatchWriter::new();
    clip.set_style(2, CssProperty::Overflow, "hidden");
    commit(&mut view, &clip).unwrap();
    let geometry = view
        .layout_geometry_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(geometry[&3].rect, rects[&3]);
    assert_eq!(
        geometry[&3].clip,
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0
        }
    );
    let mut unicode = BatchWriter::new();
    unicode.create_text(5, "🙂").insert(2, 5, 0);
    commit(&mut view, &unicode).unwrap();
    let boxes = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    // This explicit font lacks an emoji glyph: its .notdef keeps the same
    // bounded advance without asking the OS for another font.
    assert_eq!(boxes[&5].width, 8.0);
    let mut control = BatchWriter::new();
    control.set_text(5, "\u{0007}");
    commit(&mut view, &control).unwrap();
    assert!(matches!(
        view.layout_boxes_with_text(200.0, 100.0, &mut font, 20.0),
        Err(varta_view::LayoutError::Unsupported(5))
    ));
}

#[test]
fn flex_text_wraps_under_a_definite_width() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "20px")
        .insert(1, 2, 0)
        .create_text(3, "A  A")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!((rects[&2].width, rects[&2].height), (20.0, 40.0));
}

#[test]
fn flex_text_wraps_cyrillic_with_explicit_font() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "28px")
        .insert(1, 2, 0)
        .create_text(3, "Аі,  їа!")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!((rects[&2].width, rects[&2].height), (28.0, 40.0));
}

#[test]
fn grid_text_wraps_with_explicit_font() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "grid")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::GridTemplateColumns, "28px 1fr")
        .set_style(2, CssProperty::Gap, "4px")
        .insert(1, 2, 0)
        .create_text(3, "Аі, їа!")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Height, "10px")
        .insert(2, 4, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!((rects[&2].width, rects[&2].height), (100.0, 40.0));
    assert_eq!(
        (rects[&4].x, rects[&4].width, rects[&4].height),
        (32.0, 68.0, 10.0)
    );
}

#[test]
fn flex_layout_uses_explicit_glyph_fallback() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "20px")
        .set_style(
            2,
            CssProperty::FontFamily,
            "\"Varta Fixture 900\", \"Varta Fixture 400\"",
        )
        .insert(1, 2, 0)
        .create_text(3, "АA")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "10px")
        .set_style(4, CssProperty::Height, "10px")
        .insert(2, 4, 0);
    commit(&mut view, &batch).unwrap();
    let mut fonts = varta_view::TextMeasurer::new_with_fonts(vec![
        varta_view::TextFont {
            data: include_bytes!("fixtures/advance-900.ttf").to_vec(),
            family: "Varta Fixture 900".into(),
            aliases: vec![],
        },
        varta_view::TextFont {
            data: include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
            family: "Varta Fixture 400".into(),
            aliases: vec![],
        },
    ])
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 100.0, &mut fonts, 20.0)
        .unwrap();
    assert_eq!(rects[&4].x, 26.0);
}

#[test]
fn font_weight_selects_a_supplied_face_and_inherits() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "20px")
        .set_style(2, CssProperty::FontFamily, "\"Varta Fixture 400\"")
        .set_style(2, CssProperty::FontWeight, "normal")
        .insert(1, 2, 0)
        .create_text(3, "A")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "10px")
        .set_style(4, CssProperty::Height, "10px")
        .insert(2, 4, 0);
    commit(&mut view, &batch).unwrap();
    let mut fonts = varta_view::TextMeasurer::new_with_fonts(vec![
        varta_view::TextFont {
            data: include_bytes!("fixtures/advance-400.ttf").to_vec(),
            family: "Varta Fixture 400".into(),
            aliases: vec![],
        },
        varta_view::TextFont {
            data: include_bytes!("fixtures/bold-900.ttf").to_vec(),
            family: "Varta Fixture 400".into(),
            aliases: vec![],
        },
        varta_view::TextFont {
            data: include_bytes!("fixtures/italic-900.ttf").to_vec(),
            family: "Varta Fixture 400".into(),
            aliases: vec![],
        },
    ])
    .unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 100.0, &mut fonts, 20.0)
            .unwrap()[&4]
            .x,
        8.0
    );
    let mut change = BatchWriter::new();
    change.set_style(2, CssProperty::FontWeight, "700");
    commit(&mut view, &change).unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 100.0, &mut fonts, 20.0)
            .unwrap()[&4]
            .x,
        18.0
    );
    let mut italic = BatchWriter::new();
    italic
        .set_style(2, CssProperty::FontWeight, "normal")
        .set_style(2, CssProperty::FontStyle, "italic");
    commit(&mut view, &italic).unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 100.0, &mut fonts, 20.0)
            .unwrap()[&4]
            .x,
        18.0
    );
}

#[test]
fn inherited_font_size_and_line_height_follow_css_rules() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Main)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::FlexDirection, "column")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::FontSize, "16px")
        .set_style(
            2,
            CssProperty::FontFamily,
            "\"Varta Missing Fixture 832746\", \"Varta Fixture 400\", sans-serif",
        )
        .set_style(2, CssProperty::LineHeight, "1.5")
        .insert(1, 2, 0)
        .create_element(3, Tag::P)
        .set_style(3, CssProperty::Width, "28px")
        .set_style(3, CssProperty::FontSize, "125%")
        .insert(2, 3, 0)
        .create_text(4, "Аі, їа!")
        .insert(3, 4, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 12.0)
        .unwrap();
    assert_eq!(
        (rects[&2].height, rects[&3].height, rects[&3].y),
        (100.0, 60.0, 20.0)
    );
    let mut change = BatchWriter::new();
    change.set_style(2, CssProperty::LineHeight, "150%");
    commit(&mut view, &change).unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 12.0)
        .unwrap();
    assert_eq!(
        (rects[&2].height, rects[&3].height, rects[&3].y),
        (88.0, 48.0, 20.0)
    );
    let mut mismatch = BatchWriter::new();
    mismatch.set_style(2, CssProperty::FontFamily, "\"Other Font\"");
    commit(&mut view, &mismatch).unwrap();
    assert!(matches!(
        view.layout_boxes_with_text(200.0, 200.0, &mut font, 12.0),
        Err(varta_view::LayoutError::Unsupported(2))
    ));
    let mut generic = BatchWriter::new();
    generic.set_style(
        2,
        CssProperty::FontFamily,
        "system-ui, \"Varta Fixture 400\"",
    );
    commit(&mut view, &generic).unwrap();
    assert!(matches!(
        view.layout_boxes_with_text(200.0, 200.0, &mut font, 12.0),
        Err(varta_view::LayoutError::Unsupported(2))
    ));
    let mut mapped = varta_view::TextMeasurer::new_with_aliases(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
        &["system-ui"],
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut mapped, 12.0)
        .unwrap();
    assert_eq!((rects[&2].height, rects[&3].height), (88.0, 48.0));
}

#[test]
fn block_text_wraps_with_explicit_font() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "28px")
        .insert(1, 2, 0)
        .create_text(3, "Аі,  їа!")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
        .unwrap();
    assert_eq!((rects[&2].width, rects[&2].height), (28.0, 40.0));
    let mut mixed = BatchWriter::new();
    mixed.create_element(4, Tag::Div).insert(2, 4, 0);
    commit(&mut view, &mixed).unwrap();
    assert!(matches!(
        view.layout_boxes_with_text(200.0, 100.0, &mut font, 20.0),
        Err(varta_view::LayoutError::Unsupported(3))
    ));
}

#[test]
fn white_space_modes_control_collapsing_and_line_breaks() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Width, "20px")
        .insert(1, 2, 0)
        .create_text(3, "A  A")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
            .unwrap()[&2]
            .height,
        40.0
    );
    let mut nowrap = BatchWriter::new();
    nowrap.set_style(2, CssProperty::WhiteSpace, "nowrap");
    commit(&mut view, &nowrap).unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
            .unwrap()[&2]
            .height,
        20.0
    );
    let mut pre = BatchWriter::new();
    pre.set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::WhiteSpace, "pre-wrap")
        .set_text(3, "A  A\nA");
    commit(&mut view, &pre).unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
            .unwrap()[&2]
            .height,
        40.0
    );
    let mut preserved = BatchWriter::new();
    preserved
        .set_style(2, CssProperty::Width, "28px")
        .set_text(3, "A  A");
    commit(&mut view, &preserved).unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
            .unwrap()[&2]
            .height,
        40.0
    );
}

#[test]
fn nowrap_text_keeps_intrinsic_width_in_flex() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "20px")
        .set_style(2, CssProperty::WhiteSpace, "nowrap")
        .insert(1, 2, 0)
        .create_text(3, "A A")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "4px")
        .set_style(4, CssProperty::Height, "10px")
        .set_style(4, CssProperty::FlexShrink, "0")
        .insert(2, 4, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(rects[&4].x, 24.0);
}

#[test]
fn overflow_wrap_anywhere_changes_flex_min_content_and_lines() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "20px")
        .set_style(2, CssProperty::OverflowWrap, "normal")
        .insert(1, 2, 0)
        .create_text(3, "AAAA")
        .insert(2, 3, 0)
        .create_element(4, Tag::Div)
        .set_style(4, CssProperty::Width, "4px")
        .set_style(4, CssProperty::Height, "10px")
        .set_style(4, CssProperty::FlexShrink, "0")
        .insert(2, 4, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!((rects[&2].height, rects[&4].x), (20.0, 32.0));
    let mut change = BatchWriter::new();
    change.set_style(2, CssProperty::OverflowWrap, "anywhere");
    commit(&mut view, &change).unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!((rects[&2].height, rects[&4].x), (40.0, 16.0));
    let mut nowrap = BatchWriter::new();
    nowrap.set_style(2, CssProperty::WhiteSpace, "nowrap");
    commit(&mut view, &nowrap).unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!((rects[&2].height, rects[&4].x), (20.0, 32.0));
}

#[test]
fn paragraph_default_margins_and_overrides_affect_layout() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::FlexDirection, "column")
        .set_style(2, CssProperty::Width, "100px")
        .insert(1, 2, 0)
        .create_element(3, Tag::P)
        .set_style(3, CssProperty::Width, "28px")
        .insert(2, 3, 0)
        .create_text(4, "Аі, їа!")
        .insert(3, 4, 0)
        .create_element(5, Tag::Div)
        .set_style(5, CssProperty::Width, "10px")
        .set_style(5, CssProperty::Height, "10px")
        .insert(2, 5, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(
        (rects[&2].height, rects[&3].y, rects[&3].height, rects[&5].y),
        (90.0, 20.0, 40.0, 80.0)
    );
    let mut change = BatchWriter::new();
    change.set_style(3, CssProperty::MarginTop, "5px");
    commit(&mut view, &change).unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(
        (rects[&2].height, rects[&3].y, rects[&5].y),
        (75.0, 5.0, 65.0)
    );
}

#[test]
fn semantic_block_containers_participate_in_text_layout() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Main)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::FlexDirection, "column")
        .set_style(2, CssProperty::Width, "100px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Header)
        .set_style(3, CssProperty::Height, "10px")
        .insert(2, 3, 0)
        .create_element(4, Tag::Section)
        .set_style(4, CssProperty::Width, "28px")
        .set_style(4, CssProperty::Padding, "5px")
        .insert(2, 4, 0)
        .create_text(5, "Аі, їа!")
        .insert(4, 5, 0)
        .create_element(6, Tag::Footer)
        .set_style(6, CssProperty::Height, "10px")
        .insert(2, 6, 0);
    commit(&mut view, &batch).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let rects = view
        .layout_boxes_with_text(200.0, 200.0, &mut font, 20.0)
        .unwrap();
    assert_eq!(
        (rects[&2].height, rects[&4].y, rects[&4].height, rects[&6].y),
        (70.0, 10.0, 50.0, 60.0)
    );
}

#[test]
fn paint_only_css_keeps_geometry_translations_move_and_scales_remain_unsupported() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "30px")
        .set_style(2, CssProperty::BackgroundColor, "#eee")
        .set_style(2, CssProperty::Color, "#123")
        .set_style(2, CssProperty::BorderRadius, "8px")
        .set_style(2, CssProperty::Opacity, "0.5")
        .set_style(2, CssProperty::Outline, "2px solid #000")
        .set_style(2, CssProperty::Cursor, "pointer")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "20px")
        .set_style(3, CssProperty::Height, "10px")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        (
            rects[&2].width,
            rects[&2].height,
            rects[&3].x,
            rects[&3].width
        ),
        (100.0, 30.0, 0.0, 20.0)
    );
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["background-color"],
        "#eee"
    );
    let mut transform = BatchWriter::new();
    transform.set_style(2, CssProperty::Transform, "translateX(5px)");
    commit(&mut view, &transform).unwrap();
    let moved = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!((moved[&2].x, moved[&3].x), (5.0, 5.0));
    // A scale maps the box and its subtree around its centre in the
    // public geometry; layout keeps the flow.
    let mut transform = BatchWriter::new();
    transform.set_style(2, CssProperty::Transform, "scale(2)");
    commit(&mut view, &transform).unwrap();
    let scaled = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        (
            scaled[&2].x,
            scaled[&2].y,
            scaled[&2].width,
            scaled[&2].height
        ),
        (-50.0, -15.0, 200.0, 60.0)
    );
    assert_eq!((scaled[&3].x, scaled[&3].width), (-50.0, 40.0));
}

#[test]
fn hidden_box_subtree_has_zero_rectangles_and_does_not_take_space() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Padding, "5px")
        .insert(1, 2, 0)
        .create_element(3, Tag::Section)
        .set_style(3, CssProperty::Display, "none")
        .set_style(3, CssProperty::Width, "20px")
        .insert(2, 3, 0)
        .create_element(4, Tag::Button)
        .set_style(4, CssProperty::Width, "10px")
        .set_style(4, CssProperty::Height, "10px")
        .set_style(4, CssProperty::Color, "#fff")
        .insert(3, 4, 0)
        .create_text(6, "Приховано 🙂")
        .insert(4, 6, 0)
        .create_element(5, Tag::Footer)
        .set_style(5, CssProperty::Width, "10px")
        .set_style(5, CssProperty::Height, "10px")
        .insert(2, 5, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    let zero = varta_view::BoxRect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };
    assert_eq!(rects[&3], zero);
    assert_eq!(rects[&4], zero);
    assert_eq!(rects[&6], zero);
    assert_eq!((rects[&5].x, rects[&5].y), (5.0, 5.0));
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    assert_eq!(
        view.layout_boxes_with_text(200.0, 100.0, &mut font, 20.0)
            .unwrap()[&6],
        zero
    );
    let mut reveal = BatchWriter::new();
    reveal.remove_style(3, CssProperty::Display);
    commit(&mut view, &reveal).unwrap();
    assert!(view.layout_boxes(200.0, 100.0).is_err());
}

#[test]
fn box_layout_uniform_border_follows_style_and_default_width() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::Padding, "5px")
        .set_style(2, CssProperty::BorderWidth, "4px")
        .set_style(2, CssProperty::BorderStyle, "solid")
        .set_style(2, CssProperty::BorderColor, "#f00")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "20px")
        .set_style(3, CssProperty::Height, "10px")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        rects[&2],
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: 118.0,
            height: 58.0
        }
    );
    assert_eq!(
        rects[&3],
        varta_view::BoxRect {
            x: 9.0,
            y: 9.0,
            width: 20.0,
            height: 10.0
        }
    );
    let mut change = BatchWriter::new();
    change.set_style(2, CssProperty::BorderStyle, "none");
    commit(&mut view, &change).unwrap();
    assert_eq!(view.layout_boxes(200.0, 100.0).unwrap()[&2].width, 110.0);
    let mut change = BatchWriter::new();
    change.remove_style(2, CssProperty::BorderWidth).set_style(
        2,
        CssProperty::BorderStyle,
        "solid",
    );
    commit(&mut view, &change).unwrap();
    assert_eq!(view.layout_boxes(200.0, 100.0).unwrap()[&2].width, 116.0);
}

#[test]
fn box_layout_border_shorthands_and_sides() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::Border, "4px solid #f00")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "20px")
        .set_style(3, CssProperty::Height, "10px")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(rects[&2].width, 108.0);
    assert_eq!((rects[&3].x, rects[&3].y), (4.0, 4.0));
    let mut change = BatchWriter::new();
    change
        .remove_style(2, CssProperty::Border)
        .set_style(2, CssProperty::BorderTop, "2px solid #f00")
        .set_style(2, CssProperty::BorderLeft, "6px solid #f00");
    commit(&mut view, &change).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!((rects[&2].width, rects[&2].height), (106.0, 42.0));
    assert_eq!((rects[&3].x, rects[&3].y), (6.0, 2.0));
    let mut ambiguous = BatchWriter::new();
    ambiguous.set_style(2, CssProperty::Border, "1px solid #000");
    commit(&mut view, &ambiguous).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        (rects[&2].width, rects[&2].height, rects[&3].x, rects[&3].y),
        (102.0, 42.0, 1.0, 1.0)
    );
    let mut override_sides = BatchWriter::new();
    override_sides
        .set_style(2, CssProperty::BorderTop, "4px solid #f00")
        .set_style(2, CssProperty::BorderWidth, "2px");
    commit(&mut view, &override_sides).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        (rects[&2].width, rects[&2].height, rects[&3].x, rects[&3].y),
        (104.0, 44.0, 2.0, 2.0)
    );
    let mut remove = BatchWriter::new();
    remove.remove_style(2, CssProperty::BorderWidth);
    commit(&mut view, &remove).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        (rects[&2].width, rects[&2].height, rects[&3].x, rects[&3].y),
        (106.0, 46.0, 3.0, 3.0)
    );
}

#[test]
fn box_layout_border_cascade_respects_stylesheet_and_inline_order() {
    let mut view = view();
    let mut batch = BatchWriter::new();
    batch
        .stylesheet(
            1,
            ".box { border: 1px solid #000; border-left: 6px solid #f00; border-width: 2px }",
        )
        .create_element(2, Tag::Div)
        .set_class(2, "box")
        .set_style(2, CssProperty::Display, "flex")
        .set_style(2, CssProperty::Width, "100px")
        .set_style(2, CssProperty::Height, "40px")
        .set_style(2, CssProperty::BorderTop, "4px solid #f00")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_style(3, CssProperty::Width, "20px")
        .set_style(3, CssProperty::Height, "10px")
        .insert(2, 3, 0);
    commit(&mut view, &batch).unwrap();
    let rects = view.layout_boxes(200.0, 100.0).unwrap();
    assert_eq!(
        (rects[&2].width, rects[&2].height, rects[&3].x, rects[&3].y),
        (104.0, 46.0, 2.0, 4.0)
    );
}

#[test]
fn malformed_batches_fail_closed_and_apply_nothing() {
    let (mut view, _) = app();
    let before = view.snapshot();
    let mut cases: Vec<(Vec<u8>, Violation)> = Vec::new();
    // Header: version, flags, truncation, trailing bytes, padding, opcodes.
    let mut bad_version = BatchWriter::new().finish();
    bad_version[0] = 1;
    cases.push((bad_version, Violation::Version));
    let mut flags = BatchWriter::new().finish();
    flags[2] = 1;
    cases.push((flags, Violation::Flags));
    cases.push((vec![0, 0, 0], Violation::Truncated));
    let mut trailing = BatchWriter::new().finish();
    trailing.push(0);
    cases.push((trailing, Violation::TrailingBytes));
    let mut padding = BatchWriter::new();
    padding.remove(2);
    let mut padding = padding.finish();
    padding[9] = 1;
    cases.push((padding, Violation::Padding));
    let mut unknown = BatchWriter::new();
    unknown.raw(&[0, 0, 0, 0]);
    let mut unknown = unknown.finish();
    unknown[4] = 1;
    cases.push((unknown, Violation::UnknownOpcode));
    let mut opcode = BatchWriter::new();
    opcode.raw(&[99, 0, 0, 0]);
    let mut opcode = opcode.finish();
    opcode[4] = 1;
    cases.push((opcode, Violation::UnknownOpcode));
    // Strings: lone surrogates and NUL never enter the tree.
    let mut lone = BatchWriter::new();
    lone.raw(&[Opcode::CreateText.code(), 0, 0, 0, 9, 0, 0, 0])
        .raw_text(&[0xd800]);
    let mut lone = lone.finish();
    lone[4] = 1;
    cases.push((lone, Violation::LoneSurrogate));
    let mut nul = BatchWriter::new();
    nul.raw(&[Opcode::CreateText.code(), 0, 0, 0, 9, 0, 0, 0])
        .raw_text(&[0x41, 0]);
    let mut nul = nul.finish();
    nul[4] = 1;
    cases.push((nul, Violation::NulCharacter));
    // A paired surrogate is well formed.
    let mut emoji = BatchWriter::new();
    emoji
        .raw(&[Opcode::CreateText.code(), 0, 0, 0, 9, 0, 0, 0])
        .raw_text(&[0xd83d, 0xde00]);
    let mut emoji = emoji.finish();
    emoji[4] = 1;
    for (bytes, expected) in cases {
        let result = view.commit(&bytes, false, &mut Fixed::default());
        assert_eq!(violation(result), expected);
        assert_eq!(view.snapshot(), before, "nothing applied");
    }
    view.commit(&emoji, false, &mut Fixed::default()).unwrap();
}

/// Appends the operations of one negative case.
type Build = Box<dyn Fn(&mut BatchWriter)>;

#[test]
fn semantic_rules_reject_the_whole_batch() {
    let (mut view, _) = app();
    let before = view.snapshot();
    let cases: Vec<(Build, Violation)> = vec![
        (
            Box::new(|b| {
                b.create_element(2, Tag::Div);
            }),
            Violation::DuplicateNode(2),
        ),
        (
            Box::new(|b| {
                b.create_element(0, Tag::Div);
            }),
            Violation::DuplicateNode(0),
        ),
        (
            Box::new(|b| {
                b.insert(1, 99, 0);
            }),
            Violation::UnknownNode(99),
        ),
        (
            Box::new(|b| {
                b.set_text(2, "x");
            }),
            Violation::NotAText(2),
        ),
        (
            Box::new(|b| {
                b.set_class(3, "a");
            }),
            Violation::NotAnElement(3),
        ),
        (
            Box::new(|b| {
                b.set_attribute(2, Attribute::Href, "/x");
            }),
            Violation::AttributeNotAllowed("href"),
        ),
        (
            Box::new(|b| {
                b.set_attribute(2, Attribute::Disabled, "false");
            }),
            Violation::InvalidAttributeValue("disabled"),
        ),
        (
            Box::new(|b| {
                b.set_attribute(4, Attribute::Rows, "0");
            }),
            Violation::InvalidAttributeValue("rows"),
        ),
        (
            Box::new(|b| {
                b.set_attribute(2, Attribute::Type, "checkbox");
            }),
            Violation::InvalidAttributeValue("type"),
        ),
        (
            Box::new(|b| {
                b.set_attribute(4, Attribute::Id, "inc");
            }),
            Violation::DuplicateIdent("inc".into()),
        ),
        (
            Box::new(|b| {
                b.set_style(2, CssProperty::Color, "red");
            }),
            Violation::InvalidStyleValue,
        ),
        (
            Box::new(|b| {
                b.set_style(2, CssProperty::Width, "10px; color: #fff");
            }),
            Violation::InvalidStyleValue,
        ),
        (
            Box::new(|b| {
                b.set_style(2, CssProperty::FontFamily, "serif, REVERT-layer");
            }),
            Violation::FontFamilyKeyword("revert-layer"),
        ),
        (
            Box::new(|b| {
                b.set_style(2, CssProperty::FieldSizing, "content");
            }),
            Violation::PropertyNotAllowed,
        ),
        (
            Box::new(|b| {
                b.stylesheet(1, "div { color: #fff }");
            }),
            Violation::InvalidStylesheet,
        ),
        (
            Box::new(|b| {
                b.stylesheet(0, ".a { color: #fff }");
            }),
            Violation::InvalidSheet,
        ),
        (
            Box::new(|b| {
                b.insert(4, 3, 0);
            }),
            Violation::ChildNotAllowed,
        ),
        (
            Box::new(|b| {
                b.insert(3, 2, 0);
            }),
            Violation::NotAnElement(3),
        ),
        (
            Box::new(|b| {
                b.create_element(5, Tag::Div).insert(5, 5, 0);
            }),
            Violation::Cycle,
        ),
        (
            Box::new(|b| {
                b.create_element(5, Tag::Div).insert(1, 5, 3);
            }),
            Violation::InvalidBefore(3),
        ),
        (
            Box::new(|b| {
                b.remove(1);
            }),
            Violation::RootNotMovable,
        ),
        (
            Box::new(|b| {
                b.listen(2, EventKind::Click, 10, ANY);
            }),
            Violation::DuplicateHandler(10),
        ),
        (
            Box::new(|b| {
                b.listen(2, EventKind::Click, 11, ANY);
            }),
            Violation::DuplicateFilter,
        ),
        (
            Box::new(|b| {
                b.listen(
                    2,
                    EventKind::Click,
                    11,
                    Filter {
                        key: Key::Enter.code(),
                        ..ANY
                    },
                );
            }),
            Violation::InvalidFilter,
        ),
        (
            Box::new(|b| {
                b.listen(
                    4,
                    EventKind::Keydown,
                    11,
                    Filter {
                        required: 1 << 6,
                        ..ANY
                    },
                );
            }),
            Violation::InvalidFilter,
        ),
        (
            Box::new(|b| {
                b.listen(2, EventKind::CommandResult, 11, ANY);
            }),
            Violation::NotListenable,
        ),
        (
            Box::new(|b| {
                b.unlisten(2, EventKind::Click, 99);
            }),
            Violation::NotListening(99),
        ),
        (
            Box::new(|b| {
                b.set_value(2, "x", 0);
            }),
            Violation::NotAField(2),
        ),
        (
            Box::new(|b| {
                b.command(4, Command::ShowModal, 0.0);
            }),
            Violation::CommandNotAllowed("show_modal"),
        ),
        (
            Box::new(|b| {
                b.command(4, Command::Focus, 1.0);
            }),
            Violation::InvalidArgument,
        ),
        (
            Box::new(|b| {
                b.command(4, Command::ScrollTo, f64::NAN);
            }),
            Violation::InvalidArgument,
        ),
        (
            Box::new(|b| {
                b.command(3, Command::Focus, 0.0);
            }),
            Violation::NotAnElement(3),
        ),
        (
            Box::new(|b| {
                b.copy_text(0, "x");
            }),
            Violation::DuplicateRequest(0),
        ),
        (
            Box::new(|b| {
                b.create_element(5, Tag::A)
                    .set_attribute(5, Attribute::Href, "https://evil.test/");
            }),
            Violation::InvalidAttributeValue("href"),
        ),
        (
            Box::new(|b| {
                b.create_element(5, Tag::A)
                    .set_attribute(5, Attribute::Href, "//evil.test/");
            }),
            Violation::InvalidAttributeValue("href"),
        ),
    ];
    for (build, expected) in cases {
        let mut batch = BatchWriter::new();
        // A valid prefix proves atomicity: it is discarded with the batch.
        batch.create_element(50, Tag::Div).insert(1, 50, 0);
        build(&mut batch);
        assert_eq!(violation(commit(&mut view, &batch)), expected);
        assert_eq!(view.snapshot(), before, "nothing of {expected:?} applied");
    }
}

#[test]
fn ids_are_reusable_after_remove_and_removal_releases_listeners() {
    let (mut view, mut batch) = app();
    batch
        .remove(2)
        .create_element(2, Tag::Span)
        .listen(2, EventKind::Click, 10, ANY)
        .insert(1, 2, 4);
    commit(&mut view, &batch).unwrap();
    let snapshot = view.snapshot();
    assert_eq!(snapshot["tree"]["children"][0]["tag"], "span");
    assert_eq!(snapshot["tree"]["children"][1]["tag"], "textarea");
}

#[test]
fn stylesheets_cascade_by_sheet_specificity_order_and_inline_style() {
    let (mut view, mut batch) = app();
    batch
        .stylesheet(
            2,
            ".btn.primary { color: #222 } .btn { color: #111; padding: var(--pad, 2px) }",
        )
        .stylesheet(1, ":root { --pad: 6px } .btn { color: #000 }")
        .set_class(2, "btn primary");
    commit(&mut view, &batch).unwrap();
    let style = &view.snapshot()["tree"]["children"][0]["style"];
    assert_eq!(style["color"], "#222");
    assert_eq!(style["padding"], "6px");
    let mut inline = BatchWriter::new();
    inline.set_style(2, CssProperty::Color, "#333");
    commit(&mut view, &inline).unwrap();
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["color"],
        "#333"
    );
    let mut removed = BatchWriter::new();
    removed.stylesheet(2, "");
    commit(&mut view, &removed).unwrap();
    assert_eq!(view.snapshot()["stylesheets"].as_array().unwrap().len(), 1);
}

#[test]
fn clicks_bubble_in_path_order_until_stop_propagation() {
    let (mut view, mut batch) = app();
    batch
        .listen(1, EventKind::Click, 20, ANY)
        .create_element(5, Tag::Div)
        .insert(1, 5, 0);
    commit(&mut view, &batch).unwrap();
    let mut broker = Fixed::default();
    view.input(
        &Input::Click {
            target: Target::Ident("#inc".into()),
        },
        &mut broker,
    )
    .unwrap();
    let got = deliveries(&mut view);
    let handlers: Vec<u32> = got
        .iter()
        .filter(|d| d.0 == EventKind::Click)
        .map(|d| d.1)
        .collect();
    assert_eq!(handlers, [10, 20]);
    assert!(got.iter().filter(|d| d.0 == EventKind::Click).all(|d| d.3));
    let mut stop = BatchWriter::new();
    stop.unlisten(2, EventKind::Click, 10).listen(
        2,
        EventKind::Click,
        11,
        Filter {
            options: ListenOption::StopPropagation as u32,
            ..ANY
        },
    );
    commit(&mut view, &stop).unwrap();
    view.input(
        &Input::Click {
            target: Target::Node(3),
        },
        &mut broker,
    )
    .unwrap();
    let handlers: Vec<u32> = deliveries(&mut view)
        .into_iter()
        .filter(|d| d.0 == EventKind::Click)
        .map(|d| d.1)
        .collect();
    assert_eq!(handlers, [11]);
}

#[test]
fn key_filters_send_on_enter_and_newline_on_shift_enter() {
    let (mut view, mut batch) = app();
    batch.listen(
        4,
        EventKind::Keydown,
        30,
        Filter {
            options: ListenOption::PreventDefault as u32 | ListenOption::NotComposing as u32,
            key: Key::Enter.code(),
            required: 0,
            forbidden: EventFlag::Shift as u32,
        },
    );
    batch.listen(4, EventKind::Input, 31, ANY);
    commit(&mut view, &batch).unwrap();
    let mut broker = Fixed::default();
    let target = || Target::Ident("#box".into());
    view.input(
        &Input::Type {
            target: target(),
            text: "hi".into(),
        },
        &mut broker,
    )
    .unwrap();
    let key = |shift, composing| Input::Key {
        target: target(),
        key: "Enter".into(),
        shift,
        ctrl: false,
        alt: false,
        meta: false,
        composing,
    };
    view.input(&key(false, false), &mut broker).unwrap();
    view.input(&key(true, false), &mut broker).unwrap();
    view.input(&key(false, true), &mut broker).unwrap();
    let got = deliveries(&mut view);
    let keydowns: Vec<u32> = got
        .iter()
        .filter(|d| d.0 == EventKind::Keydown)
        .map(|d| d.1)
        .collect();
    // Enter matches; Shift+Enter and composing Enter match nothing.
    assert_eq!(keydowns, [30]);
    // Two edits: the typed text and Shift+Enter's newline (coalesced).
    let inputs = got.iter().filter(|d| d.0 == EventKind::Input).count();
    assert_eq!(inputs, 1);
    let field = &view.snapshot()["tree"]["children"][1];
    assert_eq!(field["value"], "hi\n");
    assert_eq!(field["seq"], 2);
}

#[test]
fn stale_set_value_is_dropped_by_the_edit_counter() {
    let (mut view, mut batch) = app();
    batch.set_value(4, "server", 0);
    commit(&mut view, &batch).unwrap();
    view.input(
        &Input::Type {
            target: Target::Node(4),
            text: "!".into(),
        },
        &mut Fixed::default(),
    )
    .unwrap();
    let mut stale = BatchWriter::new();
    stale.set_value(4, "old", 0);
    commit(&mut view, &stale).unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][1]["value"], "server!");
    let mut fresh = BatchWriter::new();
    fresh.set_value(4, "new", 1);
    commit(&mut view, &fresh).unwrap();
    assert_eq!(view.snapshot()["tree"]["children"][1]["value"], "new");
}

#[test]
fn copy_text_reports_every_status() {
    let cases = [
        (true, Access::Granted, false, CommandStatus::Ok),
        (false, Access::Granted, false, CommandStatus::InvalidContext),
        (true, Access::Denied, false, CommandStatus::NotAllowed),
        (
            true,
            Access::Granted,
            true,
            CommandStatus::RefusedByPlatform,
        ),
    ];
    for (gesture, clipboard, fails, expected) in cases {
        let mut view = View::new(
            Environment::default(),
            Platform {
                clipboard_fails: fails,
                ..Platform::default()
            },
        );
        let mut broker = Fixed {
            clipboard,
            ..Fixed::default()
        };
        let mut batch = BatchWriter::new();
        batch.copy_text(7, "copied");
        view.commit(&batch.finish(), gesture, &mut broker).unwrap();
        let delivery = view.next_delivery().unwrap();
        assert_eq!(delivery.record.kind, EventKind::CommandResult);
        assert_eq!(delivery.record.key, "copy_text");
        assert_eq!(delivery.record.seq, 7);
        assert_eq!(delivery.record.handler, 0);
        assert_eq!(delivery.record.status, expected.code());
        assert!(!delivery.gesture);
        let clipboard = &view.snapshot()["clipboard"];
        assert_eq!(clipboard.is_string(), expected == CommandStatus::Ok);
    }
}

#[test]
fn pending_copy_requests_are_bounded_and_unique() {
    let mut view = view();
    let mut broker = Fixed::default();
    let mut batch = BatchWriter::new();
    for request in 1..=DRAFT_MAX_PENDING_COMMANDS as u32 {
        batch.copy_text(request, "x");
    }
    view.commit(&batch.finish(), true, &mut broker).unwrap();
    let mut more = BatchWriter::new();
    more.copy_text(99, "x");
    assert_eq!(
        violation(view.commit(&more.finish(), true, &mut broker)),
        Violation::TooManyPendingCommands
    );
    let mut duplicate = BatchWriter::new();
    duplicate.copy_text(1, "x");
    // Delivering one result frees its id.
    view.next_delivery().unwrap();
    view.commit(&duplicate.finish(), true, &mut broker).unwrap();
}

#[test]
fn external_links_report_open_url_results_after_the_click() {
    let cases = [
        (
            Access::Granted,
            PromptAnswer::None,
            false,
            CommandStatus::Ok,
        ),
        (
            Access::Denied,
            PromptAnswer::Allow,
            false,
            CommandStatus::NotAllowed,
        ),
        (
            Access::Prompt,
            PromptAnswer::Allow,
            false,
            CommandStatus::Ok,
        ),
        (
            Access::Prompt,
            PromptAnswer::Deny,
            false,
            CommandStatus::NotAllowed,
        ),
        (
            Access::Prompt,
            PromptAnswer::None,
            false,
            CommandStatus::NotAllowed,
        ),
        (
            Access::Granted,
            PromptAnswer::None,
            true,
            CommandStatus::RefusedByPlatform,
        ),
    ];
    for (open, prompt, browser_fails, expected) in cases {
        let mut view = View::new(
            Environment::default(),
            Platform {
                prompt,
                browser_fails,
                ..Platform::default()
            },
        );
        let mut broker = Fixed {
            open,
            ..Fixed::default()
        };
        let mut batch = BatchWriter::new();
        batch
            .create_element(2, Tag::A)
            .set_attribute(2, Attribute::Href, "https://docs.test/guide")
            .create_text(3, "docs")
            .insert(2, 3, 0)
            .insert(1, 2, 0)
            .listen(2, EventKind::Click, 5, ANY);
        view.commit(&batch.finish(), false, &mut broker).unwrap();
        view.input(
            &Input::Click {
                target: Target::Node(3),
            },
            &mut broker,
        )
        .unwrap();
        let kinds: Vec<_> = std::iter::from_fn(|| view.next_delivery())
            .filter(|d| d.record.kind != EventKind::Focusin)
            .map(|d| {
                (
                    d.record.kind,
                    d.record.target,
                    d.record.status,
                    d.record.key,
                )
            })
            .collect();
        assert_eq!(kinds[0].0, EventKind::Click);
        assert_eq!(
            kinds[1],
            (
                EventKind::CommandResult,
                2,
                expected.code(),
                "open_url".to_string()
            )
        );
        assert_eq!(broker.asked, ["https://docs.test/guide"]);
        let opened = view.snapshot()["opened"].as_array().unwrap().len();
        assert_eq!(opened, usize::from(expected == CommandStatus::Ok));
    }
}

#[test]
fn coalescible_records_replace_and_others_overflow_fatally() {
    let (mut view, mut batch) = app();
    batch.listen(4, EventKind::Input, 40, ANY);
    commit(&mut view, &batch).unwrap();
    let mut broker = Fixed::default();
    for _ in 0..3 {
        view.input(
            &Input::Type {
                target: Target::Node(4),
                text: "a".into(),
            },
            &mut broker,
        )
        .unwrap();
    }
    let inputs: Vec<_> = std::iter::from_fn(|| view.next_delivery())
        .filter(|d| d.record.kind == EventKind::Input)
        .collect();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].record.seq, 3);
    assert_eq!(inputs[0].record.value, "aaa");
    let mut overflow = Ok(());
    for _ in 0..=DRAFT_MAX_PENDING_EVENTS {
        overflow = view.input(
            &Input::Click {
                target: Target::Node(2),
            },
            &mut broker,
        );
        if overflow.is_err() {
            break;
        }
    }
    assert_eq!(
        overflow,
        Err(varta_view::InputError::View(ViewError::QueueOverflow))
    );
}

#[test]
fn checkboxes_dialogs_and_focus_follow_the_platform() {
    let mut view = view();
    let mut broker = Fixed::default();
    let mut batch = BatchWriter::new();
    // The checkbox is inside the modal dialog: everything outside it is inert.
    batch
        .create_element(3, Tag::Dialog)
        .insert(1, 3, 0)
        .listen(3, EventKind::Cancel, 2, ANY)
        .listen(3, EventKind::Close, 3, ANY)
        .create_element(2, Tag::Input)
        .set_attribute(2, Attribute::Type, "checkbox")
        .insert(3, 2, 0)
        .listen(2, EventKind::Change, 1, ANY)
        .command(3, Command::ShowModal, 0.0)
        .command(2, Command::Focus, 0.0);
    view.commit(&batch.finish(), false, &mut broker).unwrap();
    view.input(
        &Input::Click {
            target: Target::Node(2),
        },
        &mut broker,
    )
    .unwrap();
    let snapshot = view.snapshot();
    assert_eq!(
        snapshot["tree"]["children"][0]["children"][0]["checked"],
        true
    );
    assert_eq!(snapshot["focus"], 2);
    assert_eq!(snapshot["accessibility"]["children"][0]["modal"], true);
    view.input(
        &Input::Key {
            target: Target::Node(2),
            key: "Escape".into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut broker,
    )
    .unwrap();
    let kinds: Vec<_> = std::iter::from_fn(|| view.next_delivery())
        .map(|d| d.record.kind)
        .collect();
    assert!(kinds.contains(&EventKind::Change));
    let cancel = kinds.iter().position(|k| *k == EventKind::Cancel).unwrap();
    let close = kinds.iter().position(|k| *k == EventKind::Close).unwrap();
    assert!(cancel < close);
    let snapshot = view.snapshot();
    assert_eq!(snapshot["tree"]["children"][0]["open"], false);
    // Nothing was focused before the dialog: focus inside it is dropped.
    assert_eq!(snapshot["focus"], serde_json::Value::Null);
}

/// Modal dialogs follow Chrome 156 (probed): `show_modal` focuses the first
/// focusable descendant (an `autofocus` one first, else the dialog), Tab
/// stays inside the topmost dialog, everything outside it is inert (scripted
/// inputs naming it are refused, `focus` ignores it, the accessibility tree
/// drops it), Esc cancels and closes the topmost dialog only and focus
/// returns to the element focused before it was shown. A closed dialog's
/// contents take no focus.
#[test]
fn modal_dialogs_trap_focus_and_make_the_rest_inert() {
    let mut view = view();
    let mut broker = Fixed::default();
    let mut batch = BatchWriter::new();
    // 2 before (button), 3 outside (input); 4 dialog d: 5 first (input),
    // 6 second (button); 7 dialog e: 8 inner (button).
    batch
        .create_element(2, Tag::Button)
        .set_attribute(2, Attribute::Id, "before")
        .insert(1, 2, 0)
        .create_element(3, Tag::Input)
        .set_attribute(3, Attribute::Id, "outside")
        .insert(1, 3, 0)
        .create_element(4, Tag::Dialog)
        .insert(1, 4, 0)
        .create_element(5, Tag::Input)
        .insert(4, 5, 0)
        .create_element(6, Tag::Button)
        .insert(4, 6, 0)
        .create_element(7, Tag::Dialog)
        .insert(1, 7, 0)
        .create_element(8, Tag::Button)
        .insert(7, 8, 0)
        .command(2, Command::Focus, 0.0);
    view.commit(&batch.finish(), false, &mut broker).unwrap();
    let focus = |view: &View| view.snapshot()["focus"].clone();
    let press = |view: &mut View, broker: &mut Fixed, key: &str, shift: bool| {
        let target = view.snapshot()["focus"].as_u64().unwrap() as u32;
        view.input(
            &Input::Key {
                target: Target::Node(target),
                key: key.into(),
                shift,
                ctrl: false,
                alt: false,
                meta: false,
                composing: false,
            },
            broker,
        )
        .unwrap();
    };
    // A closed dialog's contents take no focus: Tab goes before -> outside -> before.
    press(&mut view, &mut broker, "Tab", false);
    assert_eq!(focus(&view), 3);
    press(&mut view, &mut broker, "Tab", false);
    assert_eq!(focus(&view), 2);
    let mut batch = BatchWriter::new();
    batch.command(4, Command::ShowModal, 0.0);
    view.commit(&batch.finish(), false, &mut broker).unwrap();
    assert_eq!(
        focus(&view),
        5,
        "show_modal focuses the first focusable descendant"
    );
    for expected in [6, 5, 6] {
        press(&mut view, &mut broker, "Tab", false);
        assert_eq!(focus(&view), expected, "Tab stays inside the dialog");
    }
    press(&mut view, &mut broker, "Tab", true);
    assert_eq!(focus(&view), 5);
    // Outside is inert: a scripted click naming it is refused, focus ignores it.
    assert!(matches!(
        view.input(
            &Input::Click {
                target: Target::Node(3)
            },
            &mut broker
        ),
        Err(varta_view::InputError::Inert(_))
    ));
    let mut batch = BatchWriter::new();
    batch.command(2, Command::Focus, 0.0);
    view.commit(&batch.finish(), false, &mut broker).unwrap();
    assert_eq!(focus(&view), 5, "focus ignores an inert element");
    // The accessibility tree keeps the dialog and drops the inert rest.
    let roles: Vec<_> = view.snapshot()["accessibility"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| (node["role"].clone(), node["modal"].clone()))
        .collect();
    assert_eq!(
        roles,
        [(serde_json::json!("dialog"), serde_json::json!(true))]
    );
    // A second modal dialog on top: only it is reachable; Esc closes it alone
    // and focus returns to where it was before.
    let mut batch = BatchWriter::new();
    batch.command(7, Command::ShowModal, 0.0);
    view.commit(&batch.finish(), false, &mut broker).unwrap();
    assert_eq!(focus(&view), 8);
    assert!(matches!(
        view.input(
            &Input::Click {
                target: Target::Node(6)
            },
            &mut broker
        ),
        Err(varta_view::InputError::Inert(_))
    ));
    press(&mut view, &mut broker, "Escape", false);
    let snapshot = view.snapshot();
    assert_eq!(snapshot["tree"]["children"][3]["open"], false);
    assert_eq!(snapshot["tree"]["children"][2]["open"], true);
    assert_eq!(focus(&view), 5);
    press(&mut view, &mut broker, "Escape", false);
    assert_eq!(view.snapshot()["tree"]["children"][2]["open"], false);
    assert_eq!(
        focus(&view),
        2,
        "focus returns to the element focused before"
    );
    // The background is live again.
    view.input(
        &Input::Click {
            target: Target::Node(3),
        },
        &mut broker,
    )
    .unwrap();
    assert_eq!(focus(&view), 3);
    // autofocus wins; a dialog with nothing focusable takes focus itself.
    let mut batch = BatchWriter::new();
    batch
        .set_attribute(6, Attribute::Autofocus, "")
        .command(4, Command::ShowModal, 0.0);
    view.commit(&batch.finish(), false, &mut broker).unwrap();
    assert_eq!(focus(&view), 6);
    let mut batch = BatchWriter::new();
    batch
        .command(4, Command::Close, 0.0)
        .create_element(9, Tag::Dialog)
        .insert(1, 9, 0)
        .command(9, Command::ShowModal, 0.0);
    view.commit(&batch.finish(), false, &mut broker).unwrap();
    assert_eq!(focus(&view), 9);
}

/// Commits `build` into a fresh view and returns the top-level accessible
/// nodes with a role other than `text`.
fn accessible(build: impl FnOnce(&mut BatchWriter)) -> Vec<serde_json::Value> {
    let mut view = view();
    let mut batch = BatchWriter::new();
    build(&mut batch);
    commit(&mut view, &batch).unwrap();
    let snapshot = view.snapshot();
    snapshot["accessibility"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["role"] != "text")
        .cloned()
        .collect()
}

/// Appends a `span` (`node`) holding `text` (`node + 1`) to `parent`.
fn span(batch: &mut BatchWriter, parent: u32, node: u32, text: &str, hidden: bool) {
    batch
        .create_element(node, Tag::Span)
        .create_text(node + 1, text)
        .insert(node, node + 1, 0)
        .insert(parent, node, 0);
    if hidden {
        batch.set_attribute(node, Attribute::AriaHidden, "true");
    }
}

#[test]
fn accessible_names_exclude_hidden_descendants() {
    let nodes = accessible(|batch| {
        batch
            .create_element(2, Tag::Button)
            .set_attribute(2, Attribute::Type, "button")
            .set_class(2, "event toggle")
            .insert(1, 2, 0);
        span(batch, 2, 10, "◇", true);
        span(batch, 2, 20, "Reasoning", false);
        span(batch, 2, 30, "▸", true);
    });
    assert_eq!(nodes[0]["role"], "button");
    assert_eq!(nodes[0]["name"], "Reasoning");

    // Nested and mixed: a hidden span inside a visible one, `display: none`,
    // a closed dialog, `aria-hidden="false"` and flat-string whitespace.
    let nodes = accessible(|batch| {
        batch.create_element(2, Tag::A).insert(1, 2, 0);
        span(batch, 2, 10, " Open ", false);
        span(batch, 10, 12, "(icon)", true);
        span(batch, 2, 20, "the\n  file", false);
        batch.set_attribute(20, Attribute::AriaHidden, "false");
        span(batch, 2, 30, "secret", false);
        batch.set_style(30, CssProperty::Display, "none");
        batch.create_element(40, Tag::Dialog).insert(2, 40, 0);
        span(batch, 40, 41, "dialog text", false);
        // A button whose whole content is hidden has no name.
        batch.create_element(50, Tag::Button).insert(1, 50, 0);
        span(batch, 50, 51, "×", true);
    });
    assert_eq!(nodes[0]["role"], "link");
    assert_eq!(nodes[0]["name"], "Open the file");
    assert_eq!(nodes[1]["role"], "button");
    assert!(nodes[1].get("name").is_none());
}

#[test]
fn section_landmarks_include_labelledby() {
    let nodes = accessible(|batch| {
        batch
            .create_element(2, Tag::Span)
            .set_attribute(2, Attribute::Id, "label")
            .create_text(3, "Settings")
            .insert(2, 3, 0)
            .insert(1, 2, 0);
        batch
            .create_element(10, Tag::Section)
            .set_attribute(10, Attribute::AriaLabelledby, "label")
            .insert(1, 10, 0);
        batch
            .create_element(20, Tag::Section)
            .set_attribute(20, Attribute::AriaLabel, "  ")
            .create_element(21, Tag::Button)
            .create_text(22, "Inside")
            .insert(21, 22, 0)
            .insert(20, 21, 0)
            .insert(1, 20, 0);
        batch
            .create_element(30, Tag::Section)
            .set_attribute(30, Attribute::AriaLabelledby, "missing")
            .insert(1, 30, 0);
        batch
            .create_element(40, Tag::Section)
            .set_attribute(40, Attribute::AriaLabel, "Help")
            .insert(1, 40, 0);
    });
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0]["role"], "region");
    assert_eq!(nodes[0]["name"], "Settings");
    assert_eq!(nodes[1]["role"], "region");
    assert_eq!(nodes[1]["children"][0]["name"], "Inside");
    assert_eq!(nodes[2]["role"], "region");
    assert!(nodes[2].get("name").is_none());
    assert_eq!(nodes[3]["name"], "Help");
}

#[test]
fn accessible_names_follow_labelledby_labels_and_roles() {
    let nodes = accessible(|batch| {
        // aria-labelledby over a visible node excludes its hidden part, skips
        // an unknown id, and a hidden referenced node contributes its subtree.
        batch
            .create_element(2, Tag::Button)
            .set_attribute(2, Attribute::AriaLabelledby, "a missing b")
            .create_text(3, "ignored content")
            .insert(2, 3, 0)
            .insert(1, 2, 0);
        span(batch, 1, 10, "Send", false);
        batch.set_attribute(10, Attribute::Id, "a");
        span(batch, 10, 12, "*", true);
        span(batch, 1, 20, "now", true);
        batch.set_attribute(20, Attribute::Id, "b");
        // A blank aria-label is ignored; name from content applies.
        batch
            .create_element(30, Tag::Button)
            .set_attribute(30, Attribute::AriaLabel, "  ")
            .create_text(31, "Go")
            .insert(30, 31, 0)
            .insert(1, 30, 0);
        // A label with a hidden part, around its own field.
        batch
            .create_element(40, Tag::Label)
            .set_attribute(40, Attribute::For, "q")
            .insert(1, 40, 0);
        span(batch, 40, 41, "Query", false);
        span(batch, 40, 43, "!", true);
        batch
            .create_element(45, Tag::Input)
            .set_attribute(45, Attribute::Id, "q")
            .insert(40, 45, 0)
            .set_value(45, "typed", 0);
        // A textarea named only by its placeholder.
        batch
            .create_element(50, Tag::Textarea)
            .set_attribute(50, Attribute::Placeholder, "Message")
            .insert(1, 50, 0);
        // A listitem is named by its author only; a cell by its content.
        batch
            .create_element(60, Tag::Li)
            .create_text(61, "item")
            .insert(60, 61, 0)
            .insert(1, 60, 0)
            .create_element(70, Tag::Div)
            .set_attribute(70, Attribute::Role, "cell")
            .insert(1, 70, 0);
        span(batch, 70, 71, "cell", false);
    });
    let roles: Vec<_> = nodes.iter().map(|node| node["role"].clone()).collect();
    assert_eq!(
        roles,
        ["button", "button", "textbox", "textbox", "listitem", "cell"]
    );
    assert_eq!(nodes[0]["name"], "Send now");
    assert_eq!(nodes[1]["name"], "Go");
    // The field inside its own label is not "another widget".
    assert_eq!(nodes[2]["name"], "Query");
    assert_eq!(nodes[3]["name"], "Message");
    assert!(nodes[4].get("name").is_none());
    assert_eq!(nodes[5]["name"], "cell");
}

#[test]
fn embedded_controls_give_their_value_inside_another_name() {
    let nodes = accessible(|batch| {
        batch
            .create_element(2, Tag::Button)
            .set_attribute(2, Attribute::AriaLabelledby, "pre field choice")
            .insert(1, 2, 0);
        span(batch, 1, 10, "Retry in", false);
        batch.set_attribute(10, Attribute::Id, "pre");
        batch
            .create_element(20, Tag::Input)
            .set_attribute(20, Attribute::Id, "field")
            .set_attribute(20, Attribute::AriaLabel, "Delay")
            .insert(1, 20, 0)
            .set_value(20, "5", 0);
        batch
            .create_element(30, Tag::Select)
            .set_attribute(30, Attribute::Id, "choice")
            .insert(1, 30, 0)
            .create_element(31, Tag::Option)
            .set_attribute(31, Attribute::Value, "s")
            .create_text(32, "seconds")
            .insert(31, 32, 0)
            .insert(30, 31, 0)
            .set_value(30, "s", 0);
    });
    // In recursion the field's value replaces its aria-label (2C before 2D).
    assert_eq!(nodes[0]["name"], "Retry in 5 seconds");
    assert_eq!(nodes[1]["name"], "Delay");
}
