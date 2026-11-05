//! Positioned boxes and z-index stacking after CSS2 Appendix E: paint and hit
//! order, clip scopes, fixed and absolute scrolling, and retained/vector parity.
use crate::{Access, BatchWriter as B, Broker, PreparedFrame, RasterImage, Renderer, View};
use varta_common::view::{Command, CssProperty as P, Tag};

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

const SIDE: u32 = 80;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const GRAY: [u8; 4] = [128, 128, 128, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn hex(color: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}
fn view(build: impl FnOnce(&mut B)) -> View {
    let mut b = B::new();
    build(&mut b);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    view
}
fn div(b: &mut B, id: u32, parent: u32, styles: &[(P, &str)]) {
    b.create_element(id, Tag::Div).insert(parent, id, 0);
    for &(property, value) in styles {
        b.set_style(id, property, value);
    }
}
/// An absolute `side`-pixel square at `(left, top)` with an optional z-index.
fn square(b: &mut B, id: u32, parent: u32, at: (u32, u32), side: u32, color: [u8; 4], z: &str) {
    let (left, top, side) = (
        format!("{}px", at.0),
        format!("{}px", at.1),
        format!("{side}px"),
    );
    div(
        b,
        id,
        parent,
        &[
            (P::Position, "absolute"),
            (P::Left, &left),
            (P::Top, &top),
            (P::Width, &side),
            (P::Height, &side),
            (P::BackgroundColor, &hex(color)),
            (P::ZIndex, z),
        ],
    );
}
fn pixel(image: &RasterImage, x: u32, y: u32) -> [u8; 4] {
    let offset = (y as usize * image.width as usize + x as usize) * 4;
    image.pixels[offset..offset + 4].try_into().unwrap()
}
/// A pixel position, its painted color and its hit target.
type Sample = ((u32, u32), [u8; 4], Option<u32>);
/// Each sample paints `color` and hits `node`: hit order equals paint order.
fn expect(image: &RasterImage, samples: &[Sample]) {
    for &((x, y), color, node) in samples {
        assert_eq!(pixel(image, x, y), color, "pixel at ({x}, {y})");
        let center = |value: u32| value as f32 + 0.5;
        assert_eq!(
            image.hit_test(center(x), center(y)),
            node,
            "hit at ({x}, {y})"
        );
    }
}

#[test]
fn z_index_orders_layers_then_tree_order_and_ignores_static_blocks() {
    let view = view(|b| {
        square(b, 2, 1, (0, 0), 30, RED, "2");
        square(b, 3, 1, (10, 10), 30, BLUE, "1");
        square(b, 4, 1, (20, 20), 30, GREEN, "auto");
        square(b, 5, 1, (30, 30), 30, YELLOW, "0");
        // z-index does not apply to an unpositioned block outside flex/grid.
        div(
            b,
            6,
            1,
            &[
                (P::ZIndex, "9"),
                (P::Height, "80px"),
                (P::BackgroundColor, &hex(GRAY)),
            ],
        );
    });
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    expect(
        &image,
        &[
            ((5, 5), RED, Some(2)),
            ((25, 25), RED, Some(2)),
            ((35, 35), BLUE, Some(3)),
            ((45, 22), GREEN, Some(4)),
            ((45, 45), YELLOW, Some(5)),
            ((70, 70), GRAY, Some(6)),
        ],
    );
}

#[test]
fn negative_layers_paint_between_a_context_background_and_its_flow() {
    let view = view(|b| {
        div(
            b,
            2,
            1,
            &[
                (P::Position, "relative"),
                (P::ZIndex, "0"),
                // A formatting context: the flow child's margin stays inside.
                (P::Overflow, "hidden"),
                (P::Width, "60px"),
                (P::Height, "60px"),
                (P::BackgroundColor, &hex(GRAY)),
            ],
        );
        square(b, 3, 2, (5, 5), 40, RED, "-1");
        square(b, 4, 2, (15, 15), 40, BLUE, "-2");
        div(
            b,
            5,
            2,
            &[
                (P::Margin, "20px"),
                (P::Width, "30px"),
                (P::Height, "30px"),
                (P::BackgroundColor, &hex(GREEN)),
            ],
        );
        // In the root context a negative layer stays below unpositioned
        // blocks, including an auto-positioned box's own background.
        div(
            b,
            6,
            1,
            &[
                (P::Width, "20px"),
                (P::Height, "20px"),
                (P::BackgroundColor, &hex(GRAY)),
            ],
        );
        square(b, 7, 1, (10, 70), 20, RED, "-1");
    });
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    expect(
        &image,
        &[
            ((2, 2), GRAY, Some(2)),
            ((10, 10), RED, Some(3)),
            ((50, 50), BLUE, Some(4)),
            ((30, 30), GREEN, Some(5)),
            ((15, 75), GRAY, Some(6)),
            ((25, 75), RED, Some(7)),
        ],
    );
}

#[test]
fn layers_found_while_painting_an_auto_positioned_box_keep_tree_order() {
    let view = view(|b| {
        div(
            b,
            2,
            1,
            &[
                (P::Position, "relative"),
                (P::Width, "40px"),
                (P::Height, "40px"),
                (P::BackgroundColor, &hex(RED)),
            ],
        );
        div(
            b,
            3,
            2,
            &[
                (P::Position, "relative"),
                (P::Top, "20px"),
                (P::Left, "20px"),
                (P::Width, "30px"),
                (P::Height, "30px"),
                (P::BackgroundColor, &hex(BLUE)),
            ],
        );
        div(
            b,
            4,
            1,
            &[
                (P::Position, "relative"),
                (P::Top, "-30px"),
                (P::Left, "30px"),
                (P::Width, "30px"),
                (P::Height, "30px"),
                (P::BackgroundColor, &hex(GREEN)),
            ],
        );
    });
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    expect(
        &image,
        &[
            ((5, 5), RED, Some(2)),
            ((25, 25), BLUE, Some(3)),
            ((35, 30), GREEN, Some(4)),
            ((45, 45), BLUE, Some(3)),
        ],
    );
}

#[test]
fn flex_items_take_z_index_without_positioning() {
    let view = view(|b| {
        div(
            b,
            2,
            1,
            &[
                (P::Display, "flex"),
                (P::Width, "80px"),
                (P::Height, "40px"),
                (P::BackgroundColor, &hex(GRAY)),
            ],
        );
        for (id, z, margin, color) in [
            (3, "1", (P::MarginRight, "-10px"), RED),
            (4, "auto", (P::MarginRight, "0px"), BLUE),
            (5, "-1", (P::MarginLeft, "-10px"), GREEN),
        ] {
            div(
                b,
                id,
                2,
                &[
                    (P::ZIndex, z),
                    margin,
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, &hex(color)),
                ],
            );
        }
    });
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    expect(
        &image,
        &[
            ((25, 5), RED, Some(3)),
            ((35, 5), BLUE, Some(4)),
            // The negative item paints below its unpositioned container.
            ((60, 5), GRAY, Some(2)),
        ],
    );
}

