//! Same strict browser oracle, using the product's native glyph rasterizer.
//!
//! `native_text_probe [SCALE] [decorations]` runs the inline corpus.
//! `native_text_probe faces variable|variable-names|collection [SCALE]
//! [-AppleLanguages (LANGUAGE)]` renders the desktop face fixtures (a `wght`
//! variable font at several weights, the same face whose Mac and Windows axis
//! names differ, or member 1 of a collection) and fails unless every glyph
//! used native coverage. CoreFoundation reads the trailing pair as this
//! process's preferred languages, which CoreText's axis names follow, so the
//! face variants can be checked under a language other than the user's.
#[cfg(target_os = "macos")]
#[path = "../../varta-view/examples/native_inline_probe/probe.rs"]
mod probe;
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some(faces::MODE) {
        return faces::run();
    }
    probe::run(Some(|| {
        Box::new(varta_desktop::text_raster::CoreTextRasterizer::default())
    }))
}
#[cfg(not(target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("native text probe requires an implemented platform rasterizer".into())
}

#[cfg(target_os = "macos")]
mod faces {
    use serde_json::{Value, json};
    use varta_common::view::{CssProperty as P, Tag};
    use varta_desktop::text_raster::{CoreTextRasterizer, standalone_face};
    use varta_view::{Access, BatchWriter as B, Broker, Renderer, TextMeasurer, View};

    pub const MODE: &str = "faces";
    const KINDS: &str = "expected variable, variable-names or collection";
    /// CoreFoundation's argument-domain option for the preferred languages,
    /// followed by one property-list array such as `(uk)`.
    const LANGUAGES_OPTION: &str = "-AppleLanguages";
    const VARIABLE: &[u8] = include_bytes!("../tests/fixtures/fonts/variable.ttf");
    /// CoreGraphics names this face's axis by its Mac record ("Gewicht").
    const VARIABLE_NAMES: &[u8] = include_bytes!("../tests/fixtures/fonts/variable-names.ttf");
    const COLLECTION: &[u8] = include_bytes!("../tests/fixtures/fonts/collection.ttc");
    const VARIABLE_FAMILY: &str = "Varta Variable Fixture";
    const COLLECTION_FAMILY: &str = "Varta Collection One";
    /// The collection member whose family is `COLLECTION_FAMILY`.
    const COLLECTION_MEMBER: u32 = 1;
    /// Admitted CSS weights; the fixture's default instance is 400.
    const WEIGHTS: [&str; 4] = ["400", "500", "600", "700"];
    const VIEWPORT: f32 = 100.0;
    const BASE_FONT_SIZE: f32 = 20.0;
    const TEXTS: [&str; 2] = ["AB BA", "BAAB A"];

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

