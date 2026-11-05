//! Packaged-font oracle data for `scripts/test-native-inline.mjs`: bold,
//! italic and monospace text laid out and rendered exactly as `varta-shell`
//! does, from the faces of a packaged artifact's resource section.
//!
//! Usage: `native_font_probe [SCALE] [ARTIFACT]`. Without an artifact the
//! probe packages the view crate's fixture faces (regular, bold and italic
//! faces of one family and a second, monospace family) beside a minimal
//! portable Wasm in a temporary directory and reads that artifact back. The
//! faces are loaded through `FontSet::from_resources` and
//! `TextMeasurer::new_with_fonts`, and painted with the macOS glyph
//! rasterizer of the shell; the JSON carries the same face bytes, weights
//! and styles for Chrome's `FontFace`.
#[cfg(target_os = "macos")]
mod probe {
    use std::path::{Path, PathBuf};
    use varta_artifact::{Artifact, FontFace, FontStyle, GenericFamily, Resources};
    use varta_common::{
        Manifest,
        view::{CssProperty as P, Tag},
    };
    use varta_desktop::shell::FontSet;
    use varta_view::{Access, BatchWriter, Broker, RasterImage, Renderer, View};

    const SANS: &str = "Varta Fixture 400";
    const MONO: &str = "Varta Fixture 900";
    const REGULAR: &[u8] = include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf");
    const BOLD: &[u8] = include_bytes!("../../varta-view/tests/fixtures/bold-900.ttf");
    const ITALIC: &[u8] = include_bytes!("../../varta-view/tests/fixtures/italic-900.ttf");
    const MONO_FACE: &[u8] = include_bytes!("../../varta-view/tests/fixtures/advance-900.ttf");
    const BOLD_WEIGHT: u16 = 700;
    const NORMAL_WEIGHT: u16 = 400;
    /// A minimal portable module: the artifact only carries the fonts.
    const EMPTY_MODULE: &[u8] = b"\0asm\x01\0\0\0";
    /// The CSS viewport side and the text size, in CSS pixels.
    const VIEWPORT: f32 = 100.0;
    const TEXT_SIZE: f32 = 20.0;
    const MIN_SCALE: f32 = 0.25;
    const MAX_SCALE: f32 = 8.0;
    /// The hit-test sample grid, in CSS pixels.
    const HIT_ROWS: std::ops::Range<u32> = 0..70;
    const HIT_ROW_STEP: usize = 5;
    const HIT_COLUMNS: std::ops::Range<u32> = 0..50;
    const HIT_COLUMN_STEP: usize = 3;
    /// Hit samples sit a quarter pixel into each sample square.
    const HIT_OFFSET: f32 = 0.25;
    const LINE: u32 = 2;
    const FIRST_SPAN: u32 = 10;
    const FIRST_TEXT: u32 = 30;

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

    fn face(
        family: &str,
        weight: u16,
        style: FontStyle,
        generics: &[GenericFamily],
        data: &[u8],
    ) -> FontFace {
        FontFace {
            family: family.into(),
            weight,
            style,
            generics: generics.to_vec(),
            data: data.to_vec(),
        }
    }

    /// Package the fixture faces in a temporary directory and read the
    /// artifact back, as the shell would.
    fn packaged_fixture() -> Result<Resources, Box<dyn std::error::Error>> {
        let resources = Resources::new(vec![
            face(SANS, NORMAL_WEIGHT, FontStyle::Normal, &[], REGULAR),
            face(SANS, BOLD_WEIGHT, FontStyle::Normal, &[], BOLD),
            face(SANS, NORMAL_WEIGHT, FontStyle::Italic, &[], ITALIC),
            face(
                MONO,
                NORMAL_WEIGHT,
                FontStyle::Normal,
                &[GenericFamily::Monospace],
                MONO_FACE,
            ),
        ])?;
        let directory =
            std::env::temp_dir().join(format!("varta-native-font-probe-{}", std::process::id()));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("fonts.wasm");
        Artifact {
            wasm: EMPTY_MODULE.to_vec(),
            manifest: Manifest::new(&[]),
        }
        .write_wasm_with(&path, &resources)?;
        let read = read(&path);
        std::fs::remove_dir_all(&directory)?;
        read
    }

    fn read(path: &Path) -> Result<Resources, Box<dyn std::error::Error>> {
        Ok(Artifact::read_packaged(path)?.resources)
    }

