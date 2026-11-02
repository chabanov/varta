//! Native field display stays separate from guest values and UTF-8 editing.
use varta_common::view::{Attribute, Command, CssProperty, EventKind, Tag};
use varta_view::{
    Access, BatchWriter, Broker, Filter, HistoryAction, Input, Target, TextMeasurer, View,
};

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

fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap()
}

fn field(kind: &str, value: &str) -> View {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Input)
        .set_attribute(2, Attribute::Type, kind)
        .set_value(2, value, 0)
        .listen(2, EventKind::Input, 10, Filter::default())
        .insert(1, 2, 0);
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
        (CssProperty::FontFamily, "\"Varta Password Fixture\""),
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

fn select(view: &mut View, anchor: usize, focus: usize) {
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor,
            focus,
            activate: false,
        },
        &mut Denied,
    )
    .unwrap();
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

#[test]
fn password_paints_only_masks_and_search_uses_plain_text() {
    let mut font = font();
    let plaintext = field("text", "AAAA")
        .rasterize_with_text(100, 50, &mut font, 20.0)
        .unwrap();
    let masked = field("password", "AAAA")
        .rasterize_with_text(100, 50, &mut font, 20.0)
        .unwrap();
    // The fixture's A is a tall rectangle; its mask is a small square.
    assert_ne!(masked.pixels, plaintext.pixels);
    for (value, masks) in [
        ("AAAA", "••••"),
        ("A😀B", "•••"),
        ("Ae\u{0301}B", "•••"),
        ("A👨‍👩‍👧B", "•••"),
        ("AאבB", "••••"),
        ("", ""),
    ] {
        let password = field("password", value);
        let actual = password
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap();
        let expected = field("text", masks)
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap();
        assert_eq!(actual.pixels, expected.pixels, "{value:?}");
        assert_eq!(password.snapshot()["tree"]["children"][0]["value"], value);
    }
    assert_eq!(
        field("search", "AAAA")
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels,
        plaintext.pixels
    );
    // Numeric sanitization/spinners have not been implemented by this renderer.
    assert!(
        field("number", "1")
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .is_err()
    );
    assert!(
        field("password", "A\nB")
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .is_err()
    );
}

