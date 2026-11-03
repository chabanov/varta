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
fn password_preedit_can_join_an_existing_grapheme() {
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
}

#[test]
fn full_size_unicode_values_and_password_masks_render_without_losing_source_positions() {
    let units = varta_common::view::DRAFT_MAX_VALUE_BYTES / 2;
    let mut font = font();
    for (kind, value) in [
        ("password", "A".repeat(units)),
        ("text", "界".repeat(units)),
        ("search", "界".repeat(units)),
        ("text", "😀".repeat(units / 2)),
        ("text", "界\u{0301}".repeat(units / 2)),
    ] {
        let mut view = field(kind, &value);
        select(&mut view, 0, 0);
        let start = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
        assert_eq!(start.text_position(2, -1_000_000.0), Some(0));
        assert_eq!(start.text_position(2, 1_000_000.0), Some(value.len()));
        select(&mut view, value.len(), value.len());
        let end = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
        assert_eq!(end.text_position(2, 1_000_000.0), Some(value.len()));
        assert!(end.ime_cursor_area().is_some());
        assert_eq!(view.snapshot()["tree"]["children"][0]["value"], value);
        let before = view.snapshot();
        let mut excessive = BatchWriter::new();
        excessive.set_value(2, &format!("{value}A"), 0);
        assert!(
            view.commit(&excessive.finish(), false, &mut Denied)
                .is_err()
        );
        assert_eq!(view.snapshot(), before);
        key(&mut view, "Backspace");
        let edited = view.next_delivery().unwrap().record.value;
        let expected = match kind {
            "password" => "A".repeat(units - 1),
            _ => {
                use unicode_segmentation::UnicodeSegmentation;
                let last = value.grapheme_indices(true).next_back().unwrap().0;
                value[..last].into()
            }
        };
        assert_eq!(edited, expected);
    }
}

#[test]
fn full_size_preedit_replacement_and_overflow_keep_the_existing_value_boundary() {
    let units = varta_common::view::DRAFT_MAX_VALUE_BYTES / 2;
    let initial = "A".repeat(units);
    let replacement = "界".repeat(units);
    let mut font = font();
    for kind in ["text", "password"] {
        let mut view = field(kind, &initial);
        select(&mut view, 0, initial.len());
        view.input(
            &Input::ImePreedit {
                target: Target::Node(2),
                text: replacement.clone(),
                cursor: Some((replacement.len(), replacement.len())),
            },
            &mut Denied,
        )
        .unwrap();
        let frame = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
        assert!(frame.ime_cursor_area().is_some());
        assert_eq!(frame.text_position(2, 1_000_000.0), None);
        assert_eq!(view.snapshot()["tree"]["children"][0]["value"], initial);
        view.input(
            &Input::ImeCommit {
                target: Target::Node(2),
                text: replacement.clone(),
            },
            &mut Denied,
        )
        .unwrap();
        assert_eq!(view.next_delivery().unwrap().record.value, replacement);
        select(&mut view, replacement.len(), replacement.len());
        view.input(
            &Input::ImePreedit {
                target: Target::Node(2),
                text: "A".into(),
                cursor: Some((1, 1)),
            },
            &mut Denied,
        )
        .unwrap();
        let frame = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
        assert_eq!(frame.text_position(2, 1_000_000.0), Some(replacement.len()));
        assert_eq!(view.snapshot()["tree"]["children"][0]["value"], replacement);
    }
}

