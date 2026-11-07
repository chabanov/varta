//! K8 T2 scenes for the real-Chrome oracle: escaped class selectors,
//! cascade layers and importance, CSS-wide keywords, conditional rules,
//! registered custom properties, media features, math functions, units,
//! the font-relative units of the platform's and packaged fonts, and CSS
//! Color 4/5 colours, painted natively at a device scale. The browser
//! host's bridge (or Chrome's own cascade) applies the same stylesheet.
//! The platform measures its own fonts (`varta_desktop::shell::host_fonts`)
//! as the shell does, so this probe lives with the native host.
//!
//! cargo run -p varta-desktop --example native_css_engine_probe -- 2 > target/native-css-2.json
//! node scripts/test-native-css-engine.mjs target/native-css-2.json
use std::sync::Arc;
use varta_common::view::Tag;
use varta_style::fonts::FontSet;
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

/// A scene: its stylesheet, its elements under the root, its packaged
/// faces and, when the colours lie outside sRGB, the reason Chrome's
/// pixels differ.
struct Scene {
    name: &'static str,
    sheet: &'static str,
    nodes: &'static [Node],
    reduced_motion: bool,
    deviation: Option<&'static str>,
    fonts: &'static [Face],
    /// Text children: every element with this class gets this text.
    texts: &'static [(&'static str, &'static str)],
}

/// The first id of text nodes, apart from the elements whose boxes are
/// compared.
const TEXT_IDS: u32 = 1000;