#[test]
fn password_hit_testing_selection_editing_and_undo_keep_source_offsets() {
    let value = "AФe\u{0301}😀B";
    let mut password = field("password", value);
    let mut font = font();
    let image = password
        .rasterize_with_text(100, 50, &mut font, 20.0)
        .unwrap();
    for (x, offset) in [
        (7.0, 0),
        (12.0, 1),
        (20.0, 3),
        (28.0, 6),
        (36.0, 10),
        (60.0, 11),
    ] {
        assert_eq!(image.text_position(2, x), Some(offset));
        assert_eq!(image.text_position_at(2, x, 15.0), Some(offset));
    }
    select(&mut password, 3, 10);
    let selected = password
        .rasterize_with_text(100, 50, &mut font, 20.0)
        .unwrap();
    let mut reference = field("text", "•••••");
    select(&mut reference, 6, 12);
    assert_eq!(
        selected.pixels,
        reference
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels
    );
    let scaled = password
        .rasterize_scaled(200, 100, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    assert_eq!(scaled.text_position_at(2, 56.0, 30.0), Some(6));
    let caret = selected.ime_cursor_area().unwrap();
    let scaled_caret = scaled.ime_cursor_area().unwrap();
    assert_eq!(scaled_caret.x, caret.x * 2.0);
    assert_eq!(scaled_caret.y, caret.y * 2.0);
    password
        .input(
            &Input::Type {
                target: Target::Node(2),
                text: "X".into(),
            },
            &mut Denied,
        )
        .unwrap();
    assert_eq!(password.next_delivery().unwrap().record.value, "AФXB");
    password
        .input(
            &Input::TextKey {
                target: Target::Node(2),
                key: "z".into(),
                text: String::new(),
                shift: false,
                ctrl: true,
                alt: false,
                meta: false,
                select_all: false,
                history: Some(HistoryAction::Undo),
            },
            &mut Denied,
        )
        .unwrap();
    assert_eq!(password.next_delivery().unwrap().record.value, value);
    assert_eq!(
        password
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels,
        selected.pixels
    );
    select(&mut password, 6, 6);
    key(&mut password, "Backspace");
    assert_eq!(password.next_delivery().unwrap().record.value, "AФ😀B");
}

#[test]
fn password_masks_preedit_and_preserves_placeholder_and_type_changes() {
    let mut password = field("password", "AФ");
    let mut reference = field("text", "••");
    let mut font = font();
    select(&mut password, 1, 3);
    select(&mut reference, 3, 6);
    for (view, text, cursor) in [(&mut password, "😀e\u{0301}", 7), (&mut reference, "••", 6)]
    {
        view.input(
            &Input::ImePreedit {
                target: Target::Node(2),
                text: text.into(),
                cursor: Some((cursor, cursor)),
            },
            &mut Denied,
        )
        .unwrap();
    }
    let preedit = password
        .rasterize_with_text(100, 50, &mut font, 20.0)
        .unwrap();
    assert_eq!(preedit.text_position(2, 20.0), None);
    assert_eq!(
        preedit.pixels,
        reference
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels
    );
    assert_eq!(password.snapshot()["tree"]["children"][0]["value"], "AФ");
    password
        .input(
            &Input::ImeCommit {
                target: Target::Node(2),
                text: "😀e\u{0301}".into(),
            },
            &mut Denied,
        )
        .unwrap();
    assert_eq!(
        password.next_delivery().unwrap().record.value,
        "A😀e\u{0301}"
    );
    let mut mode = BatchWriter::new();
    mode.set_attribute(2, Attribute::Type, "text");
    commit(&mut password, &mode);
    assert_eq!(
        password.snapshot()["tree"]["children"][0]["value"],
        "A😀e\u{0301}"
    );
    let visible = password
        .rasterize_with_text(100, 50, &mut font, 20.0)
        .unwrap();
    mode.set_attribute(2, Attribute::Type, "password");
    commit(&mut password, &mode);
    assert_ne!(
        visible.pixels,
        password
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels
    );
    let mut placeholder = BatchWriter::new();
    placeholder
        .set_value(2, "", 1)
        .set_attribute(2, Attribute::Placeholder, "AAAA")
        .command(2, Command::Blur, 0.0);
    commit(&mut password, &placeholder);
    let mut plain = field("text", "");
    placeholder = BatchWriter::new();
    placeholder.set_attribute(2, Attribute::Placeholder, "AAAA");
    commit(&mut plain, &placeholder);
    assert_eq!(
        password
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels,
        plain
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels
    );
}

#[test]
fn password_preedit_can_join_an_existing_grapheme_and_retains_shaping_limits() {
    let mut password = field("password", "e");
    let mut reference = field("text", "");
    let mut font = font();
    select(&mut password, 1, 1);
    select(&mut reference, 0, 0);
    for (view, text, cursor) in [(&mut password, "\u{0301}", 2), (&mut reference, "•", 3)] {
        view.input(
            &Input::ImePreedit {
                target: Target::Node(2),
                text: text.into(),
                cursor: Some((cursor, cursor)),
            },
            &mut Denied,
        )
        .unwrap();
    }
    assert_eq!(
        password
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels,
        reference
            .rasterize_with_text(100, 50, &mut font, 20.0)
            .unwrap()
            .pixels
    );
    password
        .input(
            &Input::ImeCommit {
                target: Target::Node(2),
                text: "\u{0301}".into(),
            },
            &mut Denied,
        )
        .unwrap();
    assert_eq!(password.next_delivery().unwrap().record.value, "e\u{0301}");
    key(&mut password, "Backspace");
    assert_eq!(password.next_delivery().unwrap().record.value, "");
    let large_value = "A".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2);
    let large = field("password", &large_value);
    assert!(matches!(
        large.rasterize_with_text(100, 50, &mut font, 20.0),
        Err(varta_view::PaintError::Layout(
            varta_view::LayoutError::Text(_)
        ))
    ));
    assert_eq!(
        large.snapshot()["tree"]["children"][0]["value"],
        large_value
    );
}
