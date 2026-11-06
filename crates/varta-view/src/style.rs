//! Stylesheets of draft v0: the checked stylesheet grammar and the cascade
//! subset (sheets by ascending id, then specificity, then source order;
//! inline `set_style` wins; custom properties inherit and `var()` is
//! substituted). Only custom properties inherit in the headless output; a
//! property's computed value is its cascaded value.
use std::collections::BTreeMap;

/// Host media conditions and native reference metrics, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Viewport width in CSS pixels.
    pub width: f64,
    /// Viewport height in CSS pixels: the `height` and `orientation` media
    /// features and the viewport-percentage units (`vh`, `dvh`, …).
    pub height: f64,
    pub dark: bool,
    /// Recorded user-preference and interaction media inputs
    /// (`prefers-reduced-motion`, `prefers-contrast`, `forced-colors`,
    /// `hover`, `pointer` and their `any-` forms).
    pub preferences: varta_common::view::stylesheet::media::Preferences,
    /// Reference document-root font size for `rem`, independent of the view
    /// root's own font size and the shaper's default text size.
    pub root_font_size: f64,
    /// The platform's scrollbar style, which a native host reads from its
    /// system setting.
    pub scrollbars: Scrollbars,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            width: DEFAULT_VIEWPORT_WIDTH,
            height: f64::from(crate::geometry::DEFAULT_VIEWPORT_HEIGHT),
            dark: false,
            preferences: varta_common::view::stylesheet::media::Preferences::default(),
            root_font_size: varta_common::view::stylesheet::REM_PIXELS,
            scrollbars: Scrollbars::default(),
        }
    }
}

/// How scroll containers show their scrollbars, as Chrome on macOS follows
/// the system's "Show scroll bars" setting (`NSScroller.preferredScrollerStyle`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scrollbars {
    /// Overlay scrollbars: no layout space, and nothing painted while the
    /// container is not scrolled (the headless default).
    #[default]
    Overlay,
    /// Always-shown (legacy) scrollbars: a scroll container whose content
    /// overflows on an axis reserves a gutter for that axis's bar, and the
    /// track and thumb are painted in it.
    Classic,
}
impl Environment {
    /// The media query environment (Media Queries 4/5) of these inputs.
    #[must_use]
    pub fn media(&self) -> varta_common::view::stylesheet::Media {
        varta_common::view::stylesheet::Media {
            width: self.width,
            height: self.height,
            dark: self.dark,
            preferences: self.preferences,
        }
    }
    /// The shared style engine's inputs.
    pub(crate) fn style(&self) -> varta_style::Environment {
        varta_style::Environment {
            media: self.media(),
            context: self.lowering(),
        }
    }
    /// The reference metrics of computed-value lowering.
    pub(crate) fn lowering(&self) -> varta_common::view::css::Context {
        varta_common::view::css::Context {
            root_font_size: self.root_font_size,
            viewport_width: self.width,
            viewport_height: self.height,
        }
    }
    /// Set the host reference font. Zero is valid for geometry; text shaping
    /// still applies its independent minimum font size. No OS lookup occurs.
    pub fn with_root_font_size(mut self, size: f64) -> Result<Self, crate::LayoutError> {
        self.root_font_size = size;
        crate::units::Units::new(&self).ok_or(crate::LayoutError::RootFont)?;
        Ok(self)
    }
}
/// Default headless viewport width in CSS pixels.
pub const DEFAULT_VIEWPORT_WIDTH: f64 = 1024.0;
pub(crate) use varta_common::view::stylesheet::Name;

/// The computed-value marker of `inherit` on an inherited property: its
/// consumers take the parent's value and skip the element's own
/// user-agent row (Cascade 5 §7.3.2).
pub(crate) const INHERIT: &str = "inherit";

