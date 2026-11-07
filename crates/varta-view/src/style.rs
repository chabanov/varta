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
    /// Device pixels per CSS pixel: what an `img` `srcset` selects a
    /// candidate for (HTML §4.8.4.3.3). A renderer sets its own scale.
    pub device_pixel_ratio: f64,
    /// The host's `medium` font size and monospace behaviour, the base of
    /// computed font sizes and so of `em` and `lh` (a renderer sets its
    /// text measurer's).
    pub font: varta_style::font::FontBase,
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
            device_pixel_ratio: 1.0,
            font: varta_style::font::FontBase::default(),
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
            font: self.font,
        }
    }
    /// The reference metrics of computed-value lowering.
    pub(crate) fn lowering(&self) -> varta_common::view::css::Context {
        varta_common::view::css::Context {
            root_font_size: self.root_font_size,
            viewport_width: self.width,
            viewport_height: self.height,
            font: None,
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
        P::Cursor | P::PointerEvents => "auto",
        P::Visibility => "visible",
        P::TextTransform => "none",
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
                | P::BorderRadius
                | P::BorderTopLeftRadius
                | P::BorderTopRightRadius
                | P::BorderBottomRightRadius
                | P::BorderBottomLeftRadius => "0",
                P::Border | P::BorderTop | P::BorderRight | P::BorderBottom | P::BorderLeft => {
                    "none"
                }
                P::BorderStyle
                | P::BorderTopStyle
                | P::BorderRightStyle
                | P::BorderBottomStyle
                | P::BorderLeftStyle => "none",
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
    /// The element's font references, on the first font-relative value.
    font: std::cell::OnceCell<Option<varta_common::view::css::FontContext>>,
}
impl<'a> Variables<'a> {
    pub fn new(tree: &'a crate::tree::Tree, id: u32, environment: &'a Environment) -> Self {
        Self {
            tree,
            id,
            environment,
            custom: std::cell::OnceCell::new(),
            font: std::cell::OnceCell::new(),
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
        let mut context = self.environment.lowering();
        if varta_style::font::uses_font_units(&result) {
            context.font = *self.font.get_or_init(|| {
                self.tree
                    .styles
                    .font_context(self.tree, self.id, &self.environment.style())
                    .ok()
            });
        }
        varta_common::view::css::lower(property, &result, &context)
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

/// The `normal` font weight.
#[cfg(test)]
pub(crate) const NORMAL_FONT_WEIGHT: f32 = varta_style::fonts::NORMAL_WEIGHT as f32;
/// Inline-start padding of `ul`/`ol` (`padding-inline-start: 40px`).
pub(crate) const UA_LIST_PADDING_PX: f32 = 40.0;
/// The host's medium font size and whether the generic `monospace` keeps
/// Chrome's fixed-font size where the family switches to or from it
/// ([`varta_style::font::FontSize::switch_family`]). A generic `monospace`
/// that a packaged face names is an ordinary family in the official browser
/// host, so it does not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FontBase {
    /// The host's medium size in CSS pixels.
    pub px: f32,
    pub fixed_monospace: bool,
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
    /// values reset the weight and style to `normal`; the size and
    /// `line-height` are the style engine's rows
    /// ([`varta_style::font::UA_FONTS`]). The family stays inherited, as the
    /// official host makes it.
    pub control_font: bool,
}
impl UaDefaults {
    const fn new(tag: varta_common::view::Tag) -> Self {
        Self {
            tag,
            block: false,
            list_item: false,
            preformatted: false,
            margin_block_em: 0.0,
            nested_list_margin_zero: false,
            padding_inline_start: 0.0,
            list: None,
            text_align: None,
            control_font: false,
        }
    }
    const fn heading(tag: varta_common::view::Tag, margin: f32) -> Self {
        Self {
            block: true,
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
        UaDefaults::heading(Tag::H1, 0.67),
        UaDefaults::heading(Tag::H2, 0.83),
        UaDefaults::heading(Tag::H3, 1.0),
        UaDefaults::list(Tag::Ul, ListKind::Unordered),
        UaDefaults::list(Tag::Ol, ListKind::Ordered),
        UaDefaults {
            block: true,
            list_item: true,
            ..UaDefaults::new(Tag::Li)
        },
        UaDefaults {
            block: true,
            preformatted: true,
            margin_block_em: 1.0,
            ..UaDefaults::new(Tag::Pre)
        },
        UaDefaults::new(Tag::Strong),
        UaDefaults::new(Tag::Em),
        UaDefaults {
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
        P::BorderTopWidth,
        P::BorderRightWidth,
        P::BorderBottomWidth,
        P::BorderLeftWidth,
        P::BorderTopStyle,
        P::BorderRightStyle,
        P::BorderBottomStyle,
        P::BorderLeftStyle,
        P::BorderTopColor,
        P::BorderRightColor,
        P::BorderBottomColor,
        P::BorderLeftColor,
        P::BorderTopLeftRadius,
        P::BorderTopRightRadius,
        P::BorderBottomRightRadius,
        P::BorderBottomLeftRadius,
    ]
};

/// An `overflow` keyword (CSS Overflow 3 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Overflow {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}
impl Overflow {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            "clip" => Self::Clip,
            "scroll" => Self::Scroll,
            "auto" => Self::Auto,
            _ => return None,
        })
    }
    /// Whether the box is a scroll container on this axis.
    pub(crate) fn scrolls(self) -> bool {
        matches!(self, Self::Hidden | Self::Scroll | Self::Auto)
    }
}

/// The used `overflow` of each axis, `[x, y]`, of computed values:
/// `overflow-x`/`overflow-y` (each the component of `overflow` when only
/// the shorthand is declared; the style engine gives a longhand the
/// shorthand's component where the shorthand wins), with the rule of
/// CSS Overflow 3 §3.1: when one axis is a scroll container, `visible` on
/// the other computes to `auto` and `clip` to `hidden`. `None` for a value
/// outside the grammar.
pub(crate) fn overflow(
    values: &BTreeMap<varta_common::view::CssProperty, String>,
) -> Option<[Overflow; 2]> {
    use varta_common::view::CssProperty as P;
    let shorthand = match values.get(&P::Overflow) {
        Some(value) => varta_common::view::mapped::split(value)?,
        None => Vec::new(),
    };
    let axis = |longhand: P, index: usize| -> Option<Overflow> {
        let value = values
            .get(&longhand)
            .cloned()
            .or_else(|| shorthand.get(index).or(shorthand.first()).cloned());
        value.map_or(Some(Overflow::Visible), |value| Overflow::parse(&value))
    };
    let mut axes = [axis(P::OverflowX, 0)?, axis(P::OverflowY, 1)?];
    if axes.iter().any(|axis| axis.scrolls()) {
        for axis in &mut axes {
            *axis = match *axis {
                Overflow::Visible => Overflow::Auto,
                Overflow::Clip => Overflow::Hidden,
                other => other,
            };
        }
    }
    Some(axes)
}

/// The radii of each corner (top-left, top-right, bottom-right,
/// bottom-left) as one `border-radius` value, with `/` and the vertical
/// radii when a corner is elliptical: each corner's longhand, else its
/// `border-radius` component. `None` when no corner is declared or a value
/// is outside the grammar.
pub(crate) fn border_radius(
    values: &BTreeMap<varta_common::view::CssProperty, String>,
) -> Option<String> {
    use varta_common::view::CssProperty as P;
    const CORNERS: [P; 4] = [
        P::BorderTopLeftRadius,
        P::BorderTopRightRadius,
        P::BorderBottomRightRadius,
        P::BorderBottomLeftRadius,
    ];
    let shorthand = values.get(&P::BorderRadius);
    if CORNERS.iter().all(|corner| !values.contains_key(corner)) {
        return shorthand.cloned();
    }
    let mut horizontal = Vec::with_capacity(CORNERS.len());
    let mut vertical = Vec::with_capacity(CORNERS.len());
    for (side, corner) in (0u8..).zip(CORNERS) {
        let value = match (values.get(&corner), shorthand) {
            (Some(value), _) => value.clone(),
            (None, Some(shorthand)) => crate::layout::box_component(shorthand, side)?,
            (None, None) => "0".to_owned(),
        };
        let parts = varta_common::view::mapped::split(&value)?;
        let (h, v) = match parts.as_slice() {
            [h] => (h.clone(), h.clone()),
            [h, v] => (h.clone(), v.clone()),
            _ => return None,
        };
        horizontal.push(h);
        vertical.push(v);
    }
    Some(if horizontal == vertical {
        horizontal.join(" ")
    } else {
        format!("{} / {}", horizontal.join(" "), vertical.join(" "))
    })
}

/// The used value of an inherited keyword property of node `id` (a text
/// node takes its parent's): the nearest declared value up its ancestors,
/// else `initial`.
pub(crate) fn inherited_keyword(
    tree: &crate::tree::Tree,
    env: &Environment,
    id: u32,
    property: varta_common::view::CssProperty,
    initial: &'static str,
) -> String {
    let mut at = Some(id);
    while let Some(node) = at {
        if tree.element(node).is_some()
            && let Some(value) = tree.computed_property(node, env, property)
            && value != INHERIT
        {
            return value;
        }
        at = tree.nodes.get(&node).and_then(|node| node.parent);
    }
    initial.to_owned()
}

/// Whether node `id` is visible (CSS Display 3 §4.1: `visibility: hidden`
/// makes a box invisible, and `collapse` is `hidden` outside tables (CSS 2
/// §11.2); its descendants may be visible again).
pub(crate) fn visible(tree: &crate::tree::Tree, env: &Environment, id: u32) -> bool {
    inherited_keyword(
        tree,
        env,
        id,
        varta_common::view::CssProperty::Visibility,
        "visible",
    ) == "visible"
}

/// Whether node `id` can be the target of pointer events (CSS UI 4 §5.1
/// with the HTML values of SVG 2 §16.6: `none` never is; an invisible box
/// is not either).
pub(crate) fn pointer_target(tree: &crate::tree::Tree, env: &Environment, id: u32) -> bool {
    visible(tree, env, id)
        && inherited_keyword(
            tree,
            env,
            id,
            varta_common::view::CssProperty::PointerEvents,
            "auto",
        ) != "none"
}

/// Whether a control with these cascaded author properties keeps Chrome's
/// native theme: none of [`THEME_BREAKING`] is declared and its
/// `appearance` is not `none` (CSS UI 4 §7.1).
pub(crate) fn themed(properties: &BTreeMap<varta_common::view::CssProperty, String>) -> bool {
    !THEME_BREAKING
        .iter()
        .any(|property| properties.contains_key(property))
        && properties
            .get(&varta_common::view::CssProperty::Appearance)
            .is_none_or(|value| value != APPEARANCE_NONE)
}

/// `appearance: none`.
const APPEARANCE_NONE: &str = "none";
/// `auto` and the `<compat-auto>` values, which behave as `auto` (CSS UI 4
/// §7.1.1).
const APPEARANCE_AUTO: &[&str] = &[
    "auto",
    "button",
    "searchfield",
    "textarea",
    "checkbox",
    "radio",
    "menulist",
    "listbox",
    "meter",
    "progress-bar",
];

/// Whether the renderer draws `element` with this computed `appearance`
/// (CSS UI 4 §7.1): `auto` and the `<compat-auto>` values keep its native
/// appearance; `none` draws a button or text field as a CSS box only, as a
/// declared border or background does (`themed`). A checkbox, radio,
/// select or progress bar without its native appearance and the
/// `<compat-special>` values are not drawn yet.
pub(crate) fn appearance(element: &crate::tree::Element, value: &str) -> Option<()> {
    if value == "inherit" || APPEARANCE_AUTO.contains(&value) {
        return Some(());
    }
    if value != APPEARANCE_NONE {
        return None;
    }
    match ControlKind::of(element) {
        None | Some(ControlKind::Button | ControlKind::TextField | ControlKind::Textarea) => {
            (element.tag != varta_common::view::Tag::Progress).then_some(())
        }
        Some(_) => None,
    }
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
        assert_eq!(rows(|row| row.text_align.is_some()), CONTROL_FONT_TAGS);
        // The font rows are the style engine's.
        let fonts = |keep: fn(&varta_style::font::UaFont) -> bool| {
            varta_style::font::UA_FONTS
                .iter()
                .filter(|row| keep(row))
                .map(|row| row.tag)
                .collect::<Vec<_>>()
        };
        assert_eq!(fonts(|row| row.monospace), UA_MONOSPACE_TAGS);
        assert_eq!(fonts(|row| row.control), CONTROL_FONT_TAGS);
    }
    #[test]
    fn native_ua_table_has_one_row_per_tag_and_browser_values() {
        let mut tags: Vec<_> = NATIVE_UA_DEFAULTS.iter().map(|row| row.tag).collect();
        tags.sort();
        tags.dedup();
        assert_eq!(tags.len(), NATIVE_UA_DEFAULTS.len());
        let h1 = ua_defaults(Tag::H1).unwrap();
        assert_eq!(h1.margin_block_em, 0.67);
        assert_eq!(
            varta_style::font::ua_font(Tag::H1).unwrap().weight,
            varta_style::font::UaWeight::Absolute(700.0)
        );
        let h3 = ua_defaults(Tag::H3).unwrap();
        assert_eq!(h3.margin_block_em, 1.0);
        assert_eq!(
            varta_style::font::ua_font(Tag::H3).unwrap().scale,
            Some(1.17)
        );
        let ul = ua_defaults(Tag::Ul).unwrap();
        assert_eq!(
            (ul.padding_inline_start, ul.list, ul.nested_list_margin_zero),
            (40.0, Some(ListKind::Unordered), true)
        );
        assert!(ua_defaults(Tag::Li).unwrap().list_item);
        let pre = ua_defaults(Tag::Pre).unwrap();
        assert!(pre.preformatted && pre.block);
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
        assert!((varta_style::font::UA_CONTROL_FONT_SIZE_PX - 13.3333).abs() < 1e-3);
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
}
