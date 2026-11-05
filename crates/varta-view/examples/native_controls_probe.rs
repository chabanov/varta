//! Native form-control defaults (user-agent box, control font, Chrome's
//! native theme, disabled colors, placeholder and default focus ring) and
//! `input type=number` stepping, for `scripts/test-native-controls.mjs`.
//! Usage: native_controls_probe [SCALE]
use varta_common::view::{Attribute, Command, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Input, Target, TextFont, TextMeasurer, View};

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
        // Author outline longhands without an outline style keep Chrome's
        // auto ring: its color and offset follow the author, its width stays
        // two pixels; the `outline` shorthand without `solid` removes it.
        "focus-outline-color"
        | "focus-outline-offset"
        | "focus-outline-checkbox"
        | "focus-outline-styled"
        | "focus-outline-shorthand" => {
            let (tag, kind) = match name {
                "focus-outline-color" | "focus-outline-shorthand" => (Tag::Input, Some("text")),
                "focus-outline-checkbox" => (Tag::Input, Some("checkbox")),
                _ => (Tag::Button, None),
            };
            control(&mut b, 10, tag);
            if let Some(kind) = kind {
                b.set_attribute(10, Attribute::Type, kind);
            }
            if tag == Tag::Button {
                label(&mut b, 10, "AB");
            }
            let declarations: &[(P, &str)] = match name {
                "focus-outline-color" => &[(P::OutlineColor, "#ff0000")],
                "focus-outline-offset" => &[
                    (P::OutlineOffset, "3px"),
                    (P::OutlineWidth, "5px"),
                    (P::OutlineColor, "#00aa00"),
                ],
                "focus-outline-checkbox" => {
                    &[(P::OutlineOffset, "0"), (P::OutlineColor, "#cc00cc")]
                }
                "focus-outline-styled" => &[
                    (P::Border, "1px solid #000000"),
                    (P::BorderRadius, "6px"),
                    (P::OutlineOffset, "2px"),
                    (P::OutlineColor, "#ff0000"),
                ],
                _ => &[(P::Outline, "2px #ff0000")],
            };
            for (property, value) in declarations {
                b.set_style(10, *property, value);
            }
            b.command(10, Command::Focus, 0.0);
            Some(10)
        }
        // A select keeps its explicit-style requirement (its theme, arrow
        // and popup belong to the select painter): author-styled selects,
        // enabled and disabled, beside themed controls whose user-agent
        // colors do not reach them.
        "select" => {
            for (id, disabled) in [(10, false), (14, true)] {
                control(&mut b, id, Tag::Select);
                for (property, value) in [
                    (P::Display, "block"),
                    (P::Width, "80px"),
                    (P::Height, "24px"),
                    (P::MinWidth, "0"),
                    (P::MinHeight, "0"),
                    (P::Padding, "2px 4px"),
                    (P::Border, "1px solid #888888"),
                    (P::BorderRadius, "3px"),
                    (P::Outline, "none"),
                    (P::BackgroundColor, "#ffffff"),
                    (P::Color, "#0000cc"),
                    (P::FontFamily, "\"Varta Controls Fixture\""),
                    (P::FontSize, "13px"),
                    (P::FontWeight, "normal"),
                    (P::FontStyle, "normal"),
                    (P::LineHeight, "16px"),
                ] {
                    b.set_style(id, property, value);
                }
                for (offset, text) in [(1, "AB"), (2, "x")] {
                    let option = id + offset;
                    b.create_element(option, Tag::Option)
                        .insert(id, option, 0)
                        .create_text(option + 10, text)
                        .insert(option, option + 10, 0);
                }
                if disabled {
                    b.set_attribute(id, Attribute::Disabled, "");
                }
            }
            control(&mut b, 30, Tag::Button);
            label(&mut b, 30, "AB");
            None
        }
        // A rich label (text and an inline element) keeps the control's
        // whole-pixel line spacing, also in an element with its own size.
        // The buttons are sized: Chrome snaps each inline item's advance to
        // its 1/64px layout unit, which native text does not model.
        "mixed-label" => {
            b.set_style(2, P::Display, "block");
            control(&mut b, 10, Tag::Button);
            b.set_style(10, P::Display, "block")
                .set_style(10, P::Width, "60px")
                .create_text(11, "A ")
                .insert(10, 11, 0)
                .create_element(12, Tag::Span)
                .insert(10, 12, 0)
                .create_text(13, "B")
                .insert(12, 13, 0);
            control(&mut b, 14, Tag::Button);
            b.set_style(14, P::Display, "block")
                .set_style(14, P::Width, "60px")
                .create_text(15, "x ")
                .insert(14, 15, 0)
                .create_element(16, Tag::Span)
                .set_style(16, P::FontSize, "17px")
                .insert(14, 16, 0)
                .create_text(17, "AB")
                .insert(16, 17, 0);
            None
        }
        // Checkables take their 13px square as an intrinsic size: the
        // default `align-items: stretch` grows them beside a 4-row textarea,
        // and a stretch grid area grows both axes; the theme paints the
        // largest centered square.
        "stretch-flex" | "focus-stretch-checkbox" | "focus-stretch-radio" => {
            b.remove_style(2, P::AlignItems);
            control(&mut b, 10, Tag::Textarea);
            b.set_attribute(10, Attribute::Rows, "4");
            match name {
                "stretch-flex" => {
                    input(&mut b, 12, "checkbox");
                    input(&mut b, 13, "checkbox");
                    b.set_checked(13, 1, 0);
                    input(&mut b, 14, "radio");
                    input(&mut b, 15, "radio");
                    b.set_checked(15, 1, 0);
                    None
                }
                _ => {
                    let kind = if name == "focus-stretch-radio" {
                        "radio"
                    } else {
                        "checkbox"
                    };
                    input(&mut b, 12, kind);
                    b.set_checked(12, 1, 0).command(12, Command::Focus, 0.0);
                    Some(12)
                }
            }
        }
        "stretch-grid" => {
            b.set_style(2, P::Display, "grid")
                .set_style(2, P::GridTemplateColumns, "150px 150px")
                .set_style(2, P::GridTemplateRows, "34px 20px");
            input(&mut b, 10, "checkbox");
            b.set_checked(10, 1, 0);
            input(&mut b, 12, "radio");
            b.set_checked(12, 1, 0);
            input(&mut b, 13, "checkbox");
            input(&mut b, 14, "radio");
            None
        }
        // A themed checkable resets author padding (Blink's
        // `AdjustCheckboxStyle`): its 13px square stays the border box at
        // flex-start, stretched beside a textarea and in a grid area.
        "padding-flex" => {
            for (id, kind, padding) in [
                (10, "checkbox", "5px"),
                (12, "radio", "6px"),
                (13, "checkbox", "10px"),
                (14, "radio", "6px"),
                (15, "checkbox", "5px"),
            ] {
                input(&mut b, id, kind);
                b.set_style(id, P::Padding, padding);
            }
            b.set_checked(13, 1, 0)
                .set_checked(14, 1, 0)
                .set_style(15, P::Width, "20px")
                .command(10, Command::Focus, 0.0);
            Some(10)
        }
        "padding-stretch" => {
            b.remove_style(2, P::AlignItems);
            control(&mut b, 10, Tag::Textarea);
            b.set_attribute(10, Attribute::Rows, "4");
            for (id, kind) in [(12, "checkbox"), (13, "radio")] {
                input(&mut b, id, kind);
                b.set_style(id, P::Padding, "6px").set_checked(id, 1, 0);
            }
            None
        }
        "padding-grid" => {
            b.set_style(2, P::Display, "grid")
                .set_style(2, P::GridTemplateColumns, "150px 150px")
                .set_style(2, P::GridTemplateRows, "34px");
            input(&mut b, 10, "checkbox");
            b.set_style(10, P::Padding, "6px").set_checked(10, 1, 0);
            input(&mut b, 12, "radio");
            b.set_style(12, P::Padding, "10px");
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
    "focus-outline-color",
    "focus-outline-offset",
    "focus-outline-checkbox",
    "focus-outline-styled",
    "focus-outline-shorthand",
    "select",
    "mixed-label",
    "stretch-flex",
    "stretch-grid",
    "focus-stretch-checkbox",
    "focus-stretch-radio",
    "padding-flex",
    "padding-stretch",
    "padding-grid",
];

