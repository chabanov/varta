#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
//! The view style engine: one implementation of the stylesheet admission,
//! the cascade (CSS Cascade 5: origin and importance, layers, specificity,
//! order of appearance), registered and custom properties (Properties and
//! Values API 1, CSS Variables 1), computed values and the cascade budgets,
//! for the native renderer (`varta-view`) and, compiled to wasm32 by
//! `scripts/build-view-engine.py` (`varta-style-wasm`), the browser host.
//!
//! The grammars it applies (`varta_common::view::{syntax, css, stylesheet}`)
//! and this crate are `no_std` + `alloc`, and every binary64 function goes
//! through `libm`, so both hosts compute the same bits.
//!
//! A host supplies its node tree through [`Document`] and its media and
//! lowering inputs through [`Environment`]; the engine keeps the sheets.
extern crate alloc;

mod prelude {
    pub use alloc::borrow::ToOwned;
    pub use alloc::collections::{BTreeMap, BTreeSet};
    pub use alloc::string::String;
    pub use alloc::vec;
    pub use alloc::vec::Vec;
}
pub mod font;
pub mod fonts;
pub mod index;
pub mod selectors;
pub mod session;
pub mod variables;

use prelude::*;
use varta_common::view::css::{self, Context};
use varta_common::view::stylesheet::{
    self, Media, Name, ParsedStylesheet, Rule, Specificity, StylesheetError,
};
use varta_common::view::{
    CssProperty, DRAFT_MAX_STYLESHEET_BYTES, DRAFT_MAX_STYLESHEET_WORK, DRAFT_STYLESHEET_NODE_WORK,
    ROOT_NODE,
};

pub use index::{CascadeIndex, LayerRanks, RuleCost, RuleIndex};
pub use variables::{Registered, Scope};

/// What the cascade reads of one element.
#[derive(Debug, Clone, Copy)]
pub struct Element<'a> {
    pub tag: varta_common::view::Tag,
    pub class: &'a str,
    pub style: &'a BTreeMap<CssProperty, String>,
    pub disabled: bool,
    pub checked: bool,
}

/// A host's node tree, as the style engine reads it.
pub trait Document {
    /// The element `id`, or `None` for a text node or an unknown id.
    fn element(&self, id: u32) -> Option<Element<'_>>;
    fn parent(&self, id: u32) -> Option<u32>;
    fn children(&self, id: u32) -> &[u32];
    /// Every node without a parent: the root and detached subtrees.
    fn top_level(&self) -> Vec<u32>;
    /// Selector states (Selectors 4 §9): the host's recorded inputs.
    /// `focused`: `id` has the focus (`:focus`).
    fn focused(&self, id: u32) -> bool;
    /// `id` has the focus and the host indicates it (`:focus-visible`); by
    /// default every focus.
    fn focus_visible(&self, id: u32) -> bool {
        self.focused(id)
    }
    fn hovered(&self, id: u32) -> bool;
    fn active(&self, id: u32) -> bool;
}

/// The host inputs of style resolution: media features and the reference
/// metrics of computed values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    pub media: Media,
    pub context: Context,
    /// The host's medium font size and monospace behaviour.
    pub font: font::FontBase,
}

/// An admitted stylesheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub text: String,
    pub rules: Vec<Rule>,
    pub index: RuleIndex,
    pub work: usize,
    /// The text's UTF-16 bytes, the unit of the aggregate stylesheet budget.
    pub text_units: usize,
}

/// Why a `stylesheet` operation is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetError {
    /// Sheet id 0.
    InvalidId,
    /// The text violates the grammar.
    Invalid(StylesheetError),
    /// The transaction's or the document's parse work allowance.
    WorkLimit,
    /// The document's aggregate stylesheet bytes.
    TooLarge,
}

/// A cascade budget (work or expanded bytes) was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExceeded;

/// The winning declaration of one property: its value and, for a mapped
/// longhand whose value has `var()`, its part (`varta_common::view::mapped`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Winner<'a> {
    pub value: &'a str,
    pub part: Option<varta_common::view::mapped::Part>,
    /// Its place in the element's cascade order (higher wins): what decides
    /// between a shorthand and its longhands ([`Styles::computed`]).
    pub rank: usize,
}

