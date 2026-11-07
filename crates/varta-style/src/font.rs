//! Computed font sizes, weights, styles and line heights (CSS Fonts 4 §2.2,
//! §2.4, §2.5, CSS Inline 3 §4.2), the references of the font-relative
//! units of CSS Values 4 §6.1.1 with the first available font's metrics
//! ([`crate::fonts`]).
//!
//! The user-agent origin's font rows (headings, `strong`, `em`, monospace
//! elements and the form controls' `-webkit-small-control` font) and
//! Blink's rules where the generic family switches to or from `monospace`
//! and for the minimum logical font size are those the native renderer
//! matched against Chrome; they live here so every host derives the units
//! from the same fonts.
use crate::fonts::{FaceStyle, NORMAL_WEIGHT, SizedMetrics};
use crate::prelude::*;
use crate::{Computed, Document, Environment, Styles};
use varta_common::view::css::FontContext;
use varta_common::view::css::math::FontMetrics;
use varta_common::view::{CssProperty, Tag};

/// Font size of the form controls' `-webkit-small-control` font: 10pt at 96
/// CSS pixels per inch, independent of the host's medium size.
pub const UA_CONTROL_FONT_SIZE_PX: f64 = 40.0 / 3.0;
/// Chrome's default fixed (monospace) font size over its default
/// proportional size (13px / 16px).
pub const UA_MONOSPACE_RATIO: f64 = 13.0 / 16.0;
/// The generic family name the user-agent rows give `pre` and `code`.
pub const UA_MONOSPACE_FAMILY: &str = "monospace";
/// Chrome's minimum logical font size in CSS pixels (headless Chrome with a
/// fresh profile, Blink's `MinimumLogicalFontSize`).
pub const MINIMUM_LOGICAL_FONT_SIZE: f64 = 9.0;
/// Admitted computed font sizes in CSS pixels.
pub const MIN_FONT_SIZE: f64 = 0.01;
pub const MAX_FONT_SIZE: f64 = 4096.0;
/// The denominator of a CSS percentage.
const PERCENT: f64 = 100.0;
/// The `bold` font weight (CSS Fonts 4 §2.2).
pub const BOLD_WEIGHT: f64 = 700.0;
/// The admitted numeric font weights (CSS Fonts 4 §2.2).
const MIN_FONT_WEIGHT: f64 = 1.0;
const MAX_FONT_WEIGHT: f64 = 1000.0;

/// The `bolder` weight of an inherited weight (CSS Fonts 4 §2.2.1 table).
#[must_use]
pub fn bolder(inherited: f64) -> f64 {
    match inherited {
        weight if weight < 350.0 => NORMAL_WEIGHT,
        weight if weight < 550.0 => BOLD_WEIGHT,
        weight if weight < 900.0 => 900.0,
        weight => weight,
    }
}

/// The host's font inputs: its `medium` size and whether the generic
/// `monospace` keeps Chrome's fixed-font size (it does not once a packaged
/// face names it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontBase {
    pub medium: f64,
    pub fixed_monospace: bool,
}
impl Default for FontBase {
    fn default() -> Self {
        Self {
            medium: varta_common::view::stylesheet::REM_PIXELS,
            fixed_monospace: true,
        }
    }
}

/// How a user-agent row sets the font weight where the author sets none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UaWeight {
    Inherit,
    /// `bolder` than the inherited weight (`strong`).
    Bolder,
    /// An absolute weight (`bold` for headings).
    Absolute(f64),
}