/// The initial value (each property definition's "Initial" line) that a
/// computed `initial` must state explicitly natively: for an inherited
/// property, to stop inheritance; for another, where the native user-agent
/// origin gives this element a value. Elsewhere no value is the initial
/// value. The texts are in the grammar the consumers read; `font-size`
/// takes the keyword `medium`, `color` the light scheme's `CanvasText`.
pub(crate) fn initial_override(
    tree: &crate::tree::Tree,
    id: u32,
    property: varta_common::view::CssProperty,
) -> Option<String> {
    use varta_common::view::CssProperty as P;
    let text = match property {
        P::Color => "#000000",
        P::FontSize => "medium",
        P::FontWeight | P::FontStyle | P::LineHeight | P::WhiteSpace | P::OverflowWrap => "normal",
        P::TextAlign => "start",
        P::Cursor => "auto",
        _ => {
            let element = tree.element(id)?;
            let ua = ua_defaults(element.tag).is_some() || ControlKind::of(element).is_some();
            if !ua {
                return None;
            }
            match property {
                P::Display => "inline",
                P::Width | P::Height => "auto",
                P::Margin
                | P::MarginTop
                | P::MarginRight
                | P::MarginBottom
                | P::MarginLeft
                | P::Padding
                | P::PaddingTop
                | P::PaddingRight
                | P::PaddingBottom
                | P::PaddingLeft
                | P::BorderRadius => "0",
                P::Border | P::BorderTop | P::BorderRight | P::BorderBottom | P::BorderLeft => {
                    "none"
                }
                P::BorderStyle => "none",
                P::BackgroundColor => "transparent",
                _ => return None,
            }
        }
    };
    Some(text.into())
}

/// The native user-agent origin's value of a non-inherited `property` for
/// element `id`, where `inherit` reads it from a parent without an author
/// value: the outer display type of the user-agent table.
pub(crate) fn ua_value(
    tree: &crate::tree::Tree,
    id: u32,
    property: varta_common::view::CssProperty,
) -> Option<String> {
    let element = tree.element(id)?;
    match property {
        varta_common::view::CssProperty::Display => {
            let row = ua_defaults(element.tag)?;
            row.block.then(|| "block".into())
        }
        _ => None,
    }
}

/// One read's variable map is materialized only if a value actually uses var().
pub(crate) struct Variables<'a> {
    tree: &'a crate::tree::Tree,
    id: u32,
    environment: &'a Environment,
    custom: std::cell::OnceCell<varta_style::Scope<'a>>,
}
impl<'a> Variables<'a> {
    pub fn new(tree: &'a crate::tree::Tree, id: u32, environment: &'a Environment) -> Self {
        Self {
            tree,
            id,
            environment,
            custom: std::cell::OnceCell::new(),
        }
    }
    pub fn property(
        &self,
        property: varta_common::view::CssProperty,
        value: &str,
    ) -> Option<String> {
        let result = self.substitute(value)?;
        // Cascade 5 §7.3: a CSS-wide keyword resolves against the native
        // user-agent origin; `revert` drops this declaration.
        let keyword = result.trim();
        let computed = if keyword.eq_ignore_ascii_case("initial") {
            Some(varta_style::Computed::Initial)
        } else if keyword.eq_ignore_ascii_case("inherit") {
            Some(varta_style::Computed::Inherit)
        } else if keyword.eq_ignore_ascii_case("unset") {
            Some(if varta_style::inherited(property) {
                varta_style::Computed::Inherit
            } else {
                varta_style::Computed::Initial
            })
        } else if keyword.eq_ignore_ascii_case("revert")
            || keyword.eq_ignore_ascii_case("revert-layer")
        {
            return None;
        } else {
            None
        };
        if let Some(computed) = computed {
            return self
                .tree
                .keyword(self.id, self.environment, property, computed);
        }
        // CSS Variables 1 §3.1: an invalid substituted value is invalid at
        // computed-value time. A valid one is lowered to the host grammar the
        // renderer's consumers read; one the renderer cannot render is
        // treated the same way.
        if !self
            .tree
            .render_reads
            .validate(self.environment, property, &result)
        {
            return None;
        }
        varta_common::view::css::lower(property, &result, &self.environment.lowering())
    }
    pub fn substitute(&self, value: &str) -> Option<String> {
        if !self
            .tree
            .render_reads
            .contains_variable(self.environment, value)
        {
            return Some(value.to_owned());
        }
        self.custom
            .get_or_init(|| self.tree.custom_properties(self.id, self.environment))
            .substitute(value)
    }
}

