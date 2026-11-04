//! Explicit-font DOM comparison for horizontal non-replaced inline spacing.
//! Output feeds `scripts/test-native-inline.mjs`.
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
const FAMILY: &str = "Varta Decoration Fixture";
const VIEWPORT: [f32; 2] = [140.0, 180.0];
const CASES: [&str; 39] = [
    "padding",
    "margin",
    "both",
    "asymmetric",
    "negative",
    "negative-collapse",
    "rem",
    "fractional",
    "round",
    "outline",
    "nested",
    "nested-end",
    "empty",
    "empty-margin",
    "space-end",
    "space-start",
    "unbreakable",
    "opacity",
    "underline",
    "vertical-and-horizontal",
    "wide",
    "auto",
    "border",
    "border-wide",
    "border-sides",
    "border-radius",
    "border-radius-wide",
    "border-spacing",
    "border-nested",
    "border-empty",
    "border-opacity",
    "border-alpha",
    "border-current",
    "border-rem",
    "border-top",
    "border-outline",
    "border-unbroken-radius",
    "overlap-gap",
    "overlap-round",
];
fn scene(name: &str) -> B {
    let mut b = B::new();
    // A<span>A A A</span>A: the outer words continue across both boundaries.
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "64px")
        .set_style(2, P::Padding, "20px")
        .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "24px")
        .set_style(2, P::Color, "transparent")
        .set_style(2, P::BackgroundColor, "#00ff00")
        .create_text(6, "A")
        .insert(2, 6, 0)
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::Padding, "0 6px")
        .set_style(3, P::BackgroundColor, "#ff0000")
        .create_text(4, "A A A")
        .insert(3, 4, 0)
        .create_text(7, "A")
        .insert(2, 7, 0);
    match name {
        "margin" => {
            b.set_style(3, P::Padding, "0")
                .set_style(3, P::Margin, "0 5px");
        }
        "both" => {
            b.set_style(3, P::Padding, "2px 7px")
                .set_style(3, P::Margin, "0 3px");
        }
        "asymmetric" => {
            b.set_style(3, P::Padding, "0 2px 0 9px")
                .set_style(3, P::MarginRight, "4px");
        }
        "negative" => {
            b.set_style(3, P::Margin, "0 -4px");
        }
        "negative-collapse" => {
            b.set_style(3, P::Margin, "0 -6px");
        }
        "rem" => {
            b.set_style(3, P::Padding, "0 0.5rem")
                .set_style(3, P::Margin, "0 0.25rem");
        }
        "fractional" => {
            b.set_style(3, P::Padding, "0 1.5px")
                .set_style(3, P::Margin, "0 0.25px 0 2.75px");
        }
        "round" => {
            b.set_style(3, P::BorderRadius, "8px");
        }
        "outline" => {
            b.set_style(3, P::Outline, "2px solid #0000ff")
                .set_style(3, P::BorderRadius, "6px");
        }
        "nested" | "nested-end" => {
            // `nested` starts span 3 with span 5; `nested-end` ends with it.
            let (before, text) = if name == "nested" {
                (4, " A A")
            } else {
                (0, "A A ")
            };
            b.create_element(5, Tag::Span)
                .insert(3, 5, before)
                .set_style(5, P::Padding, "0 4px")
                .set_style(5, P::Margin, "0 2px")
                .set_style(5, P::BackgroundColor, "#0000ff")
                .set_text(4, text)
                .create_text(8, "A")
                .insert(5, 8, 0);
        }
        "empty" | "empty-margin" => {
            b.set_text(4, "").set_text(6, "A ").set_text(7, " A");
            if name == "empty-margin" {
                b.set_style(3, P::Margin, "0 3px");
            }
        }
        "space-end" => {
            b.set_text(4, "A A ").set_text(7, "A");
        }
        "space-start" => {
            b.set_text(4, " A A").set_text(6, "A ");
        }
        "unbreakable" => {
            b.set_text(4, "AA").set_text(6, "AA").set_text(7, "AA");
        }
        "opacity" => {
            b.set_style(3, P::Opacity, "0.5");
        }
        "underline" => {
            b.set_style(3, P::TextDecoration, "underline")
                .set_style(3, P::Color, "#0000ff");
        }
        "vertical-and-horizontal" => {
            b.set_style(3, P::Padding, "4px 5px 8px 3px");
        }
        "wide" => {
            b.set_style(3, P::Padding, "0 30px");
        }
        "auto" => {
            b.set_style(3, P::Margin, "0 auto");
        }
        "border" => {
            b.set_style(3, P::Padding, "0")
                .set_style(3, P::Border, "1px solid #0000ff");
        }
        "border-wide" => {
            b.set_style(3, P::Padding, "0")
                .set_style(3, P::Border, "4px solid #0000ff");
        }
        "border-sides" => {
            b.set_style(3, P::Padding, "0")
                .set_style(3, P::BorderLeft, "5px solid #0000ff")
                .set_style(3, P::BorderRight, "2px solid #0000ff")
                .set_style(3, P::BorderTop, "3px solid #0000ff");
        }
        "border-radius" => {
            b.set_style(3, P::Border, "2px solid #0000ff")
                .set_style(3, P::BorderRadius, "8px");
        }
        "border-radius-wide" => {
            b.set_style(3, P::Border, "5px solid #0000ff").set_style(
                3,
                P::BorderRadius,
                "4px 12px",
            );
        }
        "border-spacing" => {
            b.set_style(3, P::Border, "3px solid #0000ff")
                .set_style(3, P::Padding, "2px 4px")
                .set_style(3, P::Margin, "0 -2px 0 5px");
        }
        "border-nested" => {
            b.create_element(5, Tag::Span)
                .insert(3, 5, 4)
                .set_style(5, P::Border, "2px solid #ffff00")
                .set_style(5, P::BackgroundColor, "#0000ff")
                .set_text(4, " A A")
                .create_text(8, "A")
                .insert(5, 8, 0)
                .set_style(3, P::Border, "1px solid #000000");
        }
        "border-empty" => {
            b.set_text(4, "")
                .set_text(6, "A ")
                .set_text(7, " A")
                .set_style(3, P::Border, "2px solid #0000ff");
        }
        "border-opacity" => {
            b.set_style(3, P::Border, "3px solid #0000ff")
                .set_style(3, P::Opacity, "0.5");
        }
        "border-alpha" => {
            b.set_style(3, P::Border, "3px solid rgba(0, 0, 255, 0.5)");
        }
        "border-current" => {
            b.set_style(3, P::Border, "2px solid")
                .set_style(3, P::Color, "#0000ff");
        }
        "border-rem" => {
            b.set_style(3, P::Border, "0.25rem solid #0000ff");
        }
        "border-top" => {
            b.set_style(3, P::BorderTop, "4px solid #0000ff").set_style(
                3,
                P::BorderBottom,
                "2px solid #0000ff",
            );
        }
        "border-outline" => {
            b.set_style(3, P::Border, "2px solid #0000ff")
                .set_style(3, P::Outline, "2px solid #ff00ff")
                .set_style(3, P::OutlineOffset, "1px");
        }
        // An earlier line's tall fragment overlaps the next line's margin gap.
        "overlap-gap" => {
            b.set_style(3, P::Padding, "9px 7px")
                .set_style(3, P::Margin, "0 6px");
        }
        "overlap-round" => {
            b.set_style(3, P::Padding, "9px 7px")
                .set_style(3, P::Border, "3px solid #0000ff")
                .set_style(3, P::BorderRadius, "14px");
        }
        "border-unbroken-radius" => {
            b.set_text(4, "A")
                .set_style(3, P::Border, "3px solid #0000ff")
                .set_style(3, P::BorderRadius, "50%");
        }
        _ => {}
    }
    b
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
    let font = include_bytes!("../tests/fixtures/decoration.ttf");
    let mut cases = Vec::new();
    for name in CASES {
        let mut v = View::default();
        v.commit(&scene(name).finish(), false, &mut Denied)
            .map_err(|e| format!("{name}: {e}"))?;
        let mut measurer = TextMeasurer::new(font.to_vec(), FAMILY)?;
        let boxes = v
            .layout_boxes_with_text(VIEWPORT[0], VIEWPORT[1], &mut measurer, 20.0)
            .map_err(|e| format!("{name}: {e}"))?;
        let mut renderer = Renderer::new(Some(measurer), 20.0)?;
        let image = renderer
            .render(
                &v,
                (VIEWPORT[0] * scale).round() as u32,
                (VIEWPORT[1] * scale).round() as u32,
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
        for y in (10..150).step_by(3) {
            for x in (10..130).step_by(3) {
                let point = [x as f32 + 0.25, y as f32 + 0.25];
                let id = image.hit_test(point[0] * scale, point[1] * scale);
                // Omit a one-CSS-pixel neighbourhood of hit edges, as the
                // verifier does: Chrome queries a pixel footprint.
                let boundary = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
                    .into_iter()
                    .any(|(dx, dy)| {
                        image.hit_test((point[0] + dx) * scale, (point[1] + dy) * scale) != id
                    });
                hits.push(serde_json::json!({"x":x,"y":y,"id":id,"boundary":boundary}));
            }
        }
        cases.push(serde_json::json!({"name":name,"tree":v.snapshot()["tree"],"boxes":geometry,"hits":hits,"samples":[],"raster":image.pixels,"raster_size":[image.width,image.height],"roi":{"x":0,"y":0,"width":VIEWPORT[0]*scale,"height":170.0*scale}}));
    }
    use std::io::Write;
    std::io::stdout().lock().write_all(serde_json::to_string(&serde_json::json!({"family":FAMILY,"font":font.as_slice(),"scale":scale,"viewport":{"width":VIEWPORT[0] as u32,"height":VIEWPORT[1] as u32},"cases":cases}))?.as_bytes())?;
    Ok(())
}
