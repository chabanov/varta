use super::*;
use crate::{Access, BatchWriter as B, Broker, LayoutError, PaintError, Renderer, View, Violation};
use varta_common::view::images::MAX_IMAGE_DIMENSION;
use varta_common::view::{Attribute as A, CssProperty as P};

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

/// An 8-bit PNG of `pixels` in `color`.
fn encode(width: u32, height: u32, color: png::ColorType, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    out
}

/// A `width` x `height` RGBA PNG of one color.
fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let pixels: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
    encode(width, height, png::ColorType::Rgba, &pixels)
}

fn set(images: &[(&str, &[u8])]) -> Arc<ImageSet> {
    Arc::new(ImageSet::decode(images.iter().copied()).unwrap())
}

const RED: [u8; 4] = [255, 0, 0, 255];
const VIEWPORT: u32 = 100;

/// A view with `red.png` (64x64 red) and one `img` of `styles` under the
/// root, or under a block `parent` holding `siblings` text first.
fn view(styles: &[(P, &str)], text_sibling: bool) -> View {
    let red = solid(64, 64, RED);
    let mut view = View::default();
    view.set_images(Some(set(&[("red.png", &red)])));
    let mut b = B::new();
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    if text_sibling {
        b.create_text(4, "A").insert(2, 4, 0);
    }
    b.create_element(3, Tag::Img)
        .insert(2, 3, 0)
        .set_attribute(3, A::Src, "red.png")
        .set_attribute(3, A::Alt, "Red");
    for &(property, value) in styles {
        b.set_style(3, property, value);
    }
    view.commit(&b.finish(), false, &mut Denied).unwrap();
    view
}

#[test]
fn every_color_type_decodes_to_premultiplied_rgba() {
    let rgba = encode(
        2,
        1,
        png::ColorType::Rgba,
        &[200, 100, 50, 128, 1, 2, 3, 255],
    );
    let rgb = encode(1, 1, png::ColorType::Rgb, &[10, 20, 30]);
    let gray = encode(1, 1, png::ColorType::GrayscaleAlpha, &[200, 51]);
    let images = ImageSet::decode([
        ("rgba.png", rgba.as_slice()),
        ("rgb.png", rgb.as_slice()),
        ("gray.png", gray.as_slice()),
    ])
    .unwrap();
    assert_eq!(images.len(), 3);
    assert_eq!(images.size("rgba.png"), Some((2, 1)));
    let pixels = |name| images.get(name).unwrap().levels[0].pixels.clone();
    // Skia's rounding premultiplication: 200 * 128 / 255 = 100.39 -> 100.
    assert_eq!(pixels("rgba.png"), [100, 50, 25, 128, 1, 2, 3, 255]);
    assert_eq!(pixels("rgb.png"), [10, 20, 30, 255]);
    assert_eq!(pixels("gray.png"), [40, 40, 40, 51]);
    assert!(images.retained_bytes() > 0);
}

#[test]
fn level_chains_follow_skia_kernels_and_sizes() {
    // 5x2: an odd width takes 1-2-1 taps, an even height a box pair.
    let row: Vec<u8> = [0u8, 40, 80, 120, 160]
        .iter()
        .flat_map(|&v| [v, v, v, 255])
        .collect();
    let pixels: Vec<u8> = row.iter().chain(row.iter()).copied().collect();
    let png = encode(5, 2, png::ColorType::Rgba, &pixels);
    let images = set(&[("ramp.png", &png)]);
    let levels = &images.get("ramp.png").unwrap().levels;
    let sizes: Vec<_> = levels.iter().map(|l| (l.width, l.height)).collect();
    assert_eq!(sizes, [(5, 2), (2, 1), (1, 1)]);
    // (0 + 2*40 + 80) / 4 = 40 and (80 + 2*120 + 160) / 4 = 120.
    assert_eq!(levels[1].pixels, [40, 40, 40, 255, 120, 120, 120, 255]);
    // An even width of two keeps a box pair: (40 + 120) / 2 = 80.
    assert_eq!(levels[2].pixels, [80, 80, 80, 255]);
    // A one-pixel axis keeps its single row.
    let tall = encode(1, 4, png::ColorType::Rgba, &[0; 16]);
    let images = set(&[("tall.png", &tall)]);
    let sizes: Vec<_> = images
        .get("tall.png")
        .unwrap()
        .levels
        .iter()
        .map(|l| (l.width, l.height))
        .collect();
    assert_eq!(sizes, [(1, 4), (1, 2), (1, 1)]);
}

