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
    pub dark: bool,
    /// Reference document-root font size for `rem`, independent of the view
    /// root's own font size and the shaper's default text size.
    pub root_font_size: f64,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            width: DEFAULT_VIEWPORT_WIDTH,
            dark: false,
            root_font_size: varta_common::view::stylesheet::REM_PIXELS,
        }
    }
}
impl Environment {
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
pub(crate) use varta_common::view::stylesheet::{Name, Rule, Subject};

/// Whether `rule`'s media conditions hold in `environment`.
pub(crate) fn applies(rule: &Rule, environment: &Environment) -> bool {
    rule.applies(environment.width, environment.dark)
}

/// Parse a whole stylesheet with the shared grammar
/// (`varta_common::view::stylesheet`), retaining its typed work-limit error.
pub(crate) fn parse(
    text: &str,
    allowance: usize,
) -> Result<
    varta_common::view::stylesheet::ParsedStylesheet,
    varta_common::view::stylesheet::StylesheetError,
> {
    varta_common::view::stylesheet::parse_with_work_limit(text, allowance)
}

/// Rule anchors preserve the shared matcher's semantics, including comma
/// specificity. Each rule is considered once even with duplicate class names.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RuleIndex {
    roots: Vec<usize>,
    classes: BTreeMap<String, Vec<usize>>,
    pub has_custom: bool,
    pub costs: Vec<RuleCost>,
    pub expansion: Vec<Vec<(Name, varta_common::view::expansion::Value)>>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RuleCost {
    pub work: usize,
    pub variables: bool,
}
impl RuleIndex {
    pub fn new(rules: &[Rule]) -> Self {
        let mut out = Self::default();
        for (index, rule) in rules.iter().enumerate() {
            out.expansion.push(
                rule.declarations
                    .iter()
                    .map(|(name, value)| {
                        (
                            name.clone(),
                            varta_common::view::expansion::Value::new(value),
                        )
                    })
                    .collect(),
            );
            out.costs.push(RuleCost {
                work: rule.cascade_work(),
                variables: rule
                    .declarations
                    .iter()
                    .any(|(_, value)| varta_common::view::css::contains_variable(value)),
            });
            out.has_custom |= rule
                .declarations
                .iter()
                .any(|(name, _)| matches!(name, Name::Custom(_)));
            for selector in &rule.selectors {
                let list = if selector.root {
                    &mut out.roots
                } else {
                    let Some(class) = selector.classes.first() else {
                        continue;
                    };
                    out.classes.entry(class.clone()).or_default()
                };
                if list.last() != Some(&index) {
                    list.push(index);
                }
            }
        }
        out
    }
}

/// One read's variable map is materialized only if a value actually uses var().
pub(crate) struct Variables<'a> {
    tree: &'a crate::tree::Tree,
    id: u32,
    environment: &'a Environment,
    custom: std::cell::OnceCell<crate::variables::Scope<'a>>,
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
        self.tree
            .render_reads
            .validate(self.environment, property, &result)
            .then_some(result)
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
pub const NATIVE_UA_VERSION: u32 = 1;

/// Font weight of `h1`–`h3` in the browser user-agent stylesheet (`bold`).
pub(crate) const UA_BOLD_WEIGHT: f32 = 700.0;
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
            ..Self::new(tag)
        }
    }
}

/// Native user-agent defaults, version [`NATIVE_UA_VERSION`]. The official
/// browser host adds no normalization for these tags, so each row mirrors the
/// Chrome user-agent stylesheet (`h1 { font-size: 2em; margin-block: 0.67em;
/// font-weight: bold }`, `ul { margin-block: 1em; padding-inline-start: 40px }`,
/// `pre { font-family: monospace; white-space: pre; margin: 1em 0 }`, …).
/// Control rows carry only `text-align`; their box defaults remain explicit.
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

/// Incremental cross-sheet anchors avoid scanning unrelated (or empty) sheets.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CascadeIndex {
    roots: std::collections::BTreeSet<(u32, usize)>,
    classes: BTreeMap<String, std::collections::BTreeSet<(u32, usize)>>,
    custom_sheets: usize,
}
impl CascadeIndex {
    pub fn insert(&mut self, sheet: u32, index: &RuleIndex) {
        self.custom_sheets += usize::from(index.has_custom);
        self.roots
            .extend(index.roots.iter().map(|rule| (sheet, *rule)));
        for (class, rules) in &index.classes {
            self.classes
                .entry(class.clone())
                .or_default()
                .extend(rules.iter().map(|rule| (sheet, *rule)));
        }
    }
    pub fn remove(&mut self, sheet: u32, index: &RuleIndex) {
        self.custom_sheets -= usize::from(index.has_custom);
        for rule in &index.roots {
            self.roots.remove(&(sheet, *rule));
        }
        for (class, rules) in &index.classes {
            let empty = if let Some(entries) = self.classes.get_mut(class) {
                for rule in rules {
                    entries.remove(&(sheet, *rule));
                }
                entries.is_empty()
            } else {
                false
            };
            if empty {
                self.classes.remove(class);
            }
        }
    }
    pub fn has_custom(&self) -> bool {
        self.custom_sheets != 0
    }
    pub fn candidates(
        &self,
        root: bool,
        classes: &std::collections::BTreeSet<&str>,
    ) -> Vec<(u32, usize)> {
        if root {
            return self.roots.iter().copied().collect();
        }
        let mut candidates = std::collections::BTreeSet::new();
        for class in classes {
            if let Some(rules) = self.classes.get(*class) {
                candidates.extend(rules);
            }
        }
        candidates.into_iter().collect()
    }
}

#[cfg(test)]
mod ua_tests {
    use super::*;
    use varta_common::view::Tag;
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
        assert_eq!(NATIVE_UA_VERSION, 1);
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
        // Absolute lengths (px, rem) are never re-derived.
        let mut fixed = FontSize::medium(BASE);
        fixed.absolute(16.0);
        fixed.switch_family(BASE, false, true);
        assert!(close(fixed, 16.0));
        fixed.scale(1.5);
        fixed.switch_family(BASE, true, false);
        assert!(close(fixed, 24.0));
        assert!(sole_monospace(" Monospace ") && !sole_monospace("monospace, monospace"));
        assert!(!sole_monospace("\"monospace\""));
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use std::collections::BTreeSet;
    #[test]
    fn unrelated_and_duplicate_anchors_do_not_expand_candidate_lists() {
        let mut index = CascadeIndex::default();
        for sheet in 1..1001 {
            let parsed = parse(&format!(".unrelated{sheet} {{ color:#abc }}"), usize::MAX).unwrap();
            index.insert(sheet, &RuleIndex::new(&parsed.rules));
        }
        let rules = parse(".a, .a.b, .b, :root { color:#fff }", usize::MAX)
            .unwrap()
            .rules;
        let anchors = RuleIndex::new(&rules);
        index.insert(2000, &anchors);
        assert_eq!(
            index.candidates(false, &BTreeSet::from(["a", "b"])),
            vec![(2000, 0)]
        );
        assert_eq!(index.candidates(true, &BTreeSet::new()), vec![(2000, 0)]);
        index.remove(2000, &anchors);
        assert!(
            index
                .candidates(false, &BTreeSet::from(["a", "b"]))
                .is_empty()
        );
        assert!(index.candidates(true, &BTreeSet::new()).is_empty());
    }
}
