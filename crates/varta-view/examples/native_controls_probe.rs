//! Native form-control defaults (user-agent box, control font, Chrome's
//! native theme, disabled colors, placeholder and default focus ring) and
//! `input type=number`, for `scripts/test-native-controls.mjs`.
//! Usage: native_controls_probe [SCALE]
use varta_common::view::{Attribute, Command, CssProperty as P, Tag};
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

const FAMILY: &str = "Varta Controls Fixture";
const FONT: &[u8] = include_bytes!("../tests/fixtures/controls.ttf");
const VIEWPORT: (u32, u32) = (360, 120);
/// Default text size handed to the measurer (Chrome's medium size).
const HOST_FONT_SIZE: f32 = 16.0;
/// Pointer samples every this many CSS pixels.
const HIT_STEP: usize = 3;
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.0;

/// A flex row (node 2) with the fixture font; its items are blockified, as
/// an atomic inline control in a line box is outside the native subset.
fn row(b: &mut B) {
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    for (property, value) in [
        (P::Display, "flex"),
        (P::FlexWrap, "wrap"),
        (P::AlignItems, "flex-start"),
        (P::Gap, "4px"),
        (P::Padding, "4px"),
        (P::FontFamily, "\"Varta Controls Fixture\""),
        (P::Color, "#000000"),
    ] {
        b.set_style(2, property, value);
    }
}

/// Control `id` of `tag` appended to node 2.
fn control(b: &mut B, id: u32, tag: Tag) {
    b.create_element(id, tag).insert(2, id, 0);
}

fn input(b: &mut B, id: u32, kind: &str) {
    control(b, id, Tag::Input);
    b.set_attribute(id, Attribute::Type, kind);
}

fn label(b: &mut B, id: u32, text: &str) {
    b.create_text(id + 1, text).insert(id, id + 1, 0);
}

/// Every themed control once: button, text, number, password, checkbox and
/// radio (unchecked and checked) and textarea.
fn themed_set(b: &mut B, disabled: bool) {
    control(b, 10, Tag::Button);
    label(b, 10, "AB");
    input(b, 12, "text");
    b.set_value(12, "12x", 0);
    input(b, 13, "number");
    b.set_value(13, "-1.5", 0);
    input(b, 14, "checkbox");
    input(b, 15, "checkbox");
    b.set_checked(15, 1, 0);
    input(b, 16, "radio");
    input(b, 17, "radio");
    b.set_checked(17, 1, 0);
    control(b, 18, Tag::Textarea);
    b.set_value(18, "AB\nx", 0);
    input(b, 19, "password");
    b.set_value(19, "AB", 0);
    if disabled {
        for id in [10, 12, 13, 14, 15, 16, 17, 18, 19] {
            b.set_attribute(id, Attribute::Disabled, "");
        }
    }
}

