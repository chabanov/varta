//! Explicit-font DOM comparison for vertical non-replaced inline spacing.
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
        .map(|v| v.parse::<f32>())
        .transpose()?
        .unwrap_or(1.0);
    if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("invalid scale".into());
    }
    const FAMILY: &str = "Varta Decoration Fixture";
    let font = include_bytes!("../tests/fixtures/decoration.ttf");
    let mut cases = Vec::new();
    for name in [
        "padding",
        "asymmetric",
        "percentage",
        "rem",
        "margin",
        "margin-auto",
        "cascade",
        "round",
        "round-percent",
        "outline",
        "nested",
        "big-child",
        "opacity",
        "zero",
        "alpha",
        "gap",
        "overlap",
        "clip",
        "empty-between",
        "fractional",
        "underline",
    ] {
        let mut b = B::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::Width, "24px")
            .set_style(2, P::Padding, "20px")
            .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
            .set_style(2, P::FontSize, "20px")
            .set_style(2, P::LineHeight, "24px")
            .set_style(2, P::Color, "transparent")
            .create_element(3, Tag::Span)
            .insert(2, 3, 0)
            .set_style(3, P::Padding, "4px 0")
            .set_style(3, P::BackgroundColor, "#ff0000")
            .create_text(4, "A A A A A")
            .insert(3, 4, 0);
        match name {
            "asymmetric" => {
                b.set_style(3, P::Padding, "4px 0 8px");
            }
            "percentage" => {
                b.set_style(3, P::Padding, "25% 0 50%");
            }
            "rem" => {
                b.set_style(3, P::Padding, "0.5rem 0 0.25rem");
            }
            "margin" => {
                b.set_style(3, P::Margin, "100px 0 -20%");
            }
            "margin-auto" => {
                b.set_style(3, P::Margin, "auto");
            }
            "cascade" => {
                b.set_style(3, P::Padding, "4px 12px")
                    .set_style(3, P::PaddingLeft, "0")
                    .set_style(3, P::PaddingRight, "0");
            }
            "round" => {
                b.set_style(3, P::BorderRadius, "8px");
            }
            "round-percent" => {
                b.set_style(3, P::BorderRadius, "50%");
            }
            "outline" => {
                b.set_style(3, P::Outline, "2px solid #0000ff").set_style(
                    3,
                    P::BorderRadius,
                    "6px",
                );
            }
            "nested" | "big-child" => {
                b.create_element(5, Tag::Span)
                    .insert(3, 5, 0)
                    .insert(5, 4, 0)
                    .set_style(5, P::Padding, "8px 0 2px")
                    .set_style(5, P::BackgroundColor, "#0000ff");
                if name == "big-child" {
                    b.set_style(5, P::FontSize, "30px");
                }
            }
            "opacity" => {
                b.set_style(3, P::Opacity, "0.5");
            }
            "zero" => {
                b.set_style(3, P::Opacity, "0");
            }
            "alpha" => {
                b.set_style(3, P::BackgroundColor, "rgba(255, 0, 0, 0.5)");
            }
            "gap" => {
                b.set_style(2, P::LineHeight, "40px");
            }
            "overlap" => {
                b.set_style(2, P::LineHeight, "14px");
            }
            "clip" => {
                b.set_style(2, P::Height, "68px")
                    .set_style(2, P::Overflow, "hidden")
                    .set_style(2, P::BorderRadius, "14px");
            }
            "empty-between" => {
                b.set_text(4, "A")
                    .create_element(5, Tag::Span)
                    .insert(2, 5, 0)
                    .set_style(5, P::Padding, "8px 0")
                    .set_style(5, P::Outline, "2px solid #0000ff")
                    .create_text(6, " A")
                    .insert(2, 6, 0);
            }
            "fractional" => {
                b.set_style(3, P::Padding, "1.5px 0 2.75px")
                    .set_style(2, P::Margin, "0.25px");
            }
            "underline" => {
                b.set_style(3, P::TextDecoration, "underline")
                    .set_style(3, P::Color, "#0000ff");
            }
            _ => {}
        }
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Denied)
            .map_err(|e| format!("{name}: {e}"))?;
        let mut measurer = TextMeasurer::new(font.to_vec(), FAMILY)?;
        let boxes = v
            .layout_boxes_with_text(100.0, 180.0, &mut measurer, 20.0)
            .map_err(|e| format!("{name}: {e}"))?;
        let mut renderer = Renderer::new(Some(measurer), 20.0)?;
        let image = renderer
            .render(
                &v,
                (100.0 * scale).round() as u32,
                (180.0 * scale).round() as u32,
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
        let mut hits = Vec::new();
        for y in (10..120).step_by(3) {
            for x in (15..55).step_by(3) {
                let point = [x as f32 + 0.25, y as f32 + 0.25];
                let id = image.hit_test(point[0] * scale, point[1] * scale);
                // Like the verifier's existing rectangle-edge exclusion, omit
                // a one-CSS-pixel neighbourhood of curved hit edges. Chrome
                // queries a pixel footprint; the native API tests an exact point.
                // All pixels and boxes still use their unchanged strict checks.
                let boundary = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
                    .into_iter()
                    .any(|(dx, dy)| {
                        image.hit_test((point[0] + dx) * scale, (point[1] + dy) * scale) != id
                    });
                hits.push(serde_json::json!({"x":x,"y":y,"id":id,"boundary":boundary}));
            }
        }
        cases.push(serde_json::json!({"name":name,"tree":v.snapshot()["tree"],"boxes":geometry,"hits":hits,"samples":[],"raster":image.pixels,"raster_size":[image.width,image.height],"roi":{"x":12.0*scale,"y":0,"width":55.0*scale,"height":170.0*scale}}));
    }
    use std::io::Write;
    std::io::stdout().lock().write_all(serde_json::to_string(&serde_json::json!({"family":FAMILY,"font":font.as_slice(),"scale":scale,"viewport":{"width":100,"height":180},"cases":cases}))?.as_bytes())?;
    Ok(())
}
