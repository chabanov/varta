//! Native Unicode paragraph geometry for an independent browser comparison.
use varta_common::view::{CssProperty, Tag};
use varta_view::{Access, BatchWriter, Broker, TextMeasurer, View};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font_bytes = include_bytes!("../tests/fixtures/unicode.ttf");
    let family = "Varta Unicode Fixture";
    let mut font = TextMeasurer::new(font_bytes.to_vec(), family)?;
    let mut cases = Vec::new();
    for display in ["block", "flex", "grid"] {
        for (text, visual_glyphs, whitespace, width) in [
            ("↑é中", Some("AAA"), "normal", "120px"),
            ("🙂◇", Some("BB"), "normal", "120px"),
            ("Aאב", Some("ABA"), "normal", "120px"),
            ("Aבא", Some("AAB"), "normal", "120px"),
            ("Aلا", Some("AAB"), "normal", "120px"),
            ("A\u{035d}", None, "normal", "120px"),
            ("中中中中中中中中", None, "normal", "16px"),
            ("中中中中中中中中", None, "nowrap", "16px"),
            ("↑\n🙂", None, "pre-wrap", "16px"),
            ("↑ \n  🙂", None, "normal", "120px"),
            ("↑\u{00a0}🙂", None, "normal", "8px"),
        ] {
            let styles = [
                (CssProperty::Display, display),
                (CssProperty::Width, width),
                (CssProperty::WhiteSpace, whitespace),
                (CssProperty::FontFamily, "\"Varta Unicode Fixture\""),
                (CssProperty::FontSize, "20px"),
                (CssProperty::LineHeight, "20px"),
                (CssProperty::Color, "#00f"),
            ];
            let mut view = View::default();
            let mut batch = BatchWriter::new();
            batch
                .create_element(2, Tag::Div)
                .create_text(3, text)
                .insert(2, 3, 0)
                .insert(1, 2, 0);
            for (property, value) in styles {
                batch.set_style(2, property, value);
            }
            view.commit(&batch.finish(), false, &mut Denied)?;
            let bounds = view.layout_boxes_with_text(160.0, 120.0, &mut font, 20.0)?[&2];
            view.rasterize_with_text(160, 120, &mut font, 20.0)?;
            cases.push(serde_json::json!({
                "text": text, "visualGlyphs": visual_glyphs,
                "styles": styles.map(|(property,value)| (property.name(),value)),
                "bounds": {"x":bounds.x,"y":bounds.y,"width":bounds.width,"height":bounds.height}
            }));
        }
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "font":font_bytes.as_slice(),"family":family,"cases":cases
    }))?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, output)?;
    } else {
        use std::io::Write;
        std::io::stdout().write_all(&output)?;
    }
    Ok(())
}
