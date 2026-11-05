//! Native structure (headings, lists, pre, label), alignment and ellipsis
//! geometry, hits and pixels for `scripts/test-native-structure.mjs`.
//! Usage: native_structure_probe [SCALE]
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, TextFont, TextMeasurer, View};

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

const FAMILY: &str = "Varta Structure Fixture";
const REGULAR: &[u8] = include_bytes!("../tests/fixtures/structure-400.ttf");
const BOLD: &[u8] = include_bytes!("../tests/fixtures/structure-700.ttf");
const VIEWPORT: (u32, u32) = (200, 160);
/// Default text size handed to the measurer; every case sets its own.
const HOST_FONT_SIZE: f32 = 16.0;
/// Pointer samples every this many CSS pixels.
const HIT_STEP: usize = 3;
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.0;

fn fonts() -> Result<TextMeasurer, Box<dyn std::error::Error>> {
    Ok(TextMeasurer::new_with_fonts(
        [REGULAR, BOLD]
            .into_iter()
            .map(|data| TextFont {
                data: data.to_vec(),
                family: FAMILY.into(),
                aliases: Vec::new(),
            })
            .collect(),
    )?)
}

/// A root block with the fixture font, black text and the case's width.
fn base(b: &mut B, width: &str) {
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    for (property, value) in [
        (P::Width, width),
        (P::FontFamily, "\"Varta Structure Fixture\""),
        (P::FontSize, "10px"),
        (P::Color, "#000000"),
    ] {
        b.set_style(2, property, value);
    }
}

/// A root block with the fixture font and no font-size, so sizes start from the
/// initial `medium` keyword (16px proportional, 13px fixed in Chrome).
fn medium_base(b: &mut B) {
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    b.set_style(2, P::Width, "200px")
        .set_style(2, P::FontFamily, "\"Varta Structure Fixture\"")
        .set_style(2, P::Color, "#000000");
}

/// Chrome draws generic `monospace` with a system face no host can supply, so a
/// monospace element `id` is a flex container (no strut of its own) whose
/// `line-height` records its computed size; its only item, `id + 1`, switches
/// back to the fixture font and draws `text` (node `id + 2`).
fn monospace_probe(b: &mut B, id: u32, line_height: &str, text: &str) {
    b.set_style(id, P::Display, "flex")
        .set_style(id, P::LineHeight, line_height);
    text_element(b, id, id + 1, Tag::Span, text);
    b.set_style(id + 1, P::FontFamily, "\"Varta Structure Fixture\"");
}

/// Element `id` with `tag` under `parent`, holding text node `id + 1`.
fn text_element(b: &mut B, parent: u32, id: u32, tag: Tag, text: &str) {
    b.create_element(id, tag)
        .insert(parent, id, 0)
        .create_text(id + 1, text)
        .insert(id, id + 1, 0);
}

