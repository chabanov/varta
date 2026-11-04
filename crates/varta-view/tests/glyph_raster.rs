//! Host coverage shares the existing scene, source limits and input geometry.
use std::sync::{Arc, Mutex};
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter as B, Broker, GlyphRasterBounds as Bounds, GlyphRasterMask as Mask,
    GlyphRasterRequest as Request, GlyphRasterizer, PreparedFrame, Renderer, TextMeasurer, View,
};
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
fn commit(v: &mut View, b: B) {
    v.commit(&b.finish(), false, &mut Denied).unwrap();
}
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/inline.ttf").to_vec(),
        "Varta Inline Fixture",
    )
    .unwrap()
}
fn fixture() -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "40px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
        .set_style(2, P::FontSize, "20px")
        .set_style(2, P::LineHeight, "20px")
        .set_style(2, P::Color, "#ff0000")
        .create_text(3, "A")
        .insert(2, 3, 0);
    let mut v = View::default();
    commit(&mut v, b);
    v
}
#[derive(Default)]
struct Calls {
    bounds: usize,
    masks: usize,
    phases: Vec<(u32, u8)>,
}
#[derive(Clone, Copy)]
enum Mode {
    Coverage,
    Decline,
    InvalidBounds,
    WrongMask,
    ExcessiveCache,
}
struct Fake {
    calls: Arc<Mutex<Calls>>,
    mode: Mode,
}
impl GlyphRasterizer for Fake {
    fn bounds(&mut self, r: &Request<'_>) -> Option<Bounds> {
        use skrifa::MetadataProvider;
        assert_eq!(r.font_index, 0);
        assert_eq!(r.font_data, include_bytes!("fixtures/inline.ttf"));
        let font = skrifa::FontRef::new(r.font_data).unwrap();
        assert_eq!(r.glyph, font.charmap().map('A').unwrap().to_u32());
        let mut calls = self.calls.lock().unwrap();
        calls.bounds += 1;
        calls.phases.push((r.size.to_bits(), r.phase_x));
        match self.mode {
            Mode::Decline => None,
            Mode::InvalidBounds => Some(Bounds {
                left: 0,
                top: 0,
                width: u32::MAX,
                height: 2,
            }),
            _ => Some(Bounds {
                left: 0,
                top: -4,
                width: 2,
                height: 2,
            }),
        }
    }
    fn rasterize(&mut self, _: &Request<'_>, mut bounds: Bounds) -> Option<Mask> {
        self.calls.lock().unwrap().masks += 1;
        if matches!(self.mode, Mode::WrongMask) {
            bounds.left += 1;
        }
        Some(Mask::new(bounds, vec![255, 128, 64, 0]).unwrap())
    }
    fn retained_bytes(&self) -> usize {
        if matches!(self.mode, Mode::ExcessiveCache) {
            usize::MAX
        } else {
            0
        }
    }
}
fn renderer(mode: Mode) -> (Renderer, Arc<Mutex<Calls>>) {
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut r = Renderer::new(Some(font()), 20.0).unwrap();
    r.set_glyph_rasterizer(Some(Box::new(Fake {
        mode,
        calls: calls.clone(),
    })));
    (r, calls)
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    image.pixels[((y * image.width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
#[test]
fn host_coverage_uses_disjoint_existing_primitives_and_retained_damage() {
    let v = fixture();
    let before = v.snapshot();
    let (mut r, calls) = renderer(Mode::Coverage);
    let image = r.render(&v, 80, 80, 1.0).unwrap();
    assert_eq!(pixel(&image, 0, 14), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 1, 14), [255, 127, 127, 255]);
    assert_eq!(pixel(&image, 0, 15), [255, 191, 191, 255]);
    assert_eq!(pixel(&image, 1, 15), [255, 255, 255, 255]);
    assert_eq!(r.stats().raster_glyphs, 1);
    assert_eq!(r.stats().outline_glyphs, 0);
    assert_eq!(r.render(&v, 80, 80, 1.0).unwrap(), image);
    assert_eq!(r.stats().dirty_pixels, 0);
    {
        let counts = calls.lock().unwrap();
        assert_eq!((counts.bounds, counts.masks), (1, 1));
    }
    let PreparedFrame::Vector { metadata, scene } = r.prepare(&v, 80, 80, 1.0).unwrap() else {
        panic!("coverage uses bounded vector commands")
    };
    assert_eq!(scene.rasterize(metadata).unwrap(), image);
    assert_eq!(v.snapshot(), before);
}
#[test]
fn refusals_invalid_bounds_and_inconsistent_masks_keep_the_portable_fallback() {
    let v = fixture();
    let expected = v.rasterize_with_text(80, 80, &mut font(), 20.0).unwrap();
    for mode in [
        Mode::Decline,
        Mode::InvalidBounds,
        Mode::WrongMask,
        Mode::ExcessiveCache,
    ] {
        let (mut r, _) = renderer(mode);
        let actual = r.render(&v, 80, 80, 1.0).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(r.stats().raster_glyphs, 0);
        assert_eq!(r.stats().outline_glyphs, 1);
        assert_eq!(r.render(&v, 80, 80, 1.0).unwrap(), expected);
    }
}
#[test]
fn rasterizer_replacement_and_quarter_phases_invalidate_pixels_without_quantizing_hits() {
    let mut v = fixture();
    let (mut r, calls) = renderer(Mode::Coverage);
    for (scale, margin, phase) in [
        (1.0, "0px", 0),
        (1.0, "0.25px", 1),
        (1.25, "0.25px", 1),
        (2.0, "0.25px", 2),
    ] {
        let mut b = B::new();
        b.set_style(2, P::MarginLeft, margin);
        commit(&mut v, b);
        let side = (80.0 * scale) as u32;
        let image = r.render(&v, side, side, scale).unwrap();
        let raw = v
            .layout_boxes_with_text(80.0, 80.0, &mut font(), 20.0)
            .unwrap()[&2];
        assert_eq!(image.node_bounds_map()[&2].x, raw.x * scale);
        assert_eq!(
            calls.lock().unwrap().phases.last().copied(),
            Some(((20.0 * scale).to_bits(), phase))
        );
        let PreparedFrame::Vector { metadata, scene } = r.prepare(&v, side, side, scale).unwrap()
        else {
            panic!("coverage vectors")
        };
        assert_eq!(scene.rasterize(metadata).unwrap(), image);
    }
    r.set_glyph_rasterizer(None);
    assert_eq!(
        r.render(&v, 160, 160, 2.0).unwrap(),
        v.rasterize_scaled(160, 160, 2.0, Some((&mut font(), 20.0)))
            .unwrap()
    );
    assert_eq!(r.stats().raster_glyphs, 0);
    assert_eq!(r.stats().outline_glyphs, 1);
}
#[test]
fn invisible_groups_keep_targets_without_requesting_host_pixels() {
    let mut v = fixture();
    let mut b = B::new();
    b.set_style(2, P::Opacity, "0");
    commit(&mut v, b);
    let (mut r, calls) = renderer(Mode::Coverage);
    let image = r.render(&v, 80, 80, 1.0).unwrap();
    assert!(image.pixels.iter().all(|c| *c == 255));
    assert_eq!(image.hit_test(1.0, 14.0), Some(2));
    assert_eq!(calls.lock().unwrap().bounds, 0);
}