#[test]
fn zero_level_opacity_contexts_paint_in_tree_order_across_flex_items() {
    // The first context sits in a flex item, which paints in the content
    // phase after the later block was entered; tree order still wins.
    let view = view(|b| {
        div(b, 2, 1, &[(P::Display, "flex"), (P::Height, "20px")]);
        div(b, 3, 2, &[(P::Width, "20px"), (P::Height, "20px")]);
        for (id, parent, color) in [(4, 3, RED), (6, 5, BLUE)] {
            if parent == 5 {
                div(b, 5, 1, &[(P::MarginTop, "-20px")]);
            }
            div(
                b,
                id,
                parent,
                &[
                    (P::Opacity, "0.5"),
                    (P::Width, "20px"),
                    (P::Height, "20px"),
                    (P::BackgroundColor, &hex(color)),
                ],
            );
        }
    });
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    let [red, _, blue, _] = pixel(&image, 10, 10);
    assert!(blue > red, "the later context paints on top: {red} {blue}");
    assert_eq!(image.hit_test(10.5, 10.5), Some(6));
}

#[test]
fn static_opacity_contexts_ignore_z_index() {
    // Opacity makes these static blocks stacking contexts, but z-index does
    // not apply to them: both paint at level zero in tree order.
    const TRANSLUCENT: &str = "0.99";
    let view = view(|b| {
        let block = |b: &mut B, id: u32, z: &str, height: &str, color: [u8; 4]| {
            div(
                b,
                id,
                1,
                &[
                    (P::Opacity, TRANSLUCENT),
                    (P::ZIndex, z),
                    (P::Width, "40px"),
                    (P::Height, height),
                    (P::BackgroundColor, &hex(color)),
                ],
            );
        };
        // z-index -1 does not sink below the later in-flow block.
        block(b, 2, "-1", "40px", RED);
        div(
            b,
            3,
            1,
            &[
                (P::MarginTop, "-20px"),
                (P::Width, "60px"),
                (P::Height, "30px"),
                (P::BackgroundColor, &hex(BLUE)),
            ],
        );
        // z-index 5 does not rise above a later positioned z-index 1 box.
        block(b, 4, "5", "20px", YELLOW);
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
                (P::BackgroundColor, &hex(GREEN)),
            ],
        );
    });
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    let [red, _, blue, _] = pixel(&image, 10, 30);
    assert!(red > blue, "the opacity block stays above: {red} {blue}");
    assert_eq!(image.hit_test(10.5, 30.5), Some(2));
    expect(
        &image,
        &[((30, 65), GREEN, Some(5)), ((50, 45), BLUE, Some(3))],
    );
}

