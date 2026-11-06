//! Packaged images (`img`) for the real-Chrome geometry, hit, accessibility
//! and pixel oracle in `scripts/test-native-images.mjs`: natural and explicit
//! sizes, the natural ratio and its constraints, inline line boxes, flex and
//! grid items, downscaling levels and upscaling, odd level sizes, rounded
//! content clips, opacity, `transform`, fractional edges and every PNG color
//! type the decoder expands.
//! Usage: native_image_probe [SCALE]
use std::sync::Arc;
use varta_common::view::{Attribute as A, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, ImageSet, TextMeasurer, View};

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
const SIDE: u32 = 100;
/// Chrome's initial `medium` font size, handed to the measurer as the host size.
const MEDIUM: f32 = 16.0;
/// Pointer samples every this many CSS pixels.
const HIT_STEP: usize = 3;
const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.0;
/// Cases whose root keeps the initial `normal` line height.
const NORMAL_LINE_CASES: &str = "inline-normal";

const GRAY: &str = "#c0c0c0";
const RED: &str = "#e00000";
const GREEN: &str = "#00a000";
const BLUE: &str = "#0000e0";
const YELLOW: &str = "#e0c000";

type Styles<'a> = &'a [(P, &'a str)];

/// The test pattern: a two-pixel checker in red, a horizontal green ramp, a
/// vertical blue ramp and, for `alpha`, a repeating alpha ramp.
fn pattern(width: u32, height: u32, alpha: bool) -> Vec<u8> {
    let ramp = |value: u32, length: u32| (255 * value / (length - 1).max(1)) as u8;
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let checker = ((x >> 1) + (y >> 1)) & 1 == 1;
            pixels.extend([
                if checker { 230 } else { 20 },
                ramp(x, width),
                ramp(y, height),
                if alpha {
                    (255 * ((x + y) % 5) / 4) as u8
                } else {
                    255
                },
            ]);
        }
    }
    pixels
}

/// A PNG of `pixels` (`color`, eight bits per sample) with an optional
/// palette and transparency table.
fn encode(
    (width, height): (u32, u32),
    color: png::ColorType,
    pixels: &[u8],
    palette: Option<(&[u8], &[u8])>,
) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        if let Some((palette, transparency)) = palette {
            encoder.set_palette(palette.to_vec());
            encoder.set_trns(transparency.to_vec());
        }
        let mut writer = encoder.write_header()?;
        writer.write_image_data(pixels)?;
    }
    Ok(out)
}

fn images() -> Result<Vec<(&'static str, Vec<u8>)>, png::EncodingError> {
    use png::ColorType as C;
    let rgba = |w, h, alpha| encode((w, h), C::Rgba, &pattern(w, h, alpha), None);
    let rgb: Vec<u8> = pattern(16, 16, false)
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[red, green, blue, _]| [*red, *green, *blue])
        .collect();
    let gray_alpha: Vec<u8> = (0..12 * 8)
        .flat_map(|i| [(i * 37 % 256) as u8, (i * 53 % 256) as u8])
        .collect();
    let indices: Vec<u8> = (0..10 * 6).map(|i| (i % 4) as u8).collect();
    let palette = [255, 0, 0, 0, 160, 0, 0, 0, 255, 250, 200, 0];
    let transparency = [255, 128, 0];
    Ok(vec![
        ("pattern-16.png", rgba(16, 16, false)?),
        ("pattern-64.png", rgba(64, 64, false)?),
        ("wide-20x10.png", rgba(20, 10, false)?),
        ("odd-45x37.png", rgba(45, 37, false)?),
        ("alpha-16.png", rgba(16, 16, true)?),
        ("rgb-16.png", encode((16, 16), C::Rgb, &rgb, None)?),
        (
            "gray-12x8.png",
            encode((12, 8), C::GrayscaleAlpha, &gray_alpha, None)?,
        ),
        (
            "palette-10x6.png",
            encode(
                (10, 6),
                C::Indexed,
                &indices,
                Some((&palette, &transparency)),
            )?,
        ),
    ])
}

/// An element `id` of `tag` appended to `parent` with `styles`.
fn element(b: &mut B, (id, tag, parent): (u32, Tag, u32), styles: Styles) {
    b.create_element(id, tag).insert(parent, id, 0);
    for &(property, value) in styles {
        b.set_style(id, property, value);
    }
}

fn div(b: &mut B, id: u32, parent: u32, styles: Styles) {
    element(b, (id, Tag::Div, parent), styles);
}