/// One tag's font row of the user-agent origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UaFont {
    pub tag: Tag,
    /// `font-size` as a multiple of the inherited size (`2em` for `h1`).
    pub scale: Option<f64>,
    /// `font-family: monospace`.
    pub monospace: bool,
    /// `font: -webkit-small-control`: an absent author size is
    /// [`UA_CONTROL_FONT_SIZE_PX`], an absent `line-height` is `normal` and
    /// absent weight and style are `normal`.
    pub control: bool,
    pub weight: UaWeight,
    /// `font-style: italic`.
    pub italic: bool,
}
impl UaFont {
    const fn new(tag: Tag) -> Self {
        Self {
            tag,
            scale: None,
            monospace: false,
            control: false,
            weight: UaWeight::Inherit,
            italic: false,
        }
    }
    const fn heading(tag: Tag, scale: f64) -> Self {
        Self {
            scale: Some(scale),
            weight: UaWeight::Absolute(BOLD_WEIGHT),
            ..Self::new(tag)
        }
    }
    const fn monospace(tag: Tag) -> Self {
        Self {
            monospace: true,
            ..Self::new(tag)
        }
    }
    const fn control(tag: Tag) -> Self {
        Self {
            control: true,
            ..Self::new(tag)
        }
    }
}

/// The user-agent font rows (Chrome's `h1 { font-size: 2em; font-weight:
/// bold }`, `strong { font-weight: bolder }`, `em { font-style: italic }`,
/// `pre, code { font-family: monospace }`, `button, input, select,
/// textarea { font: -webkit-small-control }`).
pub const UA_FONTS: &[UaFont] = &[
    UaFont::heading(Tag::H1, 2.0),
    UaFont::heading(Tag::H2, 1.5),
    UaFont::heading(Tag::H3, 1.17),
    UaFont {
        weight: UaWeight::Bolder,
        ..UaFont::new(Tag::Strong)
    },
    UaFont {
        italic: true,
        ..UaFont::new(Tag::Em)
    },
    UaFont::monospace(Tag::Pre),
    UaFont::monospace(Tag::Code),
    UaFont::control(Tag::Button),
    UaFont::control(Tag::Input),
    UaFont::control(Tag::Textarea),
    UaFont::control(Tag::Select),
];

/// The user-agent font row of `tag`, if any.
#[must_use]
pub fn ua_font(tag: Tag) -> Option<&'static UaFont> {
    UA_FONTS.iter().find(|row| row.tag == tag)
}

/// Whether a `font-family` value is exactly the generic `monospace` family
/// (Blink's `IsMonospace`): a list such as `monospace, monospace` is not.
#[must_use]
pub fn sole_monospace(family: &str) -> bool {
    family.trim().eq_ignore_ascii_case(UA_MONOSPACE_FAMILY)
}

/// Blink's computed font-size state along an element path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSize {
    /// The specified size in CSS pixels: the basis of `em` and percentages.
    pub px: f64,
    /// Still the initial `medium` keyword: no font-size applies on the path.
    keyword: bool,
    /// Fixed by an absolute length; em-like scales and percentages keep the
    /// parent's flag.
    absolute: bool,
}
impl FontSize {
    /// The initial `medium` size of the host's proportional default.
    #[must_use]
    pub fn medium(base: f64) -> Self {
        Self {
            px: base,
            keyword: true,
            absolute: false,
        }
    }
    /// A multiple of the inherited size (an em scale or a percentage).
    pub fn scale(&mut self, factor: f64) {
        self.px *= factor;
        self.keyword = false;
    }
    /// An absolute length in CSS pixels.
    pub fn absolute(&mut self, px: f64) {
        self.px = px;
        self.keyword = false;
        self.absolute = true;
    }
    /// Whether an absolute length fixes the size on the path.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.absolute
    }
    /// Blink's `GetComputedSizeFromSpecifiedSize` at page zoom 1: a size
    /// derived without an absolute length computes to at least
    /// [`MINIMUM_LOGICAL_FONT_SIZE`]; em lengths keep the specified size.
    #[must_use]
    pub fn computed(&self) -> f64 {
        if self.absolute {
            self.px
        } else {
            self.px.max(MINIMUM_LOGICAL_FONT_SIZE)
        }
    }
    /// Blink's `CheckForGenericFamilyChange`: where the family switches
    /// between the sole generic `monospace` and anything else, a size that
    /// is not absolute is re-derived by [`UA_MONOSPACE_RATIO`].
    pub fn switch_family(&mut self, base: f64, parent_monospace: bool, monospace: bool) {
        if self.absolute || parent_monospace == monospace {
            return;
        }
        self.px = match (self.keyword, monospace) {
            (true, true) => base * UA_MONOSPACE_RATIO,
            (true, false) => base,
            (false, true) => self.px * UA_MONOSPACE_RATIO,
            (false, false) => self.px / UA_MONOSPACE_RATIO,
        };
    }
}