fn scrolled_fixed() -> View {
    let mut view = view(|b| {
        div(
            b,
            2,
            1,
            &[
                (P::Overflow, "auto"),
                (P::Width, "60px"),
                (P::Height, "40px"),
            ],
        );
        div(
            b,
            3,
            2,
            &[(P::Height, "30px"), (P::BackgroundColor, &hex(RED))],
        );
        div(
            b,
            8,
            2,
            &[(P::Height, "30px"), (P::BackgroundColor, &hex(GRAY))],
        );
        div(
            b,
            4,
            2,
            &[
                (P::Position, "fixed"),
                (P::Top, "10px"),
                (P::Left, "10px"),
                (P::Width, "20px"),
                (P::Height, "20px"),
                (P::BackgroundColor, &hex(BLUE)),
            ],
        );
        // The initial containing block, not the scroller, owns this box.
        square(b, 5, 2, (40, 30), 20, GREEN, "auto");
    });
    let mut b = B::new();
    b.command(2, Command::ScrollTo, 15.0);
    view.commit_with_layout(
        &b.finish(),
        false,
        &mut Deny,
        SIDE as f32,
        SIDE as f32,
        None,
    )
    .unwrap();
    view
}

#[test]
fn fixed_and_initial_absolute_boxes_ignore_a_scrolled_ancestor() {
    let view = scrolled_fixed();
    let boxes = view.layout_geometry(SIDE as f32, SIDE as f32).unwrap();
    assert_eq!((boxes[&3].rect.y, boxes[&4].rect.y), (-15.0, 10.0));
    assert_eq!(boxes[&5].rect.y, 30.0);
    // Neither the scroller's offset nor its clip applies.
    assert_eq!(boxes[&5].clip, boxes[&1].clip);
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    expect(
        &image,
        &[
            ((15, 15), BLUE, Some(4)),
            ((5, 5), RED, Some(3)),
            ((55, 32), GREEN, Some(5)),
            ((45, 45), GREEN, Some(5)),
        ],
    );
    // Wheel input chains through containing blocks: fixed and initial
    // absolute boxes have no scrolling ancestor.
    assert_eq!(image.scroll_target(15.0, 15.0, 5.0), None);
    assert_eq!(image.scroll_target(45.0, 45.0, 5.0), None);
    assert_eq!(image.scroll_target(5.0, 5.0, 5.0), Some((2, 20.0, 0.0)));
    assert!(!image.scroll_into_view_supported(4));
    assert!(!image.scroll_into_view_supported(5));
    assert!(image.scroll_into_view_supported(3));
    assert_eq!(
        view.scroll_into_view_plan(4, SIDE as f32, SIDE as f32, None)
            .unwrap(),
        Vec::new()
    );
}

