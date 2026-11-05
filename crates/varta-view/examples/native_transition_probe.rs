//! CSS transitions on the animation clock: easing, delays, lists, initial
//! values, premultiplied colors, reversing, retargeting, cancelling and an
//! inherited color, for the real-Chrome oracle in
//! `scripts/test-native-transitions.mjs`. Each case commits its first style
//! at time zero, applies its style changes at their times and samples the
//! painted pixel of each probe at each sample time.
//! Usage: native_transition_probe
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, View};

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

const WIDTH: u32 = 40;
const HEIGHT: u32 = 80;
/// The box whose background, opacity and transitions the cases change.
const BOX: u32 = 2;
/// A child of the box whose border paints its (inherited) `color`.
const BORDER: u32 = 3;

/// One style change: a node, a property and its new value (`None` removes it).
type Change = (u32, P, Option<&'static str>);

struct Case {
    name: &'static str,
    /// The box's and the border child's first styles.
    first: &'static [(u32, P, &'static str)],
    /// Style changes at their times, in order.
    steps: &'static [(f64, &'static [Change])],
    samples: &'static [f64],
}

/// The probes: the box's background (above the child) and the child's
/// left border, in CSS pixels.
const PROBES: [(u32, &str, u32, u32); 2] = [(BOX, "background", 20, 4), (BORDER, "border", 3, 30)];

const CASES: &[Case] = &[
    // The console's buttons: two properties, one duration, the initial ease.
    Case {
        name: "ease",
        first: &[
            (BOX, P::TransitionProperty, "background-color, color"),
            (BOX, P::TransitionDuration, "120ms"),
            (BOX, P::BackgroundColor, "#ffffff"),
            (BOX, P::Color, "#16181d"),
        ],
        steps: &[(
            0.0,
            &[
                (BOX, P::BackgroundColor, Some("#3b5bdb")),
                (BOX, P::Color, Some("#e0c000")),
            ],
        )],
        samples: &[0.0, 10.0, 30.0, 60.0, 90.0, 119.0, 120.0, 200.0],
    },
    // A delay holds the start value; linear opacity.
    Case {
        name: "opacity-delay",
        first: &[
            (BOX, P::TransitionProperty, "opacity"),
            (BOX, P::TransitionDuration, "200ms"),
            (BOX, P::TransitionDelay, "50ms"),
            (BOX, P::TransitionTimingFunction, "linear"),
            (BOX, P::BackgroundColor, "#000000"),
        ],
        steps: &[(0.0, &[(BOX, P::Opacity, Some("0.2"))])],
        samples: &[0.0, 25.0, 50.0, 100.0, 150.0, 249.0, 250.0, 300.0],
    },
    // Back to the start value at 40%: the reversing shortening factor.
    Case {
        name: "reverse",
        first: &[
            (BOX, P::TransitionProperty, "background-color"),
            (BOX, P::TransitionDuration, "100ms"),
            (BOX, P::TransitionTimingFunction, "linear"),
            (BOX, P::BackgroundColor, "#ffffff"),
        ],
        steps: &[
            (0.0, &[(BOX, P::BackgroundColor, Some("#000000"))]),
            (40.0, &[(BOX, P::BackgroundColor, Some("#ffffff"))]),
        ],
        samples: &[20.0, 40.0, 50.0, 60.0, 70.0, 79.0, 80.0, 100.0],
    },
    // A third value restarts from the current value with the full duration.
    Case {
        name: "retarget",
        first: &[
            (BOX, P::TransitionProperty, "background-color"),
            (BOX, P::TransitionDuration, "100ms"),
            (BOX, P::BackgroundColor, "#ffffff"),
        ],
        steps: &[
            (0.0, &[(BOX, P::BackgroundColor, Some("#000000"))]),
            (40.0, &[(BOX, P::BackgroundColor, Some("#e00000"))]),
        ],
        samples: &[40.0, 60.0, 100.0, 139.0, 140.0],
    },
    // From the initial transparent background to a translucent color.
    Case {
        name: "alpha",
        first: &[
            (BOX, P::TransitionProperty, "background-color"),
            (BOX, P::TransitionDuration, "100ms"),
            (BOX, P::TransitionTimingFunction, "ease-in-out"),
        ],
        steps: &[(
            0.0,
            &[(BOX, P::BackgroundColor, Some("rgba(0, 0, 255, 0.5)"))],
        )],
        samples: &[0.0, 25.0, 50.0, 75.0, 100.0],
    },
    // The last entry naming a property wins; its index picks every list
    // (repeated): color takes index 2 (50ms, linear, no delay), the
    // background index 1 (100ms, ease-out, 10ms).
    Case {
        name: "lists",
        first: &[
            (BOX, P::TransitionProperty, "color, background-color, color"),
            (BOX, P::TransitionDuration, "50ms, 100ms"),
            (BOX, P::TransitionTimingFunction, "linear, ease-out"),
            (BOX, P::TransitionDelay, "0s, 10ms"),
            (BOX, P::BackgroundColor, "#ffffff"),
            (BOX, P::Color, "#000000"),
        ],
        steps: &[(
            0.0,
            &[
                (BOX, P::BackgroundColor, Some("#00a000")),
                (BOX, P::Color, Some("#0000e0")),
            ],
        )],
        samples: &[0.0, 10.0, 35.0, 60.0, 109.0, 110.0],
    },
    // `transition-property: none` cancels the running transition (removing
    // the declaration would leave its initial `all`, which keeps it).
    Case {
        name: "cancel",
        first: &[
            (BOX, P::TransitionProperty, "background-color"),
            (BOX, P::TransitionDuration, "100ms"),
            (BOX, P::TransitionTimingFunction, "linear"),
            (BOX, P::BackgroundColor, "#ffffff"),
        ],
        steps: &[
            (0.0, &[(BOX, P::BackgroundColor, Some("#000000"))]),
            (50.0, &[(BOX, P::TransitionProperty, Some("none"))]),
        ],
        samples: &[25.0, 50.0, 60.0],
    },
    // Removing `transition-property` leaves its initial `all`: the
    // transition runs on.
    Case {
        name: "removed",
        first: &[
            (BOX, P::TransitionProperty, "background-color"),
            (BOX, P::TransitionDuration, "100ms"),
            (BOX, P::TransitionTimingFunction, "linear"),
            (BOX, P::BackgroundColor, "#ffffff"),
        ],
        steps: &[
            (0.0, &[(BOX, P::BackgroundColor, Some("#000000"))]),
            (50.0, &[(BOX, P::TransitionProperty, None)]),
        ],
        samples: &[25.0, 50.0, 75.0, 100.0],
    },
    // A child without its own color paints its parent's animated color.
    Case {
        name: "inherited",
        first: &[
            (BOX, P::TransitionProperty, "color"),
            (BOX, P::TransitionDuration, "100ms"),
            (BOX, P::TransitionTimingFunction, "linear"),
            (BOX, P::Color, "#000000"),
        ],
        steps: &[(0.0, &[(BOX, P::Color, Some("#00a000"))])],
        samples: &[0.0, 50.0, 100.0],
    },
];