/// `(case, focused node)`.
fn case(name: &str) -> (B, Option<u32>) {
    let mut b = B::new();
    row(&mut b);
    let focus = match name {
        "themed" => {
            themed_set(&mut b, false);
            None
        }
        "disabled" => {
            themed_set(&mut b, true);
            None
        }
        "focus-button" | "focus-text" | "focus-number" | "focus-checkbox" | "focus-radio"
        | "focus-textarea" | "focus-styled" => {
            let (tag, kind) = match name {
                "focus-button" | "focus-styled" => (Tag::Button, None),
                "focus-text" => (Tag::Input, Some("text")),
                "focus-number" => (Tag::Input, Some("number")),
                "focus-checkbox" => (Tag::Input, Some("checkbox")),
                "focus-radio" => (Tag::Input, Some("radio")),
                _ => (Tag::Textarea, None),
            };
            control(&mut b, 10, tag);
            if let Some(kind) = kind {
                b.set_attribute(10, Attribute::Type, kind);
            }
            if tag == Tag::Button {
                label(&mut b, 10, "AB");
            }
            if name == "focus-styled" {
                b.set_style(10, P::Border, "1px solid #0000ff").set_style(
                    10,
                    P::BorderRadius,
                    "4px",
                );
            }
            if kind == Some("number") {
                b.set_value(10, "12", 0);
            }
            b.command(10, Command::Focus, 0.0);
            Some(10)
        }
        // Author-styled controls keep user-agent padding, font, color and
        // background where the author sets none.
        "styled" => {
            control(&mut b, 10, Tag::Button);
            label(&mut b, 10, "AB");
            b.set_style(10, P::Border, "1px solid #0000ff");
            control(&mut b, 12, Tag::Button);
            label(&mut b, 12, "A");
            b.set_style(12, P::Border, "2px solid #00aa00")
                .set_style(12, P::BackgroundColor, "#ffff00")
                .set_style(12, P::BorderRadius, "6px")
                .set_style(12, P::Padding, "4px 10px")
                .set_style(12, P::Color, "#cc0000");
            input(&mut b, 14, "number");
            b.set_value(14, "42", 0)
                .set_style(14, P::Border, "1px solid #888888")
                .set_style(14, P::BorderRadius, "3px")
                .set_style(14, P::FontSize, "20px");
            control(&mut b, 15, Tag::Textarea);
            b.set_value(15, "x", 0)
                .set_style(15, P::Border, "none")
                .set_style(15, P::BackgroundColor, "#eeeeff")
                .set_style(15, P::Width, "60px");
            control(&mut b, 16, Tag::Button);
            b.set_style(16, P::Border, "1px solid #000000")
                .set_attribute(16, Attribute::Disabled, "");
            label(&mut b, 16, "A");
            None
        }
        // The control font resets size, weight and line height; an author
        // font-size still applies. The row's own font does not reach them.
        "control-font" => {
            b.set_style(2, P::FontSize, "30px")
                .set_style(2, P::LineHeight, "50px")
                .set_style(2, P::Color, "#ff0000");
            control(&mut b, 10, Tag::Button);
            label(&mut b, 10, "AB");
            control(&mut b, 12, Tag::Button);
            label(&mut b, 12, "AB");
            b.set_style(12, P::FontSize, "20px");
            input(&mut b, 14, "text");
            b.set_value(14, "x1", 0).set_style(14, P::FontSize, "10px");
            control(&mut b, 15, Tag::Textarea);
            b.set_attribute(15, Attribute::Rows, "1")
                .set_style(15, P::FontSize, "7px");
            None
        }
        // Block-level controls fit their content; a sized block button
        // centers its label.
        "block" => {
            b.set_style(2, P::Display, "block");
            control(&mut b, 10, Tag::Button);
            label(&mut b, 10, "AB");
            b.set_style(10, P::Display, "block");
            control(&mut b, 12, Tag::Button);
            label(&mut b, 12, "A");
            b.set_style(12, P::Display, "block")
                .set_style(12, P::Width, "80px")
                .set_style(12, P::Height, "30px");
            input(&mut b, 14, "number");
            b.set_style(14, P::Display, "block").set_value(14, "7", 0);
            None
        }
        // Flex buttons are ordinary flex containers: with and without author
        // alignment, as a full-width block and as a flex item.
        "flex-button" => {
            b.set_style(2, P::Display, "block");
            control(&mut b, 10, Tag::Button);
            for (property, value) in [
                (P::Display, "flex"),
                (P::AlignItems, "center"),
                (P::Gap, "10px"),
                (P::Width, "100%"),
                (P::Border, "1px solid #e2e4e9"),
                (P::BorderRadius, "10px"),
                (P::BackgroundColor, "#ffffff"),
                (P::Padding, "8px 12px"),
                (P::FontSize, "13px"),
                (P::TextAlign, "left"),
            ] {
                b.set_style(10, property, value);
            }
            for (id, text) in [(11, "A"), (13, "AB x")] {
                b.create_element(id, Tag::Span)
                    .insert(10, id, 0)
                    .create_text(id + 1, text)
                    .insert(id, id + 1, 0);
            }
            control(&mut b, 15, Tag::Button);
            b.set_style(15, P::Display, "flex")
                .set_style(15, P::Height, "30px");
            label(&mut b, 15, "AB");
            None
        }
        "placeholder" => {
            input(&mut b, 10, "text");
            b.set_attribute(10, Attribute::Placeholder, "AB x");
            input(&mut b, 12, "number");
            b.set_attribute(12, Attribute::Placeholder, "0");
            control(&mut b, 14, Tag::Textarea);
            b.set_attribute(14, Attribute::Placeholder, "x")
                .set_attribute(14, Attribute::Rows, "1");
            input(&mut b, 15, "text");
            b.set_attribute(15, Attribute::Placeholder, "AB")
                .set_attribute(15, Attribute::Disabled, "");
            None
        }
        _ => unreachable!("listed case"),
    };
    (b, focus)
}

const CASES: &[&str] = &[
    "themed",
    "disabled",
    "focus-button",
    "focus-text",
    "focus-number",
    "focus-checkbox",
    "focus-radio",
    "focus-textarea",
    "focus-styled",
    "styled",
    "control-font",
    "block",
    "flex-button",
    "placeholder",
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
    let mut font = TextMeasurer::new_with_fonts(vec![TextFont {
        data: FONT.to_vec(),
        family: FAMILY.into(),
        aliases: Vec::new(),
    }])?;
    let (width, height) = VIEWPORT;
    let mut cases = Vec::new();
    for &name in CASES {
        let (batch, focus) = case(name);
        let mut view = View::default();
        view.commit(&batch.finish(), false, &mut Denied)?;
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
            "focus": focus,
            "boxes": boxes,
            "hits": hits,
            "raster": image.pixels.as_slice(),
            "raster_size": [image.width, image.height],
        }));
    }
    let output = serde_json::to_string(&serde_json::json!({
        "family": FAMILY,
        "font": FONT,
        "ua_version": varta_view::NATIVE_UA_VERSION,
        "scale": scale,
        "viewport": {"width": width, "height": height},
        "cases": cases,
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}
