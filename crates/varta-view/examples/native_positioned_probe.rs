//! Positioned boxes, z-index stacking, opacity contexts, translations, clips
//! and scrolled ancestors, plus Chrome's minimum logical font size, for the real-Chrome
//! geometry, hit, scroll-into-view and pixel oracle in
//! `scripts/test-native-positioned.mjs`.
//! Usage: native_positioned_probe [SCALE]
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
/// The viewport in CSS pixels.
const SIDE: u32 = 80;
/// Chrome's initial `medium` font size, handed to the measurer as the host size.
const MEDIUM: f32 = 16.0;
/// Pointer samples every this many CSS pixels.
const HIT_STEP: usize = 3;
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.0;
/// Prefix of the cases that start from the initial `medium` font size.
const MINIMUM_FONT_CASES: &str = "minimum-font";

const GRAY: &str = "#c0c0c0";
const RED: &str = "#e00000";
const GREEN: &str = "#00a000";
const BLUE: &str = "#0000e0";
const YELLOW: &str = "#e0c000";
const MAGENTA: &str = "#c000c0";
const CYAN: &str = "#00b0b0";
const DARK: &str = "#202020";
/// Below one, so the box is a stacking context, while staying near opaque.
const TRANSLUCENT: &str = "0.99";

type Styles<'a> = &'a [(P, &'a str)];

/// A `div` `id` appended to `parent` with `styles`.
fn div(b: &mut B, id: u32, parent: u32, styles: Styles) {
    b.create_element(id, Tag::Div).insert(parent, id, 0);
    for &(property, value) in styles {
        b.set_style(id, property, value);
    }
}

/// An absolutely positioned square box.
fn square(b: &mut B, id: u32, parent: u32, at: (&str, &str), side: &str, color: &str) {
    div(
        b,
        id,
        parent,
        &[
            (P::Position, "absolute"),
            (P::Top, at.0),
            (P::Left, at.1),
            (P::Width, side),
            (P::Height, side),
            (P::BackgroundColor, color),
        ],
    );
}

struct Case {
    name: &'static str,
    /// Programmatic scroll offsets applied after the tree is built. Chrome
    /// snaps scrolled paint to device pixels, so offsets stay whole device
    /// pixels at the compared scales.
    scrolls: &'static [(u32, f64)],
    /// Nearest-edge scroll-into-view target compared with Chrome.
    reveal: Option<u32>,
}

fn build(name: &str, b: &mut B) {
    match name {
        // Positioned boxes paint above later in-flow blocks and their text.
        "relative-flow" => {
            let red = [
                (P::Position, "relative"),
                (P::Top, "10px"),
                (P::Left, "10px"),
                (P::Width, "40px"),
                (P::Height, "20px"),
                (P::BackgroundColor, RED),
            ];
            div(b, 2, 1, &red);
            div(
                b,
                3,
                1,
                &[
                    (P::Width, "60px"),
                    (P::Height, "20px"),
                    (P::MarginTop, "-10px"),
                    (P::BackgroundColor, BLUE),
                    (P::Color, YELLOW),
                ],
            );
            b.create_text(4, "AAA").insert(3, 4, 0);
            div(
                b,
                5,
                1,
                &[
                    (P::Position, "relative"),
                    (P::Top, "-25px"),
                    (P::Left, "30px"),
                    (P::Width, "30px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
        }
        // z-index levels, then tree order; a negative layer of the root
        // context paints below the auto-positioned container's background.
        "z-index" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Position, "relative"),
                    (P::Width, "70px"),
                    (P::Height, "70px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            for (id, z, at, color) in [
                (3, "2", "5px", RED),
                (4, "1", "15px", BLUE),
                (5, "auto", "25px", GREEN),
                (6, "0", "35px", YELLOW),
            ] {
                square(b, id, 2, (at, at), "30px", color);
                b.set_style(id, P::ZIndex, z);
            }
            square(b, 7, 2, ("0px", "60px"), "20px", MAGENTA);
            b.set_style(7, P::ZIndex, "-1");
        }
        // A real context paints its background, negative layers by z-index,
        // then in-flow blocks.
        "negative-context" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Position, "relative"),
                    (P::ZIndex, "0"),
                    (P::MarginLeft, "10px"),
                    (P::Width, "60px"),
                    (P::Height, "60px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            square(b, 3, 2, ("5px", "5px"), "40px", RED);
            b.set_style(3, P::ZIndex, "-1");
            square(b, 4, 2, ("15px", "15px"), "40px", BLUE);
            b.set_style(4, P::ZIndex, "-2");
            div(
                b,
                5,
                2,
                &[
                    (P::MarginTop, "20px"),
                    (P::MarginLeft, "20px"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
        }
        // Fixed and initial-containing-block absolute boxes ignore an
        // unpositioned scrolled ancestor's offset and clip.
        "fixed-scroll" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Overflow, "hidden"),
                    (P::Width, "60px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            div(b, 3, 2, &[(P::Height, "30px"), (P::BackgroundColor, RED)]);
            div(b, 4, 2, &[(P::Height, "30px"), (P::BackgroundColor, BLUE)]);
            div(
                b,
                5,
                2,
                &[
                    (P::Position, "fixed"),
                    (P::Top, "30px"),
                    (P::Left, "30px"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
            div(
                b,
                6,
                2,
                &[
                    (P::Position, "absolute"),
                    (P::Top, "5px"),
                    (P::Left, "50px"),
                    (P::Width, "20px"),
                    (P::Height, "50px"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
        }
        // An absolute box moves with its scrolled containing block, not with a
        // scrolled unpositioned box between them, and only the containing
        // block clips it.
        "absolute-scroll" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Position, "relative"),
                    (P::Overflow, "hidden"),
                    (P::Width, "70px"),
                    (P::Height, "50px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            div(b, 3, 2, &[(P::Height, "30px"), (P::BackgroundColor, RED)]);
            div(
                b,
                4,
                2,
                &[
                    (P::Overflow, "hidden"),
                    (P::Width, "50px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            div(
                b,
                5,
                4,
                &[
                    (P::Width, "40px"),
                    (P::Height, "50px"),
                    (P::BackgroundColor, CYAN),
                ],
            );
            square(b, 6, 4, ("30px", "40px"), "20px", GREEN);
            b.set_style(6, P::Height, "30px");
            div(
                b,
                7,
                2,
                &[(P::Height, "40px"), (P::BackgroundColor, YELLOW)],
            );
            square(b, 8, 4, ("60px", "0px"), "10px", MAGENTA);
            b.set_style(8, P::Width, "20px");
        }
        // Opacity groups its z-indexed descendants below a later z-index 1
        // sibling; an auto-positioned box stays at level zero.
        "opacity-context" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Opacity, "0.5"),
                    (P::Width, "40px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, RED),
                ],
            );
            div(
                b,
                3,
                2,
                &[
                    (P::Position, "relative"),
                    (P::ZIndex, "5"),
                    (P::Top, "10px"),
                    (P::Left, "10px"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            div(
                b,
                4,
                1,
                &[
                    (P::Position, "relative"),
                    (P::ZIndex, "1"),
                    (P::Top, "-25px"),
                    (P::Left, "25px"),
                    (P::Width, "40px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
            div(
                b,
                5,
                1,
                &[
                    (P::Position, "relative"),
                    (P::Top, "-60px"),
                    (P::Left, "5px"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
        }
        // A rounded positioned clip applies to absolute descendants through an
        // unpositioned clipping box, which does not clip them; fixed boxes
        // escape every clip.
        "clips" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Position, "relative"),
                    (P::Overflow, "hidden"),
                    (P::BorderRadius, "20px"),
                    (P::Margin, "10px"),
                    (P::Width, "60px"),
                    (P::Height, "60px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            square(b, 3, 2, ("-10px", "-10px"), "40px", RED);
            div(
                b,
                4,
                2,
                &[
                    (P::Overflow, "hidden"),
                    (P::BorderRadius, "15px"),
                    (P::MarginLeft, "30px"),
                    (P::MarginTop, "30px"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            square(b, 5, 4, ("10px", "10px"), "60px", GREEN);
            b.set_style(5, P::Height, "10px");
            div(
                b,
                6,
                4,
                &[
                    (P::Position, "fixed"),
                    (P::Top, "0px"),
                    (P::Left, "0px"),
                    (P::Width, "15px"),
                    (P::Height, "15px"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
        }
        // z-index applies to static flex items; a negative item paints below
        // the unpositioned container's background.
        "flex-items" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Display, "flex"),
                    (P::Width, "70px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            div(
                b,
                3,
                2,
                &[
                    (P::ZIndex, "1"),
                    (P::MarginRight, "-10px"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, RED),
                ],
            );
            div(
                b,
                4,
                2,
                &[
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            div(
                b,
                5,
                2,
                &[
                    (P::ZIndex, "-1"),
                    (P::MarginLeft, "-15px"),
                    (P::Width, "20px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
            square(b, 6, 2, ("30px", "50px"), "20px", YELLOW);
        }
        // Fixed toasts with text: z-index orders a translucent overlay below a
        // later-painted toast over in-flow text.
        "fixed-text" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Width, "80px"),
                    (P::Height, "60px"),
                    (P::BackgroundColor, GRAY),
                    (P::Color, BLUE),
                ],
            );
            b.create_text(3, "AA AA").insert(2, 3, 0);
            div(
                b,
                4,
                1,
                &[
                    (P::Position, "fixed"),
                    (P::ZIndex, "20"),
                    (P::Right, "4px"),
                    (P::Bottom, "4px"),
                    (P::Width, "50px"),
                    (P::Height, "24px"),
                    (P::BackgroundColor, DARK),
                    (P::Color, "#ffffff"),
                ],
            );
            b.create_text(5, "AA").insert(4, 5, 0);
            div(
                b,
                6,
                1,
                &[
                    (P::Position, "fixed"),
                    (P::ZIndex, "10"),
                    (P::Opacity, "0.6"),
                    (P::Top, "30px"),
                    (P::Left, "10px"),
                    (P::Width, "50px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, RED),
                ],
            );
        }
        // Chrome's minimum logical font size applies to sizes derived from
        // `medium` and percentages, never to absolute lengths, and em margins
        // and nested percentages keep the specified size.
        "minimum-font" => {
            div(b, 2, 1, &[(P::FontSize, "50%"), (P::Color, BLUE)]);
            b.create_text(3, "AA").insert(2, 3, 0);
            div(b, 4, 1, &[(P::FontSize, "50%"), (P::Color, RED)]);
            div(b, 5, 4, &[(P::FontSize, "200%")]);
            b.create_text(6, "AA").insert(5, 6, 0);
            div(b, 7, 1, &[(P::FontSize, "8px"), (P::Color, GREEN)]);
            b.create_text(8, "AA").insert(7, 8, 0);
            b.create_element(9, Tag::P).insert(1, 9, 0);
            for (property, value) in [
                (P::FontSize, "50%"),
                (P::LineHeight, "200%"),
                (P::Color, MAGENTA),
            ] {
                b.set_style(9, property, value);
            }
            b.create_text(10, "AA").insert(9, 10, 0);
        }
        // Opacity makes static blocks stacking contexts, but z-index does not
        // apply to them: z-index -1 stays above a later in-flow block, and
        // z-index 5 stays below a later positioned z-index 1 box.
        "opacity-z-index" => {
            for (id, z, height, color) in [(2, "-1", "40px", RED), (4, "5", "20px", YELLOW)] {
                div(
                    b,
                    id,
                    1,
                    &[
                        (P::Opacity, TRANSLUCENT),
                        (P::ZIndex, z),
                        (P::Width, "40px"),
                        (P::Height, height),
                        (P::BackgroundColor, color),
                    ],
                );
                if id == 2 {
                    div(
                        b,
                        3,
                        1,
                        &[
                            (P::MarginTop, "-20px"),
                            (P::Width, "60px"),
                            (P::Height, "30px"),
                            (P::BackgroundColor, BLUE),
                        ],
                    );
                }
            }
            div(
                b,
                5,
                1,
                &[
                    (P::Position, "relative"),
                    (P::ZIndex, "1"),
                    (P::Top, "-10px"),
                    (P::Left, "20px"),
                    (P::Width, "40px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
        }
        // p { font-size: 50% } > code: the monospace switch re-derives 8px as
        // 6.5px, which computes to the minimum. Chrome draws generic monospace
        // with a system face, so the code element is a flex container whose
        // line-height records its computed size, and its only item returns to
        // the fixture font.
        // A translated static box is a stacking context at level zero: it
        // paints above a later in-flow sibling it overlaps, with its
        // z-indexed child inside it, and both are hit there.
        "transform-stacking" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Transform, "translate(10px, 5px)"),
                    (P::Width, "40px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, RED),
                ],
            );
            div(
                b,
                3,
                2,
                &[
                    (P::Position, "relative"),
                    (P::ZIndex, "-1"),
                    (P::Left, "20px"),
                    (P::Width, "30px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            div(
                b,
                4,
                1,
                &[
                    (P::MarginTop, "-15px"),
                    (P::Width, "60px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
        }
        // Percentages of the box's own size, composed functions, and an
        // absolute child of a positioned translated box.
        "transform-percent" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Position, "relative"),
                    (
                        P::Transform,
                        "translate(50%, -25%) translateX(-4px) translateY(3px)",
                    ),
                    (P::MarginTop, "20px"),
                    (P::Width, "40px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
            square(b, 3, 2, ("10px", "-10px"), "16px", MAGENTA);
            div(
                b,
                4,
                1,
                &[
                    (P::Width, "30px"),
                    (P::Height, "10px"),
                    (P::BackgroundColor, CYAN),
                ],
            );
        }
        // A pressed author-styled button: text and border move one pixel.
        "transform-button" => {
            b.create_element(2, Tag::Button).insert(1, 2, 0);
            for (property, value) in [
                (P::Display, "block"),
                // Chrome's control font is its own; the fixture's is explicit.
                (P::FontFamily, "\"Varta Inline Fixture\""),
                (P::FontSize, "20px"),
                (P::LineHeight, "20px"),
                (P::Border, "1px solid #000000"),
                (P::BackgroundColor, GRAY),
                (P::Color, DARK),
                (P::Padding, "4px 6px"),
                (P::Transform, "translateY(1px)"),
            ] {
                b.set_style(2, property, value);
            }
            b.create_text(3, "AAA").insert(2, 3, 0);
            div(
                b,
                4,
                1,
                &[
                    (P::Width, "20px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
        }
        // A translated box inside a scrolled clipping container stays clipped
        // by it (`hidden`: Chrome's scrollbar is no part of the comparison).
        "transform-scroll" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Overflow, "hidden"),
                    (P::Width, "60px"),
                    (P::Height, "50px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            div(
                b,
                3,
                2,
                &[
                    (P::Transform, "translate(10px, -6px)"),
                    (P::Width, "30px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, RED),
                ],
            );
            div(
                b,
                4,
                2,
                &[
                    (P::Width, "40px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
        }
        // A pressed button scaled to 0.9 around its centre, its border,
        // radii, background, outline and text mapped together.
        "scale-button" => {
            b.create_element(2, Tag::Button).insert(1, 2, 0);
            for (property, value) in [
                (P::Display, "block"),
                (P::FontFamily, "\"Varta Inline Fixture\""),
                (P::FontSize, "20px"),
                (P::LineHeight, "20px"),
                (P::Border, "2px solid #000000"),
                (P::BorderRadius, "6px"),
                (P::BackgroundColor, GRAY),
                (P::Color, DARK),
                (P::Padding, "4px 6px"),
                (P::Outline, "2px solid #c000c0"),
                (P::Transform, "scale(0.9)"),
            ] {
                b.set_style(2, property, value);
            }
            b.create_text(3, "AAA").insert(2, 3, 0);
            div(
                b,
                4,
                1,
                &[
                    (P::Width, "20px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
        }
        // List order: a translation before the scale moves by the given
        // pixels, one after it by scaled pixels; anisotropic factors.
        "scale-order" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Transform, "translate(10px, 4px) scale(1.5, 0.5)"),
                    (P::Width, "30px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, RED),
                ],
            );
            div(
                b,
                3,
                1,
                &[
                    (P::Transform, "scale(0.5) translate(20px, 10px)"),
                    (P::Width, "40px"),
                    (P::Height, "40px"),
                    (P::BackgroundColor, GREEN),
                ],
            );
        }
        // Nested scaling with a translated and a stacked child.
        "scale-nested" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Transform, "scale(0.75)"),
                    (P::Width, "60px"),
                    (P::Height, "60px"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
            div(
                b,
                3,
                2,
                &[
                    (P::Transform, "translateX(8px) scale(2)"),
                    (P::Width, "16px"),
                    (P::Height, "10px"),
                    (P::BackgroundColor, BLUE),
                ],
            );
            div(
                b,
                4,
                2,
                &[
                    (P::Position, "relative"),
                    (P::ZIndex, "1"),
                    (P::Width, "20px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, CYAN),
                    (P::Opacity, TRANSLUCENT),
                ],
            );
        }
        "minimum-font-mono" => {
            b.create_element(2, Tag::P).insert(1, 2, 0);
            b.set_style(2, P::FontSize, "50%")
                .set_style(2, P::Color, MAGENTA);
            b.create_element(3, Tag::Code).insert(2, 3, 0);
            b.set_style(3, P::Display, "flex")
                .set_style(3, P::LineHeight, "100%");
            b.create_element(4, Tag::Span).insert(3, 4, 0);
            b.set_style(4, P::FontFamily, &format!("\"{FAMILY}\""));
            b.create_text(5, "AA").insert(4, 5, 0);
        }
        _ => unreachable!("every case name has a builder"),
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
    for case in [
        Case {
            name: "relative-flow",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "z-index",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "negative-context",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "fixed-scroll",
            scrolls: &[(2, 20.0)],
            reveal: Some(6),
        },
        Case {
            name: "absolute-scroll",
            scrolls: &[(2, 8.0), (4, 8.0)],
            reveal: Some(8),
        },
        Case {
            name: "opacity-context",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "clips",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "flex-items",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "fixed-text",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "minimum-font",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "opacity-z-index",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "minimum-font-mono",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "transform-stacking",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "transform-percent",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "transform-button",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "transform-scroll",
            scrolls: &[(2, 10.0)],
            reveal: Some(3),
        },
        Case {
            name: "scale-button",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "scale-order",
            scrolls: &[],
            reveal: None,
        },
        Case {
            name: "scale-nested",
            scrolls: &[],
            reveal: None,
        },
    ] {
        let mut b = B::new();
        b.set_style(1, P::FontFamily, "\"Varta Inline Fixture\"");
        if !case.name.starts_with(MINIMUM_FONT_CASES) {
            b.set_style(1, P::FontSize, "20px")
                .set_style(1, P::LineHeight, "20px");
        }
        build(case.name, &mut b);
        for &(id, top) in case.scrolls {
            b.command(id, Command::ScrollTo, top);
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
        let reveal = case
            .reveal
            .map(|target| -> Result<_, Box<dyn std::error::Error>> {
                let plan = view.scroll_into_view_plan(
                    target,
                    SIDE as f32,
                    SIDE as f32,
                    Some((&mut font, MEDIUM)),
                )?;
                Ok(serde_json::json!({"target":target,"plan":plan}))
            })
            .transpose()?;
        cases.push(serde_json::json!({
            "name":case.name,"tree":view.snapshot()["tree"],"boxes":boxes,"hits":hits,
            "raster":image.pixels,"raster_size":[image.width,image.height],
            "roi":{"x":0,"y":0,"width":side,"height":side},"reveal":reveal
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