/// A computed `line-height` (CSS Inline 3 §4.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    Normal,
    /// A `<number>`: inherited as the number, a multiple of each element's
    /// computed font size.
    Number(f64),
    /// A length or a resolved percentage, in CSS pixels.
    Absolute(f64),
}
impl LineHeight {
    /// The used length for `font_size`, or `None` for `normal`, which needs
    /// the metrics of the first available font.
    #[must_use]
    pub fn length(self, font_size: f64) -> Option<f64> {
        match self {
            Self::Normal => None,
            Self::Number(number) => Some(number * font_size),
            Self::Absolute(px) => Some(px),
        }
    }
}

/// The font of one element: its size state, whether its family is the sole
/// generic `monospace`, its nearest family declaration, weight, style and
/// line height.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    pub size: FontSize,
    pub monospace: bool,
    /// The nearest declared (or user-agent) family list and its element.
    pub family: Option<(u32, String)>,
    /// The computed `font-weight` (CSS Fonts 4 §2.2).
    pub weight: f64,
    /// The computed `font-style` is `italic` (CSS Fonts 4 §2.4).
    pub italic: bool,
    pub line_height: LineHeight,
}
impl Font {
    /// The initial font of the host.
    fn initial(medium: f64) -> Self {
        Self {
            size: FontSize::medium(medium),
            monospace: false,
            family: None,
            weight: NORMAL_WEIGHT,
            italic: false,
            line_height: LineHeight::Normal,
        }
    }
    /// The face this font asks for.
    #[must_use]
    pub fn style(&self) -> FaceStyle {
        FaceStyle {
            weight: self.weight,
            italic: self.italic,
        }
    }
}

/// Why a font cannot be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontError {
    /// A value on the path the engine cannot use, at element `0`.
    Unsupported(u32),
}

/// A computed value of a font property, as the host grammar states it, or
/// `None` where the element declares none (an inherited value or `inherit`).
fn declared(computed: Option<Computed>, initial: &str) -> Option<String> {
    match computed? {
        Computed::Value(value) => Some(value),
        Computed::Initial => Some(initial.to_owned()),
        Computed::Inherit => None,
    }
}

impl Styles {
    /// The font of `id`, along the path from its top-level ancestor:
    /// author values over the user-agent rows at each element.
    ///
    /// # Errors
    /// When a value on the path is outside what the engine computes.
    pub fn font(
        &self,
        document: &impl Document,
        id: u32,
        environment: &Environment,
    ) -> Result<Font, FontError> {
        let mut path = vec![id];
        while let Some(parent) = document.parent(*path.last().unwrap_or(&id)) {
            path.push(parent);
        }
        let mut font = Font::initial(environment.font.medium);
        for ancestor in path.into_iter().rev() {
            font = self.font_step(document, ancestor, environment, font)?;
        }
        Ok(font)
    }

