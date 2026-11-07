//! The platform's measurements of the fonts it renders text with itself,
//! for the style engine's font-relative units (CSS Values 4 §6.1.1): the
//! families Chrome takes on this platform for the CSS generic families and
//! its default font, and a system-font set's platform families
//! (`varta_style::fonts::HostFonts`).
//!
//! On macOS they are read as Chrome reads them (Blink's `SimpleFontData`
//! over Skia's CoreText backend): CoreText's ascent, descent and leading at
//! the size, each rounded, for `line-height: normal`; CoreText's cap-height;
//! the top of the `x` glyph's CoreText bounds for the x-height; and the `0`
//! and `水` advances from the face's own tables at its variation location
//! for the size and weight (the engine's packaged-face reader over the
//! platform face's tables). Other platforms supply none yet, so their
//! generic families take the Values 4 fallbacks.
use std::sync::Arc;
use varta_style::fonts::HostFonts;

/// The platform's font measurements, if this platform has them. `ui_families`
/// are the registered families of a system-font set that stand for the
/// platform's UI font, which is not reachable by name.
#[must_use]
pub fn host_fonts(ui_families: Vec<String>) -> Option<Arc<dyn HostFonts>> {
    #[cfg(target_os = "macos")]
    {
        Some(Arc::new(platform::CoreTextFonts::new(ui_families)))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ui_families;
        None
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use core_text::font::{self, CTFont, kCTFontSystemFontType};
    use core_text::font_descriptor::{
        kCTFontBoldTrait, kCTFontItalicTrait, kCTFontOrientationHorizontal,
    };
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use varta_style::fonts::{
        FaceStyle, Generic, HostFamily, HostFonts, SizedMetrics, face_metrics, glyph_id,
        host_metrics,
    };

    /// Chrome's family for each generic family on macOS (probed with Chrome
    /// 156 on macOS 27; the system-font set maps the same,
    /// `system_fonts::platform::GENERICS`); `None` is the UI font. The
    /// standard font, the family of text without `font-family`, is `serif`'s.
    fn generic_family(generic: Generic) -> Option<&'static str> {
        match generic {
            Generic::Serif => Some(STANDARD_FAMILY),
            Generic::SansSerif => Some("Helvetica"),
            Generic::Monospace => Some("Menlo"),
            Generic::SystemUi => None,
        }
    }
    const STANDARD_FAMILY: &str = "Times";
    /// The lowest CSS weight Chrome draws with a family's bold face.
    const BOLD_WEIGHT: f64 = 600.0;
    /// Families whose ascent Chrome on macOS enlarges by this fraction of
    /// their rounded height, rounded (`FontMetrics::AscentDescentWithHacks`;
    /// the native shaper's `ENLARGED_ASCENT_FRACTION`).
    const ENLARGED_ASCENT_FAMILIES: [&str; 3] = ["Times", "Helvetica", "Courier"];
    /// The tables of a platform face the advances are read from.
    const TABLES: [[u8; 4]; 10] = [
        *b"head", *b"hhea", *b"hmtx", *b"maxp", *b"cmap", *b"OS/2", *b"fvar", *b"avar", *b"HVAR",
        *b"MVAR",
    ];
    /// Answers kept per provider; past it, CoreText is asked again.
    const MAX_CACHED: usize = 1024;

    type Key = (String, u64, bool, u64);

    #[derive(Debug)]
    pub(super) struct CoreTextFonts {
        ui_families: Vec<String>,
        cache: Mutex<BTreeMap<Key, Option<SizedMetrics>>>,
    }

    impl CoreTextFonts {
        pub(super) fn new(ui_families: Vec<String>) -> Self {
            Self {
                ui_families,
                cache: Mutex::default(),
            }
        }

        /// The CoreText font Chrome draws `family` with for `style` at
        /// `size`, and its family name; `None` for the UI font's name.
        fn font(&self, family: HostFamily<'_>, style: FaceStyle, size: f64) -> Option<CTFont> {
            let name = match family {
                HostFamily::Named(name) if self.ui_families.iter().any(|ui| ui == name) => None,
                HostFamily::Named(name) => Some(name),
                HostFamily::Generic(generic) => generic_family(generic),
                HostFamily::Default => Some(STANDARD_FAMILY),
            };
            let font = match name {
                None => font::new_ui_font_for_language(kCTFontSystemFontType, size, None),
                Some(name) => {
                    let font = font::new_from_name(name, size).ok()?;
                    // CoreText substitutes a missing family; Chrome would not
                    // draw with it.
                    if !font.family_name().eq_ignore_ascii_case(name) {
                        return None;
                    }
                    font
                }
            };
            let bold = style.weight >= BOLD_WEIGHT;
            let mask = kCTFontBoldTrait | kCTFontItalicTrait;
            let wanted = if bold { kCTFontBoldTrait } else { 0 }
                | if style.italic { kCTFontItalicTrait } else { 0 };
            if font.symbolic_traits() & mask == wanted {
                return Some(font);
            }
            Some(
                font.clone_with_symbolic_traits(wanted, mask)
                    .unwrap_or(font),
            )
        }

        fn measure(
            &self,
            family: HostFamily<'_>,
            style: FaceStyle,
            size: f64,
        ) -> Option<SizedMetrics> {
            let font = self.font(family, style, size)?;
            let tables: Vec<([u8; 4], Vec<u8>)> = TABLES
                .iter()
                .filter_map(|tag| {
                    let table = font.get_font_table(u32::from_be_bytes(*tag))?;
                    Some((*tag, table.bytes().to_vec()))
                })
                .collect();
            let face = crate::sfnt::from_tables(tables)?;
            // The `0` and `水` advances at the variation location for the
            // size and weight, with the Values 4 fallbacks.
            let own = face_metrics(&face, 0, style, size)?;
            // Blink rounds the ascent and descent, enlarges the ascent of
            // three classic families, and rounds the line gap.
            let mut ascent = font.ascent().round();
            let descent = font.descent().round();
            if ENLARGED_ASCENT_FAMILIES
                .iter()
                .any(|family| font.family_name() == *family)
            {
                ascent += ((ascent + descent) * varta_view::ENLARGED_ASCENT_FRACTION as f64 + 0.5)
                    .floor();
            }
            let line_spacing = ascent + descent + font.leading().round();
            // Blink measures the x-height as the top of the `x` glyph.
            let x_height = glyph_id(&face, 0, 'x')
                .and_then(|glyph| u16::try_from(glyph).ok())
                .map(|glyph| {
                    let rect =
                        font.get_bounding_rects_for_glyphs(kCTFontOrientationHorizontal, &[glyph]);
                    rect.origin.y + rect.size.height
                })
                .unwrap_or_else(|| font.x_height());
            host_metrics(
                x_height,
                font.cap_height(),
                own.units.ch,
                own.units.ic,
                line_spacing,
            )
        }
    }

    impl HostFonts for CoreTextFonts {
        fn metrics(
            &self,
            family: HostFamily<'_>,
            style: FaceStyle,
            size: f64,
        ) -> Option<SizedMetrics> {
            let name = match family {
                HostFamily::Named(name) => format!("n{name}"),
                HostFamily::Generic(generic) => format!("g{}", generic.name()),
                HostFamily::Default => "d".into(),
            };
            let key = (name, style.weight.to_bits(), style.italic, size.to_bits());
            if let Ok(cache) = self.cache.lock()
                && let Some(found) = cache.get(&key)
            {
                return *found;
            }
            let found = self.measure(family, style, size);
            if let Ok(mut cache) = self.cache.lock()
                && cache.len() < MAX_CACHED
            {
                cache.insert(key, found);
            }
            found
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Chrome 156 on macOS 27 (`word-spacing: 1ex` and so on, read with
        /// `computedStyleMap`): `[ex, cap, ch, ic, lh]`.
        #[test]
        fn generic_families_measure_as_chrome_does() {
            let fonts = CoreTextFonts::new(Vec::new());
            let normal = FaceStyle::NORMAL;
            let bold = FaceStyle {
                weight: 700.0,
                italic: false,
            };
            let rows: [(HostFamily<'_>, FaceStyle, f64, [f64; 5]); 11] = [
                (
                    HostFamily::Default,
                    normal,
                    16.0,
                    [7.1796875, 10.5859375, 8.0, 16.0, 18.0],
                ),
                (
                    HostFamily::Generic(Generic::Serif),
                    normal,
                    13.0,
                    [5.8335, 8.60107, 6.5, 13.0, 15.0],
                ),
                (
                    HostFamily::Generic(Generic::Serif),
                    normal,
                    100.0,
                    [44.873, 66.1621, 50.0, 100.0, 115.0],
                ),
                (
                    HostFamily::Generic(Generic::Serif),
                    bold,
                    16.0,
                    [7.3671875, 10.8125, 8.0, 16.0, 18.0],
                ),
                (
                    HostFamily::Generic(Generic::SansSerif),
                    normal,
                    16.0,
                    [8.3671875, 11.4765625, 8.8984375, 16.0, 18.0],
                ),
                (
                    HostFamily::Generic(Generic::SansSerif),
                    normal,
                    100.0,
                    [52.2949, 71.7285, 55.6152, 100.0, 115.0],
                ),
                (
                    HostFamily::Generic(Generic::Monospace),
                    normal,
                    13.0,
                    [7.109375, 9.47705078125, 7.82666015625, 13.0, 15.0],
                ),
                (
                    HostFamily::Generic(Generic::Monospace),
                    normal,
                    100.0,
                    [54.6875, 72.9004, 60.2051, 100.0, 117.0],
                ),
                (
                    HostFamily::Generic(Generic::SystemUi),
                    normal,
                    13.0,
                    [6.8427734375, 9.15966796875, 8.1884765625, 13.0, 16.0],
                ),
                (
                    HostFamily::Generic(Generic::SystemUi),
                    bold,
                    13.0,
                    [6.98876953125, 9.15966796875, 8.8994140625, 13.0, 16.0],
                ),
                (
                    HostFamily::Generic(Generic::SystemUi),
                    normal,
                    100.0,
                    [50.7812, 70.459, 60.6445, 100.0, 118.0],
                ),
            ];
            let mut failures = Vec::new();
            for (family, style, size, chrome) in rows {
                let measured = fonts.metrics(family, style, size).unwrap();
                let ours = [
                    measured.units.ex,
                    measured.units.cap.unwrap(),
                    measured.units.ch,
                    measured.units.ic,
                    measured.line_spacing,
                ];
                // Chrome's values are binary32 (the 100px rows printed with
                // six significant digits).
                if ours
                    .iter()
                    .zip(chrome)
                    .any(|(ours, chrome)| (ours - chrome).abs() > 5e-6 * chrome.max(1.0))
                {
                    failures.push(format!(
                        "{family:?} {style:?} {size}: ours {ours:?}, chrome {chrome:?}"
                    ));
                }
            }
            assert!(failures.is_empty(), "{failures:#?}");
            assert_eq!(
                fonts.metrics(HostFamily::Named("No Such Family"), normal, 16.0),
                None
            );
        }
    }
}