/// `input type=number` values set by the guest and the Up/Down presses
/// (`Alt+` holds Alt) replayed on a focused field, with the caret at the end.
/// Values Chrome steps in its 18-digit decimal with precision loss are
/// refused natively and are not listed (see `number_step`'s unit tests).
const STEPS: &[(&str, &[&str])] = &[
    ("", &["ArrowUp"]),
    ("", &["ArrowDown", "ArrowDown"]),
    ("1.5", &["ArrowUp"]),
    ("1.5", &["ArrowDown"]),
    ("-1.5", &["ArrowUp"]),
    ("-0.5", &["ArrowUp"]),
    ("0.5", &["ArrowDown"]),
    ("2.0", &["ArrowUp", "ArrowUp", "ArrowDown"]),
    (".5", &["ArrowUp"]),
    ("-.5", &["ArrowUp"]),
    ("00012", &["ArrowUp"]),
    ("1e3", &["ArrowUp"]),
    ("1.e5", &["ArrowDown"]),
    ("5E-1", &["ArrowUp"]),
    ("-0", &["ArrowDown"]),
    ("12e", &["ArrowUp"]),
    ("5.", &["ArrowUp"]),
    ("+5", &["ArrowUp"]),
    (" 5", &["ArrowDown"]),
    ("1,000", &["ArrowUp"]),
    ("1e309", &["ArrowUp"]),
    ("6e-8", &["ArrowDown"]),
    ("5e-8", &["ArrowDown"]),
    ("0.99999999", &["ArrowDown"]),
    ("1.00000001", &["ArrowUp"]),
    ("1e-18", &["ArrowDown"]),
    ("0.30000000000000004", &["ArrowDown"]),
    ("123456789.123456789", &["ArrowUp"]),
    ("-123456789012344.9", &["ArrowDown"]),
    ("999999999999999.0", &["ArrowUp"]),
    ("41", &["Alt+ArrowDown"]),
    ("41", &["Alt+ArrowUp"]),
];

