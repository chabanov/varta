//! Independent Chrome comparison fixtures for underline propagation and ink.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, GlyphRasterizer, Renderer, TextMeasurer, View};
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
const FAMILY: &str = "Varta Decoration Fixture";
const FONT: &[u8] = include_bytes!("../../tests/fixtures/decoration.ttf");
pub type RasterFactory = fn() -> Box<dyn GlyphRasterizer>;
pub fn run(factory: Option<RasterFactory>) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let scale = args
        .next()
        .map(|s| s.parse::<f32>())
        .transpose()?
        .unwrap_or(1.0);
    if args.next().is_some() || !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("expected scale within0.25..8".into());
    }
    let mut cases = Vec::new();
    for name in [
        "plain",
        "descender",
        "curve",
        "cjk",
        "bidi",
        "fractional-descender",
        "overlap-lines",
        "child-color",
        "child-background",
        "child-none",
        "small-child",
        "nested",
        "opacity",
        "zero-opacity",
        "wrap",
        "pre-wrap",
        "overhang",
        "overhang-ink",
        "fractional-seams",
        "size-10",
        "size-14",
        "size-16",
        "size-24.3",
        "size-40",
        "clipped",
        "block-child",
    ] {
        let mut b = B::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::Width, "80px")
            .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
            .set_style(2, P::FontSize, "20px")
            .set_style(2, P::LineHeight, "24px")
            .set_style(2, P::Color, "#ff0000")
            .set_style(2, P::TextDecoration, "underline");
        if name == "plain" || name.starts_with("size-") {
            b.create_text(4, "A A").insert(2, 4, 0);
        } else {
            b.create_element(
                3,
                if name == "block-child" {
                    Tag::Div
                } else {
                    Tag::Span
                },
            )
            .insert(2, 3, 0)
            .create_text(4, if name == "descender" { "A g A" } else { "A A" })
            .insert(3, 4, 0);
        }
        match name {
            "overlap-lines" => {
                b.set_style(2, P::Width, "8px")
                    .set_style(2, P::PaddingTop, "24px")
                    .set_style(2, P::LineHeight, "0.01px")
                    .set_style(3, P::Color, "#0000ff")
                    .set_text(4, "g A");
            }
            "curve" => {
                b.set_text(4, "A j A");
            }
            "cjk" => {
                b.set_text(4, "A 一 A");
            }
            "bidi" => {
                b.set_text(4, "A אבג A");
            }
            "fractional-descender" => {
                b.set_style(2, P::FontSize, "24.3px").set_text(4, "A g A");
            }
            "child-color" => {
                b.set_style(3, P::Color, "#0000ff");
            }
            "child-background" => {
                b.set_style(3, P::BackgroundColor, "#00ff00");
            }
            "child-none" => {
                b.set_style(3, P::TextDecoration, "none");
            }
            "small-child" => {
                b.set_style(3, P::FontSize, "10px");
            }
            "nested" => {
                b.set_style(3, P::FontSize, "30px")
                    .set_style(3, P::Color, "#0000ff")
                    .set_style(3, P::TextDecoration, "underline");
            }
            "opacity" | "zero-opacity" => {
                b.set_style(3, P::BackgroundColor, "#00ff00").set_style(
                    3,
                    P::Opacity,
                    if name == "opacity" { "0.5" } else { "0" },
                );
            }
            "wrap" => {
                b.set_style(2, P::Width, "24px").set_text(4, "A g A g A");
            }
            "pre-wrap" => {
                b.set_style(2, P::Width, "24px")
                    .set_style(2, P::WhiteSpace, "pre-wrap")
                    .set_text(4, "A g A\nA A");
            }
            "overhang-ink" => {
                b.set_text(4, "k")
                    .set_style(3, P::Color, "#0000ff")
                    .create_element(5, Tag::Span)
                    .insert(2, 5, 0)
                    .create_text(6, "A")
                    .insert(5, 6, 0);
            }
            "overhang" => {
                b.set_text(4, "h")
                    .set_style(3, P::Color, "#0000ff")
                    .create_element(5, Tag::Span)
                    .insert(2, 5, 0)
                    .set_style(5, P::BackgroundColor, "#ff0000")
                    .create_text(6, "A")
                    .insert(5, 6, 0);
            }
            "fractional-seams" => {
                b.set_style(2, P::FontSize, "24.3px")
                    .set_text(4, "A")
                    .create_element(5, Tag::Span)
                    .insert(2, 5, 0)
                    .create_text(6, " A")
                    .insert(5, 6, 0);
            }
            "clipped" => {
                b.set_style(2, P::Width, "24px")
                    .set_style(2, P::Height, "21px")
                    .set_style(2, P::Overflow, "hidden")
                    .set_style(2, P::BorderRadius, "10px")
                    .set_text(4, "A g A g");
            }
            _ => {}
        }
        if let Some(size) = name.strip_prefix("size-") {
            b.set_style(2, P::FontSize, &format!("{size}px"));
        }
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Denied)?;
        let mut font = TextMeasurer::new(FONT.to_vec(), FAMILY)?;
        let boxes = v.layout_boxes_with_text(100.0, 100.0, &mut font, 20.0)?;
        let snapshot = v.snapshot();
        let mut renderer = Renderer::new(Some(font), 20.0)?;
        renderer.set_glyph_rasterizer(factory.map(|f| f()));
        let side = (100.0 * scale).round() as u32;
        let image = renderer
            .render(&v, side, side, scale)
            .map_err(|e| format!("{name}: {e}"))?;
        let mut geometry = serde_json::Map::new();
        for id in [2, 3, 5] {
            if let Some(rect) = boxes.get(&id) {
                geometry.insert(id.to_string(),serde_json::json!({"x":rect.x,"y":rect.y,"width":rect.width,"height":rect.height}));
            }
        }
        let ink_width = if let Some(size) = name.strip_prefix("size-") {
            size.parse::<f32>()? * 1.2
        } else {
            match name {
                "descender" | "curve" | "cjk" => 40.0,
                "bidi" => 56.0,
                "fractional-descender" => 48.6,
                "nested" => 36.0,
                "small-child" => 12.0,
                "overhang" | "overhang-ink" => 16.0,
                "fractional-seams" => 29.16,
                _ => 24.0,
            }
        };
        let roi_width = if name == "block-child" {
            80.0
        } else {
            ink_width + 2.0
        };
        let roi_height =
            boxes[&2].height.max(24.0) + if name == "overlap-lines" { 24.0 } else { 4.0 };
        cases.push(serde_json::json!({"name":name,"tree":snapshot["tree"],"boxes":geometry,"hits":[],"samples":[],"raster":image.pixels,"raster_size":[side,side],"roi":{"x":0,"y":0,"width":roi_width*scale,"height":roi_height*scale},"raster_stats":{"native":renderer.stats().raster_glyphs,"outline":renderer.stats().outline_glyphs}}));
    }
    use std::io::Write;
    std::io::stdout().lock().write_all(
        serde_json::to_string(
            &serde_json::json!({"family":FAMILY,"font":FONT,"scale":scale,"cases":cases}),
        )?
        .as_bytes(),
    )?;
    Ok(())
}
