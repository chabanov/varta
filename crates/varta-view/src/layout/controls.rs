//! User-agent box defaults and intrinsic sizes of form controls
//! ([`style::NATIVE_CONTROL_DEFAULTS`]), shared by native layout and paint.
use super::{LayoutError, text_style};
use crate::style::{self, ControlDefaults, ControlKind, Intrinsic};
use crate::text::{TextOptions, TextSource, WhiteSpaceMode};
use crate::tree::Tree;
use crate::{Environment, TextMeasurer};
use skrifa::{FontRef, MetadataProvider};
use std::collections::BTreeMap;
use taffy::prelude::*;
use varta_common::view::{Attribute, CssProperty, Tag};

/// The character whose advance Chrome on macOS takes as a text field's
/// average character width (`SimpleFontData::avg_char_width_`).
const AVERAGE_CHARACTER: char = 'x';

/// A themed or partially styled control's intrinsic content box, measured
/// once per geometry pass from its own font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ControlSize {
    pub width: f32,
    /// Min-content width: Chrome lets a flex item `textarea` shrink to its
    /// padding and borders, while text fields keep their intrinsic width.
    pub min_width: f32,
    pub height: f32,
}
impl ControlSize {
    /// The content size Taffy measures for this control's leaf.
    pub(crate) fn measure(
        self,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        Size {
            width: known.width.unwrap_or(match available.width {
                AvailableSpace::MinContent => self.min_width,
                _ => self.width,
            }),
            height: known.height.unwrap_or(self.height),
        }
    }
}

/// Whether an author `width`/`height` value leaves the axis automatic.
fn automatic(value: Option<&String>) -> bool {
    value.is_none_or(|value| value == "auto")
}

/// Whether `id`'s control needs a measured intrinsic content size: a text
/// field or textarea with an automatic width or height.
pub(crate) fn needs_intrinsic(tree: &Tree, env: &Environment, id: u32) -> bool {
    let Some(element) = tree.element(id) else {
        return false;
    };
    if !matches!(
        ControlKind::of(element),
        Some(ControlKind::TextField | ControlKind::Textarea)
    ) {
        return false;
    }
    let computed = tree.computed_read(id, env);
    if element.tag == Tag::Textarea
        && computed
            .get(&CssProperty::FieldSizing)
            .is_some_and(|value| value == "content")
    {
        return false;
    }
    automatic(computed.get(&CssProperty::Width)) || automatic(computed.get(&CssProperty::Height))
}

/// The primary font's `x` advance and ascent at `options`' size, shaped with
/// only the first resolved family (Chrome's primary font). A primary font
/// without an `x` glyph sizes fields from its x-height in Chrome, which this
/// renderer does not model, so it is refused.
fn character_metrics(
    measurer: &mut TextMeasurer,
    families: &[String],
    options: TextOptions,
    id: u32,
) -> Result<(f32, f32), LayoutError> {
    let primary = families.first().ok_or(LayoutError::Unsupported(id))?;
    let (_, layout) = measurer
        .shape_styled(
            &AVERAGE_CHARACTER.to_string(),
            None,
            std::slice::from_ref(primary),
            options,
        )
        .map_err(LayoutError::text(id))?;
    let line = layout.lines().next().ok_or(LayoutError::Unsupported(id))?;
    let mut items = line.runs();
    let run = items.next().ok_or(LayoutError::Unsupported(id))?;
    let data = run.font();
    let font = FontRef::from_index(data.data.data(), data.index)
        .map_err(|_| LayoutError::Unsupported(id))?;
    if font.charmap().map(AVERAGE_CHARACTER).is_none() || items.next().is_some() {
        return Err(LayoutError::Unsupported(id));
    }
    Ok((run.advance(), run.metrics().ascent))
}

/// Whether `id` is a form control or inside one (a button's label).
fn in_control(tree: &Tree, id: u32) -> bool {
    tree.path(id).into_iter().any(|ancestor| {
        ancestor != varta_common::view::ROOT_NODE
            && tree
                .element(ancestor)
                .is_some_and(|element| ControlKind::of(element).is_some())
    })
}