/// Version of the native user-agent default table, [`NATIVE_UA_DEFAULTS`].
/// Any change to a row's value or to the set of rows is a new version, so a
/// recorded native observation names the defaults it was taken with.
pub const NATIVE_UA_VERSION: u32 = 2;

/// Font weight of `h1`–`h3` in the browser user-agent stylesheet (`bold`).
pub(crate) const UA_BOLD_WEIGHT: f32 = 700.0;
/// The `normal` font weight, which the control font resets to.
pub(crate) const NORMAL_FONT_WEIGHT: f32 = 400.0;
/// Inline-start padding of `ul`/`ol` (`padding-inline-start: 40px`).
pub(crate) const UA_LIST_PADDING_PX: f32 = 40.0;
/// Chrome's default fixed (monospace) font size over its default proportional
/// size (13px / 16px). See [`FontSize::switch_family`].
pub(crate) const UA_MONOSPACE_RATIO: f32 = 13.0 / 16.0;
/// The generic family name the user-agent table gives `pre` and `code`.
pub(crate) const UA_MONOSPACE_FAMILY: &str = "monospace";

/// Whether a `font-family` value is exactly the generic `monospace` family
/// (Blink's `IsMonospace`): a list such as `monospace, monospace` is not.
pub(crate) fn sole_monospace(family: &str) -> bool {
    family.trim().eq_ignore_ascii_case(UA_MONOSPACE_FAMILY)
}

/// The host's medium font size and whether the generic `monospace` keeps
/// Chrome's fixed-font size where the family switches to or from it
/// ([`FontSize::switch_family`]). A generic `monospace` that a packaged face
/// names is an ordinary family in the official browser host, so it does not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FontBase {
    /// The host's medium size in CSS pixels.
    pub px: f32,
    pub fixed_monospace: bool,
}

/// Blink's computed font-size state along an element path, which decides how
/// the size changes where the generic family switches to or from monospace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FontSize {
    /// Computed size in CSS pixels.
    pub px: f32,
    /// Still the initial `medium` keyword: no font-size applies on the path.
    keyword: bool,
    /// Fixed by an absolute length (`px`, `rem`); em-like user-agent scales
    /// and percentages inherit the parent's flag.
    absolute: bool,
}
impl FontSize {
    /// The initial `medium` size of the host's proportional default.
    pub(crate) fn medium(base: f32) -> Self {
        Self {
            px: base,
            keyword: true,
            absolute: false,
        }
    }
    /// `font-size` as a multiple of the inherited size (em scale or percentage).
    pub(crate) fn scale(&mut self, factor: f32) {
        self.px *= factor;
        self.keyword = false;
    }
    /// `font-size` as an absolute length in CSS pixels.
    pub(crate) fn absolute(&mut self, px: f32) {
        self.px = px;
        self.keyword = false;
        self.absolute = true;
    }
    /// Whether an absolute length fixes the size on the path: then neither a
    /// family switch nor the minimum logical font size changes it.
    pub(crate) fn is_absolute(&self) -> bool {
        self.absolute
    }
    /// Blink's `CheckForGenericFamilyChange`: where the family switches between
    /// the sole generic `monospace` and anything else, a non-absolute size is
    /// re-derived. The `medium` keyword takes the host medium size, scaled by
    /// [`UA_MONOSPACE_RATIO`] for monospace; any other size is multiplied
    /// (into monospace) or divided (out of it) by that ratio.
    pub(crate) fn switch_family(&mut self, base: f32, parent_monospace: bool, monospace: bool) {
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

/// How a row changes the inherited font weight when the author sets none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum UaWeight {
    Inherit,
    /// CSS `bolder` relative to the inherited weight (`strong`).
    Bolder,
    /// An absolute weight (`bold` for headings).
    Absolute(f32),
}

/// The list container kind of `ul`/`ol`; it selects the item marker style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListKind {
    Unordered,
    Ordered,
}