#[test]
fn sets_are_bounded_named_and_checked_before_decoding() {
    let red = solid(2, 2, RED);
    let refused = |images: &[(&str, &[u8])]| ImageSet::decode(images.iter().copied()).unwrap_err();
    for name in [
        "",
        "../red.png",
        "/red.png",
        "a b.png",
        "https://a.test/r.png",
    ] {
        assert_eq!(refused(&[(name, &red)]), ImageError::Name(name.into()));
    }
    assert_eq!(
        refused(&[("red.png", &red), ("red.png", &red)]),
        ImageError::Duplicate("red.png".into())
    );
    let many: Vec<(String, &[u8])> = (0..=MAX_IMAGES)
        .map(|i| (format!("{i}.png"), red.as_slice()))
        .collect();
    assert_eq!(
        ImageSet::decode(many.iter().map(|(name, bytes)| (name.as_str(), *bytes))).unwrap_err(),
        ImageError::Count(MAX_IMAGES + 1)
    );
    let mut animated = red.clone();
    // The first chunk after the header becomes an animation control chunk.
    let header_end = 8 + 25;
    animated[header_end + 4..header_end + 8].copy_from_slice(b"acTL");
    assert!(matches!(
        refused(&[("a.png", &animated)]),
        ImageError::Refused {
            source: PngRefusal::Animated,
            ..
        }
    ));
    let mut corrupt = red.clone();
    let last_data = corrupt.len() - 12 - 5;
    corrupt[last_data] ^= 0xff;
    assert_eq!(
        refused(&[("a.png", &corrupt)]),
        ImageError::Malformed {
            name: "a.png".into()
        }
    );
    let mut wide = red.clone();
    wide[16..20].copy_from_slice(&(MAX_IMAGE_DIMENSION + 1).to_be_bytes());
    assert!(matches!(
        refused(&[("a.png", &wide)]),
        ImageError::Refused {
            source: PngRefusal::Dimensions { .. },
            ..
        }
    ));
    // Each image fits alone; together they exceed the decoded total.
    let big = solid(2048, 1024, RED);
    assert_eq!(
        refused(&[("a.png", &big), ("b.png", &big), ("c.png", &big)]),
        ImageError::Decoded
    );
}

#[test]
fn a_view_with_images_refuses_unpackaged_sources_atomically() {
    let red = solid(1, 1, RED);
    let mut view = View::default();
    view.set_images(Some(set(&[("red.png", &red)])));
    let mut b = B::new();
    b.create_element(2, Tag::Img)
        .insert(1, 2, 0)
        .set_attribute(2, A::Src, "missing.png");
    let error = view.commit(&b.finish(), false, &mut Denied).unwrap_err();
    assert!(matches!(
        error,
        crate::ViewError::Invalid {
            violation: Violation::UnpackagedImage(ref name),
            ..
        } if name == "missing.png"
    ));
    assert_eq!(view.commits(), 0);
    // A headless view without images keeps the grammar-only check and
    // refuses to lay the image out.
    let mut headless = View::default();
    headless.commit(&b.finish(), false, &mut Denied).unwrap();
    assert!(matches!(
        headless.layout_boxes(VIEWPORT as f32, VIEWPORT as f32),
        Err(LayoutError::Unsupported(2))
    ));
}

#[test]
fn block_images_size_from_their_ratio_without_a_font() {
    let view = view(&[(P::Display, "block"), (P::Width, "32px")], false);
    let boxes = view.layout_boxes(VIEWPORT as f32, VIEWPORT as f32).unwrap();
    let image = boxes[&3];
    assert_eq!(
        (image.x, image.y, image.width, image.height),
        (0.0, 0.0, 32.0, 32.0)
    );
    let raster = view.rasterize_boxes(VIEWPORT, VIEWPORT).unwrap();
    let at = |x: u32, y: u32| raster.pixels[((y * VIEWPORT + x) * 4) as usize..][..4].to_vec();
    assert_eq!(at(16, 16), RED);
    assert_eq!(at(33, 16)[3], 0);
    assert_eq!(raster.hit_test(16.0, 16.0), Some(3));
}

#[test]
fn unsupported_image_placements_and_styles_are_refused() {
    // Text beside an inline image would share its line box.
    let mixed = view(&[], true);
    assert!(matches!(
        mixed.layout_boxes(VIEWPORT as f32, VIEWPORT as f32),
        Err(LayoutError::Unsupported(_))
    ));
    for styles in [
        &[(P::Display, "flex")][..],
        &[(P::Display, "block"), (P::Overflow, "hidden")],
    ] {
        let refused = view(styles, false);
        assert!(
            matches!(
                refused.layout_boxes(VIEWPORT as f32, VIEWPORT as f32),
                Err(LayoutError::Unsupported(3))
            ),
            "{styles:?}"
        );
    }
    // An inline image needs its parent's font for its line box.
    let inline = view(&[], false);
    assert!(matches!(
        inline.rasterize_boxes(VIEWPORT, VIEWPORT),
        Err(PaintError::Layout(LayoutError::Unsupported(3)))
    ));
}

#[test]
fn a_rounded_image_at_a_fractional_scale_covers_its_far_edges() {
    let view = view(
        &[
            (P::Display, "block"),
            (P::Width, "40px"),
            (P::BorderRadius, "12px"),
            (P::Padding, "5px"),
        ],
        false,
    );
    let side = 150;
    let raster = view.rasterize_scaled(side, side, 1.5, None).unwrap();
    let alpha = |x: u32, y: u32| raster.pixels[((y * side + x) * 4 + 3) as usize];
    // The content box spans device pixels 7.5..67.5 on both axes: the near
    // and far edge pixels are half covered alike.
    let middle = 37;
    assert!(alpha(7, middle).abs_diff(alpha(67, middle)) <= 1);
    assert!(alpha(middle, 7).abs_diff(alpha(middle, 67)) <= 1);
    assert!((120..=136).contains(&alpha(67, middle)));
}

#[test]
fn image_frames_take_the_explicit_software_path() {
    let view = view(&[(P::Display, "block")], false);
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    let frame = renderer.prepare(&view, VIEWPORT, VIEWPORT, 1.0).unwrap();
    assert!(matches!(frame, crate::PreparedFrame::Raster(_)));
    // The software scene is retained: an unchanged frame has no damage.
    renderer.prepare(&view, VIEWPORT, VIEWPORT, 1.0).unwrap();
    assert_eq!(renderer.damage(), None);
}
