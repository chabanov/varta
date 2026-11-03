//! Native rem geometry with explicit document-root reference metrics.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter, Broker, Environment, TextMeasurer, View};

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
    for reference in [16.0, 20.0, 24.0] {
        for (name, parent, child, text) in [
            (
                "flex-edges",
                vec![
                    (P::Display, "flex"),
                    (P::Width, "12rem"),
                    (P::Height, "4rem"),
                    (P::Padding, ".5rem .25rem"),
                    (P::Margin, ".25rem .5rem"),
                    (P::Border, ".25rem solid #00f"),
                    (P::Gap, ".5rem"),
                    (P::Outline, ".25rem solid #f00"),
                    (P::OutlineOffset, "-.25rem"),
                ],
                vec![
                    (P::Width, "1rem"),
                    (P::Height, "1rem"),
                    (P::FlexBasis, "2rem"),
                    (P::MinWidth, "1rem"),
                    (P::MaxWidth, "3rem"),
                ],
                None,
            ),
            (
                "grid",
                vec![
                    (P::Display, "grid"),
                    (P::Width, "12rem"),
                    (P::GridTemplateColumns, "repeat(2, minmax(1rem, 2rem)) 25%"),
                    (P::GridTemplateRows, "1rem minmax(0, 2rem)"),
                    (P::Gap, ".25rem .5rem"),
                ],
                vec![(P::GridColumn, "2"), (P::GridRow, "2"), (P::Height, "1rem")],
                None,
            ),
            (
                "positioned",
                vec![
                    (P::Position, "relative"),
                    (P::Width, "12rem"),
                    (P::Height, "4rem"),
                ],
                vec![
                    (P::Position, "absolute"),
                    (P::Top, "-.5rem"),
                    (P::Left, "1rem"),
                    (P::Right, "2rem"),
                    (P::Bottom, ".5rem"),
                ],
                None,
            ),
            (
                "max-width",
                vec![(P::MaxWidth, "48rem"), (P::Margin, "0 auto")],
                vec![],
                None,
            ),
            (
                "text",
                vec![(P::FontSize, "25%"), (P::Width, "6rem"), (P::Color, "#00f")],
                vec![],
                Some("AAA"),
            ),
            ("media", vec![(P::Height, "1rem")], vec![], None),
        ] {
            let root_styles = [
                (P::FontSize, "2rem"),
                (P::LineHeight, "1.5rem"),
                (P::FontFamily, "\"Varta Unicode Fixture\""),
            ];
            let mut view = View::new(
                Environment::default().with_root_font_size(reference)?,
                Default::default(),
            );
            let mut batch = BatchWriter::new();
            for (property, value) in root_styles {
                batch.set_style(1, property, value);
            }
            batch.create_element(2, Tag::Div).insert(1, 2, 0);
            let stylesheet = if name == "media" {
                ".media { width:2rem } @media (min-width:64rem) { .media { width:3rem } }"
            } else {
                ""
            };
            if !stylesheet.is_empty() {
                batch.stylesheet(1, stylesheet).set_class(2, "media");
            }
            if let Some(text) = text {
                batch.create_text(3, text);
            } else {
                batch.create_element(3, Tag::Div);
            }
            batch.insert(2, 3, 0);
            for &(property, value) in &parent {
                batch.set_style(2, property, value);
            }
            for &(property, value) in &child {
                batch.set_style(3, property, value);
            }
            view.commit(&batch.finish(), false, &mut Denied)?;
            let boxes = if text.is_some() {
                view.layout_boxes_with_text(1024.0, 200.0, &mut font, 16.0)?
            } else {
                view.layout_boxes(1024.0, 200.0)?
            };
            let rectangles = [2, 3].map(|id| {
                let r = boxes[&id];
                serde_json::json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height})
            });
            cases.push(serde_json::json!({
                "name":name,"reference":reference,"text":text,"stylesheet":stylesheet,
                "rootStyles":root_styles.map(|(p,v)|(p.name(),v)),
                "parentStyles":parent.iter().map(|(p,v)|(p.name(),v)).collect::<Vec<_>>(),
                "childStyles":child.iter().map(|(p,v)|(p.name(),v)).collect::<Vec<_>>(),
                "boxes":rectangles
            }));
        }
    }
    let output = serde_json::to_vec(
        &serde_json::json!({"font":font_bytes.as_slice(),"family":family,"cases":cases}),
    )?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, output)?;
    } else {
        use std::io::Write;
        std::io::stdout().write_all(&output)?;
    }
    Ok(())
}