/// An `img` `id` showing `src` with `alt`.
fn img(b: &mut B, (id, parent): (u32, u32), (src, alt): (&str, &str), styles: Styles) {
    element(b, (id, Tag::Img, parent), styles);
    b.set_attribute(id, A::Src, src)
        .set_attribute(id, A::Alt, alt);
}

const BLOCK: (P, &str) = (P::Display, "block");

fn build(name: &str, b: &mut B) {
    match name {
        // Natural size as a block, alone on a line, centered and overflowing
        // a right-aligned line (start-aligned instead).
        "natural" => {
            img(
                b,
                (2, 1),
                ("pattern-16.png", "Pattern"),
                &[BLOCK, (P::Margin, "4px")],
            );
            div(b, 3, 1, &[(P::BackgroundColor, GRAY)]);
            img(b, (4, 3), ("wide-20x10.png", "Wide"), &[]);
            div(b, 5, 1, &[(P::Width, "60px"), (P::TextAlign, "center")]);
            img(
                b,
                (6, 5),
                ("wide-20x10.png", "Centered"),
                &[(P::MarginLeft, "6px")],
            );
            div(b, 7, 1, &[(P::Width, "10px"), (P::TextAlign, "right")]);
            img(b, (8, 7), ("wide-20x10.png", ""), &[]);
        }
        // Explicit sizes: upscaling, a ratio-derived width, a stretched
        // image and padding with a border.
        "sizes" => {
            img(
                b,
                (2, 1),
                ("pattern-16.png", "Up"),
                &[BLOCK, (P::Width, "40px")],
            );
            img(
                b,
                (3, 1),
                ("wide-20x10.png", "Ratio"),
                &[BLOCK, (P::Height, "20px")],
            );
            img(
                b,
                (4, 1),
                ("pattern-16.png", "Stretched"),
                &[
                    BLOCK,
                    (P::Width, "30px"),
                    (P::Height, "12px"),
                    (P::Position, "absolute"),
                    (P::Top, "4px"),
                    (P::Left, "50px"),
                ],
            );
            img(
                b,
                (5, 1),
                ("wide-20x10.png", "Framed"),
                &[
                    BLOCK,
                    (P::Height, "20px"),
                    (P::Padding, "2px"),
                    (P::Border, "1px solid #00a000"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
        }
        // Downscaling through the levels, an odd-sized chain and different
        // scales per axis.
        "downscale" => {
            div(
                b,
                2,
                1,
                &[(P::Display, "flex"), (P::AlignItems, "flex-start")],
            );
            img(b, (3, 2), ("pattern-64.png", "A"), &[(P::Width, "20px")]);
            img(b, (4, 2), ("pattern-64.png", "B"), &[(P::Width, "13px")]);
            img(b, (5, 2), ("odd-45x37.png", "C"), &[(P::Width, "14px")]);
            img(
                b,
                (6, 2),
                ("wide-20x10.png", "D"),
                &[(P::Width, "8px"), (P::Height, "40px")],
            );
            img(b, (7, 2), ("odd-45x37.png", "E"), &[(P::Width, "27px")]);
            img(
                b,
                (8, 1),
                ("pattern-64.png", "F"),
                &[BLOCK, (P::Width, "50px")],
            );
        }
        // The natural ratio under minimum and maximum sizes.
        "constraints" => {
            div(b, 2, 1, &[(P::Width, "30px")]);
            img(
                b,
                (3, 2),
                ("pattern-64.png", "A"),
                &[BLOCK, (P::MaxWidth, "100%")],
            );
            img(
                b,
                (4, 1),
                ("wide-20x10.png", "B"),
                &[BLOCK, (P::MinWidth, "40px")],
            );
            div(b, 5, 1, &[(P::Width, "30px")]);
            img(
                b,
                (6, 5),
                ("wide-20x10.png", "C"),
                &[BLOCK, (P::Width, "100%"), (P::MaxHeight, "5px")],
            );
            img(
                b,
                (7, 1),
                ("pattern-64.png", "D"),
                &[BLOCK, (P::MaxWidth, "24px"), (P::MaxHeight, "10px")],
            );
        }
        // Flex and grid items: a stretched cross size gives the main size
        // by ratio, no shrinking below the natural size, grid start.
        "flex" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Display, "flex"),
                    (P::Width, "100px"),
                    (P::Height, "30px"),
                ],
            );
            img(b, (3, 2), ("wide-20x10.png", "A"), &[]);
            div(
                b,
                4,
                1,
                &[
                    (P::Display, "flex"),
                    (P::FlexDirection, "column"),
                    (P::Width, "40px"),
                ],
            );
            img(b, (5, 4), ("wide-20x10.png", "B"), &[]);
            div(b, 6, 1, &[(P::Display, "flex"), (P::Width, "10px")]);
            img(b, (7, 6), ("wide-20x10.png", "C"), &[]);
            div(
                b,
                8,
                1,
                &[
                    (P::Display, "grid"),
                    (P::Width, "100px"),
                    (P::Height, "20px"),
                ],
            );
            img(b, (9, 8), ("wide-20x10.png", "D"), &[]);
        }
        // Rounded images: the content-box curve inside padding and border.
        "radius" => {
            img(
                b,
                (2, 1),
                ("pattern-64.png", "A"),
                &[
                    BLOCK,
                    (P::Width, "40px"),
                    (P::BorderRadius, "12px"),
                    (P::Padding, "3px"),
                    (P::Border, "2px solid #00a000"),
                    (P::BackgroundColor, YELLOW),
                ],
            );
            img(
                b,
                (3, 1),
                ("pattern-16.png", "B"),
                &[BLOCK, (P::Width, "32px"), (P::BorderRadius, "50%")],
            );
        }
        // Opacity groups over backgrounds and an overlapping block.
        "opacity" => {
            div(b, 2, 1, &[(P::BackgroundColor, BLUE), (P::Padding, "4px")]);
            img(
                b,
                (3, 2),
                ("alpha-16.png", "A"),
                &[BLOCK, (P::Width, "32px"), (P::Opacity, "0.5")],
            );
            div(b, 4, 1, &[(P::BackgroundColor, RED), (P::Height, "20px")]);
            img(
                b,
                (5, 1),
                ("pattern-16.png", "B"),
                &[
                    BLOCK,
                    (P::Width, "40px"),
                    (P::MarginTop, "-10px"),
                    (P::Opacity, "0.7"),
                ],
            );
        }
        // `transform`: scaling the image, scaling its container, translating.
        "scale" => {
            img(
                b,
                (2, 1),
                ("pattern-64.png", "A"),
                &[BLOCK, (P::Width, "40px"), (P::Transform, "scale(0.5)")],
            );
            div(
                b,
                3,
                1,
                &[
                    (P::Transform, "scale(1.5)"),
                    (P::Width, "30px"),
                    (P::MarginLeft, "50px"),
                ],
            );
            img(b, (4, 3), ("pattern-16.png", "B"), &[BLOCK]);
            img(
                b,
                (5, 1),
                ("wide-20x10.png", "C"),
                &[
                    BLOCK,
                    (P::Transform, "translate(5px, 3px)"),
                    (P::BorderRadius, "4px"),
                ],
            );
        }
        // Every PNG color type over a colored background.
        "decode" => {
            div(b, 2, 1, &[(P::BackgroundColor, GREEN), (P::Padding, "2px")]);
            img(b, (3, 2), ("alpha-16.png", "A"), &[BLOCK]);
            img(b, (4, 2), ("rgb-16.png", "B"), &[BLOCK]);
            img(
                b,
                (5, 2),
                ("gray-12x8.png", "C"),
                &[BLOCK, (P::Width, "24px")],
            );
            img(
                b,
                (6, 2),
                ("palette-10x6.png", "D"),
                &[BLOCK, (P::Width, "30px")],
            );
        }
        // Fractional boxes snap the drawn image to whole pixels.
        "fractional" => {
            img(
                b,
                (2, 1),
                ("pattern-16.png", "A"),
                &[
                    BLOCK,
                    (P::Margin, "3.5px"),
                    (P::Width, "21.5px"),
                    (P::Height, "13.25px"),
                ],
            );
            div(
                b,
                3,
                1,
                &[(P::PaddingLeft, "0.3px"), (P::PaddingTop, "0.6px")],
            );
            img(
                b,
                (4, 3),
                ("wide-20x10.png", "B"),
                &[BLOCK, (P::Width, "33.3px")],
            );
        }
        // Positioned images stack above later blocks and over each other.
        "positioned" => {
            div(
                b,
                2,
                1,
                &[
                    (P::Position, "relative"),
                    (P::Height, "60px"),
                    (P::BackgroundColor, GRAY),
                ],
            );
            img(
                b,
                (3, 2),
                ("pattern-16.png", "A"),
                &[
                    (P::Position, "absolute"),
                    (P::Top, "10px"),
                    (P::Left, "20px"),
                    (P::Width, "32px"),
                    (P::ZIndex, "2"),
                ],
            );
            div(
                b,
                4,
                2,
                &[
                    (P::Position, "absolute"),
                    (P::Top, "20px"),
                    (P::Left, "30px"),
                    (P::Width, "30px"),
                    (P::Height, "30px"),
                    (P::BackgroundColor, BLUE),
                    (P::ZIndex, "1"),
                ],
            );
            img(
                b,
                (5, 2),
                ("wide-20x10.png", "B"),
                &[BLOCK, (P::Position, "relative"), (P::Top, "3px")],
            );
        }
        // Inline line boxes under the font's own line height, a larger
        // font and a line height smaller than the font.
        "inline-normal" => {
            div(b, 2, 1, &[(P::BackgroundColor, GRAY)]);
            img(b, (3, 2), ("wide-20x10.png", "A"), &[]);
            div(
                b,
                4,
                1,
                &[(P::FontSize, "40px"), (P::BackgroundColor, YELLOW)],
            );
            img(
                b,
                (5, 4),
                ("wide-20x10.png", "B"),
                &[(P::MarginBottom, "2px")],
            );
            div(
                b,
                6,
                1,
                &[(P::LineHeight, "4px"), (P::BackgroundColor, GRAY)],
            );
            img(b, (7, 6), ("pattern-16.png", "C"), &[]);
        }
        _ => unreachable!("unknown image case {name}"),
    }
}

