//! K8 T2 scenes for the real-Chrome oracle: escaped class selectors,
//! cascade layers and importance, CSS-wide keywords, conditional rules,
//! registered custom properties, media features, math functions, units and
//! CSS Color 4/5 colours, painted natively at a device scale. The browser
//! host's bridge (or Chrome's own cascade) applies the same stylesheet.
//!
//! cargo run -p varta-view --example native_css_engine_probe -- 2 > target/native-css-2.json
//! node scripts/test-native-css-engine.mjs target/native-css-2.json
use varta_common::view::Tag;
use varta_view::{Access, BatchWriter as B, Broker, Environment, Renderer, View};

/// The CSS viewport of every scene.
const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;

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

/// One element: its classes and children.
struct Node(&'static str, &'static [Node]);

/// A scene: its stylesheet, its elements under the root and, when the
/// colours lie outside sRGB, the reason Chrome's pixels differ.
struct Scene {
    name: &'static str,
    sheet: &'static str,
    nodes: &'static [Node],
    reduced_motion: bool,
    deviation: Option<&'static str>,
}

const SCENES: &[Scene] = &[
    Scene {
        name: "escaped-classes",
        sheet: ".row { display: flex; flex-wrap: wrap } .sm\\:w-1\\/2 { width: calc(1/2 * 100%); height: 10px; background-color: #0a0 } .p-0\\.5 { padding: calc(0.25rem * 0.5) } .\\[bg\\:red\\] { background-color: #f00; height: 6px; width: 30px } .\\32 xl\\:h-4 { height: 1rem; width: 1rem; background-color: #00f }",
        nodes: &[Node(
            "row",
            &[
                Node("sm:w-1/2 p-0.5", &[]),
                Node("[bg:red]", &[]),
                Node("2xl:h-4", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "layers",
        sheet: "@layer base, utilities; @layer utilities { .a { background-color: #123456 } } @layer base { .a.a.a { background-color: #ff0000 } .b { background-color: #00ff00 } } .b { background-color: #0000ff } @layer base { @layer reset { .c.c { background-color: #f0f } } .c { background-color: #0ff } } .box { width: 40px; height: 20px; margin: 2px }",
        nodes: &[Node("a box", &[]), Node("b box", &[]), Node("c box", &[])],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "registered-properties",
        sheet: "@property --tw-border-style { syntax: \"*\"; inherits: false; initial-value: solid } @property --local { syntax: \"<length>\"; inherits: false; initial-value: 4px } @property --tone { syntax: \"<color>\"; inherits: true; initial-value: #808080 } .outer { --local: 20px; --tone: #ff8800; padding: var(--local); background-color: #eee } .inner { width: 60px; height: 30px; border-width: 3px; border-style: var(--tw-border-style); border-color: var(--tone); padding: var(--local); background-color: var(--tone) } .bad { --tone: 12px; width: 20px; height: 20px; background-color: var(--tone) }",
        nodes: &[Node("outer", &[Node("inner", &[]), Node("bad", &[])])],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "theme-colours",
        sheet: ":root { --color-indigo-600: oklch(51.1% 0.262 276.966); --color-emerald-500: oklch(69.6% 0.17 162.48); --color-black: #000; --color-gray-50: oklch(98.5% 0.002 247.839) } .r { display: flex; flex-wrap: wrap; background-color: var(--color-gray-50) } .i { background-color: var(--color-indigo-600) } .e { background-color: var(--color-emerald-500) } .m { background-color: color-mix(in oklab, var(--color-black) 50%, transparent) } .n { background-color: color-mix(in oklab, var(--color-indigo-600) 25%, transparent) } .box { width: 40px; height: 20px; margin: 2px }",
        nodes: &[Node(
            "r",
            &[
                Node("i box", &[]),
                Node("e box", &[]),
                Node("m box", &[]),
                Node("n box", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "colour-functions",
        sheet: ".r { display: flex; flex-wrap: wrap } .a { background-color: rgb(255 0 0 / 50%) } .b { background-color: hsl(210deg 60% 40%) } .c { background-color: hwb(120 10% 20%) } .d { background-color: lab(52% 40 -30) } .e { background-color: lch(60% 50 250) } .f { background-color: oklab(0.6 0.1 -0.1) } .g { background-color: color-mix(in srgb, #f00 30%, #00f) } .j { background-color: #0f08 } .k { background-color: rgb(10%, 20%, 30%, 0.75) } .l { background-color: hsla(0, 100%, 50%, 25%) } .m { background-color: color-mix(#f00 30%, #00f) } .n { background-color: color-mix(in srgb, #f00 0%, #00f 0%) } .box { width: 40px; height: 20px; margin: 2px }",
        nodes: &[Node(
            "r",
            &[
                Node("a box", &[]),
                Node("b box", &[]),
                Node("c box", &[]),
                Node("d box", &[]),
                Node("e box", &[]),
                Node("f box", &[]),
                Node("g box", &[]),
                Node("j box", &[]),
                Node("k box", &[]),
                Node("l box", &[]),
                Node("m box", &[]),
                Node("n box", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "out-of-gamut",
        sheet: ".r { display: flex } .a { background-color: oklch(70% 0.4 150) } .b { background-color: oklch(62.8% 0.258 29.2) } .c { background-color: lab(50% 120 -100) } .d { background-color: color-mix(in oklch, #f00, #00f) } .e { background-color: color-mix(in oklch longer hue, #f00, #00f) } .box { width: 40px; height: 20px; margin: 2px }",
        nodes: &[Node(
            "r",
            &[
                Node("a box", &[]),
                Node("b box", &[]),
                Node("c box", &[]),
                Node("d box", &[]),
                Node("e box", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: Some("CSS Color 4 §14.2 gamut mapping natively; Chrome clips per channel"),
    },
    Scene {
        name: "math-and-units",
        sheet: ":root { --spacing: 0.25rem } .r { display: flex; flex-wrap: wrap; align-items: flex-start } .a { width: calc(var(--spacing) * 10); height: calc(var(--spacing) * 5); margin-top: calc(var(--spacing) * -1); background-color: #c00 } .b { width: 10vw; height: 5vh; background-color: #0c0 } .c { width: min(50px, 10vw); height: max(4px, 1vh); background-color: #00c } .d { width: clamp(10px, 5vw, 30px); height: 0.25in; background-color: #cc0 } .e { width: calc(infinity * 1px); max-width: 24px; height: 12pt; border-radius: calc(infinity * 1px); background-color: #0cc } .f { width: 20px; height: 20px; padding: calc(4px - 10px) calc(2px + 0.0625in); opacity: 50%; background-color: #c0c } .g { width: 30px; height: 10px; margin-left: calc(2px * -3); background-color: #888 }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[]),
                Node("e", &[]),
                Node("f", &[]),
                Node("g", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "media-features",
        sheet: ".a { width: 30px; height: 10px; background-color: #ccc } @media (width >= 20rem) { .a { background-color: #f00 } } @media (320px < width) { .a { background-color: #000 } } @media (orientation: landscape) and (hover: hover) { .b { width: 30px; height: 10px; background-color: #0f0 } } @media not (pointer: coarse) { .c { width: 30px; height: 10px; background-color: #00f } } @media (100px <= height <= 200px) { .d { width: 30px; height: 10px; background-color: #f0f } } @media (prefers-reduced-motion: no-preference), print { .e { width: 30px; height: 10px; background-color: #0ff } } @media (prefers-contrast: no-preference) and (forced-colors: none) { .f { width: 30px; height: 10px; background-color: #ff0 } }",
        nodes: &[
            Node("a", &[]),
            Node("b", &[]),
            Node("c", &[]),
            Node("d", &[]),
            Node("e", &[]),
            Node("f", &[]),
        ],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "reduced-motion",
        sheet: ".a { width: 30px; height: 10px; background-color: #ccc } @media (prefers-reduced-motion: reduce) { .a { background-color: #f00 } } @media (prefers-reduced-motion) { .b { width: 30px; height: 10px; background-color: #00f } }",
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: true,
        deviation: None,
    },
    Scene {
        name: "comments-and-escapes",
        sheet: "/*! tailwindcss v4.3.5 | MIT License */ .\\61 /* comment */ { width: /* w */ 40px; height: 20px; background-color: #\\31 23 } .b{width:40px;height:20px;background-color:#08f/**/}",
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "css-wide-keywords",
        sheet: ".p { display: flex; flex-wrap: wrap; padding: 4px; margin: 3px; color: #c60; background-color: #ddd } .box { width: 30px; height: 12px; margin: 2px; background-color: #f00 } .i { background-color: inherit } .n { background-color: initial } .u { background-color: unset; border: 2px solid; border-color: unset } .v { background-color: revert; outline: 2px solid; outline-color: initial } .m { padding: inherit; margin: initial; background-color: currentcolor } .w { width: inherit; max-width: 50px }",
        nodes: &[Node(
            "p",
            &[
                Node("i box", &[]),
                Node("n box", &[]),
                Node("u box", &[]),
                Node("v box", &[]),
                Node("m box", &[]),
                Node("w box", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "importance",
        sheet: "@layer low, high; .r { display: flex; flex-wrap: wrap } .box { width: 30px; height: 12px; margin: 2px } .a { background-color: #f00 !important } .a.a.a { background-color: #00f } @layer low { .b { background-color: #0f0 !important } } @layer high { .b { background-color: #f00 !important } } .b { background-color: #00f !important } @layer high { .c { background-color: #f00 } } .c { background-color: #0aa } .d { background-color: #f00 ! important; background-color: #808 } .e { opacity: 2; background-color: #08f } .f { opacity: -1; background-color: #f00 }",
        nodes: &[Node(
            "r",
            &[
                Node("a box", &[]),
                Node("b box", &[]),
                Node("c box", &[]),
                Node("d box", &[]),
                Node("e box", &[]),
                Node("f box", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "conditional-rules",
        sheet: ".r { display: flex; flex-wrap: wrap } .box { width: 30px; height: 12px; margin: 2px; background-color: #ccc } @supports (display: grid) and (not (color: nonsense)) { .a { background-color: #0a0 } } @supports (unknown-property: 1) or (width: frobnicate) { .b { background-color: #f00 } } @supports selector(.x:hover) { .c { background-color: #00a } } @media (width < 100px) { @layer narrow { .d { background-color: #f00 } } } @layer wide { .d { background-color: #a0a } } @layer narrow { .d { background-color: #0aa } } @media (width >= 100px) { @layer late { .e { background-color: #aa0 } } } @layer early { .e { background-color: #f00 } } @supports (width: 1px) { @media (orientation: landscape) { .f { background-color: #555 } } }",
        nodes: &[Node(
            "r",
            &[
                Node("a box", &[]),
                Node("b box", &[]),
                Node("c box", &[]),
                Node("d box", &[]),
                Node("e box", &[]),
                Node("f box", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "named-colours-and-mixes",
        sheet: ".r { display: flex; flex-wrap: wrap } .box { width: 30px; height: 12px; margin: 2px } .a { background-color: rebeccapurple } .b { background-color: CornflowerBlue } .c { background-color: transparent; border: 3px solid darkolivegreen } .d { background-color: color-mix(in srgb, red, blue 25%) } .e { background-color: color-mix(in oklab, red 60%, blue 60%) } .f { background-color: color-mix(in srgb, red 20%, blue 20%) } .g { background-color: color-mix(in hsl, teal, gold) } .h { background-color: color-mix(in srgb, transparent, navy) }",
        nodes: &[Node(
            "r",
            &[
                Node("a box", &[]),
                Node("b box", &[]),
                Node("c box", &[]),
                Node("d box", &[]),
                Node("e box", &[]),
                Node("f box", &[]),
                Node("g box", &[]),
                Node("h box", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "multi-colour-mixes",
        sheet: ".r { display: flex; flex-wrap: wrap } .box { width: 30px; height: 12px; margin: 2px } .a { background-color: color-mix(in srgb, red, lime 20%, blue) } .b { background-color: color-mix(in oklch, red 10%, lime 10%, blue 10%) } .c { background-color: color-mix(in oklab, teal) }",
        nodes: &[Node(
            "r",
            &[Node("a box", &[]), Node("b box", &[]), Node("c box", &[])],
        )],
        reduced_motion: false,
        deviation: Some(
            "CSS Color 5 §3 mixes one or more colours; Chrome mixes exactly two and drops the declaration",
        ),
    },
    Scene {
        name: "font-relative-units",
        sheet: "@property --gap { syntax: \"<length>\"; inherits: true; initial-value: 0px } .r { display: flex; flex-wrap: wrap; align-items: flex-start; font-size: 20px; line-height: 1.5; --gap: 1em } .a { width: 2em; height: 1lh; background-color: #c00 } .b { font-size: 0.5em; width: 4em; height: 2lh; margin-left: var(--gap); background-color: #0a0 } .c { font-size: calc(1em + 4px); line-height: calc(1lh - 6px); width: 1em; height: 1lh; padding: 0.25em; background-color: #00c } .d { font-size: 150%; width: 1em; height: 0.5em; border: 0.1em solid #000; background-color: #cc0 }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
    Scene {
        name: "percentage-length-math",
        sheet: ":root { --gutter: 1rem } .r { width: 300px; height: 160px; padding: 10px; background-color: #eee } .a { width: calc(50% - 20px); height: calc(25% + 0.5em); margin-left: calc(10% + 5px); background-color: #c00 } .b { width: min(80%, calc(100% - 4rem)); height: 20px; padding-top: calc(5% + 2px); background-color: #0a0 } .c { width: calc(100% - var(--gutter) * 2); max-width: calc(70% + 1em); height: 10px; margin-top: calc(2% - 1px); background-color: #00c } .d { width: calc(10% - 50px); height: 8px; min-width: calc(20% + 1px); background-color: #cc0 }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
    },
];

fn append(batch: &mut B, parent: u32, nodes: &[Node], next: &mut u32) {
    for Node(class, children) in nodes {
        let id = *next;
        *next += 1;
        batch
            .create_element(id, Tag::Div)
            .set_class(id, class)
            .insert(parent, id, 0);
        append(batch, id, children, next);
    }
}

fn tree(nodes: &[Node]) -> serde_json::Value {
    serde_json::Value::Array(
        nodes
            .iter()
            .map(|Node(class, children)| serde_json::json!({"class": class, "children": tree(children)}))
            .collect(),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scale: f32 = std::env::args().nth(1).map_or(Ok(1.0), |arg| arg.parse())?;
    let mut renderer = Renderer::new(None, 16.0)?;
    let mut cases = Vec::new();
    for scene in SCENES {
        let mut environment = Environment {
            width: f64::from(WIDTH),
            height: f64::from(HEIGHT),
            ..Environment::default()
        };
        environment.preferences.reduced_motion = scene.reduced_motion;
        let mut view = View::default();
        view.set_environment(environment)?;
        let mut batch = B::new();
        batch.stylesheet(1, scene.sheet);
        let mut next = 2;
        append(&mut batch, 1, scene.nodes, &mut next);
        view.commit(&batch.finish(), false, &mut Denied)
            .map_err(|error| {
                let parsed = varta_common::view::stylesheet::parse(scene.sheet);
                format!("{}: {error:?} {parsed:?}", scene.name)
            })?;
        let boxes = view
            .layout_boxes(WIDTH as f32, HEIGHT as f32)?
            .into_iter()
            .filter(|(id, _)| *id != 1)
            .map(|(id, r)| {
                (
                    id.to_string(),
                    serde_json::json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let width = (WIDTH as f32 * scale).round() as u32;
        let height = (HEIGHT as f32 * scale).round() as u32;
        let image = renderer.render(&view, width, height, scale)?;
        cases.push(serde_json::json!({
            "name": scene.name,
            "sheet": scene.sheet,
            "nodes": tree(scene.nodes),
            "reducedMotion": scene.reduced_motion,
            "deviation": scene.deviation,
            "boxes": boxes,
            "computed": view.snapshot()["tree"],
            "raster": image.pixels,
            "rasterSize": [width, height],
        }));
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "scale": scale,
        "viewport": {"width": WIDTH, "height": HEIGHT},
        "cases": cases,
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