/// Chrome's `line-height: normal` of control text: the primary font's
/// ascent, descent and line gap each rounded to whole pixels
/// (`SimpleFontData::line_spacing_`). The control font's fractional size
/// would otherwise give a fractional line. Other text keeps the shaper's
/// `normal`; explicit line heights are unchanged.
pub(crate) fn control_line_height(
    tree: &Tree,
    id: u32,
    measurer: &mut TextMeasurer,
    families: &[String],
    mut options: TextOptions,
) -> Result<TextOptions, LayoutError> {
    if options.line_height.is_some() || !in_control(tree, id) {
        return Ok(options);
    }
    let primary = families.first().ok_or(LayoutError::Unsupported(id))?;
    let (_, layout) = measurer
        .shape_styled(" ", None, std::slice::from_ref(primary), options)
        .map_err(LayoutError::text(id))?;
    let run = layout
        .lines()
        .next()
        .and_then(|line| line.runs().next())
        .ok_or(LayoutError::Unsupported(id))?;
    let metrics = run.metrics();
    options.line_height = Some(parley::LineHeight::Absolute(
        metrics.ascent.round() + metrics.descent.round() + metrics.leading.round(),
    ));
    Ok(options)
}

/// Chrome's intrinsic content box of a themed or unsized text field or
/// textarea (`Intrinsic::Characters` / `Intrinsic::Lines`).
pub(crate) fn intrinsic(
    tree: &Tree,
    env: &Environment,
    id: u32,
    (measurer, font_size): (&mut TextMeasurer, f32),
) -> Result<ControlSize, LayoutError> {
    let element = tree.element(id).ok_or(LayoutError::Unsupported(id))?;
    let kind = ControlKind::of(element).ok_or(LayoutError::Unsupported(id))?;
    let (options, families) = text_style(tree, env, id, font_size, measurer)?;
    let mut options = control_line_height(tree, id, measurer, &families, options)?;
    options.source = TextSource::Field;
    options.white_space = WhiteSpaceMode::NoWrap;
    let line = measurer
        .measure_styled(" ", None, &families, options)
        .map_err(LayoutError::text(id))?
        .height;
    let (advance, ascent) = character_metrics(measurer, &families, options, id)?;
    // Blink rounds a valid average character width to whole pixels.
    let average = advance.round();
    Ok(match style::control_defaults(kind).intrinsic {
        Intrinsic::Characters { columns } => {
            let width = (columns.saturating_sub(1) as f32 * average + advance.max(ascent)).ceil();
            ControlSize {
                width,
                min_width: width,
                height: line,
            }
        }
        Intrinsic::Lines { columns, rows } => {
            let rows = element
                .attributes
                .get(&Attribute::Rows)
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(rows);
            ControlSize {
                width: columns as f32 * average + style::UA_TEXTAREA_SCROLLBAR_GUTTER_PX,
                min_width: 0.0,
                height: rows as f32 * line,
            }
        }
        Intrinsic::Content | Intrinsic::Square(_) | Intrinsic::Refused => {
            return Err(LayoutError::Unsupported(id));
        }
    })
}

/// Input types native layout renders; `number` edits as text (its value
/// stays a string, as L0 decides).
const ADMITTED_INPUT_TYPES: &[&str] =
    &["text", "search", "password", "number", "checkbox", "radio"];

/// Properties a control kept on the explicit-style path must declare: a
/// select (whose arrow and popup have their own painter) and an author-styled
/// checkbox or radio (Chrome keeps its theme; Varta paints the author box).
const EXPLICIT_BOX: &[CssProperty] = &[
    CssProperty::Display,
    CssProperty::Width,
    CssProperty::Height,
    CssProperty::MinWidth,
    CssProperty::MinHeight,
    CssProperty::PaddingTop,
    CssProperty::PaddingRight,
    CssProperty::PaddingBottom,
    CssProperty::PaddingLeft,
];