fn case(name: &str) -> B {
    let mut b = B::new();
    match name {
        "headings" => {
            // 12.5px keeps every user-agent em length a whole 1/64 pixel.
            base(&mut b, "200px");
            b.set_style(2, P::FontSize, "12.5px")
                .set_style(2, P::LineHeight, "16px");
            text_element(&mut b, 2, 3, Tag::H1, "AB");
            text_element(&mut b, 2, 5, Tag::H2, "A BA");
            text_element(&mut b, 2, 7, Tag::H3, "BA");
            text_element(&mut b, 2, 9, Tag::P, "A");
        }
        "heading-overrides" => {
            base(&mut b, "120px");
            text_element(&mut b, 2, 3, Tag::H2, "A B");
            b.set_style(3, P::FontWeight, "400")
                .set_style(3, P::Margin, "3px 0 5px")
                .set_style(3, P::Color, "#0000ff");
            text_element(&mut b, 2, 5, Tag::H3, "AB AB");
            b.set_style(5, P::FontSize, "12px")
                .set_style(5, P::LineHeight, "20px");
        }
        "lists" => {
            base(&mut b, "200px");
            b.create_element(3, Tag::Ul).insert(2, 3, 0);
            text_element(&mut b, 3, 4, Tag::Li, "A");
            text_element(&mut b, 3, 6, Tag::Li, "B A");
            b.create_element(8, Tag::Ol).insert(2, 8, 0);
            text_element(&mut b, 8, 9, Tag::Li, "A");
            text_element(&mut b, 8, 11, Tag::Li, "AB");
            b.set_style(11, P::Color, "#ff0000");
        }
        "nested-lists" => {
            base(&mut b, "200px");
            b.set_style(2, P::LineHeight, "14px");
            b.create_element(3, Tag::Ul).insert(2, 3, 0);
            text_element(&mut b, 3, 4, Tag::Li, "A");
            b.create_element(6, Tag::Li).insert(3, 6, 0);
            b.create_element(7, Tag::Ul).insert(6, 7, 0);
            text_element(&mut b, 7, 8, Tag::Li, "B");
            b.create_element(10, Tag::Li).insert(7, 10, 0);
            b.create_element(11, Tag::Ul).insert(10, 11, 0);
            text_element(&mut b, 11, 12, Tag::Li, "AB");
            b.create_element(14, Tag::Ol).insert(2, 14, 0);
            b.create_element(15, Tag::Li).insert(14, 15, 0);
            b.create_element(16, Tag::Ol).insert(15, 16, 0);
            text_element(&mut b, 16, 17, Tag::Li, "A");
            text_element(&mut b, 16, 19, Tag::Li, "B");
            b.create_element(21, Tag::Ul).insert(16, 21, 0);
            text_element(&mut b, 21, 22, Tag::Li, "A");
            b.set_style(14, P::FontSize, "20px");
        }
        "list-spans" => {
            base(&mut b, "120px");
            b.create_element(3, Tag::Ol).insert(2, 3, 0);
            b.set_style(3, P::PaddingLeft, "30px")
                .set_style(3, P::Margin, "4px 0");
            b.create_element(4, Tag::Li).insert(3, 4, 0);
            b.create_text(5, "A ").insert(4, 5, 0);
            text_element(&mut b, 4, 6, Tag::Strong, "AB");
            b.create_text(8, " B A A B").insert(4, 8, 0);
            text_element(&mut b, 3, 9, Tag::Li, "B");
            b.set_style(9, P::FontSize, "20px");
        }
        "pre" => {
            base(&mut b, "200px");
            text_element(&mut b, 2, 3, Tag::Pre, "A\n  AB\nB A");
            b.set_style(3, P::FontFamily, "\"Varta Structure Fixture\"");
            text_element(&mut b, 2, 5, Tag::Pre, "AAAA AAAA AAAA AAAA AAAA AAAA");
            b.set_style(5, P::FontFamily, "\"Varta Structure Fixture\"")
                .set_style(5, P::Width, "60px")
                .set_style(5, P::Overflow, "hidden");
        }
        "label" => {
            base(&mut b, "200px");
            b.create_text(3, "A ").insert(2, 3, 0);
            text_element(&mut b, 2, 4, Tag::Label, "BA");
            b.set_style(4, P::Color, "#0000ff");
            b.create_text(6, " A").insert(2, 6, 0);
            b.create_element(7, Tag::Label).insert(1, 7, 0);
            b.set_style(7, P::Display, "block")
                .set_style(7, P::FontFamily, "\"Varta Structure Fixture\"")
                .set_style(7, P::FontSize, "20px")
                .set_style(7, P::Width, "60px")
                .create_text(8, "AB")
                .insert(7, 8, 0);
        }
        "align" => {
            base(&mut b, "200px");
            b.set_style(2, P::LineHeight, "12px");
            text_element(&mut b, 2, 3, Tag::P, "A AB A AA B A");
            b.set_style(3, P::TextAlign, "center")
                .set_style(3, P::Width, "50px")
                .set_style(3, P::Margin, "0");
            text_element(&mut b, 2, 5, Tag::P, "AB A");
            b.set_style(5, P::TextAlign, "right")
                .set_style(5, P::Margin, "2px 0");
            b.create_element(7, Tag::Div).insert(2, 7, 0);
            b.set_style(7, P::TextAlign, "right")
                .set_style(7, P::Width, "70px");
            b.create_text(8, "A ").insert(7, 8, 0);
            text_element(&mut b, 7, 9, Tag::Span, "AB B");
            b.set_style(9, P::Padding, "0 3px")
                .set_style(9, P::Border, "1px solid #ff0000")
                .set_style(9, P::BackgroundColor, "#c0c0ff");
            b.create_text(11, " A A").insert(7, 11, 0);
            text_element(&mut b, 2, 12, Tag::Div, "A \u{05d0}\u{05d1} B");
            b.set_style(12, P::TextAlign, "center")
                .set_style(12, P::Width, "80px");
            text_element(
                &mut b,
                2,
                14,
                Tag::Div,
                "A \u{05d0}\u{05d1}\u{05d0} AB \u{05d1} A",
            );
            b.set_style(14, P::TextAlign, "right")
                .set_style(14, P::Width, "40px");
            text_element(&mut b, 2, 16, Tag::H3, "A B");
            b.set_style(16, P::TextAlign, "center")
                .set_style(16, P::Margin, "0");
        }
        "ellipsis" => {
            base(&mut b, "200px");
            b.create_element(3, Tag::Div).insert(2, 3, 0);
            for (property, value) in [
                (P::Width, "70px"),
                (P::WhiteSpace, "nowrap"),
                (P::Overflow, "hidden"),
                (P::TextOverflow, "ellipsis"),
                (P::FontSize, "20px"),
            ] {
                b.set_style(3, property, value);
            }
            b.create_text(4, "A ").insert(3, 4, 0);
            text_element(&mut b, 3, 5, Tag::Span, "AB AAA");
            text_element(&mut b, 3, 7, Tag::Span, "A");
            b.set_style(7, P::Color, "#ff0000");
            text_element(&mut b, 2, 9, Tag::Div, "AAAA BBBB AAAA");
            for (property, value) in [
                (P::Width, "45px"),
                (P::WhiteSpace, "nowrap"),
                (P::Overflow, "hidden"),
                (P::TextOverflow, "ellipsis"),
                (P::Color, "#0000ff"),
            ] {
                b.set_style(9, property, value);
            }
            text_element(&mut b, 2, 11, Tag::Div, "A B");
            for (property, value) in [
                (P::Width, "45px"),
                (P::WhiteSpace, "nowrap"),
                (P::Overflow, "hidden"),
                (P::TextOverflow, "ellipsis"),
            ] {
                b.set_style(11, property, value);
            }
            b.create_element(13, Tag::Div).insert(2, 13, 0);
            for (property, value) in [
                (P::Display, "flex"),
                (P::Width, "60px"),
                (P::FontSize, "12px"),
            ] {
                b.set_style(13, property, value);
            }
            text_element(&mut b, 13, 14, Tag::Span, "B");
            b.set_style(14, P::Width, "12px")
                .set_style(14, P::TextAlign, "center");
            b.create_element(16, Tag::Span).insert(13, 16, 0);
            for (property, value) in [
                (P::FlexGrow, "1"),
                (P::MinWidth, "0"),
                (P::WhiteSpace, "nowrap"),
                (P::Overflow, "hidden"),
                (P::TextOverflow, "ellipsis"),
            ] {
                b.set_style(16, property, value);
            }
            text_element(&mut b, 16, 17, Tag::Span, "AA BB AA BB");
            b.set_style(17, P::FontWeight, "700");
        }
        // Chrome 156: h1 > code is 26px (2em, then 13/16 at the switch) and its
        // fixture-font child returns to 32px.
        "mono-h1-code" => {
            medium_base(&mut b);
            b.create_element(3, Tag::H1).insert(2, 3, 0);
            b.create_element(4, Tag::Code).insert(3, 4, 0);
            monospace_probe(&mut b, 4, "100%", "AB");
        }
        // h3 > pre: 18.72px becomes 15.21px, also the pre's 1em margins.
        "mono-h3-pre" => {
            medium_base(&mut b);
            b.create_element(3, Tag::H3).insert(2, 3, 0);
            b.create_element(4, Tag::Pre).insert(3, 4, 0);
            monospace_probe(&mut b, 4, "100%", "AB");
        }
        // p { font-size: 150% } > code: 24px becomes 19.5px.
        "mono-percent-code" => {
            medium_base(&mut b);
            b.create_element(3, Tag::P).insert(2, 3, 0);
            b.set_style(3, P::FontSize, "150%");
            b.create_element(4, Tag::Code).insert(3, 4, 0);
            monospace_probe(&mut b, 4, "100%", "AB");
        }
        // pre (13px) > span { font-size: 200% } stays monospace at 26px.
        "mono-pre-span" => {
            medium_base(&mut b);
            b.create_element(3, Tag::Pre).insert(2, 3, 0);
            b.set_style(3, P::Display, "flex");
            b.create_element(4, Tag::Span).insert(3, 4, 0);
            b.set_style(4, P::FontSize, "200%");
            monospace_probe(&mut b, 4, "100%", "AB");
        }
        // h2 { font-family: monospace }: 24px becomes 19.5px, margins 0.83em of it.
        "mono-h2-family" => {
            medium_base(&mut b);
            b.create_element(3, Tag::H2).insert(2, 3, 0);
            b.set_style(3, P::FontFamily, "monospace");
            monospace_probe(&mut b, 3, "100%", "AB");
        }
        // pre { line-height: 150% } resolves against 13px: 19.5px lines.
        "mono-pre-line-height" => {
            medium_base(&mut b);
            b.create_element(3, Tag::Pre).insert(2, 3, 0);
            monospace_probe(&mut b, 3, "150%", "AB");
        }
        _ => unreachable!("listed case"),
    }
    b
}

