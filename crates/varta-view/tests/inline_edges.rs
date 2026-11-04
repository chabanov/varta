//! Horizontal non-replaced inline margins and padding become shaping edges.
//! Expected coordinates were checked against Chrome with the same fixture font
//! (`portable_edge_probe`), where "A" and the space each advance 8px.
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
const WIDTH: f32 = 140.0;
const HEIGHT: f32 = 180.0;
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/decoration.ttf").to_vec(),
        "Varta Decoration Fixture",
    )
    .unwrap()
}
/// `A<span>A A A</span>A` in a 64px block: both outer words continue across
/// the span boundaries.
fn fixture() -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "64px")
        .set_style(2, P::Padding, "20px")
        .set_style(2, P::FontFamily, "\"Varta Decoration Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "24px")
        .set_style(2, P::Color, "transparent")
        .set_style(2, P::BackgroundColor, "#00ff00")
        .create_text(6, "A")
        .insert(2, 6, 0)
        .create_element(3, Tag::Span)
        .insert(2, 3, 0)
        .set_style(3, P::BackgroundColor, "#ff0000")
        .create_text(4, "A A A")
        .insert(3, 4, 0)
        .create_text(7, "A")
        .insert(2, 7, 0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn apply(v: &mut View, styles: &[(P, &str)]) {
    let mut b = B::new();
    for &(property, value) in styles {
        b.set_style(3, property, value);
    }
    v.commit(&b.finish(), false, &mut Denied).unwrap();
}
fn boxes(v: &View) -> std::collections::BTreeMap<u32, varta_view::BoxRect> {
    v.layout_boxes_with_text(WIDTH, HEIGHT, &mut font(), 20.0)
        .unwrap()
}
fn raster(v: &View) -> varta_view::RasterImage {
    v.rasterize_with_text(WIDTH as u32, HEIGHT as u32, &mut font(), 20.0)
        .unwrap()
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
fn rect(r: varta_view::BoxRect) -> [f32; 4] {
    [r.x, r.y, r.width, r.height]
}
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

#[test]
fn padding_moves_later_text_and_keeps_edges_with_their_words() {
    let mut v = fixture();
    let plain = boxes(&v);
    // Without edges the span starts inside the first word.
    assert_eq!(rect(plain[&4])[..2], [28.0, 20.0]);
    apply(&mut v, &[(P::Padding, "0 6px")]);
    let b = boxes(&v);
    // Chrome: text 4 starts after A + 6px; "A A" fits on line one, the last
    // "A" plus the closing padding and the outer "A" wrap together.
    assert_eq!(rect(b[&4]), [20.0, 20.0, 38.0, 48.0]);
    assert_eq!(rect(b[&6]), [20.0, 20.0, 8.0, 24.0]);
    assert_eq!(rect(b[&7]), [34.0, 44.0, 8.0, 24.0]);
    assert_eq!(rect(b[&3]), [20.0, 20.0, 38.0, 48.0]);
    let image = raster(&v);
    // Opening padding is the span's own background and pointer target.
    assert_eq!(pixel(&image, 30, 30), RED);
    assert_eq!(image.hit_test(30.5, 30.5), Some(3));
    // Its closing padding follows the wrapped last word on line two.
    assert_eq!(pixel(&image, 31, 56), RED);
    assert_eq!(image.hit_test(31.5, 56.5), Some(3));
    assert_eq!(pixel(&image, 36, 56), GREEN);
}
#[test]
fn margins_take_advance_outside_the_box_but_inside_its_parent() {
    let mut v = fixture();
    apply(&mut v, &[(P::Margin, "0 5px")]);
    let b = boxes(&v);
    // Line two: the wrapped "A", the 5px closing margin, then the outer "A".
    assert_eq!(rect(b[&7]), [33.0, 44.0, 8.0, 24.0]);
    let image = raster(&v);
    // Between the outer "A" and the span's first glyph: parent, not span.
    assert_eq!(pixel(&image, 30, 30), GREEN);
    assert_eq!(image.hit_test(30.5, 30.5), Some(2));
    assert_eq!(pixel(&image, 34, 30), RED);
    assert_eq!(image.hit_test(34.5, 30.5), Some(3));
}
#[test]
fn negative_margins_pull_later_content_and_never_produce_negative_boxes() {
    let mut v = fixture();
    apply(&mut v, &[(P::Margin, "0 -4px")]);
    let b = boxes(&v);
    // 8px less advance lets the whole flow fit on line one:
    // 20 + 8 - 4 (span start) + 40 - 4 (outer "A").
    assert_eq!(rect(b[&7])[..2], [60.0, 20.0]);
    assert_eq!(rect(b[&3])[..3], [24.0, 20.0, 40.0]);
    apply(&mut v, &[(P::Margin, "0 -6px"), (P::Padding, "0")]);
    for (id, r) in boxes(&v) {
        assert!(r.width >= 0.0 && r.height >= 0.0, "node {id}");
    }
}
#[test]
fn empty_boxes_keep_their_padding_box_and_unbreakable_words_stay_whole() {
    let mut v = fixture();
    let mut b = B::new();
    b.set_text(4, "").set_text(6, "A ").set_text(7, " A");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    apply(&mut v, &[(P::Padding, "0 6px")]);
    let r = boxes(&v)[&3];
    assert_eq!(r.width, 12.0);
    let image = raster(&v);
    assert_eq!(image.hit_test(r.x + 6.5, r.y + 12.0), Some(3));

    let mut v = fixture();
    let mut b = B::new();
    b.set_text(4, "AA").set_text(6, "AA").set_text(7, "AA");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    apply(&mut v, &[(P::Padding, "0 6px")]);
    let b = boxes(&v);
    // The 76px unbreakable word overflows the 64px line as one piece: no
    // break opportunity was introduced at either boundary.
    assert_eq!(b[&6].y, b[&7].y);
    assert_eq!(rect(b[&7])[0], 64.0);
}
#[test]
fn a_later_lines_margin_gap_hits_the_block_over_an_earlier_fragment() {
    let mut v = fixture();
    apply(&mut v, &[(P::Padding, "9px 7px"), (P::Margin, "0 6px")]);
    let image = raster(&v);
    // Line one's span fragment reaches y=53, into line two's box (44..68).
    // Chrome hit-tests line two first: between its span fragment and the
    // outer "A" (the closing margin) the block wins; past the line's
    // contents, and inside a line-two fragment, the fragments do.
    let b = boxes(&v);
    let gap = b[&7].x - 3.0;
    assert_eq!(image.hit_test(gap, 49.0), Some(2));
    assert_eq!(image.hit_test(gap, 40.0), Some(3));
    assert_eq!(image.hit_test(b[&7].x + 20.0, 49.0), Some(3));
    assert_eq!(image.hit_test(22.0, 49.0), Some(3));
}
#[test]
fn rounded_slices_ignore_corners_of_absent_edges_when_normalizing() {
    let mut v = fixture();
    apply(
        &mut v,
        &[
            (P::Padding, "9px 7px"),
            (P::Border, "3px solid #0000ff"),
            (P::BorderRadius, "14px"),
        ],
    );
    let b = boxes(&v);
    let image = raster(&v);
    // The last fragment (20..38 wide) keeps its 14px bottom-right radius:
    // its absent left corners do not shrink it to fit 18px.
    assert_eq!(rect(b[&3])[..2], [20.0, 8.0]);
    assert_eq!(image.hit_test(34.25, 76.25), Some(2));
    assert_eq!(image.hit_test(30.0, 76.25), Some(3));
}
#[test]
fn a_single_fragment_content_box_excludes_horizontal_padding() {
    let mut v = fixture();
    let mut b = B::new();
    b.set_text(4, "A");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    apply(&mut v, &[(P::Padding, "0 3px 0 5px")]);
    let geometry = v
        .layout_geometry_with_text(WIDTH, HEIGHT, &mut font(), 20.0)
        .unwrap();
    let span = geometry[&3];
    assert_eq!(span.rect.width, 16.0);
    assert_eq!(span.content.x, span.rect.x + 5.0);
    assert_eq!(span.content.width, 8.0);
}
#[test]
fn edge_mutations_share_retained_stateless_and_vector_output() {
    let mut v = fixture();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for spacing in [
        [(P::Padding, "0 6px"), (P::Margin, "0")],
        [(P::Padding, "4px 5px 8px 3px"), (P::Margin, "0 -2px")],
        [(P::Padding, "0"), (P::Margin, "0 0.25rem")],
        [(P::Padding, "0 1.5px"), (P::Margin, "0 0.25px 0 2.75px")],
    ] {
        apply(&mut v, &spacing);
        apply(
            &mut v,
            &[(P::BorderRadius, "6px"), (P::Outline, "2px solid #0000ff")],
        );
        for alpha in ["1", "0.5", "0"] {
            apply(&mut v, &[(P::Opacity, alpha)]);
            for scale in [1.0, 1.25, 2.0] {
                let (width, height) = ((WIDTH * scale) as u32, (HEIGHT * scale) as u32);
                let expected = v
                    .rasterize_scaled(width, height, scale, Some((&mut font(), 20.0)))
                    .unwrap();
                assert_eq!(renderer.render(&v, width, height, scale).unwrap(), expected);
                match renderer.prepare(&v, width, height, scale).unwrap() {
                    PreparedFrame::Vector { metadata, scene } => {
                        assert_eq!(scene.rasterize(metadata).unwrap(), expected)
                    }
                    PreparedFrame::Raster(image) => assert_eq!(image, expected),
                }
            }
        }
    }
}
#[test]
fn percentages_breaks_and_bidirectional_edges_are_refused() {
    for styles in [
        [(P::Padding, "0 1%")],
        [(P::Margin, "0 -2%")],
        [(P::PaddingLeft, "10%")],
    ] {
        let mut v = fixture();
        apply(&mut v, &styles);
        assert!(
            v.layout_boxes_with_text(WIDTH, HEIGHT, &mut font(), 20.0)
                .is_err(),
            "{styles:?}"
        );
    }
    let mut v = fixture();
    let mut b = B::new();
    b.create_element(5, Tag::Br)
        .insert(3, 5, 0)
        .set_style(5, P::MarginLeft, "4px");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    assert!(
        v.layout_boxes_with_text(WIDTH, HEIGHT, &mut font(), 20.0)
            .is_err()
    );

    // Arabic joins in the inline fixture font; RTL fragment order with
    // decoration edges is not admitted, while the same text without edges is.
    let inline = || {
        TextMeasurer::new(
            include_bytes!("fixtures/inline.ttf").to_vec(),
            "Varta Inline Fixture",
        )
        .unwrap()
    };
    let mut v = fixture();
    let mut b = B::new();
    b.set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
        .set_text(4, "لا");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    assert!(
        v.layout_boxes_with_text(WIDTH, HEIGHT, &mut inline(), 20.0)
            .is_ok()
    );
    apply(&mut v, &[(P::PaddingLeft, "2px")]);
    assert!(
        v.layout_boxes_with_text(WIDTH, HEIGHT, &mut inline(), 20.0)
            .is_err()
    );
}
