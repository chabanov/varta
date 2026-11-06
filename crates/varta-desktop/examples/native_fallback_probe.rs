//! System-font oracle data for `scripts/test-native-inline.mjs`: lines whose
//! characters the generic families' faces lack, laid out and rendered as
//! `varta-shell --system-fonts` does (`FontSet::system`, the platform's
//! fallback cascade and the shell's glyph rasterizer). The JSON carries no
//! font bytes (`"system_fonts": true`): Chrome on the same machine resolves
//! the same generic families and fallback faces itself, so its boxes and
//! pixels are the oracle for this machine only.
//!
//! Usage: `native_fallback_probe [SCALE]`.
#[cfg(target_os = "macos")]
mod probe {
    use varta_common::view::{CssProperty as P, Tag};
    use varta_desktop::shell::FontSet;
    use varta_view::{Access, BatchWriter, Broker, RasterImage, Renderer, View};

    /// The CSS viewport in CSS pixels.
    const VIEWPORT_WIDTH: f32 = 240.0;
    const VIEWPORT_HEIGHT: f32 = 40.0;
    /// Chrome's default proportional size.
    const MEDIUM_SIZE: f32 = 16.0;
    const MIN_SCALE: f32 = 0.25;
    const MAX_SCALE: f32 = 8.0;
    const LINE: u32 = 2;
    const FIRST_SPAN: u32 = 10;
    const FIRST_TEXT: u32 = 30;

    /// Each case: its name, the line's family list, its runs (text and an
    /// optional weight) and whether its pixels are compared.
    type Case = (
        &'static str,
        &'static str,
        &'static [(&'static str, Option<&'static str>)],
        bool,
    );
    /// U+2699 (the console's settings icon) lacks a face in the UI font,
    /// Helvetica and Times and draws with Menlo; U+2192, U+2713, U+2605 and
    /// U+2318 draw with Lucida Grande, Times New Roman, Zapf Dingbats,
    /// Hiragino Sans or Songti SC depending on the primary font and weight.
    /// In `serif-symbols` the star right after the check mark takes the
    /// check mark's face (Blink falls back over the adjacent range by its
    /// first character) and the separate star Songti SC. `ui-plain` is the
    /// UI line without a fallback. The UI font has an `opsz` axis, which the
    /// CoreText rasterizer declines (its glyphs take the portable outline
    /// path), so the UI lines compare geometry only.
    const CASES: [Case; 7] = [
        (
            "ui-plain",
            "system-ui, sans-serif",
            &[("Settings", None)],
            false,
        ),
        (
            "ui-gear",
            "system-ui, sans-serif",
            &[("Settings \u{2699}", None)],
            false,
        ),
        (
            "ui-gear-bold",
            "system-ui, sans-serif",
            &[("Settings ", None), ("\u{2699} \u{2699}", Some("700"))],
            false,
        ),
        (
            "sans-symbols",
            "sans-serif",
            &[
                ("a \u{2192} b \u{2713} \u{2318}", None),
                (" \u{2192}", Some("700")),
            ],
            true,
        ),
        (
            "serif-symbols",
            "serif",
            &[("x \u{2192} y \u{2713}\u{2605} \u{2605} \u{2699}", None)],
            true,
        ),
        (
            "sans-star",
            "sans-serif",
            &[("\u{2605} \u{2605}", None), (" \u{2605}", Some("700"))],
            true,
        ),
        (
            "mono-gear",
            "monospace",
            &[("cfg \u{2699} \u{2192}", None)],
            true,
        ),
    ];

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

    fn case_view(
        family: &str,
        runs: &[(&str, Option<&str>)],
    ) -> Result<View, Box<dyn std::error::Error>> {
        let mut batch = BatchWriter::new();
        batch
            .create_element(LINE, Tag::Div)
            .insert(1, LINE, 0)
            .set_style(LINE, P::FontFamily, family)
            .set_style(LINE, P::LineHeight, "24px")
            .set_style(LINE, P::Color, "#000");
        for (index, (text, weight)) in (0u32..).zip(runs) {
            let (span, node) = (FIRST_SPAN + index, FIRST_TEXT + index);
            batch.create_element(span, Tag::Span).insert(LINE, span, 0);
            if let Some(weight) = weight {
                batch.set_style(span, P::FontWeight, weight);
            }
            batch.create_text(node, text).insert(span, node, 0);
        }
        let mut view = View::default();
        view.commit(&batch.finish(), false, &mut Denied)?;
        Ok(view)
    }

    fn render(
        view: &View,
        fonts: &FontSet,
        scale: f32,
    ) -> Result<RasterImage, Box<dyn std::error::Error>> {
        let mut renderer = Renderer::new(Some(fonts.measurer()?), MEDIUM_SIZE)?;
        renderer.set_glyph_rasterizer(Some(Box::new(
            varta_desktop::text_raster::CoreTextRasterizer::default(),
        )));
        Ok(renderer.render(
            view,
            (VIEWPORT_WIDTH * scale).round() as u32,
            (VIEWPORT_HEIGHT * scale).round() as u32,
            scale,
        )?)
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
        if args.next().is_some() {
            return Err("unexpected probe argument".into());
        }
        let fonts = FontSet::system()?;
        let mut measurer = fonts.measurer()?;
        let mut cases = Vec::new();
        for (name, family, runs, pixels) in CASES {
            let view = case_view(family, runs)?;
            let geometry = view.layout_boxes_with_text(
                VIEWPORT_WIDTH,
                VIEWPORT_HEIGHT,
                &mut measurer,
                MEDIUM_SIZE,
            )?;
            let image = render(&view, &fonts, scale)?;
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
            let line = geometry.get(&LINE).ok_or("the line has no box")?;
            cases.push(serde_json::json!({
                "name": name,
                "tree": view.snapshot()["tree"],
                "boxes": boxes,
                "hits": [],
                "samples": [],
                "geometry_only": !pixels,
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
        let output = serde_json::to_string(&serde_json::json!({
            "system_fonts": true,
            "scale": scale,
            "viewport": {"width": VIEWPORT_WIDTH, "height": VIEWPORT_HEIGHT},
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
    Err("native fallback probe requires an implemented platform rasterizer".into())
}