    /// The font of `id` from its parent's.
    fn font_step(
        &self,
        document: &impl Document,
        id: u32,
        environment: &Environment,
        parent: Font,
    ) -> Result<Font, FontError> {
        let base = environment.font;
        let element = document.element(id);
        let ua = element.and_then(|element| ua_font(element.tag));
        let wanted = [
            CssProperty::FontSize,
            CssProperty::FontFamily,
            CssProperty::FontWeight,
            CssProperty::FontStyle,
        ];
        let root = || self.root_metrics(environment);
        let parent_metrics = || self.metrics(&parent);
        // Values 4 §6.1.1: in `font-size`, the font-relative units refer to
        // the parent's font (`lh` to its line height).
        let parent_context = || {
            let parent_metrics = parent_metrics();
            let parent_lh = line_height_length(&parent, parent_metrics);
            let root = root();
            Some(FontContext {
                em: parent.size.px,
                parent_em: parent.size.px,
                lh: parent_lh,
                parent_lh,
                rlh: root.map(|root| root.line_spacing),
                metrics: units(&parent, parent_metrics),
                parent_metrics: units(&parent, parent_metrics),
                root_metrics: root_units(environment, root),
            })
        };
        let mut values = self.computed_only(document, id, environment, &wanted, &parent_context);
        let present = |property: CssProperty| values.contains_key(&property);
        let (size_present, family_present, weight_present, style_present) = (
            present(CssProperty::FontSize),
            present(CssProperty::FontFamily),
            present(CssProperty::FontWeight),
            present(CssProperty::FontStyle),
        );
        let size_value = declared(values.remove(&CssProperty::FontSize), "medium");
        let family_value = declared(values.remove(&CssProperty::FontFamily), "");
        let weight_value = declared(values.remove(&CssProperty::FontWeight), "normal");
        let style_value = declared(values.remove(&CssProperty::FontStyle), "normal");
        let mut font = parent.clone();
        let parent_monospace = parent.monospace;
        if let Some(row) = ua {
            if row.control && !size_present {
                font.size.absolute(UA_CONTROL_FONT_SIZE_PX);
            }
            if row.monospace && !family_present {
                font.family = Some((id, UA_MONOSPACE_FAMILY.to_owned()));
            }
            if let Some(scale) = row.scale
                && !size_present
            {
                font.size.scale(scale);
            }
            if row.control {
                if !weight_present {
                    font.weight = NORMAL_WEIGHT;
                }
                if !style_present {
                    font.italic = false;
                }
            }
            if !weight_present {
                font.weight = match row.weight {
                    UaWeight::Inherit => font.weight,
                    UaWeight::Bolder => bolder(font.weight),
                    UaWeight::Absolute(weight) => weight,
                };
            }
            if row.italic && !style_present {
                font.italic = true;
            }
        }
        if let Some(family) = family_value.filter(|family| !family.is_empty()) {
            font.family = Some((id, family));
        }
        if let Some(value) = weight_value {
            font.weight = match value.as_str() {
                "normal" => NORMAL_WEIGHT,
                "bold" => BOLD_WEIGHT,
                number => number
                    .parse::<f64>()
                    .ok()
                    .filter(|weight| (MIN_FONT_WEIGHT..=MAX_FONT_WEIGHT).contains(weight))
                    .ok_or(FontError::Unsupported(id))?,
            };
        }
        if let Some(value) = style_value {
            font.italic = match value.as_str() {
                "normal" => false,
                "italic" => true,
                _ => return Err(FontError::Unsupported(id)),
            };
        }
        let monospace = base.fixed_monospace
            && font
                .family
                .as_ref()
                .is_some_and(|(_, family)| sole_monospace(family));
        if let Some(value) = size_value {
            if value == "medium" {
                // Fonts 4 §2.5: the initial `medium`, of the generic family
                // in effect.
                font.size = FontSize::medium(if monospace && parent_monospace {
                    base.medium * UA_MONOSPACE_RATIO
                } else {
                    base.medium
                });
            } else if let Some(number) = value.strip_suffix('%') {
                let number: f64 = number.parse().map_err(|_| FontError::Unsupported(id))?;
                font.size.scale(number / PERCENT);
            } else {
                font.size.absolute(
                    pixels(&value, environment.context.root_font_size)
                        .ok_or(FontError::Unsupported(id))?,
                );
            }
        }
        // Chrome re-derives the size where the generic family switches to
        // or from monospace, before a percentage line height resolves.
        font.size
            .switch_family(base.medium, parent_monospace, monospace);
        font.monospace = monospace;
        if font.size.px != parent.size.px
            && (!font.size.px.is_finite()
                || !(MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&font.size.px))
        {
            return Err(FontError::Unsupported(id));
        }
        // `line-height` resolves `em`, `ex`, `cap`, `ch` and `ic` against
        // the element's own font and `lh` against the parent's line height
        // (Values 4 §6.1.1).
        let own_context = || {
            let own = self.metrics(&font);
            let parent_metrics = parent_metrics();
            let parent_lh = line_height_length(&parent, parent_metrics);
            let root = root();
            Some(FontContext {
                em: font.size.px,
                parent_em: parent.size.px,
                lh: parent_lh,
                parent_lh,
                rlh: root.map(|root| root.line_spacing),
                metrics: units(&font, own),
                parent_metrics: units(&parent, parent_metrics),
                root_metrics: root_units(environment, root),
            })
        };
        let mut height = self.computed_only(
            document,
            id,
            environment,
            &[CssProperty::LineHeight],
            &own_context,
        );
        let height_present = height.contains_key(&CssProperty::LineHeight);
        if ua.is_some_and(|row| row.control) && !height_present {
            font.line_height = LineHeight::Normal;
        }
        if let Some(value) = declared(height.remove(&CssProperty::LineHeight), "normal") {
            font.line_height =
                line_height(&value, &font.size, environment).ok_or(FontError::Unsupported(id))?;
        }
        Ok(font)
    }