/// One declaration of the cascade.
#[derive(Debug, Clone, Copy)]
pub struct Declared<'a> {
    pub name: &'a Name,
    pub value: &'a str,
    /// The cascade rank of its layer (see [`LayerRanks::rank`]).
    pub layer: usize,
}

/// An element's matching declarations in ascending precedence (Cascade 5
/// §6.1): normal rule declarations, before which the element's inline
/// (element-attached) values do not apply, and important ones, which win
/// over inline values. Within each, layers, then specificity, then order of
/// appearance; important layers reverse (§6.4.3).
#[derive(Debug, Clone, Default)]
pub struct Cascade<'a> {
    pub normal: Vec<Declared<'a>>,
    pub important: Vec<Declared<'a>>,
}

/// A computed value with the CSS-wide keywords the host resolves against
/// its own user-agent origin (Cascade 5 §7.3). `revert`, `revert-layer` and
/// `unset` are resolved by the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Computed {
    Value(String),
    Inherit,
    Initial,
}
impl Computed {
    /// The CSS text a browser applies inline.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Value(value) => value,
            Self::Inherit => "inherit",
            Self::Initial => "initial",
        }
    }
}

/// The CSS-wide keyword a whole value is, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wide {
    Initial,
    Inherit,
    Unset,
    Revert,
    RevertLayer,
}
fn wide(value: &str) -> Option<Wide> {
    let value = value.trim();
    Some(match () {
        () if value.eq_ignore_ascii_case("initial") => Wide::Initial,
        () if value.eq_ignore_ascii_case("inherit") => Wide::Inherit,
        () if value.eq_ignore_ascii_case("unset") => Wide::Unset,
        () if value.eq_ignore_ascii_case("revert") => Wide::Revert,
        () if value.eq_ignore_ascii_case("revert-layer") => Wide::RevertLayer,
        () => return None,
    })
}

/// The box shorthands whose longhands are registered properties too, and
/// the longhands in component order (the box rule, or row then column).
const SHORTHANDS: [(CssProperty, &[CssProperty]); 3] = [
    (
        CssProperty::Margin,
        &[
            CssProperty::MarginTop,
            CssProperty::MarginRight,
            CssProperty::MarginBottom,
            CssProperty::MarginLeft,
        ],
    ),
    (
        CssProperty::Padding,
        &[
            CssProperty::PaddingTop,
            CssProperty::PaddingRight,
            CssProperty::PaddingBottom,
            CssProperty::PaddingLeft,
        ],
    ),
    (
        CssProperty::Gap,
        &[CssProperty::RowGap, CssProperty::ColumnGap],
    ),
];

/// Cascade 5 §1.2: a shorthand sets each of its longhands at its own place
/// in the cascade. Where a shorthand's declaration wins over a longhand's,
/// the longhand's computed value is the shorthand's component, so a host
/// that applies the shorthand and then the longhands (the browser bridge)
/// gets the cascade's result; an invalid winning shorthand leaves the
/// longhand unset.
fn shorthands_over_longhands(
    out: &mut BTreeMap<CssProperty, Computed>,
    ranks: &BTreeMap<CssProperty, usize>,
    wanted: &impl Fn(CssProperty) -> bool,
) {
    for (shorthand, longhands) in SHORTHANDS {
        let Some(rank) = ranks.get(&shorthand).filter(|_| wanted(shorthand)) else {
            continue;
        };
        let components = out.get(&shorthand).map(|computed| match computed {
            Computed::Value(value) => {
                let parts: Vec<&str> = value.split_ascii_whitespace().collect();
                let pick = |index: usize| {
                    let at = match (longhands.len(), parts.len()) {
                        (_, 1) => 0,
                        (2, _) => index.min(1),
                        (_, 2) => index % 2,
                        (_, 3) => [0, 1, 2, 1][index],
                        _ => index,
                    };
                    parts
                        .get(at)
                        .map(|part| Computed::Value((*part).to_owned()))
                };
                (0..longhands.len()).map(pick).collect::<Vec<_>>()
            }
            keyword => vec![Some(keyword.clone()); longhands.len()],
        });
        for (index, longhand) in longhands.iter().enumerate() {
            if ranks.get(longhand).is_none_or(|own| own > rank) {
                continue;
            }
            match components.as_ref().and_then(|parts| parts[index].clone()) {
                Some(component) => {
                    out.insert(*longhand, component);
                }
                None => {
                    out.remove(longhand);
                }
            }
        }
    }
}

