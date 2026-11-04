//! Ordinary block, atomic item, and lifted zero-level context browser oracles.
use super::*;
pub(super) fn cases(
    font: &mut TextMeasurer,
    scale: f32,
    factory: Option<RasterFactory>,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let mut cases = Vec::new();
    for display in ["block", "flex", "grid"] {
        for nested in [false, true] {
            for opacity in ["plain", "1", "0.5", "0"] {
                let mut b = B::new();
                b.set_style(1, P::BackgroundColor, "#ffffff")
                    .create_element(10, Tag::Div)
                    .insert(1, 10, 0)
                    .set_style(10, P::Width, "80px")
                    .set_style(10, P::Display, display)
                    .set_style(10, P::FontFamily, "\"Varta Inline Fixture\"")
                    .set_style(10, P::FontSize, "20px")
                    .set_style(10, P::LineHeight, "20px")
                    .create_element(2, Tag::Div)
                    .insert(10, 2, 0)
                    .set_style(2, P::Width, "40px")
                    .set_style(2, P::Height, "20px")
                    .set_style(2, P::FlexShrink, "0")
                    .set_style(2, P::Color, "#000000")
                    .create_element(3, Tag::Div)
                    .insert(if nested { 1 } else { 10 }, 3, 0)
                    .set_style(3, P::Width, "40px")
                    .set_style(3, P::Height, "20px")
                    .set_style(3, P::FlexShrink, "0")
                    .set_style(3, P::BackgroundColor, "#0000ff");
                if opacity == "plain" {
                    b.create_text(4, "A").insert(2, 4, 0);
                } else {
                    b.create_element(5, Tag::Span)
                        .insert(2, 5, 0)
                        .set_style(5, P::Opacity, opacity)
                        .create_text(4, "A")
                        .insert(5, 4, 0);
                }
                if display == "grid" && !nested {
                    b.set_style(10, P::GridTemplateColumns, "40px").set_style(
                        10,
                        P::GridTemplateRows,
                        "20px",
                    );
                    for id in [2, 3] {
                        b.set_style(id, P::GridColumn, "1")
                            .set_style(id, P::GridRow, "1");
                    }
                } else {
                    let horizontal = display == "flex" && !nested;
                    b.set_style(
                        3,
                        if horizontal {
                            P::MarginLeft
                        } else {
                            P::MarginTop
                        },
                        if horizontal { "-40px" } else { "-20px" },
                    );
                }
                let mut v = View::default();
                v.commit(&b.finish(), false, &mut Denied)?;
                let geometry = v.layout_boxes_with_text(100.0, 100.0, font, 20.0)?;
                let side = (100.0 * scale).round() as u32;
                let (image, raster_stats) = rasterize(&v, side, scale, font, factory)?;
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
                let hits=(0..25).step_by(3).flat_map(|y|(0..44).step_by(3).map(move |x|(x,y))).map(|(x,y)|serde_json::json!({"x":x,"y":y,"id":image.hit_test((x as f32+0.25)*scale,(y as f32+0.25)*scale)})).collect::<Vec<_>>();
                cases.push(serde_json::json!({"name":format!("paint-{display}-nested{nested}-alpha{opacity}"),"raster_stats":raster_stats,"tree":v.snapshot()["tree"],"boxes":boxes,"hits":hits,"samples":[],"raster":image.pixels,"raster_size":[side,side],"roi":{"x":0,"y":0,"width":40.0*scale,"height":20.0*scale}}));
            }
        }
    }
    Ok(cases)
}
