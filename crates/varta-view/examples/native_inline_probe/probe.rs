//! Real native inline geometry/paint data for comparison with Chrome.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter as B, Broker, GlyphRasterizer, RasterImage, Renderer, TextMeasurer, View,
};
#[path = "paint_order.rs"]
mod paint_order;
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
pub type RasterFactory = fn() -> Box<dyn GlyphRasterizer>;
pub fn run(factory: Option<RasterFactory>) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let scale = args
        .next()
        .map(|s| s.parse::<f32>())
        .transpose()?
        .unwrap_or(1.0);
    if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("scale must be within 0.25..=8".into());
    }
    let decorations_only = match args.next().as_deref() {
        None => false,
        Some("decorations") => true,
        Some(_) => return Err("expected decorations filter".into()),
    };
    if args.next().is_some() {
        return Err("unexpected probe argument".into());
    }
    let family = "Varta Inline Fixture";
    let bytes = include_bytes!("../../tests/fixtures/inline.ttf");
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
        "rounded-wrap",
        "rounded-percent",
        "rounded-rem",
        "rounded-asymmetric",
        "rounded-bidi",
        "rounded-bidi-split",
        "rounded-small-child",
        "rounded-nested",
        "opacity-wrap",
        "opacity-ligature-first",
        "opacity-ligature-second",
        "opacity-zero",
        "opacity-nested",
        "opacity-overlay",
        "opacity-parent-isolated",
        "opacity-curved",
        "opacity-curved-integer",
        "opacity-curved-bg",
        "opacity-bidi",
        "opacity-overlap-lines",
        "paint-overlap-lines",
        "paint-overlap-hits",
    ] {
        if decorations_only
            && !name.starts_with("rounded-")
            && !name.starts_with("opacity-")
            && !name.starts_with("paint-")
        {
            continue;
        }
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
            "opacity-ligature-first" => ("", "A", "A"),
            "opacity-ligature-second" => ("A", "A", ""),
            "opacity-bidi" => ("", "A ا A", ""),
            name if name.starts_with("opacity-") || name.starts_with("paint-") => {
                ("", "A A A A", "")
            }
            "rounded-bidi" => ("A ", "لAا", " A"),
            "rounded-bidi-split" => ("", "A ا", "ا A"),
            name if name.starts_with("rounded-") => ("", "A A A A", ""),
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
        let rounded = name.starts_with("rounded-");
        if rounded {
            b.set_style(2, P::Color, "transparent")
                .set_style(2, P::Width, "24px")
                .set_style(4, P::Color, "transparent")
                .set_style(4, P::BackgroundColor, "#ff0000")
                .set_style(
                    4,
                    P::BorderRadius,
                    match name {
                        "rounded-percent" | "rounded-bidi-split" => "50%",
                        "rounded-rem" => "0.5rem",
                        "rounded-asymmetric" => "4px 12px 2px 8px",
                        _ => "10px",
                    },
                );
            if matches!(name, "rounded-bidi" | "rounded-bidi-split") {
                b.set_style(2, P::Width, "100px");
            }
            if matches!(name, "rounded-small-child" | "rounded-nested") {
                b.set_style(2, P::Width, "100px")
                    .set_style(2, P::LineHeight, "40px")
                    .set_style(4, P::FontSize, "40px")
                    .set_style(4, P::BorderRadius, "50%")
                    .create_element(7, Tag::Span)
                    .insert(4, 7, 0)
                    .insert(7, 5, 0)
                    .set_text(5, "A")
                    .set_style(7, P::FontSize, "20px");
                if name == "rounded-nested" {
                    b.set_style(7, P::BorderRadius, "40%").set_style(
                        7,
                        P::BackgroundColor,
                        "#0000ff80",
                    );
                }
            }
        }
        let opacity = name.starts_with("opacity-") || name.starts_with("paint-");
        if opacity {
            b.set_style(2, P::Width, "24px")
                .set_style(2, P::Color, "#000000")
                .set_style(4, P::Color, "#000000")
                .set_style(
                    4,
                    P::Opacity,
                    if name == "opacity-zero" {
                        "0"
                    } else if name.starts_with("paint-") {
                        "1"
                    } else {
                        "0.5"
                    },
                )
                .set_style(4, P::BackgroundColor, "#ff0000");
            if name.contains("ligature") {
                b.set_style(2, P::Width, "100px")
                    .set_style(4, P::BackgroundColor, "transparent");
            }
            if name == "opacity-nested" {
                b.create_element(7, Tag::Span)
                    .insert(4, 7, 0)
                    .insert(7, 5, 0)
                    .set_style(7, P::Opacity, "0.5")
                    .set_style(7, P::BackgroundColor, "#0000ff");
            }
            if matches!(name, "opacity-overlay" | "opacity-parent-isolated") {
                b.create_element(8, Tag::Div)
                    .insert(1, 8, 0)
                    .set_style(8, P::Width, "24px")
                    .set_style(8, P::Height, "40px")
                    .set_style(8, P::MarginTop, "-40px")
                    .set_style(8, P::BackgroundColor, "#0000ff");
                if name == "opacity-parent-isolated" {
                    b.set_style(2, P::Opacity, "0.5")
                        .set_style(8, P::Opacity, "0.5");
                }
            }
            if name.starts_with("opacity-curved") {
                b.set_style(2, P::Height, "40px")
                    .set_style(
                        2,
                        P::Margin,
                        if name == "opacity-curved-integer" {
                            "3px"
                        } else {
                            "3.25px"
                        },
                    )
                    .set_style(2, P::BorderRadius, "50%")
                    .set_style(2, P::Overflow, "hidden")
                    .set_style(4, P::BorderRadius, "50%");
            }
            if name == "opacity-curved-bg" {
                b.set_style(4, P::Color, "transparent");
            }
            if name == "opacity-bidi" {
                b.set_style(2, P::Width, "100px");
            }
            if name == "paint-overlap-hits" {
                b.set_style(4, P::BackgroundColor, "transparent")
                    .set_text(5, "A A A")
                    .set_text(6, " A A A");
            }
            if name.ends_with("overlap-lines") || name == "paint-overlap-hits" {
                b.set_style(2, P::LineHeight, "10px");
            }
        }
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Denied)?;
        let snapshot = v.snapshot();
        let geometry = v.layout_boxes_with_text(100.0, 100.0, &mut font, 20.0)?;
        let side = (100.0 * scale).round() as u32;
        let (image, raster_stats) = rasterize(&v, side, scale, &mut font, factory)?;
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
        let hits = (0..70).step_by(5).flat_map(|y|(0..50).step_by(3).map(move|x|(x,y))).map(|(x,y)|serde_json::json!({"x":x,"y":y,"id":image.hit_test((x as f32+0.25)*scale,(y as f32+0.25)*scale)})).collect::<Vec<_>>();
        let mut case = serde_json::json!({"name":name,"raster_stats":raster_stats,"tree":snapshot["tree"],"boxes":boxes,"hits":hits,"samples":([[3,6],[10,6]].map(|[x,y]|{let x=(x as f32*scale).floor() as u32;let y=(y as f32*scale).floor() as u32;let offset=((y*side+x)*4) as usize;serde_json::json!({"x":x,"y":y,"rgba":&image.pixels[offset..offset+4]})}))});
        if rounded || opacity {
            case["samples"] = serde_json::json!([]);
            case["raster"] = serde_json::json!(image.pixels);
            case["raster_size"] = serde_json::json!([image.width, image.height]);
            let r = if opacity { geometry[&2] } else { geometry[&4] };
            // Include logical ink bounds and snapped decoration bounds at every
            // DPI, without losing a newly exposed partial device pixel.
            let left = r.x.min((r.x + 0.5).floor());
            let top = r.y.min((r.y + 0.5).floor());
            let right = (r.x + r.width).max((r.x + r.width + 0.5).floor());
            let bottom = (r.y + r.height).max((r.y + r.height + 0.5).floor());
            case["roi"] = serde_json::json!({"x":left*scale,"y":top*scale,"width":(right-left)*scale,"height":(bottom-top)*scale});
            for hit in case["hits"].as_array_mut().expect("hit list") {
                let x = hit["x"].as_f64().expect("x") as f32 + 0.25;
                let y = hit["y"].as_f64().expect("y") as f32 + 0.25;
                let target = image.hit_test(x * scale, y * scale);
                hit["boundary"] = serde_json::json!([-1.0, 1.0].into_iter().any(|dx| {
                    [-1.0, 1.0]
                        .into_iter()
                        .any(|dy| image.hit_test((x + dx) * scale, (y + dy) * scale) != target)
                }));
            }
        }
        cases.push(case);
    }
    if !decorations_only {
        cases.extend(paint_order::cases(&mut font, scale, factory)?);
    }
    let output = serde_json::to_string(
        &serde_json::json!({"family":family,"font":bytes.as_slice(),"scale":scale,"text_rasterizer":if factory.is_some() {"host"} else {"portable"},"cases":cases}),
    )?;
    use std::io::Write;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}

fn rasterize(
    v: &View,
    side: u32,
    scale: f32,
    font: &mut TextMeasurer,
    factory: Option<RasterFactory>,
) -> Result<(RasterImage, serde_json::Value), Box<dyn std::error::Error>> {
    if let Some(factory) = factory {
        let font = TextMeasurer::new(
            include_bytes!("../../tests/fixtures/inline.ttf").to_vec(),
            "Varta Inline Fixture",
        )?;
        let mut renderer = Renderer::new(Some(font), 20.0)?;
        renderer.set_glyph_rasterizer(Some(factory()));
        let image = renderer.render(v, side, side, scale)?;
        let stats = renderer.stats();
        Ok((
            image,
            serde_json::json!({"raster_glyphs":stats.raster_glyphs,"outline_glyphs":stats.outline_glyphs,"glyph_cache_hits":stats.glyph_cache_hits}),
        ))
    } else {
        Ok((
            v.rasterize_scaled(side, side, scale, Some((font, 20.0)))?,
            serde_json::Value::Null,
        ))
    }
}
