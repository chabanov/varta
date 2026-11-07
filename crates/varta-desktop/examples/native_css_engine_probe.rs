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
use varta_common::view::{Attribute, Tag};
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
    time: Option<f64>,
    scroll: Option<(u32, f64)>,
    deviation: Option<&'static str>,
    fonts: &'static [Face],
    /// Elements of a class with another tag, attributes, a field value or
    /// a checked state.
    elements: &'static [Extra],
    /// Text children: every element with this class gets this text.
    texts: &'static [(&'static str, &'static str)],
}

/// The tag (by name), attributes, value and checked state of the elements
/// of one class.
struct Extra {
    class: &'static str,
    tag: &'static str,
    attributes: &'static [(Attribute, &'static str)],
    value: Option<&'static str>,
    checked: bool,
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

/// The text-transform fixture (`scripts/generate-style-case-font.py`).
const CASE_FACES: &[Face] = &[Face(
    "Varta Case",
    "crates/varta-style/tests/fixtures/case.ttf",
    "400",
)];

const SCENES: &[Scene] = &[
    Scene {
        name: "backdrop-mirror-edges",
        sheet: ".base{width:160px;height:100px;background-color:red}.inside,.blur{position:absolute;left:40px;top:10px;width:80px;height:80px}.inside{background-color:blue}.blur{backdrop-filter:blur(8px)}",
        nodes: &[Node("base", &[]), Node("inside", &[]), Node("blur", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        texts: &[],
        elements: &[],
    },
    Scene {
        name: "backdrop-mirror-ramp",
        sheet: ".base{width:160px;height:100px;background-color:red}.inside,.blur{position:absolute;left:40px;top:10px;width:80px;height:80px}.inside{background-image:linear-gradient(to right in srgb,blue,green)}.blur{backdrop-filter:blur(8px)}",
        nodes: &[Node("base", &[]), Node("inside", &[]), Node("blur", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        texts: &[],
        elements: &[],
    },
    Scene {
        name: "accent-controls",
        sheet: ".cb,.rd{position:absolute;left:20px;top:20px;width:20px;height:20px;margin:0;accent-color:#4f46e5}.rd{left:60px}",
        nodes: &[Node("cb", &[]), Node("rd", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        texts: &[],
        elements: &[
            Extra {
                class: "cb",
                tag: "input",
                attributes: &[(Attribute::Type, "checkbox")],
                value: None,
                checked: true,
            },
            Extra {
                class: "rd",
                tag: "input",
                attributes: &[(Attribute::Type, "radio")],
                value: None,
                checked: true,
            },
        ],
    },
    Scene {
        name: "linear-gradients",
        sheet: r#".a,.b,.c{width:100px;height:30px;margin:8px;background-image:linear-gradient(90deg in srgb,red,blue)}.b{background-image:linear-gradient(to right in oklab,red,blue)}.c{border-radius:12px;background-image:linear-gradient(to right,transparent,blue);opacity:.75}"#,
        nodes: &[Node("a", &[]), Node("b", &[]), Node("c", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "backdrop-blur",
        sheet: r#".base{width:160px;height:100px;background-image:linear-gradient(to right,red 50%,blue 50%)}.blur{position:absolute;left:40px;top:10px;width:80px;height:80px;backdrop-filter:blur(8px);border-radius:8px;background-color:rgba(255,255,255,.2);opacity:.75}"#,
        nodes: &[Node("base", &[]), Node("blur", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "screen-reader-only",
        sheet: r#".sr{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap;border-width:0;background-color:red}.escape{position:fixed;left:20px;top:20px;width:30px;height:30px;background-color:red}.visible{width:80px;height:40px;background-color:green;table-layout:auto}"#,
        nodes: &[Node("sr", &[Node("escape", &[])]), Node("visible", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "numeric-font-variants",
        sheet: r#".a,.b{font-family:"Varta Numeric Fixture";font-size:20px;line-height:24px;background-color:#ddd;width:max-content}.b{font-variant-numeric:tabular-nums}"#,
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[Face(
            "Varta Numeric Fixture",
            "crates/varta-view/tests/fixtures/numeric.ttf",
            "400",
        )],
        elements: &[],
        texts: &[("a", "010101"), ("b", "010101")],
    },
    Scene {
        name: "line-clamp",
        sheet: r#".a,.b{font-family:"Varta Case";font-size:16px;line-height:20px;width:64px;overflow:hidden;display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2;background-color:#ddd}.b{-webkit-line-clamp:3}"#,
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: CASE_FACES,
        elements: &[],
        texts: &[
            ("a", "aaa aaa aaa aaa aaa aaa aaa"),
            ("b", "aaa aaa aaa aaa aaa aaa aaa"),
        ],
    },
    Scene {
        name: "atomic-inlines",
        sheet: r#".flow{width:140px;font-family:"Varta Case";font-size:16px;line-height:20px}.a,.b{display:inline-block;padding:2px;border:1px solid black;background-color:yellow}.b{display:inline-flex;gap:4px}.cell{background-color:red}"#,
        nodes: &[Node(
            "flow",
            &[
                Node("a", &[Node("cell", &[]), Node("cell", &[])]),
                Node("b", &[Node("cell", &[]), Node("cell", &[])]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: CASE_FACES,
        elements: &[],
        texts: &[("cell", "aaa")],
    },
    Scene {
        name: "sticky-scroll",
        sheet: r#".port{position:relative;width:100px;height:80px;overflow:hidden}.wrapper{height:80px}.space{height:40px}.stick{position:sticky;top:0;height:20px;background-color:red}.tail{height:300px;background-color:#ddd}"#,
        nodes: &[Node(
            "port",
            &[
                Node("wrapper", &[Node("space", &[]), Node("stick", &[])]),
                Node("tail", &[]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: Some((2, 50.0)),
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "pretty-wrap",
        sheet: r#".flow{width:100px;font-family:"Varta Numeric Fixture";font-size:20px;line-height:20px;text-wrap:pretty}"#,
        nodes: &[Node("flow", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[Face(
            "Varta Numeric Fixture",
            "crates/varta-view/tests/fixtures/numeric.ttf",
            "400",
        )],
        elements: &[],
        texts: &[("flow", "111 111 111 111 111 111 111")],
    },
    Scene {
        name: "animation-pulse",
        sheet: r#"@keyframes pulse{50%{opacity:.5}}.a{width:80px;height:40px;background-color:red;animation:pulse 2s cubic-bezier(.4,0,.6,1) infinite}"#,
        nodes: &[Node("a", &[])],
        reduced_motion: false,
        time: Some(1000.0),
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "animation-shadow-list",
        sheet: "@keyframes ring{to{box-shadow:4px -2px 8px 2px red,inset 8px 0 4px blue}}.a{position:absolute;left:40px;top:40px;width:32px;height:24px;animation:ring 100ms linear forwards}",
        nodes: &[Node("a", &[])],
        reduced_motion: false,
        time: Some(50.0),
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "animation-shadow-discrete",
        sheet: "@keyframes ring{from{box-shadow:0 0 0 4px red}to{box-shadow:inset 4px 0 blue}}.a{position:absolute;left:40px;top:40px;width:32px;height:24px;animation:ring 100ms linear forwards}",
        nodes: &[Node("a", &[])],
        reduced_motion: false,
        time: Some(50.0),
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "animation-spin",
        sheet: r#"@keyframes spin{to{transform:rotate(360deg)}}.a{position:absolute;left:40px;top:40px;width:40px;height:20px;background-color:red;border:2px solid blue;animation:spin 1s linear infinite}"#,
        nodes: &[Node("a", &[])],
        reduced_motion: false,
        time: Some(125.0),
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "intrinsic-box-sizes",
        sheet: ".parent { width: 40px; font-family: 'Varta Case'; font-size: 12px; line-height: 16px; padding: 4px } .a { width: min-content; background-color: #f00 } .b { width: max-content; background-color: #0a0 } .c { width: fit-content; background-color: #00f } .d { width: fit-content; padding: 3px; border: 2px solid black } .child { width: 46px; height: 8px; background-color: #c0c }",
        nodes: &[Node(
            "parent",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[Node("child", &[])]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: CASE_FACES,
        elements: &[],
        texts: &[
            ("a", "aaa aaa aaa aaa"),
            ("b", "aaa aaa aaa aaa"),
            ("c", "aaa aaa aaa aaa"),
        ],
    },
    Scene {
        name: "intrinsic-height-and-stretch",
        sheet: ".row{display:flex;gap:8px}.parent{width:60px;height:50px}.parent>div{padding:2px;border:1px solid black;background-color:#ddd}.child{width:8px;height:8px;background-color:red}.a{height:min-content}.b{height:max-content}.c{height:fit-content}.d{width:stretch;height:stretch;margin:3px}",
        nodes: &[Node(
            "row",
            &[
                Node("parent", &[Node("a", &[Node("child", &[])])]),
                Node("parent", &[Node("b", &[Node("child", &[])])]),
                Node("parent", &[Node("c", &[Node("child", &[])])]),
                Node("parent", &[Node("d", &[Node("child", &[])])]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "box-shadows",
        sheet: ".row { display: flex; flex-wrap: wrap; gap: 18px; padding: 18px } .row > div { width: 44px; height: 36px; background-color: #eee } .a { border-radius: 9px; box-shadow: 3px 5px 8px 2px #0007 } .b { box-shadow: 0 0 0 3px #4f46e5, 0 0 0 7px #a5b4fc } .c { border: 3px solid #080; border-radius: 8px 12px 3px 10px; box-shadow: inset 4px 2px 5px 1px #f008 } .d { background-color: transparent !important; border-radius: 40%; box-shadow: -4px 2px 12px -3px #00f9 } .e { opacity: .5; box-shadow: 2px 3px 4px red; border-radius: 5px } .f { transform: scale(1.25); box-shadow: 0 0 4px 2px #0008; border-radius: 8px }",
        nodes: &[Node(
            "row",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[]),
                Node("e", &[]),
                Node("f", &[]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "inline-box-shadows",
        sheet: ".flow { width: 100px; padding: 16px; font-family: 'Varta Case'; font-size: 16px; line-height: 24px } span { background-color: #ddd; border-radius: 6px; padding: 1px 3px; box-shadow: 0 0 0 2px red, 2px 3px 4px #0007, inset 0 1px 2px blue }",
        nodes: &[Node("flow", &[Node("ink", &[])])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: CASE_FACES,
        elements: &[Extra {
            class: "ink",
            tag: "span",
            attributes: &[],
            value: None,
            checked: false,
        }],
        texts: &[("ink", "aaa aaa aaa aaa aaa aaa aaa")],
    },
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "layers",
        sheet: "@layer base, utilities; @layer utilities { .a { background-color: #123456 } } @layer base { .a.a.a { background-color: #ff0000 } .b { background-color: #00ff00 } } .b { background-color: #0000ff } @layer base { @layer reset { .c.c { background-color: #f0f } } .c { background-color: #0ff } } .box { width: 40px; height: 20px; margin: 2px }",
        nodes: &[Node("a box", &[]), Node("b box", &[]), Node("c box", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "registered-properties",
        sheet: "@property --tw-border-style { syntax: \"*\"; inherits: false; initial-value: solid } @property --local { syntax: \"<length>\"; inherits: false; initial-value: 4px } @property --tone { syntax: \"<color>\"; inherits: true; initial-value: #808080 } .outer { --local: 20px; --tone: #ff8800; padding: var(--local); background-color: #eee } .inner { width: 60px; height: 30px; border-width: 3px; border-style: var(--tw-border-style); border-color: var(--tone); padding: var(--local); background-color: var(--tone) } .bad { --tone: 12px; width: 20px; height: 20px; background-color: var(--tone) }",
        nodes: &[Node("outer", &[Node("inner", &[]), Node("bad", &[])])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: Some("CSS Color 4 §14.2 gamut mapping natively; Chrome clips per channel"),
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "reduced-motion",
        sheet: ".a { width: 30px; height: 10px; background-color: #ccc } @media (prefers-reduced-motion: reduce) { .a { background-color: #f00 } } @media (prefers-reduced-motion) { .b { width: 30px; height: 10px; background-color: #00f } }",
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: true,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "comments-and-escapes",
        sheet: "/*! tailwindcss v4.3.5 | MIT License */ .\\61 /* comment */ { width: /* w */ 40px; height: 20px; background-color: #\\31 23 } .b{width:40px;height:20px;background-color:#08f/**/}",
        nodes: &[Node("a", &[]), Node("b", &[])],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: Some(
            "CSS Color 5 §3 mixes one or more colours; Chrome mixes exactly two and drops the declaration",
        ),
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: METRIC_FACES,
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: METRIC_FACES,
        elements: &[],
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "border-sides",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 30px; height: 20px; margin: 4px; background-color: #eee } .a { border-bottom: 3px solid #c00; border-top-width: thin; border-top-style: solid; border-top-color: #00c } .b { border-style: solid none; border-width: 1px 2px 3px; border-color: #111 #2a2 #33c #c4c } .c { border-inline: 2px solid #00c; border-block-start: 4px solid #0a0; border-inline-end-color: #c60 } .d { border: thick solid #888; border-left-style: hidden; border-right-width: medium } .e { border-width: 2px; border-style: solid; border-color: #000; border-bottom-color: #f00 } .f { border-top-width: 6px } .f { border: 1px solid #0aa } .g { --tw-border-style: solid; border-inline-style: var(--tw-border-style); border-inline-width: 3px } .h { border-block: 2px solid #963; border-block-end-width: 5px; border-inline-start: 1px solid #369 }",
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
                Node("h", &[]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "corner-radii",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 48px; height: 32px; margin: 4px; background-color: #36c } .a { border-top-left-radius: 12px } .b { border-radius: 4px; border-bottom-right-radius: 16px 6px } .c { border-start-end-radius: 50%; border-end-start-radius: 10px } .d { border-top-right-radius: 9px } .d { border-radius: 2px 6px } .e { border: 3px solid #c00; border-top-left-radius: 20px 10px; background-color: #fc0 } .f { border-radius: 8px; border-start-start-radius: 0; overflow: hidden } .f > div { width: 60px; height: 40px; background-color: #0a0 }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[]),
                Node("b", &[]),
                Node("c", &[]),
                Node("d", &[]),
                Node("e", &[]),
                Node("f", &[Node("", &[])]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "overflow-axes",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 60px; height: 30px; margin: 8px 30px 30px 8px; padding: 2px; border: 2px solid #888; background-color: #ddd } .w { width: 100px; height: 50px; background-color: #c00 } .a { overflow-x: clip } .b { overflow-y: clip } .c { overflow: hidden visible } .d { overflow: clip } .e { overflow-x: clip; overflow-y: clip } .f { overflow: auto } .g { overflow-y: scroll }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[Node("w", &[])]),
                Node("b", &[Node("w", &[])]),
                Node("c", &[Node("w", &[])]),
                Node("d", &[Node("w", &[])]),
                Node("e", &[Node("w", &[])]),
                Node("f", &[Node("w", &[])]),
                Node("g", &[Node("w", &[])]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "visibility",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 40px; height: 30px; margin: 4px; padding: 3px; border: 2px solid #000; background-color: #c00 } .r div div { width: 16px; height: 10px; background-color: #00c } .a { visibility: hidden } .b { visibility: hidden } .b > .v { visibility: visible } .c { visibility: collapse } .d { visibility: hidden } .d > div { visibility: inherit } .e > div { visibility: hidden } .f { visibility: hidden; outline: 2px solid #0a0 }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[Node("", &[])]),
                Node("b", &[Node("v", &[])]),
                Node("c", &[Node("", &[])]),
                Node("d", &[Node("", &[])]),
                Node("e", &[Node("", &[])]),
                Node("f", &[]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "text-transform",
        sheet: ".r { display: flex; flex-direction: column; align-items: flex-start; font-family: \"Varta Case\"; font-size: 20px; line-height: 30px; color: transparent } .u { text-transform: uppercase; background-color: #fcc } .l { text-transform: lowercase; background-color: #cfc } .c { text-transform: capitalize; background-color: #ccf } .p { text-transform: uppercase } .n { text-transform: none; background-color: #fcf } .i { background-color: #ccc }",
        nodes: &[Node(
            "r",
            &[
                Node("u", &[]),
                Node("l", &[]),
                Node("c", &[]),
                Node("p", &[Node("n", &[]), Node("i", &[])]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: CASE_FACES,
        elements: &[],
        texts: &[
            ("u", "ab-ß"),
            ("l", "AB"),
            ("c", "ab ba-ab"),
            ("n", "ab"),
            ("i", "ab"),
        ],
    },
    Scene {
        name: "attribute-selectors",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 30px; height: 20px; margin: 3px; background-color: #ccc } [role] { background-color: #c00 } [aria-current=\"page\"] { background-color: #0a0 } [aria-label~=b] { border-top: 3px solid #00c } [aria-label^=a][aria-label$=c] { border-bottom: 4px solid #c0c } [aria-label|=en] { background-color: #cc0 } #x { background-color: #0cc } .r > [aria-label=\"X\" i] { width: 40px } .r > [aria-label=\"X\" s] { height: 30px } [aria-hidden] { opacity: 0.5 } [aria-label*=\" \"] { margin-left: 8px }",
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[
            Extra {
                class: "a",
                tag: "div",
                attributes: &[(Attribute::Role, "tab")],
                value: None,
                checked: false,
            },
            Extra {
                class: "b",
                tag: "div",
                attributes: &[(Attribute::AriaCurrent, "page")],
                value: None,
                checked: false,
            },
            Extra {
                class: "c",
                tag: "div",
                attributes: &[(Attribute::AriaLabel, "a b c")],
                value: None,
                checked: false,
            },
            Extra {
                class: "d",
                tag: "div",
                attributes: &[(Attribute::AriaLabel, "en-US")],
                value: None,
                checked: false,
            },
            Extra {
                class: "e",
                tag: "div",
                attributes: &[(Attribute::Id, "x")],
                value: None,
                checked: false,
            },
            Extra {
                class: "f",
                tag: "div",
                attributes: &[(Attribute::AriaLabel, "x")],
                value: None,
                checked: false,
            },
            Extra {
                class: "g",
                tag: "div",
                attributes: &[(Attribute::AriaHidden, "true")],
                value: None,
                checked: false,
            },
        ],
        texts: &[],
    },
    Scene {
        name: "has-selectors",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 40px; min-height: 20px; margin: 3px; background-color: #ccc } .r > div > div { width: 10px; height: 8px; background-color: #888 } .m > div { width: 6px; height: 6px; background-color: #444 } .r > :has(> .k) { background-color: #c00 } .r > :has(.deep) { border: 2px solid #00c } .r > :has(+ .z) { background-color: #0a0 } .r > div:not(:has(div)) { background-color: #cc0 } .r > :has(~ .z) { height: 30px } .z:has(.k, .q) { background-color: #0cc } .r:has(> .e:last-child) > .e { width: 20px }",
        nodes: &[Node(
            "r",
            &[
                Node("a", &[Node("k", &[])]),
                Node("b", &[Node("m", &[Node("deep", &[])])]),
                Node("c", &[]),
                Node("z", &[Node("q", &[])]),
                Node("e", &[]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    Scene {
        name: "input-pseudo-classes",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div, .r > form { width: 40px; height: 20px; margin: 3px; background-color: #ccc } input { display: none } .r > div > div, .r > form > div { width: 12px; height: 12px; background-color: #888 } .p:has(:checked) { background-color: #c00 } .p:has(:indeterminate) { background-color: #00c } input:invalid ~ .v { background-color: #f80 } input:valid ~ .v { background-color: #0a0 } .f:invalid { border: 3px solid #c0c } input:read-write ~ .w { height: 6px } input:optional + div { margin-left: 4px } input:placeholder-shown ~ .s { background-color: #fff }",
        nodes: &[Node(
            "r",
            &[
                Node("p", &[Node("cb", &[])]),
                Node("p", &[Node("rd", &[])]),
                Node("n", &[Node("bad", &[]), Node("v w", &[])]),
                Node("n", &[Node("good", &[]), Node("v", &[])]),
                Node("f", &[Node("half", &[]), Node("v s", &[])]),
                Node("n", &[Node("empty", &[]), Node("s", &[])]),
            ],
        )],
        reduced_motion: false,
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[
            Extra {
                class: "cb",
                tag: "input",
                attributes: &[(Attribute::Type, "checkbox")],
                value: None,
                checked: true,
            },
            Extra {
                class: "rd",
                tag: "input",
                attributes: &[(Attribute::Type, "radio"), (Attribute::Name, "g")],
                value: None,
                checked: false,
            },
            Extra {
                class: "bad",
                tag: "input",
                attributes: &[(Attribute::Type, "number")],
                value: Some("1.5"),
                checked: false,
            },
            Extra {
                class: "good",
                tag: "input",
                attributes: &[(Attribute::Type, "number")],
                value: Some("2"),
                checked: false,
            },
            Extra {
                class: "f",
                tag: "form",
                attributes: &[],
                value: None,
                checked: false,
            },
            Extra {
                class: "half",
                tag: "input",
                attributes: &[(Attribute::Type, "number")],
                value: Some("-0.25"),
                checked: false,
            },
            Extra {
                class: "empty",
                tag: "input",
                attributes: &[(Attribute::Placeholder, "Name")],
                value: None,
                checked: false,
            },
        ],
        texts: &[],
    },
    Scene {
        name: "box-sizing",
        sheet: ".r { display: flex; flex-wrap: wrap; align-items: flex-start } .r > div { width: 40px; height: 20px; padding: 4px; border: 2px solid #000; margin: 4px; background-color: #c60 } .b { box-sizing: border-box } .c { box-sizing: content-box } .d { box-sizing: border-box; width: 50%; padding: 10px }",
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
        texts: &[],
    },
    // CSS Pseudo-Elements 4 §4.1: generated `::before`/`::after` boxes
    // (Tailwind's `after:` underline, block boxes), `content: none`, no
    // `content`, `display: none`, a registered `--tw-content`, and
    // Preflight's error handling (rules and declarations CSS drops).
    Scene {
        name: "generated-boxes",
        sheet: "@property --tw-content { syntax: \"*\"; initial-value: \"\"; inherits: false } \
            :-moz-focusring { outline: auto } hr, abbr { color: #f00 } ::-webkit-search-decoration { -webkit-appearance: none } \
            *, ::after, ::before, ::backdrop, ::file-selector-button { box-sizing: border-box; margin: 0; padding: 0; border: 0 solid } \
            .r { display: flex; flex-direction: column; align-items: flex-start; gap: 16px; padding: 8px } \
            .a { position: relative; width: 120px; height: 24px; background-color: #ddd; -webkit-tap-highlight-color: red } \
            .a::after { content: var(--tw-content); position: absolute; left: 0; bottom: calc(0.25rem * -1); width: 100%; height: 4px; background-color: oklch(51.1% 0.262 276.966) } \
            .b { width: 80px; background-color: #eee } \
            .b::before { content: \"\"; display: block; width: 50%; height: 10px; background-color: #c60 } \
            .b::after { content: \"\"; display: block; height: 6px; background-color: #093; border-top: 2px solid #000 } \
            .c { width: 40px; height: 12px; background-color: #ccc } .c::after { content: none; display: block; height: 20px; background-color: #f00 } \
            .c::before { display: block; height: 20px; background-color: #f00 } \
            .d { width: 40px; height: 12px; background-color: #bbb } .d::after { content: \"\"; display: none; height: 20px; background-color: #f00 }",
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
        time: None,
        scroll: None,
        deviation: None,
        fonts: &[],
        elements: &[],
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
    (nodes, scene): (&[Node], &Scene),
    (next, text_id): (&mut u32, &mut u32),
) {
    for Node(class, children) in nodes {
        let id = *next;
        *next += 1;
        let extra = scene.elements.iter().find(|extra| extra.class == *class);
        let tag = extra.map_or(Tag::Div, |extra| {
            *Tag::ALL
                .iter()
                .find(|tag| tag.name() == extra.tag)
                .expect("scene tag")
        });
        batch
            .create_element(id, tag)
            .set_class(id, class)
            .insert(parent, id, 0);
        if let Some(extra) = extra {
            for (attribute, value) in extra.attributes {
                batch.set_attribute(id, *attribute, value);
            }
            if let Some(value) = extra.value {
                batch.set_value(id, value, 0);
            }
            if extra.checked {
                batch.set_checked(id, 1, 0);
            }
        }
        append(batch, id, (children, scene), (&mut *next, &mut *text_id));
        for (_, text) in scene.texts.iter().filter(|(owner, _)| owner == class) {
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
        if std::env::args()
            .nth(2)
            .is_some_and(|filter| !scene.name.contains(&filter))
        {
            continue;
        }
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
            (scene.nodes, scene),
            (&mut next, &mut text_id),
        );
        view.commit(&batch.finish(), false, &mut Denied)
            .map_err(|error| {
                let parsed = varta_common::view::stylesheet::parse(scene.sheet);
                format!("{}: {error:?} {parsed:?}", scene.name)
            })?;
        if let Some(time) = scene.time {
            view.set_animation_time(time);
        }
        if let Some((node, offset)) = scene.scroll {
            let mut b = B::new();
            b.command(node, varta_common::view::Command::ScrollTo, offset);
            view.commit_with_layout(
                &b.finish(),
                false,
                &mut Denied,
                WIDTH as f32,
                HEIGHT as f32,
                None,
            )?;
        }
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
            "time": scene.time,
            "scroll":scene.scroll,
            "sheet": scene.sheet,
            "nodes": tree(scene.nodes),
            "reducedMotion": scene.reduced_motion,
            "deviation": scene.deviation,
            "texts": scene.texts,
            "elements": scene.elements.iter().map(|extra| serde_json::json!({
                "class": extra.class,
                "tag": extra.tag,
                "attributes": extra.attributes.iter().map(|(attribute, value)| [attribute.name(), value]).collect::<Vec<_>>(),
                "value": extra.value,
                "checked": extra.checked,
            })).collect::<Vec<_>>(),
            "fonts": scene.fonts.iter().map(|Face(family, file, weight)| serde_json::json!({"family": family, "file": file, "weight": weight})).collect::<Vec<_>>(),
            "boxes": boxes,
            "computed": view.snapshot()["tree"],
            "raster": image.pixels,
            "rasterSize": [width, height],
            "edgeProbes": if scene.name == "inline-box-shadows" { vec![(14,26)] } else { vec![] },
            "pixelProbes": match scene.name {
                "backdrop-mirror-edges" => vec![(40,50,1),(44,50,1),(119,50,1)],
                "backdrop-mirror-ramp" => vec![(40,50,6),(44,50,6),(50,50,6),(110,50,6),(119,50,6)],
                "animation-shadow-list" => vec![(38,52,13),(42,52,13),(60,52,2),(76,52,13)],
                "animation-shadow-discrete" => vec![(38,52,1),(42,52,1),(60,52,1)],
                "animation-spin" => vec![(40,34,1),(40,66,1),(60,50,1)],
                "backdrop-blur" => vec![(60,50,6),(79,50,6),(81,50,6),(100,50,6)],
                "linear-gradients" => vec![(20,20,2),(60,20,2),(20,55,2)],
                "box-shadows" => vec![(64,35,12), (78,35,1), (74,35,1), (147,35,12), (204,34,12), (65,92,12), (132,90,12)],
                "inline-box-shadows" => vec![(15,26,4), (40,18,6), (40,42,12), (14,51,12)],
                _ => Vec::new(),
            },
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
