//! Classic (always-shown) scrollbars for the real-Chrome geometry, hit and
//! pixel oracle in `scripts/test-native-positioned.mjs`, which runs Chrome
//! with macOS's "Show scroll bars: Always" for this process
//! (`"scrollbars": "classic"`). Scroll containers that overflow vertically,
//! horizontally, on both axes (with the corner), only after the vertical
//! gutter narrows their content, with borders and padding, with a document
//! long enough for the minimum thumb, and one that fits (no bar).
//! Usage: native_scrollbar_probe [SCALE]
use varta_common::view::{Command, CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter as B, Broker, Environment, Platform, Scrollbars, TextMeasurer, View,
};

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
/// The viewport in CSS pixels.
const SIDE: u32 = 120;
/// Chrome's initial `medium` font size.
const MEDIUM: f32 = 16.0;
/// Pointer samples every this many CSS pixels.
const HIT_STEP: usize = 3;
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.0;

const BLUE: &str = "#3050e0";
const GREEN: &str = "#30a050";
const YELLOW: &str = "#e0c030";

type Styles<'a> = &'a [(P, &'a str)];

fn div(b: &mut B, id: u32, parent: u32, styles: Styles) {
    b.create_element(id, Tag::Div).insert(parent, id, 0);
    for &(property, value) in styles {
        b.set_style(id, property, value);
    }
}

/// A scroll container `id` of `width` by `height` at the root's top left.
fn scroller(b: &mut B, id: u32, width: &str, height: &str, extra: Styles) {
    div(
        b,
        id,
        1,
        &[
            (P::Overflow, "auto"),
            (P::Width, width),
            (P::Height, height),
            (P::BackgroundColor, "#ffffff"),
        ],
    );
    for &(property, value) in extra {
        b.set_style(id, property, value);
    }
}

/// Stripes of `height` each, alternating colors, filling the width.
fn stripes(b: &mut B, parent: u32, first: u32, count: u32, height: &str) {
    for index in 0..count {
        let color = [BLUE, GREEN, YELLOW][index as usize % 3];
        div(
            b,
            first + index,
            parent,
            &[(P::Height, height), (P::BackgroundColor, color)],
        );
    }
}

struct Case {
    name: &'static str,
    /// Programmatic scroll offsets, whole device pixels at the compared
    /// scales.
    scrolls: &'static [(u32, f64)],
}

fn build(name: &str, b: &mut B) {
    match name {
        "vertical" => {
            scroller(b, 2, "80px", "80px", &[]);
            stripes(b, 2, 10, 6, "40px");
        }
        "horizontal" => {
            scroller(b, 2, "100px", "50px", &[]);
            div(
                b,
                3,
                2,
                &[
                    (P::Width, "260px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
        }
        "both" => {
            scroller(b, 2, "90px", "90px", &[]);
            div(
                b,
                3,
                2,
                &[
                    (P::Width, "200px"),
                    (P::Height, "300px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
        }
        "gutter-wrap" => {
            // 70px wide: the content's 60px child fits until the vertical
            // gutter leaves 55px, which then overflows horizontally too.
            scroller(b, 2, "70px", "70px", &[]);
            div(
                b,
                3,
                2,
                &[
                    (P::Width, "60px"),
                    (P::Height, "100px"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
        }
        "border-padding" => {
            scroller(
                b,
                2,
                "90px",
                "90px",
                &[(P::Border, "3px solid #404040"), (P::Padding, "5px")],
            );
            stripes(b, 2, 10, 5, "30px");
        }
        "long" => {
            scroller(b, 2, "60px", "110px", &[]);
            div(
                b,
                3,
                2,
                &[(P::Height, "20000px"), (P::BackgroundColor, BLUE)],
            );
        }
        "fits" => {
            scroller(b, 2, "80px", "80px", &[]);
            stripes(b, 2, 10, 2, "30px");
        }
        _ => unreachable!("unknown case {name}"),
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
    let environment = Environment {
        width: f64::from(SIDE),
        scrollbars: Scrollbars::Classic,
        ..Environment::default()
    };
    let mut cases = Vec::new();
    for case in [
        Case {
            name: "vertical",
            scrolls: &[(2, 60.0)],
        },
        Case {
            name: "horizontal",
            scrolls: &[],
        },
        Case {
            name: "both",
            scrolls: &[(2, 100.0)],
        },
        Case {
            name: "gutter-wrap",
            scrolls: &[],
        },
        Case {
            name: "border-padding",
            scrolls: &[(2, 20.0)],
        },
        Case {
            name: "long",
            scrolls: &[(2, 6000.0)],
        },
        Case {
            name: "fits",
            scrolls: &[],
        },
    ] {
        let mut b = B::new();
        b.set_style(1, P::FontFamily, "\"Varta Inline Fixture\"");
        build(case.name, &mut b);
        for &(id, top) in case.scrolls {
            b.command(id, Command::ScrollTo, top);
        }
        let mut view = View::new(environment, Platform::default());
        view.commit_with_layout(
            &b.finish(),
            false,
            &mut Denied,
            SIDE as f32,
            SIDE as f32,
            Some((&mut font, MEDIUM)),
        )?;
        let named = |error: &dyn std::fmt::Display| format!("{}: {error}", case.name);
        let geometry = view
            .layout_boxes_with_text(SIDE as f32, SIDE as f32, &mut font, MEDIUM)
            .map_err(|e| named(&e))?;
        let image = view
            .rasterize_scaled(side, side, scale, Some((&mut font, MEDIUM)))
            .map_err(|e| named(&e))?;
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
            "name":case.name,"tree":view.snapshot()["tree"],"boxes":boxes,"hits":hits,
            "raster":image.pixels,"raster_size":[image.width,image.height],
            "roi":{"x":0,"y":0,"width":side,"height":side},"reveal":null
        }));
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "family":FAMILY,"font":FONT,"scale":scale,"scrollbars":"classic",
        "viewport":{"width":SIDE,"height":SIDE},"cases":cases
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