/// One tag's defaults from the browser user-agent stylesheet. Author
/// declarations (inline or stylesheet) always win over these values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UaDefaults {
    pub tag: varta_common::view::Tag,
    /// Block-level by default (`display: block` or, for `li`, `list-item`).
    pub block: bool,
    /// `display: list-item`: the box generates an outside list marker.
    pub list_item: bool,
    /// `font-size` as a multiple of the inherited size (`2em` for `h1`).
    pub font_scale: Option<f32>,
    pub weight: UaWeight,
    /// `font-style: italic`.
    pub italic: bool,
    /// `font-family: monospace`.
    pub monospace: bool,
    /// `white-space: pre`.
    pub preformatted: bool,
    /// Top and bottom margins in em of the element's own font size.
    pub margin_block_em: f32,
    /// Margins become zero for a list nested in another list (`ul ul`, `ol ul`, …).
    pub nested_list_margin_zero: bool,
    /// `padding-inline-start` in CSS pixels.
    pub padding_inline_start: f32,
    pub list: Option<ListKind>,
    /// `text-align` of form controls: `center` for `button`, `start` otherwise.
    /// These stop an ancestor's inherited alignment at the control.
    pub text_align: Option<crate::text::TextAlign>,
    /// Chrome's `font: -webkit-small-control` on form controls: absent author
    /// values reset the size to [`UA_CONTROL_FONT_SIZE_PX`] (an absolute
    /// size), the weight and style to `normal` and `line-height` to `normal`.
    /// The family stays inherited, as the official host makes it.
    pub control_font: bool,
}
impl UaDefaults {
    const fn new(tag: varta_common::view::Tag) -> Self {
        Self {
            tag,
            block: false,
            list_item: false,
            font_scale: None,
            weight: UaWeight::Inherit,
            italic: false,
            monospace: false,
            preformatted: false,
            margin_block_em: 0.0,
            nested_list_margin_zero: false,
            padding_inline_start: 0.0,
            list: None,
            text_align: None,
            control_font: false,
        }
    }
    const fn heading(tag: varta_common::view::Tag, scale: f32, margin: f32) -> Self {
        Self {
            block: true,
            font_scale: Some(scale),
            weight: UaWeight::Absolute(UA_BOLD_WEIGHT),
            margin_block_em: margin,
            ..Self::new(tag)
        }
    }
    const fn list(tag: varta_common::view::Tag, kind: ListKind) -> Self {
        Self {
            block: true,
            margin_block_em: 1.0,
            nested_list_margin_zero: true,
            padding_inline_start: UA_LIST_PADDING_PX,
            list: Some(kind),
            ..Self::new(tag)
        }
    }
    const fn control(tag: varta_common::view::Tag, align: crate::text::TextAlign) -> Self {
        Self {
            text_align: Some(align),
            control_font: true,
            ..Self::new(tag)
        }
    }
}

/// Native user-agent defaults, version [`NATIVE_UA_VERSION`]. The official
/// browser host adds no normalization for these tags, so each row mirrors the
/// Chrome user-agent stylesheet (`h1 { font-size: 2em; margin-block: 0.67em;
/// font-weight: bold }`, `ul { margin-block: 1em; padding-inline-start: 40px }`,
/// `pre { font-family: monospace; white-space: pre; margin: 1em 0 }`, …).
/// Control rows carry `text-align` and the control font; their box defaults
/// are the [`NATIVE_CONTROL_DEFAULTS`] rows of the same version.
pub(crate) const NATIVE_UA_DEFAULTS: &[UaDefaults] = {
    use crate::text::TextAlign;
    use varta_common::view::Tag;
    &[
        UaDefaults {
            block: true,
            margin_block_em: 1.0,
            ..UaDefaults::new(Tag::P)
        },
        UaDefaults::heading(Tag::H1, 2.0, 0.67),
        UaDefaults::heading(Tag::H2, 1.5, 0.83),
        UaDefaults::heading(Tag::H3, 1.17, 1.0),
        UaDefaults::list(Tag::Ul, ListKind::Unordered),
        UaDefaults::list(Tag::Ol, ListKind::Ordered),
        UaDefaults {
            block: true,
            list_item: true,
            ..UaDefaults::new(Tag::Li)
        },
        UaDefaults {
            block: true,
            monospace: true,
            preformatted: true,
            margin_block_em: 1.0,
            ..UaDefaults::new(Tag::Pre)
        },
        UaDefaults {
            weight: UaWeight::Bolder,
            ..UaDefaults::new(Tag::Strong)
        },
        UaDefaults {
            italic: true,
            ..UaDefaults::new(Tag::Em)
        },
        UaDefaults {
            monospace: true,
            ..UaDefaults::new(Tag::Code)
        },
        UaDefaults::new(Tag::Label),
        UaDefaults::control(Tag::Button, TextAlign::Center),
        UaDefaults::control(Tag::Input, TextAlign::Start),
        UaDefaults::control(Tag::Textarea, TextAlign::Start),
        UaDefaults::control(Tag::Select, TextAlign::Start),
    ]
};

