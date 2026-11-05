//! Browser-derived geometry, propagation and ordered underline ink.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, PreparedFrame, Renderer, TextMeasurer, View};
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
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/decoration.ttf").to_vec(),
        "Varta Decoration Fixture",
    )
    .unwrap()
}
fn setup(text: &str, rich: bool) -> B {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "80px")
        .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "24px")
        .set_style(2, P::Color, "#ff0000")
        .set_style(2, P::TextDecoration, "underline");
    if rich {
        b.create_element(3, Tag::Span).insert(2, 3, 0);
    }
    b.create_text(4, text)
        .insert(if rich { 3 } else { 2 }, 4, 0);
    b
}
fn view(b: B) -> View {
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn render(v: &View) -> varta_view::RasterImage {
    v.rasterize_with_text(100, 100, &mut font(), 20.0).unwrap()
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
const RED: [u8; 4] = [255, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
#[test]
fn underline_decorates_internal_spaces_but_not_trailing_normal_spaces() {
    for rich in [false, true] {
        let image = render(&view(setup("A A  ", rich)));
        for y in [19, 20] {
            for x in 0..24 {
                assert_eq!(pixel(&image, x, y), RED, "rich{rich} {x}/{y}");
            }
        }
        assert_eq!(pixel(&image, 24, 19), WHITE);
        assert_eq!(pixel(&image, 8, 18), WHITE);
        assert_eq!(pixel(&image, 8, 21), WHITE);
    }
}
#[test]
fn descender_is_skipped_using_unhinted_ink_and_preserves_underlying_glyph() {
    for rich in [false, true] {
        let image = render(&view(setup("A g A", rich)));
        for y in [19, 20] {
            for x in [0, 8, 14, 25, 31, 39] {
                assert_eq!(pixel(&image, x, y), RED);
            }
            for x in [15, 16, 23, 24] {
                assert_eq!(pixel(&image, x, y), WHITE, "rich{rich} {x}/{y}");
            }
            assert_eq!(
                pixel(&image, 20, y),
                RED,
                "the descender itself remains visible"
            );
        }
    }
}
#[test]
fn descendant_color_and_none_preserve_decorating_box_color_and_thickness() {
    let mut b = setup("A A", true);
    b.set_style(3, P::Color, "#0000ff")
        .set_style(3, P::TextDecoration, "none")
        .set_style(3, P::FontSize, "10px");
    let image = render(&view(b));
    for y in [19, 20] {
        for x in 0..12 {
            assert_eq!(pixel(&image, x, y), RED);
        }
    }
    assert_eq!(pixel(&image, 2, 13), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 12, 19), WHITE);
}
#[test]
fn nested_underlines_keep_distinct_colors_sizes_and_order() {
    let mut b = setup("A A", true);
    b.set_style(3, P::FontSize, "30px")
        .set_style(3, P::Color, "#0000ff")
        .set_style(3, P::TextDecoration, "underline");
    let image = render(&view(b));
    assert_eq!(pixel(&image, 10, 22), RED);
    for y in [23, 24, 25] {
        assert_eq!(pixel(&image, 10, y), [0, 0, 255, 255]);
    }
}
#[test]
fn descendant_background_precedes_propagated_underline_and_opacity_applies_once() {
    let mut b = setup("A A", true);
    b.set_style(3, P::BackgroundColor, "#00ff00")
        .set_style(3, P::Opacity, "0.5");
    let image = render(&view(b));
    assert_eq!(pixel(&image, 10, 19), [255, 128, 128, 255]);
    assert_eq!(pixel(&image, 10, 18), [128, 255, 128, 255]);
}
#[test]
fn underline_does_not_change_layout_hits_or_snapshot_and_retained_vector_agrees() {
    let mut v = view(setup("A g A g A", true));
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for value in ["underline", "none", "underline"] {
        let mut b = B::new();
        b.set_style(2, P::TextDecoration, value)
            .set_style(2, P::Width, "24px");
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        let snapshot = v.snapshot();
        for scale in [1.0, 1.25, 2.0] {
            let side = (100.0 * scale) as u32;
            let expected = v
                .rasterize_scaled(side, side, scale, Some((&mut font(), 20.0)))
                .unwrap();
            assert_eq!(renderer.render(&v, side, side, scale).unwrap(), expected);
            let PreparedFrame::Vector { metadata, scene } =
                renderer.prepare(&v, side, side, scale).unwrap()
            else {
                panic!("underline uses existing bounded rectangles and glyph paths")
            };
            assert_eq!(scene.rasterize(metadata).unwrap(), expected);
        }
        assert_eq!(v.snapshot(), snapshot);
    }
}
#[test]
fn replaced_fields_stop_ancestor_decoration_but_support_their_own() {
    for tag in [Tag::Input, Tag::Textarea] {
        let mut b = B::new();
        b.set_style(1, P::BackgroundColor, "#ffffff")
            .set_style(1, P::TextDecoration, "underline")
            .create_element(2, tag)
            .insert(1, 2, 0)
            .set_value(2, "A g A", 0);
        for (p, v) in [
            (P::Display, "block"),
            (P::Width, "80px"),
            (P::Height, "32px"),
            (P::MinWidth, "0"),
            (P::MinHeight, "0"),
            (P::Padding, "0"),
            (P::Border, "0"),
            (P::Outline, "none"),
            (P::BorderRadius, "0"),
            (P::BackgroundColor, "#ffffff"),
            (P::FontWeight, "normal"),
            (P::FontStyle, "normal"),
            (P::FontFamily, "\"Varta Decoration Fixture\""),
            (P::FontSize, "20px"),
            (P::LineHeight, "24px"),
            (P::Color, "#ff0000"),
        ] {
            b.set_style(2, p, v);
        }
        let mut v = view(b);
        let ancestor = render(&v);
        let mut b = B::new();
        b.set_style(1, P::TextDecoration, "none");
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        assert_eq!(
            render(&v),
            ancestor,
            "ancestor must stop at replaced {tag:?}"
        );
        let plain = render(&v);
        let mut b = B::new();
        b.set_style(2, P::TextDecoration, "underline");
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        let decorated = render(&v);
        assert_ne!(plain.pixels, decorated.pixels);
        assert!(plain.same_input_geometry(&decorated));
    }
}

#[test]
fn cjk_auto_skip_ink_keeps_the_line_continuous() {
    for rich in [false, true] {
        let image = render(&view(setup("A 一 A", rich)));
        for y in [19, 20] {
            for x in 0..40 {
                assert_eq!(pixel(&image, x, y), RED);
            }
        }
    }
}
#[test]
fn fractional_font_metrics_match_css_font_boxes_without_rounding_advances() {
    let mut b = setup("A A", true);
    b.set_style(2, P::FontSize, "24.3px");
    let v = view(b);
    let boxes = v
        .layout_boxes_with_text(100.0, 100.0, &mut font(), 20.0)
        .unwrap();
    assert_eq!(boxes[&3].y, -3.0);
    assert_eq!(boxes[&3].height, 29.0);
    assert!((boxes[&3].width - 29.16).abs() < 0.001);
    for rich in [false, true] {
        let mut b = setup("A A", rich);
        b.set_style(2, P::FontSize, "16px");
        let mut v = view(b);
        let underlined = render(&v);
        assert_eq!(
            pixel(&underlined, 2, 16),
            WHITE,
            "ink ends at the16px baseline"
        );
        assert_eq!(pixel(&underlined, 2, 17), RED);
        let before = v
            .layout_boxes_with_text(100.0, 100.0, &mut font(), 20.0)
            .unwrap();
        let mut b = B::new();
        b.set_style(2, P::TextDecoration, "none");
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        assert_eq!(
            before,
            v.layout_boxes_with_text(100.0, 100.0, &mut font(), 20.0)
                .unwrap()
        );
        assert!(underlined.same_input_geometry(&render(&v)));
    }
}
#[test]
fn later_underline_covers_prior_overhanging_ink_in_source_order() {
    let mut b = setup("k", true);
    b.set_style(3, P::Color, "#0000ff")
        .create_element(5, Tag::Span)
        .insert(2, 5, 0)
        .create_text(6, "A")
        .insert(5, 6, 0);
    let image = render(&view(b));
    assert_eq!(pixel(&image, 9, 19), RED);
    assert_eq!(pixel(&image, 4, 19), [0, 0, 255, 255]);
}
#[test]
fn ink_gaps_respect_script_runs_hanging_whitespace_and_device_clip_edges() {
    let image = render(&view(setup("A אבג A", true)));
    assert_eq!(
        pixel(&image, 15, 19),
        RED,
        "Latin space belongs to the preceding run"
    );
    let mut b = setup("A g A\nA A", true);
    b.set_style(2, P::WhiteSpace, "pre-wrap")
        .set_style(2, P::Width, "24px");
    let image = render(&view(b));
    assert_eq!(
        pixel(&image, 24, 19),
        RED,
        "hanging space is outside the ink fragment"
    );
    let mut b = setup("A g A", true);
    b.set_style(2, P::FontSize, "24.3px");
    assert_eq!(
        pixel(&render(&view(b)), 18, 21),
        WHITE,
        "hard physical cutout edge"
    );
}
/// Pixel rows of the underline on each 24px line of the 20px fixture.
const UNDERLINE_ROWS: [[u32; 2]; 2] = [[19, 20], [43, 44]];
/// The fixture's `A` and space advance, and its first tab stop (eight spaces).
const CELL: u32 = 8;
const TAB_STOP: u32 = 8 * CELL;
/// `setup`'s flow with preserved white space: `pre` is the user-agent mode of
/// a `pre` element, so that root is a margin-free `pre`.
fn preserved(text: &str, white_space: &str, rich: bool) -> B {
    let mut b = B::new();
    if white_space == "pre" {
        b.create_element(2, Tag::Pre).set_style(2, P::Margin, "0");
    } else {
        b.create_element(2, Tag::Div)
            .set_style(2, P::WhiteSpace, white_space);
    }
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .insert(1, 2, 0)
        .set_style(2, P::Width, "80px")
        .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "24px")
        .set_style(2, P::Color, "#ff0000")
        .set_style(2, P::TextDecoration, "underline");
    if rich {
        b.create_element(3, Tag::Span).insert(2, 3, 0);
    }
    b.create_text(4, text)
        .insert(if rich { 3 } else { 2 }, 4, 0);
    b
}
fn assert_row(
    image: &varta_view::RasterImage,
    line: usize,
    // Half-open `[start, end)` pixel spans that are underlined.
    red: &[(u32, u32)],
    end: u32,
    case: &str,
) {
    for y in UNDERLINE_ROWS[line] {
        for x in 0..end {
            let expected = if red.iter().any(|&(start, stop)| (start..stop).contains(&x)) {
                RED
            } else {
                WHITE
            };
            assert_eq!(pixel(image, x, y), expected, "{case} line {line} {x}/{y}");
        }
    }
}
#[test]
fn underline_skips_preserved_tabs_but_keeps_preserved_spaces() {
    // Chrome decorates text fragments only, and a preserved tab is a control
    // item of its own: its advance is never underlined, inside a line or
    // hanging at its end, while preserved spaces are.
    for white_space in ["pre", "pre-wrap"] {
        for rich in [false, true] {
            let image = render(&view(preserved("A\tA\nA \t", white_space, rich)));
            let case = format!("{white_space} rich{rich}");
            assert_row(
                &image,
                0,
                &[(0, CELL), (TAB_STOP, TAB_STOP + CELL)],
                80,
                &case,
            );
            assert_row(&image, 1, &[(0, 2 * CELL)], 80, &case);
        }
    }
}
#[test]
fn plain_preserved_trailing_spaces_keep_underline_and_hits() {
    // The text overflows its 8px block, so only the text fragment, not the
    // block, can answer a hit over the trailing spaces.
    for white_space in ["pre", "pre-wrap"] {
        let mut b = preserved("A   \nA    ", white_space, false);
        b.set_style(2, P::Width, "8px");
        let image = render(&view(b));
        assert_row(&image, 0, &[(0, 4 * CELL)], 48, white_space);
        assert_row(&image, 1, &[(0, 5 * CELL)], 48, white_space);
        for (x, y) in [(3 * CELL, 10), (4 * CELL + CELL / 2, 34)] {
            assert_eq!(
                image.hit_test(x as f32 + 0.5, y as f32),
                Some(2),
                "{white_space} hit {x}/{y}"
            );
        }
        assert_eq!(image.hit_test((5 * CELL) as f32 + 0.5, 34.0), None);
    }
}