/// Check a form control's box before author declarations apply, and set its
/// user-agent defaults. A control is either themed (no author background,
/// border or radius; [`style::themed`]) or author-styled with the `border`
/// shorthand: other partial author box styles would need Chrome's
/// inset/outset user-agent borders and are refused. An absent `display` is
/// Chrome's `inline-block`, an atomic inline in a line box that native
/// layout does not model; as a flex or grid item it is blockified. A flex or
/// grid button is an ordinary container with the initial alignment, as in
/// Chrome.
pub(crate) fn control_box(
    tree: &Tree,
    env: &Environment,
    (id, kind): (u32, ControlKind),
    style: &mut Style,
) -> Result<(), LayoutError> {
    let element = tree.element(id).ok_or(LayoutError::Unsupported(id))?;
    let has_children = !tree.nodes[&id].children.is_empty();
    let properties = super::layout_properties(tree, env, id)?;
    let computed = tree.computed_read(id, env);
    let themed = style::themed(&computed);
    let content_sizing = element.tag == Tag::Textarea
        && computed
            .get(&CssProperty::FieldSizing)
            .is_some_and(|value| value == "content");
    let explicit = || {
        EXPLICIT_BOX.iter().all(|property| {
            properties.contains_key(property)
                || (*property == CssProperty::Height && content_sizing)
        }) && [
            CssProperty::Width,
            CssProperty::MinWidth,
            CssProperty::MinHeight,
        ]
        .iter()
        .all(|property| !automatic(properties.get(property)))
            && (content_sizing || !automatic(properties.get(&CssProperty::Height)))
    };
    let checkable = matches!(kind, ControlKind::Checkbox | ControlKind::Radio);
    let refused = (element.tag == Tag::Input
        && (!ADMITTED_INPUT_TYPES.contains(&element.input_type()) || has_children))
        || (element.tag == Tag::Textarea && has_children)
        || (!themed && !computed.contains_key(&CssProperty::Border))
        || (!properties.contains_key(&CssProperty::Display)
            && !crate::inline::blockified(tree, env, id))
        || (kind == ControlKind::Select && (themed || !explicit()))
        || (checkable && !themed && !explicit())
        // An author-styled textarea's scrollbar gutter differs from the
        // themed one; it needs an explicit width.
        || (kind == ControlKind::Textarea
            && !themed
            && automatic(properties.get(&CssProperty::Width)));
    if refused {
        return Err(LayoutError::Unsupported(id));
    }
    apply_defaults(style::control_defaults(kind), &properties, style);
    style.box_sizing = BoxSizing::BorderBox;
    Ok(())
}

/// Finish a control's box after author declarations: an automatic width of a
/// block-level control fits its content (Blink's
/// `LayoutBox::AutoWidthShouldFitContent`), and a block button centers its
/// content like Chrome's anonymous button content box.
pub(crate) fn control_finish(
    tree: &Tree,
    env: &Environment,
    (id, kind): (u32, ControlKind),
    style: &mut Style,
) -> Result<(), LayoutError> {
    let properties = super::layout_properties(tree, env, id)?;
    if !crate::inline::blockified(tree, env, id) && automatic(properties.get(&CssProperty::Width)) {
        style.size.width = Dimension::fit_content();
    }
    if kind == ControlKind::Button && style.display == Display::Block {
        style.display = Display::Flex;
        style.flex_direction = FlexDirection::Column;
        style.justify_content = JustifyContent::CENTER;
        style.align_items = AlignItems::STRETCH;
    }
    Ok(())
}

/// Apply `defaults`' padding to the sides without an author value. Author
/// declarations applied afterwards replace these values.
pub(crate) fn apply_defaults(
    defaults: &ControlDefaults,
    properties: &BTreeMap<CssProperty, String>,
    style: &mut Style,
) {
    let sides = [
        CssProperty::PaddingTop,
        CssProperty::PaddingRight,
        CssProperty::PaddingBottom,
        CssProperty::PaddingLeft,
    ];
    let mut padding = [
        &mut style.padding.top,
        &mut style.padding.right,
        &mut style.padding.bottom,
        &mut style.padding.left,
    ];
    for ((side, value), slot) in sides.iter().zip(defaults.padding).zip(&mut padding) {
        if !properties.contains_key(side) {
            **slot = length(value);
        }
    }
}