    /// The metrics of `font`'s first available font at its computed size.
    #[must_use]
    pub fn metrics(&self, font: &Font) -> Option<SizedMetrics> {
        self.fonts().metrics(
            font.family.as_ref().map(|(_, family)| family.as_str()),
            font.style(),
            font.size.computed(),
        )
    }

    /// The metrics of the root element's font: the initial family, weight
    /// and style at the root font size (the root element is outside the
    /// view).
    fn root_metrics(&self, environment: &Environment) -> Option<SizedMetrics> {
        self.fonts()
            .metrics(None, FaceStyle::NORMAL, environment.context.root_font_size)
    }

    /// The font-relative references of `id` (CSS Values 4 §6.1.1).
    ///
    /// # Errors
    /// When the element's or its parent's font cannot be computed.
    pub fn font_context(
        &self,
        document: &impl Document,
        id: u32,
        environment: &Environment,
    ) -> Result<FontContext, FontError> {
        let font = self.font(document, id, environment)?;
        let parent = match document.parent(id) {
            Some(parent) => self.font(document, parent, environment)?,
            None => Font::initial(environment.font.medium),
        };
        let own = self.metrics(&font);
        let parent_metrics = self.metrics(&parent);
        let root = self.root_metrics(environment);
        Ok(FontContext {
            em: font.size.px,
            parent_em: parent.size.px,
            lh: line_height_length(&font, own),
            parent_lh: line_height_length(&parent, parent_metrics),
            // The root element is outside the view: its line height is the
            // initial `normal` of the root font.
            rlh: root.map(|root| root.line_spacing),
            metrics: units(&font, own),
            parent_metrics: units(&parent, parent_metrics),
            root_metrics: root_units(environment, root),
        })
    }
}

/// The used line height of `font`: `normal` is its first available font's
/// line spacing (CSS Inline 3 §4.2), unresolved without metrics.
fn line_height_length(font: &Font, metrics: Option<SizedMetrics>) -> Option<f64> {
    font.line_height
        .length(font.size.computed())
        .or_else(|| metrics.map(|metrics| metrics.line_spacing))
}

/// The unit references of `font`: its first available font's, or the
/// Values 4 fallbacks when no font has metrics.
fn units(font: &Font, metrics: Option<SizedMetrics>) -> FontMetrics {
    metrics.map_or_else(
        || FontMetrics::fallback(font.size.computed()),
        |metrics| metrics.units,
    )
}

/// The root element's unit references.
fn root_units(environment: &Environment, metrics: Option<SizedMetrics>) -> FontMetrics {
    metrics.map_or_else(
        || FontMetrics::fallback(environment.context.root_font_size),
        |metrics| metrics.units,
    )
}

