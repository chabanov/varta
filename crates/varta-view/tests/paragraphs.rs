//! Unicode text nodes use the same bounded shaper as native controls.
use varta_common::view::{CssProperty, Tag};
use varta_view::{Access, BatchWriter, Broker, LayoutError, TextMeasurer, View};

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

fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/unicode.ttf").to_vec(),
        "Varta Unicode Fixture",
    )
    .unwrap()
}

fn paragraph(text: &str, display: &str, whitespace: &str, width: u32) -> View {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .create_text(3, text)
        .insert(2, 3, 0)
        .insert(1, 2, 0)
        .set_style(2, CssProperty::Width, &format!("{width}px"));
    for (property, value) in [
        (CssProperty::Display, display),
        (CssProperty::WhiteSpace, whitespace),
        (CssProperty::FontFamily, "\"Varta Unicode Fixture\""),
        (CssProperty::FontSize, "20px"),
        (CssProperty::LineHeight, "20px"),
        (CssProperty::Color, "#00f"),
    ] {
        batch.set_style(2, property, value);
    }
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    view
}

#[test]
fn unicode_paragraphs_preserve_text_glyphs_and_bidirectional_runs() {
    let mut font = font();
    // The original fixture maps each pair to the same glyph sequence. Hebrew
    // and Arabic runs reverse their distinct bar/square glyphs in visual order.
    for (source, visual_glyphs) in [
        ("↑é中", "AAA"),
        ("🙂◇", "BB"),
        ("Aאב", "ABA"),
        ("Aבא", "AAB"),
        ("Aلا", "AAB"),
    ] {
        for display in ["block", "flex", "grid"] {
            let view = paragraph(source, display, "normal", 120);
            let snapshot = view.snapshot();
            let geometry = view
                .layout_boxes_with_text(160.0, 80.0, &mut font, 20.0)
                .unwrap();
            assert_eq!(geometry[&2].height, 20.0, "{source:?}, {display}");
            let image = view.rasterize_with_text(160, 80, &mut font, 20.0).unwrap();
            let expected = paragraph(visual_glyphs, display, "normal", 120)
                .rasterize_with_text(160, 80, &mut font, 20.0)
                .unwrap();
            assert!(
                image
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] != 0)
            );
            assert_eq!(image.pixels, expected.pixels, "{source:?}, {display}");
            assert_eq!(view.snapshot(), snapshot);
            assert_eq!(
                snapshot["tree"]["children"][0]["children"][0]["text"],
                source
            );
        }
    }
    let combined = paragraph("A\u{035d}", "block", "normal", 120);
    let plain = paragraph("A", "block", "normal", 120);
    assert_eq!(
        combined
            .layout_boxes_with_text(160.0, 80.0, &mut font, 20.0)
            .unwrap()[&2]
            .height,
        20.0
    );
    assert_ne!(
        combined
            .rasterize_with_text(160, 80, &mut font, 20.0)
            .unwrap()
            .pixels,
        plain
            .rasterize_with_text(160, 80, &mut font, 20.0)
            .unwrap()
            .pixels,
        "the zero-advance combining mark must be painted"
    );
}

#[test]
fn unicode_paragraphs_wrap_without_losing_css_whitespace_rules() {
    let mut font = font();
    for (text, whitespace, width, height) in [
        ("中中中中中中中中", "normal", 16, 80.0),
        ("中中中中中中中中", "nowrap", 16, 20.0),
        ("↑\n🙂", "pre-wrap", 16, 40.0),
        ("↑ \n  🙂", "normal", 120, 20.0),
        ("↑\u{00a0}🙂", "normal", 8, 20.0),
    ] {
        let view = paragraph(text, "block", whitespace, width);
        let rects = view
            .layout_boxes_with_text(160.0, 120.0, &mut font, 20.0)
            .unwrap();
        assert_eq!(rects[&2].height, height, "{text:?}, {whitespace}");
        view.rasterize_scaled(320, 240, 2.0, Some((&mut font, 20.0)))
            .unwrap();
    }
    for text in ["↑\u{0007}", "↑\u{0085}"] {
        assert!(matches!(
            paragraph(text, "block", "normal", 120)
                .layout_boxes_with_text(160.0, 80.0, &mut font, 20.0),
            Err(LayoutError::Unsupported(3))
        ));
    }
    // A preserved tab and trailing space are laid out on their one line.
    for text in ["\t↑", "↑ "] {
        let rects = paragraph(text, "block", "pre-wrap", 120)
            .layout_boxes_with_text(160.0, 80.0, &mut font, 20.0)
            .unwrap();
        assert_eq!(rects[&2].height, 20.0, "{text:?}");
    }
    // NUL is rejected earlier, while decoding the transaction itself.
    let mut view = paragraph("↑", "block", "normal", 120);
    let before = view.snapshot();
    let mut change = BatchWriter::new();
    change.set_text(3, "↑\0");
    assert!(matches!(
        view.commit(&change.finish(), false, &mut Denied),
        Err(varta_view::ViewError::Invalid {
            violation: varta_view::Violation::NulCharacter,
            ..
        })
    ));
    assert_eq!(view.snapshot(), before);
}

#[test]
fn unicode_paragraphs_keep_the_general_utf8_limit_and_recover_after_refusal() {
    const MAX_TEXT_BYTES: usize = 64 * 1024;
    let mut font = font();
    let text = "↑".repeat(MAX_TEXT_BYTES / "↑".len());
    let mut view = paragraph(&text, "block", "nowrap", 120);
    let snapshot = view.snapshot();
    view.rasterize_with_text(160, 40, &mut font, 20.0).unwrap();
    let mut change = BatchWriter::new();
    change.set_text(3, &(text.clone() + "↑"));
    view.commit(&change.finish(), false, &mut Denied).unwrap();
    assert!(matches!(
        view.layout_boxes_with_text(160.0, 80.0, &mut font, 20.0),
        Err(LayoutError::Text(_))
    ));
    change = BatchWriter::new();
    change.set_text(3, &text);
    view.commit(&change.finish(), false, &mut Denied).unwrap();
    assert_eq!(view.snapshot(), snapshot);
    view.rasterize_with_text(160, 40, &mut font, 20.0).unwrap();
}