#[test]
fn full_size_field_placeholder_and_content_sized_textarea_use_the_field_bound() {
    let value = "界".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2);
    let mut font = font();
    for kind in ["text", "password", "search"] {
        let mut view = field(kind, "");
        let mut batch = BatchWriter::new();
        batch.set_attribute(2, Attribute::Placeholder, &value);
        commit(&mut view, &batch);
        let frame = view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap();
        assert_eq!(frame.text_position(2, 1_000_000.0), Some(0));
        assert_eq!(view.snapshot()["tree"]["children"][0]["value"], "");
    }
    let mut view = field("text", "");
    let mut batch = BatchWriter::new();
    batch
        .remove(2)
        .create_element(3, Tag::Textarea)
        .insert(1, 3, 0);
    // Reuse the explicitly supported field styles rather than a platform default.
    for (property, text) in [
        (CssProperty::Display, "block"),
        (CssProperty::Width, "80px"),
        (CssProperty::Height, "auto"),
        (CssProperty::MaxHeight, "60px"),
        (CssProperty::MinWidth, "0"),
        (CssProperty::MinHeight, "0"),
        (CssProperty::FieldSizing, "content"),
        (CssProperty::Resize, "none"),
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
        batch.set_style(3, property, text);
    }
    batch.set_value(3, &value, 0);
    commit(&mut view, &batch);
    let frame = view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
    assert_eq!(
        frame.text_position_at(3, 1_000_000.0, 1_000_000.0),
        Some(value.len())
    );
    assert_eq!(view.snapshot()["tree"]["children"][0]["value"], value);
    let mut batch = BatchWriter::new();
    batch
        .set_value(3, "", 0)
        .set_attribute(3, Attribute::Placeholder, &value);
    commit(&mut view, &batch);
    view.rasterize_with_text(100, 100, &mut font, 20.0).unwrap();
}

#[test]
fn large_mixed_direction_context_matches_its_visible_suffix() {
    let units = varta_common::view::DRAFT_MAX_VALUE_BYTES / 2;
    let suffix = "אבג ABC e\u{0301}";
    let long = format!(
        "{}{}",
        "界".repeat(units - suffix.encode_utf16().count()),
        suffix
    );
    let short = format!("{}{}", "界".repeat(16), suffix);
    let mut font = font();
    let mut frames = Vec::new();
    for value in [&long, &short] {
        let mut view = field("text", value);
        select(&mut view, value.len(), value.len());
        frames.push(view.rasterize_with_text(100, 50, &mut font, 20.0).unwrap());
    }
    assert_eq!(frames[0].pixels, frames[1].pixels);
    assert_eq!(frames[0].ime_cursor_area(), frames[1].ime_cursor_area());
    let prefix_bytes = long.len() - short.len();
    for x in (6..78).step_by(4) {
        let a = frames[0].text_position(2, x as f32).unwrap();
        let b = frames[1].text_position(2, x as f32).unwrap();
        assert_eq!(a, b + prefix_bytes, "x={x}");
        assert!(long.is_char_boundary(a));
    }
    // A field render must not leave an enlarged allowance in the reused shaper.
    assert!(matches!(
        font.measure(&long, 20.0, None),
        Err(varta_view::TextMeasureError::Limit)
    ));
}

#[test]
fn expanded_combining_cluster_respects_the_existing_paint_glyph_limit() {
    let mut font = TextMeasurer::new(
        include_bytes!("fixtures/clusters.ttf").to_vec(),
        "Varta Cluster Fixture",
    )
    .unwrap();
    let mut view = field("text", "");
    let mut batch = BatchWriter::new();
    batch.set_style(2, CssProperty::FontFamily, "\"Varta Cluster Fixture\"");
    commit(&mut view, &batch);
    for (count, accepted) in [(21_846, false), (21_845, true), (4, true)] {
        let value = format!("A{}", "\u{035c}".repeat(count));
        let mut batch = BatchWriter::new();
        batch.set_value(2, &value, 0);
        commit(&mut view, &batch);
        let frame = view.rasterize_with_text(100, 50, &mut font, 20.0);
        if accepted {
            let frame = frame.unwrap();
            assert_eq!(frame.text_position(2, 1_000_000.0), Some(value.len()));
        } else {
            assert!(matches!(
                frame,
                Err(varta_view::PaintError::Layout(
                    varta_view::LayoutError::Text(_)
                ))
            ));
        }
        assert_eq!(view.snapshot()["tree"]["children"][0]["value"], value);
    }
}