#[test]
fn absolute_boxes_scroll_and_clip_with_their_containing_block_only() {
    let view = view(|b| {
        div(
            b,
            2,
            1,
            &[
                (P::Position, "relative"),
                (P::Overflow, "hidden"),
                (P::Width, "60px"),
                (P::Height, "40px"),
            ],
        );
        div(
            b,
            3,
            2,
            &[
                (P::Overflow, "hidden"),
                (P::Height, "30px"),
                (P::BackgroundColor, &hex(GRAY)),
            ],
        );
        div(b, 4, 3, &[(P::Height, "60px")]);
        square(b, 5, 3, (0, 70), 10, BLUE, "auto");
        div(
            b,
            6,
            2,
            &[(P::Height, "40px"), (P::BackgroundColor, &hex(RED))],
        );
    });
    // Revealing the absolute box scrolls its containing block, never the
    // unpositioned scroller between them.
    assert_eq!(
        view.scroll_into_view_plan(5, SIDE as f32, SIDE as f32, None)
            .unwrap(),
        // The absolute box extends the containing block's scrollable overflow.
        vec![(2, 40.0)]
    );
    let image = view.rasterize_boxes(SIDE, SIDE).unwrap();
    assert!(image.scroll_into_view_supported(5));
    // Below the containing block's scrollport: clipped, no hit.
    expect(&image, &[((5, 75), CLEAR, None), ((5, 35), RED, Some(6))]);
}

#[test]
fn opacity_groups_refuse_out_of_flow_boxes_that_escape_their_clip() {
    let tree = |clip: bool| {
        view(|b| {
            div(b, 2, 1, &[(P::Width, "20px"), (P::Height, "20px")]);
            if clip {
                b.set_style(2, P::Overflow, "hidden");
            }
            div(b, 3, 2, &[(P::Opacity, "0.5")]);
            div(
                b,
                4,
                3,
                &[
                    (P::Position, "fixed"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, &hex(RED)),
                ],
            );
        })
    };
    assert!(matches!(
        tree(true).rasterize_boxes(SIDE, SIDE),
        Err(crate::PaintError::Unsupported(4))
    ));
    let image = tree(false).rasterize_boxes(SIDE, SIDE).unwrap();
    assert_eq!(pixel(&image, 25, 25)[3], 128);
    assert_eq!(image.hit_test(25.5, 25.5), Some(4));
}

#[test]
fn retained_and_vector_frames_match_the_reference() {
    let mut view = scrolled_fixed();
    let mut b = B::new();
    square(&mut b, 6, 1, (50, 0), 20, YELLOW, "-1");
    b.set_style(6, P::Opacity, "0.5");
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let side = (SIDE as f32 * scale).round() as u32;
        let reference = view.rasterize_scaled(side, side, scale, None).unwrap();
        assert!(renderer.render(&view, side, side, scale).unwrap() == reference);
    }
    // Scrolling repaints the scrolled content under the unmoved fixed box.
    let mut b = B::new();
    b.command(2, Command::ScrollTo, 5.0);
    view.commit_with_layout(
        &b.finish(),
        false,
        &mut Deny,
        SIDE as f32,
        SIDE as f32,
        None,
    )
    .unwrap();
    let reference = view
        .rasterize_scaled(SIDE * 2, SIDE * 2, 2.0, None)
        .unwrap();
    assert!(renderer.render(&view, SIDE * 2, SIDE * 2, 2.0).unwrap() == reference);
    assert!(renderer.damage().is_some());
    // The translucent layer is a software group: an explicit CPU fallback.
    assert!(matches!(
        renderer.prepare(&view, SIDE, SIDE, 1.0).unwrap(),
        PreparedFrame::Raster(_)
    ));
    let mut b = B::new();
    b.remove_style(6, P::Opacity);
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let side = (SIDE as f32 * scale).round() as u32;
        let reference = view.rasterize_scaled(side, side, scale, None).unwrap();
        let PreparedFrame::Vector { metadata, scene } =
            renderer.prepare(&view, side, side, scale).unwrap()
        else {
            panic!("positioned layers fit the vector contract")
        };
        assert!(scene.rasterize(metadata).unwrap() == reference);
    }
}