const CASES: &[&str] = &[
    "headings",
    "heading-overrides",
    "lists",
    "nested-lists",
    "list-spans",
    "pre",
    "label",
    "align",
    "ellipsis",
    "mono-h1-code",
    "mono-h3-pre",
    "mono-percent-code",
    "mono-pre-span",
    "mono-h2-family",
    "mono-pre-line-height",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let scale = args
        .next()
        .map(|s| s.parse::<f32>())
        .transpose()?
        .unwrap_or(1.0);
    if !scale.is_finite() || !(MIN_SCALE..=MAX_SCALE).contains(&scale) {
        return Err("scale must be within 0.25..=8".into());
    }
    if args.next().is_some() {
        return Err("unexpected probe argument".into());
    }
    let mut font = fonts()?;
    let (width, height) = VIEWPORT;
    let mut cases = Vec::new();
    for &name in CASES {
        let mut view = View::default();
        view.commit(&case(name).finish(), false, &mut Denied)?;
        let geometry =
            view.layout_boxes_with_text(width as f32, height as f32, &mut font, HOST_FONT_SIZE)?;
        let side = |value: u32| (value as f32 * scale).round() as u32;
        let image = view.rasterize_scaled(
            side(width),
            side(height),
            scale,
            Some((&mut font, HOST_FONT_SIZE)),
        )?;
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
        let target = |x: f32, y: f32| image.hit_test(x * scale, y * scale);
        let hits = (0..height as usize)
            .step_by(HIT_STEP)
            .flat_map(|y| (0..width as usize).step_by(HIT_STEP).map(move |x| (x, y)))
            .map(|(x, y)| {
                let (px, py) = (x as f32 + 0.25, y as f32 + 0.25);
                let id = target(px, py);
                let boundary = [-1.0, 1.0].into_iter().any(|dx| {
                    [-1.0, 1.0]
                        .into_iter()
                        .any(|dy| target(px + dx, py + dy) != id)
                });
                serde_json::json!({"x":x,"y":y,"id":id,"boundary":boundary})
            })
            .collect::<Vec<_>>();
        cases.push(serde_json::json!({
            "name": name,
            "tree": view.snapshot()["tree"],
            "boxes": boxes,
            "hits": hits,
            "samples": [],
            "raster": image.pixels.as_slice(),
            "raster_size": [image.width, image.height],
            "roi": {"x":0,"y":0,"width":image.width,"height":image.height},
        }));
    }
    let output = serde_json::to_string(&serde_json::json!({
        "family": FAMILY,
        "fonts": [
            {"family": FAMILY, "weight": "400", "data": REGULAR},
            {"family": FAMILY, "weight": "700", "data": BOLD},
        ],
        "ua_version": varta_view::NATIVE_UA_VERSION,
        "scale": scale,
        "viewport": {"width": width, "height": height},
        "text_rasterizer": "portable",
        "cases": cases,
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}