/// The case's first tree: the box with a bordered child.
fn first(case: &Case) -> B {
    let mut b = B::new();
    b.create_element(BOX, Tag::Div).insert(1, BOX, 0);
    b.set_style(BOX, P::Width, "40px")
        .set_style(BOX, P::Height, "80px")
        .set_style(BOX, P::PaddingTop, "10px");
    b.create_element(BORDER, Tag::Div).insert(BOX, BORDER, 0);
    b.set_style(BORDER, P::Height, "50px")
        .set_style(BORDER, P::Border, "6px solid currentcolor");
    for &(node, property, value) in case.first {
        b.set_style(node, property, value);
    }
    b
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).is_some() {
        return Err("native_transition_probe takes no argument".into());
    }
    let mut cases = Vec::new();
    for case in CASES {
        let mut view = View::default();
        view.set_animation_time(0.0);
        let tree = first(case);
        view.commit(&tree.finish(), false, &mut Denied)?;
        let initial = view.snapshot()["tree"].clone();
        let mut samples = Vec::new();
        let mut steps = case.steps.iter().peekable();
        for &at in case.samples {
            while let Some(&&(time, changes)) = steps.peek()
                && time <= at
            {
                view.set_animation_time(time);
                let mut b = B::new();
                for &(node, property, value) in changes {
                    match value {
                        Some(value) => b.set_style(node, property, value),
                        None => b.remove_style(node, property),
                    };
                }
                view.commit(&b.finish(), false, &mut Denied)?;
                steps.next();
            }
            view.set_animation_time(at);
            let image = view.rasterize_boxes(WIDTH, HEIGHT)?;
            let pixels: serde_json::Map<_, _> = PROBES
                .iter()
                .map(|&(node, _, x, y)| {
                    let at = ((y * WIDTH + x) * 4) as usize;
                    (
                        node.to_string(),
                        serde_json::json!(image.pixels[at..at + 4]),
                    )
                })
                .collect();
            samples.push(
                serde_json::json!({"at": at, "pixels": pixels, "animating": view.animating()}),
            );
        }
        let steps: Vec<_> = case
            .steps
            .iter()
            .map(|(at, changes)| {
                serde_json::json!({"at": at, "changes": changes.iter().map(|(node, property, value)| {
                    serde_json::json!({"node": node, "property": property.name(), "value": value})
                }).collect::<Vec<_>>()})
            })
            .collect();
        cases.push(serde_json::json!({
            "name": case.name, "tree": initial, "steps": steps, "samples": samples,
        }));
    }
    let probes: Vec<_> = PROBES
        .iter()
        .map(|(node, kind, x, y)| serde_json::json!({"node": node, "kind": kind, "x": x, "y": y}))
        .collect();
    let output = serde_json::to_vec(&serde_json::json!({
        "viewport": {"width": WIDTH, "height": HEIGHT}, "probes": probes, "cases": cases,
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