/// Whether `property` inherits (its definition's "Inherited" line).
#[must_use]
pub fn inherited(property: CssProperty) -> bool {
    use CssProperty as P;
    matches!(
        property,
        P::Color
            | P::FontFamily
            | P::FontSize
            | P::FontWeight
            | P::FontStyle
            | P::LineHeight
            | P::TextAlign
            | P::WhiteSpace
            | P::OverflowWrap
            | P::Cursor
            // CSS Images 3 §5.1 and §5.2.
            | P::ImageOrientation
            // CSS Text 3 §8.2.
            | P::LetterSpacing
            | P::ImageRendering
    )
}

/// The document's stylesheets and their cascade index, and the view's
/// font set (the first available fonts of the font-relative units).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Styles {
    sheets: BTreeMap<u32, Sheet>,
    index: CascadeIndex,
    bytes: usize,
    work: usize,
    fonts: Fonts,
}

/// An element's font references, computed on the first value that names a
/// font-relative unit.
struct LazyFont<'f> {
    cached: Option<Option<css::FontContext>>,
    provider: &'f dyn Fn() -> Option<css::FontContext>,
}
impl LazyFont<'_> {
    fn get(&mut self) -> Option<css::FontContext> {
        *self.cached.get_or_insert_with(self.provider)
    }
}

/// A shared font set; two are equal when they are the same set.
#[derive(Debug, Clone, Default)]
struct Fonts(alloc::sync::Arc<fonts::FontSet>);
impl PartialEq for Fonts {
    fn eq(&self, other: &Self) -> bool {
        alloc::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Styles {
    /// The view's font set.
    #[must_use]
    pub fn fonts(&self) -> &fonts::FontSet {
        &self.fonts.0
    }
    /// Replace the view's font set (a host sets it with its text faces).
    pub fn set_fonts(&mut self, fonts: alloc::sync::Arc<fonts::FontSet>) {
        self.fonts = Fonts(fonts);
    }
    #[must_use]
    pub fn sheets(&self) -> &BTreeMap<u32, Sheet> {
        &self.sheets
    }
    #[must_use]
    pub fn index(&self) -> &CascadeIndex {
        &self.index
    }
    #[must_use]
    pub fn has_custom(&self) -> bool {
        self.index.has_custom()
    }
    #[must_use]
    pub fn registered(&self) -> &Registered {
        self.index.registered()
    }
    /// The `stylesheet` operation: replace (or with an empty text remove)
    /// sheet `id`. `transaction_work` is the parse work the batch has
    /// already spent (one allowance spans a batch's sheets).
    ///
    /// # Errors
    /// The refusal; nothing changes then.
    pub fn set(
        &mut self,
        id: u32,
        text: &str,
        transaction_work: &mut usize,
    ) -> Result<(), SheetError> {
        if id == 0 {
            return Err(SheetError::InvalidId);
        }
        if text.is_empty() {
            if let Some(old) = self.sheets.remove(&id) {
                self.index.remove(id, &old.index);
                self.bytes -= old.text_units;
                self.work -= old.work;
            }
            return Ok(());
        }
        let parsed: ParsedStylesheet = stylesheet::parse_with_work_limit(
            text,
            DRAFT_MAX_STYLESHEET_WORK.saturating_sub(*transaction_work),
        )
        .map_err(|error| {
            if error.violation == stylesheet::Violation::WorkLimit {
                SheetError::WorkLimit
            } else {
                SheetError::Invalid(error)
            }
        })?;
        *transaction_work = transaction_work.saturating_add(parsed.work);
        let text_units = text.encode_utf16().count() * 2;
        let old = self.sheets.get(&id);
        let bytes = self.bytes - old.map_or(0, |sheet| sheet.text_units) + text_units;
        if bytes > DRAFT_MAX_STYLESHEET_BYTES {
            return Err(SheetError::TooLarge);
        }
        let work = self.work - old.map_or(0, |sheet| sheet.work) + parsed.work;
        if work > DRAFT_MAX_STYLESHEET_WORK {
            return Err(SheetError::WorkLimit);
        }
        // Registrations are document-wide and seed every scope.
        let registered = self
            .sheets
            .iter()
            .filter(|(other, _)| **other != id)
            .map(|(_, other)| other.index.properties.len())
            .sum::<usize>()
            .saturating_add(parsed.properties.len());
        if registered > stylesheet::MAX_REGISTERED_PROPERTIES {
            return Err(SheetError::WorkLimit);
        }
        let index = RuleIndex::parsed(&parsed);
        if let Some(old) = self.sheets.get(&id) {
            self.index.remove(id, &old.index);
        }
        self.index.insert(id, &index);
        self.sheets.insert(
            id,
            Sheet {
                text: text.into(),
                rules: parsed.rules,
                index,
                work: parsed.work,
                text_units,
            },
        );
        self.bytes = bytes;
        self.work = work;
        Ok(())
    }

    /// The matching declarations of `id` (Cascade 5 §6.1).
    #[must_use]
    pub fn cascade<'a>(&'a self, document: &impl Document, id: u32, media: &Media) -> Cascade<'a> {
        let Some(element) = document.element(id) else {
            return Cascade::default();
        };
        let classes: BTreeSet<&str> = element.class.split_ascii_whitespace().collect();
        let ranks = self.index.ranks(media);
        let mut normal = Vec::new();
        let mut important = Vec::new();
        for (sheet, index) in self.index.candidates(id == ROOT_NODE, &classes) {
            let rule = &self.sheets[&sheet].rules[index];
            if rule.applies(media)
                && let Some(specificity) = selectors::matching(document, id, &rule.selectors)
            {
                let layer = ranks.rank(sheet, rule.layer);
                if rule.important {
                    important.push((layer, specificity, sheet, index, rule));
                } else {
                    normal.push((layer, specificity, sheet, index, rule));
                }
            }
        }
        normal.sort_by_key(|(layer, specificity, sheet, index, _)| {
            (*layer, *specificity, *sheet, *index)
        });
        // §6.4.3: for important declarations, earlier layers win and
        // unlayered declarations lose to every layer.
        important.sort_by_key(|(layer, specificity, sheet, index, _)| {
            (core::cmp::Reverse(*layer), *specificity, *sheet, *index)
        });
        let flatten = |list: Vec<(usize, Specificity, u32, usize, &'a Rule)>| {
            list.into_iter()
                .flat_map(|(layer, _, _, _, rule)| {
                    rule.declarations.iter().map(move |(name, value)| Declared {
                        name,
                        value: value.as_str(),
                        layer,
                    })
                })
                .collect()
        };
        Cascade {
            normal: flatten(normal),
            important: flatten(important),
        }
    }

