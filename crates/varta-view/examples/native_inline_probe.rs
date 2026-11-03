//! Real native inline geometry/paint data for comparison with Chrome.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, TextMeasurer, View};
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
    let family = "Varta Inline Fixture";
    let bytes = include_bytes!("../tests/fixtures/inline.ttf");
    let mut font = TextMeasurer::new(bytes.to_vec(), family)?;
    let mut cases = Vec::new();
    for name in [
        "wrapped",
        "strut",
        "ligature",
        "combining",
        "arabic",
        "br",
        "nowrap",
        "nested-font",
        "scroll",
        "empty",
        "empty-between",
        "empty-strut",
        "trailing-break",
    ] {
        let mut b = B::new();
        b.create_element(2, Tag::Div).insert(1, 2, 0);
        for (p, v) in [
            (P::Width, "100px"),
            (P::FontFamily, "\"Varta Inline Fixture\""),
            (P::FontSize, "20px"),
            (P::LineHeight, "20px"),
            (P::Color, "#00f"),
        ] {
            b.set_style(2, p, v);
        }
        let (left, middle, right) = match name {
            "wrapped" | "nowrap" | "scroll" => ("A ", "A A", " A"),
            "strut" => ("", "A", ""),
            "empty" => ("", "", ""),
            "empty-between" => ("A", "", "A"),
            "empty-strut" => ("", "", "A"),
            "trailing-break" => ("", "A", ""),
            "combining" => ("A", "\u{035d}", ""),
            "arabic" => ("ل", "ا", ""),
            _ => ("A", "A", ""),
        };
        b.create_text(3, left)
            .insert(2, 3, 0)
            .create_element(4, Tag::Span)
            .insert(2, 4, 0)
            .create_text(5, middle)
            .insert(4, 5, 0)
            .create_text(6, right)
            .insert(2, 6, 0);
        match name {
            "wrapped" | "nowrap" | "scroll" => {
                b.set_style(2, P::Width, "24px");
                if name == "nowrap" {
                    b.set_style(2, P::WhiteSpace, "nowrap");
                }
                if name == "scroll" {
                    b.set_style(2, P::Height, "20px")
                        .set_style(2, P::Overflow, "hidden");
                }
            }
            "strut" => {
                b.set_style(2, P::LineHeight, "30px")
                    .set_style(4, P::FontSize, "10px")
                    .set_style(4, P::LineHeight, "10px");
            }
            "empty-strut" => {
                b.set_style(4, P::LineHeight, "40px");
            }
            "trailing-break" => {
                b.create_element(7, Tag::Br).insert(2, 7, 6);
            }
            "nested-font" => {
                b.set_style(4, P::FontSize, "30px")
                    .create_element(7, Tag::Span)
                    .insert(4, 7, 0)
                    .insert(7, 5, 0)
                    .set_style(7, P::FontSize, "10px");
            }
            "br" => {
                b.create_element(7, Tag::Br)
                    .insert(2, 7, 6)
                    .set_text(6, "A");
            }
            _ => {}
        }
        b.set_style(4, P::Color, "#f00");
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Denied)?;
        let snapshot = v.snapshot();
        let geometry = v.layout_boxes_with_text(100.0, 100.0, &mut font, 20.0)?;
        let image = v.rasterize_with_text(100, 100, &mut font, 20.0)?;
        let boxes = geometry
            .iter()
            .filter(|(id, _)| **id != 1)
            .map(|(&id, r)| {
                (
                    id.to_string(),
                    serde_json::json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let hits = (0..70).step_by(5).flat_map(|y|(0..50).step_by(3).map(move|x|(x,y))).map(|(x,y)|serde_json::json!({"x":x,"y":y,"id":image.hit_test(x as f32+0.25,y as f32+0.25)})).collect::<Vec<_>>();
        cases.push(serde_json::json!({"name":name,"tree":snapshot["tree"],"boxes":boxes,"hits":hits,"samples":([[3,6],[10,6]].map(|[x,y]|{let offset=(y*100+x)*4;serde_json::json!({"x":x,"y":y,"rgba":&image.pixels[offset..offset+4]})}))}));
    }
    let output = serde_json::to_string(
        &serde_json::json!({"family":family,"font":bytes.as_slice(),"cases":cases}),
    )?;
    use std::io::Write;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}