/// A `px` or `rem` length in CSS pixels.
fn pixels(value: &str, root_font_size: f64) -> Option<f64> {
    let px = if let Some(number) = value.strip_suffix("rem") {
        number.parse::<f64>().ok()? * root_font_size
    } else if value == "0" {
        0.0
    } else {
        value.strip_suffix("px")?.parse::<f64>().ok()?
    };
    (px.is_finite() && px >= 0.0).then_some(px)
}

/// A computed `line-height` from its lowered value: a percentage resolves
/// against the computed font size, a number stays a number.
fn line_height(value: &str, size: &FontSize, environment: &Environment) -> Option<LineHeight> {
    if value == "normal" {
        return Some(LineHeight::Normal);
    }
    if let Some(number) = value.strip_suffix('%') {
        let px = size.computed() * number.parse::<f64>().ok()? / PERCENT;
        return px.is_finite().then_some(LineHeight::Absolute(px));
    }
    if value.ends_with("px") || value.ends_with("rem") || value == "0" {
        return pixels(value, environment.context.root_font_size).map(LineHeight::Absolute);
    }
    let number: f64 = value.parse().ok()?;
    (number.is_finite() && number >= 0.0).then_some(LineHeight::Number(number))
}

/// The two-letter endings of the font-relative units the lowering resolves
/// from the element's font (`em`, `ex`, `cap`, `ch`, `ic`, `lh` and their
/// root forms; `rem` matches too, harmlessly).
const FONT_UNIT_ENDINGS: [&[u8; 2]; 6] = [b"em", b"ex", b"ap", b"ch", b"ic", b"lh"];

/// Whether a value may name a font-relative unit the lowering resolves from
/// the element's font. A false positive only computes the font.
#[must_use]
pub fn uses_font_units(value: &str) -> bool {
    value.as_bytes().windows(2).any(|pair| {
        FONT_UNIT_ENDINGS
            .iter()
            .any(|ending| pair.eq_ignore_ascii_case(*ending))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chrome 156 computed sizes with a 16px proportional and 13px fixed default.
    #[test]
    fn monospace_switch_follows_blink_keyword_and_absolute_state() {
        const BASE: f64 = 16.0;
        let close = |size: FontSize, expected: f64| (size.px - expected).abs() < 1e-4;
        // body > pre: the medium keyword takes the fixed default.
        let mut pre = FontSize::medium(BASE);
        pre.switch_family(BASE, false, true);
        assert!(close(pre, 13.0));
        // pre > span { font-family: serif }: back to the proportional keyword.
        let mut span = pre;
        span.switch_family(BASE, true, false);
        assert!(close(span, 16.0));
        // h1 > code: the 2em heading keeps the non-absolute state.
        let mut code = FontSize::medium(BASE);
        code.scale(2.0);
        code.switch_family(BASE, false, true);
        assert!(close(code, 26.0));
        // h3 > pre: 18.72px becomes 15.21px.
        let mut pre = FontSize::medium(BASE);
        pre.scale(1.17);
        pre.switch_family(BASE, false, true);
        assert!(close(pre, 15.21));
        // pre > span { font-size: 200% } stays monospace.
        let mut span = FontSize::medium(BASE);
        span.switch_family(BASE, false, true);
        span.scale(2.0);
        span.switch_family(BASE, true, true);
        assert!(close(span, 26.0));
        // h1 > pre (26px) > span { font-family: serif } returns to 32px.
        let mut span = code;
        span.switch_family(BASE, true, false);
        assert!(close(span, 32.0));
        // Absolute lengths (px, rem) are never re-derived, and percentages of
        // them keep the absolute state; em-like scales of medium never set it.
        assert!(!code.is_absolute() && !span.is_absolute());
        let mut fixed = FontSize::medium(BASE);
        fixed.absolute(16.0);
        fixed.switch_family(BASE, false, true);
        assert!(close(fixed, 16.0));
        fixed.scale(1.5);
        fixed.switch_family(BASE, true, false);
        assert!(close(fixed, 24.0) && fixed.is_absolute());
        assert!(sole_monospace(" Monospace ") && !sole_monospace("monospace, monospace"));
        assert!(!sole_monospace("\"monospace\""));
    }
}
