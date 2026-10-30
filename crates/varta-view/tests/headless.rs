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
    batch
        .create_element(2, Tag::Input)
        .set_attribute(2, Attribute::Type, "checkbox")
        .insert(1, 2, 0)
        .listen(2, EventKind::Change, 1, ANY)
        .create_element(3, Tag::Dialog)
        .insert(1, 3, 0)
        .listen(3, EventKind::Cancel, 2, ANY)
        .listen(3, EventKind::Close, 3, ANY)
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
    assert_eq!(snapshot["tree"]["children"][0]["checked"], true);
    assert_eq!(snapshot["focus"], 2);
    assert_eq!(snapshot["accessibility"]["children"][1]["modal"], true);
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
    assert_eq!(view.snapshot()["tree"]["children"][1]["open"], false);
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
