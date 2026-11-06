//! Overlay scrollbars (the macOS "Show scroll bars: when scrolling" style)
//! for the real-Chrome oracle in `scripts/test-native-overlay-scrollbars.mjs`.
//! Each case builds a scroll container and runs steps on the view's
//! animation clock: a wheel or programmatic scroll, the pointer moving over
//! the bar frame or away, waits, and captures. Chrome runs the same steps
//! in real time; the captures are taken where both are steady: just after a
//! scroll (the thumb shown), after the pointer has rested on the bar (thumb
//! widened, track shown) and after the fade-out (nothing shown).
//! Usage: native_overlay_scrollbar_probe [SCALE]
use varta_common::view::{Command, CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter as B, Broker, Environment, Input, Platform, Scrollbars, Target, View,
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

/// The viewport in CSS pixels.
const WIDTH: u32 = 240;
const HEIGHT: u32 = 200;
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.0;
/// The scroll container's id; content ids start at `CONTENT`.
const SCROLLER: u32 = 2;
const CONTENT: u32 = 10;
/// The clock when the first step runs, in milliseconds.
const START_MS: f64 = 1000.0;
/// Waits Chrome needs for steady captures: the 250ms expansion and track
/// fade-in, and the 500ms delay with the 250ms fade-out.
const SETTLE_MS: f64 = 400.0;
const FADED_MS: f64 = 1100.0;

const STRIPES: [&str; 3] = ["#3050e0", "#30a050", "#e0c030"];

type Styles<'a> = &'a [(P, &'a str)];

#[derive(Clone, Copy)]
enum Step {
    /// A wheel scroll to `top` with the pointer at `(x, y)`.
    Wheel {
        x: f32,
        y: f32,
        top: f64,
    },
    /// `scrollTop = top`.
    Program {
        top: f64,
    },
    /// The pointer moves to `(x, y)`.
    Pointer {
        x: f32,
        y: f32,
    },
    Wait {
        ms: f64,
    },
    Capture {
        name: &'static str,
    },
}

struct Case {
    name: &'static str,
    scroller: Styles<'static>,
    /// Content width and height.
    content: (&'static str, &'static str),
    steps: &'static [Step],
}

const SHOW: Step = Step::Capture { name: "shown" };
const HOVERED: Step = Step::Capture { name: "hovered" };
const FADED: Step = Step::Capture { name: "faded" };
const SETTLE: Step = Step::Wait { ms: SETTLE_MS };
const FADE: Step = Step::Wait { ms: FADED_MS };
/// A content point away from the bar, and one on it.
const AWAY: Step = Step::Pointer { x: 100.0, y: 100.0 };
const ON_BAR: Step = Step::Pointer { x: 212.0, y: 100.0 };

fn cases() -> [Case; 7] {
    [
        Case {
            name: "wheel",
            scroller: &[],
            content: ("auto", "600px"),
            steps: &[
                AWAY,
                Step::Wheel {
                    x: 100.0,
                    y: 100.0,
                    top: 60.0,
                },
                SHOW,
                ON_BAR,
                SETTLE,
                HOVERED,
                AWAY,
                FADE,
                FADED,
            ],
        },
        Case {
            name: "wheel-on-bar",
            scroller: &[],
            content: ("auto", "600px"),
            steps: &[
                ON_BAR,
                Step::Wheel {
                    x: 212.0,
                    y: 100.0,
                    top: 120.0,
                },
                SETTLE,
                HOVERED,
            ],
        },
        Case {
            name: "border-padding",
            scroller: &[(P::Border, "5px solid #404040"), (P::Padding, "3px")],
            content: ("auto", "600px"),
            steps: &[Step::Program { top: 100.0 }, SHOW, FADE, FADED],
        },
        Case {
            name: "dark",
            scroller: &[(P::BackgroundColor, "#202020")],
            content: ("auto", "600px"),
            steps: &[
                AWAY,
                Step::Program { top: 100.0 },
                SHOW,
                ON_BAR,
                SETTLE,
                HOVERED,
            ],
        },
        Case {
            name: "corner",
            scroller: &[],
            content: ("400px", "600px"),
            steps: &[Step::Program { top: 100.0 }, SHOW],
        },
        Case {
            name: "rounded",
            scroller: &[(P::BorderRadius, "24px")],
            content: ("auto", "600px"),
            steps: &[Step::Program { top: 1.0 }, SHOW],
        },
        Case {
            name: "long",
            scroller: &[],
            content: ("auto", "40000px"),
            steps: &[Step::Program { top: 20000.0 }, SHOW],
        },
    ]
}

fn div(b: &mut B, id: u32, parent: u32, styles: Styles) {
    b.create_element(id, Tag::Div).insert(parent, id, 0);
    for &(property, value) in styles {
        b.set_style(id, property, value);
    }
}

fn build(case: &Case, b: &mut B) {
    div(
        b,
        SCROLLER,
        1,
        &[
            (P::Position, "absolute"),
            (P::Left, "20px"),
            (P::Top, "20px"),
            (P::Width, "200px"),
            (P::Height, "150px"),
            (P::Overflow, "auto"),
            (P::BackgroundColor, "#ffffff"),
        ],
    );
    for &(property, value) in case.scroller {
        b.set_style(SCROLLER, property, value);
    }
    // A wrapper of the content's size holding 50px stripes, transparent
    // below the last one.
    div(
        b,
        CONTENT,
        SCROLLER,
        &[(P::Width, case.content.0), (P::Height, case.content.1)],
    );
    for (index, color) in STRIPES.iter().enumerate() {
        let id = CONTENT + 1 + index as u32;
        div(
            b,
            id,
            CONTENT,
            &[(P::Height, "50px"), (P::BackgroundColor, color)],
        );
    }
}

type Probe = Result<serde_json::Value, Box<dyn std::error::Error>>;

fn run(case: &Case, scale: f32) -> Probe {
    let (width, height) = (WIDTH as f32, HEIGHT as f32);
    let (device_width, device_height) = (
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    );
    let environment = Environment {
        width: f64::from(WIDTH),
        scrollbars: Scrollbars::Overlay,
        ..Environment::default()
    };
    let mut view = View::new(environment, Platform::default());
    let mut b = B::new();
    build(case, &mut b);
    view.commit_with_layout(&b.finish(), false, &mut Denied, width, height, None)?;
    let mut now = START_MS;
    view.set_animation_time(now);
    let mut captures = Vec::new();
    for step in case.steps {
        match *step {
            Step::Wheel { top, .. } => {
                let geometry = view.layout_geometry(width, height)?;
                let scroll = geometry[&SCROLLER]
                    .scroll
                    .ok_or("the scroller has no scroll range")?;
                view.input(
                    &Input::ScrollMeasured {
                        target: Target::Node(SCROLLER),
                        top,
                        scroll_height: f64::from(scroll.viewport.height + scroll.max),
                        client_height: f64::from(scroll.viewport.height),
                    },
                    &mut Denied,
                )?;
            }
            Step::Program { top } => {
                let mut b = B::new();
                b.command(SCROLLER, Command::ScrollTo, top);
                view.commit_with_layout(&b.finish(), false, &mut Denied, width, height, None)?;
            }
            Step::Pointer { x, y } => {
                let image = view.rasterize_scaled(device_width, device_height, scale, None)?;
                view.set_scrollbar_hover(image.scrollbar_at(x * scale, y * scale));
            }
            Step::Wait { ms } => {
                now += ms;
                view.set_animation_time(now);
            }
            Step::Capture { name } => {
                let image = view.rasterize_scaled(device_width, device_height, scale, None)?;
                let geometry = view.layout_geometry(width, height)?;
                let rect = geometry[&SCROLLER].rect;
                let bar = geometry[&SCROLLER]
                    .scrollbars
                    .and_then(|bars| bars.vertical)
                    .ok_or("the scroller has no overlay bar")?;
                // Hits on the bar frame and on the content beside it.
                let hits = [
                    (bar.x + 8.0, bar.y + bar.height / 2.0),
                    (bar.x + 1.0, bar.y + 10.0),
                    (bar.x - 20.0, bar.y + bar.height / 2.0),
                ]
                .map(|(x, y)| {
                    let x = x.floor() + 0.5;
                    let y = y.floor() + 0.5;
                    serde_json::json!({"x":x,"y":y,"id":image.hit_test(x * scale, y * scale)})
                });
                // The compared region: the bar frame and 24px of content.
                let roi = serde_json::json!({
                    "x": ((bar.x - 24.0) * scale).floor(),
                    "y": (rect.y * scale).floor(),
                    "width": ((bar.width + 24.0) * scale).ceil(),
                    "height": (rect.height * scale).ceil(),
                });
                let content = geometry[&CONTENT].rect;
                captures.push(serde_json::json!({
                    "name": name,
                    "raster": image.pixels,
                    "raster_size": [image.width, image.height],
                    "roi": roi,
                    "hits": hits,
                    "content_width": content.width,
                    "bar": {"x": bar.x, "y": bar.y, "width": bar.width, "height": bar.height},
                }));
            }
        }
    }
    let steps: Vec<serde_json::Value> = case
        .steps
        .iter()
        .map(|step| match *step {
            Step::Wheel { x, y, top } => serde_json::json!({"wheel":{"x":x,"y":y,"top":top}}),
            Step::Program { top } => serde_json::json!({"program":{"top":top}}),
            Step::Pointer { x, y } => serde_json::json!({"pointer":{"x":x,"y":y}}),
            Step::Wait { ms } => serde_json::json!({"wait":ms}),
            Step::Capture { name } => serde_json::json!({"capture":name}),
        })
        .collect();
    Ok(serde_json::json!({
        "name": case.name,
        "tree": view.snapshot()["tree"],
        "steps": steps,
        "captures": captures,
    }))
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
    let mut results = Vec::new();
    for case in cases() {
        results.push(run(&case, scale).map_err(|e| format!("{}: {e}", case.name))?);
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "scale": scale,
        "viewport": {"width": WIDTH, "height": HEIGHT},
        "scroller": SCROLLER,
        "content": CONTENT,
        "cases": results,
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
