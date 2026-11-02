//! Deterministic text shaping using only explicitly supplied fonts.
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    FontContext, FontFamilyName, FontStyle, FontWeight, Layout, LayoutContext, LineHeight,
    OverflowWrap, StyleProperty, TextWrapMode,
};
use std::{borrow::Cow, sync::Arc};

/// A font and explicit CSS aliases supplied by the embedding host.
pub struct TextFont {
    pub data: Vec<u8>,
    pub family: String,
    pub aliases: Vec<String>,
}

struct FontEntry {
    family: String,
    aliases: Vec<FontFamilyName<'static>>,
}

/// A bounded explicit-font text shaper. Reuse it to retain shaping caches.
pub struct TextMeasurer {
    fonts: FontContext,
    layout: LayoutContext<()>,
    entries: Vec<FontEntry>,
}

/// The measured advance and height, in CSS pixels, and number of lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    /// Width when every available soft line break is taken.
    pub min_width: f32,
    pub height: f32,
    pub lines: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct TextOptions {
    pub size: f32,
    pub line_height: Option<LineHeight>,
    pub weight: f32,
    pub italic: bool,
    pub white_space: WhiteSpaceMode,
    pub break_anywhere: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WhiteSpaceMode {
    #[default]
    Normal,
    NoWrap,
    PreWrap,
}

#[derive(Debug, thiserror::Error)]
pub enum TextMeasureError {
    #[error("invalid, oversized, ambiguous, or mismatched font")]
    Font,
    #[error("invalid text size or layout limit")]
    Limit,
}

fn same_family(a: &FontFamilyName<'_>, b: &FontFamilyName<'_>) -> bool {
    match (a, b) {
        (FontFamilyName::Named(a), FontFamilyName::Named(b)) => a.eq_ignore_ascii_case(b),
        _ => a == b,
    }
}

impl TextMeasurer {
    /// Resolve only registered names and explicit aliases. A generic family
    /// preceding any match is rejected unless the host explicitly mapped it.
    pub(crate) fn resolve_families(&self, css_family: Option<&str>) -> Option<Vec<String>> {
        let Some(css_family) = css_family else {
            return Some(vec![self.entries.first()?.family.clone()]);
        };
        let mut resolved = Vec::new();
        for name in FontFamilyName::parse_css_list(css_family) {
            let name = name.ok()?;
            let found = self.entries.iter().find(|entry| {
                same_family(&FontFamilyName::named(&entry.family), &name)
                    || entry.aliases.iter().any(|alias| same_family(alias, &name))
            });
            if let Some(entry) = found {
                if !resolved.contains(&entry.family) {
                    resolved.push(entry.family.clone());
                }
            } else if matches!(name, FontFamilyName::Generic(_)) && resolved.is_empty() {
                return None;
            }
        }
        (!resolved.is_empty()).then_some(resolved)
    }

    /// Load one font family from bytes. No OS font lookup is enabled.
    pub fn new(font: Vec<u8>, family: &str) -> Result<Self, TextMeasureError> {
        Self::new_with_aliases(font, family, &[])
    }

    /// Map named or generic CSS families to this explicit font.
    pub fn new_with_aliases(
        font: Vec<u8>,
        family: &str,
        aliases: &[&str],
    ) -> Result<Self, TextMeasureError> {
        Self::new_with_fonts(vec![TextFont {
            data: font,
            family: family.into(),
            aliases: aliases.iter().map(|value| (*value).into()).collect(),
        }])
    }

    /// Load a bounded collection of explicit fonts. Alias collisions are rejected.
    /// The first family is the default when CSS does not specify a family list.
    pub fn new_with_fonts(fonts: Vec<TextFont>) -> Result<Self, TextMeasureError> {
        if fonts.is_empty() || fonts.len() > 16 {
            return Err(TextMeasureError::Font);
        }
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let mut entries: Vec<FontEntry> = Vec::new();
        let mut bytes = 0usize;
        for font in fonts {
            bytes = bytes
                .checked_add(font.data.len())
                .ok_or(TextMeasureError::Font)?;
            if font.data.is_empty()
                || font.data.len() > 4 * 1024 * 1024
                || bytes > 16 * 1024 * 1024
                || font.family.is_empty()
                || font.family.len() > 256
                || font.aliases.len() > 16
            {
                return Err(TextMeasureError::Font);
            }
            let aliases = font
                .aliases
                .iter()
                .map(|alias| {
                    if alias.is_empty() || alias.len() > 256 || alias.trim_end().ends_with(',') {
                        return Err(TextMeasureError::Font);
                    }
                    let mut parsed = FontFamilyName::parse_css_list(alias);
                    let name = parsed
                        .next()
                        .ok_or(TextMeasureError::Font)?
                        .map_err(|_| TextMeasureError::Font)?;
                    if parsed.next().is_some() {
                        return Err(TextMeasureError::Font);
                    }
                    Ok(name.into_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let name = FontFamilyName::named(&font.family);
            let same = entries
                .iter()
                .position(|existing| same_family(&name, &FontFamilyName::named(&existing.family)));
            for (index, existing) in entries.iter().enumerate() {
                if Some(index) == same {
                    continue;
                }
                let existing_name = FontFamilyName::named(&existing.family);
                if existing
                    .aliases
                    .iter()
                    .any(|alias| same_family(alias, &name))
                    || aliases.iter().any(|alias| {
                        same_family(alias, &existing_name)
                            || existing
                                .aliases
                                .iter()
                                .any(|other| same_family(alias, other))
                    })
                {
                    return Err(TextMeasureError::Font);
                }
            }
            let registered = collection.register_fonts(Blob::new(Arc::new(font.data)), None);
            let Some(family_id) = collection.family_id(&font.family) else {
                return Err(TextMeasureError::Font);
            };
            if !registered.iter().any(|(id, _)| *id == family_id) {
                return Err(TextMeasureError::Font);
            }
            if let Some(index) = same {
                entries[index].aliases.extend(aliases);
            } else {
                entries.push(FontEntry {
                    family: font.family,
                    aliases,
                });
            }
        }
        Ok(Self {
            fonts: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            layout: LayoutContext::new(),
            entries,
        })
    }

    /// Shape a bounded UTF-8 string with the first supplied font family.
    pub fn measure(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
    ) -> Result<TextMetrics, TextMeasureError> {
        let families = self.resolve_families(None).ok_or(TextMeasureError::Font)?;
        self.measure_styled(
            text,
            max_width,
            &families,
            TextOptions {
                size,
                line_height: None,
                weight: 400.0,
                italic: false,
                white_space: WhiteSpaceMode::Normal,
                break_anywhere: false,
            },
        )
    }

    /// Shape using a CSS family list resolved against the supplied font collection.
    pub fn measure_with_family_list(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        css_family: &str,
    ) -> Result<TextMetrics, TextMeasureError> {
        let families = self
            .resolve_families(Some(css_family))
            .ok_or(TextMeasureError::Font)?;
        self.measure_styled(
            text,
            max_width,
            &families,
            TextOptions {
                size,
                line_height: None,
                weight: 400.0,
                italic: false,
                white_space: WhiteSpaceMode::Normal,
                break_anywhere: false,
            },
        )
    }

    pub(crate) fn measure_styled(
        &mut self,
        text: &str,
        max_width: Option<f32>,
        families: &[String],
        options: TextOptions,
    ) -> Result<TextMetrics, TextMeasureError> {
        self.shape_styled(text, max_width, families, options)
            .map(|(metrics, _)| metrics)
    }

    pub(crate) fn shape_styled(
        &mut self,
        text: &str,
        max_width: Option<f32>,
        families: &[String],
        options: TextOptions,
    ) -> Result<(TextMetrics, Layout<()>), TextMeasureError> {
        if text.len() > 64 * 1024
            || !options.size.is_finite()
            || !(0.01..=4096.0).contains(&options.size)
            || max_width.is_some_and(|v| !v.is_finite() || !(0.01..=1_000_000.0).contains(&v))
            || families.is_empty()
        {
            return Err(TextMeasureError::Limit);
        }
        let family_names: Vec<_> = families
            .iter()
            .map(|name| FontFamilyName::Named(Cow::Borrowed(name.as_str())))
            .collect();
        let mut builder = self
            .layout
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(family_names.as_slice().into()));
        builder.push_default(StyleProperty::FontSize(options.size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(options.weight)));
        builder.push_default(StyleProperty::FontStyle(if options.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }));
        if options.white_space == WhiteSpaceMode::NoWrap {
            builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        }
        if options.break_anywhere {
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        }
        if let Some(line_height) = options.line_height {
            builder.push_default(StyleProperty::LineHeight(line_height));
        }
        let mut layout: Layout<()> = builder.build(text);
        let widths = layout.calculate_content_widths();
        let min_width = if options.white_space == WhiteSpaceMode::NoWrap {
            widths.max
        } else {
            widths.min
        };
        layout.break_all_lines(max_width);
        Ok((
            TextMetrics {
                width: layout.width(),
                min_width,
                height: layout.height(),
                lines: layout.len(),
            },
            layout,
        ))
    }
}