/// The user-agent row of `tag`, if the native table has one.
pub(crate) fn ua_defaults(tag: varta_common::view::Tag) -> Option<&'static UaDefaults> {
    NATIVE_UA_DEFAULTS.iter().find(|row| row.tag == tag)
}

/// Chrome's `-webkit-small-control` font size: 10pt at 96 CSS pixels per
/// inch (13.333px), independent of the host's medium size.
pub(crate) const UA_CONTROL_FONT_SIZE_PX: f32 = 40.0 / 3.0;
/// The HTML default `size` of a text field, in average characters. Varta
/// has no `size` attribute, and `number` fields ignore it in Chrome.
pub(crate) const UA_FIELD_COLUMNS: u32 = 20;
/// The HTML default `cols` of a `textarea` (Varta has no `cols` attribute).
pub(crate) const UA_TEXTAREA_COLUMNS: u32 = 20;
/// The HTML default `rows` of a `textarea`; the `rows` attribute overrides it.
pub(crate) const UA_TEXTAREA_ROWS: u32 = 2;
/// The scrollbar gutter Chrome adds to an unsized themed `textarea`'s
/// intrinsic width (observed in headless Chrome 156 on macOS for every font
/// and size probed; it is not an overlay scrollbar's painted width).
pub(crate) const UA_TEXTAREA_SCROLLBAR_GUTTER_PX: f32 = 16.0;
/// The border box side of Chrome's checkbox and radio (13px at zoom 1).
pub(crate) const UA_CHECKABLE_SIDE_PX: f32 = 13.0;

/// Form controls whose user-agent box defaults differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlKind {
    Button,
    /// `input` of type text, search, password or number.
    TextField,
    Textarea,
    Checkbox,
    Radio,
    Select,
}
impl ControlKind {
    /// The kind of a form control element, if it is one.
    pub(crate) fn of(element: &crate::tree::Element) -> Option<Self> {
        use varta_common::view::Tag;
        Some(match element.tag {
            Tag::Button => Self::Button,
            Tag::Textarea => Self::Textarea,
            Tag::Select => Self::Select,
            Tag::Input => match element.input_type() {
                "checkbox" => Self::Checkbox,
                "radio" => Self::Radio,
                _ => Self::TextField,
            },
            _ => return None,
        })
    }
}

/// How an unsized control computes its intrinsic content size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Intrinsic {
    /// The size of its children (a button's label).
    Content,
    /// `columns` average characters wide and one line high (Chrome on macOS:
    /// `⌈(columns − 1)·round(avg) + max(avg, ascent)⌉`, where `avg` is the
    /// primary font's `x` advance).
    Characters { columns: u32 },
    /// `columns` rounded average characters plus
    /// [`UA_TEXTAREA_SCROLLBAR_GUTTER_PX`] wide and `rows` lines high.
    Lines { columns: u32, rows: u32 },
    /// A square intrinsic content size (the theme square of a checkable),
    /// which stretch alignment may grow; padding and borders are zero.
    Square(f32),
    /// Not modelled natively: an unsized control is refused.
    Refused,
}

/// One control kind's user-agent box defaults, part of [`NATIVE_UA_VERSION`].
/// They mirror Chrome's user-agent stylesheet under the official browser
/// host's normalization (`box-sizing: border-box; margin: 0`). An author
/// declaration of a property replaces its value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ControlDefaults {
    pub kind: ControlKind,
    /// Padding in CSS pixels, top/right/bottom/left.
    pub padding: [f32; 4],
    /// The computed border width on every side. Chrome's native theme draws
    /// a one-pixel frame inside it; the rest is the control's fill.
    pub border: f32,
    pub intrinsic: Intrinsic,
}