/// The console mockup's toast stack and narrow-viewport context panel
/// (`examples/uarp-console/mockups/console.css`), with its light colors.
const CONSOLE_LAYERS: &str = ":root { --border: #e2e4e9; --bg-subtle: #f5f6f8; --toast-bg: #16181d; --toast-fg: #ffffff }
.page { height: 600px; background-color: #ffffff; font-family: system-ui, sans-serif; font-size: 15px; line-height: 1.5 }
.panel { display: flex; flex-direction: column; gap: 16px; min-height: 0; overflow: auto; padding: 16px; border-left: 1px solid var(--border); background-color: var(--bg-subtle) }
.toasts { position: fixed; right: 16px; bottom: 112px; z-index: 10; display: flex; flex-direction: column; gap: 8px; font-family: system-ui, sans-serif }
.toast { padding: 10px 14px; border-radius: 10px; background-color: var(--toast-bg); color: var(--toast-fg); font-size: 14px }
@media (max-width: 900px) { .panel { position: fixed; top: 0; right: 0; bottom: 0; z-index: 20; width: 340px; max-width: 88%; border: 1px solid var(--border) } }";

#[test]
fn console_toasts_and_context_panel_paint_in_z_index_order() {
    let mut b = B::new();
    b.stylesheet(1, CONSOLE_LAYERS)
        .create_element(2, Tag::Div)
        .set_class(2, "page")
        .insert(1, 2, 0)
        .create_element(3, Tag::Div)
        .set_class(3, "toasts")
        .insert(2, 3, 0);
    for (id, text) in [(4, "AAA"), (6, "AAAA AAAA")] {
        b.create_element(id, Tag::Div)
            .set_class(id, "toast")
            .insert(3, id, 0)
            .create_text(id + 1, text)
            .insert(id, id + 1, 0);
    }
    b.create_element(8, Tag::Aside)
        .set_class(8, "panel")
        .insert(2, 8, 0)
        .create_text(9, "AA")
        .insert(8, 9, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    let font = || {
        crate::TextMeasurer::new_with_aliases(
            include_bytes!("../../tests/fixtures/inline.ttf").to_vec(),
            "Varta Inline Fixture",
            &["system-ui", "sans-serif"],
        )
        .unwrap()
    };
    const MEDIUM: f32 = 16.0;
    for (width, panel) in [(1024, false), (800, true)] {
        let image = view
            .rasterize_with_text(width, 600, &mut font(), MEDIUM)
            .unwrap();
        let geometry = view
            .layout_geometry_with_text(width as f32, 600.0, &mut font(), MEDIUM)
            .unwrap();
        let toast = geometry[&6].rect;
        let (x, y) = (toast.x + toast.width - 4.0, toast.y + toast.height / 2.0);
        // The fixed stack keeps its viewport insets; the panel (z-index 20)
        // covers the toasts (z-index 10) only while it is fixed.
        assert_eq!(
            geometry[&3].rect.x + geometry[&3].rect.width,
            width as f32 - 16.0
        );
        assert_eq!(
            geometry[&3].rect.y + geometry[&3].rect.height,
            600.0 - 112.0
        );
        assert_eq!(image.hit_test(x, y), Some(if panel { 8 } else { 6 }));
    }
}