    /// One line of `A`s in the case's faces.
    fn case_view(name: &str) -> Result<View, Box<dyn std::error::Error>> {
        let spans: &[(&str, Option<(P, &str)>)] = match name {
            "bold" => &[("A A", Some((P::FontWeight, "700")))],
            "italic" => &[("A A", Some((P::FontStyle, "italic")))],
            "mono" => &[("A A", Some((P::FontFamily, "\"Varta Fixture 900\"")))],
            "mixed" => &[
                ("A", None),
                ("A", Some((P::FontWeight, "700"))),
                ("A", Some((P::FontStyle, "italic"))),
                ("A", Some((P::FontFamily, "\"Varta Fixture 900\""))),
            ],
            _ => return Err(format!("unknown case {name}").into()),
        };
        let mut batch = BatchWriter::new();
        batch.create_element(LINE, Tag::Div).insert(1, LINE, 0);
        for (property, value) in [
            (P::Width, "100px"),
            (P::FontFamily, "\"Varta Fixture 400\""),
            (P::FontSize, "20px"),
            (P::LineHeight, "20px"),
            (P::Color, "#000"),
        ] {
            batch.set_style(LINE, property, value);
        }
        for (index, (text, style)) in (0u32..).zip(spans) {
            let (span, node) = (FIRST_SPAN + index, FIRST_TEXT + index);
            batch
                .create_element(span, Tag::Span)
                .insert(LINE, span, 0)
                .create_text(node, text)
                .insert(span, node, 0);
            if let Some((property, value)) = style {
                batch.set_style(span, *property, value);
            }
        }
        let mut view = View::default();
        view.commit(&batch.finish(), false, &mut Denied)?;
        Ok(view)
    }

    fn render(
        view: &View,
        fonts: &FontSet,
        side: u32,
        scale: f32,
    ) -> Result<RasterImage, Box<dyn std::error::Error>> {
        let mut renderer = Renderer::new(Some(fonts.measurer()?), TEXT_SIZE)?;
        renderer.set_glyph_rasterizer(Some(Box::new(
            varta_desktop::text_raster::CoreTextRasterizer::default(),
        )));
        Ok(renderer.render(view, side, side, scale)?)
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let scale = args
            .next()
            .map(|s| s.parse::<f32>())
            .transpose()?
            .unwrap_or(1.0);
        if !scale.is_finite() || !(MIN_SCALE..=MAX_SCALE).contains(&scale) {
            return Err("scale must be within 0.25..=8".into());
        }
        let resources = match args.next() {
            Some(path) => read(&PathBuf::from(path))?,
            None => packaged_fixture()?,
        };
        if args.next().is_some() {
            return Err("unexpected probe argument".into());
        }
        let fonts = FontSet::from_resources(&resources).ok_or("the artifact ships no fonts")?;
        let mut measurer = fonts.measurer()?;
        let side = (VIEWPORT * scale).round() as u32;
        let mut cases = Vec::new();
        for name in ["bold", "italic", "mono", "mixed"] {
            let view = case_view(name)?;
            let geometry =
                view.layout_boxes_with_text(VIEWPORT, VIEWPORT, &mut measurer, TEXT_SIZE)?;
            let image = render(&view, &fonts, side, scale)?;
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
            let hits = HIT_ROWS
                .step_by(HIT_ROW_STEP)
                .flat_map(|y| HIT_COLUMNS.step_by(HIT_COLUMN_STEP).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let (x, y) = (x as f32 + HIT_OFFSET, y as f32 + HIT_OFFSET);
                    let target = image.hit_test(x * scale, y * scale);
                    let boundary = [-1.0, 1.0].into_iter().any(|dx| {
                        [-1.0, 1.0]
                            .into_iter()
                            .any(|dy| image.hit_test((x + dx) * scale, (y + dy) * scale) != target)
                    });
                    serde_json::json!({"x": x - HIT_OFFSET, "y": y - HIT_OFFSET, "id": target, "boundary": boundary})
                })
                .collect::<Vec<_>>();
            let line = geometry.get(&LINE).ok_or("the line has no box")?;
            cases.push(serde_json::json!({
                "name": name,
                "tree": view.snapshot()["tree"],
                "boxes": boxes,
                "hits": hits,
                "samples": [],
                "raster": image.pixels,
                "raster_size": [image.width, image.height],
                "roi": {
                    "x": line.x * scale,
                    "y": line.y * scale,
                    "width": line.width * scale,
                    "height": line.height * scale,
                },
            }));
        }
        let faces = resources
            .fonts()
            .iter()
            .map(|face| {
                serde_json::json!({
                    "family": face.family,
                    "weight": face.weight,
                    "style": face.style.name(),
                    "font": face.data,
                })
            })
            .collect::<Vec<_>>();
        let output = serde_json::to_string(&serde_json::json!({
            "fonts": faces,
            "scale": scale,
            "text_rasterizer": "host",
            "cases": cases,
        }))?;
        use std::io::Write;
        std::io::stdout().lock().write_all(output.as_bytes())?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run()
}
#[cfg(not(target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("native font probe requires an implemented platform rasterizer".into())
}
