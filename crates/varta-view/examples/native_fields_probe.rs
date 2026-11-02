//! Export native field geometry and source offsets for an independent Chrome probe.
use varta_common::view::{Attribute, CssProperty, Tag};
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
    let font_bytes = include_bytes!("../tests/fixtures/password.ttf");
    let family = "Varta Password Fixture";
    let mut font = TextMeasurer::new(font_bytes.to_vec(), family)?;
    let styles = [
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
    ];
    let mut cases = Vec::new();
    for (kind, value) in [
        ("password", "AФe\u{0301}😀B"),
        ("password", "A👨‍👩‍👧B"),
        ("search", "AAAA"),
        ("password", ""),
    ] {
        let mut view = View::default();
        let mut batch = BatchWriter::new();
        batch
            .create_element(2, Tag::Input)
            .set_attribute(2, Attribute::Type, kind)
            .set_value(2, value, 0)
            .insert(1, 2, 0);
        for (property, value) in styles {
            batch.set_style(2, property, value);
        }
        view.commit(&batch.finish(), false, &mut Denied)?;
        let bounds = view.layout_boxes(100.0, 50.0)?[&2];
        let image = view.rasterize_with_text(100, 50, &mut font, 20.0)?;
        let points = [7.0, 12.0, 20.0, 28.0, 36.0, 50.0].map(
            |x| serde_json::json!({"x":x,"y":15.0,"position":image.text_position_at(2,x,15.0)}),
        );
        cases.push(
            serde_json::json!({"kind":kind,"value":value,"points":points,
            "bounds":{"x":bounds.x,"y":bounds.y,"width":bounds.width,"height":bounds.height}}),
        );
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "font":font_bytes.as_slice(),"family":family,"cases":cases,
        "styles":styles.map(|(property,value)|(property.name(),value))
    }))?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, output)?;
    } else {
        use std::io::Write;
        std::io::stdout().write_all(&output)?;
    }
    Ok(())
}
