//! Modal dialogs in the top layer: centring, fit-content sizing, the viewport
//! reserve, user-agent colors, the backdrop above every layer (including a
//! z-indexed box), stacked dialogs and backdrop hits, for the real-Chrome
//! geometry, hit and pixel oracle in `scripts/test-native-dialog.mjs`.
//! Usage: native_dialog_probe [SCALE]
use varta_common::view::{Command, CssProperty as P, Tag};
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

const FAMILY: &str = "Varta Inline Fixture";
const FONT: &[u8] = include_bytes!("../tests/fixtures/inline.ttf");
/// The viewport in CSS pixels: room for a dialog inside the `2em + 6px`
/// reserve at the probe's 20px text.
const SIDE: u32 = 120;
/// Chrome's initial `medium` font size, handed to the measurer as the host size.
const MEDIUM: f32 = 16.0;
/// Pointer samples every this many CSS pixels.
const HIT_STEP: usize = 3;
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.0;

const RED: &str = "#e00000";
const GREEN: &str = "#00a000";
const BLUE: &str = "#0000e0";
const YELLOW: &str = "#e0c000";
const GRAY: &str = "#c0c0c0";

type Styles<'a> = &'a [(P, &'a str)];

/// An element `tag` `id` appended to `parent` with `styles`.
fn element(b: &mut B, tag: Tag, id: u32, parent: u32, styles: Styles) {
    b.create_element(id, tag).insert(parent, id, 0);
    for &(property, value) in styles {
        b.set_style(id, property, value);
    }
}

/// The page behind the dialogs: a full-width block and a z-indexed
/// positioned box, which the top layer still covers.
fn page(b: &mut B) {
    element(
        b,
        Tag::Div,
        2,
        1,
        &[(P::Height, "60px"), (P::BackgroundColor, GRAY)],
    );
    element(
        b,
        Tag::Div,
        3,
        1,
        &[
            (P::Position, "absolute"),
            (P::ZIndex, "10"),
            (P::Top, "50px"),
            (P::Left, "50px"),
            (P::Width, "60px"),
            (P::Height, "60px"),
            (P::BackgroundColor, RED),
        ],
    );
}

/// The dialogs a case shows modally, in order.
fn build(name: &str, b: &mut B) -> &'static [u32] {
    page(b);
    match name {
        // A fit-content dialog, centred over everything.
        "centered" => {
            element(b, Tag::Dialog, 4, 1, &[]);
            element(
                b,
                Tag::Div,
                5,
                4,
                &[
                    (P::Width, "30px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            &[4]
        }
        // Text takes the dialog's user-agent colors.
        "text" => {
            element(b, Tag::Dialog, 4, 1, &[]);
            b.create_text(5, "AAA").insert(4, 5, 0);
            &[4]
        }
        // A later dialog stacks above an earlier one and its backdrop.
        "stacked" => {
            element(b, Tag::Dialog, 4, 1, &[(P::MarginTop, "10px")]);
            element(
                b,
                Tag::Div,
                5,
                4,
                &[
                    (P::Width, "50px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
            element(b, Tag::Dialog, 6, 1, &[]);
            element(
                b,
                Tag::Div,
                7,
                6,
                &[
                    (P::Width, "20px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
            &[4, 6]
        }
        // Wider content than the reserve allows: the dialog is clamped to
        // the viewport minus `2em + 6px` and clips its content.
        "clamped" => {
            element(b, Tag::Dialog, 4, 1, &[(P::Overflow, "hidden")]);
            element(
                b,
                Tag::Div,
                5,
                4,
                &[
                    (P::Width, "200px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            &[4]
        }
        // Author padding, border, background and width on the dialog box.
        "author-box" => {
            element(
                b,
                Tag::Dialog,
                4,
                1,
                &[
                    (P::Width, "40px"),
                    (P::Padding, "4px"),
                    (P::Border, "2px solid #000000"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
            element(
                b,
                Tag::Div,
                5,
                4,
                &[(P::Height, "10px"), (P::BackgroundColor, GREEN)],
            );
            &[4]
        }
        other => unreachable!("unknown case {other}"),
    }
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
    let mut font = TextMeasurer::new(FONT.to_vec(), FAMILY)?;
    let side = (SIDE as f32 * scale).round() as u32;
    let mut cases = Vec::new();
    for name in ["centered", "text", "stacked", "clamped", "author-box"] {
        let mut b = B::new();
        b.set_style(1, P::FontFamily, "\"Varta Inline Fixture\"")
            .set_style(1, P::FontSize, "20px")
            .set_style(1, P::LineHeight, "20px");
        let modals = build(name, &mut b);
        for &dialog in modals {
            b.command(dialog, Command::ShowModal, 0.0);
        }
        let mut view = View::default();
        view.commit_with_layout(
            &b.finish(),
            false,
            &mut Denied,
            SIDE as f32,
            SIDE as f32,
            Some((&mut font, MEDIUM)),
        )?;
        let geometry = view.layout_boxes_with_text(SIDE as f32, SIDE as f32, &mut font, MEDIUM)?;
        let image = view.rasterize_scaled(side, side, scale, Some((&mut font, MEDIUM)))?;
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
        let mut hits = Vec::new();
        for y in (0..SIDE).step_by(HIT_STEP) {
            for x in (0..SIDE).step_by(HIT_STEP) {
                let px = x as f32 + 0.25;
                let py = y as f32 + 0.25;
                let target = image.hit_test(px * scale, py * scale);
                let boundary = [-1.0, 1.0].into_iter().any(|dx| {
                    [-1.0, 1.0]
                        .into_iter()
                        .any(|dy| image.hit_test((px + dx) * scale, (py + dy) * scale) != target)
                });
                hits.push(serde_json::json!({"x":x,"y":y,"id":target,"boundary":boundary}));
            }
        }
        cases.push(serde_json::json!({
            "name":name,"tree":view.snapshot()["tree"],"modals":modals,"boxes":boxes,"hits":hits,
            "raster":image.pixels,"raster_size":[image.width,image.height],
            "roi":{"x":0,"y":0,"width":side,"height":side}
        }));
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "family":FAMILY,"font":FONT,"scale":scale,
        "viewport":{"width":SIDE,"height":SIDE},"cases":cases
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