/// Native control box defaults, version [`NATIVE_UA_VERSION`]: Chrome's
/// `button { padding: 1px 6px; border: 2px outset }`, `input { padding: 1px
/// 2px; border: 2px inset }`, `textarea { padding: 2px; border: 1px solid }`,
/// `select { border: 1px solid }` and 13px checkable squares.
pub(crate) const NATIVE_CONTROL_DEFAULTS: &[ControlDefaults] = &[
    ControlDefaults {
        kind: ControlKind::Button,
        padding: [1.0, 6.0, 1.0, 6.0],
        border: 2.0,
        intrinsic: Intrinsic::Content,
    },
    ControlDefaults {
        kind: ControlKind::TextField,
        padding: [1.0, 2.0, 1.0, 2.0],
        border: 2.0,
        intrinsic: Intrinsic::Characters {
            columns: UA_FIELD_COLUMNS,
        },
    },
    ControlDefaults {
        kind: ControlKind::Textarea,
        padding: [2.0; 4],
        border: 1.0,
        intrinsic: Intrinsic::Lines {
            columns: UA_TEXTAREA_COLUMNS,
            rows: UA_TEXTAREA_ROWS,
        },
    },
    ControlDefaults {
        kind: ControlKind::Checkbox,
        padding: [0.0; 4],
        border: 0.0,
        intrinsic: Intrinsic::Square(UA_CHECKABLE_SIDE_PX),
    },
    ControlDefaults {
        kind: ControlKind::Radio,
        padding: [0.0; 4],
        border: 0.0,
        intrinsic: Intrinsic::Square(UA_CHECKABLE_SIDE_PX),
    },
    // The select's popup and arrow belong to its own painter; an unsized or
    // unthemed select keeps its explicit-style requirement.
    ControlDefaults {
        kind: ControlKind::Select,
        padding: [0.0; 4],
        border: 1.0,
        intrinsic: Intrinsic::Refused,
    },
];

/// The box defaults of `kind`.
pub(crate) fn control_defaults(kind: ControlKind) -> &'static ControlDefaults {
    NATIVE_CONTROL_DEFAULTS
        .iter()
        .find(|row| row.kind == kind)
        // One row per kind is asserted by `control_rows_cover_every_kind_once`.
        .unwrap_or(&NATIVE_CONTROL_DEFAULTS[0])
}

/// Author properties that make Chrome drop a control's native theme
/// (`LayoutTheme::IsControlStyled`: an author background, border or radius).
const THEME_BREAKING: &[varta_common::view::CssProperty] = {
    use varta_common::view::CssProperty as P;
    &[
        P::BackgroundColor,
        P::Border,
        P::BorderTop,
        P::BorderRight,
        P::BorderBottom,
        P::BorderLeft,
        P::BorderWidth,
        P::BorderStyle,
        P::BorderColor,
        P::BorderRadius,
    ]
};

/// Whether a control with these cascaded author properties keeps Chrome's
/// native theme (none of [`THEME_BREAKING`] is declared).
pub(crate) fn themed(properties: &BTreeMap<varta_common::view::CssProperty, String>) -> bool {
    !THEME_BREAKING
        .iter()
        .any(|property| properties.contains_key(property))
}