const NUMBER_FIELD: u32 = 2;

/// The native value after replaying `keys` on a number field holding `value`.
fn stepped(value: &str, keys: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let mut b = B::new();
    b.create_element(NUMBER_FIELD, Tag::Input)
        .set_attribute(NUMBER_FIELD, Attribute::Type, "number")
        .set_value(NUMBER_FIELD, value, 0)
        .insert(1, NUMBER_FIELD, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Denied)?;
    let target = Target::Node(NUMBER_FIELD);
    view.input(
        &Input::TextSelect {
            target: target.clone(),
            anchor: value.len(),
            focus: value.len(),
            activate: true,
        },
        &mut Denied,
    )?;
    for key in keys {
        let (alt, key) = key
            .strip_prefix("Alt+")
            .map_or((false, *key), |key| (true, key));
        view.input(
            &Input::Key {
                target: target.clone(),
                key: key.into(),
                shift: false,
                ctrl: false,
                alt,
                meta: false,
                composing: false,
            },
            &mut Denied,
        )?;
    }
    Ok(view.snapshot()["tree"]["children"][0]["value"]
        .as_str()
        .ok_or("number field value")?
        .to_owned())
}

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
    let steps = STEPS
        .iter()
        .map(|(value, keys)| {
            Ok(serde_json::json!({"value": value, "keys": keys, "native": stepped(value, keys)?}))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let output = serde_json::to_string(&serde_json::json!({
        "family": FAMILY,
        "font": FONT,
        "ua_version": varta_view::NATIVE_UA_VERSION,
        "scale": scale,
        "viewport": {"width": width, "height": height},
        "cases": cases,
        "steps": steps,
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}
