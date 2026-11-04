//! Solid borders of non-replaced inline boxes: edges in the line, vertical
//! extent outside it, and `slice` rings painted over a continuous strip.
//! Expected geometry was checked against Chrome (`portable_edge_probe`).
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
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/decoration.ttf").to_vec(),
        "Varta Decoration Fixture",
    )
    .unwrap()
}
/// `A<span>A A A</span>A` in a 64px block, as in the edge tests.
fn fixture(styles: &[(P, &str)]) -> View {
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
    for &(property, value) in styles {
        b.set_style(3, property, value);
    }
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn boxes(v: &View) -> std::collections::BTreeMap<u32, [f32; 4]> {
    v.layout_boxes_with_text(WIDTH, HEIGHT, &mut font(), 20.0)
        .unwrap()
        .into_iter()
        .map(|(id, r)| (id, [r.x, r.y, r.width, r.height]))
        .collect()
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

#[test]
fn borders_take_inline_advance_and_paint_outside_the_line() {
    let v = fixture(&[(P::Border, "4px solid #0000ff")]);
    let b = boxes(&v);
    // Chrome: 8px less room keeps the flow on one line; the span's border
    // box rises 4px above and below its font box without moving the text.
    assert_eq!(b[&3], [28.0, 16.0, 48.0, 32.0]);
    assert_eq!(b[&4], [32.0, 20.0, 40.0, 24.0]);
    assert_eq!(b[&7], [76.0, 20.0, 8.0, 24.0]);
    assert_eq!(b[&2][3], 64.0);
    let image = raster(&v);
    for (x, y) in [(29, 30), (74, 30), (50, 17), (50, 46)] {
        assert_eq!(pixel(&image, x, y), BLUE, "{x},{y}");
    }
    assert_eq!(pixel(&image, 50, 30), RED);
    assert_eq!(pixel(&image, 50, 14), GREEN);
    assert_eq!(image.hit_test(29.5, 30.5), Some(3));
    assert_eq!(image.hit_test(50.5, 17.5), Some(3));
}
#[test]
fn individual_sides_and_vertical_only_borders_keep_line_metrics() {
    let v = fixture(&[
        (P::BorderLeft, "5px solid #0000ff"),
        (P::BorderRight, "2px solid #0000ff"),
        (P::BorderTop, "3px solid #0000ff"),
    ]);
    let b = boxes(&v);
    assert_eq!(b[&3], [28.0, 17.0, 47.0, 27.0]);
    assert_eq!(b[&7][0], 75.0);
    let v = fixture(&[
        (P::Padding, "0 6px"),
        (P::BorderTop, "4px solid #0000ff"),
        (P::BorderBottom, "2px solid #0000ff"),
    ]);
    let b = boxes(&v);
    // Two lines; neither line box moves, both fragments grow vertically.
    assert_eq!(b[&4], [20.0, 20.0, 38.0, 48.0]);
    assert_eq!(b[&3], [20.0, 16.0, 38.0, 54.0]);
}
#[test]
fn broken_boxes_slice_their_border_across_lines() {
    let v = fixture(&[(P::Padding, "0 6px"), (P::Border, "2px solid #0000ff")]);
    let image = raster(&v);
    let b = boxes(&v);
    // The first fragment has a left border and no right border; the last one
    // a right border and no left border. Both keep their top and bottom.
    let first_left = 28;
    assert_eq!(pixel(&image, first_left, 30), BLUE);
    assert_eq!(
        pixel(&image, 20, 56),
        RED,
        "no left border on the last line"
    );
    let right = (b[&7][0] - 1.0) as u32;
    assert_eq!(pixel(&image, right, 56), BLUE);
    assert_eq!(pixel(&image, 30, 18), BLUE);
    // Line two's fragment (x < 36) paints over this row later; further right
    // only line one's bottom border is there.
    assert_eq!(pixel(&image, 50, 44), BLUE, "first fragment bottom border");
}
#[test]
fn empty_and_unbroken_rounded_boxes_paint_whole_rings() {
    let mut v = fixture(&[(P::Padding, "0 6px"), (P::Border, "2px solid #0000ff")]);
    let mut b = B::new();
    b.set_text(4, "").set_text(6, "A ").set_text(7, " A");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    let r = boxes(&v)[&3];
    assert_eq!(r, [36.0, 18.0, 16.0, 28.0]);
    let image = raster(&v);
    assert_eq!(pixel(&image, 36, 30), BLUE);
    assert_eq!(pixel(&image, 51, 30), BLUE);

    let mut v = fixture(&[
        (P::Padding, "0 6px"),
        (P::Border, "3px solid #0000ff"),
        (P::BorderRadius, "50%"),
    ]);
    let mut b = B::new();
    b.set_text(4, "A");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    assert_eq!(boxes(&v)[&3], [28.0, 17.0, 26.0, 30.0]);
    let image = raster(&v);
    // The elliptical corner is outside the ring: the block shows through.
    assert_eq!(pixel(&image, 29, 18), GREEN);
    assert_eq!(pixel(&image, 41, 18), BLUE);
    assert_eq!(image.hit_test(29.5, 18.5), Some(2));
}
#[test]
fn border_mutations_share_retained_stateless_and_vector_output() {
    let mut v = fixture(&[(P::Padding, "0 6px")]);
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for (border, radius) in [
        ("2px solid #0000ff", "0"),
        ("5px solid rgba(0, 0, 255, 0.5)", "4px 12px"),
        ("0.25rem solid", "50%"),
        ("none", "8px"),
    ] {
        let mut b = B::new();
        b.set_style(3, P::Border, border)
            .set_style(3, P::BorderRadius, radius)
            .set_style(3, P::Color, "#0000ff")
            .set_style(3, P::Outline, "2px solid #ff00ff");
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        for alpha in ["1", "0.5", "0"] {
            let mut b = B::new();
            b.set_style(3, P::Opacity, alpha);
            v.commit(&b.finish(), false, &mut Denied).unwrap();
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
fn distinct_side_colors_and_border_on_line_breaks_are_refused() {
    let v = fixture(&[
        (P::Border, "2px solid #0000ff"),
        (P::BorderLeft, "2px solid #ff00ff"),
    ]);
    // Layout admits the widths; painting refuses a second visible color.
    assert!(
        v.layout_boxes_with_text(WIDTH, HEIGHT, &mut font(), 20.0)
            .is_ok()
    );
    assert!(
        v.rasterize_with_text(WIDTH as u32, HEIGHT as u32, &mut font(), 20.0)
            .is_err()
    );
    // A hidden side's color does not count.
    let v = fixture(&[
        (P::BorderTop, "2px solid #0000ff"),
        (P::BorderLeft, "0 solid #ff00ff"),
    ]);
    raster(&v);

    let mut v = fixture(&[]);
    let mut b = B::new();
    b.create_element(5, Tag::Br)
        .insert(3, 5, 0)
        .set_style(5, P::Border, "1px solid #000000");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    assert!(
        v.layout_boxes_with_text(WIDTH, HEIGHT, &mut font(), 20.0)
            .is_err()
    );
}