#[cfg(test)]
mod ua_tests {
    use super::*;
    use varta_common::view::Tag;
    /// The browser host maps exactly the native monospace rows to the
    /// packaged monospace family and makes exactly the native control rows
    /// inherit their family, from these shared lists.
    #[test]
    fn monospace_and_control_rows_are_the_shared_browser_lists() {
        use varta_common::view::fonts::{CONTROL_FONT_TAGS, UA_MONOSPACE_TAGS};
        let rows = |keep: fn(&UaDefaults) -> bool| {
            NATIVE_UA_DEFAULTS
                .iter()
                .filter(|row| keep(row))
                .map(|row| row.tag)
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(|row| row.monospace), UA_MONOSPACE_TAGS);
        assert_eq!(rows(|row| row.text_align.is_some()), CONTROL_FONT_TAGS);
    }
    #[test]
    fn native_ua_table_has_one_row_per_tag_and_browser_values() {
        let mut tags: Vec<_> = NATIVE_UA_DEFAULTS.iter().map(|row| row.tag).collect();
        tags.sort();
        tags.dedup();
        assert_eq!(tags.len(), NATIVE_UA_DEFAULTS.len());
        let h1 = ua_defaults(Tag::H1).unwrap();
        assert_eq!(
            (h1.font_scale, h1.weight, h1.margin_block_em),
            (Some(2.0), UaWeight::Absolute(700.0), 0.67)
        );
        let h3 = ua_defaults(Tag::H3).unwrap();
        assert_eq!((h3.font_scale, h3.margin_block_em), (Some(1.17), 1.0));
        let ul = ua_defaults(Tag::Ul).unwrap();
        assert_eq!(
            (ul.padding_inline_start, ul.list, ul.nested_list_margin_zero),
            (40.0, Some(ListKind::Unordered), true)
        );
        assert!(ua_defaults(Tag::Li).unwrap().list_item);
        let pre = ua_defaults(Tag::Pre).unwrap();
        assert!(pre.monospace && pre.preformatted && pre.block);
        assert!(!ua_defaults(Tag::Label).unwrap().block);
        assert_eq!(
            ua_defaults(Tag::Button).unwrap().text_align,
            Some(crate::text::TextAlign::Center)
        );
        assert!(ua_defaults(Tag::Div).is_none());
        assert_eq!(NATIVE_UA_VERSION, 2);
    }

    /// Chrome 156 computed control defaults under the official host's
    /// `box-sizing: border-box; margin: 0` normalization.
    #[test]
    fn control_rows_cover_every_kind_once_with_browser_values() {
        let kinds = [
            ControlKind::Button,
            ControlKind::TextField,
            ControlKind::Textarea,
            ControlKind::Checkbox,
            ControlKind::Radio,
            ControlKind::Select,
        ];
        assert_eq!(NATIVE_CONTROL_DEFAULTS.len(), kinds.len());
        for kind in kinds {
            assert_eq!(
                NATIVE_CONTROL_DEFAULTS
                    .iter()
                    .filter(|row| row.kind == kind)
                    .count(),
                1
            );
            assert_eq!(control_defaults(kind).kind, kind);
        }
        let button = control_defaults(ControlKind::Button);
        assert_eq!((button.padding, button.border), ([1.0, 6.0, 1.0, 6.0], 2.0));
        let field = control_defaults(ControlKind::TextField);
        assert_eq!((field.padding, field.border), ([1.0, 2.0, 1.0, 2.0], 2.0));
        assert_eq!(field.intrinsic, Intrinsic::Characters { columns: 20 });
        let area = control_defaults(ControlKind::Textarea);
        assert_eq!((area.padding, area.border), ([2.0; 4], 1.0));
        assert_eq!(
            control_defaults(ControlKind::Radio).intrinsic,
            Intrinsic::Square(13.0)
        );
        assert!((UA_CONTROL_FONT_SIZE_PX - 13.3333).abs() < 1e-3);
        // Exactly the control rows carry the control font.
        for row in NATIVE_UA_DEFAULTS {
            assert_eq!(row.control_font, row.text_align.is_some());
        }
    }

    #[test]
    fn author_background_border_or_radius_drops_the_theme() {
        use varta_common::view::CssProperty as P;
        let mut properties = BTreeMap::new();
        properties.insert(P::Color, "#f00".to_owned());
        properties.insert(P::Padding, "4px".to_owned());
        assert!(themed(&properties));
        for property in [P::BackgroundColor, P::BorderLeft, P::BorderRadius] {
            let mut styled = properties.clone();
            styled.insert(property, "0".to_owned());
            assert!(!themed(&styled), "{property:?}");
        }
    }

    /// Chrome 156 computed sizes with a 16px proportional and 13px fixed default.
    #[test]
    fn monospace_switch_follows_blink_keyword_and_absolute_state() {
        const BASE: f32 = 16.0;
        let close = |size: FontSize, expected: f32| (size.px - expected).abs() < 1e-4;
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