/// A checkbox or radio's theme square as its intrinsic content size (Blink's
/// `LayoutTheme` sizes it as an intrinsic, not a specified, size): stretch
/// alignment in a flex line or grid area still grows an automatic axis, and
/// the square is also the automatic minimum, so a flex item does not shrink
/// below it unless the author allows it. Author sizes win as known sizes.
pub(crate) fn square(tree: &Tree, id: u32) -> Option<ControlSize> {
    let element = tree.element(id)?;
    match style::control_defaults(ControlKind::of(element)?).intrinsic {
        Intrinsic::Square(side) => Some(ControlSize {
            width: side,
            min_width: side,
            height: side,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Access, BatchWriter, Broker, LayoutError as Error, TextFont, View};
    use varta_common::view::CssProperty as P;

    struct Deny;
    impl Broker for Deny {
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

    const FAMILY: &str = "Varta Controls Fixture";
    const VIEWPORT: (f32, f32) = (400.0, 200.0);
    const MEDIUM: f32 = 16.0;

    fn font() -> TextMeasurer {
        TextMeasurer::new_with_fonts(vec![TextFont {
            data: include_bytes!("../../tests/fixtures/controls.ttf").to_vec(),
            family: FAMILY.into(),
            aliases: Vec::new(),
        }])
        .unwrap()
    }

    /// A container (node 2) with the fixture font and `display`, holding
    /// what `fill` adds.
    fn view(display: &str, fill: impl FnOnce(&mut BatchWriter)) -> View {
        let mut b = BatchWriter::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::Display, display)
            .set_style(2, P::AlignItems, "flex-start")
            .set_style(2, P::FontFamily, "\"Varta Controls Fixture\"");
        fill(&mut b);
        let mut view = View::default();
        view.commit(&b.finish(), false, &mut Deny).unwrap();
        view
    }

    fn boxes(view: &View) -> Result<BTreeMap<u32, crate::BoxRect>, Error> {
        view.layout_boxes_with_text(VIEWPORT.0, VIEWPORT.1, &mut font(), MEDIUM)
    }

    fn size(rect: crate::BoxRect) -> (f32, f32) {
        (rect.width, rect.height)
    }

    /// Chrome 156 (macOS) sizes, from `native_controls_probe`'s oracle: the
    /// fixture's `x` advances 500 and its ascent is 800 units.
    #[test]
    fn unsized_themed_controls_take_chrome_intrinsic_sizes() {
        let wrapped = view("flex", |b| {
            // Wrapping lines keep every item at its flex basis.
            b.set_style(2, P::FlexWrap, "wrap");
            b.create_element(3, Tag::Button)
                .insert(2, 3, 0)
                .create_text(4, "AB")
                .insert(3, 4, 0);
            for (id, kind) in [(5, "text"), (6, "number"), (7, "checkbox"), (8, "radio")] {
                b.create_element(id, Tag::Input)
                    .set_attribute(id, Attribute::Type, kind)
                    .insert(2, id, 0);
            }
            b.create_element(9, Tag::Textarea).insert(2, 9, 0);
            b.create_element(10, Tag::Textarea)
                .set_attribute(10, Attribute::Rows, "1")
                .set_style(10, P::FontSize, "20px")
                .insert(2, 10, 0);
        });
        let sizes = boxes(&wrapped).unwrap();
        // 13.333px control font: 14px lines, 2px borders and 1px 6px padding.
        assert!((sizes[&3].width - (800.0 / 75.0 + 16.0)).abs() < 1e-3);
        assert_eq!(sizes[&3].height, 20.0);
        // ⌈19·round(6.67) + max(6.67, 10.67)⌉ + 2·(2 + 2).
        assert_eq!(size(sizes[&5]), (152.0, 20.0));
        assert_eq!(size(sizes[&6]), (152.0, 20.0));
        assert_eq!(size(sizes[&7]), (13.0, 13.0));
        assert_eq!(size(sizes[&8]), (13.0, 13.0));
        // 20 columns of 7px, the 16px gutter, two 14px rows, 2px padding and
        // 1px borders; an author size scales them.
        assert_eq!(size(sizes[&9]), (162.0, 34.0));
        assert_eq!(size(sizes[&10]), (222.0, 26.0));
        // In an overflowing row a textarea shrinks (its minimum is its
        // padding and borders) while a checkable keeps its square: Chrome 156
        // gives 7px and 13px here.
        let row = view("flex", |b| {
            b.set_style(2, P::Width, "20px");
            b.create_element(3, Tag::Textarea).insert(2, 3, 0);
            b.create_element(4, Tag::Input)
                .set_attribute(4, Attribute::Type, "checkbox")
                .insert(2, 4, 0);
        });
        let shrunk = boxes(&row).unwrap();
        assert_eq!(size(shrunk[&3]), (7.0, 34.0));
        assert_eq!(size(shrunk[&4]), (13.0, 13.0));
    }

    /// Chrome 156: beside a 4-row textarea in a row with the default
    /// `align-items` (stretch) a checkbox is 13×62, and in a stretch grid
    /// area 150×34; an author size still wins.
    #[test]
    fn checkable_squares_are_intrinsic_sizes_that_stretch() {
        let row = view("flex", |b| {
            b.remove_style(2, P::AlignItems);
            b.create_element(3, Tag::Textarea)
                .set_attribute(3, Attribute::Rows, "4")
                .insert(2, 3, 0);
            for (id, kind) in [(4, "checkbox"), (5, "radio")] {
                b.create_element(id, Tag::Input)
                    .set_attribute(id, Attribute::Type, kind)
                    .insert(2, id, 0);
            }
            b.create_element(6, Tag::Input)
                .set_attribute(6, Attribute::Type, "checkbox")
                .set_style(6, P::AlignSelf, "center")
                .insert(2, 6, 0);
        });
        let sizes = boxes(&row).unwrap();
        assert_eq!(size(sizes[&3]), (162.0, 62.0));
        assert_eq!(size(sizes[&4]), (13.0, 62.0));
        assert_eq!(size(sizes[&5]), (13.0, 62.0));
        assert_eq!(size(sizes[&6]), (13.0, 13.0));
        let grid = view("grid", |b| {
            b.remove_style(2, P::AlignItems)
                .set_style(2, P::GridTemplateColumns, "150px 150px")
                .set_style(2, P::GridTemplateRows, "34px");
            b.create_element(3, Tag::Input)
                .set_attribute(3, Attribute::Type, "radio")
                .insert(2, 3, 0);
            b.create_element(4, Tag::Input)
                .set_attribute(4, Attribute::Type, "checkbox")
                .set_style(4, P::Display, "block")
                .set_style(4, P::Border, "1px solid #000000")
                .set_style(4, P::Width, "20px")
                .set_style(4, P::Height, "20px")
                .set_style(4, P::MinWidth, "0")
                .set_style(4, P::MinHeight, "0")
                .set_style(4, P::Padding, "0")
                .insert(2, 4, 0);
        });
        let sizes = boxes(&grid).unwrap();
        assert_eq!(size(sizes[&3]), (150.0, 34.0));
        assert_eq!(size(sizes[&4]), (20.0, 20.0));
        // The geometry-only path needs no font for a square.
        assert_eq!(
            size(grid.layout_boxes(VIEWPORT.0, VIEWPORT.1).unwrap()[&3]),
            (150.0, 34.0)
        );
    }

    #[test]
    fn block_controls_fit_their_content_and_buttons_center_labels() {
        let view = view("block", |b| {
            b.create_element(3, Tag::Button)
                .set_style(3, P::Display, "block")
                .set_style(3, P::Width, "80px")
                .set_style(3, P::Height, "40px")
                .insert(2, 3, 0)
                .create_text(4, "A")
                .insert(3, 4, 0)
                .create_element(5, Tag::Input)
                .set_style(5, P::Display, "block")
                .insert(2, 5, 0);
        });
        let boxes = boxes(&view).unwrap();
        assert_eq!(size(boxes[&3]), (80.0, 40.0));
        // The label line is vertically centered in the content box.
        assert_eq!(boxes[&4].y, 13.0);
        assert_eq!(size(boxes[&5]), (152.0, 20.0));
    }

    #[test]
    fn the_control_font_resets_inherited_size_weight_and_line_height() {
        let view = view("flex", |b| {
            b.set_style(2, P::FontSize, "30px")
                .set_style(2, P::LineHeight, "60px")
                .set_style(2, P::FontWeight, "700");
            b.create_element(3, Tag::Button)
                .insert(2, 3, 0)
                .create_text(4, "AB")
                .insert(3, 4, 0)
                .create_element(5, Tag::Button)
                .set_style(5, P::FontSize, "20px")
                .insert(2, 5, 0)
                .create_text(6, "AB")
                .insert(5, 6, 0);
        });
        // The bold row text never shapes: the controls use weight 400.
        let boxes = boxes(&view).unwrap();
        assert_eq!(boxes[&4].height, 14.0);
        assert_eq!(size(boxes[&6]), (16.0, 26.0 - 6.0));
    }

    #[test]
    fn unmodelled_control_placements_and_partial_author_styles_are_refused() {
        let refused = |fill: &dyn Fn(&mut BatchWriter), display: &str, node: u32| {
            let view = view(display, |b| fill(b));
            assert!(
                matches!(boxes(&view), Err(Error::Unsupported(id)) if id == node),
                "{display}"
            );
        };
        // An inline-block control in a line box.
        refused(
            &|b| {
                b.create_element(3, Tag::Input).insert(2, 3, 0);
            },
            "block",
            3,
        );
        // An author background without the `border` shorthand.
        refused(
            &|b| {
                b.create_element(3, Tag::Input)
                    .set_style(3, P::BackgroundColor, "#ffffff")
                    .insert(2, 3, 0);
            },
            "flex",
            3,
        );
        // An unsized select, and an author-styled checkbox without the
        // explicit box.
        refused(
            &|b| {
                b.create_element(3, Tag::Select).insert(2, 3, 0);
            },
            "flex",
            3,
        );
        refused(
            &|b| {
                b.create_element(3, Tag::Input)
                    .set_attribute(3, Attribute::Type, "checkbox")
                    .set_style(3, P::Border, "1px solid #000000")
                    .insert(2, 3, 0);
            },
            "flex",
            3,
        );
        // An author-styled textarea needs a width.
        refused(
            &|b| {
                b.create_element(3, Tag::Textarea)
                    .set_style(3, P::Border, "none")
                    .insert(2, 3, 0);
            },
            "flex",
            3,
        );
        // Without a text measurer there is no font for a field's size.
        let field = view("flex", |b| {
            b.create_element(3, Tag::Input).insert(2, 3, 0);
        });
        assert!(matches!(
            field.layout_boxes(VIEWPORT.0, VIEWPORT.1),
            Err(Error::Unsupported(3))
        ));
    }

    #[test]
    fn a_primary_font_without_x_cannot_size_a_field() {
        let mut b = BatchWriter::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::Display, "flex")
            .create_element(3, Tag::Input)
            .insert(2, 3, 0);
        let mut view = View::default();
        view.commit(&b.finish(), false, &mut Deny).unwrap();
        let mut font = TextMeasurer::new(
            include_bytes!("../../tests/fixtures/structure-400.ttf").to_vec(),
            "Varta Structure Fixture",
        )
        .unwrap();
        assert!(matches!(
            view.layout_boxes_with_text(VIEWPORT.0, VIEWPORT.1, &mut font, MEDIUM),
            Err(Error::Unsupported(3))
        ));
    }

    #[test]
    fn min_content_uses_the_shrinkable_width_and_known_sizes_win() {
        let size = ControlSize {
            width: 140.0,
            min_width: 0.0,
            height: 28.0,
        };
        let unknown = Size {
            width: None,
            height: None,
        };
        let measured = |width| {
            size.measure(
                unknown,
                Size {
                    width,
                    height: AvailableSpace::MaxContent,
                },
            )
        };
        assert_eq!(measured(AvailableSpace::MinContent).width, 0.0);
        assert_eq!(measured(AvailableSpace::MaxContent).width, 140.0);
        assert_eq!(measured(AvailableSpace::Definite(10.0)).width, 140.0);
        let known = size.measure(
            Size {
                width: Some(50.0),
                height: Some(9.0),
            },
            Size {
                width: AvailableSpace::MinContent,
                height: AvailableSpace::MinContent,
            },
        );
        assert_eq!((known.width, known.height), (50.0, 9.0));
    }
}