    /// Chrome registers its FontFace without a weight range, which clamps the
    /// `wght` axis to 400; the browser side therefore names the same instance
    /// explicitly with `font-variation-settings` at weight 400 (no synthesis).
    fn browser_instance(node: &mut Value) {
        if let Some(style) = node.get_mut("style").and_then(Value::as_object_mut)
            && let Some(weight) = style.get("font-weight").and_then(Value::as_str)
        {
            let weight = weight.to_owned();
            style.insert("font-weight".into(), json!("400"));
            style.insert(
                "font-variation-settings".into(),
                json!(format!("\"wght\" {weight}")),
            );
        }
        if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
            children.iter_mut().for_each(browser_instance);
        }
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(2).collect::<Vec<_>>();
        let languages = match args.iter().position(|arg| arg == LANGUAGES_OPTION) {
            Some(at) if at + 2 == args.len() => args.drain(at..).nth(1),
            Some(_) => return Err(format!("{LANGUAGES_OPTION} takes one final value").into()),
            None => None,
        };
        let mut args = args.into_iter();
        let kind = args.next().ok_or(KINDS)?;
        let scale = args
            .next()
            .map(|s| s.parse::<f32>())
            .transpose()?
            .unwrap_or(1.0);
        if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
            return Err("scale must be within 0.25..=8".into());
        }
        if args.next().is_some() {
            return Err("unexpected probe argument".into());
        }
        // Varta renders from the file the application supplies; Chrome gets
        // the identical face (a collection member rebuilt from its own tables).
        let (family, file, browser_font, weights): (_, &[u8], Vec<u8>, &[&str]) =
            match kind.as_str() {
                "variable" => (VARIABLE_FAMILY, VARIABLE, VARIABLE.to_vec(), &WEIGHTS),
                "variable-names" => (
                    VARIABLE_FAMILY,
                    VARIABLE_NAMES,
                    VARIABLE_NAMES.to_vec(),
                    &WEIGHTS,
                ),
                "collection" => (
                    COLLECTION_FAMILY,
                    COLLECTION,
                    standalone_face(COLLECTION, COLLECTION_MEMBER).ok_or("invalid member")?,
                    &WEIGHTS[..1],
                ),
                _ => return Err(KINDS.into()),
            };
        let mut cases = Vec::new();
        for weight in weights {
            for (index, text) in TEXTS.iter().enumerate() {
                let mut font = TextMeasurer::new(file.to_vec(), family)?;
                let mut b = B::new();
                b.create_element(2, Tag::Div).insert(1, 2, 0);
                for (p, v) in [
                    (P::Width, "100px"),
                    (P::FontFamily, &format!("\"{family}\"") as &str),
                    (P::FontSize, if index == 0 { "20px" } else { "13px" }),
                    (P::LineHeight, "24px"),
                    (P::FontWeight, weight),
                    (P::Color, "#000"),
                ] {
                    b.set_style(2, p, v);
                }
                b.create_text(3, text).insert(2, 3, 0);
                let mut v = View::default();
                v.commit(&b.finish(), false, &mut Denied)?;
                let mut tree = v.snapshot()["tree"].clone();
                if kind != "collection" {
                    browser_instance(&mut tree);
                }
                let geometry =
                    v.layout_boxes_with_text(VIEWPORT, VIEWPORT, &mut font, BASE_FONT_SIZE)?;
                let side = (VIEWPORT * scale).round() as u32;
                let mut renderer = Renderer::new(Some(font), BASE_FONT_SIZE)?;
                let raster = CoreTextRasterizer::default();
                let statistics = raster.statistics();
                renderer.set_glyph_rasterizer(Some(Box::new(raster)));
                let image = renderer.render(&v, side, side, scale)?;
                let stats = renderer.stats();
                let counts = statistics.counts();
                let name = format!("{kind}-{weight}-{index}");
                if stats.outline_glyphs != 0 || stats.raster_glyphs == 0 {
                    return Err(format!(
                        "{name}: {} outline and {} native glyphs ({counts:?})",
                        stats.outline_glyphs, stats.raster_glyphs
                    )
                    .into());
                }
                let boxes = geometry
                    .iter()
                    .filter(|(id, _)| **id != 1)
                    .map(|(&id, r)| {
                        (
                            id.to_string(),
                            json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();
                let r = geometry[&2];
                cases.push(json!({
                    "name": name,
                    "tree": tree,
                    "boxes": boxes,
                    "hits": [],
                    "samples": [],
                    "raster": image.pixels,
                    "raster_size": [side, side],
                    "roi": {"x": r.x * scale, "y": r.y * scale, "width": r.width * scale, "height": r.height * scale},
                    "raster_stats": {
                        "raster_glyphs": stats.raster_glyphs,
                        "outline_glyphs": stats.outline_glyphs,
                        "glyph_cache_hits": stats.glyph_cache_hits,
                        "collection_members": counts.collection_members,
                        "variation_instances": counts.variation_instances,
                        "declined_color": counts.declined_color,
                        "declined_optical_size": counts.declined_optical_size,
                        "declined_unsupported": counts.declined_unsupported,
                        "declined_variation": counts.declined_variation,
                        "declined_resource": counts.declined_resource,
                        "declined_request": counts.declined_request,
                    },
                }));
            }
        }
        let output = serde_json::to_string(&json!({
            "family": family,
            "font": browser_font,
            "scale": scale,
            "languages": languages,
            "text_rasterizer": "host",
            "cases": cases,
        }))?;
        use std::io::Write;
        std::io::stdout().lock().write_all(output.as_bytes())?;
        Ok(())
    }
}