/// A packaged face: its family, its file (relative to the workspace) and
/// its `FontFace` weight descriptor.
struct Face(&'static str, &'static str, &'static str);

/// The style engine's metric fixtures (`scripts/generate-style-metrics-fonts.py`).
const METRIC_FACES: &[Face] = &[
    Face(
        "Varta Metrics",
        "crates/varta-style/tests/fixtures/metrics.ttf",
        "400",
    ),
    Face(
        "Varta Metrics",
        "crates/varta-style/tests/fixtures/metrics-bold.ttf",
        "700",
    ),
    Face(
        "Varta Metrics Variable",
        "crates/varta-style/tests/fixtures/metrics-variable.ttf",
        "100 900",
    ),
];

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
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "layers",
        sheet: "@layer base, utilities; @layer utilities { .a { background-color: #123456 } } @layer base { .a.a.a { background-color: #ff0000 } .b { background-color: #00ff00 } } .b { background-color: #0000ff } @layer base { @layer reset { .c.c { background-color: #f0f } } .c { background-color: #0ff } } .box { width: 40px; height: 20px; margin: 2px }",
        nodes: &[Node("a box", &[]), Node("b box", &[]), Node("c box", &[])],
        reduced_motion: false,
        deviation: None,
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "registered-properties",
        sheet: "@property --tw-border-style { syntax: \"*\"; inherits: false; initial-value: solid } @property --local { syntax: \"<length>\"; inherits: false; initial-value: 4px } @property --tone { syntax: \"<color>\"; inherits: true; initial-value: #808080 } .outer { --local: 20px; --tone: #ff8800; padding: var(--local); background-color: #eee } .inner { width: 60px; height: 30px; border-width: 3px; border-style: var(--tw-border-style); border-color: var(--tone); padding: var(--local); background-color: var(--tone) } .bad { --tone: 12px; width: 20px; height: 20px; background-color: var(--tone) }",
        nodes: &[Node("outer", &[Node("inner", &[]), Node("bad", &[])])],
        reduced_motion: false,
        deviation: None,
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "reduced-motion",
        sheet: ".a { width: 30px; height: 10px; background-color: #ccc } @media (prefers-reduced-motion: reduce) { .a { background-color: #f00 } } @media (prefers-reduced-motion) { .b { width: 30px; height: 10px; background-color: #00f } }",
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: true,
        deviation: None,
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "comments-and-escapes",
        sheet: "/*! tailwindcss v4.3.5 | MIT License */ .\\61 /* comment */ { width: /* w */ 40px; height: 20px; background-color: #\\31 23 } .b{width:40px;height:20px;background-color:#08f/**/}",
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: false,
        deviation: None,
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
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
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "platform-font-units",
        sheet: ".r { position: relative; width: 320px; height: 200px } .a { position: absolute; left: 0; top: 0; width: 10ex; height: 2cap; background-color: #c00 } .b { position: absolute; left: 80px; top: 0; width: 5ch; height: 1lh; background-color: #0a0 } .c { position: absolute; left: 130px; top: 0; width: 2ic; height: calc(1rex + 1rcap); margin-top: 1rlh; background-color: #00c } .s { position: absolute; left: 170px; top: 0; font-family: sans-serif; width: 6ex; height: 1lh; background-color: #cc0 } .m { position: absolute; left: 230px; top: 0; font-family: monospace; width: 4ch; height: 1lh; background-color: #0cc } .u { position: absolute; left: 0; top: 60px; font-family: system-ui; font-size: 13px; width: 5ch; height: 1lh; background-color: #c0c } .w { position: absolute; left: 50px; top: 60px; font-family: system-ui; font-weight: bold; width: 5ch; height: 3ex; background-color: #888 } .i { position: absolute; left: 100px; top: 60px; font-family: serif; font-style: italic; font-size: 30px; width: 3cap; height: 1ex; background-color: #c60 } .n { position: absolute; left: 170px; top: 60px; font-family: Missing, sans-serif; font-size: 100px; width: 1ch; height: 0.5lh; background-color: #06c }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("s", &[]),
                Node("m", &[]),
                Node("u", &[]),
                Node("w", &[]),
                Node("i", &[]),
                Node("n", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "font-metric-references",
        sheet: "@property --gap { syntax: \"<length>\"; inherits: true; initial-value: 0px } .r { position: relative; width: 320px; height: 200px; font-family: sans-serif; font-size: 20px; --gap: 1ch } .a { position: absolute; left: 0; top: 0; font-size: 2ex; width: 1em; height: 1lh; margin-left: var(--gap); background-color: #c00 } .b { position: absolute; left: 60px; top: 0; line-height: 3ex; width: 2cap; height: 1lh; background-color: #0a0 } .c { position: absolute; left: 0; top: 80px; font-family: monospace; width: calc(50% - 4ch); height: calc(1lh + 1rlh); background-color: #00c } .d { position: absolute; left: 200px; top: 0; font-size: 1.5em; font-family: system-ui; line-height: calc(1ex + 2ch); width: 1lh; height: 2ic; background-color: #cc0 }",
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
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "packaged-font-units",
        sheet: ".r { position: relative; width: 320px; height: 200px; font-family: \"Varta Metrics\", serif } .a { position: absolute; left: 0; top: 0; width: 10ex; height: 1lh; background-color: #c00 } .b { position: absolute; left: 100px; top: 0; font-weight: bold; width: 5ch; height: 1lh; background-color: #0a0 } .c { position: absolute; left: 160px; top: 0; width: 2ic; height: 2cap; background-color: #00c } .d { position: absolute; left: 200px; top: 0; font-weight: 600; font-size: 13px; width: 3ex; height: 3cap; background-color: #cc0 } .v { position: absolute; left: 0; top: 60px; font-family: \"Varta Metrics Variable\"; font-weight: 500; width: 4ch; height: 3ex; background-color: #0cc } .w { position: absolute; left: 60px; top: 60px; font-family: \"Varta Metrics Variable\"; font-weight: 900; font-size: 40px; width: 1ch; height: 1ex; background-color: #c0c } .x { position: absolute; left: 120px; top: 60px; font-family: Missing, \"Varta Metrics\"; font-size: 3ex; width: 1ch; height: 1lh; background-color: #888 }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[]),
                Node("v", &[]),
                Node("w", &[]),
                Node("x", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
        fonts: METRIC_FACES,
        texts: &[],
    },
    Scene {
        name: "logical-and-mapped",
        sheet: ":root { --pair: 6px 2px } .r { position: relative; width: 320px; height: 200px } .a { position: absolute; left: 0; top: 0; padding-inline: 4px 10px; padding-block: 2px; margin-inline: 3px; width: 20px; height: 10px; background-color: #c00 } .b { position: absolute; inset: 40px 200px 120px 30px; background-color: #0a0 } .c { position: absolute; top: 100px; inset-inline: 10px 250px; height: 8px; background-color: #00c } .d { position: absolute; left: 100px; top: 0; padding-inline: var(--pair); padding-block-start: calc(1rem * 0.5); margin-block-end: 4px; width: 10px; height: 10px; background-color: #cc0 } .e { position: absolute; left: 150px; top: 0; padding-left: 9px; padding-inline-start: 1px; width: 10px; height: 10px; background-color: #0cc } .f { position: absolute; left: 0; top: 150px; width: 300px; display: flex } .f1 { flex: 1; height: 10px; background-color: #c0c } .f2 { flex: 2 1 20px; height: 10px; background-color: #888 } .f3 { flex: none; width: 30px; height: 10px; background-color: #c60 } .g { position: absolute; top: 120px; left: 200px; width: 40px; height: 10px; margin-inline-start: auto; inset-block-end: auto; background-color: #06c }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[]),
                Node("e", &[]),
                Node("f", &[Node("f1", &[]), Node("f2", &[]), Node("f3", &[])]),
                Node("g", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "individual-transforms",
        sheet: ".r { position: relative; width: 320px; height: 200px } .t { position: absolute; left: 10px; top: 10px; width: 40px; height: 20px; translate: 10px 50%; background-color: #c00 } .s { position: absolute; left: 100px; top: 10px; width: 40px; height: 40px; scale: 50%; transform-origin: top right; background-color: #0a0 } .u { position: absolute; left: 200px; top: 10px; width: 40px; height: 40px; translate: -4px 2px; scale: 1.5 0.5; transform: translateX(3px); background-color: #00c } .v { position: absolute; left: 10px; top: 100px; width: 60px; height: 30px; --x: 8px; translate: var(--x); scale: 1 1.25; transform-origin: 0 100%; background-color: #cc0 } .w { position: absolute; left: 120px; top: 100px; width: 40px; height: 40px; scale: 0.75; transform-origin: left; background-color: #0cc }",
        nodes: &[Node(
            "r",
            &[
                Node("t", &[]),
                Node("s", &[]),
                Node("u", &[]),
                Node("v", &[]),
                Node("w", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
        fonts: &[],
        texts: &[],
    },
    Scene {
        name: "letter-spacing",
        sheet: ".r { display: flex; flex-direction: column; align-items: flex-start; font-family: \"Varta Metrics\"; font-size: 20px; line-height: 30px; color: transparent } .a { letter-spacing: 3px; background-color: #fcc } .b { letter-spacing: -0.1em; background-color: #cfc } .c { letter-spacing: 0.25rem; font-size: 10px; background-color: #ccf } .d { background-color: #ffc } .e { letter-spacing: normal; background-color: #cff }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("e", &[]),
                Node("d", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
        fonts: METRIC_FACES,
        texts: &[
            ("a", "AHx 0A"),
            ("b", "AHx 0A"),
            ("c", "AH"),
            ("e", "x0"),
            ("d", "AHx 0A"),
        ],
    },
    Scene {
        name: "complex-selectors",
        sheet: ":root, :host { --c: #c00 } .r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 30px; height: 12px; margin: 2px; background-color: #ccc } .r > :first-child { background-color: var(--c) } .r > :nth-child(3n+2) { background-color: #0a0 } .r > div:last-child { background-color: #00c } :where(.r > :not(:last-child)) { margin-right: 6px } .x + .y { background-color: #cc0 } .x ~ .z { height: 20px } .g .inner { width: 8px; height: 6px; background-color: #0cc } :is(.a, .b.c) div { width: 10px; height: 6px; background-color: #f80 } .r .q:only-child { background-color: #808 } div:not(.p):not(.x):nth-of-type(even) { border: 2px solid #000 } .r > .p:nth-last-child(1) { opacity: 0.5 }",
        nodes: &[Node(
            "r",
            &[
                Node("p", &[]),
                Node("x", &[]),
                Node("y", &[]),
                Node("p", &[]),
                Node("z", &[]),
                Node("g", &[Node("inner", &[])]),
                Node("a", &[Node("q", &[])]),
                Node("p", &[]),
            ],
        )],
        reduced_motion: false,
        deviation: None,
        fonts: &[],
        texts: &[],
    },
];

/// A text measurer over the scene's packaged faces.
fn font_measurer(scene: &Scene) -> Result<varta_view::TextMeasurer, Box<dyn std::error::Error>> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let fonts = scene
        .fonts
        .iter()
        .map(|Face(family, file, _)| {
            Ok(varta_view::TextFont {
                data: std::fs::read(format!("{root}/{file}"))?,
                family: (*family).to_owned(),
                aliases: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    Ok(varta_view::TextMeasurer::new_with_packaged_fonts(fonts)?)
}

/// The view's fonts: the scene's packaged faces, the platform for the rest.
fn font_set(scene: &Scene) -> Result<FontSet, Box<dyn std::error::Error>> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut set = FontSet::new(None, varta_desktop::shell::host_fonts(Vec::new()));
    for Face(family, file, _) in scene.fonts {
        let bytes = std::fs::read(format!("{root}/{file}"))?;
        set.add_file(family, &[], &bytes)
            .map_err(|error| format!("{file}: {error:?}"))?;
    }
    Ok(set)
}

fn append(
    batch: &mut B,
    parent: u32,
    (nodes, texts): (&[Node], &[(&str, &str)]),
    (next, text_id): (&mut u32, &mut u32),
) {
    for Node(class, children) in nodes {
        let id = *next;
        *next += 1;
        batch
            .create_element(id, Tag::Div)
            .set_class(id, class)
            .insert(parent, id, 0);
        append(batch, id, (children, texts), (&mut *next, &mut *text_id));
        for (_, text) in texts.iter().filter(|(owner, _)| owner == class) {
            batch.create_text(*text_id, text).insert(id, *text_id, 0);
            *text_id += 1;
        }
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
    let mut cases = Vec::new();
    for scene in SCENES {
        // A scene with text lays it out and draws it with its own faces.
        let mut measurer = match &scene.texts {
            [] => None,
            _ => Some(font_measurer(scene)?),
        };
        let mut renderer = Renderer::new(
            match &scene.texts {
                [] => None,
                _ => Some(font_measurer(scene)?),
            },
            16.0,
        )?;
        let mut environment = Environment {
            width: f64::from(WIDTH),
            height: f64::from(HEIGHT),
            ..Environment::default()
        };
        environment.preferences.reduced_motion = scene.reduced_motion;
        let mut view = View::default();
        view.set_fonts(Arc::new(font_set(scene)?));
        view.set_environment(environment)?;
        let mut batch = B::new();
        batch.stylesheet(1, scene.sheet);
        let (mut next, mut text_id) = (2, TEXT_IDS);
        append(
            &mut batch,
            1,
            (scene.nodes, scene.texts),
            (&mut next, &mut text_id),
        );
        view.commit(&batch.finish(), false, &mut Denied)
            .map_err(|error| {
                let parsed = varta_common::view::stylesheet::parse(scene.sheet);
                format!("{}: {error:?} {parsed:?}", scene.name)
            })?;
        let boxes = match measurer.as_mut() {
            Some(measurer) => {
                view.layout_boxes_with_text(WIDTH as f32, HEIGHT as f32, measurer, 16.0)?
            }
            None => view.layout_boxes(WIDTH as f32, HEIGHT as f32)?,
        };
        let boxes = boxes
            .into_iter()
            .filter(|(id, _)| *id != 1 && *id < TEXT_IDS)
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
            "texts": scene.texts,
            "fonts": scene.fonts.iter().map(|Face(family, file, weight)| serde_json::json!({"family": family, "file": file, "weight": weight})).collect::<Vec<_>>(),
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