/// The ids of the `img` elements of a snapshot tree.
fn image_ids(node: &serde_json::Value) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if node["tag"] == "img"
            && let Some(id) = node["id"].as_u64()
        {
            ids.push(id as u32);
        }
        if let Some(children) = node["children"].as_array() {
            pending.extend(children);
        }
    }
    ids.sort_unstable();
    ids
}

const CASES: [&str; 12] = [
    "natural",
    "sizes",
    "downscale",
    "constraints",
    "flex",
    "radius",
    "opacity",
    "scale",
    "decode",
    "fractional",
    "positioned",
    "inline-normal",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let scale = args
        .next()
        .map(|value| value.parse::<f32>())
        .transpose()?
        .unwrap_or(1.0);
    if !scale.is_finite() || !(MIN_SCALE..=MAX_SCALE).contains(&scale) {
        return Err("scale must be within 0.25..=8".into());
    }
    if args.next().is_some() {
        return Err("unexpected probe argument".into());
    }
    let sources = images()?;
    let set = Arc::new(ImageSet::decode(
        sources
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_slice())),
    )?);
    let mut font = TextMeasurer::new(FONT.to_vec(), FAMILY)?;
    let side = (SIDE as f32 * scale).round() as u32;
    let mut cases = Vec::new();
    for name in CASES {
        let mut b = B::new();
        b.set_style(1, P::FontFamily, "\"Varta Inline Fixture\"")
            .set_style(1, P::FontSize, "20px");
        if name != NORMAL_LINE_CASES {
            b.set_style(1, P::LineHeight, "20px");
        }
        build(name, &mut b);
        let mut view = View::default();
        view.set_images(Some(Arc::clone(&set)));
        view.commit_with_layout(
            &b.finish(),
            false,
            &mut Denied,
            SIDE as f32,
            SIDE as f32,
            Some((&mut font, MEDIUM)),
        )?;
        let named = |error: &dyn std::fmt::Display| format!("{name}: {error}");
        let geometry = view
            .layout_geometry_with_text(SIDE as f32, SIDE as f32, &mut font, MEDIUM)
            .map_err(|e| named(&e))?;
        let image = view
            .rasterize_scaled(side, side, scale, Some((&mut font, MEDIUM)))
            .map_err(|e| named(&e))?;
        let boxes = geometry
            .iter()
            .filter(|(id, _)| **id != 1)
            .map(|(&id, g)| {
                let r = g.rect;
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
        let snapshot = view.snapshot()["tree"].clone();
        let tree = view.accessibility_tree().map_err(|e| named(&e))?;
        let accessibility = image_ids(&snapshot)
            .into_iter()
            .map(|id| {
                let node = tree.nodes.get(&id);
                serde_json::json!({
                    "id":id,
                    "role":node.map(|node| node.role.as_str()),
                    "name":node.map(|node| node.name.as_str()),
                })
            })
            .collect::<Vec<_>>();
        cases.push(serde_json::json!({
            "name":name,"tree":snapshot,"boxes":boxes,"hits":hits,
            "accessibility":accessibility,
            "raster":image.pixels,"raster_size":[image.width,image.height],
            "roi":{"x":0,"y":0,"width":side,"height":side}
        }));
    }
    let images = sources
        .iter()
        .map(|(name, bytes)| ((*name).to_owned(), serde_json::json!(bytes)))
        .collect::<serde_json::Map<_, _>>();
    let output = serde_json::to_vec(&serde_json::json!({
        "family":FAMILY,"font":FONT,"scale":scale,"images":images,
        "viewport":{"width":SIDE,"height":SIDE},"cases":cases
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
