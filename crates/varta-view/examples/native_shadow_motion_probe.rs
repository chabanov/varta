//! Virtual-time shadow motion samples for scripts/test-native-shadow-motion.mjs.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, View};
struct Deny;
impl Broker for Deny {
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
type Change = (P, &'static str);
struct Case {
    name: &'static str,
    first: &'static str,
    steps: &'static [(f64, &'static [Change])],
    samples: &'static [f64],
    tolerance: u8,
}
const CASES: &[Case] = &[
    Case {
        name: "none-to-list",
        first: "transition:box-shadow 100ms linear",
        steps: &[(
            0.0,
            &[(P::BoxShadow, "4px -2px 8px 2px red,inset 8px 0 4px blue")],
        )],
        samples: &[0.0, 25.0, 50.0, 75.0, 100.0],
        tolerance: 13,
    },
    Case {
        name: "list-to-none",
        first: "box-shadow:0 0 0 8px red,inset 8px 0 blue;transition:box-shadow 100ms linear",
        steps: &[(0.0, &[(P::BoxShadow, "none")])],
        samples: &[0.0, 25.0, 50.0, 75.0, 100.0],
        tolerance: 2,
    },
    Case {
        name: "unequal-lists",
        first: "box-shadow:0 0 0 2px red;transition:box-shadow 100ms linear",
        steps: &[(
            0.0,
            &[(
                P::BoxShadow,
                "4px -2px 8px -2px rgba(0,0,255,.5),inset 4px 0 2px blue",
            )],
        )],
        samples: &[0.0, 25.0, 50.0, 100.0],
        tolerance: 13,
    },
    Case {
        name: "inset-mismatch",
        first: "box-shadow:0 0 0 4px red,0 0 0 8px blue;transition:box-shadow 100ms linear",
        steps: &[(0.0, &[(P::BoxShadow, "0 0 0 4px red,inset 4px 0 blue")])],
        samples: &[0.0, 50.0, 100.0],
        tolerance: 2,
    },
    Case {
        name: "reverse",
        first: "transition:box-shadow 100ms linear",
        steps: &[
            (0.0, &[(P::BoxShadow, "0 0 0 8px red")]),
            (40.0, &[(P::BoxShadow, "none")]),
        ],
        samples: &[20.0, 40.0, 60.0, 80.0],
        tolerance: 2,
    },
    Case {
        name: "retarget-cancel",
        first: "transition:box-shadow 100ms linear",
        steps: &[
            (0.0, &[(P::BoxShadow, "0 0 0 8px red")]),
            (40.0, &[(P::BoxShadow, "0 0 0 4px blue")]),
            (100.0, &[(P::TransitionProperty, "none")]),
        ],
        samples: &[20.0, 40.0, 70.0, 100.0, 140.0],
        tolerance: 2,
    },
    Case {
        name: "delay",
        first: "transition:box-shadow 100ms linear -20ms",
        steps: &[(0.0, &[(P::BoxShadow, "0 0 0 8px red")])],
        samples: &[0.0, 30.0, 80.0],
        tolerance: 2,
    },
    Case {
        name: "currentcolor-only",
        first: "color:red;box-shadow:0 0 0 4px;transition:box-shadow 100ms linear",
        steps: &[(0.0, &[(P::Color, "blue")])],
        samples: &[0.0, 50.0, 100.0],
        tolerance: 2,
    },
    Case {
        name: "currentcolor-animated",
        first: "color:red;box-shadow:0 0 0 4px;transition:all 100ms linear",
        steps: &[(0.0, &[(P::Color, "blue")])],
        samples: &[0.0, 25.0, 50.0, 100.0],
        tolerance: 2,
    },
    Case {
        name: "currentcolor-retarget",
        first: "color:red;box-shadow:0 0 0 4px currentcolor;transition:box-shadow 100ms linear",
        steps: &[
            (0.0, &[(P::BoxShadow, "0 0 0 4px blue")]),
            (50.0, &[(P::Color, "lime")]),
            (60.0, &[(P::BoxShadow, "0 0 0 4px currentcolor")]),
        ],
        samples: &[25.0, 50.0, 60.0, 80.0, 120.0],
        tolerance: 2,
    },
    Case {
        name: "keyframes",
        first: "animation:ring 100ms linear forwards",
        steps: &[],
        samples: &[0.0, 25.0, 50.0, 75.0, 100.0],
        tolerance: 2,
    },
    Case {
        name: "keyframes-discrete",
        first: "animation:insets 100ms linear forwards",
        steps: &[],
        samples: &[0.0, 49.0, 50.0, 99.0, 100.0],
        tolerance: 2,
    },
    Case {
        name: "overshoot",
        first: "box-shadow:0 0 8px red;transition:box-shadow 100ms cubic-bezier(.2,0,.8,2)",
        steps: &[(0.0, &[(P::BoxShadow, "0 0 0 blue")])],
        samples: &[25.0, 50.0, 75.0, 100.0],
        tolerance: 13,
    },
];
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scale = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "1".into())
        .parse::<f32>()?;
    if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("invalid scale".into());
    }
    let sheet = "@keyframes ring{50%{box-shadow:0 0 0 8px red}}@keyframes insets{from{box-shadow:0 0 0 4px red}to{box-shadow:inset 4px 0 blue}}";
    let (width, height) = (
        (128.0 * scale).round() as u32,
        (128.0 * scale).round() as u32,
    );
    let mut cases = Vec::new();
    for case in CASES {
        let style = format!(
            "position:absolute;left:40px;top:40px;width:32px;height:24px;{}",
            case.first
        );
        let mut b = B::new();
        b.stylesheet(1, &format!("{sheet}.box{{{style}}}"))
            .create_element(2, Tag::Div)
            .set_class(2, "box")
            .insert(1, 2, 0);
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Deny)?;
        let mut steps = case.steps.iter().peekable();
        let mut samples = Vec::new();
        for &at in case.samples {
            while let Some(&&(time, changes)) = steps.peek()
                && time <= at
            {
                v.set_animation_time(time);
                let mut b = B::new();
                for &(p, value) in changes {
                    b.set_style(2, p, value);
                }
                v.commit(&b.finish(), false, &mut Deny)?;
                steps.next();
            }
            v.set_animation_time(at);
            let image = v.rasterize_scaled(width, height, scale, None)?;
            samples
                .push(serde_json::json!({"at":at,"raster":image.pixels,"animating":v.animating()}));
        }
        cases.push(serde_json::json!({"name":case.name,"style":style,"sheet":sheet,"tolerance":case.tolerance,"steps":case.steps.iter().map(|(at,changes)|serde_json::json!({"at":at,"changes":changes.iter().map(|(p,v)|serde_json::json!({"property":p.name(),"value":v})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"samples":samples}));
    }
    let output = serde_json::json!({"scale":scale,"viewport":{"width":128,"height":128},"rasterSize":[width,height],"cases":cases});
    use std::io::Write;
    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    std::io::stdout().flush()?;
    Ok(())
}
