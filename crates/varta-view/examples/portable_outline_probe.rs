//! Standards-mode DOM oracle data for fragmented inline outlines.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Renderer, TextMeasurer, View};
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
    let scale = std::env::args()
        .nth(1)
        .map(|s| s.parse::<f32>())
        .transpose()?
        .unwrap_or(1.0);
    if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("invalid scale".into());
    }
    const FAMILY: &str = "Varta Decoration Fixture";
    let bytes = include_bytes!("../tests/fixtures/decoration.ttf");
    let mut cases = Vec::new();
    for name in [
        "single",
        "wrap",
        "stagger",
        "gap",
        "overlap",
        "round-single",
        "round-wrap",
        "round-stagger",
        "round-gap",
        "round-asymmetric",
        "round-percent",
        "round-inset",
        "offset",
        "inset",
        "collapse",
        "empty",
        "empty-between",
        "foreground",
        "nested",
        "opacity",
        "zero",
        "alpha",
        "small-child",
        "big-child",
        "fractional",
        "thin",
        "offset-small",
        "offset-small-negative",
        "clip",
        "block-parent",
        "block-nested",
        "round-large-asymmetric",
    ] {
        let mut b = B::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::Width, "24px")
            .set_style(2, P::Padding, "12px")
            .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
            .set_style(2, P::FontSize, "20px")
            .set_style(2, P::LineHeight, "24px")
            .set_style(2, P::Color, "transparent")
            .create_element(
                3,
                if name == "block-nested" {
                    Tag::Div
                } else {
                    Tag::Span
                },
            )
            .insert(2, 3, 0)
            .set_style(3, P::Outline, "2px solid #ff0000")
            .create_text(4, "A A A A")
            .insert(3, 4, 0);
        match name {
            "single" | "round-single" | "foreground" | "nested" => {
                b.set_style(2, P::Width, "80px").set_text(4, "A A");
            }
            "stagger" | "round-stagger" => {
                b.set_style(2, P::Width, "32px").set_text(4, "A A A A A");
            }
            "gap" | "round-gap" => {
                b.set_style(2, P::LineHeight, "40px");
            }
            "overlap" => {
                b.set_style(2, P::LineHeight, "12px");
            }
            "offset" => {
                b.set_style(3, P::OutlineOffset, "3px");
            }
            "inset" | "round-inset" => {
                b.set_style(3, P::OutlineOffset, "-3px");
            }
            "collapse" => {
                b.set_style(3, P::OutlineOffset, "-100px");
            }
            "empty" | "empty-between" => {
                b.set_text(4, "");
                if name == "empty-between" {
                    b.create_text(5, "A").insert(2, 5, 0);
                }
            }
            "opacity" | "zero" => {
                b.set_style(3, P::Opacity, if name == "zero" { "0" } else { "0.5" });
            }
            "alpha" => {
                b.set_style(3, P::OutlineColor, "#ff000080");
            }
            "small-child" | "big-child" => {
                b.create_element(5, Tag::Span)
                    .insert(3, 5, 0)
                    .insert(5, 4, 0)
                    .set_style(
                        5,
                        P::FontSize,
                        if name == "small-child" {
                            "10px"
                        } else {
                            "30px"
                        },
                    );
                if name == "small-child" {
                    b.set_text(4, "A A A A A A A A");
                }
            }
            "thin" => {
                b.set_style(3, P::OutlineWidth, "0.5px");
            }
            "offset-small" => {
                b.set_style(3, P::OutlineOffset, "0.25px");
            }
            "offset-small-negative" => {
                b.set_style(3, P::OutlineOffset, "-0.25px");
            }
            "clip" => {
                b.set_style(2, P::Padding, "0")
                    .set_style(2, P::Margin, "12px")
                    .set_style(2, P::Height, "36px")
                    .set_style(2, P::Overflow, "hidden")
                    .set_style(2, P::BorderRadius, "12px");
            }
            "block-parent" | "block-nested" => {
                b.set_style(2, P::Padding, "0")
                    .set_style(2, P::Margin, "12px")
                    .set_style(2, P::Height, "24px")
                    .set_style(2, P::Outline, "2px solid #ff0000")
                    .set_style(2, P::OutlineOffset, "-4px")
                    .set_style(3, P::Outline, "2px solid #0000ff")
                    .set_style(3, P::OutlineOffset, "-4px")
                    .set_text(4, "A A");
            }
            "fractional" => {
                b.set_style(2, P::FontSize, "24.3px")
                    .set_style(2, P::LineHeight, "28.25px")
                    .set_style(2, P::Width, "29.25px")
                    .set_style(2, P::Padding, "12.25px")
                    .set_style(3, P::OutlineWidth, "2.5px")
                    .set_style(3, P::OutlineOffset, "1.25px");
            }
            _ => {}
        }
        if name.starts_with("round-") {
            b.set_style(
                3,
                P::BorderRadius,
                match name {
                    "round-asymmetric" => "2px 10px 6px 3px",
                    "round-large-asymmetric" => "40px 10px 40px 5px",
                    "round-percent" => "50%",
                    _ => "10px",
                },
            );
        }
        if name == "foreground" {
            b.set_style(3, P::Color, "#0000ff")
                .set_style(3, P::OutlineOffset, "-4px");
        }
        if name == "nested" {
            b.set_style(3, P::OutlineOffset, "-4px")
                .create_element(5, Tag::Span)
                .insert(3, 5, 0)
                .insert(5, 4, 0)
                .set_style(5, P::Outline, "2px solid #0000ff")
                .set_style(5, P::OutlineOffset, "-4px");
        }
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Denied)?;
        let mut font = TextMeasurer::new(bytes.to_vec(), FAMILY)?;
        let boxes = v
            .layout_boxes_with_text(100.0, 140.0, &mut font, 20.0)
            .map_err(|e| format!("{name}: {e}"))?;
        let mut renderer = Renderer::new(Some(font), 20.0)?;
        let image = renderer
            .render(
                &v,
                (100.0 * scale).round() as u32,
                (140.0 * scale).round() as u32,
                scale,
            )
            .map_err(|e| format!("{name}: {e}"))?;
        let mut geometry = serde_json::Map::new();
        for id in [2, 3, 5] {
            if let Some(r) = boxes.get(&id) {
                geometry.insert(
                    id.to_string(),
                    serde_json::json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}),
                );
            }
        }
        let rect = boxes[&3];
        let roi = serde_json::json!({"x":(rect.x-7.0).max(0.0)*scale,"y":(rect.y-7.0).max(0.0)*scale,"width":(rect.width+14.0)*scale,"height":(rect.height+14.0)*scale});
        let mut samples = Vec::new();
        if scale == 1.0 && name == "collapse" {
            for (x, y) in [(22, 22), (25, 25), (22, 46), (25, 49)] {
                let at = ((y * image.width + x) * 4) as usize;
                samples.push(serde_json::json!({"x":x,"y":y,"rgba":&image.pixels[at..at+4]}));
            }
        }
        cases.push(serde_json::json!({"name":name,"tree":v.snapshot()["tree"],"boxes":geometry,"hits":[],"samples":samples,"raster":image.pixels,"raster_size":[image.width,image.height],"roi":roi}));
    }
    use std::io::Write;
    std::io::stdout().lock().write_all(serde_json::to_string(&serde_json::json!({"family":FAMILY,"font":bytes.as_slice(),"scale":scale,"viewport":{"width":100,"height":140},"cases":cases}))?.as_bytes())?;
    Ok(())
}