    /// The custom-property scope of `id`: each ancestor's declarations
    /// resolved before inheritance, registered properties seeded with their
    /// initial values.
    #[must_use]
    pub fn scope<'a>(
        &'a self,
        document: &impl Document,
        id: u32,
        environment: &Environment,
    ) -> Scope<'a> {
        let media = &environment.media;
        let registered = self.index.registered();
        let mut scope = if registered.is_empty() {
            Scope::default()
        } else {
            Scope::registered(registered)
        };
        if self.index.has_custom() {
            let mut path = vec![id];
            let mut at = id;
            while let Some(parent) = document.parent(at) {
                path.push(parent);
                at = parent;
            }
            for ancestor in path.into_iter().rev() {
                scope.enter();
                let cascade = self.cascade(document, ancestor, media);
                let mut raw = BTreeMap::new();
                for declared in cascade.normal.iter().chain(&cascade.important) {
                    if let Name::Custom(name) = declared.name {
                        raw.insert(name.as_str(), declared.value);
                    }
                }
                // A registered length computes against the declaring
                // element's font (Properties and Values API 1 §2.4).
                let font = core::cell::OnceCell::new();
                let absolutize = |value: &str| {
                    let font = if font::uses_font_units(value) {
                        *font
                            .get_or_init(|| self.font_context(document, ancestor, environment).ok())
                    } else {
                        None
                    };
                    let context = css::Context {
                        font,
                        ..environment.context
                    };
                    css::absolutize_lengths(value, &context)
                };
                scope.extend(raw, &absolutize);
            }
        }
        scope
    }

    /// The declared value of every property of `id`: normal rule
    /// declarations, then inline values, then important declarations, with
    /// `revert-layer` rolled back to the earlier layers' winner and `revert`
    /// rolled back to the user-agent origin (no author value). Values still
    /// carry `var()` references.
    #[must_use]
    pub fn declared<'a>(
        &'a self,
        document: &'a impl Document,
        id: u32,
        media: &Media,
    ) -> BTreeMap<CssProperty, Winner<'a>> {
        let cascade = self.cascade(document, id, media);
        let inline = document.element(id).map(|element| element.style);
        let mut out = BTreeMap::new();
        for property in CssProperty::ALL.iter().copied() {
            if let Some(value) = Self::winner(&cascade, inline, property) {
                out.insert(property, value);
            }
        }
        out
    }

    /// The cascaded value of one property (Cascade 5 §6.1, §7.3.4–§7.3.5).
    fn winner<'a>(
        cascade: &Cascade<'a>,
        inline: Option<&'a BTreeMap<CssProperty, String>>,
        property: CssProperty,
    ) -> Option<Winner<'a>> {
        // Candidates in descending precedence, with the layer of each rule
        // declaration (inline values are element-attached, outside layers).
        // A mapped longhand declares its physical property (Logical 1 §4).
        let mut candidates: Vec<(Option<usize>, Winner<'a>)> = Vec::new();
        let inline_rank = cascade.normal.len();
        let rule = |offset: usize| {
            move |(index, declared): (usize, &Declared<'a>)| {
                (declared.name.property() == Some(property)).then(|| {
                    (
                        Some(declared.layer),
                        Winner {
                            value: declared.value,
                            part: declared.name.part(),
                            rank: offset + index,
                        },
                    )
                })
            }
        };
        candidates.extend(
            cascade
                .important
                .iter()
                .enumerate()
                .rev()
                .filter_map(rule(inline_rank + 1)),
        );
        if let Some(value) = inline.and_then(|inline| inline.get(&property)) {
            candidates.push((
                None,
                Winner {
                    value: value.as_str(),
                    part: None,
                    rank: inline_rank,
                },
            ));
        }
        candidates.extend(cascade.normal.iter().enumerate().rev().filter_map(rule(0)));
        let mut at = 0;
        while let Some((layer, Winner { value, .. })) = candidates.get(at) {
            match wide(value) {
                Some(Wide::Revert) => return None,
                Some(Wide::RevertLayer) => {
                    // Roll back to the cascade without this layer's values;
                    // an element-attached value rolls back to the rules.
                    let layer = *layer;
                    at += 1;
                    while layer.is_some()
                        && candidates.get(at).is_some_and(|(other, _)| *other == layer)
                    {
                        at += 1;
                    }
                }
                _ => return Some(candidates[at].1),
            }
        }
        None
    }

    /// The computed values of `id`: the declared values after `var()`
    /// substitution (CSS Variables 1 §3; an invalid result is invalid at
    /// computed-value time and leaves the property to inheritance or its
    /// initial value), checked by `validate`, CSS-wide keywords resolved to
    /// [`Computed::Inherit`]/[`Computed::Initial`] (`unset` by inheritance,
    /// Cascade 5 §7.3.3) and values lowered by [`css::lower`].
    #[must_use]
    pub fn computed(
        &self,
        document: &impl Document,
        id: u32,
        environment: &Environment,
        validate: impl Fn(CssProperty, &str) -> bool,
    ) -> BTreeMap<CssProperty, Computed> {
        let font = || match environment.context.font {
            Some(font) => Some(font),
            None => self.font_context(document, id, environment).ok(),
        };
        self.computed_where(document, id, environment, |_| true, (&validate, &font))
    }

    /// The computed values of `wanted` properties of `id` only, lowered in
    /// `environment` as given (see [`Styles::computed`]).
    /// The computed values of `wanted` properties of `id` only, with
    /// font-relative units resolved from `font` (called at most once, on
    /// the first value that names one).
    pub(crate) fn computed_only(
        &self,
        document: &impl Document,
        id: u32,
        environment: &Environment,
        wanted: &[CssProperty],
        font: &dyn Fn() -> Option<css::FontContext>,
    ) -> BTreeMap<CssProperty, Computed> {
        self.computed_where(
            document,
            id,
            environment,
            |property| wanted.contains(&property),
            (&|_, _| true, font),
        )
    }

    fn computed_where(
        &self,
        document: &impl Document,
        id: u32,
        environment: &Environment,
        wanted: impl Fn(CssProperty) -> bool,
        (validate, font_context): (
            &impl Fn(CssProperty, &str) -> bool,
            &dyn Fn() -> Option<css::FontContext>,
        ),
    ) -> BTreeMap<CssProperty, Computed> {
        let declared = self.declared(document, id, &environment.media);
        let mut scope: Option<Scope<'_>> = None;
        // The element's font references, computed on the first value that
        // names a font-relative unit.
        let mut font = LazyFont {
            cached: None,
            provider: font_context,
        };
        let mut out = BTreeMap::new();
        let ranks: BTreeMap<CssProperty, usize> = declared
            .iter()
            .map(|(property, winner)| (*property, winner.rank))
            .collect();
        for (property, raw) in declared {
            if !wanted(property) {
                continue;
            }
            if let Some(computed) = self.compute(
                document,
                (id, environment),
                (&mut scope, &mut font),
                property,
                raw,
                validate,
            ) {
                out.insert(property, computed);
            }
        }
        shorthands_over_longhands(&mut out, &ranks, &wanted);
        out
    }

    /// One computed value (see [`Styles::computed`]).
    fn compute<'a>(
        &'a self,
        document: &impl Document,
        (id, environment): (u32, &Environment),
        (scope, font): (&mut Option<Scope<'a>>, &mut LazyFont<'_>),
        property: CssProperty,
        raw: Winner<'_>,
        validate: &impl Fn(CssProperty, &str) -> bool,
    ) -> Option<Computed> {
        let substituted = if css::contains_variable(raw.value) {
            scope
                .get_or_insert_with(|| self.scope(document, id, environment))
                .substitute(raw.value)?
        } else {
            raw.value.to_owned()
        };
        // A mapped longhand's component of the substituted value (Variables
        // 1 §3.1: the pending-substitution value of a shorthand).
        let substituted = match raw.part {
            Some(part) => part.source.component(&substituted, part.index)?,
            None => substituted,
        };
        match wide(&substituted) {
            Some(Wide::Initial) => return Some(Computed::Initial),
            Some(Wide::Inherit) => return Some(Computed::Inherit),
            Some(Wide::Unset) => {
                return Some(if inherited(property) {
                    Computed::Inherit
                } else {
                    Computed::Initial
                });
            }
            // A substituted `revert` rolls back to the user-agent origin.
            Some(Wide::Revert | Wide::RevertLayer) => return None,
            None => {}
        }
        if !validate(property, &substituted) {
            return None;
        }
        let mut context = environment.context;
        if font::uses_font_units(&substituted) {
            context.font = font.get();
        }
        css::lower(property, &substituted, &context).map(Computed::Value)
    }

    /// Whether any rule declares a transition duration or delay.
    #[must_use]
    pub fn declares_transitions(&self) -> bool {
        self.sheets.values().any(|sheet| {
            sheet.rules.iter().any(|rule| {
                rule.declarations.iter().any(|(name, _)| {
                    matches!(
                        name,
                        Name::Property(
                            CssProperty::TransitionDuration | CssProperty::TransitionDelay
                        )
                    )
                })
            })
        })
    }

    /// Whether every rule's media decision agrees in two environments.
    #[must_use]
    pub fn same_media_decisions(&self, a: &Media, b: &Media) -> bool {
        self.sheets.values().all(|sheet| {
            sheet
                .rules
                .iter()
                .all(|rule| rule.applies(a) == rule.applies(b))
        }) && (!self.index.conditional_layers() || self.index.ranks(a) == self.index.ranks(b))
    }

    /// Potential cascade work of every node (an admission estimate,
    /// independent of media and selector state).
    ///
    /// # Errors
    /// When the work exceeds `allowance`.
    pub fn potential_cascade_work(
        &self,
        document: &impl Document,
        allowance: usize,
    ) -> Result<usize, BudgetExceeded> {
        let weight = DRAFT_STYLESHEET_NODE_WORK;
        let mut total = 0usize;
        let mut charge = |work: usize| {
            total = total.saturating_add(work);
            if total > allowance {
                Err(BudgetExceeded)
            } else {
                Ok(())
            }
        };
        // (node, inherited scope work, its ancestors, its siblings).
        let top = document.top_level();
        let mut pending: Vec<(u32, usize, usize, usize)> =
            top.iter().map(|id| (*id, 0usize, 0usize, 0usize)).collect();
        while let Some((id, ancestors, depth, siblings)) = pending.pop() {
            charge(weight)?;
            let mut own = weight;
            let mut variables = false;
            if let Some(element) = document.element(id) {
                charge(element.class.len())?;
                own = own.saturating_add(element.class.len());
                for value in element.style.values() {
                    let work = weight + value.len();
                    charge(work)?;
                    own = own.saturating_add(work);
                    variables |= css::contains_variable(value);
                }
                let classes = element.class.split_ascii_whitespace().collect();
                for (sheet, index) in self.index.candidates(id == ROOT_NODE, &classes) {
                    let cost = self.sheets[&sheet].index.costs[index];
                    charge(cost.work)?;
                    own = own.saturating_add(cost.work);
                    variables |= cost.variables;
                    // Selectors that search ancestors, siblings or nested
                    // arguments charge that search (`selectors`).
                    for selector in &self.sheets[&sheet].rules[index].selectors {
                        if selector.searches() {
                            charge(selectors::selector_work(selector, depth, siblings))?;
                        }
                    }
                }
            }
            let scope = ancestors.saturating_add(own);
            if variables && self.index.has_custom() {
                charge(scope)?;
            }
            let children = document.children(id);
            pending.extend(
                children
                    .iter()
                    .map(|child| (*child, scope, depth + 1, children.len())),
            );
        }
        Ok(total)
    }

    /// Potential UTF-16 component text. Scope alternatives ignore
    /// environment, local state and winners, while aliases do not duplicate
    /// fresh literals.
    ///
    /// # Errors
    /// When the bytes exceed `allowance`.
    pub fn potential_expanded_values(
        &self,
        document: &impl Document,
        allowance: usize,
    ) -> Result<usize, BudgetExceeded> {
        use varta_common::view::expansion::{Checkpoint, Scope, Value};
        enum Work {
            Enter(u32),
            Leave(Checkpoint),
        }
        let mut pending: Vec<Work> = document.top_level().into_iter().map(Work::Enter).collect();
        let mut scope = Scope::default();
        let mut total = 0usize;
        let mut charge = |units: usize| {
            total = total.saturating_add(units.saturating_mul(2));
            if total > allowance {
                Err(BudgetExceeded)
            } else {
                Ok(())
            }
        };
        // Registered initial values (Properties and Values API 1 §3.3) are
        // the outermost definitions of every scope.
        let initial: Vec<(&str, Value)> = self
            .index
            .registered()
            .values()
            .filter_map(|registration| {
                let value = registration.initial.as_deref()?;
                Some((registration.name.as_str(), Value::new(value)))
            })
            .collect();
        if !initial.is_empty() {
            let base: BTreeMap<&str, Vec<&Value>> = initial
                .iter()
                .map(|(name, value)| (*name, vec![value]))
                .collect();
            let (_, fresh) = scope.extend(&base);
            charge(fresh)?;
        }
        while let Some(task) = pending.pop() {
            let id = match task {
                Work::Leave(checkpoint) => {
                    scope.restore(checkpoint);
                    continue;
                }
                Work::Enter(id) => id,
            };
            let mut custom: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
            let mut properties: BTreeMap<CssProperty, Vec<&Value>> = BTreeMap::new();
            let mut inline = Vec::new();
            if let Some(element) = document.element(id) {
                let classes = element.class.split_ascii_whitespace().collect();
                for (sheet, index) in self.index.candidates(id == ROOT_NODE, &classes) {
                    for (name, value) in &self.sheets[&sheet].index.expansion[index] {
                        match name {
                            Name::Custom(name) => custom.entry(name).or_default().push(value),
                            Name::Property(property) | Name::Mapped(property, _) => {
                                properties.entry(*property).or_default().push(value);
                            }
                        }
                    }
                }
                inline.extend(
                    element
                        .style
                        .iter()
                        .map(|(property, value)| (*property, Value::new(value))),
                );
                for (property, value) in &inline {
                    properties.entry(*property).or_default().push(value);
                }
            }
            let (checkpoint, fresh) = scope.extend(&custom);
            charge(fresh)?;
            for values in properties.values() {
                charge(
                    values
                        .iter()
                        .map(|value| scope.upper(value))
                        .max()
                        .unwrap_or(0),
                )?;
            }
            pending.push(Work::Leave(checkpoint));
            pending.extend(
                document
                    .children(id)
                    .iter()
                    .rev()
                    .map(|id| Work::Enter(*id)),
            );
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests;
